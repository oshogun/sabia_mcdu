use super::{pipes::Input, Event, Operation, Worker};
use crate::{
    datalink,
    restart::{RESTART_DELAY, SHUTDOWN_GRACE},
};
use serde_json::json;
use std::{
    process::ExitStatus,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

impl Worker {
    pub(super) fn run(&mut self, requests: Receiver<Operation>) {
        while !self.stopping.load(Ordering::Acquire) {
            match requests.recv_timeout(Duration::from_millis(20)) {
                Ok(operation) => self.apply(operation),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            // Bound work per tick so flooding stdout cannot starve STOP/exit.
            for _ in 0..64 {
                let Ok(line) = self.stream.try_recv() else {
                    break;
                };
                // A dead child may leave logs queued behind its exit. Keep
                // those diagnostics, but never let its late status undo the
                // synthetic crashed/restarting state or a newer child's state.
                let current = line.generation == self.generation;
                self.read_output(line, current);
            }
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        self.child.take();
                        self.input.take();
                        let exited = self.generation;
                        self.generation += 1;
                        self.datalink_lost(exited);
                        self.unexpected_exit(status);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        self.log("error", "Cannot inspect sidecar process; stopping it");
                        self.terminate();
                        self.synthetic("app.crashed");
                    }
                }
            }
            if self.restart_at.is_some_and(|at| Instant::now() >= at)
                && !self.stopping.load(Ordering::Acquire)
            {
                self.restart_at = None;
                self.spawn();
            }
        }
        self.restart_at = None;
        self.terminate();
        // Requests still queued behind the stop would otherwise wait out the
        // whole relay deadline for a worker that is gone.
        while let Ok(operation) = requests.try_recv() {
            if let Operation::Datalink { id, .. } = operation {
                self.relay.fail_unbound(&id, "sidecar-unavailable");
            }
        }
    }

    fn apply(&mut self, operation: Operation) {
        match operation {
            Operation::Start => {
                self.desired_running = true;
                if self.child.is_some() {
                    self.control("start");
                }
                // START cannot bypass an exhausted crash budget; RESTART is
                // the explicit action that clears it.
                else if self.crash_latched {
                    self.log("warn", "Sidecar stopped after failure; use RESTART");
                } else if self.restart_at.is_none() {
                    self.spawn();
                }
            }
            Operation::Stop => {
                self.desired_running = false;
                self.restart_at = None;
                // A live sidecar reports its own state after a stop. A dead
                // one is a fault, and saying "stopped" for it would claim a
                // working config nobody has checked.
                if self.child.is_some() {
                    self.control("stop");
                } else {
                    self.synthetic("app.crashed");
                }
            }
            Operation::Restart => {
                self.restart_at = None;
                self.budget.reset();
                self.crash_latched = false;
                self.terminate();
                self.synthetic("app.restarting");
                self.spawn();
            }
            Operation::Reload => {
                // The sidecar re-reads and re-validates the file; saving
                // settings never implies an uplink start. A pending respawn
                // picks the new file up on its own, so leave it alone.
                if self.child.is_some() {
                    self.control("config");
                } else if self.restart_at.is_none() {
                    self.synthetic("app.crashed");
                }
            }
            Operation::Datalink { id, line } => {
                let Some(input) = self.input.as_ref().filter(|_| self.child.is_some()) else {
                    self.relay.fail_unbound(&id, "sidecar-unavailable");
                    return;
                };
                if input.try_send(Input::Line(line)).is_err() {
                    self.relay.fail_unbound(&id, "busy");
                } else {
                    self.relay.bind_generation(&id, self.generation);
                }
            }
        }
    }

    pub(super) fn control(&mut self, kind: &'static str) {
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.try_send(Input::Control(kind)).is_err())
        {
            self.log(
                "warn",
                "Cannot queue sidecar control; input is busy or closed",
            );
        }
    }

    /// Everything waiting on the sidecar that just went away is answered now
    /// rather than at the relay deadline.
    fn datalink_lost(&self, generation: u64) {
        self.relay.fail_generation(generation);
        self.set_datalink_state(datalink::synthetic_state(datalink::STATE_UNAVAILABLE));
    }

    fn unexpected_exit(&mut self, status: ExitStatus) {
        let remaining = self.budget.reserve(Instant::now());
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal().map(|signal| signal.to_string())
        };
        #[cfg(not(unix))]
        let signal: Option<String> = None;
        eprintln!(
            "[sidecar] exited: code={:?} signal={:?}",
            status.code(),
            signal
        );
        (self.sink)(Event::Exit(
            json!({"code":status.code(), "signal":signal, "restarting":remaining.is_some(), "restartsRemaining":remaining.unwrap_or(0)}),
        ));
        self.synthetic("app.crashed");
        if remaining.is_some() {
            self.restart_at = Some(Instant::now() + RESTART_DELAY);
            self.synthetic("app.restarting");
        } else {
            self.crash_latched = true;
            self.log(
                "error",
                "Sidecar restart budget exhausted (5 in 60 seconds); use RESTART",
            );
        }
    }

    pub(super) fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let exited = self.generation;
            self.generation += 1;
            // Queue graceful shutdown, close the input queue (the writer then
            // closes stdin/EOF), and enforce a two-second kill deadline. Input
            // writes run separately so a child that stops reading cannot block
            // window close. Child::kill uses TerminateProcess on Windows.
            if let Some(input) = self.input.take() {
                let _ = input.try_send(Input::Control("shutdown"));
            }
            self.datalink_lost(exited);
            let deadline = Instant::now() + SHUTDOWN_GRACE;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    _ => break,
                }
            }
            if child.kill().is_ok() {
                let _ = child.wait();
            } else {
                self.log("error", "Could not terminate sidecar process");
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.terminate();
    }
}
