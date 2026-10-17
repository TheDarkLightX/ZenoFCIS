//! Native strategy knowledge regressions. LLM-generated; execution is deferred.
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_loop_doors::strategy_knowledge::EXAMPLE;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "fcis-knowledge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("{e}"));
        Self(p)
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, text).unwrap_or_else(|e| panic!("{e}"));
        p
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(extra: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["loop", "strategy"])
        .args(extra)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    let value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}; {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code().unwrap_or(-1), value)
}
fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_else(|| panic!("path"))
}
fn reference(
    dir: &Directory,
    operation: &str,
    report: &Value,
    source: Option<&Path>,
    more: &[&str],
) {
    let report_path = dir.file("report.json", &report.to_string());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("root"));
    let mut command = Command::new("python3");
    command
        .arg("-B")
        .arg(root.join("tools/strategy_knowledge_reference.py"))
        .args(["--operation", operation, "--report", s(&report_path)])
        .args(more);
    if let Some(path) = source {
        command.args(["--source", s(path)]);
    }
    let result = command.output().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn strategy_knowledge_public_example_needs_no_external_knowledge_file() {
    let d = Directory::new();
    let (code, report) = run(&["--kb-op", "check"]);
    assert_eq!(code, 0);
    assert_eq!(report["status"], "consistent");
    assert_eq!(
        report["detail"]["binding"]["source"],
        "compiled-public-example"
    );
    assert_eq!(report["detail"]["problems"], json!([]));
    assert_eq!(report["authority"], "none");
    reference(&d, "check", &report, None, &[]);
    let (code, report) = run(&["--kb-op", "query", "--query", "strategy(S)"]);
    assert_eq!(code, 0);
    assert_eq!(report["detail"]["rows"].as_array().map(Vec::len), Some(3));
    reference(&d, "query", &report, None, &["--query", "strategy(S)"]);
}
#[test]
fn strategy_knowledge_all_three_named_plans_use_the_existing_type_surface() {
    let d = Directory::new();
    for (have, want, name) in [
        ("original,vocabulary", "qualified", "split-by-cases"),
        ("model", "answer", "diagnose"),
        ("proposal,model,owner-gate", "answer", "ask-when-it-matters"),
    ] {
        let (code, report) = run(&["--kb-op", "plan", "--have", have, "--want", want]);
        assert_eq!(code, 0);
        assert_eq!(report["status"], "proposed-plan");
        assert_eq!(report["detail"]["selected_strategy"], name);
        assert_eq!(report["detail"]["type_check"], "passed");
        assert!(report["detail"]["best"].is_null());
        assert_eq!(report["detail"]["attempts_spent"], 0);
        reference(&d, "plan", &report, None, &["--have", have, "--want", want]);
    }
}
#[test]
fn strategy_knowledge_external_query_recursion_negation_and_comparison_match_reference() {
    let d = Directory::new();
    let source = d.file("external.kb", "edge(a,b). edge(b,c). edge(c,d). blocked(c). path(X,Y):-edge(X,Y). path(X,Z):-edge(X,Y),path(Y,Z). open(X,Y):-path(X,Y),not blocked(Y). pairs(X,Y):-path(X,_),path(_,Y),X!=Y.");
    for query in [
        "path(X,Y)",
        "open(X,Y)",
        "pairs(X,Y)",
        "path(a,\"d\")",
        "path(a,_)",
    ] {
        let (code, report) = run(&["--kb", s(&source), "--kb-op", "query", "--query", query]);
        assert_eq!(code, 0);
        assert_eq!(report["detail"]["binding"]["source"], "external-user-file");
        reference(&d, "query", &report, Some(&source), &["--query", query]);
    }
}
#[test]
fn strategy_knowledge_planted_signatures_refuse_planning_and_checker_stub_must_fail() {
    let d = Directory::new();
    let bad = d.file(
        "external.kb",
        &EXAMPLE.replace("needs(diagnose,model).", "needs(diagnose,proposal)."),
    );
    let (code, report) = run(&["--kb", s(&bad), "--kb-op", "check"]);
    assert_eq!(code, 1);
    assert_eq!(report["status"], "inconsistent");
    assert!(
        report["detail"]["problems"]
            .as_array()
            .is_some_and(|p| p.contains(&json!("kb-needs-code-mismatch")))
    );
    reference(&d, "check", &report, Some(&bad), &[]);
    let (code, report) = run(&[
        "--kb",
        s(&bad),
        "--kb-op",
        "plan",
        "--have",
        "model",
        "--want",
        "answer",
    ]);
    assert_eq!(code, 1);
    assert_eq!(report["detail"]["reason"], "kb-code-inconsistent");
    assert!(report["detail"]["steps"].is_null());
}
#[test]
fn strategy_knowledge_work_state_depth_deadline_and_no_path_stops_spend_no_attempt() {
    for extra in [
        vec!["--max-search-work", "0"],
        vec!["--kb-max-steps", "1"],
        vec!["--kb-max-states", "1"],
        vec!["--deadline-ms", "0"],
        vec!["--max-cells", "1"],
    ] {
        let mut args = vec![
            "--kb-op",
            "plan",
            "--have",
            "original,vocabulary",
            "--want",
            "qualified",
        ];
        args.extend(extra);
        let (code, report) = run(&args);
        assert_eq!(code, 2);
        assert_eq!(report["status"], "inconclusive");
        assert_eq!(report["detail"]["attempts_spent"], 0);
        assert!(report["detail"]["rows"].is_null());
        assert!(report["detail"]["steps"].is_null());
    }
    let (code, report) = run(&[
        "--kb-op",
        "plan",
        "--have",
        "question",
        "--want",
        "qualified",
    ]);
    assert_eq!(code, 2);
    assert_eq!(report["detail"]["reason"], "kb-bounded-no-plan");
}
#[test]
fn strategy_knowledge_no_hidden_path_or_report_export_and_io_failures_stay_distinct() {
    let inside = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("crates"))
        .join("zeno-fcis-loop-doors/examples/strategy-example.kb");
    let (code, report) = run(&["--kb", s(&inside), "--kb-op", "check"]);
    assert_eq!(code, 1);
    assert_eq!(
        report["detail"]["reason"],
        "kb-path-must-be-outside-checkout"
    );
    let d = Directory::new();
    let missing = d.0.join("missing.kb");
    assert_eq!(run(&["--kb", s(&missing), "--kb-op", "check"]).0, 3);
    let out = d.0.join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["loop", "strategy", "--kb-op", "check", "--out", s(&out)])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!result.status.success());
    assert!(!out.exists());
}
#[test]
fn strategy_knowledge_unsafe_rules_and_trailing_query_input_are_refused() {
    let d = Directory::new();
    for source in [
        "p(X):-not q(X).",
        "p(a). p(a,b).",
        "q(a). p(X):-q(X),not r(X). r(X):-q(X),not p(X).",
    ] {
        let p = d.file("external.kb", source);
        let (code, report) = run(&["--kb", s(&p), "--kb-op", "query", "--query", "p(X)"]);
        assert_eq!(code, 1);
        assert_eq!(report["status"], "refused");
        assert!(report["detail"]["rows"].is_null());
    }
    let (code, report) = run(&["--kb-op", "query", "--query", "strategy(S). stray"]);
    assert_eq!(code, 1);
    assert_eq!(report["detail"]["reason"], "kb-query-trailing-input");
}
