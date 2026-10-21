//! Required private driver scenarios; no ignored cases.
use super::{
    Arguments, common, compilation, files, layout, native, phases, proposal, qualification,
    reference, search,
};
use clap::Parser;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn driver() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("ZENO_FCIS_CONSTRUCTION_DRIVER")
            .unwrap_or_else(|| panic!("ZENO_FCIS_CONSTRUCTION_DRIVER is required; no skip")),
    );
    assert!(path.is_file(), "actual driver required: {}", path.display());
    path
}
fn test_root(name: &str) -> PathBuf {
    driver();
    let base = PathBuf::from(
        std::env::var_os("E").unwrap_or_else(|| panic!("E evidence directory is required")),
    )
    .join("construction/driver-tests");
    std::fs::create_dir_all(&base)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let root = base.join(format!(
        "{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    root
}
fn passed(name: &str, root: &Path, detail: Value) {
    common::write_json(
        &root.join("scenario.json"),
        &json!({"scenario":name,"status":"passed","detail":detail}),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
}
fn rejected<T>(result: common::Result<T>) -> common::Failure {
    match result {
        Err(e) => e,
        Ok(_) => panic!("required refusal was not produced"),
    }
}
fn fixture(root: &Path) -> (PathBuf, reference::Relation) {
    let path = root.join("reference");
    let result = std::process::Command::new("python3")
        .arg(
            files::repository()
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
                .join("tools/priority_reference.py"),
        )
        .arg("--out")
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    common::write(&path.join("recompute.txt"), &result.stdout)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let relation = reference::validate_files(
        &common::read(&path.join("relation.json"), common::SOURCE_LIMIT)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        &common::read(&path.join("reference-report.json"), common::DOCUMENT_LIMIT)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        &common::read(&path.join("source.sha256"), common::DOCUMENT_LIMIT)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        &result.stdout,
        &common::read(
            &files::repository()
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
                .join("tools/priority_reference.py"),
            common::SOURCE_LIMIT,
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    (path, relation)
}

#[test]
fn literal_reference_compiles_and_matches_actual_authority() {
    let path = test_root("native-reference");
    let (_, relation) = fixture(&path);
    let (project, policy) = relation
        .abstract_source()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    common::write(&path.join("abstract/project.zeno"), project.as_bytes())
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    common::write(&path.join("abstract/v2/policy.json"), &policy)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut counts = native::Counts::default();
    let result = native::reference(
        &relation,
        &project,
        std::str::from_utf8(&policy)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        &mut counts,
    );
    let report = match &result {
        Ok(value) => value.clone(),
        Err(error) => {
            serde_json::json!({"status":"failed","exit":error.code,"reason":error.reason,"counts":counts.json()})
        }
    };
    common::write_json(&path.join("native.json"), &report)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    println!("{}", serde_json::json!({"evidence":path,"result":report}));
    let report = result.unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(report["status"], "reference-agrees");
    assert_eq!(counts.calls, 721);
    assert_eq!(counts.rows, 720);
    for name in ["T", "I"] {
        let layout = layout::Layout::parse(name)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let alpha = layout
            .alpha(&relation)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(alpha.iter().flatten().count(), 45);
        let features = layout::Features::new(layout);
        assert_eq!(features.names.len(), if name == "T" { 22 } else { 26 });
        let grammar = features
            .grammar(&alpha)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        common::write_json(&path.join(format!("{name}.grammar.json")), &grammar)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        for (i, state) in relation.states.iter().enumerate() {
            let raw = layout
                .encode(state)
                .unwrap_or_else(|error| panic!("required test result: {error:?}"));
            assert_eq!(
                alpha[layout
                    .ordinal(&raw)
                    .unwrap_or_else(|| panic!("required test value missing"))],
                Some(i)
            );
        }
        let checked = features
            .check_features(&relation)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(checked["raw_inputs"], (layout.raw_count() * 16) as u64);
        common::write_json(&path.join(format!("{name}.features.json")), &checked)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    }
    passed("native-reference", &path, report);
}

#[test]
fn reference_pins_duplicate_missing_rows_and_numeric_kinds_are_strict() {
    let root = test_root("reference-intake");
    let (path, relation) = fixture(&root);
    let data = common::read(&path.join("relation.json"), common::SOURCE_LIMIT)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let report = common::read(&path.join("reference-report.json"), common::DOCUMENT_LIMIT)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let pin = common::read(&path.join("source.sha256"), common::DOCUMENT_LIMIT)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let recompute = common::read(&path.join("recompute.txt"), common::DOCUMENT_LIMIT)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let script = common::read(
        &files::repository()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            .join("tools/priority_reference.py"),
        common::SOURCE_LIMIT,
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut wrong_report =
        common::strict(&report).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    wrong_report["relation_sha256"] = json!("0".repeat(64));
    assert_eq!(
        rejected(reference::validate_files(
            &data,
            &common::canonical(&wrong_report)
                .unwrap_or_else(|error| panic!("required test result: {error:?}")),
            &pin,
            &recompute,
            &script
        ))
        .code,
        65
    );
    assert_eq!(
        rejected(reference::validate_files(
            &data, &report, b"wrong\n", &recompute, &script
        ))
        .reason,
        "reference-source-pin"
    );
    for kind in 0..5 {
        let mut value = relation.value.clone();
        match kind {
            0 => {
                value["rows"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("required test value missing"))
                    .pop();
            }
            1 => value["rows"][1] = value["rows"][0].clone(),
            2 => value["rows"][0]["state"] = json!(false),
            3 => value["rows"][0]["state"] = json!(0.0),
            _ => value["rows"][0]["unknown"] = json!(0),
        }
        assert_eq!(
            rejected(reference::Relation::read(
                &common::canonical(&value)
                    .unwrap_or_else(|error| panic!("required test result: {error:?}"))
            ))
            .code,
            65
        );
    }
    let duplicate = String::from_utf8(data.clone())
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
        .replacen('{', "{\"genesis\":0,", 1);
    assert_eq!(
        rejected(reference::Relation::read(duplicate.as_bytes())).code,
        65
    );
    passed("reference-intake", &root, json!({"malformed_controls":8}));
}

#[test]
fn frozen_manifest_pins_and_source_overwrite_are_enforced() {
    let root = test_root("manifest-and-overwrite");
    let bundle = root.join("bundle");
    common::write(&bundle.join("fixed.txt"), b"before")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    files::seal(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    files::verify(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        rejected(common::write(&bundle.join("fixed.txt"), b"after")).code,
        3
    );
    assert_eq!(
        common::read(&bundle.join("fixed.txt"), 10)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        b"before"
    );
    std::fs::write(bundle.join("fixed.txt"), b"tampered")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(rejected(files::verify(&bundle)).code, 65);
    let before =
        files::manifest(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let result = std::process::Command::new(driver())
        .args(["prepare", "--reference"])
        .arg(root.join("missing-reference"))
        .arg("--f51-evidence")
        .arg(root.join("missing-f51"))
        .arg("--out")
        .arg(&bundle)
        .output()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(
        files::manifest(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}")),
        before
    );
    passed(
        "manifest-and-overwrite",
        &root,
        json!({"existing_output_exit":3,"source_unchanged":true}),
    );
}

#[test]
fn changed_frozen_grammar_is_refused_by_its_bundle_pin() {
    let root = test_root("grammar-pin");
    let (_, relation) = fixture(&root);
    let layout = layout::Layout::T;
    let grammar = layout::Features::new(layout)
        .grammar(
            &layout
                .alpha(&relation)
                .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let bundle = root.join("bundle");
    let file = bundle.join("T/grammar.json");
    common::write_json(&file, &grammar)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    files::seal(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    files::verify(&bundle).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut wrong = grammar;
    wrong["limits"]["max_nodes"] = json!(225);
    std::fs::write(
        &file,
        common::canonical(&wrong).unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        rejected(files::verify(&bundle)).reason,
        "artifact-manifest-mismatch"
    );
    passed(
        "grammar-pin",
        &root,
        json!({"changed_limit_refused":true,"search_started":false}),
    );
}

#[test]
fn missing_and_stale_meaning_approval_are_refused() {
    let root = test_root("meaning-approval");
    let bundle = root.join("bundle");
    common::write(&bundle.join("manifest.json"), b"negative-test-bundle\n")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        rejected(qualification::approval(
            &root,
            &root.join("missing.json"),
            &bundle
        ))
        .code,
        3
    );
    // Deliberately invalid negative input, never an owner decision or approval.
    let bad = json!({"schema":"zeno-fcis/priority-meaning-approval/1","revision":"construction-r2","bundle_sha256":"0".repeat(64),"status":"approved",
        "approver":"owner (recorded 2026-10-16T13:35:19Z)","decision_record":{"path":"missing-decision.md","sha256":"0".repeat(64)}});
    common::write_json(&root.join("invalid-stale-approval.json"), &bad)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let refusal = rejected(qualification::approval(
        &root,
        &root.join("invalid-stale-approval.json"),
        &bundle,
    ));
    assert_eq!(refusal.code, 65);
    assert_eq!(refusal.reason, "missing-or-stale-meaning-approval");
    passed(
        "meaning-approval",
        &root,
        json!({"missing":3,"stale":65,"accepted":false}),
    );
}

#[test]
fn proposals_have_finite_namespaced_fills_and_cumulative_work() {
    let root = test_root("proposal-grammar");
    let (_, relation) = fixture(&root);
    let coordinates: BTreeSet<_> = (0..16).map(|q| (0, q)).collect();
    for layout in [layout::Layout::T, layout::Layout::I] {
        let p = proposal::propose(&relation, layout, &coordinates)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert!(p.stop.is_none(), "{}", p.document);
        assert!(p.expressions.is_some());
        let outputs = p.document["outputs"]
            .as_array()
            .unwrap_or_else(|| panic!("required test value missing"));
        assert_eq!(
            p.work,
            outputs
                .iter()
                .map(|r| r["work"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("required test value missing")))
                .sum::<u64>()
        );
        assert!(p.work <= proposal::PROPOSAL_WORK);
        assert_eq!(
            outputs
                .iter()
                .map(|r| r["fills"]
                    .as_object()
                    .unwrap_or_else(|| panic!("required test value missing"))
                    .len())
                .sum::<usize>(),
            if layout == layout::Layout::T { 12 } else { 15 }
        );
        assert!(
            proposal::replay_proposal(&relation, layout, &p.document, &p.examples)
                .unwrap_or_else(|error| panic!("required test result: {error:?}"))
                .is_some()
        );
        let mut over = p.document.clone();
        let records = over["outputs"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("required test value missing"));
        for record in records.iter_mut() {
            record["work"] = json!(1);
        }
        records[0]["work"] = json!(proposal::PROPOSAL_WORK);
        let count = records.len();
        over["proposal_work"] = json!(proposal::PROPOSAL_WORK + count as u64 - 1);
        assert_eq!(
            rejected(proposal::replay_proposal(
                &relation,
                layout,
                &over,
                &p.examples
            ))
            .reason,
            "proposal-cumulative-work"
        );
        let mut seed = p.document.clone();
        seed["seed"] = json!({"completed_source":"not permitted"});
        assert_eq!(
            rejected(proposal::replay_proposal(
                &relation,
                layout,
                &seed,
                &p.examples
            ))
            .code,
            65
        );
        let mut fill = p.document.clone();
        fill["outputs"][0]["fills"]["out0/hole0"] = json!(99);
        assert_eq!(
            rejected(proposal::replay_proposal(
                &relation,
                layout,
                &fill,
                &p.examples
            ))
            .code,
            65
        );
        for value in [json!(true), json!(1.0), json!({"ast":"true"})] {
            let mut bad = p.document.clone();
            bad["outputs"][0]["fills"]["out0/hole0"] = value;
            assert_eq!(
                rejected(proposal::replay_proposal(
                    &relation,
                    layout,
                    &bad,
                    &p.examples
                ))
                .code,
                65
            );
        }
        let mut body = p.document.clone();
        body["outputs"][0]["expression"] = json!("5");
        assert_eq!(
            rejected(proposal::replay_proposal(
                &relation,
                layout,
                &body,
                &p.examples
            ))
            .reason,
            "out-of-grammar-expression-or-seed"
        );
        let mut adopted = p.document.clone();
        adopted["adoptions"] = json!(["not permitted"]);
        assert_eq!(
            rejected(proposal::replay_proposal(
                &relation,
                layout,
                &adopted,
                &p.examples
            ))
            .code,
            65
        );
        common::write_json(
            &root.join(format!("{}-proposal.json", layout.name())),
            &p.document,
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    }
    passed(
        "proposal-grammar",
        &root,
        json!({"layouts":2,"cumulative_work_enforced":true,"no_seed_or_adoption":true}),
    );
}

#[test]
fn actual_compiler_node_limit_is_used() {
    let root = test_root("actual-node-limit");
    let (_, relation) = fixture(&root);
    let (project, policy) = relation
        .abstract_source()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let compiled = compilation::compile(
        layout::Layout::T,
        &project,
        std::str::from_utf8(&policy)
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(compiled.inventory["nodes"], 2221);
    assert_eq!(
        compiled.refusal.as_deref(),
        Some("candidate-scalar-admission-limit")
    );
    assert!(compiled.lowered.is_none());
    common::write_json(&root.join("actual-inventory.json"), &compiled.inventory)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    passed(
        "actual-node-limit",
        &root,
        json!({"actual_nodes":2221,"limit":224,"qualified":false}),
    );
}

#[test]
fn failed_subprocess_retains_real_exit_and_both_streams() {
    let root = test_root("failed-subprocess");
    let result=files::process(Path::new("python3"),&["-c".into(),"import sys; print('partial output'); sys.stderr.write('deliberate failure\\n'); sys.exit(7)".into()],&files::repository().unwrap_or_else(|error| panic!("required test result: {error:?}"))).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(result.exit, Some(7));
    result
        .complete()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(result.stdout, b"partial output\n");
    assert_eq!(result.stderr, b"deliberate failure\n");
    let recorded = result
        .save(&root, "failed")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(recorded["exit"], 7);
    // Synthetic stage-ledger unit fixture; these values are not native evidence.
    let mut stages = json!({"proposal":{"status":"proposed","work":31},"source":{"status":"materialized"},"compilation":{"status":"admitted"},"native":{"status":"refines","authority_evaluations":123},"transform":{"status":"running"}});
    search::finish_stages(&mut stages, "failed", &json!("transform-check-failed"))
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(stages["native"]["authority_evaluations"], 123);
    assert_eq!(stages["proposal"]["work"], 31);
    assert_eq!(stages["transform"]["status"], "failed");
    assert_eq!(stages["transform"]["reason"], "transform-check-failed");
    common::write_json(&root.join("failed-attempt-stages.json"), &stages)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut early = json!({"proposal":{"status":"incomplete","work":17},"source":{"status":"not-run"},"compilation":{"status":"not-run"},"native":{"status":"not-run"},"transform":{"status":"not-run"}});
    search::finish_stages(&mut early, "incomplete", &json!("proposal-budget"))
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    for stage in ["source", "compilation", "native", "transform"] {
        assert_eq!(early[stage]["reason"]["stop_reason"], "proposal-budget");
    }
    common::write_json(&root.join("incomplete-attempt-stages.json"), &early)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    passed("failed-subprocess", &root, recorded);
}

#[test]
fn oversized_input_and_child_output_never_count_as_complete() {
    let root = test_root("bounded-intake");
    common::write(
        &root.join("oversized.json"),
        &vec![b'x'; common::DOCUMENT_LIMIT + 1],
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        rejected(common::read(
            &root.join("oversized.json"),
            common::DOCUMENT_LIMIT
        ))
        .code,
        65
    );
    let result = files::process(
        Path::new("python3"),
        &[
            "-c".into(),
            format!(
                "import sys; sys.stdout.write('x'*{})",
                common::LOG_LIMIT + 1
            ),
        ],
        &files::repository().unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert!(result.truncated);
    assert_eq!(result.stdout.len(), common::LOG_LIMIT);
    assert_eq!(rejected(result.complete()).code, 3);
    result
        .save(&root, "truncated")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    passed(
        "bounded-intake",
        &root,
        json!({"retained_prefix":common::LOG_LIMIT,"truncated":true,"complete":false}),
    );
}

#[test]
fn incomplete_phase_has_a_complete_failed_report() {
    let root = test_root("failed-phase-report");
    let out = root.join("out");
    let direct_out = root.join("direct-out");
    let parsed = Arguments::try_parse_from([
        "construction_pilot".to_string(),
        "prepare".into(),
        "--reference".into(),
        root.join("missing-reference")
            .to_string_lossy()
            .into_owned(),
        "--f51-evidence".into(),
        root.join("missing-qualification")
            .to_string_lossy()
            .into_owned(),
        "--out".into(),
        direct_out.to_string_lossy().into_owned(),
    ])
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(phases::execute(parsed.phase), 3);
    files::verify(&direct_out).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let result = std::process::Command::new(driver())
        .arg("prepare")
        .arg("--reference")
        .arg(root.join("missing-reference"))
        .arg("--f51-evidence")
        .arg(root.join("missing-qualification"))
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        result.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    files::verify(&out).unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let report = files::phase_report(&out.join("prepare.json"), "prepare")
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(report["status"], "transport-failure");
    assert_eq!(report["counts"]["authority_evaluations"], 0);
    assert!(!report["stop_reason"].is_null());
    let mut bad = report.clone();
    bad["counts"]["proposal_work"] = json!(true);
    common::write_json(&root.join("bad-report.json"), &bad)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        rejected(files::phase_report(
            &root.join("bad-report.json"),
            "prepare"
        ))
        .code,
        65
    );
    passed(
        "failed-phase-report",
        &root,
        json!({"phase_exit":3,"manifest_complete":true,"native_calls":0}),
    );
}

#[test]
fn repeated_counterexample_stops_without_changing_examples() {
    let root = test_root("repeated-counterexample");
    let (_, relation) = fixture(&root);
    let layout = layout::Layout::T;
    let alpha = layout
        .alpha(&relation)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut examples = BTreeSet::from([(0, 0)]);
    let before = examples.clone();
    let report = json!({"verdict":"counterexample","witness":{"obligation":"observation-refinement","state":0,"input":0}});
    let refusal = rejected(search::feedback(layout, &alpha, &report, &mut examples));
    assert_eq!(refusal.code, 1);
    assert_eq!(
        refusal.reason,
        "integration-mismatch:repeated-counterexample"
    );
    assert_eq!(examples, before);
    let mut invalid = report;
    invalid["witness"]["state"] = json!(7);
    assert_eq!(
        rejected(search::feedback(layout, &alpha, &invalid, &mut examples)).code,
        1
    );
    assert_eq!(examples, before);
    passed(
        "repeated-counterexample",
        &root,
        json!({"stop":"integration-mismatch","examples_unchanged":true}),
    );
}

#[test]
fn stale_f51_is_refused_before_native_work() {
    let root = test_root("stale-f51");
    let evidence = PathBuf::from(
        std::env::var_os("E").unwrap_or_else(|| panic!("required test value missing")),
    );
    let real = common::strict(
        &common::read(
            &evidence.join("f51/qualification.json"),
            common::DOCUMENT_LIMIT,
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let mut stale = real;
    stale["source_manifest_sha256"] = json!("0".repeat(64));
    let path = root.join("invalid-stale-qualification.json");
    common::write_json(&path, &stale)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    common::write(
        &root.join("source.sha256"),
        &files::source_inventory()
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let refusal = rejected(qualification::load(
        &evidence,
        &path,
        &root.join("native-out"),
    ));
    assert_eq!(refusal.code, 65);
    assert_eq!(refusal.reason, "f51-stale-source-manifest");
    assert!(!root.join("native-out/f51-current.json").exists());
    passed(
        "stale-f51",
        &root,
        json!({"refusal":refusal.reason,"native_calls":0}),
    );
}

#[test]
fn no_limit_override_model_or_checker_bypass_is_exposed() {
    let root = test_root("no-bypass");
    for flag in ["--max-work", "--disable-reason", "--model"] {
        let result = std::process::Command::new(driver())
            .args(["search", "--bundle"])
            .arg(root.join("bundle"))
            .arg("--owner-approval")
            .arg(root.join("approval.json"))
            .args(["--layout", "T", "--cli"])
            .arg(driver())
            .arg("--out")
            .arg(root.join("out"))
            .args([flag, "1"])
            .output()
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert_eq!(result.status.code(), Some(65));
        assert!(!root.join("out").exists());
    }
    assert_ne!(
        files::binary_sha(&driver())
            .unwrap_or_else(|error| panic!("required test result: {error:?}")),
        "0".repeat(64)
    );
    passed(
        "no-bypass",
        &root,
        json!({"rejected_flags":3,"native_calls":0}),
    );
}
