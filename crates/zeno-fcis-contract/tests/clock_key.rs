//! The optional server clock key: its form is checked, and it leaves the
//! canonical policy, and so the contract identity, unchanged.
use serde_json::json;
use zeno_fcis_contract::internals::{
    declarations::Declarations, model::Contract, policy, rules::Rules, schema, schema_commitment,
};

const PROJECT: &str = "zeno 1; project 1 deadline; namespace 2 core;
type 100 state State; type 101 command Command; type 102 context Context;
type 105 int Count in 0..=3; type 106 int Day in 0..=14; type 107 bool Allowed;
field 110 100 count 105; field 130 102 allowed 107; field 131 102 day 106;
variant 120 101 Add none;
component 400 counter { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; } merge [400];
law 500 bounded on commit, genesis = post.100.110 <= 3;
";

fn rules(clock: Option<serde_json::Value>) -> serde_json::Value {
    let mut rules = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"deadline",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"107":["Bool"]},"variables":{},
        "cases":[{"when":"pre.100.110 < 3 && context.102.131 > 7","class":"Accept","reason":null,"post":{"110":"pre.100.110 + 1"},"outbox":[]},
                 {"when":true,"class":"Accept","reason":null,"post":{},"outbox":[]}],
        "genesis":{"110":0},"law_kinds":{"500":"StateInvariant"}});
    if let Some(clock) = clock {
        rules["clock"] = clock;
    }
    rules
}

fn canonical(rules: &serde_json::Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rules = Rules::read(&rules.to_string())?;
    let d = Declarations::read(PROJECT, &rules.leaf_bindings)?;
    let schema = schema::encode(&d)?;
    let contract = Contract::build(&d, &rules, schema_commitment(&schema)?)?;
    Ok(policy::encode(&contract, &schema)?)
}

#[test]
fn a_clock_is_checked_and_leaves_the_contract_identity_unchanged()
-> Result<(), Box<dyn std::error::Error>> {
    let clock = json!({"field":131,"unit":"days","epoch":"2026-10-21"});
    assert_eq!(canonical(&rules(Some(clock)))?, canonical(&rules(None))?);
    let minutes = json!({"field":131,"unit":"minutes","epoch":"2024-02-29"});
    assert!(Rules::read(&rules(Some(minutes)).to_string()).is_ok());
    for (bad, place) in [
        (
            json!({"field":131,"unit":"hours","epoch":"2026-10-21"}),
            "unit",
        ),
        (
            json!({"field":131,"unit":"days","epoch":"2026-02-29"}),
            "epoch",
        ),
        (
            json!({"field":131,"unit":"days","epoch":"1969-12-31"}),
            "epoch",
        ),
        (
            json!({"field":131,"unit":"days","epoch":"2026-1-21"}),
            "epoch",
        ),
        (
            json!({"field":131,"unit":"days","epoch":"2026-10-21T00:00"}),
            "epoch",
        ),
        (
            json!({"field":-1,"unit":"days","epoch":"2026-10-21"}),
            "field",
        ),
        (json!({"field":131,"unit":"days"}), "epoch"),
        (
            json!({"field":131,"unit":"days","epoch":"2026-10-21","zone":"UTC"}),
            "zone",
        ),
    ] {
        let Err(error) = Rules::read(&rules(Some(bad)).to_string()) else {
            panic!("malformed clock accepted");
        };
        assert!(format!("{error}").contains(place), "{error} names {place}");
    }
    Ok(())
}
