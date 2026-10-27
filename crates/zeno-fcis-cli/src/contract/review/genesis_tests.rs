//! Starting-state agreement with the runtime Authority, including the boundary
//! between initial conditions and laws of arbitrary committed states.

use super::{DEFAULT_MAX_TUPLES, lemma, machine, probe, profile, scenario};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::genesis;

fn loans(initial: [i32; 3]) -> (String, String) {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/apps/library-loans");
    let project =
        std::fs::read_to_string(base.join("project.zeno")).unwrap_or_else(|e| panic!("{e}"));
    let mut rules: Value = serde_json::from_str(
        &std::fs::read_to_string(base.join("v2/policy.json")).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    rules["genesis"] = json!({"110": initial[0], "111": initial[1], "112": initial[2]});
    (project, rules.to_string())
}

fn refused(packet: Value, law: u32) {
    assert_eq!(packet["admissible_starting_states"], 0);
    assert_eq!(packet["states"]["reachable"], 0);
    assert_eq!(packet["law"], law);
    assert!(
        packet["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no admissible starting state"))
    );
}

#[test]
fn genesis_profile_refuses_law_500() {
    let (project, rules) = loans([1, 1, 1]);
    let result =
        profile::profile(&project, &rules, DEFAULT_MAX_TUPLES).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(result, profile::Profile::NoStartingState(_)),
        "assertion failed: profile must refuse invalid genesis"
    );
    if let profile::Profile::NoStartingState(packet) = result {
        refused(packet, 500);
    }
}

#[test]
fn genesis_lemma_and_probe_refuse_law_500() {
    let (project, rules) = loans([1, 1, 1]);
    let lemma = lemma::judge(
        &project,
        &rules,
        &["post.100.110 >= 0".into()],
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(lemma, lemma::Lemmas::NoStartingState(_)),
        "{lemma:?}"
    );
    if let lemma::Lemmas::NoStartingState(packet) = lemma {
        refused(packet, 500);
    }
    let probe = probe::probe(
        &project,
        &rules,
        &probe::Question::Reachable {
            state: "post.100.110 >= 0".into(),
        },
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(probe, probe::Answer::NoStartingState(_)),
        "{probe:?}"
    );
    if let probe::Answer::NoStartingState(packet) = probe {
        refused(packet, 500);
    }
    let loaded = machine::Loaded::read(&project, &rules).unwrap_or_else(|e| panic!("{e}"));
    let observed = machine::observe_contract(&loaded, DEFAULT_MAX_TUPLES)
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!observed.machine.reachable().0.into_iter().any(|v| v));
}

#[test]
fn genesis_runtime_checks_initial_condition_and_preserves_prestates() {
    let (project, rules) = loans([0, 0, 0]);
    let loaded = machine::Loaded::read(&project, &rules).unwrap_or_else(|e| panic!("{e}"));
    loaded
        .bind(&loaded.rules, |authority, framer| {
            let tuple = [0, 0, 1, 150, 1, 1, 130];
            let frame = framer.frame(&tuple);
            let refusal = genesis::admit(authority, &frame[0])
                .err()
                .unwrap_or_else(|| panic!("admitted wrong genesis"));
            assert_eq!(refusal.law, Some(990));
            assert!(authority.genesis(&frame[0]).result().is_err());
            let ids = super::evaluate::state_laws(authority.descriptor());
            assert!(!ids.contains(&990));
            assert_eq!(
                super::evaluate::first_unsatisfied_state_law(
                    authority.descriptor(),
                    &ids,
                    framer,
                    &tuple
                ),
                None
            );
            assert!(genesis::admit(authority, &[]).is_err());
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn genesis_checks_other_laws_scoped_to_genesis() {
    let (mut project, text) = loans([0, 0, 0]);
    project.push_str("\nlaw 550 startup_guard on accept, genesis = 0 == 1;\n");
    let mut rules: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
    rules["law_kinds"]["550"] = json!("FeeAndRounding");
    let result = profile::profile(&project, &rules.to_string(), DEFAULT_MAX_TUPLES)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(result, profile::Profile::NoStartingState(_)),
        "{result:?}"
    );
    if let profile::Profile::NoStartingState(packet) = result {
        refused(packet, 550);
    }
}

#[test]
fn genesis_unsigned_lemma_retains_law_500() {
    let (project, text) = loans([1, 1, 1]);
    let mut rules: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
    rules["schema"] = json!("zeno-fcis/template-declarative-policy/4");
    let result = lemma::judge(
        &project,
        &rules.to_string(),
        &["post.100.110 >= 0".into()],
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(result, lemma::Lemmas::NoStartingState(_)),
        "{result:?}"
    );
    if let lemma::Lemmas::NoStartingState(packet) = result {
        refused(packet, 500);
    }
}

#[test]
fn genesis_prestates_agree_across_numeric_profiles() {
    use zeno_fcis_contract::internals::{
        declarations::Declarations, model::Contract, policy, rules::Rules, schema,
        schema_commitment,
    };
    use zeno_fcis_synthesis::finite::{v2_composition as c, v2_laws as l};
    let (project, text) = loans([0, 0, 0]);
    let states: Vec<_> = (0..4)
        .flat_map(|x| (0..4).flat_map(move |y| (0..4).map(move |z| [x, y, z])))
        .collect();
    let loaded = machine::Loaded::read(&project, &text).unwrap_or_else(|e| panic!("{e}"));
    let signed = loaded
        .bind(&loaded.rules, |authority, framer| {
            let ids = super::evaluate::state_laws(authority.descriptor());
            Ok(states
                .iter()
                .map(|state| {
                    let tuple = [state[0], state[1], state[2], 150, 1, 1, 130];
                    super::evaluate::first_unsatisfied_state_law(
                        authority.descriptor(),
                        &ids,
                        framer,
                        &tuple,
                    )
                })
                .collect::<Vec<_>>())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let mut rules: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
    rules["schema"] = json!("zeno-fcis/template-declarative-policy/4");
    let rules = Rules::read(&rules.to_string()).unwrap_or_else(|e| panic!("{e}"));
    let declarations =
        Declarations::read(&project, &rules.leaf_bindings).unwrap_or_else(|e| panic!("{e}"));
    let schema = schema::encode(&declarations).unwrap_or_else(|e| panic!("{e}"));
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let numeric = policy::with_numeric_authority(&contract, &schema, |authority| {
        let ids = super::evaluate::state_law_ids(authority.descriptor().laws);
        let laws = super::evaluate::LawSet::new_numeric(authority.descriptor(), &ids);
        states
            .iter()
            .map(|state| {
                let fields: Vec<_> = state
                    .iter()
                    .enumerate()
                    .map(|(i, value)| c::Field {
                        id: 110 + i as u16,
                        value: c::Atom::I128(*value),
                    })
                    .collect();
                laws.first_unsatisfied_state_law(l::RootView::Record(&fields))
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(signed, numeric);
    for (state, excluded) in states.iter().zip(signed) {
        assert_eq!(
            excluded,
            if state[0] != 0 && state[0] == state[1] && state[0] == state[2] {
                Some(500)
            } else {
                None
            }
        );
    }
}

#[test]
fn genesis_valid_dual_approval_retains_14_states_and_tables() {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    let project =
        std::fs::read_to_string(base.join("project.zeno")).unwrap_or_else(|e| panic!("{e}"));
    let rules =
        std::fs::read_to_string(base.join("v2/policy.json")).unwrap_or_else(|e| panic!("{e}"));
    let result =
        profile::profile(&project, &rules, DEFAULT_MAX_TUPLES).unwrap_or_else(|e| panic!("{e}"));
    let profile::Profile::Measured(packet) = result else {
        panic!("{result:?}")
    };
    assert_eq!(packet["states"]["reachable"], 14);
    assert_eq!(
        packet["tables"]["decisions_sha256"],
        "9552f8565762b0a75d8f8136154b49343f50282fda4181f362c8ea25bdc98c72"
    );
    assert_eq!(
        packet["tables"]["states_sha256"],
        "fbe2421697678f5210011907f64e7ae942aabbd2732f9a5bcb37b106809ff3ec"
    );
}

#[test]
fn genesis_scenarios_check_genesis_and_stated_prestates() {
    let (project, rules) = loans([1, 1, 1]);
    let text = "# scenario start\n1 1 1 | 150 1 1 | 131 | reject 200 1 1 1 | -\n";
    let result =
        scenario::replay(&project, &rules, text, "scenario").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(result.failed, 1);
    assert!(
        result.packet["scenarios"][0]["failure"]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("law 500"))
    );
    let (project, rules) = loans([0, 0, 0]);
    let result =
        scenario::replay(&project, &rules, text, "scenario").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(result.failed, 1);
    assert_eq!(result.packet["scenarios"][0]["start"], "stated");
    // An ordinary noninitial committed state is permitted; law 990 is not a pre-state law.
    let text = "# scenario stated\n1 0 0 | 150 1 1 | 131 | reject 200 1 0 0 | -\n";
    assert_eq!(
        scenario::replay(&project, &rules, text, "scenario")
            .unwrap_or_else(|e| panic!("{e}"))
            .failed,
        0
    );
}

#[test]
fn genesis_run_properties_refuse_law_500() {
    let (project, rules) = loans([1, 1, 1]);
    let properties = json!({
        "schema":"zeno-fcis/run-properties/1",
        "properties":[{"kind":"predicate","name":"tautology","formula":"true"}]
    });
    let result = super::run_properties::judge(
        &project,
        &rules,
        &properties.to_string(),
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(result, lemma::Lemmas::NoStartingState(_)),
        "{result:?}"
    );
    if let lemma::Lemmas::NoStartingState(packet) = result {
        refused(packet, 500);
    }
}
