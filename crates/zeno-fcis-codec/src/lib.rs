//! Zeno Canonical Value Encoding reference implementation.
//!
//! ZCVE/1 is deliberately small: fixed-width integers, definite lengths,
//! stable numeric record and variant identifiers, ASCII text in the initial
//! profile, and canonical encoded-key map order. It is not a generic Serde
//! format and does not accept alternate encodings for one semantic value.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_value::{Field, MapEntry, Value, ValueError, ValueLimits};

const TAG_UNIT: u8 = 0x00;
const TAG_BOOL_FALSE: u8 = 0x01;
const TAG_BOOL_TRUE: u8 = 0x02;
const TAG_U128: u8 = 0x03;
const TAG_I128: u8 = 0x04;
const TAG_BYTES: u8 = 0x05;
const TAG_TEXT: u8 = 0x06;
const TAG_ENUM: u8 = 0x07;
const TAG_TUPLE: u8 = 0x08;
const TAG_RECORD: u8 = 0x09;
const TAG_SUM: u8 = 0x0a;
const TAG_VECTOR: u8 = 0x0b;
const TAG_MAP: u8 = 0x0c;
const ENVELOPE_MAGIC: &[u8; 8] = b"ZFCISV1\0";
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

/// A cryptographic commitment provider.
///
/// The semantic kernel owns domain separation and preimage construction. A
/// provider supplies only the vetted 32-byte hash primitive.
pub trait CommitmentHasher {
    /// Stable algorithm identifier, such as `sha2-256/rustcrypto-0.11`.
    const ALGORITHM_ID: &'static str;

    /// Hashes exact bytes to 32 bytes.
    fn hash(bytes: &[u8]) -> Hash32;
}

/// A versioned domain-separation tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Domain<'a> {
    name: &'a str,
    version: u16,
}

impl<'a> Domain<'a> {
    /// Validates an ASCII, non-empty, bounded domain name.
    pub fn new(name: &'a str, version: u16) -> Result<Self, EncodeError> {
        if name.is_empty() || !name.is_ascii() || name.len() > usize::from(u16::MAX) {
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

/// Builds the exact domain-separated hash preimage.
pub fn domain_preimage(domain: Domain<'_>, payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    let domain_length = u16::try_from(domain.name.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let mut output = Vec::with_capacity(
        HASH_MAGIC.len() + 2 + 2 + domain.name.len() + 8 + payload.len(),
    );
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
    let preimage = domain_preimage(domain, payload)?;
    Ok(H::hash(&preimage))
}

/// Canonical encoding interface.
pub trait CanonicalEncode {
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
        self.validate_limits(ValueLimits::default())
            .map_err(EncodeError::InvalidValue)?;
        encode_value(self, output)
    }
}

fn encode_value(value: &Value, output: &mut Vec<u8>) -> Result<(), EncodeError> {
    match value {
        Value::Unit => output.push(TAG_UNIT),
        Value::Bool(false) => output.push(TAG_BOOL_FALSE),
        Value::Bool(true) => output.push(TAG_BOOL_TRUE),
        Value::U128(integer) => {
            output.push(TAG_U128);
            output.extend_from_slice(&integer.to_be_bytes());
        }
        Value::I128(integer) => {
            output.push(TAG_I128);
            output.extend_from_slice(&integer.to_be_bytes());
        }
        Value::Bytes(bytes) => {
            output.push(TAG_BYTES);
            put_bytes(output, bytes)?;
        }
        Value::Text(text) => {
            if !text.is_ascii() {
                return Err(EncodeError::NonAsciiText);
            }
            output.push(TAG_TEXT);
            put_bytes(output, text.as_bytes())?;
        }
        Value::Enum { type_id, variant } => {
            output.push(TAG_ENUM);
            output.extend_from_slice(&type_id.to_be_bytes());
            output.extend_from_slice(&variant.to_be_bytes());
        }
        Value::Tuple(items) => {
            output.push(TAG_TUPLE);
            put_length(output, items.len())?;
            for item in items {
                encode_value(item, output)?;
            }
        }
        Value::Record(fields) => {
            output.push(TAG_RECORD);
            put_length(output, fields.len())?;
            let mut previous = None;
            for field in fields {
                if previous.is_some_and(|value| value >= field.id()) {
                    return Err(EncodeError::NonCanonicalRecord);
                }
                previous = Some(field.id());
                output.extend_from_slice(&field.id().to_be_bytes());
                encode_value(field.value(), output)?;
            }
        }
        Value::Sum {
            type_id,
            variant,
            payload,
        } => {
            output.push(TAG_SUM);
            output.extend_from_slice(&type_id.to_be_bytes());
            output.extend_from_slice(&variant.to_be_bytes());
            match payload {
                None => output.push(0),
                Some(value) => {
                    output.push(1);
                    encode_value(value, output)?;
                }
            }
        }
        Value::Vector(items) => {
            output.push(TAG_VECTOR);
            put_length(output, items.len())?;
            for item in items {
                encode_value(item, output)?;
            }
        }
        Value::Map(entries) => {
            output.push(TAG_MAP);
            put_length(output, entries.len())?;
            let mut previous: Option<&[u8]> = None;
            for entry in entries {
                if previous.is_some_and(|value| value >= entry.encoded_key()) {
                    return Err(EncodeError::NonCanonicalMap);
                }
                let actual_key = entry.key().canonical_bytes()?;
                if actual_key.as_slice() != entry.encoded_key() {
                    return Err(EncodeError::MapKeyMismatch);
                }
                previous = Some(entry.encoded_key());
                put_bytes(output, entry.encoded_key())?;
                let encoded_value = entry.value().canonical_bytes()?;
                put_bytes(output, &encoded_value)?;
            }
        }
    }
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
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

impl CanonicalEncode for Envelope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(ENVELOPE_MAGIC);
        output.extend_from_slice(&self.type_id.to_be_bytes());
        output.extend_from_slice(self.schema_hash.as_bytes());
        let payload = self.value.canonical_bytes()?;
        put_bytes(output, &payload)
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
            max_input_bytes: 64 * 1024 * 1024,
            value: ValueLimits::default(),
        }
    }
}

/// Decodes one value and rejects trailing bytes and noncanonical aliases.
pub fn decode_value(bytes: &[u8], limits: DecodeLimits) -> Result<Value, DecodeError> {
    enforce_input_limit(bytes, limits)?;
    let mut cursor = Cursor::new(bytes);
    let mut state = DecodeState::new(limits.value);
    let value = decode_value_inner(&mut cursor, &mut state, 0)?;
    if cursor.remaining() != 0 {
        return Err(DecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    let encoded = value.canonical_bytes().map_err(DecodeError::Encode)?;
    if encoded.as_slice() != bytes {
        return Err(DecodeError::NonCanonical);
    }
    Ok(value)
}

/// Decodes one canonical envelope.
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
    let envelope = Envelope::new(type_id, Hash32::new(schema_hash), value);
    let encoded = envelope.canonical_bytes().map_err(DecodeError::Encode)?;
    if encoded.as_slice() != bytes {
        return Err(DecodeError::NonCanonical);
    }
    Ok(envelope)
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
}

impl DecodeState {
    const fn new(limits: ValueLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            payload_bytes: 0,
        }
    }

    fn enter(&mut self, depth: u32) -> Result<(), DecodeError> {
        if depth > self.limits.max_depth {
            return Err(DecodeError::DepthLimit {
                limit: self.limits.max_depth,
                attempted: depth,
            });
        }
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
        TAG_UNIT => Ok(Value::Unit),
        TAG_BOOL_FALSE => Ok(Value::Bool(false)),
        TAG_BOOL_TRUE => Ok(Value::Bool(true)),
        TAG_U128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            Ok(Value::U128(u128::from_be_bytes(bytes)))
        }
        TAG_I128 => {
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(cursor.take(16)?);
            Ok(Value::I128(i128::from_be_bytes(bytes)))
        }
        TAG_BYTES => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            Ok(Value::Bytes(bytes.to_vec().into_boxed_slice()))
        }
        TAG_TEXT => {
            let bytes = cursor.take_blob(state.limits.max_payload_bytes)?;
            state.payload(bytes.len())?;
            if !bytes.is_ascii() {
                return Err(DecodeError::NonAsciiText);
            }
            let text = core::str::from_utf8(bytes).map_err(|_| DecodeError::Utf8)?;
            Ok(Value::Text(String::from(text).into_boxed_str()))
        }
        TAG_ENUM => Ok(Value::Enum {
            type_id: cursor.take_u32()?,
            variant: cursor.take_u16()?,
        }),
        TAG_TUPLE | TAG_VECTOR => {
            let count = cursor.take_u32()?;
            state.collection(count)?;
            let capacity = usize::try_from(count).map_err(|_| DecodeError::LengthOverflow)?;
            let mut items = Vec::with_capacity(capacity);
            for _ in 0..count {
                items.push(decode_value_inner(cursor, state, depth + 1)?);
            }
            if tag == TAG_TUPLE {
                Ok(Value::Tuple(items.into_boxed_slice()))
            } else {
                Ok(Value::Vector(items.into_boxed_slice()))
            }
        }
        TAG_RECORD => {
            let count = cursor.take_u32()?;
            state.collection(count)?;
            let capacity = usize::try_from(count).map_err(|_| DecodeError::LengthOverflow)?;
            let mut fields = Vec::with_capacity(capacity);
            let mut previous = None;
            for _ in 0..count {
                let id = cursor.take_u16()?;
                if previous.is_some_and(|value| value >= id) {
                    return Err(DecodeError::NonCanonicalRecord);
                }
                previous = Some(id);
                fields.push(Field::new(
                    id,
                    decode_value_inner(cursor, state, depth + 1)?,
                ));
            }
            Value::record_canonical(fields).map_err(DecodeError::InvalidValue)
        }
        TAG_SUM => {
            let type_id = cursor.take_u32()?;
            let variant = cursor.take_u16()?;
            let payload = match cursor.take_u8()? {
                0 => None,
                1 => Some(Box::new(decode_value_inner(cursor, state, depth + 1)?)),
                _ => return Err(DecodeError::InvalidSumFlag),
            };
            Ok(Value::Sum {
                type_id,
                variant,
                payload,
            })
        }
        TAG_MAP => {
            let count = cursor.take_u32()?;
            state.collection(count)?;
            let capacity = usize::try_from(count).map_err(|_| DecodeError::LengthOverflow)?;
            let mut entries = Vec::with_capacity(capacity);
            let mut previous: Option<Vec<u8>> = None;
            for _ in 0..count {
                let encoded_key = cursor.take_blob(state.limits.max_payload_bytes)?.to_vec();
                state.payload(encoded_key.len())?;
                if previous
                    .as_ref()
                    .is_some_and(|value| value.as_slice() >= encoded_key.as_slice())
                {
                    return Err(DecodeError::NonCanonicalMap);
                }
                let encoded_value = cursor.take_blob(state.limits.max_payload_bytes)?;
                state.payload(encoded_value.len())?;

                let mut key_cursor = Cursor::new(&encoded_key);
                let key = decode_value_inner(&mut key_cursor, state, depth + 1)?;
                if key_cursor.remaining() != 0 {
                    return Err(DecodeError::TrailingBytes {
                        offset: key_cursor.offset,
                    });
                }
                if key.canonical_bytes().map_err(DecodeError::Encode)? != encoded_key {
                    return Err(DecodeError::NonCanonical);
                }

                let mut value_cursor = Cursor::new(encoded_value);
                let value = decode_value_inner(&mut value_cursor, state, depth + 1)?;
                if value_cursor.remaining() != 0 {
                    return Err(DecodeError::TrailingBytes {
                        offset: value_cursor.offset,
                    });
                }
                if value.canonical_bytes().map_err(DecodeError::Encode)? != encoded_value {
                    return Err(DecodeError::NonCanonical);
                }

                previous = Some(encoded_key.clone());
                entries.push(MapEntry::new(encoded_key, key, value));
            }
            Value::map_canonical(entries).map_err(DecodeError::InvalidValue)
        }
        other => Err(DecodeError::UnknownTag(other)),
    }
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
    /// The value itself violates closed-value invariants.
    InvalidValue(ValueError),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthOverflow => formatter.write_str("canonical length overflow"),
            Self::NonAsciiText => formatter.write_str("canonical text is not ASCII"),
            Self::NonCanonicalRecord => formatter.write_str("record fields are not canonical"),
            Self::NonCanonicalMap => formatter.write_str("map entries are not canonical"),
            Self::MapKeyMismatch => formatter.write_str("map encoded key does not match semantic key"),
            Self::InvalidDomain => formatter.write_str("invalid domain-separation tag"),
            Self::InvalidValue(error) => error.fmt(formatter),
        }
    }
}

/// Canonical decoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
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
            Self::TrailingBytes { offset } => write!(formatter, "trailing bytes at offset {offset}"),
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
                write!(formatter, "collection length {attempted} exceeds limit {limit}")
            }
            Self::InvalidValue(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct XorTestHasher;

    impl CommitmentHasher for XorTestHasher {
        const ALGORITHM_ID: &'static str = "test/xor/v1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                output[index % 32] ^= byte;
            }
            Hash32::new(output)
        }
    }

    #[test]
    fn value_round_trip_is_exact() {
        let value = Value::record_canonical(vec![
            Field::new(1, Value::U128(42)),
            Field::new(2, Value::Bool(true)),
            Field::new(3, Value::tuple(vec![Value::I128(-7), Value::bytes(vec![1, 2])])),
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
    fn map_order_is_encoded_key_order() {
        let key_one = Value::U128(1);
        let key_two = Value::U128(2);
        let encoded_one = key_one.canonical_bytes();
        let encoded_two = key_two.canonical_bytes();
        assert!(encoded_one.is_ok() && encoded_two.is_ok());
        let entries = vec![
            MapEntry::new(encoded_one.unwrap_or_default(), key_one, Value::Bool(true)),
            MapEntry::new(encoded_two.unwrap_or_default(), key_two, Value::Bool(false)),
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
    fn trailing_bytes_are_rejected() {
        let mut bytes = Value::Bool(true).canonical_bytes().unwrap_or_default();
        bytes.push(0);
        assert!(matches!(
            decode_value(&bytes, DecodeLimits::default()),
            Err(DecodeError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn domain_preimage_is_explicit_and_versioned() {
        let domain = Domain::new("zeno/test", 1);
        assert!(domain.is_ok());
        let domain = match domain {
            Ok(domain) => domain,
            Err(error) => panic!("unexpected domain error: {error}"),
        };
        let left = commitment::<XorTestHasher>(domain, b"payload");
        let right = commitment::<XorTestHasher>(
            Domain::new("zeno/test", 2).unwrap_or(domain),
            b"payload",
        );
        assert!(left.is_ok() && right.is_ok());
        assert_ne!(left, right);
    }

    #[test]
    fn envelope_round_trip_binds_type_and_schema() {
        let envelope = Envelope::new(7, Hash32::new([3; 32]), Value::U128(9));
        let bytes = envelope.canonical_bytes().unwrap_or_default();
        assert_eq!(
            decode_envelope(&bytes, DecodeLimits::default()),
            Ok(envelope)
        );
    }
}
