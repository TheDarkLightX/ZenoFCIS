//! Private feasibility regressions; no production or release qualification.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::{
    finite::{Domain, Op, Program, V2ExecutionFailure, V2Resource, v2_zero_limits},
    finite_runtime,
};

fn full() -> Domain {
    Domain::Int {
        min: i128::MIN,
        max: i128::MAX,
    }
}

fn graph(nodes: Vec<Op>, roots: Vec<u16>) -> Program {
    Program::try_new(vec![], roots.iter().map(|_| full()).collect(), nodes, roots)
        .unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn scalar_crosses_old_machine_bound_and_roundtrips_full_width_literals() {
    let old_max = i128::from(i64::MAX);
    let program = graph(vec![Op::Int(old_max), Op::Int(1), Op::Add(0, 1)], vec![2]);
    assert_eq!(
        program.evaluate(&[]).unwrap_or_else(|e| panic!("{e:?}")),
        vec![1i128 << 63]
    );
    for value in [
        i128::MIN,
        i128::MIN + 1,
        -(1i128 << 100),
        1i128 << 100,
        i128::MAX - 1,
        i128::MAX,
    ] {
        let literal = graph(vec![Op::Int(value)], vec![0]);
        let bytes = literal
            .value()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            finite_runtime::import_program(&bytes)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .evaluate(&[])
                .unwrap_or_else(|e| panic!("{e:?}")),
            vec![value]
        );
    }
}

#[test]
fn unused_i128_trap_refuses_and_retains_attempted_steps_on_retry() {
    let program = graph(vec![Op::Int(i128::MAX), Op::Int(1), Op::Add(0, 1)], vec![1]);
    let limits = v2_zero_limits().with_limit(V2Resource::Step, 3);
    for _ in 0..2 {
        let (result, usage) = program.execute_v2(&[], limits).into_parts();
        assert_eq!(result, Err(V2ExecutionFailure::Arithmetic));
        assert_eq!(usage.used(V2Resource::Step), 3);
        for resource in [V2Resource::Read, V2Resource::Byte, V2Resource::Effect] {
            assert_eq!(usage.used(resource), 0);
        }
    }
    let (result, usage) = program
        .execute_v2(&[], limits.with_limit(V2Resource::Step, 2))
        .into_parts();
    assert!(matches!(result, Err(V2ExecutionFailure::Budget(_))));
    assert_eq!(usage.used(V2Resource::Step), 2);
}

#[test]
fn unselected_branch_is_eager_and_signed_minimum_subtraction_traps() {
    let program = graph(
        vec![
            Op::Bool(false),
            Op::Int(i128::MIN),
            Op::Int(1),
            Op::Sub(1, 2),
            Op::Select(0, 3, 2),
        ],
        vec![4],
    );
    let limits = v2_zero_limits().with_limit(V2Resource::Step, 5);
    let (result, usage) = program.execute_v2(&[], limits).into_parts();
    assert_eq!(result, Err(V2ExecutionFailure::Arithmetic));
    assert_eq!(usage.used(V2Resource::Step), 4);
}

#[test]
fn input_and_output_domains_remain_distinct_from_machine_width() {
    let program = Program::try_new(
        vec![Domain::Int {
            min: 1i128 << 100,
            max: (1i128 << 100) + 1,
        }],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![Op::Input(0)],
        vec![0],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let limits = v2_zero_limits().with_limit(V2Resource::Step, 1);
    let (outside, usage) = program.execute_v2(&[0], limits).into_parts();
    assert_eq!(outside, Err(V2ExecutionFailure::InputDomain));
    assert_eq!(usage.used(V2Resource::Step), 0);
    let (output, usage) = program.execute_v2(&[1i128 << 100], limits).into_parts();
    assert_eq!(output, Err(V2ExecutionFailure::OutputDomain));
    assert_eq!(usage.used(V2Resource::Step), 1);
}
