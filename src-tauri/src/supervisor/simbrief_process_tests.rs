// The SimBrief ops through the relay, against the Python fixture, with relay
// deadlines scaled down so nothing waits the production 12 s or 30 s.

use super::process_fixture::{code, python_path, ProcessFixture, FAKE_SIDECAR};
use super::*;
use serde_json::json;
use std::{ops::Deref, sync::atomic::AtomicUsize, time::Duration};

const SENTINEL: &str = "SENTINEL-SIMBRIEF-TOKEN-0000";
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
            "msfslogger-simbrief-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        let fixture = Self(ProcessFixture::launch(
            root,
            FAKE_SIDECAR,
            json!({"nodePath":python_path(), "autoUplink":false, "datalinkMode":mode,
            "prefileDelayMs":1500, "serverUrl":"http://127.0.0.1:1", "ingestToken":SENTINEL}),
            Some(SCALED),
        ));
        fixture.wait(|| fixture.app_state() == "app.stopped", 10);
        fixture.wait(|| fixture.count("datalink") >= 1, 5);
        fixture
    }

    fn prefile_lines(&self) -> usize {
        self.ops()
            .iter()
            .filter(|op| *op == "simbrief-prefile")
            .count()
    }

    fn spawn_prefile(&self) -> thread::JoinHandle<(Value, Duration)> {
        let supervisor = self.supervisor.clone();
        thread::spawn(move || {
            let started = Instant::now();
            (
                supervisor.datalink("simbrief-prefile", json!({})),
                started.elapsed(),
            )
        })
    }
}

/// The server's duplicate override, spelled so its key never appears in
/// client source, even in a test.
fn duplicate_override() -> Value {
    let mut params = serde_json::Map::new();
    params.insert(format!("allow{}duplicates", '_'), json!(true));
    Value::Object(params)
}

#[test]
fn production_relay_timeouts_are_the_frozen_pair() {
    assert_eq!(
        datalink::RelayTimeouts::PRODUCTION,
        datalink::RelayTimeouts {
            default: datalink::REQUEST_TIMEOUT,
            prefile: datalink::PREFILE_REQUEST_TIMEOUT,
            sayintentions: datalink::SAYINTENTIONS_REQUEST_TIMEOUT,
        }
    );
    assert!(SCALED.prefile > SCALED.default);
}

#[test]
fn a_prefile_answered_after_the_default_timeout_still_resolves_with_that_answer() {
    let fixture = Fixture::new("slow-prefile");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let (envelope, elapsed) = fixture.timed("watch", json!({"on":true}));
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"watch"}}));
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");

    let (envelope, elapsed) = fixture.timed("simbrief-prefile", json!({}));
    assert_eq!(
        envelope,
        json!({"ok":true, "result":{"echo":"simbrief-prefile"}})
    );
    assert!(elapsed >= Duration::from_millis(1_500), "{elapsed:?}");
    assert!(elapsed > SCALED.default, "{elapsed:?}");
    assert!(elapsed < SCALED.prefile, "{elapsed:?}");
    assert_eq!(fixture.ops(), ["watch", "simbrief-prefile"]);

    // Settings and clear are answered at once and wait only the default.
    for op in ["simbrief-settings", "prefile-clear"] {
        let (envelope, elapsed) = fixture.timed(op, json!({}));
        assert_eq!(envelope, json!({"ok":true, "result":{"echo":op}}));
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }
    // Extra keys never reach the sidecar.
    for (op, params) in [
        ("simbrief-prefile", duplicate_override()),
        ("simbrief-prefile", json!({"tripId":1})),
        ("simbrief-settings", json!({"pilotId":"1"})),
        ("prefile-clear", json!({"plannedLegId":123})),
        ("simbrief-prefile", json!(null)),
    ] {
        let (envelope, elapsed) = fixture.timed(op, params);
        assert_eq!(code(&envelope), "bad-request");
        assert!(elapsed < Duration::from_secs(1));
    }
    assert_eq!(
        fixture.ops(),
        [
            "watch",
            "simbrief-prefile",
            "simbrief-settings",
            "prefile-clear"
        ]
    );
    assert!(!fixture.supervisor.prefile_in_flight.load(Ordering::Acquire));
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn an_unanswered_prefile_times_out_at_its_own_deadline_and_is_single_flight() {
    let fixture = Fixture::new("ignore");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let first = fixture.spawn_prefile();
    fixture.wait(|| fixture.prefile_lines() == 1, 5);

    let (envelope, elapsed) = fixture.timed("simbrief-prefile", json!({}));
    assert_eq!(code(&envelope), "prefile-in-progress");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(fixture.prefile_lines(), 1);

    // Every other op still waits only the default deadline.
    let (envelope, elapsed) = fixture.timed("watch", json!({"on":true}));
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= SCALED.default, "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");

    let (envelope, elapsed) = first.join().unwrap();
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= SCALED.prefile, "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(3_500), "{elapsed:?}");
    assert!(!fixture.supervisor.prefile_in_flight.load(Ordering::Acquire));
    assert_eq!(fixture.supervisor.relay.pending(), 0);

    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.prefile_lines(), 1);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn a_prefile_is_never_resent_after_the_sidecar_exits() {
    let fixture = Fixture::new("exit-on-request");
    let (envelope, elapsed) = fixture.timed("simbrief-prefile", json!({}));
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    assert!(!fixture.supervisor.prefile_in_flight.load(Ordering::Acquire));
    fixture.wait(|| fixture.starts() == 2, 10);
    fixture.wait(|| lock(&fixture.supervisor.snapshot).hello.is_some(), 5);
    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.ops(), ["simbrief-prefile"]);
}

#[test]
fn a_prefile_is_never_resent_after_a_restart() {
    let fixture = Fixture::new("ignore");
    let waiter = fixture.spawn_prefile();
    fixture.wait(|| fixture.prefile_lines() == 1, 5);
    fixture.supervisor.request(Operation::Restart).unwrap();
    let (envelope, elapsed) = waiter.join().unwrap();
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    fixture.wait(|| fixture.starts() == 2, 10);
    thread::sleep(Duration::from_secs(1));
    assert_eq!(fixture.ops(), ["simbrief-prefile"]);
    assert!(!fixture.supervisor.prefile_in_flight.load(Ordering::Acquire));
}

#[test]
fn a_sidecar_without_simbrief_is_refused_at_once_and_keeps_its_datalink() {
    let fixture = Fixture::new("no-simbrief");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let events = fixture.count("datalink");
    let state = fixture.supervisor.current_datalink_state();
    for op in ["simbrief-settings", "simbrief-prefile", "prefile-clear"] {
        let (envelope, elapsed) = fixture.timed(op, json!({}));
        assert_eq!(
            envelope,
            json!({"ok":false, "error":{"code":"sidecar-outdated", "httpStatus":null, "serverCode":null}})
        );
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }
    // Gated before params: even a malformed request is simply outdated.
    let (envelope, _) = fixture.timed("simbrief-prefile", duplicate_override());
    assert_eq!(code(&envelope), "sidecar-outdated");
    thread::sleep(Duration::from_millis(300));
    assert_eq!(fixture.ops(), Vec::<String>::new());
    assert_eq!(fixture.count("datalink"), events);
    assert_eq!(fixture.supervisor.current_datalink_state(), state);
    assert!(!fixture.supervisor.prefile_in_flight.load(Ordering::Acquire));

    let (envelope, _) = fixture.timed("watch", json!({"on":true}));
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"watch"}}));
    assert_eq!(fixture.ops(), ["watch"]);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn the_ingest_token_never_leaves_the_shell_on_the_simbrief_ops() {
    let fixture = Fixture::new("echo-token");
    let backend = fixture.backend();
    let mut envelopes = Vec::new();
    for op in ["simbrief-settings", "simbrief-prefile", "prefile-clear"] {
        let (envelope, _) = fixture.timed(op, json!({}));
        assert_eq!(envelope["ok"], true, "{op}");
        assert_eq!(envelope["result"]["note"], "token [REDACTED]");
        envelopes.push(envelope.to_string());
    }
    fixture.wait(|| fixture.count("datalink") >= 4, 5);
    let state = fixture.supervisor.current_datalink_state();
    let events = serde_json::to_string(&*lock(&fixture.events)).unwrap();
    let logs = serde_json::to_string(&lock(&fixture.supervisor.snapshot).logs).unwrap();
    for output in envelopes
        .into_iter()
        .chain([state.to_string(), events, logs])
    {
        assert!(!output.contains(SENTINEL));
    }
    assert_eq!(fixture.backend(), backend);
}
