use super::{pipes, Worker};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::Ordering,
};

/// Strips Windows' `\\?\` extended-length/verbatim prefix, if present,
/// leaving the ordinary drive-letter form (`C:\...`). Leaves `\\?\UNC\...`
/// network paths and `\\?\Volume{GUID}\...` volume paths alone — both need a
/// different reconstruction than a plain prefix strip (`\\?\UNC\` expands to
/// `\\`; a volume GUID has no drive-letter form at all), which this narrow
/// fix doesn't need to handle. A no-op on any path that never had the
/// prefix, which is every path on non-Windows.
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(s)
            if s.starts_with(r"\\?\")
                && !s[4..].starts_with("UNC\\")
                && !s[4..].starts_with("Volume") =>
        {
            PathBuf::from(&s[4..])
        }
        _ => path.to_path_buf(),
    }
}

impl Worker {
    pub(super) fn spawn(&mut self) {
        if self.child.is_some() || self.stopping.load(Ordering::Acquire) {
            return;
        }
        // Built one component at a time, not `.join("../sidecar/dist/index.js")`:
        // that embeds a literal ".." next to a forward-slash string, which on
        // Windows produces a mixed-separator, unnormalized path. Node's own
        // module-resolution directory walk chokes on that — it can degenerate
        // down to the bare string "C:" (no trailing backslash), which Windows
        // treats specially and fs.lstat rejects with EISDIR. Per-component
        // .join() never embeds a raw separator character, so this can't happen.
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR is always windows-client/src-tauri, which has a parent")
            .join("sidecar")
            .join("dist")
            .join("index.js");
        let entry = self
            .resource_entry
            .as_ref()
            .filter(|path| path.is_file())
            .cloned()
            .or_else(|| development.is_file().then_some(development.clone()));
        let Some(entry) = entry else {
            self.crash_latched = true;
            self.log(
                "error",
                &format!(
                    "Sidecar entry missing; tried {} and {}",
                    self.resource_entry
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| {
                            "<unavailable resource directory>/sidecar/dist/index.js".into()
                        }),
                    development.display()
                ),
            );
            self.synthetic("app.crashed");
            return;
        };
        // Tauri's path resolver returns Windows' "\\?\"-prefixed verbatim form
        // (the same form std::fs::canonicalize produces). The OS handles it
        // fine for actually opening the file, but Node's own package.json
        // boundary-walk for the main module does plain string manipulation
        // that does not expect that prefix, and mishandles the drive root —
        // producing exactly the EISDIR-on-"C:" crash a real Windows run hit.
        // Strip it back to the normal drive-letter form before handing this
        // to node; irrelevant off Windows since the prefix never appears there.
        let entry = strip_verbatim_prefix(&entry);
        let raw = self.config.read().ok().flatten();
        let node = raw
            .as_ref()
            .and_then(|raw| raw.get("nodePath"))
            .and_then(Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .unwrap_or("node");
        let Some(directory) = entry.parent().and_then(|dist| dist.parent()) else {
            self.log("error", "Sidecar entry has no parent directory");
            self.synthetic("app.crashed");
            return;
        };
        let mut command = Command::new(node);
        command
            .arg(&entry)
            .arg("--config")
            .arg(&self.config.path)
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Inherit the environment unchanged. Credentials and CA configuration
        // travel only through the file, never through argv or injected env vars.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        match command.spawn() {
            Ok(mut child) => {
                self.generation += 1;
                self.version_error_logged = false;
                let stdin = child.stdin.take();
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                self.child = Some(child);
                let input = stdin
                    .ok_or_else(|| std::io::Error::other("Missing child stdin"))
                    .and_then(|pipe| self.writer(pipe));
                if input.is_err() {
                    self.log("error", "Cannot start sidecar input writer");
                    self.terminate();
                    self.synthetic("app.crashed");
                    return;
                }
                let readers = stdout
                    .map(|pipe| self.reader(pipe, false))
                    .transpose()
                    .and_then(|_| stderr.map(|pipe| self.reader(pipe, true)).transpose());
                if readers.is_err() {
                    self.log("error", "Cannot start sidecar output reader");
                    self.terminate();
                    self.synthetic("app.crashed");
                    return;
                }
                self.log("info", "Sidecar process started");
                // A fresh sidecar is idle and reports so itself; it is only
                // told to start when autoUplink or the user asked for it.
                if self.desired_running {
                    self.control("start");
                }
            }
            Err(_) => {
                self.crash_latched = true;
                self.log(
                    "error",
                    "Cannot launch sidecar; install Node 24 on PATH or set nodePath in config.json",
                );
                self.synthetic("app.crashed");
            }
        }
    }

    fn reader(&self, pipe: impl Read + Send + 'static, stderr: bool) -> std::io::Result<()> {
        let sender = self.lines.clone();
        let generation = self.generation;
        pipes::spawn_reader(pipe, stderr, generation, sender)
    }

    fn writer(&mut self, stdin: std::process::ChildStdin) -> std::io::Result<()> {
        self.input = Some(pipes::spawn_writer(stdin)?);
        Ok(())
    }
}
