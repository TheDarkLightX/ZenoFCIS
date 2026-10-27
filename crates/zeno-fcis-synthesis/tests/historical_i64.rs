//! Independent eager-overflow witnesses; scalar results confer no authority.
use zeno_fcis_synthesis::finite::{
    Domain as CurrentDomain, Op as CurrentOp, V2Resource as Resource, execute_v2,
    historical_i64 as released, v2_zero_limits,
};

#[test]
fn unused_node_overflow() {
    let full = released::Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let nodes = [
        released::Op::Int(0),
        released::Op::Int(i64::MAX),
        released::Op::Int(1),
        released::Op::Add(1, 2),
        released::Op::Int(7),
    ];
    let limits = v2_zero_limits().with_limit(Resource::Step, 256);
    let (result, usage) = released::execute(&[], &[full], &nodes, &[0], &[], limits).into_parts();
    assert_eq!(result, Err(released::Failure::Arithmetic));
    assert_eq!(usage.used(Resource::Step), 4);
    let current = [
        CurrentOp::Int(0),
        CurrentOp::Int(i128::from(i64::MAX)),
        CurrentOp::Int(1),
        CurrentOp::Add(1, 2),
        CurrentOp::Int(7),
    ];
    let (result, usage) = execute_v2(
        &[],
        &[CurrentDomain::Int {
            min: i128::MIN,
            max: i128::MAX,
        }],
        &current,
        &[0],
        &[],
        limits,
    )
    .into_parts();
    assert_eq!(result, Ok(vec![0]));
    assert_eq!(usage.used(Resource::Step), 5);
}

#[test]
fn unselected_arm_overflow() {
    let full = released::Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let nodes = [
        released::Op::Bool(false),
        released::Op::Int(i64::MAX),
        released::Op::Int(1),
        released::Op::Add(1, 2),
        released::Op::Int(0),
        released::Op::Select(0, 3, 4),
    ];
    let limits = v2_zero_limits().with_limit(Resource::Step, 256);
    for _ in 0..2 {
        let (result, usage) =
            released::execute(&[], &[full], &nodes, &[5], &[], limits).into_parts();
        assert_eq!(result, Err(released::Failure::Arithmetic));
        assert_eq!(usage.used(Resource::Step), 4);
    }
    let (result, usage) = released::execute(
        &[],
        &[full],
        &nodes,
        &[5],
        &[],
        limits.with_limit(Resource::Step, 3),
    )
    .into_parts();
    assert!(matches!(result, Err(released::Failure::Budget(_))));
    assert_eq!(usage.used(Resource::Step), 3);
}
