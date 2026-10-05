//! Actual canonical Value bytes entering the shared account composition route.
#[path = "../../zeno-fcis-cli/templates/account-lockout/src/v2_contract.rs"]
#[allow(dead_code, unreachable_pub)]
mod contract;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{V2Resource, v2_composition as c};
use zeno_fcis_value::{Field, Value};

fn bytes(value: Value) -> Vec<u8> {
    value
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("canonical Value: {e}"))
}
fn record(values: &[(u16, Value)]) -> Vec<u8> {
    bytes(
        Value::record_canonical(
            values
                .iter()
                .map(|(id, value)| Field::new(*id, value.clone()))
                .collect(),
        )
        .unwrap_or_else(|e| panic!("record: {e}")),
    )
}
#[test]
fn all_reviewed_examples_bind_actual_value_bytes_without_command_reencoding() {
    let examples =
        include_str!("../../zeno-fcis-cli/templates/account-lockout/tests/decision-examples.txt");
    let contract = contract::Contract::new();
    let descriptor = contract.descriptor();
    let core = c::bind(&descriptor).unwrap_or_else(|e| panic!("account contract: {e:?}"));
    let mut count = 0;
    for line in examples
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let input = line
            .split('|')
            .next()
            .unwrap_or_else(|| panic!("example input"));
        let n: Vec<i128> = input
            .split_whitespace()
            .map(|s| s.parse().unwrap_or_else(|e| panic!("number: {e}")))
            .collect();
        let state = record(&[
            (110, Value::signed(n[0])),
            (111, Value::signed(n[1])),
            (112, Value::signed(n[2])),
        ]);
        let command = bytes(Value::sum(
            101,
            u16::try_from(n[3]).unwrap_or_else(|e| panic!("variant: {e}")),
            None,
        ));
        let context = record(&[(130, Value::signed(n[4])), (131, Value::boolean(n[5] == 1))]);
        let originals = (state.clone(), command.clone(), context.clone());
        let out = core.execute(c::Raw {
            state: &state,
            command: &command,
            context: &context,
        });
        let candidate = out
            .result()
            .unwrap_or_else(|e| panic!("example {line}: {e:?}"));
        let expected: Vec<_> = line
            .split('|')
            .nth(1)
            .unwrap_or_else(|| panic!("decision"))
            .split_whitespace()
            .collect();
        assert_eq!(
            candidate.class(),
            match expected[0] {
                "accept" => c::Class::Accept,
                "reject" => c::Class::Reject,
                "failure" => c::Class::CommittedFailure,
                _ => panic!("class"),
            }
        );
        assert_eq!(
            candidate.reason(),
            if expected[1] == "-" {
                None
            } else {
                Some(
                    expected[1]
                        .parse()
                        .unwrap_or_else(|e| panic!("reason: {e}")),
                )
            }
        );
        for (field, value) in candidate.pre().iter().zip(&n[..3]) {
            assert_eq!(field.value, c::Atom::I128(*value));
        }
        if candidate.class() != c::Class::Reject {
            for (field, value) in candidate.post().iter().zip(&expected[2..5]) {
                assert_eq!(
                    field.value,
                    c::Atom::I128(value.parse().unwrap_or_else(|e| panic!("post: {e}")))
                );
            }
        } else {
            assert!(candidate.post().is_empty());
        }
        let ingress = out.ingress_usage().unwrap_or_else(|| panic!("ingress"));
        assert_eq!(
            ingress.used(V2Resource::Byte),
            (state.len() + command.len() + context.len()) as u64
        );
        assert_eq!(ingress.used(V2Resource::Read), 6);
        assert_eq!(ingress.used(V2Resource::Step), 0);
        assert_eq!(
            out.decision_usage()
                .unwrap_or_else(|| panic!("decision usage"))
                .used(V2Resource::Step),
            descriptor.program.nodes.len() as u64
        );
        assert_eq!(out.reads().len(), 6);
        assert_eq!(
            (out.raw().state, out.raw().command, out.raw().context),
            (&state[..], &command[..], &context[..])
        );
        assert_eq!((state, command, context), originals);
        count += 1;
    }
    assert_eq!(count, 20);
}
#[test]
fn canonical_values_outside_account_schema_are_not_normalized() {
    let contract = contract::Contract::new();
    let descriptor = contract.descriptor();
    let core = c::bind(&descriptor).unwrap_or_else(|e| panic!("account contract: {e:?}"));
    let state = record(&[
        (110, Value::signed(0)),
        (111, Value::signed(0)),
        (112, Value::signed(0)),
    ]);
    let command = bytes(Value::sum(101, 120, None));
    for now in [i128::MIN, -1, 4102444801, i128::MAX] {
        let context = record(&[(130, Value::signed(now)), (131, Value::boolean(false))]);
        assert!(
            core.execute(c::Raw {
                state: &state,
                command: &command,
                context: &context
            })
            .result()
            .is_err()
        );
    }
    let context = record(&[(130, Value::signed(0)), (131, Value::boolean(false))]);
    let wrong_command = bytes(Value::enumeration(101, 120));
    assert!(
        core.execute(c::Raw {
            state: &state,
            command: &wrong_command,
            context: &context
        })
        .result()
        .is_err()
    );
}
