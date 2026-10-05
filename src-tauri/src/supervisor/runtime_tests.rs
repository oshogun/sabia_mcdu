// Cross-platform: the launch-failure text and the runtime block on statuses
// the shell invents, against a missing node binary and a Python stand-in.

use super::process_fixture::{python_path, ProcessFixture};
use super::*;
use serde_json::json;
use std::{ops::Deref, sync::atomic::AtomicUsize, time::Duration};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

// Prints a hello and one stopped status carrying a driver ABI mismatch,
// then exits as if it had crashed.
const MISMATCH_SIDECAR: &str = r#"import json, os, sys
runtime = {"nodeVersion": "20.20.2", "nodeAbi": 115, "driver": "abi-mismatch",
           "driverAbi": 137, "requiredNodeMajor": 24}
hello = dict(v=1, type='hello', at=1, pid=os.getpid(), sidecarVersion='fixture',
             nodeVersion='fixture', configPath='fixture')
status = dict(v=1, type='status', at=1, app=dict(state='app.stopped'),
              sim=dict(state='sim.idle', attempt=0, nextRetryAt=None, retryDelayMs=None,
                       protocol='KittyHawk', appName=None, appVersion=None, lastError=None),
              backend=dict(state='net.idle', httpStatus=None, lastOkAt=None, lastErrorAt=None,
                           message=None),
              pause=dict(state='pause.off', flags=0, label='off', usingPauseEx1=False),
              traffic=dict(enabled=True, radiusM=40000, lastSweepAt=None, lastBatchSize=None,
                           lastError=None),
              runtime=runtime, config=None)
print(json.dumps(hello), flush=True)
print(json.dumps(status), flush=True)
sys.exit(21)
"#;

fn mismatch_runtime() -> Value {
    json!({"nodeVersion":"20.20.2", "nodeAbi":115, "driver":"abi-mismatch",
        "driverAbi":137, "requiredNodeMajor":24})
}

struct Fixture(ProcessFixture);

impl Deref for Fixture {
    type Target = ProcessFixture;

    fn deref(&self) -> &ProcessFixture {
        &self.0
    }
}

impl Fixture {
    fn new(node_path: Value, script: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "msfslogger-runtime-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        Self(ProcessFixture::launch(
            root,
            script,
            json!({"nodePath":node_path, "autoUplink":false}),
            None,
        ))
    }

    fn wait(&self, predicate: impl Fn(&[(&'static str, Value)]) -> bool, seconds: u64) {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while !predicate(&lock(&self.events)) {
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for fixture events"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn statuses(&self) -> Vec<Value> {
        lock(&self.events)
            .iter()
            .filter(|(kind, _)| *kind == "status")
            .map(|(_, v)| v.clone())
            .collect()
    }
}

#[test]
fn launch_failure_names_node_24_and_nodepath() {
    let root =
        std::env::temp_dir().join(format!("msfslogger-runtime-missing-{}", std::process::id()));
    let fixture = Fixture::new(json!(root.join("missing-node.exe")), "");
    let expected = "Cannot launch sidecar; install Node 24 on PATH or set nodePath in config.json";
    fixture.wait(
        |events| {
            events
                .iter()
                .any(|(kind, v)| *kind == "log" && v["message"] == expected)
        },
        3,
    );
    fixture.wait(
        |events| {
            events
                .iter()
                .any(|(kind, v)| *kind == "status" && v["app"]["state"] == "app.crashed")
        },
        3,
    );
    let snapshot = lock(&fixture.supervisor.snapshot).status.clone().unwrap();
    assert_eq!(snapshot["app"]["state"], "app.crashed");
    assert!(snapshot.get("runtime").is_none());
    // Spelled out so the old release number never appears in the source.
    let stale = format!("Node {}", 20);
    for (_, event) in lock(&fixture.events).iter() {
        assert!(!event.to_string().contains(&stale), "{event}");
    }
}

#[test]
fn synthetic_statuses_carry_no_runtime() {
    let fixture = Fixture::new(json!(python_path()), MISMATCH_SIDECAR);
    fixture.wait(
        |events| {
            events
                .iter()
                .any(|(kind, v)| *kind == "status" && v["app"]["state"] == "app.restarting")
        },
        10,
    );
    let statuses = fixture.statuses();
    let stopped = statuses
        .iter()
        .position(|v| v["app"]["state"] == "app.stopped")
        .expect("the sidecar's own status is forwarded");
    assert_eq!(statuses[stopped]["runtime"], mismatch_runtime());
    let restarting = statuses[stopped..]
        .iter()
        .find(|v| v["app"]["state"] == "app.restarting")
        .unwrap();
    assert!(restarting.get("runtime").is_none(), "{restarting}");
    let crashed = statuses[stopped..]
        .iter()
        .find(|v| v["app"]["state"] == "app.crashed")
        .unwrap();
    assert!(crashed.get("runtime").is_none(), "{crashed}");
}
