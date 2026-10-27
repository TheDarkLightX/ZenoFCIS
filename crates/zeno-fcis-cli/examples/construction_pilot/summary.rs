//! Source-bound completion conjunction and the concrete independent-review packet.

use super::{
    common::*,
    controls, files,
    layout::Layout,
    native,
    phases::{Bundle, Context},
    qualification, search,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn include(root: &Path, path: &Path, pairs: &mut BTreeMap<String, String>) -> Result<()> {
    let reference = files::reference(root, path)?;
    let name = reference["path"]
        .as_str()
        .ok_or_else(|| malformed("qualification-input-path"))?
        .to_string();
    let hash = reference["sha256"]
        .as_str()
        .ok_or_else(|| malformed("qualification-input-hash"))?
        .to_string();
    if let Some(old) = pairs.insert(name, hash.clone())
        && old != hash
    {
        return fail(65, "qualification-input-changed");
    }
    Ok(())
}
fn include_tree(root: &Path, path: &Path, pairs: &mut BTreeMap<String, String>) -> Result<()> {
    include(root, &path.join("manifest.json"), pairs)?;
    let current = files::manifest(path)?;
    for item in current["files"]
        .as_array()
        .ok_or_else(|| malformed("manifest-files"))?
    {
        let name = relative(
            item[0]
                .as_str()
                .ok_or_else(|| malformed("manifest-file-name"))?,
        )?;
        include(root, &path.join(name), pairs)?;
    }
    Ok(())
}
fn count_sum(counts: &[Value]) -> Result<Value> {
    let mut sum = json!({"authority_evaluations":0,"checked_rows":0,"proposal_work":0,"scalar_evaluations":0,"scalar_steps":"0","transform_evaluations":0,"transform_accounting_complete":true,"authority_usage":["0","0","0","0","0","0","0","0"]});
    for c in counts {
        for key in [
            "authority_evaluations",
            "checked_rows",
            "proposal_work",
            "scalar_evaluations",
            "transform_evaluations",
        ] {
            let old = sum[key].as_u64().ok_or_else(|| malformed("cost-sum"))?;
            let added = match c.get(key) {
                Some(v) => v.as_u64().ok_or_else(|| malformed("cost-kind"))?,
                None => 0,
            };
            sum[key] = json!(
                old.checked_add(added)
                    .ok_or_else(|| malformed("cost-overflow"))?
            );
        }
        let steps = c
            .get("scalar_steps")
            .map(|v| v.as_str().ok_or_else(|| malformed("scalar-cost-kind")))
            .transpose()?
            .unwrap_or("0")
            .parse::<u128>()
            .map_err(|e| malformed(e.to_string()))?;
        let old = sum["scalar_steps"]
            .as_str()
            .ok_or_else(|| malformed("scalar-cost-sum"))?
            .parse::<u128>()
            .map_err(|e| malformed(e.to_string()))?;
        sum["scalar_steps"] = json!((old + steps).to_string());
        for j in 0..8 {
            let used = c["authority_usage"][j]
                .as_str()
                .ok_or_else(|| malformed("usage-cost-kind"))?
                .parse::<u128>()
                .map_err(|e| malformed(e.to_string()))?;
            let old = sum["authority_usage"][j]
                .as_str()
                .ok_or_else(|| malformed("usage-sum"))?
                .parse::<u128>()
                .map_err(|e| malformed(e.to_string()))?;
            sum["authority_usage"][j] = json!((old + used).to_string());
        }
        if c["transform_accounting_complete"] == false {
            sum["transform_accounting_complete"] = json!(false);
        }
    }
    Ok(sum)
}

fn meta(
    context: &mut Context,
    bundle: &Bundle,
    path: &Path,
    log: &Path,
    control_report: &Value,
) -> Result<(Value, Value)> {
    let bytes = context.input(&path.join("meta.json"), DOCUMENT_LIMIT)?;
    let value = strict(&bytes)?;
    closed(
        &value,
        &[
            "schema",
            "revision",
            "source_manifest_sha256",
            "f51_field_controls",
            "positives",
            "controls",
        ],
    )?;
    if value["schema"] != "zeno-fcis/priority-controls-meta/1"
        || value["revision"] != REVISION
        || value["source_manifest_sha256"] != context.identities["source_manifest_sha256"]
    {
        return fail(1, "meta-source-or-schema");
    }
    let field = bundle.f51.record["checks"]
        .as_array()
        .ok_or_else(|| malformed("F51 checks"))?
        .iter()
        .rev()
        .find(|c| c["gate"] == "F02")
        .ok_or_else(|| malformed("F02 required"))?;
    if value["f51_field_controls"] != field["log"] {
        return fail(1, "meta-F51-field-controls");
    }
    check_ref(&context.evidence, &value["f51_field_controls"], LOG_LIMIT)?;
    closed(&value["positives"], &["T", "I"])?;
    let records = value["controls"]
        .as_array()
        .filter(|r| r.len() == 16)
        .ok_or_else(|| malformed("meta-control-count"))?;
    let original = control_report["detail"]["controls"]
        .as_array()
        .filter(|r| r.len() == 16)
        .ok_or_else(|| malformed("original-control-count"))?;
    let mut costs = Vec::new();
    for (index, record) in records.iter().enumerate() {
        closed(
            record,
            &[
                "layout",
                "control",
                "parent_sha256",
                "mutant_sha256",
                "disabled_predicate",
                "normal_result",
                "disabled_result",
                "detection_assertion_failed",
            ],
        )?;
        let layout = if index < 8 { Layout::T } else { Layout::I };
        let k = index % 8;
        if record["layout"] != layout.name()
            || record["control"] != controls::NAMES[k]
            || record["disabled_predicate"] != controls::DISABLED[k]
            || record["parent_sha256"] != original[index]["parent_sha256"]
            || record["mutant_sha256"] != original[index]["mutant_sha256"]
            || record["normal_result"] != original[index]["normal_result"]
            || record["detection_assertion_failed"] != true
        {
            return fail(1, "meta-control-table-or-detection");
        }
        let normal = strict(&check_ref(
            &context.evidence,
            &record["normal_result"],
            LOG_LIMIT,
        )?)?;
        let disabled = strict(&check_ref(
            &context.evidence,
            &record["disabled_result"],
            LOG_LIMIT,
        )?)?;
        let disabled_path = format!(
            "{}/meta-results/{}/{k:02}-{}.json",
            path.strip_prefix(&context.evidence)
                .map_err(|e| malformed(e.to_string()))?
                .display(),
            layout.name(),
            controls::NAMES[k]
        );
        if record["disabled_result"]["path"] != disabled_path
            || !controls::detected(&normal, layout, k)
            || disabled["exit"] != 0
        {
            return fail(1, "meta-normal-or-disabled-result");
        }
        search::require_positive(&disabled["report"], layout)?;
        if !normal["report"].is_null() {
            costs.push(normal["report"].clone());
        }
        costs.push(disabled["report"].clone());
    }
    for layout in [Layout::T, Layout::I] {
        if value["positives"][layout.name()] != control_report["detail"]["positives"][layout.name()]
        {
            return fail(1, "meta-positive-pin");
        }
        let positive = strict(&check_ref(
            &context.evidence,
            &value["positives"][layout.name()],
            LOG_LIMIT,
        )?)?;
        search::require_positive(&positive, layout)?;
        costs.push(positive);
    }
    let expected = count_sum(&costs)?;
    let log_bytes = context.input(log, LOG_LIMIT)?;
    let text = std::str::from_utf8(&log_bytes).map_err(|e| malformed(e.to_string()))?;
    if !text.contains("1 passed; 0 failed; 0 ignored") {
        return fail(1, "meta-test-not-passing-one-test");
    }
    let mut markers = Vec::new();
    for line in text.lines() {
        if let Some(at) = line.find("{\"counts\":")
            && let Ok(record) = strict(&line.as_bytes()[at..])
            && record["schema"] == "zeno-fcis/priority-controls-meta-work/1"
        {
            markers.push(record);
        }
    }
    if markers.len() != 1
        || markers[0]["status"] != "passed"
        || markers[0]["counts"]["authority_evaluations"] != expected["authority_evaluations"]
        || markers[0]["counts"]["checked_rows"] != expected["checked_rows"]
        || markers[0]["counts"]["authority_usage"] != expected["authority_usage"]
    {
        return fail(1, "meta-measured-work-or-log");
    }
    Ok((value, expected))
}

pub(super) fn summarize(
    context: &mut Context,
    path: &Path,
    runs: [&Path; 2],
    replays: &Path,
    controls_path: &Path,
    meta_log: &Path,
    review: Option<&Path>,
) -> Result<&'static str> {
    let [t_run, i_run] = runs;
    let bundle = Bundle::load(context, path)?;
    let mut run_reports = Vec::new();
    let mut construction_costs = Vec::new();
    for (layout, run) in [(Layout::T, t_run), (Layout::I, i_run)] {
        context.input(&run.join("manifest.json"), LOG_LIMIT)?;
        files::verify(run)?;
        let report = files::phase_report(&run.join("run.json"), "search")?;
        if report["schema"] != "zeno-fcis/priority-construction-run/1"
            || report["phase"] != "search"
            || report["detail"]["layout"] != layout.name()
        {
            return fail(65, "summary-run-schema-or-layout");
        }
        construction_costs.push(report["counts"].clone());
        run_reports.push(report);
        context.detail["runs"] = json!(run_reports);
    }
    let failed = run_reports
        .iter()
        .any(|r| r["status"] != "candidate-checked");
    if failed {
        let incomplete = run_reports.iter().all(|r| {
            matches!(
                r["status"].as_str(),
                Some("candidate-checked" | "incomplete")
            )
        });
        context.detail = json!({"runs":run_reports,"construction_costs":count_sum(&construction_costs)?,"replays":{"status":"not-run","reason":"search-not-successful"},
            "controls":{"status":"not-run","reason":"search-not-successful"},"review":{"status":"not-run","reason":"technical-gates-incomplete"},"qualification_inputs_sha256":Value::Null});
        return fail(
            if incomplete { 2 } else { 1 },
            "two-layout-construction-criterion-not-met",
        );
    }
    let mut pairs = BTreeMap::new();
    include_tree(&context.evidence, path, &mut pairs)?;
    for reference in bundle.f51.record["checks"]
        .as_array()
        .ok_or_else(|| malformed("F51 checks"))?
    {
        for key in ["log", "stderr"] {
            let p = relative(
                reference[key]["path"]
                    .as_str()
                    .ok_or_else(|| malformed("F51 log path"))?,
            )?;
            include(&context.evidence, &context.evidence.join(p), &mut pairs)?;
        }
    }
    let review_path = relative(
        bundle.f51.record["review"]["path"]
            .as_str()
            .ok_or_else(|| malformed("F51 review path"))?,
    )?;
    include(
        &context.evidence,
        &context.evidence.join(review_path),
        &mut pairs,
    )?;
    let qualification_path = relative(
        bundle.binding["f51_evidence"]["path"]
            .as_str()
            .ok_or_else(|| malformed("F51 evidence path"))?,
    )?;
    include(
        &context.evidence,
        &context.evidence.join(qualification_path),
        &mut pairs,
    )?;
    include(
        &context.evidence,
        &context
            .evidence
            .join(qualification_path)
            .parent()
            .ok_or_else(|| malformed("F51 parent"))?
            .join("source.sha256"),
        &mut pairs,
    )?;
    let prepare_report = files::phase_report(&path.join("prepare.json"), "prepare")?;
    let preflight_dir = context.evidence.join("preflight");
    files::verify(&preflight_dir)?;
    let preflight_report = files::phase_report(&preflight_dir.join("preflight.json"), "preflight")?;
    if prepare_report["status"] != "prepared" || preflight_report["status"] != "bounds-ready" {
        return fail(1, "summary-preparation-or-preflight");
    }
    include_tree(&context.evidence, &preflight_dir, &mut pairs)?;
    let mut replay_costs = Vec::new();
    for (index, (layout, run)) in [(Layout::T, t_run), (Layout::I, i_run)]
        .into_iter()
        .enumerate()
    {
        let report = &run_reports[index];
        if report["identities"]["source_manifest_sha256"]
            != context.identities["source_manifest_sha256"]
            || report["identities"]["checker_identity_sha256"]
                != bundle.f51.record["checker_identity_sha256"]
        {
            return fail(65, "summary-run-source-identity");
        }
        let cli = Path::new(
            report["detail"]["cli"]
                .as_str()
                .ok_or_else(|| malformed("run-CLI-path"))?,
        );
        search::bind_cli(context, &bundle, cli)?;
        let approval_ref = &report["detail"]["owner_approval"];
        check_ref(&context.evidence, approval_ref, DOCUMENT_LIMIT)?;
        let approval_path = context.evidence.join(relative(
            approval_ref["path"]
                .as_str()
                .ok_or_else(|| malformed("owner-approval-path"))?,
        )?);
        let approved = qualification::approval(&context.evidence, &approval_path, path)?;
        include(&context.evidence, &approval_path, &mut pairs)?;
        include(
            &context.evidence,
            &context.evidence.join(relative(
                approved["decision_record"]["path"]
                    .as_str()
                    .ok_or_else(|| malformed("owner-decision-path"))?,
            )?),
            &mut pairs,
        )?;
        include_tree(&context.evidence, run, &mut pairs)?;
        let replay_dir = replays.join(layout.name());
        context.input(&replay_dir.join("manifest.json"), LOG_LIMIT)?;
        files::verify(&replay_dir)?;
        let replay = files::phase_report(&replay_dir.join("replay.json"), "replay")?;
        if replay["status"] != "replay-agrees"
            || replay["identities"]["source_manifest_sha256"]
                != context.identities["source_manifest_sha256"]
        {
            return fail(1, "summary-replay-not-current");
        }
        replay_costs.push(replay["counts"].clone());
        include_tree(&context.evidence, &replay_dir, &mut pairs)?;
        let verification = context.out.join(format!("verification/{}", layout.name()));
        let verified = search::replay_run(context, &bundle, run, cli, &verification)?;
        if verified["subject_sha256"] != replay["detail"]["subject_sha256"]
            || verified["attempts"] != replay["detail"]["attempts"]
        {
            return fail(1, "summary-replay-coverage-differs");
        }
    }
    controls::verify_original(controls_path)?;
    context.input(&controls_path.join("controls.json"), LOG_LIMIT)?;
    let controls_report = files::phase_report(&controls_path.join("controls.json"), "controls")?;
    if controls_report["status"] != "controls-caught"
        || controls_report["detail"]["control_count"] != 16
        || controls_report["identities"]["source_manifest_sha256"]
            != context.identities["source_manifest_sha256"]
    {
        return fail(1, "summary-controls-not-current");
    }
    for (index, record) in controls_report["detail"]["controls"]
        .as_array()
        .filter(|r| r.len() == 16)
        .ok_or_else(|| malformed("sixteen controls"))?
        .iter()
        .enumerate()
    {
        let layout = if index < 8 { Layout::T } else { Layout::I };
        let k = index % 8;
        let source = [
            check_ref(&context.evidence, &record["project"], SOURCE_LIMIT)?,
            check_ref(&context.evidence, &record["policy"], SOURCE_LIMIT)?,
            check_ref(&context.evidence, &record["abstraction"], SOURCE_LIMIT)?,
        ];
        let text = source
            .iter()
            .map(|b| std::str::from_utf8(b).map_err(|e| malformed(e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        let actual =
            controls::api_result(search::native(&bundle, layout, text[0], text[1], text[2]));
        if !actual["report"].is_null() {
            context.add_native(&actual["report"])?;
        }
        write_json(
            &context
                .out
                .join(format!("verification/controls/{index:02}.json")),
            &actual,
        )?;
        if !controls::detected(&actual, layout, k)
            || actual
                != strict(&check_ref(
                    &context.evidence,
                    &record["normal_result"],
                    LOG_LIMIT,
                )?)?
        {
            return fail(1, "summary-control-replay-differs");
        }
    }
    let (meta_report, meta_costs) =
        meta(context, &bundle, controls_path, meta_log, &controls_report)?;
    include_tree(&context.evidence, controls_path, &mut pairs)?;
    include(&context.evidence, meta_log, &mut pairs)?;
    let final_source = context.evidence.join("final/source.sha256");
    let source = context.input(&final_source, LOG_LIMIT)?;
    if sha(&source)
        != context.identities["source_manifest_sha256"]
            .as_str()
            .ok_or_else(|| malformed("source identity"))?
    {
        return fail(65, "final-source-differs");
    }
    include(&context.evidence, &final_source, &mut pairs)?;
    let binaries = context.evidence.join("final/binaries.sha256");
    let binary_bytes = context.input(&binaries, DOCUMENT_LIMIT)?;
    include(&context.evidence, &binaries, &mut pairs)?;
    let lines = std::str::from_utf8(&binary_bytes).map_err(|e| malformed(e.to_string()))?;
    let mut hashes = BTreeSet::new();
    for line in lines.lines() {
        let (hash, file) = line
            .split_once("  ")
            .ok_or_else(|| malformed("binary inventory row"))?;
        if files::binary_sha(Path::new(file))? != hash {
            return fail(65, "final-binary-differs");
        }
        hashes.insert(hash.to_string());
    }
    if !hashes.contains(
        bundle.f51.record["cli_sha256"]
            .as_str()
            .ok_or_else(|| malformed("CLI identity"))?,
    ) || !hashes.contains(
        context.identities["driver_sha256"]
            .as_str()
            .ok_or_else(|| malformed("driver identity"))?,
    ) {
        return fail(65, "final-native-and-driver-pins");
    }
    let pairs: Vec<_> = pairs.into_iter().map(|(path, hash)| [path, hash]).collect();
    let qualification_digest = sha(&canonical(&pairs)?);
    write_json(&context.out.join("qualification-inputs.json"), &pairs)?;
    let mut reference_counts = native::Counts::default();
    let reference = native::reference(
        &bundle.relation,
        &bundle.abstract_project,
        &bundle.abstract_policy,
        &mut reference_counts,
    );
    context.add_native(&reference_counts.json())?;
    write_json(
        &context.out.join("verification/reference.json"),
        &match &reference {
            Ok(v) => v.clone(),
            Err(e) => json!({"status":"failed","reason":e.reason,"counts":reference_counts.json()}),
        },
    )?;
    reference?;
    let mut costs = vec![
        prepare_report["counts"].clone(),
        preflight_report["counts"].clone(),
    ];
    costs.extend(construction_costs.clone());
    costs.extend(replay_costs.clone());
    costs.push(controls_report["counts"].clone());
    costs.push(meta_costs.clone());
    costs.push(context.counts.clone());
    let mut prior_summary = Value::Null;
    if review.is_some() {
        let prior_dir = context.evidence.join("review-packet");
        files::verify(&prior_dir)?;
        let prior = files::phase_report(&prior_dir.join("summary.json"), "summarize")?;
        if prior["status"] != "awaiting-review"
            || prior["detail"]["qualification_inputs_sha256"] != qualification_digest
            || prior["counts"] != context.counts
        {
            return fail(65, "review-packet-subject-or-verification-differs");
        }
        costs.push(prior["counts"].clone());
        prior_summary = prior["counts"].clone();
    }
    let all_costs = count_sum(&costs)?;
    if all_costs["transform_accounting_complete"] != true {
        return fail(1, "incomplete-transform-work-accounting");
    }
    context.detail = json!({"runs":run_reports,"construction_costs":count_sum(&construction_costs)?,"replay_costs":count_sum(&replay_costs)?,"control_costs":controls_report["counts"],
        "meta_costs":meta_costs,"preparation_costs":prepare_report["counts"],"preflight_costs":preflight_report["counts"],"prior_summary_verification_costs":prior_summary,"summary_verification_costs":context.counts,"all_work_by_unit":all_costs,"qualification_inputs_sha256":qualification_digest,
        "meta_source_sha256":meta_report["source_manifest_sha256"],"owner_question_time":{"current_run_ms":"0","prior_decision_elapsed":"not-measured"},
        "optional_model":"off","optional_optimization":"not-run","scope":"finite construction only; no family, history, migration, external delivery or publication authority"});
    let status = if let Some(review) = review {
        let accepted = strict(&context.input(review, DOCUMENT_LIMIT)?)?;
        closed(
            &accepted,
            &[
                "schema",
                "status",
                "reviewer",
                "qualification_inputs_sha256",
                "decision_record",
            ],
        )?;
        if accepted["schema"] != "zeno-fcis/priority-review/1"
            || accepted["status"] != "accepted"
            || accepted["reviewer"].as_str().is_none_or(str::is_empty)
            || accepted["qualification_inputs_sha256"] != qualification_digest
        {
            return fail(65, "review-not-accepted-for-these-inputs");
        }
        check_ref(
            &context.evidence,
            &accepted["decision_record"],
            DOCUMENT_LIMIT,
        )?;
        context.detail["review"] = accepted;
        "qualified"
    } else {
        context.detail["review"] =
            json!({"status":"awaiting-review","reason":"independent-decision-required"});
        "awaiting-review"
    };
    write(&context.out.join("REPORT.md"),format!("# Finite priority construction pilot\n\nStatus: {status}. Both T and I passed native refinement, scalar totality, lowering and complete replay. All sixteen semantic/binding controls and their isolated negative tests passed.\n\nQualification inputs: {qualification_digest}.\n\nEvery failed proposal is retained in the source-bound run manifests. Costs are recorded separately for construction, replay, controls, meta-tests and this summary's verification. No benchmark or optimization improvement is claimed.\n\nAt capacity two, non-strict interior insertion has no witness; the tested extrema restriction is strict. A66 family proofs and external delivery/history/migration/publication claims remain outside this finite result.\n").as_bytes())?;
    Ok(status)
}
