//! Public construction custody, exact wire vectors, and supplied-limit boundaries.

use zeno_fcis_value::{
    AdmittedValue, Field, MapEntry, Value, ValueError, ValueLimits, ValueMetrics, ValueRef,
};

fn ok<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

#[test]
fn all_forms_keep_independent_fixed_wire_vectors() {
    let samples = vec![
        (Value::unit(), vec![0]),
        (Value::boolean(false), vec![1]),
        (Value::boolean(true), vec![2]),
        (
            Value::unsigned(u128::MAX),
            [vec![3], vec![255; 16]].concat(),
        ),
        (
            Value::signed(i128::MIN),
            [vec![4, 128], vec![0; 15]].concat(),
        ),
        (ok(Value::bytes(vec![254, 0])), vec![5, 0, 0, 0, 2, 254, 0]),
        (
            ok(Value::text_ascii("az".into())),
            vec![6, 0, 0, 0, 2, 97, 122],
        ),
        (Value::enumeration(0, 0), vec![7, 0, 0, 0, 0, 0, 0]),
        (
            Value::enumeration(u32::MAX, u16::MAX),
            vec![7, 255, 255, 255, 255, 255, 255],
        ),
        (
            ok(Value::tuple(vec![Value::unit(), Value::boolean(true)])),
            vec![8, 0, 0, 0, 2, 0, 2],
        ),
        (
            ok(Value::record_canonical(vec![
                Field::new(0, Value::unit()),
                Field::new(u16::MAX, Value::boolean(false)),
            ])),
            vec![9, 0, 0, 0, 2, 0, 0, 0, 255, 255, 1],
        ),
        (Value::sum(0, 0, None), vec![10, 0, 0, 0, 0, 0, 0, 0]),
        (
            Value::sum(u32::MAX, u16::MAX, Some(Value::boolean(true))),
            vec![10, 255, 255, 255, 255, 255, 255, 1, 2],
        ),
        (
            ok(Value::vector(vec![Value::unit()])),
            vec![11, 0, 0, 0, 1, 0],
        ),
        (
            ok(Value::map_canonical(vec![ok(MapEntry::try_new(
                Value::unit(),
                Value::boolean(true),
            ))])),
            vec![12, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 2],
        ),
    ];
    for (value, expected) in samples {
        assert_eq!(ok(value.zcve_bytes()), expected);
        assert_eq!(ok(value.encoded_length()), expected.len());
    }
}

#[test]
fn borrowed_views_preserve_transitive_ownership() {
    let mut caller_bytes = vec![1, 2, 3];
    let leaf = ok(Value::bytes(caller_bytes.clone()));
    let record = ok(Value::record_canonical(vec![Field::new(0, leaf)]));
    let admitted = ok(AdmittedValue::try_new(record));
    let before = ok(admitted.value().zcve_bytes());
    caller_bytes.fill(99);
    let ValueRef::Record(fields) = admitted.value().view() else {
        panic!("record expected")
    };
    let ValueRef::Bytes(bytes) = fields[0].value().view() else {
        panic!("bytes expected")
    };
    assert_eq!(bytes, &[1, 2, 3]);
    assert_eq!(ok(admitted.value().zcve_bytes()), before);
    assert_eq!(admitted.limits(), ValueLimits::default());
}

#[test]
fn ordering_duplicate_and_ascii_refusals_are_construction_boundaries() {
    assert!(matches!(
        Value::record_canonical(vec![
            Field::new(2, Value::unit()),
            Field::new(1, Value::unit())
        ]),
        Err(ValueError::RecordFieldOrder { .. })
    ));
    assert!(matches!(
        Value::normalize_record(vec![
            Field::new(2, Value::unit()),
            Field::new(2, Value::unit())
        ]),
        Err(ValueError::RecordFieldOrder { .. })
    ));
    let a = ok(MapEntry::try_new(Value::unit(), Value::boolean(false)));
    let b = ok(MapEntry::try_new(Value::boolean(true), Value::unit()));
    assert_eq!(
        Value::map_canonical(vec![b.clone(), a.clone()]),
        Err(ValueError::MapKeyOrder)
    );
    assert_eq!(
        Value::normalize_map(vec![a.clone(), a]),
        Err(ValueError::MapKeyOrder)
    );
    assert!(Value::normalize_map(vec![b]).is_ok());
    let limits = ValueLimits {
        max_nodes: 0,
        max_payload_bytes: 0,
        ..ValueLimits::default()
    };
    assert_eq!(
        Value::text_ascii_with_limits("é".into(), limits),
        Err(ValueError::NonAsciiText)
    );
    assert_eq!(
        Value::bytes_with_limits(vec![1], limits),
        Err(ValueError::NodeLimit {
            limit: 0,
            attempted: 1
        })
    );
}

#[test]
fn map_metrics_charge_both_raw_blobs_and_semantic_leaves() {
    let entry = ok(MapEntry::try_new(
        ok(Value::bytes(b"ab".to_vec())),
        ok(Value::bytes(b"cd".to_vec())),
    ));
    let map = ok(Value::map_canonical(vec![entry]));
    let limits = ValueLimits {
        max_payload_bytes: 18,
        max_depth: 1,
        max_nodes: 3,
        max_collection_len: 1,
    };
    assert_eq!(
        ok(map.validate_limits(limits)),
        ValueMetrics {
            nodes: 3,
            payload_bytes: 18,
            depth: 1
        }
    );
    assert_eq!(ok(map.zcve_bytes_with_limits(limits)).len(), 27);
    for (ceiling, attempted) in [
        (6, 7),
        (7, 14),
        (13, 14),
        (14, 16),
        (15, 16),
        (16, 18),
        (17, 18),
    ] {
        let low = ValueLimits {
            max_payload_bytes: ceiling,
            ..limits
        };
        let mut output = vec![44, 55];
        assert_eq!(
            map.encode_zcve_to_with_limits(&mut output, low),
            Err(ValueError::PayloadLimit {
                limit: ceiling,
                attempted
            })
        );
        assert_eq!(output, vec![44, 55]);
    }
}

#[test]
fn supplied_limits_cover_each_dimension_without_default_substitution() {
    let value = ok(Value::vector(vec![ok(Value::bytes(vec![1, 2, 3]))]));
    let exact = ValueLimits {
        max_depth: 1,
        max_nodes: 2,
        max_payload_bytes: 3,
        max_collection_len: 1,
    };
    assert_eq!(
        ok(value.validate_limits(exact)),
        ValueMetrics {
            nodes: 2,
            payload_bytes: 3,
            depth: 1
        }
    );
    let limits_and_errors = [
        (
            ValueLimits {
                max_depth: 0,
                ..exact
            },
            ValueError::DepthLimit {
                limit: 0,
                attempted: 1,
            },
        ),
        (
            ValueLimits {
                max_nodes: 1,
                ..exact
            },
            ValueError::NodeLimit {
                limit: 1,
                attempted: 2,
            },
        ),
        (
            ValueLimits {
                max_payload_bytes: 2,
                ..exact
            },
            ValueError::PayloadLimit {
                limit: 2,
                attempted: 3,
            },
        ),
        (
            ValueLimits {
                max_collection_len: 0,
                ..exact
            },
            ValueError::CollectionLimit {
                limit: 0,
                attempted: 1,
            },
        ),
    ];
    for (limits, expected) in limits_and_errors {
        let mut output = vec![77];
        assert_eq!(
            value.encode_zcve_to_with_limits(&mut output, limits),
            Err(expected)
        );
        assert_eq!(output, vec![77]);
    }
    let mut deep = Value::unit();
    for _ in 0..65 {
        deep = Value::sum(0, 0, Some(deep));
    }
    let permissive = ValueLimits {
        max_depth: 65,
        ..ValueLimits::default()
    };
    let admitted = ok(AdmittedValue::try_new_with_limits(deep, permissive));
    assert_eq!(admitted.metrics().depth, 65);
    assert_eq!(admitted.limits(), permissive);
    assert!(admitted.value().zcve_bytes().is_err());
    let mut bytes = vec![77];
    ok(admitted.encode_zcve_to(&mut bytes));
    assert_eq!(bytes.len(), 522);
    assert_eq!(bytes[0], 77);
}

#[test]
fn legal_leaf_above_default_payload_remains_constructible_and_encodable() {
    let length = ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES + 1;
    let limits = ValueLimits {
        max_payload_bytes: u64::try_from(length).unwrap_or(u64::MAX),
        ..ValueLimits::default()
    };
    let value = ok(Value::bytes_with_limits(vec![0xa5; length], limits));
    assert!(value.zcve_bytes().is_err());
    let encoded = ok(value.zcve_bytes_with_limits(limits));
    assert_eq!(encoded.len(), length + 5);
    assert_eq!(&encoded[..5], &[5, 4, 0, 0, 1]);
    assert!(encoded[5..].iter().all(|byte| *byte == 0xa5));
    drop(encoded);
    drop(value);
    let text = ok(Value::text_ascii_with_limits("x".repeat(length), limits));
    let admitted = ok(AdmittedValue::try_new_with_limits(text, limits));
    assert_eq!(admitted.metrics().payload_bytes, limits.max_payload_bytes);
    let mut output = Vec::new();
    ok(admitted.encode_zcve_to(&mut output));
    assert_eq!(&output[..5], &[6, 4, 0, 0, 1]);
    assert!(output[5..].iter().all(|byte| *byte == b'x'));
}

#[test]
fn nested_map_and_payload_sum_have_hand_derived_metrics() {
    let inner = ok(Value::map_canonical(vec![ok(MapEntry::try_new(
        Value::unit(),
        ok(Value::bytes(vec![1, 2])),
    ))]));
    let outer = ok(Value::map_canonical(vec![ok(MapEntry::try_new(
        Value::enumeration(0, 0),
        inner,
    ))]));
    let value = Value::sum(0, 0, Some(outer));
    let limits = ValueLimits {
        max_nodes: 6,
        max_depth: 3,
        max_payload_bytes: 38,
        max_collection_len: 1,
    };
    assert_eq!(
        ok(value.validate_limits(limits)),
        ValueMetrics {
            nodes: 6,
            depth: 3,
            payload_bytes: 38
        }
    );
    let expected = vec![
        10, 0, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 1, 0, 0, 0, 7, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 12,
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 7, 5, 0, 0, 0, 2, 1, 2,
    ];
    assert_eq!(ok(value.zcve_bytes_with_limits(limits)), expected);
    assert_eq!(ok(value.encoded_length()), 49);
    let low = ValueLimits {
        max_payload_bytes: 37,
        ..limits
    };
    assert_eq!(
        value.validate_limits(low),
        Err(ValueError::PayloadLimit {
            limit: 37,
            attempted: 38
        })
    );
}

#[test]
fn empty_shapes_are_distinct_and_allowed_by_zero_collection_limit() {
    let samples = [
        (ok(Value::bytes(Vec::new())), 5),
        (ok(Value::text_ascii(String::new())), 6),
        (ok(Value::tuple(Vec::new())), 8),
        (ok(Value::record_canonical(Vec::new())), 9),
        (ok(Value::vector(Vec::new())), 11),
        (ok(Value::map_canonical(Vec::new())), 12),
    ];
    let limits = ValueLimits {
        max_depth: 0,
        max_nodes: 1,
        max_payload_bytes: 0,
        max_collection_len: 0,
    };
    for (value, tag) in samples {
        assert_eq!(
            ok(value.zcve_bytes_with_limits(limits)),
            vec![tag, 0, 0, 0, 0]
        );
        assert_eq!(
            ok(value.validate_limits(limits)),
            ValueMetrics {
                nodes: 1,
                payload_bytes: 0,
                depth: 0
            }
        );
    }
}
