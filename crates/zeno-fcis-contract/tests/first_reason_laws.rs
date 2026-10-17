//! Declared rejection laws distinguish reasons without framework law 991.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{Framer, LawSet, Proposed},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};

const PROJECT: &str = r#"zeno 1;
project 1 first_reason;
namespace 2 test;
type 100 state State;
type 101 command Command;
type 102 context Context;
type 103 int Small in 0..=2;
field 110 100 value 103;
field 120 101 amount 103;
reason 200 first precedence 0;
reason 201 second precedence 1;
component 400 core {
  owns 100; reads pre.100; writes post.100; contexts context.102;
  budget steps 128;
}
merge [400];
law 500 shape on commit, genesis = post.100.110 >= 0;
law 501 progress on accept = command.101.120 > 1;
law 502 refusal on reject = command.101.120 <= 1;
law 503 first_reason on reject =
  (command.101.120 == 0 && decision_reason == 200) ||
  (command.101.120 == 1 && decision_reason == 201);
"#;
const POLICY: &str = r#"{
"schema":"zeno-fcis/template-declarative-policy/2", "template":"first-reason",
"roots":{"state":100,"command":101,"context":102},
"leaf_bindings":{"102":["Bool"]},"variables":{},
"cases":[
 {"when":"command.101.120 == 0","class":"Reject","reason":200,"post":{},"outbox":[]},
 {"when":"command.101.120 <= 1","class":"Reject","reason":201,"post":{},"outbox":[]},
 {"when":true,"class":"Accept","reason":null,"post":{"110":"pre.100.110"},"outbox":[]}],
"genesis":{"110":0},
"law_kinds":{"500":"StateInvariant","501":"AuthoritySubjectRecipient",
             "502":"RejectNoAuthority","503":"RejectNoAuthority"}}
"#;

#[test]
fn declared_first_reason_law_kills_later_reason_without_framework_991() {
    let rules = Rules::read(POLICY).unwrap();
    let declarations = Declarations::read(PROJECT, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    let positions = domain::positions(&declarations).unwrap();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        let laws = LawSet::new(authority.descriptor(), &[500, 501, 502, 503]);
        for (amount, correct, wrong) in [(0, 200, 201), (1, 201, 200)] {
            for (reason, expected) in [(correct, true), (wrong, false)] {
                let (admitted, _) = laws.on_transition(
                    &framer,
                    &[0, amount, 1],
                    &Proposed {
                        class: Class::Reject,
                        reason: Some(reason),
                        post: &[],
                    },
                );
                assert_eq!(admitted, expected, "amount={amount}, reason={reason}");
            }
        }
    })
    .unwrap();
}

#[test]
fn reason_observation_refuses_accept_and_genesis_scopes() {
    for scope in ["any", "accept", "commit, genesis"] {
        let source = PROJECT.replace(
            "first_reason on reject",
            &format!("first_reason on {scope}"),
        );
        let rules_text = POLICY.replace(
            "\"503\":\"RejectNoAuthority\"",
            "\"503\":\"AuthoritySubjectRecipient\"",
        );
        let rules = Rules::read(&rules_text).unwrap();
        let result = Declarations::read(&source, &rules.leaf_bindings).and_then(|declarations| {
            let schema = schema::encode(&declarations)?;
            Contract::build(&declarations, &rules, schema_commitment(&schema)?).map(|_| ())
        });
        assert!(result.is_err(), "unsafe reason scope {scope}");
    }
}

#[test]
fn existing_rule_variable_does_not_shadow_candidate_reason() {
    let text = POLICY
        .replace(
            "\"variables\":{}",
            "\"variables\":{\"decision_reason\":\"command.101.120\"}",
        )
        .replace(
            "\"post\":{\"110\":\"pre.100.110\"}",
            "\"post\":{\"110\":\"decision_reason\"}",
        );
    let rules = Rules::read(&text).unwrap();
    let declarations = Declarations::read(PROJECT, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    let positions = domain::positions(&declarations).unwrap();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        let laws = LawSet::new(authority.descriptor(), &[503]);
        let correct = Proposed {
            class: Class::Reject,
            reason: Some(200),
            post: &[],
        };
        assert!(laws.on_transition(&framer, &[0, 0, 1], &correct).0);
        let result = zeno_fcis_contract::internals::review::evaluate::evaluate(
            authority,
            &framer,
            &[0, 2, 1],
        );
        assert!(matches!(
            result,
            zeno_fcis_contract::internals::review::evaluate::Outcome::Decision(_)
        ));
    })
    .unwrap();
}

#[test]
#[ignore = "requires the generated five bounded Spot judge projects; see SPOT_JUDGE_ROOT"]
fn spot_228_declared_laws_determine_every_reject_reason() {
    use std::{fs, path::PathBuf};
    use zeno_fcis_contract::internals::declarations::Source;
    use zeno_fcis_contract::internals::review::evaluate::{self, Outcome};
    let root = PathBuf::from(std::env::var("SPOT_JUDGE_ROOT").expect("SPOT_JUDGE_ROOT"));
    let (mut tuples, mut accepts, mut rejects) = (0, 0, 0);
    for name in ["create", "swap-in", "swap-out", "lp", "ingress-edge"] {
        let source = fs::read_to_string(root.join(name).join("project.zeno")).unwrap();
        let policy_text = fs::read_to_string(root.join(name).join("v2/policy.json")).unwrap();
        let rules = Rules::read(&policy_text).unwrap();
        let declarations = Declarations::read(&source, &rules.leaf_bindings).unwrap();
        let schema = schema::encode(&declarations).unwrap();
        let contract =
            Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
        let positions = domain::positions(&declarations).unwrap();
        let framer = Framer::new(&positions, &contract);
        let mut all = vec![Vec::new()];
        for position in &positions {
            let mut next = Vec::new();
            for prefix in &all {
                for value in position.domain.values_within(512).unwrap() {
                    let mut tuple = prefix.clone();
                    tuple.push(value);
                    next.push(tuple);
                }
            }
            all = next;
        }
        policy::with_authority(&contract, &schema, |authority| {
            let ids: Vec<_> = contract
                .laws
                .iter()
                .map(|law| law.id)
                .filter(|id| *id < 900)
                .collect();
            let laws = LawSet::new(authority.descriptor(), &ids);
            for tuple in all {
                let Outcome::Decision(actual) = evaluate::evaluate(authority, &framer, &tuple)
                else {
                    panic!("{name}: original decision refused");
                };
                tuples += 1;
                let mut successor = tuple.clone();
                for (index, position) in positions.iter().enumerate() {
                    if position.source == Source::State && actual.class != Class::Reject {
                        let field = position.field.unwrap();
                        successor[index] = i64::try_from(
                            actual
                                .post
                                .iter()
                                .find(|(id, _)| *id == field)
                                .unwrap()
                                .1
                                .number()
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                let correct = Proposed {
                    class: actual.class,
                    reason: actual.reason,
                    post: &successor,
                };
                assert!(
                    laws.on_transition(&framer, &tuple, &correct).0,
                    "{name}: correct decision refused"
                );
                if actual.class == Class::Reject {
                    rejects += 1;
                } else {
                    accepts += 1;
                }
                for reason in &declarations.reasons {
                    let proposed = Proposed {
                        class: Class::Reject,
                        reason: Some(*reason),
                        post: &[],
                    };
                    let admitted = laws.on_transition(&framer, &tuple, &proposed).0;
                    assert_eq!(
                        admitted,
                        actual.class == Class::Reject && actual.reason == Some(*reason),
                        "{name}: first-reason freedom, tuple={tuple:?}, alternative={reason}"
                    );
                }
            }
        })
        .unwrap();
    }
    assert_eq!((tuples, accepts, rejects), (228, 54, 174));
    println!(
        "228 tuples; 54 accepts; 174 rejects; zero reject-reason freedom; framework 991 excluded"
    );
}
