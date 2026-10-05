//! Independent canonical wire, decoder metric, explicit-limit and custody controls.
use zeno_fcis_codec::{
    AdmittedEnvelope, CanonicalEncode, DecodeError, DecodeLimits, EncodeError, Envelope, Hash32,
    canonical_value_bytes_with_limits, decode_envelope, decode_value, decode_value_with_metrics,
    encode_value_with_limits,
};
use zeno_fcis_value::{AdmittedValue, MapEntry, Value, ValueError, ValueLimits, ValueMetrics};

fn ok<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

#[test]
fn independent_wire_corpus_agrees_with_actual_decoder_metrics() {
    let corpus = vec![
        (vec![0], 1, 0, 0),
        (vec![1], 1, 0, 0),
        (vec![2], 1, 0, 0),
        ([vec![3], vec![255; 16]].concat(), 1, 0, 0),
        ([vec![4, 128], vec![0; 15]].concat(), 1, 0, 0),
        (vec![5, 0, 0, 0, 2, 9, 10], 1, 2, 0),
        (vec![6, 0, 0, 0, 2, 97, 98], 1, 2, 0),
        (vec![7, 0, 0, 0, 0, 0, 0], 1, 0, 0),
        (vec![7, 255, 255, 255, 255, 255, 255], 1, 0, 0),
        (vec![8, 0, 0, 0, 2, 0, 2], 3, 0, 1),
        (vec![9, 0, 0, 0, 2, 0, 0, 0, 255, 255, 1], 3, 0, 1),
        (vec![10, 0, 0, 0, 0, 0, 0, 0], 1, 0, 0),
        (vec![10, 255, 255, 255, 255, 255, 255, 1, 2], 2, 0, 1),
        (vec![11, 0, 0, 0, 1, 0], 2, 0, 1),
        (vec![12, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 2], 3, 2, 1),
        (
            vec![
                10, 0, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 1, 0, 0, 0, 7, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                21, 12, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 7, 5, 0, 0, 0, 2, 1, 2,
            ],
            6,
            38,
            3,
        ),
    ];
    for (bytes, nodes, payload_bytes, depth) in corpus {
        let expected = ValueMetrics {
            nodes,
            payload_bytes,
            depth,
        };
        let limits = DecodeLimits {
            max_input_bytes: ok(u64::try_from(bytes.len())),
            value: ValueLimits {
                max_nodes: nodes,
                max_payload_bytes: payload_bytes,
                max_depth: depth,
                max_collection_len: 2,
            },
        };
        let (value, observed) = ok(decode_value_with_metrics(&bytes, limits));
        assert_eq!(observed, expected);
        assert_eq!(ok(value.validate_limits(limits.value)), expected);
        assert_eq!(
            ok(canonical_value_bytes_with_limits(&value, limits.value)),
            bytes
        );
        assert_eq!(ok(value.encoded_length()), bytes.len());
        assert!(matches!(
            decode_value(
                &bytes,
                DecodeLimits {
                    max_input_bytes: limits.max_input_bytes - 1,
                    ..limits
                }
            ),
            Err(DecodeError::InputLimit { .. })
        ));
    }
}

#[test]
fn every_map_payload_boundary_matches_encoder_and_decoder_order() {
    let wire = vec![
        12, 0, 0, 0, 1, 0, 0, 0, 7, 5, 0, 0, 0, 2, 97, 98, 0, 0, 0, 7, 5, 0, 0, 0, 2, 99, 100,
    ];
    let limits = DecodeLimits {
        max_input_bytes: 27,
        value: ValueLimits {
            max_depth: 1,
            max_nodes: 3,
            max_payload_bytes: 18,
            max_collection_len: 1,
        },
    };
    let (map, metrics) = ok(decode_value_with_metrics(&wire, limits));
    assert_eq!(
        metrics,
        ValueMetrics {
            nodes: 3,
            payload_bytes: 18,
            depth: 1
        }
    );
    // At7 and above each individual blob fits; both paths then fail on the same
    // cumulative operation, including the final semantic leaf charge.
    for (limit, attempted) in [(7, 14), (13, 14), (14, 16), (15, 16), (16, 18), (17, 18)] {
        let low = DecodeLimits {
            value: ValueLimits {
                max_payload_bytes: limit,
                ..limits.value
            },
            ..limits
        };
        assert_eq!(
            decode_value(&wire, low),
            Err(DecodeError::PayloadLimit { limit, attempted })
        );
        let mut output = vec![99];
        assert_eq!(
            encode_value_with_limits(&map, &mut output, low.value),
            Err(EncodeError::InvalidValue(ValueError::PayloadLimit {
                limit,
                attempted
            }))
        );
        assert_eq!(output, vec![99]);
    }
    assert_eq!(
        decode_value(
            &wire,
            DecodeLimits {
                value: ValueLimits {
                    max_payload_bytes: 6,
                    ..limits.value
                },
                ..limits
            }
        ),
        Err(DecodeError::BlobLimit {
            limit: 6,
            attempted: 7
        })
    );
}

#[test]
fn all_structural_dimensions_are_exact_and_shared() {
    let wire = vec![11, 0, 0, 0, 1, 5, 0, 0, 0, 3, 1, 2, 3];
    let exact = DecodeLimits {
        max_input_bytes: 13,
        value: ValueLimits {
            max_depth: 1,
            max_nodes: 2,
            max_payload_bytes: 3,
            max_collection_len: 1,
        },
    };
    assert!(decode_value(&wire, exact).is_ok());
    assert_eq!(
        decode_value(
            &wire,
            DecodeLimits {
                value: ValueLimits {
                    max_depth: 0,
                    ..exact.value
                },
                ..exact
            }
        ),
        Err(DecodeError::DepthLimit {
            limit: 0,
            attempted: 1
        })
    );
    assert_eq!(
        decode_value(
            &wire,
            DecodeLimits {
                value: ValueLimits {
                    max_nodes: 1,
                    ..exact.value
                },
                ..exact
            }
        ),
        Err(DecodeError::NodeLimit {
            limit: 1,
            attempted: 2
        })
    );
    assert_eq!(
        decode_value(
            &wire,
            DecodeLimits {
                value: ValueLimits {
                    max_collection_len: 0,
                    ..exact.value
                },
                ..exact
            }
        ),
        Err(DecodeError::CollectionLimit {
            limit: 0,
            attempted: 1
        })
    );
    let mut deep = Value::unit();
    for _ in 0..65 {
        deep = Value::sum(0, 0, Some(deep));
    }
    let limits = DecodeLimits {
        value: ValueLimits {
            max_depth: 65,
            ..ValueLimits::default()
        },
        ..DecodeLimits::default()
    };
    let bytes = ok(canonical_value_bytes_with_limits(&deep, limits.value));
    let (decoded, metrics) = ok(decode_value_with_metrics(&bytes, limits));
    assert_eq!(decoded, deep);
    assert_eq!(metrics.depth, 65);
    assert_eq!(metrics.nodes, 66);
    assert!(matches!(
        decode_value(&bytes, DecodeLimits::default()),
        Err(DecodeError::DepthLimit {
            limit: 64,
            attempted: 65
        })
    ));
}

#[test]
fn envelope_admission_retains_actual_limits_and_append_is_transactional() {
    let raw = Envelope::new(0, Hash32::ZERO, Value::unit());
    let exact = DecodeLimits {
        max_input_bytes: 49,
        value: ValueLimits {
            max_nodes: 1,
            max_depth: 0,
            max_payload_bytes: 0,
            max_collection_len: 0,
        },
    };
    let expected = [b"ZFCISV1\0".as_slice(), &[0; 4], &[0; 32], &[0, 0, 0, 1, 0]].concat();
    assert_eq!(ok(raw.canonical_bytes_with_limits(exact)), expected);
    let admitted = ok(AdmittedEnvelope::try_from_envelope_with_limits(
        raw.clone(),
        exact,
    ));
    assert_eq!(admitted.limits(), exact);
    assert_eq!(admitted.encoded_length(), 49);
    assert_eq!(ok(admitted.canonical_bytes()), expected);
    assert_eq!(ok(decode_envelope(&expected, exact)), raw);
    let mut output = vec![1, 2, 3];
    assert_eq!(
        raw.encode_to_with_limits(
            &mut output,
            DecodeLimits {
                max_input_bytes: 48,
                ..exact
            }
        ),
        Err(EncodeError::EnvelopeInputLimit {
            limit: 48,
            attempted: 49
        })
    );
    assert_eq!(output, vec![1, 2, 3]);
    assert!(
        AdmittedEnvelope::try_new_with_limits(
            0,
            Hash32::ZERO,
            ok(AdmittedValue::try_new(Value::unit())),
            DecodeLimits {
                value: ValueLimits {
                    max_nodes: 0,
                    ..exact.value
                },
                ..exact
            }
        )
        .is_err()
    );
    let mut deep = Value::unit();
    for _ in 0..65 {
        deep = Value::sum(0, 0, Some(deep));
    }
    assert!(
        Envelope::new(0, Hash32::ZERO, deep)
            .encode_to(&mut output)
            .is_err()
    );
    assert_eq!(output, vec![1, 2, 3]);
}

#[test]
fn explicit_large_map_key_round_trips_without_default_fallback() {
    let length = ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES + 1;
    let limit = ok(u64::try_from(length)) * 2 + 6;
    let limits = DecodeLimits {
        max_input_bytes: ok(u64::try_from(length)) + 19,
        value: ValueLimits {
            max_payload_bytes: limit,
            ..ValueLimits::default()
        },
    };
    let key = ok(Value::bytes_with_limits(vec![0xab; length], limits.value));
    let entry = ok(MapEntry::try_new_with_limits(
        key,
        Value::unit(),
        limits.value,
    ));
    let map = ok(Value::map_canonical(vec![entry]));
    let wire = ok(canonical_value_bytes_with_limits(&map, limits.value));
    assert_eq!(ok(u64::try_from(wire.len())), limits.max_input_bytes);
    drop(map);
    let (decoded, metrics) = ok(decode_value_with_metrics(&wire, limits));
    assert_eq!(metrics.payload_bytes, limit);
    assert_eq!(metrics.nodes, 3);
    assert_eq!(metrics.depth, 1);
    assert_eq!(
        ok(canonical_value_bytes_with_limits(&decoded, limits.value)),
        wire
    );
    assert!(matches!(
        decode_value(&wire, DecodeLimits::default()),
        Err(DecodeError::InputLimit { .. })
    ));
}

#[test]
fn malformed_controls_keep_raw_decoder_precedence() {
    assert_eq!(
        decode_value(&[6, 0, 0, 0, 2, 0xc3, 0xa9], DecodeLimits::default()),
        Err(DecodeError::NonAsciiText)
    );
    assert_eq!(
        decode_value(&[10, 0, 0, 0, 0, 0, 0, 2], DecodeLimits::default()),
        Err(DecodeError::InvalidSumFlag)
    );
    assert_eq!(
        decode_value(&[0, 1], DecodeLimits::default()),
        Err(DecodeError::TrailingBytes { offset: 1 })
    );
    let duplicate = vec![9, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        decode_value(&duplicate, DecodeLimits::default()),
        Err(DecodeError::NonCanonicalRecord)
    );
}
