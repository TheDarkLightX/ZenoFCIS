//! Shell orchestration. Every output directory is fresh; pure library results
//! supply native verdicts and measured work.

use super::{
    Phase,
    common::*,
    files,
    layout::{Features, Layout},
    native, qualification,
    reference::Relation,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(super) struct Context {
    pub(super) evidence: PathBuf,
    pub(super) out: PathBuf,
    pub(super) inputs: Vec<Value>,
    pub(super) counts: Value,
    pub(super) identities: Value,
    pub(super) detail: Value,
    started: Instant,
}
impl Context {
    fn new(phase: &Phase) -> Result<Self> {
        let evidence = phase.evidence()?.canonicalize()?;
        if evidence.starts_with(files::repository()?.canonicalize()?) {
            return fail(65, "evidence-must-be-outside-source");
        }
        let out = phase.out();
        let relative_out = out
            .strip_prefix(&evidence)
            .map_err(|_| malformed("output-outside-evidence"))?;
        relative(
            relative_out
                .to_str()
                .ok_or_else(|| malformed("output-utf8"))?,
        )?;
        for ancestor in out.ancestors().skip(1) {
            if ancestor == evidence {
                break;
            }
            if ancestor.exists() && !ancestor.canonicalize()?.starts_with(&evidence) {
                return fail(65, "output-parent-escapes-evidence");
            }
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir(out)?;
        let identities = json!({"source_manifest_sha256":Value::Null,
            "driver_sha256":Value::Null,"cargo_lock_sha256":Value::Null,"rust_toolchain_sha256":Value::Null,
            "native_cli_sha256":Value::Null,"checker_identity_sha256":Value::Null,"evaluator_identity_sha256":Value::Null});
        Ok(Self {
            evidence,
            out: out.to_path_buf(),
            inputs: vec![],
            counts: json!({"authority_evaluations":0,"checked_rows":0,"proposal_work":0,"transform_evaluations":0,"transform_accounting_complete":true,"scalar_evaluations":0,"scalar_steps":"0","authority_usage":["0","0","0","0","0","0","0","0"]}),
            identities,
            detail: json!({}),
            started: Instant::now(),
        })
    }
    fn initialize(&mut self) -> Result<()> {
        let source = files::source_inventory()?;
        write(&self.out.join("source.sha256"), &source)?;
        self.identities = json!({"source_manifest_sha256":sha(&source),
            "driver_sha256":files::binary_sha(&std::env::current_exe()?)?,
            "cargo_lock_sha256":sha(&read(&files::repository()?.join("Cargo.lock"),SOURCE_LIMIT)?),
            "rust_toolchain_sha256":sha(&read(&files::repository()?.join("rust-toolchain.toml"),DOCUMENT_LIMIT)?),
            "native_cli_sha256":Value::Null,"checker_identity_sha256":Value::Null,"evaluator_identity_sha256":Value::Null});
        Ok(())
    }
    pub(super) fn input(&mut self, path: &Path, limit: usize) -> Result<Vec<u8>> {
        let bytes = read(path, limit)?;
        let reference = files::reference(&self.evidence, path)?;
        if !self.inputs.contains(&reference) {
            self.inputs.push(reference);
        }
        Ok(bytes)
    }
    pub(super) fn add_native(&mut self, report: &Value) -> Result<()> {
        for key in ["authority_evaluations", "checked_rows"] {
            let n = report[key]
                .as_u64()
                .ok_or_else(|| gate("native-count-missing"))?;
            self.counts[key] = json!(
                self.counts[key]
                    .as_u64()
                    .ok_or_else(|| gate("native-count-kind"))?
                    .checked_add(n)
                    .ok_or_else(|| gate("native-count-overflow"))?
            );
        }
        for j in 0..8 {
            let value = report["authority_usage"][j]
                .as_str()
                .ok_or_else(|| gate("native-usage-kind"))?
                .parse::<u128>()
                .map_err(|e| gate(e.to_string()))?;
            let old = self.counts["authority_usage"][j]
                .as_str()
                .ok_or_else(|| gate("native-usage-kind"))?
                .parse::<u128>()
                .map_err(|e| gate(e.to_string()))?;
            self.counts["authority_usage"][j] = json!(
                old.checked_add(value)
                    .ok_or_else(|| gate("native-usage-overflow"))?
                    .to_string()
            );
        }
        Ok(())
    }
    pub(super) fn f51(&mut self, file: &Path) -> Result<qualification::QualifiedF51> {
        self.input(file, DOCUMENT_LIMIT)?;
        let outcome = qualification::load(&self.evidence, file, &self.out);
        // The current native replay file exists even when comparison refuses.
        let current = self.out.join("f51-current.json");
        if current.is_file() {
            let report = strict(&read(&current, LOG_LIMIT)?)?;
            self.add_native(&report)?;
        }
        let qualified = outcome?;
        self.identities["checker_identity_sha256"] =
            qualified.record["checker_identity_sha256"].clone();
        self.identities["evaluator_identity_sha256"] =
            qualified.record["evaluator_identity_sha256"].clone();
        Ok(qualified)
    }
    pub(super) fn add_scalar(&mut self, inventory: &Value) -> Result<()> {
        if let Some(n) = inventory["scalar_totality"]["evaluations"].as_u64() {
            self.counts["scalar_evaluations"] = json!(
                self.counts["scalar_evaluations"]
                    .as_u64()
                    .ok_or_else(|| gate("scalar-count"))?
                    + n
            );
            let steps = inventory["scalar_totality"]["steps"]
                .as_str()
                .ok_or_else(|| gate("scalar-steps"))?
                .parse::<u128>()
                .map_err(|e| gate(e.to_string()))?;
            let old = self.counts["scalar_steps"]
                .as_str()
                .ok_or_else(|| gate("scalar-steps"))?
                .parse::<u128>()
                .map_err(|e| gate(e.to_string()))?;
            self.counts["scalar_steps"] = json!((old + steps).to_string());
        }
        Ok(())
    }
}

pub(super) struct Bundle {
    pub(super) path: PathBuf,
    pub(super) relation: Relation,
    pub(super) abstract_project: String,
    pub(super) abstract_policy: String,
    pub(super) f51: qualification::QualifiedF51,
    pub(super) binding: Value,
}
impl Bundle {
    pub(super) fn load(context: &mut Context, path: &Path) -> Result<Self> {
        context.input(&path.join("manifest.json"), LOG_LIMIT)?;
        files::verify(path)?;
        let binding = strict(&read(&path.join("binding.json"), DOCUMENT_LIMIT)?)?;
        closed(
            &binding,
            &[
                "schema",
                "revision",
                "f51_evidence",
                "reference_source_sha256",
                "reference_relation_sha256",
                "design_inputs",
            ],
        )?;
        if binding["schema"] != "zeno-fcis/priority-bundle/1" || binding["revision"] != REVISION {
            return fail(65, "bundle-schema");
        }
        check_ref(&context.evidence, &binding["f51_evidence"], DOCUMENT_LIMIT)?;
        let f51_path = context.evidence.join(relative(
            binding["f51_evidence"]["path"]
                .as_str()
                .ok_or_else(|| malformed("bundle-f51-path"))?,
        )?);
        let f51 = context.f51(&f51_path)?;
        let relation_bytes = read(&path.join("reference/relation.json"), SOURCE_LIMIT)?;
        let reference_source = read(&path.join("reference/priority_reference.py"), SOURCE_LIMIT)?;
        let relation = super::reference::validate_files(
            &relation_bytes,
            &read(
                &path.join("reference/reference-report.json"),
                DOCUMENT_LIMIT,
            )?,
            &read(&path.join("reference/source.sha256"), DOCUMENT_LIMIT)?,
            &read(&path.join("reference/recompute.txt"), DOCUMENT_LIMIT)?,
            &reference_source,
        )?;
        if binding["reference_relation_sha256"].as_str() != Some(sha(&relation_bytes).as_str())
            || binding["reference_source_sha256"].as_str() != Some(sha(&reference_source).as_str())
            || reference_source
                != read(
                    &files::repository()?.join("tools/priority_reference.py"),
                    SOURCE_LIMIT,
                )?
        {
            return fail(65, "bundle-reference-pin");
        }
        let (project, policy) = relation.abstract_source()?;
        if project.as_bytes() != read(&path.join("abstract/project.zeno"), SOURCE_LIMIT)?
            || policy != read(&path.join("abstract/v2/policy.json"), SOURCE_LIMIT)?
        {
            return fail(65, "bundle-abstract-source-differs");
        }
        for layout in [Layout::T, Layout::I] {
            let alpha = layout.alpha(&relation)?;
            let features = Features::new(layout);
            let prefix = path.join(layout.name());
            let pre = features.validity("pre")?;
            let post = features.validity("post")?;
            if super::reference::project(layout.name(), &layout.bounds(), Some((&pre, &post)))
                .as_bytes()
                != read(&prefix.join("project.zeno"), SOURCE_LIMIT)?
            {
                return fail(65, "bundle-fixed-declarations-or-laws");
            }
            if canonical(&alpha)? != read(&prefix.join("alpha.json"), SOURCE_LIMIT)?
                || canonical(&features.grammar(&alpha)?)?
                    != read(&prefix.join("grammar.json"), DOCUMENT_LIMIT)?
            {
                return fail(65, "bundle-grammar-or-codec-pin");
            }
        }
        for (name, bytes) in &f51.design {
            if *bytes != read(&path.join("design").join(name), SOURCE_LIMIT)? {
                return fail(65, "bundle-design-pin");
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            relation,
            abstract_project: project,
            abstract_policy: String::from_utf8(policy).map_err(|e| malformed(e.to_string()))?,
            f51,
            binding,
        })
    }
}

fn prepare(context: &mut Context, reference: &Path, f51: &Path) -> Result<&'static str> {
    let qualified = context.f51(f51)?;
    let relation_bytes = context.input(reference, SOURCE_LIMIT)?;
    let reference = reference
        .parent()
        .ok_or_else(|| malformed("reference-parent"))?;
    let report_bytes = context.input(&reference.join("reference-report.json"), DOCUMENT_LIMIT)?;
    let script = read(
        &files::repository()?.join("tools/priority_reference.py"),
        SOURCE_LIMIT,
    )?;
    let source_pin = context.input(&reference.join("source.sha256"), DOCUMENT_LIMIT)?;
    let recompute = context.input(&reference.join("recompute.txt"), DOCUMENT_LIMIT)?;
    let relation = super::reference::validate_files(
        &relation_bytes,
        &report_bytes,
        &source_pin,
        &recompute,
        &script,
    )?;
    let report = strict(&report_bytes)?;
    for (name, bytes) in [
        ("relation.json", relation_bytes),
        ("reference-report.json", report_bytes),
        ("source.sha256", source_pin),
        ("recompute.txt", recompute),
        ("priority_reference.py", script.clone()),
    ] {
        write(&context.out.join("reference").join(name), &bytes)?;
    }
    let (project, policy) = relation.abstract_source()?;
    write(
        &context.out.join("abstract/project.zeno"),
        project.as_bytes(),
    )?;
    write(&context.out.join("abstract/v2/policy.json"), &policy)?;
    for layout in [Layout::T, Layout::I] {
        let alpha = layout.alpha(&relation)?;
        let features = Features::new(layout);
        let pre = features.validity("pre")?;
        let post = features.validity("post")?;
        write(
            &context.out.join(layout.name()).join("project.zeno"),
            super::reference::project(layout.name(), &layout.bounds(), Some((&pre, &post)))
                .as_bytes(),
        )?;
        write_json(&context.out.join(layout.name()).join("alpha.json"), &alpha)?;
        write_json(
            &context.out.join(layout.name()).join("grammar.json"),
            &features.grammar(&alpha)?,
        )?;
    }
    let mut design_inputs = Vec::new();
    for (name, bytes) in &qualified.design {
        let path = context.out.join("design").join(name);
        write(&path, bytes)?;
        design_inputs.push(files::reference(&context.evidence, &path)?);
    }
    write_json(
        &context.out.join("binding.json"),
        &json!({"schema":"zeno-fcis/priority-bundle/1","revision":REVISION,"f51_evidence":qualified.input,
        "reference_source_sha256":sha(&script),"reference_relation_sha256":report["relation_sha256"],"design_inputs":design_inputs}),
    )?;
    write_json(
        &context.out.join("meaning.json"),
        &json!({"revision":REVISION,"identities":[0,1,2],"priorities":[0,1],"capacity":2,"lifetime_insertions":2,
        "priority_order":"lower-first","tie_order":"earlier-arrival-first","reject_precedence":[201,202,203,204,205],"genesis":0,"carrier":45,"commands":16,"rows":720,
        "effects":"empty","accept_outbox_count":1,"destination":"priority-events","payload_fields":[140,141,142,143],"no_external_delivery_claim":true,
        "minimal_loop":"approved meaning, finite sketch proposals, native refinement feedback, lowering parity, independent replay/review","optional_model":"off","optional_optimization":"not-run"}),
    )?;
    context.detail = json!({"states":45,"commands":16,"rows":720,"observations":29,"candidate_sources":"not-emitted"});
    Ok("prepared")
}

fn preflight(context: &mut Context, path: &Path) -> Result<&'static str> {
    let bundle = Bundle::load(context, path)?;
    let mut counts = native::Counts::default();
    let outcome = native::reference(
        &bundle.relation,
        &bundle.abstract_project,
        &bundle.abstract_policy,
        &mut counts,
    );
    context.add_native(&counts.json())?;
    let report = match &outcome {
        Ok(v) => v.clone(),
        Err(error) => json!({"status":"failed","reason":error.reason,"counts":counts.json()}),
    };
    write_json(&context.out.join("reference-native.json"), &report)?;
    outcome?;
    let mut layouts = Vec::new();
    for layout in [Layout::T, Layout::I] {
        let alpha = layout.alpha(&bundle.relation)?;
        let mut covered = std::collections::BTreeSet::new();
        for state in alpha.iter().flatten() {
            if !covered.insert(*state) {
                return fail(1, "codec-duplicate-representation");
            }
        }
        if covered.len() != 45
            || alpha.iter().filter(|v| v.is_none()).count()
                != if layout == Layout::T { 102 } else { 330 }
        {
            return fail(1, "codec-coverage-or-null-set");
        }
        for (a, state) in bundle.relation.states.iter().enumerate() {
            let raw = layout.encode(state)?;
            if layout.decode(&raw).as_ref() != Some(state)
                || layout.ordinal(&raw).and_then(|s| alpha[s]) != Some(a)
            {
                return fail(1, "codec-roundtrip");
            }
        }
        let features = Features::new(layout);
        let checked = features.check_features(&bundle.relation)?;
        let grammar = canonical(&features.grammar(&alpha)?)?;
        if grammar.len() > DOCUMENT_LIMIT {
            return fail(1, "grammar-size-limit");
        }
        layouts.push(json!({"layout":layout.name(),"raw_states":layout.raw_count(),"valid_states":45,"null_states":alpha.len()-45,"features":checked,"grammar_bytes":grammar.len(),
            "candidate_bound":"unmeasured-until-materialization"}));
    }
    context.detail = json!({"reference":report,"layouts":layouts,"checker_limits":"unchanged","construction_feasibility":"not-yet-qualified"});
    Ok("bounds-ready")
}

pub(super) fn execute(phase: Phase) -> i32 {
    let name = phase.name();
    let mut context = match Context::new(&phase) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{}",
                json!({"phase":name,"status":"failed","stop_reason":e.reason,"exit":e.code})
            );
            return e.code;
        }
    };
    let result = context.initialize().and_then(|()| match &phase {
        Phase::Prepare {
            reference,
            f51_evidence,
            ..
        } => prepare(&mut context, reference, f51_evidence),
        Phase::Preflight { bundle, .. } => preflight(&mut context, bundle),
        Phase::Search {
            bundle,
            owner_approval,
            layout,
            cli,
            ..
        } => super::search::search(&mut context, bundle, owner_approval, layout, cli),
        Phase::Replay {
            bundle, run, cli, ..
        } => super::search::replay(&mut context, bundle, run, cli),
        Phase::Controls {
            bundle,
            t_run,
            i_run,
            cli,
            ..
        } => super::controls::controls(&mut context, bundle, t_run, i_run, cli),
        Phase::Summarize {
            bundle,
            t_run,
            i_run,
            replays,
            controls,
            meta_log,
            review,
            ..
        } => super::summary::summarize(
            &mut context,
            bundle,
            [t_run, i_run],
            replays,
            controls,
            meta_log,
            review.as_deref(),
        ),
    });
    let (status, code, reason) = match result {
        Ok(status) => (
            status,
            if status == "awaiting-review" { 2 } else { 0 },
            if status == "awaiting-review" {
                json!("independent-review-required")
            } else {
                Value::Null
            },
        ),
        Err(error) => (
            match error.code {
                2 => "incomplete",
                65 => "malformed",
                3 => "transport-failure",
                _ => "failed",
            },
            error.code,
            json!(error.reason),
        ),
    };
    context
        .inputs
        .sort_by_key(|v| v["path"].as_str().unwrap_or_default().to_string());
    let report = json!({"schema":"zeno-fcis/priority-construction-run/1","revision":REVISION,"phase":name,"status":status,"stop_reason":reason,
        "inputs":context.inputs,"identities":context.identities,"counts":context.counts,"detail":context.detail,"elapsed_ms":context.started.elapsed().as_millis().to_string(),
        "evidence_paths":files::manifest(&context.out).ok().map(|v|v["files"].clone())});
    let report_name = if name == "search" {
        "run.json"
    } else if name == "summarize" {
        "summary.json"
    } else {
        match name {
            "prepare" => "prepare.json",
            "preflight" => "preflight.json",
            "replay" => "replay.json",
            _ => "controls.json",
        }
    };
    if name == "summarize" && !context.out.join("REPORT.md").exists()
        && let Err(error)=write(&context.out.join("REPORT.md"),format!("# Finite priority construction pilot\n\nStatus: {status}.\n\nStop reason: {reason}.\n\nNo overall qualification is claimed. The complete phase report retains all readable supplied runs, measured work and failed or absent downstream stages.\n").as_bytes()){eprintln!("{}",error.reason);return error.code;
    }
    if let Err(error) = write_json(&context.out.join(report_name), &report)
        .and_then(|()| files::seal(&context.out).map(|_| ()))
    {
        eprintln!("{}", error.reason);
        return error.code;
    }
    println!(
        "{}",
        String::from_utf8_lossy(&canonical(&report).unwrap_or_else(|_| b"{}\n".to_vec()))
    );
    code
}
