// The five SayIntentions ops through the relay, against the Python fixture,
// with the relay deadlines scaled down so nothing waits the production 12 s
// or 25 s.

use super::process_fixture::{code, python_path, ProcessFixture, FAKE_SIDECAR};
use super::*;
use serde_json::json;
use std::{ops::Deref, sync::atomic::AtomicUsize, time::Duration};

const SENTINEL: &str = "SENTINEL-SAYINTENTIONS-TOKEN0";
const SCALED: datalink::RelayTimeouts = datalink::RelayTimeouts {
    default: Duration::from_millis(1_000),
    prefile: Duration::from_millis(3_000),
    sayintentions: Duration::from_millis(2_000),
};
/// The accepted shape of each op, in the order the contract lists them.
const ACCEPTED: [(&str, &str); 5] = [
    ("si-status", r#"{"flightId":92}"#),
    ("si-link", r#"{"flightId":92,"from":"now"}"#),
    ("si-unlink", r#"{"flightId":92}"#),
    ("si-import", r#"{"flightId":92}"#),
    ("si-pdc", r#"{"plannedLegId":12}"#),
];
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
            "msfslogger-sayintentions-{}-{}",
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

    fn in_flight(&self) -> bool {
        self.supervisor
            .sayintentions_in_flight
            .load(Ordering::Acquire)
    }

    fn spawn(&self, op: &'static str, params: Value) -> thread::JoinHandle<(Value, Duration)> {
        let supervisor = self.supervisor.clone();
        thread::spawn(move || {
            let started = Instant::now();
            (supervisor.datalink(op, params), started.elapsed())
        })
    }
}

fn params(shape: &str) -> Value {
    serde_json::from_str(shape).unwrap()
}

#[test]
fn production_relay_timeouts_carry_the_sayintentions_pair() {
    assert_eq!(
        datalink::RelayTimeouts::PRODUCTION.sayintentions,
        datalink::SAYINTENTIONS_REQUEST_TIMEOUT
    );
    assert!(SCALED.sayintentions > SCALED.default);
}

#[test]
fn each_op_relays_one_line_and_params_out_of_contract_never_reach_the_sidecar() {
    let fixture = Fixture::new("answer");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    for (op, shape) in ACCEPTED {
        let (envelope, elapsed) = fixture.timed(op, params(shape));
        assert_eq!(envelope, json!({"ok":true, "result":{"echo":op}}), "{op}");
        assert!(elapsed < Duration::from_secs(1), "{op} {elapsed:?}");
        assert!(!fixture.in_flight(), "{op}");
    }
    // The flight-free question is the same op with a null id.
    let (envelope, _) = fixture.timed("si-status", json!({"flightId":null}));
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"si-status"}}));
    let relayed = fixture.ops();
    assert_eq!(
        relayed,
        [
            "si-status",
            "si-link",
            "si-unlink",
            "si-import",
            "si-pdc",
            "si-status"
        ]
    );
    // Refused before the relay is entered: the fixture is reading stdin, so
    // an unchanged op list means no line was written for any of these.
    for (op, shape) in [
        ("si-status", r#"{}"#),
        ("si-status", r#"{"flightId":92,"plannedLegId":12}"#),
        ("si-status", r#"{"flightId":"92"}"#),
        ("si-status", r#"{"flightId":0}"#),
        ("si-status", r#"{"flightId":1.5}"#),
        ("si-link", r#"{"flightId":92}"#),
        ("si-link", r#"{"flightId":92,"from":"session_start"}"#),
        ("si-link", r#"{"flightId":92,"from":"now","sinceId":1}"#),
        ("si-unlink", r#"{"flightId":null}"#),
        ("si-import", r#"{"flightId":92,"sinceId":1}"#),
        ("si-pdc", r#"{"flightId":92}"#),
        ("si-pdc", r#"null"#),
    ] {
        let (envelope, elapsed) = fixture.timed(op, params(shape));
        assert_eq!(code(&envelope), "bad-request", "{op} {shape}");
        assert!(elapsed < Duration::from_secs(1), "{op} {shape}");
    }
    thread::sleep(Duration::from_millis(300));
    assert_eq!(fixture.ops(), relayed);
    assert!(!fixture.in_flight());
    assert_eq!(fixture.supervisor.relay.pending(), 0);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn an_unanswered_write_waits_its_own_deadline_and_one_guard_covers_all_four() {
    let fixture = Fixture::new("ignore");
    let backend = fixture.backend();
    let first = fixture.spawn("si-import", json!({"flightId":92}));
    fixture.wait(|| fixture.ops() == ["si-import"], 5);

    // Any other write, on any page, is refused while that one is open.
    for (op, shape) in [
        ("si-link", r#"{"flightId":92,"from":"now"}"#),
        ("si-unlink", r#"{"flightId":92}"#),
        ("si-import", r#"{"flightId":92}"#),
        ("si-pdc", r#"{"plannedLegId":12}"#),
    ] {
        let (envelope, elapsed) = fixture.timed(op, params(shape));
        assert_eq!(
            envelope,
            json!({"ok":false, "error":{"code":"sayintentions-in-progress", "httpStatus":null, "serverCode":null}}),
            "{op}"
        );
        assert!(elapsed < Duration::from_secs(1), "{op} {elapsed:?}");
    }
    // The read takes no guard and waits only the default deadline.
    let (envelope, elapsed) = fixture.timed("si-status", json!({"flightId":92}));
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= SCALED.default, "{elapsed:?}");
    assert!(elapsed < SCALED.sayintentions, "{elapsed:?}");

    let (envelope, elapsed) = first.join().unwrap();
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= SCALED.sayintentions, "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(3_000), "{elapsed:?}");
    assert!(!fixture.in_flight());
    assert_eq!(fixture.supervisor.relay.pending(), 0);
    assert_eq!(fixture.ops(), ["si-import", "si-status"]);

    // Once settled, the next press is accepted and writes its own line.
    let (envelope, _) = fixture.timed("si-pdc", json!({"plannedLegId":12}));
    assert_eq!(code(&envelope), "shell-timeout");
    assert_eq!(fixture.ops(), ["si-import", "si-status", "si-pdc"]);
    assert!(!fixture.in_flight());
    assert_eq!(fixture.backend(), backend);
}

#[test]
fn a_sidecar_without_sayintentions_is_refused_at_once_and_keeps_the_rest() {
    let fixture = Fixture::new("no-sayintentions");
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let events = fixture.count("datalink");
    let state = fixture.supervisor.current_datalink_state();
    for (op, shape) in ACCEPTED {
        let (envelope, elapsed) = fixture.timed(op, params(shape));
        assert_eq!(
            envelope,
            json!({"ok":false, "error":{"code":"sidecar-outdated", "httpStatus":null, "serverCode":null}}),
            "{op}"
        );
        assert!(elapsed < Duration::from_secs(1), "{op} {elapsed:?}");
    }
    // Gated before params: a malformed request reads as outdated, because
    // the feature is missing whatever the params say.
    for (op, shape) in [
        ("si-status", r#"{}"#),
        ("si-link", r#"{"flightId":0,"from":"whenever"}"#),
        ("si-pdc", r#"{"flightId":92}"#),
    ] {
        let (envelope, _) = fixture.timed(op, params(shape));
        assert_eq!(code(&envelope), "sidecar-outdated", "{op} {shape}");
    }
    thread::sleep(Duration::from_millis(300));
    assert_eq!(fixture.ops(), Vec::<String>::new());
    assert_eq!(fixture.count("datalink"), events);
    assert_eq!(fixture.supervisor.current_datalink_state(), state);
    assert!(!fixture.in_flight());

    // Everything the sidecar does announce still works.
    for (op, shape) in [
        ("watch", r#"{"on":true}"#),
        ("clearance", r#"{"plannedLegId":12}"#),
    ] {
        let (envelope, _) = fixture.timed(op, params(shape));
        assert_eq!(envelope, json!({"ok":true, "result":{"echo":op}}), "{op}");
    }
    assert_eq!(fixture.ops(), ["watch", "clearance"]);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn the_ingest_token_never_leaves_the_shell_on_the_sayintentions_ops() {
    let fixture = Fixture::new("echo-token");
    let backend = fixture.backend();
    let mut envelopes = Vec::new();
    for (op, shape) in ACCEPTED {
        let (envelope, _) = fixture.timed(op, params(shape));
        assert_eq!(envelope["ok"], true, "{op}");
        assert_eq!(envelope["result"]["note"], "token [REDACTED]", "{op}");
        envelopes.push(envelope.to_string());
    }
    fixture.wait(|| fixture.count("datalink") >= 6, 5);
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
