//! Growth and provenance fixtures; not native performance or layer-admission evidence.
use crate::knowledge_assessment::*;
use crate::knowledge_growth::{self as G, Episode};
use crate::knowledge_store::{self as S, Layer};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn hash(value: &str) -> String {
    S::hash(&value).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
}
fn scope(family: &str, environment: Option<&str>) -> Scope {
    let mut scope = BTreeMap::from([("family".into(), BTreeSet::from([family.into()]))]);
    if let Some(environment) = environment {
        scope.insert("environment".into(), BTreeSet::from([environment.into()]));
    }
    scope
}
fn case(label: &str, scope: Scope, on_work: u64) -> Case {
    let off = Run {
        task: hash(label),
        seed: 7,
        budget: 1000,
        inputs: hash(&format!("subject-{label}")),
        verdict: Verdict::Accepted,
        work: BTreeMap::from([("check_requests".into(), 10)]),
        result_cost: Some([20, 500]),
        budget_stop: false,
        stopping_bound: None,
        receipt_sha256: hash("receipt"),
        output_sha256: hash(&format!("off-{label}")),
    };
    let mut on = off.clone();
    on.work.insert("check_requests".into(), on_work);
    on.output_sha256 = hash(&format!("on-{label}"));
    Case {
        task: off.task.clone(),
        scope,
        applicable: true,
        off,
        on,
    }
}
fn assessed(layer: &mut Layer, id: &str, scope: Scope, cases: Vec<Case>) {
    let mut candidate: Candidate = serde_json::from_value(
        json!({"id":id,"subject":"pipeline","scope":scope,"effect":"helps",
        "action":{"skip":["canonical"],"order":[]},"proposing_tasks":[],"families":[]}),
    )
    .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    candidate.statement_sha256 = S::statement(&candidate)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    let (decision, reason) = decide(&cases, &BTreeSet::new(), &Policy::default(), &[])
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    let assessment = Assessment {
        schema: "zeno-fcis/kb-assessment/1".into(),
        suite_sha256: hash("suite"),
        revision_sha256: hash("revision"),
        candidates: BTreeSet::from([candidate.statement_sha256.clone()]),
        proposing_tasks: BTreeSet::new(),
        proposing_subjects: BTreeSet::new(),
        total_budget: 100000,
        justifications: BTreeMap::new(),
        engine_sha256: hash("engine"),
        checker_sha256: hash("checker"),
        tool_version: "v1".into(),
        policy: Policy::default(),
        assessment_work: recorded_work(&cases, &[])
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}")),
        cases,
        partial_runs: vec![],
        annotations: vec![],
        decision,
        reason: reason.into(),
        resource_stop: None,
        reserved_work: 100000,
        prior_assessment_work: 0,
        curation_work: 0,
    };
    let key =
        S::hash(&assessment).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    layer.assessments.insert(key.clone(), assessment);
    candidate.assessment = Some(key);
    candidate.state = if decision == Decision::Promote {
        State::Promoted
    } else {
        State::Rejected
    };
    layer.candidates.push(candidate);
}
#[test]
fn generalization_carries_parents_and_reserves_all_parent_cases_for_proposing() {
    let mut layer = Layer::default();
    layer.isa = BTreeSet::from([
        ("left".into(), "class".into()),
        ("right".into(), "class".into()),
    ]);
    for family in ["left", "right"] {
        assessed(
            &mut layer,
            family,
            scope(family, None),
            vec![case(family, scope(family, None), 5)],
        );
    }
    let grown =
        G::grow(&layer).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(grown.len(), 1);
    let proposal = &grown[0];
    assert_eq!(proposal.scope, scope("class", None));
    assert_eq!(proposal.state, State::Proposed);
    assert!(proposal.assessment.is_none());
    assert_eq!(
        proposal.parents,
        layer
            .candidates
            .iter()
            .map(|c| c.statement_sha256.clone())
            .collect()
    );
    for family in ["left", "right"] {
        assert!(proposal.proposing_tasks.contains(&hash(family)));
        assert!(
            proposal
                .families
                .iter()
                .any(|f| f.subject == hash(&format!("subject-{family}")))
        );
    }
}
#[test]
fn generalization_requires_promoted_compatible_siblings() {
    let mut layer = Layer::default();
    layer.isa = BTreeSet::from([
        ("left".into(), "class".into()),
        ("right".into(), "class".into()),
    ]);
    for family in ["left", "right"] {
        assessed(
            &mut layer,
            family,
            scope(family, None),
            vec![case(family, scope(family, None), 5)],
        );
    }
    layer.candidates[1].state = State::Proposed;
    assert!(
        G::grow(&layer)
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
            .is_empty()
    );
    layer.candidates[1].state = State::Promoted;
    layer.candidates[1]
        .scope
        .insert("environment".into(), BTreeSet::from(["different".into()]));
    assert!(
        G::grow(&layer)
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
            .is_empty()
    );
}
#[test]
fn specialization_uses_a_declared_dimension_and_retains_both_signs_as_proposing() {
    let mut layer = Layer::default();
    assessed(
        &mut layer,
        "mixed",
        scope("counter", None),
        vec![
            case("positive", scope("counter", Some("fast")), 5),
            case("negative", scope("counter", Some("slow")), 15),
        ],
    );
    let grown =
        G::grow(&layer).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(grown.len(), 1);
    let proposal = &grown[0];
    assert_eq!(proposal.scope, scope("counter", Some("fast")));
    assert_eq!(proposal.state, State::Proposed);
    assert!(proposal.assessment.is_none());
    assert_eq!(
        proposal.parents,
        BTreeSet::from([layer.candidates[0].statement_sha256.clone()])
    );
    assert_eq!(
        proposal.proposing_tasks,
        BTreeSet::from([hash("positive"), hash("negative")])
    );
}
#[test]
fn repeated_failure_requires_distinct_subjects_and_never_becomes_advice() {
    let episode = |subject: &str| -> Episode {
        serde_json::from_value(json!({"schema":"zeno-fcis/kb-episode/1","task":hash(subject),"subject":hash(subject),
        "scope":{"family":["counter"]},"door":"pipeline","steps":[],"advice":[],"counterexamples":[],"stopping_bound":"stage-budget","verdict":"inconclusive",
        "receipt_sha256":hash("receipt"),"work":{"control_frames":62},"checker_sha256":hash("checker"),"engine_sha256":hash("engine"),
        "family":{"subject":hash(subject),"session":"session","actor":"contributor","tool":"v1","environment":"sandbox"}})).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
    };
    let first = episode("first");
    let mut episodes = BTreeMap::from([(
        S::hash(&first).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}")),
        first.clone(),
    )]);
    episodes.insert(
        S::hash(&first).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}")),
        first,
    );
    assert!(
        G::mine(&episodes)
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
            .is_empty()
    );
    let second = episode("second");
    episodes.insert(
        S::hash(&second).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}")),
        second,
    );
    let mined =
        G::mine(&episodes).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(mined.len(), 1);
    assert_eq!(mined[0].state, State::Proposed);
    assert_eq!(mined[0].families.len(), 2);
    assert!(mined[0].assessment.is_none());
    assert_eq!(
        mined[0].proposing_tasks,
        BTreeSet::from([hash("first"), hash("second")])
    );
}

#[test]
fn unpublished_work_survives_a_revision_race_without_layer_authority() {
    let root = std::env::temp_dir().join(format!("kb-unpublished-{}", std::process::id()));
    std::fs::create_dir(&root)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    }
    let path = root.join("layer.json");
    let original = Layer::default();
    S::commit(&path, None, &original)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    let before =
        std::fs::read(&path).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    let mut next = original.clone();
    assessed(
        &mut next,
        "candidate",
        scope("counter", None),
        vec![case("positive", scope("counter", None), 5)],
    );
    let key = next
        .assessments
        .keys()
        .next()
        .unwrap_or_else(|| panic!("expected knowledge fixture value"))
        .clone();
    let value =
        crate::knowledge_workflow::publish_assessment(&path, &hash("wrong-revision"), &next, &key)
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(value["status"], "refused");
    assert_eq!(value["publication"], "unconfirmed");
    assert_eq!(value["reason"], "kb-revision-conflict");
    assert_eq!(
        std::fs::read(&path).unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}")),
        before
    );
    assert_eq!(value["assessment"]["assessment_work"], 62);
    let recovered = std::path::PathBuf::from(
        value["recovery"]["path"]
            .as_str()
            .unwrap_or_else(|| panic!("expected knowledge fixture value")),
    );
    let bytes = std::fs::read(&recovered)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(
        crate::transform::sha256_hex(&bytes),
        value["recovery"]["sha256"]
    );
    let restored: Layer = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(restored, next);
    assert!(S::load(&recovered).is_err());
    assert_eq!(value["recovery"]["is_layer_descriptor"], false);
    std::fs::remove_dir_all(root)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
}

#[test]
fn a_partial_program_proof_keeps_its_receipt_without_claiming_a_solve() {
    let bytes =
        include_bytes!("../tests/fixtures/i128-door-programs/bounded-counter-c1.original.zcve");
    let mut output = crate::loop_command::doors::assessment_certificate(bytes, bytes)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    output["detail"]["status"] = json!("inconclusive");
    output["detail"]["reason"] = json!("gate-budget");
    let run = crate::knowledge_replay::measure(
        &output,
        hash("task"),
        7,
        1000,
        crate::transform::sha256_hex(bytes),
    )
    .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(run.verdict, Verdict::Inconclusive);
    assert!(run.budget_stop);
    assert!(run.result_cost.is_none());
    assert_eq!(
        run.receipt_sha256,
        output["detail"]["best_check"]["receipt_sha256"]
    );
    G::replay_proof(&output, &run)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
}
#[test]
fn artifact_refusal_keeps_measured_work_and_never_qualifies_advice() {
    let mut layer = Layer::default();
    assessed(
        &mut layer,
        "candidate",
        scope("counter", None),
        vec![case("positive", scope("counter", None), 5)],
    );
    let assessment = layer
        .assessments
        .values()
        .next()
        .unwrap_or_else(|| panic!("expected knowledge fixture value"))
        .clone();
    // The model deliberately omits its suite/artifacts; it cannot supply evidence.
    layer.assessments.clear();
    let (next, record, key) = crate::knowledge_replay::record_artifacts(layer, assessment)
        .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"));
    assert_eq!(record.assessment_work, 62);
    assert_eq!(record.decision, Decision::Inconclusive);
    assert_eq!(record.resource_stop.as_deref(), Some("artifact-validation"));
    assert!(next.facts.is_empty());
    assert_eq!(next.assessments[&key], record);
}

#[test]
fn specialization_must_exclude_a_harmful_capability_superset() {
    let mut layer = Layer::default();
    let mut positive = scope("counter", None);
    positive.insert("capability".into(), BTreeSet::from(["a".into()]));
    let mut negative = positive.clone();
    negative
        .get_mut("capability")
        .unwrap_or_else(|| panic!("expected knowledge fixture value"))
        .insert("b".into());
    assessed(
        &mut layer,
        "mixed",
        scope("counter", None),
        vec![
            case("positive", positive, 5),
            case("negative", negative, 15),
        ],
    );
    assert!(
        G::grow(&layer)
            .unwrap_or_else(|error| panic!("knowledge fixture failed: {error:?}"))
            .is_empty(),
        "a narrower proposal must exclude every known harmful case"
    );
}
