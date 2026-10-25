//! Native private-boundary and automatic-session tests; no private fixture input.
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "fcis-usage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("{e}"));
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .env_remove("ZENO_FCIS_KB")
        .output()
        .unwrap_or_else(|e| panic!("{e}"))
}
fn json(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"))
}
#[test]
fn kb_usage_public_advice_has_exact_answers_and_native_recipe_traces() {
    for (task, expected) in [
        ("task(subject,check-program).", vec!["check-inputs"]),
        (
            "task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).",
            vec!["containment", "diagnostic-order"],
        ),
        (
            "task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4097).",
            vec!["learner-bound"],
        ),
    ] {
        let out = cli(&["kb", "advise", "--task", task]);
        assert_eq!(out.status.code(), Some(0));
        let r = json(&out);
        assert_eq!(r["lessons"], serde_json::json!(expected));
        let d = Directory::new();
        let report = d.0.join("report.json");
        fs::write(&report, serde_json::to_vec(&r).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap_or_else(|| panic!("root"));
        let replay = Command::new("python3")
            .arg("-B")
            .arg(root.join("tools/check_usage_trace.py"))
            .args(["--report", report.to_str().unwrap_or(""), "--task", task])
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            replay.status.success(),
            "{}",
            String::from_utf8_lossy(&replay.stderr)
        );
        assert!(r["explanations"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.iter()
                    .all(|e| e["trace"].as_array().is_some_and(|t| !t.is_empty()))
        }));
        assert!(r["work"].as_u64().is_some_and(|n| n <= 2_000_000));
    }
}
#[test]
#[cfg(target_os = "linux")]
fn kb_usage_configured_session_skips_blocked_step_writes_one_proposal_and_no_fact_logs() {
    let d = Directory::new();
    let source = d.0.join("team.kb");
    let canary = "synthetic-private-canary-9a83";
    fs::write(&source, format!("custom(\"{canary}\").")).unwrap_or_else(|e| panic!("{e}"));
    let original = d.0.join("not-acquired.zcve");
    let session = d.0.join("not-opened");
    let out = cli(&[
        "loop",
        "--kb",
        source.to_str().unwrap_or(""),
        "--kb-task",
        "task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).",
        "open",
        "--original",
        original.to_str().unwrap_or(""),
        "--session",
        session.to_str().unwrap_or(""),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json(&out)["reason"], "knowledge-step-excluded");
    assert!(!session.exists());
    assert!(!String::from_utf8_lossy(&out.stdout).contains(canary));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(canary));
    let files = fs::read_dir(d.0.join("inbox"))
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|e| e.unwrap_or_else(|e| panic!("{e}")).path())
        .collect::<Vec<_>>();
    let proposals = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "kb"))
        .collect::<Vec<_>>();
    assert_eq!(proposals.len(), 1);
    let proposal = fs::read_to_string(proposals[0]).unwrap_or_else(|e| panic!("{e}"));
    assert!(proposal.contains(",proposed,usage-v1,"));
    assert!(!proposal.contains(canary));
    assert_eq!(
        fs::read_to_string(&source).unwrap_or_default(),
        format!("custom(\"{canary}\").")
    );
}

#[test]
#[cfg(target_os = "macos")]
fn kb_usage_unqualified_loop_refuses_before_acquisition_or_knowledge_writes() {
    let d = Directory::new();
    let source = d.0.join("team.kb");
    let canary = "synthetic-private-canary-9a83";
    let original_source = format!("custom(\"{canary}\").");
    fs::write(&source, &original_source).unwrap_or_else(|e| panic!("{e}"));
    let original = d.0.join("not-acquired.zcve");
    let session = d.0.join("not-opened");
    let out = cli(&[
        "loop",
        "--kb",
        source.to_str().unwrap_or(""),
        "--kb-task",
        "task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).",
        "open",
        "--original",
        original.to_str().unwrap_or(""),
        "--session",
        session.to_str().unwrap_or(""),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let report = json(&out);
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(report["authority"], "none");
    assert_eq!(
        report["reason"],
        "containment-unavailable:posix:platform-unqualified"
    );
    assert_eq!(report["containment"]["startup"]["admitted"], false);
    assert_eq!(
        report["containment"]["final"]["application_work_started"],
        false
    );
    assert!(!session.exists());
    assert!(!original.exists());
    assert!(!d.0.join("inbox").exists());
    assert_eq!(
        fs::read_dir(&d.0).unwrap_or_else(|e| panic!("{e}")).count(),
        1
    );
    assert_eq!(
        fs::read_to_string(&source).unwrap_or_default(),
        original_source
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains(canary));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(canary));
}
#[test]
fn kb_usage_layer_conflict_and_empty_evidence_are_not_advice() {
    let d = Directory::new();
    let first = d.0.join("a.kb");
    let second = d.0.join("b.kb");
    let data = "lesson(fixture,sample).strength(fixture,observed).effect(fixture,neutral).evidence(fixture,r,\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\").record(fixture,proposed,usage-v1,\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",observation,fixture).";
    fs::write(&first, data).unwrap_or_else(|e| panic!("{e}"));
    fs::write(&second, data.replace("neutral", "helps")).unwrap_or_else(|e| panic!("{e}"));
    let out = cli(&[
        "kb",
        "check",
        "--kb",
        first.to_str().unwrap_or(""),
        "--kb",
        second.to_str().unwrap_or(""),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let r = json(&out);
    assert_eq!(r["layer_conflicts"], 1);
    assert!(r["overrides"][0]["earlier"].is_array());
    assert!(r["overrides"][0]["later"].is_array());
    fs::write(
        &first,
        "lesson(no-evidence,sample).strength(no-evidence,observed).effect(no-evidence,neutral).",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let out = cli(&["kb", "check", "--kb", first.to_str().unwrap_or("")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        json(&out)["problems"]
            .as_array()
            .is_some_and(|p| p.contains(&serde_json::json!("kb-lesson-without-evidence")))
    );
}
#[test]
fn kb_usage_plan_and_replan_use_observed_artifacts_only() {
    let task =
        "task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).";
    let out = cli(&[
        "kb",
        "plan",
        "--task",
        task,
        "--have",
        "model,observations",
        "--want",
        "answer",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(json(&out)["next_step"], "explain");
    let out = cli(&[
        "kb",
        "replan",
        "--task",
        task,
        "--have",
        "model,observations",
        "--want",
        "answer",
        "--produced",
        "diagnoses",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(json(&out)["next_step"], "isolate");
    let out = cli(&[
        "kb",
        "replan",
        "--task",
        task,
        "--have",
        "model,observations",
        "--want",
        "answer",
    ]);
    assert_eq!(json(&out)["next_step"], "explain");
}
