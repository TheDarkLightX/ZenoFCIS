//! Boundary witnesses stated independently of the interpreter/specification.
use super::*;
use alloc::vec;
use typed::NumericOp as N;
use value::IntegerWidth::{Signed64, Signed128, Unsigned128};

const SIGNED: Domain<'static> = Domain::I128 {
    min: i128::MIN,
    max: i128::MAX,
};
const UNSIGNED: Domain<'static> = Domain::U128 {
    min: 0,
    max: u128::MAX,
};

fn run<'a>(
    program: NumericProgram<'_>,
    input: &[Atom<'a>],
    quota: u64,
) -> (Result<Vec<Atom<'a>>, Failure>, u64) {
    let (result, usage) = execute(
        program,
        input,
        super::super::zero_limits().with_limit(Resource::Step, quota),
    )
    .into_parts();
    for resource in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
    ] {
        assert_eq!(usage.used(resource), 0);
    }
    (result, usage.used(Resource::Step))
}

#[test]
fn full_width_input_and_output_endpoints_survive() {
    let domains = [SIGNED, SIGNED, UNSIGNED, UNSIGNED];
    let program = NumericProgram {
        inputs: &domains,
        outputs: &domains,
        nodes: &[N::Input(0), N::Input(1), N::Input(2), N::Input(3)],
        roots: &[0, 1, 2, 3],
    };
    assert_eq!(program.admit(), Ok(()));
    let input = [
        Atom::I128(i128::MIN),
        Atom::I128(i128::MAX),
        Atom::U128(1 << 63),
        Atom::U128(u128::MAX),
    ];
    assert_eq!(run(program, &input, 4), (Ok(input.to_vec()), 4));
}

#[test]
fn signed_unsigned_comparison_and_selection_preserve_kinds() {
    let program = NumericProgram {
        inputs: &[],
        outputs: &[Domain::Bool, Domain::Bool, SIGNED, UNSIGNED],
        nodes: &[
            N::I128(i128::MIN),
            N::I128(i128::MAX),
            N::Lt(0, 1),
            N::U128(1 << 127),
            N::U128(u128::MAX),
            N::Lt(3, 4),
            N::Select(2, 0, 1),
            N::Select(5, 4, 3),
        ],
        roots: &[2, 5, 6, 7],
    };
    assert_eq!(program.admit(), Ok(()));
    assert_eq!(
        run(program, &[], 8),
        (
            Ok(vec![
                Atom::Bool(true),
                Atom::Bool(true),
                Atom::I128(i128::MIN),
                Atom::U128(u128::MAX)
            ]),
            8
        )
    );
}

#[test]
fn checked_arithmetic_works_beyond_legacy_width() {
    let program = NumericProgram {
        inputs: &[UNSIGNED, SIGNED],
        outputs: &[UNSIGNED, SIGNED],
        nodes: &[
            N::Input(0),
            N::U128(1),
            N::Add(Unsigned128, 0, 1),
            N::Input(1),
            N::I128(1),
            N::Sub(Signed128, 3, 4),
        ],
        roots: &[2, 5],
    };
    assert_eq!(program.admit(), Ok(()));
    assert_eq!(
        run(
            program,
            &[Atom::U128(1 << 63), Atom::I128(i64::MIN as i128)],
            6
        ),
        (
            Ok(vec![
                Atom::U128((1 << 63) + 1),
                Atom::I128(i64::MIN as i128 - 1)
            ]),
            6
        )
    );
}

#[test]
fn overflow_and_width_operand_bounds_are_distinct_from_kind() {
    for nodes in [
        vec![N::I128(i128::MAX), N::I128(1), N::Add(Signed128, 0, 1)],
        vec![N::I128(i128::MIN), N::I128(1), N::Sub(Signed128, 0, 1)],
        vec![N::U128(u128::MAX), N::U128(1), N::Add(Unsigned128, 0, 1)],
        vec![N::U128(0), N::U128(1), N::Sub(Unsigned128, 0, 1)],
        // Mathematical result is zero, but Signed64 must also reject wide operands.
        vec![N::I128(1 << 63), N::I128(1 << 63), N::Sub(Signed64, 0, 1)],
        vec![
            N::I128(i64::MAX as i128),
            N::I128(1),
            N::Add(Signed64, 0, 1),
        ],
    ] {
        let program = NumericProgram {
            inputs: &[],
            outputs: &[SIGNED],
            nodes: &nodes,
            roots: &[2],
        };
        assert_eq!(run(program, &[], 3), (Err(Failure::Arithmetic), 3));
    }
    let program = NumericProgram {
        inputs: &[],
        outputs: &[SIGNED],
        nodes: &[N::I128(1), N::U128(1), N::Add(Signed128, 0, 1)],
        roots: &[2],
    };
    assert_eq!(run(program, &[], 3), (Err(Failure::Kind), 3));
    assert_eq!(
        program.admit(),
        Err(evaluation::admission::AdmissionFailure::TypeMismatch)
    );
}

#[test]
fn typed_boolean_provenance_is_not_integer_truthiness() {
    let good = NumericProgram {
        inputs: &[Domain::Bool],
        outputs: &[Domain::Bool],
        nodes: &[
            N::Input(0),
            N::Bool(true),
            N::And(0, 1),
            N::Not(2),
            N::Eq(0, 3),
        ],
        roots: &[4],
    };
    assert_eq!(good.admit(), Ok(()));
    assert_eq!(
        run(good, &[Atom::Bool(false)], 5),
        (Ok(vec![Atom::Bool(false)]), 5)
    );
    assert_eq!(
        run(good, &[Atom::I128(0)], 5),
        (Err(Failure::InputDomain), 0)
    );
    let bad = NumericProgram {
        inputs: &[],
        outputs: &[Domain::Bool],
        nodes: &[N::I128(1), N::Not(0)],
        roots: &[1],
    };
    assert_eq!(run(bad, &[], 2), (Err(Failure::Kind), 2));
    assert_eq!(
        bad.admit(),
        Err(evaluation::admission::AdmissionFailure::TypeMismatch)
    );
}

#[test]
fn budget_then_all_references_then_kind_then_range() {
    let program = NumericProgram {
        inputs: &[],
        outputs: &[SIGNED],
        nodes: &[N::Bool(true), N::Add(Signed128, 0, 99)],
        roots: &[1],
    };
    assert_eq!(run(program, &[], 2), (Err(Failure::Reference), 2));
    let (result, used) = run(program, &[], 1);
    assert_eq!(used, 1);
    assert!(matches!(
        result,
        Err(Failure::Budget(MeterFailure {
            resource: Resource::Step,
            limit: 1,
            attempted: 2,
            overflow: false
        }))
    ));
    let no_budget = run(
        NumericProgram {
            inputs: &[],
            outputs: &[],
            nodes: &[N::Input(99)],
            roots: &[],
        },
        &[],
        0,
    );
    assert!(matches!(no_budget, (Err(Failure::Budget(_)), 0)));
}

#[test]
fn select_checks_the_unselected_reference_and_kind() {
    let bad_ref = NumericProgram {
        inputs: &[],
        outputs: &[SIGNED],
        nodes: &[N::Bool(true), N::I128(7), N::Select(0, 1, 9)],
        roots: &[2],
    };
    assert_eq!(run(bad_ref, &[], 3), (Err(Failure::Reference), 3));
    let bad_kind = NumericProgram {
        inputs: &[],
        outputs: &[SIGNED],
        nodes: &[N::Bool(true), N::I128(7), N::U128(8), N::Select(0, 1, 2)],
        roots: &[3],
    };
    assert_eq!(run(bad_kind, &[], 4), (Err(Failure::Kind), 4));
}

#[test]
fn unused_and_unselected_nodes_still_trap() {
    let nodes = [
        N::Bool(true),
        N::U128(u128::MAX),
        N::U128(1),
        N::Add(Unsigned128, 1, 2),
        N::Select(0, 2, 3),
    ];
    for roots in [&[2u16][..], &[4][..]] {
        assert_eq!(
            run(
                NumericProgram {
                    inputs: &[],
                    outputs: &[UNSIGNED],
                    nodes: &nodes,
                    roots
                },
                &[],
                5
            ),
            (Err(Failure::Arithmetic), 4)
        );
    }
}

#[test]
fn domain_root_and_output_refusals_have_original_order() {
    let input_bad = NumericProgram {
        inputs: &[UNSIGNED],
        outputs: &[SIGNED],
        nodes: &[N::Input(99)],
        roots: &[99],
    };
    assert_eq!(
        run(input_bad, &[Atom::I128(-1)], 0),
        (Err(Failure::InputDomain), 0)
    );
    let root_bad = NumericProgram {
        inputs: &[],
        outputs: &[SIGNED],
        nodes: &[N::U128(3)],
        roots: &[99],
    };
    assert_eq!(run(root_bad, &[], 1), (Err(Failure::Reference), 1));
    let output_bad = NumericProgram {
        roots: &[0],
        ..root_bad
    };
    assert_eq!(run(output_bad, &[], 1), (Err(Failure::OutputDomain), 1));
    assert!(matches!(
        run(output_bad, &[], 0),
        (Err(Failure::Budget(_)), 0)
    ));
}

#[test]
fn compound_domains_are_not_numeric_authority() {
    let program = NumericProgram {
        inputs: &[Domain::Bytes],
        outputs: &[Domain::Bytes],
        nodes: &[N::Input(0)],
        roots: &[0],
    };
    assert_eq!(
        program.admit(),
        Err(evaluation::admission::AdmissionFailure::Shape)
    );
    assert_eq!(
        run(program, &[Atom::Bytes(b"full payload")], 1),
        (Err(Failure::InputDomain), 0)
    );
}

#[test]
fn raw_legacy_integer_boolean_semantics_and_domain_are_preserved() {
    use super::super::{execute as legacy, zero_limits};
    let nodes = [
        Op::Int(2),
        Op::Int(1),
        Op::Int(9),
        Op::Not(0),
        Op::And(0, 1),
        Op::Select(0, 1, 2),
    ];
    let outputs = [
        LegacyDomain::Bool,
        LegacyDomain::Bool,
        LegacyDomain::Int { min: 9, max: 9 },
    ];
    let (result, usage) = legacy(
        &[],
        &outputs,
        &nodes,
        &[3, 4, 5],
        &[],
        zero_limits().with_limit(Resource::Step, 6),
    )
    .into_parts();
    assert_eq!(result, Ok(vec![0, 0, 9]));
    assert_eq!(usage.used(Resource::Step), 6);
    for value in [0, 1] {
        assert_eq!(
            legacy(
                &[LegacyDomain::Bool],
                &[LegacyDomain::Bool],
                &[Op::Input(0)],
                &[0],
                &[value],
                zero_limits().with_limit(Resource::Step, 1)
            )
            .into_parts()
            .0,
            Ok(vec![value])
        );
    }
    let (result, usage) = legacy(
        &[LegacyDomain::Bool],
        &[],
        &[Op::Input(99)],
        &[],
        &[2],
        zero_limits(),
    )
    .into_parts();
    assert_eq!(result, Err(super::super::Failure::InputDomain));
    assert_eq!(usage.used(Resource::Step), 0);
}

#[test]
fn raw_legacy_execution_remains_outside_admission_caps() {
    use super::super::{execute as legacy, zero_limits};
    let input_domains = vec![LegacyDomain::Int { min: 7, max: 7 }; 33];
    let output_domains = vec![LegacyDomain::Int { min: 7, max: 7 }; 17];
    let nodes = vec![Op::Input(32); 257];
    let roots = vec![256; 17];
    let (result, usage) = legacy(
        &input_domains,
        &output_domains,
        &nodes,
        &roots,
        &[7; 33],
        zero_limits().with_limit(Resource::Step, 257),
    )
    .into_parts();
    assert_eq!(result, Ok(vec![7; 17]));
    assert_eq!(usage.used(Resource::Step), 257);
    let (result, usage) = legacy(
        &input_domains,
        &output_domains,
        &nodes,
        &roots,
        &[7; 33],
        zero_limits().with_limit(Resource::Step, 256),
    )
    .into_parts();
    assert!(matches!(result, Err(super::super::Failure::Budget(_))));
    assert_eq!(usage.used(Resource::Step), 256);
}

#[test]
fn old_admission_retains_bool_provenance_from_domain_and_literal() {
    use evaluation::admission::{self, AdmissionFailure};
    // Equal encoded integer values have distinct original static kinds.
    assert_eq!(
        admission::kind(&Op::Input(0), &[LegacyDomain::Bool], &[]),
        Ok(true)
    );
    assert_eq!(admission::kind(&Op::Bool(false), &[], &[]), Ok(true));
    assert_eq!(admission::kind(&Op::Int(0), &[], &[]), Ok(false));
    assert_eq!(
        admission::kind(&Op::Not(0), &[], &[false]),
        Err(AdmissionFailure::TypeMismatch)
    );
}

#[test]
fn moved_membership_retains_nominal_and_payload_domains() {
    assert!(value::admitted(
        Domain::Enum {
            type_id: 7,
            variants: &[2, 9]
        },
        Atom::Enum {
            type_id: 7,
            variant: 9
        }
    ));
    assert!(!value::admitted(
        Domain::Enum {
            type_id: 7,
            variants: &[2, 9]
        },
        Atom::Enum {
            type_id: 8,
            variant: 9
        }
    ));
    assert!(!value::admitted(
        Domain::Enum {
            type_id: 7,
            variants: &[2, 9]
        },
        Atom::Sum {
            type_id: 7,
            variant: 9
        }
    ));
    assert!(value::admitted(
        Domain::Sum {
            type_id: 8,
            variants: &[1]
        },
        Atom::Sum {
            type_id: 8,
            variant: 1
        }
    ));
    assert!(value::admitted(Domain::Bytes, Atom::Bytes(b"\0\xff")));
    assert!(value::admitted(
        Domain::Text,
        Atom::Text(b"uninterpreted here")
    ));
}

#[test]
fn contracted_kind_equality_matches_every_existing_derived_case() {
    use typed::Kind;
    let kinds = [Kind::Bool, Kind::Signed, Kind::Unsigned];
    for a in kinds {
        for b in kinds {
            assert_eq!(typed::kind_equals(a, b), a == b);
        }
    }
    let optional = [
        None,
        Some(Kind::Bool),
        Some(Kind::Signed),
        Some(Kind::Unsigned),
    ];
    for a in optional {
        for b in optional {
            assert_eq!(typed::option_kind_equals(a, b), a == b);
        }
    }
}

#[test]
fn wider_numeric_admission_does_not_change_legacy_shape_limits() {
    use evaluation::admission::{AdmissionFailure, validate_shape};
    assert_eq!((evaluation::MAX_FIELDS, evaluation::MAX_NODES), (16, 256));
    let inputs = [LegacyDomain::Bool; 32];
    let outputs = [LegacyDomain::Bool; 16];
    let roots = [0; 16];
    assert_eq!(validate_shape(&inputs, &outputs, 256, &roots), Ok(()));
    assert_eq!(
        validate_shape(&inputs, &outputs, 257, &roots),
        Err(AdmissionFailure::Shape)
    );
    assert_eq!(
        validate_shape(&[LegacyDomain::Bool; 33], &outputs, 256, &roots),
        Err(AdmissionFailure::Shape)
    );
    assert_eq!(
        validate_shape(&inputs, &[LegacyDomain::Bool; 17], 256, &[0; 17]),
        Err(AdmissionFailure::Shape)
    );
}
