//! Operator-only monotonic observations. Never read for admission, accounting,
//! verdicts or publication. One invocation owns each bounded record exclusively.
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::Instant;

pub(super) fn process_start_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

/// Bounded, best-effort kernel wait observations after an idle refusal. These
/// never determine that refusal, usage, cleanup or acceptance. Unknown reads
/// stay null. At most eight threads/256-byte wait channels bound record growth.
pub(super) fn waiters(group: Option<&Path>, direct_pid: u32) -> serde_json::Value {
    let pids = group
        .and_then(|p| fs::read_to_string(p.join("cgroup.procs")).ok())
        .map(|text| {
            text.lines()
                .filter_map(|s| s.parse::<u32>().ok())
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![direct_pid]);
    let mut rows = Vec::new();
    for pid in pids {
        if let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten().take(8 - rows.len()) {
                let path = task.path();
                let text = |name| {
                    fs::read_to_string(path.join(name))
                        .ok()
                        .map(|s| s.chars().take(256).collect::<String>())
                };
                rows.push(
                    serde_json::json!({"pid":pid,"tid":task.file_name().to_string_lossy(),
                    "process_start_ticks":process_start_ticks(pid),
                    "comm":text("comm"),"wchan":text("wchan"),"syscall":text("syscall")}),
                );
            }
        }
        if rows.len() == 8 {
            break;
        }
    }
    serde_json::json!({"authority":"none","after_idle_refusal":true,"threads":rows})
}

pub(super) struct Record {
    started: Instant,
    fields: serde_json::Map<String, serde_json::Value>,
}
impl Record {
    pub(super) fn new(observer: &str, backend: &str) -> Self {
        let mut this = Self {
            started: Instant::now(),
            fields: Default::default(),
        };
        this.set(
            "schema",
            serde_json::json!("zeno-fcis/operator-containment/1"),
        );
        this.set("authority", serde_json::json!("none"));
        this.set("observer", serde_json::json!(observer));
        this.set("backend", serde_json::json!(backend));
        this.set("pid", serde_json::json!(std::process::id()));

        this.set(
            "parent_pid",
            serde_json::json!(nix::unistd::getppid().as_raw()),
        );
        this
    }
    pub(super) fn started(&self) -> Instant {
        self.started
    }
    pub(super) fn set(&mut self, name: &str, value: serde_json::Value) {
        self.fields.insert(name.into(), value);
    }
    pub(super) fn duration(&mut self, name: &str, started: Instant) {
        // Bounded diagnostic milliseconds, never unbounded u128 JSON numbers.
        self.set(
            name,
            serde_json::json!(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
        );
    }
    fn write(&self) -> Result<(), String> {
        let root = Path::new("/tmp").join(format!("zeno-fcis-operator-{}", nix::unistd::geteuid()));
        match fs::DirBuilder::new().mode(0o700).create(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
        let metadata = fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o700
        {
            return Err("operator-directory-ownership".into());
        }
        let mut nonce = [0; 16];
        fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut nonce))
            .map_err(|e| e.to_string())?;
        // Unit identity in the filename lets readers select their invocation
        // before opening JSON, including a malformed record for that unit.
        let unit = self
            .fields
            .get("unit")
            .and_then(|v| v.as_str())
            .map(|u| format!("unit-{}-", crate::transform::sha256_hex(u.as_bytes())))
            .unwrap_or_default();
        let name = format!(
            "{}-{unit}{}.json",
            std::process::id(),
            crate::transform::sha256_hex(&nonce)
        );
        let bytes = serde_json::to_vec(&self.fields).map_err(|e| e.to_string())?;
        if bytes.len() > 32768 {
            return Err("operator-record-bound".into());
        }
        publish_record(&root, &name, |file| {
            file.write_all(&bytes)?;
            file.flush()
        })
        .map_err(|e| e.to_string())
    }
}
// The callback borrows this exclusively owned temporary FD only for the write.
// No FD or mutable alias escapes. Rename is atomic within the same directory;
// durability is unnecessary for best-effort diagnostics. Never open a new FD
// before startup custody/ACK is settled (Record::new remains I/O-free).
fn publish_record(
    root: &Path,
    name: &str,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let temporary = root.join(format!(".{name}.tmp"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&temporary)?;
    let result = write(&mut file).and_then(|()| fs::rename(&temporary, root.join(name)));
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl Drop for Record {
    fn drop(&mut self) {
        self.duration("observer_ms", self.started);
        // Record construction is I/O-free: a worker constructs its observation
        // after HELLO while the guardian may be inspecting its descriptor set.
        // The ACK result is settled before this Drop performs any file reads.
        self.set(
            "process_start_ticks",
            serde_json::json!(process_start_ticks(std::process::id())),
        );
        if let Err(error) = self.write() {
            // Diagnostic failure never supplies authority. Retain exact reason
            // on the operator stderr fallback, never in proposer-facing JSON.
            let _ = writeln!(
                std::io::stderr(),
                "zeno-fcis: operator containment record unavailable: {error}; {}",
                serde_json::Value::Object(self.fields.clone())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_private_complete_and_atomically_visible() {
        use std::sync::mpsc;
        let root = std::env::temp_dir().join(format!(
            "zeno-record-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let (ready, waiting) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let path = root.clone();
        let writer = std::thread::spawn(move || {
            publish_record(&path, "record.json", |f| {
                f.write_all(b"{\"complete\":")?;
                ready.send(()).unwrap();
                resumed.recv().unwrap();
                f.write_all(b"true}")?;
                f.flush()
            })
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            !root.join("record.json").exists(),
            "partial JSON must never be visible under final name"
        );
        assert_eq!(
            fs::symlink_metadata(root.join(".record.json.tmp"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        resume.send(()).unwrap();
        writer.join().unwrap().unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("record.json")).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!({"complete":true}));
        assert_eq!(
            fs::symlink_metadata(root.join("record.json"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        assert!(
            publish_record(&root, "failed.json", |f| {
                f.write_all(b"{")?;
                Err(std::io::Error::other("injected write refusal"))
            })
            .is_err()
        );
        assert!(!root.join("failed.json").exists());
        assert!(!root.join(".failed.json.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_record_and_closed_stderr_do_not_change_exit() {
        const PROBE: &str = "ZENO_FCIS_DIAGNOSTIC_CLOSED_STDERR_TEST";
        if std::env::var_os(PROBE).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let mut record = Record::new("closed-stderr-control", "test-only");
            // Force the bounded-record refusal, then the real stderr EPIPE.
            record.set("oversized", serde_json::json!("x".repeat(32769)));
            drop(record);
            return;
        }
        use std::os::{fd::OwnedFd, unix::net::UnixStream};
        use std::process::{Command, Stdio};
        let (writer, reader) = UnixStream::pair().unwrap();
        drop(reader);
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "checker_process::diagnostics::tests::failed_record_and_closed_stderr_do_not_change_exit", "--nocapture"])
            .env(PROBE, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::from(OwnedFd::from(writer)))
            .output().unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("1 passed; 0 failed"),
            "control must execute: {out:?}"
        );
    }
}
