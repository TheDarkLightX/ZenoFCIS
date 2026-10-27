//! Reproducibility checks for actual, separately reviewed F51 evidence.
//! Recorded commands are inspected, never executed as supplied instructions.

use super::{common::*, files};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use zeno_fcis_contract::refinement::{self, Sources};

pub(super) struct QualifiedF51 {
    pub(super) record: Value,
    pub(super) input: Value,
    pub(super) design: Vec<(String, Vec<u8>)>,
}

pub(super) fn load(evidence: &Path, file: &Path, out: &Path) -> Result<QualifiedF51> {
    let bytes = read(file, DOCUMENT_LIMIT)?;
    let record = strict(&bytes)?;
    closed(
        &record,
        &[
            "schema",
            "status",
            "checker_identity_sha256",
            "evaluator_identity_sha256",
            "cli_sha256",
            "source_manifest_sha256",
            "checks",
            "review",
        ],
    )?;
    if record["schema"] != "zeno-fcis/refinement-qualification/1" || record["status"] != "qualified"
    {
        return fail(65, "f51-not-qualified");
    }
    let review = &record["review"];
    closed(review, &["path", "sha256", "reviewer", "status"])?;
    if review["status"] != "accepted" || review["reviewer"].as_str().is_none_or(str::is_empty) {
        return fail(65, "f51-review-not-accepted");
    }
    check_ref(
        evidence,
        &json!({"path":review["path"],"sha256":review["sha256"]}),
        DOCUMENT_LIMIT,
    )?;
    let checks = record["checks"]
        .as_array()
        .ok_or_else(|| malformed("f51-checks"))?;
    let mut gates = BTreeSet::new();
    for check in checks {
        closed(
            check,
            &["gate", "id", "command", "exit", "log", "stderr", "counts"],
        )?;
        let gate_name = check["gate"]
            .as_str()
            .ok_or_else(|| malformed("f51-gate-name"))?;
        gates.insert(gate_name);
        let expected = if gate_name == "F07" { 2 } else { 0 };
        if number(&check["exit"], expected, expected)? != expected {
            return fail(65, "f51-gate-exit");
        }
        check_ref(evidence, &check["log"], LOG_LIMIT)?;
        check_ref(evidence, &check["stderr"], LOG_LIMIT)?;
        if !check["command"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
        {
            return fail(65, "f51-command-vector");
        }
    }
    for gate in 1..=15 {
        if !gates.contains(format!("F{gate:02}").as_str()) {
            return fail(65, "f51-missing-required-gate");
        }
    }
    let source = files::source_inventory()?;
    let source_path = file
        .parent()
        .ok_or_else(|| malformed("f51-parent"))?
        .join("source.sha256");
    if read(&source_path, LOG_LIMIT)? != source
        || record["source_manifest_sha256"].as_str() != Some(sha(&source).as_str())
    {
        return fail(65, "f51-stale-source-manifest");
    }
    let root = files::repository()?;
    let fixture = root.join("examples/contract-refinement/reservation");
    let strings = [
        fixture.join("abstract/project.zeno"),
        fixture.join("abstract/v2/policy.json"),
        fixture.join("concrete/project.zeno"),
        fixture.join("concrete/v2/policy.json"),
        fixture.join("abstraction.json"),
    ]
    .iter()
    .map(|p| String::from_utf8(read(p, SOURCE_LIMIT)?).map_err(|e| malformed(e.to_string())))
    .collect::<Result<Vec<_>>>()?;
    let baseline = refinement::check(
        Sources {
            abstract_project: &strings[0],
            abstract_policy: &strings[1],
            concrete_project: &strings[2],
            concrete_policy: &strings[3],
            abstraction: &strings[4],
        },
        384,
    )?
    .json();
    write_json(&out.join("f51-current.json"), &baseline)?;
    if baseline["verdict"] != "refines"
        || baseline["checked_rows"] != 384
        || baseline["authority_evaluations"] != 542
        || baseline["checker"]["source_digest"] != record["checker_identity_sha256"]
        || baseline["checker"]["sources"]["evaluator"] != record["evaluator_identity_sha256"]
    {
        return fail(65, "f51-current-native-identity-or-result");
    }
    let post_review = checks
        .iter()
        .rev()
        .find(|c| c["gate"] == "F15" && c["counts"]["checked_rows"] == 384)
        .ok_or_else(|| malformed("f51-post-review-baseline-missing"))?;
    let expected = strict(&check_ref(evidence, &post_review["log"], LOG_LIMIT)?)?;
    if expected != baseline {
        return fail(65, "f51-post-review-baseline-differs");
    }
    let a05 = checks
        .iter()
        .rev()
        .find(|c| c["gate"] == "A05")
        .ok_or_else(|| malformed("f51-design-inventory-missing"))?;
    let args = a05["command"]
        .as_array()
        .ok_or_else(|| malformed("design-command"))?;
    if args.first().and_then(Value::as_str) != Some("sha256sum") {
        return fail(65, "design-command-shape");
    }
    let log = String::from_utf8(check_ref(evidence, &a05["log"], LOG_LIMIT)?)
        .map_err(|e| malformed(e.to_string()))?;
    for arg in args.iter().skip(1) {
        let path = Path::new(arg.as_str().ok_or_else(|| malformed("design-argument"))?);
        let hash = sha(&read(path, SOURCE_LIMIT)?);
        let line = format!("{hash}  {}", path.display());
        if !log.lines().any(|v| v == line) {
            return fail(65, "design-source-pin");
        }
    }
    let design_root = PathBuf::from(
        args.get(1)
            .and_then(Value::as_str)
            .ok_or_else(|| malformed("design-spec-path"))?,
    )
    .parent()
    .ok_or_else(|| malformed("design-root"))?
    .to_path_buf();
    let mut design = Vec::new();
    for name in [
        "SPEC.md",
        "CHECKLIST.md",
        "DESIGN_R2.md",
        "DESIGN_R1.md",
        "LEAD_REVIEW_R1.md",
        "LITERATURE.md",
        "USER_STORY.md",
    ] {
        design.push((name.into(), read(&design_root.join(name), SOURCE_LIMIT)?));
    }
    let input = files::reference(evidence, file)?;
    Ok(QualifiedF51 {
        record,
        input,
        design,
    })
}

pub(super) fn approval(evidence: &Path, file: &Path, bundle: &Path) -> Result<Value> {
    let value = strict(&read(file, DOCUMENT_LIMIT)?)?;
    closed(
        &value,
        &[
            "schema",
            "revision",
            "bundle_sha256",
            "status",
            "approver",
            "decision_record",
        ],
    )?;
    if value["schema"] != "zeno-fcis/priority-meaning-approval/1"
        || value["revision"] != REVISION
        || value["status"] != "approved"
        || value["approver"] != "owner (recorded 2026-10-16T13:35:19Z)"
        || value["bundle_sha256"].as_str()
            != Some(sha(&read(&bundle.join("manifest.json"), LOG_LIMIT)?).as_str())
    {
        return fail(65, "missing-or-stale-meaning-approval");
    }
    let decision = check_ref(evidence, &value["decision_record"], DOCUMENT_LIMIT)?;
    if decision != read(&bundle.join("design/LEAD_REVIEW_R1.md"), SOURCE_LIMIT)? {
        return fail(65, "meaning-decision-record-differs");
    }
    Ok(value)
}
