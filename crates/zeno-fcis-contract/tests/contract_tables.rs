//! The library loans table example through the library Authority.

use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain::{self, Position},
        evaluate::{self, Framer, Outcome},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};

const PROJECT: &str = include_str!("../../../examples/contract-tables/library-loans/project.zeno");
const POLICY: &str = include_str!("../../../examples/contract-tables/library-loans/v2/policy.json");
const BORROW: i128 = 150;
const RETURN: i128 = 151;
const LIBRARIAN: i128 = 130;

/// Decides each `(copies, action, copy, member)` input at the librarian desk.
fn decide(project: &str, rules: &str, inputs: &[([i128; 6], i128, i128, i128)]) -> Vec<Outcome> {
    let rules = Rules::read_for_project(project, rules).unwrap_or_else(|error| panic!("{error:?}"));
    let declarations = Declarations::read(project, &rules.leaf_bindings).unwrap_or_else(|error| panic!("{error:?}"));
    let positions: Vec<Position> = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error:?}"));
    let names: Vec<&str> = positions.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "pre.100.110",
            "pre.100.111",
            "pre.100.112",
            "pre.100.113",
            "pre.100.114",
            "pre.100.115",
            "command.101.120",
            "command.101.121",
            "command.101.122",
            "context.102"
        ]
    );
    assert_eq!(domain::domain_size(&positions), Some(294_912));
    let schema = schema::encode(&declarations).unwrap_or_else(|error| panic!("{error:?}"));
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema).unwrap_or_else(|error| panic!("{error:?}")),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        inputs
            .iter()
            .map(|(copies, action, copy, member)| {
                let mut tuple = copies.to_vec();
                tuple.extend([*action, *copy, *member, LIBRARIAN]);
                evaluate::evaluate(authority, &framer, &tuple)
            })
            .collect()
    })
    .unwrap_or_else(|error| panic!("{error:?}"))
}

fn post(outcome: &Outcome) -> Vec<i128> {
    match outcome {
        Outcome::Decision(decision) => decision
            .post
            .iter()
            .map(|(_, v)| {
                v.number()
                    .unwrap_or_else(|| panic!("expected integer outcome"))
            })
            .collect(),
        Outcome::Refused(refusal) => panic!("refused: {refusal:?}"),
    }
}

#[test]
fn contract_tables_example_decides_through_its_expanded_laws() {
    let outcomes = decide(
        PROJECT,
        POLICY,
        &[
            ([0, 0, 0, 0, 0, 0], BORROW, 5, 1),
            ([1, 0, 0, 0, 0, 0], BORROW, 5, 1),
            ([1, 0, 0, 0, 0, 1], BORROW, 3, 1),
            ([1, 0, 0, 0, 0, 1], RETURN, 5, 1),
        ],
    );
    assert_eq!(post(&outcomes[0]), [0, 0, 0, 0, 0, 1]);
    // A second copy is lawful under `count ... <= 2`.
    assert_eq!(post(&outcomes[1]), [1, 0, 0, 0, 0, 1]);
    match &outcomes[2] {
        Outcome::Decision(decision) => {
            assert_eq!(
                (decision.class, decision.reason),
                (Class::Reject, Some(202))
            );
        }
        refused => panic!("{refused:?}"),
    }
    assert_eq!(post(&outcomes[3]), [1, 0, 0, 0, 0, 0]);
}

#[test]
fn contract_tables_expanded_laws_refuse_rules_that_break_them() {
    // A borrow that also changes the last copy breaks the frame law 502, and
    // one that gives the member a third copy breaks the count law 500.
    let mut rules: serde_json::Value =
        serde_json::from_str(&Rules::expand_for_project(PROJECT, POLICY).unwrap_or_else(|error| panic!("{error:?}"))).unwrap_or_else(|error| panic!("{error:?}"));
    rules["cases"][4]["post"]["115"] = serde_json::json!("choose(copy == 0, member, pre.100.115)");
    let frame = decide(
        PROJECT,
        &rules.to_string(),
        &[([0, 0, 0, 0, 0, 0], BORROW, 0, 2)],
    );
    let mut rules: serde_json::Value =
        serde_json::from_str(&Rules::expand_for_project(PROJECT, POLICY).unwrap_or_else(|error| panic!("{error:?}"))).unwrap_or_else(|error| panic!("{error:?}"));
    rules["cases"][2]["when"] = serde_json::json!("false");
    let limit = decide(
        PROJECT,
        &rules.to_string(),
        &[([1, 1, 0, 0, 0, 0], BORROW, 4, 1)],
    );
    for (outcome, law) in [(&frame[0], 502), (&limit[0], 500)] {
        match outcome {
            Outcome::Refused(refusal) => assert_eq!(refusal.law, Some(law), "{refusal:?}"),
            decided => panic!("law {law} did not refuse: {decided:?}"),
        }
    }
}
