//! Discriminating controls at actual judge/admission boundaries.
use super::*;
use crate::contract::review::{gaps, lemma, scenario};
#[path = "../tests/support/arithmetic_judges.rs"]
mod fixture;

fn core(successor: &str, condition: &str) -> Core {
    Core::from_sources(
        fixture::project().to_owned(),
        fixture::policy(successor, condition).to_string(),
    )
    .unwrap()
}

fn law_judges(core: &Core, formula: &str, expected: &str) {
    let proposal = format!("law 700 arithmetic_control on accept = {formula};");
    let result = gaps::judge(
        &core.project,
        &core.rules_text,
        gaps::Options {
            max_tuples: 1000,
            max_work: 1_000_000,
            proposals: Some(&proposal),
            mutants: true,
        },
    )
    .unwrap();
    assert_eq!(
        result.status(),
        if expected == "checked" {
            "measured"
        } else {
            "proposal-false-alarm"
        },
        "{formula}: {result:?}"
    );
    let lemma::Lemmas::Judged(results) =
        lemma::judge_transitions(&core.project, &core.rules_text, &[formula.to_owned()], 1000)
            .unwrap()
    else {
        panic!("small finite relation was inconclusive")
    };
    assert_eq!(results[0]["verdict"], expected, "{formula}: {results:?}");
}

fn actual_admission(core: &Core, formula: &str, amount: i128) -> bool {
    let admitted = core
        .admissible(&[("accept".to_owned(), formula.to_owned())])
        .unwrap();
    let k = core.names.iter().position(|name| name == "amount").unwrap();
    let ordinal = (0..core.tuples())
        .find(|o| core.columns[k][*o] == amount)
        .unwrap();
    let d = core.decisions[ordinal].unwrap();
    admitted[ordinal].as_ref().unwrap().contains(&Candidate {
        class: d.class,
        reason: d.reason,
        next: d.next,
    })
}

#[test]
fn arithmetic_emitted_rounding_laws_reject_wrong_actual_decisions() {
    for (target, wrong) in [
        (fixture::FLOOR, fixture::CEIL),
        (fixture::CEIL, fixture::FLOOR),
    ] {
        let core = core(target, "true");
        let good = core
            .successor_law(0, &core.read_checked(target).unwrap())
            .unwrap();
        let bad = core
            .successor_law(0, &core.read_checked(wrong).unwrap())
            .unwrap();
        law_judges(&core, &good, "checked");
        law_judges(&core, &bad, "false");
        assert!(actual_admission(&core, &good, 1));
        assert!(
            !actual_admission(&core, &bad, 1),
            "wrong emitted law admitted the actual rounded successor"
        );
    }
}

#[test]
fn arithmetic_emitted_product_conditions_reject_wrong_actual_decisions() {
    for (good, bad) in [
        (
            "products_ge(amount, 2, 3, 1)",
            "products_ge(amount, 1, 3, 1)",
        ),
        (
            "products_ge(amount, 2, 1, 3, 1, 1)",
            "products_ge(amount, 1, 1, 3, 1, 1)",
        ),
    ] {
        let core = core(fixture::FLOOR, good);
        let good = core.law_text(&core.read_checked(good).unwrap()).unwrap();
        let bad = core.law_text(&core.read_checked(bad).unwrap()).unwrap();
        law_judges(&core, &good, "checked");
        law_judges(&core, &bad, "false");
        assert!(actual_admission(&core, &good, 2));
        assert!(
            !actual_admission(&core, &bad, 2),
            "wrong emitted product condition admitted the actual decision"
        );
    }
}

fn teach(core: &Core, condition: &str, successor: &str) -> Value {
    let rules = core
        .rules(&json!({"schema":"zeno-fcis/loop-rules/1", "rules":[{
            "id":"arithmetic", "where":condition, "class":"Accept", "reason":null,
            "sets":{"value":successor},
        }]}))
        .unwrap();
    teach_back(core, &rules, &[], false).unwrap()["rules"][0].clone()
}

#[test]
fn arithmetic_teach_back_rejects_floor_ceil_successor_mismatches() {
    for (target, wrong) in [
        (fixture::FLOOR, fixture::CEIL),
        (fixture::CEIL, fixture::FLOOR),
    ] {
        let core = core(target, "true");
        assert_eq!(teach(&core, "true", target)["verdict"], "exact");
        let bad = teach(&core, "true", wrong);
        assert_eq!(bad["verdict"], "wrong", "{bad}");
        assert_eq!(bad["exceptions"], 4, "{bad}");
    }
}

#[test]
fn arithmetic_teach_back_product_conditions_are_discriminating() {
    for (good, bad) in [
        (
            "products_ge(amount, 2, 3, 1)",
            "products_ge(amount, 3, 3, 1)",
        ),
        (
            "products_ge(amount, 2, 1, 3, 1, 1)",
            "products_ge(amount, 3, 1, 3, 1, 1)",
        ),
    ] {
        let core = core(fixture::FLOOR, good);
        assert_eq!(teach(&core, good, fixture::FLOOR)["verdict"], "exact");
        let bad = teach(&core, bad, fixture::FLOOR);
        assert_eq!(bad["verdict"], "wrong", "{bad}");
        assert_eq!(bad["exceptions"], 4, "{bad}");
    }
}

#[test]
fn arithmetic_undefined_values_are_refused_by_actual_judges() {
    let core = core(fixture::FLOOR, "true");
    for text in ["mul_div_floor(amount, 1, 0)", "mul_div_ceil(amount, 1, 0)"] {
        let bad = teach(&core, "true", text);
        assert_eq!(bad["verdict"], "refused", "{bad}");
        let formula = core
            .successor_law(0, &core.read_checked(text).unwrap())
            .unwrap();
        law_judges(&core, &formula, "undefined");
    }
    for text in [
        "products_ge(mul_div_floor(amount, 1, 0), 1, 0, 1)",
        "products_ge(mul_div_ceil(amount, 1, 0), 1, 1, 0, 1, 1)",
        "products_ge(amount, 1, -1, 1)",
        "products_ge(amount, 1, 1, -1, 1, 1)",
    ] {
        let bad = teach(&core, text, fixture::FLOOR);
        assert_eq!(bad["verdict"], "refused", "{bad}");
        let formula = core.law_text(&core.read_checked(text).unwrap()).unwrap();
        law_judges(&core, &formula, "undefined");
    }
}

#[test]
fn arithmetic_scenario_judge_rejects_wrong_rounded_successors_and_refusal() {
    for (target, correct, wrong) in [(fixture::FLOOR, 1, 2), (fixture::CEIL, 2, 1)] {
        let policy = fixture::policy(target, "true").to_string();
        for (expected, failed) in [(correct, 0), (wrong, 1)] {
            let text = format!("# scenario rounding\n0 1 0 | accept - {expected} | -\n");
            let result =
                scenario::replay(fixture::project(), &policy, &text, "arithmetic-control").unwrap();
            assert_eq!(result.failed, failed, "{target}: {}", result.packet);
            assert_eq!(
                result.packet["scenarios"][0]["verdict"],
                if failed == 0 { "passed" } else { "failed" }
            );
        }
    }
    let policy = fixture::policy("mul_div_floor(amount, 1, 0)", "true").to_string();
    let result = scenario::replay(
        fixture::project(),
        &policy,
        "# scenario undefined\n0 1 0 | accept - 0 | -\n",
        "arithmetic-control",
    )
    .unwrap();
    assert_eq!(result.failed, 1, "{}", result.packet);
}

#[test]
fn arithmetic_judges_keep_i128_full_products_at_actual_admission() {
    let max = i128::MAX;
    let expression = format!("mul_div_floor(amount, {max}, {max})");
    let core = core(&expression, "true");
    let law = core
        .successor_law(0, &core.read_checked(&expression).unwrap())
        .unwrap();
    assert!(actual_admission(&core, &law, 2));
    let wrong = core
        .successor_law(0, &core.read_checked("0").unwrap())
        .unwrap();
    assert!(!actual_admission(&core, &wrong, 2));
    let product = format!(
        "products_ge({max}, {max}, {max}, {max}, {max}, {})",
        max - 1
    );
    let good = core
        .law_text(&core.read_checked(&product).unwrap())
        .unwrap();
    assert_eq!(
        core.admissible(&[("commit".to_owned(), good)]).unwrap(),
        core.admissible(&[]).unwrap()
    );
}
