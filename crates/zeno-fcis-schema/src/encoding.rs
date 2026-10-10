//! Canonical schema encoding and content commitments.

extern crate alloc;

use alloc::vec::Vec;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

use crate::{EnumVariantDef, FieldDef, Schema, SchemaError, SumVariantDef, TypeDef, TypeKind};

const SCHEMA_MAGIC: &[u8; 13] = b"ZFCISSCHEMA1\0";
const KIND_UNIT: u8 = 0;
const KIND_BOOL: u8 = 1;
const KIND_U128: u8 = 2;
const KIND_I128: u8 = 3;
const KIND_BYTES: u8 = 4;
const KIND_TEXT: u8 = 5;
const KIND_ENUM: u8 = 6;
const KIND_TUPLE: u8 = 7;
const KIND_RECORD: u8 = 8;
const KIND_SUM: u8 = 9;
const KIND_VECTOR: u8 = 10;
const KIND_MAP: u8 = 11;

impl CanonicalEncode for Schema {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(SCHEMA_MAGIC);
        put_text(output, self.profile().as_str())?;
        output.extend_from_slice(&self.version().to_be_bytes());
        output.extend_from_slice(&self.root_type().get().to_be_bytes());
        put_length(output, self.types().len())?;
        for item in self.types() {
            encode_type(output, item)?;
        }
        Ok(())
    }
}

impl Schema {
    /// Computes the schema commitment under the fixed ZenoFCIS schema domain.
    pub fn schema_hash<H: CommitmentHasher>(&self) -> Result<Hash32, SchemaError> {
        let bytes = self.canonical_bytes().map_err(|_| SchemaError::Encoding)?;
        let domain = Domain::new("zeno-fcis/schema", 1).map_err(|_| SchemaError::Encoding)?;
        commitment::<H>(domain, &bytes).map_err(|_| SchemaError::Encoding)
    }
}

fn encode_type(output: &mut Vec<u8>, item: &TypeDef) -> Result<(), EncodeError> {
    output.extend_from_slice(&item.id().get().to_be_bytes());
    put_text(output, item.name().as_str())?;
    match item.kind() {
        TypeKind::Unit => output.push(KIND_UNIT),
        TypeKind::Bool => output.push(KIND_BOOL),
        TypeKind::U128 { min, max } => {
            output.push(KIND_U128);
            output.extend_from_slice(&min.to_be_bytes());
            output.extend_from_slice(&max.to_be_bytes());
        }
        TypeKind::I128 { min, max } => {
            output.push(KIND_I128);
            output.extend_from_slice(&min.to_be_bytes());
            output.extend_from_slice(&max.to_be_bytes());
        }
        TypeKind::Bytes { min_len, max_len } => {
            output.push(KIND_BYTES);
            output.extend_from_slice(&min_len.to_be_bytes());
            output.extend_from_slice(&max_len.to_be_bytes());
        }
        TypeKind::Text { min_len, max_len } => {
            output.push(KIND_TEXT);
            output.extend_from_slice(&min_len.to_be_bytes());
            output.extend_from_slice(&max_len.to_be_bytes());
        }
        TypeKind::Enum { variants } => {
            output.push(KIND_ENUM);
            put_length(output, variants.len())?;
            for variant in variants {
                encode_enum_variant(output, variant)?;
            }
        }
        TypeKind::Tuple { items } => {
            output.push(KIND_TUPLE);
            put_length(output, items.len())?;
            for child in items {
                output.extend_from_slice(&child.get().to_be_bytes());
            }
        }
        TypeKind::Record { fields } => {
            output.push(KIND_RECORD);
            put_length(output, fields.len())?;
            for field in fields {
                encode_field(output, field)?;
            }
        }
        TypeKind::Sum { variants } => {
            output.push(KIND_SUM);
            put_length(output, variants.len())?;
            for variant in variants {
                encode_sum_variant(output, variant)?;
            }
        }
        TypeKind::Vector {
            element,
            min_len,
            max_len,
        } => {
            output.push(KIND_VECTOR);
            output.extend_from_slice(&element.get().to_be_bytes());
            output.extend_from_slice(&min_len.to_be_bytes());
            output.extend_from_slice(&max_len.to_be_bytes());
        }
        TypeKind::Map {
            key,
            value,
            min_len,
            max_len,
        } => {
            output.push(KIND_MAP);
            output.extend_from_slice(&key.get().to_be_bytes());
            output.extend_from_slice(&value.get().to_be_bytes());
            output.extend_from_slice(&min_len.to_be_bytes());
            output.extend_from_slice(&max_len.to_be_bytes());
        }
    }
    Ok(())
}

fn encode_field(output: &mut Vec<u8>, field: &FieldDef) -> Result<(), EncodeError> {
    output.extend_from_slice(&field.id().get().to_be_bytes());
    put_text(output, field.name().as_str())?;
    output.extend_from_slice(&field.type_id().get().to_be_bytes());
    Ok(())
}

fn encode_enum_variant(output: &mut Vec<u8>, variant: &EnumVariantDef) -> Result<(), EncodeError> {
    output.extend_from_slice(&variant.id().get().to_be_bytes());
    put_text(output, variant.name().as_str())
}

fn encode_sum_variant(output: &mut Vec<u8>, variant: &SumVariantDef) -> Result<(), EncodeError> {
    output.extend_from_slice(&variant.id().get().to_be_bytes());
    put_text(output, variant.name().as_str())?;
    match variant.payload() {
        None => output.push(0),
        Some(payload) => {
            output.push(1);
            output.extend_from_slice(&payload.get().to_be_bytes());
        }
    }
    Ok(())
}

fn put_text(output: &mut Vec<u8>, value: &str) -> Result<(), EncodeError> {
    let length = u16::try_from(value.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}
