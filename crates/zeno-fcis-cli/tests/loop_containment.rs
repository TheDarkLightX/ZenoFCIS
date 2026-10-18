//! Production entry points: capability refusal is independent of OS cfg.
#![forbid(unsafe_code)]
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn python_leaf_limits_preserve_inheritance_and_readback() {
    let done = Command::new("/usr/bin/python3")
        .args(["-I", "-S", "-B"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test-data/capture_limits.py"))
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(done.status.success(), "{done:?}");
    assert!(
        String::from_utf8_lossy(&done.stderr).contains("Ran 3 tests"),
        "{done:?}"
    );
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zeno-containment-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("{e}"));
        Self(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn json(out: &Output) -> Value {
    assert!(out.stderr.is_empty(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {out:?}"))
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap_or_else(|e| panic!("{e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("{e}"));
        if entry.path().is_dir() {
            out.insert(entry.path(), Vec::new());
            out.extend(snapshot(&entry.path()));
        } else {
            out.insert(
                entry.path(),
                fs::read(entry.path()).unwrap_or_else(|e| panic!("{e}")),
            );
        }
    }
    out
}
fn entries<'a>(session: &'a str, original: &'a str, output: &'a str) -> Vec<Vec<&'a str>> {
    vec![
        vec!["loop", "open", "--original", original, "--session", output],
        vec!["loop", "encode", "--program", original, "--out", output],
        vec!["loop", "resume", "--session", session],
        vec![
            "loop",
            "candidate",
            "--session",
            session,
            "--candidate",
            original,
        ],
        vec![
            "loop",
            "candidate",
            "--session",
            session,
            "--candidate-json",
            original,
        ],
        vec![
            "loop",
            "candidate",
            "--session",
            session,
            "--expressions",
            original,
        ],
        vec!["loop", "run", "--session", session, "--proposer", "local"],
        vec![
            "loop",
            "run",
            "--session",
            session,
            "--proposer",
            "fake",
            "--script",
            original,
        ],
        vec!["loop", "run", "--session", session, "--proposer", "hosted"],
        vec!["loop", "sketch", "--session", session, "--sketch", original],
        vec![
            "loop",
            "family",
            "--instances",
            original,
            "--program",
            original,
        ],
        vec![
            "loop",
            "transfer",
            "--from",
            original,
            "--to-instances",
            original,
        ],
        vec![
            "loop",
            "meaning",
            "--original",
            original,
            "--candidate",
            original,
        ],
        vec![
            "loop",
            "evaluate",
            "--original",
            original,
            "--expressions",
            original,
            "--input",
            "0",
        ],
        vec!["loop", "question", "--contract", original],
        vec!["loop", "question", "--specification", original],
        vec![
            "loop",
            "teach-back",
            "--contract",
            original,
            "--rule",
            original,
        ],
        vec![
            "loop",
            "decompose",
            "--contract",
            original,
            "--architecture",
            original,
        ],
        vec![
            "loop",
            "pipeline",
            "--original",
            original,
            "--proposal",
            original,
            "--out",
            output,
        ],
        vec!["loop", "strategy", "--tasks", original, "--out", output],
        vec![
            "loop",
            "strategy",
            "--kind",
            "split-by-cases",
            "--problem",
            original,
            "--original",
            original,
        ],
        vec![
            "loop",
            "strategy",
            "--kind",
            "diagnose",
            "--problem",
            original,
        ],
        vec![
            "loop",
            "strategy",
            "--kind",
            "ask-when-it-matters",
            "--problem",
            original,
        ],
        vec![
            "loop",
            "strategy",
            "--kb-op",
            "query",
            "--query",
            "anything(X)",
        ],
        vec!["loop", "strategy", "--kb-op", "check"],
        vec![
            "loop", "strategy", "--kb-op", "plan", "--have", "a", "--want", "b",
        ],
        vec!["loop", "strategy", "--kb-op", "advise"],
        vec!["loop", "strategy", "--kb-op", "explain"],
        vec![
            "loop",
            "decompose",
            "--contract",
            original,
            "--architecture",
            original,
            "--compose",
            "--export",
            output,
        ],
        vec![
            "loop",
            "learn",
            "--spec",
            original,
            "--original",
            original,
            "--out",
            output,
        ],
        vec![
            "loop",
            "rules",
            "--original",
            original,
            "--terms",
            original,
            "--out",
            output,
        ],
        vec!["loop", "learn", "--spec", original],
        vec![
            "loop",
            "capture",
            "open",
            "--spec",
            original,
            "--reference",
            original,
            "--session",
            output,
        ],
        vec![
            "loop",
            "capture",
            "whole",
            "--spec",
            original,
            "--reference",
            original,
            "--candidate-json",
            original,
        ],
        vec!["loop", "capture", "packet", "--session", session],
        vec![
            "loop",
            "capture",
            "candidate",
            "--session",
            session,
            "--candidate-json",
            original,
        ],
        vec!["loop", "capture", "status", "--session", session],
        vec![
            "transform",
            "check",
            "--original",
            original,
            "--candidate",
            original,
            "--offline-parallel",
            "--receipt",
            output,
        ],
    ]
}

#[test]
#[cfg(target_os = "linux")]
fn automatic_strong_profiles_require_explicit_backend_before_input_or_session_work() {
    let d = Directory::new();
    let session = d.path("session");
    let input = d.path("must-not-be-read");
    let output = d.path("must-not-be-written");
    let cases = [
        vec![
            "loop",
            "capture",
            "open",
            "--spec",
            &input,
            "--reference",
            &input,
            "--session",
            &session,
        ],
        vec![
            "loop",
            "capture",
            "whole",
            "--spec",
            &input,
            "--reference",
            &input,
            "--expressions",
            &input,
        ],
        vec![
            "transform",
            "check",
            "--original",
            &input,
            "--candidate",
            &input,
            "--offline-parallel",
            "--receipt",
            &output,
        ],
    ];
    let before = snapshot(&d.0);
    for args in cases {
        // Automatic selection refuses without a manager probe or input work.
        let out = Command::new(CLI)
            .args(&args)
            .args(["--no-kb"])
            .env_remove("ZENO_FCIS_CONTAINMENT_BACKEND")
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let report = json(&out);
        let capture = args[0] == "loop";
        assert_eq!(
            out.status.code(),
            Some(if capture { 1 } else { 2 }),
            "{report}"
        );
        assert_eq!(report["authority"], "none");
        assert_eq!(report["backend"], "auto");
        assert_eq!(
            report["reason"],
            "containment-unavailable:auto:explicit-backend-required"
        );
        assert_eq!(report["containment"]["selection_source"], "default");
        let message = report["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{report}"));
        assert!(message.contains("--containment-backend"));
        assert!(message.contains("ZENO_FCIS_CONTAINMENT_BACKEND"));
        assert_eq!(report["containment"]["startup"]["admitted"], false);
        assert_eq!(
            report["containment"]["final"]["application_work_started"],
            false
        );
        if capture {
            assert_eq!(
                report["detail"]["reason"],
                "capture-containment-unavailable"
            );
            assert_eq!(report["detail"]["attempt_spent"], false);
        } else {
            assert_eq!(
                report["reason"],
                "containment-unavailable:auto:explicit-backend-required"
            );
        }
        assert_eq!(
            snapshot(&d.0),
            before,
            "preflight refusal must preserve every byte"
        );
    }
}

#[test]
fn contained_loop_entry_points_refuse_without_a_supported_backend() {
    let d = Directory::new();
    let session = d.path("session");
    let input = d.path("submitted-input-must-not-be-read");
    let output = d.path("must-not-exist");
    fs::create_dir(&session).unwrap_or_else(|e| panic!("{e}"));
    for subdir in ["receipts", "artifacts", "witnesses"] {
        fs::create_dir(Path::new(&session).join(subdir)).unwrap_or_else(|e| panic!("{e}"));
    }
    fs::write(Path::new(&session).join("ledger.jsonl"), b"").unwrap_or_else(|e| panic!("{e}"));
    let before = snapshot(&d.0);
    for args in entries(&session, &input, &output) {
        let out = Command::new(CLI)
            .args(&args)
            .env("ZENO_FCIS_CONTAINMENT_MISSING", "wall")
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "systemd")
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let report = json(&out);
        let capture = args.get(1) == Some(&"capture");
        assert_eq!(
            out.status.code(),
            Some(if capture { 1 } else { 2 }),
            "{args:?}: {report}"
        );
        assert_eq!(report["authority"], "none", "{args:?}: {report}");
        assert_eq!(
            report["status"],
            if capture { "refused" } else { "inconclusive" }
        );
        assert!(report["detail"]["trusted_incumbent"].is_null());
        if capture {
            assert_eq!(
                report["detail"]["reason"],
                "capture-containment-unavailable"
            );
            assert_eq!(report["detail"]["attempt_spent"], false);
        } else {
            assert!(
                report["reason"].as_str().is_some_and(|s| s
                    .starts_with("containment-unavailable:")
                    && s.ends_with(":wall")),
                "{args:?}: {report}"
            );
        }
        if args[0] == "loop" {
            assert!(report["acquisition"]["observed_intake"].is_null());
            assert_eq!(report["acquisition"]["accounting_complete"], false);
        }
        assert_eq!(
            snapshot(&d.0),
            before,
            "{args:?}: refusal must preserve every session/ledger/artifact byte and directory"
        );
    }
}

#[test]
fn backend_precedence_and_malformed_environment_are_observed_by_real_cli() {
    let d = Directory::new();
    let before = snapshot(&d.0);
    for (flag, environment, backend, source) in [
        (None, None, "auto", "default"),
        (None, Some("auto"), "auto", "environment"),
        (None, Some("systemd"), "systemd", "environment"),
        (None, Some("posix"), "posix", "environment"),
        (Some("auto"), Some("systemd"), "auto", "flag"),
        (Some("systemd"), Some("posix"), "systemd", "flag"),
        (Some("posix"), Some("systemd"), "posix", "flag"),
        (Some("systemd"), Some("invalid"), "systemd", "flag"),
    ] {
        let mut command = Command::new(CLI);
        command.args([
            "--no-kb",
            "transform",
            "check",
            "--offline-parallel",
            "--original",
            "/must-not-read",
            "--candidate",
            "/must-not-read",
            "--receipt",
            &d.path("absent"),
        ]);
        command
            .env_remove("ZENO_FCIS_CONTAINMENT_BACKEND")
            .env("ZENO_FCIS_CONTAINMENT_MISSING", "wall");
        if let Some(value) = environment {
            command.env("ZENO_FCIS_CONTAINMENT_BACKEND", value);
        }
        if let Some(value) = flag {
            command.args(["--containment-backend", value]);
        }
        let out = command.output().unwrap_or_else(|e| panic!("{e}"));
        let report = json(&out);
        assert_eq!(out.status.code(), Some(2), "{report}");
        assert_eq!(report["backend"], backend, "{report}");
        assert_eq!(
            report["containment"]["selection_source"], source,
            "{report}"
        );
        assert_eq!(snapshot(&d.0), before);
    }
    for bad in ["", "invalid", "SYSTEMD", " systemd", "systemd "] {
        let out = Command::new(CLI)
            .args([
                "transform",
                "check",
                "--offline-parallel",
                "--original",
                "/must-not-read",
                "--candidate",
                "/must-not-read",
            ])
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", bad)
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let report = json(&out);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(
            report["reason"],
            "containment-unavailable:environment:invalid-ZENO_FCIS_CONTAINMENT_BACKEND"
        );
        assert_eq!(report["containment"]["selection_source"], "environment");
        assert_eq!(snapshot(&d.0), before);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn automatic_finite_data_still_selects_posix_before_work() {
    let d = Directory::new();
    let before = snapshot(&d.0);
    let out = Command::new(CLI)
        .args([
            "--no-kb",
            "loop",
            "open",
            "--original",
            "/must-not-read",
            "--session",
            &d.path("absent"),
        ])
        .env_remove("ZENO_FCIS_CONTAINMENT_BACKEND")
        .env("ZENO_FCIS_CONTAINMENT_MISSING", "wall")
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    let report = json(&out);
    assert_eq!(report["reason"], "containment-unavailable:posix:wall");
    assert_eq!(report["containment"]["required_profile"], "finite-data/1");
    assert_eq!(report["containment"]["selection_source"], "default");
    assert_eq!(snapshot(&d.0), before);
}

#[cfg(target_os = "linux")]
#[test]
fn every_required_posix_capability_refuses_before_open_and_no_reference_is_routed_to_it() {
    let d = Directory::new();
    let target = d.path("session");
    for missing in [
        "wall",
        "bounded-data",
        "environment",
        "file-descriptors",
        "owned-cleanup",
        "cpu-seconds",
        "file-size",
        "open-files",
        "core-size",
        "address-space",
    ] {
        let out = Command::new(CLI)
            .args([
                "--containment-backend",
                "posix",
                "loop",
                "open",
                "--original",
                "/must-not-read",
                "--session",
                &target,
            ])
            .env("ZENO_FCIS_CONTAINMENT_MISSING", missing)
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let report = json(&out);
        assert_eq!(out.status.code(), Some(2), "{report}");
        assert_eq!(
            report["reason"],
            format!("containment-unavailable:posix:{missing}")
        );
        assert!(!Path::new(&target).exists());
    }
    let out = Command::new(CLI)
        .args([
            "--containment-backend",
            "posix",
            "loop",
            "capture",
            "open",
            "--spec",
            "/must-not-read",
            "--reference",
            "/must-not-execute",
            "--session",
            &target,
        ])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        json(&out)["detail"]["reason"],
        "capture-containment-unavailable"
    );
    assert!(!Path::new(&target).exists());
}

#[cfg(target_os = "linux")]
mod posix_controls {
    use super::*;
    use std::io::Read;
    use std::os::fd::OwnedFd;
    use std::os::unix::{net::UnixStream, process::CommandExt};
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};

    fn contract(d: &Directory) -> String {
        let app = d.0.join("counter");
        fs::create_dir_all(app.join("v2")).unwrap_or_else(|e| panic!("{e}"));
        let template =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
        for (source, target) in [
            ("project.zeno.in", "project.zeno"),
            ("policy.json.in", "v2/policy.json"),
        ] {
            let text = fs::read_to_string(template.join(source))
                .unwrap_or_else(|e| panic!("{e}"))
                .replace("${C}", "128")
                .replace("${INSTANCE}", "c128");
            fs::write(app.join(target), text).unwrap_or_else(|e| panic!("{e}"));
        }
        app.to_string_lossy().into_owned()
    }
    fn signal(pid: u32, signal: nix::sys::signal::Signal) {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(i32::try_from(pid).unwrap_or_else(|e| panic!("{e}"))),
            signal,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }
    fn suspend_worker(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let tasks = fs::read_dir(format!("/proc/{pid}/task"))
                .map(|entries| entries.count())
                .unwrap_or(0);
            if tasks >= 2 {
                break;
            } // The parent monitor is created after startup ACK.
            assert!(
                Instant::now() < deadline,
                "worker startup ACK was never accepted"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(30)); // Search stage is active.
        signal(pid, nix::sys::signal::Signal::SIGSTOP);
    }
    fn children(pid: u32) -> Vec<u32> {
        fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect()
    }
    fn find_worker(parent: u32, nested: bool) -> (u32, u32) {
        // Harness observation includes unoptimized caller/guardian preparation;
        // product ACK/worker/guardian clocks are asserted from their own records.
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            for child in children(parent) {
                if nested {
                    for worker in children(child) {
                        if fs::read(format!("/proc/{worker}/environ")).is_ok_and(|b| {
                            b.split(|v| *v == 0)
                                .any(|v| v == b"ZENO_FCIS_POSIX_ROLE=worker")
                        }) {
                            return (child, worker);
                        }
                    }
                } else {
                    if fs::read(format!("/proc/{child}/environ")).is_ok_and(|b| {
                        b.split(|v| *v == 0)
                            .any(|v| v == b"ZENO_FCIS_POSIX_ROLE=worker")
                    }) {
                        return (parent, child);
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "production worker was not launched"
            );
            std::thread::sleep(Duration::from_micros(100));
        }
    }
    fn wait(mut child: Child, timeout: Duration) -> Output {
        let deadline = Instant::now() + timeout;
        loop {
            if child.try_wait().unwrap_or_else(|e| panic!("{e}")).is_some() {
                return child.wait_with_output().unwrap_or_else(|e| panic!("{e}"));
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("independent harness deadline exceeded");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn guardian(input: &str, _output: &str) -> (Child, UnixStream) {
        let (caller, channel) = UnixStream::pair().unwrap_or_else(|e| panic!("{e}"));
        caller
            .set_nonblocking(true)
            .unwrap_or_else(|e| panic!("{e}"));
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["--no-kb", "loop", "question", "--contract", input])
            .env_clear()
            .env("LC_ALL", "C")
            .env("MALLOC_ARENA_MAX", "1")
            .env("ZENO_FCIS_POSIX_ROLE", "guardian")
            .env("ZENO_FCIS_POSIX_INVOCATION", "harness-owned-watchdog")
            .stdin(Stdio::from(OwnedFd::from(channel)))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        (child, caller)
    }
    fn report(mut channel: UnixStream) -> Value {
        let mut bytes = Vec::new();
        channel
            .read_to_end(&mut bytes)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(bytes.len() <= 32768);
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}: {bytes:?}"))
    }
    fn assert_absent(pid: u32) {
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "owned child must have been reaped"
        );
    }

    #[test]
    fn posix_worker_readbacks_environment_descriptors_group_and_reap_are_observed() {
        let d = Directory::new();
        let input = contract(&d);
        let output = d.path("program.zcve");
        let sentinel = d.path("inherited-secret-fd");
        // The trusted harness deliberately passes an inheritable descriptor to
        // the public CLI. It must disappear on re-exec into guardian/worker.
        let child = Command::new("/usr/bin/python3").env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["-c", "import os,sys\nf=os.open(sys.argv[1],os.O_CREAT|os.O_RDWR,0o600)\nos.set_inheritable(f,True)\nos.execv(sys.argv[2],sys.argv[2:])", &sentinel, CLI,
                "--no-kb", "loop", "question", "--contract", &input])
            .env("ZENO_FCIS_TEST_SECRET", "outside-secret-must-be-absent")
            .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap_or_else(|e| panic!("{e}"));
        let (guardian, worker) = find_worker(child.id(), true);
        let root = PathBuf::from(format!("/proc/{worker}"));
        suspend_worker(worker);
        assert_ne!(
            fs::read_link(format!("/proc/{guardian}/fd/0")).unwrap_or_else(|e| panic!("{e}")),
            fs::read_link(root.join("fd/0")).unwrap_or_else(|e| panic!("{e}")),
            "the worker must not retain the caller watchdog"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let limits = fs::read_to_string(root.join("limits")).unwrap_or_default();
            if limits.lines().any(|s| {
                s.starts_with("Max address space")
                    && s.split_whitespace().nth(3) == Some("67108864")
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let environment = fs::read(root.join("environ")).unwrap_or_else(|e| panic!("{e}"));
        assert!(!String::from_utf8_lossy(&environment).contains("outside-secret"));
        assert!(!String::from_utf8_lossy(&environment).contains("ZENO_FCIS_TEST_SECRET"));
        for entry in fs::read_dir(root.join("fd")).unwrap_or_else(|e| panic!("{e}")) {
            let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
            let target = fs::read_link(path).unwrap_or_else(|e| panic!("{e}"));
            assert_ne!(target, Path::new(&sentinel));
        }
        let stat = fs::read_to_string(root.join("stat")).unwrap_or_else(|e| panic!("{e}"));
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .unwrap_or_else(|| panic!("stat"))
            .1
            .split_whitespace()
            .collect();
        assert_eq!(fields[1], guardian.to_string());
        assert_eq!(fields[2], worker.to_string());
        let limits = fs::read_to_string(root.join("limits")).unwrap_or_else(|e| panic!("{e}"));
        for (label, value) in [
            ("Max address space", "67108864"),
            ("Max cpu time", "25"),
            ("Max file size", "16777216"),
            ("Max open files", "64"),
            ("Max core file size", "0"),
        ] {
            let values: Vec<_> = limits
                .lines()
                .find_map(|s| s.strip_prefix(label))
                .unwrap_or_else(|| panic!("{label}"))
                .split_whitespace()
                .collect();
            assert_eq!(&values[..2], [value, value]);
        }
        signal(worker, nix::sys::signal::Signal::SIGCONT);
        let out = wait(child, Duration::from_secs(5));
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(json(&out)["authority"], "none");
        assert!(!Path::new(&output).exists());
        assert_absent(worker);
        assert_absent(guardian);
    }

    #[test]
    fn guardian_caller_eof_kills_and_reaps_without_releasing_output() {
        let d = Directory::new();
        let input = contract(&d);
        let output = d.path("must-not-exist");
        let (child, caller) = guardian(&input, &output);
        let (_, worker) = find_worker(child.id(), false);
        suspend_worker(worker);
        drop(caller); // The same EOF the OS produces on caller death.
        let out = wait(child, Duration::from_secs(5));
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!Path::new(&output).exists());
        assert_absent(worker);
    }

    #[test]
    fn guardian_cancellation_kills_reaps_and_reports_verified_cleanup() {
        let d = Directory::new();
        let input = contract(&d);
        let output = d.path("must-not-exist");
        let (child, caller) = guardian(&input, &output);
        let guardian_pid = child.id();
        let (_, worker) = find_worker(guardian_pid, false);
        suspend_worker(worker);
        signal(guardian_pid, nix::sys::signal::Signal::SIGTERM);
        let out = wait(child, Duration::from_secs(5));
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        let r = report(caller);
        assert!(
            r["result"]["Err"]["reason"]
                .as_str()
                .is_some_and(|s| s.starts_with("interrupted:SIGTERM")),
            "{r}"
        );
        assert_eq!(r["evidence"]["cleanup"], "owned-workers-reaped");
        assert_eq!(r["evidence"]["absence_verified"], true);
        assert!(!Path::new(&output).exists());
        assert_absent(worker);
    }

    #[test]
    fn guardian_stage_deadline_kills_reaps_and_never_releases_unfinished_output() {
        let d = Directory::new();
        let input = contract(&d);
        let output = d.path("must-not-exist");
        let (child, caller) = guardian(&input, &output);
        let (_, worker) = find_worker(child.id(), false);
        suspend_worker(worker);
        let started = Instant::now();
        let out = wait(child, Duration::from_secs(9));
        assert!(started.elapsed() >= Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(9));
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        let r = report(caller);
        assert!(
            r["result"]["Err"]["reason"]
                .as_str()
                .is_some_and(|s| s.starts_with("contained-deadline")),
            "{r}"
        );
        assert_eq!(r["evidence"]["cleanup"], "owned-workers-reaped");
        assert_eq!(r["evidence"]["absence_verified"], true);
        assert!(!Path::new(&output).exists());
        assert_absent(worker);
    }

    #[test]
    fn lost_worker_startup_ack_refuses_before_any_submitted_file_is_read() {
        let d = Directory::new();
        let output = d.path("must-not-exist");
        let (mut control, channel) = UnixStream::pair().unwrap_or_else(|e| panic!("{e}"));
        control
            .set_nonblocking(true)
            .unwrap_or_else(|e| panic!("{e}"));
        let cpus = fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find_map(|s| {
                s.strip_prefix("Cpus_allowed_list:")
                    .map(|s| s.trim().to_owned())
            })
            .unwrap_or_else(|| panic!("affinity"));
        let started = Instant::now();
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args([
                "--no-kb",
                "loop",
                "encode",
                "--program",
                "/must-not-read",
                "--out",
                &output,
            ])
            .env_clear()
            .env("LC_ALL", "C")
            .env("MALLOC_ARENA_MAX", "1")
            .env("ZENO_FCIS_POSIX_ROLE", "worker")
            .env("ZENO_FCIS_CHECKER_CONTROL", "stdin")
            .env("ZENO_FCIS_CHECKER_CPUS", cpus)
            .env("ZENO_FCIS_CHECKER_NICE", "0")
            .process_group(0)
            .stdin(Stdio::from(OwnedFd::from(channel)))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        // The measured startup budget is 10 s. Keep an independent 15 s
        // harness guard, below the 25 s whole-run cap, and require the worker
        // to spend that budget before refusing. No refusal/read/output check
        // below is removed or relaxed.
        let out = wait(child, Duration::from_secs(15));
        assert!(started.elapsed() >= Duration::from_secs(10));
        let mut byte = [0];
        assert_eq!(control.read(&mut byte).unwrap_or_else(|e| panic!("{e}")), 1);
        assert_eq!(byte, [0xA0]);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("startup-ack-unavailable:deadline"));
        assert!(!Path::new(&output).exists());
    }

    #[test]
    fn refused_frozen_capture_read_creates_no_operational_or_checked_artifact() {
        let d = Directory::new();
        let before = snapshot(&d.0);
        let out = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args([
                "--no-kb",
                "loop",
                "capture",
                "packet",
                "--session",
                &d.0.to_string_lossy(),
            ])
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out.status.code(), Some(1));
        let r = json(&out);
        assert_eq!(r["authority"], "none");
        assert_eq!(r["status"], "refused");
        assert_eq!(snapshot(&d.0), before);
    }

    #[test]
    fn imported_frozen_table_packet_candidate_and_status_use_only_finite_data_guarantees() {
        use serde_json::json;
        use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
        use zeno_fcis_crypto::RustCryptoSha256;
        use zeno_fcis_synthesis::finite::{Domain, v2_authority};
        let sha = |bytes: &[u8]| {
            RustCryptoSha256::hash(bytes)
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let d = Directory::new();
        let session = d.path("frozen");
        fs::create_dir(&session).unwrap_or_else(|e| panic!("{e}"));
        let rows = vec![vec![0_i128], vec![1_i128]];
        let table = zeno_fcis_loop_doors::capture::ReferenceTable::new(
            vec![Domain::Bool],
            vec![Domain::Bool],
            rows.clone(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let program = table.program().unwrap_or_else(|e| panic!("{e:?}"));
        let bytes = program
            .value()
            .and_then(|v| v.canonical_bytes())
            .unwrap_or_else(|e| panic!("{e:?}"));
        // An imported data fixture supplies no current Python isolation evidence.
        let source_hash = "77".repeat(32);
        let evaluator = v2_authority::EVALUATOR
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let state = json!({"schema":"zeno-fcis/capture-session/1", "spec":{"inputs":[{"kind":"Bool"}],"outputs":[{"kind":"Bool"}]},
            "source_sha256":source_hash,"table_sha256":sha(&serde_json::to_vec(&rows).unwrap_or_else(|e| panic!("{e}"))),"program_sha256":sha(&bytes),
            "checker":{"semantics":"zeno-fcis/transform-check/4","evaluator_identity":evaluator},"rows":rows,"reference_observations":12,
            "budget":{"max_tuples":2,"rounds":2,"max_examples":2,"node_visits":100000},"attempts":0,"invocations":0,"reserved_visits":0,
            "examples":[],"status":"ready","events":[],"candidate":null,"receipt":null});
        fs::write(Path::new(&session).join("capture.json"), state.to_string())
            .unwrap_or_else(|e| panic!("{e}"));
        let run = |args: &[&str]| {
            let out = Command::new(CLI)
                .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
                .args(["--containment-backend", "posix", "--no-kb"])
                .args(args)
                .output()
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            json(&out)
        };
        let packet = run(&["loop", "capture", "packet", "--session", &session]);
        assert_eq!(
            packet["detail"]["packet"]
                .as_object()
                .unwrap_or_else(|| panic!("packet"))
                .len(),
            4
        );
        assert!(!packet.to_string().contains(&source_hash));
        let candidate = d.path("candidate.json");
        fs::write(&candidate, json!({"inputs":[{"kind":"Bool"}],"outputs":[{"kind":"Bool"}],"nodes":[["Input",0]],"roots":[0]}).to_string()).unwrap_or_else(|e| panic!("{e}"));
        let checked = run(&[
            "loop",
            "capture",
            "candidate",
            "--session",
            &session,
            "--candidate-json",
            &candidate,
        ]);
        assert_eq!(checked["status"], "captured-exact");
        let status = run(&["loop", "capture", "status", "--session", &session]);
        assert_eq!(status["status"], "captured-exact");
        let e: Value = serde_json::from_slice(
            &fs::read(Path::new(&session).join("containment-run.json"))
                .unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(e["required_profile"], "finite-data/1");
        let leaf = e["guarantees"]
            .as_array()
            .unwrap_or_else(|| panic!("guarantees"))
            .iter()
            .find(|g| g["control"] == "leaf-isolation")
            .unwrap_or_else(|| panic!("leaf"));
        assert_eq!(leaf["state"], "absent");
        let retained: Value = serde_json::from_slice(
            &fs::read(Path::new(&session).join("capture.json")).unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(retained["source_sha256"], source_hash);
        assert_eq!(retained["reference_observations"], 12);
    }

    #[test]
    fn a_nonregular_profile_file_refuses_before_any_worker_or_session_mutation() {
        let d = Directory::new();
        let session = d.path("session");
        fs::create_dir(&session).unwrap_or_else(|e| panic!("{e}"));
        let profile = Path::new(&session).join("containment-profile.json");
        assert!(
            Command::new("/usr/bin/mkfifo")
                .arg(&profile)
                .status()
                .unwrap_or_else(|e| panic!("{e}"))
                .success()
        );
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["--no-kb", "loop", "resume", "--session", &session])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        let out = wait(child, Duration::from_secs(2));
        assert_eq!(out.status.code(), Some(2));
        let r = json(&out);
        assert_eq!(r["authority"], "none");
        assert!(
            r["reason"]
                .as_str()
                .is_some_and(|s| s.ends_with(":session-policy-not-regular")),
            "{r}"
        );
        assert_eq!(
            fs::read_dir(&session)
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            1
        );
    }

    #[test]
    fn forged_internal_roles_cannot_admit_python_acquisition() {
        let d = Directory::new();
        let session = d.path("must-not-exist");
        let cpus = fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find_map(|s| {
                s.strip_prefix("Cpus_allowed_list:")
                    .map(|s| s.trim().to_owned())
            })
            .unwrap_or_else(|| panic!("affinity"));
        for role in ["guardian", "worker"] {
            let (_caller, channel) = UnixStream::pair().unwrap_or_else(|e| panic!("{e}"));
            let child = Command::new(CLI)
                .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
                .args([
                    "--no-kb",
                    "loop",
                    "capture",
                    "open",
                    "--spec",
                    "/must-not-read",
                    "--reference",
                    "/must-not-execute",
                    "--session",
                    &session,
                ])
                .env_clear()
                .env("LC_ALL", "C")
                .env("MALLOC_ARENA_MAX", "1")
                .env("ZENO_FCIS_POSIX_ROLE", role)
                .env("ZENO_FCIS_POSIX_INVOCATION", "forged-role-test")
                .env("ZENO_FCIS_CHECKER_CONTROL", "stdin")
                .env("ZENO_FCIS_CHECKER_CPUS", &cpus)
                .env("ZENO_FCIS_CHECKER_NICE", "0")
                .process_group(0)
                .stdin(Stdio::from(OwnedFd::from(channel)))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| panic!("{e}"));
            let out = wait(child, Duration::from_secs(3));
            assert_eq!(out.status.code(), Some(2), "{role}: {out:?}");
            assert!(out.stdout.is_empty(), "{role}: {out:?}");
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("containment-unavailable:posix:command-profile:python-reference/1"),
                "{role}: {out:?}"
            );
            assert!(!Path::new(&session).exists());
        }
    }

    #[test]
    fn guardian_loss_withholds_output_and_never_claims_verified_cleanup() {
        let d = Directory::new();
        let input = contract(&d);
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["--no-kb", "loop", "question", "--contract", &input])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        let (guardian, worker) = find_worker(child.id(), true);
        suspend_worker(worker);
        signal(guardian, nix::sys::signal::Signal::SIGKILL);
        signal(worker, nix::sys::signal::Signal::SIGCONT);
        let caller = child.id();
        let out = wait(child, Duration::from_secs(5));
        assert_eq!(out.status.code(), Some(2));
        let r = json(&out);
        assert_eq!(r["authority"], "none");
        assert_eq!(r["status"], "inconclusive");
        assert_eq!(r["cleanup"]["absence_verified"], false);
        let (_, finish, _) = verify_envelope(&r, caller);
        assert_eq!(finish["cleanup"], "unknown");
        assert_eq!(finish["absence_verified"], false);
        assert_eq!(r["accounting_complete"], false);
        assert!(r["detail"]["trusted_incumbent"].is_null());
        // The monitor stops work on control EOF. With its guardian lost, a
        // zombie awaiting the enclosing reaper is still cleanup UNKNOWN.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state = fs::read_to_string(format!("/proc/{worker}/stat"));
            if state
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                || state.as_ref().is_ok_and(|s| {
                    s.rsplit_once(')')
                        .is_some_and(|(_, s)| s.trim_start().starts_with('Z'))
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker did not stop on guardian loss"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn inherited_limits_are_never_raised_and_private_evidence_records_both_values() {
        let d = Directory::new();
        let session = d.path("session");
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/i128-programs/boolean-kernel-original.zcve");
        let out = Command::new("/usr/bin/python3").env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto").args(["-c", "import os,resource,sys\nresource.setrlimit(resource.RLIMIT_CPU,(8,8))\nresource.setrlimit(resource.RLIMIT_FSIZE,(8192,8192))\nresource.setrlimit(resource.RLIMIT_NOFILE,(32,32))\nos.execv(sys.argv[1],sys.argv[1:])", CLI, "--no-kb", "loop", "open", "--original", &original.to_string_lossy(), "--session", &session])
            .output().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(json(&out)["authority"], "none");
        let e: Value = serde_json::from_slice(
            &fs::read(Path::new(&session).join("containment-run.json"))
                .unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for (control, requested, effective) in [
            ("cpu-seconds", 25, 8),
            ("file-size", 16777216, 8192),
            ("open-files", 64, 32),
        ] {
            let g = e["guarantees"]
                .as_array()
                .unwrap_or_else(|| panic!("guarantees"))
                .iter()
                .find(|g| g["control"] == control)
                .unwrap_or_else(|| panic!("{control}"));
            assert_eq!(g["requested"]["value"], requested);
            assert_eq!(g["effective"]["value"], effective);
        }
        assert_eq!(e["cleanup"], "owned-workers-reaped");
        assert_eq!(e["absence_verified"], true);
        assert_eq!(
            e["subject_sha256"],
            hash(&fs::read(&original).unwrap_or_else(|e| panic!("{e}")))
        );
        assert!(e["semantic_receipt_sha256"].is_null());
    }
    fn private_directory(caller: u32) -> PathBuf {
        let root = PathBuf::from(format!(
            "/tmp/zeno-fcis-containment-{}",
            nix::unistd::geteuid()
        ));
        fs::read_dir(root)
            .unwrap_or_else(|e| panic!("{e}"))
            .filter_map(|entry| {
                let directory = entry.ok()?.path();
                let path = directory.join("command.json");
                let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                if value["caller_pid"] != caller {
                    return None;
                }
                Some((fs::metadata(path).ok()?.modified().ok()?, directory))
            })
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| path)
            .unwrap_or_else(|| panic!("private journal for caller {caller}"))
    }
    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(
            &fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }
    fn hash(bytes: &[u8]) -> String {
        use zeno_fcis_codec::CommitmentHasher;
        zeno_fcis_crypto::RustCryptoSha256::hash(bytes)
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn verify_envelope(r: &Value, caller: u32) -> (Value, Value, Value) {
        assert_eq!(r["backend"], "posix");
        assert_eq!(r["required_profile"], "finite-data/1");
        let directory = private_directory(caller);
        let startup = read_json(&directory.join("startup.json"));
        let final_record = read_json(&directory.join("final.json"));
        let envelope = read_json(&directory.join("envelope.json"));
        assert_eq!(envelope["schema"], "zeno-fcis/containment-envelope/1");
        assert_eq!(
            envelope["invocation_sha256"]
                .as_str()
                .unwrap_or_else(|| panic!("{envelope}"))
                .len(),
            64
        );
        assert_eq!(startup["invocation_sha256"], envelope["invocation_sha256"]);
        assert_eq!(
            final_record["invocation_sha256"],
            envelope["invocation_sha256"]
        );
        for (name, field) in [
            ("startup.json", "startup_sha256"),
            ("final.json", "final_sha256"),
        ] {
            assert_eq!(
                envelope[field],
                hash(&fs::read(directory.join(name)).unwrap_or_else(|e| panic!("{e}")))
            );
        }
        assert_eq!(envelope["receipt_bindings_complete"], true);
        (startup, final_record, envelope)
    }
    fn original() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/i128-programs/boolean-kernel-original.zcve")
    }
    fn candidate() -> PathBuf {
        original().with_file_name("boolean-kernel-candidate.zcve")
    }
    struct InvocationOutput {
        output: Output,
        caller: u32,
    }
    impl std::ops::Deref for InvocationOutput {
        type Target = Output;
        fn deref(&self) -> &Output {
            &self.output
        }
    }
    impl std::fmt::Debug for InvocationOutput {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.output.fmt(f)
        }
    }
    fn ordinary(args: &[&str]) -> InvocationOutput {
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["--no-kb", "--containment-backend", "posix"])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        let caller = child.id();
        InvocationOutput {
            output: wait(child, Duration::from_secs(10)),
            caller,
        }
    }
    #[cfg(target_arch = "x86_64")]
    fn filtered(mode: &str, args: &[&str]) -> Output {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test-data/containment_seccomp.py");
        let child = Command::new("/usr/bin/python3")
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .arg(script)
            .arg(mode)
            .arg(CLI)
            .args(["--no-kb", "--containment-backend", "posix"])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        wait(child, Duration::from_secs(8))
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn denied_group_signalling_and_reaping_refuse_before_any_session_or_receipt_write() {
        let d = Directory::new();
        let session = d.path("new-session");
        for mode in ["deny-killpg", "deny-group-absence", "deny-waitid"] {
            let before = snapshot(&d.0);
            let out = filtered(
                mode,
                &[
                    "loop",
                    "open",
                    "--original",
                    &original().to_string_lossy(),
                    "--session",
                    &session,
                ],
            );
            let r = json(&out);
            assert_eq!(out.status.code(), Some(2), "{mode}: {r}");
            assert_eq!(r["authority"], "none");
            assert!(
                r["reason"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("containment-unavailable:posix:")),
                "{r}"
            );
            assert_eq!(r["containment"]["startup"]["admitted"], false);
            assert_eq!(r["containment"]["final"]["application_work_started"], false);
            assert_eq!(snapshot(&d.0), before, "{mode}");
            assert!(!Path::new(&session).exists(), "{mode}");
        }
        let open = ordinary(&[
            "loop",
            "open",
            "--original",
            &original().to_string_lossy(),
            "--session",
            &session,
        ]);
        assert_eq!(open.status.code(), Some(0), "{open:?}");
        let before = snapshot(Path::new(&session));
        let out = filtered(
            "deny-killpg",
            &[
                "loop",
                "candidate",
                "--session",
                &session,
                "--candidate",
                &candidate().to_string_lossy(),
            ],
        );
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert_eq!(snapshot(Path::new(&session)), before);
        assert!(
            fs::read_dir(Path::new(&session).join("receipts"))
                .unwrap_or_else(|e| panic!("{e}"))
                .next()
                .is_none()
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn successful_noop_rlimit_installations_are_rejected_by_observed_readback() {
        let d = Directory::new();
        let session = d.path("must-not-exist");
        for mode in [
            "noop-limit-0",
            "noop-limit-1",
            "noop-limit-4",
            "noop-limit-7",
            "noop-limit-9",
        ] {
            let before = snapshot(&d.0);
            let out = filtered(
                mode,
                &[
                    "loop",
                    "open",
                    "--original",
                    &original().to_string_lossy(),
                    "--session",
                    &session,
                ],
            );
            let r = json(&out);
            assert_eq!(out.status.code(), Some(2), "{mode}: {r}");
            assert!(
                r["reason"].as_str().is_some_and(|s| s.contains("readback")),
                "{mode}: {r}"
            );
            assert_eq!(snapshot(&d.0), before, "{mode}");
            assert!(!Path::new(&session).exists());
        }
    }

    #[test]
    fn every_success_and_failure_appends_bound_evidence_and_receipt_envelopes() {
        let d = Directory::new();
        let session = d.path("session");
        let open = ordinary(&[
            "loop",
            "open",
            "--original",
            &original().to_string_lossy(),
            "--session",
            &session,
        ]);
        assert_eq!(open.status.code(), Some(0), "{open:?}");
        let r = json(&open);
        let first_directory = private_directory(open.caller);
        let first_snapshot = snapshot(&first_directory);
        let (startup, finish, _) = verify_envelope(&r, open.caller);
        assert_eq!(startup["termination"], "running");
        assert_eq!(startup["cleanup"], "pending");
        assert_eq!(finish["termination"], "exited");
        assert_eq!(finish["exit_code"], 0);
        let checked = ordinary(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--candidate",
            &candidate().to_string_lossy(),
        ]);
        assert_eq!(checked.status.code(), Some(0), "{checked:?}");
        let checked_caller = checked.caller;
        let checked = json(&checked);
        let (_, finish, envelope) = verify_envelope(&checked, checked_caller);
        let binding = &envelope["receipts"][0];
        let receipt = Path::new(&session).join(
            binding["location"]
                .as_str()
                .unwrap_or_else(|| panic!("{envelope}")),
        );
        let bytes = fs::read(&receipt).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(binding["semantic_receipt_sha256"], hash(&bytes));
        assert_eq!(
            finish["semantic_receipt_sha256"],
            binding["semantic_receipt_sha256"]
        );
        assert_eq!(finish["subject_sha256"], binding["candidate_sha256"]);
        let canonical = read_json(&receipt);
        assert!(canonical.get("containment").is_none());
        assert_eq!(canonical["schema"], "zeno-fcis/transform-receipt/1");
        let failure = ordinary(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--candidate",
            "/does-not-exist",
        ]);
        assert!(!failure.status.success(), "{failure:?}");
        let failure_caller = failure.caller;
        let failure = json(&failure);
        let (_, finish, envelope) = verify_envelope(&failure, failure_caller);
        assert_ne!(
            private_directory(failure_caller),
            private_directory(checked_caller)
        );
        assert_ne!(finish["exit_code"], 0);
        assert_eq!(finish["absence_verified"], true);
        assert_eq!(envelope["receipts"], serde_json::json!([]));
        assert_eq!(
            snapshot(&first_directory),
            first_snapshot,
            "old invocation evidence must remain byte-identical"
        );
        let sessionless = ordinary(&[
            "loop",
            "encode",
            "--program",
            "/does-not-exist",
            "--out",
            &d.path("must-not-exist"),
        ]);
        assert!(!sessionless.status.success());
        let (_, finish, _) = verify_envelope(&json(&sessionless), sessionless.caller);
        assert_ne!(finish["exit_code"], 0);
    }

    #[test]
    fn whole_invocation_deadline_stops_an_idle_worker_without_an_active_stage() {
        let d = Directory::new();
        let input = contract(&d);
        // Stop at exec/startup, before a stage starts. A scheduling race may
        // catch an already active search instead; retain that cleanup check
        // and retry setup at most three times. Only a full 25s stop qualifies.
        for attempt in 0..3 {
            let started = Instant::now();
            let mut launcher = Command::new(CLI);
            launcher.env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto");
            launcher.args(["--no-kb", "loop", "question", "--contract", &input]);
            // Bounded argv parsing gives the harness a startup observation
            // window. --no-kb prevents these inert path arguments being opened.
            for _ in 0..512 {
                launcher.args(["--kb", "/unused-no-kb-path"]);
            }
            let child = launcher
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| panic!("{e}"));
            let (guardian_pid, worker) = find_worker(child.id(), true);
            let stat = fs::read_to_string(format!("/proc/{guardian_pid}/stat")).unwrap();
            let guardian_ticks: u64 = stat
                .rsplit_once(") ")
                .unwrap()
                .1
                .split_whitespace()
                .nth(19)
                .unwrap()
                .parse()
                .unwrap();
            signal(worker, nix::sys::signal::Signal::SIGSTOP);
            let caller = child.id();
            // Independent harness: caller guardian30s plus bounded3s+1s
            // cancellation/reap can exceed32s. This does not renew either
            // product cap; all worker-clock/refusal/cleanup assertions below
            // remain mandatory. The separate whole-CLI bound remains180s.
            let out = wait(child, Duration::from_secs(40));
            let elapsed = started.elapsed();
            let r = json(&out);
            assert_eq!(out.status.code(), Some(2), "{r}");
            assert_eq!(
                r["reason"],
                format!(
                    "contained-deadline; PosixOwnedWorker={worker}\nCleanupError=None\nWorkerDiagnostic=\n"
                )
            );
            let (_, finish, _) = verify_envelope(&r, caller);
            assert_eq!(finish["termination"], "stopped");
            assert_eq!(finish["absence_verified"], true);
            assert_absent(worker);
            let root =
                Path::new("/tmp").join(format!("zeno-fcis-operator-{}", nix::unistd::geteuid()));
            let records: Vec<_> = fs::read_dir(root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(&format!("{guardian_pid}-"))
                })
                .map(|p| read_json(&p))
                .filter(|r| {
                    r["observer"] == "worker-supervisor"
                        && r["process_start_ticks"] == guardian_ticks
                })
                .collect();
            assert_eq!(records.len(), 1, "{records:?}");
            let timing = &records[0];
            assert_eq!(timing["worker_pid"], worker);
            assert_eq!(timing["worker_budget_ms"], 25000);
            let worker_ms = timing["worker_observed_ms"].as_u64().unwrap();
            // One second is scheduling/observation tolerance, not a renewed cap.
            assert!(
                worker_ms < 26000,
                "worker deadline was not enforced: {timing}"
            );
            assert!(
                elapsed < Duration::from_secs(180),
                "independent harness bound: {elapsed:?}"
            );
            if worker_ms >= 24000 {
                return;
            }
            assert!(
                attempt < 2,
                "could not establish an idle worker; only a stage deadline was exercised"
            );
        }
    }

    #[test]
    fn caller_death_retains_guardian_final_evidence_after_owned_cleanup() {
        let d = Directory::new();
        let input = contract(&d);
        let child = Command::new(CLI)
            .env("ZENO_FCIS_CONTAINMENT_BACKEND", "auto")
            .args(["--no-kb", "loop", "question", "--contract", &input])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        let (guardian, worker) = find_worker(child.id(), true);
        suspend_worker(worker);
        let environment =
            fs::read(format!("/proc/{guardian}/environ")).unwrap_or_else(|e| panic!("{e}"));
        let identity = environment
            .split(|b| *b == 0)
            .find_map(|s| s.strip_prefix(b"ZENO_FCIS_POSIX_INVOCATION="))
            .unwrap_or_else(|| panic!("guardian identity"));
        let directory = PathBuf::from(format!(
            "/tmp/zeno-fcis-containment-{}",
            nix::unistd::geteuid()
        ))
        .join(std::str::from_utf8(identity).unwrap_or_else(|e| panic!("{e}")));
        signal(child.id(), nix::sys::signal::Signal::SIGKILL);
        let _ = wait(child, Duration::from_secs(5));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.join("guardian-envelope.json").exists() {
            assert!(
                Instant::now() < deadline,
                "guardian must retain final evidence without caller"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let finish = read_json(&directory.join("guardian-final.json"));
        let envelope = read_json(&directory.join("guardian-envelope.json"));
        assert_eq!(
            finish["termination_reason"],
            format!(
                "caller-eof; PosixOwnedWorker={worker}\nCleanupError=None\nWorkerDiagnostic=\n"
            )
        );
        assert_eq!(finish["cleanup"], "owned-workers-reaped");
        assert_eq!(finish["absence_verified"], true);
        assert_eq!(envelope["observer"], "guardian");
        assert_eq!(
            envelope["final_sha256"],
            hash(
                &fs::read(directory.join("guardian-final.json")).unwrap_or_else(|e| panic!("{e}"))
            )
        );
        assert_absent(worker);
    }
    #[test]
    fn checker_reported_receipt_hashes_are_bound_when_no_receipt_file_is_retained() {
        let d = Directory::new();
        let session = d.path("session");
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../zeno-fcis-loop-doors/tests/data");
        let program = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/i128-door-programs/idempotency-slot-c2.original.zcve");
        let open = ordinary(&[
            "loop",
            "open",
            "--original",
            &program.to_string_lossy(),
            "--session",
            &session,
        ]);
        assert_eq!(open.status.code(), Some(0), "{open:?}");
        let out = ordinary(&[
            "loop",
            "sketch",
            "--session",
            &session,
            "--sketch",
            &data.join("is-c2-s1.json").to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let r = json(&out);
        let check = &r["detail"]["door"]["check"];
        assert_eq!(check["verdict"], "equivalent");
        let (_, finish, envelope) = verify_envelope(&r, out.caller);
        let rows = envelope["worker_output"]["reported_receipts"]
            .as_array()
            .unwrap_or_else(|| panic!("{envelope}"));
        assert!(rows.iter().any(
            |row| row["semantic_receipt_sha256"] == check["receipt_sha256"]
                && row["candidate_sha256"] == check["candidate_sha256"]
        ));
        assert_eq!(finish["semantic_receipt_sha256"], check["receipt_sha256"]);
        assert_eq!(finish["subject_sha256"], check["candidate_sha256"]);
    }
}
