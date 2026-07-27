//! Root-schema-bound canonical envelope admission.

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{AdmittedEnvelope, CanonicalEncode, CommitmentHasher, EncodeError, Hash32};
use zeno_fcis_value::{AdmittedValue, Value, ValueError};

use crate::{
    Schema, SchemaError, TypeId, ValidationLimits, ValidationReport, ValueValidationError,
};

/// Failure while binding an owned root value to a reviewed schema envelope.
///
/// The variants record the fixed local admission order. They are not protocol
/// rejection reasons and do not define application-level precedence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaEnvelopeError {
    /// The value failed validation against the schema's declared root type.
    SchemaValidation(ValueValidationError),
    /// The value failed the reviewed default structural and canonical limits.
    ValueAdmission(ValueError),
    /// The schema could not be canonically committed by the selected provider.
    SchemaCommitment(SchemaError),
    /// The complete envelope failed canonical size admission.
    EnvelopeAdmission(EncodeError),
}

impl fmt::Display for SchemaEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaValidation(error) => {
                write!(formatter, "schema root validation failed: {error}")
            }
            Self::ValueAdmission(error) => {
                write!(formatter, "default value admission failed: {error}")
            }
            Self::SchemaCommitment(error) => {
                write!(formatter, "schema commitment failed: {error}")
            }
            Self::EnvelopeAdmission(error) => {
                write!(formatter, "complete envelope admission failed: {error}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SchemaEnvelopeError {}

/// An owned canonical envelope admitted against one reviewed schema root.
///
/// Construction validates the value against the schema's declared root under
/// caller-supplied deterministic validation limits, applies the reviewed
/// default structural value limits, computes the exact schema commitment, and
/// admits the complete envelope under the default decoder input limit.
///
/// Private fields prevent callers from pairing an envelope with invented
/// schema-validation metrics:
///
/// ```compile_fail
/// use zeno_fcis_codec::AdmittedEnvelope;
/// use zeno_fcis_schema::{SchemaAdmittedEnvelope, ValidationReport};
///
/// fn forge(envelope: AdmittedEnvelope) {
///     let _ = SchemaAdmittedEnvelope {
///         envelope,
///         validation: ValidationReport {
///             nodes: 0,
///             maximum_depth: 0,
///         },
///     };
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaAdmittedEnvelope {
    envelope: AdmittedEnvelope,
    validation: ValidationReport,
}

impl SchemaAdmittedEnvelope {
    /// Validates and owns a value as the supplied schema's root envelope.
    ///
    /// Local failures are selected in this fixed order:
    ///
    /// 1. root-schema validation;
    /// 2. default structural value admission;
    /// 3. schema commitment;
    /// 4. complete-envelope size admission.
    ///
    /// The order is an API diagnostic contract, not a protocol rejection
    /// precedence registry.
    pub fn try_new<H: CommitmentHasher>(
        schema: &Schema,
        value: Value,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        let validation = schema
            .validate_root(&value, limits)
            .map_err(SchemaEnvelopeError::SchemaValidation)?;
        let value = AdmittedValue::try_new(value).map_err(SchemaEnvelopeError::ValueAdmission)?;
        let schema_hash = schema
            .schema_hash::<H>()
            .map_err(SchemaEnvelopeError::SchemaCommitment)?;
        let envelope = AdmittedEnvelope::try_new(schema.root_type().get(), schema_hash, value)
            .map_err(SchemaEnvelopeError::EnvelopeAdmission)?;
        Ok(Self {
            envelope,
            validation,
        })
    }

    /// Returns the schema's declared root type bound into the envelope.
    #[must_use]
    pub const fn root_type(&self) -> TypeId {
        TypeId::new(self.envelope.type_id())
    }

    /// Returns the exact schema commitment bound into the envelope.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.envelope.schema_hash()
    }

    /// Returns the exact successful schema-validation resource report.
    #[must_use]
    pub const fn validation_report(&self) -> ValidationReport {
        self.validation
    }

    /// Returns the structurally admitted immutable root value.
    #[must_use]
    pub const fn value(&self) -> &AdmittedValue {
        self.envelope.value()
    }

    /// Returns the exact complete canonical envelope length.
    #[must_use]
    pub fn encoded_length(&self) -> u64 {
        self.envelope.encoded_length()
    }

    /// Returns the compatible structurally admitted envelope.
    #[must_use]
    pub const fn envelope(&self) -> &AdmittedEnvelope {
        &self.envelope
    }

    /// Consumes the schema-bound witness and returns its admitted envelope.
    #[must_use]
    pub fn into_envelope(self) -> AdmittedEnvelope {
        self.envelope
    }
}

impl CanonicalEncode for SchemaAdmittedEnvelope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.envelope.encode_to(output)
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;

    use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Envelope, Hash32, decode_envelope};

    use super::*;
    use crate::{SchemaLimits, TypeDef, TypeKind};

    struct TestHash;

    impl CommitmentHasher for TestHash {
        const ALGORITHM_ID: &'static str = "test/noncryptographic";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(byte)
                    .rotate_left((slot % 7) as u32);
            }
            Hash32::new(output)
        }
    }

    fn type_def(id: u32, name: &str, kind: TypeKind) -> TypeDef {
        match TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default()) {
            Ok(value) => value,
            Err(error) => panic!("type rejected: {error}"),
        }
    }

    fn schema(version: u16, types: Vec<TypeDef>) -> Schema {
        match Schema::try_new(
            "EnvelopeProfile",
            version,
            TypeId::new(7),
            types,
            SchemaLimits::default(),
        ) {
            Ok(value) => value,
            Err(error) => panic!("schema rejected: {error}"),
        }
    }

    fn amount_schema(version: u16) -> Schema {
        schema(
            version,
            vec![type_def(7, "Amount", TypeKind::U128 { min: 1, max: 100 })],
        )
    }

    #[test]
    fn binds_root_type_schema_hash_and_validation_report() {
        let schema = amount_schema(1);
        let admitted = match SchemaAdmittedEnvelope::try_new::<TestHash>(
            &schema,
            Value::U128(42),
            ValidationLimits::default(),
        ) {
            Ok(value) => value,
            Err(error) => panic!("root envelope rejected: {error}"),
        };

        assert_eq!(admitted.root_type(), TypeId::new(7));
        let expected_hash = match schema.schema_hash::<TestHash>() {
            Ok(value) => value,
            Err(error) => panic!("schema hash failed: {error}"),
        };
        assert_eq!(admitted.schema_hash(), expected_hash);
        assert_eq!(
            admitted.validation_report(),
            ValidationReport {
                nodes: 1,
                maximum_depth: 0,
            }
        );
        assert_eq!(admitted.value().value(), &Value::U128(42));

        let bytes = match admitted.canonical_bytes() {
            Ok(value) => value,
            Err(error) => panic!("encoding failed: {error}"),
        };
        assert_eq!(
            admitted.encoded_length(),
            u64::try_from(bytes.len()).unwrap_or(u64::MAX)
        );
        let raw = Envelope::new(7, admitted.schema_hash(), admitted.value().value().clone());
        assert_eq!(
            decode_envelope(&bytes, DecodeLimits::default()),
            Ok(raw.clone())
        );
        assert_eq!(admitted.into_envelope().into_envelope(), raw);
    }

    #[test]
    fn rejects_value_outside_root_schema() {
        let schema = amount_schema(1);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::U128(0),
                ValidationLimits::default(),
            ),
            Err(SchemaEnvelopeError::SchemaValidation(
                ValueValidationError::IntegerRange
            ))
        );
    }

    #[test]
    fn rejects_value_with_wrong_root_kind() {
        let schema = amount_schema(1);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::Bool(true),
                ValidationLimits::default(),
            ),
            Err(SchemaEnvelopeError::SchemaValidation(
                ValueValidationError::TypeMismatch
            ))
        );
    }

    #[test]
    fn rejects_structurally_invalid_value_after_schema_shape_validation() {
        let schema = schema(
            1,
            vec![type_def(
                7,
                "Label",
                TypeKind::Text {
                    min_len: 1,
                    max_len: 8,
                },
            )],
        );
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::Text(Box::<str>::from("é")),
                ValidationLimits::default(),
            ),
            Err(SchemaEnvelopeError::ValueAdmission(
                ValueError::NonAsciiText
            ))
        );
    }

    #[test]
    fn validation_budget_is_applied_before_later_admission_stages() {
        let schema = amount_schema(1);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::U128(42),
                ValidationLimits {
                    max_depth: 0,
                    max_nodes: 0,
                },
            ),
            Err(SchemaEnvelopeError::SchemaValidation(
                ValueValidationError::BudgetExceeded
            ))
        );
    }

    #[test]
    fn schema_declaration_order_does_not_change_envelope_bytes() {
        let amount = type_def(7, "Amount", TypeKind::U128 { min: 1, max: 100 });
        let flag = type_def(8, "Flag", TypeKind::Bool);
        let left = schema(1, vec![flag.clone(), amount.clone()]);
        let right = schema(1, vec![amount, flag]);

        let left = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &left,
            Value::U128(42),
            ValidationLimits::default(),
        );
        let right = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &right,
            Value::U128(42),
            ValidationLimits::default(),
        );
        let (left, right) = match (left, right) {
            (Ok(left), Ok(right)) => (left, right),
            (left, right) => panic!("envelopes rejected: {left:?} {right:?}"),
        };
        assert_eq!(left.schema_hash(), right.schema_hash());
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn schema_version_changes_bound_envelope_bytes() {
        let first = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &amount_schema(1),
            Value::U128(42),
            ValidationLimits::default(),
        );
        let second = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &amount_schema(2),
            Value::U128(42),
            ValidationLimits::default(),
        );
        let (first, second) = match (first, second) {
            (Ok(first), Ok(second)) => (first, second),
            (first, second) => panic!("envelopes rejected: {first:?} {second:?}"),
        };
        assert_ne!(first.schema_hash(), second.schema_hash());
        assert_ne!(first.canonical_bytes(), second.canonical_bytes());
    }
}
