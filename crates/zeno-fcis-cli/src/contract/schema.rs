//! The original schema bytes, `v2/schema.zcve`.
//!
//! The canonical encoding of a schema description is unique: the library's
//! `canonical_v2::schema::encoding_matches` accepts exactly one byte string
//! for each description, and catalog binding runs that check on these bytes
//! before any file is written. They equal what
//! `zeno_fcis_bootstrap::lower_schema` produces for the same declarations,
//! as every template's retained schema shows.

use super::ContractError;
use super::declarations::{Declarations, Form, Leaf, ROOTS};

const MAGIC: &[u8; 13] = b"ZFCISSCHEMA1\0";
/// Schema format version of every contract.
const VERSION: u16 = 1;
const BOOL: u8 = 1;
const I128: u8 = 3;
const TEXT: u8 = 5;
const RECORD: u8 = 8;
const SUM: u8 = 9;
/// A sum variant without a payload.
const NO_PAYLOAD: u8 = 0;

/// Every declared type, in ID order, with its name and form.
pub(super) fn encode(declarations: &Declarations) -> Result<Vec<u8>, ContractError> {
    let mut bytes = MAGIC.to_vec();
    text(&mut bytes, &declarations.profile)?;
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&ROOTS[0].1.to_be_bytes());
    length(&mut bytes, declarations.types.len())?;
    for (id, declared) in &declarations.types {
        bytes.extend_from_slice(&id.to_be_bytes());
        text(&mut bytes, &declared.name)?;
        match &declared.form {
            Form::Leaf(Leaf::Bool) => bytes.push(BOOL),
            Form::Leaf(Leaf::I128 { min, max }) => {
                bytes.push(I128);
                bytes.extend_from_slice(&min.to_be_bytes());
                bytes.extend_from_slice(&max.to_be_bytes());
            }
            Form::Leaf(Leaf::Text { min, max }) => {
                bytes.push(TEXT);
                bytes.extend_from_slice(&min.to_be_bytes());
                bytes.extend_from_slice(&max.to_be_bytes());
            }
            Form::Record(fields) => {
                bytes.push(RECORD);
                length(&mut bytes, fields.len())?;
                for field in fields {
                    bytes.extend_from_slice(&field.id.to_be_bytes());
                    text(&mut bytes, &field.name)?;
                    bytes.extend_from_slice(&field.type_id.to_be_bytes());
                }
            }
            Form::Sum(variants) => {
                bytes.push(SUM);
                length(&mut bytes, variants.len())?;
                for variant in variants {
                    bytes.extend_from_slice(&variant.id.to_be_bytes());
                    text(&mut bytes, &variant.name)?;
                    bytes.push(NO_PAYLOAD);
                }
            }
        }
    }
    Ok(bytes)
}

/// A name with its 16-bit length.
fn text(bytes: &mut Vec<u8>, value: &str) -> Result<(), ContractError> {
    let size = u16::try_from(value.len())
        .map_err(|_| ContractError::new("project.zeno", format!("name `{value}` is too long")))?;
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

/// A count with its 32-bit width.
fn length(bytes: &mut Vec<u8>, count: usize) -> Result<(), ContractError> {
    let count = u32::try_from(count)
        .map_err(|_| ContractError::new("project.zeno", "too many declarations"))?;
    bytes.extend_from_slice(&count.to_be_bytes());
    Ok(())
}
