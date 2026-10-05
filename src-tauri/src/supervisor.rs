mod inbound;
mod launch;
mod lifecycle;
mod pipes;
mod relay;
mod report;

use crate::{
    config::{lock, ConfigStore},
    datalink::{self, DatalinkRelay},
    protocol,
    restart::RestartBudget,
};
use pipes::{Input, StreamLine};
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone)]
pub enum Event {
    Status(Value),
    Log(Value),
    Exit(Value),
    Datalink(Value),
}
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Clone)]
pub enum Operation {
    Start,
    Stop,
    Restart,
    Reload,
    Datalink { id: String, line: String },
}

#[derive(Default)]
pub struct Snapshot {
    pub status: Option<Value>,
    pub hello: Option<Value>,
    pub pong: Option<Value>,
    pub datalink: Option<Value>,
    logs: VecDeque<Value>,
}

#[derive(Clone)]
pub struct Supervisor {
    pub config: Arc<ConfigStore>,
    pub snapshot: Arc<Mutex<Snapshot>>,
    requests: SyncSender<Operation>,
    stopping: Arc<AtomicBool>,
    sink: EventSink,
    relay: Arc<DatalinkRelay>,
    relay_timeouts: datalink::RelayTimeouts,
    // Set while a SimBrief prefile is outstanding, so a second press is refused
    // here rather than creating a second planned leg.
    prefile_in_flight: Arc<AtomicBool>,
    // Set while a clearance request is outstanding. The server writes logbook
    // rows for the first one, so a second press is refused rather than sent.
    clearance_in_flight: Arc<AtomicBool>,
    // One flag for every SayIntentions write. Each is one press against one
    // upstream session, so interleaving a link with an import is meaningless,
    // and a second press is refused while the first one's fate is open.
    sayintentions_in_flight: Arc<AtomicBool>,
    worker: Arc<WorkerHandle>,
}

impl Supervisor {
    pub fn new(
        config: ConfigStore,
        resource_entry: Option<PathBuf>,
        sink: EventSink,
    ) -> Result<Self, String> {
        let config = Arc::new(config);
        // The only thing the shell reads out of the file is autoUplink, which
        // is its own decision to act on. Whether the config is usable at all
        // is the sidecar's judgement, reported by the sidecar, so there is no
        // second set of validation rules here to drift out of step.
        let auto_uplink = config
            .read()
            .ok()
            .flatten()
            .and_then(|raw| raw.get("autoUplink").and_then(Value::as_bool))
            .unwrap_or(false);
        let snapshot = Arc::new(Mutex::new(Snapshot {
            status: Some(protocol::idle_status("app.starting")),
            ..Snapshot::default()
        }));
        let stopping = Arc::new(AtomicBool::new(false));
        let relay = Arc::new(DatalinkRelay::default());
        let (requests, receiver) = mpsc::sync_channel(64);
        let (lines, stream) = mpsc::sync_channel(128);
        let mut worker = Worker {
            config: config.clone(),
            snapshot: snapshot.clone(),
            stopping: stopping.clone(),
            sink: sink.clone(),
            relay: relay.clone(),
            resource_entry,
            child: None,
            input: None,
            generation: 0,
            lines,
            stream,
            budget: RestartBudget::default(),
            restart_at: None,
            desired_running: auto_uplink,
            version_error_logged: false,
            crash_latched: false,
        };
        let thread = thread::Builder::new()
            .name("sidecar-supervisor".into())
            .spawn(move || {
                // The sidecar always runs, so it can report on its own config and
                // on the sim. Manual START stays the baseline for the uplink: an
                // absent or non-boolean autoUplink reads false, and then nothing
                // is posted to the server until the user presses START.
                worker.spawn();
                worker.run(receiver);
            })
            .map_err(|_| "Cannot start sidecar supervisor")?;
        Ok(Self {
            config,
            snapshot,
            requests,
            stopping: stopping.clone(),
            sink,
            relay,
            relay_timeouts: datalink::RelayTimeouts::PRODUCTION,
            prefile_in_flight: Arc::new(AtomicBool::new(false)),
            clearance_in_flight: Arc::new(AtomicBool::new(false)),
            sayintentions_in_flight: Arc::new(AtomicBool::new(false)),
            worker: Arc::new(WorkerHandle {
                stopping,
                thread: Mutex::new(Some(thread)),
            }),
        })
    }

    pub fn request(&self, operation: Operation) -> Result<(), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("App is shutting down".into());
        }
        self.requests
            .try_send(operation)
            .map_err(|_| "Sidecar supervisor is busy or unavailable".into())
    }

    pub fn shutdown(&self) {
        self.worker.shutdown();
    }
}

// The supervisor is cloneable, so stop-on-drop lives on the shared worker
// handle: only the last clone going away stops the sidecar.
struct WorkerHandle {
    stopping: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl WorkerHandle {
    fn shutdown(&self) {
        // A separate flag gives close/exit priority over queued button presses.
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = lock(&self.thread).take() {
            let _ = worker.join();
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// Owned by the sidecar-supervisor thread. Its methods are split by job:
// lifecycle.rs (tick loop, operations, terminate, and the Drop that
// terminates the child), launch.rs (spawn), inbound.rs (sidecar output)
// and report.rs (snapshot and events).
struct Worker {
    config: Arc<ConfigStore>,
    snapshot: Arc<Mutex<Snapshot>>,
    stopping: Arc<AtomicBool>,
    sink: EventSink,
    relay: Arc<DatalinkRelay>,
    resource_entry: Option<PathBuf>,
    child: Option<Child>,
    input: Option<SyncSender<Input>>,
    generation: u64,
    lines: SyncSender<StreamLine>,
    stream: Receiver<StreamLine>,
    budget: RestartBudget,
    restart_at: Option<Instant>,
    desired_running: bool,
    version_error_logged: bool,
    crash_latched: bool,
}

#[cfg(test)]
mod clearance_process_tests;
#[cfg(test)]
mod datalink_process_tests;
#[cfg(test)]
mod handle_tests;
#[cfg(test)]
mod process_fixture;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod sayintentions_process_tests;
#[cfg(test)]
mod simbrief_process_tests;
#[cfg(all(test, target_os = "linux"))]
mod tests;
#[cfg(test)]
mod writer_tests;
