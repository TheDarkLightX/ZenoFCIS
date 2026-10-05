use super::*;
use crate::finite::canonical_v2::{envelope::frame, read_big_endian, read_signed_128};
use crate::finite::execution_v2::{
    self as execution, InputField, InputLeaf, InputVariant, Resource,
};
use alloc::{string::String, vec};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_value::{Field as ValueField, Value};

// This oracle delegates to the actual independent legacy value encoder. It
// never calls the new size, tag, byte, order or refusal helpers.
fn value(atom: Atom<'_>) -> Value {
    match atom {
        Atom::Bool(v) => Value::boolean(v),
        Atom::I128(v) => Value::signed(v),
        Atom::U128(v) => Value::unsigned(v),
        Atom::Enum { type_id, variant } => Value::enumeration(type_id, variant),
        Atom::Sum { type_id, variant } => Value::sum(type_id, variant, None),
        Atom::Bytes(b) => Value::bytes(alloc::vec::Vec::from(b))
            .unwrap_or_else(|error| panic!("value fixture: {error}")),
        Atom::Text(b) => Value::text_ascii(
            String::from_utf8(b.to_vec()).unwrap_or_else(|error| panic!("ASCII: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("text: {error:?}")),
    }
}
fn record(fields: &[Field<'_>]) -> Value {
    Value::record_canonical(
        fields
            .iter()
            .map(|f| ValueField::new(f.id, value(f.value)))
            .collect(),
    )
    .unwrap_or_else(|error| panic!("record: {error:?}"))
}
fn atom_caps(atom: Atom<'_>) {
    let expected = value(atom)
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("legacy bytes: {error:?}"));
    for cap in 0..expected.len() {
        assert_eq!(encode_atom(atom, cap), Err(Failure::Limit));
    }
    for cap in [expected.len(), expected.len() + 1, usize::MAX] {
        assert_eq!(encode_atom(atom, cap), Ok(expected.clone()));
    }
}
#[test]
fn actual_value_oracle_all_atom_kinds_and_every_cap() {
    let bytes: Vec<u8> = (0..=255).collect();
    let text: Vec<u8> = (0..=127).collect();
    for atom in [
        Atom::Bool(false),
        Atom::Bool(true),
        Atom::Bytes(&[]),
        Atom::Bytes(&bytes),
        Atom::Text(&[]),
        Atom::Text(&text),
        Atom::Text(b"a\0z"),
    ] {
        atom_caps(atom);
    }
    for n in [
        i128::MIN,
        i128::MIN + 1,
        -257,
        -256,
        -1,
        0,
        1,
        255,
        256,
        i128::MAX,
    ] {
        atom_caps(Atom::I128(n));
    }
    for n in [0, 1, 255, 256, 1u128 << 127, u128::MAX - 1, u128::MAX] {
        atom_caps(Atom::U128(n));
    }
    for type_id in [0, 1, 255, 256, 65535, 65536, 0xfedc_0123, u32::MAX] {
        for variant in [u16::MAX, 0, 1, 256, 255, 0xabc1] {
            atom_caps(Atom::Enum { type_id, variant });
            atom_caps(Atom::Sum { type_id, variant });
        }
    }
}
#[test]
fn standard_integer_bytes_and_actual_readers_round_trip_all_bit_positions() {
    let mut values = vec![0, u128::MAX, i128::MIN as u128, i128::MAX as u128];
    for bit in 0..128 {
        values.push(1u128 << bit);
        values.push(!(1u128 << bit));
    }
    for unsigned in values {
        let signed = unsigned as i128;
        let bytes =
            encode_atom(Atom::U128(unsigned), 17).unwrap_or_else(|error| panic!("u128: {error:?}"));
        assert_eq!(bytes[0], 3);
        assert_eq!(&bytes[1..], unsigned.to_be_bytes());
        assert_eq!(read_big_endian(&bytes, 1, 16), Some((unsigned, 17)));
        let bytes =
            encode_atom(Atom::I128(signed), 17).unwrap_or_else(|error| panic!("i128: {error:?}"));
        assert_eq!(bytes[0], 4);
        assert_eq!(&bytes[1..], signed.to_be_bytes());
        assert_eq!(read_signed_128(&bytes, 1), Some((signed, 17)));
    }
}
#[test]
fn actual_record_oracle_preserves_zero_gaps_complete_ids_empty_and_large() {
    let data = [0, 255, 128, 1];
    for count in [0usize, 1, 32, 100] {
        let fields: Vec<_> = (0..count)
            .map(|i| Field {
                id: (i * 661) as u16,
                value: match i % 8 {
                    0 => Atom::Bool(i % 16 == 0),
                    1 => Atom::I128(i128::MIN + i as i128),
                    2 => Atom::U128(u128::MAX - i as u128),
                    3 => Atom::Enum {
                        type_id: u32::MAX,
                        variant: i as u16,
                    },
                    4 => Atom::Sum {
                        type_id: 0,
                        variant: u16::MAX - i as u16,
                    },
                    5 => Atom::Bytes(&data),
                    6 => Atom::Text(b"a\0z"),
                    _ => Atom::Bytes(&[]),
                },
            })
            .collect();
        let before = fields.clone();
        let expected = record(&fields)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("legacy record: {error:?}"));
        for cap in 0..expected.len() {
            assert_eq!(encode_record(&fields, cap), Err(Failure::Limit));
        }
        for cap in [expected.len(), expected.len() + 1, usize::MAX] {
            assert_eq!(encode_record(&fields, cap), Ok(expected.clone()));
        }
        assert_eq!(fields, before);
    }
    let fields = [
        Field {
            id: 0,
            value: Atom::Bool(false),
        },
        Field {
            id: u16::MAX,
            value: Atom::Bool(true),
        },
    ];
    assert_eq!(
        encode_record(&fields, 11),
        Ok(vec![9, 0, 0, 0, 2, 0, 0, 1, 255, 255, 2])
    );
}
#[test]
fn actual_envelope_oracle_original_header_every_cap_and_frame_round_trip() {
    let fields = [
        Field {
            id: 0,
            value: Atom::I128(i128::MIN),
        },
        Field {
            id: u16::MAX,
            value: Atom::Sum {
                type_id: u32::MAX,
                variant: 0,
            },
        },
    ];
    for original in [
        value(Atom::Bool(false)),
        value(Atom::U128(u128::MAX)),
        record(&[]),
        record(&fields),
    ] {
        let payload = original
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("legacy: {error:?}"));
        for root in [0, 1, 256, 0x1234_5678, u32::MAX] {
            let schema = core::array::from_fn(|i| (i * 7 + 1) as u8);
            let expected = Envelope::new(root, Hash32::new(schema), original.clone())
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("legacy envelope: {error:?}"));
            for cap in 0..expected.len() {
                assert_eq!(
                    encode_envelope(root, &schema, &payload, cap),
                    Err(Failure::Limit)
                );
            }
            for cap in [expected.len(), expected.len() + 1, usize::MAX] {
                let encoded = encode_envelope(root, &schema, &payload, cap)
                    .unwrap_or_else(|error| panic!("frame: {error:?}"));
                assert_eq!(encoded, expected);
                let parsed = frame(&encoded, root, &schema, cap as u64)
                    .unwrap_or_else(|error| panic!("actual frame parser: {error:?}"));
                assert_eq!(parsed.original(), encoded);
                assert_eq!(parsed.bytes(), payload);
            }
        }
    }
    let empty = encode_envelope(0, &[0; 32], &[], 48)
        .unwrap_or_else(|error| panic!("empty frame: {error:?}"));
    assert_eq!(&empty[..8], b"ZFCISV1\0");
    assert_eq!(&empty[8..], &[0; 40]);
    assert_eq!(
        frame(&empty, 0, &[0; 32], 48)
            .unwrap_or_else(|error| panic!("frame: {error:?}"))
            .bytes(),
        &[]
    );
}
#[test]
fn malformed_text_fields_and_refusal_precedence_are_explicit() {
    for b in 128..=255 {
        for cap in [0, 1, 5, 6, usize::MAX] {
            assert_eq!(encode_atom(Atom::Text(&[b]), cap), Err(Failure::Text));
        }
    }
    let malformed = [
        Field {
            id: 9,
            value: Atom::Bool(true),
        },
        Field {
            id: 9,
            value: Atom::Text(&[255]),
        },
    ];
    assert_eq!(encode_record(&malformed, 4), Err(Failure::Limit));
    assert_eq!(encode_record(&malformed, 7), Err(Failure::Limit));
    assert_eq!(encode_record(&malformed, 8), Err(Failure::Order));
    assert_eq!(encode_record(&malformed, usize::MAX), Err(Failure::Order));
    let reversed = [
        Field {
            id: u16::MAX,
            value: Atom::Bool(false),
        },
        Field {
            id: 0,
            value: Atom::Bool(true),
        },
    ];
    assert_eq!(encode_record(&reversed, usize::MAX), Err(Failure::Order));
    let text = [Field {
        id: 0,
        value: Atom::Text(&[255]),
    }];
    assert_eq!(encode_record(&text, 6), Err(Failure::Limit));
    assert_eq!(encode_record(&text, 7), Err(Failure::Text));
    assert_eq!(encode_record(&text, usize::MAX), Err(Failure::Text));
}
#[test]
fn actual_flat_input_projection_preserves_original_ids_and_permuted_codes() {
    let mut limits = execution::zero_limits();
    for r in [Resource::Read, Resource::Byte] {
        limits = limits.with_limit(r, u64::MAX);
    }
    for count in [0usize, 1, 32, 100] {
        let fields: Vec<_> = (0..count)
            .map(|i| Field {
                id: (i * 661) as u16,
                value: match i % 4 {
                    0 => Atom::Bool(true),
                    1 => Atom::I128(-7),
                    2 => Atom::Enum {
                        type_id: u32::MAX,
                        variant: 0,
                    },
                    _ => Atom::Sum {
                        type_id: 0,
                        variant: 500,
                    },
                },
            })
            .collect();
        let schema: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| InputField {
                id: f.id,
                leaf: match i % 4 {
                    0 => InputLeaf::Bool,
                    1 => InputLeaf::I128 { min: -7, max: -7 },
                    2 => InputLeaf::Enum {
                        type_id: u32::MAX,
                        min: -2,
                        max: -1,
                        variants: vec![
                            InputVariant { id: 500, code: -1 },
                            InputVariant { id: 0, code: -2 },
                        ],
                    },
                    _ => InputLeaf::Sum {
                        type_id: 0,
                        min: 17,
                        max: 18,
                        variants: vec![
                            InputVariant { id: 500, code: 18 },
                            InputVariant { id: 0, code: 17 },
                        ],
                    },
                },
            })
            .collect();
        let encoded =
            encode_record(&fields, usize::MAX).unwrap_or_else(|error| panic!("record: {error:?}"));
        let projected = execution::project_record(&encoded, &schema, limits);
        let (result, _, _) = projected.into_parts();
        let expected: Vec<_> = (0..count).map(|i| [1, -7, -2, 18][i % 4]).collect();
        assert_eq!(result, Ok(expected));
    }
}
