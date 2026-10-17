//! Physical containment for an invocation that retains all checked capabilities.
//! The supervisor receives output bytes, never an Equivalence/Completed/report.
//! A fixed start/finish protocol fences terminal I/O; it is not a proof protocol.

use std::time::Duration;

const STAGING: &str = "ZENO_FCIS_CHECKER_STAGING";
#[cfg(target_os = "linux")]
const MAX_STAGED_RECEIPTS: usize = 16;
/// Shell diagnostic only: neither field supplies a verdict or cleanup authority.
#[derive(Debug, Eq, PartialEq)]
struct SupervisorFailure {
    reason: String,
    cleanup: Option<String>,
}
impl From<String> for SupervisorFailure {
    fn from(reason: String) -> Self {
        Self {
            reason,
            cleanup: None,
        }
    }
}
impl std::fmt::Display for SupervisorFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)?;
        if let Some(physical) = &self.cleanup {
            write!(f, "; process-group-cleanup-incomplete; {physical}")?;
        }
        Ok(())
    }
}
impl SupervisorFailure {
    #[cfg(target_os = "linux")]
    fn cleanup(primary: Option<String>, physical: String) -> Self {
        Self {
            reason: primary.unwrap_or_else(|| "process-group-cleanup-incomplete".into()),
            cleanup: Some(physical),
        }
    }
    fn report(&self, offline: bool) -> serde_json::Value {
        let mut report = serde_json::json!({"schema":if offline {"zeno-fcis/transform-result/1"}else{"zeno-fcis/transform-loop/1"},
            "status":"inconclusive","authority":"none","reason":self.reason,"accounting_complete":false});
        self.add_cleanup(&mut report, false);
        report
    }
    fn add_cleanup(&self, report: &mut serde_json::Value, sanitized: bool) {
        if let Some(physical) = &self.cleanup {
            report["cleanup"] = serde_json::json!({"status":"incomplete", "reason":"process-group-cleanup-incomplete", "absence_verified":false});
            if !sanitized {
                report["cleanup"]["physical"] = serde_json::json!(physical);
            }
        }
    }
}
/// Name prefix of a supervisor-owned staging directory beside final receipts.
pub(crate) const STAGING_PREFIX: &str = ".zeno-checker-stage-";

fn parent_directory(path: &std::path::Path) -> &std::path::Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
}

/// Publish only after obtaining and checking the directory sync handle. A failed
/// post-link sync rolls the link back; every cleanup failure remains explicit.
fn publish_receipt(staged: &std::path::Path, receipt: &std::path::Path) -> std::io::Result<()> {
    publish_receipt_with(staged, receipt, std::fs::File::sync_all)
}

fn publish_receipt_with(
    staged: &std::path::Path,
    receipt: &std::path::Path,
    mut sync: impl FnMut(&std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = std::fs::File::open(parent_directory(receipt))?;
    sync(&parent)?;
    std::fs::hard_link(staged, receipt)?;
    if let Err(error) = sync(&parent) {
        if let Err(cleanup) = std::fs::remove_file(receipt) {
            return Err(std::io::Error::other(format!(
                "receipt-publication-sync-failed:{error}; receipt-rollback-unlink-failed:{cleanup}; receipt-may-exist"
            )));
        }
        if let Err(cleanup) = sync(&parent) {
            return Err(std::io::Error::other(format!(
                "receipt-publication-sync-failed:{error}; receipt-rollback-sync-failed:{cleanup}; receipt-absent-durability-uncertain"
            )));
        }
        return Err(std::io::Error::other(format!(
            "receipt-publication-sync-failed:{error}; receipt-removed"
        )));
    }
    Ok(())
}

/// Fully written private inode; publication is one exclusive hard-link. In a
/// contained invocation the supervisor owns and cleans its staging namespace.
pub(crate) struct StagedReceipt {
    staged: std::path::PathBuf,
    final_path: std::path::PathBuf,
    sequence: u64,
    supervised: bool,
    retained: bool,
}
impl StagedReceipt {
    pub(crate) fn new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::Write;
        if bytes.len() > 65_536 {
            return Err(std::io::Error::other("receipt-byte-cap"));
        }
        let parent = parent_directory(path);
        let namespace = std::env::var_os(STAGING).map(std::path::PathBuf::from);
        if let Some(namespace) = &namespace
            && std::fs::canonicalize(parent_directory(namespace))? != std::fs::canonicalize(parent)?
        {
            return Err(std::io::Error::other("receipt-staging-parent-mismatch"));
        }
        let sequence = crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // The supervisor names a handed-over file from its sequence alone.
        let staged = match &namespace {
            Some(namespace) => namespace.join(format!(".zeno-receipt-{sequence}")),
            None => parent.join(format!(".zeno-receipt-{}-{sequence}", std::process::id())),
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        let pending = Self {
            staged,
            final_path: path.to_path_buf(),
            sequence,
            supervised: namespace.is_some(),
            retained: false,
        };
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(pending)
    }
    /// Publish in this process: one exclusive hard link, then a directory sync.
    pub(crate) fn publish(self) -> std::io::Result<()> {
        publish_receipt(&self.staged, &self.final_path)
    }
    /// In a contained run, leave the staged file to the supervisor, which
    /// links it only after it has accepted how the whole run ended. Outside
    /// containment this is an ordinary in-process publication.
    pub(crate) fn publish_after_supervision(mut self) -> std::io::Result<()> {
        if !(self.supervised && contained()) {
            return self.publish();
        }
        hand_over(self.sequence).map_err(std::io::Error::other)?;
        self.retained = true;
        Ok(())
    }
}
impl Drop for StagedReceipt {
    fn drop(&mut self) {
        if !self.retained {
            let _ = std::fs::remove_file(&self.staged);
        }
    }
}

#[cfg(all(test, unix))]
fn test_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected success: {error:?}"),
    }
}

#[cfg(all(test, unix))]
fn test_err<T: std::fmt::Debug, E>(result: Result<T, E>) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("expected refusal: {value:?}"),
    }
}

#[cfg(all(test, target_os = "linux"))]
fn test_some<T>(value: Option<T>, context: &str) -> T {
    match value {
        Some(value) => value,
        None => panic!("{context}"),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    const CONTROL: &str = "ZENO_FCIS_CHECKER_CONTROL";
    const UNIT: &str = "ZENO_FCIS_CHECKER_UNIT";
    const CALLER_CPUS: &str = "ZENO_FCIS_CHECKER_CPUS";
    const CALLER_NICE: &str = "ZENO_FCIS_CHECKER_NICE";
    /// The control channel is the inherited standard input socket. It has no
    /// file system name, so TMPDIR length and other same-user processes
    /// cannot reach it; SO_PEERCRED of a socket pair names only its creator.
    const STDIN_CHANNEL: &str = "stdin";
    const OUTPUT_LIMIT: usize = 1024 * 1024;
    const MEMORY: u64 = 64 * 1024 * 1024;
    const STARTUP_HELLO: u8 = 0xA0;
    const STARTUP_ACK: u8 = 0xA1;
    const CHECK_STAGE: u8 = 1;
    const SEARCH_STAGE: u8 = 2;
    const FINISH_STAGE: u8 = 3;
    const RECEIPT_HANDOVER: u8 = 4;
    const EXIT_REPORT: u8 = 5;
    /// The child's own deadline starts before it requests a stage; the
    /// supervisor kills only this long after the requested stage ends, so a
    /// cooperative stop is recorded with its progress instead of a kill.
    const STAGE_GRACE: Duration = Duration::from_secs(1);
    /// Whole-run caps from spawn: each deadline ceiling (60 s offline, the
    /// loop's 20 s session) plus a margin for startup, grace and teardown.
    const OFFLINE_WHOLE: Duration = Duration::from_secs(65);
    const LOOP_WHOLE: Duration = Duration::from_secs(25);
    /// Highest CPU index accepted from an affinity list; larger is refused.
    const MAX_CPU: usize = 8191;
    static CLIENT: OnceLock<Mutex<Client>> = OnceLock::new();
    static INTERRUPTED: AtomicI32 = AtomicI32::new(0);
    static SIGNAL_WATCH: OnceLock<Result<(), String>> = OnceLock::new();
    #[cfg(test)]
    type PhysicalObservation = (String, String, bool, Vec<u32>, bool);
    #[cfg(test)]
    pub(super) static LAST_PHYSICAL: Mutex<Option<PhysicalObservation>> = Mutex::new(None);

    struct Client {
        writer: UnixStream,
        replies: mpsc::Receiver<u8>,
        closing: Arc<AtomicBool>,
        monitor: Option<JoinHandle<()>>,
        active: bool,
    }

    fn group() -> Result<PathBuf, String> {
        let text = fs::read_to_string("/proc/self/cgroup").map_err(|e| e.to_string())?;
        let relative = text
            .lines()
            .find_map(|s| s.strip_prefix("0::"))
            .ok_or("cgroup-v2-unavailable")?;
        Ok(Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/')))
    }

    fn verify_group(p: &Path, unit: &str) -> Result<(), String> {
        if p.file_name().and_then(|s| s.to_str()) != Some(unit) {
            return Err("wrong-contained-unit".into());
        }
        let number = |name: &str| -> Result<u64, String> {
            fs::read_to_string(p.join(name))
                .map_err(|e| e.to_string())?
                .trim()
                .parse()
                .map_err(|_| format!("unbounded-{name}"))
        };
        if number("memory.max")? > MEMORY
            || number("memory.swap.max")? != 0
            || number("pids.max")? > 64
        {
            return Err("ineffective-process-limits".into());
        }
        Ok(())
    }

    /// Kernel CPU list syntax (`0-3,8`) as a bounded membership vector.
    pub(super) fn cpu_set(list: &str) -> Result<Vec<bool>, String> {
        let mut set = Vec::new();
        for part in list.trim().split(',').filter(|part| !part.is_empty()) {
            let (low, high) = part.split_once('-').unwrap_or((part, part));
            let parse = |text: &str| -> Result<usize, String> {
                text.trim()
                    .parse()
                    .ok()
                    .filter(|cpu| *cpu <= MAX_CPU)
                    .ok_or_else(|| format!("invalid-cpu-list:{list}"))
            };
            let (low, high) = (parse(low)?, parse(high)?);
            if low > high {
                return Err(format!("invalid-cpu-list:{list}"));
            }
            if set.len() <= high {
                set.resize(high + 1, false);
            }
            set[low..=high].iter_mut().for_each(|cpu| *cpu = true);
        }
        if !set.contains(&true) {
            return Err(format!("empty-cpu-list:{list}"));
        }
        Ok(set)
    }

    /// This process's CPU affinity and nice value, read without privileges.
    pub(super) fn scheduling() -> Result<(String, i64), String> {
        let status = fs::read_to_string("/proc/self/status").map_err(|e| e.to_string())?;
        let cpus = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            .map(|list| list.trim().to_owned())
            .ok_or("cpu-affinity-unavailable")?;
        cpu_set(&cpus)?;
        let stat = fs::read_to_string("/proc/self/stat").map_err(|e| e.to_string())?;
        // Fields after the parenthesized command name start at `state` (3);
        // `nice` is field 19.
        let nice = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(16))
            .and_then(|field| field.parse().ok())
            .ok_or("nice-unavailable")?;
        Ok((cpus, nice))
    }

    /// The contained child may run on no CPU and at no priority its caller
    /// could not; the supervisor passes both and systemd applies them.
    fn verify_scheduling() -> Result<(), String> {
        let caller_cpus = std::env::var(CALLER_CPUS).map_err(|_| "missing-caller-cpus")?;
        let caller_nice: i64 = std::env::var(CALLER_NICE)
            .ok()
            .and_then(|nice| nice.parse().ok())
            .ok_or("missing-caller-nice")?;
        let (own_cpus, own_nice) = scheduling()?;
        let allowed = cpu_set(&caller_cpus)?;
        let own = cpu_set(&own_cpus)?;
        if own
            .iter()
            .enumerate()
            .any(|(cpu, set)| *set && !allowed.get(cpu).copied().unwrap_or(false))
        {
            return Err(format!("cpu-affinity-escaped:{own_cpus}"));
        }
        if own_nice < caller_nice {
            return Err(format!("nice-escaped:{own_nice}"));
        }
        Ok(())
    }

    fn await_startup_ack(stream: &mut UnixStream) -> Result<(), String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| format!("startup-ack-timeout: {e}"))?;
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .map_err(|e| format!("startup-ack-unavailable: {e}"))?;
        if byte[0] != STARTUP_ACK {
            return Err("wrong-startup-ack".into());
        }
        stream
            .set_read_timeout(None)
            .map_err(|e| format!("startup-ack-timeout-reset: {e}"))
    }

    pub(super) fn child() -> Result<bool, String> {
        let Some(channel) = std::env::var_os(CONTROL) else {
            return Ok(false);
        };
        if channel != STDIN_CHANNEL {
            return Err("unknown-control-channel".into());
        }
        let unit = std::env::var(UNIT).map_err(|_| "missing-contained-unit")?;
        verify_group(&group()?, &unit)?;
        verify_scheduling()?;
        // A duplicate of the inherited socket; a non-socket fails below.
        let mut writer = UnixStream::from(
            std::io::stdin()
                .as_fd()
                .try_clone_to_owned()
                .map_err(|e| e.to_string())?,
        );
        writer
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&[STARTUP_HELLO])
            .map_err(|e| e.to_string())?;
        // Do not start the monitor or application until the parent has recorded
        // and validated the actual group. Fast refusal must not outrun custody.
        await_startup_ack(&mut writer)?;
        let mut reader = writer.try_clone().map_err(|e| e.to_string())?;
        let closing = Arc::new(AtomicBool::new(false));
        let watch = Arc::clone(&closing);
        let (tx, replies) = mpsc::sync_channel(1);
        // Parent death closes the control socket. Exit the execution process,
        // causing systemd's KillMode=control-group to terminate descendants.
        let monitor = std::thread::Builder::new()
            .name("checker-parent-watch".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                loop {
                    let mut byte = [0];
                    if reader.read_exact(&mut byte).is_err() {
                        if !watch.load(Ordering::Acquire) {
                            std::process::exit(crate::BLOCKED.into());
                        }
                        break;
                    }
                    if tx.try_send(byte[0]).is_err() {
                        if !watch.load(Ordering::Acquire) {
                            std::process::exit(crate::BLOCKED.into());
                        }
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        CLIENT
            .set(Mutex::new(Client {
                writer,
                replies,
                closing,
                monitor: Some(monitor),
                active: false,
            }))
            .map_err(|_| "duplicate-control-client")?;
        Ok(true)
    }

    pub(super) fn contained() -> bool {
        CLIENT.get().is_some()
    }

    fn exchange(kind: u8, value: u64) -> Result<(), String> {
        let mut client = CLIENT
            .get()
            .ok_or("containment-unavailable")?
            .lock()
            .map_err(|_| "control-poisoned")?;
        if (kind == FINISH_STAGE) != client.active {
            return Err("control-stage-order".into());
        }
        let mut frame = [0; 9];
        frame[0] = kind;
        frame[1..].copy_from_slice(&value.to_be_bytes());
        client.writer.write_all(&frame).map_err(|e| e.to_string())?;
        match client.replies.recv_timeout(Duration::from_secs(1)) {
            Ok(1) => {
                client.active = matches!(kind, CHECK_STAGE | SEARCH_STAGE);
                Ok(())
            }
            _ => Err("stage-deadline-or-control-failure".into()),
        }
    }

    pub(super) fn start(duration: Duration, search: bool) -> Result<(), String> {
        // Unit tests exercise local state transitions without claiming physical
        // containment. Integration tests execute the ordinary non-test binary.
        #[cfg(test)]
        if CLIENT.get().is_none() {
            return Ok(());
        }
        exchange(
            if search { SEARCH_STAGE } else { CHECK_STAGE },
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        )
    }
    pub(super) fn finish() -> Result<(), String> {
        #[cfg(test)]
        if CLIENT.get().is_none() {
            return Ok(());
        }
        exchange(FINISH_STAGE, 0)
    }
    pub(super) fn hand_over(sequence: u64) -> Result<(), String> {
        exchange(RECEIPT_HANDOVER, sequence)
    }
    pub(super) fn shutdown(code: u8) {
        if CLIENT.get().is_none() {
            return;
        }
        // Report the exit code only after every output byte has been written;
        // the supervisor accepts no exit without this final frame.
        if std::io::stdout().flush().is_err() || exchange(EXIT_REPORT, code.into()).is_err() {
            std::process::exit(crate::BLOCKED.into());
        }
        if let Some(client) = CLIENT.get()
            && let Ok(mut c) = client.lock()
        {
            c.closing.store(true, Ordering::Release);
            let _ = c.writer.shutdown(std::net::Shutdown::Both);
            if let Some(monitor) = c.monitor.take() {
                let deadline = Instant::now() + Duration::from_secs(1);
                while !monitor.is_finished() {
                    if Instant::now() >= deadline {
                        std::process::exit(crate::BLOCKED.into());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                if monitor.join().is_err() {
                    std::process::exit(crate::BLOCKED.into());
                }
            }
        }
    }

    /// Block SIGINT, SIGTERM and SIGHUP in the supervisor and receive them on
    /// one watcher thread, so an interrupted run still tears down its unit
    /// and staging instead of dying with them in place.
    fn watch_signals() -> Result<(), String> {
        use nix::sys::signal::{SigSet, Signal};
        let set: SigSet = [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP]
            .into_iter()
            .collect();
        set.thread_block()
            .map_err(|e| format!("signal-mask-unavailable:{e}"))?;
        SIGNAL_WATCH
            .get_or_init(|| {
                std::thread::Builder::new()
                    .name("checker-signal-watch".into())
                    .stack_size(64 * 1024)
                    .spawn(move || {
                        loop {
                            if let Ok(signal) = set.wait() {
                                INTERRUPTED.store(signal as i32, Ordering::Release);
                            }
                        }
                    })
                    .map(|_| ())
                    .map_err(|e| format!("signal-watch-unavailable:{e}"))
            })
            .clone()
    }

    fn interrupted() -> Result<(), String> {
        match INTERRUPTED.load(Ordering::Acquire) {
            0 => Ok(()),
            number => Err(format!(
                "interrupted:{}",
                nix::sys::signal::Signal::try_from(number).map_or("signal", |s| s.as_str())
            )),
        }
    }

    struct Directory {
        staging: Option<PathBuf>,
        writers_gone: bool,
    }
    impl Directory {
        fn clean_staging(&self) -> std::io::Result<()> {
            let Some(path) = &self.staging else {
                return Ok(());
            };
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            };
            // This is a private parent-owned namespace, not a scan of the
            // caller's receipt directory. Never follow or recursively erase it.
            let entries: Vec<_> = entries
                .take(super::MAX_STAGED_RECEIPTS + 1)
                .collect::<std::io::Result<_>>()?;
            if entries.len() > super::MAX_STAGED_RECEIPTS {
                return Err(std::io::Error::other("receipt-staging-entry-cap"));
            }
            for entry in entries {
                fs::remove_file(entry.path())?;
            }
            fs::remove_dir(path)
        }

        /// Link the handed-over receipt; called only after the whole run was
        /// accepted and every contained writer has gone.
        fn publish(&self, sequence: u64, receipt: &Path) -> Result<(), String> {
            let staging = self.staging.as_ref().ok_or("receipt-staging-unavailable")?;
            let staged = staging.join(format!(".zeno-receipt-{sequence}"));
            let metadata = fs::symlink_metadata(&staged).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > 65_536 {
                return Err("handed-over-receipt-not-a-bounded-file".into());
            }
            super::publish_receipt(&staged, receipt).map_err(|e| e.to_string())
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            if self.writers_gone {
                let _ = self.clean_staging();
            }
        }
    }
    #[cfg(test)]
    pub(super) fn remove_owned_staging(path: PathBuf) -> std::io::Result<()> {
        let directory = Directory {
            staging: Some(path),
            writers_gone: true,
        };
        directory.clean_staging()
    }

    /// Own the output socket and its bytes without a blocking reader thread.
    struct CapturedOutput {
        stream: UnixStream,
        bytes: Vec<u8>,
        limit: usize,
        closed: bool,
    }
    impl CapturedOutput {
        fn new(limit: usize) -> Result<(Self, Stdio), String> {
            let capacity = limit.checked_add(1).ok_or("contained-output-cap")?;
            let (stream, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
            stream.set_nonblocking(true).map_err(|e| e.to_string())?;
            let descriptor: std::os::fd::OwnedFd = writer.into();
            Ok((
                Self {
                    stream,
                    bytes: Vec::with_capacity(capacity),
                    limit,
                    closed: false,
                },
                Stdio::from(descriptor),
            ))
        }

        fn read_available(&mut self, deadline: Instant) -> Result<(), String> {
            if self.bytes.len() > self.limit {
                return Err("contained-output-cap".into());
            }
            let mut buffer = [0_u8; 4096];
            while !self.closed {
                if Instant::now() >= deadline {
                    return Err("contained-output-deadline".into());
                }
                let take = (self.limit + 1 - self.bytes.len()).min(buffer.len());
                match self.stream.read(&mut buffer[..take]) {
                    Ok(0) => self.closed = true,
                    Ok(n) => {
                        self.bytes.extend_from_slice(&buffer[..n]);
                        if self.bytes.len() > self.limit {
                            return Err("contained-output-cap".into());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok(())
        }

        /// One line of at most 512 bytes for a refusal reason.
        fn excerpt(&self) -> String {
            let text = String::from_utf8_lossy(&self.bytes);
            let mut line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if line.len() > 512 {
                let mut end = 512;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                line.truncate(end);
            }
            line
        }
    }

    fn reap_before(child: &mut Child, deadline: Instant) -> Result<(), String> {
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("control-process-cleanup-unknown".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn control_process(
        mut command: Command,
        deadline: Instant,
        limit: usize,
    ) -> Result<std::process::Output, String> {
        if Instant::now() >= deadline {
            return Err("control-process-deadline".into());
        }
        let (mut stdout, output) = CapturedOutput::new(limit)?;
        let (mut stderr, errors) = CapturedOutput::new(limit)?;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(errors)
            .spawn()
            .map_err(|e| e.to_string())?;
        // Command owns the configured writer descriptors even after spawn.
        drop(command);
        let result = (|| {
            let mut status = None;
            loop {
                if Instant::now() >= deadline {
                    return Err("control-process-deadline".into());
                }
                stdout.read_available(deadline)?;
                stderr.read_available(deadline)?;
                if status.is_none() {
                    status = child.try_wait().map_err(|e| e.to_string())?;
                }
                if stdout.closed
                    && stderr.closed
                    && let Some(status) = status
                {
                    return Ok(std::process::Output {
                        status,
                        stdout: std::mem::take(&mut stdout.bytes),
                        stderr: std::mem::take(&mut stderr.bytes),
                    });
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        if result.is_err() {
            let _ = child.kill();
            if let Err(cleanup) = reap_before(&mut child, Instant::now() + Duration::from_secs(1)) {
                return Err(format!("{cleanup}; {}", result.err().unwrap_or_default()));
            }
        }
        result
    }

    fn manager(args: &[&str], deadline: Instant) -> Result<std::process::Output, String> {
        let mut command = Command::new("/usr/bin/systemctl");
        command.args(args);
        control_process(command, deadline, 8192)
    }

    fn unit_loaded(output: &std::process::Output) -> Result<bool, String> {
        if !output.status.success() {
            return Err("manager-status-unavailable".into());
        }
        let text = std::str::from_utf8(&output.stdout).map_err(|_| "invalid-manager-unit-state")?;
        let mut states = text
            .lines()
            .filter_map(|line| line.strip_prefix("LoadState="));
        let state = states.next();
        if states.next().is_some() {
            return Err("duplicate-manager-unit-state".into());
        }
        match state {
            Some("loaded") => Ok(true),
            Some("not-found") => Ok(false),
            _ => Err("unknown-manager-unit-state".into()),
        }
    }

    /// Decide the invocation's exit from how the unit actually ended. A
    /// still-loaded unit must have exited normally with the waited code; an
    /// unloaded one was collected after success. Either way the child must
    /// have reported that same code after its last output byte.
    pub(super) fn accepted_exit(
        code: u8,
        reported: Option<u8>,
        loaded: bool,
        properties: &str,
    ) -> Result<u8, String> {
        if loaded {
            let property = |name: &str| -> Result<&str, String> {
                let mut values = properties
                    .lines()
                    .filter_map(|line| line.strip_prefix(name)?.strip_prefix('='));
                match (values.next(), values.next()) {
                    (Some(value), None) => Ok(value),
                    _ => Err(format!("unit-property-unavailable:{name}")),
                }
            };
            let result = property("Result")?;
            let main_code = property("ExecMainCode")?;
            let main_status = property("ExecMainStatus")?;
            // ExecMainCode 1 is CLD_EXITED: a normal exit, not a signal or dump.
            if !matches!(result, "success" | "exit-code")
                || main_code != "1"
                || main_status != code.to_string()
            {
                return Err(format!(
                    "unit-ended-abnormally:Result={result},ExecMainCode={main_code},ExecMainStatus={main_status}"
                ));
            }
        } else if code != 0 {
            return Err("unit-state-unavailable".into());
        }
        if reported != Some(code) {
            return Err(format!(
                "exit-report-mismatch:waited={code},reported={}",
                reported.map_or("none".to_owned(), |r| r.to_string())
            ));
        }
        Ok(code)
    }

    fn reset_unit(unit: &str) -> Result<(), String> {
        let reset = manager(
            &["--user", "reset-failed", unit],
            Instant::now() + Duration::from_secs(1),
        );
        if reset.as_ref().is_ok_and(|output| output.status.success()) {
            return Ok(());
        }
        // A completed transient unit may disappear between show and reset.
        // Confirm absence through the manager; never infer it from error text.
        let shown = manager(
            &["--user", "show", "--property=LoadState", unit],
            Instant::now() + Duration::from_secs(1),
        )?;
        if unit_loaded(&shown)? {
            Err(format!("manager-reset-unavailable:{reset:?}"))
        } else {
            Ok(())
        }
    }

    /// A unit with no known group is gone once the manager has no job for it
    /// and it is unloaded, inactive or failed: no process can remain in it.
    fn unit_stopped(unit: &str) -> bool {
        let Ok(shown) = manager(
            &[
                "--user",
                "show",
                "--property=LoadState",
                "--property=ActiveState",
                "--property=Job",
                unit,
            ],
            Instant::now() + Duration::from_secs(1),
        ) else {
            return false;
        };
        let text = String::from_utf8_lossy(&shown.stdout);
        let value = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
        };
        shown.status.success()
            && value("Job") == Some("")
            && (value("LoadState") == Some("not-found")
                || matches!(value("ActiveState"), Some("inactive" | "failed")))
    }

    struct OwnedUnit {
        child: Child,
        unit: String,
        finished: bool,
        group: Option<PathBuf>,
        members: Vec<u32>,
        cleanup_error: Option<String>,
    }
    impl OwnedUnit {
        fn terminate(&mut self) {
            // Stopped before custody: learn the group, if the unit has one,
            // so its members can still be confirmed gone after the kill.
            if self.group.is_none()
                && let Ok(shown) = manager(
                    &[
                        "--user",
                        "show",
                        "--property=ControlGroup",
                        "--value",
                        &self.unit,
                    ],
                    Instant::now() + Duration::from_secs(1),
                )
                && shown.status.success()
                && let Ok(relative) = String::from_utf8(shown.stdout)
                && relative.trim().starts_with('/')
            {
                self.group =
                    Some(Path::new("/sys/fs/cgroup").join(relative.trim().trim_start_matches('/')));
            }
            if let Some(group) = &self.group {
                self.members = fs::read_to_string(group.join("cgroup.procs"))
                    .unwrap_or_default()
                    .lines()
                    .take(64)
                    .filter_map(|s| s.parse().ok())
                    .collect();
            }
            // systemd controls all descendants, including new process groups.
            // Kernel/manager cleanup latency is not a hard realtime bound.
            match manager(
                &[
                    "--user",
                    "kill",
                    "--kill-whom=all",
                    "--signal=KILL",
                    &self.unit,
                ],
                Instant::now() + Duration::from_secs(1),
            ) {
                Ok(output) if output.status.success() => {}
                Ok(output) => self.cleanup_error = Some(format!("manager-kill:{}", output.status)),
                Err(error) => self.cleanup_error = Some(format!("manager-kill:{error}")),
            }
            let _ = self.child.kill();
            if let Err(error) =
                reap_before(&mut self.child, Instant::now() + Duration::from_secs(1))
            {
                self.cleanup_error = Some(match self.cleanup_error.take() {
                    Some(previous) => format!("{previous}; {error}"),
                    None => error,
                });
            }
            self.finished = true;
        }
    }
    impl Drop for OwnedUnit {
        fn drop(&mut self) {
            if !self.finished {
                self.terminate();
            }
        }
    }

    /// Respect the effective enclosing qualification quota when creating a
    /// systemd sibling unit. A four-CPU benchmark therefore needs a four-CPU
    /// parent allocation; the ordinary AUX lane remains one CPU.
    fn quota_percent(text: &str) -> Result<Option<u64>, String> {
        let fields: Vec<_> = text.split_whitespace().collect();
        if fields.len() != 2 {
            return Err("caller-cpu-quota-malformed".into());
        }
        let period = fields[1]
            .parse::<u64>()
            .ok()
            .filter(|v| *v != 0)
            .ok_or("caller-cpu-quota-malformed")?;
        if fields[0] == "max" {
            return Ok(None);
        }
        let quota = fields[0]
            .parse::<u64>()
            .ok()
            .filter(|v| *v != 0)
            .ok_or("caller-cpu-quota-malformed")?;
        // Integer percentages round down conservatively. Refuse a quota
        // smaller than our representable unit rather than rounding it up.
        let percent = u128::from(quota) * 100 / u128::from(period);
        if percent == 0 {
            return Err("caller-cpu-quota-below-one-percent".into());
        }
        Ok(Some(u64::try_from(percent).unwrap_or(u64::MAX)))
    }

    fn cpu_percent(workers: u64) -> Result<u64, String> {
        let path = group().map_err(|error| format!("caller-cpu-quota-unavailable:{error}"))?;
        chain_percent(Path::new("/sys/fs/cgroup"), path, workers)
    }

    /// The tightest `cpu.max` from the caller's group up to the cgroup-v2
    /// root, which has none. A group without the cpu controller has no
    /// `cpu.max` and inherits its ancestors' limits; a missing group, or an
    /// unreadable or malformed limit, refuses.
    fn chain_percent(root: &Path, mut path: PathBuf, workers: u64) -> Result<u64, String> {
        let mut percent = workers.clamp(1, 8) * 100;
        while path != root {
            if !path.starts_with(root) {
                return Err("caller-cpu-quota-group-outside-root".into());
            }
            if !path.join("cgroup.procs").is_file() {
                return Err(format!(
                    "caller-cpu-quota-unavailable:{}:not a cgroup",
                    path.display()
                ));
            }
            match fs::read_to_string(path.join("cpu.max")) {
                Ok(text) => {
                    if let Some(quota) = quota_percent(&text)? {
                        percent = percent.min(quota);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "caller-cpu-quota-unavailable:{}:{error}",
                        path.display()
                    ));
                }
            }
            if !path.pop() {
                return Err("caller-cpu-quota-group-outside-root".into());
            }
        }
        Ok(percent)
    }

    /// Where the contained run may stage and, for an offline check, the
    /// final receipt the supervisor links once it has accepted the run.
    pub(super) struct Receipts<'a> {
        pub(super) staging_parent: Option<&'a Path>,
        pub(super) receipt: Option<&'a Path>,
    }

    pub(super) fn supervise(
        args: &[std::ffi::OsString],
        offline: bool,
        workers: u64,
        receipts: Receipts<'_>,
    ) -> Result<u8, super::SupervisorFailure> {
        watch_signals()?;
        interrupted()?;
        let sequence = crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let (caller_cpus, caller_nice) = scheduling()?;
        let cpu_quota = cpu_percent(workers)?;
        let mut dir = Directory {
            staging: None,
            writers_gone: true,
        };
        if let Some(parent) = receipts.staging_parent {
            use std::os::unix::fs::DirBuilderExt;
            let staging = parent.join(format!(
                "{}{}-{sequence}",
                super::STAGING_PREFIX,
                std::process::id()
            ));
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&staging)
                .map_err(|e| e.to_string())?;
            dir.staging = Some(staging);
        }
        let (mut control, channel) = UnixStream::pair().map_err(|e| e.to_string())?;
        control.set_nonblocking(true).map_err(|e| e.to_string())?;
        let unit = format!("zeno-checker-{}-{sequence}.service", std::process::id());
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let whole = if offline { OFFLINE_WHOLE } else { LOOP_WHOLE };
        let (mut stdout, output) = CapturedOutput::new(OUTPUT_LIMIT)?;
        let (mut stderr, errors) = CapturedOutput::new(8192)?;
        let mut child = Command::new("/usr/bin/systemd-run");
        child
            .args(["--user", "--wait", "--pipe", "--quiet"])
            .arg("--working-directory")
            .arg(std::env::current_dir().map_err(|e| e.to_string())?)
            .arg(format!("--unit={unit}"))
            .args([
                "--property=MemoryMax=64M",
                "--property=MemorySwapMax=0",
                "--property=TasksMax=64",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=1",
            ])
            .arg(format!("--property=RuntimeMaxSec={}", whole.as_secs() + 2))
            .arg(format!("--property=CPUQuota={cpu_quota}%"))
            // The user manager would otherwise run the child on every CPU
            // at its own priority, outside the caller's scheduling policy.
            .arg(format!("--property=CPUAffinity={caller_cpus}"))
            .arg(format!("--setenv={CONTROL}={STDIN_CHANNEL}"))
            .arg(format!("--setenv={UNIT}={unit}"))
            .arg(format!("--setenv={CALLER_CPUS}={caller_cpus}"))
            .arg(format!("--setenv={CALLER_NICE}={}", caller_nice.max(0)));
        if caller_nice > 0 {
            child.arg(format!("--property=Nice={caller_nice}"));
        }
        if let Some(staging) = &dir.staging {
            child.arg(format!("--setenv={}={}", super::STAGING, staging.display()));
        }
        child
            .arg(executable)
            .args(args)
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(channel)))
            .stdout(output)
            .stderr(errors);
        let started = Instant::now();
        let end = started + whole;
        let mut owned = OwnedUnit {
            child: child
                .spawn()
                .map_err(|e| format!("containment-unavailable:systemd-run:{e}"))?,
            unit,
            finished: false,
            group: None,
            members: Vec::new(),
            cleanup_error: None,
        };
        // Command owns the child's socket end; only the unit may keep it.
        drop(child);
        dir.writers_gone = false;
        let mut connected = false;
        let mut frame = [0; 9];
        let mut used = 0;
        let mut frames = 0;
        let mut active: Option<Instant> = None;
        let mut terminal = false;
        let mut handed_over: Option<u64> = None;
        let mut reported: Option<u8> = None;
        let result: Result<u8, String> = (|| {
            loop {
                interrupted()?;
                let now = Instant::now();
                if now >= end || active.is_some_and(|deadline| now >= deadline) {
                    return Err("contained-deadline".into());
                }
                let output_deadline = active.unwrap_or(end).min(end);
                stdout.read_available(output_deadline)?;
                stderr.read_available(output_deadline)?;
                if !connected {
                    let mut hello = [0];
                    match control.read(&mut hello) {
                        Ok(1) if hello[0] == STARTUP_HELLO => {
                            let shown = manager(
                                &[
                                    "--user",
                                    "show",
                                    "--property=ControlGroup",
                                    "--value",
                                    &owned.unit,
                                ],
                                end.min(Instant::now() + Duration::from_secs(1)),
                            )?;
                            let relative = String::from_utf8(shown.stdout)
                                .map_err(|_| "invalid-control-group")?;
                            if !shown.status.success() || !relative.trim().starts_with('/') {
                                return Err("control-group-unavailable".into());
                            }
                            let actual_group = owned.group.insert(
                                Path::new("/sys/fs/cgroup")
                                    .join(relative.trim().trim_start_matches('/')),
                            );
                            verify_group(actual_group, &owned.unit)?;
                            if Instant::now() >= end {
                                return Err("contained-deadline".into());
                            }
                            // Record custody before ACK, including on a failed write.
                            control
                                .write_all(&[STARTUP_ACK])
                                .map_err(|e| e.to_string())?;
                            connected = true;
                        }
                        Ok(0) => {}
                        Ok(_) => return Err("wrong-startup-hello".into()),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e.to_string()),
                    }
                } else {
                    match control.read(&mut frame[used..]) {
                        Ok(0) if used != 0 => return Err("truncated-control-frame".into()),
                        Ok(0) => {} // Child exit is confirmed below, never interpreted as a verdict.
                        Ok(n) => {
                            used += n;
                            if used == 9 {
                                used = 0;
                                frames += 1;
                                if frames > 64 {
                                    return Err("control-frame-cap".into());
                                }
                                let value = u64::from_be_bytes(
                                    frame[1..].try_into().map_err(|_| "control-frame")?,
                                );
                                if reported.is_some() {
                                    return Err("control-frame-after-exit-report".into());
                                }
                                match frame[0] {
                                    CHECK_STAGE | SEARCH_STAGE if active.is_none() => {
                                        let cap = if offline {
                                            60_000
                                        } else if frame[0] == SEARCH_STAGE {
                                            5_000
                                        } else {
                                            2_000
                                        };
                                        // A run with no time left records its
                                        // deadline without asking for a stage.
                                        if value == 0 || value > cap {
                                            return Err("stage-limit".into());
                                        }
                                        active = Some(
                                            (Instant::now()
                                                + Duration::from_millis(value)
                                                + STAGE_GRACE)
                                                .min(end),
                                        );
                                        terminal = false;
                                    }
                                    FINISH_STAGE if active.is_some() && value == 0 => {
                                        if active.is_some_and(|deadline| Instant::now() >= deadline)
                                        {
                                            return Err("contained-deadline".into());
                                        }
                                        active = None;
                                        terminal = true;
                                    }
                                    RECEIPT_HANDOVER
                                        if offline
                                            && terminal
                                            && active.is_none()
                                            && handed_over.is_none()
                                            && dir.staging.is_some()
                                            && receipts.receipt.is_some() =>
                                    {
                                        handed_over = Some(value);
                                    }
                                    EXIT_REPORT if active.is_none() && value <= 255 => {
                                        reported = u8::try_from(value).ok();
                                    }
                                    _ => return Err("control-stage-order".into()),
                                }
                                control.write_all(&[1]).map_err(|e| e.to_string())?;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                if let Some(status) = owned.child.try_wait().map_err(|e| e.to_string())? {
                    if active.is_some() {
                        return Err(format!(
                            "worker-{}-during-stage",
                            crate::neural_loop::session::WorkerFailure::Died.name(),
                        ));
                    }
                    if !connected {
                        let _ = stderr.read_available(Instant::now() + Duration::from_millis(100));
                        return Err(startup_failure(&stderr.excerpt(), status));
                    }
                    // Non-checking loop commands may finish with no stage.
                    if offline && !terminal {
                        return Err("missing-terminal-permit".into());
                    }
                    return u8::try_from(status.code().ok_or("worker-signal")?)
                        .map_err(|_| "worker-exit".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        if result.is_err() {
            owned.terminate();
        } else {
            owned.finished = true;
        }
        drop(control);
        let drain_deadline = Instant::now() + Duration::from_secs(1);
        let drained = (|| {
            while !stdout.closed || !stderr.closed {
                stdout.read_available(drain_deadline)?;
                stderr.read_available(drain_deadline)?;
                if !stdout.closed || !stderr.closed {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            Ok::<(), String>(())
        })();
        let status = manager(
            &[
                "--user",
                "show",
                "--property=LoadState",
                "--property=Result",
                "--property=ExecMainCode",
                "--property=ExecMainStatus",
                "--property=MemoryMax",
                "--property=MemorySwapMax",
                "--property=TasksMax",
                "--property=CPUQuotaPerSecUSec",
                "--property=CPUAffinity",
                "--property=Nice",
                &owned.unit,
            ],
            Instant::now() + Duration::from_secs(1),
        );
        let terminal_state = status
            .as_ref()
            .map_err(|error| format!("manager-status-unavailable:{error}"))
            .and_then(unit_loaded);
        let mut physical = if terminal_state == Ok(false) {
            // Properties of an unloaded unit are defaults, not observations
            // of this invocation's limits or exit. Startup custody, the
            // child's exit report and the waited code remain checked.
            "TerminalUnitLoadState=not-found\n".to_owned()
        } else {
            status
                .as_ref()
                .ok()
                .filter(|s| s.status.success())
                .map(|s| String::from_utf8_lossy(&s.stdout).into_owned())
                .unwrap_or_default()
        };
        physical.push_str(&format!(
            "ObservedControlGroup={}\nCallerCpusAllowed={caller_cpus}\nCallerNice={caller_nice}\n",
            owned
                .group
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        ));
        if let Some(error) = &owned.cleanup_error {
            physical.push_str(&format!("ControlCleanupError={error}\n"));
        }
        let reset = if terminal_state == Ok(false) {
            Ok(())
        } else {
            reset_unit(&owned.unit)
        };
        let mut group_gone = false;
        for _ in 0..100 {
            group_gone = match &owned.group {
                Some(group) => !group.exists(),
                None => unit_stopped(&owned.unit),
            };
            if group_gone
                && owned
                    .members
                    .iter()
                    .all(|pid| !Path::new("/proc").join(pid.to_string()).exists())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        #[cfg(test)]
        {
            *super::test_ok(LAST_PHYSICAL.lock()) = Some((
                owned.unit.clone(),
                physical.clone(),
                group_gone,
                owned.members.clone(),
                owned
                    .members
                    .iter()
                    .all(|pid| !Path::new("/proc").join(pid.to_string()).exists()),
            ));
        }
        // Only remove staging after all contained writers have disappeared.
        // A failure here keeps the public result inconclusive and names it.
        dir.writers_gone = group_gone
            && owned
                .members
                .iter()
                .all(|pid| !Path::new("/proc").join(pid.to_string()).exists());
        if !dir.writers_gone {
            return Err(super::SupervisorFailure::cleanup(result.err(), physical));
        }
        let accepted = (|| {
            let waited = result?;
            drained?;
            let loaded = terminal_state?;
            reset?;
            let code = accepted_exit(waited, reported, loaded, &physical)?;
            interrupted()?;
            match (handed_over, receipts.receipt) {
                (Some(sequence), Some(receipt)) if code == crate::OK => {
                    dir.publish(sequence, receipt)
                        .map_err(|error| format!("receipt-publication:{error}"))?;
                    Ok((code, true))
                }
                (Some(_), _) => Err("receipt-handover-without-success".to_owned()),
                (None, _) => Ok((code, false)),
            }
        })();
        let cleaned = dir.clean_staging();
        let (code, published) = accepted.map_err(|reason| format!("{reason}; {physical}"))?;
        if let Err(error) = cleaned {
            if !published {
                return Err(format!("staging-cleanup-incomplete:{error}; {physical}").into());
            }
            // The run was accepted and its receipt linked: a leftover private
            // staging entry is reported, never turned into a second outcome.
            let _ = writeln!(
                std::io::stderr(),
                "zeno-fcis: receipt staging cleanup incomplete: {error}"
            );
        }
        // Acceptance/publication is final. An output transport failure cannot
        // change it into an inconclusive result after a receipt was linked.
        let diagnostic = std::io::stderr().write_all(&stderr.bytes);
        let output = std::io::stdout().write_all(&stdout.bytes);
        if let Err(error) = diagnostic.and(output) {
            let _ = writeln!(
                std::io::stderr(),
                "zeno-fcis: accepted result output unavailable: {error}"
            );
        }
        Ok(code)
    }

    fn startup_failure(stderr: &str, status: std::process::ExitStatus) -> String {
        let prerequisite = if stderr.contains("Failed to connect to bus") {
            "systemd-user-manager"
        } else {
            "systemd-run"
        };
        let detail = if stderr.is_empty() {
            format!("exit:{status}")
        } else {
            stderr.to_owned()
        };
        format!("containment-unavailable:{prerequisite}:{detail}")
    }
    #[cfg(test)]
    mod control_tests {
        use super::*;
        use crate::checker_process::{test_err, test_ok};

        #[test]
        fn missing_user_manager_is_primary_and_cleanup_is_a_separate_unverified_failure() {
            use std::os::unix::process::ExitStatusExt;
            let primary = startup_failure(
                "Failed to connect to bus: Operation not permitted",
                std::process::ExitStatus::from_raw(256),
            );
            assert!(primary.starts_with("containment-unavailable:systemd-user-manager:"));
            let failure = super::super::SupervisorFailure::cleanup(
                Some(primary.clone()),
                "ObservedControlGroup=\n".into(),
            );
            let report = failure.report(false);
            assert_eq!(report["reason"], primary);
            assert_eq!(
                report["cleanup"]["reason"],
                "process-group-cleanup-incomplete"
            );
            assert_eq!(report["cleanup"]["absence_verified"], false);
            assert_eq!(report["authority"], "none");
            assert_eq!(report["accounting_complete"], false);
            let mut capture = serde_json::json!({"authority":"none"});
            failure.add_cleanup(&mut capture, true);
            assert!(capture["cleanup"].get("physical").is_none());
            let cleanup_only = super::super::SupervisorFailure::cleanup(
                None,
                "ObservedControlGroup=/unknown\n".into(),
            );
            assert_eq!(
                cleanup_only.report(false)["reason"],
                "process-group-cleanup-incomplete"
            );
        }

        #[test]
        fn caller_cpu_quota_is_explicit_conservative_and_fail_closed() {
            assert_eq!(test_ok(quota_percent("max 100000")), None);
            assert_eq!(test_ok(quota_percent("400000 100000")), Some(400));
            assert_eq!(test_ok(quota_percent("1999 100000")), Some(1));
            assert_eq!(
                test_ok(quota_percent("18446744073709551615 1")),
                Some(u64::MAX)
            );
            for text in ["1000 200000", "1000 1000000"] {
                assert_eq!(
                    test_err(quota_percent(text)),
                    "caller-cpu-quota-below-one-percent"
                );
            }
            for text in [
                "",
                "max",
                "max 0",
                "max no",
                "0 100000",
                "1 0",
                "-1 100000",
                "oops 100000",
                "1 2 3",
            ] {
                assert_eq!(
                    test_err(quota_percent(text)),
                    "caller-cpu-quota-malformed",
                    "{text}"
                );
            }
        }

        #[test]
        fn a_group_without_the_cpu_controller_inherits_its_ancestors_quota() {
            let root = std::env::temp_dir().join(format!(
                "zeno-cgroup-chain-{}-{}",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            // A terminal scope without the cpu controller has no cpu.max.
            let groups = [
                "user.slice",
                "user.slice/app.slice",
                "user.slice/app.slice/vte.scope",
            ];
            let leaf = root.join(groups[2]);
            test_ok(fs::create_dir_all(&leaf));
            for group in groups {
                test_ok(fs::write(root.join(group).join("cgroup.procs"), ""));
            }
            assert_eq!(test_ok(chain_percent(&root, leaf.clone(), 4)), 400);
            test_ok(fs::write(
                root.join(groups[0]).join("cpu.max"),
                "max 100000",
            ));
            test_ok(fs::write(
                root.join(groups[1]).join("cpu.max"),
                "200000 100000",
            ));
            assert_eq!(test_ok(chain_percent(&root, leaf.clone(), 4)), 200);
            assert_eq!(test_ok(chain_percent(&root, leaf.clone(), 1)), 100);
            // Every limit that is present still has to be readable and valid.
            test_ok(fs::write(leaf.join("cpu.max"), "oops 100000"));
            assert_eq!(
                test_err(chain_percent(&root, leaf.clone(), 4)),
                "caller-cpu-quota-malformed"
            );
            test_ok(fs::write(leaf.join("cpu.max"), "50000 100000"));
            assert_eq!(test_ok(chain_percent(&root, leaf.clone(), 4)), 50);
            {
                use std::os::unix::fs::PermissionsExt as _;
                let limit = leaf.join("cpu.max");
                test_ok(fs::set_permissions(
                    &limit,
                    fs::Permissions::from_mode(0o000),
                ));
                let unreadable = fs::read_to_string(&limit).is_err();
                let refusal = chain_percent(&root, leaf.clone(), 4);
                test_ok(fs::set_permissions(
                    &limit,
                    fs::Permissions::from_mode(0o600),
                ));
                if unreadable {
                    assert!(test_err(refusal).starts_with("caller-cpu-quota-unavailable:"));
                }
            }
            // A group that is not a cgroup, or outside the root, refuses.
            assert!(
                test_err(chain_percent(&root, root.join("missing.scope"), 4))
                    .starts_with("caller-cpu-quota-unavailable:")
            );
            assert_eq!(
                test_err(chain_percent(&root, PathBuf::from("/elsewhere"), 4)),
                "caller-cpu-quota-group-outside-root"
            );
            let _ = fs::remove_dir_all(&root);
        }

        fn python(script: &str) -> Command {
            let mut command = Command::new("/usr/bin/python3");
            command.args(["-c", script]);
            command
        }

        fn pid_path() -> PathBuf {
            std::env::temp_dir().join(format!(
                "zeno-control-process-{}-{}.pid",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            ))
        }

        fn assert_reaped(path: &Path) {
            let pid: u32 = test_ok(test_ok(fs::read_to_string(path)).parse());
            assert!(!Path::new("/proc").join(pid.to_string()).exists());
            test_ok(fs::remove_file(path));
        }

        #[test]
        fn control_capture_preserves_exact_cap_bytes_and_nonzero_exit() {
            let command =
                python("import os; os.write(1, b'out'); os.write(2, b'err'); raise SystemExit(7)");
            let output = test_ok(control_process(
                command,
                Instant::now() + Duration::from_secs(3),
                3,
            ));
            assert_eq!(output.status.code(), Some(7));
            assert_eq!(output.stdout, b"out");
            assert_eq!(output.stderr, b"err");
        }

        #[test]
        fn control_capture_deadline_kills_and_reaps_an_actual_sleeping_process() {
            let path = pid_path();
            assert!(!path.exists());
            let mut command = python(
                "import os, sys, time\nwith open(sys.argv[1], 'x') as f: f.write(str(os.getpid()))\ntime.sleep(60)",
            );
            command.arg(&path);
            let started = Instant::now();
            let error = test_err(control_process(
                command,
                started + Duration::from_millis(500),
                8192,
            ));
            assert!(error.contains("deadline"), "{error}");
            assert!(started.elapsed() < Duration::from_secs(3));
            assert_reaped(&path);
        }

        #[test]
        fn control_capture_cap_plus_one_refuses_both_output_streams_and_reaps() {
            for descriptor in [1, 2] {
                let path = pid_path();
                assert!(!path.exists());
                let mut command = python(
                    "import os, sys, time\nwith open(sys.argv[1], 'x') as f: f.write(str(os.getpid()))\nos.write(int(sys.argv[2]), b'x' * 9)\ntime.sleep(60)",
                );
                command.arg(&path).arg(descriptor.to_string());
                let error = test_err(control_process(
                    command,
                    Instant::now() + Duration::from_secs(3),
                    8,
                ));
                assert_eq!(error, "contained-output-cap");
                assert_reaped(&path);
            }
        }

        #[test]
        fn control_capture_expired_deadline_refuses_before_process_spawn() {
            let path = pid_path();
            assert!(!path.exists());
            let mut command =
                python("import sys\nwith open(sys.argv[1], 'x') as f: f.write('ran')");
            command.arg(&path);
            assert_eq!(
                test_err(control_process(command, Instant::now(), 8)),
                "control-process-deadline"
            );
            assert!(!path.exists());
        }

        #[test]
        fn terminal_unit_state_requires_one_successful_known_response() {
            use std::os::unix::process::ExitStatusExt;
            let response = |code, bytes: &[u8]| std::process::Output {
                status: std::process::ExitStatus::from_raw(code),
                stdout: bytes.to_vec(),
                stderr: Vec::new(),
            };
            assert_eq!(unit_loaded(&response(0, b"LoadState=loaded\n")), Ok(true));
            assert_eq!(
                unit_loaded(&response(0, b"LoadState=not-found\n")),
                Ok(false)
            );
            for bytes in [
                &b""[..],
                &b"Result=success\n"[..],
                &b"LoadState=error\n"[..],
                &b"LoadState=masked\n"[..],
                &b"LoadState=loaded\nLoadState=not-found\n"[..],
                &b"LoadState=not-found\n\xff"[..],
            ] {
                assert!(unit_loaded(&response(0, bytes)).is_err());
            }
            assert!(unit_loaded(&response(256, b"LoadState=not-found\n")).is_err());
        }

        #[test]
        fn terminal_query_confirms_an_actual_unloaded_unit() {
            let unit = format!(
                "zeno-unloaded-control-{}-{}.service",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            );
            let response = test_ok(manager(
                &["--user", "show", "--property=LoadState", &unit],
                Instant::now() + Duration::from_secs(3),
            ));
            assert_eq!(unit_loaded(&response), Ok(false));
        }

        #[test]
        fn resetting_an_actual_unloaded_unit_requires_confirmed_absence() {
            let unit = format!(
                "zeno-unloaded-reset-{}-{}.service",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            );
            assert_eq!(reset_unit(&unit), Ok(()));
        }
    }

    #[cfg(test)]
    mod startup_tests {
        use super::*;
        use crate::checker_process::{test_err, test_ok};

        #[test]
        fn startup_ack_accepts_only_the_startup_byte_and_restores_blocking_reads() {
            let (mut child, mut parent) = test_ok(UnixStream::pair());
            test_ok(parent.write_all(&[STARTUP_ACK]));
            assert_eq!(await_startup_ack(&mut child), Ok(()));
            assert_eq!(test_ok(child.read_timeout()), None);
            assert!(CLIENT.get().is_none(), "handshake must not spawn a monitor");
        }

        #[test]
        fn wrong_or_closed_startup_ack_refuses_and_releases_the_socket() {
            let (mut child, mut parent) = test_ok(UnixStream::pair());
            test_ok(parent.write_all(&[1])); // A stage permit is not startup ACK.
            assert_eq!(
                await_startup_ack(&mut child),
                Err("wrong-startup-ack".into())
            );
            drop(child);
            assert_eq!(test_ok(parent.read(&mut [0])), 0);
            let (mut child, parent) = test_ok(UnixStream::pair());
            drop(parent);
            assert!(
                test_err(await_startup_ack(&mut child)).starts_with("startup-ack-unavailable:")
            );
            assert!(
                CLIENT.get().is_none(),
                "failed handshake must not spawn a monitor"
            );
        }

        #[test]
        fn missing_startup_ack_times_out_without_starting_a_monitor() {
            let (mut child, mut parent) = test_ok(UnixStream::pair());
            let started = Instant::now();
            assert!(
                test_err(await_startup_ack(&mut child)).starts_with("startup-ack-unavailable:")
            );
            assert!(started.elapsed() >= Duration::from_millis(900));
            assert!(started.elapsed() < Duration::from_secs(3));
            assert!(CLIENT.get().is_none());
            drop(child);
            assert_eq!(test_ok(parent.read(&mut [0])), 0);
        }
    }
}

/// The commands `dispatch` runs in a contained transient unit, with whether
/// the profile is offline and its worker count: every `loop` command and the
/// parallel transform check.
/// Shared loop route used by dispatch and its effect declaration.
pub(crate) const fn loop_profile() -> Option<(bool, u64)> {
    Some((false, 4))
}

pub(crate) fn contained_profile(command: &crate::Command) -> Option<(bool, u64)> {
    match command {
        crate::Command::Transform {
            command:
                crate::transform_command::Command::Check {
                    offline_parallel: true,
                    workers,
                    ..
                },
        } => Some((true, workers.unwrap_or(4))),
        crate::Command::Loop { .. } => loop_profile(),
        _ => None,
    }
}

/// Run the complete invocation under an externally controlled process, or
/// initialize the child control channel before application work begins.
pub(crate) fn dispatch(
    command: &crate::Command,
    knowledge_snapshot: Option<&str>,
    knowledge_invocation: Option<&str>,
    knowledge_task: Option<&str>,
    knowledge_paths: Option<&[std::path::PathBuf]>,
) -> Option<u8> {
    let profile = contained_profile(command)?;
    let (staging_parent, receipt) = match command {
        crate::Command::Transform {
            command:
                crate::transform_command::Command::Check {
                    receipt: Some(path),
                    ..
                },
        } => (
            Some(parent_directory(path).to_path_buf()),
            Some(path.as_path()),
        ),
        crate::Command::Loop {
            command:
                crate::loop_command::Command::Candidate { session, .. }
                | crate::loop_command::Command::Run { session, .. },
        } => (Some(session.join("receipts")), None),
        _ => (None, None),
    };
    let acquisition = match command {
        crate::Command::Loop { command } => {
            use crate::neural_loop::acquisition::{Acquisition, Mode};
            let mode = match command {
                crate::loop_command::Command::Open { .. } => Mode::Open,
                crate::loop_command::Command::Encode { .. } => Mode::Encode,
                _ => Mode::Resume,
            };
            Some(Acquisition::reserve(mode))
        }
        _ => None,
    };
    #[cfg(target_os = "linux")]
    let outcome = match linux::child() {
        Ok(true) => return None,
        Ok(false) => linux::supervise(
            &{
                let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
                if let Some(hash) = knowledge_snapshot {
                    args.extend([
                        std::ffi::OsString::from("--kb-snapshot"),
                        std::ffi::OsString::from(hash),
                    ]);
                }
                if let Some(id) = knowledge_invocation {
                    args.extend([
                        std::ffi::OsString::from("--kb-invocation"),
                        std::ffi::OsString::from(id),
                    ]);
                }
                if let Some(task) = knowledge_task
                    && !task.is_empty()
                {
                    args.extend([
                        std::ffi::OsString::from("--kb-task"),
                        std::ffi::OsString::from(task),
                    ]);
                }
                if let Some(paths) = knowledge_paths {
                    for p in paths {
                        args.extend([
                            std::ffi::OsString::from("--kb"),
                            p.as_os_str().to_os_string(),
                        ]);
                    }
                }
                args
            },
            profile.0,
            profile.1,
            linux::Receipts {
                staging_parent: staging_parent.as_deref(),
                receipt,
            },
        ),
        Err(error) => {
            // The supervisor quotes the child's stderr in its refusal.
            eprintln!("zeno-fcis: contained child refused: {error}");
            Err(error.into())
        }
    };
    #[cfg(not(target_os = "linux"))]
    let outcome: Result<u8, SupervisorFailure> = {
        let _ = (
            staging_parent,
            receipt,
            knowledge_snapshot,
            knowledge_invocation,
            knowledge_task,
            knowledge_paths,
        );
        Err("containment-unavailable:platform".to_owned().into())
    };
    Some(match outcome {
        Ok(code) => code,
        Err(failure) => {
            let reason = &failure.reason;
            let mut exit_code = crate::BLOCKED;
            let mut report = failure.report(profile.0);
            // Capture never falls back to host execution. Sanitize supervisor
            // diagnostics before they can reach an example-only proposer API.
            if let crate::Command::Loop { command } = command
                && crate::loop_command::is_capture(command)
            {
                let unavailable = reason.starts_with("containment-unavailable")
                    || !cfg!(all(target_os = "linux", target_arch = "x86_64"));
                exit_code = if unavailable {
                    crate::INVALID
                } else {
                    crate::BLOCKED
                };
                report = serde_json::json!({"schema":"zeno-fcis/black-box-capture/1","door":"capture",
                    "status":if unavailable {"refused"}else{"inconclusive"},"authority":"none",
                    "detail":{"reason":if unavailable {"capture-containment-unavailable"}else{"capture-contained-invocation-stopped"},
                        "attempt_spent":if unavailable {serde_json::json!(false)}else{serde_json::Value::Null},"accounting_complete":false}});
                if reason.starts_with("containment-unavailable:systemd-user-manager:") {
                    report["detail"]["prerequisite"] = serde_json::json!("systemd-user-manager");
                }
                failure.add_cleanup(&mut report, true);
            }
            if let Some(acquisition) = acquisition {
                let mut intake = acquisition.json();
                // A killed child's counts are unknown. Its entire allowance
                // remains reserved; zero observed work would be misleading.
                intake["observed_intake"] = serde_json::Value::Null;
                intake["accounting_complete"] = serde_json::Value::Bool(false);
                report["acquisition"] = intake;
            }
            crate::print_json(&report);
            exit_code
        }
    })
}

pub(crate) fn start(duration: Duration, search: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::start(duration, search)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (duration, search);
        Err("containment-unavailable:platform".into())
    }
}
pub(crate) fn finish() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::finish()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("containment-unavailable:platform".into())
    }
}
/// Whether this process is a contained child with a live control channel.
/// Observed scheduling policy of this process; diagnostics only.
pub(crate) fn scheduling_evidence() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        match linux::scheduling() {
            Ok((cpus, nice)) => serde_json::json!({"cpu_affinity": cpus, "nice": nice}),
            Err(reason) => serde_json::json!({"unavailable": reason}),
        }
    }
    #[cfg(not(target_os = "linux"))]
    serde_json::Value::Null
}

pub(crate) fn contained() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::contained()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
fn hand_over(sequence: u64) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::hand_over(sequence)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = sequence;
        Err("containment-unavailable:platform".into())
    }
}
/// Report the exit code to the supervisor after the last output byte.
pub(crate) fn shutdown(code: u8) {
    #[cfg(target_os = "linux")]
    linux::shutdown(code);
    #[cfg(not(target_os = "linux"))]
    let _ = code;
}

// Receipt publication is the same on every Unix host, so these tests also run on macOS.
#[cfg(all(test, unix))]
mod publication_tests {
    use super::*;

    #[test]
    fn staged_receipt_is_complete_exclusive_and_removed_without_permit() {
        let dir = std::env::temp_dir().join(format!("zeno-stage-test-{}", std::process::id()));
        test_ok(std::fs::create_dir(&dir));
        let final_path = dir.join("receipt.json");
        {
            let _staged = test_ok(StagedReceipt::new(&final_path, b"complete\n"));
            assert!(!final_path.exists());
        }
        assert_eq!(test_ok(std::fs::read_dir(&dir)).count(), 0);
        test_ok(test_ok(StagedReceipt::new(&final_path, b"complete\n")).publish());
        assert!(
            test_ok(StagedReceipt::new(&final_path, b"replacement\n"))
                .publish()
                .is_err()
        );
        assert_eq!(test_ok(std::fs::read(&final_path)), b"complete\n");
        test_ok(std::fs::remove_file(final_path));
        test_ok(std::fs::remove_dir(dir));
    }

    #[test]
    fn receipt_publication_preopens_the_parent_before_linking() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("zeno-publish-denied-{}", std::process::id()));
        test_ok(std::fs::create_dir(&dir));
        let receipt = dir.join("receipt.json");
        let staged = test_ok(StagedReceipt::new(&receipt, b"complete\n"));
        test_ok(std::fs::set_permissions(
            &dir,
            std::fs::Permissions::from_mode(0o300),
        ));
        // The test runs as an ordinary user: creation/linking is possible but
        // acquiring the read-only sync handle must fail before publication.
        let outcome = staged.publish();
        test_ok(std::fs::set_permissions(
            &dir,
            std::fs::Permissions::from_mode(0o700),
        ));
        assert_eq!(
            test_err(outcome).kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(!receipt.exists());
        assert_eq!(test_ok(std::fs::read_dir(&dir)).count(), 0);
        test_ok(std::fs::remove_dir(dir));
    }

    #[test]
    fn receipt_publication_rolls_back_sync_failure_and_reports_cleanup_failure() {
        let dir = std::env::temp_dir().join(format!("zeno-publish-sync-{}", std::process::id()));
        test_ok(std::fs::create_dir(&dir));
        let staged = dir.join("staged");
        let receipt = dir.join("receipt.json");
        test_ok(std::fs::write(&staged, b"complete\n"));
        for rollback_fails in [false, true] {
            let mut calls = 0;
            let error = test_err(publish_receipt_with(&staged, &receipt, |_| {
                calls += 1;
                if calls == 2 || (calls == 3 && rollback_fails) {
                    Err(std::io::Error::other("injected-sync-failure"))
                } else {
                    Ok(())
                }
            }));
            assert_eq!(calls, 3);
            assert!(!receipt.exists());
            let expected = if rollback_fails {
                "receipt-absent-durability-uncertain"
            } else {
                "receipt-removed"
            };
            assert!(error.to_string().contains(expected), "{error}");
        }
        let mut calls = 0;
        let error = test_err(publish_receipt_with(&staged, &receipt, |_| {
            calls += 1;
            if calls == 2 {
                // Model a hostile namespace change or failed unlink target.
                test_ok(std::fs::remove_file(&receipt));
                test_ok(std::fs::create_dir(&receipt));
                Err(std::io::Error::other("injected-sync-failure"))
            } else {
                Ok(())
            }
        }));
        assert!(error.to_string().contains("receipt-rollback-unlink-failed"));
        assert!(error.to_string().contains("receipt-may-exist"));
        test_ok(std::fs::remove_dir_all(dir));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::time::Instant;

    // LAST_PHYSICAL is one test-only observation slot. Hold this across each
    // entire parent probe loop and record consumption, including parallel tests.
    static PHYSICAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // These probes are selected only in an actually contained copy of the
    // test binary. An ordinary unit-test invocation does no probe work.
    #[test]
    fn probe_blocked_worker_with_descendant() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        test_ok(start(Duration::from_millis(50), false));
        let mut descendant = test_ok(std::process::Command::new("/bin/sleep").arg("120").spawn());
        std::thread::sleep(Duration::from_secs(10));
        // Reap on the watchdog-failure path; successful containment kills this
        // entire group before these lines and still requires member disappearance.
        test_ok(descendant.kill());
        test_ok(descendant.wait());
        panic!("watchdog failed to terminate blocked worker");
    }

    #[test]
    fn probe_fast_refusal_after_startup_ack() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        let staging = test_some(std::env::var_os(STAGING), "owned staging namespace");
        // Process exit deliberately leaves a private file for the parent to clean.
        test_ok(std::fs::write(
            std::path::Path::new(&staging).join("unpublished"),
            b"private",
        ));
        shutdown(3);
        std::process::exit(3);
    }

    #[test]
    fn fast_refusal_preserves_exit_and_cleans_the_observed_group_and_staging() {
        let _guard = match PHYSICAL_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(error) => panic!("physical test lock poisoned: {error}"),
        };
        let dir = std::env::temp_dir().join(format!(
            "zeno-fast-refusal-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        test_ok(std::fs::create_dir(&dir));
        let final_receipt = dir.join("receipt.json");
        test_ok(std::fs::write(&final_receipt, b"published"));
        let args = [
            "--exact",
            "checker_process::tests::probe_fast_refusal_after_startup_ack",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(std::ffi::OsString::from);
        for _ in 0..3 {
            let receipts = linux::Receipts {
                staging_parent: Some(&dir),
                receipt: None,
            };
            assert_eq!(linux::supervise(&args, false, 1, receipts), Ok(3));
            let (unit, physical, gone, _, members_gone) = test_some(
                test_ok(linux::LAST_PHYSICAL.lock()).take(),
                "actual group and cleanup evidence required",
            );
            println!(
                "fast refusal {unit}: {physical:?}, group_gone={gone}, members_gone={members_gone}"
            );
            assert!(gone && members_gone);
            assert!(physical.lines().any(|s| s == "ExecMainCode=1"));
            assert!(physical.lines().any(|s| s == "ExecMainStatus=3"));
            assert!(physical.lines().any(
                |s| s.starts_with("ObservedControlGroup=/sys/fs/cgroup/") && s.ends_with(&unit)
            ));
            // The caller's affinity reaches the unit (systemd separates
            // ranges with spaces where the kernel uses commas).
            let (cpus, _) = test_ok(linux::scheduling());
            assert!(
                physical
                    .lines()
                    .any(|s| s == format!("CPUAffinity={}", cpus.replace(',', " "))),
                "{physical}"
            );
            assert_eq!(test_ok(std::fs::read(&final_receipt)), b"published");
            assert_eq!(test_ok(std::fs::read_dir(&dir)).count(), 1);
        }
        test_ok(std::fs::remove_dir_all(dir));
    }

    #[test]
    fn probe_allocator_pressure() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        test_ok(start(Duration::from_secs(2), false));
        let mut bytes = Vec::new();
        for _ in 0..128 {
            bytes.extend(std::iter::repeat_n(0xA5_u8, 1024 * 1024));
            std::hint::black_box(&bytes);
        }
        test_ok(finish()); // A normal test panic must never masquerade as OOM.
        panic!("64MiB memory cap did not stop 128MiB touched allocation");
    }

    #[test]
    fn actual_process_deadline_memory_and_descendants_fail_closed() {
        let _guard = match PHYSICAL_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(error) => panic!("physical test lock poisoned: {error}"),
        };
        for name in [
            "checker_process::tests::probe_blocked_worker_with_descendant",
            "checker_process::tests::probe_allocator_pressure",
        ] {
            let args =
                ["--exact", name, "--nocapture", "--test-threads=1"].map(std::ffi::OsString::from);
            let start = Instant::now();
            let receipts = linux::Receipts {
                staging_parent: None,
                receipt: None,
            };
            let result = linux::supervise(&args, false, 1, receipts);
            assert!(result.is_err(), "probe unexpectedly accepted: {result:?}");
            let (unit, physical, gone, members, members_gone) = test_some(
                test_ok(linux::LAST_PHYSICAL.lock()).take(),
                "actual process status required; unavailable is not a passed control",
            );
            println!(
                "physical control {unit}: {physical:?}, group_gone={gone}, members={members:?}, members_gone={members_gone}"
            );
            assert!(gone && members_gone, "actual group/member cleanup required");
            assert!(physical.lines().any(|s| s == "ExecMainCode=2"));
            assert!(physical.lines().any(|s| s == "ExecMainStatus=9"));
            assert!(physical.lines().any(|s| s == "MemoryMax=67108864"));
            assert!(physical.lines().any(|s| s == "MemorySwapMax=0"));
            assert!(physical.lines().any(|s| s == "TasksMax=64"));
            if name.ends_with("descendant") {
                assert!(test_err(result).reason.starts_with("contained-deadline;"));
                assert!(physical.lines().any(|s| s == "Result=signal"));
                assert!(
                    members.len() >= 2,
                    "worker and actual descendant must be observed"
                );
            } else {
                assert!(
                    physical.lines().any(|s| s == "Result=oom-kill"),
                    "panic/exit/timeout is not OOM evidence"
                );
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "cleanup exceeded qualification allowance"
            );
        }
    }

    #[test]
    fn owned_staging_cleanup_is_bounded_and_does_not_touch_final_receipts() {
        let dir = std::env::temp_dir().join(format!(
            "zeno-stage-cleanup-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        test_ok(std::fs::create_dir(&dir));
        let final_receipt = dir.join("receipt.json");
        test_ok(std::fs::write(&final_receipt, b"published"));
        let staging = dir.join("private");
        test_ok(std::fs::create_dir(&staging));
        for index in 0..MAX_STAGED_RECEIPTS {
            test_ok(std::fs::write(
                staging.join(format!("receipt-{index}")),
                b"private",
            ));
        }
        test_ok(linux::remove_owned_staging(staging.clone()));
        assert!(!staging.exists());
        assert_eq!(test_ok(std::fs::read(&final_receipt)), b"published");
        test_ok(std::fs::create_dir(&staging));
        for index in 0..=MAX_STAGED_RECEIPTS {
            test_ok(std::fs::write(
                staging.join(format!("receipt-{index}")),
                b"private",
            ));
        }
        assert!(linux::remove_owned_staging(staging.clone()).is_err());
        assert_eq!(
            test_ok(std::fs::read_dir(&staging)).count(),
            MAX_STAGED_RECEIPTS + 1
        );
        test_ok(std::fs::remove_dir_all(dir));
    }

    #[test]
    fn exit_is_accepted_only_from_a_normal_unit_exit_matching_the_report() {
        let exited = |status: u8| {
            format!("LoadState=loaded\nResult=exit-code\nExecMainCode=1\nExecMainStatus={status}\n")
        };
        let accept = linux::accepted_exit;
        assert_eq!(accept(3, Some(3), true, &exited(3)), Ok(3));
        assert_eq!(
            accept(
                0,
                Some(0),
                true,
                "Result=success\nExecMainCode=1\nExecMainStatus=0\n"
            ),
            Ok(0)
        );
        assert_eq!(accept(0, Some(0), false, ""), Ok(0));
        // An unloaded unit was collected after success; a failure stays loaded.
        assert!(accept(1, Some(1), false, "").is_err());
        // The waited code must equal both the report and the unit's status.
        assert!(accept(1, Some(1), true, &exited(3)).is_err());
        assert!(accept(1, None, true, &exited(1)).is_err());
        assert!(accept(1, Some(0), true, &exited(1)).is_err());
        for abnormal in [
            "Result=oom-kill\nExecMainCode=2\nExecMainStatus=9\n",
            "Result=signal\nExecMainCode=2\nExecMainStatus=15\n",
            "Result=timeout\nExecMainCode=1\nExecMainStatus=1\n",
            "Result=core-dump\nExecMainCode=3\nExecMainStatus=6\n",
            "Result=exit-code\nExecMainCode=2\nExecMainStatus=1\n",
            "Result=exit-code\nExecMainCode=1\n",
            "Result=exit-code\nResult=success\nExecMainCode=1\nExecMainStatus=1\n",
        ] {
            assert!(accept(1, Some(1), true, abnormal).is_err(), "{abnormal}");
        }
    }

    #[test]
    fn cpu_lists_are_bounded_and_exact() {
        let set = test_ok(linux::cpu_set("0-2,5"));
        assert_eq!(set, [true, true, true, false, false, true]);
        for invalid in ["", ",", "3-1", "a", "0-8192", "1,,x"] {
            assert!(linux::cpu_set(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn probe_exit_without_report() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        std::process::exit(0);
    }

    #[test]
    fn probe_allocation_after_terminal_permit() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        test_ok(start(Duration::from_secs(2), false));
        test_ok(finish());
        let mut bytes = Vec::new();
        for _ in 0..128 {
            bytes.extend(std::iter::repeat_n(0xA5_u8, 1024 * 1024));
            std::hint::black_box(&bytes);
        }
        shutdown(0);
        std::process::exit(0);
    }

    #[test]
    fn a_run_ending_without_a_normal_reported_exit_is_never_a_verdict() {
        let _guard = match PHYSICAL_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(error) => panic!("physical test lock poisoned: {error}"),
        };
        for (name, offline, expected) in [
            (
                "checker_process::tests::probe_exit_without_report",
                false,
                "exit-report-mismatch:waited=0,reported=none",
            ),
            (
                "checker_process::tests::probe_allocation_after_terminal_permit",
                true,
                "unit-ended-abnormally:Result=oom-kill",
            ),
        ] {
            let args =
                ["--exact", name, "--nocapture", "--test-threads=1"].map(std::ffi::OsString::from);
            let receipts = linux::Receipts {
                staging_parent: None,
                receipt: None,
            };
            let error = test_err(linux::supervise(&args, offline, 1, receipts));
            let (unit, physical, gone, _, members_gone) = test_some(
                test_ok(linux::LAST_PHYSICAL.lock()).take(),
                "actual process status required",
            );
            println!("abnormal exit {unit}: {error}");
            assert!(error.reason.starts_with(expected), "{error}");
            assert!(gone && members_gone, "{physical}");
        }
    }

    fn handed_over_receipt(then_exceed_memory: bool) {
        let staging = std::path::PathBuf::from(test_some(
            std::env::var_os(STAGING),
            "owned staging namespace",
        ));
        let receipt = test_some(staging.parent(), "receipt directory").join("receipt.json");
        assert_eq!(linux::child(), Ok(true));
        test_ok(start(Duration::from_secs(2), false));
        let staged = test_ok(StagedReceipt::new(&receipt, b"complete\n"));
        test_ok(finish());
        test_ok(staged.publish_after_supervision());
        // Nothing is linked until the supervisor accepts the whole run.
        assert!(!receipt.exists());
        if then_exceed_memory {
            let mut bytes = Vec::new();
            for _ in 0..128 {
                bytes.extend(std::iter::repeat_n(0xA5_u8, 1024 * 1024));
                std::hint::black_box(&bytes);
            }
        }
        shutdown(0);
        std::process::exit(0);
    }

    #[test]
    fn probe_handed_over_receipt() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_some() {
            handed_over_receipt(false);
        }
    }

    #[test]
    fn probe_handed_over_receipt_then_killed() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_some() {
            handed_over_receipt(true);
        }
    }

    #[test]
    fn supervisor_links_a_handed_over_receipt_only_after_accepting_the_run() {
        let _guard = match PHYSICAL_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(error) => panic!("physical test lock poisoned: {error}"),
        };
        for (name, accepted) in [
            ("checker_process::tests::probe_handed_over_receipt", true),
            (
                "checker_process::tests::probe_handed_over_receipt_then_killed",
                false,
            ),
        ] {
            let dir = std::env::temp_dir().join(format!(
                "zeno-handover-{}-{}",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
            test_ok(std::fs::create_dir(&dir));
            let receipt = dir.join("receipt.json");
            let args =
                ["--exact", name, "--nocapture", "--test-threads=1"].map(std::ffi::OsString::from);
            let receipts = linux::Receipts {
                staging_parent: Some(&dir),
                receipt: Some(&receipt),
            };
            let result = linux::supervise(&args, true, 1, receipts);
            let (_, physical, gone, _, members_gone) = test_some(
                test_ok(linux::LAST_PHYSICAL.lock()).take(),
                "actual process status required",
            );
            assert!(gone && members_gone, "{physical}");
            if accepted {
                assert_eq!(result, Ok(0));
                assert_eq!(test_ok(std::fs::read(&receipt)), b"complete\n");
            } else {
                assert!(
                    test_err(result)
                        .reason
                        .starts_with("unit-ended-abnormally:Result=oom-kill")
                );
                assert!(!receipt.exists());
            }
            // Only the published receipt, if any, remains: staging is gone.
            assert_eq!(
                test_ok(std::fs::read_dir(&dir)).count(),
                usize::from(accepted)
            );
            test_ok(std::fs::remove_dir_all(dir));
        }
    }
    #[test]
    fn probe_loop_worker_overrun() {
        if std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_none() {
            return;
        }
        assert_eq!(linux::child(), Ok(true));
        let duration = Duration::from_millis(10);
        test_ok(start(duration, true));
        let result = crate::loop_command::supervise(duration, || {
            std::thread::sleep(Duration::from_millis(100));
        });
        assert!(matches!(
            result,
            Err(crate::neural_loop::session::WorkerFailure::Timeout)
        ));
        test_ok(finish());
        println!("{{\"status\":\"inconclusive\",\"reason\":\"worker-timeout\"}}");
        shutdown(crate::BLOCKED);
        std::process::exit(crate::BLOCKED.into());
    }

    #[test]
    fn loop_worker_overrun_is_reported_before_the_process_stage_kill() {
        let _guard = test_ok(PHYSICAL_TEST_LOCK.lock());
        let args = [
            "--exact",
            "checker_process::tests::probe_loop_worker_overrun",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(std::ffi::OsString::from);
        let result = linux::supervise(
            &args,
            false,
            1,
            linux::Receipts {
                staging_parent: None,
                receipt: None,
            },
        );
        assert_eq!(result, Ok(crate::BLOCKED));
        let (_, physical, gone, _, members_gone) = test_some(
            test_ok(linux::LAST_PHYSICAL.lock()).take(),
            "physical evidence",
        );
        assert!(gone && members_gone, "{physical}");
        assert!(
            physical.lines().any(|line| line == "ExecMainCode=1"),
            "{physical}"
        );
        assert!(
            physical.lines().any(|line| line == "ExecMainStatus=2"),
            "{physical}"
        );
    }
}
