use super::{Event, EventSink, Snapshot, Worker};
use crate::{
    config::{lock, ConfigStore},
    protocol,
};
use serde_json::{json, Value};
use std::sync::Mutex;

impl Worker {
    pub(super) fn set_datalink_state(&self, value: Value) {
        lock(&self.snapshot).datalink = Some(value.clone());
        (self.sink)(Event::Datalink(value));
    }

    pub(super) fn status(&self, value: Value) {
        lock(&self.snapshot).status = Some(value.clone());
        (self.sink)(Event::Status(value));
    }

    pub(super) fn synthetic(&self, state: &str) {
        let mut value = lock(&self.snapshot)
            .status
            .clone()
            .unwrap_or_else(|| protocol::idle_status(state));
        value["at"] = json!(protocol::now());
        value["app"] = json!({"state": state});
        value["sim"]["state"] = json!("sim.idle");
        value["sim"]["nextRetryAt"] = Value::Null;
        value["sim"]["retryDelayMs"] = Value::Null;
        value["backend"]["state"] = json!("net.idle");
        // A status the shell invents never claims a runtime: either no node
        // ran, or the one that did is gone and the next may be a different
        // release. The next real status line brings it back.
        if let Some(object) = value.as_object_mut() {
            object.remove("runtime");
        }
        self.status(value);
    }

    pub(super) fn log(&self, level: &str, text: &str) {
        record_log(&self.config, &self.snapshot, &self.sink, level, text);
    }

    pub(super) fn log_value(&self, value: Value) {
        record_log_value(&self.snapshot, &self.sink, value);
    }
}

pub(super) fn record_log(
    config: &ConfigStore,
    snapshot: &Mutex<Snapshot>,
    sink: &EventSink,
    level: &str,
    text: &str,
) {
    let message = config.redact_text(text);
    // Also visible in the `cargo tauri dev` terminal itself, not just the
    // webview's one-line scratchpad — the scratchpad only ever shows the
    // latest message, so a fast crash loop's real cause is otherwise gone
    // by the time anyone looks at the window.
    eprintln!("[sidecar:{level}] {message}");
    record_log_value(
        snapshot,
        sink,
        json!({"v":1, "type":"log", "at":protocol::now(), "level":level, "message":message}),
    );
}

fn record_log_value(snapshot: &Mutex<Snapshot>, sink: &EventSink, value: Value) {
    {
        let mut cache = lock(snapshot);
        if cache.logs.len() == 200 {
            cache.logs.pop_front();
        }
        cache.logs.push_back(value.clone());
    }
    sink(Event::Log(value));
}
