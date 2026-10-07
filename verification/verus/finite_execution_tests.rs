use alloc::{vec, vec::Vec};

use super::evaluation::admission::{AdmissionFailure, kind, validate_program};
use super::evaluation::{Domain, Failure, Op, evaluate_into};

const INTEGER: Domain = Domain::Int {
    min: i64::MIN,
    max: i64::MAX,
};

fn execute(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i64],
) -> (Result<(), Failure>, Vec<i64>, Vec<i64>) {
    let mut scratch = vec![123, 456];
    let mut output = vec![789];
    let result = evaluate_into(
        inputs,
        outputs,
        nodes,
        roots,
        input,
        &mut scratch,
        &mut output,
    );
    (result, scratch, output)
}

#[test]
fn all_instructions_preserve_boolean_encoding_and_abi_order() {
    let inputs = [INTEGER, Domain::Bool, Domain::Bool];
    let outputs = [
        INTEGER,
        INTEGER,
        Domain::Bool,
        Domain::Bool,
        Domain::Bool,
        Domain::Bool,
    ];
    let nodes = [
        Op::Input(0),
        Op::Input(1),
        Op::Input(2),
        Op::Int(1),
        Op::Bool(true),
        Op::Add(0, 3),
        Op::Sub(5, 3),
        Op::Eq(0, 6),
        Op::Lt(0, 5),
        Op::And(1, 2),
        Op::Not(9),
        Op::Select(1, 5, 6),
        Op::And(4, 7),
    ];
    let roots = [11, 6, 7, 8, 10, 12];
    assert_eq!(validate_program(&inputs, &outputs, &nodes, &roots), Ok(()));
    for (input, expected) in [
        ([-2, 1, 0], [-1, -2, 1, 1, 1, 1]),
        ([-2, 0, 1], [-2, -2, 1, 1, 1, 1]),
        ([17, 1, 1], [18, 17, 1, 1, 0, 1]),
    ] {
        let (result, scratch, output) = execute(&inputs, &outputs, &nodes, &roots, &input);
        assert_eq!(result, Ok(()));
        assert_eq!(scratch.len(), nodes.len());
        assert_eq!(output, expected);
    }
    let (_, _, output) = execute(&[], &[Domain::Bool], &[Op::Bool(false)], &[0], &[]);
    assert_eq!(output, [0]);
}

#[test]
fn checked_arithmetic_covers_the_machine_boundaries() {
    for (op, input, expected) in [
        (Op::Add(0, 1), [i64::MAX, 1], Err(Failure::Arithmetic)),
        (Op::Add(0, 1), [i64::MIN, -1], Err(Failure::Arithmetic)),
        (Op::Sub(0, 1), [i64::MIN, 1], Err(Failure::Arithmetic)),
        (Op::Sub(0, 1), [i64::MAX, -1], Err(Failure::Arithmetic)),
        (Op::Add(0, 1), [i64::MAX, 0], Ok(i64::MAX)),
        (Op::Sub(0, 1), [i64::MIN, 0], Ok(i64::MIN)),
        (Op::Add(0, 1), [i64::MAX, i64::MIN], Ok(-1)),
        (Op::Sub(0, 1), [i64::MIN, i64::MIN], Ok(0)),
    ] {
        let nodes = [Op::Input(0), Op::Input(1), op];
        let (result, scratch, output) =
            execute(&[INTEGER, INTEGER], &[INTEGER], &nodes, &[2], &input);
        match expected {
            Ok(value) => {
                assert_eq!(result, Ok(()));
                assert_eq!(output, [value]);
            }
            Err(error) => {
                assert_eq!(result, Err(error));
                assert!(scratch.is_empty());
                assert!(output.is_empty());
            }
        }
    }
}

#[test]
fn every_refusal_clears_both_buffers() {
    for (inputs, outputs, nodes, roots, input, expected) in [
        (
            vec![Domain::Bool],
            vec![Domain::Bool],
            vec![Op::Input(0)],
            vec![0],
            vec![2],
            Failure::InputDomain,
        ),
        (
            vec![INTEGER],
            vec![INTEGER],
            vec![Op::Input(0)],
            vec![0],
            vec![],
            Failure::InputDomain,
        ),
        (
            vec![],
            vec![INTEGER],
            vec![Op::Input(0)],
            vec![0],
            vec![],
            Failure::Reference,
        ),
        (
            vec![],
            vec![INTEGER],
            vec![Op::Int(5)],
            vec![1],
            vec![],
            Failure::Reference,
        ),
        (
            vec![],
            vec![INTEGER],
            vec![Op::Add(0, 0)],
            vec![0],
            vec![],
            Failure::Reference,
        ),
        (
            vec![],
            vec![Domain::Bool],
            vec![Op::Int(2)],
            vec![0],
            vec![],
            Failure::OutputDomain,
        ),
        (
            vec![],
            vec![INTEGER, INTEGER],
            vec![Op::Int(5)],
            vec![0],
            vec![],
            Failure::OutputDomain,
        ),
    ] {
        let (result, scratch, output) = execute(&inputs, &outputs, &nodes, &roots, &input);
        assert_eq!(result, Err(expected));
        assert!(scratch.is_empty());
        assert!(output.is_empty());
    }
}

#[test]
fn unselected_and_unused_nodes_still_trap_eagerly() {
    let nodes = [
        Op::Bool(false),
        Op::Int(i64::MAX),
        Op::Int(1),
        Op::Add(1, 2),
        Op::Int(7),
        Op::Select(0, 3, 4),
    ];
    for root in [4, 5] {
        assert_eq!(validate_program(&[], &[INTEGER], &nodes, &[root]), Ok(()));
        let (result, scratch, output) = execute(&[], &[INTEGER], &nodes, &[root], &[]);
        assert_eq!(result, Err(Failure::Arithmetic));
        assert!(scratch.is_empty());
        assert!(output.is_empty());
    }
}

#[test]
fn structural_admission_rejects_bad_shape_types_and_topology_in_order() {
    use AdmissionFailure::{InputReference, NodeReference, OutputType, Shape, TypeMismatch};
    assert_eq!(validate_program(&[], &[], &[Op::Int(0)], &[]), Err(Shape));
    assert_eq!(validate_program(&[], &[INTEGER], &[], &[0]), Err(Shape));
    assert_eq!(
        validate_program(&[], &[INTEGER], &[Op::Int(0)], &[]),
        Err(Shape)
    );
    assert_eq!(
        validate_program(
            &[Domain::Int { min: 2, max: 1 }],
            &[INTEGER],
            &[Op::Int(0)],
            &[0]
        ),
        Err(Shape)
    );
    assert_eq!(
        validate_program(&[INTEGER; 33], &[INTEGER], &[Op::Int(0)], &[0]),
        Err(Shape)
    );
    assert_eq!(
        validate_program(&[], &[INTEGER; 17], &[Op::Int(0)], &[0; 17]),
        Err(Shape)
    );
    assert_eq!(
        validate_program(&[], &[INTEGER], &vec![Op::Int(0); 257], &[0]),
        Err(Shape)
    );
    assert_eq!(
        validate_program(&[], &[INTEGER], &vec![Op::Int(0); 256], &[255]),
        Ok(())
    );
    assert_eq!(
        validate_program(&[], &[INTEGER], &[Op::Input(0)], &[0]),
        Err(InputReference)
    );
    assert_eq!(
        validate_program(&[], &[INTEGER], &[Op::Add(0, 0)], &[0]),
        Err(NodeReference)
    );
    assert_eq!(
        validate_program(&[], &[Domain::Bool], &[Op::Int(0)], &[0]),
        Err(OutputType)
    );
    assert_eq!(
        validate_program(&[], &[INTEGER], &[Op::Int(0)], &[1]),
        Err(OutputType)
    );
    assert_eq!(kind(&Op::Add(0, u16::MAX), &[], &[true]), Err(TypeMismatch));
    assert_eq!(
        kind(&Op::And(0, u16::MAX), &[], &[false]),
        Err(TypeMismatch)
    );
    assert_eq!(kind(&Op::Eq(0, u16::MAX), &[], &[true]), Err(NodeReference));
    assert_eq!(
        kind(&Op::Select(0, u16::MAX, u16::MAX), &[], &[false]),
        Err(TypeMismatch)
    );
    assert_eq!(kind(&Op::Eq(0, 1), &[], &[false, true]), Err(TypeMismatch));
    assert_eq!(
        kind(&Op::Select(0, 1, 2), &[], &[true, false, true]),
        Err(TypeMismatch)
    );
}
