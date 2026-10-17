//! Source-connected scalar arithmetic, codec, refusal and meter controls.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::emit::{
    JavaScriptEmitter, PythonEmitter, RustEmitter, TargetEmitter,
};
use zeno_fcis_synthesis::finite::{
    ARITHMETIC_EXTENDED_PROFILE, ARITHMETIC_PROFILE, Domain, Error, Op, PROFILE, Program,
    V2ExecutionFailure, V2Resource as Resource, execute_v2, v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::import_program;
use zeno_fcis_value::{Value, ValueRef};

const I128: Domain = Domain::Int {
    min: i128::MIN,
    max: i128::MAX,
};

fn program(operation: &str, arity: usize) -> Program {
    let mut nodes: Vec<Op> = (0..arity).map(|i| Op::Input(i as u16)).collect();
    let output = if operation == "products_ge" {
        Domain::Bool
    } else {
        I128
    };
    let op = match operation {
        "mul" => Op::Mul(0, 1),
        "div_floor" => Op::DivFloor(0, 1),
        "div_ceil" => Op::DivCeil(0, 1),
        "mul_div_floor" => Op::MulDivFloor(0, 1, 2),
        "mul_div_ceil" => Op::MulDivCeil(0, 1, 2),
        "products_ge" if arity == 4 => {
            nodes.push(Op::Int(1));
            Op::ProductsGe([0, 1, 4], [2, 3, 4])
        }
        "products_ge" if arity == 6 => Op::ProductsGe([0, 1, 2], [3, 4, 5]),
        _ => panic!("test operation/arity"),
    };
    nodes.push(op);
    let root = nodes.len() - 1;
    Program::try_new(vec![I128; arity], vec![output], nodes, vec![root as u16]).unwrap()
}

#[test]
fn independent_fraction_vectors_match_native_and_metered_execution() {
    let mut count = 0;
    for line in include_str!("../../../tools/test-data/scalar-arithmetic/i128-vectors.tsv").lines()
    {
        let fields: Vec<_> = line.split('\t').collect();
        let operands: Vec<i128> = fields[1..fields.len() - 1]
            .iter()
            .map(|v| v.parse().unwrap())
            .collect();
        let expected = if fields[fields.len() - 1] == "arithmetic" {
            Err(Error::Arithmetic)
        } else {
            Ok(vec![fields[fields.len() - 1].parse::<i128>().unwrap()])
        };
        let p = program(fields[0], operands.len());
        assert_eq!(p.profile(), ARITHMETIC_PROFILE);
        assert_eq!(
            p.evaluate(&operands),
            expected,
            "oracle row {count}: {line}"
        );
        let (actual, usage) = p
            .execute_v2(
                &operands,
                v2_zero_limits().with_limit(Resource::Step, p.nodes().len() as u64),
            )
            .into_parts();
        let metered = match actual {
            Ok(v) => Ok(v),
            Err(V2ExecutionFailure::Arithmetic) => Err(Error::Arithmetic),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(metered, expected, "metered oracle row {count}: {line}");
        assert_eq!(usage.used(Resource::Step), p.nodes().len() as u64);
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
        let bytes = p.value().unwrap().canonical_bytes().unwrap();
        let decoded = import_program(&bytes);
        assert_eq!(
            decoded,
            Ok(p.clone()),
            "canonical opcode round-trip at oracle row {count}: {line}"
        );
        let decoded = decoded.unwrap();
        assert_eq!(decoded.evaluate(&operands), expected);
        count += 1;
    }
    assert_eq!(count, 1165);
}

#[test]
fn malformed_references_precede_arithmetic_and_charge_is_retained() {
    let nodes = [Op::Int(-1), Op::Int(0), Op::MulDivFloor(0, 1, 99)];
    for (limit, reference) in [(2, false), (3, true)] {
        let (result, usage) = execute_v2(
            &[],
            &[I128],
            &nodes,
            &[2],
            &[],
            v2_zero_limits().with_limit(Resource::Step, limit),
        )
        .into_parts();
        if reference {
            assert_eq!(result, Err(V2ExecutionFailure::Reference));
        } else {
            assert!(
                matches!(result,Err(V2ExecutionFailure::Budget(e)) if e.resource==Resource::Step && e.attempted==3)
            );
        }
        assert_eq!(usage.used(Resource::Step), limit);
    }
    assert_eq!(
        Program::try_new(
            vec![],
            vec![I128],
            vec![Op::Bool(false), Op::Mul(0, 99)],
            vec![1]
        ),
        Err(Error::Invalid("type-mismatch"))
    );
    assert_eq!(
        Program::try_new(
            vec![],
            vec![Domain::Bool],
            vec![Op::Int(1), Op::ProductsGe([0, 0, 0], [0, 0, 99])],
            vec![1]
        ),
        Err(Error::Invalid("node-reference"))
    );
}

#[test]
fn unused_and_unselected_arithmetic_traps_stay_eager() {
    let p = Program::try_new(
        vec![],
        vec![I128],
        vec![
            Op::Int(i128::MAX),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Int(0),
            Op::Mul(3, 2),
        ],
        vec![3],
    )
    .unwrap();
    assert_eq!(p.evaluate(&[]), Err(Error::Arithmetic));
    let (result, usage) = p
        .execute_v2(&[], v2_zero_limits().with_limit(Resource::Step, 5))
        .into_parts();
    assert_eq!(result, Err(V2ExecutionFailure::Arithmetic));
    assert_eq!(usage.used(Resource::Step), 3);
    let p = Program::try_new(
        vec![],
        vec![I128],
        vec![
            Op::Bool(false),
            Op::Int(i128::MIN),
            Op::Int(-1),
            Op::DivFloor(1, 2),
            Op::Int(0),
            Op::Select(0, 3, 4),
        ],
        vec![5],
    )
    .unwrap();
    assert_eq!(p.evaluate(&[]), Err(Error::Arithmetic));
    let p = Program::try_new(
        vec![],
        vec![I128],
        vec![Op::Int(0), Op::DivCeil(0, 0)],
        vec![0],
    )
    .unwrap();
    assert_eq!(p.evaluate(&[]), Err(Error::Arithmetic));
}

#[test]
fn narrow_multiply_and_fused_multiply_divide_are_distinct() {
    let narrow = Program::try_new(
        vec![],
        vec![I128],
        vec![Op::Int(i128::MAX), Op::Mul(0, 0), Op::DivFloor(1, 0)],
        vec![2],
    )
    .unwrap();
    let fused = Program::try_new(
        vec![],
        vec![I128],
        vec![Op::Int(i128::MAX), Op::MulDivFloor(0, 0, 0)],
        vec![1],
    )
    .unwrap();
    assert_eq!(narrow.evaluate(&[]), Err(Error::Arithmetic));
    assert_eq!(fused.evaluate(&[]), Ok(vec![i128::MAX]));
    assert_ne!(narrow.value().unwrap(), fused.value().unwrap());
}

#[test]
fn arithmetic_tags_require_their_profile_and_keep_old_program_bytes() {
    let old = Program::try_new(
        vec![],
        vec![I128],
        vec![Op::Int(2), Op::Int(3), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap();
    assert_eq!(old.profile(), PROFILE);
    let arithmetic = program("mul", 2);
    let value = arithmetic.value().unwrap();
    let ValueRef::Tuple(fields) = value.view() else {
        panic!("tuple")
    };
    let mut wrong = fields.to_vec();
    wrong[0] = Value::text_ascii(PROFILE.into()).unwrap();
    assert!(import_program(&Value::tuple(wrong).unwrap().canonical_bytes().unwrap()).is_err());
    let mut wrong = fields.to_vec();
    wrong[0] = Value::text_ascii("zeno-fcis/finite-i64/3".into()).unwrap();
    assert!(import_program(&Value::tuple(wrong).unwrap().canonical_bytes().unwrap()).is_err());
    // A decoder swapping tags 11 and 12 is a semantic difference at this row.
    let floor = program("div_floor", 2);
    let ceil = program("div_ceil", 2);
    assert_eq!(
        import_program(&floor.value().unwrap().canonical_bytes().unwrap())
            .unwrap()
            .evaluate(&[5, 2]),
        Ok(vec![2])
    );
    assert_eq!(
        import_program(&ceil.value().unwrap().canonical_bytes().unwrap())
            .unwrap()
            .evaluate(&[5, 2]),
        Ok(vec![3])
    );
    let mut nodes = vec![Op::Int(1); 4095];
    nodes.push(Op::Mul(0, 0));
    let large = Program::try_new_extended(vec![], vec![I128], nodes, vec![4095]).unwrap();
    assert_eq!(large.profile(), ARITHMETIC_EXTENDED_PROFILE);
    assert!(import_program(&large.value().unwrap().canonical_bytes().unwrap()).is_err());
    assert_eq!(old.evaluate(&[]), Ok(vec![5]));
    assert_eq!(
        old.value().unwrap().canonical_bytes().unwrap(),
        import_program(&old.value().unwrap().canonical_bytes().unwrap())
            .unwrap()
            .value()
            .unwrap()
            .canonical_bytes()
            .unwrap()
    );
}

#[test]
fn scalar_capacity_and_step_boundaries_are_complete() {
    for count in [256usize, 257, 4096, 4097] {
        let mut nodes = vec![Op::Int(1); count - 1];
        nodes.push(Op::Mul(0, 0));
        let admitted =
            Program::try_new_extended(vec![], vec![I128], nodes, vec![(count - 1) as u16]);
        if count == 4097 {
            assert_eq!(admitted, Err(Error::Invalid("program-shape")));
            continue;
        }
        let p = admitted.unwrap();
        for quota in [0, count as u64 - 1, count as u64, count as u64 + 1] {
            let (actual, usage) = p
                .execute_v2(&[], v2_zero_limits().with_limit(Resource::Step, quota))
                .into_parts();
            if quota < count as u64 {
                assert!(matches!(actual, Err(V2ExecutionFailure::Budget(_))));
            } else {
                assert_eq!(actual, Ok(vec![1]));
            }
            assert_eq!(usage.used(Resource::Step), quota.min(count as u64));
        }
    }
}

/// With an explicit output directory, the heavy conformance runner obtains
/// real source emitted by each registered adapter for every new instruction.
#[test]
fn emit_qualification_subjects() {
    for (operation, arity) in [
        ("mul", 2),
        ("div_floor", 2),
        ("div_ceil", 2),
        ("mul_div_floor", 3),
        ("mul_div_ceil", 3),
        ("products_ge", 4),
        ("products_ge", 6),
    ] {
        let p = program(operation, arity);
        for emitter in [
            &RustEmitter as &dyn TargetEmitter,
            &PythonEmitter,
            &JavaScriptEmitter,
        ] {
            let source = emitter.emit(&p).unwrap();
            assert!(source.contains(ARITHMETIC_PROFILE));
            if let Some(out) = std::env::var_os("ZENO_FCIS_ARITHMETIC_EMIT_DIR") {
                let out = std::path::PathBuf::from(out);
                std::fs::create_dir_all(&out).unwrap();
                std::fs::write(
                    out.join(format!(
                        "{operation}-{arity}.{}",
                        emitter.target().extension
                    )),
                    source,
                )
                .unwrap();
            }
        }
    }

    let eager = Program::try_new(
        vec![I128; 2],
        vec![I128],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::DivFloor(0, 1),
            Op::Int(0),
            Op::Bool(false),
            Op::Select(4, 2, 3),
        ],
        vec![5],
    )
    .unwrap();
    assert_eq!(eager.evaluate(&[1, 0]), Err(Error::Arithmetic));
    assert_eq!(eager.evaluate(&[1, 2]), Ok(vec![0]));
    for emitter in [
        &RustEmitter as &dyn TargetEmitter,
        &PythonEmitter,
        &JavaScriptEmitter,
    ] {
        let source = emitter.emit(&eager).unwrap();
        if let Some(out) = std::env::var_os("ZENO_FCIS_ARITHMETIC_EMIT_DIR") {
            let out = std::path::PathBuf::from(out);
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(
                out.join(format!("eager-choose.{}", emitter.target().extension)),
                source,
            )
            .unwrap();
        }
    }
}

#[test]
fn prepared_arithmetic_identity_compares_every_operand_and_keeps_owned_work() {
    use zeno_fcis_synthesis::finite::v2_continuation as fold;
    let context = fold::Context {
        state_root: [1; 32],
        state_version: 0,
        invocation_hash: [2; 32],
    };
    let prepared = |op: Op| {
        let mut nodes: Vec<Op> = (0..6).map(Op::Input).collect();
        nodes.push(op);
        let graph = fold::admit_graph(vec![I128; 6], vec![I128], nodes, vec![0]).unwrap();
        let budget = [
            Resource::Read,
            Resource::Write,
            Resource::Candidate,
            Resource::Effect,
            Resource::Byte,
            Resource::WitnessByte,
            Resource::Depth,
            Resource::Step,
        ]
        .into_iter()
        .fold(v2_zero_limits(), |b, r| b.with_limit(r, u64::MAX));
        fold::start(
            graph,
            vec![1],
            vec![vec![1, 2, 3, 4, 5]],
            context,
            fold::default_limits(),
            budget,
        )
        .unwrap()
    };
    for (op, changed) in [
        (Op::Mul(0, 1), Op::Mul(1, 0)),
        (Op::DivFloor(0, 1), Op::DivFloor(0, 2)),
        (Op::DivCeil(0, 1), Op::DivCeil(1, 0)),
        (Op::MulDivFloor(0, 1, 2), Op::MulDivFloor(0, 1, 3)),
        (Op::MulDivCeil(0, 1, 2), Op::MulDivCeil(1, 0, 2)),
        (
            Op::ProductsGe([0, 1, 2], [3, 4, 5]),
            Op::ProductsGe([0, 1, 2], [3, 4, 4]),
        ),
        (
            Op::ProductsGe([0, 1, 2], [3, 4, 5]),
            Op::ProductsGe([0, 0, 2], [3, 4, 5]),
        ),
    ] {
        let mut actual = prepared(op.clone());
        let original = prepared(op);
        assert!(actual.same_operation(&original));
        assert!(!actual.same_operation(&prepared(changed)));
        actual.advance(0, 1).unwrap();
        assert!(actual.same_operation(&original));
        assert_eq!(actual.finish(context), Ok(vec![1]));
        assert_eq!(actual.usage().used(Resource::Step), 7);
    }
}
