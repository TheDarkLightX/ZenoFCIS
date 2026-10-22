//! Table preparation preserves scalar policy identity and native decisions.
use serde_json::{Value, json};
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
const PROJECT: &str = include_str!("../../../examples/contract-tables/library-loans/project.zeno");
const TABLED: &str = include_str!("../../../examples/contract-tables/library-loans/v2/policy.json");
const SCALAR: &str = include_str!("test-data/library-loans-policy.json");

fn bound<T>(project: &str, source: &str, check: impl FnOnce(&Contract<'_>, &[u8]) -> T) -> T {
    let rules = Rules::read_for_project(project, source)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let declarations = Declarations::read(project, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("table fixture failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    check(&contract, &schema)
}
fn inputs() -> Vec<Vec<i128>> {
    include_str!("../../../examples/contract-tables/library-loans/tests/decision-examples.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.split('|')
                .take(3)
                .flat_map(|part| {
                    part.split_whitespace().map(|v| {
                        v.parse()
                            .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"))
                    })
                })
                .collect()
        })
        .collect()
}
fn decisions(source: &str, inputs: &[Vec<i128>]) -> Vec<Outcome> {
    bound(PROJECT, source, |contract, schema| {
        let framer = Framer::new(
            &domain::positions(contract.declarations)
                .unwrap_or_else(|error| panic!("table fixture failed: {error:?}")),
            contract,
        );
        policy::with_authority(contract, schema, |authority| {
            inputs
                .iter()
                .map(|input| evaluate::evaluate(authority, &framer, input))
                .collect()
        })
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"))
    })
}

#[test]
fn table_policy_preserves_complete_library_policy_bytes() {
    let bytes = |source| {
        bound(PROJECT, source, |contract, schema| {
            policy::encode(contract, schema)
                .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"))
        })
    };
    assert_eq!(bytes(SCALAR), bytes(TABLED));
    // Deterministic runtime inputs and policy bytes are identical, so the
    // policy rewrite preserves every decision, trap and work observation.
    let old = Rules::read(SCALAR).unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let new = Rules::read_for_project(PROJECT, TABLED)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    for name in old.variables.keys() {
        assert_eq!(
            expr::expand(&old.variables[name], &old.variables)
                .unwrap_or_else(|error| panic!("table fixture failed: {error:?}")),
            expr::expand(&new.variables[name], &new.variables)
                .unwrap_or_else(|error| panic!("table fixture failed: {error:?}")),
            "{name}"
        );
    }
}

#[test]
fn indexed_policy_write_preserves_library_decision_examples() {
    let inputs = inputs();
    let old = decisions(SCALAR, &inputs);
    let new = decisions(TABLED, &inputs);
    for ((input, expected), actual) in inputs.iter().zip(old).zip(new) {
        assert_eq!(actual, expected, "library decision input: {input:?}");
    }
}

#[test]
fn table_rule_quantifiers_match_the_explicit_boolean_chains() {
    for (form, operator) in [("forall", "&&"), ("exists", "||")] {
        let mut rules: Value = serde_json::from_str(SCALAR)
            .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
        rules["variables"]["test"] = json!(format!(
            "{form} j in table 110 {{ pre.100.110[j] == member }}"
        ));
        let loaded = Rules::read_for_project(PROJECT, &rules.to_string())
            .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
        let written = (110..116)
            .map(|f| format!("pre.100.{f} == member"))
            .collect::<Vec<_>>()
            .join(&format!(" {operator} "));
        assert_eq!(
            loaded.variables["test"],
            expr::parse(&written).unwrap_or_else(|error| panic!("table fixture failed: {error:?}"))
        );
    }
    let mut rules: Value = serde_json::from_str(SCALAR)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    rules["cases"][0]["when"] =
        json!("desk != 130 || !(forall j in table 110 { pre.100.110[j] >= 0 })");
    assert_eq!(
        decisions(SCALAR, &inputs()),
        decisions(&rules.to_string(), &inputs())
    );
}

#[test]
fn table_rules_refuse_unknown_tables_overlapping_writes_and_broad_indices() {
    let broad = PROJECT.replace(
        "type 106 index CopyNumber of 110;",
        "type 106 int CopyNumber in 0..=6;",
    );
    let error = Rules::read_for_project(&broad, TABLED)
        .err()
        .unwrap_or_else(|| panic!("policy must be refused"));
    assert!(error.reason().contains("index range"), "{error}");
    for (holder, post) in [
        ("pre.100.999[command.101.121]", json!({})),
        ("pre.100.110[6]", json!({})),
        ("pre.100.110[command.101.121 + 1]", json!({})),
        (
            "pre.100.110[command.101.121]",
            json!({"110[command.101.121]":"member", "110":0}),
        ),
    ] {
        let mut r: Value = serde_json::from_str(TABLED)
            .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
        r["variables"]["holder"] = json!(holder);
        r["cases"][4]["post"] = post;
        assert!(
            Rules::read_for_project(PROJECT, &r.to_string()).is_err(),
            "{holder}"
        );
    }
    let mut r: Value = serde_json::from_str(SCALAR)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    r["variables"]["huge"] = json!(
        "sum a in table 110 { sum b in table 110 { sum c in table 110 { sum d in table 110 { sum e in table 110 { sum f in table 110 { 1 } } } } } }"
    );
    assert!(
        Rules::read_for_project(PROJECT, &r.to_string())
            .err()
            .unwrap_or_else(|| panic!("policy must be refused"))
            .reason()
            .contains("work cap")
    );
}

#[test]
fn table_rule_work_bounds_identifier_copy_amplification() {
    let mut rules: Value = serde_json::from_str(SCALAR)
        .unwrap_or_else(|error| panic!("table fixture failed: {error:?}"));
    let long_name = "x".repeat(8192);
    rules["variables"]["large"] = json!(format!(
        "sum a in table 110 {{ sum b in table 110 {{ sum c in table 110 {{ {long_name} }} }} }}"
    ));
    let error = Rules::read_for_project(PROJECT, &rules.to_string())
        .err()
        .unwrap_or_else(|| panic!("policy must be refused"));
    assert!(
        error.reason().contains("work cap"),
        "identifier copies must stop at the work cap: {error}"
    );
}
