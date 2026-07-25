//! Public generation request and output values.

use std::fmt;

use zeno_fcis_codec::Hash32;

/// Stable renderer/formatter semantic identity.
///
/// The formatter identity is evidence metadata. Changing formatting without
/// changing stable identifiers or schema bytes changes generated-file evidence
/// but not protocol meaning.
pub const FORMATTER_ID: &str = "zeno-fcis-codegen-renderer/1";

/// Inputs that influence generated file names and evidence identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationSpec {
    rust_module: String,
    python_module: String,
}

impl GenerationSpec {
    /// Validates deterministic Rust and Python module identifiers.
    pub fn try_new(
        rust_module: impl Into<String>,
        python_module: impl Into<String>,
    ) -> Result<Self, CodegenError> {
        let rust_module = rust_module.into();
        let python_module = python_module.into();
        if !valid_module_name(&rust_module) || !valid_module_name(&python_module) {
            return Err(CodegenError::InvalidModuleName);
        }
        Ok(Self {
            rust_module,
            python_module,
        })
    }

    /// Returns the generated Rust module name.
    #[must_use]
    pub fn rust_module(&self) -> &str {
        &self.rust_module
    }

    /// Returns the generated Python module name.
    #[must_use]
    pub fn python_module(&self) -> &str {
        &self.python_module
    }
}

/// One deterministic generated file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFile {
    path: String,
    bytes: Vec<u8>,
}

impl GeneratedFile {
    pub(crate) fn new(path: String, bytes: Vec<u8>) -> Self {
        Self { path, bytes }
    }

    /// Returns the portable forward-slash path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns exact file bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the file value.
    #[must_use]
    pub fn into_parts(self) -> (String, Vec<u8>) {
        (self.path, self.bytes)
    }
}

/// A complete content-addressed generation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedBundle {
    generator_id: &'static str,
    formatter_id: &'static str,
    schema_hash: Hash32,
    manifest_hash: Hash32,
    vector_set_hash: Hash32,
    files: Vec<GeneratedFile>,
}

impl GeneratedBundle {
    pub(crate) fn new(
        generator_id: &'static str,
        formatter_id: &'static str,
        schema_hash: Hash32,
        manifest_hash: Hash32,
        vector_set_hash: Hash32,
        files: Vec<GeneratedFile>,
    ) -> Self {
        Self {
            generator_id,
            formatter_id,
            schema_hash,
            manifest_hash,
            vector_set_hash,
            files,
        }
    }

    /// Returns the generator semantic version identifier.
    #[must_use]
    pub const fn generator_id(&self) -> &'static str {
        self.generator_id
    }

    /// Returns the renderer/formatter semantic identity.
    #[must_use]
    pub const fn formatter_id(&self) -> &'static str {
        self.formatter_id
    }

    /// Returns the closed schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the manifest commitment.
    #[must_use]
    pub const fn manifest_hash(&self) -> Hash32 {
        self.manifest_hash
    }

    /// Returns the commitment over the complete ordered vector set.
    #[must_use]
    pub const fn vector_set_hash(&self) -> Hash32 {
        self.vector_set_hash
    }

    /// Returns files in canonical path order.
    #[must_use]
    pub fn files(&self) -> &[GeneratedFile] {
        &self.files
    }
}

/// Deterministic source-generation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenError {
    /// A requested module name was not a bounded lower-snake identifier.
    InvalidModuleName,
    /// Two schema names would generate the same exported constant.
    ConstantCollision,
    /// Canonical schema encoding or commitment failed.
    SchemaEncoding,
    /// A generated file path or content length exceeded its format.
    LengthOverflow,
    /// A schema name was not a valid Rust or Python identifier.
    InvalidIdentifier,
    /// A negative vector could not be constructed deterministically.
    VectorConstruction,
}

impl fmt::Display for CodegenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModuleName => formatter.write_str("invalid generated module name"),
            Self::ConstantCollision => formatter.write_str("generated constant name collision"),
            Self::SchemaEncoding => formatter.write_str("schema encoding or commitment failed"),
            Self::LengthOverflow => formatter.write_str("generated length overflow"),
            Self::InvalidIdentifier => {
                formatter.write_str("schema name is not a valid generated identifier")
            }
            Self::VectorConstruction => {
                formatter.write_str("deterministic vector construction failed")
            }
        }
    }
}

impl std::error::Error for CodegenError {}

fn valid_module_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 96 {
        return false;
    }
    let bytes = value.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first == b'_') {
        return false;
    }
    bytes
        .iter()
        .copied()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The assurance category of one generated codec vector.
///
/// Categories are evidence metadata. They describe which boundary law a vector
/// exercises; they do not change protocol meaning.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum VectorKind {
    /// A well-formed canonical value that decodes and validates.
    Positive,
    /// An exact inclusive-bound value (min/max, empty/full).
    Boundary,
    /// Truncated or unknown-tag input rejected by the decoder.
    Malformed,
    /// A structurally decodable but noncanonical ordering rejected by the codec.
    NonCanonical,
    /// A record carrying a field identifier absent from the schema.
    UnknownField,
    /// An enum or sum carrying a variant ordinal absent from the schema.
    UnknownVariant,
    /// A complete value followed by extra bytes.
    TrailingBytes,
}

impl VectorKind {
    /// Returns the stable lowercase evidence label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Boundary => "boundary",
            Self::Malformed => "malformed",
            Self::NonCanonical => "noncanonical",
            Self::UnknownField => "unknown_field",
            Self::UnknownVariant => "unknown_variant",
            Self::TrailingBytes => "trailing_bytes",
        }
    }
}
