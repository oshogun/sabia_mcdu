use super::pipes::write_input;
use super::*;

#[test]
fn control_messages_are_written_byte_for_byte_as_before() {
    let mut out = Vec::new();
    for kind in ["start", "stop", "config", "shutdown", "ping"] {
        let keep_going = write_input(&mut out, &Input::Control(kind)).unwrap();
        assert_eq!(keep_going, kind != "shutdown");
    }
    // Recorded from the writer before datalink lines shared its queue.
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "{\"type\":\"start\",\"v\":1}\n{\"type\":\"stop\",\"v\":1}\n{\"type\":\"config\",\"v\":1}\n{\"type\":\"shutdown\",\"v\":1}\n{\"type\":\"ping\",\"v\":1}\n"
    );
    let mut out = Vec::new();
    assert!(write_input(&mut out, &Input::Line("{\"v\":1}".into())).unwrap());
    assert_eq!(out, b"{\"v\":1}\n");
}
