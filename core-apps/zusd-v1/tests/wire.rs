use zeno_fcis_collections::bounded::CollectionLimits;
use zeno_fcis_schema::{TypeId, TypeKind};
use zeno_fcis_value::{Field, MapEntry, Value, ValueRef};
use zenofcis_zusd_v1::{wire::*, *};
fn limits() -> Limits {
    Limits {
        steps: 100_000_000,
        reads: 100_000_000,
        bytes: 100_000_000,
    }
}
fn bounds() -> Bounds {
    Bounds {
        tables: [CollectionLimits {
            max_entries: 2,
            max_item_bytes: 4096,
            max_snapshot_bytes: 65536,
        }; 14],
        sequence_bytes: 4096,
        frame_bytes: 100_000,
        bounded_bytes: 64,
    }
}
fn record(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(i, v)| Field::new(i, v)).collect()).unwrap()
}
fn replace(v: &Value, id: u16, x: Value) -> Value {
    match v.view() {
        ValueRef::Record(fs) => Value::record_canonical(
            fs.iter()
                .map(|f| {
                    Field::new(
                        f.id(),
                        if f.id() == id {
                            x.clone()
                        } else {
                            f.value().clone()
                        },
                    )
                })
                .collect(),
        )
        .unwrap(),
        _ => panic!(),
    }
}
/// Structural minimum is a wire fixture, not an economic/deployment genesis.
fn minimal(s: &zeno_fcis_schema::Schema, id: TypeId) -> Value {
    match s.type_by_id(id).unwrap().kind() {
        TypeKind::Unit => Value::unit(),
        TypeKind::Bool => Value::boolean(false),
        TypeKind::U128 { min, .. } => Value::unsigned(*min),
        TypeKind::Bytes { min_len, .. } => Value::bytes(vec![0; *min_len as usize]).unwrap(),
        TypeKind::Enum { variants } => Value::enumeration(id.get(), variants[0].id().get()),
        TypeKind::Sum { variants } => Value::sum(
            id.get(),
            variants[0].id().get(),
            variants[0].payload().map(|t| minimal(s, t)),
        ),
        TypeKind::Tuple { items } => {
            Value::tuple(items.iter().map(|t| minimal(s, *t)).collect()).unwrap()
        }
        TypeKind::Record { fields } => record(
            fields
                .iter()
                .map(|f| (f.id().get(), minimal(s, f.type_id())))
                .collect(),
        ),
        TypeKind::Map { .. } => Value::map_canonical(vec![]).unwrap(),
        TypeKind::Vector { .. } => Value::vector(vec![]).unwrap(),
        _ => panic!(),
    }
}
fn state(p: &Policy) -> Value {
    let v = minimal(p.schema(), TypeId::new(STATE));
    let header = match v.view() {
        ValueRef::Record(fs) => fs[0].value(),
        _ => panic!(),
    };
    replace(
        &v,
        1,
        replace(
            header,
            1,
            Value::bytes(p.identity().as_bytes().to_vec()).unwrap(),
        ),
    )
}
fn nonce(v: &Value, n: u128) -> Value {
    replace(
        v,
        13,
        Value::map_canonical(vec![
            MapEntry::try_new(Value::bytes(vec![1; 32]).unwrap(), Value::unsigned(n)).unwrap(),
        ])
        .unwrap(),
    )
}
#[test]
fn complete_state_and_all_command_variants_roundtrip_and_bind_policy() {
    let p = Policy::prepare(bounds()).unwrap();
    let v = state(&p);
    let f = p.frame(STATE, &v, limits());
    assert_eq!(
        p.admit(STATE, f.result().unwrap(), limits()).result(),
        Ok(&v)
    );
    let high = nonce(&v, u64::MAX as u128);
    let f = p.frame(STATE, &high, limits());
    assert_eq!(
        p.admit(STATE, f.result().unwrap(), limits()).result(),
        Ok(&high)
    );
    let command_ty = p.schema().type_by_id(TypeId::new(COMMAND)).unwrap();
    let opid = match command_ty.kind() {
        TypeKind::Record { fields } => fields[4].type_id(),
        _ => panic!(),
    };
    let variants = match p.schema().type_by_id(opid).unwrap().kind() {
        TypeKind::Sum { variants } => variants,
        _ => panic!(),
    };
    assert_eq!(variants.len(), 20);
    for var in variants {
        let base = minimal(p.schema(), TypeId::new(COMMAND));
        let operation = Value::sum(
            opid.get(),
            var.id().get(),
            var.payload().map(|t| minimal(p.schema(), t)),
        );
        let cmd = replace(
            &replace(
                &base,
                4,
                Value::bytes(p.identity().as_bytes().to_vec()).unwrap(),
            ),
            5,
            operation,
        );
        let bytes = p.frame(COMMAND, &cmd, limits());
        assert_eq!(
            p.admit(COMMAND, bytes.result().unwrap(), limits()).result(),
            Ok(&cmd)
        );
    }
    let mut b = bounds();
    b.tables[0].max_item_bytes += 1;
    let q = Policy::prepare(b).unwrap();
    assert_ne!(p.identity(), q.identity());
    assert_eq!(
        q.admit(
            STATE,
            p.frame(STATE, &v, limits()).result().unwrap(),
            limits()
        )
        .result(),
        Err(Failure::Domain)
    );
    let mut bytes = p.frame(STATE, &v, limits()).result().unwrap().clone();
    bytes.push(0);
    assert_eq!(
        p.admit(STATE, &bytes, limits()).result(),
        Err(Failure::Canonical)
    );
    let malformed = replace(
        &v,
        13,
        Value::map_canonical(vec![
            MapEntry::try_new(Value::bytes(vec![1; 32]).unwrap(), Value::signed(1)).unwrap(),
        ])
        .unwrap(),
    );
    assert_eq!(
        p.frame(STATE, &malformed, limits()).result(),
        Err(Failure::Domain)
    );
}
#[test]
fn all_output_limit_reasons_exact_frames_and_metered_refusal() {
    for (item, count, snapshot, sequence, reason) in [
        (69, 2, 65536, 4096, 1060),
        (4096, 0, 65536, 4096, 1061),
        (4096, 2, 98, 4096, 1062),
        (4096, 2, 65536, 41, 1063),
    ] {
        let mut b = bounds();
        b.tables[11] = CollectionLimits {
            max_entries: count,
            max_item_bytes: item,
            max_snapshot_bytes: snapshot,
        };
        b.sequence_bytes = sequence;
        let p = Policy::prepare(b).unwrap();
        let before = state(&p);
        let after = if reason == 1063 {
            replace(
                &before,
                4,
                Value::vector(vec![Value::bytes(vec![1; 32]).unwrap()]).unwrap(),
            )
        } else {
            nonce(&before, 1)
        };
        let frozen = before.clone();
        let result = p.preflight(&before, &after, limits());
        assert_eq!(result.result(), Err(Failure::Reject(reason)), "{reason}");
        assert_eq!(before, frozen);
        let denied = Limits {
            steps: result.usage().steps() - 1,
            ..limits()
        };
        let a = p.preflight(&before, &after, denied);
        let b = p.preflight(&before, &after, denied);
        assert!(matches!(a.result(), Err(Failure::Budget { .. })));
        assert_eq!(a.usage(), b.usage());
    }
    let mut b = bounds();
    b.tables[11].max_entries = 1;
    let p = Policy::prepare(b).unwrap();
    let empty = state(&p);
    let old = nonce(&empty, 1);
    let next = nonce(&empty, 2);
    assert_eq!(p.preflight(&old, &next, limits()).result(), Ok(&next));
    assert_eq!(p.preflight(&next, &empty, limits()).result(), Ok(&empty));
    assert_eq!(
        p.preflight(&empty, &nonce(&empty, 1u128 << 127), limits())
            .result(),
        Err(Failure::Domain)
    );
}

#[test]
fn selected_parameter_record_checks_every_v1_constant_and_keeps_profiles_unbound() {
    let p = Policy::prepare(bounds()).unwrap();
    let mut v = state(&p);
    let type_id = p
        .schema()
        .types()
        .iter()
        .find(|t| t.name().as_str() == "Parameters")
        .unwrap()
        .id();
    let mut params = minimal(p.schema(), type_id);
    let constants = [
        500 * Q,
        50 * Q,
        1_100_000_000_000_000_000,
        1_500_000_000_000_000_000,
        200,
        5_000_000_000_000_000,
        5_000_000_000_000_000,
        Q / 20,
        2,
        999_037_758_833_783_000,
        525600000,
        20160,
        1_000_000_000,
        35_000_000 * Q,
    ];
    for (i, n) in constants.into_iter().enumerate() {
        params = replace(&params, i as u16 + 1, Value::unsigned(n));
    }
    let map = |params: Value| {
        Value::map_canonical(vec![MapEntry::try_new(Value::unsigned(0), params).unwrap()]).unwrap()
    };
    v = replace(&v, 15, map(params.clone()));
    let bound = parameters::bind(&p, &v, limits());
    assert_eq!(bound.result().unwrap().native().minimum_net, 500 * Q);
    assert_eq!(bound.result().unwrap().profiles(), &[None; 5]);
    for (i, n) in constants.into_iter().enumerate() {
        let wrong = replace(
            &v,
            15,
            map(replace(&params, i as u16 + 1, Value::unsigned(n + 1))),
        );
        assert!(
            matches!(
                parameters::bind(&p, &wrong, limits()).result(),
                Err(Failure::UnboundProfile | Failure::Domain)
            ),
            "field {}",
            i + 1
        );
    }
    assert_eq!(
        parameters::bind(
            &p,
            &replace(&v, 15, Value::map_canonical(vec![]).unwrap()),
            limits()
        )
        .result()
        .err(),
        Some(Failure::Domain)
    );
}
