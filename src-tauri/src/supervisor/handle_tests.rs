use super::*;
use serde_json::json;

#[test]
fn dropping_a_clone_keeps_the_supervisor_running() {
    let root = std::env::temp_dir().join(format!(
        "msfslogger-supervisor-clone-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let config = ConfigStore::new(root.join("config.json"));
    // A missing node binary keeps this test from launching a real sidecar.
    config
        .save(json!({"nodePath": root.join("missing-node.exe")}))
        .unwrap();
    let supervisor = Supervisor::new(config, None, Arc::new(|_: Event| {})).unwrap();
    drop(supervisor.clone());
    assert!(!supervisor.stopping.load(Ordering::Acquire));
    assert!(supervisor.request(Operation::Stop).is_ok());
    supervisor.shutdown();
    assert!(supervisor.request(Operation::Stop).is_err());
    let _ = std::fs::remove_dir_all(&root);
}
