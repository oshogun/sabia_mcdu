// Runs the Python fixture as the sidecar, on every platform, so the relay is
// exercised against a real child process and real pipes.

use super::process_fixture::{code, python_path, ProcessFixture, FAKE_SIDECAR};
use super::*;
use serde_json::json;
use std::{fs, ops::Deref, sync::atomic::AtomicUsize, time::Duration};

const SENTINEL: &str = "SENTINEL-DATALINK-TOKEN-0000";
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
            "msfslogger-datalink-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        let fixture = Self(ProcessFixture::launch(
            root,
            FAKE_SIDECAR,
            json!({"nodePath":python_path(), "autoUplink":false, "datalinkMode":mode,
            "serverUrl":"http://127.0.0.1:1", "ingestToken":SENTINEL}),
            None,
        ));
        fixture.wait(|| fixture.app_state() == "app.stopped", 10);
        fixture
    }

    fn has_log(&self, message: &str) -> bool {
        lock(&self.events)
            .iter()
            .any(|(kind, v)| *kind == "log" && v["message"] == message)
    }

    fn has_datalink_state(&self, state: &str) -> bool {
        lock(&self.events)
            .iter()
            .any(|(kind, v)| *kind == "datalink" && v["state"] == state)
    }

    // One control type per line, whatever newline the platform wrote.
    fn controls(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("controls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn a_request_resolves_with_the_response_carrying_its_id() {
    let fixture = Fixture::new("answer");
    fixture.wait(|| fixture.has_datalink_state("dl.idle"), 5);
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let (envelope, _) = fixture.timed("watch", json!({"on":true}));
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"watch"}}));
    let (envelope, _) = fixture.timed(
        "wx",
        json!({"target":{"kind":"leg","id":12}, "icao":"LFPG"}),
    );
    assert_eq!(envelope, json!({"ok":true, "result":{"echo":"wx"}}));
    fixture.wait(|| fixture.has_datalink_state("dl.ok"), 5);
    assert_eq!(
        fixture.supervisor.current_datalink_state()["state"],
        "dl.ok"
    );
    assert_eq!(fixture.controls(), ["datalink-request", "datalink-request"]);
    // Invalid params never reach the sidecar.
    let (envelope, elapsed) = fixture.timed(
        "send-canned",
        json!({"target":{"kind":"flight","id":1}, "cannedId":"gate-request", "body":"FREE TEXT"}),
    );
    assert_eq!(code(&envelope), "bad-request");
    assert!(elapsed < Duration::from_secs(1));
    assert_eq!(fixture.controls(), ["datalink-request", "datalink-request"]);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
}

#[test]
fn datalink_errors_never_move_the_backend_status() {
    let fixture = Fixture::new("answer-error");
    fixture.wait(|| fixture.has_datalink_state("dl.idle"), 5);
    let backend = fixture.backend();
    let statuses = fixture.count("status");
    let (envelope, _) = fixture.timed("loadsheet", json!({"plannedLegId":12}));
    assert_eq!(
        envelope,
        json!({"ok":false, "error":{"code":"no-dispatch-data", "httpStatus":409, "serverCode":"NO_DISPATCH_DATA"}})
    );
    fixture.wait(|| fixture.has_datalink_state("dl.unavailable"), 5);
    assert_eq!(fixture.backend(), backend);
    assert_eq!(fixture.count("status"), statuses);
    assert_eq!(fixture.backend()["state"], "net.idle");
}

#[test]
fn a_response_with_an_unknown_id_is_dropped_without_its_payload() {
    let fixture = Fixture::new("wrong-id");
    fixture.wait(|| fixture.has_datalink_state("dl.idle"), 5);
    let backend = fixture.backend();
    let (envelope, elapsed) = fixture.timed("refresh", json!({}));
    assert_eq!(code(&envelope), "shell-timeout");
    assert!(elapsed >= datalink::REQUEST_TIMEOUT);
    assert!(elapsed < datalink::REQUEST_TIMEOUT + Duration::from_secs(1));
    assert!(fixture.has_log("Dropped datalink response with an unknown id"));
    assert!(fixture.has_log("Datalink request timed out"));
    let logs = serde_json::to_string(&*lock(&fixture.events)).unwrap();
    assert!(!logs.contains("dl-999999"));
    assert!(!logs.contains("echo"));
    assert_eq!(fixture.backend(), backend);
}

#[test]
fn an_ignored_request_times_out_and_the_pending_bound_refuses_the_ninth() {
    let fixture = Fixture::new("ignore");
    fixture.wait(|| lock(&fixture.supervisor.snapshot).hello.is_some(), 5);
    let backend = fixture.backend();
    let waiting: Vec<_> = (0..datalink::PENDING_MAX)
        .map(|_| {
            let supervisor = fixture.supervisor.clone();
            thread::spawn(move || {
                let started = Instant::now();
                (supervisor.datalink("refresh", json!({})), started.elapsed())
            })
        })
        .collect();
    fixture.wait(
        || fixture.supervisor.relay.pending() == datalink::PENDING_MAX,
        5,
    );
    let (envelope, elapsed) = fixture.timed("canned-list", json!({}));
    assert_eq!(code(&envelope), "busy");
    assert!(elapsed < Duration::from_secs(1));
    for waiter in waiting {
        let (envelope, elapsed) = waiter.join().unwrap();
        assert_eq!(code(&envelope), "shell-timeout");
        assert!(elapsed < datalink::REQUEST_TIMEOUT + Duration::from_secs(1));
    }
    assert_eq!(fixture.supervisor.relay.pending(), 0);
    // The supervisor is still serving everything else.
    assert!(lock(&fixture.supervisor.snapshot).status.is_some());
    assert_eq!(fixture.backend(), backend);
    fixture.supervisor.request(Operation::Start).unwrap();
    fixture.wait(|| fixture.app_state() == "app.running", 5);
    assert_eq!(
        fixture
            .controls()
            .iter()
            .filter(|c| *c == "datalink-request")
            .count(),
        datalink::PENDING_MAX
    );
}

#[test]
fn a_sidecar_exit_fails_the_pending_request_promptly() {
    let fixture = Fixture::new("exit-on-request");
    fixture.wait(|| fixture.has_datalink_state("dl.idle"), 5);
    let (envelope, elapsed) = fixture.timed("watch", json!({"on":true}));
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    fixture.wait(
        || fixture.has_datalink_state(datalink::STATE_UNAVAILABLE),
        5,
    );
    assert_eq!(fixture.supervisor.relay.pending(), 0);
}

#[test]
fn a_restart_fails_the_pending_request_promptly() {
    let fixture = Fixture::new("ignore");
    fixture.wait(|| lock(&fixture.supervisor.snapshot).hello.is_some(), 5);
    let supervisor = fixture.supervisor.clone();
    let waiter = thread::spawn(move || {
        let started = Instant::now();
        (supervisor.datalink("refresh", json!({})), started.elapsed())
    });
    fixture.wait(
        || fixture.controls().iter().any(|c| c == "datalink-request"),
        5,
    );
    fixture.supervisor.request(Operation::Restart).unwrap();
    let (envelope, elapsed) = waiter.join().unwrap();
    assert_eq!(code(&envelope), "sidecar-exited");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    fixture.wait(
        || fixture.has_datalink_state(datalink::STATE_UNAVAILABLE),
        5,
    );
}

#[test]
fn a_sidecar_without_the_feature_is_refused_without_a_request() {
    let fixture = Fixture::new("no-features");
    fixture.wait(|| fixture.has_datalink_state(datalink::STATE_OUTDATED), 5);
    assert_eq!(
        fixture.supervisor.current_datalink_state()["state"],
        datalink::STATE_OUTDATED
    );
    for (op, params) in [
        ("watch", json!({"on":true})),
        ("thread", json!({"epoch":1, "endSeq":0})),
        ("loadsheet", json!({"plannedLegId":12})),
    ] {
        let (envelope, elapsed) = fixture.timed(op, params);
        assert_eq!(code(&envelope), "sidecar-outdated");
        assert!(elapsed < Duration::from_secs(1));
    }
    // The sidecar is reading stdin, so an absent line means none was sent.
    fixture.supervisor.request(Operation::Start).unwrap();
    fixture.wait(|| fixture.app_state() == "app.running", 5);
    assert_eq!(fixture.controls(), ["start"]);
    assert_eq!(fixture.count("datalink"), 1);
}

#[test]
fn the_ingest_token_never_leaves_the_shell() {
    let fixture = Fixture::new("echo-token");
    fixture.wait(|| fixture.has_datalink_state("dl.idle"), 5);
    let (envelope, _) = fixture.timed("canned-list", json!({}));
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["result"]["note"], "token [REDACTED]");
    fixture.wait(|| fixture.has_datalink_state("dl.ok"), 5);
    let state = fixture.supervisor.current_datalink_state();
    assert_eq!(state["scope"]["note"], "[REDACTED]");
    let events = serde_json::to_string(&*lock(&fixture.events)).unwrap();
    let logs = serde_json::to_string(&lock(&fixture.supervisor.snapshot).logs).unwrap();
    for output in [envelope.to_string(), state.to_string(), events, logs] {
        assert!(!output.contains(SENTINEL));
    }
    // The fixture also echoes the token in a log line and on stderr.
    assert!(fixture.has_log("redact [REDACTED]"));
}
