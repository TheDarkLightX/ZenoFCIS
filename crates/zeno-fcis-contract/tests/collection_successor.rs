//! Canonical stored successors are checked with the original invocation classes.
use std::collections::BTreeMap;
use zeno_fcis_contract::collections::runtime::{Entries, Input, Key, Layout};
use zeno_fcis_contract::internals::{
    declarations::Declarations, model::Contract, policy, review::evaluate::Framer, rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{v2_composition as c, v2_laws as laws};

const PROJECT: &str = include_str!("../../../test-data/collections/deletion/project.zeno");
const POLICY: &str = include_str!("../../../test-data/collections/deletion/v2/policy.json");
struct Records(Option<Vec<u8>>);
impl Entries for Records {
    fn record(&self, _: u32, _: &Key) -> Option<Vec<u8>> {
        self.0.clone()
    }
}
#[test]
fn canonical_deletion_rechecks_laws_and_retains_refusal_work()
-> Result<(), Box<dyn std::error::Error>> {
    deletion_check(PROJECT, POLICY, false)
}
#[test]
fn lawful_canonical_deletion_preserves_the_checked_behavior()
-> Result<(), Box<dyn std::error::Error>> {
    let project = PROJECT
        .lines()
        .filter(|line| !line.starts_with("law 500"))
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let mut rules: serde_json::Value = serde_json::from_str(POLICY)?;
    rules["law_kinds"]
        .as_object_mut()
        .ok_or("law kinds")?
        .remove("500");
    deletion_check(&project, &rules.to_string(), true)
}
fn deletion_check(
    project: &str,
    policy_source: &str,
    lawful: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = Layout::read(project, policy_source)?;
    let rules = Rules::read(policy_source)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let framer = Framer::new(layout.positions(), &contract);
    let context = Input {
        scalars: BTreeMap::from([(0, 180)]),
        ..Default::default()
    };
    policy::with_authority(&contract, &schema, |authority| {
        let mut records = Records(None);
        for action in [150, 151] {
            let command = Input {
                scalars: BTreeMap::from([(130, action)]),
                keys: BTreeMap::from([(131, [0x10; 32])]),
            };
            let framed = layout
                .frame(&BTreeMap::new(), &command, &context, &records)
                .unwrap_or_else(|e| panic!("{e}"));
            let raw = framer.frame(&framed.tuple);
            let evaluation = authority.evaluate(c::Raw {
                state: &raw[0],
                command: &raw[1],
                context: &raw[2],
            });
            let candidate = evaluation.result().unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(candidate.class(), c::Class::Accept);
            let post = candidate
                .post()
                .iter()
                .map(|f| {
                    let n = match f.value {
                        c::Atom::I128(n) => n,
                        _ => panic!("numeric"),
                    };
                    (f.id, n)
                })
                .collect();
            let writes = layout
                .writes(&framed, &post)
                .unwrap_or_else(|e| panic!("{e}"));
            let successor = layout
                .successor(&framed, &writes)
                .unwrap_or_else(|e| panic!("{e}"));
            let checked = layout
                .check_successor(authority.descriptor(), &framed, &writes, &evaluation)
                .unwrap_or_else(|e| panic!("{e}"));
            let (result, usage, diagnostics, reads) = checked.into_parts();
            if action == 150 {
                assert!(result.is_ok());
                records.0 = writes.entries[0].2.clone();
            } else {
                assert_eq!(
                    post[&161], 1,
                    "witness must pass raw laws with quantity one"
                );
                assert_eq!(successor[1], 0, "stored absence must have quantity zero");
                if lawful {
                    assert!(
                        result.is_ok(),
                        "lawful canonical deletion was refused: {diagnostics:?}"
                    );
                } else {
                    assert!(result.is_err(), "canonical deletion bypassed the bound law");
                }
                assert!(
                    lawful
                        || diagnostics
                            .iter()
                            .any(|d| d.id == 500 && matches!(d.verdict, laws::Verdict::Refused(_)))
                );
                assert!(usage.used(zeno_fcis_synthesis::finite::V2Resource::Step) > 0);
                assert!(!reads.is_empty());
                assert!(records.0.is_some(), "refused successor was retained");
            }
        }
    })?;
    Ok(())
}
