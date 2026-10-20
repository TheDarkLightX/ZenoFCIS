use super::super::meter;
use super::*;
use alloc::{boxed::Box, vec};
#[test]
fn entry_laws_are_eager_and_runtime_sum_is_checked() {
    // Exercise defensive runtime arithmetic independently of static admission.
    let d = Descriptor {
        order: None,
        profile: b"test".to_vec(),
        types: vec![],
        roots: [100, 101, 102],
        frame_bytes: [1024; 3],
        genesis: Value::unit(),
        reasons: vec![303],
        cases: vec![],
        laws: vec![],
        limits: schema::all_limits(),
    };
    let table = Value::map_canonical(vec![
        MapEntry::try_new(Value::signed(0), Value::signed(i128::MAX))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        MapEntry::try_new(Value::signed(1), Value::signed(1))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    ])
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let expression = Expr::SumI128(
        Box::new(Expr::Literal(0, table)),
        Box::new(Expr::EntryValue),
    );
    let unit = Value::unit();
    let mut env = expression::Env {
        pre: &unit,
        command: None,
        context: None,
        post: None,
        payload: None,
        entry: None,
        error: None,
        candidate_reason: None,
    };
    let mut m = meter::new(schema::all_limits());
    assert!(matches!(
        env.eval(&d, &expression, &mut m),
        Err(Failure::Arithmetic)
    ));
    let bad = Expr::And(
        Box::new(Expr::Literal(0, Value::boolean(false))),
        Box::new(Expr::Eq(
            Box::new(Expr::Payload),
            Box::new(Expr::Literal(0, Value::unit())),
        )),
    );
    assert!(matches!(
        env.eval(&d, &bad, &mut m),
        Err(Failure::Undefined)
    ));
}
#[test]
fn union_frame_charges_complete_values_and_write_comparisons() {
    let v = Value::bytes(vec![1; 1000])
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let table = Value::map_canonical(vec![
        MapEntry::try_new(Value::signed(0), v)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    ])
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let mut m = meter::new(schema::all_limits());
    assert!(
        collection::frame(&table, &table, &[], &mut m)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
    );
    assert_eq!(m.used.used(Resource::Byte), 34 + 2 * 1005);
    assert_eq!(m.used.used(Resource::Read), 1);
    let mut limit = schema::all_limits();
    limit = limit.with_limit(Resource::Byte, 34 + 2 * 1005 - 1);
    let mut m = meter::new(limit);
    assert!(matches!(
        collection::frame(&table, &table, &[], &mut m),
        Err(Failure::Budget(_))
    ));
}
#[test]
fn lookup_full_scan_has_exact_reference_tariff() {
    let d = Descriptor {
        order: None,
        profile: b"lookup".to_vec(),
        types: vec![
            Type {
                id: 200,
                name: b"Key".to_vec(),
                shape: Shape::I128 { min: 0, max: 3 },
            },
            Type {
                id: 201,
                name: b"Val".to_vec(),
                shape: Shape::I128 { min: 0, max: 1 },
            },
            Type {
                id: 204,
                name: b"Map".to_vec(),
                shape: Shape::Table {
                    key: 200,
                    value: 201,
                    set: false,
                    limits: CollectionLimits {
                        max_entries: 3,
                        max_item_bytes: 50,
                        max_snapshot_bytes: 179,
                    },
                },
            },
        ],
        roots: [200; 3],
        frame_bytes: [1024; 3],
        genesis: Value::unit(),
        reasons: vec![303],
        cases: vec![],
        laws: vec![],
        limits: schema::all_limits(),
    };
    let empty = Value::map_canonical(vec![])
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let full = Value::map_canonical(vec![
        MapEntry::try_new(Value::signed(0), Value::signed(0))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        MapEntry::try_new(Value::signed(1), Value::signed(1))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    ])
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let key = Value::signed(2);
    let mut base = meter::new(schema::all_limits());
    assert_eq!(
        collection::lookup(&d, 204, &empty, &key, &mut base)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        None
    );
    let mut scan = meter::new(schema::all_limits());
    assert_eq!(
        collection::lookup(&d, 204, &full, &key, &mut scan)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        None
    );
    assert_eq!(
        scan.used.used(Resource::Read) - base.used.used(Resource::Read),
        2
    );
    assert_eq!(
        scan.used.used(Resource::Step) - base.used.used(Resource::Step),
        2
    );
    assert_eq!(
        scan.used.used(Resource::Byte) - base.used.used(Resource::Byte),
        68
    );
    for k in 0..2 {
        let mut m = meter::new(schema::all_limits());
        assert_eq!(
            collection::lookup(&d, 204, &full, &Value::signed(k), &mut m)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            Some(Value::signed(k))
        );
    }
}

fn unsigned_comparison(enabled: bool, holds: bool, witness: &str) {
    if enabled {
        assert!(holds, "U128_TABLE_MISMATCH {witness}");
    }
}
#[test]
fn u128_table_wide_sum_oracle() {
    let enabled = std::env::var_os("U128_TABLE_DISABLE_COMPARISON").is_none();
    std::println!("U128_TABLE_WITNESS wide sum including MAX+MAX");
    let d = Descriptor {
        order: None,
        profile: b"sum-oracle".to_vec(),
        types: vec![],
        roots: [100, 101, 102],
        frame_bytes: [4096; 3],
        genesis: Value::unit(),
        reasons: vec![303],
        cases: vec![],
        laws: vec![],
        limits: schema::all_limits(),
    };
    for row in include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/test-data/u128-tables/sums.tsv"
    ))
    .lines()
    {
        let columns: Vec<_> = row.split('\t').collect();
        let xs: Vec<u128> = if columns[0] == "-" {
            vec![]
        } else {
            columns[0]
                .split(',')
                .map(|n| {
                    n.parse()
                        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                })
                .collect()
        };
        let expected = [
            columns[1]
                .parse::<u128>()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            columns[2]
                .parse::<u128>()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ];
        let table = Value::map_canonical(
            xs.iter()
                .enumerate()
                .map(|(i, n)| {
                    MapEntry::try_new(Value::unsigned(i as u128), Value::unsigned(*n))
                        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let expression = Expr::SumU128(
            Box::new(Expr::Literal(0, table)),
            Box::new(Expr::EntryValue),
        );
        let unit = Value::unit();
        let mut env = expression::Env {
            pre: &unit,
            command: None,
            context: None,
            post: None,
            payload: None,
            entry: None,
            error: None,
            candidate_reason: None,
        };
        let mut m = meter::new(schema::all_limits());
        let result = env.eval(&d, &expression, &mut m);
        unsigned_comparison(
            enabled,
            matches!(result,Ok(Data::WideSum(sum)) if sum==expected),
            row,
        );
        let used = m.used;
        let mut retry = meter::new(schema::all_limits());
        let again = env.eval(&d, &expression, &mut retry);
        unsigned_comparison(
            enabled,
            matches!(again,Ok(Data::WideSum(sum)) if sum==expected),
            row,
        );
        assert_eq!(used, retry.used);
        assert!(env.entry.is_none());
        if used.used(Resource::Step) > 0 {
            let limit =
                schema::all_limits().with_limit(Resource::Step, used.used(Resource::Step) - 1);
            let mut denied = meter::new(limit);
            assert!(matches!(
                env.eval(&d, &expression, &mut denied),
                Err(Failure::Budget(_))
            ));
            assert!(env.entry.is_none());
        }
    }
}
#[test]
fn u128_table_nicr_wide_oracle() {
    let enabled = std::env::var_os("U128_TABLE_DISABLE_COMPARISON").is_none();
    std::println!("U128_TABLE_WITNESS unsigned NICR with full-width product");
    for row in include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/test-data/u128-tables/nicr.tsv"
    ))
    .lines()
    {
        let c: Vec<_> = row.split('\t').collect();
        let expected = c[2].parse::<u128>().ok();
        let actual = redemption::nicr(
            c[0].parse()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            c[1].parse()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &mut meter::new(schema::all_limits()),
        );
        unsigned_comparison(enabled, actual.ok() == expected, row);
    }
    // Three-factor quotient crosses the old signed boundary and checks its final U128 range.
    assert_eq!(
        redemption::mul3div(u128::MAX, 1, 1, 1, &mut meter::new(schema::all_limits()))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        u128::MAX
    );
    assert!(
        redemption::mul3div(u128::MAX, 2, 1, 1, &mut meter::new(schema::all_limits())).is_err()
    );
}

#[test]
fn u128_table_kind_checks_keep_left_before_right() {
    let d = Descriptor {
        order: None,
        profile: vec![],
        types: vec![],
        roots: [0; 3],
        frame_bytes: [1024; 3],
        genesis: Value::unit(),
        reasons: vec![],
        cases: vec![],
        laws: vec![],
        limits: schema::all_limits(),
    };
    let unit = Value::unit();
    let mut env = expression::Env {
        pre: &unit,
        command: None,
        context: None,
        post: None,
        payload: None,
        entry: None,
        error: None,
        candidate_reason: None,
    };
    for expression in [
        Expr::Lt(
            Box::new(Expr::Literal(0, Value::boolean(false))),
            Box::new(Expr::Payload),
        ),
        Expr::Add(
            Box::new(Expr::Literal(0, Value::boolean(false))),
            Box::new(Expr::Payload),
        ),
    ] {
        assert!(matches!(
            env.eval(&d, &expression, &mut meter::new(schema::all_limits())),
            Err(Failure::Kind)
        ));
    }
}
