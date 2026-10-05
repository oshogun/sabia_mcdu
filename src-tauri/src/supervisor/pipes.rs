use crate::framing::{self, Line};
use serde_json::json;
use std::{
    io::{BufReader, Read, Write},
    sync::mpsc::{self, SyncSender},
    thread,
};

/// What the stdin writer thread sends to the sidecar, one line per item.
pub(super) enum Input {
    Control(&'static str),
    Line(String),
}

pub(super) struct StreamLine {
    pub(super) generation: u64,
    pub(super) stderr: bool,
    pub(super) line: Line,
}

pub(super) fn spawn_reader(
    pipe: impl Read + Send + 'static,
    stderr: bool,
    generation: u64,
    sender: SyncSender<StreamLine>,
) -> std::io::Result<()> {
    thread::Builder::new()
        .name(
            if stderr {
                "sidecar-stderr"
            } else {
                "sidecar-stdout"
            }
            .into(),
        )
        .spawn(move || {
            let mut reader = BufReader::new(pipe);
            loop {
                match framing::read_line(&mut reader) {
                    Ok(Some(line)) => {
                        if sender
                            .send(StreamLine {
                                generation,
                                stderr,
                                line,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        let _ = sender.send(StreamLine {
                            generation,
                            stderr: true,
                            line: Line::Text("Sidecar output pipe read failed".into()),
                        });
                        break;
                    }
                }
            }
        })
        .map(|_| ())
}

pub(super) fn spawn_writer(
    mut stdin: std::process::ChildStdin,
) -> std::io::Result<SyncSender<Input>> {
    // Room for every pending datalink line plus controls, so a burst of
    // datalink requests never starves START/STOP.
    let (sender, receiver) = mpsc::sync_channel::<Input>(16);
    thread::Builder::new()
        .name("sidecar-stdin".into())
        .spawn(move || {
            for input in receiver {
                if !matches!(write_input(&mut stdin, &input), Ok(true)) {
                    break;
                }
            }
            // Dropping this handle sends EOF, including when the supervisor
            // closes its queue. A stuck write cannot block the kill deadline.
        })?;
    Ok(sender)
}

/// Writes one queued item; false means the writer is done after this line.
pub(super) fn write_input(out: &mut impl Write, input: &Input) -> std::io::Result<bool> {
    match input {
        Input::Control(kind) => {
            writeln!(out, "{}", json!({"v":1,"type":kind}))?;
            out.flush()?;
            Ok(*kind != "shutdown")
        }
        Input::Line(line) => {
            writeln!(out, "{line}")?;
            out.flush()?;
            Ok(true)
        }
    }
}
