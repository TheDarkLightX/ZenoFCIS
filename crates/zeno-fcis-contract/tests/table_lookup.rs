//! Project-aware lookups preserve the ordinary bound evaluator and its work.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::json;
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    expr,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, Outcome},
    },
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{V2Resource, v2_zero_limits};

const PROJECT: &str = "zeno 1; project 1 fees;
type 100 state S; type 101 command C; type 102 context X;
type 105 int Fee in 0..=30; type 106 int Tier in 0..=3;
const table 110 fee_bps 105 size 4 [30,25,20,10];
field 120 101 tier 106; field 130 100 fee 105; variant 140 102 Session none;
component 400 quote { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 256; } merge [400];
law 500 range on commit, genesis = post.100.130 >= 0 && post.100.130 <= 30;";

fn rules(value: &str) -> String {
    json!({"schema":"zeno-fcis/template-declarative-policy/2", "template":"fees",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{},
        "variables":{"rate":value},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"130":"rate"},"outbox":[]}],
        "genesis":{"130":30},"law_kinds":{"500":"StateInvariant"}}).to_string()
}
fn bound<T>(source: &str, policy: &str, check: impl FnOnce(&Contract<'_>, &[u8]) -> T) -> T {
    let rules = Rules::read_for_project(source, policy).unwrap();
    let declarations = Declarations::read(source, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    check(&contract, &schema)
}
fn quote(source: &str, expression: &str, expected: [i128; 4]) {
    quote_policy(source, &rules(expression), expected);
}
fn quote_policy(source: &str, policy_text: &str, expected: [i128; 4]) {
    bound(source, policy_text, |contract, schema| {
        let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
        let verify = |actual: Outcome, tier: usize, fee: i128| {
            assert!(
                matches!(&actual, Outcome::Decision(d) if d.post.first().and_then(|(_, v)| v.number())==Some(fee)),
                "tier={tier} expected fee={fee}, actual={actual:?}"
            );
        };
        if contract.unsigned {
            policy::with_numeric_authority(contract, schema, |authority| {
                for tier in [3, 0, 1, 2] {
                    let fee = expected[tier];
                    let frames = framer.frame(&[30, tier as i128, 140]);
                    let result =
                        authority.evaluate(zeno_fcis_synthesis::finite::v2_composition::Raw {
                            state: &frames[0],
                            command: &frames[1],
                            context: &frames[2],
                        });
                    verify(evaluate::from_evaluation(&result), tier, fee);
                }
            })
            .unwrap();
        } else {
            policy::with_authority(contract, schema, |authority| {
                for tier in [3, 0, 1, 2] {
                    let fee = expected[tier];
                    verify(
                        evaluate::evaluate(authority, &framer, &[30, tier as i128, 140]),
                        tier,
                        fee,
                    );
                }
            })
            .unwrap();
        }
    });
}

#[test]
fn computed_policy_lookup_returns_ten_for_tier_three() {
    quote(PROJECT, "fee_bps[command.101.120 + 0]", [30, 25, 20, 10]);
    quote(PROJECT, "fee_bps[3 - command.101.120]", [10, 20, 25, 30]);
}

#[test]
fn policy_lookups_match_scalar_encoding_and_eager_work() {
    let tabled = rules("fee_bps[command.101.120 + 0]");
    let scalar = rules(
        "choose(command.101.120 + 0 == 0,30,choose(command.101.120 + 0 == 1,25,choose(command.101.120 + 0 == 2,20,10)))",
    );
    let native = |text: &str| {
        bound(PROJECT, text, |contract, schema| {
            (
                contract.program().unwrap(),
                policy::encode(contract, schema).unwrap(),
            )
        })
    };
    let (a, abytes) = native(&tabled);
    let (b, bbytes) = native(&scalar);
    assert_eq!(abytes, bbytes);
    for tier in [3, 0, 1, 2] {
        let input = [30, tier, 140];
        let limits = v2_zero_limits().with_limit(V2Resource::Step, 10000);
        let (actual, usage) = a.execute_v2(&input, limits).into_parts();
        let (expected, reference_usage) = b.execute_v2(&input, limits).into_parts();
        assert!(actual.is_ok());
        assert_eq!(actual, expected);
        assert_eq!(usage, reference_usage);
        assert_eq!(
            usage.used(V2Resource::Step),
            a.nodes().len() as u64,
            "every eager node must be charged; tier={tier}"
        );
        let (refused, spent) = a
            .execute_v2(
                &input,
                v2_zero_limits().with_limit(V2Resource::Step, a.nodes().len() as u64 - 1),
            )
            .into_parts();
        assert!(refused.is_err());
        assert_eq!(spent.used(V2Resource::Step), a.nodes().len() as u64 - 1);
        println!(
            "tier={tier} eager_steps={} nodes={}",
            usage.used(V2Resource::Step),
            a.nodes().len()
        );
    }
}

#[test]
fn policy_reads_closed_boolean_and_word_derived_entries() {
    let source=format!("{PROJECT}
        derived table 700 flags[i] in table 110 rounds 4 {{ when true then fee_bps[i] < 30; default false; }};
        derived table 701 words[i] in table 110 bits 2 rounds 8 {{ when fee_bps[i] < 30 then u128(3); default u128(0); }};
        derived table 702 live[i] in table 110 rounds 4 {{ when true then command.101.120 == i; default false; }};");
    quote(
        &source,
        "choose(flags[3 - command.101.120],1,0)",
        [1, 1, 1, 0],
    );
    let numeric = rules("to_i128(words[command.101.120])").replace(
        "template-declarative-policy/2",
        "template-declarative-policy/4",
    );
    quote_policy(&source, &numeric, [0, 3, 3, 3]);
}

#[test]
fn policy_lookup_refuses_unproved_range_and_keeps_work_guard() {
    let broad = PROJECT.replace("Tier in 0..=3", "Tier in 0..=4");
    let error = Rules::read_for_project(&broad, &rules("fee_bps[command.101.120]")).unwrap_err();
    assert!(error.reason().contains("index range"), "{error}");
    let source = rules(
        "sum a in table 110 { sum b in table 110 { sum c in table 110 { sum d in table 110 { fee_bps[command.101.120] } } } }",
    );
    let error = Rules::read_for_project(PROJECT, &source).unwrap_err();
    assert!(error.reason().contains("work cap"), "{error}");
    assert!(
        expr::parse_for_project(
            PROJECT,
            "fee_bps[command.101.120] }; law 999 injected = true;"
        )
        .is_err()
    );
}

#[test]
fn fee_tier_example_quotes_all_declared_tiers() {
    quote_policy(
        include_str!("../../../examples/contract-tables/fee-tiers/project.zeno"),
        include_str!("../../../examples/contract-tables/fee-tiers/v2/policy.json"),
        [30, 25, 20, 10],
    );
}
