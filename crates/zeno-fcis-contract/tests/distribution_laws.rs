//! Finite distribution checks and named mutation-detection obligations.

use zeno_fcis_contract::distribution::{Limits, PREMISE, Verdict, check};

const REPORTER: &str =
    include_str!("../../../examples/distribution-laws/reporter-choice/project.zeno");
const POLICY: &str =
    include_str!("../../../examples/distribution-laws/reporter-choice/v2/policy.json");
const BIAS: &str = include_str!("../../../examples/distribution-laws/modulo-bias/project.zeno");
const BIAS_POLICY: &str =
    include_str!("../../../examples/distribution-laws/modulo-bias/v2/policy.json");
const PAYOUT: &str = include_str!("../../../examples/distribution-laws/payout-tie/project.zeno");
const PAYOUT_POLICY: &str =
    include_str!("../../../examples/distribution-laws/payout-tie/v2/policy.json");
// Review witness: r == 3 commits a failure that writes x = 3; other r accept x = r.
const FAILURE: &str = include_str!("test-data/distribution/committed-failure/project.zeno");
const FAILURE_POLICY: &str =
    include_str!("test-data/distribution/committed-failure/v2/policy.json");

fn mass(kind: &str, numerator: &str, denominator: &str) -> serde_json::Value {
    serde_json::json!({"kind": kind, "numerator": numerator, "denominator": denominator})
}

#[test]
fn distribution_examples_check_exactly_with_their_premise() -> Result<(), Box<dyn std::error::Error>>
{
    for (project, policy, tuples, work) in
        [(REPORTER, POLICY, 16, 32), (PAYOUT, PAYOUT_POLICY, 8, 24)]
    {
        let result = check(project, policy, Limits::default())?;
        assert_eq!(result.verdict(), Verdict::Checked, "{:?}", result.packet());
        assert_eq!(result.packet()["premise"], PREMISE);
        assert_eq!(result.packet()["measured"]["tuples"], tuples);
        assert_eq!(result.packet()["measured"]["work"], work);
        assert_eq!(result.packet()["authority"], "none");
        assert_eq!(
            result.packet()["result"]["decision_mass"],
            serde_json::json!([
                mass("Accept", "1", "1"),
                mass("Reject", "0", "1"),
                mass("CommittedFailure", "0", "1")
            ])
        );
    }
    Ok(())
}

#[test]
fn distribution_committed_failure_is_one_distinct_outcome() -> Result<(), Box<dyn std::error::Error>>
{
    // Counting the failure's written value would make this target hold.
    let result = check(FAILURE, FAILURE_POLICY, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{:?}", result.packet());
    let r = &result.packet()["result"];
    assert_eq!(r["non_random_input"][0]["value"], "0");
    assert_eq!(r["actual"][3]["value"], "committed-failure");
    assert_eq!(r["actual"][3]["denominator"], "4");
    assert_eq!(r["witness"]["outcome"], "committed-failure");
    assert_eq!(r["witness"]["random_values"], serde_json::json!([["3"]]));
    assert_eq!(
        r["decision_mass"],
        serde_json::json!([
            mass("Accept", "3", "4"),
            mass("Reject", "0", "1"),
            mass("CommittedFailure", "1", "4")
        ])
    );
    // Declared failure mass, explicitly or as a uniform member, is checked.
    for target in [
        "{0: 1/4, 1: 1/4, 2: 1/4, committed-failure: 1/4}",
        "uniform over {committed-failure, 2, 1, 0}",
    ] {
        let project = FAILURE.replace("uniform over {0, 1, 2, 3}", target);
        let result = check(&project, FAILURE_POLICY, Limits::default())?;
        assert_eq!(result.verdict(), Verdict::Checked, "{:?}", result.packet());
        assert_eq!(result.packet()["measured"]["complete_fibers"], 4);
        assert_eq!(
            result.packet()["laws"][0]["expected"][3]["value"],
            "committed-failure"
        );
        assert_eq!(
            result.packet()["result"]["decision_mass"][2],
            mass("CommittedFailure", "1", "4")
        );
    }
    // Failure mass is not interchangeable with the value the failure writes.
    let wrong = FAILURE.replace(
        "uniform over {0, 1, 2, 3}",
        "{0: 1/4, 1: 1/4, 2: 1/4, 3: 1/8, committed-failure: 1/8}",
    );
    let result = check(&wrong, FAILURE_POLICY, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{:?}", result.packet());
    assert_eq!(
        result.packet()["result"]["witness"]["outcome"],
        "committed-failure"
    );
    Ok(())
}

#[test]
fn distribution_modulo_bias_is_refuted_with_a_minimal_witness()
-> Result<(), Box<dyn std::error::Error>> {
    let result = check(BIAS, BIAS_POLICY, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{:?}", result.packet());
    let r = &result.packet()["result"];
    assert_eq!(r["expected"][0]["numerator"], "1");
    assert_eq!(r["expected"][0]["denominator"], "3");
    assert_eq!(r["actual"][0]["numerator"], "1");
    assert_eq!(r["actual"][0]["denominator"], "2");
    assert_eq!(r["witness"]["size"], 2);
    assert_eq!(
        r["witness"]["random_values"],
        serde_json::json!([["0"], ["3"]])
    );
    // One assignment has mass 1/4 <= 1/3; these two have mass 1/2 > 1/3.
    assert_eq!(result.packet()["measured"]["tuples"], 4);
    Ok(())
}

#[test]
fn distribution_random_width_must_match_the_entire_input_domain()
-> Result<(), Box<dyn std::error::Error>> {
    let wrong = REPORTER.replace("bits 2", "bits 1");
    assert!(
        check(&wrong, POLICY, Limits::default())
            .err()
            .ok_or("random-width mismatch was accepted")?
            .reason()
            .contains("width")
    );
    Ok(())
}

#[test]
fn distribution_weights_must_sum_exactly_to_one() {
    let wrong = PAYOUT.replace("{0: 1/2, 1: 1/2}", "{0: 1/2, 1: 1/3}");
    assert!(check(&wrong, PAYOUT_POLICY, Limits::default()).is_err());
}

#[test]
fn distribution_counts_every_random_value() -> Result<(), Box<dyn std::error::Error>> {
    let result = check(REPORTER, POLICY, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{:?}", result.packet());
    assert_eq!(result.packet()["measured"]["tuples"], 16);
    assert_eq!(result.packet()["measured"]["complete_fibers"], 4);
    Ok(())
}

#[test]
fn distribution_every_verdict_retains_the_randomness_premise()
-> Result<(), Box<dyn std::error::Error>> {
    for (p, r, limits, wanted) in [
        (REPORTER, POLICY, Limits::default(), Verdict::Checked),
        (BIAS, BIAS_POLICY, Limits::default(), Verdict::Refuted),
        (
            REPORTER,
            POLICY,
            Limits {
                max_work: 3,
                ..Limits::default()
            },
            Verdict::Unknown,
        ),
    ] {
        let result = check(p, r, limits)?;
        assert_eq!(result.verdict(), wanted);
        assert_eq!(result.packet()["premise"], PREMISE);
        assert_eq!(result.packet()["random_inputs"][0]["uniform"], true);
        assert_eq!(result.packet()["random_inputs"][0]["independent"], true);
        if wanted != Verdict::Checked {
            assert!(result.packet()["claim"].is_null());
        }
    }
    Ok(())
}

#[test]
fn distribution_singleton_parameter_boundary_includes_zero_bits()
-> Result<(), Box<dyn std::error::Error>> {
    let singleton = REPORTER
        .replace("Reporter in 0..=3", "Reporter in 0..=0")
        .replace("RandomBits in 0..=3", "RandomBits in 0..=0")
        .replace("bits 2", "bits 0")
        .replace("{0, 1, 2, 3}", "{0}");
    let result = check(&singleton, POLICY, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{:?}", result.packet());
    assert_eq!(result.packet()["random_domain_size"], "1");
    assert_eq!(result.packet()["measured"]["tuples"], 1);
    Ok(())
}

#[test]
fn distribution_budgets_never_promote_a_partial_fold() -> Result<(), Box<dyn std::error::Error>> {
    for (limit, tuples, work) in [
        (
            Limits {
                max_tuples: 15,
                ..Limits::default()
            },
            0,
            0,
        ),
        (
            Limits {
                max_work: 31,
                ..Limits::default()
            },
            16,
            31,
        ),
        (
            Limits {
                max_work: 0,
                ..Limits::default()
            },
            0,
            0,
        ),
    ] {
        let result = check(REPORTER, POLICY, limit)?;
        assert_eq!(result.verdict(), Verdict::Unknown);
        assert_eq!(result.packet()["measured"]["tuples"], tuples);
        assert_eq!(result.packet()["measured"]["work"], work);
    }
    assert_eq!(
        check(
            REPORTER,
            POLICY,
            Limits {
                max_work: 32,
                ..Limits::default()
            }
        )?
        .verdict(),
        Verdict::Checked
    );
    Ok(())
}

#[test]
fn distribution_conditions_on_each_non_random_input_tuple() -> Result<(), Box<dyn std::error::Error>>
{
    // Pooled over both command values this is uniform. Each fixed command
    // is deterministic, so the conditional distribution must be refuted.
    let project = PAYOUT.replace("1 - context.102.130", "command.101");
    let policy = PAYOUT_POLICY
        .replace(
            "[\n      \"I128\",\n      0,\n      0\n",
            "[\n      \"I128\",\n      0,\n      1\n",
        )
        .replace("1 - context.102.130", "command.101")
        .replace(
            "\"111\": \"context.102.130\"",
            "\"111\": \"1 - command.101\"",
        );
    let result = check(&project, &policy, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{:?}", result.packet());
    assert_eq!(result.packet()["result"]["witness"]["size"], 2);
    Ok(())
}

#[test]
fn distribution_multiple_random_inputs_form_the_full_product()
-> Result<(), Box<dyn std::error::Error>> {
    let project = REPORTER
        .replace("RandomBits in 0..=3", "RandomBits in 0..=1")
        .replace(
            "field 130 102 entropy 104;",
            "field 130 102 entropy 104;\nfield 131 102 other 104;",
        )
        .replace("bits 2", "bits 1")
        .replace(
            "distribution 601",
            "random 602 other on context.102.131 bits 1 independent uniform;\ndistribution 601",
        );
    let policy = POLICY.replace(
        "\"110\": \"context.102.130\"",
        "\"110\": \"2 * context.102.130 + context.102.131\"",
    );
    let result = check(&project, &policy, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{:?}", result.packet());
    assert_eq!(result.packet()["random_domain_size"], "4");
    assert_eq!(
        result.packet()["random_inputs"]
            .as_array()
            .ok_or("missing random input array")?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn distribution_refusals_and_undefined_projections_are_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    // An ordinary reject is a decision, but has no post-state projection.
    let mut policy: serde_json::Value = serde_json::from_str(POLICY)?;
    policy["cases"][0]["class"] = serde_json::json!("Reject");
    policy["cases"][0]["reason"] = serde_json::json!(200);
    policy["cases"][0]["post"] = serde_json::json!({});
    let result = check(
        &format!("{REPORTER}\nreason 200 denied precedence 0;"),
        &policy.to_string(),
        Limits::default(),
    )?;
    assert_eq!(result.verdict(), Verdict::Unknown);
    assert_eq!(
        result.packet()["result"]["reason"],
        "undefined outcome projection"
    );
    assert_eq!(result.packet()["result"]["decision_kind"], "Reject");
    let refused = POLICY.replace(
        "\"110\": \"context.102.130\"",
        "\"110\": \"context.102.130 + 4\"",
    );
    assert_eq!(
        check(REPORTER, &refused, Limits::default())?.verdict(),
        Verdict::Unknown
    );
    Ok(())
}

#[test]
fn distribution_invalid_declarations_fail_closed_and_results_are_owned()
-> Result<(), Box<dyn std::error::Error>> {
    for project in [
        REPORTER.replace("independent uniform", "uniform"),
        REPORTER.replace("bits 2", "bits 128"),
        REPORTER.replace(
            "distribution 601",
            "random 602 duplicate on context.102.130 bits 2 independent uniform;\ndistribution 601",
        ),
        REPORTER.replace("{0, 1, 2, 3}", "{}"),
        REPORTER.replace("{0, 1, 2, 3}", "{0, 0, 1, 2}"),
        REPORTER.replace("post.100.110 = uniform", "post.100.999 = uniform"),
    ] {
        assert!(check(&project, POLICY, Limits::default()).is_err());
    }
    let mut project = REPORTER.to_owned();
    let result = check(&project, POLICY, Limits::default())?;
    let original = result.packet().clone();
    project.clear();
    let mut detached = result.packet().clone();
    detached["premise"] = serde_json::Value::Null;
    assert_eq!(result.packet(), &original);
    assert_eq!(
        check(REPORTER, POLICY, Limits::default())?.packet(),
        &original
    );
    Ok(())
}
