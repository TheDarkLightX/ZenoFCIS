//! `zeno-fcis optimize` through the real binary: exit codes, output files,
//! replay of the written receipt, strategy refusal and determinism.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite_runtime::import_program;

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-optimize-{}-{}",
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

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn run_raw(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn run(arguments: &[&std::ffi::OsStr]) -> (Option<i32>, Value) {
    let output = run_raw(arguments);
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn optimize(program: &Path, extra: &[&str]) -> (Option<i32>, Value) {
    let mut arguments = vec![
        "optimize".as_ref(),
        "--program".as_ref(),
        program.as_os_str(),
    ];
    arguments.extend(extra.iter().map(|argument| std::ffi::OsStr::new(*argument)));
    run(&arguments)
}

fn replay(receipt: &Path, original: &Path, candidate: &Path) -> (Option<i32>, Value) {
    run(&[
        "transform".as_ref(),
        "replay".as_ref(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--original".as_ref(),
        original.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
    ])
}

#[test]
fn kernel_improves_to_seven_nodes_and_the_written_receipt_replays() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let candidate = directory.0.join("candidate.zcve");
    let receipt = directory.0.join("receipt.json");
    let (exit, report) = optimize(
        &original,
        &[
            "--candidate-out",
            &candidate.to_string_lossy(),
            "--receipt",
            &receipt.to_string_lossy(),
        ],
    );
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["schema"], "zeno-fcis/optimize-result/1");
    assert_eq!(report["status"], "improved");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["original_sha256"], sha256(&read(&original)));
    assert_eq!(
        report["limits"],
        json!({"steps": 256, "input_tuples": 100_000_000})
    );
    assert!(
        report["strategy_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)
    );
    let detail = &report["detail"];
    assert_eq!(detail["original"]["nodes"], 16);
    assert_eq!(detail["best"]["nodes"], 7);
    assert_eq!(detail["best"]["max_steps"], 7);
    assert_eq!(
        detail["candidate_path"],
        candidate.to_string_lossy().as_ref()
    );
    assert_eq!(detail["receipt_path"], receipt.to_string_lossy().as_ref());
    // Without --strategy the fixed portfolio runs its three strategies.
    assert_eq!(
        detail["strategy"]["schema"],
        "zeno-fcis/optimize-portfolio/1"
    );
    assert_eq!(
        detail["strategy"]["strategies"][0]["schema"],
        "zeno-fcis/optimize-strategy/1"
    );
    assert_eq!(detail["domain"]["exact_signatures"], true);
    assert_eq!(detail["search"]["phases_requested"], 36);
    assert_eq!(detail["search"]["phases_run"], 36);
    let runs = detail["search"]["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("runs"));
    assert_eq!(runs.len(), 3);
    // Only the cycles strategy reaches a limit here, and the report names
    // it on each phase concerned.
    let limited: Vec<u64> = detail["search"]["phases"]
        .as_array()
        .unwrap_or_else(|| panic!("phases"))
        .iter()
        .filter(|phase| !phase["limit_hit"].is_null())
        .map(|phase| phase["run"].as_u64().unwrap_or(u64::MAX))
        .collect();
    assert!(limited.iter().all(|run| *run == 2), "{limited:?}");
    assert_eq!(detail["search"]["any_limit_hit"], !limited.is_empty());
    assert_eq!(detail["best"]["run"], 0);
    // The written files are what the report describes.
    let bytes = read(&candidate);
    assert_eq!(detail["best"]["sha256"], sha256(&bytes));
    assert_eq!(detail["best"]["bytes"], bytes.len());
    let program = import_program(&bytes).unwrap_or_else(|error| panic!("import: {error}"));
    assert_eq!(program.nodes().len(), 7);
    let receipt_bytes = read(&receipt);
    assert_eq!(detail["best"]["receipt_sha256"], sha256(&receipt_bytes));
    let written: Value =
        serde_json::from_slice(&receipt_bytes).unwrap_or_else(|error| panic!("receipt: {error}"));
    assert_eq!(detail["best"]["receipt"], written);
    assert_eq!(written["schema"], "zeno-fcis/transform-receipt/1");
    assert_eq!(written["candidate"]["sha256"], sha256(&bytes));
    // `transform replay` reproduces the receipt from the two programs.
    let (exit, report) = replay(&receipt, &original, &candidate);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "replayed");
    assert_eq!(report["detail"]["receipt_sha256"], sha256(&receipt_bytes));
}

#[test]
fn repeated_runs_print_identical_reports() {
    let directory = Directory::new();
    // The default strategy alone on the controller, and the portfolio on
    // the kernel; the portfolio takes tens of seconds on the controller in
    // a debug build.
    let strategy = directory.write("default.json", DEFAULT_STRATEGY.as_bytes());
    for (name, extra) in [
        (
            "retained-controller-original",
            vec![
                "--strategy".to_owned(),
                strategy.to_string_lossy().into_owned(),
            ],
        ),
        ("boolean-kernel-original", Vec::new()),
    ] {
        let original = artifact(name);
        let mut arguments: Vec<&std::ffi::OsStr> = vec![
            "optimize".as_ref(),
            "--program".as_ref(),
            original.as_os_str(),
        ];
        arguments.extend(extra.iter().map(std::ffi::OsStr::new));
        let first = run_raw(&arguments);
        let second = run_raw(&arguments);
        assert_eq!(first.status.code(), Some(0), "{name}");
        assert_eq!(first.stdout, second.stdout, "{name}");
        let report: Value =
            serde_json::from_slice(&first.stdout).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(report["status"], "improved");
        assert_eq!(report["detail"]["candidate_path"], Value::Null);
        assert_eq!(report["detail"]["receipt_path"], Value::Null);
        assert_eq!(report["detail"]["domain"]["exact_signatures"], true);
        let expected = if name == "boolean-kernel-original" {
            7
        } else {
            36
        };
        assert_eq!(report["detail"]["best"]["nodes"], expected, "{name}");
    }
}

/// The optimizer's default strategy document, version 1.
const DEFAULT_STRATEGY: &str = r#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [
    {"phase": "fold", "rounds": 1}, {"phase": "share", "rounds": 1},
    {"phase": "boolean", "rounds": 3}, {"phase": "select", "rounds": 3},
    {"phase": "semantic-merge", "rounds": 2}, {"phase": "boolean", "rounds": 2},
    {"phase": "select", "rounds": 2}, {"phase": "fold", "rounds": 1}],
  "limits": {"max_enodes": 20000, "max_classes": 10000,
             "max_rewrites_per_round": 10000, "max_extraction_rounds": 8},
  "extractor": "dag-greedy"}"#;

#[test]
fn a_minimal_program_has_no_checked_improvement() {
    let directory = Directory::new();
    // The recorded kernel candidate is already seven nodes.
    let candidate = artifact("boolean-kernel-candidate");
    let out = directory.0.join("unwritten.zcve");
    let (exit, report) = optimize(&candidate, &["--candidate-out", &out.to_string_lossy()]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "no-checked-improvement");
    assert_eq!(report["detail"]["best"], Value::Null);
    assert_eq!(report["detail"]["original"]["nodes"], 7);
    let verdicts: Vec<&Value> = report["detail"]["candidates"]
        .as_array()
        .unwrap_or_else(|| panic!("candidates"))
        .iter()
        .map(|candidate| &candidate["verdict"])
        .collect();
    assert!(
        verdicts.iter().all(|verdict| **verdict == "not-smaller"),
        "{verdicts:?}"
    );
    assert!(!out.exists(), "nothing is written without an improvement");
}

#[test]
fn a_supplied_strategy_is_used_and_an_invalid_one_is_refused() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let strategy = directory.write(
        "strategy.json",
        br#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "boolean", "rounds": 1}], "extractor": "tree"}"#,
    );
    let (exit, report) = optimize(&original, &["--strategy", &strategy.to_string_lossy()]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "improved");
    assert_eq!(report["strategy_sha256"], sha256(&read(&strategy)));
    assert_eq!(report["detail"]["strategy"]["extractor"], "tree");
    assert_eq!(report["detail"]["search"]["phases_run"], 1);
    // The reference tree extractor counts a shared operand twice, so it
    // keeps the De Morgan forms whose operands are shared; dag-greedy gets 7.
    assert_eq!(report["detail"]["best"]["nodes"], 14);
    let dag = directory.write(
        "dag.json",
        br#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "boolean", "rounds": 1}]}"#,
    );
    let (exit, report) = optimize(&original, &["--strategy", &dag.to_string_lossy()]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["strategy"]["extractor"], "dag-greedy");
    assert_eq!(report["detail"]["best"]["nodes"], 7);
    for (name, text) in [
        (
            "unknown-key.json",
            r#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "boolean", "rounds": 1}], "egg": true}"#,
        ),
        (
            "unknown-phase.json",
            r#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "ilp", "rounds": 1}]}"#,
        ),
        (
            "bad-rounds.json",
            r#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "boolean", "rounds": 99}]}"#,
        ),
        (
            "wrong-schema.json",
            r#"{"schema": "zeno-fcis/optimize-strategy/9", "phases": [{"phase": "boolean", "rounds": 1}]}"#,
        ),
        ("garbage.json", "not json"),
    ] {
        let path = directory.write(name, text.as_bytes());
        let (exit, report) = optimize(&original, &["--strategy", &path.to_string_lossy()]);
        assert_eq!(exit, Some(1), "{name}: {report}");
        assert_eq!(report["status"], "invalid-strategy", "{name}");
        assert!(
            report["detail"]["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }
    let oversized = directory.write("oversized.json", &vec![b' '; 64 * 1024 + 1]);
    let (exit, report) = optimize(&original, &["--strategy", &oversized.to_string_lossy()]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "invalid-strategy");
}

#[test]
fn a_profile_bounds_the_search_and_conflicts_are_refused() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let (exit, report) = optimize(&original, &["--profile", "functional-bool-v1"]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["best"]["nodes"], 7);
    // The profile applies to every strategy of the portfolio.
    let strategies = report["detail"]["strategy"]["strategies"]
        .as_array()
        .unwrap_or_else(|| panic!("strategies"));
    assert_eq!(strategies.len(), 3);
    for strategy in strategies {
        assert_eq!(strategy["profile"], "functional-bool-v1");
    }
    // A strategy naming another profile than --profile is refused.
    let named = directory.write(
        "named.json",
        br#"{"schema": "zeno-fcis/optimize-strategy/1", "phases": [{"phase": "boolean", "rounds": 1}], "profile": "checked-i64-v1"}"#,
    );
    let (exit, report) = optimize(
        &original,
        &[
            "--strategy",
            &named.to_string_lossy(),
            "--profile",
            "functional-bool-v1",
        ],
    );
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "invalid-strategy");
    // An original whose inputs the profile cannot hold (eight, one of them
    // an integer) is refused before any search.
    let controller = artifact("retained-controller-original");
    let (exit, report) = optimize(&controller, &["--profile", "functional-bool-v1"]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused");
    assert_eq!(report["detail"]["reason"], "original-outside-profile");
    assert_eq!(report["detail"]["profile"]["reason"], "profile-inputs");
    // An unknown profile name is a usage error.
    let output = run_raw(&[
        "optimize".as_ref(),
        "--program".as_ref(),
        original.as_os_str(),
        "--profile".as_ref(),
        "functional-bool-v2".as_ref(),
    ]);
    assert_ne!(output.status.code(), Some(0));
}

#[test]
fn supplied_candidates_are_checked_before_they_are_fused() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let recorded = artifact("boolean-kernel-candidate");
    let wrong = directory.write("wrong.zcve", &read(&artifact("boolean-kernel-padded")));
    let strategy = directory.write("default.json", DEFAULT_STRATEGY.as_bytes());
    let (exit, report) = optimize(
        &original,
        &[
            "--strategy",
            &strategy.to_string_lossy(),
            "--with-candidate",
            &recorded.to_string_lossy(),
            "--with-candidate",
            &wrong.to_string_lossy(),
        ],
    );
    assert_eq!(exit, Some(0), "{report}");
    let supplied = report["detail"]["supplied"]
        .as_array()
        .unwrap_or_else(|| panic!("supplied"));
    assert_eq!(supplied.len(), 2);
    assert_eq!(supplied[0]["sha256"], sha256(&read(&recorded)));
    assert_eq!(supplied[0]["fused"], true);
    // The padded kernel is equivalent but larger, so it is fused, not chosen.
    assert_eq!(supplied[1]["verdict"], "accepted-not-better");
    assert_eq!(report["detail"]["best"]["nodes"], 7);
    // More than eight supplied programs are refused before any search.
    let mut arguments = Vec::new();
    for _ in 0..9 {
        arguments.push("--with-candidate".to_owned());
        arguments.push(recorded.to_string_lossy().into_owned());
    }
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let (exit, report) = optimize(&original, &arguments);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["reason"], "too-many-candidates");
}

#[test]
fn refusals_inconclusive_results_and_io_errors_have_distinct_exits() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let (exit, report) = optimize(&original, &["--max-input-tuples", "15"]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(
        report["detail"],
        json!({"cause": "domain-too-large", "domain_size": "16", "max_input_tuples": 15})
    );
    let mut trailing = read(&original);
    trailing.push(0);
    let trailing = directory.write("trailing.zcve", &trailing);
    let (exit, report) = optimize(&trailing, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused");
    assert_eq!(
        report["detail"],
        json!({"reason": "original-not-admitted", "code": "program-encoding"})
    );
    let oversized = directory.write("oversized.zcve", &vec![0; 64 * 1024 + 1]);
    let (exit, report) = optimize(&oversized, &[]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(
        report["detail"],
        json!({"reason": "original-not-admitted", "code": "program-too-large", "max_bytes": 65536})
    );
    assert!(report.get("original_sha256").is_none());
    let (exit, report) = optimize(&directory.0.join("missing.zcve"), &[]);
    assert_eq!(exit, Some(3), "{report}");
    assert_eq!(report["status"], "io-error");
}

#[test]
fn output_files_are_never_overwritten() {
    let directory = Directory::new();
    let original = artifact("boolean-kernel-original");
    let existing = directory.write("candidate.zcve", b"keep");
    let dangling = directory.0.join("dangling.json");
    std::os::unix::fs::symlink(directory.0.join("absent"), &dangling)
        .unwrap_or_else(|error| panic!("symlink: {error}"));
    let (exit, report) = optimize(&original, &["--candidate-out", &existing.to_string_lossy()]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "output-exists");
    assert_eq!(read(&existing), b"keep");
    let (exit, report) = optimize(&original, &["--receipt", &dangling.to_string_lossy()]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "output-exists");
    assert!(!directory.0.join("absent").exists());
}

#[test]
fn describe_declares_optimize_effects() {
    let (exit, report) = run(&["describe".as_ref(), "optimize".as_ref()]);
    assert_eq!(exit, Some(0));
    let command = &report["command"];
    assert_eq!(command["name"], "optimize");
    assert_eq!(
        command["effects"],
        json!({
            "classification": "declared", "executes_tools": false, "read_only_flag": null,
            "reads": ["original-program", "optional-strategy", "optional-candidate-programs"],
            "writes": ["optional-candidate-program", "optional-equivalence-receipt"]
        })
    );
    let arguments = command["arguments"]
        .as_array()
        .unwrap_or_else(|| panic!("arguments"));
    let ids: Vec<&str> = arguments
        .iter()
        .filter_map(|argument| argument["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "candidate_out",
            "help",
            "max_input_tuples",
            "profile",
            "program",
            "receipt",
            "strategy",
            "with_candidate"
        ]
    );
    let cap = arguments
        .iter()
        .find(|argument| argument["id"] == "max_input_tuples")
        .unwrap_or_else(|| panic!("tuple cap"));
    assert_eq!(cap["defaults"], json!(["100000000"]));
    assert_eq!(command["subcommands"], json!([]));
}
