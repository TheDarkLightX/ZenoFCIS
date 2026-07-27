//! Deterministic positive and negative codec vector construction.
//!
//! Vectors are fixed ZCVE/1 byte evidence. Positive and boundary vectors
//! encode canonical admitted values. Negative vectors exercise malformed input,
//! noncanonical ordering, unknown schema fields/variants, and trailing bytes.
//! Every vector binds a stable category, an expected decode outcome, and (when
//! the decode stage succeeds) an expected schema-validation outcome.

use std::fmt::Write as _;

use zeno_fcis_codec::{CanonicalEncode as _, Hash32};
use zeno_fcis_schema::{EnumVariantDef, Schema, TypeDef, TypeId, TypeKind};
use zeno_fcis_value::{Field, MapEntry, Value};

use crate::{CodegenError, VectorKind};

const TAG_RECORD: u8 = 0x09;
const TAG_MAP: u8 = 0x0c;
const UNKNOWN_VARIANT_ORDINAL: u16 = 999;
const UNKNOWN_FIELD_ID: u16 = 99;

/// The expected full-pipeline outcome for one vector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VectorExpect {
    /// Decodes and validates against the named type.
    Accept,
    /// Rejected at the decode stage.
    DecodeReject(DecodeRejection),
    /// Decodes but is rejected at schema validation.
    ValidateReject(ValidateRejection),
}

/// A subset of `DecodeError` kinds produced as vector evidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum DecodeRejection {
    TrailingBytes,
    NonCanonicalRecord,
    NonCanonicalMap,
    UnknownTag,
    UnexpectedEnd,
}

/// A subset of `ValueValidationError` kinds produced as vector evidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ValidateRejection {
    RecordShape,
    UnknownVariant,
}

/// One constructed vector before rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VectorCase {
    pub(crate) name: String,
    pub(crate) kind: VectorKind,
    pub(crate) bytes: Vec<u8>,
    pub(crate) expect: VectorExpect,
    pub(crate) validate_type: Option<u32>,
}

/// Builds the complete ordered vector set for one closed schema.
pub(crate) fn build(schema: &Schema) -> Result<Vec<VectorCase>, CodegenError> {
    let mut cases = Vec::new();
    let root = schema.root_type();

    let minimal = minimal_value(schema, root)?;
    let minimal_bytes = encode(&minimal)?;
    cases.push(VectorCase {
        name: "positive_root_minimal".to_owned(),
        kind: VectorKind::Positive,
        bytes: minimal_bytes.clone(),
        expect: VectorExpect::Accept,
        validate_type: Some(root.get()),
    });

    if let Some(maximal) = maximal_value(schema, root)? {
        let maximal_bytes = encode(&maximal)?;
        cases.push(VectorCase {
            name: "boundary_root_maximal".to_owned(),
            kind: VectorKind::Boundary,
            bytes: maximal_bytes,
            expect: VectorExpect::Accept,
            validate_type: Some(root.get()),
        });
    }

    push_malformed(&mut cases, &minimal_bytes);
    push_noncanonical(&mut cases, schema)?;
    push_unknown_field(&mut cases, schema, &minimal)?;
    push_unknown_variant(&mut cases, schema)?;
    push_trailing(&mut cases, &minimal_bytes);

    cases.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(cases)
}

fn push_malformed(cases: &mut Vec<VectorCase>, minimal_bytes: &[u8]) {
    if minimal_bytes.len() > 1 {
        let truncated = minimal_bytes[..minimal_bytes.len() - 1].to_vec();
        cases.push(VectorCase {
            name: "malformed_truncated".to_owned(),
            kind: VectorKind::Malformed,
            bytes: truncated,
            expect: VectorExpect::DecodeReject(DecodeRejection::UnexpectedEnd),
            validate_type: None,
        });
    }
    cases.push(VectorCase {
        name: "malformed_unknown_tag".to_owned(),
        kind: VectorKind::Malformed,
        bytes: vec![0xff],
        expect: VectorExpect::DecodeReject(DecodeRejection::UnknownTag),
        validate_type: None,
    });
}

fn push_noncanonical(cases: &mut Vec<VectorCase>, schema: &Schema) -> Result<(), CodegenError> {
    let amount = minimal_value(schema, TypeId::new(1)).unwrap_or(Value::U128(0));
    let signed = Value::I128(0);
    let record = encode_record_raw(&[(2, signed.clone()), (1, amount.clone())])?;
    cases.push(VectorCase {
        name: "noncanonical_record_order".to_owned(),
        kind: VectorKind::NonCanonical,
        bytes: record,
        expect: VectorExpect::DecodeReject(DecodeRejection::NonCanonicalRecord),
        validate_type: None,
    });

    let key_two = Value::U128(2);
    let key_one = Value::U128(1);
    let map = encode_map_raw(&[(key_two, Value::U128(0)), (key_one, Value::U128(0))])?;
    cases.push(VectorCase {
        name: "noncanonical_map_order".to_owned(),
        kind: VectorKind::NonCanonical,
        bytes: map,
        expect: VectorExpect::DecodeReject(DecodeRejection::NonCanonicalMap),
        validate_type: None,
    });
    Ok(())
}

fn push_unknown_field(
    cases: &mut Vec<VectorCase>,
    schema: &Schema,
    minimal: &Value,
) -> Result<(), CodegenError> {
    let Value::Record(fields) = minimal else {
        return Ok(());
    };
    let mut extended: Vec<Field> = fields.clone().into_vec();
    extended.push(Field::new(UNKNOWN_FIELD_ID, Value::U128(0)));
    let record = match Value::record_canonical(extended) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let bytes = encode(&record)?;
    cases.push(VectorCase {
        name: "unknown_field_extra".to_owned(),
        kind: VectorKind::UnknownField,
        bytes,
        expect: VectorExpect::ValidateReject(ValidateRejection::RecordShape),
        validate_type: Some(schema.root_type().get()),
    });
    Ok(())
}

fn push_unknown_variant(cases: &mut Vec<VectorCase>, schema: &Schema) -> Result<(), CodegenError> {
    let Some(enum_type) = first_enum_type(schema) else {
        return Ok(());
    };
    let value = Value::Enum {
        type_id: enum_type.id().get(),
        variant: UNKNOWN_VARIANT_ORDINAL,
    };
    let bytes = encode(&value)?;
    cases.push(VectorCase {
        name: "unknown_variant_enum".to_owned(),
        kind: VectorKind::UnknownVariant,
        bytes,
        expect: VectorExpect::ValidateReject(ValidateRejection::UnknownVariant),
        validate_type: Some(enum_type.id().get()),
    });
    Ok(())
}

fn push_trailing(cases: &mut Vec<VectorCase>, minimal_bytes: &[u8]) {
    let mut bytes = minimal_bytes.to_vec();
    bytes.push(0x00);
    cases.push(VectorCase {
        name: "trailing_bytes_extra".to_owned(),
        kind: VectorKind::TrailingBytes,
        bytes,
        expect: VectorExpect::DecodeReject(DecodeRejection::TrailingBytes),
        validate_type: None,
    });
}

fn first_enum_type(schema: &Schema) -> Option<&TypeDef> {
    schema
        .types()
        .iter()
        .find(|definition| matches!(definition.kind(), TypeKind::Enum { .. }))
}

fn minimal_value(schema: &Schema, type_id: TypeId) -> Result<Value, CodegenError> {
    let definition = schema
        .type_by_id(type_id)
        .ok_or(CodegenError::VectorConstruction)?;
    match definition.kind() {
        TypeKind::Unit => Ok(Value::Unit),
        TypeKind::Bool => Ok(Value::Bool(false)),
        TypeKind::U128 { min, .. } => Ok(Value::U128(*min)),
        TypeKind::I128 { min, .. } => Ok(Value::I128(*min)),
        TypeKind::Bytes { min_len, .. } => {
            let length = usize::try_from(*min_len).map_err(|_| CodegenError::VectorConstruction)?;
            Value::bytes(vec![0; length]).map_err(|_| CodegenError::VectorConstruction)
        }
        TypeKind::Text { min_len, .. } => Ok(text_minimal(*min_len)?),
        TypeKind::Enum { variants } => Ok(enum_minimal(definition.id().get(), variants)),
        TypeKind::Tuple { items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items.iter() {
                values.push(minimal_value(schema, *item)?);
            }
            Ok(Value::tuple(values))
        }
        TypeKind::Record { fields } => {
            let mut built = Vec::with_capacity(fields.len());
            for field in fields.iter() {
                let value = minimal_value(schema, field.type_id())?;
                built.push(Field::new(field.id().get(), value));
            }
            Value::record_canonical(built).map_err(|_| CodegenError::VectorConstruction)
        }
        TypeKind::Sum { variants } => sum_minimal(schema, definition.id().get(), variants),
        TypeKind::Vector {
            element, min_len, ..
        } => {
            let count = usize::try_from(*min_len).map_err(|_| CodegenError::VectorConstruction)?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(minimal_value(schema, *element)?);
            }
            Ok(Value::vector(items))
        }
        TypeKind::Map {
            key,
            value,
            min_len,
            ..
        } => map_minimal(schema, *key, *value, *min_len),
    }
}

fn maximal_value(schema: &Schema, type_id: TypeId) -> Result<Option<Value>, CodegenError> {
    let definition = schema
        .type_by_id(type_id)
        .ok_or(CodegenError::VectorConstruction)?;
    match definition.kind() {
        TypeKind::Unit => Ok(Some(Value::Unit)),
        TypeKind::Bool => Ok(Some(Value::Bool(true))),
        TypeKind::U128 { max, .. } => Ok(Some(Value::U128(*max))),
        TypeKind::I128 { max, .. } => Ok(Some(Value::I128(*max))),
        TypeKind::Bytes { max_len, .. } => {
            let length = usize::try_from(*max_len).map_err(|_| CodegenError::VectorConstruction)?;
            Value::bytes(vec![0; length])
                .map(Some)
                .map_err(|_| CodegenError::VectorConstruction)
        }
        TypeKind::Text { max_len, .. } => Ok(Some(text_minimal(*max_len)?)),
        TypeKind::Enum { variants } => Ok(Some(enum_maximal(definition.id().get(), variants))),
        TypeKind::Tuple { items } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items.iter() {
                match maximal_value(schema, *item)? {
                    Some(value) => values.push(value),
                    None => return Ok(None),
                }
            }
            Ok(Some(Value::tuple(values)))
        }
        TypeKind::Record { fields } => {
            let mut built = Vec::with_capacity(fields.len());
            for field in fields.iter() {
                match maximal_value(schema, field.type_id())? {
                    Some(value) => built.push(Field::new(field.id().get(), value)),
                    None => return Ok(None),
                }
            }
            Value::record_canonical(built)
                .map(Some)
                .map_err(|_| CodegenError::VectorConstruction)
        }
        TypeKind::Sum { variants } => sum_maximal(schema, definition.id().get(), variants),
        TypeKind::Vector {
            element, max_len, ..
        } => {
            let count = usize::try_from(*max_len).map_err(|_| CodegenError::VectorConstruction)?;
            let element = match maximal_value(schema, *element)? {
                Some(value) => value,
                None => minimal_value(schema, *element)?,
            };
            let items = vec![element; count];
            Ok(Some(Value::vector(items)))
        }
        TypeKind::Map {
            key,
            value,
            max_len,
            ..
        } => map_maximal(schema, *key, *value, *max_len),
    }
}

fn text_minimal(len: u32) -> Result<Value, CodegenError> {
    let count = usize::try_from(len).map_err(|_| CodegenError::VectorConstruction)?;
    let text = "a".repeat(count);
    Value::text_ascii(text).map_err(|_| CodegenError::VectorConstruction)
}

fn enum_minimal(type_id: u32, variants: &[EnumVariantDef]) -> Value {
    let variant = variants.first().map(|item| item.id().get()).unwrap_or(0);
    Value::Enum { type_id, variant }
}

fn enum_maximal(type_id: u32, variants: &[EnumVariantDef]) -> Value {
    let variant = variants.last().map(|item| item.id().get()).unwrap_or(0);
    Value::Enum { type_id, variant }
}

fn sum_minimal(
    schema: &Schema,
    type_id: u32,
    variants: &[zeno_fcis_schema::SumVariantDef],
) -> Result<Value, CodegenError> {
    let Some(first) = variants.first() else {
        return Err(CodegenError::VectorConstruction);
    };
    let payload = match first.payload() {
        None => None,
        Some(child_type) => Some(Box::new(minimal_value(schema, child_type)?)),
    };
    Ok(Value::Sum {
        type_id,
        variant: first.id().get(),
        payload,
    })
}

fn sum_maximal(
    schema: &Schema,
    type_id: u32,
    variants: &[zeno_fcis_schema::SumVariantDef],
) -> Result<Option<Value>, CodegenError> {
    let Some(last) = variants.last() else {
        return Err(CodegenError::VectorConstruction);
    };
    let payload = match last.payload() {
        None => None,
        Some(child_type) => match maximal_value(schema, child_type)? {
            Some(value) => Some(Box::new(value)),
            None => Some(Box::new(minimal_value(schema, child_type)?)),
        },
    };
    Ok(Some(Value::Sum {
        type_id,
        variant: last.id().get(),
        payload,
    }))
}

fn map_minimal(
    schema: &Schema,
    key: TypeId,
    value: TypeId,
    min_len: u32,
) -> Result<Value, CodegenError> {
    let keys = distinct_keys(schema, key, min_len)?;
    let mut entries = Vec::with_capacity(keys.len());
    for key_value in keys {
        let value_value = minimal_value(schema, value)?;
        entries.push(
            MapEntry::try_new(key_value, value_value)
                .map_err(|_| CodegenError::VectorConstruction)?,
        );
    }
    Value::normalize_map(entries).map_err(|_| CodegenError::VectorConstruction)
}

fn map_maximal(
    schema: &Schema,
    key: TypeId,
    value: TypeId,
    max_len: u32,
) -> Result<Option<Value>, CodegenError> {
    let keys = distinct_keys(schema, key, max_len)?;
    let mut entries = Vec::with_capacity(keys.len());
    for key_value in keys {
        let value_value = match maximal_value(schema, value)? {
            Some(v) => v,
            None => minimal_value(schema, value)?,
        };
        entries.push(
            MapEntry::try_new(key_value, value_value)
                .map_err(|_| CodegenError::VectorConstruction)?,
        );
    }
    Value::normalize_map(entries)
        .map(Some)
        .map_err(|_| CodegenError::VectorConstruction)
}

fn distinct_keys(schema: &Schema, key: TypeId, count: u32) -> Result<Vec<Value>, CodegenError> {
    let definition = schema
        .type_by_id(key)
        .ok_or(CodegenError::VectorConstruction)?;
    let count = usize::try_from(count).map_err(|_| CodegenError::VectorConstruction)?;
    let mut keys = Vec::with_capacity(count);
    match definition.kind() {
        TypeKind::U128 { min, .. } => {
            for offset in 0..u128::try_from(count).map_err(|_| CodegenError::VectorConstruction)? {
                keys.push(Value::U128(min + offset));
            }
        }
        TypeKind::I128 { min, .. } => {
            for offset in 0..i128::try_from(count).map_err(|_| CodegenError::VectorConstruction)? {
                keys.push(Value::I128(min + offset));
            }
        }
        TypeKind::Bool => {
            keys.push(Value::Bool(false));
            if count >= 2 {
                keys.push(Value::Bool(true));
            }
        }
        TypeKind::Enum { variants } => {
            for variant in variants.iter().take(count) {
                keys.push(Value::Enum {
                    type_id: definition.id().get(),
                    variant: variant.id().get(),
                });
            }
        }
        TypeKind::Text { .. } => {
            for offset in 0..u32::try_from(count).map_err(|_| CodegenError::VectorConstruction)? {
                let label = char::from(
                    b'a' + u8::try_from(offset).map_err(|_| CodegenError::VectorConstruction)?,
                )
                .to_string();
                keys.push(Value::text_ascii(label).map_err(|_| CodegenError::VectorConstruction)?);
            }
        }
        TypeKind::Bytes { .. } => {
            for offset in 0..u32::try_from(count).map_err(|_| CodegenError::VectorConstruction)? {
                keys.push(
                    Value::bytes(vec![
                        u8::try_from(offset).map_err(|_| CodegenError::VectorConstruction)?,
                    ])
                    .map_err(|_| CodegenError::VectorConstruction)?,
                );
            }
        }
        TypeKind::Unit => {
            if count >= 1 {
                keys.push(Value::Unit);
            }
        }
        _ => return Err(CodegenError::VectorConstruction),
    }
    Ok(keys)
}

fn encode(value: &Value) -> Result<Vec<u8>, CodegenError> {
    value
        .canonical_bytes()
        .map_err(|_| CodegenError::VectorConstruction)
}

fn encode_record_raw(pairs: &[(u16, Value)]) -> Result<Vec<u8>, CodegenError> {
    let mut output = Vec::new();
    output.push(TAG_RECORD);
    put_u32(
        &mut output,
        u32::try_from(pairs.len()).map_err(|_| CodegenError::VectorConstruction)?,
    );
    for (id, value) in pairs {
        output.extend_from_slice(&id.to_be_bytes());
        output.extend_from_slice(&encode(value)?);
    }
    Ok(output)
}

fn encode_map_raw(entries: &[(Value, Value)]) -> Result<Vec<u8>, CodegenError> {
    let mut output = Vec::new();
    output.push(TAG_MAP);
    put_u32(
        &mut output,
        u32::try_from(entries.len()).map_err(|_| CodegenError::VectorConstruction)?,
    );
    for (key, value) in entries {
        let encoded_key = encode(key)?;
        put_blob(&mut output, &encoded_key)?;
        let encoded_value = encode(value)?;
        put_blob(&mut output, &encoded_value)?;
    }
    Ok(output)
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), CodegenError> {
    let length = u32::try_from(bytes.len()).map_err(|_| CodegenError::VectorConstruction)?;
    put_u32(output, length);
    output.extend_from_slice(bytes);
    Ok(())
}

/// Computes the commitment over the complete ordered vector set.
pub(crate) fn set_hash<E>(
    _domain_name: &'static str,
    cases: &[VectorCase],
    hash: impl Fn(&[u8]) -> Result<Hash32, E>,
) -> Result<Hash32, E> {
    let mut preimage = Vec::new();
    for case in cases {
        preimage.extend_from_slice(case.name.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(case.kind.label().as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(&case.bytes);
        preimage.push(0);
    }
    hash(&preimage)
}

/// Renders the Rust vector evidence block (enums, case struct, VECTORS static).
pub(crate) fn render_rust(cases: &[VectorCase]) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum VectorKind {{");
    let _ = writeln!(
        output,
        "    Positive, Boundary, Malformed, NonCanonical, UnknownField, UnknownVariant, TrailingBytes,"
    );
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum DecodeRejection {{");
    let _ = writeln!(
        output,
        "    TrailingBytes, NonCanonicalRecord, NonCanonicalMap, UnknownTag, UnexpectedEnd,"
    );
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum ValidateRejection {{");
    let _ = writeln!(output, "    RecordShape, UnknownVariant,");
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum VectorExpect {{");
    let _ = writeln!(output, "    Accept,");
    let _ = writeln!(output, "    DecodeReject(DecodeRejection),");
    let _ = writeln!(output, "    ValidateReject(ValidateRejection),");
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct VectorCase {{");
    let _ = writeln!(output, "    pub name: &'static str,");
    let _ = writeln!(output, "    pub kind: VectorKind,");
    let _ = writeln!(output, "    pub bytes: &'static [u8],");
    let _ = writeln!(output, "    pub expect: VectorExpect,");
    let _ = writeln!(output, "    pub validate_type: Option<u32>,");
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "pub static VECTORS: &[VectorCase] = &[");
    for case in cases {
        let kind = rust_kind(case.kind);
        let expect = rust_expect(&case.expect);
        let bytes = rust_bytes(&case.bytes);
        let validate = match case.validate_type {
            Some(id) => format!("Some({id})"),
            None => "None".to_owned(),
        };
        let _ = writeln!(
            output,
            "    VectorCase {{ name: \"{}\", kind: VectorKind::{}, bytes: {}, expect: {}, validate_type: {} }},",
            case.name, kind, bytes, expect, validate
        );
    }
    let _ = writeln!(output, "];\n");
    output
}

fn rust_kind(kind: VectorKind) -> &'static str {
    match kind {
        VectorKind::Positive => "Positive",
        VectorKind::Boundary => "Boundary",
        VectorKind::Malformed => "Malformed",
        VectorKind::NonCanonical => "NonCanonical",
        VectorKind::UnknownField => "UnknownField",
        VectorKind::UnknownVariant => "UnknownVariant",
        VectorKind::TrailingBytes => "TrailingBytes",
    }
}

fn rust_expect(expect: &VectorExpect) -> String {
    match expect {
        VectorExpect::Accept => "VectorExpect::Accept".to_owned(),
        VectorExpect::DecodeReject(rejection) => {
            let label = match rejection {
                DecodeRejection::TrailingBytes => "TrailingBytes",
                DecodeRejection::NonCanonicalRecord => "NonCanonicalRecord",
                DecodeRejection::NonCanonicalMap => "NonCanonicalMap",
                DecodeRejection::UnknownTag => "UnknownTag",
                DecodeRejection::UnexpectedEnd => "UnexpectedEnd",
            };
            format!("VectorExpect::DecodeReject(DecodeRejection::{label})")
        }
        VectorExpect::ValidateReject(rejection) => {
            let label = match rejection {
                ValidateRejection::RecordShape => "RecordShape",
                ValidateRejection::UnknownVariant => "UnknownVariant",
            };
            format!("VectorExpect::ValidateReject(ValidateRejection::{label})")
        }
    }
}

fn rust_bytes(bytes: &[u8]) -> String {
    let mut output = String::from("&[");
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let _ = write!(output, "0x{byte:02x}");
    }
    output.push(']');
    output
}

/// Renders the Python vector evidence block (VECTORS list with byte arrays).
pub(crate) fn render_python(cases: &[VectorCase]) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "VECTORS = [");
    for case in cases {
        let kind = case.kind.label();
        let bytes = python_bytes(&case.bytes);
        let (expect_kind, expect_label) = python_expect(&case.expect);
        let validate = match case.validate_type {
            Some(id) => id.to_string(),
            None => "None".to_owned(),
        };
        let _ = writeln!(
            output,
            "    VectorCase(name=\"{}\", kind=\"{}\", bytes={}, expect_kind=\"{}\", expect_label=\"{}\", validate_type={}),",
            case.name, kind, bytes, expect_kind, expect_label, validate
        );
    }
    let _ = writeln!(output, "]\n");
    output
}

fn python_expect(expect: &VectorExpect) -> (&'static str, &'static str) {
    match expect {
        VectorExpect::Accept => ("accept", ""),
        VectorExpect::DecodeReject(rejection) => {
            let label = match rejection {
                DecodeRejection::TrailingBytes => "trailing_bytes",
                DecodeRejection::NonCanonicalRecord => "non_canonical_record",
                DecodeRejection::NonCanonicalMap => "non_canonical_map",
                DecodeRejection::UnknownTag => "unknown_tag",
                DecodeRejection::UnexpectedEnd => "unexpected_end",
            };
            ("decode_reject", label)
        }
        VectorExpect::ValidateReject(rejection) => {
            let label = match rejection {
                ValidateRejection::RecordShape => "record_shape",
                ValidateRejection::UnknownVariant => "unknown_variant",
            };
            ("validate_reject", label)
        }
    }
}

fn python_bytes(bytes: &[u8]) -> String {
    let mut output = String::from("bytes([");
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let _ = write!(output, "0x{byte:02x}");
    }
    output.push_str("])");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_codec::{CommitmentHasher, decode_value};

    struct XorHasher;
    impl CommitmentHasher for XorHasher {
        const ALGORITHM_ID: &'static str = "test/xor";
        fn hash(bytes: &[u8]) -> Hash32 {
            let mut out = [0_u8; 32];
            for (i, b) in bytes.iter().enumerate() {
                out[i % 32] ^= b;
            }
            Hash32::new(out)
        }
    }

    fn fixture_schema() -> Schema {
        crate::fixture_schema().unwrap_or_else(|e| panic!("fixture schema failed: {e}"))
    }

    #[test]
    fn hardcoded_record_tag_matches_codec() {
        let value = Value::record_canonical(vec![Field::new(1, Value::U128(0))]);
        let value = match value {
            Ok(v) => v,
            Err(e) => panic!("record rejected: {e}"),
        };
        let bytes = match value.canonical_bytes() {
            Ok(b) => b,
            Err(e) => panic!("encode failed: {e}"),
        };
        assert_eq!(bytes[0], TAG_RECORD);
    }

    #[test]
    fn positive_vector_round_trips() {
        let schema = fixture_schema();
        let cases = build(&schema).unwrap_or_else(|e| panic!("build failed: {e}"));
        let positive = cases
            .iter()
            .find(|c| matches!(c.kind, VectorKind::Positive))
            .unwrap_or_else(|| panic!("missing positive vector"));
        let decoded = decode_value(&positive.bytes, zeno_fcis_codec::DecodeLimits::default());
        assert!(decoded.is_ok(), "positive vector must decode");
    }

    #[test]
    fn vector_set_is_deterministic() {
        let schema = fixture_schema();
        let left = build(&schema).unwrap_or_else(|e| panic!("build failed: {e}"));
        let right = build(&schema).unwrap_or_else(|e| panic!("build failed: {e}"));
        let left_hash = set_hash("zeno-fcis/vectors", &left, |bytes| {
            Ok::<_, core::convert::Infallible>(XorHasher::hash(bytes))
        });
        let right_hash = set_hash("zeno-fcis/vectors", &right, |bytes| {
            Ok::<_, core::convert::Infallible>(XorHasher::hash(bytes))
        });
        assert_eq!(left_hash, right_hash);
        assert_eq!(left, right);
    }
}
