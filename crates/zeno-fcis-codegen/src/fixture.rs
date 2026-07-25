//! Canonical fixture schema and generation spec used by the compiled fixture
//! crate and by codegen self-tests.
//!
//! The fixture exercises every closed-schema type kind: unit, bool, bounded
//! integers, bytes, text, enum, tuple, record, sum, bounded vector, and bounded
//! map. It is evidence infrastructure, not a promoted profile.

use zeno_fcis_schema::{
    EnumVariantDef, FieldDef, FieldId, Schema, SchemaLimits, SumVariantDef, TypeDef, TypeId,
    TypeKind, VariantId,
};

use crate::{CodegenError, GenerationSpec};

/// Returns the canonical codegen fixture schema.
///
/// Construction is deterministic and panics only on a programmer error in the
/// fixture itself (surfaced as a `CodegenError`).
pub fn fixture_schema() -> Result<Schema, CodegenError> {
    let limits = SchemaLimits::default();
    let amount = type_def(
        1,
        "Amount",
        TypeKind::U128 {
            min: 0,
            max: 1_000_000,
        },
        limits,
    )?;
    let signed = type_def(
        2,
        "Signed",
        TypeKind::I128 {
            min: -1_000,
            max: 1_000,
        },
        limits,
    )?;
    let label = type_def(
        3,
        "Label",
        TypeKind::Text {
            min_len: 1,
            max_len: 16,
        },
        limits,
    )?;
    let blob = type_def(
        4,
        "Blob",
        TypeKind::Bytes {
            min_len: 0,
            max_len: 32,
        },
        limits,
    )?;
    let flag = type_def(5, "Flag", TypeKind::Bool, limits)?;
    let nil = type_def(6, "Nil", TypeKind::Unit, limits)?;
    let tag = type_def(
        7,
        "Tag",
        TypeKind::Enum {
            variants: vec![
                EnumVariantDef::try_new(VariantId::new(1), "Idle")?,
                EnumVariantDef::try_new(VariantId::new(2), "Active")?,
            ]
            .into_boxed_slice(),
        },
        limits,
    )?;
    let point = type_def(
        8,
        "Point",
        TypeKind::Tuple {
            items: vec![TypeId::new(1), TypeId::new(7)].into_boxed_slice(),
        },
        limits,
    )?;
    let event = type_def(
        9,
        "Event",
        TypeKind::Sum {
            variants: vec![
                SumVariantDef::try_new(VariantId::new(1), "Stop", None)?,
                SumVariantDef::try_new(VariantId::new(2), "Move", Some(TypeId::new(1)))?,
            ]
            .into_boxed_slice(),
        },
        limits,
    )?;
    let labels = type_def(
        10,
        "Labels",
        TypeKind::Vector {
            element: TypeId::new(3),
            min_len: 0,
            max_len: 4,
        },
        limits,
    )?;
    let scores = type_def(
        11,
        "Scores",
        TypeKind::Map {
            key: TypeId::new(1),
            value: TypeId::new(1),
            min_len: 0,
            max_len: 4,
        },
        limits,
    )?;
    let balance_state = type_def(
        12,
        "BalanceState",
        TypeKind::Record {
            fields: vec![
                FieldDef::try_new(FieldId::new(1), "amount", TypeId::new(1))?,
                FieldDef::try_new(FieldId::new(2), "signed", TypeId::new(2))?,
                FieldDef::try_new(FieldId::new(3), "label", TypeId::new(3))?,
                FieldDef::try_new(FieldId::new(4), "blob", TypeId::new(4))?,
                FieldDef::try_new(FieldId::new(5), "flag", TypeId::new(5))?,
                FieldDef::try_new(FieldId::new(6), "nil", TypeId::new(6))?,
                FieldDef::try_new(FieldId::new(7), "tag", TypeId::new(7))?,
                FieldDef::try_new(FieldId::new(8), "point", TypeId::new(8))?,
                FieldDef::try_new(FieldId::new(9), "event", TypeId::new(9))?,
                FieldDef::try_new(FieldId::new(10), "labels", TypeId::new(10))?,
                FieldDef::try_new(FieldId::new(11), "scores", TypeId::new(11))?,
            ]
            .into_boxed_slice(),
        },
        limits,
    )?;
    Schema::try_new(
        "CodegenFixture",
        1,
        TypeId::new(12),
        vec![
            amount,
            signed,
            label,
            blob,
            flag,
            nil,
            tag,
            point,
            event,
            labels,
            scores,
            balance_state,
        ],
        limits,
    )
    .map_err(|_| CodegenError::SchemaEncoding)
}

/// Returns the canonical codegen fixture generation spec.
pub fn fixture_spec() -> Result<GenerationSpec, CodegenError> {
    GenerationSpec::try_new("codegen_fixture", "codegen_fixture")
}

fn type_def(
    id: u32,
    name: &str,
    kind: TypeKind,
    limits: SchemaLimits,
) -> Result<TypeDef, CodegenError> {
    TypeDef::try_new(TypeId::new(id), name, kind, limits).map_err(|_| CodegenError::SchemaEncoding)
}

impl From<zeno_fcis_schema::SchemaError> for CodegenError {
    fn from(_: zeno_fcis_schema::SchemaError) -> Self {
        CodegenError::SchemaEncoding
    }
}
