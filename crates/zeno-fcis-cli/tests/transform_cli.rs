//! `zeno-fcis transform` through the real binary: exit codes, receipt files and replay.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::Program;
use zeno_fcis_synthesis::finite_runtime::import_program;

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-transform-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn artifact(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/benchmarks/withdrawal-queue/artifacts")
        .join(format!("{name}.zcve"))
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn run(arguments: &[&std::ffi::OsStr]) -> (Option<i32>, Value) {
    let output: Output = Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn check(original: &Path, candidate: &Path, extra: &[&str]) -> (Option<i32>, Value) {
    let mut arguments = vec![
        "transform".as_ref(),
        "check".as_ref(),
        "--original".as_ref(),
        original.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
    ];
    arguments.extend(extra.iter().map(|argument| std::ffi::OsStr::new(*argument)));
    run(&arguments)
}

fn replay(
    receipt: &Path,
    original: &Path,
    candidate: &Path,
    extra: &[&str],
) -> (Option<i32>, Value) {
    let mut arguments = vec![
        "transform".as_ref(),
        "replay".as_ref(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--original".as_ref(),
        original.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
    ];
    arguments.extend(extra.iter().map(|argument| std::ffi::OsStr::new(*argument)));
    run(&arguments)
}

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The kernel candidate with its first two outputs swapped.
fn swapped_kernel_candidate() -> Vec<u8> {
    let candidate = import_program(&read(&artifact("boolean-kernel-candidate")))
        .unwrap_or_else(|error| panic!("import: {error}"));
    let mut roots = candidate.roots().to_vec();
    roots.swap(0, 1);
    Program::try_new(
        candidate.inputs().to_vec(),
        candidate.outputs().to_vec(),
        candidate.nodes().to_vec(),
        roots,
    )
    .and_then(|program| Ok(program.value()?.canonical_bytes()?))
    .unwrap_or_else(|error| panic!("swapped candidate: {error}"))
}

#[test]
fn equivalent_pair_writes_a_canonical_receipt_that_replays() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let receipt = directory.0.join("receipt.json");
    let receipt_text = receipt.to_string_lossy().into_owned();
    let (exit, report) = check(&original, &candidate, &["--receipt", &receipt_text]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["schema"], "zeno-fcis/transform-result/1");
    assert_eq!(report["status"], "equivalent");
    assert_eq!(report["authority"], "none");
    assert_eq!(
        report["limits"],
        json!({"steps": 256, "input_tuples": 100_000_000})
    );
    assert_eq!(report["original_sha256"], sha256(&read(&original)));
    assert_eq!(report["candidate_sha256"], sha256(&read(&candidate)));
    let detail = &report["detail"];
    assert_eq!(detail["receipt_path"], receipt_text.as_str());
    let bytes = read(&receipt);
    assert_eq!(detail["receipt_sha256"], sha256(&bytes));
    let written: Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("receipt: {error}"));
    assert_eq!(detail["receipt"], written);
    // Compact, sorted keys, one trailing newline.
    let mut canonical =
        serde_json::to_vec(&written).unwrap_or_else(|error| panic!("encode: {error}"));
    canonical.push(b'\n');
    assert_eq!(bytes, canonical);
    assert_eq!(written["inputs_checked"], 16);
    assert_eq!(written["domain"]["inputs"][0], json!({"kind": "bool"}));
    assert_eq!(written["usage"]["usage_preserved"], false);
    let (exit, report) = replay(&receipt, &original, &candidate, &[]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "replayed");
    assert_eq!(report["detail"]["receipt_sha256"], sha256(&bytes));
}

#[test]
fn replay_refuses_a_tampered_receipt_or_other_programs() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let receipt = directory.0.join("receipt.json");
    let (exit, _) = check(
        &original,
        &candidate,
        &["--receipt", &receipt.to_string_lossy()],
    );
    assert_eq!(exit, Some(0));
    let value: Value =
        serde_json::from_slice(&read(&receipt)).unwrap_or_else(|error| panic!("receipt: {error}"));
    let rewrite = |name: &str, change: &dyn Fn(&mut Value)| {
        let mut changed = value.clone();
        change(&mut changed);
        let mut bytes = serde_json::to_vec(&changed).unwrap_or_default();
        bytes.push(b'\n');
        directory.write(name, &bytes)
    };
    let tampered = rewrite("tampered.json", &|value| {
        value["usage"]["max_steps"]["candidate"] = json!(6);
    });
    let (exit, report) = replay(&tampered, &original, &candidate, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "replay-mismatch");
    assert_eq!(report["detail"]["differing_fields"], json!(["usage"]));
    // A binding Step limit is rerun and reported as such.
    let binding = rewrite("binding.json", &|value| value["limits"]["steps"] = json!(6));
    let (exit, report) = replay(&binding, &original, &candidate, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(
        report["detail"]["recomputed"]["detail"]["cause"],
        "budget-boundary"
    );
    // Other programs fail on their digests before any evaluation.
    let swapped = directory.write("swapped.zcve", &swapped_kernel_candidate());
    let (exit, report) = replay(&receipt, &original, &swapped, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["differing_fields"], json!(["candidate"]));
    // The verifier's cap, not the receipt's, bounds the work.
    let (exit, report) = replay(
        &receipt,
        &original,
        &candidate,
        &["--max-input-tuples", "15"],
    );
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(
        report["detail"],
        json!({"cause": "domain-too-large", "domain_size": "16", "max_input_tuples": 15})
    );
    let garbage = directory.write("garbage.json", b"{}");
    let (exit, report) = replay(&garbage, &original, &candidate, &[]);
    assert_eq!(exit, Some(1));
    assert_eq!(report["status"], "invalid-receipt");
}

#[test]
fn counterexample_reports_both_observations_and_writes_no_receipt() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let swapped = directory.write("swapped.zcve", &swapped_kernel_candidate());
    let receipt = directory.0.join("receipt.json");
    let (exit, report) = check(
        &original,
        &swapped,
        &["--receipt", &receipt.to_string_lossy()],
    );
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "counterexample");
    assert_eq!(
        report["detail"],
        json!({
            "ordinal": 1, "input": ["0", "0", "0", "1"],
            "original": {"ok": ["0", "1", "1"], "steps": 16},
            "candidate": {"ok": ["1", "0", "1"], "steps": 7}
        })
    );
    assert!(receipt.symlink_metadata().is_err());
}

#[test]
fn a_binding_step_limit_is_inconclusive() {
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    for (limit, over_original, over_candidate) in [("6", 16, 16), ("7", 16, 0)] {
        let (exit, report) = check(&original, &candidate, &["--step-limit", limit]);
        assert_eq!(exit, Some(2), "{report}");
        assert_eq!(report["status"], "inconclusive");
        assert_eq!(
            report["limits"]["steps"],
            limit.parse::<u64>().unwrap_or_default()
        );
        assert_eq!(
            report["detail"],
            json!({
                "cause": "budget-boundary",
                "over_limit": {"original": over_original, "candidate": over_candidate},
                "minimum_step_limit": 16,
                "usage": {
                    "max_steps": {"original": 16, "candidate": 7},
                    "candidate_uses_more": 0, "usage_preserved": false
                }
            })
        );
    }
    let (exit, report) = check(&original, &candidate, &["--step-limit", "16"]);
    assert_eq!(exit, Some(0), "{report}");
}

#[test]
fn refusals_and_inconclusive_checks_have_distinct_exits() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let candidate = artifact("boolean-kernel-candidate");
    let (exit, report) = check(&original, &candidate, &["--max-input-tuples", "15"]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(
        report["detail"],
        json!({"cause": "domain-too-large", "domain_size": "16", "max_input_tuples": 15})
    );
    let controller = artifact("retained-controller-original");
    let (exit, report) = check(&original, &controller, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused");
    assert_eq!(report["detail"]["reason"], "input-abi-mismatch");
    assert_eq!(report["detail"]["original"][0], json!({"kind": "bool"}));
    assert_eq!(
        report["detail"]["candidate"][2],
        json!({"kind": "int", "min": "0", "max": "2"})
    );
    let mut trailing = read(&candidate);
    trailing.push(0);
    let trailing = directory.write("trailing.zcve", &trailing);
    let (exit, report) = check(&original, &trailing, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(
        report["detail"],
        json!({"reason": "candidate-not-admitted", "code": "program-encoding"})
    );
    // A file above the importer's limit is refused before it is hashed.
    let oversized = directory.write("oversized.zcve", &vec![0; 64 * 1024 + 1]);
    let (exit, report) = check(&original, &oversized, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(
        report["detail"],
        json!({"reason": "candidate-not-admitted", "code": "program-too-large", "max_bytes": 65536})
    );
    assert!(report.get("candidate_sha256").is_none());
}

#[test]
fn receipt_files_are_never_overwritten() {
    let directory = Directory::new();
    let existing = directory.write("receipt.json", b"keep");
    let dangling = directory.0.join("dangling.json");
    std::os::unix::fs::symlink(directory.0.join("absent"), &dangling)
        .unwrap_or_else(|error| panic!("symlink: {error}"));
    for path in [&existing, &dangling] {
        let (exit, report) = check(
            &artifact("boolean-kernel-original"),
            &artifact("boolean-kernel-candidate"),
            &["--receipt", &path.to_string_lossy()],
        );
        assert_eq!(exit, Some(1), "{report}");
        assert_eq!(report["status"], "receipt-exists");
    }
    assert_eq!(read(&existing), b"keep");
    assert!(!directory.0.join("absent").exists());
}

#[test]
fn unreadable_inputs_fail_without_blocking() {
    let directory = Directory::new();
    let candidate = artifact("boolean-kernel-candidate");
    let missing = directory.0.join("missing.zcve");
    let (exit, report) = check(&missing, &candidate, &[]);
    assert_eq!(exit, Some(3), "{report}");
    assert_eq!(report["status"], "io-error");
    let fifo = directory.0.join("fifo.zcve");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap_or_else(|error| panic!("mkfifo: {error}"))
            .success()
    );
    let started = Instant::now();
    let (exit, report) = check(&fifo, &candidate, &[]);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(exit, Some(3), "{report}");
    assert_eq!(report["status"], "io-error");
}

#[test]
fn describe_declares_transform_effects() {
    let (exit, report) = run(&["describe".as_ref(), "transform".as_ref()]);
    assert_eq!(exit, Some(0));
    let group = &report["command"];
    assert_eq!(group["effects"]["classification"], "command-group");
    let names: Vec<_> = group["subcommands"]
        .as_array()
        .unwrap_or_else(|| panic!("subcommands"))
        .iter()
        .map(|command| command["name"].clone())
        .collect();
    assert_eq!(names, [json!("check"), json!("replay")]);
    assert_eq!(
        group["subcommands"][0]["effects"],
        json!({
            "classification": "declared", "executes_tools": false, "read_only_flag": null,
            "reads": ["original-program", "candidate-program"],
            "writes": ["optional-equivalence-receipt"]
        })
    );
    let replay = &group["subcommands"][1];
    assert_eq!(replay["effects"]["writes"], json!([]));
    let cap = replay["arguments"]
        .as_array()
        .unwrap_or_else(|| panic!("replay arguments"))
        .iter()
        .find(|argument| argument["id"] == "max_input_tuples")
        .unwrap_or_else(|| panic!("replay declares its own tuple cap"));
    assert_eq!(cap["defaults"], json!(["100000000"]));
}
