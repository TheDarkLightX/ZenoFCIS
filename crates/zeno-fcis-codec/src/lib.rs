//! Zeno Canonical Value Encoding reference implementation.
//!
//! ZCVE/1 is deliberately small: fixed-width integers, definite lengths,
//! stable numeric record and variant identifiers, ASCII text in the initial
//! profile, and canonical encoded-key map order. It is not a generic Serde
//! format and does not accept alternate encodings for one semantic value.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod artifact;
mod providers;
#[cfg(feature = "libcrux")]
pub use providers::LibcruxSha256;
pub use providers::RustCryptoSha256;
pub mod domains;
pub use artifact::EvidenceArtifact;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_value::zcve::{
    TAG_BOOL_FALSE, TAG_BOOL_TRUE, TAG_BYTES, TAG_ENUM, TAG_I128, TAG_MAP, TAG_RECORD, TAG_SUM,
    TAG_TEXT, TAG_TUPLE, TAG_U128, TAG_UNIT, TAG_VECTOR,
};
use zeno_fcis_value::{
    AdmittedValue, Field, MapEntry, Value, ValueError, ValueLimits, ValueMetrics,
};
const ENVELOPE_MAGIC: &[u8; 8] = b"ZFCISV1\0";
const ENVELOPE_OVERHEAD_BYTES: u64 = 8 + 4 + 32 + 4;
const HASH_MAGIC: &[u8; 14] = b"ZENOFCIS-HASH\0";

/// A 256-bit commitment value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Hash32([u8; 32]);

impl Hash32 {
    /// The all-zero hash, useful only for explicit sentinel schemas.
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

impl fmt::Display for Hash32 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

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
pub trait CommitmentHasher: hasher_seal::Sealed {
    /// Stable algorithm identifier, such as `sha2-256/rustcrypto-0.11`.
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Domain<'a> {
    name: &'a str,
    version: u16,
}

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
pub const RESERVED_DOMAIN_NAMESPACE: &str = "zeno-fcis";

/// Tests the exact reserved namespace boundary without normalization.
#[must_use]
pub fn is_reserved_domain_name(name: &str) -> bool {
    name == RESERVED_DOMAIN_NAMESPACE || name.starts_with("zeno-fcis/")
}

/// Builds the exact domain-separated hash preimage.
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

impl CanonicalEncode for Value {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output).map_err(map_value_encode_error)
    }
}

impl CanonicalEncode for AdmittedValue {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output).map_err(map_value_encode_error)
    }
}

/// Appends a canonical value using exactly the supplied structural limits.
/// Every refusal preserves the existing output prefix.
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Envelope {
    type_id: u32,
    schema_hash: Hash32,
    value: Value,
}

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedEnvelope {
    type_id: u32,
    schema_hash: Hash32,
    value: AdmittedValue,
    payload_length: u32,
    max_input_bytes: u64,
}

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeLimits {
    /// Maximum input bytes.
    pub max_input_bytes: u64,
    /// Closed-value structural limits.
    pub value: ValueLimits,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: Self::DEFAULT_MAX_INPUT_BYTES,
            value: ValueLimits::default(),
        }
    }
}

impl DecodeLimits {
    /// Reviewed default maximum for complete input bytes.
    pub const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
}

/// Decodes one value and rejects trailing bytes and noncanonical aliases.
pub fn decode_value(bytes: &[u8], limits: DecodeLimits) -> Result<Value, DecodeError> {
    decode_value_with_metrics(bytes, limits).map(|(value, _)| value)
}

/// Decodes a value and returns the exact counters observed by the actual decoder.
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
    if canonical_value_bytes_with_limits(&value, state.limits).map_err(DecodeError::Encode)?
        != bytes
    {
        return Err(DecodeError::NonCanonical);
    }
    Ok(value)
}

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

struct DecodeState {
    limits: ValueLimits,
    nodes: u64,
    payload_bytes: u64,
    depth: u32,
}

impl DecodeState {
    const fn new(limits: ValueLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            payload_bytes: 0,
            depth: 0,
        }
    }

    fn enter(&mut self, depth: u32) -> Result<(), DecodeError> {
        if depth > self.limits.max_depth {
            return Err(DecodeError::DepthLimit {
                limit: self.limits.max_depth,
                attempted: depth,
            });
        }
        self.depth = self.depth.max(depth);
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(DecodeError::LengthOverflow)?;
        if self.nodes > self.limits.max_nodes {
            return Err(DecodeError::NodeLimit {
                limit: self.limits.max_nodes,
                attempted: self.nodes,
            });
        }
        Ok(())
    }

    fn payload(&mut self, length: usize) -> Result<(), DecodeError> {
        let length = u64::try_from(length).map_err(|_| DecodeError::LengthOverflow)?;
        self.payload_bytes = self
            .payload_bytes
            .checked_add(length)
            .ok_or(DecodeError::LengthOverflow)?;
        if self.payload_bytes > self.limits.max_payload_bytes {
            return Err(DecodeError::PayloadLimit {
                limit: self.limits.max_payload_bytes,
                attempted: self.payload_bytes,
            });
        }
        Ok(())
    }

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

fn decode_value_inner(
    cursor: &mut Cursor<'_>,
    state: &mut DecodeState,
    depth: u32,
) -> Result<Value, DecodeError> {
    state.enter(depth)?;
    let tag = cursor.take_u8()?;
    match tag {
        TAG_UNIT => Ok(Value::unit()),
        TAG_BOOL_FALSE => Ok(Value::boolean(false)),
        TAG_BOOL_TRUE => Ok(Value::boolean(true)),
        TAG_U128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            Ok(Value::unsigned(u128::from_be_bytes(bytes)))
        }
        TAG_I128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            Ok(Value::signed(i128::from_be_bytes(bytes)))
        }
        TAG_BYTES => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            Value::bytes_with_limits(bytes.to_vec(), state.limits)
                .map_err(DecodeError::InvalidValue)
        }
        TAG_TEXT => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            if !bytes.is_ascii() {
                return Err(DecodeError::NonAsciiText);
            }
            let text = core::str::from_utf8(bytes).map_err(|_| DecodeError::Utf8)?;
            Value::text_ascii_with_limits(String::from(text), state.limits)
                .map_err(DecodeError::InvalidValue)
        }
        TAG_ENUM => Ok(Value::enumeration(cursor.take_u32()?, cursor.take_u16()?)),
        TAG_TUPLE | TAG_VECTOR => {
            let count = cursor.take_u32()?;
            state.collection(count)?;
            let mut items =
                Vec::with_capacity(initial_collection_capacity(count, cursor.remaining(), 1)?);
            for _ in 0..count {
                items.push(decode_value_inner(
                    cursor,
                    state,
                    depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                )?);
            }
            if tag == TAG_TUPLE {
                Value::tuple(items).map_err(DecodeError::InvalidValue)
            } else {
                Value::vector(items).map_err(DecodeError::InvalidValue)
            }
        }
        TAG_RECORD => {
            let count = cursor.take_u32()?;
            state.collection(count)?;
            // One field requires its u16 identifier and at least one value tag.
            let mut fields =
                Vec::with_capacity(initial_collection_capacity(count, cursor.remaining(), 3)?);
            let mut previous = None;
            for _ in 0..count {
                let id = cursor.take_u16()?;
                if previous.is_some_and(|value| value >= id) {
                    return Err(DecodeError::NonCanonicalRecord);
                }
                previous = Some(id);
                fields.push(Field::new(
                    id,
                    decode_value_inner(
                        cursor,
                        state,
                        depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                    )?,
                ));
            }
            Value::record_canonical(fields).map_err(DecodeError::InvalidValue)
        }
        TAG_SUM => {
            let type_id = cursor.take_u32()?;
            let variant = cursor.take_u16()?;
            let payload = match cursor.take_u8()? {
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
            state.collection(count)?;
            // One entry requires two u32 blob lengths plus at least one value
            // tag in each encoded key and value.
            let mut entries =
                Vec::with_capacity(initial_collection_capacity(count, cursor.remaining(), 10)?);
            // Encoded keys are compared as they are borrowed from the input, so
            // canonical key order is decided on the exact wire bytes.
            let mut previous: Option<&[u8]> = None;
            for _ in 0..count {
                let encoded_key = cursor.take_blob(state.limits.max_payload_bytes)?;
                state.payload(encoded_key.len())?;
                if previous.is_some_and(|value| value >= encoded_key) {
                    return Err(DecodeError::NonCanonicalMap);
                }
                let encoded_value = cursor.take_blob(state.limits.max_payload_bytes)?;
                state.payload(encoded_value.len())?;

                let key = decode_complete_value(
                    encoded_key,
                    state,
                    depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                )?;
                let value = decode_complete_value(
                    encoded_value,
                    state,
                    depth.checked_add(1).ok_or(DecodeError::LengthOverflow)?,
                )?;

                let entry = MapEntry::try_new_with_limits(key, value, state.limits)
                    .map_err(DecodeError::InvalidValue)?;
                if entry.encoded_key() != encoded_key {
                    return Err(DecodeError::NonCanonical);
                }
                previous = Some(encoded_key);
                entries.push(entry);
            }
            Value::map_canonical(entries).map_err(DecodeError::InvalidValue)
        }
        other => Err(DecodeError::UnknownTag(other)),
    }
}

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

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

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

    fn take_u8(&mut self) -> Result<u8, DecodeError> {
        let bytes = self.take(1)?;
        bytes.first().copied().ok_or(DecodeError::UnexpectedEnd {
            offset: self.offset,
            requested: 1,
        })
    }

    fn take_u16(&mut self) -> Result<u16, DecodeError> {
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(bytes))
    }

    fn take_u32(&mut self) -> Result<u32, DecodeError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

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
#[derive(Clone, Debug, Eq, PartialEq)]
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

impl core::error::Error for EncodeError {}

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
#[derive(Clone, Debug, Eq, PartialEq)]
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

impl core::error::Error for DecodeError {}

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
