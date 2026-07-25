//! Schema construction and value-validation errors.

use core::fmt;

use crate::{FieldId, TypeId, VariantId};

/// Closed-schema construction or encoding failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaError {
    /// A name was empty, too long, non-ASCII, or not an identifier.
    InvalidName,
    /// The schema contained no type definitions.
    EmptySchema,
    /// A configured schema-size limit was exceeded.
    LimitExceeded,
    /// Two types used the same stable numeric identifier.
    DuplicateTypeId(TypeId),
    /// Two types used the same stable name.
    DuplicateTypeName,
    /// Two fields in one record used the same identifier.
    DuplicateFieldId(FieldId),
    /// Two fields in one record used the same name.
    DuplicateFieldName,
    /// Two variants in one enum or sum used the same identifier.
    DuplicateVariantId(VariantId),
    /// Two variants in one enum or sum used the same name.
    DuplicateVariantName,
    /// A range had a lower bound greater than its upper bound.
    InvalidRange,
    /// The declared root type does not exist.
    UnknownRootType(TypeId),
    /// A type definition references an unknown type.
    UnknownTypeReference(TypeId),
    /// The closed schema contains a recursive type cycle.
    RecursiveTypeCycle(TypeId),
    /// Canonical schema encoding failed.
    Encoding,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => formatter.write_str("invalid schema identifier"),
            Self::EmptySchema => formatter.write_str("schema must define at least one type"),
            Self::LimitExceeded => formatter.write_str("schema limit exceeded"),
            Self::DuplicateTypeId(id) => write!(formatter, "duplicate type identifier {id}"),
            Self::DuplicateTypeName => formatter.write_str("duplicate type name"),
            Self::DuplicateFieldId(id) => write!(formatter, "duplicate field identifier {id}"),
            Self::DuplicateFieldName => formatter.write_str("duplicate field name"),
            Self::DuplicateVariantId(id) => write!(formatter, "duplicate variant identifier {id}"),
            Self::DuplicateVariantName => formatter.write_str("duplicate variant name"),
            Self::InvalidRange => formatter.write_str("invalid schema range"),
            Self::UnknownRootType(id) => write!(formatter, "unknown root type {id}"),
            Self::UnknownTypeReference(id) => write!(formatter, "unknown referenced type {id}"),
            Self::RecursiveTypeCycle(id) => write!(formatter, "recursive type cycle at {id}"),
            Self::Encoding => formatter.write_str("canonical schema encoding failed"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SchemaError {}

/// Failure while checking a closed value against a schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueValidationError {
    /// The requested schema type does not exist.
    UnknownType(TypeId),
    /// The runtime value kind does not match the schema kind.
    TypeMismatch,
    /// A bounded integer lies outside the declared interval.
    IntegerRange,
    /// A byte, text, vector, record, tuple, or map length is invalid.
    Length,
    /// A value-level type identifier differs from the schema type.
    TypeIdentity,
    /// An enum or sum variant is not declared by the schema.
    UnknownVariant(VariantId),
    /// A unit variant unexpectedly carried a payload.
    UnexpectedPayload,
    /// A payload-bearing variant omitted its payload.
    MissingPayload,
    /// A record's field identifiers differ from the declared field set.
    RecordShape,
    /// A deterministic validation budget was exhausted.
    BudgetExceeded,
}

impl fmt::Display for ValueValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(id) => write!(formatter, "unknown schema type {id}"),
            Self::TypeMismatch => formatter.write_str("value kind does not match schema"),
            Self::IntegerRange => formatter.write_str("integer lies outside schema range"),
            Self::Length => formatter.write_str("value length lies outside schema bounds"),
            Self::TypeIdentity => formatter.write_str("value type identifier differs from schema"),
            Self::UnknownVariant(id) => write!(formatter, "unknown variant {id}"),
            Self::UnexpectedPayload => formatter.write_str("unit variant carried a payload"),
            Self::MissingPayload => formatter.write_str("payload variant omitted its payload"),
            Self::RecordShape => formatter.write_str("record fields differ from schema"),
            Self::BudgetExceeded => formatter.write_str("value-validation budget exhausted"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ValueValidationError {}
