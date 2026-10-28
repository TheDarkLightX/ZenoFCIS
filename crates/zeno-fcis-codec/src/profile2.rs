//! Experimental explicit ZCVE/2 codec over the closed profile2 value grammar.
//!
//! Choosing this namespace selects the profile. Legacy entry points never
//! infer it. Decoding carries supplied schema/type pins without authenticating
//! expected pins; schema/catalog/Authority admission remains a separate gate.
use crate::CanonicalEncode;
pub use crate::{DecodeError, DecodeLimits, EncodeError, Hash32};
use alloc::{string::String, vec::Vec};
pub use zeno_fcis_value::profile2::{AdmittedValue, Field, MapEntry, Value, ValueRef};
use zeno_fcis_value::profile2::{I256, TAG_I256, TAG_U256, unsigned_from_payload};
use zeno_fcis_value::{ValueLimits, ValueMetrics, zcve::*};

const MAGIC: &[u8; 8] = b"ZFCISV2\0";
const OVERHEAD: u64 = 48;

impl crate::canonical_seal::Sealed for Value {}
impl crate::canonical_seal::Sealed for AdmittedValue {}
impl crate::canonical_seal::Sealed for Envelope {}
impl crate::canonical_seal::Sealed for AdmittedEnvelope {}
impl CanonicalEncode for Value {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output)
            .map_err(crate::map_value_encode_error)
    }
}
impl CanonicalEncode for AdmittedValue {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.encode_zcve_to(output)
            .map_err(crate::map_value_encode_error)
    }
}

/// Encodes after exactly the supplied structural admission; refusal preserves the prefix.
pub fn encode_value_with_limits(
    value: &Value,
    output: &mut Vec<u8>,
    limits: ValueLimits,
) -> Result<(), EncodeError> {
    value
        .encode_zcve_to_with_limits(output, limits)
        .map_err(crate::map_value_encode_error)
}
/// Canonical bare bytes for exactly supplied structural limits.
pub fn canonical_value_bytes_with_limits(
    value: &Value,
    limits: ValueLimits,
) -> Result<Vec<u8>, EncodeError> {
    value
        .zcve_bytes_with_limits(limits)
        .map_err(crate::map_value_encode_error)
}

/// Raw immutable profile2 envelope; construction alone does not admit a schema or resources.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Envelope {
    type_id: u32,
    schema_hash: Hash32,
    value: Value,
}
impl Envelope {
    /// Owns exact supplied pins and value, without asserting their authority.
    #[must_use]
    pub const fn new(type_id: u32, schema_hash: Hash32, value: Value) -> Self {
        Self {
            type_id,
            schema_hash,
            value,
        }
    }
    /// Supplied type identifier.
    #[must_use]
    pub const fn type_id(&self) -> u32 {
        self.type_id
    }
    /// Supplied schema hash, not an authenticated expected schema.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }
    /// Immutable value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
    /// Consumes the envelope.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }
    /// Exact supplied structural/input limits; every refusal preserves output.
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
    /// Complete canonical envelope under exactly supplied limits.
    pub fn canonical_bytes_with_limits(
        &self,
        limits: DecodeLimits,
    ) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to_with_limits(&mut output, limits)?;
        Ok(output)
    }
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

/// Owned value/input admission; fields are private and no mutable alias escapes.
/// ```compile_fail
/// use zeno_fcis_codec::profile2::AdmittedEnvelope;
/// let _ = AdmittedEnvelope { type_id: 1 };
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
    /// Admits the complete default envelope limits.
    pub fn try_new(
        type_id: u32,
        schema_hash: Hash32,
        value: AdmittedValue,
    ) -> Result<Self, EncodeError> {
        Self::try_new_with_limits(type_id, schema_hash, value, DecodeLimits::default())
    }
    /// Owns exactly supplied limits, re-admitting when the previous value witness differs.
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
                .map_err(crate::map_value_encode_error)?
        };
        let payload_length = u32::try_from(
            value
                .value()
                .encoded_length()
                .map_err(crate::map_value_encode_error)?,
        )
        .map_err(|_| EncodeError::LengthOverflow)?;
        envelope_length(payload_length, limits.max_input_bytes)?;
        Ok(Self {
            type_id,
            schema_hash,
            value,
            payload_length,
            max_input_bytes: limits.max_input_bytes,
        })
    }
    /// Admits a raw envelope under default limits.
    pub fn try_from_envelope(envelope: Envelope) -> Result<Self, EncodeError> {
        Self::try_from_envelope_with_limits(envelope, DecodeLimits::default())
    }
    /// Admits a raw envelope under exactly supplied limits.
    pub fn try_from_envelope_with_limits(
        envelope: Envelope,
        limits: DecodeLimits,
    ) -> Result<Self, EncodeError> {
        let value = AdmittedValue::try_new_with_limits(envelope.value, limits.value)
            .map_err(crate::map_value_encode_error)?;
        Self::try_new_with_limits(envelope.type_id, envelope.schema_hash, value, limits)
    }
    /// Supplied type ID.
    #[must_use]
    pub const fn type_id(&self) -> u32 {
        self.type_id
    }
    /// Supplied schema pin; expected-pin comparison is separate.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }
    /// Immutable value admission witness.
    #[must_use]
    pub const fn value(&self) -> &AdmittedValue {
        &self.value
    }
    /// Exact retained limits.
    #[must_use]
    pub const fn limits(&self) -> DecodeLimits {
        DecodeLimits {
            max_input_bytes: self.max_input_bytes,
            value: self.value.limits(),
        }
    }
    /// Exact payload length.
    #[must_use]
    pub const fn payload_length(&self) -> u32 {
        self.payload_length
    }
    /// Exact complete envelope length.
    #[must_use]
    pub fn encoded_length(&self) -> u64 {
        OVERHEAD + u64::from(self.payload_length)
    }
    /// Consumes the envelope witness.
    #[must_use]
    pub fn into_value(self) -> AdmittedValue {
        self.value
    }
    /// Consumes the witness into an owned raw envelope.
    #[must_use]
    pub fn into_envelope(self) -> Envelope {
        Envelope::new(self.type_id, self.schema_hash, self.value.into_value())
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
fn envelope_length(payload: u32, limit: u64) -> Result<u64, EncodeError> {
    let attempted = OVERHEAD
        .checked_add(u64::from(payload))
        .ok_or(EncodeError::LengthOverflow)?;
    if attempted > limit {
        return Err(EncodeError::EnvelopeInputLimit { limit, attempted });
    }
    Ok(attempted)
}
fn append_envelope(
    out: &mut Vec<u8>,
    type_id: u32,
    schema: Hash32,
    payload: &[u8],
    limit: u64,
) -> Result<(), EncodeError> {
    let length = u32::try_from(payload.len()).map_err(|_| EncodeError::LengthOverflow)?;
    let total = usize::try_from(envelope_length(length, limit)?)
        .map_err(|_| EncodeError::LengthOverflow)?;
    out.len()
        .checked_add(total)
        .ok_or(EncodeError::LengthOverflow)?;
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&type_id.to_be_bytes());
    out.extend_from_slice(schema.as_bytes());
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

/// Complete bare-value decode, with shared counters across all nested map blobs.
pub fn decode_value(bytes: &[u8], limits: DecodeLimits) -> Result<Value, DecodeError> {
    decode_value_with_metrics(bytes, limits).map(|(value, _)| value)
}
/// Returns actual decoder counters on successful canonical admission.
pub fn decode_value_with_metrics(
    bytes: &[u8],
    limits: DecodeLimits,
) -> Result<(Value, ValueMetrics), DecodeError> {
    input_limit(bytes, limits.max_input_bytes)?;
    let mut state = State {
        limits: limits.value,
        metrics: ValueMetrics::default(),
    };
    let value = complete(bytes, &mut state, 0)?;
    Ok((value, state.metrics))
}
/// Only ZFCISV2 magic is accepted, including when the payload contains only old tags.
/// Shared error kinds remain the same as the legacy codec; EnvelopeMagic is a
/// profile mismatch here even though its legacy Display wording names ZCVE/1.
pub fn decode_envelope(bytes: &[u8], limits: DecodeLimits) -> Result<Envelope, DecodeError> {
    input_limit(bytes, limits.max_input_bytes)?;
    let mut c = Cursor::new(bytes);
    if c.take(8)? != MAGIC {
        return Err(DecodeError::EnvelopeMagic);
    }
    let type_id = c.u32()?;
    let mut schema = [0; 32];
    schema.copy_from_slice(c.take(32)?);
    let payload = c.blob(limits.max_input_bytes)?;
    if c.remaining() != 0 {
        return Err(DecodeError::TrailingBytes { offset: c.offset });
    }
    let value = decode_value(payload, limits)?;
    Ok(Envelope::new(type_id, Hash32::new(schema), value))
}
fn input_limit(bytes: &[u8], limit: u64) -> Result<(), DecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| DecodeError::LengthOverflow)?;
    if actual > limit {
        return Err(DecodeError::InputLimit { limit, actual });
    }
    Ok(())
}
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or(DecodeError::LengthOverflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(DecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: n,
            })?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let mut b = [0; 2];
        b.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(b))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        let mut b = [0; 4];
        b.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(b))
    }
    fn blob(&mut self, max: u64) -> Result<&'a [u8], DecodeError> {
        let n = self.u32()?;
        if u64::from(n) > max {
            return Err(DecodeError::BlobLimit {
                limit: max,
                attempted: u64::from(n),
            });
        }
        self.take(usize::try_from(n).map_err(|_| DecodeError::LengthOverflow)?)
    }
}
struct State {
    limits: ValueLimits,
    metrics: ValueMetrics,
}
impl State {
    fn enter(&mut self, depth: u32) -> Result<(), DecodeError> {
        let limit = self.limits.effective_max_depth();
        if depth > limit {
            return Err(DecodeError::DepthLimit {
                limit,
                attempted: depth,
            });
        }
        let nodes = self
            .metrics
            .nodes
            .checked_add(1)
            .ok_or(DecodeError::LengthOverflow)?;
        if nodes > self.limits.max_nodes {
            return Err(DecodeError::NodeLimit {
                limit: self.limits.max_nodes,
                attempted: nodes,
            });
        }
        self.metrics.depth = self.metrics.depth.max(depth);
        self.metrics.nodes = nodes;
        Ok(())
    }
    fn payload(&mut self, n: usize) -> Result<(), DecodeError> {
        let n = u64::try_from(n).map_err(|_| DecodeError::LengthOverflow)?;
        let attempted = self
            .metrics
            .payload_bytes
            .checked_add(n)
            .ok_or(DecodeError::LengthOverflow)?;
        if attempted > self.limits.max_payload_bytes {
            return Err(DecodeError::PayloadLimit {
                limit: self.limits.max_payload_bytes,
                attempted,
            });
        }
        self.metrics.payload_bytes = attempted;
        Ok(())
    }
    fn collection(&self, n: u32) -> Result<(), DecodeError> {
        if n > self.limits.max_collection_len {
            return Err(DecodeError::CollectionLimit {
                limit: self.limits.max_collection_len,
                attempted: n,
            });
        }
        Ok(())
    }
    fn capacity(&self, n: u32, remaining: usize, min_bytes: usize) -> Result<usize, DecodeError> {
        let count = usize::try_from(n).map_err(|_| DecodeError::LengthOverflow)?;
        let nodes = usize::try_from(self.limits.max_nodes.saturating_sub(self.metrics.nodes))
            .unwrap_or(usize::MAX);
        Ok(count
            .min(
                remaining
                    .checked_div(min_bytes)
                    .ok_or(DecodeError::LengthOverflow)?,
            )
            .min(nodes))
    }
}
fn next(depth: u32) -> Result<u32, DecodeError> {
    depth.checked_add(1).ok_or(DecodeError::LengthOverflow)
}
fn complete(bytes: &[u8], state: &mut State, depth: u32) -> Result<Value, DecodeError> {
    let mut c = Cursor::new(bytes);
    let value = inner(&mut c, state, depth)?;
    if c.remaining() != 0 {
        return Err(DecodeError::TrailingBytes { offset: c.offset });
    }
    let canonical =
        canonical_value_bytes_with_limits(&value, state.limits).map_err(DecodeError::Encode)?;
    if canonical.as_slice() != bytes {
        return Err(DecodeError::NonCanonical);
    }
    Ok(value)
}
fn inner(c: &mut Cursor<'_>, s: &mut State, depth: u32) -> Result<Value, DecodeError> {
    s.enter(depth)?;
    let tag = c.u8()?;
    match tag {
        TAG_UNIT => Ok(Value::unit()),
        TAG_BOOL_FALSE => Ok(Value::boolean(false)),
        TAG_BOOL_TRUE => Ok(Value::boolean(true)),
        TAG_U128 => {
            let mut b = [0; 16];
            b.copy_from_slice(c.take(16)?);
            Ok(Value::unsigned(u128::from_be_bytes(b)))
        }
        TAG_I128 => {
            let mut b = [0; 16];
            b.copy_from_slice(c.take(16)?);
            Ok(Value::signed(i128::from_be_bytes(b)))
        }
        TAG_U256 | TAG_I256 => {
            let mut b = [0; 32];
            b.copy_from_slice(c.take(32)?);
            let bits = unsigned_from_payload(b);
            Ok(if tag == TAG_U256 {
                Value::unsigned256(bits)
            } else {
                Value::signed256(I256::from_bits(bits))
            })
        }
        TAG_BYTES => {
            let b = c.blob(s.limits.max_payload_bytes)?;
            s.payload(b.len())?;
            Value::bytes_with_limits(b.to_vec(), s.limits).map_err(DecodeError::InvalidValue)
        }
        TAG_TEXT => {
            let b = c.blob(s.limits.max_payload_bytes)?;
            s.payload(b.len())?;
            if !b.is_ascii() {
                return Err(DecodeError::NonAsciiText);
            }
            let text = core::str::from_utf8(b).map_err(|_| DecodeError::Utf8)?;
            Value::text_ascii_with_limits(String::from(text), s.limits)
                .map_err(DecodeError::InvalidValue)
        }
        TAG_ENUM => Ok(Value::enumeration(c.u32()?, c.u16()?)),
        TAG_TUPLE | TAG_VECTOR => decode_sequence(c, s, depth, tag),
        TAG_RECORD => decode_record(c, s, depth),
        TAG_SUM => decode_sum(c, s, depth),
        TAG_MAP => decode_map(c, s, depth),
        other => Err(DecodeError::UnknownTag(other)),
    }
}

// Keep collection/sum locals out of the recursive dispatcher frame. The
// unoptimized decoder must retain the existing 8 MiB depth-test profile.
fn decode_sequence(
    c: &mut Cursor<'_>,
    s: &mut State,
    depth: u32,
    tag: u8,
) -> Result<Value, DecodeError> {
    let count = c.u32()?;
    s.collection(count)?;
    let mut items = Vec::with_capacity(s.capacity(count, c.remaining(), 1)?);
    for _ in 0..count {
        items.push(inner(c, s, next(depth)?)?);
    }
    if tag == TAG_TUPLE {
        Value::tuple(items)
    } else {
        Value::vector(items)
    }
    .map_err(DecodeError::InvalidValue)
}

fn decode_record(c: &mut Cursor<'_>, s: &mut State, depth: u32) -> Result<Value, DecodeError> {
    let count = c.u32()?;
    s.collection(count)?;
    let mut fields = Vec::with_capacity(s.capacity(count, c.remaining(), 3)?);
    let mut previous = None;
    for _ in 0..count {
        let id = c.u16()?;
        if previous.is_some_and(|p| p >= id) {
            return Err(DecodeError::NonCanonicalRecord);
        }
        previous = Some(id);
        fields.push(Field::new(id, inner(c, s, next(depth)?)?));
    }
    Value::record_canonical(fields).map_err(DecodeError::InvalidValue)
}

fn decode_sum(c: &mut Cursor<'_>, s: &mut State, depth: u32) -> Result<Value, DecodeError> {
    let type_id = c.u32()?;
    let variant = c.u16()?;
    let payload = match c.u8()? {
        0 => None,
        1 => Some(inner(c, s, next(depth)?)?),
        _ => return Err(DecodeError::InvalidSumFlag),
    };
    Ok(Value::sum(type_id, variant, payload))
}

fn decode_map(c: &mut Cursor<'_>, s: &mut State, depth: u32) -> Result<Value, DecodeError> {
    let count = c.u32()?;
    s.collection(count)?;
    let mut entries = Vec::with_capacity(s.capacity(count, c.remaining(), 10)?);
    let mut previous: Option<&[u8]> = None;
    for _ in 0..count {
        let key_bytes = c.blob(s.limits.max_payload_bytes)?;
        s.payload(key_bytes.len())?;
        if previous.is_some_and(|p| p >= key_bytes) {
            return Err(DecodeError::NonCanonicalMap);
        }
        let value_bytes = c.blob(s.limits.max_payload_bytes)?;
        s.payload(value_bytes.len())?;
        let key = complete(key_bytes, s, next(depth)?)?;
        let value = complete(value_bytes, s, next(depth)?)?;
        let entry = MapEntry::try_new_with_limits(key, value, s.limits)
            .map_err(DecodeError::InvalidValue)?;
        if entry.encoded_key() != key_bytes {
            return Err(DecodeError::NonCanonical);
        }
        previous = Some(key_bytes);
        entries.push(entry);
    }
    Value::map_canonical(entries).map_err(DecodeError::InvalidValue)
}

#[cfg(test)]
mod tests;
