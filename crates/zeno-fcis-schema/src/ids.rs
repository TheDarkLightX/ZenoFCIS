//! Stable schema identifiers and validated names.

extern crate alloc;

use alloc::boxed::Box;
use core::fmt;

use crate::SchemaError;

/// Stable numeric type identifier inside one closed schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeId(u32);

impl TypeId {
    /// Constructs a type identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable numeric field identifier inside one record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldId(u16);

impl FieldId {
    /// Constructs a field identifier.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for FieldId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable numeric variant identifier inside one enum or sum.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VariantId(u16);

impl VariantId {
    /// Constructs a variant identifier.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for VariantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A bounded ASCII identifier used in generated source and evidence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaName(Box<str>);

impl SchemaName {
    /// Maximum UTF-8 bytes in one name.
    pub const MAX_BYTES: usize = 96;

    /// Validates and owns an ASCII identifier.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, SchemaError> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > Self::MAX_BYTES {
            return Err(SchemaError::InvalidName);
        }
        let Some(first) = bytes.first().copied() else {
            return Err(SchemaError::InvalidName);
        };
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return Err(SchemaError::InvalidName);
        }
        if bytes
            .iter()
            .copied()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
        {
            return Err(SchemaError::InvalidName);
        }
        Ok(Self(value))
    }

    /// Returns the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SchemaName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
