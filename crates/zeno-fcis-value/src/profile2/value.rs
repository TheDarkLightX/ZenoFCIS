//! Closed, owned ZCVE/2 values. Admission and encoding grant no decision authority.
use super::{I256, U256, unsigned_payload};
pub use crate::{ValueError, ValueLimits, ValueMetrics};
use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt;

/// Immutable field in stable identifier order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    id: u16,
    value: Value,
}
impl Field {
    /// Owns a field; the enclosing record checks order and uniqueness.
    #[must_use]
    pub const fn new(id: u16, value: Value) -> Self {
        Self { id, value }
    }
    /// Stable field identifier.
    #[must_use]
    pub const fn id(&self) -> u16 {
        self.id
    }
    /// Immutable field value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }
    /// Consumes the field.
    #[must_use]
    pub fn into_parts(self) -> (u16, Value) {
        (self.id, self.value)
    }
}

/// Map key ordering bytes are derived internally, never supplied by a caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapEntry {
    encoded_key: Box<[u8]>,
    key: Value,
    value: Value,
}
impl MapEntry {
    /// Owns a key/value and derives key bytes under default limits.
    pub fn try_new(key: Value, value: Value) -> Result<Self, ValueError> {
        Self::try_new_with_limits(key, value, ValueLimits::default())
    }
    /// Derives key bytes under exactly the supplied limits.
    pub fn try_new_with_limits(
        key: Value,
        value: Value,
        limits: ValueLimits,
    ) -> Result<Self, ValueError> {
        let encoded_key = key.zcve_bytes_with_limits(limits)?;
        wire_length(encoded_key.len())?;
        Ok(Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        })
    }
    /// Canonical bytes used for lexicographic key order, not numeric order.
    #[must_use]
    pub fn encoded_key(&self) -> &[u8] {
        &self.encoded_key
    }
    /// Immutable key.
    #[must_use]
    pub fn key(&self) -> &Value {
        &self.key
    }
    /// Immutable value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }
    /// Consumes the entry.
    #[must_use]
    pub fn into_parts(self) -> (Box<[u8]>, Value, Value) {
        (self.encoded_key, self.key, self.value)
    }
}

/// Immutable inspection; this view cannot construct an admission witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueRef<'a> {
    /// Unit.
    Unit,
    /// Boolean.
    Bool(bool),
    /// Legacy unsigned integer.
    U128(u128),
    /// Legacy signed integer.
    I128(i128),
    /// Full unsigned256 integer.
    U256(U256),
    /// Full signed256 integer.
    I256(I256),
    /// Bytes.
    Bytes(&'a [u8]),
    /// ASCII text.
    Text(&'a str),
    /// Stable enum identifiers.
    Enum {
        /// Stable type identifier.
        type_id: u32,
        /// Stable variant identifier.
        variant: u16,
    },
    /// Tuple.
    Tuple(&'a [Value]),
    /// Ordered fields.
    Record(&'a [Field]),
    /// Sum and its optional payload.
    Sum {
        /// Stable type identifier.
        type_id: u32,
        /// Stable variant identifier.
        variant: u16,
        /// Immutable optional payload.
        payload: Option<&'a Value>,
    },
    /// Vector.
    Vector(&'a [Value]),
    /// Ordered map.
    Map(&'a [MapEntry]),
}

/// Kind is separate from encoded-key order and never coerces integer widths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueKind {
    /// Unit.
    Unit,
    /// Boolean.
    Bool,
    /// Unsigned128.
    U128,
    /// Signed128.
    I128,
    /// Unsigned256.
    U256,
    /// Signed256.
    I256,
    /// Bytes.
    Bytes,
    /// ASCII text.
    Text,
    /// Enum.
    Enum,
    /// Tuple.
    Tuple,
    /// Record.
    Record,
    /// Sum.
    Sum,
    /// Vector.
    Vector,
    /// Map.
    Map,
}

/// Complete structurally canonical profile2 value with privately owned storage.
/// Structural construction is not resource admission; use [`AdmittedValue`].
/// ```compile_fail
/// use zeno_fcis_value::profile2::Value;
/// let _ = Value { repr: () };
/// ```
/// ```compile_fail
/// use zeno_fcis_value::profile2::{Value, ValueRef};
/// let value = Value::bytes(vec![1]).unwrap();
/// if let ValueRef::Bytes(bytes) = value.view() { bytes[0] = 2; }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Value {
    repr: Repr,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Repr {
    Unit,
    Bool(bool),
    U128(u128),
    I128(i128),
    U256(U256),
    I256(I256),
    Bytes(Box<[u8]>),
    Text(Box<str>),
    Enum(u32, u16),
    Tuple(Box<[Value]>),
    Record(Box<[Field]>),
    Sum(u32, u16, Option<Box<Value>>),
    Vector(Box<[Value]>),
    Map(Box<[MapEntry]>),
}

impl Value {
    /// Unit.
    #[must_use]
    pub const fn unit() -> Self {
        Self { repr: Repr::Unit }
    }
    /// Boolean.
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self {
            repr: Repr::Bool(value),
        }
    }
    /// Distinct legacy-width unsigned integer.
    #[must_use]
    pub const fn unsigned(value: u128) -> Self {
        Self {
            repr: Repr::U128(value),
        }
    }
    /// Distinct legacy-width signed integer.
    #[must_use]
    pub const fn signed(value: i128) -> Self {
        Self {
            repr: Repr::I128(value),
        }
    }
    /// Full unsigned256 integer.
    #[must_use]
    pub const fn unsigned256(value: U256) -> Self {
        Self {
            repr: Repr::U256(value),
        }
    }
    /// Full signed256 integer.
    #[must_use]
    pub const fn signed256(value: I256) -> Self {
        Self {
            repr: Repr::I256(value),
        }
    }
    /// Stable enum identifiers, including zero.
    #[must_use]
    pub const fn enumeration(type_id: u32, variant: u16) -> Self {
        Self {
            repr: Repr::Enum(type_id, variant),
        }
    }
    /// Owns an optional payload; admission checks its depth and work.
    #[must_use]
    pub fn sum(type_id: u32, variant: u16, payload: Option<Self>) -> Self {
        Self {
            repr: Repr::Sum(type_id, variant, payload.map(Box::new)),
        }
    }
    /// Owns bytes under default structural limits.
    pub fn bytes(bytes: Vec<u8>) -> Result<Self, ValueError> {
        Self::bytes_with_limits(bytes, ValueLimits::default())
    }
    /// Owns bytes admitted under exactly supplied limits.
    pub fn bytes_with_limits(bytes: Vec<u8>, limits: ValueLimits) -> Result<Self, ValueError> {
        wire_length(bytes.len())?;
        let value = Self {
            repr: Repr::Bytes(bytes.into_boxed_slice()),
        };
        value.validate_limits(limits)?;
        Ok(value)
    }
    /// Owns ASCII text under default limits.
    pub fn text_ascii(text: String) -> Result<Self, ValueError> {
        Self::text_ascii_with_limits(text, ValueLimits::default())
    }
    /// NonASCII refusal precedes wire-length and structural-limit refusal.
    pub fn text_ascii_with_limits(text: String, limits: ValueLimits) -> Result<Self, ValueError> {
        if !text.is_ascii() {
            return Err(ValueError::NonAsciiText);
        }
        wire_length(text.len())?;
        let value = Self {
            repr: Repr::Text(text.into_boxed_str()),
        };
        value.validate_limits(limits)?;
        Ok(value)
    }
    /// Owns a tuple; admission remains explicit.
    pub fn tuple(items: Vec<Self>) -> Result<Self, ValueError> {
        wire_length(items.len())?;
        Ok(Self {
            repr: Repr::Tuple(items.into_boxed_slice()),
        })
    }
    /// Owns a vector; admission remains explicit.
    pub fn vector(items: Vec<Self>) -> Result<Self, ValueError> {
        wire_length(items.len())?;
        Ok(Self {
            repr: Repr::Vector(items.into_boxed_slice()),
        })
    }
    /// Requires strictly increasing field IDs; duplicates are never combined.
    pub fn record_canonical(fields: Vec<Field>) -> Result<Self, ValueError> {
        wire_length(fields.len())?;
        fields_ordered(&fields)?;
        Ok(Self {
            repr: Repr::Record(fields.into_boxed_slice()),
        })
    }
    /// Sorts by stable field ID and refuses duplicates.
    pub fn normalize_record(mut fields: Vec<Field>) -> Result<Self, ValueError> {
        fields.sort_by_key(Field::id);
        Self::record_canonical(fields)
    }
    /// Requires strictly increasing encoded keys and representable value blobs.
    pub fn map_canonical(entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        wire_length(entries.len())?;
        keys_ordered(&entries)?;
        for entry in &entries {
            wire_length(entry.value.encoded_length()?)?;
        }
        Ok(Self {
            repr: Repr::Map(entries.into_boxed_slice()),
        })
    }
    /// Sorts by complete canonical key bytes, never numeric comparison.
    pub fn normalize_map(mut entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        entries.sort_by(|a, b| a.encoded_key.cmp(&b.encoded_key));
        Self::map_canonical(entries)
    }
    /// Borrows immutable children and copies fixed numeric leaves.
    #[must_use]
    pub fn view(&self) -> ValueRef<'_> {
        match &self.repr {
            Repr::Unit => ValueRef::Unit,
            Repr::Bool(v) => ValueRef::Bool(*v),
            Repr::U128(v) => ValueRef::U128(*v),
            Repr::I128(v) => ValueRef::I128(*v),
            Repr::U256(v) => ValueRef::U256(*v),
            Repr::I256(v) => ValueRef::I256(*v),
            Repr::Bytes(v) => ValueRef::Bytes(v),
            Repr::Text(v) => ValueRef::Text(v),
            Repr::Enum(t, v) => ValueRef::Enum {
                type_id: *t,
                variant: *v,
            },
            Repr::Tuple(v) => ValueRef::Tuple(v),
            Repr::Record(v) => ValueRef::Record(v),
            Repr::Sum(t, v, p) => ValueRef::Sum {
                type_id: *t,
                variant: *v,
                payload: p.as_deref(),
            },
            Repr::Vector(v) => ValueRef::Vector(v),
            Repr::Map(v) => ValueRef::Map(v),
        }
    }
    /// Complete structural kind.
    #[must_use]
    pub const fn kind(&self) -> ValueKind {
        match &self.repr {
            Repr::Unit => ValueKind::Unit,
            Repr::Bool(_) => ValueKind::Bool,
            Repr::U128(_) => ValueKind::U128,
            Repr::I128(_) => ValueKind::I128,
            Repr::U256(_) => ValueKind::U256,
            Repr::I256(_) => ValueKind::I256,
            Repr::Bytes(_) => ValueKind::Bytes,
            Repr::Text(_) => ValueKind::Text,
            Repr::Enum(..) => ValueKind::Enum,
            Repr::Tuple(_) => ValueKind::Tuple,
            Repr::Record(_) => ValueKind::Record,
            Repr::Sum(..) => ValueKind::Sum,
            Repr::Vector(_) => ValueKind::Vector,
            Repr::Map(_) => ValueKind::Map,
        }
    }
    /// Exact shared structural counters, root at depth0 and hard ceiling1024.
    pub fn validate_limits(&self, limits: ValueLimits) -> Result<ValueMetrics, ValueError> {
        let mut metrics = ValueMetrics::default();
        validate(self, 0, limits, &mut metrics)?;
        Ok(metrics)
    }
    /// Exact checked wire length; excessive depth refuses before deeper descent.
    pub fn encoded_length(&self) -> Result<usize, ValueError> {
        length(self, 0)
    }
    /// Default-limit canonical bytes; no profile inference occurs.
    pub fn zcve_bytes(&self) -> Result<Vec<u8>, ValueError> {
        self.zcve_bytes_with_limits(ValueLimits::default())
    }
    /// Canonical bytes after exact supplied limit admission.
    pub fn zcve_bytes_with_limits(&self, limits: ValueLimits) -> Result<Vec<u8>, ValueError> {
        let mut out = Vec::new();
        self.encode_zcve_to_with_limits(&mut out, limits)?;
        Ok(out)
    }
    /// Prefix-preserving default-limit encoding.
    pub fn encode_zcve_to(&self, out: &mut Vec<u8>) -> Result<(), ValueError> {
        self.encode_zcve_to_with_limits(out, ValueLimits::default())
    }
    /// All fallible work finishes in owned scratch before any output append.
    pub fn encode_zcve_to_with_limits(
        &self,
        out: &mut Vec<u8>,
        limits: ValueLimits,
    ) -> Result<(), ValueError> {
        self.validate_limits(limits)?;
        append_validated(self, out)
    }
    /// Explicit bounded deep lift; legacy bare bytes remain identical.
    pub fn lift_legacy(value: &crate::Value, limits: ValueLimits) -> Result<Self, ValueError> {
        value.validate_limits(limits)?;
        let lifted = lift(value)?;
        lifted.validate_limits(limits)?;
        Ok(lifted)
    }
    /// Checks the complete supplied limits before converting or allocating output.
    /// Resource refusal precedes depth-first wire-order conversion; every nested
    /// wide leaf refuses, even when its magnitude fits128. No partial result escapes.
    pub fn try_into_legacy(self, limits: ValueLimits) -> Result<crate::Value, ConversionError> {
        self.validate_limits(limits)
            .map_err(ConversionError::InvalidValue)?;
        narrow(self, limits)
    }
}

/// Narrowing never silently changes a wide type into a legacy type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversionError {
    /// At least one nested U256/I256 leaf exists.
    WideInteger,
    /// Supplied limits or legacy construction refused.
    InvalidValue(ValueError),
}
impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WideInteger => f.write_str("wide integer has no legacy value representation"),
            Self::InvalidValue(e) => write!(f, "{e}"),
        }
    }
}
impl core::error::Error for ConversionError {}

/// Owned resource admission with exact retained metrics; no public constructor fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedValue {
    value: Value,
    metrics: ValueMetrics,
    limits: ValueLimits,
}
impl AdmittedValue {
    /// Admits under default limits.
    pub fn try_new(value: Value) -> Result<Self, ValueError> {
        Self::try_new_with_limits(value, ValueLimits::default())
    }
    /// Admits exactly the supplied limits and retains the measured counters.
    pub fn try_new_with_limits(value: Value, limits: ValueLimits) -> Result<Self, ValueError> {
        let metrics = value.validate_limits(limits)?;
        Ok(Self {
            value,
            metrics,
            limits,
        })
    }
    /// Immutable admitted value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
    /// Actual admission limits.
    #[must_use]
    pub const fn limits(&self) -> ValueLimits {
        self.limits
    }
    /// Actual structural metrics.
    #[must_use]
    pub const fn metrics(&self) -> ValueMetrics {
        self.metrics
    }
    /// Consumes the witness.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }
    /// Encodes the retained immutable admitted value without changing the prefix on refusal.
    pub fn encode_zcve_to(&self, out: &mut Vec<u8>) -> Result<(), ValueError> {
        append_validated(&self.value, out)
    }
}

fn wire_length(n: usize) -> Result<(), ValueError> {
    u32::try_from(n)
        .map(|_| ())
        .map_err(|_| ValueError::ArithmeticOverflow)
}
fn add(a: usize, b: usize) -> Result<usize, ValueError> {
    a.checked_add(b).ok_or(ValueError::ArithmeticOverflow)
}
fn fields_ordered(fields: &[Field]) -> Result<(), ValueError> {
    for pair in fields.windows(2) {
        if pair[0].id >= pair[1].id {
            return Err(ValueError::RecordFieldOrder {
                previous: pair[0].id,
                current: pair[1].id,
            });
        }
    }
    Ok(())
}
fn keys_ordered(entries: &[MapEntry]) -> Result<(), ValueError> {
    for pair in entries.windows(2) {
        if pair[0].encoded_key >= pair[1].encoded_key {
            return Err(ValueError::MapKeyOrder);
        }
    }
    Ok(())
}
fn child_depth(depth: u32) -> Result<u32, ValueError> {
    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)
}
fn depth_check(depth: u32, limit: u32) -> Result<(), ValueError> {
    if depth > limit {
        Err(ValueError::DepthLimit {
            limit,
            attempted: depth,
        })
    } else {
        Ok(())
    }
}
fn collection(n: usize, limits: ValueLimits) -> Result<(), ValueError> {
    let attempted = u32::try_from(n).map_err(|_| ValueError::ArithmeticOverflow)?;
    if attempted > limits.max_collection_len {
        Err(ValueError::CollectionLimit {
            limit: limits.max_collection_len,
            attempted,
        })
    } else {
        Ok(())
    }
}
fn payload(n: usize, limits: ValueLimits, m: &mut ValueMetrics) -> Result<(), ValueError> {
    m.payload_bytes = m
        .payload_bytes
        .checked_add(u64::try_from(n).map_err(|_| ValueError::ArithmeticOverflow)?)
        .ok_or(ValueError::ArithmeticOverflow)?;
    if m.payload_bytes > limits.max_payload_bytes {
        return Err(ValueError::PayloadLimit {
            limit: limits.max_payload_bytes,
            attempted: m.payload_bytes,
        });
    }
    Ok(())
}
fn validate(
    v: &Value,
    depth: u32,
    limits: ValueLimits,
    m: &mut ValueMetrics,
) -> Result<(), ValueError> {
    depth_check(depth, limits.effective_max_depth())?;
    m.depth = m.depth.max(depth);
    m.nodes = m
        .nodes
        .checked_add(1)
        .ok_or(ValueError::ArithmeticOverflow)?;
    if m.nodes > limits.max_nodes {
        return Err(ValueError::NodeLimit {
            limit: limits.max_nodes,
            attempted: m.nodes,
        });
    }
    match &v.repr {
        Repr::Bytes(b) => payload(b.len(), limits, m)?,
        Repr::Text(t) => {
            if !t.is_ascii() {
                return Err(ValueError::NonAsciiText);
            }
            payload(t.len(), limits, m)?;
        }
        Repr::Tuple(items) | Repr::Vector(items) => {
            collection(items.len(), limits)?;
            for v in items.iter() {
                validate(v, child_depth(depth)?, limits, m)?;
            }
        }
        Repr::Record(fields) => {
            collection(fields.len(), limits)?;
            fields_ordered(fields)?;
            for f in fields.iter() {
                validate(&f.value, child_depth(depth)?, limits, m)?;
            }
        }
        Repr::Sum(_, _, Some(v)) => validate(v, child_depth(depth)?, limits, m)?,
        Repr::Map(entries) => {
            collection(entries.len(), limits)?;
            keys_ordered(entries)?;
            for e in entries.iter() {
                payload(e.encoded_key.len(), limits, m)?;
                payload(e.value.encoded_length()?, limits, m)?;
                validate(&e.key, child_depth(depth)?, limits, m)?;
                validate(&e.value, child_depth(depth)?, limits, m)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn length(v: &Value, depth: u32) -> Result<usize, ValueError> {
    depth_check(depth, ValueLimits::MAX_SUPPORTED_DEPTH)?;
    match &v.repr {
        Repr::Unit | Repr::Bool(_) => Ok(1),
        Repr::U128(_) | Repr::I128(_) => Ok(17),
        Repr::U256(_) | Repr::I256(_) => Ok(33),
        Repr::Enum(..) => Ok(7),
        Repr::Bytes(b) => {
            wire_length(b.len())?;
            add(5, b.len())
        }
        Repr::Text(t) => {
            wire_length(t.len())?;
            add(5, t.len())
        }
        Repr::Tuple(items) | Repr::Vector(items) => {
            wire_length(items.len())?;
            let mut n = 5;
            for v in items.iter() {
                n = add(n, length(v, child_depth(depth)?)?)?;
            }
            Ok(n)
        }
        Repr::Record(fields) => {
            wire_length(fields.len())?;
            let mut n = 5;
            for f in fields.iter() {
                n = add(add(n, 2)?, length(&f.value, child_depth(depth)?)?)?;
            }
            Ok(n)
        }
        Repr::Sum(_, _, p) => match p {
            None => Ok(8),
            Some(v) => add(8, length(v, child_depth(depth)?)?),
        },
        Repr::Map(entries) => {
            wire_length(entries.len())?;
            let mut n = 5;
            for e in entries.iter() {
                let k = length(&e.key, child_depth(depth)?)?;
                let v = length(&e.value, child_depth(depth)?)?;
                wire_length(v)?;
                wire_length(k)?;
                n = add(add(add(n, 8)?, k)?, v)?;
            }
            Ok(n)
        }
    }
}
fn append_validated(v: &Value, output: &mut Vec<u8>) -> Result<(), ValueError> {
    let mut scratch = Vec::new();
    encode(v, &mut scratch)?;
    output
        .len()
        .checked_add(scratch.len())
        .ok_or(ValueError::ArithmeticOverflow)?;
    output.extend_from_slice(&scratch);
    Ok(())
}
fn count(out: &mut Vec<u8>, n: usize) -> Result<(), ValueError> {
    let n = u32::try_from(n).map_err(|_| ValueError::ArithmeticOverflow)?;
    out.extend_from_slice(&n.to_be_bytes());
    Ok(())
}
fn blob(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ValueError> {
    count(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}
fn encode(v: &Value, out: &mut Vec<u8>) -> Result<(), ValueError> {
    use crate::zcve::*;
    match &v.repr {
        Repr::Unit => out.push(TAG_UNIT),
        Repr::Bool(b) => out.push(if *b { TAG_BOOL_TRUE } else { TAG_BOOL_FALSE }),
        Repr::U128(n) => {
            out.push(TAG_U128);
            out.extend_from_slice(&n.to_be_bytes());
        }
        Repr::I128(n) => {
            out.push(TAG_I128);
            out.extend_from_slice(&n.to_be_bytes());
        }
        Repr::U256(n) => {
            out.push(super::TAG_U256);
            out.extend_from_slice(&unsigned_payload(*n));
        }
        Repr::I256(n) => {
            out.push(super::TAG_I256);
            out.extend_from_slice(&unsigned_payload(n.bits()));
        }
        Repr::Bytes(b) => {
            out.push(TAG_BYTES);
            blob(out, b)?;
        }
        Repr::Text(t) => {
            out.push(TAG_TEXT);
            blob(out, t.as_bytes())?;
        }
        Repr::Enum(t, v) => {
            out.push(TAG_ENUM);
            out.extend_from_slice(&t.to_be_bytes());
            out.extend_from_slice(&v.to_be_bytes());
        }
        Repr::Tuple(items) | Repr::Vector(items) => {
            out.push(if matches!(&v.repr, Repr::Tuple(_)) {
                TAG_TUPLE
            } else {
                TAG_VECTOR
            });
            count(out, items.len())?;
            for v in items.iter() {
                encode(v, out)?;
            }
        }
        Repr::Record(fields) => {
            out.push(TAG_RECORD);
            count(out, fields.len())?;
            for f in fields.iter() {
                out.extend_from_slice(&f.id.to_be_bytes());
                encode(&f.value, out)?;
            }
        }
        Repr::Sum(t, v, p) => {
            out.push(TAG_SUM);
            out.extend_from_slice(&t.to_be_bytes());
            out.extend_from_slice(&v.to_be_bytes());
            out.push(u8::from(p.is_some()));
            if let Some(v) = p {
                encode(v, out)?;
            }
        }
        Repr::Map(entries) => {
            out.push(TAG_MAP);
            count(out, entries.len())?;
            for e in entries.iter() {
                blob(out, &e.encoded_key)?;
                let mut b = Vec::new();
                encode(&e.value, &mut b)?;
                blob(out, &b)?;
            }
        }
    }
    Ok(())
}
fn lift(v: &crate::Value) -> Result<Value, ValueError> {
    use crate::ValueRef as L;
    Ok(match v.view() {
        L::Unit => Value::unit(),
        L::Bool(v) => Value::boolean(v),
        L::U128(v) => Value::unsigned(v),
        L::I128(v) => Value::signed(v),
        L::Bytes(v) => Value {
            repr: Repr::Bytes(v.to_vec().into_boxed_slice()),
        },
        L::Text(v) => Value {
            repr: Repr::Text(String::from(v).into_boxed_str()),
        },
        L::Enum { type_id, variant } => Value::enumeration(type_id, variant),
        L::Tuple(v) => Value::tuple(v.iter().map(lift).collect::<Result<Vec<_>, _>>()?)?,
        L::Vector(v) => Value::vector(v.iter().map(lift).collect::<Result<Vec<_>, _>>()?)?,
        L::Record(v) => Value::record_canonical(
            v.iter()
                .map(|f| Ok(Field::new(f.id(), lift(f.value())?)))
                .collect::<Result<Vec<_>, ValueError>>()?,
        )?,
        L::Sum {
            type_id,
            variant,
            payload,
        } => Value::sum(type_id, variant, payload.map(lift).transpose()?),
        L::Map(entries) => Value::map_canonical(
            entries
                .iter()
                .map(|e| {
                    Ok(MapEntry {
                        encoded_key: e.encoded_key().to_vec().into_boxed_slice(),
                        key: lift(e.key())?,
                        value: lift(e.value())?,
                    })
                })
                .collect::<Result<Vec<_>, ValueError>>()?,
        )?,
    })
}
fn narrow(v: Value, limits: ValueLimits) -> Result<crate::Value, ConversionError> {
    use crate::Value as L;
    let invalid = ConversionError::InvalidValue;
    Ok(match v.repr {
        Repr::Unit => L::unit(),
        Repr::Bool(v) => L::boolean(v),
        Repr::U128(v) => L::unsigned(v),
        Repr::I128(v) => L::signed(v),
        Repr::U256(_) | Repr::I256(_) => return Err(ConversionError::WideInteger),
        Repr::Bytes(v) => L::bytes_with_limits(v.into_vec(), limits).map_err(invalid)?,
        Repr::Text(v) => L::text_ascii_with_limits(v.into_string(), limits).map_err(invalid)?,
        Repr::Enum(t, v) => L::enumeration(t, v),
        Repr::Sum(t, v, p) => L::sum(t, v, p.map(|v| narrow(*v, limits)).transpose()?),
        Repr::Tuple(v) => L::tuple(
            v.into_vec()
                .into_iter()
                .map(|v| narrow(v, limits))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(invalid)?,
        Repr::Vector(v) => L::vector(
            v.into_vec()
                .into_iter()
                .map(|v| narrow(v, limits))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(invalid)?,
        Repr::Record(v) => L::record_canonical(
            v.into_vec()
                .into_iter()
                .map(|f| Ok(crate::Field::new(f.id, narrow(f.value, limits)?)))
                .collect::<Result<Vec<_>, ConversionError>>()?,
        )
        .map_err(invalid)?,
        Repr::Map(v) => L::map_canonical(
            v.into_vec()
                .into_iter()
                .map(|e| {
                    let key = narrow(e.key, limits)?;
                    let value = narrow(e.value, limits)?;
                    crate::MapEntry::try_new_with_limits(key, value, limits).map_err(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(invalid)?,
    })
}
