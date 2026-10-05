use super::{report::record_log, Event, Operation, Supervisor};
use crate::{config::lock, datalink};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

/// Holds an in-flight flag for one relayed write and releases it on every way
/// out: an answer, a timeout, an exited sidecar or a refused queue.
struct InFlightGuard(Arc<AtomicBool>);

impl InFlightGuard {
    fn acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag.clone()))
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Supervisor {
    #[cfg(test)]
    pub fn with_relay_timeouts(mut self, timeouts: datalink::RelayTimeouts) -> Self {
        self.relay_timeouts = timeouts;
        self
    }

    /// Relays one datalink op to the sidecar and waits for its answer. Always
    /// returns an envelope, and never waits longer than the relay deadline.
    pub fn datalink(&self, op: &str, params: Value) -> Value {
        let mut envelope = self.relay_datalink(op, params);
        self.config.redact(&mut envelope);
        envelope
    }

    fn relay_datalink(&self, op: &str, params: Value) -> Value {
        if self.stopping.load(Ordering::Acquire) {
            return datalink::error_envelope("sidecar-unavailable");
        }
        let (refused, simbrief, clearance, sayintentions) = {
            let mut snapshot = lock(&self.snapshot);
            let simbrief = snapshot
                .hello
                .as_ref()
                .is_some_and(datalink::supports_simbrief);
            let clearance = snapshot
                .hello
                .as_ref()
                .is_some_and(datalink::supports_clearance);
            let sayintentions = snapshot
                .hello
                .as_ref()
                .is_some_and(datalink::supports_sayintentions);
            let state = match snapshot.hello.as_ref() {
                None => Some(datalink::STATE_UNAVAILABLE),
                Some(hello) if !datalink::supports_datalink(hello) => {
                    Some(datalink::STATE_OUTDATED)
                }
                Some(_) => None,
            };
            let refused = state.map(|state| {
                let current = snapshot
                    .datalink
                    .as_ref()
                    .is_some_and(|value| value["state"] == state);
                let synthetic = (!current).then(|| datalink::synthetic_state(state));
                if synthetic.is_some() {
                    snapshot.datalink = synthetic.clone();
                }
                (state, synthetic)
            });
            (refused, simbrief, clearance, sayintentions)
        };
        if let Some((state, synthetic)) = refused {
            if let Some(value) = synthetic {
                (self.sink)(Event::Datalink(value));
            }
            return datalink::error_envelope(if state == datalink::STATE_OUTDATED {
                "sidecar-outdated"
            } else {
                "sidecar-unavailable"
            });
        }
        // A sidecar that has the datalink but not SimBrief keeps its datalink
        // state untouched: only these ops are out of date.
        if datalink::SIMBRIEF_OPS.contains(&op) && !simbrief {
            return datalink::error_envelope("sidecar-outdated");
        }
        // Likewise a sidecar that predates the clearance op: nothing is written
        // and the datalink state is left as it is.
        if datalink::CLEARANCE_OPS.contains(&op) && !clearance {
            return datalink::error_envelope("sidecar-outdated");
        }
        // And a sidecar that predates the SayIntentions ops: nothing is sent,
        // nothing is written and the datalink state is left as it is.
        if datalink::SAYINTENTIONS_OPS.contains(&op) && !sayintentions {
            return datalink::error_envelope("sidecar-outdated");
        }
        if !datalink::valid_params(op, &params) {
            return datalink::error_envelope("bad-request");
        }
        let _prefile = if op == "simbrief-prefile" {
            match InFlightGuard::acquire(&self.prefile_in_flight) {
                Some(guard) => Some(guard),
                None => return datalink::error_envelope("prefile-in-progress"),
            }
        } else {
            None
        };
        let _clearance = if op == "clearance" {
            match InFlightGuard::acquire(&self.clearance_in_flight) {
                Some(guard) => Some(guard),
                None => return datalink::error_envelope("clearance-in-progress"),
            }
        } else {
            None
        };
        // One guard for all four SayIntentions writes, so a second press cannot
        // reach the upstream while the first one's fate is open — including a
        // press that crosses from one page to another. The status read takes no
        // guard: the pages discard a stale answer by themselves.
        let _sayintentions = if matches!(op, "si-link" | "si-unlink" | "si-import" | "si-pdc") {
            match InFlightGuard::acquire(&self.sayintentions_in_flight) {
                Some(guard) => Some(guard),
                None => return datalink::error_envelope("sayintentions-in-progress"),
            }
        } else {
            None
        };
        let (id, reply) = match self.relay.register() {
            Ok(registered) => registered,
            Err(busy) => return busy,
        };
        let Some(line) = datalink::request_line(&id, op, &params) else {
            self.relay.abandon(&id);
            return datalink::error_envelope("bad-request");
        };
        let queued = Operation::Datalink {
            id: id.clone(),
            line,
        };
        if self.requests.try_send(queued).is_err() {
            self.relay.abandon(&id);
            return datalink::error_envelope("busy");
        }
        match reply.recv_timeout(self.relay_timeouts.for_op(op)) {
            Ok(envelope) => envelope,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.relay.abandon(&id);
                // An answer that landed between the timeout and the abandon
                // is still the right answer.
                if let Ok(envelope) = reply.try_recv() {
                    return envelope;
                }
                record_log(
                    &self.config,
                    &self.snapshot,
                    &self.sink,
                    "warn",
                    "Datalink request timed out",
                );
                datalink::error_envelope("shell-timeout")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => datalink::error_envelope("sidecar-exited"),
        }
    }

    /// The latest datalink state, without waiting on the sidecar.
    pub fn current_datalink_state(&self) -> Value {
        let snapshot = lock(&self.snapshot);
        let mut value = match (snapshot.datalink.as_ref(), snapshot.hello.as_ref()) {
            (Some(state), _) => state.clone(),
            (None, None) => datalink::synthetic_state(datalink::STATE_UNAVAILABLE),
            (None, Some(hello)) if !datalink::supports_datalink(hello) => {
                datalink::synthetic_state(datalink::STATE_OUTDATED)
            }
            // A current sidecar whose own first state has not arrived yet.
            (None, Some(_)) => Value::Null,
        };
        drop(snapshot);
        self.config.redact(&mut value);
        value
    }
}
