//! Zeno Canonical Value Encoding reference implementation.
//!
//! ZCVE/1 is deliberately small: fixed-width integers, definite lengths,
//! stable numeric record and variant identifiers, ASCII text in the initial
//! profile, and canonical encoded-key map order. It is not a generic Serde
//! format and does not accept alternate encodings for one semantic value.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(verus_keep_ghost)]
use decoder_spec::{read_blob, tail};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use zeno_fcis_value::decoder_model::*;
#[cfg(verus_keep_ghost)]
use zeno_fcis_value::{
    entries_model, entries_model_properties, entries_model_push, entries_valid, entry_values_fit,
    fields_model, fields_model_properties, fields_model_push, fields_valid, values_model,
    values_model_properties, values_model_push, values_valid,
};
#[cfg(verus_keep_ghost)]
include!("decoder_refinement.rs");
#[cfg(verus_keep_ghost)]
pub mod decoder_induction;
#[cfg(verus_keep_ghost)]
pub mod decoder_spec;

#[cfg(not(verus_keep_ghost))]
mod artifact;
#[cfg(not(verus_keep_ghost))]
mod providers;
#[cfg(feature = "libcrux")]
pub use providers::LibcruxSha256;
#[cfg(not(verus_keep_ghost))]
pub use providers::RustCryptoSha256;
#[cfg(not(verus_keep_ghost))]
pub mod domains;
#[cfg(not(verus_keep_ghost))]
pub use artifact::EvidenceArtifact;

use alloc::borrow::ToOwned;

use alloc::vec::Vec;
use core::fmt;

mod decoder_bytes;
use decoder_bytes::{bits_to_signed, bytes_before, read_word};
use zeno_fcis_value::zcve::{
    TAG_BOOL_FALSE, TAG_BOOL_TRUE, TAG_BYTES, TAG_ENUM, TAG_I128, TAG_MAP, TAG_RECORD, TAG_SUM,
    TAG_TEXT, TAG_TUPLE, TAG_U128, TAG_UNIT, TAG_VECTOR,
};
use zeno_fcis_value::{
    AdmittedValue, Field, MapEntry, Value, ValueError, ValueLimits, ValueMetrics,
};
#[cfg(not(verus_keep_ghost))]
const ENVELOPE_MAGIC: &[u8; 8] = b"ZFCISV1\0";
#[cfg(not(verus_keep_ghost))]
const ENVELOPE_OVERHEAD_BYTES: u64 = 8 + 4 + 32 + 4;
#[cfg(not(verus_keep_ghost))]
const HASH_MAGIC: &[u8; 14] = b"ZENOFCIS-HASH\0";

/// A 256-bit commitment value.
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct Hash32([u8; 32]);

#[cfg(not(verus_keep_ghost))]
impl Hash32 {
    /// The all-zero hash, useful only for explicit sentinel schemas.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const ZERO: Self = Self([0; 32]);

    /// Creates a hash from exact bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consumes the wrapper.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[cfg(not(verus_keep_ghost))]
impl fmt::Display for Hash32 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
mod hasher_seal {
    pub trait Sealed {}
}

/// A cryptographic commitment provider.
///
/// The semantic kernel owns domain separation and preimage construction. A
/// provider supplies only the vetted 32-byte hash primitive.
///
/// Only the library's closed providers implement this trait.
/// ```compile_fail
/// use zeno_fcis_codec::{CommitmentHasher, Hash32};
/// struct Impostor;
/// impl CommitmentHasher for Impostor {
///     const ALGORITHM_ID: &'static str = "sha2-256/rustcrypto-0.11.0";
///     fn hash(_: &[u8]) -> Hash32 { Hash32::ZERO }
/// }
/// ```
/// ```compile_fail
/// struct Impostor;
/// impl zeno_fcis_codec::hasher_seal::Sealed for Impostor {}
/// ```
#[cfg(not(verus_keep_ghost))]
pub trait CommitmentHasher: hasher_seal::Sealed {
    /// Stable algorithm identifier, such as `sha2-256/rustcrypto-0.11`.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const ALGORITHM_ID: &'static str;

    /// Hashes exact bytes to 32 bytes.
    fn hash(bytes: &[u8]) -> Hash32;

    /// Hashes the concatenation of ordered byte slices.
    ///
    /// This must equal `Self::hash(&parts.concat())`: slice boundaries and
    /// empty slices have no meaning. Providers may override this with an
    /// incremental implementation to avoid allocating the concatenation.
    /// The default preserves existing one-shot provider implementations.
    fn hash_parts(parts: &[&[u8]]) -> Hash32 {
        Self::hash(&parts.concat())
    }
}

/// A versioned domain-separation tag.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
pub struct Domain<'a> {
    name: &'a str,
    version: u16,
}

#[cfg(not(verus_keep_ghost))]
impl<'a> Domain<'a> {
    /// Validates a project domain outside the reserved library namespace.
    ///
    /// Fixed library domains are available only through [`domains`]. A caller
    /// cannot recreate even a registered reserved domain by supplying its name.
    pub fn new(name: &'a str, version: u16) -> Result<Self, EncodeError> {
        if name.is_empty()
            || !name.is_ascii()
            || name.len() > usize::from(u16::MAX)
            || is_reserved_domain_name(name)
        {
            return Err(EncodeError::InvalidDomain);
        }
        Ok(Self { name, version })
    }

    /// Returns the domain name.
    #[must_use]
    pub const fn name(self) -> &'a str {
        self.name
    }

    /// Returns the domain version.
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }
}

/// Namespace reserved for fixed library commitment identities.
#[cfg(not(verus_keep_ghost))]
pub const RESERVED_DOMAIN_NAMESPACE: &str = "zeno-fcis";

/// Tests the exact reserved namespace boundary without normalization.
#[must_use]
#[cfg(not(verus_keep_ghost))]
pub fn is_reserved_domain_name(name: &str) -> bool {
    name == RESERVED_DOMAIN_NAMESPACE || name.starts_with("zeno-fcis/")
}

/// Builds the exact domain-separated hash preimage.
#[cfg(not(verus_keep_ghost))]
pub fn domain_preimage(domain: Domain<'_>, payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    let domain_length =
        u16::try_from(domain.name.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let mut output =
        Vec::with_capacity(HASH_MAGIC.len() + 2 + 2 + domain.name.len() + 8 + payload.len());
    output.extend_from_slice(HASH_MAGIC);
    output.extend_from_slice(&domain.version.to_be_bytes());
    output.extend_from_slice(&domain_length.to_be_bytes());
    output.extend_from_slice(domain.name.as_bytes());
    output.extend_from_slice(&payload_length.to_be_bytes());
    output.extend_from_slice(payload);
    Ok(output)
}

/// Computes a domain-separated commitment with a selected provider.
#[cfg(not(verus_keep_ghost))]
pub fn commitment<H: CommitmentHasher>(
    domain: Domain<'_>,
    payload: &[u8],
) -> Result<Hash32, EncodeError> {
    let domain_length =
        u16::try_from(domain.name.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let parts: [&[u8]; 6] = [
        HASH_MAGIC,
        &domain.version.to_be_bytes(),
        &domain_length.to_be_bytes(),
        domain.name.as_bytes(),
        &payload_length.to_be_bytes(),
        payload,
    ];
    // Small commitments need only one primitive call. The bounded local
    // buffer avoids both a heap allocation and multiple incremental updates.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    const SMALL_PREIMAGE_BYTES: usize = 128;
    let header_length = HASH_MAGIC.len() + 2 + 2 + domain.name.len() + 8;
    if let Some(length) = header_length
        .checked_add(payload.len())
        .filter(|length| *length <= SMALL_PREIMAGE_BYTES)
    {
        let mut preimage = [0_u8; SMALL_PREIMAGE_BYTES];
        let mut offset = 0;
        for part in parts {
            let end = offset + part.len();
            preimage[offset..end].copy_from_slice(part);
            offset = end;
        }
        return Ok(H::hash(&preimage[..length]));
    }
    Ok(H::hash_parts(&parts))
}

#[cfg(not(verus_keep_ghost))]
mod canonical_seal {
    pub trait Sealed {}
    impl Sealed for zeno_fcis_value::Value {}
    impl Sealed for zeno_fcis_value::AdmittedValue {}
    impl Sealed for super::Envelope {}
    impl Sealed for super::AdmittedEnvelope {}
}

/// Canonical encoding for the four closed library value/envelope types.
///
/// The private supertrait prevents downstream encoding substitution.
/// ```compile_fail
/// use zeno_fcis_codec::{CanonicalEncode, EncodeError};
/// struct Forged;
/// impl CanonicalEncode for Forged {
///     fn encode_to(&self, _: &mut Vec<u8>) -> Result<(), EncodeError> { Ok(()) }
/// }
/// ```
/// ```compile_fail
/// impl zeno_fcis_codec::canonical_seal::Sealed for Forged {}
/// struct Forged;
/// ```
#[cfg(not(verus_keep_ghost))]
pub trait CanonicalEncode: canonical_seal::Sealed {
    /// Appends one canonical encoding.
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError>;

    /// Returns newly allocated canonical bytes.
    fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

#[cfg(not(verus_keep_ghost))]
impl CanonicalEncode for Value {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output).map_err(map_value_encode_error)
    }
}

#[cfg(not(verus_keep_ghost))]
impl CanonicalEncode for AdmittedValue {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output).map_err(map_value_encode_error)
    }
}

/// Appends a canonical value using exactly the supplied structural limits.
/// Every refusal preserves the existing output prefix.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures value.representation_valid() ==> result.is_ok() == (wf(value.model(), limits.model(0))
        && old(output)@.len() + spec_encode(value.model()).len() <= usize::MAX),
        value.representation_valid() && result.is_ok() ==>
            final(output)@ == old(output)@ + spec_encode(value.model()),
        result.is_err() ==> final(output)@ == old(output)@,
))]
pub fn encode_value_with_limits(
    value: &Value,
    output: &mut Vec<u8>,
    limits: ValueLimits,
) -> Result<(), EncodeError> {
    value
        .encode_zcve_to_with_limits(output, limits)
        .map_err(map_value_encode_error)
}

/// Returns canonical value bytes using exactly the supplied structural limits.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures value.representation_valid() ==> result.is_ok() == (wf(value.model(), limits.model(0))
        && spec_encode(value.model()).len() <= usize::MAX),
        value.representation_valid() && result.is_ok() ==> result->Ok_0@ == spec_encode(value.model()),
))]
pub fn canonical_value_bytes_with_limits(
    value: &Value,
    limits: ValueLimits,
) -> Result<Vec<u8>, EncodeError> {
    value
        .zcve_bytes_with_limits(limits)
        .map_err(map_value_encode_error)
}

fn map_value_encode_error(error: ValueError) -> EncodeError {
    match error {
        ValueError::RecordFieldOrder { .. } => EncodeError::NonCanonicalRecord,
        ValueError::MapKeyOrder => EncodeError::NonCanonicalMap,
        ValueError::NonAsciiText => EncodeError::NonAsciiText,
        other => EncodeError::InvalidValue(other),
    }
}

/// A canonical typed value envelope.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub struct Envelope {
    type_id: u32,
    schema_hash: Hash32,
    value: Value,
}

#[cfg(not(verus_keep_ghost))]
impl Envelope {
    /// Creates an envelope.
    #[must_use]
    pub const fn new(type_id: u32, schema_hash: Hash32, value: Value) -> Self {
        Self {
            type_id,
            schema_hash,
            value,
        }
    }

    /// Appends a complete frame admitted by the exact supplied limits.
    /// Every refusal leaves the output prefix unchanged.
    pub fn encode_to_with_limits(
        &self,
        output: &mut Vec<u8>,
        limits: DecodeLimits,
    ) -> Result<(), EncodeError> {
        let payload = canonical_value_bytes_with_limits(&self.value, limits.value)?;
        append_envelope(
            output,
            self.type_id,
            self.schema_hash,
            &payload,
            limits.max_input_bytes,
        )
    }

    /// Returns a complete frame admitted by the exact supplied limits.
    pub fn canonical_bytes_with_limits(
        &self,
        limits: DecodeLimits,
    ) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to_with_limits(&mut output, limits)?;
        Ok(output)
    }

    /// Returns the type identifier.
    #[must_use]
    pub const fn type_id(&self) -> u32 {
        self.type_id
    }

    /// Returns the schema hash.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the envelope.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }
}

/// A canonical envelope admitted under its exact retained value and input limits.
///
/// Construction owns an [`AdmittedValue`] and retains its exact canonical
/// payload length. Private fields prevent callers from pairing a different
/// value with that retained length:
///
/// ```compile_fail
/// use zeno_fcis_codec::{AdmittedEnvelope, Hash32};
/// use zeno_fcis_value::{AdmittedValue, Value};
///
/// let value = AdmittedValue::try_new(Value::unit())?;
/// let _ = AdmittedEnvelope {
///     type_id: 1,
///     schema_hash: Hash32::ZERO,
///     value,
///     payload_length: 1,
///     max_input_bytes: 1,
/// };
/// # Ok::<(), zeno_fcis_value::ValueError>(())
/// ```
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub struct AdmittedEnvelope {
    type_id: u32,
    schema_hash: Hash32,
    value: AdmittedValue,
    payload_length: u32,
    max_input_bytes: u64,
}

#[cfg(not(verus_keep_ghost))]
impl AdmittedEnvelope {
    /// Creates an envelope whose complete canonical bytes fit the default
    /// decoder input limit.
    pub fn try_new(
        type_id: u32,
        schema_hash: Hash32,
        value: AdmittedValue,
    ) -> Result<Self, EncodeError> {
        Self::try_new_with_limit(
            type_id,
            schema_hash,
            value,
            DecodeLimits::DEFAULT_MAX_INPUT_BYTES,
        )
    }

    fn try_new_with_limit(
        type_id: u32,
        schema_hash: Hash32,
        value: AdmittedValue,
        max_input_bytes: u64,
    ) -> Result<Self, EncodeError> {
        Self::try_new_with_limits(
            type_id,
            schema_hash,
            value,
            DecodeLimits {
                max_input_bytes,
                value: ValueLimits::default(),
            },
        )
    }

    /// Owns a value admitted under the exact supplied structural and input limits.
    pub fn try_new_with_limits(
        type_id: u32,
        schema_hash: Hash32,
        value: AdmittedValue,
        limits: DecodeLimits,
    ) -> Result<Self, EncodeError> {
        let value = if value.limits() == limits.value {
            value
        } else {
            AdmittedValue::try_new_with_limits(value.into_value(), limits.value)
                .map_err(map_value_encode_error)?
        };
        let max_input_bytes = limits.max_input_bytes;
        let payload_length = u32::try_from(
            value
                .value()
                .encoded_length()
                .map_err(map_value_encode_error)?,
        )
        .map_err(|_| EncodeError::LengthOverflow)?;
        let attempted = ENVELOPE_OVERHEAD_BYTES
            .checked_add(u64::from(payload_length))
            .ok_or(EncodeError::LengthOverflow)?;
        if attempted > max_input_bytes {
            return Err(EncodeError::EnvelopeInputLimit {
                limit: max_input_bytes,
                attempted,
            });
        }
        Ok(Self {
            type_id,
            schema_hash,
            value,
            payload_length,
            max_input_bytes,
        })
    }

    /// Admits an existing raw envelope under the reviewed default limits.
    pub fn try_from_envelope(envelope: Envelope) -> Result<Self, EncodeError> {
        Self::try_from_envelope_with_limits(envelope, DecodeLimits::default())
    }

    /// Admits an existing envelope under exactly the supplied limits.
    pub fn try_from_envelope_with_limits(
        envelope: Envelope,
        limits: DecodeLimits,
    ) -> Result<Self, EncodeError> {
        let value = AdmittedValue::try_new_with_limits(envelope.value, limits.value)
            .map_err(map_value_encode_error)?;
        Self::try_new_with_limits(envelope.type_id, envelope.schema_hash, value, limits)
    }

    /// Returns the actual limits retained at admission.
    #[must_use]
    pub const fn limits(&self) -> DecodeLimits {
        DecodeLimits {
            max_input_bytes: self.max_input_bytes,
            value: self.value.limits(),
        }
    }

    /// Returns the type identifier.
    #[must_use]
    pub const fn type_id(&self) -> u32 {
        self.type_id
    }

    /// Returns the schema hash.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the admitted immutable value.
    #[must_use]
    pub const fn value(&self) -> &AdmittedValue {
        &self.value
    }

    /// Returns the exact canonical payload length retained at admission.
    #[must_use]
    pub const fn payload_length(&self) -> u32 {
        self.payload_length
    }

    /// Returns the complete canonical envelope length.
    #[must_use]
    pub fn encoded_length(&self) -> u64 {
        ENVELOPE_OVERHEAD_BYTES + u64::from(self.payload_length)
    }

    /// Consumes the wrapper and returns the admitted value.
    #[must_use]
    pub fn into_value(self) -> AdmittedValue {
        self.value
    }

    /// Consumes the wrapper and returns the compatible raw envelope.
    #[must_use]
    pub fn into_envelope(self) -> Envelope {
        Envelope::new(self.type_id, self.schema_hash, self.value.into_value())
    }
}

#[cfg(not(verus_keep_ghost))]
fn append_envelope(
    output: &mut Vec<u8>,
    type_id: u32,
    schema_hash: Hash32,
    payload: &[u8],
    max_input_bytes: u64,
) -> Result<(), EncodeError> {
    let payload_length = u32::try_from(payload.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let attempted = ENVELOPE_OVERHEAD_BYTES
        .checked_add(u64::from(payload_length))
        .ok_or(EncodeError::LengthOverflow)?;
    if attempted > max_input_bytes {
        return Err(EncodeError::EnvelopeInputLimit {
            limit: max_input_bytes,
            attempted,
        });
    }
    let total = usize::try_from(attempted).map_err(|_| EncodeError::LengthOverflow)?;
    output
        .len()
        .checked_add(total)
        .ok_or(EncodeError::LengthOverflow)?;
    // All fallible validation and arithmetic precedes the first output write.
    output.extend_from_slice(ENVELOPE_MAGIC);
    output.extend_from_slice(&type_id.to_be_bytes());
    output.extend_from_slice(schema_hash.as_bytes());
    output.extend_from_slice(&payload_length.to_be_bytes());
    output.extend_from_slice(payload);
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
impl CanonicalEncode for Envelope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_to_with_limits(
            output,
            DecodeLimits {
                max_input_bytes: u64::MAX,
                value: ValueLimits::default(),
            },
        )
    }
}

#[cfg(not(verus_keep_ghost))]
impl CanonicalEncode for AdmittedEnvelope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        let payload = self.value.canonical_bytes()?;
        append_envelope(
            output,
            self.type_id,
            self.schema_hash,
            &payload,
            self.max_input_bytes,
        )
    }
}

/// Canonical decoder with explicit structural limits.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
#[cfg_attr(verus_keep_ghost, derive(Clone, Copy))]
pub struct DecodeLimits {
    /// Maximum input bytes.
    pub max_input_bytes: u64,
    /// Closed-value structural limits.
    pub value: ValueLimits,
}

#[cfg(not(verus_keep_ghost))]
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: Self::DEFAULT_MAX_INPUT_BYTES,
            value: ValueLimits::default(),
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl DecodeLimits {
    pub open spec fn model(self) -> ModelLimits { self.value.model(self.max_input_bytes) }
}
}

impl DecodeLimits {
    /// Reviewed default maximum for complete input bytes.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
}

/// Decodes one value and rejects trailing bytes and noncanonical aliases.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures decoded_result(&result, bytes@.len()) == decoder_spec::spec_decode(bytes@, limits.model()),
        result matches Ok(value) ==> value.representation_valid()
            && wf(value.model(), limits.model()) && bytes@ == spec_encode(value.model())
            && bytes.len() <= limits.max_input_bytes,
))]
pub fn decode_value(bytes: &[u8], limits: DecodeLimits) -> Result<Value, DecodeError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        decoder_spec::spec_t2_canonical(bytes@, limits.model());
        decoder_spec::spec_t3_input_limit(bytes@, limits.model());
    }
    match decode_value_with_metrics(bytes, limits) {
        Ok((value, _)) => Ok(value),
        Err(error) => Err(error),
    }
}

/// Decodes a value and returns the exact counters observed by the actual decoder.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures decoded_metrics_result(&result, bytes@.len()) == decoder_spec::spec_decode(bytes@, limits.model()),
        result matches Ok((value, _)) ==> value.representation_valid(),
))]
pub fn decode_value_with_metrics(
    bytes: &[u8],
    limits: DecodeLimits,
) -> Result<(Value, ValueMetrics), DecodeError> {
    enforce_input_limit(bytes, limits)?;
    let mut state = DecodeState::new(limits.value);
    let value = decode_complete_value(bytes, &mut state, 0)?;
    Ok((
        value,
        ValueMetrics {
            nodes: state.nodes,
            payload_bytes: state.payload_bytes,
            depth: state.depth,
        },
    ))
}

/// Decodes one canonical envelope.
///
/// The envelope frame is reconstructed byte for byte from checks already made
/// here: the magic was compared exactly, the type identifier and schema hash
/// are copied through unchanged, the payload length was read as a `u32` and
/// exactly that many bytes were taken, [`decode_value`] accepted the payload
/// only after it re-encoded to those same bytes, and trailing bytes were
/// rejected before the payload was decoded. Re-encoding the assembled envelope
/// and comparing it with the input can therefore only ever succeed, so this
/// function does not repeat that work.
#[cfg(not(verus_keep_ghost))]
pub fn decode_envelope(bytes: &[u8], limits: DecodeLimits) -> Result<Envelope, DecodeError> {
    enforce_input_limit(bytes, limits)?;
    let mut cursor = Cursor::new(bytes);
    if cursor.take(ENVELOPE_MAGIC.len())? != ENVELOPE_MAGIC {
        return Err(DecodeError::EnvelopeMagic);
    }
    let type_id = cursor.take_u32()?;
    let schema_bytes = cursor.take(32)?;
    let mut schema_hash = [0_u8; 32];
    schema_hash.copy_from_slice(schema_bytes);
    let payload = cursor.take_blob(limits.max_input_bytes)?;
    if cursor.remaining() != 0 {
        return Err(DecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    let value = decode_value(payload, limits)?;
    Ok(Envelope::new(type_id, Hash32::new(schema_hash), value))
}

/// Decodes one complete framed value from exactly `bytes`.
///
/// Top-level and nested framed values are decided identically: the whole slice
/// must be consumed and the decoded value must re-encode to it. The caller owns
/// the structural budget. [`decode_value`] enforces the input size and starts a
/// fresh [`DecodeState`] at depth zero; a nested map key or value shares its
/// parent's state and enters at `depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?`, so nesting inside a blob is
/// charged against the same depth, node, and payload budgets as nesting on the
/// wire. Reported trailing-byte offsets are relative to `bytes`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(state).invariant(),
    ensures final(state).invariant(), final(state).limits == old(state).limits,
        result matches Ok(value) ==> value.representation_valid(),
        result matches Ok(value) ==> wf(value.model(), old(state).limits.model(0))
            && bytes@ == spec_encode(value.model()),
        parsed_result(&result, bytes@.len(), final(state).nodes as nat, final(state).payload_bytes as nat)
            == decoder_spec::parse_complete(bytes@, old(state).limits.model(0), depth as nat,
                old(state).nodes as nat, old(state).payload_bytes as nat),
    decreases if depth <= 1024 { 1025 - depth as int } else { 0 }, 1nat,
))]
fn decode_complete_value(
    bytes: &[u8],
    state: &mut DecodeState,
    depth: u32,
) -> Result<Value, DecodeError> {
    let mut cursor = Cursor::new(bytes);
    let value = decode_value_inner(&mut cursor, state, depth)?;
    if cursor.remaining() != 0 {
        return Err(DecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(cursor.offset == bytes.len()); }
    let canonical =
        canonical_value_bytes_with_limits(&value, state.limits).map_err(map_decode_encode_error)?;
    if canonical.as_slice() != bytes {
        return Err(DecodeError::NonCanonical);
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(canonical@ == bytes@); }
    Ok(value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (bytes.len() <= limits.max_input_bytes),
))]
fn enforce_input_limit(bytes: &[u8], limits: DecodeLimits) -> Result<(), DecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| DecodeError::LengthOverflow)?;
    if actual > limits.max_input_bytes {
        return Err(DecodeError::InputLimit {
            limit: limits.max_input_bytes,
            actual,
        });
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == DecodeError::InvalidValue(error),
))]
fn map_value_decode_error(error: ValueError) -> DecodeError {
    DecodeError::InvalidValue(error)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == DecodeError::Encode(error),
))]
fn map_decode_encode_error(error: EncodeError) -> DecodeError {
    DecodeError::Encode(error)
}

struct DecodeState {
    limits: ValueLimits,
    nodes: u64,
    payload_bytes: u64,
    depth: u32,
}

#[cfg(verus_keep_ghost)]
verus! {
impl DecodeState {
    pub closed spec fn invariant(&self) -> bool {
        self.nodes <= self.limits.max_nodes
        && self.payload_bytes <= self.limits.max_payload_bytes
        && self.depth <= zeno_fcis_value::decoder_model::depth_limit(self.limits.model(u64::MAX))
    }
}
}

impl DecodeState {
    fn new(limits: ValueLimits) -> Self {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Self| result.invariant()
            && result.limits == limits
            && result.nodes == 0
            && result.payload_bytes == 0
            && result.depth == 0));
        Self {
            limits,
            nodes: 0,
            payload_bytes: 0,
            depth: 0,
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).limits == old(self).limits,
            final(self).payload_bytes == old(self).payload_bytes,
            result.is_ok() == (depth <= zeno_fcis_value::decoder_model::depth_limit(old(self).limits.model(u64::MAX))
                && old(self).nodes < old(self).limits.max_nodes),
            result.is_ok() ==> final(self).nodes == old(self).nodes + 1,
            result.is_ok() ==> final(self).depth == if old(self).depth > depth { old(self).depth } else { depth },
            result.is_err() ==> *final(self) == *old(self),
    ))]
    fn enter(&mut self, depth: u32) -> Result<(), DecodeError> {
        if depth > self.limits.effective_max_depth() {
            return Err(DecodeError::DepthLimit {
                limit: self.limits.effective_max_depth(),
                attempted: depth,
            });
        }
        let nodes = self
            .nodes
            .checked_add(1)
            .ok_or(DecodeError::LengthOverflow)?;
        if nodes > self.limits.max_nodes {
            return Err(DecodeError::NodeLimit {
                limit: self.limits.max_nodes,
                attempted: nodes,
            });
        }
        self.depth = self.depth.max(depth);
        self.nodes = nodes;
        Ok(())
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).limits == old(self).limits,
            final(self).nodes == old(self).nodes, final(self).depth == old(self).depth,
            result.is_ok() == (old(self).payload_bytes + length <= old(self).limits.max_payload_bytes),
            result.is_ok() ==> final(self).payload_bytes == old(self).payload_bytes + length,
            result.is_err() ==> *final(self) == *old(self),
    ))]
    fn payload(&mut self, length: usize) -> Result<(), DecodeError> {
        let length = u64::try_from(length).map_err(|_| DecodeError::LengthOverflow)?;
        let payload_bytes = self
            .payload_bytes
            .checked_add(length)
            .ok_or(DecodeError::LengthOverflow)?;
        if payload_bytes > self.limits.max_payload_bytes {
            return Err(DecodeError::PayloadLimit {
                limit: self.limits.max_payload_bytes,
                attempted: payload_bytes,
            });
        }
        self.payload_bytes = payload_bytes;
        Ok(())
    }

    /// Children to reserve for a collection that declares `count`: no more
    /// than the remaining wire bytes can encode, and no more than the node
    /// limit still admits, since every child takes at least one node.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires self.invariant(),
        ensures result matches Ok(capacity) ==> capacity <= count
            && capacity <= self.limits.max_nodes - self.nodes,
            minimum_wire_bytes_per_item > 0 ==> result.is_ok(),
    ))]
    fn reservation(
        &self,
        count: u32,
        remaining_wire_bytes: usize,
        minimum_wire_bytes_per_item: usize,
    ) -> Result<usize, DecodeError> {
        let nodes = match usize::try_from(self.limits.max_nodes.saturating_sub(self.nodes)) {
            Ok(nodes) => nodes,
            Err(_) => usize::MAX,
        };
        Ok(
            initial_collection_capacity(count, remaining_wire_bytes, minimum_wire_bytes_per_item)?
                .min(nodes),
        )
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires self.invariant(),
        ensures result.is_ok() == (length <= self.limits.max_collection_len),
    ))]
    fn collection(&self, length: u32) -> Result<(), DecodeError> {
        if length > self.limits.max_collection_len {
            return Err(DecodeError::CollectionLimit {
                limit: self.limits.max_collection_len,
                attempted: length,
            });
        }
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(cursor).invariant(), old(state).invariant(),
    ensures final(cursor).invariant(), final(state).invariant(),
        final(cursor).bytes == old(cursor).bytes, final(state).limits == old(state).limits,
        final(cursor).offset >= old(cursor).offset,
        final(cursor).unread() == tail(old(cursor).unread(), (final(cursor).offset - old(cursor).offset) as nat),
        result matches Ok(value) ==> value.representation_valid(),
        parsed_result(&result, (final(cursor).offset - old(cursor).offset) as nat,
            final(state).nodes as nat, final(state).payload_bytes as nat)
            == decoder_spec::parse_inner(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                old(state).nodes as nat, old(state).payload_bytes as nat),
    decreases if depth <= 1024 { 1025 - depth as int } else { 0 }, 0nat,
))]
fn decode_value_inner(
    cursor: &mut Cursor<'_>,
    state: &mut DecodeState,
    depth: u32,
) -> Result<Value, DecodeError> {
    state.enter(depth)?;
    let tag = cursor.take_u8()?;
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal_with_fuel(decoder_spec::parse_inner, 2);
        assert(cursor.unread() == tail(old(cursor).unread(), 1));
    }
    match tag {
        TAG_UNIT => Ok(Value::unit()),
        TAG_BOOL_FALSE => Ok(Value::boolean(false)),
        TAG_BOOL_TRUE => Ok(Value::boolean(true)),
        TAG_U128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            #[cfg(verus_keep_ghost)]
            proof! { assert(bytes@ == old(cursor).unread().subrange(1, 17)); }
            Ok(Value::unsigned(read_word(&bytes)))
        }
        TAG_I128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            #[cfg(verus_keep_ghost)]
            proof! { assert(bytes@ == old(cursor).unread().subrange(1, 17)); }
            Ok(Value::signed(bits_to_signed(read_word(&bytes))))
        }
        TAG_BYTES => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            let owned = bytes.to_vec();
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(owned@ == bytes@);
                assert(wf(ModelValue::Bytes(bytes@), state.limits.model(0)));
            }
            Value::bytes_with_limits(owned, state.limits).map_err(map_value_decode_error)
        }
        TAG_TEXT => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            if !bytes.is_ascii() {
                return Err(DecodeError::NonAsciiText);
            }
            #[cfg(verus_keep_ghost)]
            proof! { ascii_is_utf8(bytes@); }
            let text = core::str::from_utf8(bytes).map_err(|_| DecodeError::Utf8)?;
            Value::text_ascii_with_limits(text.to_owned(), state.limits)
                .map_err(map_value_decode_error)
        }
        TAG_ENUM => {
            let type_id = cursor.take_u32()?;
            #[cfg(verus_keep_ghost)]
            proof! { assert(cursor.unread() == tail(old(cursor).unread(), 5)); }
            let variant = cursor.take_u16()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tail(old(cursor).unread(), 1).subrange(0, 4)
                    == old(cursor).unread().subrange(1, 5));
                assert(tail(old(cursor).unread(), 5).subrange(0, 2)
                    == old(cursor).unread().subrange(5, 7));
                assert(type_id == unsigned_bytes(old(cursor).unread().subrange(1, 5)) as u32);
                assert(variant == unsigned_bytes(old(cursor).unread().subrange(5, 7)) as u16);
            }
            Ok(Value::enumeration(type_id, variant))
        }
        TAG_TUPLE | TAG_VECTOR => {
            let count = cursor.take_u32()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tail(old(cursor).unread(), 1).subrange(0, 4)
                    == old(cursor).unread().subrange(1, 5));
                assert(cursor.unread() == tail(old(cursor).unread(), 5));
            }
            state.collection(count)?;
            let items = decode_items(
                cursor,
                state,
                depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                count,
            )?;
            if tag == TAG_TUPLE {
                Value::tuple(items).map_err(map_value_decode_error)
            } else {
                Value::vector(items).map_err(map_value_decode_error)
            }
        }
        TAG_RECORD => {
            let count = cursor.take_u32()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tail(old(cursor).unread(), 1).subrange(0, 4)
                    == old(cursor).unread().subrange(1, 5));
                assert(cursor.unread() == tail(old(cursor).unread(), 5));
            }
            state.collection(count)?;
            let fields = decode_fields(
                cursor,
                state,
                depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                count,
            )?;
            Value::record_canonical(fields).map_err(map_value_decode_error)
        }
        TAG_SUM => {
            let type_id = cursor.take_u32()?;
            #[cfg(verus_keep_ghost)]
            proof! { assert(cursor.unread() == tail(old(cursor).unread(), 5)); }
            let variant = cursor.take_u16()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tail(old(cursor).unread(), 1).subrange(0, 4)
                    == old(cursor).unread().subrange(1, 5));
                assert(tail(old(cursor).unread(), 5).subrange(0, 2)
                    == old(cursor).unread().subrange(5, 7));
                assert(cursor.unread() == tail(old(cursor).unread(), 7));
                assert(type_id == unsigned_bytes(old(cursor).unread().subrange(1, 5)) as u32);
                assert(variant == unsigned_bytes(old(cursor).unread().subrange(5, 7)) as u16);
            }
            let flag = cursor.take_u8()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(flag == old(cursor).unread()[7]);
                assert(cursor.unread() == tail(old(cursor).unread(), 8));
            }
            let payload = match flag {
                0 => None,
                1 => Some(decode_value_inner(
                    cursor,
                    state,
                    depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                )?),
                _ => return Err(DecodeError::InvalidSumFlag),
            };
            Ok(Value::sum(type_id, variant, payload))
        }
        TAG_MAP => {
            let count = cursor.take_u32()?;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tail(old(cursor).unread(), 1).subrange(0, 4)
                    == old(cursor).unread().subrange(1, 5));
                assert(cursor.unread() == tail(old(cursor).unread(), 5));
            }
            state.collection(count)?;
            let entries = decode_entries(
                cursor,
                state,
                depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                count,
            )?;
            Value::map_canonical(entries).map_err(map_value_decode_error)
        }
        other => Err(DecodeError::UnknownTag(other)),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(cursor).invariant(), old(state).invariant(),
    ensures final(cursor).invariant(), final(state).invariant(),
        final(cursor).bytes == old(cursor).bytes, final(state).limits == old(state).limits,
        final(cursor).offset >= old(cursor).offset,
        result matches Ok(items) ==> values_valid(items@) && items.len() == count,
        parsed_items_result(&result, (final(cursor).offset - old(cursor).offset) as nat,
            final(state).nodes as nat, final(state).payload_bytes as nat)
            == decoder_spec::parse_items(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, old(state).nodes as nat, old(state).payload_bytes as nat),
    decreases if depth <= 1024 { 1025 - depth as int } else { 0 }, 2nat,
))]
fn decode_items(
    cursor: &mut Cursor<'_>,
    state: &mut DecodeState,
    depth: u32,
    count: u32,
) -> Result<Vec<Value>, DecodeError> {
    let mut items = Vec::with_capacity(state.reservation(count, cursor.remaining(), 1)?);
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= count, items.len() == index, values_valid(items@),
            cursor.invariant(), state.invariant(), cursor.bytes == old(cursor).bytes,
            state.limits == old(state).limits, cursor.offset >= old(cursor).offset,
            decoder_spec::parse_items(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, old(state).nodes as nat, old(state).payload_bytes as nat)
            == prepend_items(values_model(items@), (cursor.offset - old(cursor).offset) as nat,
                decoder_spec::parse_items(cursor.unread(), state.limits.model(0), depth as nat,
                    (count - index) as nat, state.nodes as nat, state.payload_bytes as nat)),
        decreases count - index,
    ))]
    while index < count {
        #[cfg(verus_keep_ghost)]
        let start = cursor.offset;
        #[cfg(verus_keep_ghost)]
        let nodes_before = state.nodes;
        #[cfg(verus_keep_ghost)]
        let payload_before = state.payload_bytes;
        let child = decode_value_inner(cursor, state, depth)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            let before = old(cursor).bytes@.subrange(start as int, old(cursor).bytes@.len() as int);
            let used = (cursor.offset - start) as nat;
            let rest = decoder_spec::parse_items(cursor.unread(), state.limits.model(0), depth as nat,
                (count - index - 1) as nat, state.nodes as nat, state.payload_bytes as nat);
            assert(tail(before, used) == cursor.unread());
            assert(decoder_spec::parse_items(before, state.limits.model(0), depth as nat,
                (count - index) as nat, nodes_before as nat, payload_before as nat)
                == prepend_items(seq![child.model()], used, rest));
            prepend_items_associative(values_model(items@), (start - old(cursor).offset) as nat, child.model(), used, rest);
            values_model_push(items@, child);
        }
        items.push(child);
        index += 1;
    }
    Ok(items)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(cursor).invariant(), old(state).invariant(),
    ensures final(cursor).invariant(), final(state).invariant(),
        final(cursor).bytes == old(cursor).bytes, final(state).limits == old(state).limits,
        final(cursor).offset >= old(cursor).offset,
        result matches Ok(fields) ==> fields_valid(fields@) && fields.len() == count
            && fields_ordered(fields_model(fields@)),
        parsed_fields_result(&result, (final(cursor).offset - old(cursor).offset) as nat,
            final(state).nodes as nat, final(state).payload_bytes as nat)
            == decoder_spec::parse_fields(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, None, old(state).nodes as nat, old(state).payload_bytes as nat),
    decreases if depth <= 1024 { 1025 - depth as int } else { 0 }, 2nat,
))]
fn decode_fields(
    cursor: &mut Cursor<'_>,
    state: &mut DecodeState,
    depth: u32,
    count: u32,
) -> Result<Vec<Field>, DecodeError> {
    // A field has a u16 identifier followed by at least one value tag.
    let mut fields = Vec::with_capacity(state.reservation(count, cursor.remaining(), 3)?);
    let mut previous = None;
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= count, fields.len() == index, fields_valid(fields@),
            fields_model(fields@).len() == index, fields_ordered(fields_model(fields@)),
            previous == if index == 0 { None } else { Some(fields_model(fields@).last().0) },
            cursor.invariant(), state.invariant(), cursor.bytes == old(cursor).bytes,
            state.limits == old(state).limits, cursor.offset >= old(cursor).offset,
            decoder_spec::parse_fields(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, None, old(state).nodes as nat, old(state).payload_bytes as nat)
            == prepend_fields(fields_model(fields@), (cursor.offset - old(cursor).offset) as nat,
                decoder_spec::parse_fields(cursor.unread(), state.limits.model(0), depth as nat,
                    (count - index) as nat, previous, state.nodes as nat, state.payload_bytes as nat)),
        decreases count - index,
    ))]
    while index < count {
        #[cfg(verus_keep_ghost)]
        let start = cursor.offset;
        #[cfg(verus_keep_ghost)]
        let nodes_before = state.nodes;
        #[cfg(verus_keep_ghost)]
        let payload_before = state.payload_bytes;
        #[cfg(verus_keep_ghost)]
        let previous_before = previous;
        let id = cursor.take_u16()?;
        if match previous {
            Some(value) => value >= id,
            None => false,
        } {
            return Err(DecodeError::NonCanonicalRecord);
        }
        previous = Some(id);
        let child = decode_value_inner(cursor, state, depth)?;
        let field = Field::new(id, child);
        #[cfg(verus_keep_ghost)]
        proof! {
            fields_model_properties(fields@);
            let before = old(cursor).bytes@.subrange(start as int, old(cursor).bytes@.len() as int);
            let used = (cursor.offset - start) as nat;
            let head = (id, field.spec_value().model());
            let rest = decoder_spec::parse_fields(cursor.unread(), state.limits.model(0), depth as nat,
                (count - index - 1) as nat, Some(id), state.nodes as nat, state.payload_bytes as nat);
            assert(tail(before, used) == cursor.unread());
            assert(decoder_spec::parse_fields(before, state.limits.model(0), depth as nat,
                (count - index) as nat, previous_before, nodes_before as nat, payload_before as nat)
                == prepend_fields(seq![head], used, rest));
            prepend_fields_associative(fields_model(fields@), (start - old(cursor).offset) as nat, head, used, rest);
            fields_ordered_push(fields_model(fields@), head);
            fields_model_push(fields@, field);
        }
        fields.push(field);
        index += 1;
    }
    Ok(fields)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(cursor).invariant(), old(state).invariant(),
    ensures final(cursor).invariant(), final(state).invariant(),
        final(cursor).bytes == old(cursor).bytes, final(state).limits == old(state).limits,
        final(cursor).offset >= old(cursor).offset,
        result matches Ok(entries) ==> entries_valid(entries@) && entry_values_fit(entries@)
            && entries.len() == count && entries_ordered(entries_model(entries@)),
        parsed_entries_result(&result, (final(cursor).offset - old(cursor).offset) as nat,
            final(state).nodes as nat, final(state).payload_bytes as nat)
            == decoder_spec::parse_entries(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, None, old(state).nodes as nat, old(state).payload_bytes as nat),
    decreases if depth <= 1024 { 1025 - depth as int } else { 0 }, 2nat,
))]
// Preserve the recursive frame shape while keeping a separate proof contract.
#[inline(always)]
fn decode_entries(
    cursor: &mut Cursor<'_>,
    state: &mut DecodeState,
    depth: u32,
    count: u32,
) -> Result<Vec<MapEntry>, DecodeError> {
    // An entry has two u32 frames, each holding at least one value tag.
    let mut entries = Vec::with_capacity(state.reservation(count, cursor.remaining(), 10)?);
    let mut previous: Option<&[u8]> = None;
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= count, entries.len() == index,
            entries_valid(entries@), entry_values_fit(entries@),
            entries_model(entries@).len() == index, entries_ordered(entries_model(entries@)),
            (match previous { None => None, Some(bytes) => Some(bytes@) })
                == if index == 0 { None } else { Some(spec_encode(entries_model(entries@).last().0)) },
            cursor.invariant(), state.invariant(), cursor.bytes == old(cursor).bytes,
            state.limits == old(state).limits, cursor.offset >= old(cursor).offset,
            decoder_spec::parse_entries(old(cursor).unread(), old(state).limits.model(0), depth as nat,
                count as nat, None, old(state).nodes as nat, old(state).payload_bytes as nat)
            == prepend_entries(entries_model(entries@), (cursor.offset - old(cursor).offset) as nat,
                decoder_spec::parse_entries(cursor.unread(), state.limits.model(0), depth as nat,
                    (count - index) as nat, match previous { None => None, Some(bytes) => Some(bytes@) },
                    state.nodes as nat, state.payload_bytes as nat)),
        decreases count - index,
    ))]
    while index < count {
        #[cfg(verus_keep_ghost)]
        let start = cursor.offset;
        #[cfg(verus_keep_ghost)]
        let nodes_before = state.nodes;
        #[cfg(verus_keep_ghost)]
        let payload_before = state.payload_bytes;
        #[cfg(verus_keep_ghost)]
        let previous_before = previous;
        let encoded_key = cursor.take_blob(state.limits.max_payload_bytes)?;
        #[cfg(verus_keep_ghost)]
        let key_end = cursor.offset;
        state.payload(encoded_key.len())?;
        if match previous {
            Some(value) => !bytes_before(value, encoded_key),
            None => false,
        } {
            return Err(DecodeError::NonCanonicalMap);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(previous matches Some(bytes) ==> lex_less(bytes@, encoded_key@));
        }
        let encoded_value = cursor.take_blob(state.limits.max_payload_bytes)?;
        state.payload(encoded_value.len())?;
        let key = decode_complete_value(encoded_key, state, depth)?;
        let value = decode_complete_value(encoded_value, state, depth)?;
        let entry = MapEntry::try_new_with_limits(key, value, state.limits)
            .map_err(map_value_decode_error)?;
        if entry.encoded_key() != encoded_key {
            return Err(DecodeError::NonCanonical);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            entries_model_properties(entries@);
            let before = old(cursor).bytes@.subrange(start as int, old(cursor).bytes@.len() as int);
            let used = (cursor.offset - start) as nat;
            let head = (entry.spec_key().model(), entry.spec_value().model());
            let rest = decoder_spec::parse_entries(cursor.unread(), state.limits.model(0), depth as nat,
                (count - index - 1) as nat, Some(encoded_key@), state.nodes as nat, state.payload_bytes as nat);
            assert(tail(before, used) == cursor.unread());
            assert(read_blob(before, state.limits.max_payload_bytes as nat)
                == Some((encoded_key@, (key_end - start) as nat)));
            assert(decoder_spec::parse_entries(before, state.limits.model(0), depth as nat,
                (count - index) as nat, match previous_before { None => None, Some(bytes) => Some(bytes@) },
                nodes_before as nat, payload_before as nat)
                == prepend_entries(seq![head], used, rest));
            prepend_entries_associative(entries_model(entries@), (start - old(cursor).offset) as nat, head, used, rest);
            entries_ordered_push(entries_model(entries@), head);
            entries_model_push(entries@, entry);
        }
        previous = Some(encoded_key);
        entries.push(entry);
        index += 1;
    }
    Ok(entries)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result matches Ok(capacity) ==> capacity <= count && capacity <= remaining_wire_bytes,
        minimum_wire_bytes_per_item > 0 ==> result.is_ok(),
))]
fn initial_collection_capacity(
    count: u32,
    remaining_wire_bytes: usize,
    minimum_wire_bytes_per_item: usize,
) -> Result<usize, DecodeError> {
    let count = usize::try_from(count).map_err(|_| DecodeError::LengthOverflow)?;
    let wire_bound = remaining_wire_bytes
        .checked_div(minimum_wire_bytes_per_item)
        .ok_or(DecodeError::LengthOverflow)?;
    Ok(count.min(wire_bound))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a> Cursor<'a> {
    pub closed spec fn invariant(&self) -> bool { self.offset <= self.bytes@.len() }
    pub closed spec fn consumed(&self) -> Seq<u8> { self.bytes@.subrange(0, self.offset as int) }
    pub closed spec fn unread(&self) -> Seq<u8> { self.bytes@.subrange(self.offset as int, self.bytes@.len() as int) }
}
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Self| result.invariant() && result.bytes == bytes && result.offset == 0
            && result.consumed() == Seq::<u8>::empty() && result.unread() == bytes@));
        Self { bytes, offset: 0 }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires self.invariant(),
        ensures result == self.bytes.len() - self.offset,
            result == self.unread().len(), self.bytes@ == self.consumed() + self.unread(),
    ))]
    const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).bytes == old(self).bytes,
            result.is_ok() == (old(self).offset + length <= old(self).bytes.len()),
            result matches Ok(bytes) ==> bytes@ == old(self).bytes@.subrange(old(self).offset as int, old(self).offset + length)
                && final(self).offset == old(self).offset + length
                && bytes@ == old(self).unread().subrange(0, length as int)
                && final(self).unread() == tail(old(self).unread(), length as nat),
            result.is_err() ==> final(self).offset == old(self).offset,
    ))]
    fn take(&mut self, length: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(DecodeError::LengthOverflow)?;
        let Some(value) = self.bytes.get(self.offset..end) else {
            return Err(DecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: length,
            });
        };
        self.offset = end;
        Ok(value)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).bytes == old(self).bytes,
            result.is_ok() == (old(self).offset + 1 <= old(self).bytes.len()),
            result matches Ok(word) ==> word as u128 == unsigned_bytes(old(self).bytes@.subrange(old(self).offset as int, old(self).offset + 1))
                && final(self).offset == old(self).offset + 1
                && word == old(self).unread()[0]
                && final(self).unread() == tail(old(self).unread(), 1),
            result.is_err() ==> final(self).offset == old(self).offset,
    ))]
    fn take_u8(&mut self) -> Result<u8, DecodeError> {
        let bytes = self.take(1)?;
        #[cfg(verus_keep_ghost)]
        proof! { zeno_fcis_value::decoder_model::unsigned_small(bytes@); }
        Ok(bytes[0])
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).bytes == old(self).bytes,
            result.is_ok() == (old(self).offset + 2 <= old(self).bytes.len()),
            result matches Ok(word) ==> word as u128 == unsigned_bytes(old(self).bytes@.subrange(old(self).offset as int, old(self).offset + 2))
                && final(self).offset == old(self).offset + 2
                && word as u128 == unsigned_bytes(old(self).unread().subrange(0, 2))
                && final(self).unread() == tail(old(self).unread(), 2),
            result.is_err() ==> final(self).offset == old(self).offset,
    ))]
    fn take_u16(&mut self) -> Result<u16, DecodeError> {
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(self.take(2)?);
        #[cfg(verus_keep_ghost)]
        proof! { zeno_fcis_value::decoder_model::unsigned_small(bytes@); }
        Ok(read_word(&bytes) as u16)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).bytes == old(self).bytes,
            result.is_ok() == (old(self).offset + 4 <= old(self).bytes.len()),
            result matches Ok(word) ==> word as u128 == unsigned_bytes(old(self).bytes@.subrange(old(self).offset as int, old(self).offset + 4))
                && final(self).offset == old(self).offset + 4
                && word as u128 == unsigned_bytes(old(self).unread().subrange(0, 4))
                && final(self).unread() == tail(old(self).unread(), 4),
            result.is_err() ==> final(self).offset == old(self).offset,
    ))]
    fn take_u32(&mut self) -> Result<u32, DecodeError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        #[cfg(verus_keep_ghost)]
        proof! { zeno_fcis_value::decoder_model::unsigned_small(bytes@); }
        Ok(read_word(&bytes) as u32)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        requires old(self).invariant(),
        ensures final(self).invariant(), final(self).bytes == old(self).bytes,
            final(self).offset >= old(self).offset,
            result matches Ok(bytes) ==> bytes@.len() <= maximum && bytes@.len() <= u32::MAX
                && read_blob(old(self).unread(), maximum as nat)
                    == Some((bytes@, (final(self).offset - old(self).offset) as nat))
                && final(self).unread() == tail(old(self).unread(), (final(self).offset - old(self).offset) as nat),
            result.is_err() ==> read_blob(old(self).unread(), maximum as nat) is None,
    ))]
    fn take_blob(&mut self, maximum: u64) -> Result<&'a [u8], DecodeError> {
        let length = self.take_u32()?;
        if u64::from(length) > maximum {
            return Err(DecodeError::BlobLimit {
                limit: maximum,
                attempted: u64::from(length),
            });
        }
        let length = usize::try_from(length).map_err(|_| DecodeError::LengthOverflow)?;
        self.take(length)
    }
}

/// Canonical encoding failure.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
#[non_exhaustive]
pub enum EncodeError {
    /// A collection cannot be represented by the canonical length field.
    LengthOverflow,
    /// The initial text profile accepts ASCII only.
    NonAsciiText,
    /// Record fields are duplicate or out of order.
    NonCanonicalRecord,
    /// Map keys are duplicate or out of order.
    NonCanonicalMap,
    /// A stored encoded map key does not match its semantic key.
    MapKeyMismatch,
    /// Domain names must be non-empty bounded ASCII.
    InvalidDomain,
    /// A complete admitted envelope exceeds its supplied decoder input limit.
    EnvelopeInputLimit {
        /// Reviewed input limit.
        limit: u64,
        /// Attempted complete envelope bytes.
        attempted: u64,
    },
    /// The value itself violates closed-value invariants.
    InvalidValue(ValueError),
    /// A protocol variant is unknown to this encoding version.
    UnsupportedProtocolVariant(&'static str),
}

#[cfg(not(verus_keep_ghost))]
impl core::error::Error for EncodeError {}

#[cfg(not(verus_keep_ghost))]
impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthOverflow => formatter.write_str("canonical length overflow"),
            Self::NonAsciiText => formatter.write_str("canonical text is not ASCII"),
            Self::NonCanonicalRecord => formatter.write_str("record fields are not canonical"),
            Self::NonCanonicalMap => formatter.write_str("map entries are not canonical"),
            Self::MapKeyMismatch => {
                formatter.write_str("map encoded key does not match semantic key")
            }
            Self::InvalidDomain => formatter.write_str("invalid domain-separation tag"),
            Self::EnvelopeInputLimit { limit, attempted } => write!(
                formatter,
                "canonical envelope bytes {attempted} exceeds input limit {limit}"
            ),
            Self::InvalidValue(error) => error.fmt(formatter),
            Self::UnsupportedProtocolVariant(name) => {
                write!(formatter, "unsupported {name} protocol variant")
            }
        }
    }
}

/// Canonical decoding failure.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
#[non_exhaustive]
pub enum DecodeError {
    /// Input exceeds the global byte limit.
    InputLimit {
        /// Configured limit.
        limit: u64,
        /// Actual input bytes.
        actual: u64,
    },
    /// A length conversion or counter overflowed.
    LengthOverflow,
    /// Input ended before a complete value was available.
    UnexpectedEnd {
        /// Byte offset.
        offset: usize,
        /// Requested byte count.
        requested: usize,
    },
    /// Bytes remained after a complete value.
    TrailingBytes {
        /// First trailing byte offset.
        offset: usize,
    },
    /// Unknown value tag.
    UnknownTag(u8),
    /// Text is invalid UTF-8.
    Utf8,
    /// Text violates the ASCII profile.
    NonAsciiText,
    /// Sum payload flag is not zero or one.
    InvalidSumFlag,
    /// Record fields are duplicate or out of order.
    NonCanonicalRecord,
    /// Map keys are duplicate or out of order.
    NonCanonicalMap,
    /// Input has an alternate noncanonical encoding.
    NonCanonical,
    /// Envelope magic does not match ZCVE/1.
    EnvelopeMagic,
    /// A nested blob exceeds a declared limit.
    BlobLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted length.
        attempted: u64,
    },
    /// Recursive nesting exceeded its limit.
    DepthLimit {
        /// Configured limit.
        limit: u32,
        /// Attempted depth.
        attempted: u32,
    },
    /// Total node count exceeded its limit.
    NodeLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted node count.
        attempted: u64,
    },
    /// Aggregate payload bytes exceeded the limit.
    PayloadLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted payload bytes.
        attempted: u64,
    },
    /// A collection exceeded its item limit.
    CollectionLimit {
        /// Configured limit.
        limit: u32,
        /// Attempted item count.
        attempted: u32,
    },
    /// Reconstructed value violates closed-value invariants.
    InvalidValue(ValueError),
    /// Re-encoding failed.
    Encode(EncodeError),
}

#[cfg(not(verus_keep_ghost))]
impl core::error::Error for DecodeError {}

#[cfg(not(verus_keep_ghost))]
impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { limit, actual } => {
                write!(formatter, "input bytes {actual} exceeds limit {limit}")
            }
            Self::LengthOverflow => formatter.write_str("decoded length overflow"),
            Self::UnexpectedEnd { offset, requested } => write!(
                formatter,
                "unexpected end at offset {offset}; requested {requested} bytes"
            ),
            Self::TrailingBytes { offset } => {
                write!(formatter, "trailing bytes at offset {offset}")
            }
            Self::UnknownTag(tag) => write!(formatter, "unknown value tag {tag}"),
            Self::Utf8 => formatter.write_str("invalid UTF-8"),
            Self::NonAsciiText => formatter.write_str("decoded text is not ASCII"),
            Self::InvalidSumFlag => formatter.write_str("invalid sum payload flag"),
            Self::NonCanonicalRecord => formatter.write_str("record fields are not canonical"),
            Self::NonCanonicalMap => formatter.write_str("map entries are not canonical"),
            Self::NonCanonical => formatter.write_str("noncanonical alternate encoding"),
            Self::EnvelopeMagic => formatter.write_str("invalid ZCVE/1 envelope magic"),
            Self::BlobLimit { limit, attempted } => {
                write!(formatter, "blob length {attempted} exceeds limit {limit}")
            }
            Self::DepthLimit { limit, attempted } => {
                write!(formatter, "depth {attempted} exceeds limit {limit}")
            }
            Self::NodeLimit { limit, attempted } => {
                write!(formatter, "node count {attempted} exceeds limit {limit}")
            }
            Self::PayloadLimit { limit, attempted } => {
                write!(formatter, "payload bytes {attempted} exceeds limit {limit}")
            }
            Self::CollectionLimit { limit, attempted } => {
                write!(
                    formatter,
                    "collection length {attempted} exceeds limit {limit}"
                )
            }
            Self::InvalidValue(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
use RustCryptoSha256 as TestHasher;

/// Pure canonical commitments and path updates for opaque-key collections.
///
/// Empty subtrees and singleton leaves collapse; branches with two or more
/// entries retain their key-bit split. Sibling summaries are supplied by storage,
/// not authenticated by this utility. No database or I/O enters a path update.
pub mod collection_merkle {
    use crate::{CommitmentHasher, RustCryptoSha256};
    use alloc::vec::Vec;

    /// An opaque collection key.
    pub type Key = [u8; 32];
    /// A SHA-256 commitment.
    pub type Digest = [u8; 32];
    /// The fixed maximum path length.
    pub const KEY_BITS: usize = 256;

    /// The only cardinality distinction needed to collapse a subtree.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Subtree {
        /// No entries.
        Empty,
        /// Exactly one keyed leaf.
        Leaf(Digest),
        /// At least two entries, split at this prefix's next key bit.
        Branch(Digest),
    }
    impl Subtree {
        fn digest_with(self, empty: &Digest) -> Digest {
            match self {
                Self::Empty => *empty,
                Self::Leaf(hash) | Self::Branch(hash) => hash,
            }
        }
        /// The canonical root represented by this summary.
        #[must_use]
        pub fn digest(self) -> Digest {
            self.digest_with(&empty_hash())
        }
    }

    /// One node row to insert/update, or remove when its subtree has fewer than two entries.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NodeWrite {
        /// Key bits fixed by this prefix, from0 through255.
        pub depth: usize,
        /// Prefix bits followed by zero bits.
        pub prefix: Key,
        /// Branch hash, or removal.
        pub hash: Option<Digest>,
    }
    /// Owned results of one pure entry update. This is not a publication capability.
    #[must_use]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PathUpdate {
        /// The new keyed leaf, or removal.
        pub leaf: Option<Digest>,
        /// The canonical root after applying this path update.
        pub root: Digest,
        /// Node writes in deepest-first order.
        pub nodes: Vec<NodeWrite>,
    }

    /// The empty collection/subtree commitment, preserving the existing domain.
    #[must_use]
    pub fn empty_hash() -> Digest {
        hash(&[b"ZFCIS-MAP-EMPTY\0"])
    }
    /// A keyed record's leaf commitment, preserving the existing domains and byte order.
    #[must_use]
    pub fn leaf_hash(key: &Key, record: &[u8]) -> Digest {
        hash(&[b"ZFCIS-MAP-LEAF\0", key, &hash(&[record])])
    }
    /// An ordered branch commitment.
    #[must_use]
    pub fn node_hash(left: &Digest, right: &Digest) -> Digest {
        hash(&[b"ZFCIS-MAP-NODE\0", left, right])
    }
    fn hash(parts: &[&[u8]]) -> Digest {
        *RustCryptoSha256::hash_parts(parts).as_bytes()
    }
    fn bit(key: &Key, depth: usize) -> u8 {
        (key[depth / 8] >> (7 - depth % 8)) & 1
    }
    /// First `depth` key bits followed by zeros; depths above256 clamp to256.
    #[must_use]
    pub fn prefix(key: &Key, depth: usize) -> Key {
        let depth = depth.min(KEY_BITS);
        let (whole, remainder) = (depth / 8, depth % 8);
        let mut out = [0; 32];
        out[..whole].copy_from_slice(&key[..whole]);
        if remainder != 0 {
            out[whole] = key[whole] & (u8::MAX << (8 - remainder));
        }
        out
    }
    /// Inclusive least/greatest keys with this prefix, with depth clamped to256.
    #[must_use]
    pub fn bounds(key: &Key, depth: usize) -> (Key, Key) {
        let depth = depth.min(KEY_BITS);
        let low = prefix(key, depth);
        let mut high = low;
        let (whole, remainder) = (depth / 8, depth % 8);
        if remainder == 0 {
            high[whole..].fill(u8::MAX);
        } else {
            high[whole] |= u8::MAX >> remainder;
            high[whole + 1..].fill(u8::MAX);
        }
        (low, high)
    }
    /// Prefix of the other child at `depth`; refuses a depth outside the key.
    #[must_use]
    pub fn sibling_prefix(key: &Key, depth: usize) -> Option<Key> {
        if depth >= KEY_BITS {
            return None;
        }
        let mut other = prefix(key, depth + 1);
        other[depth / 8] ^= 1 << (7 - depth % 8);
        Some(other)
    }
    /// Equal leading bits, from0 through256.
    #[must_use]
    pub fn shared_bits(a: &Key, b: &Key) -> usize {
        (0..KEY_BITS)
            .take_while(|i| bit(a, *i) == bit(b, *i))
            .count()
    }
    /// Necessary path length from the indexed predecessor/successor (or all other keys).
    /// Equal target keys are ignored; the result is at most256.
    #[must_use]
    pub fn shared_path_length(key: &Key, neighbours: &[Key]) -> usize {
        neighbours
            .iter()
            .filter(|other| *other != key)
            .map(|other| shared_bits(key, other) + 1)
            .max()
            .unwrap_or(0)
    }
    fn combine(left: Subtree, right: Subtree, empty: &Digest) -> Subtree {
        match (left, right) {
            (Subtree::Empty, Subtree::Empty) => Subtree::Empty,
            (Subtree::Empty, Subtree::Leaf(hash)) | (Subtree::Leaf(hash), Subtree::Empty) => {
                Subtree::Leaf(hash)
            }
            _ => Subtree::Branch(node_hash(
                &left.digest_with(empty),
                &right.digest_with(empty),
            )),
        }
    }
    /// Update one key from root-to-leaf sibling summaries, without changing any input.
    ///
    /// The path ends below the deepest prefix shared with another entry. Below
    /// that point the changed subtree is empty or the new leaf. The caller must
    /// supply siblings from that same pre-state; this function does not authenticate
    /// storage. A path longer than256 bits is refused before allocating its plan.
    #[must_use]
    pub fn update_path(
        key: &Key,
        record: Option<&[u8]>,
        siblings: &[Subtree],
    ) -> Option<PathUpdate> {
        if siblings.len() > KEY_BITS {
            return None;
        }
        let empty = empty_hash();
        let leaf = record.map(|record| leaf_hash(key, record));
        let mut child = leaf.map_or(Subtree::Empty, Subtree::Leaf);
        let mut nodes = Vec::with_capacity(siblings.len());
        for (depth, other) in siblings.iter().enumerate().rev() {
            let (left, right) = if bit(key, depth) == 0 {
                (child, *other)
            } else {
                (*other, child)
            };
            child = combine(left, right, &empty);
            let hash = match child {
                Subtree::Branch(hash) => Some(hash),
                _ => None,
            };
            nodes.push(NodeWrite {
                depth,
                prefix: prefix(key, depth),
                hash,
            });
        }
        Some(PathUpdate {
            leaf,
            root: child.digest_with(&empty),
            nodes,
        })
    }

    /// Reference rebuild from all entries. Duplicate keys retain their first record.
    #[must_use]
    pub fn rebuild_root(entries: &[(Key, Vec<u8>)]) -> Digest {
        let mut leaves: Vec<(Key, Digest)> = entries
            .iter()
            .map(|(key, record)| (*key, leaf_hash(key, record)))
            .collect();
        leaves.sort_by_key(|(key, _)| *key);
        leaves.dedup_by_key(|(key, _)| *key);
        rebuild(&leaves, 0)
    }
    fn rebuild(leaves: &[(Key, Digest)], depth: usize) -> Digest {
        match leaves {
            [] => empty_hash(),
            [(_, leaf)] => *leaf,
            _ => {
                let split = leaves.partition_point(|(key, _)| bit(key, depth) == 0);
                node_hash(
                    &rebuild(&leaves[..split], depth + 1),
                    &rebuild(&leaves[split..], depth + 1),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// One value per wire tag, including both sum payload flags and a map with
    /// more than one entry.
    fn tag_corpus() -> Vec<Value> {
        let entry = |key: Value, value: Value| {
            MapEntry::try_new(key, value).unwrap_or_else(|error| panic!("map entry: {error}"))
        };
        vec![
            Value::unit(),
            Value::boolean(false),
            Value::boolean(true),
            Value::unsigned(0),
            Value::unsigned(u128::MAX),
            Value::signed(i128::MIN),
            Value::bytes(vec![1, 2]).unwrap_or_else(|error| panic!("bytes: {error}")),
            Value::text_ascii(String::from("abc")).unwrap_or_else(|error| panic!("text: {error}")),
            Value::enumeration(4, 5),
            Value::tuple(vec![Value::unit(), Value::boolean(true)])
                .unwrap_or_else(|error| panic!("collection: {error}")),
            Value::vector(vec![Value::unsigned(1), Value::unsigned(2)])
                .unwrap_or_else(|error| panic!("collection: {error}")),
            Value::record_canonical(vec![
                Field::new(1, Value::unit()),
                Field::new(2, Value::signed(-1)),
            ])
            .unwrap_or_else(|error| panic!("record: {error}")),
            Value::sum(6, 7, None),
            Value::sum(6, 7, Some(Value::boolean(false))),
            Value::map_canonical(vec![
                entry(Value::unsigned(1), Value::boolean(true)),
                entry(
                    Value::unsigned(2),
                    Value::tuple(vec![Value::unit()])
                        .unwrap_or_else(|error| panic!("collection: {error}")),
                ),
            ])
            .unwrap_or_else(|error| panic!("map: {error}")),
        ]
    }

    fn envelope_bytes(value: Value) -> Vec<u8> {
        Envelope::new(7, Hash32::new([3; 32]), value)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("envelope bytes: {error}"))
    }

    /// Frames an arbitrary, possibly malformed, payload as an envelope.
    fn framed_payload(payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ENVELOPE_MAGIC);
        bytes.extend_from_slice(&7_u32.to_be_bytes());
        bytes.extend_from_slice(&[3_u8; 32]);
        bytes.extend_from_slice(
            &u32::try_from(payload.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        bytes.extend_from_slice(payload);
        bytes
    }

    fn blob(bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
        output.extend_from_slice(bytes);
        output
    }

    fn map_payload(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut output = vec![TAG_MAP];
        output.extend_from_slice(
            &u32::try_from(entries.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        for (key, value) in entries {
            output.extend_from_slice(&blob(key));
            output.extend_from_slice(&blob(value));
        }
        output
    }

    fn unsigned_key(value: u128) -> Vec<u8> {
        let mut output = vec![TAG_U128];
        output.extend_from_slice(&value.to_be_bytes());
        output
    }

    fn structural_limits(value: ValueLimits) -> DecodeLimits {
        DecodeLimits {
            value,
            ..DecodeLimits::default()
        }
    }

    #[test]
    fn every_value_tag_round_trips_in_a_value_and_an_envelope() {
        for value in tag_corpus() {
            let bytes = value
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("value bytes: {error}"));
            assert_eq!(
                decode_value(&bytes, DecodeLimits::default()),
                Ok(value.clone())
            );
            let envelope = Envelope::new(7, Hash32::new([3; 32]), value);
            let bytes = envelope
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("envelope bytes: {error}"));
            assert_eq!(
                decode_envelope(&bytes, DecodeLimits::default()),
                Ok(envelope)
            );
        }
    }

    #[test]
    fn decoded_values_and_envelopes_always_re_encode_to_their_input() {
        // Dropping the envelope's second re-encode is behavior preserving only
        // because every accepted decode reproduces its input exactly. Byte
        // mutations drive the parse, limit, and canonical-order guards that
        // remain, and no accepted result may differ from its input.
        let originals: Vec<Vec<u8>> = tag_corpus().into_iter().map(envelope_bytes).collect();
        let mut inputs = originals.clone();
        for original in &originals {
            for index in 0..original.len() {
                for delta in [1_u8, 0x7f, 0xff] {
                    let mut mutated = original.clone();
                    mutated[index] = mutated[index].wrapping_add(delta);
                    inputs.push(mutated);
                }
                let mut truncated = original.clone();
                truncated.truncate(index);
                inputs.push(truncated);
                let mut extended = original.clone();
                extended.insert(index, 0);
                inputs.push(extended);
            }
        }
        let mut accepted_envelopes = 0_u32;
        for input in inputs {
            if let Ok(envelope) = decode_envelope(&input, DecodeLimits::default()) {
                accepted_envelopes += 1;
                assert_eq!(envelope.canonical_bytes(), Ok(input.clone()));
            }
            if let Ok(value) = decode_value(&input, DecodeLimits::default()) {
                assert_eq!(value.canonical_bytes(), Ok(input));
            }
        }
        // Mutation alone must not silence the corpus.
        assert!(accepted_envelopes >= 15);
    }

    #[test]
    fn envelope_byte_mutations_report_exact_errors() {
        let limits = DecodeLimits::default();
        let base = envelope_bytes(Value::unsigned(9));
        let length_offset = ENVELOPE_MAGIC.len() + 4 + 32;
        let payload_length = base.len() - length_offset - 4;

        let mut wrong_magic = base.clone();
        wrong_magic[0] ^= 0xff;
        assert_eq!(
            decode_envelope(&wrong_magic, limits),
            Err(DecodeError::EnvelopeMagic)
        );

        let mut extra_byte = base.clone();
        extra_byte.push(0);
        assert_eq!(
            decode_envelope(&extra_byte, limits),
            Err(DecodeError::TrailingBytes { offset: base.len() })
        );

        let mut long_payload = base.clone();
        long_payload[length_offset..length_offset + 4].copy_from_slice(
            &u32::try_from(payload_length + 1)
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        assert_eq!(
            decode_envelope(&long_payload, limits),
            Err(DecodeError::UnexpectedEnd {
                offset: length_offset + 4,
                requested: payload_length + 1,
            })
        );

        let mut short_payload = base.clone();
        short_payload[length_offset..length_offset + 4].copy_from_slice(
            &u32::try_from(payload_length - 1)
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        assert_eq!(
            decode_envelope(&short_payload, limits),
            Err(DecodeError::TrailingBytes {
                offset: base.len() - 1,
            })
        );

        let mut swapped_fields = vec![TAG_RECORD];
        swapped_fields.extend_from_slice(&2_u32.to_be_bytes());
        swapped_fields.extend_from_slice(&2_u16.to_be_bytes());
        swapped_fields.push(TAG_UNIT);
        swapped_fields.extend_from_slice(&1_u16.to_be_bytes());
        swapped_fields.push(TAG_UNIT);
        assert_eq!(
            decode_envelope(&framed_payload(&swapped_fields), limits),
            Err(DecodeError::NonCanonicalRecord)
        );

        let high_key = unsigned_key(2);
        let low_key = unsigned_key(1);
        let swapped_keys = map_payload(&[
            (&high_key[..], &[TAG_BOOL_TRUE][..]),
            (&low_key[..], &[TAG_BOOL_TRUE][..]),
        ]);
        assert_eq!(
            decode_envelope(&framed_payload(&swapped_keys), limits),
            Err(DecodeError::NonCanonicalMap)
        );

        let mut sum_flag = vec![TAG_SUM];
        sum_flag.extend_from_slice(&6_u32.to_be_bytes());
        sum_flag.extend_from_slice(&7_u16.to_be_bytes());
        sum_flag.push(2);
        assert_eq!(
            decode_envelope(&framed_payload(&sum_flag), limits),
            Err(DecodeError::InvalidSumFlag)
        );

        let mut wide_text = vec![TAG_TEXT];
        wide_text.extend_from_slice(&1_u32.to_be_bytes());
        wide_text.push(0xff);
        assert_eq!(
            decode_envelope(&framed_payload(&wide_text), limits),
            Err(DecodeError::NonAsciiText)
        );

        assert_eq!(
            decode_envelope(&framed_payload(&[0xd0_u8]), limits),
            Err(DecodeError::UnknownTag(0xd0))
        );
    }

    #[test]
    fn nested_map_blobs_share_the_parent_depth_node_and_payload_budgets() {
        let entry = MapEntry::try_new(
            Value::unsigned(1),
            Value::tuple(vec![Value::unit()]).unwrap_or_else(|error| panic!("collection: {error}")),
        )
        .unwrap_or_else(|error| panic!("map entry: {error}"));
        let nested =
            Value::map_canonical(vec![entry]).unwrap_or_else(|error| panic!("nested map: {error}"));
        let bytes = nested
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("nested map bytes: {error}"));

        // The map is depth zero, its key and value blobs decode at depth one,
        // and the value's own child reaches depth two on the parent's budget.
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_depth: 1,
                    ..ValueLimits::default()
                })
            ),
            Err(DecodeError::DepthLimit {
                limit: 1,
                attempted: 2,
            })
        );
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_depth: 2,
                    ..ValueLimits::default()
                })
            ),
            Ok(nested.clone())
        );

        // Map, key, value, and the value's child are four nodes on one counter.
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_nodes: 3,
                    ..ValueLimits::default()
                })
            ),
            Err(DecodeError::NodeLimit {
                limit: 3,
                attempted: 4,
            })
        );
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_nodes: 4,
                    ..ValueLimits::default()
                })
            ),
            Ok(nested)
        );

        // Both raw entry blobs are charged, and the byte payload inside each is
        // charged again: seven plus seven raw bytes plus two plus two owned.
        let leaves = MapEntry::try_new(
            Value::bytes(vec![b'a', b'b']).unwrap_or_else(|error| panic!("key bytes: {error}")),
            Value::bytes(vec![b'c', b'd']).unwrap_or_else(|error| panic!("value bytes: {error}")),
        )
        .unwrap_or_else(|error| panic!("leaf entry: {error}"));
        let leaves =
            Value::map_canonical(vec![leaves]).unwrap_or_else(|error| panic!("leaf map: {error}"));
        let bytes = leaves
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("leaf map bytes: {error}"));
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_payload_bytes: 17,
                    ..ValueLimits::default()
                })
            ),
            Err(DecodeError::PayloadLimit {
                limit: 17,
                attempted: 18,
            })
        );
        assert_eq!(
            decode_value(
                &bytes,
                structural_limits(ValueLimits {
                    max_payload_bytes: 18,
                    ..ValueLimits::default()
                })
            ),
            Ok(leaves)
        );
    }

    #[test]
    fn map_entry_checks_keep_their_order_from_raw_blob_to_value() {
        let limits = DecodeLimits::default();
        let key = unsigned_key(1);
        let trailing_value = [TAG_BOOL_TRUE, 0];
        // A nested blob is decoded with its own cursor, so its trailing-byte
        // offset stays relative to that blob.
        assert_eq!(
            decode_value(&map_payload(&[(&key[..], &trailing_value[..])]), limits),
            Err(DecodeError::TrailingBytes { offset: 1 })
        );

        let mut trailing_key = unsigned_key(1);
        trailing_key.push(0);
        assert_eq!(
            decode_value(
                &map_payload(&[(&trailing_key[..], &[TAG_BOOL_TRUE][..])]),
                limits
            ),
            Err(DecodeError::TrailingBytes { offset: 17 })
        );

        // A malformed key is decided before a malformed value.
        assert_eq!(
            decode_value(&map_payload(&[(&trailing_key[..], &[0xd0_u8][..])]), limits),
            Err(DecodeError::TrailingBytes { offset: 17 })
        );
        assert_eq!(
            decode_value(&map_payload(&[(&key[..], &[0xd0_u8][..])]), limits),
            Err(DecodeError::UnknownTag(0xd0))
        );

        // Both raw blobs are charged before either is decoded, so an exhausted
        // payload budget is reported ahead of the malformed value.
        assert_eq!(
            decode_value(
                &map_payload(&[(&key[..], &trailing_value[..])]),
                structural_limits(ValueLimits {
                    max_payload_bytes: 18,
                    ..ValueLimits::default()
                })
            ),
            Err(DecodeError::PayloadLimit {
                limit: 18,
                attempted: 19,
            })
        );
    }

    #[test]
    fn value_round_trip_is_exact() {
        let bytes = Value::bytes(vec![1, 2]).unwrap_or_else(|error| panic!("bytes: {error}"));
        let value = Value::record_canonical(vec![
            Field::new(1, Value::unsigned(42)),
            Field::new(2, Value::boolean(true)),
            Field::new(
                3,
                Value::tuple(vec![Value::signed(-7), bytes])
                    .unwrap_or_else(|error| panic!("collection: {error}")),
            ),
        ]);
        assert!(value.is_ok());
        let value = match value {
            Ok(value) => value,
            Err(error) => panic!("unexpected value error: {error}"),
        };
        let bytes = value.canonical_bytes();
        assert!(bytes.is_ok());
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => panic!("unexpected encode error: {error}"),
        };
        assert_eq!(decode_value(&bytes, DecodeLimits::default()), Ok(value));
    }

    #[test]
    fn admitted_value_encoding_matches_raw_and_is_repeatable() {
        let value = Value::tuple(vec![Value::unsigned(42), Value::boolean(true)])
            .unwrap_or_else(|error| panic!("collection: {error}"));
        let raw = value
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("raw encoding: {error}"));
        let admitted =
            AdmittedValue::try_new(value).unwrap_or_else(|error| panic!("admission: {error}"));
        let first = admitted
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("first encoding: {error}"));
        let second = admitted
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("second encoding: {error}"));

        assert_eq!(first, raw);
        assert_eq!(second, raw);
    }

    #[test]
    fn raw_value_encoding_remains_fail_closed() {
        assert_eq!(
            Value::text_ascii(String::from("é")),
            Err(zeno_fcis_value::TextError::NonAscii)
        );
    }

    #[test]
    fn map_order_is_encoded_key_order() {
        let key_one = Value::unsigned(1);
        let key_two = Value::unsigned(2);
        let entry_one = MapEntry::try_new(key_one, Value::boolean(true));
        let entry_two = MapEntry::try_new(key_two, Value::boolean(false));
        assert!(entry_one.is_ok() && entry_two.is_ok());
        let entries = vec![
            entry_one.unwrap_or_else(|error| panic!("map entry: {error}")),
            entry_two.unwrap_or_else(|error| panic!("map entry: {error}")),
        ];
        let map = Value::map_canonical(entries);
        assert!(map.is_ok());
        let map = match map {
            Ok(map) => map,
            Err(error) => panic!("unexpected map error: {error}"),
        };
        let bytes = map.canonical_bytes();
        assert!(bytes.is_ok());
        let bytes = bytes.unwrap_or_default();
        assert_eq!(decode_value(&bytes, DecodeLimits::default()), Ok(map));
    }

    #[test]
    fn map_encoding_preserves_the_zcve_v1_golden_bytes() {
        let entry = MapEntry::try_new(Value::unsigned(1), Value::boolean(true));
        assert!(entry.is_ok());
        let map = Value::map_canonical(vec![
            entry.unwrap_or_else(|error| panic!("map entry: {error}")),
        ]);
        assert!(map.is_ok());
        let bytes = map
            .unwrap_or_else(|error| panic!("map value: {error}"))
            .canonical_bytes();
        assert_eq!(
            bytes,
            Ok(vec![
                0x0c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x11, 0x03, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
                0x00, 0x01, 0x02,
            ])
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = Value::boolean(true).canonical_bytes().unwrap_or_default();
        bytes.push(0);
        assert!(matches!(
            decode_value(&bytes, DecodeLimits::default()),
            Err(DecodeError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn collection_reservation_is_bounded_by_remaining_wire_bytes() {
        assert_eq!(initial_collection_capacity(1_000_000, 0, 1), Ok(0));
        assert_eq!(initial_collection_capacity(1_000_000, 7, 1), Ok(7));
        assert_eq!(initial_collection_capacity(1_000_000, 11, 3), Ok(3));
        assert_eq!(initial_collection_capacity(1_000_000, 29, 10), Ok(2));
        assert_eq!(initial_collection_capacity(2, 100, 1), Ok(2));
        assert_eq!(
            initial_collection_capacity(1, 1, 0),
            Err(DecodeError::LengthOverflow)
        );
    }

    #[test]
    fn collection_reservation_is_bounded_by_the_node_limit() {
        let mut state = DecodeState::new(ValueLimits {
            max_nodes: 2,
            ..ValueLimits::default()
        });
        assert_eq!(state.reservation(1_000_000, 1_000_000, 1), Ok(2));
        assert_eq!(state.enter(0), Ok(()));
        assert_eq!(state.reservation(1_000_000, 1_000_000, 1), Ok(1));
        assert_eq!(state.enter(1), Ok(()));
        assert_eq!(state.reservation(1_000_000, 1_000_000, 3), Ok(0));
        // The wire bound still applies under a generous node limit.
        let state = DecodeState::new(ValueLimits::default());
        assert_eq!(state.reservation(1_000_000, 29, 10), Ok(2));
    }

    #[test]
    fn truncated_large_collection_declarations_are_rejected() {
        for tag in [TAG_TUPLE, TAG_VECTOR, TAG_RECORD, TAG_MAP] {
            let mut bytes = vec![tag];
            bytes.extend_from_slice(&1_000_000_u32.to_be_bytes());
            assert!(matches!(
                decode_value(&bytes, DecodeLimits::default()),
                Err(DecodeError::UnexpectedEnd { .. })
            ));
        }
    }

    #[test]
    fn domain_preimage_is_explicit_and_versioned() {
        let domain = Domain::new("zeno/test", 1);
        assert!(domain.is_ok());
        let domain = match domain {
            Ok(domain) => domain,
            Err(error) => panic!("unexpected domain error: {error}"),
        };
        let left = commitment::<TestHasher>(domain, b"payload");
        let right =
            commitment::<TestHasher>(Domain::new("zeno/test", 2).unwrap_or(domain), b"payload");
        assert!(left.is_ok() && right.is_ok());
        assert_ne!(left, right);
    }

    #[test]
    fn sealed_provider_parts_preserve_concatenation_and_empty_parts() {
        for parts in [
            vec![],
            vec![&b""[..]],
            vec![&b"a"[..], &b""[..], &b"bc"[..]],
            vec![&b"abc"[..]],
        ] {
            assert_eq!(
                TestHasher::hash_parts(&parts),
                TestHasher::hash(&parts.concat())
            );
        }
    }

    #[test]
    fn commitment_preserves_exact_framing_with_sealed_provider() {
        // Independent literal: magic, big-endian version, domain length/name,
        // big-endian payload length, payload. Empty parts add no framing.
        #[cfg_attr(verus_keep_ghost, verus_verify)]
        const EXPECTED: &[u8] =
            b"ZENOFCIS-HASH\0\x12\x34\x00\x01x\x00\x00\x00\x00\x00\x00\x00\x03abc";
        let domain =
            Domain::new("x", 0x1234).unwrap_or_else(|error| panic!("test domain: {error}"));
        assert_eq!(domain_preimage(domain, b"abc"), Ok(EXPECTED.to_vec()));
        // Independently generated SHA-256 known answer for the literal frame.
        let expected_digest = Hash32::new([
            254, 87, 153, 91, 207, 133, 214, 43, 165, 243, 234, 226, 106, 81, 234, 221, 24, 42,
            154, 78, 166, 64, 54, 158, 9, 202, 84, 83, 23, 112, 229, 122,
        ]);
        assert_eq!(TestHasher::hash(EXPECTED), expected_digest);
        assert_eq!(
            commitment::<TestHasher>(domain, b"abc"),
            Ok(expected_digest)
        );
    }

    #[test]
    fn domain_name_admission_boundaries_are_unchanged() {
        let maximum = "x".repeat(usize::from(u16::MAX));
        assert!(Domain::new(&maximum, u16::MAX).is_ok());
        for name in ["", "é", &(maximum + "x")] {
            assert_eq!(Domain::new(name, 1), Err(EncodeError::InvalidDomain));
        }
    }

    #[test]
    fn envelope_round_trip_binds_type_and_schema() {
        let envelope = Envelope::new(7, Hash32::new([3; 32]), Value::unsigned(9));
        let bytes = envelope.canonical_bytes().unwrap_or_default();
        assert_eq!(
            decode_envelope(&bytes, DecodeLimits::default()),
            Ok(envelope)
        );
    }

    #[test]
    fn admitted_envelope_matches_raw_bytes_and_is_repeatable() {
        let raw = Envelope::new(
            7,
            Hash32::new([3; 32]),
            Value::tuple(vec![Value::unsigned(9), Value::boolean(true)])
                .unwrap_or_else(|error| panic!("collection: {error}")),
        );
        let raw_bytes = raw
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("raw envelope: {error}"));
        let admitted_value = AdmittedValue::try_new(raw.value().clone())
            .unwrap_or_else(|error| panic!("value admission: {error}"));
        let admitted = AdmittedEnvelope::try_new(raw.type_id(), raw.schema_hash(), admitted_value)
            .unwrap_or_else(|error| panic!("envelope admission: {error}"));
        let first = admitted
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("first encoding: {error}"));
        let second = admitted
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("second encoding: {error}"));

        assert_eq!(first, raw_bytes);
        assert_eq!(second, raw_bytes);
        assert_eq!(
            decode_envelope(&first, DecodeLimits::default()),
            Ok(raw.clone())
        );
        assert_eq!(
            admitted.encoded_length(),
            u64::try_from(raw_bytes.len()).unwrap_or(u64::MAX)
        );
        assert_eq!(
            admitted.payload_length(),
            u32::try_from(
                raw.value()
                    .canonical_bytes()
                    .unwrap_or_else(|error| panic!("raw value: {error}"))
                    .len()
            )
            .unwrap_or(u32::MAX)
        );
        assert_eq!(admitted.into_envelope(), raw);
    }

    #[test]
    fn admitted_envelope_limit_is_exact() {
        let admitted_value = AdmittedValue::try_new(Value::boolean(true))
            .unwrap_or_else(|error| panic!("value admission: {error}"));
        let exact = ENVELOPE_OVERHEAD_BYTES + 1;
        let admitted = AdmittedEnvelope::try_new_with_limit(
            1,
            Hash32::new([1; 32]),
            admitted_value.clone(),
            exact,
        );
        assert!(admitted.is_ok());
        assert_eq!(
            AdmittedEnvelope::try_new_with_limit(
                1,
                Hash32::new([1; 32]),
                admitted_value,
                exact - 1,
            ),
            Err(EncodeError::EnvelopeInputLimit {
                limit: exact - 1,
                attempted: exact,
            })
        );
    }

    #[test]
    fn raw_envelope_admission_is_fail_closed() {
        let valid = Envelope::new(1, Hash32::new([2; 32]), Value::unsigned(3));
        let admitted = AdmittedEnvelope::try_from_envelope(valid.clone())
            .unwrap_or_else(|error| panic!("envelope admission: {error}"));
        assert_eq!(admitted.into_envelope(), valid);

        let mut too_deep = Value::unit();
        for _ in 0..65 {
            too_deep = Value::sum(0, 0, Some(too_deep));
        }
        let invalid = Envelope::new(1, Hash32::new([2; 32]), too_deep);
        assert_eq!(
            AdmittedEnvelope::try_from_envelope(invalid),
            Err(EncodeError::InvalidValue(ValueError::DepthLimit {
                limit: 64,
                attempted: 65
            }))
        );
    }
}
