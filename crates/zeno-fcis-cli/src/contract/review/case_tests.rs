//! Complete-domain case coverage, causal witnesses, and scope guards.

use super::super::{DEFAULT_MAX_TUPLES, ReviewSources, review};
use super::{App, packet};
use serde_json::{Value, json};

fn inserted(when: &str, index: usize) -> App {
    let mut app = App::dual_approval();
    let mut rules: Value = serde_json::from_str(&app.rules).unwrap_or_else(|e| panic!("{e}"));
    rules["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("cases"))
        .insert(
            index,
            json!({
                "rule": "coverage control", "when": when, "class": "Reject", "reason": 200,
                "post": {}, "outbox": [],
            }),
        );
    app.rules = rules.to_string();
    app
}

fn case_findings(packet: &Value) -> Vec<&Value> {
    packet["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("findings"))
        .iter()
        .filter(|finding| {
            finding["kind"]
                .as_str()
                .is_some_and(|k| k.starts_with("case-"))
        })
        .collect()
}

#[test]
fn case_coverage_counts_every_shipped_dual_approval_selection() {
    for typed in [false, true] {
        let mut app = App::dual_approval();
        if typed {
            app.rules = app.rules.replace("policy/2", "policy/4");
        }
        let packet = packet(&app.review());
        assert_eq!(packet["case_coverage"]["status"], "checked");
        let rows = packet["case_coverage"]["cases"]
            .as_array()
            .unwrap_or_else(|| panic!("cases"));
        let counts: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r["decided"].as_u64().unwrap_or(0),
                    r["law_consistent"].as_u64().unwrap_or(0),
                )
            })
            .collect();
        assert_eq!(
            counts,
            [(256, 176), (32, 32), (48, 48), (12, 12), (12, 12), (24, 24)]
        );
        assert!(case_findings(&packet).is_empty());
        assert_eq!(packet["summary"]["case_findings"], 0);
    }
}

#[test]
fn case_coverage_reports_shadowed_case_with_first_earlier_witness() {
    for typed in [false, true] {
        let mut app = inserted("status == 151", 1);
        if typed {
            app.rules = app.rules.replace("policy/2", "policy/4");
        }
        let reviewed = app.review();
        let packet = packet(&reviewed);
        let findings = case_findings(&packet);
        assert_eq!(findings.len(), 1);
        let found = findings[0];
        assert_eq!(found["kind"], "case-never-decides");
        assert_eq!(found["case"], 1);
        assert_eq!(found["rule"], "coverage control");
        assert_eq!(found["guard_true"], 128);
        assert_eq!(found["decided"], 0);
        assert_eq!(found["law_consistent"], 0);
        assert_eq!(found["cause"], "covered by earlier cases");
        assert_eq!(
            found["covered_by"],
            json!([{"case": 0, "when": "status != 150", "witness": {"ordinal": 128, "input": ["151", "160", "160", "140", "160"]}}])
        );
        assert_eq!(
            found["text"],
            "dead case: cases[1] `status == 151` decides no tuple: covered by earlier cases; cases[0] `status != 150` at 151 160 160 140 160"
        );
        assert_eq!(reviewed.summary().case_findings, 1);
    }
}

#[test]
fn case_coverage_reports_a_guard_that_is_never_true() {
    let app = inserted("status == 150 && status != 150", 1);
    let packet = packet(&app.review());
    let findings = case_findings(&packet);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["kind"], "case-never-decides");
    assert_eq!(findings[0]["guard_true"], 0);
    assert_eq!(findings[0]["cause"], "guard never true");
    assert_eq!(findings[0]["covered_by"], json!([]));
    assert_eq!(
        findings[0]["text"],
        "dead case: cases[1] `(status == 150) && (status != 150)` decides no tuple: guard never true"
    );
}

#[test]
fn case_coverage_names_each_actual_earlier_decider_in_a_union() {
    let app = inserted("status != 150 || officer == 160", 2);
    let packet = packet(&app.review());
    let findings = case_findings(&packet);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0]["covered_by"],
        json!([
            {"case": 0, "when": "status != 150", "witness": {"ordinal": 128, "input": ["151", "160", "160", "140", "160"]}},
            {"case": 1, "when": "officer == 160", "witness": {"ordinal": 0, "input": ["150", "160", "160", "140", "160"]}}
        ])
    );
    // A live overlap is not a dead case.
    let app = inserted("status == 150", 1);
    let packet = super::packet(&app.review());
    assert!(packet["case_coverage"]["cases"][1]["decided"].as_u64() > Some(0));
    assert!(!case_findings(&packet).iter().any(|f| f["case"] == 1));
}

#[test]
fn case_coverage_separates_excluded_only_from_globally_dead() {
    let app = inserted("status == 151 && first == 160", 0);
    let packet = packet(&app.review());
    let findings = case_findings(&packet);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["kind"], "case-only-on-excluded-pre-states");
    assert_eq!(findings[0]["case"], 0);
    assert_eq!(findings[0]["decided"], 32);
    assert_eq!(findings[0]["law_consistent"], 0);
    assert_eq!(
        findings[0]["first"],
        json!({"ordinal": 128, "input": ["151", "160", "160", "140", "160"], "unsatisfied_state_law": 500})
    );
}

#[test]
fn case_coverage_does_not_replace_state_laws_with_reachability() {
    // A Cancelled state with a recorded second approval is unreachable from
    // genesis, but law 500 excludes only malformed Approved states.
    let app = inserted("status == 152 && second != 160", 0);
    let packet = packet(&app.review());
    assert_eq!(packet["case_coverage"]["cases"][0]["decided"], 96);
    assert_eq!(packet["case_coverage"]["cases"][0]["law_consistent"], 96);
    assert!(case_findings(&packet).is_empty());
    let reach = super::super::probe::probe(
        &app.project,
        &app.rules,
        &super::super::probe::Question::Reachable {
            state: "post.100.110 == 152 && post.100.112 != 160".into(),
        },
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let super::super::probe::Answer::Answered(answer) = reach else {
        panic!("reachable probe did not complete");
    };
    assert_eq!(answer["answer"], "no");
}

#[test]
fn case_coverage_is_not_checked_for_a_boundary_set() {
    let app = inserted("status == 151", 1);
    let reviewed = review(app.sources(), 383).unwrap_or_else(|e| panic!("{e}"));
    let packet = packet(&reviewed);
    assert_eq!(packet["case_coverage"]["status"], "not checked");
    assert_eq!(packet["case_coverage"]["cases"], Value::Null);
    assert!(case_findings(&packet).is_empty());
}

#[test]
fn case_coverage_does_not_infer_deadness_from_assignment_refusals() {
    let mut app = App::dual_approval();
    // Every accept now violates law 501, while guard selection is unchanged.
    let mut rules: Value = serde_json::from_str(&app.rules).unwrap_or_else(|e| panic!("{e}"));
    rules["cases"][4]["post"]["110"] = json!(152);
    app.rules = rules.to_string();
    let packet = packet(&app.review());
    assert!(packet["summary"]["law_refusal_findings"].as_u64() > Some(0));
    assert!(packet["case_coverage"]["cases"][4]["decided"].as_u64() > Some(0));
    assert!(case_findings(&packet).is_empty());
}

#[test]
fn case_coverage_uses_exact_full_width_typed_inputs() {
    let mut app = App::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/guide-counter"),
    );
    let max = i128::MAX as u128 + 10;
    app.project = app.project.replace(
        "int Units in 0..=2",
        &format!("u128 Units in {}..={max}", max - 2),
    );
    // This test uses the actual declared name, asserted to avoid a vacuous edit.
    assert!(
        app.project.contains("u128 Units"),
        "wide input declaration changed"
    );
    let mut rules: Value = serde_json::from_str(&app.rules).unwrap_or_else(|e| panic!("{e}"));
    rules["schema"] = json!("zeno-fcis/template-declarative-policy/4");
    rules["cases"] = json!([
        {"when": format!("value == u128({max})"), "class": "Reject", "reason": 200, "post": {}, "outbox": []},
        {"when": format!("value == u128({max})"), "class": "Reject", "reason": 200, "post": {}, "outbox": []},
        {"when": true, "class": "Reject", "reason": 201, "post": {}, "outbox": []}
    ]);
    rules["genesis"]["110"] = json!(format!("u128({})", max - 2));
    // Remove state formulas about signed arithmetic, retaining a true state law.
    app.project = app
        .project
        .lines()
        .filter(|line| {
            !line.starts_with("law ")
                && !line.starts_with("claim ")
                && !line.starts_with("reason 202 ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\nlaw 500 wide_state on commit, genesis = post.100.110 >= u128(0);\n";
    rules["law_kinds"] = json!({"500": "StateInvariant"});
    app.rules = rules.to_string();
    let reviewed = review(
        ReviewSources {
            project: &app.project,
            rules: &app.rules,
            examples: None,
        },
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let packet = packet(&reviewed);
    assert_eq!(packet["case_coverage"]["status"], "checked");
    let findings = case_findings(&packet);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["case"], 1);
    assert_eq!(
        findings[0]["covered_by"][0]["witness"]["input"][0],
        max.to_string()
    );
}

#[test]
fn case_coverage_does_not_turn_undefined_guard_arithmetic_into_false() {
    let mut app = App::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/guide-counter"),
    );
    let mut rules: Value = serde_json::from_str(&app.rules).unwrap_or_else(|e| panic!("{e}"));
    rules["cases"][0]["when"] = json!("div_floor(1, value - value) == 0");
    app.rules = rules.to_string();
    let reviewed = review(
        ReviewSources {
            project: &app.project,
            rules: &app.rules,
            examples: None,
        },
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let packet = packet(&reviewed);
    assert_eq!(packet["case_coverage"]["status"], "not checked");
    assert_eq!(packet["case_coverage"]["cases"], Value::Null);
    assert!(case_findings(&packet).is_empty());
    assert!(
        packet["case_coverage"]["reason"]
            .as_str()
            .is_some_and(|s| s.contains("Arithmetic"))
    );
}
