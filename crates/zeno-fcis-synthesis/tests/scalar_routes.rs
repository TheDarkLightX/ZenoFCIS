//! Distinct scalar profiles, exact historical bytes and checked refusal usage.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::{
    finite::{
        Domain, Op, Program, V2ExecutionFailure as Failure, V2Resource as Resource, v2_zero_limits,
    },
    finite_runtime::{
        self, I64_ARITHMETIC_EXTENDED_PROFILE, I64_ARITHMETIC_PROFILE, I64_EXTENDED_PROFILE,
        I64_PROFILE, ImportedScalarProgram,
    },
};
use zeno_fcis_value::{Value, ValueLimits, ValueRef};

fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|error| panic!("scalar fixture: {error:?}"))
}
fn full_i64() -> Domain {
    Domain::Int {
        min: i128::from(i64::MIN),
        max: i128::from(i64::MAX),
    }
}
fn old_bytes(program: &Program) -> Vec<u8> {
    let value = ok(program.value());
    let ValueRef::Tuple(fields) = value.view() else {
        panic!("program tuple")
    };
    let mut fields = fields.to_vec();
    let profile = match program.profile() {
        zeno_fcis_synthesis::finite::ARITHMETIC_PROFILE => I64_ARITHMETIC_PROFILE,
        zeno_fcis_synthesis::finite::ARITHMETIC_EXTENDED_PROFILE => I64_ARITHMETIC_EXTENDED_PROFILE,
        zeno_fcis_synthesis::finite::EXTENDED_PROFILE => I64_EXTENDED_PROFILE,
        _ => I64_PROFILE,
    };
    fields[0] = ok(Value::text_ascii_with_limits(
        profile.into(),
        ValueLimits::default(),
    ));
    ok(ok(Value::tuple(fields)).canonical_bytes())
}

#[test]
fn frozen_i64_program_is_read_and_reencoded_without_relabeling() {
    let bytes = include_bytes!(
        "../../../verification/kernel-laws/src/oracle/templates/durable-counter/synthesized/program.zcve"
    );
    let program = ok(finite_runtime::import_i64_program(bytes));
    assert_eq!(program.profile(), I64_PROFILE);
    assert_eq!(ok(ok(program.value()).canonical_bytes()), bytes);
    assert!(matches!(
        ok(finite_runtime::import_scalar_program(bytes)),
        ImportedScalarProgram::I64(_)
    ));
    // The i128-only importer cannot erase an i64 semantic profile.
    assert!(finite_runtime::import_program(bytes).is_err());
}

#[test]
fn old_unused_trap_and_usage_are_distinct_from_the_wide_success() {
    let narrow = ok(Program::try_new(
        vec![],
        vec![full_i64()],
        vec![
            Op::Int(i128::from(i64::MAX)),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Int(0),
        ],
        vec![3],
    ));
    let bytes = old_bytes(&narrow);
    let old = ok(finite_runtime::import_i64_program(&bytes));
    let limits = v2_zero_limits().with_limit(Resource::Step, 4);
    for _ in 0..2 {
        let (result, usage) = old.execute_v2(&[], limits).into_parts();
        assert_eq!(result, Err(Failure::Arithmetic));
        assert_eq!(usage.used(Resource::Step), 3);
    }
    let (result, usage) = old
        .execute_v2(&[], limits.with_limit(Resource::Step, 2))
        .into_parts();
    assert!(matches!(result, Err(Failure::Budget(_))));
    assert_eq!(usage.used(Resource::Step), 2);
    let (result, usage) = narrow.execute_v2(&[], limits).into_parts();
    assert_eq!(result, Ok(vec![0]));
    assert_eq!(usage.used(Resource::Step), 4);
    assert_eq!(ok(ok(old.value()).canonical_bytes()), bytes);
}

#[test]
fn legacy_boolean_and_integer_adapter_agrees_with_an_i64_oracle() {
    let program = ok(Program::try_new(
        vec![full_i64(), full_i64(), Domain::Bool],
        vec![Domain::Bool, Domain::Bool, full_i64()],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::Int(0),
            Op::Bool(true),
            Op::Add(0, 1),
            Op::Sub(0, 1),
            Op::Eq(0, 1),
            Op::Lt(0, 1),
            Op::Not(2),
            Op::And(4, 9),
            Op::Select(10, 5, 6),
        ],
        vec![7, 8, 11],
    ));
    let old = ok(finite_runtime::import_i64_program(&old_bytes(&program)));
    let limits = v2_zero_limits().with_limit(Resource::Step, 12);
    for a in [i64::MIN, -1, 0, 1, i64::MAX] {
        for b in [i64::MIN, -1, 0, 1, i64::MAX] {
            for flag in [0, 1] {
                let expected = a.checked_add(b).ok_or(Failure::Arithmetic).and_then(|sum| {
                    a.checked_sub(b)
                        .ok_or(Failure::Arithmetic)
                        .map(|difference| {
                            vec![
                                i64::from(a == b),
                                i64::from(a < b),
                                if flag == 0 { sum } else { difference },
                            ]
                        })
                });
                let (result, usage) = old.execute_v2(&[a, b, flag], limits).into_parts();
                let steps = if a.checked_add(b).is_none() {
                    6
                } else if a.checked_sub(b).is_none() {
                    7
                } else {
                    12
                };
                assert_eq!(result, expected, "{a}, {b}, {flag}");
                assert_eq!(usage.used(Resource::Step), steps);
            }
        }
    }
    for input in [&[0, 0, 2][..], &[0, 0][..], &[0, 0, 0, 0][..]] {
        let (result, usage) = old.execute_v2(input, limits).into_parts();
        assert_eq!(result, Err(Failure::InputDomain));
        assert_eq!(usage.used(Resource::Step), 0);
    }
}

#[test]
fn extended_i64_profile_keeps_its_budget_and_bytes() {
    let program = ok(Program::try_new_extended(
        vec![],
        vec![Domain::Bool],
        vec![Op::Bool(true); 257],
        vec![0],
    ));
    let bytes = old_bytes(&program);
    let old = ok(finite_runtime::import_i64_program(&bytes));
    assert_eq!(old.profile(), I64_EXTENDED_PROFILE);
    assert_eq!(ok(ok(old.value()).canonical_bytes()), bytes);
    let mut oversized_profile_one = bytes.clone();
    let start = oversized_profile_one
        .windows(I64_EXTENDED_PROFILE.len())
        .position(|window| window == I64_EXTENDED_PROFILE.as_bytes())
        .unwrap_or_else(|| panic!("profile token missing"));
    oversized_profile_one[start + I64_EXTENDED_PROFILE.len() - 1] = b'1';
    assert!(matches!(
        finite_runtime::import_i64_program(&oversized_profile_one),
        Err(zeno_fcis_synthesis::finite::Error::Invalid(
            "program-encoding"
        ))
    ));
    let limits = v2_zero_limits().with_limit(Resource::Step, 256);
    let (result, usage) = old.execute_v2(&[], limits).into_parts();
    assert!(matches!(result, Err(Failure::Budget(_))));
    assert_eq!(usage.used(Resource::Step), 256);
    assert_eq!(
        old.execute_v2(&[], limits.with_limit(Resource::Step, 257))
            .into_parts()
            .0,
        Ok(vec![1])
    );
}

#[test]
fn wide_literals_cannot_enter_the_legacy_profile_and_routes_roundtrip_extremes() {
    let unused_wide = ok(Program::try_new(
        vec![],
        vec![full_i64()],
        vec![Op::Int(1i128 << 100), Op::Int(0)],
        vec![1],
    ));
    assert!(finite_runtime::import_i64_program(&old_bytes(&unused_wide)).is_err());
    for value in [i128::MIN, 1i128 << 100, i128::MAX] {
        let program = ok(Program::try_new(
            vec![],
            vec![Domain::Int {
                min: value,
                max: value,
            }],
            vec![Op::Int(value)],
            vec![0],
        ));
        let bytes = ok(ok(program.value()).canonical_bytes());
        assert!(finite_runtime::import_i64_program(&old_bytes(&program)).is_err());
        let ImportedScalarProgram::I128(decoded) =
            ok(finite_runtime::import_scalar_program(&bytes))
        else {
            panic!("wide profile was narrowed")
        };
        assert_eq!(ok(decoded.evaluate(&[])), vec![value]);
        assert_eq!(ok(ok(decoded.value()).canonical_bytes()), bytes);
    }
}

#[test]
fn legacy_output_refusal_and_minimum_subtraction_keep_actual_usage() {
    let program = ok(Program::try_new(
        vec![],
        vec![Domain::Int { min: 0, max: 1 }],
        vec![Op::Int(2)],
        vec![0],
    ));
    let old = ok(finite_runtime::import_i64_program(&old_bytes(&program)));
    let limits = v2_zero_limits().with_limit(Resource::Step, 3);
    let (result, usage) = old.execute_v2(&[], limits).into_parts();
    assert_eq!(result, Err(Failure::OutputDomain));
    assert_eq!(usage.used(Resource::Step), 1);
    let program = ok(Program::try_new(
        vec![],
        vec![full_i64()],
        vec![Op::Int(i128::from(i64::MIN)), Op::Int(1), Op::Sub(0, 1)],
        vec![1],
    ));
    let old = ok(finite_runtime::import_i64_program(&old_bytes(&program)));
    let (result, usage) = old.execute_v2(&[], limits).into_parts();
    assert_eq!(result, Err(Failure::Arithmetic));
    assert_eq!(usage.used(Resource::Step), 3);
}

#[test]
fn legacy_d1_all_1165_original_oracle_rows_keep_i64_meaning() {
    let mut count = 0;
    for line in include_str!("../../../tools/test-data/scalar-arithmetic/vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        let input: Vec<i64> = f[1..f.len() - 1]
            .iter()
            .map(|n| n.parse().unwrap())
            .collect();
        let mut nodes: Vec<_> = (0..input.len()).map(|i| Op::Input(i as u16)).collect();
        let op = match f[0] {
            "mul" => Op::Mul(0, 1),
            "div_floor" => Op::DivFloor(0, 1),
            "div_ceil" => Op::DivCeil(0, 1),
            "mul_div_floor" => Op::MulDivFloor(0, 1, 2),
            "mul_div_ceil" => Op::MulDivCeil(0, 1, 2),
            "products_ge" if input.len() == 4 => {
                nodes.push(Op::Int(1));
                Op::ProductsGe([0, 1, 4], [2, 3, 4])
            }
            "products_ge" => Op::ProductsGe([0, 1, 2], [3, 4, 5]),
            _ => panic!("original oracle"),
        };
        nodes.push(op);
        let root = nodes.len() - 1;
        let p = ok(Program::try_new(
            vec![full_i64(); input.len()],
            vec![if f[0] == "products_ge" {
                Domain::Bool
            } else {
                full_i64()
            }],
            nodes,
            vec![root as u16],
        ));
        let bytes = old_bytes(&p);
        let old = ok(finite_runtime::import_i64_program(&bytes));
        assert_eq!(old.profile(), I64_ARITHMETIC_PROFILE);
        assert_eq!(ok(ok(old.value()).canonical_bytes()), bytes);
        let expected = if f[f.len() - 1] == "arithmetic" {
            Err(Failure::Arithmetic)
        } else {
            Ok(vec![f[f.len() - 1].parse::<i64>().unwrap()])
        };
        for _ in 0..2 {
            let (r, u) = old
                .execute_v2(
                    &input,
                    v2_zero_limits().with_limit(Resource::Step, p.nodes().len() as u64),
                )
                .into_parts();
            assert_eq!(r, expected, "historical row {count}: {line}");
            assert_eq!(u.used(Resource::Step), p.nodes().len() as u64);
        }
        count += 1;
    }
    assert_eq!(count, 1165);
}

#[test]
fn legacy_d1_unused_mul_and_div_overflow_keep_first_failure_and_usage() {
    for op in [
        Op::Mul(0, 1),
        Op::DivFloor(0, 1),
        Op::DivCeil(0, 1),
        Op::MulDivFloor(0, 0, 1),
        Op::MulDivCeil(0, 0, 1),
    ] {
        let division = matches!(op, Op::DivFloor(..) | Op::DivCeil(..));
        let a = if division { i64::MIN } else { i64::MAX };
        let b = if division { -1 } else { 2 };
        let p = ok(Program::try_new(
            vec![],
            vec![full_i64()],
            vec![Op::Int(a as i128), Op::Int(b), op, Op::Int(0)],
            vec![3],
        ));
        let old = ok(finite_runtime::import_i64_program(&old_bytes(&p)));
        for _ in 0..2 {
            let (r, u) = old
                .execute_v2(&[], v2_zero_limits().with_limit(Resource::Step, 4))
                .into_parts();
            assert_eq!(r, Err(Failure::Arithmetic));
            assert_eq!(u.used(Resource::Step), 3);
        }
        let (r, u) = old
            .execute_v2(&[], v2_zero_limits().with_limit(Resource::Step, 2))
            .into_parts();
        assert!(matches!(r, Err(Failure::Budget(_))));
        assert_eq!(u.used(Resource::Step), 2);
        assert_eq!(p.evaluate(&[]), Ok(vec![0]));
    }
    let mut nodes = vec![Op::Int(1); 257];
    nodes.push(Op::Mul(0, 0));
    let p = ok(Program::try_new_extended(
        vec![],
        vec![full_i64()],
        nodes,
        vec![257],
    ));
    let bytes = old_bytes(&p);
    let old = ok(finite_runtime::import_i64_program(&bytes));
    assert_eq!(old.profile(), I64_ARITHMETIC_EXTENDED_PROFILE);
    assert_eq!(ok(ok(old.value()).canonical_bytes()), bytes);
    assert_eq!(old.evaluate(&[]), Ok(vec![1]));
}
