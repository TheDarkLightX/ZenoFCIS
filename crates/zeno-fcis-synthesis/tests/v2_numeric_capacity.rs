//! Numeric profile capacity, exact dependent execution and denied work.
use std::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    MAX_NUMERIC_INPUTS, MAX_NUMERIC_NODES, MAX_NUMERIC_OUTPUTS, V2IntegerWidth as Width,
    V2NumericAdmissionFailure as AdmissionFailure, V2NumericAtom as Atom,
    V2NumericDomain as Domain, V2NumericFailure as Failure, V2NumericOp as Op,
    V2NumericProgram as Program, V2Resource as Resource, execute_numeric_v2, v2_zero_limits,
};

const SIGNED: Domain<'static> = Domain::I128 {
    min: i128::MIN,
    max: i128::MAX,
};
const UNSIGNED: Domain<'static> = Domain::U128 {
    min: 0,
    max: u128::MAX,
};

#[test]
fn sixty_four_inputs_and_outputs_keep_full_width_values_and_order() {
    assert_eq!((MAX_NUMERIC_INPUTS, MAX_NUMERIC_OUTPUTS), (64, 64));
    let domains: Vec<_> = (0u16..64)
        .map(|i| if i % 4 < 2 { SIGNED } else { UNSIGNED })
        .collect();
    let input: Vec<_> = (0u16..64)
        .map(|i| match i % 4 {
            0 => Atom::I128(i128::MIN),
            1 => Atom::I128(i128::MAX),
            2 => Atom::U128(0),
            _ => Atom::U128(u128::MAX),
        })
        .collect();
    let nodes: Vec<_> = (0u16..64).map(Op::Input).collect();
    let roots: Vec<_> = (0u16..64).rev().collect();
    let outputs: Vec<_> = domains.iter().rev().copied().collect();
    let program = Program {
        inputs: &domains,
        outputs: &outputs,
        nodes: &nodes,
        roots: &roots,
    };
    assert_eq!(program.admit(), Ok(()));
    let (result, usage) = execute_numeric_v2(
        program,
        &input,
        v2_zero_limits().with_limit(Resource::Step, 64),
    )
    .into_parts();
    assert_eq!(result, Ok(input.iter().rev().copied().collect()));
    assert_eq!(usage.used(Resource::Step), 64);

    let too_many_inputs = vec![SIGNED; 65];
    assert_eq!(
        Program {
            inputs: &too_many_inputs,
            ..program
        }
        .admit(),
        Err(AdmissionFailure::Shape)
    );
    let too_many_outputs = vec![SIGNED; 65];
    let too_many_roots = vec![0; 65];
    assert_eq!(
        Program {
            outputs: &too_many_outputs,
            roots: &too_many_roots,
            ..program
        }
        .admit(),
        Err(AdmissionFailure::Shape)
    );
}

#[test]
fn dependent_graphs_cross_the_old_ceiling_and_stop_before_denied_work() {
    assert_eq!(MAX_NUMERIC_NODES, 4096);
    for count in [256u16, 257, 294, 4096] {
        // Every intermediate contributes to the final sum; no padding nodes.
        let mut nodes = vec![Op::Input(0), Op::I128(1)];
        for i in 2..count {
            nodes.push(Op::Add(Width::Signed128, i - 1, 1));
        }
        let roots = [count - 1];
        let program = Program {
            inputs: &[SIGNED],
            outputs: &[SIGNED],
            nodes: &nodes,
            roots: &roots,
        };
        assert_eq!(program.admit(), Ok(()));
        let (result, usage) = execute_numeric_v2(
            program,
            &[Atom::I128(1)],
            v2_zero_limits().with_limit(Resource::Step, u64::from(count)),
        )
        .into_parts();
        assert_eq!(result, Ok(vec![Atom::I128(i128::from(count) - 1)]));
        assert_eq!(usage.used(Resource::Step), u64::from(count));

        let limit = u64::from(count) - 1;
        let (result, usage) = execute_numeric_v2(
            program,
            &[Atom::I128(1)],
            v2_zero_limits().with_limit(Resource::Step, limit),
        )
        .into_parts();
        let Err(Failure::Budget(denied)) = result else {
            panic!("expected Step refusal, got {result:?}");
        };
        assert_eq!(
            (
                denied.resource,
                denied.limit,
                denied.attempted,
                denied.overflow
            ),
            (Resource::Step, limit, u64::from(count), false)
        );
        assert_eq!(usage.used(Resource::Step), limit);
    }
}

#[test]
fn a_graph_above_the_new_ceiling_is_refused_at_admission() {
    let mut nodes = vec![Op::Input(0), Op::I128(1)];
    for i in 2u16..4097 {
        nodes.push(Op::Add(Width::Signed128, i - 1, 1));
    }
    let program = Program {
        inputs: &[SIGNED],
        outputs: &[SIGNED],
        nodes: &nodes,
        roots: &[4096],
    };
    assert_eq!(program.admit(), Err(AdmissionFailure::Shape));
}
