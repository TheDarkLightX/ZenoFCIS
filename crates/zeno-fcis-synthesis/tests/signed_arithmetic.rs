//! D7 native qualification targets. Written here; execution is a separate gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{
    ARITHMETIC_EXTENDED_PROFILE, ARITHMETIC_PROFILE, Domain, Error, Op, Program,
    V2ExecutionFailure as Failure, V2NumericAdmissionFailure as AdmissionFailure,
    V2NumericAtom as Atom, V2NumericDomain as NumericDomain, V2NumericFailure as NumericFailure,
    V2NumericOp as NumericOp, V2NumericProgram as NumericProgram, V2Resource as Resource,
    execute_numeric_v2, v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::{self, ImportedScalarProgram};
use zeno_fcis_value::{Value, ValueRef};
const FULL: Domain = Domain::Int {
    min: i128::MIN,
    max: i128::MAX,
};
const TYPED: NumericDomain<'static> = NumericDomain::I128 {
    min: i128::MIN,
    max: i128::MAX,
};
fn program() -> Program {
    Program::try_new(
        vec![FULL; 3],
        vec![FULL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::SignedMulDivFloor(0, 1, 2),
        ],
        vec![3],
    )
    .unwrap()
}
#[test]
fn signed_oracle_matches_scalar_typed_decoded_and_actual_usage() {
    let p = program();
    assert_eq!(p.profile(), ARITHMETIC_PROFILE);
    let bytes = p.value().unwrap().canonical_bytes().unwrap();
    let decoded = finite_runtime::import_program(&bytes).unwrap();
    let typed_nodes = [
        NumericOp::Input(0),
        NumericOp::Input(1),
        NumericOp::Input(2),
        NumericOp::SignedMulDivFloor(0, 1, 2),
    ];
    let typed = NumericProgram {
        inputs: &[TYPED; 3],
        outputs: &[TYPED],
        nodes: &typed_nodes,
        roots: &[3],
    };
    assert_eq!(typed.admit(), Ok(()));
    let limits = v2_zero_limits().with_limit(Resource::Step, 4);
    let mut count = 0;
    for line in include_str!("../../../tools/test-data/signed-arithmetic/vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        let input: Vec<i128> = f[..3].iter().map(|n| n.parse().unwrap()).collect();
        let expected = if f[3] == "arithmetic" {
            None
        } else {
            Some(f[3].parse::<i128>().unwrap())
        };
        let want = expected.map(|n| vec![n]).ok_or(Error::Arithmetic);
        assert_eq!(p.evaluate(&input), want, "{line}");
        assert_eq!(decoded.evaluate(&input), want, "decoded {line}");
        for _ in 0..2 {
            let (actual, usage) = p.execute_v2(&input, limits).into_parts();
            assert_eq!(
                actual,
                expected.map(|n| vec![n]).ok_or(Failure::Arithmetic),
                "{line}"
            );
            assert_eq!(usage.used(Resource::Step), 4);
            let atoms: Vec<_> = input.iter().copied().map(Atom::I128).collect();
            let (actual, usage) = execute_numeric_v2(typed, &atoms, limits).into_parts();
            assert_eq!(
                actual,
                expected
                    .map(|n| vec![Atom::I128(n)])
                    .ok_or(NumericFailure::Arithmetic),
                "typed {line}"
            );
            assert_eq!(usage.used(Resource::Step), 4);
            assert_eq!(usage.used(Resource::Write), 0);
            assert_eq!(usage.used(Resource::Effect), 0);
        }
        println!(
            "SIGNED_ARITHMETIC {} {} {} {} 4",
            input[0], input[1], input[2], f[3]
        );
        count += 1;
    }
    assert_eq!(count, 2456);
}
#[test]
fn signed_node_keeps_budget_reference_kind_and_admission_order() {
    let nodes = [
        NumericOp::Bool(false),
        NumericOp::I128(0),
        NumericOp::SignedMulDivFloor(0, 1, 99),
    ];
    let p = NumericProgram {
        inputs: &[],
        outputs: &[TYPED],
        nodes: &nodes,
        roots: &[2],
    };
    // Static admission checks operand kinds in declared order.
    assert_eq!(p.admit(), Err(AdmissionFailure::TypeMismatch));
    // Defensive execution resolves every reference before checking any kind or divisor.
    for (steps, expected) in [(2, None), (3, Some(NumericFailure::Reference))] {
        let (r, u) = execute_numeric_v2(p, &[], v2_zero_limits().with_limit(Resource::Step, steps))
            .into_parts();
        if let Some(e) = expected {
            assert_eq!(r, Err(e));
        } else {
            assert!(matches!(r, Err(NumericFailure::Budget(_))));
        }
        assert_eq!(u.used(Resource::Step), steps);
    }
    let nodes = [
        NumericOp::U128(0),
        NumericOp::I128(0),
        NumericOp::I128(-1),
        NumericOp::SignedMulDivFloor(0, 1, 2),
    ];
    let p = NumericProgram {
        inputs: &[],
        outputs: &[TYPED],
        nodes: &nodes,
        roots: &[3],
    };
    let (r, u) =
        execute_numeric_v2(p, &[], v2_zero_limits().with_limit(Resource::Step, 4)).into_parts();
    assert_eq!(r, Err(NumericFailure::Kind));
    assert_eq!(u.used(Resource::Step), 4);
    let (r, u) = program()
        .execute_v2(&[0, 1], v2_zero_limits().with_limit(Resource::Step, 4))
        .into_parts();
    assert_eq!(r, Err(Failure::InputDomain));
    assert_eq!(u.used(Resource::Step), 0);
}
#[test]
fn signed_traps_are_eager_even_when_unused_or_unselected() {
    for d in [0, -1] {
        let p = Program::try_new(
            vec![],
            vec![FULL],
            vec![
                Op::Int(0),
                Op::Int(d),
                Op::SignedMulDivFloor(0, 0, 1),
                Op::Bool(false),
                Op::Select(3, 2, 0),
            ],
            vec![0],
        )
        .unwrap();
        for _ in 0..2 {
            let (r, u) = p
                .execute_v2(&[], v2_zero_limits().with_limit(Resource::Step, 5))
                .into_parts();
            assert_eq!(r, Err(Failure::Arithmetic));
            assert_eq!(u.used(Resource::Step), 3);
        }
    }
}
#[test]
fn signed_opcode_cannot_enter_old_width_or_old_language_profiles() {
    let p = program();
    let value = p.value().unwrap();
    let ValueRef::Tuple(fields) = value.view() else {
        panic!("tuple")
    };
    for profile in [
        "zeno-fcis/finite-i128/1",
        "zeno-fcis/finite-i128/2",
        "zeno-fcis/finite-i64/1",
        "zeno-fcis/finite-i64/2",
        "zeno-fcis/finite-i64/3",
        "zeno-fcis/finite-i64/4",
    ] {
        let mut fields = fields.to_vec();
        fields[0] = Value::text_ascii(profile.into()).unwrap();
        let bytes = Value::tuple(fields).unwrap().canonical_bytes().unwrap();
        assert!(
            finite_runtime::import_scalar_program(&bytes).is_err(),
            "{profile}"
        );
    }
    let mut nodes = vec![Op::Int(1); 4095];
    nodes.push(Op::SignedMulDivFloor(0, 0, 0));
    let p = Program::try_new_extended(vec![], vec![FULL], nodes, vec![4095]).unwrap();
    assert_eq!(p.profile(), ARITHMETIC_EXTENDED_PROFILE);
    let bytes = p.value().unwrap().canonical_bytes().unwrap();
    assert!(finite_runtime::import_program(&bytes).is_err());
    let ImportedScalarProgram::I128(decoded) =
        finite_runtime::import_scalar_program(&bytes).unwrap()
    else {
        panic!("route")
    };
    assert_eq!(decoded.evaluate(&[]), Ok(vec![1]));
}
