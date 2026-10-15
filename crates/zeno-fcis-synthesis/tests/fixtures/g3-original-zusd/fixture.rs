//! Original single-vault characterization through declarative library-owned data.
//! The supplied authorization bit is an assumption, not caller authentication.
#![allow(dead_code)]
use c::Atom as A;
use l::{Observation as O, Op as L};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::{FieldDef, FieldId, Schema, SchemaLimits, TypeDef, TypeId, TypeKind};
use zeno_fcis_synthesis::finite::{
    V2InputField, V2IntegerWidth as Width, V2NumericOp as N, V2NumericProgram, V2Resource as R,
    canonical_v2::schema as s,
    v2_authority::{self as a, Authority, Publication, PublicationOutcome},
    v2_catalog as catalog, v2_composition as c, v2_laws as l, v2_zero_limits,
};
use zeno_fcis_value::{Field, Value};

const AMOUNT: c::Domain<'static> = c::Domain::U128 {
    min: 0,
    max: 1_000_000_000_000_000_000_000_000_000_000,
};
const RAW: c::Domain<'static> = c::Domain::U128 {
    min: 0,
    max: u128::MAX,
};
const BIT: c::Domain<'static> = c::Domain::U128 { min: 0, max: 1 };
const CONTEXT: c::Domain<'static> = c::Domain::Bool;
include!("data.rs");

pub(crate) fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|e| panic!("original zUSD fixture refused: {e:?}"))
}
pub(crate) fn descriptor() -> c::NumericDescriptor<'static> {
    let mut limits = v2_zero_limits();
    for resource in [
        R::Read,
        R::Write,
        R::Candidate,
        R::Effect,
        R::Byte,
        R::WitnessByte,
        R::Depth,
        R::Step,
    ] {
        limits = limits.with_limit(resource, 4_000_000);
    }
    c::NumericDescriptor {
        state: c::Schema::Record(STATE),
        command: c::Schema::Record(COMMAND),
        context: c::Schema::Leaf(&CONTEXT),
        program: V2NumericProgram {
            inputs: INPUTS,
            outputs: OUTPUTS,
            nodes: NODE_DATA,
            roots: ROOTS,
        },
        bindings: BINDINGS,
        output_types: OUTPUTS,
        decision_output: 0,
        branches: BRANCHES,
        reasons: REASONS,
        channels: CHANNELS,
        laws: LAWS,
        required: REQUIRED,
        limits,
    }
}
fn original_schema() -> Schema {
    let limits = SchemaLimits::default();
    let definitions = DESCRIPTION
        .definitions
        .iter()
        .map(|d| {
            let kind = match d.kind {
                s::Kind::Bool => TypeKind::Bool,
                s::Kind::U128 { min, max } => TypeKind::U128 { min, max },
                s::Kind::Text { min, max } => TypeKind::Text {
                    min_len: min,
                    max_len: max,
                },
                s::Kind::Record(fields) => TypeKind::Record {
                    fields: fields
                        .iter()
                        .map(|f| {
                            ok(FieldDef::try_new(
                                FieldId::new(f.id),
                                ok(std::str::from_utf8(f.name)),
                                TypeId::new(f.type_id),
                            ))
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                },
                other => panic!("unsupported original fixture declaration: {other:?}"),
            };
            ok(TypeDef::try_new(
                TypeId::new(d.id),
                ok(std::str::from_utf8(d.name)),
                kind,
                limits,
            ))
        })
        .collect();
    ok(Schema::try_new(
        "OriginalZusd",
        1,
        TypeId::new(100),
        definitions,
        limits,
    ))
}
pub(crate) fn with_authority(
    run: impl FnOnce(&Authority<'_, c::NumericDescriptor<'_>>, &c::Framing),
) {
    with_descriptor(descriptor(), run);
}
pub(crate) fn with_descriptor(
    descriptor: c::NumericDescriptor<'_>,
    run: impl FnOnce(&Authority<'_, c::NumericDescriptor<'_>>, &c::Framing),
) {
    let schema = original_schema();
    let raw = ok(schema.canonical_bytes());
    let hash = ok(schema.schema_hash::<RustCryptoSha256>());
    let binding = |root| c::FrameBinding {
        root,
        schema: *hash.as_bytes(),
        max_bytes: 4096,
    };
    let framing = c::Framing {
        state: binding(100),
        command: binding(101),
        context: binding(102),
    };
    let Some(policy) = a::numeric_policy_bytes(&descriptor, &raw, &framing, CHANNEL_ROOTS) else {
        panic!("complete policy bytes");
    };
    let bound = ok(catalog::bind_original(
        &raw,
        &DESCRIPTION,
        catalog::Limits {
            schema: s::Limits {
                bytes: raw.len() as u64,
                types: 32,
                fields: 256,
                variants: 0,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &framing,
        CHANNEL_ROOTS,
    ));
    let authority = ok(a::bind(&bound));
    run(&authority, &framing);
}
pub(crate) fn numbers(binding: c::FrameBinding, first: u16, values: &[u128]) -> Vec<u8> {
    let fields = values
        .iter()
        .enumerate()
        .map(|(i, value)| Field::new(first + ok(u16::try_from(i)), Value::unsigned(*value)))
        .collect();
    envelope(binding, ok(Value::record_canonical(fields)))
}
pub(crate) fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}
pub(crate) fn commit(outcome: PublicationOutcome<'_>) -> Publication<'_> {
    match outcome {
        PublicationOutcome::Commit(p) => p,
        other => panic!("expected checked publication: {other:?}"),
    }
}
pub(crate) struct Case {
    pub(crate) inputs: [u128; 35],
    pub(crate) code: u32,
    pub(crate) post: [u128; 32],
    pub(crate) effects: Vec<u128>,
    pub(crate) event: &'static str,
}
pub(crate) fn cases() -> impl Iterator<Item = Case> {
    include_str!("native-cases.txt").lines().map(|line| {
        let parts: Vec<_> = line.split('|').collect();
        assert_eq!(parts.len(), 5);
        let numbers = |part: &str| {
            part.split_whitespace()
                .map(|x| ok(x.parse::<u128>()))
                .collect::<Vec<_>>()
        };
        Case {
            inputs: ok(numbers(parts[0]).try_into()),
            code: ok(parts[1].parse()),
            post: ok(numbers(parts[2]).try_into()),
            effects: numbers(parts[3]),
            event: parts[4],
        }
    })
}
pub(crate) fn effect_payload(event: &str, values: &[u128]) -> Vec<u8> {
    let mut fields = vec![Field::new(1000, ok(Value::text_ascii(event.to_owned())))];
    fields.extend(
        values
            .iter()
            .enumerate()
            .map(|(i, value)| Field::new(1001 + ok(u16::try_from(i)), Value::unsigned(*value))),
    );
    ok(ok(Value::record_canonical(fields)).canonical_bytes())
}
