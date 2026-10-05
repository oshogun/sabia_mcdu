// Compiled on Linux only (see the gate on `mod tests` in supervisor.rs): the
// fixture runs /usr/bin/python3 as the sidecar and checks pids under /proc.

use super::*;
use crate::restart::SHUTDOWN_GRACE;
use serde_json::json;
use std::{fs, sync::atomic::AtomicUsize, time::Duration};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    supervisor: Supervisor,
    events: Arc<Mutex<Vec<Value>>>,
}
impl Fixture {
    fn new(auto: bool, mode: &str) -> Self {
        Self::with_config(
            json!({"nodePath":"/usr/bin/python3", "autoUplink":auto, "fixtureMode":mode,
            "serverUrl":"http://127.0.0.1:1", "ingestToken":"PLACEHOLDER-TOKEN"}),
        )
    }
    fn with_config(settings: Value) -> Self {
        let root = std::env::temp_dir().join(format!(
            "msfslogger-supervisor-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(root.join("dist")).unwrap();
        let entry = root.join("dist/index.js");
        fs::write(&entry, include_str!("../../tests/fake-sidecar.py")).unwrap();
        let config = ConfigStore::new(root.join("config.json"));
        config.save(settings).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sink = Arc::new(move |event| {
            let value = match event {
                Event::Status(v) | Event::Log(v) | Event::Exit(v) | Event::Datalink(v) => v,
            };
            lock(&captured).push(value);
        });
        let supervisor = Supervisor::new(config, Some(entry), sink).unwrap();
        Self {
            root,
            supervisor,
            events,
        }
    }
    fn wait(&self, predicate: impl Fn() -> bool, seconds: u64) {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for fixture state"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn state(&self) -> String {
        lock(&self.supervisor.snapshot).status.as_ref().unwrap()["app"]["state"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    fn problems(&self) -> Value {
        lock(&self.supervisor.snapshot).status.as_ref().unwrap()["app"]["problems"].clone()
    }
    fn pid(&self) -> u64 {
        lock(&self.supervisor.snapshot).hello.as_ref().unwrap()["pid"]
            .as_u64()
            .unwrap()
    }
    fn starts(&self) -> usize {
        fs::read_to_string(self.root.join("starts"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn controls(&self) -> String {
        fs::read_to_string(self.root.join("controls")).unwrap_or_default()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.supervisor.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn manual_start_stop_redaction_and_graceful_close() {
    let fixture = Fixture::new(false, "normal");
    // Spawned at launch so the sidecar can report on itself, but idle:
    // without autoUplink nothing starts the uplink until the user does.
    fixture.wait(|| fixture.state() == "app.stopped", 3);
    assert_eq!(fixture.starts(), 1);
    assert_eq!(fixture.controls(), "");
    fixture.supervisor.request(Operation::Start).unwrap();
    fixture.wait(|| fixture.state() == "app.running", 3);
    assert_eq!(fixture.controls(), "start\n");
    let pid = fixture.pid();
    fixture.supervisor.request(Operation::Stop).unwrap();
    fixture.wait(|| fixture.state() == "app.stopped", 3);
    assert!(PathBuf::from(format!("/proc/{pid}")).exists());
    assert_eq!(fixture.starts(), 1);
    let recorded = serde_json::to_string(&*lock(&fixture.events)).unwrap();
    assert!(!recorded.contains("PLACEHOLDER-TOKEN"));
    assert!(recorded.contains("Dropped malformed"));
    assert!(recorded.contains("[REDACTED]"));
    fixture.supervisor.shutdown();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    assert!(fs::read_to_string(fixture.root.join("controls"))
        .unwrap()
        .contains("shutdown"));
}

#[test]
fn a_config_that_parses_but_is_invalid_reports_the_sidecar_state_and_its_problems() {
    // The shell must not read meaning into the file: a config that parses
    // is not a config that works, and only the sidecar knows the rules.
    let fixture = Fixture::with_config(json!({"nodePath":"/usr/bin/python3", "autoUplink":false,
        "serverUrl":"ftp://nope", "ingestToken":"", "sim":"2019"}));
    fixture.wait(|| fixture.state() == "app.error-config", 3);
    assert_eq!(fixture.problems()[0]["field"], "serverUrl");
    assert_eq!(fixture.starts(), 1);
    // START stays the sidecar's to refuse; the shell invents no running state.
    fixture.supervisor.request(Operation::Start).unwrap();
    thread::sleep(Duration::from_millis(200));
    assert_eq!(fixture.state(), "app.error-config");
    assert_eq!(fixture.starts(), 1);
}

#[test]
fn auto_uplink_is_the_only_thing_that_starts_the_uplink_at_launch() {
    let auto = Fixture::new(true, "normal");
    auto.wait(|| auto.state() == "app.running", 3);
    assert_eq!(auto.controls(), "start\n");
    let manual = Fixture::new(false, "normal");
    manual.wait(|| manual.state() == "app.stopped", 3);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(manual.controls(), "");
    assert_eq!(manual.starts(), 1);
}

#[test]
fn stalled_child_is_killed_at_shutdown_deadline() {
    let fixture = Fixture::new(true, "stalled");
    fixture.wait(|| lock(&fixture.supervisor.snapshot).hello.is_some(), 3);
    let pid = fixture.pid();
    let start = Instant::now();
    fixture.supervisor.shutdown();
    assert!(start.elapsed() >= SHUTDOWN_GRACE);
    assert!(start.elapsed() < SHUTDOWN_GRACE + Duration::from_secs(1));
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
}

#[test]
fn crash_loop_stops_after_five_restarts_and_manual_restart_resets_budget() {
    let fixture = Fixture::new(true, "crash");
    fixture.wait(
        || {
            lock(&fixture.events)
                .iter()
                .any(|e| e.get("restarting") == Some(&Value::Bool(false)))
        },
        15,
    );
    assert_eq!(fixture.starts(), 6);
    assert_eq!(fixture.state(), "app.crashed");
    fixture.supervisor.request(Operation::Stop).unwrap();
    fixture.supervisor.request(Operation::Start).unwrap();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(fixture.starts(), 6);
    assert_eq!(fixture.state(), "app.crashed");
    fixture
        .supervisor
        .config
        .save(json!({"fixtureMode":"normal"}))
        .unwrap();
    fixture.supervisor.request(Operation::Restart).unwrap();
    fixture.wait(|| fixture.state() == "app.running", 3);
    assert_eq!(fixture.starts(), 7);
    assert!(lock(&fixture.events)
        .iter()
        .any(|e| e.get("restartsRemaining") == Some(&json!(4))));
}
