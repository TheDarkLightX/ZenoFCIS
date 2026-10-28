//! Prepared source tests. No Rust execution is claimed by this private packet.
use super::*;
use alloc::{boxed::Box, vec, vec::Vec};
use zeno_fcis_codec::profile2::Envelope;
use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, Hash32, RustCryptoSha256 as Hash,
};
use zeno_fcis_value::profile2::{
    Field, I256, MapEntry, U256, Value, ValueError, ValueProfile, ValueRef,
};

fn type_def(id: u32, name: &str, kind: TypeKind) -> TypeDef {
    TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default()).expect("type fixture")
}

fn schema(root: u32, types: Vec<TypeDef>) -> Schema {
    Schema::try_new(
        SchemaProfile::VALUE2,
        "Test",
        1,
        TypeId::new(root),
        types,
        SchemaLimits::default(),
    )
    .expect("schema fixture")
}

fn unsigned_schema(min: U256, max: U256) -> Schema {
    schema(1, vec![type_def(1, "Number", TypeKind::U256 { min, max })])
}

fn signed_schema(min: I256, max: I256) -> Schema {
    schema(1, vec![type_def(1, "Number", TypeKind::I256 { min, max })])
}

fn validation(value: &Schema, item: &Value) -> Result<ValidationReport, ValueValidationError> {
    value.validate_root(item, ValidationLimits::default())
}

fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("golden hex"),
            };
            (nibble(pair[0]) << 4) | nibble(pair[1])
        })
        .collect()
}

#[test]
fn profile_selection_is_closed_and_not_application_revision() {
    assert_eq!(
        SchemaProfile::admit(2, ValueProfile::Zcve2),
        Ok(SchemaProfile::VALUE2)
    );
    for format in [0, 1, 3, u16::MAX] {
        assert_eq!(
            SchemaProfile::admit(format, ValueProfile::Zcve2),
            Err(ProfileError::UnsupportedSchemaFormat(format))
        );
    }
    assert_eq!(
        SchemaProfile::admit(2, ValueProfile::Zcve1),
        Err(ProfileError::UnsupportedValueProfile(ValueProfile::Zcve1))
    );
    assert_eq!(
        SchemaProfile::admit(1, ValueProfile::Zcve1),
        Err(ProfileError::UnsupportedSchemaFormat(1))
    );
    assert_eq!(SchemaProfile::VALUE2.schema_format(), 2);
    assert_eq!(SchemaProfile::VALUE2.value_profile(), ValueProfile::Zcve2);
    let def = type_def(
        0,
        "ZeroId",
        TypeKind::U256 {
            min: U256::ZERO,
            max: U256::MAX,
        },
    );
    let app = Schema::try_new(
        SchemaProfile::VALUE2,
        "App",
        1,
        TypeId::new(0),
        vec![def],
        SchemaLimits::default(),
    )
    .unwrap();
    assert_eq!(app.version(), 1);
    assert_eq!(app.root_type(), TypeId::new(0));
    assert!(app.canonical_bytes().unwrap().starts_with(SCHEMA_MAGIC));
}

#[test]
fn admits_every_unsigned_boundary_without_narrowing() {
    let schema = unsigned_schema(U256::ZERO, U256::MAX);
    for number in [
        U256::ZERO,
        U256::ONE,
        U256::from_u128(u128::MAX),
        U256::from_words_le([0, 1]),
        U256::from_words_le([0, 1u128 << 127]),
        U256::MAX,
    ] {
        let value = Value::unsigned256(number);
        let before = value.clone();
        assert_eq!(
            validation(&schema, &value),
            Ok(ValidationReport {
                nodes: 1,
                maximum_depth: 0
            })
        );
        assert_eq!(value, before);
    }
}

#[test]
fn admits_signed_minimum_negative_zero_and_maximum() {
    let schema = signed_schema(I256::MIN, I256::MAX);
    for number in [
        I256::MIN,
        I256::from_i128(i128::MIN),
        I256::from_i128(-1),
        I256::ZERO,
        I256::from_i128(1),
        I256::MAX,
    ] {
        assert!(validation(&schema, &Value::signed256(number)).is_ok());
    }
}

#[test]
fn exact_unsigned_nontrivial_interval_and_neighbors() {
    let min = U256::from_words_le([17, 3]);
    let max = U256::from_words_le([31, 5]);
    let schema = unsigned_schema(min, max);
    for n in [min, max] {
        assert!(validation(&schema, &Value::unsigned256(n)).is_ok());
    }
    for n in [
        min.checked_sub(U256::ONE).unwrap(),
        max.checked_add(U256::ONE).unwrap(),
    ] {
        assert_eq!(
            validation(&schema, &Value::unsigned256(n)),
            Err(ValueValidationError::IntegerRange)
        );
    }
}

#[test]
fn exact_signed_interval_crosses_zero_at_full_width() {
    let min = I256::from_bits(U256::from_words_le([
        u128::MAX - 8,
        u128::MAX - (1u128 << 72),
    ]));
    let max = I256::from_bits(U256::from_words_le([31, 5]));
    let schema = signed_schema(min, max);
    for n in [min, I256::from_i128(-1), I256::ZERO, max] {
        assert!(validation(&schema, &Value::signed256(n)).is_ok());
    }
    for n in [
        min.checked_sub(I256::from_i128(1)).unwrap(),
        max.checked_add(I256::from_i128(1)).unwrap(),
    ] {
        assert_eq!(
            validation(&schema, &Value::signed256(n)),
            Err(ValueValidationError::IntegerRange)
        );
    }
}

#[test]
fn reversed_ranges_refuse_using_nominal_numeric_order() {
    assert_eq!(
        TypeDef::try_new(
            TypeId::new(1),
            "Bad",
            TypeKind::U256 {
                min: U256::MAX,
                max: U256::ZERO
            },
            SchemaLimits::default()
        ),
        Err(SchemaError::InvalidRange)
    );
    assert_eq!(
        TypeDef::try_new(
            TypeId::new(1),
            "Bad",
            TypeKind::I256 {
                min: I256::MAX,
                max: I256::MIN
            },
            SchemaLimits::default()
        ),
        Err(SchemaError::InvalidRange)
    );
}

#[test]
fn signedness_and_width_are_nominal_even_for_zero() {
    let unsigned = unsigned_schema(U256::ZERO, U256::MAX);
    let signed = signed_schema(I256::MIN, I256::MAX);
    for value in [
        Value::unsigned(0),
        Value::signed(0),
        Value::signed256(I256::ZERO),
    ] {
        assert_eq!(
            validation(&unsigned, &value),
            Err(ValueValidationError::TypeMismatch)
        );
    }
    for value in [
        Value::unsigned(0),
        Value::signed(0),
        Value::unsigned256(U256::ZERO),
    ] {
        assert_eq!(
            validation(&signed, &value),
            Err(ValueValidationError::TypeMismatch)
        );
    }
    let narrow = schema(
        1,
        vec![type_def(1, "Narrow", TypeKind::U128 { min: 0, max: 10 })],
    );
    assert_eq!(
        validation(&narrow, &Value::unsigned256(U256::ZERO)),
        Err(ValueValidationError::TypeMismatch)
    );
}

#[test]
fn schema_bytes_match_independent_distinct_limb_and_signed_goldens() {
    let u = Schema::try_new(
        SchemaProfile::VALUE2,
        "Wide",
        4,
        TypeId::new(11),
        vec![type_def(
            11,
            "Unsigned",
            TypeKind::U256 {
                min: U256::from_words_le([17, 3]),
                max: U256::from_words_le([31, 5]),
            },
        )],
        SchemaLimits::default(),
    )
    .unwrap();
    let i = Schema::try_new(
        SchemaProfile::VALUE2,
        "Wide",
        4,
        TypeId::new(12),
        vec![type_def(
            12,
            "Signed",
            TypeKind::I256 {
                min: I256::from_bits(U256::from_words_le([
                    u128::MAX - 8,
                    u128::MAX - (1u128 << 72),
                ])),
                max: I256::from_bits(U256::from_words_le([31, 5])),
            },
        )],
        SchemaLimits::default(),
    )
    .unwrap();
    assert_eq!(u.canonical_bytes().unwrap(), hex(GOLDEN_UNSIGNED));
    assert_eq!(i.canonical_bytes().unwrap(), hex(GOLDEN_SIGNED));
}

#[test]
fn schema1_golden_and_domain_remain_legacy_while_schema2_is_disjoint() {
    let old = crate::Schema::try_new(
        "Compat",
        9,
        TypeId::new(7),
        vec![
            crate::TypeDef::try_new(
                TypeId::new(7),
                "Small",
                crate::TypeKind::U128 { min: 1, max: 100 },
                SchemaLimits::default(),
            )
            .unwrap(),
        ],
        SchemaLimits::default(),
    )
    .unwrap();
    let new = Schema::try_new(
        SchemaProfile::VALUE2,
        "Compat",
        9,
        TypeId::new(7),
        vec![type_def(7, "Small", TypeKind::U128 { min: 1, max: 100 })],
        SchemaLimits::default(),
    )
    .unwrap();
    assert_eq!(old.canonical_bytes().unwrap(), hex(GOLDEN_LEGACY));
    assert_eq!(new.canonical_bytes().unwrap(), hex(GOLDEN_COMPAT2));
    assert_eq!(
        &old.canonical_bytes().unwrap()[13..],
        &new.canonical_bytes().unwrap()[13..]
    );
    assert_ne!(
        old.schema_hash::<Hash>().unwrap(),
        new.schema_hash::<Hash>().unwrap()
    );
}

fn record_map_schema() -> Schema {
    let amount = type_def(
        1,
        "Amount",
        TypeKind::U256 {
            min: U256::ONE,
            max: U256::MAX,
        },
    );
    let signed = type_def(
        2,
        "Signed",
        TypeKind::I256 {
            min: I256::MIN,
            max: I256::MAX,
        },
    );
    let record = type_def(
        3,
        "Entry",
        TypeKind::Record {
            fields: vec![
                FieldDef::try_new(FieldId::new(2), "delta", TypeId::new(2)).unwrap(),
                FieldDef::try_new(FieldId::new(1), "amount", TypeId::new(1)).unwrap(),
            ]
            .into_boxed_slice(),
        },
    );
    let map = type_def(
        4,
        "Entries",
        TypeKind::Map {
            key: TypeId::new(2),
            value: TypeId::new(3),
            min_len: 1,
            max_len: 2,
        },
    );
    schema(4, vec![map, signed, amount, record])
}

fn record(amount: Value, delta: I256) -> Value {
    Value::record_canonical(vec![
        Field::new(1, amount),
        Field::new(2, Value::signed256(delta)),
    ])
    .unwrap()
}

fn record_map() -> Value {
    Value::normalize_map(vec![
        MapEntry::try_new(
            Value::signed256(I256::from_i128(-1)),
            record(Value::unsigned256(U256::MAX), I256::MIN),
        )
        .unwrap(),
        MapEntry::try_new(
            Value::signed256(I256::ZERO),
            record(Value::unsigned256(U256::ONE), I256::MAX),
        )
        .unwrap(),
    ])
    .unwrap()
}

#[test]
fn nested_record_map_uses_encoded_keys_and_counts_both_children() {
    let schema = record_map_schema();
    let value = record_map();
    assert_eq!(
        validation(&schema, &value),
        Ok(ValidationReport {
            nodes: 9,
            maximum_depth: 2
        })
    );
    let ValueRef::Map(entries) = value.view() else {
        panic!("map fixture");
    };
    assert_eq!(entries[0].key(), &Value::signed256(I256::ZERO));
    assert_eq!(entries[1].key(), &Value::signed256(I256::from_i128(-1)));
    assert_eq!(
        schema.validate_root(
            &value,
            ValidationLimits {
                max_nodes: 8,
                max_depth: 2
            }
        ),
        Err(ValueValidationError::BudgetExceeded)
    );
    assert_eq!(
        schema.validate_root(
            &value,
            ValidationLimits {
                max_nodes: 9,
                max_depth: 1
            }
        ),
        Err(ValueValidationError::BudgetExceeded)
    );
}

#[test]
fn nested_range_and_record_shape_refuse() {
    let schema = record_map_schema();
    let invalid = Value::map_canonical(vec![
        MapEntry::try_new(
            Value::signed256(I256::ZERO),
            record(Value::unsigned256(U256::ZERO), I256::ZERO),
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(
        validation(&schema, &invalid),
        Err(ValueValidationError::IntegerRange)
    );
    let wrong_fields = Value::record_canonical(vec![
        Field::new(1, Value::unsigned256(U256::ONE)),
        Field::new(3, Value::signed256(I256::ZERO)),
    ])
    .unwrap();
    assert_eq!(
        schema.validate_value(TypeId::new(3), &wrong_fields, ValidationLimits::default()),
        Err(ValueValidationError::RecordShape)
    );
    let both_bad = Value::record_canonical(vec![
        Field::new(1, Value::unsigned(1)),
        Field::new(3, Value::signed256(I256::ZERO)),
    ])
    .unwrap();
    assert_eq!(
        schema.validate_value(TypeId::new(3), &both_bad, ValidationLimits::default()),
        Err(ValueValidationError::TypeMismatch)
    );
}

#[test]
fn schema_and_field_declaration_order_are_normalized() {
    let left = record_map_schema();
    let mut types = left.types().to_vec();
    types.reverse();
    let right = Schema::try_new(
        SchemaProfile::VALUE2,
        "Test",
        1,
        TypeId::new(4),
        types,
        SchemaLimits::default(),
    )
    .unwrap();
    assert_eq!(left.canonical_bytes(), right.canonical_bytes());
    assert_eq!(left.schema_hash::<Hash>(), right.schema_hash::<Hash>());
}

#[test]
fn complete_structural_vocabulary_remains_available_in_schema2() {
    let defs = vec![
        type_def(1, "Unit", TypeKind::Unit),
        type_def(2, "Bool", TypeKind::Bool),
        type_def(
            3,
            "U128",
            TypeKind::U128 {
                min: 0,
                max: u128::MAX,
            },
        ),
        type_def(
            4,
            "I128",
            TypeKind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        ),
        type_def(
            5,
            "Bytes",
            TypeKind::Bytes {
                min_len: 1,
                max_len: 2,
            },
        ),
        type_def(
            6,
            "Text",
            TypeKind::Text {
                min_len: 1,
                max_len: 2,
            },
        ),
        type_def(
            7,
            "Enum",
            TypeKind::Enum {
                variants: vec![EnumVariantDef::try_new(VariantId::new(0), "Zero").unwrap()]
                    .into_boxed_slice(),
            },
        ),
        type_def(
            8,
            "Tuple",
            TypeKind::Tuple {
                items: vec![TypeId::new(1)].into_boxed_slice(),
            },
        ),
        type_def(
            9,
            "Record",
            TypeKind::Record {
                fields: vec![FieldDef::try_new(FieldId::new(0), "flag", TypeId::new(2)).unwrap()]
                    .into_boxed_slice(),
            },
        ),
        type_def(
            10,
            "Sum",
            TypeKind::Sum {
                variants: vec![
                    SumVariantDef::try_new(VariantId::new(0), "Item", Some(TypeId::new(3)))
                        .unwrap(),
                ]
                .into_boxed_slice(),
            },
        ),
        type_def(
            11,
            "Vector",
            TypeKind::Vector {
                element: TypeId::new(1),
                min_len: 1,
                max_len: 1,
            },
        ),
        type_def(
            12,
            "Map",
            TypeKind::Map {
                key: TypeId::new(3),
                value: TypeId::new(4),
                min_len: 1,
                max_len: 1,
            },
        ),
        type_def(
            13,
            "U256",
            TypeKind::U256 {
                min: U256::ZERO,
                max: U256::MAX,
            },
        ),
        type_def(
            14,
            "I256",
            TypeKind::I256 {
                min: I256::MIN,
                max: I256::MAX,
            },
        ),
        type_def(
            15,
            "Root",
            TypeKind::Tuple {
                items: (1..=14)
                    .map(TypeId::new)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            },
        ),
    ];
    let schema = schema(15, defs);
    let values = vec![
        Value::unit(),
        Value::boolean(true),
        Value::unsigned(u128::MAX),
        Value::signed(i128::MIN),
        Value::bytes(vec![1]).unwrap(),
        Value::text_ascii("x".into()).unwrap(),
        Value::enumeration(7, 0),
        Value::tuple(vec![Value::unit()]).unwrap(),
        Value::record_canonical(vec![Field::new(0, Value::boolean(true))]).unwrap(),
        Value::sum(10, 0, Some(Value::unsigned(3))),
        Value::vector(vec![Value::unit()]).unwrap(),
        Value::map_canonical(vec![
            MapEntry::try_new(Value::unsigned(1), Value::signed(-1)).unwrap(),
        ])
        .unwrap(),
        Value::unsigned256(U256::MAX),
        Value::signed256(I256::MIN),
    ];
    assert!(validation(&schema, &Value::tuple(values).unwrap()).is_ok());
}

#[test]
fn enum_sum_type_identity_payload_and_variant_are_checked() {
    let schema = schema(
        2,
        vec![
            type_def(
                1,
                "Wide",
                TypeKind::U256 {
                    min: U256::ZERO,
                    max: U256::MAX,
                },
            ),
            type_def(
                2,
                "Choice",
                TypeKind::Sum {
                    variants: vec![
                        SumVariantDef::try_new(VariantId::new(0), "None", None).unwrap(),
                        SumVariantDef::try_new(VariantId::new(1), "Some", Some(TypeId::new(1)))
                            .unwrap(),
                    ]
                    .into_boxed_slice(),
                },
            ),
            type_def(
                3,
                "Flag",
                TypeKind::Enum {
                    variants: vec![EnumVariantDef::try_new(VariantId::new(0), "No").unwrap()]
                        .into_boxed_slice(),
                },
            ),
        ],
    );
    assert_eq!(
        validation(&schema, &Value::sum(3, 0, None)),
        Err(ValueValidationError::TypeIdentity)
    );
    assert_eq!(
        validation(&schema, &Value::sum(2, 2, None)),
        Err(ValueValidationError::UnknownVariant(VariantId::new(2)))
    );
    assert_eq!(
        validation(&schema, &Value::sum(2, 1, None)),
        Err(ValueValidationError::MissingPayload)
    );
    assert_eq!(
        validation(&schema, &Value::sum(2, 0, Some(Value::unit()))),
        Err(ValueValidationError::UnexpectedPayload)
    );
    assert!(
        validation(
            &schema,
            &Value::sum(2, 1, Some(Value::unsigned256(U256::MAX)))
        )
        .is_ok()
    );
    assert_eq!(
        schema.validate_value(
            TypeId::new(3),
            &Value::enumeration(2, 0),
            ValidationLimits::default()
        ),
        Err(ValueValidationError::TypeIdentity)
    );
}

#[test]
fn budgets_precede_unknown_type_and_kind_checks() {
    let schema = unsigned_schema(U256::ZERO, U256::MAX);
    let none = ValidationLimits {
        max_nodes: 0,
        max_depth: 0,
    };
    assert_eq!(
        schema.validate_value(TypeId::new(99), &Value::unit(), none),
        Err(ValueValidationError::BudgetExceeded)
    );
    assert_eq!(
        schema.validate_value(TypeId::new(99), &Value::unit(), ValidationLimits::default()),
        Err(ValueValidationError::UnknownType(TypeId::new(99)))
    );
    assert_eq!(
        schema.validate_root(&Value::boolean(false), none),
        Err(ValueValidationError::BudgetExceeded)
    );
}

#[test]
fn closure_duplicates_cycles_and_local_schema_caps_refuse() {
    let limits = SchemaLimits::default();
    let missing = type_def(
        1,
        "Missing",
        TypeKind::Tuple {
            items: vec![TypeId::new(2)].into_boxed_slice(),
        },
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![missing],
            limits
        ),
        Err(SchemaError::UnknownTypeReference(TypeId::new(2)))
    );
    let cycle = type_def(
        1,
        "Cycle",
        TypeKind::Tuple {
            items: vec![TypeId::new(1)].into_boxed_slice(),
        },
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![cycle],
            limits
        ),
        Err(SchemaError::RecursiveTypeCycle(TypeId::new(1)))
    );
    let def = type_def(1, "A", TypeKind::Unit);
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![def.clone(), def],
            limits
        ),
        Err(SchemaError::DuplicateTypeId(TypeId::new(1)))
    );
    assert_eq!(
        TypeDef::try_new(
            TypeId::new(1),
            "Fields",
            TypeKind::Record {
                fields: vec![FieldDef::try_new(FieldId::new(1), "a", TypeId::new(2)).unwrap()]
                    .into_boxed_slice()
            },
            SchemaLimits {
                max_fields: 0,
                ..limits
            }
        ),
        Err(SchemaError::LimitExceeded)
    );
}

#[test]
fn root_and_selected_type_witness_bind_exact_owned_value_and_metrics() {
    let schema = schema(
        1,
        vec![
            type_def(
                1,
                "Wide",
                TypeKind::U256 {
                    min: U256::ZERO,
                    max: U256::MAX,
                },
            ),
            type_def(2, "Flag", TypeKind::Bool),
        ],
    );
    let root = SchemaAdmittedEnvelope::try_new::<Hash>(
        &schema,
        Value::unsigned256(U256::MAX),
        ValidationLimits::default(),
    )
    .unwrap();
    assert_eq!(root.root_type(), TypeId::new(1));
    assert_eq!(
        root.validation_report(),
        ValidationReport {
            nodes: 1,
            maximum_depth: 0
        }
    );
    assert_eq!(root.value().value(), &Value::unsigned256(U256::MAX));
    let selected = SchemaAdmittedTypeEnvelope::try_new::<Hash>(
        &schema,
        TypeId::new(2),
        Value::boolean(true),
        ValidationLimits::default(),
    )
    .unwrap();
    assert_eq!(selected.type_id(), TypeId::new(2));
    assert_eq!(selected.schema_hash(), root.schema_hash());
    assert_eq!(selected.value().value(), &Value::boolean(true));
}

#[test]
fn expected_type_precedes_hash_value_and_validation_budget() {
    let schema = unsigned_schema(U256::ZERO, U256::MAX);
    let raw = Envelope::new(99, Hash32::ZERO, Value::boolean(false));
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &schema,
            raw,
            ValidationLimits {
                max_nodes: 0,
                max_depth: 0
            }
        ),
        Err(SchemaEnvelopeError::TypeIdentity {
            expected: TypeId::new(1),
            actual: TypeId::new(99)
        })
    );
}

#[test]
fn expected_hash_precedes_shape_and_matching_pin_is_not_certification() {
    let schema = unsigned_schema(U256::ONE, U256::MAX);
    let expected = schema.schema_hash::<Hash>().unwrap();
    let wrong = Envelope::new(1, Hash32::ZERO, Value::boolean(false));
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &schema,
            wrong,
            ValidationLimits {
                max_nodes: 0,
                max_depth: 0
            }
        ),
        Err(SchemaEnvelopeError::SchemaIdentity {
            expected,
            actual: Hash32::ZERO
        })
    );
    let forged = Envelope::new(1, expected, Value::unsigned256(U256::ZERO));
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &schema,
            forged,
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::SchemaValidation(
            ValueValidationError::IntegerRange
        ))
    );
}

#[test]
fn codec2_roundtrip_rechecks_expected_schema_and_exact_bytes() {
    let schema = record_map_schema();
    let root =
        SchemaAdmittedEnvelope::try_new::<Hash>(&schema, record_map(), ValidationLimits::default())
            .unwrap();
    let bytes = root.canonical_bytes().unwrap();
    assert_eq!(&bytes[..8], b"ZFCISV2\0");
    let replay = SchemaAdmittedEnvelope::decode_expected::<Hash>(
        &schema,
        &bytes,
        DecodeLimits::default(),
        ValidationLimits::default(),
    )
    .unwrap();
    assert_eq!(replay, root);
    assert_eq!(replay.canonical_bytes().unwrap(), bytes);
    assert_eq!(replay.encoded_length(), u64::try_from(bytes.len()).unwrap());
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(matches!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema,
            &extra,
            DecodeLimits::default(),
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::Decode(
            DecodeError::TrailingBytes { .. }
        ))
    ));
    assert!(matches!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema,
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
fn legacy_envelope_magic_refuses_even_for_old_tag_payload() {
    let schema = schema(
        1,
        vec![type_def(1, "Narrow", TypeKind::U128 { min: 0, max: 100 })],
    );
    let old = zeno_fcis_codec::Envelope::new(
        1,
        schema.schema_hash::<Hash>().unwrap(),
        zeno_fcis_value::Value::unsigned(42),
    );
    let mut bytes = Vec::new();
    old.encode_to(&mut bytes).unwrap();
    assert_eq!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema,
            &bytes,
            DecodeLimits::default(),
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::Decode(DecodeError::EnvelopeMagic))
    );
    assert!(matches!(
        SchemaAdmittedEnvelope::decode_expected::<Hash>(
            &schema,
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
fn codec2_magic_does_not_turn_a_schema1_hash_into_schema2_admission() {
    let old = crate::Schema::try_new(
        "Test",
        1,
        TypeId::new(1),
        vec![
            crate::TypeDef::try_new(
                TypeId::new(1),
                "Narrow",
                crate::TypeKind::U128 { min: 0, max: 100 },
                SchemaLimits::default(),
            )
            .unwrap(),
        ],
        SchemaLimits::default(),
    )
    .unwrap();
    let new = schema(
        1,
        vec![type_def(1, "Narrow", TypeKind::U128 { min: 0, max: 100 })],
    );
    let old_hash = old.schema_hash::<Hash>().unwrap();
    let expected = new.schema_hash::<Hash>().unwrap();
    assert_ne!(expected, old_hash);
    let raw = Envelope::new(1, old_hash, Value::unsigned(42));
    assert_eq!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(&new, raw, ValidationLimits::default()),
        Err(SchemaEnvelopeError::SchemaIdentity {
            expected,
            actual: old_hash
        })
    );
}

#[test]
fn application_revision_changes_bound_hash_without_changing_profile() {
    let first = unsigned_schema(U256::ZERO, U256::MAX);
    let second = Schema::try_new(
        SchemaProfile::VALUE2,
        "Test",
        2,
        first.root_type(),
        first.types().to_vec(),
        SchemaLimits::default(),
    )
    .unwrap();
    assert_ne!(first.schema_hash::<Hash>(), second.schema_hash::<Hash>());
    let raw = Envelope::new(
        1,
        first.schema_hash::<Hash>().unwrap(),
        Value::unsigned256(U256::ONE),
    );
    assert!(matches!(
        SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
            &second,
            raw,
            ValidationLimits::default()
        ),
        Err(SchemaEnvelopeError::SchemaIdentity { .. })
    ));
}

#[test]
fn default_structural_admission_is_not_replaced_by_schema_budget() {
    let mut types = vec![type_def(
        65,
        "Leaf",
        TypeKind::U256 {
            min: U256::ZERO,
            max: U256::MAX,
        },
    )];
    let mut value = Value::unsigned256(U256::ONE);
    for id in (0..65).rev() {
        types.push(type_def(
            id,
            &alloc::format!("Layer{id}"),
            TypeKind::Tuple {
                items: Box::new([TypeId::new(id + 1)]),
            },
        ));
        value = Value::tuple(vec![value]).unwrap();
    }
    let schema = schema(0, types);
    assert_eq!(
        SchemaAdmittedEnvelope::try_new::<Hash>(
            &schema,
            value,
            ValidationLimits {
                max_depth: 65,
                max_nodes: 66
            }
        ),
        Err(SchemaEnvelopeError::ValueAdmission(
            ValueError::DepthLimit {
                limit: 64,
                attempted: 65
            }
        ))
    );
}

#[test]
fn schema_limits_revalidate_types_created_under_looser_local_caps() {
    let limits = SchemaLimits::default();
    let leaf = type_def(
        1,
        "Leaf",
        TypeKind::U256 {
            min: U256::ZERO,
            max: U256::MAX,
        },
    );
    let variants = vec![
        EnumVariantDef::try_new(VariantId::new(0), "A").unwrap(),
        EnumVariantDef::try_new(VariantId::new(1), "B").unwrap(),
    ];
    let sums = vec![
        SumVariantDef::try_new(VariantId::new(0), "A", None).unwrap(),
        SumVariantDef::try_new(VariantId::new(1), "B", Some(TypeId::new(1))).unwrap(),
    ];
    let fields = vec![
        FieldDef::try_new(FieldId::new(1), "a", TypeId::new(1)).unwrap(),
        FieldDef::try_new(FieldId::new(2), "b", TypeId::new(1)).unwrap(),
    ];
    let cases = vec![
        (
            TypeKind::Tuple {
                items: vec![TypeId::new(1), TypeId::new(1)].into_boxed_slice(),
            },
            SchemaLimits {
                max_tuple_items: 1,
                ..limits
            },
        ),
        (
            TypeKind::Record {
                fields: fields.into_boxed_slice(),
            },
            SchemaLimits {
                max_fields: 1,
                ..limits
            },
        ),
        (
            TypeKind::Enum {
                variants: variants.into_boxed_slice(),
            },
            SchemaLimits {
                max_variants: 1,
                ..limits
            },
        ),
        (
            TypeKind::Sum {
                variants: sums.into_boxed_slice(),
            },
            SchemaLimits {
                max_variants: 1,
                ..limits
            },
        ),
    ];
    for (kind, strict) in cases {
        let root = type_def(2, "Root", kind);
        let before = root.clone();
        let types = vec![root.clone(), leaf.clone()];
        assert_eq!(
            Schema::try_new(
                SchemaProfile::VALUE2,
                "Test",
                1,
                TypeId::new(2),
                types.clone(),
                strict
            ),
            Err(SchemaError::LimitExceeded)
        );
        assert_eq!(types[0], before);
        let accepted = Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(2),
            types,
            limits,
        )
        .unwrap();
        assert_eq!(accepted.type_by_id(TypeId::new(2)).unwrap(), &before);
    }
}

#[test]
fn transitive_schema_caps_have_explicit_refusal_order() {
    let zero_fields = SchemaLimits {
        max_fields: 0,
        ..SchemaLimits::default()
    };
    let root = type_def(
        1,
        "Root",
        TypeKind::Record {
            fields: vec![FieldDef::try_new(FieldId::new(1), "missing", TypeId::new(9)).unwrap()]
                .into_boxed_slice(),
        },
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![root.clone(), root.clone()],
            zero_fields
        ),
        Err(SchemaError::DuplicateTypeId(TypeId::new(1)))
    );
    let same_name = type_def(2, "Root", TypeKind::Unit);
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![root.clone(), same_name],
            zero_fields
        ),
        Err(SchemaError::DuplicateTypeName)
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "9Invalid",
            1,
            TypeId::new(99),
            vec![root.clone()],
            zero_fields
        ),
        Err(SchemaError::LimitExceeded)
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![root.clone()],
            SchemaLimits::default()
        ),
        Err(SchemaError::UnknownTypeReference(TypeId::new(9)))
    );
    assert_eq!(
        Schema::try_new(
            SchemaProfile::VALUE2,
            "Test",
            1,
            TypeId::new(1),
            vec![root],
            SchemaLimits {
                max_types: 0,
                ..zero_fields
            }
        ),
        Err(SchemaError::LimitExceeded)
    );
}

const GOLDEN_LEGACY: &str = "5a46434953534348454d4131000006436f6d70617400090000000700000001000000070005536d616c6c020000000000000000000000000000000100000000000000000000000000000064";
const GOLDEN_COMPAT2: &str = "5a46434953534348454d4132000006436f6d70617400090000000700000001000000070005536d616c6c020000000000000000000000000000000100000000000000000000000000000064";
const GOLDEN_UNSIGNED: &str = "5a46434953534348454d41320000045769646500040000000b000000010000000b0008556e7369676e65640c0000000000000000000000000000000300000000000000000000000000000011000000000000000000000000000000050000000000000000000000000000001f";
const GOLDEN_SIGNED: &str = "5a46434953534348454d41320000045769646500040000000c000000010000000c00065369676e65640dfffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffff7000000000000000000000000000000050000000000000000000000000000001f";
