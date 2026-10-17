use super::*;
#[path = "../../../tests/support/cpmm.rs"]
mod cpmm;

fn judged(project: &str, policy: &str, formula: &str) -> Value {
    match judge_transitions(
        project,
        policy,
        &[formula.to_owned()],
        super::super::DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|error| panic!("judge: {error}"))
    {
        Lemmas::Judged(mut results) => results.remove(0),
        Lemmas::Inconclusive(stop) => panic!("inconclusive: {stop:?}"),
    }
}

#[test]
fn scaled_cpmm_transition_lemmas_check_the_full_declared_domains() {
    let project = cpmm::project(40);
    for bps in [0, 30, 100] {
        let result = judged(
            &project,
            &cpmm::policy(40, bps, false).to_string(),
            cpmm::RELATION,
        );
        assert_eq!(result["mode"], "transition-relation");
        assert_eq!(result["verdict"], "checked", "{result}");
        assert_eq!(result["tuples"], 136_161);
        assert_eq!(result["admitted"], 68_921);
        assert_eq!(result["checked"], 68_921);
        assert_eq!(result["rejected_without_state_change"], 67_240);
        assert_eq!(result["refused"], 0, "{result}");
        assert_eq!(result["undefined"], 0);
        assert!(
            result.get("initiation").is_none(),
            "state induction relabelled"
        );
        eprintln!("CPMM transition fee={bps}: {result}");
    }
}

fn product_control(stub: bool, three_factors: bool) -> bool {
    let project = cpmm::project(3);
    let relation = if stub {
        "true"
    } else if three_factors {
        "products_ge(post.100.110, post.100.111, 3, pre.100.110, pre.100.111, 3)"
    } else {
        cpmm::RELATION
    };
    let good = judged(&project, &cpmm::policy(3, 0, false).to_string(), relation);
    let bad = judged(&project, &cpmm::policy(3, 0, true).to_string(), relation);
    good["verdict"] == "checked"
        && bad["verdict"] == "false"
        && bad["failed"].as_u64().unwrap_or(0) > 0
        && bad["counterexample"]["relation"] == "fails"
}

#[test]
fn transition_lemma_product_stub_and_a_decreasing_step_fail_the_control() {
    for three in [false, true] {
        assert!(product_control(false, three));
        assert!(
            !product_control(true, three),
            "stubbed product comparison survived"
        );
    }
    let bad = judged(
        &cpmm::project(3),
        &cpmm::policy(3, 0, true).to_string(),
        cpmm::RELATION,
    );
    assert_eq!(bad["counterexample"]["state"], json!([1, 1]));
    assert_eq!(bad["counterexample"]["input"], json!([1, 0]));
    assert_eq!(bad["counterexample"]["outcome"]["post"], json!([2, 0]));
}

#[test]
fn transition_lemma_cannot_pass_an_undefined_vacuous_or_incomplete_scope() {
    let project = cpmm::project(2);
    let policy = cpmm::policy(2, 0, false);
    let undefined = judged(
        &project,
        &policy.to_string(),
        "products_ge(mul_div_floor(1, 1, command.101.120), 1, 0, 1)",
    );
    assert_eq!(undefined["verdict"], "undefined", "{undefined}");
    assert!(undefined["undefined"].as_u64().unwrap_or(0) > 0);
    let mut reject = policy.clone();
    reject["cases"] = json!([{"when":true,"class":"Reject","reason":201,"post":{},"outbox":[]}]);
    let vacuous = judged(&project, &reject.to_string(), cpmm::RELATION);
    assert_eq!(vacuous["verdict"], "vacuous", "{vacuous}");
    assert_eq!(vacuous["admitted"], 0);
    assert!(matches!(
        judge_transitions(
            &project,
            &policy.to_string(),
            &[cpmm::RELATION.to_owned()],
            1
        ),
        Ok(Lemmas::Inconclusive(_))
    ));
    assert!(
        judge_transitions(
            &project,
            &policy.to_string(),
            &["true; law 99 hidden on any = true".to_owned()],
            1000
        )
        .is_err()
    );
    assert!(
        judge(
            &project,
            &policy.to_string(),
            &[cpmm::RELATION.to_owned()],
            1000
        )
        .is_err(),
        "state-only mode changed"
    );
}
