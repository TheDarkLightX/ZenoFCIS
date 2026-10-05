//! Schema-bound canonical envelope admission.

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{AdmittedEnvelope, CanonicalEncode, CommitmentHasher, EncodeError, Hash32};
use zeno_fcis_value::{AdmittedValue, Value, ValueError};

use crate::{
    Schema, SchemaError, TypeId, ValidationLimits, ValidationReport, ValueValidationError,
};

/// Failure while binding an owned value to a reviewed schema envelope.
///
/// The variants record the fixed local admission order. They are not protocol
/// rejection reasons and do not define application-level precedence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SchemaEnvelopeError {
    /// The value failed validation against the selected schema type.
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
                write!(formatter, "schema value validation failed: {error}")
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
        let (envelope, validation) = admit_type::<H>(schema, schema.root_type(), value, limits)?;
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

impl SchemaAdmittedEnvelope {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.envelope.encode_to(output)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// An owned canonical envelope admitted against one selected type in a reviewed schema.
///
/// Unlike [`SchemaAdmittedEnvelope`], this witness is not restricted to the schema root.
/// The selected type identifier is bound into the existing canonical envelope bytes, and
/// private fields prevent callers from pairing those bytes with invented validation metrics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaAdmittedTypeEnvelope {
    envelope: AdmittedEnvelope,
    validation: ValidationReport,
}

impl SchemaAdmittedTypeEnvelope {
    /// Validates and owns a value as the selected schema type.
    ///
    /// Local failures use the same fixed diagnostic order as root admission:
    /// schema validation, default structural admission, schema commitment, then
    /// complete-envelope size admission.
    pub fn try_new<H: CommitmentHasher>(
        schema: &Schema,
        type_id: TypeId,
        value: Value,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        let (envelope, validation) = admit_type::<H>(schema, type_id, value, limits)?;
        Ok(Self {
            envelope,
            validation,
        })
    }

    /// Returns the selected schema type bound into the envelope.
    #[must_use]
    pub const fn type_id(&self) -> TypeId {
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

    /// Returns the structurally admitted immutable value.
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

impl SchemaAdmittedTypeEnvelope {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.envelope.encode_to(output)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

fn admit_type<H: CommitmentHasher>(
    schema: &Schema,
    type_id: TypeId,
    value: Value,
    limits: ValidationLimits,
) -> Result<(AdmittedEnvelope, ValidationReport), SchemaEnvelopeError> {
    let validation = schema
        .validate_value(type_id, &value, limits)
        .map_err(SchemaEnvelopeError::SchemaValidation)?;
    let value = AdmittedValue::try_new(value).map_err(SchemaEnvelopeError::ValueAdmission)?;
    let schema_hash = schema
        .schema_hash::<H>()
        .map_err(SchemaEnvelopeError::SchemaCommitment)?;
    let envelope = AdmittedEnvelope::try_new(type_id.get(), schema_hash, value)
        .map_err(SchemaEnvelopeError::EnvelopeAdmission)?;
    Ok((envelope, validation))
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;

    use zeno_fcis_codec::{DecodeLimits, Envelope, decode_envelope};

    use super::*;
    use crate::{SchemaLimits, TypeDef, TypeKind};

    use zeno_fcis_codec::RustCryptoSha256 as TestHash;

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
            Value::unsigned(42),
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
        assert_eq!(admitted.value().value(), &Value::unsigned(42));

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
    fn binds_selected_non_root_type_schema_hash_and_validation_report() {
        let amount = type_def(7, "Amount", TypeKind::U128 { min: 1, max: 100 });
        let flag = type_def(8, "Flag", TypeKind::Bool);
        let schema = schema(1, vec![amount, flag]);
        let admitted = SchemaAdmittedTypeEnvelope::try_new::<TestHash>(
            &schema,
            TypeId::new(8),
            Value::boolean(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("typed envelope rejected: {error}"));

        assert_eq!(admitted.type_id(), TypeId::new(8));
        assert_eq!(
            admitted.schema_hash(),
            schema
                .schema_hash::<TestHash>()
                .unwrap_or_else(|error| panic!("schema hash failed: {error}"))
        );
        assert_eq!(
            admitted.validation_report(),
            ValidationReport {
                nodes: 1,
                maximum_depth: 0,
            }
        );
        assert_eq!(admitted.value().value(), &Value::boolean(true));
        let bytes = admitted
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("encoding failed: {error}"));
        assert_eq!(
            decode_envelope(&bytes, DecodeLimits::default()),
            Ok(Envelope::new(
                8,
                admitted.schema_hash(),
                Value::boolean(true)
            ))
        );
        assert_eq!(
            admitted.encoded_length(),
            u64::try_from(bytes.len()).unwrap_or(u64::MAX)
        );
    }

    #[test]
    fn selected_type_mismatch_fails_before_structural_admission() {
        let amount = type_def(7, "Amount", TypeKind::U128 { min: 1, max: 100 });
        let flag = type_def(8, "Flag", TypeKind::Bool);
        let schema = schema(1, vec![amount, flag]);
        assert_eq!(
            SchemaAdmittedTypeEnvelope::try_new::<TestHash>(
                &schema,
                TypeId::new(7),
                Value::boolean(true),
                ValidationLimits::default(),
            ),
            Err(SchemaEnvelopeError::SchemaValidation(
                ValueValidationError::TypeMismatch
            ))
        );
    }

    #[test]
    fn rejects_value_outside_root_schema() {
        let schema = amount_schema(1);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::unsigned(0),
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
                Value::boolean(true),
                ValidationLimits::default(),
            ),
            Err(SchemaEnvelopeError::SchemaValidation(
                ValueValidationError::TypeMismatch
            ))
        );
    }

    #[test]
    fn rejects_structurally_invalid_value_after_schema_shape_validation() {
        // Invalid ASCII cannot enter the private Value representation.
        assert_eq!(
            Value::text_ascii_with_limits("é".into(), zeno_fcis_value::ValueLimits::default()),
            Err(ValueError::NonAsciiText)
        );
        // A legal tree can still exceed admission limits. The schema admits
        // all 66 nodes before default structural admission rejects depth 65.
        let mut definitions = vec![type_def(72, "Leaf", TypeKind::Unit)];
        let mut value = Value::unit();
        for id in (7..72).rev() {
            definitions.push(type_def(
                id,
                &alloc::format!("Layer{id}"),
                TypeKind::Tuple {
                    items: Box::new([TypeId::new(id + 1)]),
                },
            ));
            value =
                Value::tuple(vec![value]).unwrap_or_else(|error| panic!("tuple rejected: {error}"));
        }
        let schema = schema(1, definitions);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                value,
                ValidationLimits {
                    max_depth: 65,
                    max_nodes: 66
                },
            ),
            Err(SchemaEnvelopeError::ValueAdmission(
                ValueError::DepthLimit {
                    limit: 64,
                    attempted: 65
                }
            ))
        );
    }

    #[test]
    fn validation_budget_is_applied_before_later_admission_stages() {
        let schema = amount_schema(1);
        assert_eq!(
            SchemaAdmittedEnvelope::try_new::<TestHash>(
                &schema,
                Value::unsigned(42),
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
            Value::unsigned(42),
            ValidationLimits::default(),
        );
        let right = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &right,
            Value::unsigned(42),
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
            Value::unsigned(42),
            ValidationLimits::default(),
        );
        let second = SchemaAdmittedEnvelope::try_new::<TestHash>(
            &amount_schema(2),
            Value::unsigned(42),
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
