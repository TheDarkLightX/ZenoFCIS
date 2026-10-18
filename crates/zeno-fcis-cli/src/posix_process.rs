//! Direct children only: no native-command admission or hostile tree claim.
//! macOS shares the limit-policy structure; its runtime remains unqualified.
use crate::containment_policy::{Control, Guarantee, Profile, Quantity, Unit};
use nix::sys::resource::{Resource, getrlimit, setrlimit};

fn limits() -> Vec<(Control, Resource, u64, Unit)> {
    let mut limits = vec![
        (Control::CpuSeconds, Resource::RLIMIT_CPU, 25, Unit::Seconds),
        (
            Control::FileSize,
            Resource::RLIMIT_FSIZE,
            16 * 1024 * 1024,
            Unit::Bytes,
        ),
        (Control::OpenFiles, Resource::RLIMIT_NOFILE, 64, Unit::Count),
        (Control::CoreSize, Resource::RLIMIT_CORE, 0, Unit::Bytes),
    ];
    // Never credit AS/RSS/DATA on macOS as a qualified hard memory bound.
    #[cfg(target_os = "linux")]
    limits.push((
        Control::AddressSpace,
        Resource::RLIMIT_AS,
        64 * 1024 * 1024,
        Unit::Bytes,
    ));
    limits
}

#[cfg(target_os = "linux")]
pub(super) use runtime::*;

#[cfg(target_os = "linux")]
mod runtime {
    use super::*;
    use crate::checker_process::{SupervisorFailure, linux};
    use crate::containment_policy::{
        self as policy, Backend, Cleanup, RunEvidence, Scope, State, Termination, Unavailable,
    };
    use nix::sys::signal::{Signal, killpg};
    use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
    use nix::unistd::{Pid, close, getpgid, getpgrp};
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::os::unix::{
        net::UnixStream,
        process::{CommandExt, ExitStatusExt},
    };
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    const ROLE: &str = "ZENO_FCIS_POSIX_ROLE";
    const INVOCATION: &str = "ZENO_FCIS_POSIX_INVOCATION";
    const EVIDENCE_CAP: usize = 32768;
    // ACK follows limit/environment/FD readback and durable startup custody.
    // Same-core CPU contention measured that journal sync up to 5.378 s (idle
    // ACK max 31 ms); 10 s gives almost twice that observed tail, unlike 1 s.
    // Host5 independently measured systemd:30 idle+30 loaded startups passed;
    // loaded ACK p95=89ms,max=100ms (supervisor custody observations up to240ms).
    // Keep10s for the separately observed durable-I/O tail; samples do not
    // guarantee worst-case scheduling or storage latency.
    // The supervisor's existing 25 s whole-run cap still includes startup;
    // this wait neither extends it nor retries an invocation or admission.
    pub(in super::super) const STARTUP_ACK_BUDGET: Duration = Duration::from_secs(10);
    const ENVIRONMENT: &[&str] = &[
        "LC_ALL",
        "MALLOC_ARENA_MAX",
        ROLE,
        INVOCATION,
        "ZENO_FCIS_CHECKER_CONTROL",
        "ZENO_FCIS_CHECKER_CPUS",
        "ZENO_FCIS_CHECKER_NICE",
        super::super::STAGING,
    ];

    // Owner-private append-only invocation directories. No session is created
    // merely to retain a refusal. Only qualified admission may create a journal.
    fn evidence_root() -> PathBuf {
        Path::new("/tmp").join(format!("zeno-fcis-containment-{}", nix::unistd::geteuid()))
    }

    fn evidence_directory(invocation: &str) -> Result<PathBuf, String> {
        if invocation.len() != 64
            || !invocation
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("evidence-invocation-identity".into());
        }
        Ok(evidence_root().join(invocation))
    }

    fn append_record(
        directory: &Path,
        name: &str,
        value: &impl serde::Serialize,
    ) -> Result<String, String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if bytes.len() > EVIDENCE_CAP {
            return Err("evidence-record-bound".into());
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(directory.join(name))
            .map_err(|e| format!("evidence-append:{name}:{e}"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(crate::transform::sha256_hex(&bytes))
    }

    fn empty_evidence(invocation: &str, build: &str) -> RunEvidence {
        RunEvidence {
            schema: policy::EVIDENCE_SCHEMA.into(),
            backend: Backend::Posix,
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            required_profile: Profile::FiniteData,
            profile_sha256: Profile::FiniteData.hash(),
            policy_build_sha256: build.into(),
            invocation_sha256: invocation.into(),
            subject_sha256: None,
            semantic_receipt_sha256: None,
            guarantees: Vec::new(),
            worker_pid: 0,
            termination: Termination::Running,
            exit_code: None,
            signal: None,
            termination_reason: None,
            cleanup: Cleanup::Pending,
            absence_verified: false,
        }
    }

    #[derive(serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct ReceiptBinding {
        location: String,
        semantic_receipt_sha256: String,
        original_sha256: String,
        candidate_sha256: String,
    }

    /// Exclusive shell state. Drop records interrupted preparation as UNKNOWN;
    /// it cannot mint a semantic result or change canonical receipt bytes.
    pub(in super::super) struct Invocation {
        invocation: String,
        build: String,
        directory: PathBuf,
        session: Option<PathBuf>,
        before_receipts: std::collections::BTreeMap<String, String>,
        finished: bool,
        guardian: bool,
        timing: super::super::diagnostics::Record,
    }

    fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("evidence-input-not-regular".into());
        }
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("evidence-input-bound".into());
        }
        Ok(bytes)
    }

    fn receipt_binding(location: String, bytes: &[u8]) -> Result<ReceiptBinding, String> {
        let receipt: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if receipt["schema"] != "zeno-fcis/transform-receipt/1" {
            return Err("evidence-receipt-schema".into());
        }
        Ok(ReceiptBinding {
            location,
            semantic_receipt_sha256: crate::transform::sha256_hex(bytes),
            original_sha256: receipt["original"]["sha256"]
                .as_str()
                .ok_or("receipt-original-identity")?
                .into(),
            candidate_sha256: receipt["candidate"]["sha256"]
                .as_str()
                .ok_or("receipt-candidate-identity")?
                .into(),
        })
    }

    fn receipts(
        session: Option<&Path>,
        before: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Result<Vec<ReceiptBinding>, String> {
        let Some(session) = session else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        match fs::read_dir(session.join("receipts")) {
            Ok(entries) => {
                for (index, entry) in entries.enumerate() {
                    if index >= 4096 {
                        return Err("evidence-receipt-count".into());
                    }
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.path().extension() == Some(OsStr::new("json"))
                        && entry.file_type().map_err(|e| e.to_string())?.is_file()
                    {
                        let location = format!(
                            "receipts/{}",
                            entry.file_name().to_str().ok_or("receipt-file-name")?
                        );
                        let bytes = bounded_read(&entry.path())?;
                        let hash = crate::transform::sha256_hex(&bytes);
                        if before.is_some_and(|map| map.get(&location) == Some(&hash)) {
                            continue;
                        }
                        if before.is_none() {
                            // Inventory is opaque. Semantic replay, including
                            // corrupt old receipts, remains the worker's job.
                            out.push(ReceiptBinding {
                                location,
                                semantic_receipt_sha256: hash,
                                original_sha256: String::new(),
                                candidate_sha256: String::new(),
                            });
                        } else {
                            out.push(receipt_binding(location, &bytes)?);
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        let capture = session.join("capture.json");
        if capture.exists()
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bounded_read(&capture)?)
            && value["receipt"].is_object()
        {
            let bytes = serde_json::to_vec(&value["receipt"]).map_err(|e| e.to_string())?;
            let binding = receipt_binding("capture.json#/receipt".into(), &bytes)?;
            if !before.is_some_and(|map| {
                map.get(&binding.location) == Some(&binding.semantic_receipt_sha256)
            }) {
                out.push(binding);
            }
        }
        out.sort_by(|a, b| a.location.cmp(&b.location));
        Ok(out)
    }

    impl Invocation {
        pub(in super::super) fn new(
            command: &crate::Command,
            actual: Vec<Guarantee>,
            source: super::super::SelectionSource,
        ) -> Result<Self, String> {
            let mut timing = super::super::diagnostics::Record::new("caller", "posix");
            let mut nonce = [0; 32];
            fs::File::open("/dev/urandom")
                .and_then(|mut f| f.read_exact(&mut nonce))
                .map_err(|e| e.to_string())?;
            let invocation = crate::transform::sha256_hex(&nonce);
            timing.set("invocation_sha256", serde_json::json!(invocation));
            let build = build_identity()?;
            let root = evidence_root();
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
                return Err("evidence-directory-ownership".into());
            }
            let directory = evidence_directory(&invocation)?;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .map_err(|e| e.to_string())?;
            let session =
                crate::loop_command::knowledge_session(command).map(|(p, _)| p.to_path_buf());
            let mut this = Self {
                invocation,
                build,
                directory,
                session,
                before_receipts: Default::default(),
                finished: false,
                guardian: false,
                timing,
            };
            let mut admission = empty_evidence(&this.invocation, &this.build);
            admission.guarantees = actual;
            append_record(&this.directory, "admission.json", &admission)?;
            let mut command_bytes = Vec::new();
            for arg in std::env::args_os().skip(1) {
                let bytes = arg.as_encoded_bytes();
                command_bytes.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
                command_bytes.extend_from_slice(bytes);
            }
            append_record(
                &this.directory,
                "command.json",
                &serde_json::json!({
                    "schema":"zeno-fcis/containment-command/1","caller_pid":std::process::id(),"argv_sha256":crate::transform::sha256_hex(&command_bytes)
                }),
            )?;
            this.before_receipts = receipts(this.session.as_deref(), None)?
                .into_iter()
                .map(|r| (r.location, r.semantic_receipt_sha256))
                .collect();
            append_record(
                &this.directory,
                "receipts-before.json",
                &this.before_receipts,
            )?;
            super::super::set_public_label(
                serde_json::json!({"schema":"zeno-fcis/containment-result/1",
                "backend":"posix","required_profile":"finite-data/1",
                "selection_source":source,
                "evidence":"private-append-only-journal"}),
            );
            Ok(this)
        }

        fn recover(
            command: &crate::Command,
            identity: &str,
            build: &str,
        ) -> Result<Option<Self>, String> {
            // Internal protocol harnesses have no public admission journal.
            let Ok(directory) = evidence_directory(identity) else {
                return Ok(None);
            };
            let before_receipts =
                serde_json::from_slice(&bounded_read(&directory.join("receipts-before.json"))?)
                    .map_err(|e| format!("evidence-receipt-snapshot:{e}"))?;
            Ok(Some(Self {
                invocation: identity.into(),
                build: build.into(),
                directory,
                session: crate::loop_command::knowledge_session(command)
                    .map(|(p, _)| p.to_path_buf()),
                before_receipts,
                finished: false,
                guardian: true,
                timing: super::super::diagnostics::Record::new("guardian-journal", "posix"),
            }))
        }

        fn finish(&mut self, evidence: &mut RunEvidence) -> Result<(), String> {
            // If startup failed or the guardian disappeared, retain an explicit
            // unknown startup rather than inheriting an earlier invocation.
            let startup_path = self.directory.join("startup.json");
            if !startup_path.exists() {
                append_record(
                    &self.directory,
                    "startup.json",
                    &empty_evidence(&self.invocation, &self.build),
                )?;
            }
            let startup_sha256 = crate::transform::sha256_hex(&bounded_read(&startup_path)?);
            let (bindings, binding_error) =
                match receipts(self.session.as_deref(), Some(&self.before_receipts)) {
                    Ok(receipts) => (
                        receipts
                            .into_iter()
                            .filter(|r| {
                                self.before_receipts.get(&r.location)
                                    != Some(&r.semantic_receipt_sha256)
                            })
                            .collect::<Vec<_>>(),
                        None,
                    ),
                    Err(error) => (Vec::new(), Some(error)),
                };
            let worker_output = self.directory.join("worker-output.json");
            let reported: serde_json::Value = if worker_output.exists() {
                serde_json::from_slice(&bounded_read(&worker_output)?).map_err(|e| e.to_string())?
            } else {
                serde_json::Value::Null
            };
            if let [binding] = bindings.as_slice() {
                evidence.subject_sha256 = Some(binding.candidate_sha256.clone());
                evidence.semantic_receipt_sha256 = Some(binding.semantic_receipt_sha256.clone());
            }
            if bindings.is_empty()
                && let Some(rows) = reported["reported_receipts"].as_array()
                && let [row] = rows.as_slice()
            {
                evidence.subject_sha256 = row["candidate_sha256"].as_str().map(str::to_owned);
                evidence.semantic_receipt_sha256 =
                    row["semantic_receipt_sha256"].as_str().map(str::to_owned);
            }
            if evidence.subject_sha256.is_none()
                && let Some(session) = &self.session
                && session.join("original.zcve").is_file()
            {
                evidence.subject_sha256 = Some(crate::transform::sha256_hex(&bounded_read(
                    &session.join("original.zcve"),
                )?));
            }
            // Both observers append their own final record. Guardian records
            // survive caller death; the caller records transport/validation loss.
            let (final_name, envelope_name) = if self.guardian {
                ("guardian-final.json", "guardian-envelope.json")
            } else {
                ("final.json", "envelope.json")
            };
            let final_sha256 = append_record(&self.directory, final_name, &evidence)?;
            append_record(
                &self.directory,
                envelope_name,
                &serde_json::json!({
                    "schema":"zeno-fcis/containment-envelope/1","observer":if self.guardian {"guardian"}else{"caller"},"invocation_sha256":self.invocation,
                    "backend":"posix","required_profile":"finite-data/1",
                    "startup_sha256":startup_sha256,"final_sha256":final_sha256,"receipts":bindings,
                "receipt_bindings_complete":binding_error.is_none(),"receipt_binding_error":binding_error,
                    "worker_output":reported,
                    "command_identity_sha256":crate::transform::sha256_hex(&bounded_read(&self.directory.join("command.json"))?)
                }),
            )?;
            self.finished = true;
            if let Some(error) = binding_error {
                return Err(error);
            }
            // Compatibility snapshot only; the journal/envelope is authoritative
            // for operational provenance. Never create a failed new session.
            if !self.guardian
                && let Some(session) = &self.session
                && fs::symlink_metadata(session).is_ok_and(|m| m.file_type().is_dir())
                && (session.join("request.json").is_file()
                    || session.join("capture.json").is_file())
            {
                super::super::retain_run_evidence(
                    session,
                    &serde_json::to_vec(&evidence).map_err(|e| e.to_string())?,
                )?;
            }
            Ok(())
        }
    }

    impl Drop for Invocation {
        fn drop(&mut self) {
            if !self.finished {
                let mut evidence = empty_evidence(&self.invocation, &self.build);
                evidence.termination = Termination::Stopped;
                evidence.cleanup = Cleanup::Unknown;
                evidence.termination_reason = Some("invocation-stopped-before-final-report".into());
                if let Err(e) = self.finish(&mut evidence) {
                    eprintln!("containment-evidence-unavailable:{e}");
                }
            }
        }
    }

    fn unavailable(missing: impl Into<String>) -> Unavailable {
        Unavailable {
            backend: Backend::Posix,
            missing: missing.into(),
        }
    }

    fn build_identity() -> Result<String, String> {
        // Pin the actual executable inode, including every compiled policy and
        // runtime module. A renamed executable path cannot change this subject.
        // Only parent/guardian read it; the 64 MiB worker never loads a copy.
        let file =
            fs::File::open("/proc/self/exe").map_err(|e| format!("policy-build-digest:{e}"))?;
        let mut bytes = Vec::new();
        file.take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("policy-build-digest:{e}"))?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("policy-build-size".into());
        }
        Ok(crate::transform::sha256_hex(&bytes))
    }

    fn close_inherited_descriptors() -> Result<(), String> {
        // This runs at the very start of a fresh exec, before any application
        // handle or monitor exists. Collect and drop the directory handle
        // before closing raw inherited descriptors; no owned alias escapes.
        let descriptors = fs::read_dir("/proc/self/fd")
            .map_err(|e| format!("fd-allowlist:{e}"))?
            .map(|entry| {
                entry.map_err(|e| e.to_string()).and_then(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .parse::<i32>()
                        .map_err(|e| e.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for fd in descriptors.into_iter().filter(|fd| *fd > 2) {
            match close(fd) {
                Ok(()) | Err(nix::errno::Errno::EBADF) => {}
                Err(e) => return Err(format!("fd-allowlist:{e}")),
            }
        }
        Ok(())
    }

    fn check_environment() -> Result<(), String> {
        if std::env::vars_os().any(|(name, _)| {
            !ENVIRONMENT
                .iter()
                .any(|allowed| name == OsStr::new(allowed))
        }) || std::env::var("LC_ALL").as_deref() != Ok("C")
            || std::env::var("MALLOC_ARENA_MAX").as_deref() != Ok("1")
        {
            return Err("minimal-environment".into());
        }
        Ok(())
    }

    // All three readback boundaries use this pure validation; callers retain
    // observed values rather than copying a required cap into evidence.
    fn tightened_limits(soft: u64, hard: u64, target: u64) -> (u64, u64) {
        (soft.min(target), hard.min(target))
    }

    fn checked_limit(
        control: Control,
        requested: u64,
        expected: Option<(u64, u64)>,
        soft: u64,
        hard: u64,
        unit: Unit,
    ) -> Result<Guarantee, String> {
        if soft > hard || hard > requested || expected.is_some_and(|e| (soft, hard) != e) {
            return Err(format!("{}:readback", control.name()));
        }
        Ok(observed_limit(control, requested, hard, unit))
    }

    fn install_limits() -> Result<Vec<Guarantee>, String> {
        let mut guarantees = Vec::new();
        for (control, resource, requested, unit) in limits() {
            let (soft, hard) =
                getrlimit(resource).map_err(|e| format!("{}:{e}", control.name()))?;
            let effective = tightened_limits(soft, hard, requested);
            setrlimit(resource, effective.0, effective.1)
                .map_err(|e| format!("{}:{e}", control.name()))?;
            let (soft, hard) = getrlimit(resource).map_err(|e| e.to_string())?;
            guarantees.push(checked_limit(
                control,
                requested,
                Some(effective),
                soft,
                hard,
                unit,
            )?);
        }
        Ok(guarantees)
    }

    #[cfg(test)]
    #[test]
    fn leaf_limits_preserve_inherited_soft_and_hard_independently() {
        for (soft, hard, target, expected) in [
            (1024, u64::MAX, 65536, (1024, 65536)),
            (1024, 1024, 65536, (1024, 1024)),
            (1, u64::MAX, 25, (1, 25)),
            (16, 16, 64, (16, 16)),
            (32 << 20, 48 << 20, 64 << 20, (32 << 20, 48 << 20)),
        ] {
            assert_eq!(tightened_limits(soft, hard, target), expected);
            assert!(
                checked_limit(
                    Control::FileSize,
                    target,
                    Some(expected),
                    expected.0,
                    expected.1,
                    Unit::Bytes
                )
                .is_ok()
            );
            assert!(
                checked_limit(
                    Control::FileSize,
                    target,
                    Some(expected),
                    expected.0 + 1,
                    expected.1,
                    Unit::Bytes
                )
                .is_err()
            );
        }
    }

    pub(in super::super) fn write_protocol(
        stream: &UnixStream,
        mut bytes: &[u8],
    ) -> Result<(), String> {
        // UnixStream::write uses send(MSG_NOSIGNAL), which a network-denying
        // caller sandbox may refuse even on a private local socket. write(2)
        // needs no network capability. All channels are nonblocking and fixed
        // protocol bytes retain the same one-second monotonic write budget.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !bytes.is_empty() {
            match nix::unistd::write(stream, bytes) {
                Ok(0) => return Err("control-write-eof".into()),
                Ok(n) => bytes = &bytes[n..],
                Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR) => {}
                Err(e) => return Err(format!("control-write:{e}")),
            }
            if Instant::now() >= deadline {
                return Err("control-write-deadline".into());
            }
            if !bytes.is_empty() {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }

    pub(in super::super) fn await_startup_ack(stream: &mut UnixStream) -> Result<(), String> {
        let started = Instant::now();
        let backend = if is_worker() { "posix" } else { "systemd" };
        // Construction performs no I/O during the guardian's FD snapshot.
        // Identity/record I/O is deferred to Drop, after ACK acceptance/refusal.
        let mut timing = super::super::diagnostics::Record::new("worker-startup", backend);
        timing.set(
            "ack_budget_ms",
            serde_json::json!(STARTUP_ACK_BUDGET.as_millis() as u64),
        );
        if let Ok(unit) = std::env::var("ZENO_FCIS_CHECKER_UNIT") {
            timing.set("unit", serde_json::json!(unit));
        }
        let result = await_startup_ack_before(stream, started + STARTUP_ACK_BUDGET);
        timing.duration("ack_wait_ms", started);
        timing.set("ack_accepted", serde_json::json!(result.is_ok()));
        timing.set("startup_reason", serde_json::json!(result.as_ref().err()));
        result
    }

    fn await_startup_ack_before(stream: &mut UnixStream, deadline: Instant) -> Result<(), String> {
        loop {
            let mut byte = [0];
            let read = stream.read(&mut byte);
            // Check after the nonblocking read, before accepting any reply:
            // descheduling or a queued late ACK cannot bypass the deadline.
            if Instant::now() >= deadline {
                return Err("startup-ack-unavailable:deadline".into());
            }
            match read {
                Ok(1) if byte[0] == 0xA1 => return Ok(()),
                Ok(0) => return Err("startup-ack-unavailable:eof".into()),
                Ok(_) => return Err("wrong-startup-ack".into()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("startup-ack-unavailable:{e}")),
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[cfg(test)]
    mod startup_tests {
        use super::*;

        #[test]
        fn delayed_custody_ack_preserves_the_budget_and_exact_protocol_byte() {
            let (mut child, parent) = UnixStream::pair().unwrap();
            child.set_nonblocking(true).unwrap();
            let writer = std::thread::spawn(move || {
                // A durable custody write can exceed the former one-second wait.
                std::thread::sleep(Duration::from_millis(1200));
                write_protocol(&parent, &[0xA1]).unwrap();
            });
            let started = Instant::now();
            assert_eq!(await_startup_ack(&mut child), Ok(()));
            assert!(started.elapsed() >= Duration::from_millis(1200));
            assert!(STARTUP_ACK_BUDGET < Duration::from_secs(25));
            writer.join().unwrap();
        }

        #[test]
        fn wrong_closed_missing_and_late_startup_ack_all_refuse() {
            let (mut child, parent) = UnixStream::pair().unwrap();
            child.set_nonblocking(true).unwrap();
            write_protocol(&parent, &[1]).unwrap();
            assert_eq!(
                await_startup_ack(&mut child),
                Err("wrong-startup-ack".into())
            );
            drop(parent);
            assert_eq!(
                await_startup_ack(&mut child),
                Err("startup-ack-unavailable:eof".into())
            );
            let (mut child, parent) = UnixStream::pair().unwrap();
            child.set_nonblocking(true).unwrap();
            assert_eq!(
                await_startup_ack_before(&mut child, Instant::now() + Duration::from_millis(20)),
                Err("startup-ack-unavailable:deadline".into())
            );
            write_protocol(&parent, &[0xA1]).unwrap();
            assert_eq!(
                await_startup_ack_before(&mut child, Instant::now()),
                Err("startup-ack-unavailable:deadline".into())
            );
        }
    }

    pub(in super::super) fn is_worker() -> bool {
        std::env::var(ROLE).as_deref() == Ok("worker")
    }

    pub(in super::super) fn verify_worker() -> Result<(), String> {
        check_environment()?;
        if getpgrp().as_raw() != i32::try_from(std::process::id()).map_err(|_| "worker-pid")? {
            return Err("fresh-process-group".into());
        }
        for (control, resource, cap, unit) in limits() {
            let (soft, hard) = getrlimit(resource).map_err(|e| e.to_string())?;
            checked_limit(control, cap, None, soft, hard, unit)?;
        }
        Ok(())
    }

    fn base_command(executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .env_clear()
            .env("LC_ALL", "C")
            .env("MALLOC_ARENA_MAX", "1");
        command
    }

    pub(in super::super) fn worker_command(
        executable: &Path,
        args: &[OsString],
        staging: Option<&Path>,
        cpus: &str,
        nice: i64,
    ) -> Command {
        let mut command = base_command(executable);
        command
            .args(args)
            .env(ROLE, "worker")
            .env("ZENO_FCIS_CHECKER_CONTROL", "stdin")
            .env("ZENO_FCIS_CHECKER_CPUS", cpus)
            .env("ZENO_FCIS_CHECKER_NICE", nice.to_string())
            .process_group(0);
        if let Some(path) = staging {
            command.env(super::super::STAGING, path);
        }
        command
    }

    fn observed(control: Control, scope: Scope) -> Guarantee {
        Guarantee {
            control,
            state: State::Holds,
            scope,
            requested: None,
            effective: None,
        }
    }

    fn observed_limit(control: Control, requested: u64, effective: u64, unit: Unit) -> Guarantee {
        Guarantee {
            control,
            state: State::Holds,
            scope: Scope::TrustedWorker,
            requested: Some(Quantity {
                value: requested,
                unit,
            }),
            effective: Some(Quantity {
                value: effective,
                unit,
            }),
        }
    }

    fn verify_closed_descriptors() -> Result<(), String> {
        let descriptors = fs::read_dir("/proc/self/fd")
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // The enumeration FD is closed before checking the captured names.
        for path in descriptors {
            let fd = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or("fd-allowlist-name")?;
            if fd > 2 && fs::read_link(path).is_ok() {
                return Err("fd-allowlist-readback".into());
            }
        }
        Ok(())
    }

    /// A disposable, quiescent child exercises the same group-kill/reap/absence
    /// path as a worker. No application input, staging or durable write occurs.
    pub(in super::super) fn probe() -> Result<Vec<Guarantee>, Unavailable> {
        let executable =
            std::env::current_exe().map_err(|e| unavailable(format!("executable:{e}")))?;
        let (mut stdout, output) = linux::CapturedOutput::new(8192).map_err(unavailable)?;
        let (mut stderr, errors) = linux::CapturedOutput::new(8192).map_err(unavailable)?;
        let mut command = base_command(&executable);
        command
            .env(ROLE, "probe")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(errors);
        let mut child = command
            .spawn()
            .map_err(|e| unavailable(format!("setup-probe:{e}")))?;
        drop(command);
        let deadline = Instant::now() + Duration::from_secs(3);
        let result = (|| -> Result<Vec<Guarantee>, String> {
            loop {
                stdout.read_available(deadline)?;
                stderr.read_available(deadline)?;
                if stdout.bytes.last() == Some(&b'\n') {
                    let mut actual: Vec<Guarantee> = serde_json::from_slice(&stdout.bytes)
                        .map_err(|_| "setup-probe-readback")?;
                    if poll_child(&child)?.is_some() {
                        return Err("setup-probe-not-live".into());
                    }
                    terminate(&mut child).map_err(|e| format!("owned-cleanup:{e}"))?;
                    actual.push(observed(Control::OwnedCleanup, Scope::TrustedWorker));
                    // These are properties of this installed trusted controller,
                    // not capabilities copied from the required profile.
                    actual.push(observed_limit(Control::Wall, 25, 25, Unit::Seconds));
                    actual.last_mut().ok_or("wall-evidence")?.scope = Scope::Invocation;
                    actual.push(observed(Control::BoundedData, Scope::Invocation));
                    return Ok(actual);
                }
                if poll_child(&child)?.is_some() {
                    return Err(format!(
                        "setup-probe:{}",
                        String::from_utf8_lossy(&stderr.bytes).trim()
                    ));
                }
                if Instant::now() >= deadline {
                    return Err("setup-probe-deadline".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        if result.is_err() {
            // This helper owns no descendants; a denied group signal still
            // permits the independent single-child fallback and bounded reap.
            let _ = child.kill();
            let _ = linux::reap_before(&mut child, Instant::now() + Duration::from_secs(1));
        }
        result.map_err(unavailable)
    }

    /// Observe without reaping. The zombie leader reserves PID/PGID until all
    /// signalling has finished, including normal completion and refusal paths.
    pub(in super::super) fn poll_child(child: &Child) -> Result<Option<ExitStatus>, String> {
        let pid = Pid::from_raw(i32::try_from(child.id()).map_err(|_| "worker-pid")?);
        match waitid(
            Id::Pid(pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        )
        .map_err(|e| format!("worker-wait:{e}"))?
        {
            WaitStatus::StillAlive => Ok(None),
            WaitStatus::Exited(_, code) => Ok(Some(ExitStatus::from_raw(code << 8))),
            WaitStatus::Signaled(_, signal, core) => Ok(Some(ExitStatus::from_raw(
                (signal as i32) | if core { 128 } else { 0 },
            ))),
            _ => Err("worker-wait-state".into()),
        }
    }

    pub(in super::super) fn terminate(child: &mut Child) -> Result<(), String> {
        let pid = Pid::from_raw(i32::try_from(child.id()).map_err(|_| "worker-pid")?);
        let group = getpgid(Some(pid));
        if pid.as_raw() <= 0 || pid == getpgrp() || group != Ok(pid) {
            let _ = child.kill();
            let reaped = linux::reap_before(child, Instant::now() + Duration::from_secs(1));
            return Err(format!(
                "worker-group-identity:{group:?}; individual-reap={reaped:?}"
            ));
        }
        match killpg(pid, Signal::SIGKILL) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
            Err(e) => {
                let _ = child.kill();
                let reaped = linux::reap_before(child, Instant::now() + Duration::from_secs(1));
                return Err(format!("worker-group-kill:{e}; individual-reap={reaped:?}"));
            }
        }
        linux::reap_before(child, Instant::now() + Duration::from_secs(1))?;
        verify_cleanup(pid)
    }

    fn verify_cleanup(pid: Pid) -> Result<(), String> {
        match waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG) {
            Err(nix::errno::Errno::ECHILD) => {}
            other => return Err(format!("worker-owned-reap:{other:?}")),
        }
        // No more signals after releasing the leader identity. This backend
        // admits no untrusted process creation; it promises owned children only.
        match killpg(pid, None) {
            Err(nix::errno::Errno::ESRCH) => Ok(()),
            other => Err(format!("worker-group-absence:{other:?}")),
        }
    }

    pub(in super::super) struct Guardian {
        caller: UnixStream,
        invocation: String,
        build: String,
        evidence: Option<RunEvidence>,
        journal: Option<Invocation>,
    }
    impl Guardian {
        pub(in super::super) fn spawned(&mut self, pid: u32) {
            let mut evidence = empty_evidence(&self.invocation, &self.build);
            evidence.worker_pid = pid;
            self.evidence = Some(evidence);
        }

        pub(in super::super) fn check_caller(&mut self) -> Result<(), String> {
            match self.caller.read(&mut [0]) {
                Ok(0) => Err("caller-eof".into()),
                Ok(_) => Err("unexpected-caller-frame".into()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
                Err(e) => Err(format!("caller-watchdog:{e}")),
            }
        }

        pub(in super::super) fn accept(&mut self, child: &Child) -> Result<(), String> {
            let pid = Pid::from_raw(i32::try_from(child.id()).map_err(|_| "worker-pid")?);
            if pid == getpgrp() || getpgid(Some(pid)).map_err(|e| e.to_string())? != pid {
                return Err("fresh-process-group".into());
            }
            let root = Path::new("/proc").join(child.id().to_string());
            let readback = fs::read_to_string(root.join("limits")).map_err(|e| e.to_string())?;
            let mut guarantees = Vec::new();
            for (control, _, requested, unit) in limits() {
                let label = match control {
                    Control::AddressSpace => "Max address space",
                    Control::CpuSeconds => "Max cpu time",
                    Control::FileSize => "Max file size",
                    Control::OpenFiles => "Max open files",
                    Control::CoreSize => "Max core file size",
                    _ => return Err("unknown-limit".into()),
                };
                let fields: Vec<_> = readback
                    .lines()
                    .find_map(|s| s.strip_prefix(label))
                    .ok_or("limit-readback-missing")?
                    .split_whitespace()
                    .collect();
                let soft = fields
                    .first()
                    .and_then(|s| {
                        if *s == "unlimited" {
                            Some(u64::MAX)
                        } else {
                            s.parse::<u64>().ok()
                        }
                    })
                    .ok_or("limit-readback-soft")?;
                let hard = fields
                    .get(1)
                    .and_then(|s| {
                        if *s == "unlimited" {
                            Some(u64::MAX)
                        } else {
                            s.parse::<u64>().ok()
                        }
                    })
                    .ok_or("limit-readback-hard")?;
                guarantees.push(checked_limit(control, requested, None, soft, hard, unit)?);
            }
            let environment = fs::read(root.join("environ")).map_err(|e| e.to_string())?;
            for pair in environment.split(|b| *b == 0).filter(|s| !s.is_empty()) {
                let name = pair
                    .split(|b| *b == b'=')
                    .next()
                    .ok_or("worker-environment")?;
                if !ENVIRONMENT.iter().any(|s| s.as_bytes() == name) {
                    return Err("worker-environment".into());
                }
            }
            // The worker duplicates stdin once to perform HELLO. This fourth
            // descriptor must be that same protocol socket, never an outside FD.
            let input = fs::read_link(root.join("fd/0")).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(root.join("fd")).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let fd: u32 = entry
                    .file_name()
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "worker-fd")?;
                if fd > 2
                    && (fd != 3 || fs::read_link(entry.path()).map_err(|e| e.to_string())? != input)
                {
                    return Err("worker-fd-allowlist".into());
                }
            }
            guarantees.extend([
                observed(Control::Environment, Scope::TrustedWorker),
                observed(Control::FileDescriptors, Scope::TrustedWorker),
                observed(Control::OwnedCleanup, Scope::TrustedWorker),
                observed(Control::BoundedData, Scope::Invocation),
            ]);
            let mut wall = observed_limit(Control::Wall, 25, 25, Unit::Seconds);
            wall.scope = Scope::Invocation;
            guarantees.push(wall);
            policy::compare(Profile::FiniteData, Backend::Posix, true, &guarantees)
                .map_err(|e| e.to_string())?;
            for control in [
                Control::GroupMemory,
                Control::GroupSwap,
                Control::GroupTasks,
                Control::TreeCleanup,
                Control::LeafMemory,
                Control::LeafIsolation,
                Control::Files,
                Control::Network,
            ] {
                guarantees.push(Guarantee {
                    control,
                    state: State::Absent,
                    scope: if matches!(control, Control::LeafMemory | Control::LeafIsolation) {
                        Scope::PythonLeaf
                    } else if matches!(control, Control::Files | Control::Network) {
                        Scope::TrustedWorker
                    } else {
                        Scope::Group
                    },
                    requested: None,
                    effective: None,
                });
            }
            self.evidence = Some(RunEvidence {
                schema: policy::EVIDENCE_SCHEMA.into(),
                backend: Backend::Posix,
                os: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
                required_profile: Profile::FiniteData,
                profile_sha256: Profile::FiniteData.hash(),
                policy_build_sha256: self.build.clone(),
                invocation_sha256: self.invocation.clone(),
                subject_sha256: None,
                semantic_receipt_sha256: None,
                guarantees,
                worker_pid: child.id(),
                termination: Termination::Running,
                exit_code: None,
                signal: None,
                termination_reason: None,
                cleanup: Cleanup::Pending,
                absence_verified: false,
            });
            if let Ok(directory) = evidence_directory(&self.invocation) {
                append_record(
                    &directory,
                    "startup.json",
                    self.evidence.as_ref().ok_or("startup-evidence")?,
                )?;
            }
            Ok(())
        }

        pub(in super::super) fn observe_output(&mut self, bytes: &[u8]) -> Result<(), String> {
            let Some(journal) = &self.journal else {
                return Ok(());
            };
            let value = serde_json::from_slice::<serde_json::Value>(bytes).ok();
            let mut reported = Vec::new();
            if let Some(value) = &value {
                reported_receipts(value, "", &mut reported)?;
            }
            // These are digests emitted by the trusted checker shell, not
            // guardian equivalence claims. Bind their exact output and location.
            append_record(
                &journal.directory,
                "worker-output.json",
                &serde_json::json!({
                    "schema":"zeno-fcis/containment-worker-output/1", "result_sha256":crate::transform::sha256_hex(bytes),
                    "complete_json":value.is_some(),"reported_receipts":reported
                }),
            )?;
            Ok(())
        }

        pub(in super::super) fn observe_exit(&mut self, status: ExitStatus) {
            if let Some(e) = &mut self.evidence {
                e.signal = status.signal().and_then(|s| u8::try_from(s).ok());
            }
        }

        pub(in super::super) fn finish(&mut self, code: Option<u8>, gone: bool) {
            if let Some(e) = &mut self.evidence {
                e.termination = if code.is_some() {
                    Termination::Exited
                } else {
                    Termination::Stopped
                };
                e.exit_code = code;
                e.cleanup = if gone {
                    Cleanup::OwnedWorkersReaped
                } else {
                    Cleanup::Unknown
                };
                e.absence_verified = gone;
            }
        }
    }

    fn reported_receipts(
        value: &serde_json::Value,
        location: &str,
        out: &mut Vec<serde_json::Value>,
    ) -> Result<(), String> {
        match value {
            serde_json::Value::Object(object) => {
                // The checker shell is trusted to report its own receipts; this
                // metadata only binds reported digests, never supplies a verdict.
                if object
                    .get("checker")
                    .is_some_and(|v| v == crate::transform::CHECKER)
                    && object.get("verdict").is_some_and(|v| v == "equivalent")
                    && object.get("replay").is_some_and(|v| v == "matched")
                {
                    let digest = |name: &str| {
                        object.get(name).and_then(|v| v.as_str()).filter(|s| {
                            s.len() == 64
                                && s.bytes()
                                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        })
                    };
                    if let (Some(receipt), Some(candidate)) =
                        (digest("receipt_sha256"), digest("candidate_sha256"))
                    {
                        if out.len() >= 64 {
                            return Err("reported-receipt-bound".into());
                        }
                        out.push(serde_json::json!({"location":location,"semantic_receipt_sha256":receipt,"candidate_sha256":candidate,
                            "source":"trusted-worker-reported-digest"}));
                    }
                }
                for (key, value) in object {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    reported_receipts(value, &format!("{location}/{key}"), out)?;
                }
            }
            serde_json::Value::Array(array) => {
                for (index, value) in array.iter().enumerate() {
                    reported_receipts(value, &format!("{location}/{index}"), out)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GuardianReport {
        schema: String,
        invocation_sha256: String,
        result: Result<u8, SupervisorFailure>,
        evidence: Option<RunEvidence>,
    }

    fn finite_command(command: &crate::Command) -> Result<(), String> {
        if super::super::contained_profile(command).is_none() {
            return Err("command-profile:unclassified-command".into());
        }
        let profile = super::super::required_profile(command)?;
        if profile != Profile::FiniteData {
            return Err(format!("command-profile:{}", profile.name()));
        }
        Ok(())
    }

    /// Internal modes are not containment evidence. Recheck command eligibility
    /// in both re-execs; only the outer supervisor validates completed evidence.
    /// Worker limits precede even this parse and its bounded policy-file read.
    pub(in super::super) fn bootstrap() -> Option<u8> {
        let role = std::env::var(ROLE).ok()?;
        let result = (|| -> Result<Option<u8>, String> {
            check_environment()?;
            close_inherited_descriptors()?;
            match role.as_str() {
                "worker" => {
                    install_limits()?;
                    verify_worker()?;
                    {
                        use clap::Parser;
                        let cli = crate::Cli::try_parse().map_err(|e| e.to_string())?;
                        finite_command(&cli.command)?;
                    }
                    linux::child()?;
                    Ok(None)
                }
                "probe" => {
                    let mut guarantees = install_limits()?;
                    check_environment()?;
                    verify_closed_descriptors()?;
                    guarantees.push(observed(Control::Environment, Scope::TrustedWorker));
                    guarantees.push(observed(Control::FileDescriptors, Scope::TrustedWorker));
                    serde_json::to_writer(std::io::stdout(), &guarantees)
                        .map_err(|e| e.to_string())?;
                    println!();
                    std::io::stdout().flush().map_err(|e| e.to_string())?;
                    // Parent must kill/reap this live helper. Independent bound
                    // also limits a failed or abandoned qualification attempt.
                    std::thread::sleep(Duration::from_secs(3));
                    Ok(Some(crate::BLOCKED))
                }
                "guardian" => {
                    let prepared = Instant::now();
                    let mut preparation =
                        super::super::diagnostics::Record::new("guardian-preparation", "posix");
                    use clap::Parser;
                    let cli = crate::Cli::try_parse().map_err(|e| e.to_string())?;
                    finite_command(&cli.command)?;
                    let invocation =
                        std::env::var(INVOCATION).map_err(|_| "guardian-invocation")?;
                    let caller = UnixStream::from(
                        std::io::stdin()
                            .as_fd()
                            .try_clone_to_owned()
                            .map_err(|e| format!("guardian-caller-clone:{e}"))?,
                    );
                    caller
                        .set_nonblocking(true)
                        .map_err(|e| format!("guardian-caller-nonblocking:{e}"))?;
                    let mut guardian = Guardian {
                        caller,
                        invocation: invocation.clone(),
                        build: build_identity()?,
                        evidence: None,
                        journal: None,
                    };
                    let actual = probe().map_err(|e| e.to_string())?;
                    policy::compare(Profile::FiniteData, Backend::Posix, true, &actual)
                        .map_err(|e| e.to_string())?;
                    guardian.journal =
                        Invocation::recover(&cli.command, &invocation, &guardian.build)?;
                    preparation.duration("preparation_ms", prepared);
                    preparation.set("invocation_sha256", serde_json::json!(invocation));
                    let (staging_parent, receipt) = super::super::receipt_paths(&cli.command);
                    let mut result = linux::supervise_posix(
                        &std::env::args_os().skip(1).collect::<Vec<_>>(),
                        linux::Receipts {
                            staging_parent: staging_parent.as_deref(),
                            receipt,
                        },
                        &mut guardian,
                    );
                    if let Err(failure) = &result
                        && let Some(e) = &mut guardian.evidence
                    {
                        e.termination = Termination::Stopped;
                        e.exit_code = None;
                        e.termination_reason = Some(failure.reason.clone());
                    }
                    if let Some(journal) = &mut guardian.journal {
                        let e = guardian.evidence.get_or_insert_with(|| {
                            let mut e = empty_evidence(&invocation, &guardian.build);
                            e.termination = Termination::Stopped;
                            e.cleanup = Cleanup::Unknown;
                            e.termination_reason = result.as_ref().err().map(|f| f.reason.clone());
                            e
                        });
                        if let Err(error) = journal.finish(e) {
                            result = Err(format!("containment-evidence:{error}").into());
                        }
                    }
                    let code = result.as_ref().copied().unwrap_or(crate::BLOCKED);
                    let report = GuardianReport {
                        schema: "zeno-fcis/containment-guardian/1".into(),
                        invocation_sha256: invocation,
                        result,
                        evidence: guardian.evidence,
                    };
                    let bytes = serde_json::to_vec(&report).map_err(|e| e.to_string())?;
                    if bytes.len() > EVIDENCE_CAP {
                        return Err("guardian-report-bound".into());
                    }
                    write_protocol(&guardian.caller, &bytes)
                        .map_err(|e| format!("guardian-report-write:{e}"))?;
                    Ok(Some(code))
                }
                _ => Err("unknown-internal-role".into()),
            }
        })();
        match result {
            Ok(code) => code,
            Err(error) => {
                eprintln!("{}", unavailable(error));
                Some(crate::BLOCKED)
            }
        }
    }

    pub(in super::super) fn supervise(
        command: &crate::Command,
        args: &[OsString],
        mut invocation: Invocation,
    ) -> Result<u8, SupervisorFailure> {
        let build = invocation.build.clone();
        let (mut watchdog, channel) = UnixStream::pair().map_err(|e| e.to_string())?;
        watchdog.set_nonblocking(true).map_err(|e| e.to_string())?;
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let identity = invocation.invocation.clone();
        let (mut stdout, output) = linux::CapturedOutput::new(1024 * 1024)?;
        let (mut stderr, errors) = linux::CapturedOutput::new(8192)?;
        let mut launcher = base_command(&executable);
        launcher
            .args(args)
            .env(ROLE, "guardian")
            .env(INVOCATION, &identity)
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(channel)))
            .stdout(output)
            .stderr(errors);
        invocation
            .timing
            .duration("preparation_ms", invocation.timing.started());
        let guardian_started = Instant::now();
        let mut child = launcher
            .spawn()
            .map_err(|e| unavailable(format!("guardian-launch:{e}")).to_string())?;
        drop(launcher); // Only the caller owns this watchdog endpoint.
        let guardian_clock = Instant::now();
        let deadline = guardian_clock + Duration::from_secs(30);
        invocation
            .timing
            .set("guardian_pid", serde_json::json!(child.id()));
        invocation.timing.set(
            "guardian_start_ticks",
            serde_json::json!(super::super::diagnostics::process_start_ticks(child.id())),
        );
        invocation
            .timing
            .set("guardian_budget_ms", serde_json::json!(30000));
        let mut evidence = Vec::new();
        let captured = (|| -> Result<ExitStatus, String> {
            let mut status = None;
            let mut closed = false;
            loop {
                stdout.read_available(deadline)?;
                stderr.read_available(deadline)?;
                let mut buffer = [0; 4096];
                match watchdog.read(&mut buffer) {
                    Ok(0) => closed = true,
                    Ok(n) => {
                        evidence.extend_from_slice(&buffer[..n]);
                        if evidence.len() > EVIDENCE_CAP {
                            return Err("guardian-report-bound".into());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(format!("guardian-channel:{e}")),
                }
                if status.is_none() {
                    status = child.try_wait().map_err(|e| e.to_string())?;
                }
                if closed
                    && stdout.closed
                    && stderr.closed
                    && let Some(status) = status
                {
                    return Ok(status);
                }
                if Instant::now() >= deadline {
                    return Err("guardian-deadline".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        invocation
            .timing
            .duration("guardian_observed_ms", guardian_clock);
        invocation
            .timing
            .duration("guardian_launch_and_wait_ms", guardian_started);
        invocation.timing.set(
            "guardian_transport_reason",
            serde_json::json!(captured.as_ref().err()),
        );
        let mut final_evidence = empty_evidence(&identity, &build);
        final_evidence.termination = Termination::Stopped;
        final_evidence.cleanup = Cleanup::Unknown;
        let outcome = (|| -> Result<u8, SupervisorFailure> {
            if let Err(error) = captured {
                drop(watchdog); // Request cleanup before considering guardian kill.
                if linux::reap_before(&mut child, Instant::now() + Duration::from_secs(3)).is_err()
                {
                    let _ = child.kill();
                    let _ = linux::reap_before(&mut child, Instant::now() + Duration::from_secs(1));
                }
                return Err(SupervisorFailure::cleanup(
                    Some(error),
                    "guardian-cleanup-unknown".into(),
                ));
            }
            let status = captured.map_err(SupervisorFailure::from)?;
            let report: GuardianReport = serde_json::from_slice(&evidence).map_err(|_| {
                SupervisorFailure::cleanup(
                    Some("guardian-report-unavailable".into()),
                    "guardian-cleanup-unknown".into(),
                )
            })?;
            if report.schema != "zeno-fcis/containment-guardian/1"
                || report.invocation_sha256 != identity
            {
                return Err(SupervisorFailure::cleanup(
                    Some("guardian-report-binding".into()),
                    "guardian-cleanup-unknown".into(),
                ));
            }
            if let Some(e) = &report.evidence {
                e.validate_posix_identity(
                    &build,
                    &identity,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                )
                .map_err(|error| {
                    SupervisorFailure::cleanup(Some(error), "guardian-cleanup-unknown".into())
                })?;
                final_evidence = e.clone();
            }
            let code = report.result?;
            let e = report.evidence.ok_or_else(|| {
                SupervisorFailure::cleanup(
                    Some("guardian-startup-evidence-missing".into()),
                    "guardian-cleanup-unknown".into(),
                )
            })?;
            if status.code() != Some(i32::from(code)) {
                return Err(SupervisorFailure::cleanup(
                    Some("guardian-exit-mismatch".into()),
                    "guardian-cleanup-unknown".into(),
                ));
            }
            e.validate_posix_completion(
                &build,
                &identity,
                std::env::consts::OS,
                std::env::consts::ARCH,
                code,
            )
            .map_err(|error| {
                SupervisorFailure::cleanup(Some(error), "guardian-cleanup-unknown".into())
            })?;
            if code == crate::OK
                && let Some((session, true)) = crate::loop_command::knowledge_session(command)
            {
                let policy = super::super::SessionPolicy {
                    schema: "zeno-fcis/containment-session/1".into(),
                    required_profile: Profile::FiniteData,
                    profile_sha256: Profile::FiniteData.hash(),
                };
                let bytes = serde_json::to_vec(&policy).map_err(|e| e.to_string())?;
                crate::atomic_create(&session.join(super::super::SESSION_POLICY), &bytes)
                    .map_err(|e| format!("session-containment-policy:{e}"))?;
            }
            Ok(code)
        })();
        if let Err(failure) = &outcome {
            final_evidence.termination = Termination::Stopped;
            final_evidence.exit_code = None;
            final_evidence.termination_reason = Some(failure.reason.clone());
        }
        invocation.timing.set(
            "supervisor_reason",
            serde_json::json!(outcome.as_ref().err().map(|f| &f.reason)),
        );
        invocation.timing.set(
            "cleanup_verified",
            serde_json::json!(final_evidence.absence_verified),
        );
        invocation
            .finish(&mut final_evidence)
            .map_err(|e| format!("containment-evidence:{e}"))?;
        let code = outcome?;
        // Add only nonsecret operational labels to the public result. The
        // capture packet's four fields and canonical receipt remain intact.
        let mut public: serde_json::Value = serde_json::from_slice(&stdout.bytes)
            .map_err(|_| "contained-result-json".to_string())?;
        super::super::label_public_result(&mut public);
        stdout.bytes = serde_json::to_vec(&public).map_err(|e| e.to_string())?;
        stdout.bytes.push(b'\n');
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
    #[cfg(test)]
    mod stage_tests {
        use super::*;
        // Actual re-executed workers: no production flag or alternate decision API.
        fn worker(spin: bool) {
            if !is_worker() {
                return;
            }
            close_inherited_descriptors().unwrap();
            install_limits().unwrap();
            assert_eq!(linux::child(), Ok(true));
            super::super::super::start(Duration::from_millis(if spin { 1000 } else { 100 }), false)
                .unwrap();
            if spin {
                loop {
                    std::hint::black_box(1u64.wrapping_mul(7));
                }
            }
            std::thread::sleep(Duration::from_millis(600));
            super::super::super::finish().unwrap();
            println!("{{\"status\":\"inconclusive\",\"authority\":\"none\"}}");
            super::super::super::shutdown(crate::BLOCKED);
            std::process::exit(crate::BLOCKED.into());
        }
        #[test]
        fn probe_small_stage_admission() {
            if !is_worker() {
                return;
            }
            close_inherited_descriptors().unwrap();
            install_limits().unwrap();
            assert_eq!(linux::child(), Ok(true));
            for requested in [18, 19] {
                assert_eq!(
                    super::super::super::start(Duration::from_millis(requested), false),
                    Err(super::super::super::STAGE_CPU_RESOLUTION.into())
                );
            }
            // Both denials occurred before START; a subsequent ordinary
            // stage must still work, rather than inheriting an active stage.
            super::super::super::start(Duration::from_millis(100), false).unwrap();
            assert_eq!(
                super::super::super::start(Duration::from_millis(18), false),
                Err("control-stage-order".into()),
                "protocol order must precede the small-budget prescreen"
            );
            super::super::super::finish().unwrap();
            println!("{{\"status\":\"inconclusive\",\"authority\":\"none\"}}");
            super::super::super::shutdown(crate::BLOCKED);
            std::process::exit(crate::BLOCKED.into());
        }
        #[test]
        fn probe_stage_spin() {
            worker(true);
        }
        #[test]
        fn probe_stage_wait() {
            worker(false);
        }

        #[test]
        fn actual_posix_stage_cpu_spin_is_stopped_but_waiting_does_not_spend_cpu() {
            let root = evidence_root();
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&root)
                .unwrap();
            for name in [
                "probe_stage_wait",
                "probe_stage_spin",
                "probe_small_stage_admission",
            ] {
                let invocation = crate::transform::sha256_hex(
                    format!(
                        "stage-cpu-{}-{}",
                        std::process::id(),
                        crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    )
                    .as_bytes(),
                );
                let directory = evidence_directory(&invocation).unwrap();
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&directory)
                    .unwrap();
                let (caller, _held) = UnixStream::pair().unwrap();
                caller.set_nonblocking(true).unwrap();
                let mut guardian = Guardian {
                    caller,
                    invocation,
                    build: "stage-control".into(),
                    evidence: None,
                    journal: None,
                };
                let probe = format!("checker_process::posix::runtime::stage_tests::{name}");
                let args =
                    ["--exact", &probe, "--nocapture", "--test-threads=1"].map(OsString::from);
                let result = linux::supervise_posix(
                    &args,
                    linux::Receipts {
                        staging_parent: None,
                        receipt: None,
                    },
                    &mut guardian,
                );
                let evidence = guardian.evidence.as_ref().unwrap();
                assert!(evidence.absence_verified);
                assert_eq!(evidence.cleanup, Cleanup::OwnedWorkersReaped);
                let records = Path::new("/tmp")
                    .join(format!("zeno-fcis-operator-{}", nix::unistd::geteuid()));
                let observation = fs::read_dir(records)
                    .unwrap()
                    .filter_map(|p| {
                        serde_json::from_slice::<serde_json::Value>(&fs::read(p.ok()?.path()).ok()?)
                            .ok()
                    })
                    .find(|v| {
                        v["observer"] == "worker-supervisor"
                            && v["pid"] == std::process::id()
                            && v["process_start_ticks"]
                                == super::super::super::diagnostics::process_start_ticks(
                                    std::process::id(),
                                )
                                .unwrap()
                            && v["worker_pid"] == evidence.worker_pid
                    })
                    .unwrap();
                println!("stage-control {name}: {observation}");
                assert_eq!(observation["cleanup_verified"], true);
                if name.ends_with("spin") {
                    let failure = result.unwrap_err();
                    assert!(
                        failure.reason.starts_with("contained-deadline"),
                        "{failure:?}"
                    );
                    assert!(failure.cleanup.is_none());
                    assert_eq!(observation["stages"][0]["stopped_by"], "cpu");
                    assert!(
                        observation["stages"][0]["cpu_observed_us"]
                            .as_u64()
                            .unwrap()
                            >= 1_000_000
                    );
                } else if name == "probe_small_stage_admission" {
                    assert_eq!(result, Ok(crate::BLOCKED));
                    assert_eq!(observation["stages"].as_array().unwrap().len(), 1);
                    assert_eq!(observation["stages"][0]["requested_ms"], 100);
                    assert_eq!(observation["stages"][0]["finished"], true);
                } else {
                    assert_eq!(result, Ok(crate::BLOCKED));
                    assert_eq!(observation["stages"][0]["finished"], true);
                    assert!(
                        observation["stages"][0]["wall_elapsed_ms"]
                            .as_u64()
                            .unwrap()
                            >= 600
                    );
                    assert!(
                        observation["stages"][0]["cpu_observed_us"]
                            .as_u64()
                            .unwrap()
                            < 100_000
                    );
                }
                fs::remove_dir_all(directory).unwrap();
            }
        }
        // Isolated re-exec: an external controller stops the supervisor for7s
        // after the worker got its permit. FINISH queues while that peer is
        // starved; the worker must wait for the actual permit, then complete.
        #[test]
        fn actual_starved_supervisor_accepts_queued_finish_and_hung_stage_refuses() {
            const PROBE: &str = "ZENO_FCIS_STARVED_SUPERVISOR_TEST";
            if std::env::var_os(PROBE).is_none() {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "checker_process::posix::runtime::stage_tests::actual_starved_supervisor_accepts_queued_finish_and_hung_stage_refuses", "--nocapture", "--test-threads=1"])
                    .env(PROBE, "1").output().unwrap();
                assert!(output.status.success(), "{output:?}");
                println!("{}", String::from_utf8_lossy(&output.stdout));
                return;
            }
            let root = evidence_root();
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&root)
                .unwrap();
            for starved in [true, false] {
                let invocation = crate::transform::sha256_hex(
                    format!(
                        "peer-control-{}-{}-{}",
                        std::process::id(),
                        super::super::super::diagnostics::process_start_ticks(std::process::id())
                            .unwrap(),
                        crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    )
                    .as_bytes(),
                );
                let directory = evidence_directory(&invocation).unwrap();
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&directory)
                    .unwrap();
                let marker = root.join(format!(
                    ".peer-{}-{}",
                    std::process::id(),
                    super::super::super::diagnostics::process_start_ticks(std::process::id())
                        .unwrap()
                ));
                assert!(!marker.exists());
                let controller = if starved {
                    Some(std::process::Command::new("python3")
                    .args(["-I", "-S", "-B", "-c", r#"
import os,signal,sys,time
from pathlib import Path
marker=Path(sys.argv[1]); pid=int(sys.argv[2]); ticks=Path(f'/proc/{pid}/stat').read_text().rsplit(') ',1)[1].split()[19]
end=time.monotonic()+20
while not marker.exists():
 if time.monotonic()>=end: raise RuntimeError('stage marker missing')
 time.sleep(.002)
assert Path(f'/proc/{pid}/stat').read_text().rsplit(') ',1)[1].split()[19]==ticks
os.kill(pid,signal.SIGSTOP)
try: time.sleep(7)
finally: os.kill(pid,signal.SIGCONT)
"#]).arg(&marker).arg(std::process::id().to_string()).spawn().unwrap())
                } else {
                    None
                };
                let (caller, _held) = UnixStream::pair().unwrap();
                caller.set_nonblocking(true).unwrap();
                let mut guardian = Guardian {
                    caller,
                    invocation,
                    build: "stage-control".into(),
                    evidence: None,
                    journal: None,
                };
                let probe = if starved {
                    "probe_stage_starved_peer"
                } else {
                    "probe_stage_hung"
                };
                // Marker binds the live supervisor PID and kernel start ticks.
                // The controller is our owned direct child; never signal after reap.
                let args = [
                    "--exact",
                    &format!("checker_process::posix::runtime::stage_tests::{probe}"),
                    "--nocapture",
                    "--test-threads=1",
                ]
                .map(OsString::from);
                let result = linux::supervise_posix(
                    &args,
                    linux::Receipts {
                        staging_parent: None,
                        receipt: None,
                    },
                    &mut guardian,
                );
                if let Some(mut controller) = controller {
                    assert!(controller.wait().unwrap().success());
                }
                let evidence = guardian.evidence.unwrap();
                assert!(evidence.absence_verified);
                assert_eq!(evidence.cleanup, Cleanup::OwnedWorkersReaped);
                let records = Path::new("/tmp")
                    .join(format!("zeno-fcis-operator-{}", nix::unistd::geteuid()));
                let observation = fs::read_dir(records)
                    .unwrap()
                    .filter_map(|p| {
                        serde_json::from_slice::<serde_json::Value>(&fs::read(p.ok()?.path()).ok()?)
                            .ok()
                    })
                    .find(|v| {
                        v["observer"] == "worker-supervisor"
                            && v["pid"] == std::process::id()
                            && v["process_start_ticks"]
                                == super::super::super::diagnostics::process_start_ticks(
                                    std::process::id(),
                                )
                                .unwrap()
                            && v["worker_pid"] == evidence.worker_pid
                    })
                    .unwrap();
                assert_eq!(observation["cleanup_verified"], true);
                assert!(observation["worker_observed_ms"].as_u64().unwrap() < 25000);
                let stage = &observation["stages"][0];
                assert!(stage["cpu_observed_us"].as_u64().unwrap() < 1_000_000);
                if starved {
                    assert_eq!(result, Ok(crate::BLOCKED));
                    assert_eq!(stage["finished"], true);
                    assert!(stage["wall_elapsed_ms"].as_u64().unwrap() >= 7000);
                } else {
                    assert!(result.unwrap_err().reason.starts_with("contained-deadline"));
                    assert_eq!(stage["stopped_by"], "no-cpu-or-protocol-progress");
                    assert!(stage["wall_elapsed_ms"].as_u64().unwrap() >= 2000);
                    assert!(stage["wall_elapsed_ms"].as_u64().unwrap() < 4000);
                    assert!(
                        !observation["idle_waiters"]["threads"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
                println!("physical peer control starved={starved}: {observation}");
                if starved {
                    fs::remove_file(marker).unwrap();
                }
                fs::remove_dir_all(directory).unwrap();
            }
        }
        fn peer_worker(starved: bool) {
            if !is_worker() {
                return;
            }
            close_inherited_descriptors().unwrap();
            install_limits().unwrap();
            assert_eq!(linux::child(), Ok(true));
            super::super::super::start(Duration::from_millis(1000), true).unwrap();
            if starved {
                let parent = nix::unistd::getppid().as_raw() as u32;
                let ticks = super::super::super::diagnostics::process_start_ticks(parent).unwrap();
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(evidence_root().join(format!(".peer-{parent}-{ticks}")))
                    .unwrap();
                // Wait for the real fault, not a scheduling guess. Under
                // eight burners the controller can run after a100ms sleep.
                let stop_end = Instant::now() + Duration::from_secs(2);
                loop {
                    assert_eq!(
                        super::super::super::diagnostics::process_start_ticks(parent),
                        Some(ticks)
                    );
                    let stat = fs::read_to_string(format!("/proc/{parent}/stat")).unwrap();
                    if stat.rsplit_once(") ").unwrap().1.split_whitespace().next() == Some("T") {
                        break;
                    }
                    assert!(
                        Instant::now() < stop_end,
                        "controller did not stop the actual supervisor"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                super::super::super::finish().unwrap();
                println!("{{\"status\":\"inconclusive\",\"authority\":\"none\"}}");
                super::super::super::shutdown(crate::BLOCKED);
                std::process::exit(crate::BLOCKED.into());
            }
            loop {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        #[test]
        fn probe_stage_starved_peer() {
            peer_worker(true);
        }
        #[test]
        fn probe_stage_hung() {
            peer_worker(false);
        }
    }
}
