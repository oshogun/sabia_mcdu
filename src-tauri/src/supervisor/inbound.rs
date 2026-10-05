use super::{pipes::StreamLine, Worker};
use crate::{
    config::lock,
    datalink,
    framing::Line,
    protocol::{self, DecodeError},
};
use serde_json::Value;

impl Worker {
    pub(super) fn read_output(&mut self, line: StreamLine, current: bool) {
        let text = match line.line {
            Line::Oversize => {
                self.log("warn", "Dropped sidecar line larger than 65536 bytes");
                return;
            }
            Line::InvalidUtf8 => {
                self.log("warn", "Dropped sidecar line with invalid UTF-8");
                return;
            }
            Line::Text(text) => text,
        };
        if text.trim().is_empty() {
            return;
        }
        if line.stderr {
            self.log("error", &text);
            return;
        }
        match protocol::decode(&text) {
            Ok(mut value) => {
                self.config.redact(&mut value);
                match value["type"].as_str() {
                    Some("status") if current => self.status(value),
                    Some("log") => self.log_value(value),
                    Some("hello") if current => self.hello(value),
                    Some("pong") if current => lock(&self.snapshot).pong = Some(value),
                    // A response is matched by id and by the generation it was
                    // sent to, so one from a replaced sidecar is simply unknown.
                    Some("datalink-response") => {
                        let envelope = datalink::response_envelope(&value);
                        let id = value["id"].as_str().unwrap_or_default();
                        if !self.relay.complete(id, line.generation, envelope) {
                            self.log("warn", "Dropped datalink response with an unknown id");
                        }
                    }
                    Some("datalink-state") if current => self.set_datalink_state(value),
                    _ => {} // Reserved messages have no webview events yet.
                }
            }
            // Never include the rejected bytes or parser details: a malformed
            // line can contain a token even though the normal protocol cannot.
            Err(DecodeError::BadVersion) => {
                if !self.version_error_logged {
                    self.log("error", "Sidecar protocol version mismatch; expected 1");
                    self.version_error_logged = true;
                }
            }
            Err(DecodeError::UnknownType) => {}
            Err(_) => self.log("warn", "Dropped malformed or non-JSON sidecar message"),
        }
    }

    fn hello(&self, value: Value) {
        let supported = datalink::supports_datalink(&value);
        let mut snapshot = lock(&self.snapshot);
        snapshot.hello = Some(value);
        if supported {
            // The sidecar reports its own datalink state right after hello.
            snapshot.datalink = None;
        } else {
            drop(snapshot);
            self.set_datalink_state(datalink::synthetic_state(datalink::STATE_OUTDATED));
        }
    }
}
