//! Bounded proposal/native-feedback loop and synthesis-free replay.

use super::{
    common::*,
    compilation, files,
    layout::Layout,
    phases::{Bundle, Context},
    proposal::{self, Coordinate},
    qualification,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;
use zeno_fcis_contract::refinement::{self, Sources};

pub(super) fn subject_sha(project: &[u8], policy: &[u8], abstraction: &[u8]) -> Result<String> {
    Ok(sha(&canonical(&json!([
        ["abstraction.json", sha(abstraction)],
        ["project.zeno", sha(project)],
        ["v2/policy.json", sha(policy)]
    ]))?))
}
pub(super) fn native(
    bundle: &Bundle,
    layout: Layout,
    project: &str,
    policy: &str,
    abstraction: &str,
) -> Result<Value> {
    refinement::check(
        Sources {
            abstract_project: &bundle.abstract_project,
            abstract_policy: &bundle.abstract_policy,
            concrete_project: project,
            concrete_policy: policy,
            abstraction,
        },
        (layout.raw_count() * 16) as u64,
    )
    .map(|r| r.json())
    .map_err(|e| malformed(e.to_string()))
}
pub(super) fn require_positive(report: &Value, layout: Layout) -> Result<()> {
    if report["schema"] != refinement::RESULT_SCHEMA
        || report["verdict"] != "refines"
        || report["checked_rows"] != (layout.raw_count() * 16) as u64
        || report["raw_states"] != layout.raw_count()
        || report["valid_states"] != 45
        || report["abstract_states"] != 45
        || report["inputs"] != 16
        || report["authority_evaluations"] != if layout == Layout::T { 3074 } else { 6722 }
    {
        return fail(1, "native-full-coverage-required");
    }
    Ok(())
}
pub(super) fn bind_cli(context: &mut Context, bundle: &Bundle, cli: &Path) -> Result<()> {
    let hash = files::binary_sha(cli)?;
    if bundle.f51.record["cli_sha256"].as_str() != Some(hash.as_str()) {
        return fail(65, "native-cli-sha256");
    }
    context.identities["native_cli_sha256"] = json!(hash);
    Ok(())
}
pub(super) fn feedback(
    layout: Layout,
    alpha: &[Option<usize>],
    report: &Value,
    examples: &mut BTreeSet<Coordinate>,
) -> Result<Coordinate> {
    if report["verdict"] != "counterexample" {
        return fail(2, "native-inconclusive");
    }
    let witness = &report["witness"];
    let s = witness["state"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| gate("qualification-defect:initial-or-coverage"))?;
    let q = witness["input"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| gate("qualification-defect:initial-or-coverage"))?;
    if s >= layout.raw_count()
        || q >= 16
        || alpha.get(s).copied().flatten().is_none()
        || witness["obligation"] == "malformed-state"
    {
        return fail(1, "qualification-defect:invalid-state-or-coordinate");
    }
    if examples.contains(&(s, q)) {
        return fail(1, "integration-mismatch:repeated-counterexample");
    }
    if examples.len() >= 48 {
        return fail(1, "example-cap-exceeded");
    }
    examples.insert((s, q));
    Ok((s, q))
}

pub(super) fn finish_stages(stages: &mut Value, status: &str, stop_reason: &Value) -> Result<()> {
    closed(
        stages,
        &["proposal", "source", "compilation", "native", "transform"],
    )?;
    for stage in stages
        .as_object_mut()
        .ok_or_else(|| malformed("attempt-stages"))?
        .values_mut()
    {
        if stage["status"] == "running" {
            stage["status"] = json!("failed");
            stage["reason"] = stop_reason.clone();
        } else if stage["status"] == "not-run" {
            stage["reason"] = json!({"prerequisite_status":status,"stop_reason":stop_reason});
        }
    }
    Ok(())
}

fn transform_work(
    context: &mut Context,
    process: &files::Process,
    rows: u64,
    replay: bool,
) -> Result<()> {
    let report = if process.truncated || process.timed_out {
        None
    } else {
        strict(&process.stdout).ok()
    };
    let evaluated = report.as_ref().and_then(|r| {
        if (!replay && r["status"] == "equivalent")
            || (replay && (r["status"] == "replayed" || r["detail"]["differing_fields"].is_array()))
        {
            Some(2 * rows)
        } else if !replay && r["status"] == "counterexample" {
            r["detail"]["ordinal"]
                .as_u64()
                .filter(|n| *n < rows)
                .map(|n| 2 * (n + 1))
        } else if !replay && r["status"] == "refused" {
            Some(0)
        } else {
            None
        }
    });
    if let Some(n) = evaluated {
        context.counts["transform_evaluations"] = json!(
            context.counts["transform_evaluations"]
                .as_u64()
                .ok_or_else(|| gate("transform-count"))?
                .checked_add(n)
                .ok_or_else(|| gate("transform-count-overflow"))?
        );
    } else {
        context.counts["transform_accounting_complete"] = json!(false);
    }
    Ok(())
}

pub(super) fn transform(
    context: &mut Context,
    cli: &Path,
    dir: &Path,
    source: &Path,
    layout: Layout,
    check: bool,
) -> Result<Value> {
    let original = source.join("compiled.zcve");
    let candidate = source.join("lowered.zcve");
    let receipt = source.join("transform-receipt.json");
    let rows = (layout.raw_count() * 16) as u64;
    let mut records = Vec::new();
    if check {
        let args = vec![
            "transform".into(),
            "check".into(),
            "--original".into(),
            original.to_string_lossy().into_owned(),
            "--candidate".into(),
            candidate.to_string_lossy().into_owned(),
            "--receipt".into(),
            receipt.to_string_lossy().into_owned(),
            "--max-input-tuples".into(),
            rows.to_string(),
        ];
        let process = files::process(cli, &args, &files::repository()?)?;
        let metadata = process.save(dir, "transform-check")?;
        records.push(metadata);
        transform_work(context, &process, rows, false)?;
        process.complete()?;
        if process.exit != Some(0) {
            return fail(
                if process.exit == Some(2) { 2 } else { 1 },
                "transform-check-failed",
            );
        }
        let report = strict(&process.stdout)?;
        if report["status"] != "equivalent" {
            return fail(1, "transform-check-status");
        }
    }
    let receipt_bytes = read(&receipt, DOCUMENT_LIMIT)?;
    let value = strict(&receipt_bytes)?;
    if value["schema"] != "zeno-fcis/transform-receipt/1"
        || value["verdict"] != "equivalent"
        || value["inputs_checked"] != rows
        || value["domain"]["size"] != rows
        || value["original"]["sha256"].as_str()
            != Some(sha(&read(&original, SOURCE_LIMIT)?).as_str())
        || value["candidate"]["sha256"].as_str()
            != Some(sha(&read(&candidate, SOURCE_LIMIT)?).as_str())
    {
        return fail(1, "transform-receipt-binding-or-coverage");
    }
    let args = vec![
        "transform".into(),
        "replay".into(),
        "--original".into(),
        original.to_string_lossy().into_owned(),
        "--candidate".into(),
        candidate.to_string_lossy().into_owned(),
        "--receipt".into(),
        receipt.to_string_lossy().into_owned(),
        "--max-input-tuples".into(),
        rows.to_string(),
    ];
    let process = files::process(cli, &args, &files::repository()?)?;
    let metadata = process.save(dir, "transform-replay")?;
    records.push(metadata);
    transform_work(context, &process, rows, true)?;
    process.complete()?;
    if process.exit != Some(0) || strict(&process.stdout)?["status"] != "replayed" {
        return fail(
            if process.exit == Some(2) { 2 } else { 1 },
            "transform-replay-failed",
        );
    }
    Ok(
        json!({"status":"replayed","receipt_sha256":sha(&receipt_bytes),"inputs_checked":rows,"processes":records,"receipt":value}),
    )
}

pub(super) fn search(
    context: &mut Context,
    path: &Path,
    approval: &Path,
    name: &str,
    cli: &Path,
) -> Result<&'static str> {
    context.detail = json!({"layout":name,"cli":cli.to_string_lossy(),"attempts":[],"final_attempt":Value::Null,"first_correct_ms":Value::Null,"layout_work":0});
    let bundle = Bundle::load(context, path)?;
    let layout = Layout::parse(name)?;
    bind_cli(context, &bundle, cli)?;
    context.input(approval, DOCUMENT_LIMIT)?;
    let approved = qualification::approval(&context.evidence, approval, path)?;
    let preflight = context.evidence.join("preflight");
    files::verify(&preflight)?;
    context.input(&preflight.join("preflight.json"), LOG_LIMIT)?;
    let preflight_report = files::phase_report(&preflight.join("preflight.json"), "preflight")?;
    if preflight_report["status"] != "bounds-ready"
        || preflight_report["identities"]["source_manifest_sha256"]
            != context.identities["source_manifest_sha256"]
        || preflight_report["detail"]["reference"]["counts"]["checked_rows"] != 720
    {
        return fail(65, "preflight-not-current");
    }
    let features = super::layout::Features::new(layout);
    features.check_features(&bundle.relation)?;
    let alpha = layout.alpha(&bundle.relation)?;
    let genesis = layout
        .ordinal(&layout.encode(&bundle.relation.states[0])?)
        .ok_or_else(|| gate("genesis-codec"))?;
    let mut examples: BTreeSet<_> = (0..16).map(|q| (genesis, q)).collect();
    let mut attempts = Vec::new();
    let started = Instant::now();
    let mut total_work = 0u64;
    context.detail = json!({"layout":name,"cli":cli.to_string_lossy(),"owner_approval":files::reference(&context.evidence,approval)?,"owner_decision":approved["decision_record"],"attempts":[],"final_attempt":Value::Null,
        "optional_model":"off","optional_optimization":"not-run","first_correct_ms":Value::Null,"examples":16,"layout_work":0});
    for index in 0..32 {
        let dir = context.out.join(format!("attempts/{index:04}"));
        std::fs::create_dir_all(&dir)?;
        let attempt_started = Instant::now();
        let mut stages = json!({"proposal":{"status":"not-run","reason":"not-started"},"source":{"status":"not-run","reason":"no-proposal"},
            "compilation":{"status":"not-run","reason":"no-source"},"native":{"status":"not-run","reason":"no-admitted-source"},"transform":{"status":"not-run","reason":"no-native-positive"}});
        let outcome = (|| -> Result<Option<Coordinate>> {
            stages["proposal"] = json!({"status":"running"});
            let proposal = proposal::propose(&bundle.relation, layout, &examples)?;
            write_json(&dir.join("examples.json"), &proposal.examples)?;
            write_json(&dir.join("proposal.json"), &proposal.document)?;
            total_work = total_work
                .checked_add(proposal.work)
                .ok_or_else(|| gate("layout-work-overflow"))?;
            context.counts["proposal_work"] = json!(total_work);
            if total_work > proposal::LAYOUT_WORK {
                return fail(1, "layout-work-over-cap");
            }
            stages["proposal"] = json!({"status":if proposal.stop.is_none(){"proposed"}else{"incomplete"},"work":proposal.work,"stop_reason":proposal.stop});
            let Some(expressions) = proposal.expressions else {
                return fail(
                    2,
                    proposal
                        .stop
                        .unwrap_or_else(|| "proposal-incomplete".into()),
                );
            };
            stages["source"] = json!({"status":"running"});
            let (project, policy) = proposal::sources(layout, &expressions)?;
            if project.as_bytes()
                != read(
                    &bundle.path.join(layout.name()).join("project.zeno"),
                    SOURCE_LIMIT,
                )?
            {
                return fail(1, "integration-mismatch:frozen-declarations");
            }
            write(&dir.join("candidate/project.zeno"), project.as_bytes())?;
            write(&dir.join("candidate/v2/policy.json"), &policy)?;
            let abstraction = proposal::abstraction(
                &bundle.abstract_project,
                &bundle.abstract_policy,
                &bundle.relation,
                layout,
                project.as_bytes(),
                &policy,
            )?;
            write(&dir.join("abstraction.json"), &abstraction)?;
            stages["source"] = json!({"status":"materialized","subject_sha256":subject_sha(project.as_bytes(),&policy,&abstraction)?});
            let policy_text = std::str::from_utf8(&policy).map_err(|e| malformed(e.to_string()))?;
            stages["compilation"] = json!({"status":"running"});
            let compilation = compilation::compile(layout, &project, policy_text)?;
            context.add_scalar(&compilation.inventory)?;
            write(&dir.join("compiled.zcve"), &compilation.original)?;
            let mut inventory = compilation.inventory;
            inventory["source_manifest_sha256"] =
                context.identities["source_manifest_sha256"].clone();
            inventory["native_cli_sha256"] = context.identities["native_cli_sha256"].clone();
            inventory["checker_identity_sha256"] =
                context.identities["checker_identity_sha256"].clone();
            inventory["evaluator_identity_sha256"] =
                context.identities["evaluator_identity_sha256"].clone();
            inventory["cargo_lock_sha256"] = context.identities["cargo_lock_sha256"].clone();
            inventory["rust_toolchain_sha256"] =
                context.identities["rust_toolchain_sha256"].clone();
            inventory["grammar_sha256"] = proposal.document["grammar_sha256"].clone();
            inventory["reference_sha256"] = bundle.binding["reference_relation_sha256"].clone();
            inventory["reference_source_sha256"] =
                bundle.binding["reference_source_sha256"].clone();
            inventory["alpha_sha256"] = json!(sha(&canonical(&alpha)?));
            inventory["proposal_budget"] = json!({"max_work":proposal::PROPOSAL_WORK,"max_assignments":20000,"max_attempts":32,"max_layout_work":proposal::LAYOUT_WORK});
            write_json(&dir.join("inventory.json"), &inventory)?;
            if let Some(reason) = compilation.refusal {
                return fail(1, reason);
            }
            let lowered = compilation.lowered.ok_or_else(|| gate("lowering-absent"))?;
            write(&dir.join("lowered.zcve"), &lowered)?;
            stages["compilation"] = json!({"status":"admitted","nodes":inventory["nodes"],"lowering":inventory["lowering"]});
            stages["native"] = json!({"status":"running"});
            let report = native(
                &bundle,
                layout,
                &project,
                policy_text,
                std::str::from_utf8(&abstraction).map_err(|e| malformed(e.to_string()))?,
            )?;
            context.add_native(&report)?;
            write_json(&dir.join("native.json"), &report)?;
            stages["native"] =
                json!({"status":report["verdict"],"checked_rows":report["checked_rows"]});
            if report["verdict"] != "refines" {
                return Ok(Some(feedback(layout, &alpha, &report, &mut examples)?));
            }
            require_positive(&report, layout)?;
            stages["transform"] = json!({"status":"running"});
            let transformed = transform(context, cli, &dir, &dir, layout, true)?;
            stages["transform"] = transformed;
            Ok(None)
        })();
        let (status, stop_reason) = match &outcome {
            Ok(None) => ("candidate-checked", Value::Null),
            Ok(Some(coordinate)) => (
                "counterexample",
                json!({"state":coordinate.0,"input":coordinate.1}),
            ),
            Err(error) => (
                if error.code == 2 {
                    "incomplete"
                } else {
                    "failed"
                },
                json!(error.reason),
            ),
        };
        finish_stages(&mut stages, status, &stop_reason)?;
        write_json(
            &dir.join("attempt.json"),
            &json!({"schema":"zeno-fcis/priority-construction-attempt/1","revision":REVISION,"layout":name,"index":index,"status":status,"stop_reason":stop_reason,
            "stages":stages,"elapsed_ms":attempt_started.elapsed().as_millis().to_string()}),
        )?;
        files::seal(&dir)?;
        let attempt = json!({"index":index,"path":format!("attempts/{index:04}"),"manifest_sha256":sha(&read(&dir.join("manifest.json"),LOG_LIMIT)?),"status":status,"stop_reason":stop_reason});
        attempts.push(attempt);
        context.detail["attempts"] = json!(attempts);
        context.detail["examples"] = json!(examples.len());
        context.detail["layout_work"] = json!(total_work);
        match outcome {
            Ok(None) => {
                context.detail["final_attempt"] = json!(format!("attempts/{index:04}"));
                context.detail["first_correct_ms"] =
                    json!(started.elapsed().as_millis().to_string());
                return Ok("candidate-checked");
            }
            Ok(Some(_)) => {}
            Err(error) => return Err(error),
        }
    }
    fail(2, "attempt-limit/no-hit")
}

pub(super) struct Positive {
    pub(super) layout: Layout,
    pub(super) directory: std::path::PathBuf,
    pub(super) project: String,
    pub(super) policy: String,
    pub(super) abstraction: String,
    pub(super) subject: String,
}
pub(super) fn positive(bundle: &Bundle, run: &Path) -> Result<Positive> {
    files::verify(run)?;
    let report = files::phase_report(&run.join("run.json"), "search")?;
    if report["schema"] != "zeno-fcis/priority-construction-run/1"
        || report["phase"] != "search"
        || report["status"] != "candidate-checked"
    {
        return fail(1, "run-not-candidate-checked");
    }
    if report["identities"]["source_manifest_sha256"] != bundle.f51.record["source_manifest_sha256"]
        || report["identities"]["checker_identity_sha256"]
            != bundle.f51.record["checker_identity_sha256"]
        || report["identities"]["native_cli_sha256"] != bundle.f51.record["cli_sha256"]
    {
        return fail(65, "parent-run-source-identity");
    }
    let evidence = bundle
        .path
        .parent()
        .ok_or_else(|| malformed("bundle-parent"))?;
    let approval_ref = &report["detail"]["owner_approval"];
    check_ref(evidence, approval_ref, DOCUMENT_LIMIT)?;
    qualification::approval(
        evidence,
        &evidence.join(relative(
            approval_ref["path"]
                .as_str()
                .ok_or_else(|| malformed("owner-approval-path"))?,
        )?),
        &bundle.path,
    )?;
    let layout = Layout::parse(
        report["detail"]["layout"]
            .as_str()
            .ok_or_else(|| malformed("run-layout"))?,
    )?;
    let final_path = relative(
        report["detail"]["final_attempt"]
            .as_str()
            .ok_or_else(|| malformed("final-attempt"))?,
    )?;
    let directory = run.join(final_path);
    files::verify(&directory)?;
    let document = strict(&read(&directory.join("proposal.json"), DOCUMENT_LIMIT)?)?;
    let examples = strict(&read(&directory.join("examples.json"), DOCUMENT_LIMIT)?)?;
    let outputs = proposal::replay_proposal(&bundle.relation, layout, &document, &examples)?
        .ok_or_else(|| gate("final-proposal-incomplete"))?;
    let (project, policy_bytes) = proposal::sources(layout, &outputs)?;
    let abstraction_bytes = proposal::abstraction(
        &bundle.abstract_project,
        &bundle.abstract_policy,
        &bundle.relation,
        layout,
        project.as_bytes(),
        &policy_bytes,
    )?;
    if project.as_bytes() != read(&directory.join("candidate/project.zeno"), SOURCE_LIMIT)?
        || policy_bytes != read(&directory.join("candidate/v2/policy.json"), SOURCE_LIMIT)?
        || abstraction_bytes != read(&directory.join("abstraction.json"), SOURCE_LIMIT)?
    {
        return fail(65, "source-overwrite-or-adoption");
    }
    let subject = subject_sha(project.as_bytes(), &policy_bytes, &abstraction_bytes)?;
    Ok(Positive {
        layout,
        directory,
        project,
        policy: String::from_utf8(policy_bytes).map_err(|e| malformed(e.to_string()))?,
        abstraction: String::from_utf8(abstraction_bytes).map_err(|e| malformed(e.to_string()))?,
        subject,
    })
}

pub(super) fn replay(
    context: &mut Context,
    path: &Path,
    run: &Path,
    cli: &Path,
) -> Result<&'static str> {
    let bundle = Bundle::load(context, path)?;
    bind_cli(context, &bundle, cli)?;
    let out = context.out.clone();
    context.detail = replay_run(context, &bundle, run, cli, &out)?;
    Ok("replay-agrees")
}
pub(super) fn replay_run(
    context: &mut Context,
    bundle: &Bundle,
    run: &Path,
    cli: &Path,
    verification: &Path,
) -> Result<Value> {
    context.input(&run.join("manifest.json"), LOG_LIMIT)?;
    files::verify(run)?;
    let original = files::phase_report(&run.join("run.json"), "search")?;
    let layout = Layout::parse(
        original["detail"]["layout"]
            .as_str()
            .ok_or_else(|| malformed("run-layout"))?,
    )?;
    let attempts = original["detail"]["attempts"]
        .as_array()
        .ok_or_else(|| malformed("run-attempts"))?;
    if attempts.is_empty() || attempts.len() > 32 {
        return fail(65, "run-attempt-count");
    }
    let mut verified = Vec::new();
    let mut total_work = 0u64;
    for (index, attempt) in attempts.iter().enumerate() {
        if attempt["index"] != index || attempt["path"] != format!("attempts/{index:04}") {
            return fail(65, "run-attempt-order");
        }
        let dir = run.join(format!("attempts/{index:04}"));
        files::verify(&dir)?;
        if attempt["manifest_sha256"].as_str()
            != Some(sha(&read(&dir.join("manifest.json"), LOG_LIMIT)?).as_str())
        {
            return fail(65, "run-attempt-pin");
        }
        let proposal = strict(&read(&dir.join("proposal.json"), DOCUMENT_LIMIT)?)?;
        let examples = strict(&read(&dir.join("examples.json"), DOCUMENT_LIMIT)?)?;
        total_work = total_work
            .checked_add(
                proposal["proposal_work"]
                    .as_u64()
                    .ok_or_else(|| malformed("proposal-work"))?,
            )
            .ok_or_else(|| gate("layout-work-overflow"))?;
        if total_work > proposal::LAYOUT_WORK {
            return fail(65, "replay-layout-work-over-cap");
        }
        let expressions =
            proposal::replay_proposal(&bundle.relation, layout, &proposal, &examples)?;
        if let Some(outputs) = expressions {
            let (project, policy) = proposal::sources(layout, &outputs)?;
            if project.as_bytes() != read(&dir.join("candidate/project.zeno"), SOURCE_LIMIT)?
                || policy != read(&dir.join("candidate/v2/policy.json"), SOURCE_LIMIT)?
            {
                return fail(65, "replay-source-overwrite-or-adoption");
            }
            let abstraction = proposal::abstraction(
                &bundle.abstract_project,
                &bundle.abstract_policy,
                &bundle.relation,
                layout,
                project.as_bytes(),
                &policy,
            )?;
            if abstraction != read(&dir.join("abstraction.json"), SOURCE_LIMIT)? {
                return fail(65, "replay-alpha-or-source-pin");
            }
            let compiled = compilation::compile(
                layout,
                &project,
                std::str::from_utf8(&policy).map_err(|e| malformed(e.to_string()))?,
            )?;
            context.add_scalar(&compiled.inventory)?;
            if compiled.original != read(&dir.join("compiled.zcve"), SOURCE_LIMIT)? {
                return fail(1, "replay-compiler-graph-differs");
            }
            if let Some(lowered) = compiled.lowered {
                if lowered != read(&dir.join("lowered.zcve"), SOURCE_LIMIT)? {
                    return fail(1, "replay-lowered-graph-differs");
                }
                let result = native(
                    bundle,
                    layout,
                    &project,
                    std::str::from_utf8(&policy).map_err(|e| malformed(e.to_string()))?,
                    std::str::from_utf8(&abstraction).map_err(|e| malformed(e.to_string()))?,
                )?;
                context.add_native(&result)?;
                let out = verification.join(format!("attempts/{index:04}"));
                write_json(&out.join("native.json"), &result)?;
                if result != strict(&read(&dir.join("native.json"), LOG_LIMIT)?)? {
                    return fail(1, "replay-native-verdict-or-witness-differs");
                }
                if result["verdict"] == "refines" {
                    require_positive(&result, layout)?;
                    transform(context, cli, &out, &dir, layout, false)?;
                }
            } else if attempt["status"] != "failed" {
                return fail(1, "replay-missing-lowering-for-completed-attempt");
            }
        } else if attempt["status"] != "incomplete" {
            return fail(1, "replay-incomplete-proposal-status");
        }
        verified
            .push(json!({"index":index,"status":"matches","original_status":attempt["status"]}));
    }
    if original["status"] != "candidate-checked" {
        context.detail["replay_failure"] = json!({"layout":layout.name(),"attempts":verified,"final_positive":"not-run","reason":"original-run-not-successful"});
        return fail(2, "original-search-incomplete-or-failed");
    }
    let parent = positive(bundle, run)?;
    let final_report = native(
        bundle,
        layout,
        &parent.project,
        &parent.policy,
        &parent.abstraction,
    )?;
    context.add_native(&final_report)?;
    write_json(&verification.join("final-positive.json"), &final_report)?;
    require_positive(&final_report, layout)?;
    Ok(
        json!({"layout":layout.name(),"attempts":verified,"final_positive":files::reference(&context.evidence,&verification.join("final-positive.json"))?,"subject_sha256":parent.subject,"proposal_work":0,"replayed_layout_work":total_work}),
    )
}
