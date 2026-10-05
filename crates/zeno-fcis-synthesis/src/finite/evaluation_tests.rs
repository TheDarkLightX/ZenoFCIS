//! Storage reuse must preserve rejection and recovery behavior.

use super::*;
use alloc::vec;
use zeno_fcis_codec::CanonicalEncode;

fn identity() -> Program {
    Program::try_new(
        vec![Domain::Int { min: -1, max: 1 }],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![Op::Input(0)],
        vec![0],
    )
    .unwrap_or_else(|error| panic!("identity program: {error}"))
}

#[test]
fn canonical_program_import_rejects_hostile_artifacts() {
    let program = identity();
    let bytes = program
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("canonical program: {error}"));
    assert_eq!(
        crate::finite_runtime::import_program(&bytes),
        Ok(program.clone())
    );

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(crate::finite_runtime::import_program(&trailing).is_err());
    assert!(crate::finite_runtime::import_program(&bytes[..bytes.len() - 1]).is_err());

    let source = program
        .value()
        .unwrap_or_else(|error| panic!("program: {error}"));
    let zeno_fcis_value::ValueRef::Tuple(fields) = source.view() else {
        panic!("program tuple")
    };
    let mut changed = fields.to_vec();
    changed[0] = Value::text_ascii("other-profile".into())
        .unwrap_or_else(|error| panic!("profile: {error}"));
    let wrong_profile = Value::tuple(changed).unwrap_or_else(|error| panic!("tuple: {error}"));
    assert!(
        crate::finite_runtime::import_program(
            &wrong_profile
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("changed profile: {error}"))
        )
        .is_err()
    );

    let mut changed = fields.to_vec();
    let zeno_fcis_value::ValueRef::Tuple(nodes) = changed[2].view() else {
        panic!("nodes tuple")
    };
    let mut nodes = nodes.to_vec();
    nodes[0] = Value::tuple(vec![Value::signed(0), Value::signed(99)])
        .unwrap_or_else(|error| panic!("node: {error}"));
    changed[2] = Value::tuple(nodes).unwrap_or_else(|error| panic!("nodes: {error}"));
    let bad_reference = Value::tuple(changed).unwrap_or_else(|error| panic!("tuple: {error}"));
    assert!(
        crate::finite_runtime::import_program(
            &bad_reference
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("changed node: {error}"))
        )
        .is_err()
    );
}

#[test]
fn invalid_inputs_are_rejected_before_buffer_reservation() {
    let program = identity();
    let mut values = Vec::new();
    let mut output = Vec::new();
    assert_eq!(
        program.evaluate_into(&[2], &mut values, &mut output),
        Err(Error::Invalid("input-domain"))
    );
    assert_eq!((values.capacity(), output.capacity()), (0, 0));
}

#[test]
fn domain_failures_clear_old_results_and_allow_reuse() {
    let program = identity();
    let mut values = Vec::new();
    let mut output = Vec::new();
    for (input, expected) in [
        (vec![1], Ok(vec![1])),
        (vec![], Err(Error::Invalid("input-domain"))),
        (vec![0], Ok(vec![0])),
        (vec![-1], Err(Error::Invalid("output-domain"))),
        (vec![1], Ok(vec![1])),
    ] {
        let result = program.evaluate_into(&input, &mut values, &mut output);
        assert_eq!(result.clone().map(|()| output.clone()), expected);
        if result.is_err() {
            assert!(values.is_empty());
            assert!(output.is_empty());
        }
    }
}

#[test]
fn arithmetic_traps_clear_partial_nodes_before_the_next_input() {
    let program = Program::try_new(
        vec![Domain::Bool],
        vec![Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        }],
        vec![
            Op::Input(0),
            Op::Int(1),
            Op::Int(0),
            Op::Select(0, 1, 2),
            Op::Int(i64::MAX),
            Op::Add(4, 3),
        ],
        vec![5],
    )
    .unwrap_or_else(|error| panic!("arithmetic program: {error}"));
    let mut values = Vec::new();
    let mut output = Vec::new();
    for input in [0, 1, 0] {
        let result = program.evaluate_into(&[input], &mut values, &mut output);
        if input == 1 {
            assert_eq!(result, Err(Error::Arithmetic));
            assert!(values.is_empty());
            assert!(output.is_empty());
        } else {
            assert_eq!(result, Ok(()));
            assert_eq!(output, [i64::MAX]);
        }
    }
}
