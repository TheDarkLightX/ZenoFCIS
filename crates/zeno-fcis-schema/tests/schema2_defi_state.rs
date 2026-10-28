//! Public API conformance for an inert full-width DeFi state schema.
//! No pricing, transition, authorization or effects are inferred from admission.
#![cfg(feature = "schema-profile-2")]

use zeno_fcis_codec::profile2::Envelope;
use zeno_fcis_codec::{DecodeError, DecodeLimits, Hash32, RustCryptoSha256 as Hash};
use zeno_fcis_schema::profile2::{
    FieldDef, FieldId, Schema, SchemaAdmittedEnvelope, SchemaEnvelopeError, SchemaLimits,
    SchemaProfile, TypeDef, TypeId, TypeKind, ValidationLimits, ValidationReport,
    ValueValidationError,
};
use zeno_fcis_value::profile2::{
    ConversionError, Field, I256, MapEntry, U256, Value, ValueError, ValueLimits,
};

fn definition(id: u32, name: &str, kind: TypeKind) -> TypeDef {
    TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default()).unwrap()
}

fn schema(revision: u16) -> Schema {
    let definitions = vec![
        definition(
            1,
            "AssetId",
            TypeKind::U256 {
                min: U256::ZERO,
                max: U256::MAX,
            },
        ),
        definition(
            2,
            "Amount",
            TypeKind::U256 {
                min: U256::ZERO,
                max: U256::MAX,
            },
        ),
        definition(
            3,
            "Position",
            TypeKind::I256 {
                min: I256::MIN,
                max: I256::MAX,
            },
        ),
        definition(
            4,
            "Balances",
            TypeKind::Map {
                key: TypeId::new(1),
                value: TypeId::new(2),
                min_len: 0,
                max_len: 2,
            },
        ),
        definition(
            5,
            "Positions",
            TypeKind::Vector {
                element: TypeId::new(3),
                min_len: 0,
                max_len: 2,
            },
        ),
        definition(
            6,
            "State",
            TypeKind::Record {
                fields: vec![
                    FieldDef::try_new(FieldId::new(1), "balances", TypeId::new(4)).unwrap(),
                    FieldDef::try_new(FieldId::new(2), "positions", TypeId::new(5)).unwrap(),
                    FieldDef::try_new(FieldId::new(3), "sequence", TypeId::new(7)).unwrap(),
                ]
                .into_boxed_slice(),
            },
        ),
        definition(
            7,
            "Sequence",
            TypeKind::U128 {
                min: 0,
                max: u128::MAX,
            },
        ),
    ];
    Schema::try_new(
        SchemaProfile::VALUE2,
        "DefiState",
        revision,
        TypeId::new(6),
        definitions,
        SchemaLimits::default(),
    )
    .unwrap()
}

fn state_with_first_amount(amount: Value) -> Value {
    let balances = Value::map_canonical(vec![
        MapEntry::try_new(Value::unsigned256(U256::ZERO), amount).unwrap(),
        MapEntry::try_new(
            Value::unsigned256(U256::from_words_le([0, 1])),
            Value::unsigned256(U256::ONE),
        )
        .unwrap(),
    ])
    .unwrap();
    let positions = Value::vector(vec![
        Value::signed256(I256::MIN),
        Value::signed256(I256::MAX),
    ])
    .unwrap();
    Value::record_canonical(vec![
        Field::new(1, balances),
        Field::new(2, positions),
        Field::new(3, Value::unsigned(u128::MAX)),
    ])
    .unwrap()
}

fn state() -> Value {
    state_with_first_amount(Value::unsigned256(U256::MAX))
}

fn admitted() -> SchemaAdmittedEnvelope {
    SchemaAdmittedEnvelope::try_new::<Hash>(&schema(1), state(), ValidationLimits::default())
        .unwrap()
}

#[test]
fn full_width_state_public_wire_roundtrip_and_owned_metrics() {
    let original = state();
    let retained = original.clone();
    let witness =
        SchemaAdmittedEnvelope::try_new::<Hash>(&schema(1), original, ValidationLimits::default())
            .unwrap();
    assert_eq!(witness.value().value(), &retained);
    assert_eq!(
        witness.validation_report(),
        ValidationReport {
            nodes: 10,
            maximum_depth: 2
        }
    );
    assert_eq!(
        witness.value().value().zcve_bytes().unwrap(),
        golden_state_bytes()
    );
    let bytes = witness.canonical_bytes().unwrap();
    let replay = SchemaAdmittedEnvelope::decode_expected::<Hash>(
        &schema(1),
        &bytes,
        DecodeLimits::default(),
        ValidationLimits::default(),
    )
    .unwrap();
    assert_eq!(replay, witness);
    assert_eq!(replay.canonical_bytes().unwrap(), bytes);
    assert_eq!(retained, state());
}

#[test]
fn matching_schema_hash_does_not_certify_narrow_amount() {
    let selected = schema(1);
    let raw = Envelope::new(
        6,
        selected.schema_hash::<Hash>().unwrap(),
        state_with_first_amount(Value::unsigned(0)),
    );
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &selected,
            raw,
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::SchemaValidation(
            ValueValidationError::TypeMismatch
        ))
    );
}

#[test]
fn equal_zero_payload_does_not_erase_signedness() {
    assert_eq!(
        schema(1).validate_root(
            &state_with_first_amount(Value::signed256(I256::ZERO)),
            ValidationLimits::default()
        ),
        Err(ValueValidationError::TypeMismatch)
    );
}

#[test]
fn signed_full_width_subrange_is_numeric_not_bit_or_narrow_order() {
    let selected = Schema::try_new(
        SchemaProfile::VALUE2,
        "NegativePosition",
        1,
        TypeId::new(1),
        vec![definition(
            1,
            "Negative",
            TypeKind::I256 {
                min: I256::MIN,
                max: I256::from_i128(-1),
            },
        )],
        SchemaLimits::default(),
    )
    .unwrap();
    for value in [I256::MIN, I256::from_i128(-1)] {
        assert!(
            selected
                .validate_root(&Value::signed256(value), ValidationLimits::default())
                .is_ok()
        );
    }
    assert_eq!(
        selected.validate_root(&Value::signed256(I256::ZERO), ValidationLimits::default()),
        Err(ValueValidationError::IntegerRange)
    );
}

#[test]
fn expected_type_refuses_before_hash_or_validation_budget() {
    let raw = Envelope::new(7, Hash32::ZERO, state());
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &schema(1),
            raw,
            ValidationLimits {
                max_depth: 0,
                max_nodes: 0
            }
        ),
        Err(SchemaEnvelopeError::TypeIdentity {
            expected: TypeId::new(6),
            actual: TypeId::new(7)
        })
    );
}

#[test]
fn application_revision_is_part_of_expected_schema_identity() {
    let current = schema(2);
    let old = schema(1);
    let raw = Envelope::new(6, old.schema_hash::<Hash>().unwrap(), state());
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &current,
            raw,
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::SchemaIdentity {
            expected: current.schema_hash::<Hash>().unwrap(),
            actual: old.schema_hash::<Hash>().unwrap(),
        })
    );
}

#[test]
fn logical_resource_boundaries_and_decoder_precedence_are_explicit() {
    let selected = schema(1);
    let value = state();
    assert_eq!(
        selected.validate_root(
            &value,
            ValidationLimits {
                max_depth: 2,
                max_nodes: 10
            }
        ),
        Ok(ValidationReport {
            nodes: 10,
            maximum_depth: 2
        })
    );
    for limits in [
        ValidationLimits {
            max_depth: 1,
            max_nodes: 10,
        },
        ValidationLimits {
            max_depth: 2,
            max_nodes: 9,
        },
    ] {
        assert_eq!(
            selected.validate_root(&value, limits),
            Err(ValueValidationError::BudgetExceeded)
        );
    }
    let bytes = admitted().canonical_bytes().unwrap();
    assert!(matches!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &selected,
            &bytes,
            DecodeLimits {
                max_input_bytes: 0,
                ..DecodeLimits::default()
            },
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::Decode(DecodeError::InputLimit { .. }))
    ));
}

#[test]
fn trailing_bytes_and_legacy_magic_do_not_admit_as_profile2() {
    let bytes = admitted().canonical_bytes().unwrap();
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema(1),
            &trailing,
            DecodeLimits::default(),
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::Decode(
            DecodeError::TrailingBytes { .. }
        ))
    ));
    let mut legacy = bytes;
    legacy[..8].copy_from_slice(b"ZFCISV1\0");
    assert_eq!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema(1),
            &legacy,
            DecodeLimits::default(),
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::Decode(DecodeError::EnvelopeMagic))
    );
}

#[test]
fn noncanonical_map_is_rejected_at_structural_boundary() {
    let greater =
        MapEntry::try_new(Value::unsigned256(U256::ONE), Value::unsigned256(U256::ONE)).unwrap();
    let smaller = MapEntry::try_new(
        Value::unsigned256(U256::ZERO),
        Value::unsigned256(U256::ONE),
    )
    .unwrap();
    assert_eq!(
        Value::map_canonical(vec![greater, smaller]),
        Err(ValueError::MapKeyOrder)
    );
}

#[test]
fn legacy_bare_bytes_stay_identical_but_nested_wide_leaves_never_narrow() {
    let legacy = zeno_fcis_value::Value::unsigned(u128::MAX);
    let lifted = Value::lift_legacy(&legacy, ValueLimits::default()).unwrap();
    assert_eq!(legacy.zcve_bytes().unwrap(), lifted.zcve_bytes().unwrap());
    assert_eq!(
        state().try_into_legacy(ValueLimits::default()),
        Err(ConversionError::WideInteger)
    );
    assert_eq!(
        state_with_first_amount(Value::unsigned256(U256::ZERO))
            .try_into_legacy(ValueLimits::default()),
        Err(ConversionError::WideInteger)
    );
}

fn golden_state_bytes() -> Vec<u8> {
    const HEX: &str = include_str!("schema2-defi-state.hex");
    let text = HEX.trim();
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b| match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                _ => panic!("golden hex"),
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect()
}
