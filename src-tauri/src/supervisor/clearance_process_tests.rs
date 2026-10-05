// The clearance op through the relay, against the Python fixture, with the
// relay deadline scaled down so nothing waits the production 12 s.

use super::process_fixture::{code, python_path, ProcessFixture, FAKE_SIDECAR};
use super::*;
use serde_json::json;
use std::{ops::Deref, sync::atomic::AtomicUsize, time::Duration};

const SENTINEL: &str = "SENTINEL-CLEARANCE-TOKEN-0000";
const SCALED: datalink::RelayTimeouts = datalink::RelayTimeouts {
    default: Duration::from_millis(1_000),
    prefile: Duration::from_millis(2_500),
    sayintentions: Duration::from_millis(2_000),
};
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(ProcessFixture);

impl Deref for Fixture {
    type Target = ProcessFixture;

    fn deref(&self) -> &ProcessFixture {
        &self.0
    }
}

impl Fixture {
    fn new(mode: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "msfslogger-clearance-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        let fixture = Self(ProcessFixture::launch(
            root,
            FAKE_SIDECAR,
            json!({"nodePath":python_path(), "autoUplink":false, "datalinkMode":mode,
            "serverUrl":"http://127.0.0.1:1", "ingestToken":SENTINEL}),
            Some(SCALED),
        ));
        fixture.wait(|| fixture.app_state() == "app.stopped", 10);
        fixture.wait(|| fixture.count("datalink") >= 1, 5);
        fixture
    }

    fn clearance_lines(&self) -> usize {
        self.ops().iter().filter(|op| *op == "clearance").count()
    }

    fn in_flight(&self) -> bool {
        self.supervisor.clearance_in_flight.load(Ordering::Acquire)
    }

    fn spawn_clearance(&self) -> thread::JoinHandle<(Value, Duration)> {
        let supervisor = self.supervisor.clone();
        thread::spawn(move || {
            let started = Instant::now();
            (
                supervisor.datalink("clearance", json!({"plannedLegId":12})),
                started.elapsed(),
            )
        })
    }
}

#[test]
fn one_clearance_writes_one_line_and_waits_only_the_default_deadline() {
    let fixture = Fixture::new("answer");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let (envelope, elapsed) = fixture.timed("clearance", json!({"plannedLegId":12}));
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"clearance"}}));
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(fixture.ops(), ["clearance"]);
    assert!(!fixture.in_flight());
    assert_eq!(
        fixture.supervisor.relay_timeouts.for_op("clearance"),
        SCALED.default
    );

    // Params out of contract never reach the sidecar.
    for params in [
        json!({"plannedLegId":12, "tripId":1}),
        json!({"plannedLegId":12, "flightId":92}),
        json!({"plannedLegId":0}),
        json!({"plannedLegId":"12"}),
        json!({"plannedLegId":12.5}),
        json!({"plannedLegId":9_007_199_254_740_992_u64}),
        json!({}),
        json!(null),
    ] {
        let (envelope, elapsed) = fixture.timed("clearance", params);
        assert_eq!(code(&envelope), "bad-request");
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }
    thread::sleep(Duration::from_millis(300));
    assert_eq!(fixture.ops(), ["clearance"]);
    assert!(!fixture.in_flight());
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn an_unanswered_clearance_times_out_is_single_flight_and_is_never_resent() {
    let fixture = Fixture::new("ignore");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let first = fixture.spawn_clearance();
    fixture.wait(|| fixture.clearance_lines() == 1, 5);

    let (envelope, elapsed) = fixture.timed("clearance", json!({"plannedLegId":12}));
    assert_eq!(
        envelope,
        json!({"ok":false, "error":{"code":"clearance-in-progress", "httpStatus":null, "serverCode":null}})
    );
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(fixture.clearance_lines(), 1);
    // Nothing was registered for the refused press.
    assert_eq!(fixture.supervisor.relay.pending(), 1);

    let (envelope, elapsed) = first.join().unwrap();
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= SCALED.default, "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert!(!fixture.in_flight());
    assert_eq!(fixture.supervisor.relay.pending(), 0);

    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.clearance_lines(), 1);

    // Once settled, a new press is accepted and writes its own line.
    let (envelope, _) = fixture.timed("clearance", json!({"plannedLegId":12}));
    assert_eq!(code(&envelope), "shell-timeout");
    assert_eq!(fixture.clearance_lines(), 2);
    assert!(!fixture.in_flight());
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn a_clearance_is_never_resent_after_the_sidecar_exits() {
    let fixture = Fixture::new("exit-on-request");
    let (envelope, elapsed) = fixture.timed("clearance", json!({"plannedLegId":12}));
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    assert!(!fixture.in_flight());
    fixture.wait(|| fixture.starts() == 2, 10);
    fixture.wait(|| lock(&fixture.supervisor.snapshot).hello.is_some(), 5);
    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.ops(), ["clearance"]);
}

#[test]
fn a_clearance_is_never_resent_after_a_restart() {
    let fixture = Fixture::new("ignore");
    let waiter = fixture.spawn_clearance();
    fixture.wait(|| fixture.clearance_lines() == 1, 5);
    fixture.supervisor.request(Operation::Restart).unwrap();
    let (envelope, elapsed) = waiter.join().unwrap();
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    fixture.wait(|| fixture.starts() == 2, 10);
    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.ops(), ["clearance"]);
    assert!(!fixture.in_flight());
}

#[test]
fn a_sidecar_without_the_clearance_feature_is_refused_at_once_and_keeps_the_rest() {
    for mode in ["no-clearance", "no-simbrief"] {
        let fixture = Fixture::new(mode);
        let backend = fixture.backend();
        let statuses = fixture.count("status");
        let events = fixture.count("datalink");
        let state = fixture.supervisor.current_datalink_state();
        let (envelope, elapsed) = fixture.timed("clearance", json!({"plannedLegId":12}));
        assert_eq!(
            envelope,
            json!({"ok":false, "error":{"code":"sidecar-outdated", "httpStatus":null, "serverCode":null}}),
            "{mode}"
        );
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
        // Gated before params: even a malformed request is simply outdated.
        let (envelope, _) = fixture.timed("clearance", json!({"tripId":1}));
        assert_eq!(code(&envelope), "sidecar-outdated");
        thread::sleep(Duration::from_millis(300));
        assert_eq!(fixture.ops(), Vec::<String>::new(), "{mode}");
        assert_eq!(fixture.count("datalink"), events);
        assert_eq!(fixture.supervisor.current_datalink_state(), state);
        assert!(!fixture.in_flight());

        let (envelope, _) = fixture.timed("watch", json!({"on":true}));
        assert_eq!(envelope, json!({"ok":true, "result":{"echo":"watch"}}));
        assert_eq!(fixture.ops(), ["watch"]);
        assert_eq!(fixture.backend(), backend);
        assert_eq!(fixture.count("status"), statuses);
    }
    let fixture = Fixture::new("no-clearance");
    let (envelope, _) = fixture.timed("simbrief-settings", json!({}));
    assert_eq!(
        envelope,
        json!({"ok":true, "result":{"echo":"simbrief-settings"}})
    );
}

#[test]
fn the_ingest_token_never_leaves_the_shell_on_the_clearance_op() {
    let fixture = Fixture::new("echo-token");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let (envelope, _) = fixture.timed("clearance", json!({"plannedLegId":12}));
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["result"]["note"], "token [REDACTED]");
    fixture.wait(|| fixture.count("datalink") >= 2, 5);
    let state = fixture.supervisor.current_datalink_state();
    let events = serde_json::to_string(&*lock(&fixture.events)).unwrap();
    let logs = serde_json::to_string(&lock(&fixture.supervisor.snapshot).logs).unwrap();
    for output in [envelope.to_string(), state.to_string(), events, logs] {
        assert!(!output.contains(SENTINEL));
    }
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}
