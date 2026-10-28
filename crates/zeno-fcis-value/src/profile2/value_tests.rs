use super::*;
use alloc::{string::String, vec, vec::Vec};

#[test]
fn wide_extrema_are_fixed_numeric_bytes_with_nominal_tags() {
    let mut unsigned_max = vec![0x0d];
    unsigned_max.extend([0xff; 32]);
    assert_eq!(
        Value::unsigned256(U256::MAX).zcve_bytes().unwrap(),
        unsigned_max
    );
    let mut signed_min = vec![0x0e, 0x80];
    signed_min.extend([0; 31]);
    assert_eq!(
        Value::signed256(I256::MIN).zcve_bytes().unwrap(),
        signed_min
    );
    let mut minus_one = vec![0x0e];
    minus_one.extend([0xff; 32]);
    assert_eq!(
        Value::signed256(I256::from_i128(-1)).zcve_bytes().unwrap(),
        minus_one
    );
    let value = Value::unsigned256(U256::from_words_le([1, 2]));
    let bytes = value.zcve_bytes().unwrap();
    assert_eq!(bytes.len(), 33);
    assert_eq!(bytes[16], 2);
    assert_eq!(bytes[32], 1);
    assert_ne!(Value::unsigned(0), Value::unsigned256(U256::ZERO));
    assert_ne!(Value::signed(0), Value::signed256(I256::ZERO));
}

#[test]
fn complete_legacy_tree_lifts_byte_identically_and_narrows_explicitly() {
    use crate::{Field as F, MapEntry as E, Value as L};
    let map = L::normalize_map(vec![
        E::try_new(L::signed(-1), L::unit()).unwrap(),
        E::try_new(L::unsigned(0), L::boolean(true)).unwrap(),
    ])
    .unwrap();
    let legacy = L::tuple(vec![
        L::unit(),
        L::boolean(false),
        L::boolean(true),
        L::unsigned(u128::MAX),
        L::signed(i128::MIN),
        L::bytes(vec![0, 255]).unwrap(),
        L::text_ascii(String::from("ASCII")).unwrap(),
        L::enumeration(0, 0),
        L::record_canonical(vec![F::new(0, L::unit()), F::new(u16::MAX, L::signed(-1))]).unwrap(),
        L::sum(u32::MAX, u16::MAX, None),
        L::sum(0, 0, Some(L::vector(vec![L::unit()]).unwrap())),
        map,
    ])
    .unwrap();
    let lifted = Value::lift_legacy(&legacy, ValueLimits::default()).unwrap();
    assert_eq!(lifted.zcve_bytes().unwrap(), legacy.zcve_bytes().unwrap());
    assert_eq!(
        lifted.validate_limits(ValueLimits::default()).unwrap(),
        legacy.validate_limits(ValueLimits::default()).unwrap()
    );
    assert_eq!(
        lifted.try_into_legacy(ValueLimits::default()).unwrap(),
        legacy
    );
}

#[test]
fn nested_wide_zero_cannot_silently_narrow() {
    let wide = Value::unsigned256(U256::ZERO);
    let values = vec![
        wide.clone(),
        Value::signed256(I256::ZERO),
        Value::tuple(vec![wide.clone()]).unwrap(),
        Value::vector(vec![wide.clone()]).unwrap(),
        Value::sum(1, 0, Some(wide.clone())),
        Value::record_canonical(vec![Field::new(1, wide.clone())]).unwrap(),
        Value::map_canonical(vec![
            MapEntry::try_new(wide.clone(), Value::unit()).unwrap(),
        ])
        .unwrap(),
        Value::map_canonical(vec![MapEntry::try_new(Value::unit(), wide).unwrap()]).unwrap(),
    ];
    for value in values {
        assert_eq!(
            value.try_into_legacy(ValueLimits::default()),
            Err(ConversionError::WideInteger)
        );
    }
}

#[test]
fn complete_limit_admission_precedes_any_narrowing_or_copy() {
    let wide = Value::unsigned256(U256::ZERO);
    let limits = ValueLimits {
        max_nodes: 0,
        ..ValueLimits::default()
    };
    assert_eq!(
        wide.try_into_legacy(limits),
        Err(ConversionError::InvalidValue(ValueError::NodeLimit {
            limit: 0,
            attempted: 1
        }))
    );
    let mixed = Value::tuple(vec![
        Value::signed256(I256::ZERO),
        Value::bytes(vec![9]).unwrap(),
    ])
    .unwrap();
    let limits = ValueLimits {
        max_payload_bytes: 0,
        ..ValueLimits::default()
    };
    assert_eq!(
        mixed.try_into_legacy(limits),
        Err(ConversionError::InvalidValue(ValueError::PayloadLimit {
            limit: 0,
            attempted: 1
        }))
    );
}

#[test]
fn encoded_key_order_is_not_signed_numeric_order() {
    let negative = I256::from_i128(-1);
    assert_eq!(negative.cmp(I256::ZERO), core::cmp::Ordering::Less);
    let a = MapEntry::try_new(Value::signed256(negative), Value::boolean(false)).unwrap();
    let b = MapEntry::try_new(Value::signed256(I256::ZERO), Value::boolean(true)).unwrap();
    assert!(a.encoded_key() > b.encoded_key());
    assert_eq!(
        Value::map_canonical(vec![a.clone(), b.clone()]),
        Err(ValueError::MapKeyOrder)
    );
    let map = Value::normalize_map(vec![a, b]).unwrap();
    let ValueRef::Map(entries) = map.view() else {
        panic!("map")
    };
    assert_eq!(entries[0].key(), &Value::signed256(I256::ZERO));
    for entry in entries {
        assert_eq!(entry.encoded_key(), entry.key().zcve_bytes().unwrap());
    }
}

#[test]
fn duplicate_fields_and_encoded_keys_refuse_without_combining() {
    assert!(matches!(
        Value::normalize_record(vec![
            Field::new(1, Value::unit()),
            Field::new(1, Value::boolean(true))
        ]),
        Err(ValueError::RecordFieldOrder { .. })
    ));
    let e = MapEntry::try_new(Value::unsigned256(U256::ZERO), Value::unit()).unwrap();
    assert_eq!(
        Value::normalize_map(vec![e.clone(), e]),
        Err(ValueError::MapKeyOrder)
    );
}

fn nested_map() -> Value {
    let inner = Value::map_canonical(vec![
        MapEntry::try_new(Value::unit(), Value::unit()).unwrap(),
    ])
    .unwrap();
    Value::map_canonical(vec![MapEntry::try_new(Value::unit(), inner).unwrap()]).unwrap()
}
#[test]
fn nested_map_blobs_share_payload_and_node_counters() {
    let value = nested_map();
    assert_eq!(value.encoded_length().unwrap(), 29);
    assert_eq!(
        value.validate_limits(ValueLimits::default()).unwrap(),
        ValueMetrics {
            nodes: 5,
            payload_bytes: 18,
            depth: 2
        }
    );
    let tight = ValueLimits {
        max_payload_bytes: 17,
        ..ValueLimits::default()
    };
    assert_eq!(
        value.validate_limits(tight),
        Err(ValueError::PayloadLimit {
            limit: 17,
            attempted: 18
        })
    );
    let mut prefix = vec![77, 88];
    let before = prefix.clone();
    assert!(
        value
            .encode_zcve_to_with_limits(&mut prefix, tight)
            .is_err()
    );
    assert_eq!(prefix, before);
    let node_limit = ValueLimits {
        max_nodes: 4,
        ..ValueLimits::default()
    };
    assert_eq!(
        value.validate_limits(node_limit),
        Err(ValueError::NodeLimit {
            limit: 4,
            attempted: 5
        })
    );
}

#[test]
fn admitted_custom_depth_is_not_silently_replaced_by_default() {
    let mut value = Value::unsigned256(U256::MAX);
    for _ in 0..65 {
        value = Value::sum(0, 0, Some(value));
    }
    assert!(matches!(
        value.zcve_bytes(),
        Err(ValueError::DepthLimit {
            limit: 64,
            attempted: 65
        })
    ));
    let limits = ValueLimits {
        max_depth: 65,
        ..ValueLimits::default()
    };
    let admitted = AdmittedValue::try_new_with_limits(value, limits).unwrap();
    assert_eq!(admitted.limits(), limits);
    assert_eq!(
        admitted.metrics(),
        ValueMetrics {
            nodes: 66,
            payload_bytes: 0,
            depth: 65
        }
    );
    let mut wire = Vec::new();
    admitted.encode_zcve_to(&mut wire).unwrap();
    assert_eq!(wire.len(), 65 * 8 + 33);
}

#[test]
fn hard_depth_cannot_be_disabled_and_refusal_preserves_prefix() {
    // The existing depth profile documents 8 MiB for unoptimized x86-64 samples.
    // Keep construction, validation, encoding, comparisons and ordinary drop here.
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(|| {
            let mut value = Value::unit();
            for _ in 0..1025 {
                value = Value::sum(0, 0, Some(value));
            }
            let limits = ValueLimits {
                max_depth: u32::MAX,
                ..ValueLimits::default()
            };
            assert_eq!(
                value.validate_limits(limits),
                Err(ValueError::DepthLimit {
                    limit: 1024,
                    attempted: 1025
                })
            );
            assert_eq!(
                value.encoded_length(),
                Err(ValueError::DepthLimit {
                    limit: 1024,
                    attempted: 1025
                })
            );
            let mut out = vec![1, 2, 3];
            assert!(value.encode_zcve_to_with_limits(&mut out, limits).is_err());
            assert_eq!(out, vec![1, 2, 3]);
        })
        .unwrap_or_else(|error| panic!("deep-profile thread creation: {error}"))
        .join()
        .unwrap_or_else(|error| std::panic::resume_unwind(error));
}

#[test]
fn owned_construction_and_views_cannot_retain_mutable_inputs() {
    let mut bytes = vec![1, 2];
    let value = Value::bytes(bytes.clone()).unwrap();
    bytes[0] = 9;
    assert_eq!(value.view(), ValueRef::Bytes(&[1, 2]));
    let mut fields = vec![Field::new(1, value.clone())];
    let record = Value::record_canonical(fields.clone()).unwrap();
    fields[0] = Field::new(2, Value::unit());
    let ValueRef::Record(fs) = record.view() else {
        panic!("record")
    };
    assert_eq!(fs[0].id(), 1);
    assert_eq!(fs[0].value(), &value);
}
