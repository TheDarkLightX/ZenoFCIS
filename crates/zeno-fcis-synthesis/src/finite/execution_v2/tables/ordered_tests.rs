use super::super::meter;
use super::*;
use alloc::vec;
fn record(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(id, v)| Field::new(id, v)).collect())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
}
fn fixture(mode: u8) -> (Descriptor, Value) {
    let field = |id, ty| FieldType {
        id,
        name: vec![b'a' + id as u8],
        type_id: ty,
    };
    let types = vec![
        Type {
            id: 1,
            name: vec![],
            shape: Shape::I128 { min: -3, max: 3 },
        },
        Type {
            id: 2,
            name: vec![],
            shape: Shape::Unit,
        },
        Type {
            id: 3,
            name: vec![],
            shape: Shape::Record(vec![field(1, 1), field(2, 2), field(3, 2)]),
        },
        Type {
            id: 4,
            name: vec![],
            shape: Shape::Table {
                key: 1,
                value: 3,
                set: false,
                limits: CollectionLimits {
                    max_entries: 3,
                    max_item_bytes: 100,
                    max_snapshot_bytes: 1000,
                },
            },
        },
        Type {
            id: 5,
            name: vec![],
            shape: Shape::Vector {
                element: 1,
                max: 3,
                bytes: 1000,
            },
        },
        Type {
            id: 100,
            name: vec![],
            shape: Shape::Record(if mode == 0 {
                vec![field(1, 4)]
            } else {
                vec![field(1, 4), field(2, 5)]
            }),
        },
    ];
    let es = (0..3)
        .map(|id| {
            MapEntry::try_new(
                Value::signed(id),
                record(vec![
                    (1, Value::signed(if id == 1 { -1 } else { 1 })),
                    (2, Value::unit()),
                    (3, Value::unit()),
                ]),
            )
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        })
        .collect();
    let mut fs = vec![(
        1,
        Value::map_canonical(es)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )];
    if mode != 0 {
        fs.push((
            2,
            Value::vector(vec![Value::signed(0), Value::signed(2), Value::signed(1)])
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ));
    }
    let pre = record(fs);
    (
        Descriptor {
            order: Some(OrderSpec {
                mode,
                map: 1,
                sequence: if mode == 0 { None } else { Some(2) },
            }),
            profile: vec![],
            types,
            roots: [100; 3],
            frame_bytes: [1000; 3],
            genesis: pre.clone(),
            reasons: vec![303],
            cases: vec![],
            laws: vec![],
            limits: schema::all_limits(),
        },
        pre,
    )
}
#[test]
fn ordered_scan_and_iteration_reference_reads() {
    for mode in 0..2 {
        let (d, pre) = fixture(mode);
        let spec = d
            .order
            .as_ref()
            .unwrap_or_else(|| panic!("required value missing"));
        let mut m = meter::new(schema::all_limits());
        let pair = ordered::minimum(&d, spec, &pre, &mut m)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .unwrap_or_else(|| panic!("required value missing"));
        assert_eq!(pair.0, Value::signed(1));
        assert_eq!(m.used.used(Resource::Read), if mode == 0 { 3 } else { 4 });
        let mut m = meter::new(schema::all_limits());
        let _ = ordered::items(&d, spec, &pre, &mut m)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(m.used.used(Resource::Read), if mode == 0 { 9 } else { 12 });
    }
}
#[test]
fn ordered_redemption_fused_wide_integer_boundaries() {
    let mut denied = meter::new(schema::all_limits().with_limit(Resource::Byte, 33));
    assert!(matches!(
        ordered::number_cmp(
            ordered::Priority::Signed(1),
            ordered::Priority::Signed(0),
            &mut denied
        ),
        Err(Failure::Budget(_))
    ));
    assert_eq!(denied.used.used(Resource::Step), 1);
    assert_eq!(denied.used.used(Resource::Byte), 0);
    let mut exact = meter::new(schema::all_limits().with_limit(Resource::Byte, 34));
    assert_eq!(
        ordered::number_cmp(
            ordered::Priority::Signed(1),
            ordered::Priority::Signed(0),
            &mut exact
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        core::cmp::Ordering::Greater
    );
    assert_eq!(exact.used.used(Resource::Byte), 34);
    let max = u128::MAX;
    for (a, b, c, d, want) in [
        (7, 11, 13, 17, Some(58)),
        (max, max, 1, max, Some(max)),
        (
            max,
            100_000_000_000_000_000_000,
            10_000_000_000,
            max,
            Some(1_000_000_000_000_000_000_000_000_000_000),
        ),
        (max, max, 1, max - 1, None),
        (max, max, max, max, None),
        (1, 1, 1, 0, None),
    ] {
        let mut meter = meter::new(schema::all_limits());
        let result = redemption::mul3div(a, b, c, d, &mut meter);
        assert_eq!(result.ok(), want, "{a}*{b}*{c}/{d}");
    }
}

#[test]
fn ordered_frame_admission_is_metered_and_retry_stable() {
    let (mut d, pre) = fixture(1);
    let mut m = meter::new(schema::all_limits());
    let framed = collection::encode(&pre, &mut m)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        .len() as u64
        + 48;
    d.frame_bytes[0] = framed - 1;
    let mut one = meter::new(schema::all_limits());
    assert!(matches!(
        order_laws::admit_frame(&d, 0, &pre, &mut one),
        Err(Failure::Wire)
    ));
    assert!(one.used.used(Resource::Step) > 0);
    assert!(one.used.used(Resource::Read) > 0);
    assert_eq!(one.used.used(Resource::Byte), framed);
    let mut retry = meter::new(schema::all_limits());
    assert!(matches!(
        order_laws::admit_frame(&d, 0, &pre, &mut retry),
        Err(Failure::Wire)
    ));
    assert_eq!(one.used, retry.used);
    let mut denied = meter::new(schema::all_limits().with_limit(Resource::Byte, framed - 1));
    assert!(matches!(
        order_laws::admit_frame(&d, 0, &pre, &mut denied),
        Err(Failure::Budget(_))
    ));
    assert!(denied.used.used(Resource::Step) > 0);
    assert_eq!(denied.used.used(Resource::Byte), 0);
    d.frame_bytes[0] = framed;
    let mut exact = meter::new(schema::all_limits());
    assert!(order_laws::admit_frame(&d, 0, &pre, &mut exact).is_ok());
}
