use super::*;
use alloc::{string::String, vec, vec::Vec};
use zeno_fcis_value::profile2::{
    ConversionError, I256, U256, parse_signed_decimal, parse_unsigned_decimal,
};

fn limits() -> DecodeLimits {
    DecodeLimits::default()
}
fn nested_map() -> Value {
    let inner = Value::map_canonical(vec![
        MapEntry::try_new(Value::unit(), Value::unit()).unwrap(),
    ])
    .unwrap();
    Value::map_canonical(vec![MapEntry::try_new(Value::unit(), inner).unwrap()]).unwrap()
}
fn values() -> Vec<Value> {
    vec![
        Value::unit(),
        Value::boolean(false),
        Value::boolean(true),
        Value::unsigned(u128::MAX),
        Value::signed(i128::MIN),
        Value::unsigned256(U256::ZERO),
        Value::unsigned256(U256::MAX),
        Value::signed256(I256::MIN),
        Value::signed256(I256::MAX),
        Value::bytes(vec![0, 255]).unwrap(),
        Value::text_ascii(String::from("ASCII")).unwrap(),
        Value::enumeration(0, 0),
        Value::tuple(vec![Value::unsigned256(U256::MAX), Value::boolean(true)]).unwrap(),
        Value::vector(vec![Value::signed256(I256::MIN)]).unwrap(),
        Value::record_canonical(vec![
            Field::new(0, Value::unsigned256(U256::MAX)),
            Field::new(u16::MAX, Value::unit()),
        ])
        .unwrap(),
        Value::sum(u32::MAX, u16::MAX, None),
        Value::sum(0, 0, Some(Value::signed256(I256::MIN))),
        Value::normalize_map(vec![
            MapEntry::try_new(Value::signed256(I256::from_i128(-1)), Value::unit()).unwrap(),
            MapEntry::try_new(Value::signed256(I256::ZERO), Value::unsigned256(U256::MAX)).unwrap(),
        ])
        .unwrap(),
        nested_map(),
    ]
}
fn hex(text: &str) -> Vec<u8> {
    assert!(text.is_ascii() && text.len() % 2 == 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn independent_atom_vectors_decode_full_width_and_preserve_nominal_kind() {
    for row in include_str!("atom-vectors.tsv").lines().skip(1) {
        let parts: Vec<_> = row.split('\t').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].len(), 66);
        let expected = match parts[0] {
            "u256" => Value::unsigned256(parse_unsigned_decimal(parts[1]).unwrap()),
            "i256" => Value::signed256(parse_signed_decimal(parts[1]).unwrap()),
            _ => panic!("vector kind"),
        };
        let wire = hex(parts[2]);
        assert_eq!(expected.canonical_bytes().unwrap(), wire);
        let (actual, metrics) = decode_value_with_metrics(&wire, limits()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            metrics,
            ValueMetrics {
                nodes: 1,
                payload_bytes: 0,
                depth: 0
            }
        );
        assert_eq!(
            actual.clone().try_into_legacy(limits().value),
            Err(ConversionError::WideInteger)
        );
        assert_eq!(
            crate::decode_value(&wire, limits()),
            Err(DecodeError::UnknownTag(wire[0]))
        );
    }
}

#[test]
fn complete_grammar_roundtrips_with_exact_structural_metrics() {
    for original in values() {
        let bytes = original.canonical_bytes().unwrap();
        assert_eq!(original.encoded_length().unwrap(), bytes.len());
        let (decoded, metrics) = decode_value_with_metrics(&bytes, limits()).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.canonical_bytes().unwrap(), bytes);
        assert_eq!(metrics, original.validate_limits(limits().value).unwrap());
    }
}

#[test]
fn legacy_bare_value_bytes_are_unchanged_but_new_envelope_is_explicit() {
    use zeno_fcis_value::{Field as F, MapEntry as E, Value as L};
    let legacy = L::tuple(vec![
        L::unit(),
        L::boolean(true),
        L::unsigned(u128::MAX),
        L::signed(i128::MIN),
        L::bytes(vec![0, 255]).unwrap(),
        L::text_ascii(String::from("A")).unwrap(),
        L::enumeration(1, 2),
        L::record_canonical(vec![F::new(1, L::unit())]).unwrap(),
        L::sum(0, 0, Some(L::vector(vec![L::signed(-1)]).unwrap())),
        L::map_canonical(vec![E::try_new(L::unit(), L::boolean(false)).unwrap()]).unwrap(),
    ])
    .unwrap();
    let old = legacy.canonical_bytes().unwrap();
    let lifted = Value::lift_legacy(&legacy, limits().value).unwrap();
    assert_eq!(lifted.canonical_bytes().unwrap(), old);
    assert_eq!(
        decode_value(&old, limits())
            .unwrap()
            .try_into_legacy(limits().value)
            .unwrap(),
        legacy
    );
    let wide = Value::unsigned256(U256::ZERO);
    let raw = wide.canonical_bytes().unwrap();
    assert_ne!(raw, Value::unsigned(0).canonical_bytes().unwrap());
    assert_eq!(decode_value(&raw, limits()).unwrap(), wide);
}

#[test]
fn every_truncation_and_trailing_suffix_refuses_for_values_and_envelopes() {
    for original in values() {
        let wire = original.canonical_bytes().unwrap();
        for n in 0..wire.len() {
            assert!(decode_value(&wire[..n], limits()).is_err(), "prefix {n}");
        }
        let mut suffix = wire.clone();
        suffix.push(0);
        assert_eq!(
            decode_value(&suffix, limits()),
            Err(DecodeError::TrailingBytes { offset: wire.len() })
        );
        let env = Envelope::new(9, Hash32::new([7; 32]), original);
        let frame = env.canonical_bytes().unwrap();
        for n in 0..frame.len() {
            assert!(
                decode_envelope(&frame[..n], limits()).is_err(),
                "frame prefix {n}"
            );
        }
        let mut suffix = frame.clone();
        suffix.push(255);
        assert_eq!(
            decode_envelope(&suffix, limits()),
            Err(DecodeError::TrailingBytes {
                offset: frame.len()
            })
        );
    }
}

#[test]
fn envelope_golden_magic_length_pins_and_wrong_profile_refusal() {
    let env = Envelope::new(42, Hash32::new([3; 32]), Value::unsigned256(U256::MAX));
    let mut expected = b"ZFCISV2\0".to_vec();
    expected.extend(42_u32.to_be_bytes());
    expected.extend([3; 32]);
    expected.extend(33_u32.to_be_bytes());
    expected.push(0x0d);
    expected.extend([255; 32]);
    let actual = env.canonical_bytes().unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 81);
    assert_eq!(decode_envelope(&actual, limits()).unwrap(), env);
    assert_eq!(
        crate::decode_envelope(&actual, limits()),
        Err(DecodeError::EnvelopeMagic)
    );
    let legacy = crate::Envelope::new(42, Hash32::new([3; 32]), zeno_fcis_value::Value::unit())
        .canonical_bytes()
        .unwrap();
    assert_eq!(
        decode_envelope(&legacy, limits()),
        Err(DecodeError::EnvelopeMagic)
    );
    // Decoding preserves untrusted pins. A schema/catalog consumer must compare
    // independently retained expected pins instead of trusting decode success.
    let mut changed = actual.clone();
    changed[11] ^= 1;
    changed[12] ^= 1;
    let decoded = decode_envelope(&changed, limits()).unwrap();
    assert_ne!(decoded.type_id(), 42);
    assert_ne!(decoded.schema_hash(), Hash32::new([3; 32]));
}

#[test]
fn nested_map_blob_depth_node_payload_counters_never_restart() {
    let wire = nested_map().canonical_bytes().unwrap();
    assert_eq!(wire.len(), 29);
    assert_eq!(
        decode_value_with_metrics(&wire, limits()).unwrap().1,
        ValueMetrics {
            nodes: 5,
            payload_bytes: 18,
            depth: 2
        }
    );
    let mut tight = limits();
    tight.value.max_payload_bytes = 17;
    assert_eq!(
        decode_value(&wire, tight),
        Err(DecodeError::PayloadLimit {
            limit: 17,
            attempted: 18
        })
    );
    tight = limits();
    tight.value.max_nodes = 4;
    assert_eq!(
        decode_value(&wire, tight),
        Err(DecodeError::NodeLimit {
            limit: 4,
            attempted: 5
        })
    );
    tight = limits();
    tight.value.max_depth = 0;
    assert_eq!(
        decode_value(&wire, tight),
        Err(DecodeError::DepthLimit {
            limit: 0,
            attempted: 1
        })
    );
    tight = limits();
    tight.value.max_collection_len = 0;
    assert_eq!(
        decode_value(&wire, tight),
        Err(DecodeError::CollectionLimit {
            limit: 0,
            attempted: 1
        })
    );
    tight = limits();
    tight.max_input_bytes = 28;
    assert_eq!(
        decode_value(&wire, tight),
        Err(DecodeError::InputLimit {
            limit: 28,
            actual: 29
        })
    );
}

#[test]
fn wide_atom_is_not_payload_but_its_framed_map_blob_is() {
    let atom = Value::unsigned256(U256::MAX);
    let zero_payload = DecodeLimits {
        value: ValueLimits {
            max_payload_bytes: 0,
            ..limits().value
        },
        ..limits()
    };
    assert_eq!(
        decode_value_with_metrics(&atom.canonical_bytes().unwrap(), zero_payload)
            .unwrap()
            .1,
        ValueMetrics {
            nodes: 1,
            payload_bytes: 0,
            depth: 0
        }
    );
    let map = Value::map_canonical(vec![MapEntry::try_new(atom, Value::unit()).unwrap()]).unwrap();
    let wire = map.canonical_bytes().unwrap();
    assert_eq!(
        decode_value_with_metrics(&wire, limits()).unwrap().1,
        ValueMetrics {
            nodes: 3,
            payload_bytes: 34,
            depth: 1
        }
    );
    let blob_too_small = DecodeLimits {
        value: ValueLimits {
            max_payload_bytes: 32,
            ..limits().value
        },
        ..limits()
    };
    assert_eq!(
        decode_value(&wire, blob_too_small),
        Err(DecodeError::BlobLimit {
            limit: 32,
            attempted: 33
        })
    );
    // Value validation has no input blob parser; its corresponding first
    // refusal is the aggregate payload charge, not a BlobLimit diagnostic.
    assert_eq!(
        map.validate_limits(blob_too_small.value),
        Err(zeno_fcis_value::ValueError::PayloadLimit {
            limit: 32,
            attempted: 33
        })
    );
    let aggregate_too_small = DecodeLimits {
        value: ValueLimits {
            max_payload_bytes: 33,
            ..limits().value
        },
        ..limits()
    };
    assert_eq!(
        decode_value(&wire, aggregate_too_small),
        Err(DecodeError::PayloadLimit {
            limit: 33,
            attempted: 34
        })
    );
}

#[test]
fn noncanonical_record_map_sum_text_and_nested_frame_negatives() {
    assert_eq!(
        decode_value(&hex("ff"), limits()),
        Err(DecodeError::UnknownTag(255))
    );
    assert_eq!(
        decode_value(&hex("0a00000000000002"), limits()),
        Err(DecodeError::InvalidSumFlag)
    );
    assert_eq!(
        decode_value(&hex("0600000001ff"), limits()),
        Err(DecodeError::NonAsciiText)
    );
    assert_eq!(
        decode_value(&hex("09000000020001000001ff"), limits()),
        Err(DecodeError::NonCanonicalRecord)
    );
    // The second key is out of order; refusal precedes its missing value blob.
    let wire = hex("0c00000002000000010200000001000000000101");
    assert_eq!(
        decode_value(&wire, limits()),
        Err(DecodeError::NonCanonicalMap)
    );
    let nested_trailing = hex("0c000000010000000200ff0000000100");
    assert_eq!(
        decode_value(&nested_trailing, limits()),
        Err(DecodeError::TrailingBytes { offset: 1 })
    );
    let declared = hex("08000f4241");
    assert_eq!(
        decode_value(&declared, limits()),
        Err(DecodeError::CollectionLimit {
            limit: 1_000_000,
            attempted: 1_000_001
        })
    );
}

#[test]
fn oversized_counts_do_not_reserve_from_untrusted_count_alone() {
    let wire = hex("08ffffffff");
    let mut custom = limits();
    custom.value.max_collection_len = u32::MAX;
    custom.value.max_nodes = 1;
    assert_eq!(
        decode_value(&wire, custom),
        Err(DecodeError::NodeLimit {
            limit: 1,
            attempted: 2
        })
    );
}

#[test]
fn all_encoding_refusals_preserve_existing_prefix() {
    let value = nested_map();
    let mut out = vec![0xde, 0xad];
    let before = out.clone();
    let tight = ValueLimits {
        max_payload_bytes: 17,
        ..limits().value
    };
    assert!(encode_value_with_limits(&value, &mut out, tight).is_err());
    assert_eq!(out, before);
    let env = Envelope::new(1, Hash32::ZERO, Value::unsigned256(U256::MAX));
    let tiny = DecodeLimits {
        max_input_bytes: 80,
        ..limits()
    };
    assert_eq!(
        env.encode_to_with_limits(&mut out, tiny),
        Err(EncodeError::EnvelopeInputLimit {
            limit: 80,
            attempted: 81
        })
    );
    assert_eq!(out, before);
    let bytes = env.canonical_bytes().unwrap();
    assert_eq!(
        decode_envelope(&bytes, tiny),
        Err(DecodeError::InputLimit {
            limit: 80,
            actual: 81
        })
    );
}

#[test]
fn admitted_envelope_retains_custom_limits_and_readmits_changed_witness() {
    let value = nested_map();
    let admitted = AdmittedValue::try_new(value.clone()).unwrap();
    let exact = DecodeLimits {
        max_input_bytes: 77,
        value: ValueLimits {
            max_payload_bytes: 18,
            max_nodes: 5,
            max_depth: 2,
            max_collection_len: 1,
        },
    };
    let env =
        AdmittedEnvelope::try_new_with_limits(1, Hash32::ZERO, admitted.clone(), exact).unwrap();
    assert_eq!(env.limits(), exact);
    assert_eq!(env.payload_length(), 29);
    assert_eq!(env.encoded_length(), 77);
    assert_eq!(
        env.value().metrics(),
        ValueMetrics {
            nodes: 5,
            payload_bytes: 18,
            depth: 2
        }
    );
    let wire = env.canonical_bytes().unwrap();
    assert_eq!(decode_envelope(&wire, exact).unwrap().into_value(), value);
    let tighter = DecodeLimits {
        value: ValueLimits {
            max_payload_bytes: 17,
            ..exact.value
        },
        ..exact
    };
    assert!(matches!(
        AdmittedEnvelope::try_new_with_limits(1, Hash32::ZERO, admitted, tighter),
        Err(EncodeError::InvalidValue(_))
    ));
}

#[test]
fn admitted_envelope_encodes_under_retained_depth_instead_of_default_depth() {
    let mut value = Value::unsigned256(U256::MAX);
    for _ in 0..65 {
        value = Value::sum(0, 0, Some(value));
    }
    let custom = DecodeLimits {
        value: ValueLimits {
            max_depth: 65,
            ..limits().value
        },
        ..limits()
    };
    let admitted = AdmittedValue::try_new_with_limits(value.clone(), custom.value).unwrap();
    let envelope =
        AdmittedEnvelope::try_new_with_limits(1, Hash32::ZERO, admitted, custom).unwrap();
    let wire = envelope.canonical_bytes().unwrap();
    assert_eq!(decode_envelope(&wire, custom).unwrap().into_value(), value);
    assert_eq!(
        decode_envelope(&wire, limits()),
        Err(DecodeError::DepthLimit {
            limit: 64,
            attempted: 65
        })
    );
    assert_eq!(
        envelope
            .into_envelope()
            .canonical_bytes_with_limits(custom)
            .unwrap(),
        wire
    );
}

#[test]
fn hard_depth1024_is_supported_but_cannot_be_disabled() {
    // The existing depth profile documents 8 MiB for unoptimized x86-64 samples.
    // Keep construction, validation, encoding, comparisons and ordinary drop here.
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(|| {
            let prefix = hex("0a00000000000001");
            let mut wire = Vec::new();
            for _ in 0..1024 {
                wire.extend_from_slice(&prefix);
            }
            wire.push(0);
            let mut custom = limits();
            custom.value.max_depth = u32::MAX;
            let (value, metrics) = decode_value_with_metrics(&wire, custom).unwrap();
            assert_eq!(metrics.depth, 1024);
            assert_eq!(metrics.nodes, 1025);
            assert_eq!(
                canonical_value_bytes_with_limits(&value, custom.value).unwrap(),
                wire
            );
            let mut deeper = prefix;
            deeper.extend_from_slice(&wire);
            assert_eq!(
                decode_value(&deeper, custom),
                Err(DecodeError::DepthLimit {
                    limit: 1024,
                    attempted: 1025
                })
            );
        })
        .unwrap_or_else(|error| panic!("deep-profile thread creation: {error}"))
        .join()
        .unwrap_or_else(|error| std::panic::resume_unwind(error));
}
