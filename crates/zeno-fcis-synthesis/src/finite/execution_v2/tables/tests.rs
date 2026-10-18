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
        MapEntry::try_new(Value::signed(0), Value::signed(i128::MAX)).unwrap(),
        MapEntry::try_new(Value::signed(1), Value::signed(1)).unwrap(),
    ])
    .unwrap();
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
    let v = Value::bytes(vec![1; 1000]).unwrap();
    let table =
        Value::map_canonical(vec![MapEntry::try_new(Value::signed(0), v).unwrap()]).unwrap();
    let mut m = meter::new(schema::all_limits());
    assert!(collection::frame(&table, &table, &[], &mut m).unwrap());
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
    let empty = Value::map_canonical(vec![]).unwrap();
    let full = Value::map_canonical(vec![
        MapEntry::try_new(Value::signed(0), Value::signed(0)).unwrap(),
        MapEntry::try_new(Value::signed(1), Value::signed(1)).unwrap(),
    ])
    .unwrap();
    let key = Value::signed(2);
    let mut base = meter::new(schema::all_limits());
    assert_eq!(
        collection::lookup(&d, 204, &empty, &key, &mut base).unwrap(),
        None
    );
    let mut scan = meter::new(schema::all_limits());
    assert_eq!(
        collection::lookup(&d, 204, &full, &key, &mut scan).unwrap(),
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
            collection::lookup(&d, 204, &full, &Value::signed(k), &mut m).unwrap(),
            Some(Value::signed(k))
        );
    }
}
