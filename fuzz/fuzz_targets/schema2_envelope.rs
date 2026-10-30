#![no_main]

//! Schema2 envelope admission over codec2 bytes, against a small fixed schema.
//!
//! Differential oracle: the same bytes go through two admission routes that
//! must agree. Route A is `SchemaAdmittedEnvelope::decode_expected` on the raw
//! bytes. Route B is codec2's `decode_envelope` followed by
//! `SchemaAdmittedEnvelope::try_from_envelope`. Both must accept or both
//! refuse; a codec2 refusal in A must be the same `DecodeError` B sees; an
//! accepted envelope must carry the schema's root type and hash, re-encode
//! to the input bytes and admit identically when rebuilt from its parts.
//! Schema admission grants nothing beyond structural validity.

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::profile2::decode_envelope;
use zeno_fcis_codec::{DecodeLimits, RustCryptoSha256 as Hash};
use zeno_fcis_schema::profile2::{
    FieldDef, FieldId, Schema, SchemaAdmittedEnvelope, SchemaEnvelopeError, SchemaLimits,
    SchemaProfile, TypeDef, TypeId, TypeKind, ValidationLimits,
};

fn definition(id: u32, name: &str, kind: TypeKind) -> TypeDef {
    let Ok(definition) = TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default())
    else {
        panic!("fixed harness schema type must build");
    };
    definition
}

fn field(id: u16, name: &str, of: u32) -> FieldDef {
    let Ok(field) = FieldDef::try_new(FieldId::new(id), name, TypeId::new(of)) else {
        panic!("fixed harness schema field must build");
    };
    field
}

/// A small record so that random bytes reach the validator: a flag, a bounded
/// count, a short byte string and a short vector of counts.
fn schema() -> Schema {
    let definitions = vec![
        definition(1, "Flag", TypeKind::Bool),
        definition(2, "Count", TypeKind::U128 { min: 0, max: 1000 }),
        definition(
            3,
            "Tag",
            TypeKind::Bytes {
                min_len: 0,
                max_len: 8,
            },
        ),
        definition(
            4,
            "Counts",
            TypeKind::Vector {
                element: TypeId::new(2),
                min_len: 0,
                max_len: 4,
            },
        ),
        definition(
            5,
            "State",
            TypeKind::Record {
                fields: vec![
                    field(1, "flag", 1),
                    field(2, "count", 2),
                    field(3, "tag", 3),
                    field(4, "counts", 4),
                ]
                .into_boxed_slice(),
            },
        ),
    ];
    let Ok(schema) = Schema::try_new(
        SchemaProfile::VALUE2,
        "FuzzState",
        1,
        TypeId::new(5),
        definitions,
        SchemaLimits::default(),
    ) else {
        panic!("fixed harness schema must build");
    };
    schema
}

fuzz_target!(|input: &[u8]| {
    let schema = schema();
    let decode_limits = DecodeLimits::default();
    let validation_limits = ValidationLimits::default();

    let route_a = SchemaAdmittedEnvelope::decode_expected::<Hash>(
        &schema,
        input,
        decode_limits,
        validation_limits,
    );
    let codec = decode_envelope(input, decode_limits);

    match (route_a, codec) {
        (Ok(admitted), Ok(envelope)) => {
            assert_eq!(
                admitted.root_type(),
                schema.root_type(),
                "admitted root type is the schema's"
            );
            assert_eq!(
                envelope.type_id(),
                schema.root_type().get(),
                "codec2 type id equals the schema root"
            );
            assert_eq!(
                envelope.schema_hash(),
                admitted.schema_hash(),
                "codec2 hash equals the admitted hash"
            );
            let Ok(canonical) = admitted.canonical_bytes() else {
                panic!("an admitted envelope must be canonically encodable");
            };
            assert_eq!(canonical, input, "admission accepts canonical bytes only");
            let Ok(route_b) = SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
                &schema,
                envelope,
                validation_limits,
            ) else {
                panic!("route B must admit what route A admitted");
            };
            assert_eq!(
                route_b, admitted,
                "both admission routes must produce the same envelope"
            );
            assert_eq!(route_b.value(), admitted.value());
        }
        (Ok(_), Err(error)) => panic!("schema admission accepted bytes codec2 refuses: {error:?}"),
        (Err(SchemaEnvelopeError::Decode(a)), Err(b)) => {
            assert_eq!(a, b, "route A must report codec2's own decode error");
        }
        (Err(other), Err(b)) => panic!("codec2 refused ({b:?}) but admission reported {other:?}"),
        (Err(reason), Ok(envelope)) => {
            // codec2 accepted the framing and value; the schema layer refused. Route B
            // must refuse for the same reason, and a decode error is impossible here.
            assert!(
                !matches!(reason, SchemaEnvelopeError::Decode(_)),
                "decode error after a successful decode"
            );
            match SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
                &schema,
                envelope,
                validation_limits,
            ) {
                Ok(_) => panic!("route B admitted what route A refused: {reason:?}"),
                Err(route_b) => assert_eq!(
                    route_b, reason,
                    "both routes must refuse for the same reason"
                ),
            }
        }
    }
});
