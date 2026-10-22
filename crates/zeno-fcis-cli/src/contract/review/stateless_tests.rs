//! A singleton empty state must flow through the same Authority and judges.
use super::*;

const PROJECT: &str = "zeno 1;
project 1 admission;
namespace 2 core;
type 100 state NoState;
type 101 command Request;
type 102 context Context;
type 105 int Bit in 0..=1;
field 120 101 request 105;
reason 200 denied precedence 0;
component 400 admission {
 owns 100;
 reads command.101;
 writes post.100;
 contexts context.102;
 budget steps 32;
}
merge [400];
law 501 allowed on accept = command.101.120 == 1;
law 502 denied on reject = command.101.120 == 0;
";

fn rules() -> String {
    json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"stateless",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},
        "variables":{},"genesis":{},
        "law_kinds":{"501":"AuthoritySubjectRecipient","502":"RejectNoAuthority"},
        "cases":[{"when":"command.101.120 == 1","class":"Accept","reason":null,"post":{},"outbox":[]},
        {"when":true,"class":"Reject","reason":200,"post":{},"outbox":[]}]}).to_string()
}

#[test]
fn stateless_contract_has_one_empty_state_and_all_judges_accept_it() {
    let rules = rules();
    let examples = "1 0 | accept - | -\n0 0 | reject 200 | -";
    let loaded = machine::Loaded::read(PROJECT, &rules).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(loaded.state_width, 0);
    let contract = Contract::build(&loaded.declarations, &loaded.rules, loaded.commitment)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(contract.genesis.is_empty());
    assert_eq!(contract.budgets.write, 0);
    assert!(contract.laws.iter().any(|law| law.id == 907));
    let reviewed = review(
        ReviewSources {
            project: PROJECT,
            rules: &rules,
            examples: Some(examples),
        },
        100,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let packet: Value = serde_json::from_str(reviewed.packet()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(packet["examples"]["compared"], 2);
    assert_eq!(packet["examples"]["disagreements"], json!([]));
    assert_eq!(packet["summary"]["refusals"]["count"], 0);
    assert_eq!(
        packet["decision_table"]["tallies"]["classes"],
        json!({"Accept":2,"Reject":2})
    );
    let generated = super::super::generate_contract(super::super::ContractSources {
        project: PROJECT,
        rules: &rules,
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(generated.schema(), loaded.schema);
    assert_eq!(
        generated.policy(),
        policy::encode(&contract, &loaded.schema).unwrap_or_else(|e| panic!("{e}"))
    );
    syn::parse_file(generated.source()).unwrap_or_else(|e| panic!("{e}"));
    let measured = profile::profile(PROJECT, &rules, 100).unwrap_or_else(|e| panic!("{e}"));
    let profile::Profile::Measured(measured) = measured else {
        panic!("profile withheld")
    };
    assert_eq!(
        measured["states"],
        json!({"minimal":1,"reachable":1,"schema":1,"valid":1})
    );
    assert_eq!(
        measured["memory_bits"],
        json!({"declared":0,"minimal":0,"reachable":0})
    );
    let observed = machine::observe_contract(&loaded, 100)
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(observed.machine.space.states, 1);
    let gaps = gaps::judge(
        PROJECT,
        &rules,
        gaps::Options {
            max_tuples: 100,
            max_work: 100000,
            proposals: None,
            mutants: true,
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(gaps, gaps::Gaps::Measured(_)), "{gaps:?}");
    let lemmas =
        lemma::judge(PROJECT, &rules, &["true".into()], 100).unwrap_or_else(|e| panic!("{e}"));
    let lemma::Lemmas::Judged(lemmas) = lemmas else {
        panic!("{lemmas:?}")
    };
    assert_eq!(lemmas[0]["verdict"], "inductive");
    let transitions =
        lemma::judge_transitions(PROJECT, &rules, &["command.101.120 == 1".into()], 100)
            .unwrap_or_else(|e| panic!("{e}"));
    let lemma::Lemmas::Judged(transitions) = transitions else {
        panic!("{transitions:?}")
    };
    assert_eq!(transitions[0]["verdict"], "checked");
    let answer = probe::probe(
        PROJECT,
        &rules,
        &probe::Question::Count {
            condition: "true".into(),
        },
        100,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let probe::Answer::Answered(answer) = answer else {
        panic!("{answer:?}")
    };
    assert_eq!(answer["answer"], 4);
    let scenarios = scenario::replay(
        PROJECT,
        &rules,
        &format!("# scenario accept_and_reject\n{examples}"),
        "scenarios",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(scenarios.failed, 0, "{}", scenarios.packet);
    let properties = run_properties::judge(
        PROJECT,
        &rules,
        &json!({"schema":"zeno-fcis/run-properties/1",
        "properties":[{"kind":"predicate","name":"empty_state","formula":"true"}]})
        .to_string(),
        100,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let lemma::Lemmas::Judged(properties) = properties else {
        panic!("{properties:?}")
    };
    assert_eq!(properties[0]["verdict"], "inductive");
    eprintln!(
        "stateless witness: one empty state, four decided tuples, two accepts/two rejects, all finite judges completed"
    );

    // The symbolic judge accepts the same subject through its exhaustive
    // route. Missing solvers supply no symbolic proof or admission authority.
    use crate::symbolic::verdict::{Answer, Answers, Query};
    let mut no_solvers = |_: &Query<'_>| Answers {
        cvc5: Answer::NotConfigured,
        z3: Answer::NotConfigured,
        millis: [None, None],
    };
    let symbolic = super::super::symbolic::check(
        super::super::symbolic::SymbolicSources {
            project: PROJECT,
            rules: &rules,
            strengthening: None,
        },
        100,
        &mut no_solvers,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        symbolic.status,
        super::super::symbolic::Status::Proved,
        "{}",
        symbolic.report
    );

    // Removing the state invariant is permitted only for an empty record.
    let stateful = PROJECT.replace(
        "type 101 command Request;",
        "type 101 command Request;\nfield 110 100 flag 105;",
    );
    let mut stateful_rules: Value = serde_json::from_str(&rules).unwrap_or_else(|e| panic!("{e}"));
    stateful_rules["genesis"] = json!({"110":0});
    let loaded = machine::Loaded::read(&stateful, &stateful_rules.to_string())
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(Contract::build(&loaded.declarations, &loaded.rules, loaded.commitment).is_err());
}
