//! Public typed API executes full-width values without publication authority.
use std::vec;
use zeno_fcis_synthesis::finite::{
    V2IntegerWidth as Width, V2NumericAtom as Atom, V2NumericDomain as Domain, V2NumericOp as Op,
    V2NumericProgram as Program, V2Resource as Resource, execute_numeric_v2, v2_zero_limits,
};

#[test]
fn unsigned_deposit_exceeds_i128_through_public_shared_interpreter() {
    let domain = Domain::U128 {
        min: 0,
        max: u128::MAX,
    };
    let program = Program {
        inputs: &[domain],
        outputs: &[domain],
        nodes: &[Op::Input(0), Op::U128(1), Op::Add(Width::Unsigned128, 0, 1)],
        roots: &[2],
    };
    assert_eq!(program.admit(), Ok(()));
    let (result, usage) = execute_numeric_v2(
        program,
        &[Atom::U128(1 << 63)],
        v2_zero_limits().with_limit(Resource::Step, 3),
    )
    .into_parts();
    assert_eq!(result, Ok(vec![Atom::U128((1 << 63) + 1)]));
    assert_eq!(usage.used(Resource::Step), 3);
}

#[test]
fn all_numeric_endpoints_cross_the_public_graph_boundary() {
    let signed = Domain::I128 {
        min: i128::MIN,
        max: i128::MAX,
    };
    let unsigned = Domain::U128 {
        min: 0,
        max: u128::MAX,
    };
    let program = Program {
        inputs: &[signed, signed, unsigned],
        outputs: &[signed, signed, unsigned, Domain::Bool, Domain::Bool],
        nodes: &[
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::U128(1 << 127),
            Op::Eq(2, 3),
            Op::Eq(3, 3),
        ],
        roots: &[0, 1, 2, 4, 5],
    };
    let (result, usage) = execute_numeric_v2(
        program,
        &[
            Atom::I128(i128::MIN),
            Atom::I128(i128::MAX),
            Atom::U128(u128::MAX),
        ],
        v2_zero_limits().with_limit(Resource::Step, 6),
    )
    .into_parts();
    assert_eq!(
        result,
        Ok(vec![
            Atom::I128(i128::MIN),
            Atom::I128(i128::MAX),
            Atom::U128(u128::MAX),
            Atom::Bool(false),
            Atom::Bool(true)
        ])
    );
    assert_eq!(usage.used(Resource::Step), 6);
}
