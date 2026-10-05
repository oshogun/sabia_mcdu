// The harness the five process-test modules share: a temp root holding the
// sidecar entry, a supervisor over it, and every event it emits, tagged by kind.
// Each wraps it with its own temp-root prefix, config and any scaled deadlines.

use super::{Event, Supervisor};
use crate::{
    config::{lock, ConfigStore},
    datalink::RelayTimeouts,
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub(super) const FAKE_SIDECAR: &str = include_str!("../../tests/fake-sidecar.py");

pub(super) fn python_path() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "/usr/bin/python3"
    }
}

pub(super) fn code(envelope: &Value) -> &str {
    envelope["error"]["code"].as_str().unwrap_or_default()
}

pub(super) struct ProcessFixture {
    pub(super) root: PathBuf,
    pub(super) supervisor: Supervisor,
    pub(super) events: Arc<Mutex<Vec<(&'static str, Value)>>>,
}

impl ProcessFixture {
    pub(super) fn launch(
        root: PathBuf,
        script: &str,
        settings: Value,
        timeouts: Option<RelayTimeouts>,
    ) -> Self {
        fs::create_dir_all(root.join("dist")).unwrap();
        let entry = root.join("dist").join("index.js");
        fs::write(&entry, script).unwrap();
        let config = ConfigStore::new(root.join("config.json"));
        config.save(settings).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sink = Arc::new(move |event| {
            let recorded = match event {
                Event::Status(v) => ("status", v),
                Event::Log(v) => ("log", v),
                Event::Exit(v) => ("exit", v),
                Event::Datalink(v) => ("datalink", v),
            };
            lock(&captured).push(recorded);
        });
        let supervisor = Supervisor::new(config, Some(entry), sink).unwrap();
        let supervisor = match timeouts {
            Some(timeouts) => supervisor.with_relay_timeouts(timeouts),
            None => supervisor,
        };
        Self {
            root,
            supervisor,
            events,
        }
    }

    pub(super) fn wait(&self, predicate: impl Fn() -> bool, seconds: u64) {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for fixture state"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn app_state(&self) -> String {
        lock(&self.supervisor.snapshot).status.as_ref().unwrap()["app"]["state"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub(super) fn backend(&self) -> Value {
        lock(&self.supervisor.snapshot).status.as_ref().unwrap()["backend"].clone()
    }

    pub(super) fn count(&self, kind: &str) -> usize {
        lock(&self.events)
            .iter()
            .filter(|(k, _)| *k == kind)
            .count()
    }

    pub(super) fn starts(&self) -> usize {
        fs::read_to_string(self.root.join("starts"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// Every op the fixture received, one per request line, in order.
    pub(super) fn ops(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("datalink-ops"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn timed(&self, op: &str, params: Value) -> (Value, Duration) {
        let started = Instant::now();
        let envelope = self.supervisor.datalink(op, params);
        (envelope, started.elapsed())
    }
}

impl Drop for ProcessFixture {
    fn drop(&mut self) {
        self.supervisor.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}
