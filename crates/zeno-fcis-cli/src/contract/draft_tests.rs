//! Focused state/observation regressions; native execution is a separate gate.
use super::draft::*;
use super::review::{ReviewSources, draft_examples};
use serde_json::{Value, json};

const PROJECT: &str = include_str!("../../templates/durable-counter/project.zeno");
const RULES: &str = include_str!("../../templates/durable-counter/v2/policy.json");
const EXAMPLES: &str = include_str!("../../templates/durable-counter/tests/decision-examples.txt");
fn start(examples: &str, rounds: u8) -> Draft {
    Draft::start(
        "Bounded counters; denial before capacity".to_owned(),
        PROJECT.to_owned(),
        examples.to_owned(),
        "Retained fixture provenance in source examples; no live owner".to_owned(),
        rounds,
        128,
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
}
fn propose(draft: &mut Draft, rules: &str) {
    draft
        .reserve(rules.to_owned(), "test proposal".to_owned())
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let result = draft
        .assess()
        .map(|a| a.report)
        .unwrap_or_else(|e| json!({"refused":e}));
    draft
        .record_assessment(result)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
}

#[test]
fn draft_no_labels_cannot_be_ready_and_suggestions_are_not_labels() {
    let mut draft = start("", 2);
    propose(&mut draft, RULES);
    let assessment = draft
        .assess()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(!assessment.ready);
    assert!(
        assessment.report["labels_compared"]
            .as_array()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .is_empty()
    );
    assert!(!assessment.questions.is_empty());
}

#[test]
fn draft_invalid_and_interrupted_attempts_consume_the_budget() {
    let mut draft = start(EXAMPLES, 1);
    draft
        .reserve(
            "invalid".to_owned(),
            "malformed supplied proposal".to_owned(),
        )
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(draft.completed().is_err());
    assert!(draft.assess().is_err());
    let saved = draft
        .bytes()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let mut restored: Draft = serde_json::from_slice(&saved)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(restored.proposals.len(), 1);
    assert!(
        restored
            .reserve(RULES.to_owned(), "retry".to_owned())
            .err()
            .unwrap_or_else(|| panic!("expected operation to be refused"))
            .contains("exhausted")
    );
    assert!(restored.completed().is_err());
}

#[test]
fn draft_stale_labels_are_refused_and_roundtrip_preserves_revision() {
    let mut draft = start(EXAMPLES, 2);
    let stale = draft
        .revision()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    propose(&mut draft, RULES);
    let before = draft
        .bytes()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(
        draft
            .add_labels(
                &stale,
                "0 0 120 1 | accept - 1 0 | 300 1 0".to_owned(),
                "stale supplier".to_owned()
            )
            .err()
            .unwrap_or_else(|| panic!("expected operation to be refused"))
            .contains("stale")
    );
    assert_eq!(
        before,
        draft
            .bytes()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
    );
    let restored: Draft = serde_json::from_slice(&before)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(
        draft
            .revision()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
        restored
            .revision()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
    );
    assert_eq!(
        draft
            .assess()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
            .report,
        restored
            .assess()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
            .report
    );
}

#[test]
fn draft_compares_reason_poststate_and_delivery_observations() {
    for line in [
        "0 0 120 0 | reject 201 0 0 | -",     // wrong reason
        "0 0 120 1 | accept - 2 0 | 300 1 0", // wrong post
        "0 0 120 1 | accept - 1 0 | -",       // missing delivery
        "0 0 120 1 | accept - 1 0 | 300 2 0", // wrong payload
        "0 0 121 1 | accept - 0 1 | 300 0 1", // wrong class/reason
    ] {
        let (_, labels) = draft_examples(ReviewSources {
            project: PROJECT,
            rules: RULES,
            examples: Some(line),
        })
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        assert!(labels[0].difference.is_some(), "{line}");
    }
    let (_, labels) = draft_examples(ReviewSources {
        project: PROJECT,
        rules: RULES,
        examples: Some(EXAMPLES),
    })
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(labels.iter().all(|l| l.difference.is_none()));
}

#[test]
fn draft_rechecks_prior_labels_and_rejects_schema_changes() {
    let mut draft = start(EXAMPLES, 3);
    propose(&mut draft, RULES);
    let mut wrong: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    // Swap both declared reasons so the mutant remains an admissible contract.
    wrong["cases"][0]["reason"] = json!(201);
    wrong["cases"][1]["reason"] = json!(200);
    propose(&mut draft, &wrong.to_string());
    let assessed = draft
        .assess()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(!assessed.ready);
    assert!(
        assessed.report["labels_compared"]
            .as_array()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .iter()
            .any(|l| !l["difference"].is_null())
    );
    let mut changed: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    changed["leaf_bindings"]["103"][2] = json!(33);
    propose(&mut draft, &changed.to_string());
    // Erasing cached assessment metadata cannot unfreeze the source-derived shape.
    draft.proposals[0].assessment = None;
    assert!(
        draft
            .assess()
            .err()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .contains("schema/input order")
    );
}

#[test]
fn draft_label_byte_limit_includes_separator_and_refusal_preserves_transcript() {
    let mut draft = start("", 2);
    propose(&mut draft, RULES);
    let assessment = draft
        .assess()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    // A simulated test label from this retained original fixture, not owner input.
    let example = assessment.report["questions"][0]["proposed_example"]
        .as_str()
        .unwrap_or_else(|| panic!("required fixture value is missing"));
    let prefix = format!("{example}\n#");
    let admitted = MAX_TEXT - draft.examples().len() - 1;
    let exact = format!("{prefix}{}", "x".repeat(admitted - prefix.len()));
    let too_large = format!("{exact}x");
    let revision = draft
        .revision()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let before = draft
        .bytes()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(
        draft
            .add_labels(&revision, too_large, "byte-limit fixture".to_owned())
            .err()
            .unwrap_or_else(|| panic!("expected operation to be refused"))
            .contains("combined examples")
    );
    assert_eq!(
        draft
            .bytes()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
        before
    );
    draft
        .add_labels(&revision, exact, "byte-limit fixture".to_owned())
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(draft.examples().len(), MAX_TEXT);
    let restored: Draft = serde_json::from_slice(
        &draft
            .bytes()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert!(restored.assess().is_ok());
}
