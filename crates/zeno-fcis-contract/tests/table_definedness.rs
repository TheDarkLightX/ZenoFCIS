//! Checked table expansion retains eager arithmetic refusals.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::json;
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, Outcome, RefusalClass},
    },
    rules::Rules,
    schema, schema_commitment,
};

const PROJECT: &str = "zeno 1; project 1 eager_counts;
type 100 state S; type 101 command C; type 102 context X;
type 105 int V in 0..=1; table 110 100 entries 105 size 2;
variant 120 101 Tick none; variant 130 102 Desk none;
component 400 keep { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 256; } merge [400]; law 499 state_range on commit, genesis = true;";

fn outcomes(extra: &str, formula: &str) -> Vec<Outcome> {
    outcomes_at_size(2, extra, formula)
}

fn outcomes_at_size(size: u32, extra: &str, formula: &str) -> Vec<Outcome> {
    let project = PROJECT.replace("size 2;", &format!("size {size};"));
    let project = format!("{project} {extra} law 500 checked on commit = {formula};");
    let mut policy = json!({"schema":"zeno-fcis/template-declarative-policy/2", "template":"eager-counts",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{},"variables":{},
        "cases":[{"when":true,"class":"Accept","reason":null,
            "post":{"110":"pre.100.110","111":"pre.100.111"},"outbox":[]}],
        "genesis":{"110":0,"111":0},"law_kinds":{"499":"StateInvariant","500":"FeeAndRounding"}});
    if size == 1 {
        policy["genesis"].as_object_mut().unwrap().remove("111");
        policy["cases"][0]["post"]
            .as_object_mut()
            .unwrap()
            .remove("111");
    }
    let rules = Rules::read_for_project(&project, &policy.to_string()).unwrap();
    let declarations = Declarations::read(&project, &rules.leaf_bindings).unwrap();
    let positions = domain::positions(&declarations).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        [
            [1, 0, 120, 130],
            [0, 0, 120, 130],
            [0, 1, 120, 130],
            [1, 1, 120, 130],
        ]
        .iter()
        .map(|input| {
            if size == 1 {
                evaluate::evaluate(authority, &framer, &[input[0], input[2], input[3]])
            } else {
                evaluate::evaluate(authority, &framer, input)
            }
        })
        .collect()
    })
    .unwrap()
}

fn assert_undefined(outcome: &Outcome) {
    assert!(
        matches!(outcome, Outcome::Refused(r) if r.class == RefusalClass::Law
        && r.law == Some(500) && r.text == "Core(Law(Undefined))"),
        "eager overflow must refuse law 500 as undefined: {outcome:?}"
    );
}

#[test]
fn degenerate_counts_preserve_eager_predicate_definedness() {
    let condition = format!("post.100.110[j] + {} > 0", i128::MAX);
    let scalar = format!(
        "choose(post.100.110 + {0} > 0,1,0) + choose(post.100.111 + {0} > 0,1,0)",
        i128::MAX
    );
    for (op, bound) in [
        ("<=", 2),
        ("<=", 3),
        (">=", 0),
        ("<", 3),
        (">", -1),
        ("==", 3),
        ("!=", 3),
        ("<", 0),
        (">", 2),
        (">=", 3),
        ("<=", -1),
    ] {
        let direct = outcomes("", &format!("{scalar} {op} {bound}"));
        assert_undefined(&direct[0]);
        for form in [
            format!("count j in table 110 {{ {condition} }} {op} {bound}"),
            format!("sum j in table 110 {{ choose({condition},1,0) }} {op} {bound}"),
        ] {
            let actual = outcomes("", &form);
            assert_eq!(
                actual, direct,
                "count expansion must retain predicate definedness: {form}"
            );
        }
    }
}

#[test]
fn constant_counts_preserve_eager_boolean_definedness() {
    let trap = format!("{} + 1 == 0", i128::MAX);
    for predicate in [
        format!("false && {trap}"),
        format!("true || {trap}"),
        format!("false -> {trap}"),
    ] {
        let direct = outcomes_at_size(1, "", &predicate);
        assert_undefined(&direct[0]);
        for form in [
            format!("count j in table 110 {{ {predicate} }} <= 0"),
            format!("sum j in table 110 {{ choose({predicate},1,0) }} <= 0"),
        ] {
            let actual = outcomes_at_size(1, "", &form);
            assert_eq!(
                actual, direct,
                "constant count must retain both eager operands: {form}"
            );
        }
    }
}

#[test]
fn filtered_sums_keep_false_guard_body_and_guard_traps() {
    for (guard, body) in [
        ("false".to_owned(), format!("{} + 1", i128::MAX)),
        (format!("false && {} + 1 == 0", i128::MAX), "3".to_owned()),
    ] {
        let scalar = format!("choose({guard},{body},0) + choose({guard},{body},0) == 0");
        let direct = outcomes("", &scalar);
        assert_undefined(&direct[0]);
        let form = format!("sum j in table 110 where {guard} {{ {body} }} == 0");
        assert_eq!(
            outcomes("", &form),
            direct,
            "a false filter must retain eager definedness: {form}"
        );
    }
}

#[test]
fn recursive_constant_specialization_keeps_eager_guard_traps() {
    let predicate = format!("false && {} + 1 == 0", i128::MAX);
    let extra = format!(
        "derived table 700 flags[i] in table 110 rounds 2 {{ when {predicate} then true; default false; }};"
    );
    let direct = outcomes("", &predicate);
    assert_undefined(&direct[0]);
    assert_eq!(
        outcomes(&extra, "flags[0]"),
        direct,
        "constant specialization must evaluate the full recursive circuit"
    );
}
