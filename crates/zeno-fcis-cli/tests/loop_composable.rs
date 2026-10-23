//! Real CLI controls for guarded pipelines, strategy freeze and concrete rules.
//! No proposer result is treated as a judge: exported programs are checked and
//! their receipts replayed independently. LLM-generated test scenarios.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_loop_doors::learn::{
    self, Budget as LearnBudget, Limits as LearnLimits, Operation, Specification,
};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fcis-program-loops-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("scratch: {e}"));
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap_or_else(|e| panic!("fixture: {e}"));
        path
    }
    fn json(&self, name: &str, value: &Value) -> PathBuf {
        self.file(
            name,
            &serde_json::to_vec(value).unwrap_or_else(|e| panic!("JSON: {e}")),
        )
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(arguments: &[String]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(arguments)
        .output()
        .unwrap_or_else(|e| panic!("CLI: {e}"));
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "JSON: {e}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap_or(-1), report)
}
fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_owned()).collect()
}
fn sha(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn bytes(program: &Program) -> Vec<u8> {
    program
        .value()
        .and_then(|v| v.canonical_bytes())
        .unwrap_or_else(|e| panic!("encoding: {e:?}"))
}
fn scalar(high: i128, nodes: Vec<Op>, root: u16) -> Program {
    Program::try_new(
        vec![Domain::Int { min: 0, max: high }],
        vec![Domain::Int {
            min: i128::MIN,
            max: i128::MAX,
        }],
        nodes,
        vec![root],
    )
    .unwrap_or_else(|e| panic!("program: {e:?}"))
}
fn long(high: i128) -> Program {
    scalar(high, vec![Op::Input(0), Op::Add(0, 0), Op::Sub(1, 0)], 2)
}
fn exported(program: &Program) -> Value {
    let domain = |d: &Domain| match d {
        Domain::Bool => json!({"kind":"Bool"}),
        Domain::Int { min, max } => {
            json!({"kind":"Int","min":min.to_string(),"max":max.to_string()})
        }
        _ => panic!("unsupported fixture domain"),
    };
    let nodes = program
        .nodes()
        .iter()
        .map(|n| match n {
            Op::Input(k) => json!(["Input", k]),
            Op::Int(v) => json!(["Int", v.to_string()]),
            Op::Bool(v) => json!(["Bool", v]),
            Op::Add(a, b) => json!(["Add", a, b]),
            Op::Sub(a, b) => json!(["Sub", a, b]),
            Op::Eq(a, b) => json!(["Eq", a, b]),
            Op::Lt(a, b) => json!(["Lt", a, b]),
            Op::Not(a) => json!(["Not", a]),
            Op::Select(c, a, b) => json!(["Select", c, a, b]),
            _ => panic!("unsupported fixture node"),
        })
        .collect::<Vec<_>>();
    json!({"inputs":program.inputs().iter().map(domain).collect::<Vec<_>>(),
        "outputs":program.outputs().iter().map(domain).collect::<Vec<_>>(),"nodes":nodes,"roots":program.roots()})
}
fn packet(original: &Path) -> Value {
    let (exit, report) = run(&args(&[
        "loop",
        "pipeline",
        "--original",
        &original.to_string_lossy(),
        "--packet",
    ]));
    assert_eq!(exit, 0, "{report}");
    report["detail"].clone()
}
fn proposal(original: &Path, candidate: Value, stages: &[&str]) -> Value {
    let packet = packet(original);
    json!({"schema":"zeno-fcis/loop-pipeline/1","original_sha256":packet["original_sha256"],
        "implementation_sha256":packet["implementation_sha256"],"program":candidate,"stages":stages})
}
fn expressions(original: &Path, text: &str, stages: &[&str]) -> Value {
    let packet = packet(original);
    json!({"schema":"zeno-fcis/loop-pipeline/1","original_sha256":packet["original_sha256"],
        "implementation_sha256":packet["implementation_sha256"],"stages":stages,
        "expressions":{"schema":"zeno-fcis/loop-expressions/1","vocabulary":{"inputs":["x"]},"outputs":[text]}})
}
fn pipeline(original: &Path, proposal: &Path, extra: &[&str]) -> (i32, Value) {
    let mut arguments = args(&[
        "loop",
        "pipeline",
        "--original",
        &original.to_string_lossy(),
        "--proposal",
        &proposal.to_string_lossy(),
    ]);
    arguments.extend(args(extra));
    run(&arguments)
}
fn confirm(out: &Path, check: &Value) {
    let original = out.join("original.zcve");
    let candidate = out.join("candidate.zcve");
    let receipt = out.join("receipt.json");
    let (exit, report) = run(&args(&[
        "transform",
        "check",
        "--original",
        &original.to_string_lossy(),
        "--candidate",
        &candidate.to_string_lossy(),
        "--receipt",
        &receipt.to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
    assert_eq!(
        json!(sha(&fs::read(&receipt).unwrap_or_else(|e| panic!("{e}")))),
        check["receipt_sha256"]
    );
    let (exit, report) = run(&args(&[
        "transform",
        "replay",
        "--receipt",
        &receipt.to_string_lossy(),
        "--original",
        &original.to_string_lossy(),
        "--candidate",
        &candidate.to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
}

#[test]
fn pipeline_completes_repairs_and_checks_each_program() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    for (name, text, stages) in [
        (
            "complete",
            "x + ?c[0..1]",
            vec!["repair", "complete", "canonical"],
        ),
        ("repair", "x + 1", vec!["repair", "canonical"]),
    ] {
        let request = dir.json(
            &format!("{name}.json"),
            &expressions(&original, text, &stages),
        );
        let out = dir.0.join(name);
        let (exit, report) = pipeline(&original, &request, &["--out", &out.to_string_lossy()]);
        assert_eq!(exit, 0, "{report}");
        assert_eq!(report["status"], "qualified");
        assert_eq!(report["detail"]["attempts_spent"], 0);
        if name == "complete" {
            assert_eq!(report["detail"]["trace"][0]["status"], "guard-skipped");
        }
        confirm(&out, &report["detail"]["best_check"]);
        for row in report["detail"]["candidates"]
            .as_array()
            .unwrap_or_else(|| panic!("candidates"))
        {
            if row["check"]["verdict"] == "equivalent" {
                assert_eq!(row["check"]["replay"], "matched");
            }
        }
    }
}

#[test]
fn pipeline_forged_program_abi_and_stale_bindings_never_qualify() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let wrong = scalar(3, vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)], 2);
    let request = dir.json("wrong.json", &proposal(&original, exported(&wrong), &[]));
    let (exit, report) = pipeline(&original, &request, &[]);
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "refuted");
    let alien = long(4);
    let request = dir.json("abi.json", &proposal(&original, exported(&alien), &[]));
    let (exit, report) = pipeline(&original, &request, &[]);
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "refused");
    let mut stale = proposal(&original, exported(&long(3)), &[]);
    stale["implementation_sha256"] = json!("0".repeat(64));
    let request = dir.json("stale-code.json", &stale);
    assert_eq!(pipeline(&original, &request, &[]).0, 1);
    let request = dir.json(
        "stale-subject.json",
        &proposal(&original, exported(&long(3)), &[]),
    );
    fs::write(&original, bytes(&long(4))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(pipeline(&original, &request, &[]).0, 1);
}

#[test]
fn pipeline_late_budget_stop_retains_evidence_but_exports_no_candidate() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let request = dir.json(
        "request.json",
        &proposal(&original, exported(&long(3)), &["canonical"]),
    );
    let out = dir.0.join("stopped");
    let (exit, report) = pipeline(
        &original,
        &request,
        &["--max-gate-pairs", "8", "--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(
        report["detail"]["candidates"][0]["check"]["verdict"],
        "equivalent"
    );
    assert_eq!(report["detail"]["attempts_spent"], 0);
    assert!(!out.join("candidate.zcve").exists());
    assert!(out.join("result.json").exists());
    let (exit, report) = pipeline(&original, &request, &["--max-tuples", "1"]);
    assert_eq!(exit, 2, "{report}");
}

#[test]
fn pipeline_pruning_dead_eager_overflow_is_refuted_and_all_failure_is_inconclusive() {
    let dir = Directory::new();
    let p = Program::try_new(
        vec![Domain::Int { min: -1, max: 1 }],
        vec![Domain::Int {
            min: i128::MIN,
            max: i128::MAX,
        }],
        vec![Op::Input(0), Op::Int(i128::MAX), Op::Add(0, 1), Op::Int(0)],
        vec![3],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let original = dir.file("original.zcve", &bytes(&p));
    let request = dir.json(
        "request.json",
        &proposal(&original, exported(&p), &["canonical"]),
    );
    let out = dir.0.join("out");
    let (exit, report) = pipeline(&original, &request, &["--out", &out.to_string_lossy()]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(
        report["detail"]["candidates"][1]["check"]["verdict"],
        "counterexample"
    );
    assert_eq!(report["detail"]["candidates"][1]["check"]["ordinal"], 2);
    confirm(&out, &report["detail"]["best_check"]);
    let trap = scalar(0, vec![Op::Int(i128::MAX), Op::Int(1), Op::Add(0, 1)], 2);
    let original = dir.file("trap.zcve", &bytes(&trap));
    let request = dir.json("trap.json", &proposal(&original, exported(&trap), &[]));
    let (exit, report) = pipeline(&original, &request, &[]);
    assert_eq!(exit, 2, "{report}");
    assert!(report["detail"]["best"].is_null());
}

#[test]
fn strategy_freezes_on_development_and_refuses_content_overlap() {
    let dir = Directory::new();
    let dev = dir.file("dev.zcve", &bytes(&long(1)));
    let eval = dir.file("eval.zcve", &bytes(&long(3)));
    let dev_task = json!({"original":"dev.zcve","proposal":proposal(&dev,exported(&long(1)),&[])});
    let eval_task =
        json!({"original":"eval.zcve","proposal":proposal(&eval,exported(&long(3)),&[])});
    let mut tasks = json!({"schema":"zeno-fcis/loop-strategy/1","development":[dev_task],"evaluation":[eval_task]});
    let path = dir.json("tasks.json", &tasks);
    let arguments = args(&["loop", "strategy", "--tasks", &path.to_string_lossy()]);
    let (exit, a) = run(&arguments);
    assert_eq!(exit, 0, "{a}");
    assert_eq!(a["detail"]["policies_generated"], 5);
    assert_eq!(a["detail"]["selection_frozen_before_evaluation"], true);
    assert_eq!(a["detail"]["evaluation_influenced_selection"], false);
    let wrong = scalar(3, vec![Op::Int(0)], 0);
    tasks["evaluation"][0]["proposal"] = proposal(&eval, exported(&wrong), &[]);
    dir.json("tasks.json", &tasks);
    let (exit, b) = run(&arguments);
    assert_eq!(exit, 0, "{b}");
    assert_eq!(
        a["detail"]["selection_sha256"],
        b["detail"]["selection_sha256"]
    );
    tasks["evaluation"][0] = tasks["development"][0].clone();
    dir.json("tasks.json", &tasks);
    let (exit, r) = run(&arguments);
    assert_eq!(exit, 1, "{r}");
    assert_eq!(
        r["detail"]["reason"],
        "strategy-subject-overlap-or-duplicate"
    );
}

#[test]
fn rules_discovers_concrete_replacements_and_independently_replays_export() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let out = dir.0.join("rules");
    let (exit, report) = run(&args(&[
        "loop",
        "rules",
        "--original",
        &original.to_string_lossy(),
        "--out",
        &out.to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["detail"]["best"]["nodes"], 1);
    assert!(
        !report["detail"]["applied"]
            .as_array()
            .unwrap_or_else(|| panic!("applied"))
            .is_empty()
    );
    confirm(&out, &report["detail"]["best_check"]);
    assert!(out.join("bank.json").exists());
    let (exit, report) = run(&args(&[
        "loop",
        "rules",
        "--original",
        &original.to_string_lossy(),
        "--max-search-work",
        "1",
    ]));
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
}

#[test]
fn rules_a_forged_bank_and_dead_eager_trap_cannot_authorize_a_rewrite() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let wrong = scalar(3, vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)], 2);
    let x = scalar(3, vec![Op::Input(0)], 0);
    let forged = dir.json(
        "forged.json",
        &json!({"schema":"zeno-fcis/loop-rule-bank/1","authority":"all-domains",
        "scope":{"trusted":true},"rules":[{"lhs":exported(&wrong),"rhs":exported(&x)}]}),
    );
    let (exit, report) = run(&args(&[
        "loop",
        "rules",
        "--original",
        &original.to_string_lossy(),
        "--bank",
        &forged.to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["detail"]["rules_requalified"], 0);
    assert_eq!(
        report["detail"]["checks"][0]["check"]["verdict"],
        "counterexample"
    );
    assert_eq!(report["detail"]["best"]["nodes"], 3);
    let source_bank = dir.0.join("source-bank");
    assert_eq!(
        run(&args(&[
            "loop",
            "rules",
            "--original",
            &original.to_string_lossy(),
            "--out",
            &source_bank.to_string_lossy()
        ]))
        .0,
        0
    );
    let trap = scalar(
        3,
        vec![
            Op::Input(0),
            Op::Add(0, 0),
            Op::Sub(1, 0),
            Op::Int(i128::MAX),
            Op::Int(1),
            Op::Add(3, 4),
        ],
        2,
    );
    let target = dir.file("target.zcve", &bytes(&trap));
    let (exit, report) = run(&args(&[
        "loop",
        "rules",
        "--original",
        &target.to_string_lossy(),
        "--bank",
        &source_bank.join("bank.json").to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["detail"]["best"]["nodes"], 6);
    assert!(
        report["detail"]["checks"]
            .as_array()
            .unwrap_or_else(|| panic!("checks"))
            .iter()
            .any(|r| r["stage"] == "whole-program" && r["check"]["verdict"] == "counterexample")
    );
    assert!(
        report["rules_summary"]["refutations"]
            .as_array()
            .unwrap_or_else(|| panic!("summary refutations"))
            .iter()
            .any(|row| row["stage"] == "whole-program" && row["input"].is_array())
    );
}

#[test]
fn rules_source_domain_success_is_not_target_domain_authority() {
    let dir = Directory::new();
    let eq = |high| {
        Program::try_new(
            vec![Domain::Int { min: 0, max: high }],
            vec![Domain::Bool],
            vec![Op::Input(0), Op::Int(0), Op::Eq(0, 1)],
            vec![2],
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    };
    let truth = Program::try_new(
        vec![Domain::Int { min: 0, max: 0 }],
        vec![Domain::Bool],
        vec![Op::Bool(true)],
        vec![0],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let source = dir.file("source.zcve", &bytes(&eq(0)));
    let terms = dir.json(
        "terms.json",
        &json!({"schema":"zeno-fcis/loop-terms/1","terms":[exported(&truth)]}),
    );
    let out = dir.0.join("bank");
    assert_eq!(
        run(&args(&[
            "loop",
            "rules",
            "--original",
            &source.to_string_lossy(),
            "--terms",
            &terms.to_string_lossy(),
            "--out",
            &out.to_string_lossy()
        ]))
        .0,
        0
    );
    let target = dir.file("target.zcve", &bytes(&eq(1)));
    let (exit, report) = run(&args(&[
        "loop",
        "rules",
        "--original",
        &target.to_string_lossy(),
        "--bank",
        &out.join("bank.json").to_string_lossy(),
    ]));
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["detail"]["best"]["nodes"], 3);
    assert!(
        report["detail"]["checks"]
            .as_array()
            .unwrap_or_else(|| panic!("checks"))
            .iter()
            .any(|r| r["check"]["verdict"] == "counterexample" && r["check"]["ordinal"] == 1)
    );
}

#[test]
fn composable_export_preserves_an_existing_directory() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let request = dir.json(
        "request.json",
        &proposal(&original, exported(&long(3)), &[]),
    );
    let out = dir.0.join("owned");
    fs::create_dir(&out).unwrap_or_else(|e| panic!("{e}"));
    fs::write(out.join("sentinel"), b"preserved").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        pipeline(&original, &request, &["--out", &out.to_string_lossy()]).0,
        1
    );
    assert_eq!(
        fs::read(out.join("sentinel")).unwrap_or_else(|e| panic!("{e}")),
        b"preserved"
    );
}

#[test]
fn pipeline_found_repair_is_checked_despite_synthesis_stop() {
    let dir = Directory::new();
    let original = dir.0.join("original.zcve");
    fs::write(&original, bytes(&scalar(3, vec![Op::Input(0)], 0)))
        .unwrap_or_else(|e| panic!("{e}"));
    let request = dir.0.join("proposal.json");
    let proposal = expressions(&original, "x == 0 ? x : x + 1", &["repair"]);
    fs::write(
        &request,
        serde_json::to_vec(&proposal).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let out = dir.0.join("result");
    let (exit, report) = pipeline(
        &original,
        &request,
        &["--max-search-work", "16", "--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "qualified");
    assert_eq!(report["detail"]["best_check"]["verdict"], "equivalent");
    assert_eq!(report["detail"]["best_check"]["replay"], "matched");
    assert_eq!(report["detail"]["attempts_spent"], 0);
    assert!(
        report["detail"]["trace"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["synthesis_stopped"] == true))
    );
    assert!(out.join("candidate.zcve").exists());
    confirm(&out, &report["detail"]["best_check"]);
}

#[test]
fn strategy_development_check_budget_stop_selects_nothing() {
    let dir = Directory::new();
    let dev = dir.file("dev.zcve", &bytes(&long(3)));
    let eval = dir.file("eval.zcve", &bytes(&long(0)));
    let tasks = dir.json("tasks.json", &json!({"schema":"zeno-fcis/loop-strategy/1",
        "development":[{"original":"dev.zcve","proposal":proposal(&dev, exported(&long(3)), &[])}],
        "evaluation":[{"original":"eval.zcve","proposal":proposal(&eval, exported(&long(0)), &[])}]}));
    let out = dir.0.join("result");
    let (exit, report) = run(&args(&[
        "loop",
        "strategy",
        "--tasks",
        &tasks.to_string_lossy(),
        "--max-gate-pairs",
        "44",
        "--out",
        &out.to_string_lossy(),
    ]));
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert!(report["detail"]["selection"].is_null());
    assert_eq!(report["detail"]["evaluation"], json!([]));
    assert_eq!(report["detail"]["attempts_spent"], 0);
    assert!(!out.join("selection.json").exists());
    assert!(!out.join("candidate.zcve").exists());
    let last = report["detail"]["trials"]
        .as_array()
        .and_then(|trials| trials.last())
        .unwrap_or_else(|| panic!("development trials"));
    assert_eq!(last["runs"][0]["reason"], "gate-budget");
}

#[test]
fn pipeline_first_gate_stop_preserves_its_budget_reason() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let request = dir.json(
        "proposal.json",
        &proposal(&original, exported(&long(3)), &[]),
    );
    let (exit, report) = pipeline(&original, &request, &["--max-gate-pairs", "0"]);
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["detail"]["reason"], "gate-budget");
    assert_eq!(report["detail"]["attempts_spent"], 0);
}

#[test]
fn strategy_runtime_depth_and_task_restrictions_refuse_before_search() {
    let dir = Directory::new();
    let dev = dir.file("dev.zcve", &bytes(&long(3)));
    let eval = dir.file("eval.zcve", &bytes(&long(0)));
    let mut content = json!({"schema":"zeno-fcis/loop-strategy/1",
        "development":[{"original":"dev.zcve","proposal":proposal(&dev, exported(&long(3)), &[])}],
        "evaluation":[{"original":"eval.zcve","proposal":proposal(&eval, exported(&long(0)), &[])}]});
    let tasks = dir.json("tasks.json", &content);
    for depth in ["2", "3"] {
        let (exit, report) = run(&args(&[
            "loop",
            "strategy",
            "--tasks",
            &tasks.to_string_lossy(),
            "--depth",
            depth,
        ]));
        assert_eq!(exit, 1, "{report}");
        assert_eq!(report["status"], "refused");
    }
    let extra = dir.file("extra.zcve", &bytes(&long(1)));
    content["development"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("tasks"))
        .push(
            json!({"original":"extra.zcve","proposal":proposal(&extra, exported(&long(1)), &[])}),
        );
    let tasks = dir.json("extra-tasks.json", &content);
    let (exit, report) = run(&args(&[
        "loop",
        "strategy",
        "--tasks",
        &tasks.to_string_lossy(),
    ]));
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "refused");
}

#[test]
fn pipeline_refused_completion_is_refused_not_inconclusive() {
    let dir = Directory::new();
    let original = dir.file("original.zcve", &bytes(&long(3)));
    let request = dir.json(
        "proposal.json",
        &expressions(&original, "?e + ?e", &["complete"]),
    );
    let (exit, report) = pipeline(&original, &request, &[]);
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "refused");
    assert!(
        report["detail"]["candidates"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
}

#[test]
fn rules_unchanged_original_is_labelled_without_an_improvement_claim() {
    let dir = Directory::new();
    for (name, program) in [
        ("successful", scalar(3, vec![Op::Input(0)], 0)),
        (
            "trapping",
            scalar(0, vec![Op::Int(i128::MAX), Op::Int(1), Op::Add(0, 1)], 2),
        ),
    ] {
        let original = dir.file(&format!("{name}.zcve"), &bytes(&program));
        let out = dir.0.join(name);
        let (exit, report) = run(&args(&[
            "loop",
            "rules",
            "--original",
            &original.to_string_lossy(),
            "--out",
            &out.to_string_lossy(),
        ]));
        assert_eq!(exit, 0, "{report}");
        assert_eq!(report["detail"]["improved"], false);
        assert_eq!(report["detail"]["result_kind"], "unchanged-original");
        assert_eq!(
            report["detail"]["claim"],
            "original-equivalence-only; no improving rewrite checked"
        );
        confirm(&out, &report["detail"]["best_check"]);
    }
}

// BEGIN typed strategy modes: same command and existing exact program gate.
fn mode(
    dir: &Directory,
    kind: &str,
    problem: &Value,
    original: Option<&Path>,
    extra: &[&str],
) -> (i32, Value) {
    let input = dir.json("mode.json", problem);
    let mut argv = args(&[
        "loop",
        "strategy",
        "--kind",
        kind,
        "--problem",
        &input.to_string_lossy(),
    ]);
    if let Some(original) = original {
        argv.extend(args(&["--original", &original.to_string_lossy()]));
    }
    argv.extend(args(extra));
    run(&argv)
}
fn case_problem(original: &Path, initial: usize) -> Value {
    let p = scalar(
        3,
        vec![
            Op::Input(0),
            Op::Int(2),
            Op::Lt(0, 1),
            Op::Int(7),
            Op::Select(2, 3, 0),
        ],
        4,
    );
    let pred = Program::try_new(
        p.inputs().to_vec(),
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Int(2), Op::Lt(0, 1)],
        vec![2],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    json!({"schema":"zeno-fcis/loop-strategy-problem/1","learn":{"schema":"zeno-fcis/learn-terms/1","original_sha256":sha(&fs::read(original).unwrap_or_else(|e|panic!("{e}"))),"status":"complete","integers":[exported(&scalar(3,vec![Op::Int(7)],0)),exported(&scalar(3,vec![Op::Input(0)],0))],"predicates":[exported(&pred)]},"config":{"initial":initial,"spot_checks":1}})
}
fn producer_problem(
    original: &Path,
    reference: &Program,
    initial: usize,
    limits: LearnLimits,
) -> Value {
    let spec = Specification {
        names: vec!["x".into()],
        inputs: reference.inputs().to_vec(),
        outputs: reference.outputs().to_vec(),
        constants: vec![0, 2, 7, 17],
        operations: vec![Operation::Lt, Operation::Eq],
        max_size: 3,
        examples: vec![],
    };
    let binding = sha(&fs::read(original).unwrap_or_else(|e| panic!("{e}")));
    let mut producer_budget = LearnBudget::new(limits);
    let corpus = match learn::strategy_terms_v1(&spec, &mut producer_budget) {
        Ok(terms) => {
            let owned: zeno_fcis_loop_doors::strategy_split::LearnTermsV1 = terms.into();
            assert!(owned.complete);
            json!({"schema":"zeno-fcis/learn-terms/1","original_sha256":binding,"status":"complete",
                "integers":owned.integers.iter().map(exported).collect::<Vec<_>>(),
                "predicates":owned.predicates.iter().map(exported).collect::<Vec<_>>()})
        }
        Err(learn::Stop::Inconclusive(_)) => {
            json!({"schema":"zeno-fcis/learn-terms/1","original_sha256":binding,"status":"inconclusive"})
        }
        Err(other) => panic!("producer refused: {other:?}"),
    };
    json!({"schema":"zeno-fcis/loop-strategy-problem/1","learn":corpus,"config":{"initial":initial,"spot_checks":1}})
}
fn diagnosis_problem() -> Value {
    json!({"schema":"zeno-fcis/loop-strategy-problem/1","circuit":{"inputs":1,"gates":[{"kind":"not","wires":[0]},{"kind":"not","wires":[1]}],"outputs":[2]},"vector":[0],"observations":[[2,1]],"max_faults":2})
}
fn reading_problem() -> Value {
    let formula = |nodes, root| {
        Program::try_new(vec![Domain::Bool], vec![Domain::Bool], nodes, vec![root])
            .unwrap_or_else(|e| panic!("{e:?}"))
    };
    let yes = formula(vec![Op::Bool(true)], 0);
    let same = formula(vec![Op::Input(0), Op::Eq(0, 0)], 1);
    let x = formula(vec![Op::Input(0)], 0);
    json!({"schema":"zeno-fcis/loop-strategy-problem/1","background":exported(&yes),"query":"count","readings":[{"id":"s1","text":"the statement","alternatives":[exported(&yes),exported(&same),exported(&x)]}]})
}
#[test]
fn strategy_profiles_real_learn_producer_is_checked_and_replayed_including_a_rare_exception() {
    let dir = Directory::new();
    for (label, high, pivot, equality, initial) in
        [("piecewise", 3, 2, false, 2), ("rare", 31, 17, true, 1)]
    {
        let reference = scalar(
            high,
            vec![
                Op::Input(0),
                Op::Int(pivot),
                if equality { Op::Eq(0, 1) } else { Op::Lt(0, 1) },
                Op::Int(7),
                if equality { Op::Int(0) } else { Op::Input(0) },
                Op::Select(2, 3, 4),
            ],
            5,
        );
        let original = dir.file(&format!("{label}.zcve"), &bytes(&reference));
        let out = dir.0.join(label);
        let (exit, report) = mode(
            &dir,
            "split-by-cases",
            &producer_problem(&original, &reference, initial, LearnLimits::default()),
            Some(&original),
            &["--out", &out.to_string_lossy()],
        );
        assert_eq!(exit, 0, "{report}");
        assert_eq!(report["status"], "qualified");
        assert_eq!(report["detail"]["best_check"]["verdict"], "equivalent");
        assert_eq!(report["detail"]["best_check"]["replay"], "matched");
        assert_eq!(report["detail"]["plan_check"]["type_checked"], true);
        assert_eq!(report["detail"]["plan_check"]["trace_conforms"], true);
        // BEGIN L2: deleting capture must fail even if the old gate still accepts.
        assert_eq!(report["detail"]["capture"]["status"], "captured");
        assert_eq!(report["detail"]["capture"]["check"]["replay"], "matched");
        let trace = report["detail"]["trace"]
            .as_array()
            .unwrap_or_else(|| panic!("trace"));
        assert_eq!(
            &trace[trace.len() - 2..],
            &[json!("capture"), json!("check-program")]
        );
        assert!(out.join("capture-reference.zcve").is_file());
        let captured = out.join("capture-reference.zcve");
        let receipt = out.join("capture.receipt.json");
        let candidate = out.join("candidate.zcve");
        let (exit, checked) = run(&args(&[
            "transform",
            "check",
            "--original",
            &captured.to_string_lossy(),
            "--candidate",
            &candidate.to_string_lossy(),
            "--receipt",
            &receipt.to_string_lossy(),
        ]));
        assert_eq!(exit, 0, "{checked}");
        let (exit, replay) = run(&args(&[
            "transform",
            "replay",
            "--original",
            &captured.to_string_lossy(),
            "--candidate",
            &candidate.to_string_lossy(),
            "--receipt",
            &receipt.to_string_lossy(),
        ]));
        assert_eq!(exit, 0, "{replay}");
        // END L2.
        if equality {
            assert!(
                report["detail"]["queried"]
                    .as_array()
                    .is_some_and(|q| q.contains(&json!(17))),
                "{report}"
            );
        }
        confirm(&out, &report["detail"]["best_check"]);
    }
}
#[test]
fn strategy_profiles_sample_fit_cannot_pass_the_full_domain_checker() {
    let dir = Directory::new();
    let original = dir.file(
        "case.zcve",
        &bytes(&scalar(
            3,
            vec![
                Op::Input(0),
                Op::Int(2),
                Op::Lt(0, 1),
                Op::Int(7),
                Op::Select(2, 3, 0),
            ],
            4,
        )),
    );
    let out = dir.0.join("refuted");
    let (exit, report) = mode(
        &dir,
        "split-by-cases",
        &case_problem(&original, 1),
        Some(&original),
        &["--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 1, "{report}");
    assert_eq!(report["status"], "refuted");
    assert_eq!(report["detail"]["best_check"]["verdict"], "counterexample");
    assert!(!out.join("candidate.zcve").exists());
}
#[test]
fn strategy_profiles_incomplete_corpus_stops_before_decoding_partial_lists() {
    let dir = Directory::new();
    let original = dir.file("case.zcve", &bytes(&long(3)));
    let mut problem = producer_problem(
        &original,
        &long(3),
        2,
        LearnLimits {
            work: 0,
            ..LearnLimits::default()
        },
    );
    assert_eq!(problem["learn"]["status"], "inconclusive");
    problem["learn"]["integers"] = json!("partial lists must not be decoded");
    problem["learn"]["predicates"] = json!(false);
    let out = dir.0.join("stopped");
    let (exit, report) = mode(
        &dir,
        "split-by-cases",
        &problem,
        Some(&original),
        &["--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(report["detail"]["reason"], "strategy-learn-incomplete");
    assert_eq!(report["detail"]["attempts_spent"], 0);
    assert!(!out.join("candidate.zcve").exists());
    problem["learn"]["status"] = json!("complete");
    let (exit, report) = mode(&dir, "split-by-cases", &problem, Some(&original), &[]);
    assert_eq!(exit, 1, "{report}");
}
#[test]
fn strategy_profiles_corpus_binding_and_budgets_cannot_qualify() {
    let dir = Directory::new();
    let original = dir.file("case.zcve", &bytes(&long(3)));
    let mut problem = case_problem(&original, 2);
    problem["learn"]["original_sha256"] = json!("0".repeat(64));
    let (exit, report) = mode(&dir, "split-by-cases", &problem, Some(&original), &[]);
    assert_eq!(exit, 1, "{report}");
    problem = case_problem(&original, 2);
    let out = dir.0.join("no-work");
    let (exit, report) = mode(
        &dir,
        "split-by-cases",
        &problem,
        Some(&original),
        &["--max-search-work", "0", "--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert!(
        report["detail"]["proposer_work"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    assert!(!out.join("candidate.zcve").exists());
}
#[test]
fn strategy_profiles_diagnose_conditions_minimal_proposals_on_measurements() {
    let dir = Directory::new();
    let mut problem = diagnosis_problem();
    let (exit, report) = mode(&dir, "diagnose", &problem, None, &[]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "question");
    assert_eq!(
        report["detail"]["question"],
        json!({"wire":1,"zero":1,"one":1})
    );
    assert_eq!(
        report["detail"]["diagnoses"].as_array().map(Vec::len),
        Some(2)
    );
    problem["observations"] = json!([[2, 1], [1, 0]]);
    let (exit, report) = mode(&dir, "diagnose", &problem, None, &[]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "proposed-diagnosis");
    assert_eq!(report["detail"]["diagnoses"][0]["gates"], json!([0]));
    assert_eq!(report["detail"]["authority"], "none");
    let (exit, report) = mode(&dir, "diagnose", &problem, None, &["--max-cells", "0"]);
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(report["detail"]["attempts_spent"], 0);
}
#[test]
fn strategy_profiles_readings_ask_only_for_answer_changes() {
    let dir = Directory::new();
    let mut problem = reading_problem();
    let (exit, report) = mode(&dir, "ask-when-it-matters", &problem, None, &[]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "question");
    assert_eq!(report["detail"]["distinct"], json!([[0, 2]]));
    assert_eq!(report["detail"]["question"]["holds"], json!([0]));
    assert_eq!(report["detail"]["question"]["does_not_hold"], json!([2]));
    let x = problem["readings"][0]["alternatives"][2].clone();
    let mut notx = x.clone();
    notx["nodes"] = json!([["Input", 0], ["Not", 0]]);
    notx["roots"] = json!([1]);
    problem["readings"][0]["alternatives"] = json!([x, notx]);
    let (exit, report) = mode(&dir, "ask-when-it-matters", &problem, None, &[]);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "proposed-answer");
    assert_eq!(report["detail"]["answer"], 1);
    assert!(report["detail"]["question"].is_null());
    assert_eq!(report["detail"]["plan_check"]["trace_conforms"], true);
}
#[test]
fn strategy_profiles_owner_and_primitive_fields_are_refused_and_stops_keep_no_answer() {
    let dir = Directory::new();
    for key in ["owner_answers", "owner_confirmations", "primitives"] {
        let mut problem = reading_problem();
        problem[key] = json!([]);
        let (exit, report) = mode(&dir, "ask-when-it-matters", &problem, None, &[]);
        assert_eq!(exit, 1, "{report}");
        assert_eq!(report["status"], "refused");
    }
    let out = dir.0.join("reading-stop");
    let (exit, report) = mode(
        &dir,
        "ask-when-it-matters",
        &reading_problem(),
        None,
        &["--max-search-work", "0", "--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert!(report["detail"]["answer"].is_null());
    assert!(report["detail"]["question"].is_null());
    assert!(!out.join("candidate.zcve").exists());
}
#[test]
fn strategy_profiles_io_failures_are_not_successful_questions() {
    let dir = Directory::new();
    let out = dir.0.join("missing-parent").join("report");
    let (exit, report) = mode(
        &dir,
        "diagnose",
        &diagnosis_problem(),
        None,
        &["--out", &out.to_string_lossy()],
    );
    assert_eq!(exit, 3, "{report}");
    assert_ne!(report["status"], "question");
    let missing = dir.0.join("missing.zcve");
    let (exit, report) = mode(
        &dir,
        "split-by-cases",
        &json!({"schema":"zeno-fcis/loop-strategy-problem/1","learn":{}}),
        Some(&missing),
        &[],
    );
    assert_eq!(exit, 3, "{report}");
    assert_ne!(report["status"], "qualified");
}
// END typed strategy modes.

#[test]
fn rules_summary_exposes_refutations_without_changing_qualification() {
    let dir = Directory::new();
    let original_bytes = bytes(&long(3));
    let original = dir.file("original.zcve", &original_bytes);
    let wrong = scalar(3, vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)], 2);
    let x = scalar(3, vec![Op::Input(0)], 0);
    let bank = dir.json(
        "bank.json",
        &json!({"schema":"zeno-fcis/loop-rule-bank/1",
        "rules":[{"lhs":exported(&wrong),"rhs":exported(&x)}]}),
    );
    let out = dir.0.join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args([
            "loop",
            "rules",
            "--original",
            &original.to_string_lossy(),
            "--bank",
            &bank.to_string_lossy(),
            "--out",
            &out.to_string_lossy(),
        ])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    assert_eq!(output.status.code(), Some(0), "{report}");
    assert_eq!(report["status"], "qualified");
    assert_eq!(report["detail"]["result_kind"], "unchanged-original");
    assert_eq!(report["detail"]["improved"], false);
    assert_eq!(report["detail"]["rules_requalified"], 0);
    assert_eq!(report["detail"]["baseline_check"]["verdict"], "equivalent");
    assert_eq!(report["detail"]["baseline_check"]["replay"], "matched");
    let summary = &report["rules_summary"];
    for (field, value) in [
        ("considered", 1),
        ("qualified", 0),
        ("refuted", 1),
        ("refused", 0),
        ("inconclusive", 0),
        ("unexamined", 0),
    ] {
        assert_eq!(summary[field], value, "top-level {field}: {report}");
    }
    let witness = &summary["refutations"][0];
    assert_eq!(witness["rule"], 0);
    assert_eq!(witness["stage"], "target-rule");
    assert_eq!(witness["ordinal"], 0);
    assert_eq!(witness["input"], json!(["0"]));
    assert_eq!(witness["original"]["ok"], json!(["1"]));
    assert_eq!(witness["candidate"]["ok"], json!(["0"]));
    assert_eq!(
        witness["input"],
        report["detail"]["checks"][0]["check"]["input"]
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .any(|line| line == "rules: rule 0 refuted at target-rule input=[\"0\"]"),
        "{output:?}"
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(out.join("result.json")).unwrap_or_default())
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(saved["rules_summary"], *summary);
    assert_eq!(
        fs::read(out.join("candidate.zcve")).unwrap_or_default(),
        original_bytes
    );

    // Refusals and unexamined work must not become zero-counterexample claims.
    let bank = dir.json(
        "mixed.json",
        &json!({"schema":"zeno-fcis/loop-rule-bank/1",
        "rules":[{"lhs":exported(&x),"rhs":exported(&long(3))},
                  {"lhs":exported(&wrong),"rhs":exported(&x)}]}),
    );
    let arguments = args(&[
        "loop",
        "rules",
        "--original",
        &original.to_string_lossy(),
        "--bank",
        &bank.to_string_lossy(),
    ]);
    let (exit, report) = run(&arguments);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["rules_summary"]["considered"], 2);
    assert_eq!(report["rules_summary"]["refuted"], 1);
    assert_eq!(report["rules_summary"]["refused"], 1);
    assert_eq!(
        report["rules_summary"]["refusals"][0]["code"],
        "nondecreasing-rule"
    );
    let mut stopped = arguments;
    stopped.extend(args(&["--max-checks", "0"]));
    let (exit, report) = run(&stopped);
    assert_eq!(exit, 2, "{report}");
    assert_eq!(report["rules_summary"]["considered"], 2);
    assert_eq!(report["rules_summary"]["unexamined"], 2);
    assert_eq!(report["rules_summary"]["refuted"], 0);
    assert_eq!(report["rules_summary"]["qualified"], 0);
}
