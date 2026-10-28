use super::*;

const INTENT: &str = "An authorized increment increases the counter by exactly one; a decrement decreases it by exactly one. The value stays within 0..=2.";
const MOVEMENT: &str = "(command.101.120 == 150 -> post.100.110 == pre.100.110 + 1) && (command.101.120 == 151 -> post.100.110 + 1 == pre.100.110)";

fn fixture() -> (String, String) {
    // Keep the range/authorization laws; withhold exact movement to model a
    // specification gap. This is an explicit test fixture, not a user edit.
    let project = include_str!("../../../../../../core-components/bounded-counter/project.zeno.in")
        .replace("${C}", "2");
    let project = project
        .lines()
        .filter(|line| !line.starts_with("law 502 "))
        .collect::<Vec<_>>()
        .join("\n");
    let policy = include_str!("../../../../../../core-components/bounded-counter/policy.json.in")
        .replace("${C}", "2")
        .replace("${INSTANCE}", "c2");
    let mut policy: Value = serde_json::from_str(&policy).unwrap_or_else(|e| panic!("{e}"));
    policy["law_kinds"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("object"))
        .remove("502");
    (project, policy.to_string())
}

fn proposals(project: &str, policy: &str, formulas: &[&str], limit: u64) -> String {
    let request = request(project, policy, INTENT, limit).unwrap_or_else(|e| panic!("request {e}"));
    json!({"schema":PROPOSALS_SCHEMA,"request_sha256":request["request_sha256"],"proposer":"test-agent",
        "candidates": formulas.iter().enumerate().map(|(i,f)|json!({"id":format!("p{i}"),"formula":f,"rationale":"counter intent"})).collect::<Vec<_>>()}).to_string()
}

fn mutant(policy: &str) -> String {
    let mut value = strict_json(policy).unwrap_or_else(|e| panic!("{e}"));
    value["cases"][3]["post"]["110"] = json!("value");
    value.to_string()
}

fn report(formulas: &[&str], mutants: bool, limit: u64) -> Value {
    let (project, policy) = fixture();
    check(
        &project,
        &policy,
        INTENT,
        &proposals(&project, &policy, formulas, limit),
        &if mutants {
            vec![mutant(&policy)]
        } else {
            vec![]
        },
        limit,
    )
    .unwrap_or_else(|e| panic!("check: {e}"))
}

#[test]
fn postconditions_strong_candidate_kills_stutter_with_a_replayed_witness() {
    let result = report(&[MOVEMENT], true, 4096);
    let row = &result["candidates"][0];
    assert_eq!(row["status"], "checked-discriminating", "{result}");
    assert_eq!(row["reference"]["tuples"], 12);
    assert_eq!(row["reference"]["admitted"], 4);
    assert_eq!(row["killed"], 1);
    assert_eq!(
        row["mutants"][0]["detail"]["counterexample"]["relation"],
        "fails"
    );
    assert_eq!(result["authority"], "none");
    assert_eq!(result, report(&[MOVEMENT], true, 4096), "not deterministic");
}

#[test]
fn postconditions_truth_is_separate_from_usefulness() {
    let result = report(&["true", "post.100.110 >= 0", MOVEMENT], true, 4096);
    assert_eq!(result["candidates"][0]["status"], "checked-trivial");
    assert_eq!(
        result["candidates"][1]["status"],
        "checked-no-discrimination"
    );
    assert_eq!(result["candidates"][2]["status"], "checked-discriminating");
    assert_eq!(
        report(&[MOVEMENT], false, 4096)["candidates"][0]["status"],
        "checked-no-discrimination"
    );
}

#[test]
fn postconditions_wrong_formula_is_refuted_not_repaired_to_fit() {
    let row = &report(&["post.100.110 == pre.100.110"], true, 4096)["candidates"][0];
    assert_eq!(row["status"], "refuted");
    assert!(row["reference"]["counterexample"].is_object());
    assert_eq!(row["killed"], 0);
}

#[test]
fn postconditions_incomplete_empty_and_undefined_never_pass() {
    assert_eq!(
        report(&[MOVEMENT], true, 1)["candidates"][0]["status"],
        "inconclusive"
    );
    let (project, policy) = fixture();
    let mut reject = strict_json(&policy).unwrap_or_else(|e| panic!("{e}"));
    reject["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("cases"))
        .truncate(3);
    reject["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("cases"))
        .push(json!({"when":"true","class":"Reject","reason":200,"post":{},"outbox":[]}));
    let reject = reject.to_string();
    let batch = proposals(&project, &reject, &[MOVEMENT], 4096);
    let result =
        check(&project, &reject, INTENT, &batch, &[], 4096).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        result["candidates"][0]["reference"]["verdict"], "vacuous",
        "{result}"
    );
    assert_eq!(result["candidates"][0]["status"], "inconclusive");
    let overflow = "post.100.110 * 170141183460469231731687303715884105727 >= 0";
    assert_eq!(
        report(&[overflow], false, 4096)["candidates"][0]["status"],
        "inconclusive"
    );
}

#[test]
fn postconditions_invalid_candidates_do_not_hide_valid_peers() {
    let result = report(
        &[
            "true; law 99 hidden on any = true",
            "post.999 == 1",
            "forall x in 0..=4: true",
            "post.100 == post.100",
            MOVEMENT,
        ],
        true,
        4096,
    );
    for i in 0..4 {
        assert_eq!(result["candidates"][i]["status"], "refused");
    }
    assert_eq!(result["candidates"][4]["status"], "checked-discriminating");
    let duplicates = report(&["post.100.110 >= 0", "(post.100.110 >= 0)"], false, 4096);
    assert_eq!(duplicates["candidates"][1]["status"], "refused");
}

#[test]
fn postconditions_bind_exact_source_intent_and_limits() {
    let (project, policy) = fixture();
    let batch = proposals(&project, &policy, &[MOVEMENT], 4096);
    for (p, r, i, l) in [
        (
            format!("{project}\n"),
            policy.clone(),
            INTENT.to_owned(),
            4096,
        ),
        (
            project.clone(),
            format!("{policy}\n"),
            INTENT.to_owned(),
            4096,
        ),
        (
            project.clone(),
            policy.clone(),
            format!("{INTENT} changed"),
            4096,
        ),
        (project.clone(), policy.clone(), INTENT.to_owned(), 12),
    ] {
        assert!(check(&p, &r, &i, &batch, &[], l).is_err());
    }
}

#[test]
fn postconditions_unknown_fields_duplicates_and_narrowed_domains_refuse() {
    let (project, policy) = fixture();
    let batch = proposals(&project, &policy, &[MOVEMENT], 4096);
    let mut value = strict_json(&batch).unwrap_or_else(|e| panic!("{e}"));
    value["candidates"][0]["precondition"] = json!("pre.100.110 == 0");
    assert!(check(&project, &policy, INTENT, &value.to_string(), &[], 4096).is_err());
    let duplicate = batch.replacen("\"proposer\":", "\"proposer\":\"hidden\",\"proposer\":", 1);
    assert!(check(&project, &policy, INTENT, &duplicate, &[], 4096).is_err());
    let mut bad = strict_json(&mutant(&policy)).unwrap_or_else(|e| panic!("{e}"));
    bad["genesis"]["110"] = json!(1);
    assert!(check(&project, &policy, INTENT, &batch, &[bad.to_string()], 4096).is_err());
    assert!(
        check(
            &project,
            &policy,
            INTENT,
            &batch,
            &[format!("{policy}\n")],
            4096
        )
        .is_err()
    );
    assert!(
        check(
            &project,
            &policy,
            INTENT,
            &batch,
            &[mutant(&policy), mutant(&policy)],
            4096
        )
        .is_err()
    );
}

#[test]
fn postconditions_invalid_and_refusing_mutants_are_not_kills() {
    let (project, policy) = fixture();
    let batch = proposals(&project, &policy, &[MOVEMENT], 4096);
    let mut invalid = strict_json(&mutant(&policy)).unwrap_or_else(|e| panic!("{e}"));
    invalid["cases"][3]["post"]["110"] = json!("missing_variable");
    let result = check(
        &project,
        &policy,
        INTENT,
        &batch,
        &[invalid.to_string()],
        4096,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        result["candidates"][0]["mutants"][0]["status"],
        "invalid-control"
    );
    assert_eq!(result["candidates"][0]["killed"], 0);
    invalid["cases"][3]["post"]["110"] = json!("99");
    let result = check(
        &project,
        &policy,
        INTENT,
        &batch,
        &[invalid.to_string()],
        4096,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(result["candidates"][0]["killed"], 0);
}

#[test]
fn postconditions_limits_and_empty_evidence_refuse() {
    let (project, policy) = fixture();
    assert!(request(&project, &policy, INTENT, 0).is_err());
    assert!(request(&project, &policy, INTENT, 4097).is_err());
    assert!(request(&project, &policy, "", 4096).is_err());
    assert!(
        check(
            &project,
            &policy,
            INTENT,
            &proposals(&project, &policy, &[], 4096),
            &[],
            4096
        )
        .is_err()
    );
    assert!(
        check(
            &project,
            &policy,
            INTENT,
            &proposals(&project, &policy, &[MOVEMENT; 17], 4096),
            &[],
            4096
        )
        .is_err()
    );
    assert_eq!(
        report(&[&"true ".repeat(300)], false, 4096)["candidates"][0]["status"],
        "refused"
    );
}

#[test]
fn postconditions_prompt_uses_intent_and_interface_and_keeps_policy_private() {
    let (project, policy) = fixture();
    let r = request(&project, &policy, INTENT, 4096).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r["prompt"]["intent"], INTENT);
    assert_eq!(r["prompt"]["project"], project);
    assert!(r["prompt"].get("policy").is_none());
    assert_eq!(r["binding"]["policy_sha256"], sha256_hex(policy.as_bytes()));
}

#[test]
fn postconditions_comments_cannot_redirect_the_evaluator_or_abort_valid_peers() {
    for marker in ["outbox.", "verdict.", "delivery."] {
        let bad = format!("post.100.110 >= 0 // {marker}");
        let result = report(&[&bad, MOVEMENT], true, 4096);
        assert_eq!(result["candidates"][0]["status"], "refused");
        assert_eq!(result["candidates"][1]["status"], "checked-discriminating");
    }
}

#[test]
fn postconditions_expansion_bombs_cycles_and_long_expressions_are_bounded() {
    let (project, policy) = fixture();
    let batch = proposals(&project, &policy, &[MOVEMENT], 4096);
    for mode in 0..3 {
        let mut bad = strict_json(&policy).unwrap_or_else(|e| panic!("{e}"));
        if mode == 0 {
            bad["variables"]["v0"] = json!("0");
            for i in 1..30 {
                bad["variables"][format!("v{i}")] = json!(format!("v{} + v{}", i - 1, i - 1));
            }
            bad["cases"][3]["post"]["110"] = json!("v29");
        } else if mode == 1 {
            bad["variables"]["cycle"] = json!("cycle");
        } else {
            bad["cases"][3]["post"]["110"] = json!("1+".repeat(20_000) + "1");
        }
        let bad = bad.to_string();
        assert!(request(&project, &bad, INTENT, 4096).is_err());
        let result = check(&project, &policy, INTENT, &batch, &[bad], 4096)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            result["candidates"][0]["mutants"][0]["status"],
            "invalid-control"
        );
        assert_eq!(result["candidates"][0]["killed"], 0);
    }
}
