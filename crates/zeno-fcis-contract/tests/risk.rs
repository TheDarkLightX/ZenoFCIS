//! Finite answers, witnesses and named mutation controls.
use serde_json::{Value, json};
use zeno_fcis_contract::risk::{Limits, Question, check};
const PROJECT: &str = include_str!("../../../examples/quantitative-risk/swap-pool/project.zeno");
const POLICY: &str = include_str!("../../../examples/quantitative-risk/swap-pool/v2/policy.json");
fn answer(q: Question) -> Result<Value, Box<dyn std::error::Error>> {
    let r = check(PROJECT, POLICY, &q, Limits::default())?;
    assert!(r.complete(), "{}", r.packet());
    Ok(r.packet()["result"].clone())
}
fn extremes(k: u32) -> Question {
    Question::Extremes {
        reward: 610,
        steps: k,
    }
}
#[test]
fn counts_include_unreachable_declared_inputs() -> Result<(), Box<dyn std::error::Error>> {
    // reserve 4, fee 2, limit=true is declared but cannot be reached:
    // a successful positive swap reduces reserve; failure at reserve 4 is impossible.
    let r = answer(Question::Inputs {
        condition: "pre.100.110 == 4 && pre.100.111 == 2 && pre.100.112 == 1".into(),
    })?;
    assert_eq!(r["count"], "4");
    assert_eq!(r["total"], "120");
    assert_eq!(
        r["satisfying_witness"]["input"],
        json!(["4", "2", "1", "0", "0"])
    );
    Ok(())
}
#[test]
fn slippage_share_has_both_witnesses() -> Result<(), Box<dyn std::error::Error>> {
    let r = answer(Question::Inputs {
        condition: "post.100.112 == 1".into(),
    })?;
    assert_eq!(r["count"], "36");
    assert_eq!(r["total"], "120");
    assert_eq!(r["share"], json!({"numerator":"3","denominator":"10"}));
    assert!(r["satisfying_witness"].is_object());
    assert!(r["other_witness"].is_object());
    Ok(())
}
#[test]
fn max_plus_uses_maximum_fee() -> Result<(), Box<dyn std::error::Error>> {
    let r = answer(extremes(3))?;
    assert_eq!(r["best"]["value"]["numerator"], "0");
    assert_eq!(r["worst"]["value"]["numerator"], "6");
    assert_eq!(r["worst"]["length"], 3);
    Ok(())
}
#[test]
fn horizon_includes_length_k() -> Result<(), Box<dyn std::error::Error>> {
    for k in [3, 0, 1, 2] {
        let r = answer(extremes(k))?;
        assert_eq!(r["worst"]["value"]["numerator"], (2 * k).to_string());
        assert_eq!(r["worst"]["length"], k);
    }
    Ok(())
}
#[test]
fn reachable_states_are_distinct_and_include_genesis() -> Result<(), Box<dyn std::error::Error>> {
    let r = answer(Question::States {
        condition: "post.100.110 == 4".into(),
        steps: 0,
    })?;
    assert_eq!(r["count"], "1");
    assert_eq!(r["total"], "1");
    assert_eq!(r["satisfying_witness"]["run"], json!([]));
    let r = answer(Question::States {
        condition: "post.100.112 == 1".into(),
        steps: 3,
    })?;
    assert!(r["count"].as_str().ok_or("count")?.parse::<u32>()? > 0);
    assert!(r["other_witness"].is_object());
    Ok(())
}
#[test]
fn budgets_keep_consumed_work_and_make_no_claim() -> Result<(), Box<dyn std::error::Error>> {
    for (q, limits) in [
        (
            extremes(3),
            Limits {
                max_tuples: 119,
                max_work: 1000,
            },
        ),
        (
            extremes(3),
            Limits {
                max_tuples: 120,
                max_work: 2,
            },
        ),
        (extremes(65), Limits::default()),
        (
            Question::Inputs {
                condition: "post.100.112 == 1".into(),
            },
            Limits {
                max_tuples: 120,
                max_work: 3,
            },
        ),
    ] {
        let r = check(PROJECT, POLICY, &q, limits)?;
        assert!(!r.complete());
        assert!(r.packet()["claim"].is_null());
        assert!(r.packet()["measured"]["work"].as_u64().ok_or("work")? <= limits.max_work);
    }
    Ok(())
}
#[test]
fn malformed_and_undefined_properties_are_separate() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        check(
            PROJECT,
            POLICY,
            &Question::Inputs {
                condition: "true; law 9999 injected on any = true".into()
            },
            Limits::default()
        )
        .is_err()
    );
    let q = Question::States {
        condition: "command.101.120 == 0".into(),
        steps: 0,
    };
    assert!(check(PROJECT, POLICY, &q, Limits::default()).is_err());
    Ok(())
}
#[test]
fn refused_genesis_is_unknown_even_at_zero_steps() -> Result<(), Box<dyn std::error::Error>> {
    let project = PROJECT.replace("post.100.110 >= 0", "post.100.110 < 4");
    let r = check(&project, POLICY, &extremes(0), Limits::default())?;
    assert!(!r.complete());
    assert_eq!(r.packet()["measured"]["authority_invocations"], 1);
    Ok(())
}
#[test]
fn negative_rewards_use_min_plus_and_keep_empty_run() -> Result<(), Box<dyn std::error::Error>> {
    let project = PROJECT.replace("Fee in 0..=2", "Fee in -2..=2");
    let policy = POLICY.replace(
        "choose(command.101.120 == 0, 1, 2)",
        "0 - choose(command.101.120 == 0, 1, 2)",
    );
    let r = check(&project, &policy, &extremes(3), Limits::default())?;
    assert!(r.complete(), "{}", r.packet());
    assert_eq!(r.packet()["result"]["best"]["value"]["numerator"], "-6");
    assert_eq!(r.packet()["result"]["worst"]["value"]["numerator"], "0");
    Ok(())
}

#[test]
fn rewards_on_rejects_and_missing_failure_rewards_are_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let mut policy: serde_json::Value = serde_json::from_str(POLICY)?;
    policy["cases"][0]["class"] = json!("Reject");
    policy["cases"][0]["post"] = json!({});
    policy["law_kinds"]["503"] = json!("RejectNoAuthority");
    let rejected =
        PROJECT.to_owned() + "\nlaw 503 reject_limit on reject = command.101.120 > pre.100.110;\n";
    let policy = serde_json::to_string(&policy)?;
    let r = check(&rejected, &policy, &extremes(3), Limits::default())?;
    assert!(!r.complete());
    assert_eq!(r.packet()["result"]["reason"], "undefined reward");
    let project = PROJECT.replace(" failure 0", "");
    let r = check(&project, POLICY, &extremes(3), Limits::default())?;
    assert!(!r.complete());
    assert_eq!(r.packet()["result"]["reason"], "undefined reward");
    Ok(())
}
#[test]
fn authority_refusal_is_not_a_false_property_or_filtered_tuple()
-> Result<(), Box<dyn std::error::Error>> {
    let project = PROJECT.replace("post.100.110 >= 0", "post.100.110 >= 1");
    let r = check(
        &project,
        POLICY,
        &Question::Inputs {
            condition: "post.100.112 == 1".into(),
        },
        Limits::default(),
    )?;
    assert!(!r.complete());
    assert_eq!(r.packet()["result"]["reason"], "Authority refused");
    assert_eq!(r.packet()["measured"]["authority_invocations"], 1);
    assert_eq!(r.packet()["measured"]["work"], 1);
    Ok(())
}
#[test]
fn count_witnesses_are_null_exactly_when_the_kind_is_absent()
-> Result<(), Box<dyn std::error::Error>> {
    for (formula, yes, no) in [("pre.100.110 >= 0", 120, 0), ("pre.100.110 < 0", 0, 120)] {
        let r = answer(Question::Inputs {
            condition: formula.into(),
        })?;
        assert_eq!(r["count"], yes.to_string());
        assert_eq!(r["does_not_satisfy"], no.to_string());
        assert_eq!(r["satisfying_witness"].is_null(), yes == 0);
        assert_eq!(r["other_witness"].is_null(), no == 0);
    }
    Ok(())
}

#[test]
fn comments_and_numeric_laws_do_not_occupy_property_ids_for_input_or_state_queries()
-> Result<(), Box<dyn std::error::Error>> {
    const LARGE_NUMBER: &str = "4294967295";

    let comment_project = format!("{PROJECT}\n// external counter {LARGE_NUMBER}\n");

    let constant_project = format!(
        "{PROJECT}\nlaw 505 external_bound on commit, genesis = post.100.110 <= {LARGE_NUMBER};\n"
    );
    let mut constant_policy: Value = serde_json::from_str(POLICY)?;
    constant_policy["law_kinds"]["505"] = json!("StateInvariant");
    let constant_policy = serde_json::to_string(&constant_policy)?;

    let declared_id_project = format!(
        "{PROJECT}\nlaw {LARGE_NUMBER} external_id on commit, genesis = post.100.110 <= 4;\n"
    );
    let mut declared_id_policy: Value = serde_json::from_str(POLICY)?;
    declared_id_policy["law_kinds"][LARGE_NUMBER] = json!("StateInvariant");
    let declared_id_policy = serde_json::to_string(&declared_id_policy)?;

    for (case, project, policy) in [
        ("comment", comment_project.as_str(), POLICY),
        (
            "law constant",
            constant_project.as_str(),
            constant_policy.as_str(),
        ),
        (
            "declared law ID",
            declared_id_project.as_str(),
            declared_id_policy.as_str(),
        ),
    ] {
        let input = check(
            project,
            policy,
            &Question::Inputs {
                condition: "true".into(),
            },
            Limits::default(),
        )
        .map(|result| result.packet().clone())
        .unwrap_or(Value::Null);
        assert_eq!(input["status"], "checked", "{case}: input query");
        assert_eq!(input["result"]["count"], "120", "{case}");
        assert_eq!(input["result"]["total"], "120", "{case}");

        let state = check(
            project,
            policy,
            &Question::States {
                condition: "true".into(),
                steps: 0,
            },
            Limits::default(),
        )
        .map(|result| result.packet().clone())
        .unwrap_or(Value::Null);
        assert_eq!(state["status"], "checked", "{case}: state query");
        assert_eq!(state["result"]["count"], "1", "{case}");
        assert_eq!(state["result"]["total"], "1", "{case}");
    }
    Ok(())
}

#[test]
fn generated_property_names_do_not_collide_with_existing_laws()
-> Result<(), Box<dyn std::error::Error>> {
    for declarations in [
        vec![(505_u32, "risk_property_992"), (506, "risk_property_993")],
        vec![(992_u32, "risk_property_993")],
    ] {
        let mut project = PROJECT.to_owned();
        let mut policy: Value = serde_json::from_str(POLICY)?;
        for (id, name) in declarations {
            project.push_str(&format!(
                "\nlaw {id} {name} on commit, genesis = post.100.110 <= 4;\n"
            ));
            policy["law_kinds"][id.to_string()] = json!("StateInvariant");
        }
        let policy = serde_json::to_string(&policy)?;
        for (question, total) in [
            (
                Question::Inputs {
                    condition: "true".into(),
                },
                "120",
            ),
            (
                Question::States {
                    condition: "true".into(),
                    steps: 0,
                },
                "1",
            ),
        ] {
            let result = check(&project, &policy, &question, Limits::default());
            assert_eq!(
                result.as_ref().ok().map(|value| value.complete()),
                Some(true),
                "a generated property name must be fresh: {result:?}"
            );
            let result = result?;
            assert_eq!(result.packet()["status"], "checked");
            assert_eq!(result.packet()["result"]["count"], total);
            assert_eq!(result.packet()["result"]["total"], total);
        }
    }
    Ok(())
}

#[test]
fn table_policy_preparation_reaches_the_same_declared_input_domain()
-> Result<(), Box<dyn std::error::Error>> {
    let result = check(
        include_str!("../../../examples/contract-tables/fee-tiers/project.zeno"),
        include_str!("../../../examples/contract-tables/fee-tiers/v2/policy.json"),
        &Question::Inputs {
            condition: "true".into(),
        },
        Limits::default(),
    )?;
    assert!(result.complete(), "{}", result.packet());
    assert_eq!(result.packet()["result"]["count"], "124");
    assert_eq!(result.packet()["result"]["total"], "124");
    Ok(())
}
