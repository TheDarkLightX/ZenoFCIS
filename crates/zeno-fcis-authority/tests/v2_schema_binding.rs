//! Correspondence with the actual existing canonical Schema encoder.
use zeno_fcis_schema::{
    FieldDef, FieldId, Schema, SchemaLimits, SumVariantDef, TypeDef, TypeId, TypeKind, VariantId,
};
use zeno_fcis_synthesis::finite::canonical_v2::schema::{
    self, Definition, Description, Field, Kind, Limits, Variant,
};

#[test]
fn original_canonical_schema_and_top_level_command_are_bound_without_reencoding_values() {
    let limits = SchemaLimits::default();
    let actual = Schema::try_new(
        "Original",
        0,
        TypeId::new(0),
        vec![
            TypeDef::try_new(
                TypeId::new(0),
                "State",
                TypeKind::Record {
                    fields: vec![
                        FieldDef::try_new(FieldId::new(0), "wide", TypeId::new(1))
                            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
                    ]
                    .into_boxed_slice(),
                },
                limits,
            )
            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
            TypeDef::try_new(
                TypeId::new(1),
                "Integer",
                TypeKind::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
                limits,
            )
            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
            TypeDef::try_new(
                TypeId::new(2),
                "Command",
                TypeKind::Sum {
                    variants: vec![
                        SumVariantDef::try_new(VariantId::new(0), "First", None)
                            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
                        SumVariantDef::try_new(VariantId::new(u16::MAX), "Last", None)
                            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
                    ]
                    .into_boxed_slice(),
                },
                limits,
            )
            .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}")),
        ],
        limits,
    )
    .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}"));
    let bytes = actual
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}"));
    let fields = [Field {
        id: 0,
        name: b"wide",
        type_id: 1,
    }];
    let variants = [
        Variant {
            id: 0,
            name: b"First",
        },
        Variant {
            id: u16::MAX,
            name: b"Last",
        },
    ];
    let definitions = [
        Definition {
            id: 0,
            name: b"State",
            kind: Kind::Record(&fields),
        },
        Definition {
            id: 1,
            name: b"Integer",
            kind: Kind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        Definition {
            id: 2,
            name: b"Command",
            kind: Kind::Sum(&variants),
        },
    ];
    let description = Description {
        profile: b"Original",
        version: 0,
        root: 0,
        definitions: &definitions,
    };
    let policy = Limits {
        bytes: u64::MAX,
        types: 4096,
        fields: 1024,
        variants: 1024,
    };
    let checked = schema::admit(&bytes, &description, policy)
        .unwrap_or_else(|error| panic!("schema oracle refused: {error:?}"));
    assert_eq!(checked.original(), bytes);
    assert_eq!(checked.original().as_ptr(), bytes.as_ptr());
    for index in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[index] ^= 1;
        assert!(schema::admit(&changed, &description, policy).is_err());
    }
    let omitted = Description {
        definitions: &definitions[..2],
        ..description
    };
    assert_eq!(
        schema::admit(&bytes, &omitted, policy).err(),
        Some(schema::Failure::Encoding)
    );
}
