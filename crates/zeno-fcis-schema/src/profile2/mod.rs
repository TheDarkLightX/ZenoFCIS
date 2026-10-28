//! Explicit, experimental Schema2 admission over owned Value2 and codec2.
//!
//! This namespace fixes schema format2 and ZCVE/2. Application names/revisions
//! are separate identity fields. Structural/schema admission is not Catalog,
//! Authority, publication, authentication or deployment admission.

use core::fmt;
use zeno_fcis_value::profile2::ValueProfile;

pub use crate::{
    EnumVariantDef, FieldDef, FieldId, SchemaError, SchemaLimits, SchemaMetrics, SchemaName,
    SumVariantDef, TypeId, ValidationLimits, ValidationReport, ValueValidationError, VariantId,
};

/// Exact schema2 prefix, disjoint from schema1 even for legacy-only kinds.
pub const SCHEMA_MAGIC: &[u8; 13] = b"ZFCISSCHEMA2\0";
/// The canonical schema format selected by this namespace.
pub const SCHEMA_FORMAT: u16 = 2;

/// Failure to select the one supported schema/value profile pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProfileError {
    /// The supplied schema-format version is not2.
    UnsupportedSchemaFormat(u16),
    /// The supplied value profile is not ZCVE/2.
    UnsupportedValueProfile(ValueProfile),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaFormat(version) => {
                write!(formatter, "unsupported schema format {version}")
            }
            Self::UnsupportedValueProfile(_) => formatter.write_str("schema2 requires ZCVE/2"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ProfileError {}

/// Closed explicit profile selection; it has no default or caller-forged fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaProfile(());

impl SchemaProfile {
    /// Explicit static selection of schema format2 over ZCVE/2.
    pub const VALUE2: Self = Self(());

    /// Admits only the exact format2/ZCVE2 pair, before schema construction.
    pub fn admit(schema_format: u16, value_profile: ValueProfile) -> Result<Self, ProfileError> {
        if schema_format != SCHEMA_FORMAT {
            return Err(ProfileError::UnsupportedSchemaFormat(schema_format));
        }
        if value_profile != ValueProfile::Zcve2 {
            return Err(ProfileError::UnsupportedValueProfile(value_profile));
        }
        Ok(Self::VALUE2)
    }

    /// Returns the fixed schema format, not the application revision.
    #[must_use]
    pub const fn schema_format(self) -> u16 {
        SCHEMA_FORMAT
    }

    /// Returns the fixed recursive value profile.
    #[must_use]
    pub const fn value_profile(self) -> ValueProfile {
        ValueProfile::Zcve2
    }
}

mod encoding;
mod envelope;
mod model;
mod validate;

pub use envelope::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, SchemaEnvelopeError};
pub use model::{Schema, TypeDef, TypeKind};

#[cfg(test)]
mod tests;
