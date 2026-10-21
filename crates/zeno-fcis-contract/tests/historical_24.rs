//! Reconstruction of complete retained policy bytes, including the edit control.
use zeno_fcis_contract::internals::{
    declarations::Declarations, model::Contract, policy, rules::Rules, schema,
    schema_commitment,
};

const PROJECT: &str = include_str!("../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/project.zeno");
const RULES: &str = include_str!("../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/v2/policy.json");
const RETAINED: &[u8] = include_bytes!("../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/v2/policy_v1.zcve");

fn reconstructed(project: &str) -> Vec<u8> {
    let rules = Rules::read(RULES).unwrap_or_else(|error| panic!("released rules: {error}"));
    let declarations = Declarations::read(project, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("released declarations: {error}"));
    let schema = schema::encode(&declarations).unwrap_or_else(|error| panic!("schema: {error}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|error| panic!("commitment: {error}"));
    let compiled = Contract::build_released_24(&declarations, &rules, commitment)
        .unwrap_or_else(|error| panic!("historical compiler: {error}"));
    policy::encode(&compiled, &schema).unwrap_or_else(|error| panic!("policy: {error}"))
}

#[test]
fn historical_policy_bytes_exact() {
    assert_eq!(reconstructed(PROJECT), RETAINED);
}

#[test]
fn edited_superseded_version_refuses() {
    let anchor = "= post.100.120 <= 4 &&";
    assert_eq!(PROJECT.matches(anchor).count(), 1);
    let edited = PROJECT.replace(anchor, "= post.100.120 <= 5 &&");
    assert_ne!(reconstructed(&edited), RETAINED,
        "an edited version cannot satisfy the complete adoption policy binding");
}
