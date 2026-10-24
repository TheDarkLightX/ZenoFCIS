//! Operator-only monotonic observations. Never read for admission, accounting,
//! verdicts or publication. One invocation owns each bounded record exclusively.
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

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
        let retention = Retention::environment()?;
        prune(&root, retention, SystemTime::now()).map_err(|e| e.to_string())?;
        publish_record(&root, &name, |file| {
            file.write_all(&bytes)?;
            file.flush()
        })
        .map_err(|e| e.to_string())?;
        prune(&root, retention, SystemTime::now()).map_err(|e| e.to_string())
    }
}
// Retention is shell housekeeping, never decision or accounting authority.
// A live writer or parent may still have a reader: those files are exempt.
#[derive(Clone, Copy)]
struct Retention {
    bytes: u64,
    age: Duration,
}
impl Retention {
    fn environment() -> Result<Self, String> {
        let number = |name: &str, default| match std::env::var(name) {
            Err(std::env::VarError::NotPresent) => Ok(default),
            Ok(value) => value.parse::<u64>().map_err(|_| format!("invalid-{name}")),
            Err(_) => Err(format!("invalid-{name}")),
        };
        Ok(Self {
            bytes: number("ZENO_FCIS_OPERATOR_MAX_BYTES", 64 * 1024 * 1024)?,
            age: Duration::from_secs(number("ZENO_FCIS_OPERATOR_MAX_AGE_SECONDS", 86400)?),
        })
    }
}

fn prune(root: &Path, retention: Retention, now: SystemTime) -> std::io::Result<()> {
    prune_with(root, retention, now, &mut exempt)
}

/// A live writer or parent may still have a reader: its record is exempt.
fn exempt(path: &Path, pid: u32) -> bool {
    if process_start_ticks(pid).is_some() {
        return true;
    }
    // Open without following links and bound reads even for legacy or
    // malformed files. Malformed matching records of a live writer above
    // remain visible to its reader, which must still reject corruption.
    let mut text = Vec::new();
    if let Ok(file) = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
    {
        let _ = file.take(32769).read_to_end(&mut text);
    }
    if let Ok(record) = serde_json::from_slice::<serde_json::Value>(&text)
        && let Some(parent) = record["parent_pid"]
            .as_u64()
            .and_then(|p| u32::try_from(p).ok())
        && let Some(ticks) = process_start_ticks(parent)
        && record
            .get("parent_start_ticks")
            .is_none_or(|v| v.as_u64() == Some(ticks))
    {
        return true;
    }
    false
}

// The listing reads metadata only. A record is opened, and its writer and parent
// inspected, only when the age or size cap would remove it, oldest first, so a
// directory at its cap costs one listing per write instead of reading every
// record. An exempt record leaves the size total when it is reached; until then
// it counts, so removal of dead writers' records can be slightly more eager.
fn prune_with(
    root: &Path,
    retention: Retention,
    now: SystemTime,
    exempt: &mut dyn FnMut(&Path, u32) -> bool,
) -> std::io::Result<()> {
    let mut completed = Vec::new();
    let mut bytes = 0u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let plain = name.strip_prefix('.').unwrap_or(name);
        let Some(pid) = plain.split('-').next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if !plain.ends_with(".json") && !plain.ends_with(".json.tmp") {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.uid() != nix::unistd::geteuid().as_raw() {
            continue;
        }
        // Account allocated blocks as well as logical length: many one-byte
        // records must not evade the cap. Directory metadata is not included.
        let size = metadata.len().max(metadata.blocks().saturating_mul(512));
        bytes = bytes.saturating_add(size);
        completed.push((metadata.modified()?, entry.path(), size, pid));
    }
    completed.sort_unstable_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    for (modified, path, size, pid) in completed {
        let expired = !retention.age.is_zero()
            && now
                .duration_since(modified)
                .is_ok_and(|age| age > retention.age);
        if !expired && (retention.bytes == 0 || bytes <= retention.bytes) {
            // Oldest first: no later record is expired, and the cap holds.
            break;
        }
        if exempt(&path, pid) {
            bytes = bytes.saturating_sub(size);
            continue;
        }
        match fs::remove_file(path) {
            Ok(()) => bytes = bytes.saturating_sub(size),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                bytes = bytes.saturating_sub(size);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
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
        self.set(
            "parent_start_ticks",
            serde_json::json!(
                self.fields
                    .get("parent_pid")
                    .and_then(|v| v.as_u64())
                    .and_then(|pid| u32::try_from(pid).ok())
                    .and_then(process_start_ticks)
            ),
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
    fn retention_bounds_many_small_files_and_age_but_keeps_live_readers() {
        let root = std::env::temp_dir().join(format!(
            "zeno-retention-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|e| panic!("{e}"));
        // PID 0 cannot own a userspace process. Every byte occupies a block.
        for i in 0..1024 {
            fs::write(root.join(format!("0-{i:04}.json")), b"x").unwrap_or_else(|e| panic!("{e}"));
        }
        let live = root.join(format!("{}-live.json", std::process::id()));
        fs::write(&live, b"{malformed").unwrap_or_else(|e| panic!("{e}"));
        let parent = root.join("0-live-parent.json");
        fs::write(&parent, serde_json::to_vec(&serde_json::json!({
            "parent_pid": std::process::id(), "parent_start_ticks": process_start_ticks(std::process::id())
        })).unwrap_or_else(|e| panic!("{e}"))).unwrap_or_else(|e| panic!("{e}"));
        let bytes = 64 * 1024;
        let policy = Retention {
            bytes,
            age: Duration::ZERO,
        };
        prune(&root, policy, SystemTime::now()).unwrap_or_else(|e| panic!("{e}"));
        let completed_bytes: u64 = fs::read_dir(&root)
            .unwrap_or_else(|e| panic!("{e}"))
            .map(|entry| entry.unwrap_or_else(|e| panic!("{e}")).path())
            .filter(|p| p != &live && p != &parent)
            .map(|p| {
                let m = fs::metadata(p).unwrap_or_else(|e| panic!("{e}"));
                m.len().max(m.blocks() * 512)
            })
            .sum();
        assert!(
            completed_bytes <= bytes,
            "size cap failed: {completed_bytes} > {bytes}"
        );
        assert!(
            live.exists() && parent.exists(),
            "running readers lost diagnostics"
        );
        let future = SystemTime::now() + Duration::from_secs(2);
        prune(
            &root,
            Retention {
                bytes: 0,
                age: Duration::from_secs(1),
            },
            future,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read_dir(&root)
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            2
        );
        assert!(
            live.exists() && parent.exists(),
            "age cap removed live diagnostics"
        );
        fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn a_directory_within_its_caps_is_listed_but_no_record_is_opened() {
        let root = std::env::temp_dir().join(format!(
            "zeno-retention-listing-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|e| panic!("{e}"));
        let base = SystemTime::now() - Duration::from_secs(4000);
        for i in 0..2000u64 {
            let path = root.join(format!("0-{i:04}.json"));
            fs::write(&path, b"x").unwrap_or_else(|e| panic!("{e}"));
            let file = fs::File::options()
                .write(true)
                .open(&path)
                .unwrap_or_else(|e| panic!("{e}"));
            file.set_modified(base + Duration::from_secs(i))
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let size = |path: &Path| {
            let m = fs::metadata(path).unwrap_or_else(|e| panic!("{e}"));
            m.len().max(m.blocks() * 512)
        };
        let total: u64 = (0..2000u64)
            .map(|i| size(&root.join(format!("0-{i:04}.json"))))
            .sum();
        let mut visited = Vec::new();
        let mut count = |path: &Path, _pid: u32| {
            visited.push(path.to_path_buf());
            false
        };
        let at_cap = Retention {
            bytes: total,
            age: Duration::from_secs(86400),
        };
        prune_with(&root, at_cap, SystemTime::now(), &mut count).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            visited.is_empty(),
            "opened {} records under the caps",
            visited.len()
        );
        assert_eq!(
            fs::read_dir(&root)
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            2000
        );
        // Three records over the size cap: exactly the three oldest are visited and removed.
        let block = size(&root.join("0-0000.json"));
        let over = Retention {
            bytes: total - 3 * block,
            age: Duration::from_secs(86400),
        };
        let mut count = |path: &Path, _pid: u32| {
            visited.push(path.to_path_buf());
            false
        };
        prune_with(&root, over, SystemTime::now(), &mut count).unwrap_or_else(|e| panic!("{e}"));
        let oldest: Vec<_> = (0..3u64)
            .map(|i| root.join(format!("0-{i:04}.json")))
            .collect();
        assert_eq!(visited, oldest);
        assert!(oldest.iter().all(|p| !p.exists()));
        assert_eq!(
            fs::read_dir(&root)
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            1997
        );
        fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn records_are_private_complete_and_atomically_visible() {
        use std::sync::mpsc;
        let root = std::env::temp_dir().join(format!(
            "zeno-record-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let (ready, waiting) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let path = root.clone();
        let writer = std::thread::spawn(move || {
            publish_record(&path, "record.json", |f| {
                f.write_all(b"{\"complete\":")?;
                ready.send(()).unwrap_or_else(|e| panic!("{e:?}"));
                resumed.recv().unwrap_or_else(|e| panic!("{e:?}"));
                f.write_all(b"true}")?;
                f.flush()
            })
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(
            !root.join("record.json").exists(),
            "partial JSON must never be visible under final name"
        );
        assert_eq!(
            fs::symlink_metadata(root.join(".record.json.tmp"))
                .unwrap_or_else(|e| panic!("{e:?}"))
                .mode()
                & 0o777,
            0o600
        );
        resume.send(()).unwrap_or_else(|e| panic!("{e:?}"));
        writer
            .join()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("record.json")).unwrap_or_else(|e| panic!("{e:?}")),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(value, serde_json::json!({"complete":true}));
        assert_eq!(
            fs::symlink_metadata(root.join("record.json"))
                .unwrap_or_else(|e| panic!("{e:?}"))
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
        fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e:?}"));
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
        let (writer, reader) = UnixStream::pair().unwrap_or_else(|e| panic!("{e:?}"));
        drop(reader);
        let out = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e:?}")))
            .args(["--exact", "checker_process::diagnostics::tests::failed_record_and_closed_stderr_do_not_change_exit", "--nocapture"])
            .env(PROBE, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::from(OwnedFd::from(writer)))
            .output().unwrap_or_else(|e| panic!("{e:?}"));
        assert!(out.status.success(), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("1 passed; 0 failed"),
            "control must execute: {out:?}"
        );
    }
}
