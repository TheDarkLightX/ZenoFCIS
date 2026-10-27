//! Ordered compiler metadata, with unchanged ordinary program and policy bytes.
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use zeno_fcis_contract::internals::declarations::Declarations;
use zeno_fcis_contract::internals::expr;
use zeno_fcis_contract::internals::graph::{Graph, ScalarGraph};
use zeno_fcis_contract::internals::model::{Contract, Expr, ScalarDomain};
use zeno_fcis_contract::internals::rules::Rules;
use zeno_fcis_contract::internals::{policy, program_bytes, schema, schema_commitment};

const PROJECT: &str =
    include_str!("../../../examples/contract-refinement/reservation/abstract/project.zeno");
const POLICY: &str =
    include_str!("../../../examples/contract-refinement/reservation/abstract/v2/policy.json");

#[test]
fn literal_table_has_only_its_selector_and_ordinary_bytes_are_unchanged() {
    let rules = Rules::read(POLICY).unwrap_or_else(|e| panic!("{e}"));
    let declarations =
        Declarations::read(PROJECT, &rules.leaf_bindings).unwrap_or_else(|e| panic!("{e}"));
    let schema = schema::encode(&declarations).unwrap_or_else(|e| panic!("{e}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|e| panic!("{e}"));
    let ordinary =
        Contract::build(&declarations, &rules, commitment).unwrap_or_else(|e| panic!("{e}"));
    let (compiled, roots) = Contract::build_with_roots(&declarations, &rules, commitment)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(roots.len(), 1);
    assert_eq!(compiled.program_roots.len(), 1);
    assert!(matches!(
        roots[0].domain,
        ScalarDomain::Int { min: 0, max } if max == rules.cases.len() as i128 - 1
    ));
    assert_eq!(
        program_bytes(&ordinary.program().unwrap_or_else(|e| panic!("{e}"))),
        program_bytes(&compiled.program().unwrap_or_else(|e| panic!("{e}")))
    );
    assert_eq!(
        policy::encode(&ordinary, &schema),
        policy::encode(&compiled, &schema)
    );
    let inputs = declarations.inputs().unwrap_or_else(|e| panic!("{e}"));
    let mut rebuilt = ScalarGraph::new(&declarations, &inputs);
    let indices: Vec<_> = roots
        .iter()
        .map(|root| {
            rebuilt
                .compile(&root.expression)
                .unwrap_or_else(|e| panic!("{e}"))
                .index
        })
        .collect();
    assert_eq!(indices, compiled.program_roots);
    assert_eq!(rebuilt.table.nodes, compiled.nodes);
}

#[test]
fn expanded_equal_typed_outputs_share_a_root_but_constants_and_reads_do_not() {
    let mut project = PROJECT.replace(
        "field 140 104 event 106;",
        "field 140 104 event 105;\nfield 141 104 constant 105;\nfield 142 104 original 105;",
    );
    project = project
        .lines()
        .filter(|line| !line.starts_with("reason ") || line.starts_with("reason 200 "))
        .collect::<Vec<_>>()
        .join("\n");
    let mut policy: Value = serde_json::from_str(POLICY).unwrap_or_else(|e| panic!("{e}"));
    policy["variables"]["updated"] = json!("choose(state == 0, 1, 0)");
    policy["cases"] = json!([
        {"when":"false", "class":"Reject", "reason":200, "post":{}, "outbox":[]},
        {"when":"true", "class":"Accept", "reason":null, "post":{"110":"updated"},
         "outbox":[{"ordinal":0, "channel":300, "destination":"reservation",
                    "payload":{"140":"updated", "141":"0", "142":"state"},
                    "idempotency_ordinal":0}]}
    ]);
    let rules = Rules::read(&policy.to_string()).unwrap_or_else(|e| panic!("{e}"));
    let declarations =
        Declarations::read(&project, &rules.leaf_bindings).unwrap_or_else(|e| panic!("{e}"));
    let schema = schema::encode(&declarations).unwrap_or_else(|e| panic!("{e}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|e| panic!("{e}"));
    let (compiled, roots) = Contract::build_with_roots(&declarations, &rules, commitment)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(roots.len(), 2);
    assert_eq!(
        roots[1].expression,
        expr::parse("choose(pre.100.110 == 0, 1, 0)").unwrap_or_else(|e| panic!("{e}"))
    );
    assert!(matches!(
        roots[1].domain,
        ScalarDomain::Int {
            min: i128::MIN,
            max: i128::MAX
        }
    ));
    let branch = &compiled.branches[1];
    assert!(matches!(branch.assignments[0].value, Expr::Output(1)));
    assert!(matches!(branch.outbox[0].payload[0].1, Expr::Output(1)));
    assert!(matches!(branch.outbox[0].payload[1].1, Expr::Constant(_)));
    assert!(matches!(branch.outbox[0].payload[2].1, Expr::Input(_, 110)));
    let inputs = declarations.inputs().unwrap_or_else(|e| panic!("{e}"));
    let mut rebuilt = ScalarGraph::new(&declarations, &inputs);
    let indices: Vec<_> = roots
        .iter()
        .map(|root| {
            rebuilt
                .compile(&root.expression)
                .unwrap_or_else(|e| panic!("{e}"))
                .index
        })
        .collect();
    assert_eq!(indices, compiled.program_roots);
    assert_eq!(rebuilt.table.nodes, compiled.nodes);
}
