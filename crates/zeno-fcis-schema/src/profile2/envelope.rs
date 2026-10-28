//! Owned schema2 witnesses and expected codec2 envelope admission.
use super::{
    Schema, SchemaError, TypeId, ValidationLimits, ValidationReport, ValueValidationError,
};
use alloc::vec::Vec;
use core::fmt;
use zeno_fcis_codec::profile2::{AdmittedEnvelope, Envelope, decode_envelope};
use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, EncodeError, Hash32,
};
use zeno_fcis_value::profile2::{AdmittedValue, Value, ValueError};

/// Local schema/resource/pin-admission failure, not a business rejection reason.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SchemaEnvelopeError {
    /// The existing codec2 decoder refused bytes or supplied decode resources.
    Decode(DecodeError),
    /// The decoded/supplied envelope type differs from the selected schema type.
    TypeIdentity {
        /// Selected expected type.
        expected: TypeId,
        /// Supplied type.
        actual: TypeId,
    },
    /// The supplied schema hash differs from the selected schema commitment.
    SchemaIdentity {
        /// Exact selected schema commitment.
        expected: Hash32,
        /// Supplied untrusted pin.
        actual: Hash32,
    },
    /// Schema shape, nominal type, range or validation resources failed.
    SchemaValidation(ValueValidationError),
    /// Default structural/canonical Value2 admission failed.
    ValueAdmission(ValueError),
    /// Commitment with the existing sealed provider failed.
    SchemaCommitment(SchemaError),
    /// Complete default codec2 envelope admission failed.
    EnvelopeAdmission(EncodeError),
}

impl fmt::Display for SchemaEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "codec2 decode failed: {error}"),
            Self::TypeIdentity { expected, actual } => {
                write!(
                    formatter,
                    "expected schema type {expected}, received {actual}"
                )
            }
            Self::SchemaIdentity { .. } => {
                formatter.write_str("supplied schema hash differs from expected schema2")
            }
            Self::SchemaValidation(error) => {
                write!(formatter, "schema2 validation failed: {error}")
            }
            Self::ValueAdmission(error) => {
                write!(formatter, "default Value2 admission failed: {error}")
            }
            Self::SchemaCommitment(error) => {
                write!(formatter, "schema2 commitment failed: {error}")
            }
            Self::EnvelopeAdmission(error) => {
                write!(formatter, "codec2 envelope admission failed: {error}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SchemaEnvelopeError {}

/// Owns one root value admitted against a selected Schema2 and default resources.
///
/// This is a schema/resource witness, not an Authority or publication capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaAdmittedEnvelope {
    typed: SchemaAdmittedTypeEnvelope,
}

impl SchemaAdmittedEnvelope {
    /// Admits in order: schema validation, default Value2, commitment, envelope2.
    pub fn try_new<H: CommitmentHasher>(
        schema: &Schema,
        value: Value,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        Ok(Self {
            typed: SchemaAdmittedTypeEnvelope::try_new::<H>(
                schema,
                schema.root_type(),
                value,
                limits,
            )?,
        })
    }

    /// Checks supplied pins before schema/value admission of the owned envelope.
    pub fn try_from_envelope<H: CommitmentHasher>(
        schema: &Schema,
        envelope: Envelope,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        Ok(Self {
            typed: SchemaAdmittedTypeEnvelope::try_from_envelope::<H>(
                schema,
                schema.root_type(),
                envelope,
                limits,
            )?,
        })
    }

    /// Uses codec2's complete decode first, then expected type/hash and admission.
    /// Legacy magic refuses even when its bare value contains only old tags.
    pub fn decode_expected<H: CommitmentHasher>(
        schema: &Schema,
        bytes: &[u8],
        decode_limits: DecodeLimits,
        validation_limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        Ok(Self {
            typed: SchemaAdmittedTypeEnvelope::decode_expected::<H>(
                schema,
                schema.root_type(),
                bytes,
                decode_limits,
                validation_limits,
            )?,
        })
    }

    /// Exact declared schema root bound into the envelope.
    #[must_use]
    pub const fn root_type(&self) -> TypeId {
        self.typed.type_id()
    }
    /// Exact selected schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.typed.schema_hash()
    }
    /// Successful schema-validation resource report.
    #[must_use]
    pub const fn validation_report(&self) -> ValidationReport {
        self.typed.validation_report()
    }
    /// Immutable owned structural Value2 admission.
    #[must_use]
    pub const fn value(&self) -> &AdmittedValue {
        self.typed.value()
    }
    /// Complete canonical envelope length.
    #[must_use]
    pub fn encoded_length(&self) -> u64 {
        self.typed.encoded_length()
    }
    /// Immutable default-resource codec2 envelope witness.
    #[must_use]
    pub const fn envelope(&self) -> &AdmittedEnvelope {
        self.typed.envelope()
    }
    /// Consumes this schema witness into the weaker structural envelope witness.
    #[must_use]
    pub fn into_envelope(self) -> AdmittedEnvelope {
        self.typed.into_envelope()
    }
    /// Appends the exact canonical codec2 envelope.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.typed.encode_to(output)
    }
    /// Exact canonical codec2 bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        self.typed.canonical_bytes()
    }
}

/// Owns a value admitted for one selected type in a closed Schema2.
///
/// Private fields prevent inventing a value/validation pairing. The returned
/// immutable witness grants neither authentication nor a committing capability.
/// ```compile_fail
/// use zeno_fcis_schema::profile2::{SchemaAdmittedTypeEnvelope, ValidationReport};
/// use zeno_fcis_codec::profile2::AdmittedEnvelope;
/// fn forge(envelope: AdmittedEnvelope) {
///     let _ = SchemaAdmittedTypeEnvelope { envelope, validation: ValidationReport { nodes: 0, maximum_depth: 0 } };
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaAdmittedTypeEnvelope {
    envelope: AdmittedEnvelope,
    validation: ValidationReport,
}

impl SchemaAdmittedTypeEnvelope {
    /// Schema validation, default structural admission, commitment, then envelope.
    pub fn try_new<H: CommitmentHasher>(
        schema: &Schema,
        type_id: TypeId,
        value: Value,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        let (value, validation) = admit_value(schema, type_id, value, limits)?;
        let schema_hash = schema
            .schema_hash::<H>()
            .map_err(SchemaEnvelopeError::SchemaCommitment)?;
        finish(type_id, schema_hash, value, validation)
    }

    /// Checks exact supplied type then schema hash, then schema/default resources.
    /// A matching hash alone never certifies an arbitrary value.
    pub fn try_from_envelope<H: CommitmentHasher>(
        schema: &Schema,
        type_id: TypeId,
        envelope: Envelope,
        limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        let actual = TypeId::new(envelope.type_id());
        if actual != type_id {
            return Err(SchemaEnvelopeError::TypeIdentity {
                expected: type_id,
                actual,
            });
        }
        let expected = schema
            .schema_hash::<H>()
            .map_err(SchemaEnvelopeError::SchemaCommitment)?;
        let actual = envelope.schema_hash();
        if actual != expected {
            return Err(SchemaEnvelopeError::SchemaIdentity { expected, actual });
        }
        let (value, validation) = admit_value(schema, type_id, envelope.into_value(), limits)?;
        finish(type_id, expected, value, validation)
    }

    /// Applies existing complete codec2 decode, then expected pins and admission.
    /// Decode resources/canonicality precede expected-pin checks. Successful
    /// schema admission always additionally applies default structural resources.
    pub fn decode_expected<H: CommitmentHasher>(
        schema: &Schema,
        type_id: TypeId,
        bytes: &[u8],
        decode_limits: DecodeLimits,
        validation_limits: ValidationLimits,
    ) -> Result<Self, SchemaEnvelopeError> {
        let envelope =
            decode_envelope(bytes, decode_limits).map_err(SchemaEnvelopeError::Decode)?;
        Self::try_from_envelope::<H>(schema, type_id, envelope, validation_limits)
    }

    /// Exact selected type bound into codec2 bytes.
    #[must_use]
    pub const fn type_id(&self) -> TypeId {
        TypeId::new(self.envelope.type_id())
    }
    /// Exact selected schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.envelope.schema_hash()
    }
    /// Actual successful schema-validation logical resources.
    #[must_use]
    pub const fn validation_report(&self) -> ValidationReport {
        self.validation
    }
    /// Immutable structurally admitted Value2.
    #[must_use]
    pub const fn value(&self) -> &AdmittedValue {
        self.envelope.value()
    }
    /// Exact complete codec2 envelope length.
    #[must_use]
    pub fn encoded_length(&self) -> u64 {
        self.envelope.encoded_length()
    }
    /// Immutable structural/default-resource envelope witness.
    #[must_use]
    pub const fn envelope(&self) -> &AdmittedEnvelope {
        &self.envelope
    }
    /// Consumes this schema witness into the weaker structural witness.
    #[must_use]
    pub fn into_envelope(self) -> AdmittedEnvelope {
        self.envelope
    }
    /// Appends exact canonical codec2 bytes.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.envelope.encode_to(output)
    }
    /// Exact canonical codec2 envelope bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

fn admit_value(
    schema: &Schema,
    type_id: TypeId,
    value: Value,
    limits: ValidationLimits,
) -> Result<(AdmittedValue, ValidationReport), SchemaEnvelopeError> {
    let validation = schema
        .validate_value(type_id, &value, limits)
        .map_err(SchemaEnvelopeError::SchemaValidation)?;
    let value = AdmittedValue::try_new(value).map_err(SchemaEnvelopeError::ValueAdmission)?;
    Ok((value, validation))
}

fn finish(
    type_id: TypeId,
    schema_hash: Hash32,
    value: AdmittedValue,
    validation: ValidationReport,
) -> Result<SchemaAdmittedTypeEnvelope, SchemaEnvelopeError> {
    let envelope = AdmittedEnvelope::try_new(type_id.get(), schema_hash, value)
        .map_err(SchemaEnvelopeError::EnvelopeAdmission)?;
    Ok(SchemaAdmittedTypeEnvelope {
        envelope,
        validation,
    })
}
