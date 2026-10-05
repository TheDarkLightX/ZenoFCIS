//! Transitively immutable, bounded value types for ZenoFCIS.
//!
//! Values expose no interior mutation. Variable-sized children are owned by
//! boxed slices or boxed strings so committed values cannot retain mutable
//! caller aliases.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// ZCVE/1 value tags shared by construction and decoding.
///
/// These identifiers are protocol meaning. They are exposed so the codec can
/// decode the exact byte format whose encoder lives with [`Value`].
pub mod zcve {
    /// Unit value tag.
    pub const TAG_UNIT: u8 = 0x00;
    /// Boolean false tag.
    pub const TAG_BOOL_FALSE: u8 = 0x01;
    /// Boolean true tag.
    pub const TAG_BOOL_TRUE: u8 = 0x02;
    /// Unsigned 128-bit integer tag.
    pub const TAG_U128: u8 = 0x03;
    /// Signed 128-bit integer tag.
    pub const TAG_I128: u8 = 0x04;
    /// Byte string tag.
    pub const TAG_BYTES: u8 = 0x05;
    /// ASCII text tag.
    pub const TAG_TEXT: u8 = 0x06;
    /// Closed enum tag.
    pub const TAG_ENUM: u8 = 0x07;
    /// Tuple tag.
    pub const TAG_TUPLE: u8 = 0x08;
    /// Record tag.
    pub const TAG_RECORD: u8 = 0x09;
    /// Closed sum tag.
    pub const TAG_SUM: u8 = 0x0a;
    /// Vector tag.
    pub const TAG_VECTOR: u8 = 0x0b;
    /// Map tag.
    pub const TAG_MAP: u8 = 0x0c;
}

/// A length-bound violation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LengthError {
    minimum: usize,
    maximum: usize,
    actual: usize,
}

impl LengthError {
    /// Returns the minimum permitted length.
    #[must_use]
    pub const fn minimum(self) -> usize {
        self.minimum
    }

    /// Returns the maximum permitted length.
    #[must_use]
    pub const fn maximum(self) -> usize {
        self.maximum
    }

    /// Returns the observed length.
    #[must_use]
    pub const fn actual(self) -> usize {
        self.actual
    }
}

impl core::error::Error for LengthError {}

impl fmt::Display for LengthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "length {} is outside inclusive range {}..={}",
            self.actual, self.minimum, self.maximum
        )
    }
}

/// An immutable vector with compile-time declared inclusive length bounds.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundedVec<T, const MIN: usize, const MAX: usize> {
    items: Box<[T]>,
}

impl<T, const MIN: usize, const MAX: usize> BoundedVec<T, MIN, MAX> {
    /// Owns a vector after validating its length.
    pub fn try_from_vec(items: Vec<T>) -> Result<Self, LengthError> {
        let actual = items.len();
        if MIN > MAX || actual < MIN || actual > MAX {
            return Err(LengthError {
                minimum: MIN,
                maximum: MAX,
                actual,
            });
        }
        Ok(Self {
            items: items.into_boxed_slice(),
        })
    }

    /// Returns the immutable slice.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// Returns the number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether the value is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Consumes the value and returns the owned slice.
    #[must_use]
    pub fn into_boxed_slice(self) -> Box<[T]> {
        self.items
    }
}

/// A non-empty immutable vector.
pub type NonEmptyVec<T> = BoundedVec<T, 1, { usize::MAX }>;

/// Immutable bounded bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OwnedBytes<const MAX: usize> {
    bytes: Box<[u8]>,
}

impl<const MAX: usize> OwnedBytes<MAX> {
    /// Owns bytes after validating the upper bound.
    pub fn try_from_vec(bytes: Vec<u8>) -> Result<Self, LengthError> {
        let actual = bytes.len();
        if actual > MAX {
            return Err(LengthError {
                minimum: 0,
                maximum: MAX,
                actual,
            });
        }
        Ok(Self {
            bytes: bytes.into_boxed_slice(),
        })
    }

    /// Returns the immutable bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

/// A bounded ASCII protocol label.
///
/// The initial kernel intentionally rejects non-ASCII text. A future Unicode
/// profile may add NFC-normalized text behind a separately versioned codec.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsciiText<const MAX: usize> {
    text: Box<str>,
}

impl<const MAX: usize> AsciiText<MAX> {
    /// Owns a protocol label after validating ASCII and byte length.
    pub fn try_from_string(text: String) -> Result<Self, TextError> {
        let actual = text.len();
        if actual > MAX {
            return Err(TextError::TooLong(LengthError {
                minimum: 0,
                maximum: MAX,
                actual,
            }));
        }
        if !text.is_ascii() {
            return Err(TextError::NonAscii);
        }
        Ok(Self {
            text: text.into_boxed_str(),
        })
    }

    /// Returns the text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// Protocol-text construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TextError {
    /// The initial profile accepts ASCII only.
    NonAscii,
    /// The encoded text exceeds its bound.
    TooLong(LengthError),
}

impl core::error::Error for TextError {}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAscii => formatter.write_str("protocol text must be ASCII"),
            Self::TooLong(error) => error.fmt(formatter),
        }
    }
}

/// A record field ordered by stable numeric field identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Field {
    id: u16,
    value: Value,
}

impl Field {
    /// Creates a field.
    #[must_use]
    pub const fn new(id: u16, value: Value) -> Self {
        Self { id, value }
    }

    /// Returns the stable field identifier.
    #[must_use]
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// Returns the field value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the field and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (u16, Value) {
        (self.id, self.value)
    }
}

/// A canonical map entry carrying the authoritative encoded key ordering.
///
/// Encoded key bytes cannot be supplied independently:
///
/// ```compile_fail
/// use zeno_fcis_value::{MapEntry, Value};
///
/// let _ = MapEntry::new(vec![0], Value::unit(), Value::unit());
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MapEntry {
    encoded_key: Box<[u8]>,
    key: Value,
    value: Value,
}

impl MapEntry {
    /// Creates an entry whose ordering bytes are derived from the semantic key.
    ///
    /// Callers cannot supply the encoded key independently. The enclosing map
    /// still validates strict ordering and uniqueness.
    pub fn try_new(key: Value, value: Value) -> Result<Self, ValueError> {
        Self::try_new_with_limits(key, value, ValueLimits::default())
    }

    /// Derives key bytes using exactly the supplied value limits.
    pub fn try_new_with_limits(
        key: Value,
        value: Value,
        limits: ValueLimits,
    ) -> Result<Self, ValueError> {
        let encoded_key = key.zcve_bytes_with_limits(limits)?;
        check_wire_length(encoded_key.len())?;
        Ok(Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        })
    }

    /// Returns the authoritative encoded key bytes.
    #[must_use]
    pub fn encoded_key(&self) -> &[u8] {
        &self.encoded_key
    }

    /// Returns the semantic key.
    #[must_use]
    pub fn key(&self) -> &Value {
        &self.key
    }

    /// Returns the semantic value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (Box<[u8]>, Value, Value) {
        (self.encoded_key, self.key, self.value)
    }
}

/// An immutable borrowed inspection of a value, never a construction witness.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueRef<'a> {
    /// Unit.
    Unit,
    /// Boolean.
    Bool(bool),
    /// Unsigned integer.
    U128(u128),
    /// Signed integer.
    I128(i128),
    /// Exact bytes.
    Bytes(&'a [u8]),
    /// ASCII text.
    Text(&'a str),
    /// Exact enum identifiers.
    Enum {
        /// Type identifier.
        type_id: u32,
        /// Variant identifier.
        variant: u16,
    },
    /// Tuple items.
    Tuple(&'a [Value]),
    /// Ordered record fields.
    Record(&'a [Field]),
    /// Exact sum identifiers and optional payload.
    Sum {
        /// Type identifier.
        type_id: u32,
        /// Variant identifier.
        variant: u16,
        /// Immutable optional payload.
        payload: Option<&'a Value>,
    },
    /// Vector items.
    Vector(&'a [Value]),
    /// Ordered map entries.
    Map(&'a [MapEntry]),
}

/// A structurally canonical, transitively owned protocol value.
///
/// Construction checks ASCII, ordered fields/keys and fixed wire lengths.
/// Resource admission is explicit through [`Self::validate_limits`]. A value
/// can be well formed while exceeding a caller's resource policy.
///
/// ```compile_fail
/// use zeno_fcis_value::Value;
/// let _ = Value { repr: () };
/// ```
/// ```compile_fail
/// use zeno_fcis_value::Value;
/// let _ = Value::Text("non-ASCII: é".into());
/// ```
/// ```compile_fail
/// use zeno_fcis_value::ValueRepr;
/// ```
/// ```compile_fail
/// use zeno_fcis_value::{Value, ValueRef};
/// let value = Value::bytes(vec![1]).unwrap();
/// if let ValueRef::Bytes(bytes) = value.view() { bytes[0] = 2; }
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Value {
    repr: ValueRepr,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ValueRepr {
    /// Unit value.
    Unit,
    /// Boolean value.
    Bool(bool),
    /// Unsigned 128-bit integer.
    U128(u128),
    /// Signed 128-bit integer.
    I128(i128),
    /// Owned bytes.
    Bytes(Box<[u8]>),
    /// ASCII text.
    Text(Box<str>),
    /// Closed enum variant identified by type and variant ordinals.
    Enum {
        /// Stable enum type identifier.
        type_id: u32,
        /// Stable variant ordinal.
        variant: u16,
    },
    /// Fixed-position product value.
    Tuple(Box<[Value]>),
    /// Stable-field product value.
    Record(Box<[Field]>),
    /// Closed sum value.
    Sum {
        /// Stable sum type identifier.
        type_id: u32,
        /// Stable variant ordinal.
        variant: u16,
        /// Optional variant payload.
        payload: Option<Box<Value>>,
    },
    /// Bounded sequence reference value.
    Vector(Box<[Value]>),
    /// Canonically encoded-key-ordered map.
    Map(Box<[MapEntry]>),
}

impl Value {
    /// Creates Unit.
    #[must_use]
    pub const fn unit() -> Self {
        Self {
            repr: ValueRepr::Unit,
        }
    }
    /// Creates a Boolean.
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self {
            repr: ValueRepr::Bool(value),
        }
    }
    /// Creates a full-width unsigned integer.
    #[must_use]
    pub const fn unsigned(value: u128) -> Self {
        Self {
            repr: ValueRepr::U128(value),
        }
    }
    /// Creates a full-width signed integer.
    #[must_use]
    pub const fn signed(value: i128) -> Self {
        Self {
            repr: ValueRepr::I128(value),
        }
    }
    /// Creates a generic enum; zero and all full-width identifiers are legal.
    #[must_use]
    pub const fn enumeration(type_id: u32, variant: u16) -> Self {
        Self {
            repr: ValueRepr::Enum { type_id, variant },
        }
    }
    /// Owns a sum payload; scalar and payload-bearing variants remain distinct.
    #[must_use]
    pub fn sum(type_id: u32, variant: u16, payload: Option<Self>) -> Self {
        Self {
            repr: ValueRepr::Sum {
                type_id,
                variant,
                payload: payload.map(Box::new),
            },
        }
    }
    /// Borrows every value component without exposing mutable representation.
    #[must_use]
    pub fn view(&self) -> ValueRef<'_> {
        match &self.repr {
            ValueRepr::Unit => ValueRef::Unit,
            ValueRepr::Bool(v) => ValueRef::Bool(*v),
            ValueRepr::U128(v) => ValueRef::U128(*v),
            ValueRepr::I128(v) => ValueRef::I128(*v),
            ValueRepr::Bytes(v) => ValueRef::Bytes(v),
            ValueRepr::Text(v) => ValueRef::Text(v),
            ValueRepr::Enum { type_id, variant } => ValueRef::Enum {
                type_id: *type_id,
                variant: *variant,
            },
            ValueRepr::Tuple(v) => ValueRef::Tuple(v),
            ValueRepr::Record(v) => ValueRef::Record(v),
            ValueRepr::Sum {
                type_id,
                variant,
                payload,
            } => ValueRef::Sum {
                type_id: *type_id,
                variant: *variant,
                payload: payload.as_deref(),
            },
            ValueRepr::Vector(v) => ValueRef::Vector(v),
            ValueRepr::Map(v) => ValueRef::Map(v),
        }
    }
    /// Constructs bytes under exactly the supplied resource limits.
    pub fn bytes_with_limits(bytes: Vec<u8>, limits: ValueLimits) -> Result<Self, ValueError> {
        check_wire_length(bytes.len())?;
        let value = Self {
            repr: ValueRepr::Bytes(bytes.into_boxed_slice()),
        };
        value.validate_limits(limits)?;
        Ok(value)
    }
    /// Constructs ASCII text under exactly the supplied resource limits.
    /// Non-ASCII refusal precedes size and resource refusal.
    pub fn text_ascii_with_limits(text: String, limits: ValueLimits) -> Result<Self, ValueError> {
        if !text.is_ascii() {
            return Err(ValueError::NonAsciiText);
        }
        check_wire_length(text.len())?;
        let value = Self {
            repr: ValueRepr::Text(text.into_boxed_str()),
        };
        value.validate_limits(limits)?;
        Ok(value)
    }

    /// Creates owned bytes admitted by the default payload ceiling.
    ///
    /// A single byte leaf cannot exceed
    /// [`ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`]. Enclosing values must still
    /// call [`Self::validate_limits`] because several individually admitted
    /// leaves can exceed the aggregate payload budget.
    pub fn bytes(bytes: Vec<u8>) -> Result<Self, LengthError> {
        Self::bytes_with_max(bytes, ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES)
    }

    fn bytes_with_max(bytes: Vec<u8>, maximum: usize) -> Result<Self, LengthError> {
        let actual = bytes.len();
        if actual > maximum {
            return Err(LengthError {
                minimum: 0,
                maximum,
                actual,
            });
        }
        Ok(Self {
            repr: ValueRepr::Bytes(bytes.into_boxed_slice()),
        })
    }

    /// Creates ASCII text admitted by the default payload ceiling.
    ///
    /// A single text leaf cannot exceed
    /// [`ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`]. Enclosing values must still
    /// call [`Self::validate_limits`] because several individually admitted
    /// leaves can exceed the aggregate payload budget.
    pub fn text_ascii(text: String) -> Result<Self, TextError> {
        Self::text_ascii_with_max(text, ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES)
    }

    fn text_ascii_with_max(text: String, maximum: usize) -> Result<Self, TextError> {
        if !text.is_ascii() {
            return Err(TextError::NonAscii);
        }
        let actual = text.len();
        if actual > maximum {
            return Err(TextError::TooLong(LengthError {
                minimum: 0,
                maximum,
                actual,
            }));
        }
        Ok(Self {
            repr: ValueRepr::Text(text.into_boxed_str()),
        })
    }

    /// Creates a tuple after checking its canonical u32 item count.
    pub fn tuple(items: Vec<Self>) -> Result<Self, ValueError> {
        check_wire_length(items.len())?;
        Ok(Self {
            repr: ValueRepr::Tuple(items.into_boxed_slice()),
        })
    }

    /// Creates a vector after checking its canonical u32 item count.
    pub fn vector(items: Vec<Self>) -> Result<Self, ValueError> {
        check_wire_length(items.len())?;
        Ok(Self {
            repr: ValueRepr::Vector(items.into_boxed_slice()),
        })
    }

    /// Creates a record only when field identifiers are strictly increasing.
    pub fn record_canonical(fields: Vec<Field>) -> Result<Self, ValueError> {
        check_wire_length(fields.len())?;
        ensure_strict_fields(&fields)?;
        Ok(Self {
            repr: ValueRepr::Record(fields.into_boxed_slice()),
        })
    }

    /// Sorts record fields by identifier and rejects duplicates.
    pub fn normalize_record(mut fields: Vec<Field>) -> Result<Self, ValueError> {
        fields.sort_by_key(Field::id);
        Self::record_canonical(fields)
    }

    /// Creates a map only when encoded keys are strictly increasing.
    pub fn map_canonical(entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        check_wire_length(entries.len())?;
        ensure_strict_map_keys(&entries)?;
        for entry in &entries {
            check_wire_length(entry.value().encoded_length()?)?;
        }
        Ok(Self {
            repr: ValueRepr::Map(entries.into_boxed_slice()),
        })
    }

    /// Sorts entries by encoded key and rejects duplicates.
    pub fn normalize_map(mut entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        entries.sort_by(|left, right| left.encoded_key.cmp(&right.encoded_key));
        Self::map_canonical(entries)
    }

    /// Returns the exact ZCVE/1 bytes for this value.
    ///
    /// This low-level primitive is shared with `zeno-fcis-codec` so map-entry
    /// construction and public canonical encoding cannot drift. It validates
    /// the default closed-value limits before emitting any bytes.
    pub fn zcve_bytes(&self) -> Result<Vec<u8>, ValueError> {
        let mut output = Vec::new();
        self.encode_zcve_to(&mut output)?;
        Ok(output)
    }

    /// Appends the exact ZCVE/1 bytes for this value.
    ///
    /// Most callers should use the codec's `CanonicalEncode` API. This method
    /// exists at the lower dependency ring so [`MapEntry::try_new`] can derive
    /// canonical ordering bytes without accepting a caller-supplied encoding.
    pub fn encode_zcve_to(&self, output: &mut Vec<u8>) -> Result<(), ValueError> {
        self.encode_zcve_to_with_limits(output, ValueLimits::default())
    }

    /// Returns exact canonical bytes admitted under the supplied limits.
    pub fn zcve_bytes_with_limits(&self, limits: ValueLimits) -> Result<Vec<u8>, ValueError> {
        let mut output = Vec::new();
        self.encode_zcve_to_with_limits(&mut output, limits)?;
        Ok(output)
    }

    /// Validates exactly the supplied limits before appending a complete encoding.
    /// Every refusal preserves the original output prefix.
    pub fn encode_zcve_to_with_limits(
        &self,
        output: &mut Vec<u8>,
        limits: ValueLimits,
    ) -> Result<(), ValueError> {
        self.validate_limits(limits)?;
        let mut encoded = Vec::new();
        encode_zcve_value(self, &mut encoded)?;
        output
            .len()
            .checked_add(encoded.len())
            .ok_or(ValueError::ArithmeticOverflow)?;
        output.extend_from_slice(&encoded);
        Ok(())
    }

    /// Computes the exact canonical wire length using checked arithmetic.
    pub fn encoded_length(&self) -> Result<usize, ValueError> {
        encoded_length(self)
    }

    /// Returns the structural value kind.
    #[must_use]
    pub const fn kind(&self) -> ValueKind {
        match &self.repr {
            ValueRepr::Unit => ValueKind::Unit,
            ValueRepr::Bool(_) => ValueKind::Bool,
            ValueRepr::U128(_) => ValueKind::U128,
            ValueRepr::I128(_) => ValueKind::I128,
            ValueRepr::Bytes(_) => ValueKind::Bytes,
            ValueRepr::Text(_) => ValueKind::Text,
            ValueRepr::Enum { .. } => ValueKind::Enum,
            ValueRepr::Tuple(_) => ValueKind::Tuple,
            ValueRepr::Record(_) => ValueKind::Record,
            ValueRepr::Sum { .. } => ValueKind::Sum,
            ValueRepr::Vector(_) => ValueKind::Vector,
            ValueRepr::Map(_) => ValueKind::Map,
        }
    }

    /// Validates deterministic structural limits.
    pub fn validate_limits(&self, limits: ValueLimits) -> Result<ValueMetrics, ValueError> {
        let mut metrics = ValueMetrics::default();
        validate_value(self, 0, limits, &mut metrics)?;
        Ok(metrics)
    }
}

fn encode_zcve_value(value: &Value, output: &mut Vec<u8>) -> Result<(), ValueError> {
    use zcve::{
        TAG_BOOL_FALSE, TAG_BOOL_TRUE, TAG_BYTES, TAG_ENUM, TAG_I128, TAG_MAP, TAG_RECORD, TAG_SUM,
        TAG_TEXT, TAG_TUPLE, TAG_U128, TAG_UNIT, TAG_VECTOR,
    };

    match &value.repr {
        ValueRepr::Unit => output.push(TAG_UNIT),
        ValueRepr::Bool(false) => output.push(TAG_BOOL_FALSE),
        ValueRepr::Bool(true) => output.push(TAG_BOOL_TRUE),
        ValueRepr::U128(integer) => {
            output.push(TAG_U128);
            output.extend_from_slice(&integer.to_be_bytes());
        }
        ValueRepr::I128(integer) => {
            output.push(TAG_I128);
            output.extend_from_slice(&integer.to_be_bytes());
        }
        ValueRepr::Bytes(bytes) => {
            output.push(TAG_BYTES);
            put_zcve_blob(output, bytes)?;
        }
        ValueRepr::Text(text) => {
            output.push(TAG_TEXT);
            put_zcve_blob(output, text.as_bytes())?;
        }
        ValueRepr::Enum { type_id, variant } => {
            output.push(TAG_ENUM);
            output.extend_from_slice(&type_id.to_be_bytes());
            output.extend_from_slice(&variant.to_be_bytes());
        }
        ValueRepr::Tuple(items) => {
            output.push(TAG_TUPLE);
            put_zcve_length(output, items.len())?;
            for item in items {
                encode_zcve_value(item, output)?;
            }
        }
        ValueRepr::Record(fields) => {
            output.push(TAG_RECORD);
            put_zcve_length(output, fields.len())?;
            for field in fields {
                output.extend_from_slice(&field.id().to_be_bytes());
                encode_zcve_value(field.value(), output)?;
            }
        }
        ValueRepr::Sum {
            type_id,
            variant,
            payload,
        } => {
            output.push(TAG_SUM);
            output.extend_from_slice(&type_id.to_be_bytes());
            output.extend_from_slice(&variant.to_be_bytes());
            match payload {
                None => output.push(0),
                Some(child) => {
                    output.push(1);
                    encode_zcve_value(child, output)?;
                }
            }
        }
        ValueRepr::Vector(items) => {
            output.push(TAG_VECTOR);
            put_zcve_length(output, items.len())?;
            for item in items {
                encode_zcve_value(item, output)?;
            }
        }
        ValueRepr::Map(entries) => {
            output.push(TAG_MAP);
            put_zcve_length(output, entries.len())?;
            for entry in entries {
                put_zcve_blob(output, entry.encoded_key())?;
                let mut encoded_value = Vec::new();
                encode_zcve_value(entry.value(), &mut encoded_value)?;
                put_zcve_blob(output, &encoded_value)?;
            }
        }
    }
    Ok(())
}

fn check_wire_length(length: usize) -> Result<(), ValueError> {
    u32::try_from(length)
        .map(|_| ())
        .map_err(|_| ValueError::ArithmeticOverflow)
}

fn encoded_length(value: &Value) -> Result<usize, ValueError> {
    fn add(left: usize, right: usize) -> Result<usize, ValueError> {
        left.checked_add(right)
            .ok_or(ValueError::ArithmeticOverflow)
    }
    match &value.repr {
        ValueRepr::Unit | ValueRepr::Bool(_) => Ok(1),
        ValueRepr::U128(_) | ValueRepr::I128(_) => Ok(17),
        ValueRepr::Bytes(bytes) => {
            check_wire_length(bytes.len())?;
            add(5, bytes.len())
        }
        ValueRepr::Text(text) => {
            check_wire_length(text.len())?;
            add(5, text.len())
        }
        ValueRepr::Enum { .. } => Ok(7),
        ValueRepr::Tuple(items) | ValueRepr::Vector(items) => {
            check_wire_length(items.len())?;
            let mut length = 5;
            for child in items {
                length = add(length, encoded_length(child)?)?;
            }
            Ok(length)
        }
        ValueRepr::Record(fields) => {
            check_wire_length(fields.len())?;
            let mut length = 5;
            for field in fields {
                length = add(add(length, 2)?, encoded_length(field.value())?)?;
            }
            Ok(length)
        }
        ValueRepr::Sum { payload, .. } => match payload {
            None => Ok(8),
            Some(child) => add(8, encoded_length(child)?),
        },
        ValueRepr::Map(entries) => {
            check_wire_length(entries.len())?;
            let mut length = 5;
            for entry in entries {
                let key = entry.encoded_key().len();
                let value = encoded_length(entry.value())?;
                check_wire_length(key)?;
                check_wire_length(value)?;
                length = add(add(add(length, 8)?, key)?, value)?;
            }
            Ok(length)
        }
    }
}

fn put_zcve_length(output: &mut Vec<u8>, length: usize) -> Result<(), ValueError> {
    let length = u32::try_from(length).map_err(|_| ValueError::ArithmeticOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_zcve_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ValueError> {
    put_zcve_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn ensure_strict_fields(fields: &[Field]) -> Result<(), ValueError> {
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

fn ensure_strict_map_keys(entries: &[MapEntry]) -> Result<(), ValueError> {
    for pair in entries.windows(2) {
        if pair[0].encoded_key >= pair[1].encoded_key {
            return Err(ValueError::MapKeyOrder);
        }
    }
    Ok(())
}

/// Structural kind of a closed value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ValueKind {
    /// Unit.
    Unit,
    /// Boolean.
    Bool,
    /// Unsigned integer.
    U128,
    /// Signed integer.
    I128,
    /// Bytes.
    Bytes,
    /// Text.
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

/// Deterministic closed-value resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueLimits {
    /// Maximum recursive nesting depth, counting the root as depth zero.
    pub max_depth: u32,
    /// Maximum total value nodes.
    pub max_nodes: u64,
    /// Maximum decoder-compatible payload charge: byte/text leaves plus both map entry blobs.
    pub max_payload_bytes: u64,
    /// Maximum children in any single collection.
    pub max_collection_len: u32,
}

impl ValueLimits {
    /// Default maximum aggregate byte and text payload bytes.
    ///
    /// [`Value::text_ascii`] and [`Value::bytes`] also use this ceiling for one
    /// leaf so their successful outputs are admissible under the default
    /// payload budget.
    pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
}

impl Default for ValueLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 1_000_000,
            max_payload_bytes: u64::try_from(Self::DEFAULT_MAX_PAYLOAD_BYTES).unwrap_or(u64::MAX),
            max_collection_len: 1_000_000,
        }
    }
}

/// Exact structural metrics observed during limit validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValueMetrics {
    /// Total value nodes.
    pub nodes: u64,
    /// Byte/text leaf bytes plus both canonical map entry blob lengths.
    pub payload_bytes: u64,
    /// Maximum observed depth.
    pub depth: u32,
}

/// An owned value admitted by exact retained canonical limits.
///
/// Construction performs one complete validation and retains the supplied
/// limits and exact observed metrics. Private fields prevent callers from
/// pairing an unvalidated value with invented metrics:
///
/// ```compile_fail
/// use zeno_fcis_value::{AdmittedValue, Value, ValueLimits, ValueMetrics};
///
/// let _ = AdmittedValue {
///     value: Value::unit(),
///     metrics: ValueMetrics::default(),
///     limits: ValueLimits::default(),
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedValue {
    value: Value,
    metrics: ValueMetrics,
    limits: ValueLimits,
}

impl AdmittedValue {
    /// Owns a value after validating the exact default canonical envelope.
    pub fn try_new(value: Value) -> Result<Self, ValueError> {
        Self::try_new_with_limits(value, ValueLimits::default())
    }

    /// Owns a value and its exact observed metrics under the supplied limits.
    pub fn try_new_with_limits(value: Value, limits: ValueLimits) -> Result<Self, ValueError> {
        let metrics = value.validate_limits(limits)?;
        Ok(Self {
            value,
            metrics,
            limits,
        })
    }

    /// Returns the actual limits used for this admission.
    #[must_use]
    pub const fn limits(&self) -> ValueLimits {
        self.limits
    }

    /// Returns the admitted immutable value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the exact metrics observed during admission.
    #[must_use]
    pub const fn metrics(&self) -> ValueMetrics {
        self.metrics
    }

    /// Consumes the witness and returns the underlying value.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }

    /// Appends exact ZCVE/1 bytes without repeating limit validation.
    ///
    /// This is sound because construction owns the value, fields are private,
    /// and the public API exposes no mutable access to the admitted value.
    pub fn encode_zcve_to(&self, output: &mut Vec<u8>) -> Result<(), ValueError> {
        let mut encoded = Vec::new();
        encode_zcve_value(&self.value, &mut encoded)?;
        output
            .len()
            .checked_add(encoded.len())
            .ok_or(ValueError::ArithmeticOverflow)?;
        output.extend_from_slice(&encoded);
        Ok(())
    }
}

fn validate_value(
    value: &Value,
    depth: u32,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    if depth > limits.max_depth {
        return Err(ValueError::DepthLimit {
            limit: limits.max_depth,
            attempted: depth,
        });
    }
    metrics.depth = metrics.depth.max(depth);
    metrics.nodes = metrics
        .nodes
        .checked_add(1)
        .ok_or(ValueError::ArithmeticOverflow)?;
    if metrics.nodes > limits.max_nodes {
        return Err(ValueError::NodeLimit {
            limit: limits.max_nodes,
            attempted: metrics.nodes,
        });
    }

    match &value.repr {
        ValueRepr::Bytes(bytes) => add_payload(bytes.len(), limits, metrics)?,
        ValueRepr::Text(text) => {
            if !text.is_ascii() {
                return Err(ValueError::NonAsciiText);
            }
            add_payload(text.len(), limits, metrics)?;
        }
        ValueRepr::Tuple(items) | ValueRepr::Vector(items) => {
            validate_collection_len(items.len(), limits)?;
            for child in items {
                validate_value(
                    child,
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
            }
        }
        ValueRepr::Record(fields) => {
            validate_collection_len(fields.len(), limits)?;
            ensure_strict_fields(fields)?;
            for field in fields {
                validate_value(
                    field.value(),
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
            }
        }
        ValueRepr::Sum { payload, .. } => {
            if let Some(child) = payload {
                validate_value(
                    child,
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
            }
        }
        ValueRepr::Map(entries) => {
            validate_collection_len(entries.len(), limits)?;
            ensure_strict_map_keys(entries)?;
            for entry in entries {
                add_payload(entry.encoded_key().len(), limits, metrics)?;
                add_payload(entry.value().encoded_length()?, limits, metrics)?;
                validate_value(
                    entry.key(),
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
                validate_value(
                    entry.value(),
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
            }
        }
        ValueRepr::Unit
        | ValueRepr::Bool(_)
        | ValueRepr::U128(_)
        | ValueRepr::I128(_)
        | ValueRepr::Enum { .. } => {}
    }
    Ok(())
}

fn validate_collection_len(length: usize, limits: ValueLimits) -> Result<(), ValueError> {
    let attempted = u32::try_from(length).map_err(|_| ValueError::ArithmeticOverflow)?;
    if attempted > limits.max_collection_len {
        return Err(ValueError::CollectionLimit {
            limit: limits.max_collection_len,
            attempted,
        });
    }
    Ok(())
}

fn add_payload(
    length: usize,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    let length = u64::try_from(length).map_err(|_| ValueError::ArithmeticOverflow)?;
    metrics.payload_bytes = metrics
        .payload_bytes
        .checked_add(length)
        .ok_or(ValueError::ArithmeticOverflow)?;
    if metrics.payload_bytes > limits.max_payload_bytes {
        return Err(ValueError::PayloadLimit {
            limit: limits.max_payload_bytes,
            attempted: metrics.payload_bytes,
        });
    }
    Ok(())
}

/// Closed-value construction or validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueError {
    /// Record fields are duplicated or out of order.
    RecordFieldOrder {
        /// Previous field identifier.
        previous: u16,
        /// Current field identifier.
        current: u16,
    },
    /// Map keys are duplicated or out of canonical order.
    MapKeyOrder,
    /// Text is not permitted by the ASCII profile.
    NonAsciiText,
    /// Recursive nesting exceeded the declared limit.
    DepthLimit {
        /// Configured limit.
        limit: u32,
        /// Attempted depth.
        attempted: u32,
    },
    /// Total node count exceeded the declared limit.
    NodeLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted node count.
        attempted: u64,
    },
    /// Aggregate payload bytes exceeded the declared limit.
    PayloadLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted payload bytes.
        attempted: u64,
    },
    /// One collection exceeded its declared limit.
    CollectionLimit {
        /// Configured limit.
        limit: u32,
        /// Attempted collection length.
        attempted: u32,
    },
    /// Metric arithmetic overflowed.
    ArithmeticOverflow,
}

impl core::error::Error for ValueError {}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordFieldOrder { previous, current } => write!(
                formatter,
                "record fields are not strictly increasing: {previous} then {current}"
            ),
            Self::MapKeyOrder => {
                formatter.write_str("map keys are duplicate or not strictly ordered")
            }
            Self::NonAsciiText => formatter.write_str("text is not ASCII"),
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
            Self::ArithmeticOverflow => formatter.write_str("value metric arithmetic overflow"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn bounded_vector_owns_and_checks_length() {
        let value = BoundedVec::<u8, 1, 3>::try_from_vec(vec![1, 2]);
        assert!(value.is_ok());
        let error = BoundedVec::<u8, 1, 3>::try_from_vec(Vec::new());
        assert!(error.is_err());
    }

    #[test]
    fn admitted_value_retains_exact_default_metrics_and_value() {
        let bytes = Value::bytes(vec![1, 2]).unwrap_or_else(|error| panic!("bytes: {error}"));
        let text =
            Value::text_ascii(String::from("ab")).unwrap_or_else(|error| panic!("text: {error}"));
        let value =
            Value::tuple(vec![bytes, text]).unwrap_or_else(|error| panic!("collection: {error}"));
        let admitted = AdmittedValue::try_new(value.clone())
            .unwrap_or_else(|error| panic!("admission: {error}"));

        assert_eq!(
            admitted.metrics(),
            ValueMetrics {
                nodes: 3,
                payload_bytes: 4,
                depth: 1,
            }
        );
        assert_eq!(admitted.value(), &value);

        let mut admitted_bytes = Vec::new();
        admitted
            .encode_zcve_to(&mut admitted_bytes)
            .unwrap_or_else(|error| panic!("admitted encode: {error}"));
        assert_eq!(
            admitted_bytes,
            value
                .zcve_bytes()
                .unwrap_or_else(|error| panic!("value encode: {error}"))
        );
        assert_eq!(admitted.into_value(), value);
    }

    #[test]
    fn admitted_value_rejects_raw_non_ascii_text() {
        let value = Value {
            repr: ValueRepr::Text(String::from("é").into_boxed_str()),
        };
        assert_eq!(AdmittedValue::try_new(value), Err(ValueError::NonAsciiText));
    }

    #[test]
    fn admitted_value_is_bound_to_default_limits() {
        let mut value = Value::unit();
        for _ in 0..65 {
            value =
                Value::vector(vec![value]).unwrap_or_else(|error| panic!("collection: {error}"));
        }
        let permissive = ValueLimits {
            max_depth: 65,
            ..ValueLimits::default()
        };
        assert!(value.validate_limits(permissive).is_ok());
        assert_eq!(
            AdmittedValue::try_new(value),
            Err(ValueError::DepthLimit {
                limit: 64,
                attempted: 65,
            })
        );
    }

    #[test]
    fn value_bytes_accept_exact_bound() {
        assert_eq!(
            Value::bytes_with_max(vec![1, 2, 3], 3),
            Ok(Value {
                repr: ValueRepr::Bytes(vec![1, 2, 3].into_boxed_slice())
            })
        );
    }

    #[test]
    fn value_bytes_reject_one_over_bound() {
        let error = Value::bytes_with_max(vec![1, 2, 3, 4], 3);
        match error {
            Err(length) => {
                assert_eq!(length.minimum(), 0);
                assert_eq!(length.maximum(), 3);
                assert_eq!(length.actual(), 4);
            }
            Ok(value) => panic!("expected exact byte length error, got {value:?}"),
        }
    }

    #[test]
    fn admitted_bytes_keep_exact_zcve_bytes() {
        let value = Value::bytes(vec![1, 2, 3]).unwrap_or_else(|error| panic!("bytes: {error}"));
        assert_eq!(
            value.zcve_bytes(),
            Ok(vec![zcve::TAG_BYTES, 0, 0, 0, 3, 1, 2, 3])
        );
    }

    #[test]
    fn individually_admitted_byte_leaves_still_obey_aggregate_limits() {
        let first = Value::bytes(vec![1, 2]);
        let second = Value::bytes(vec![3, 4]);
        assert!(first.is_ok() && second.is_ok());
        let value = Value::vector(vec![
            first.unwrap_or_else(|error| panic!("first bytes: {error}")),
            second.unwrap_or_else(|error| panic!("second bytes: {error}")),
        ])
        .unwrap_or_else(|error| panic!("collection: {error}"));
        let limits = ValueLimits {
            max_payload_bytes: 3,
            ..ValueLimits::default()
        };
        assert_eq!(
            value.validate_limits(limits),
            Err(ValueError::PayloadLimit {
                limit: 3,
                attempted: 4,
            })
        );
    }

    #[test]
    fn value_ascii_text_accepts_exact_bound() {
        let value = Value::text_ascii_with_max(String::from("abc"), 3);
        assert_eq!(
            value,
            Ok(Value {
                repr: ValueRepr::Text(String::from("abc").into_boxed_str())
            })
        );
    }

    #[test]
    fn value_ascii_text_rejects_one_over_bound() {
        let error = Value::text_ascii_with_max(String::from("abcd"), 3);
        match error {
            Err(TextError::TooLong(length)) => {
                assert_eq!(length.minimum(), 0);
                assert_eq!(length.maximum(), 3);
                assert_eq!(length.actual(), 4);
            }
            other => panic!("expected exact text length error, got {other:?}"),
        }
    }

    #[test]
    fn value_ascii_text_preserves_non_ascii_precedence() {
        assert_eq!(
            Value::text_ascii_with_max(String::from("é"), 0),
            Err(TextError::NonAscii)
        );
    }

    #[test]
    fn default_helper_ceiling_matches_default_payload_limit() {
        assert_eq!(
            ValueLimits::default().max_payload_bytes,
            u64::try_from(ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES).unwrap_or(u64::MAX)
        );
    }

    #[test]
    fn admitted_text_keeps_exact_zcve_bytes() {
        let value =
            Value::text_ascii(String::from("abc")).unwrap_or_else(|error| panic!("text: {error}"));
        assert_eq!(
            value.zcve_bytes(),
            Ok(vec![zcve::TAG_TEXT, 0, 0, 0, 3, b'a', b'b', b'c'])
        );
    }

    #[test]
    fn individually_admitted_text_leaves_still_obey_aggregate_limits() {
        let first = Value::text_ascii(String::from("ab"));
        let second = Value::text_ascii(String::from("cd"));
        assert!(first.is_ok() && second.is_ok());
        let value = Value::vector(vec![
            first.unwrap_or_else(|error| panic!("first text: {error}")),
            second.unwrap_or_else(|error| panic!("second text: {error}")),
        ])
        .unwrap_or_else(|error| panic!("collection: {error}"));
        let limits = ValueLimits {
            max_payload_bytes: 3,
            ..ValueLimits::default()
        };
        assert_eq!(
            value.validate_limits(limits),
            Err(ValueError::PayloadLimit {
                limit: 3,
                attempted: 4,
            })
        );
    }

    #[test]
    fn records_require_stable_field_order() {
        let fields = vec![
            Field::new(2, Value::unsigned(2)),
            Field::new(1, Value::unsigned(1)),
        ];
        assert!(Value::record_canonical(fields.clone()).is_err());
        let normalized = Value::normalize_record(fields);
        assert!(normalized.is_ok());
    }

    #[test]
    fn maps_reject_duplicate_encoded_keys() {
        let first = MapEntry::try_new(Value::unsigned(1), Value::boolean(true));
        let second = MapEntry::try_new(Value::unsigned(1), Value::boolean(false));
        assert!(first.is_ok() && second.is_ok());
        let entries = vec![
            first.unwrap_or_else(|error| panic!("map entry: {error}")),
            second.unwrap_or_else(|error| panic!("map entry: {error}")),
        ];
        assert_eq!(Value::map_canonical(entries), Err(ValueError::MapKeyOrder));
    }

    #[test]
    fn map_entry_derives_exact_key_bytes() {
        let key = Value::tuple(vec![Value::unsigned(7), Value::boolean(true)])
            .unwrap_or_else(|error| panic!("collection: {error}"));
        let expected = key.zcve_bytes();
        let entry = MapEntry::try_new(key, Value::unit());
        assert!(expected.is_ok() && entry.is_ok());
        let expected = expected.unwrap_or_else(|error| panic!("key bytes: {error}"));
        let entry = entry.unwrap_or_else(|error| panic!("map entry: {error}"));
        assert_eq!(entry.encoded_key(), expected.as_slice());
    }

    #[test]
    fn structural_limits_are_checked_transitively() {
        let bytes = Value::bytes(vec![1, 2, 3]).unwrap_or_else(|error| panic!("bytes: {error}"));
        let inner =
            Value::vector(vec![bytes]).unwrap_or_else(|error| panic!("collection: {error}"));
        let value =
            Value::vector(vec![inner]).unwrap_or_else(|error| panic!("collection: {error}"));
        let limits = ValueLimits {
            max_depth: 1,
            ..ValueLimits::default()
        };
        assert!(matches!(
            value.validate_limits(limits),
            Err(ValueError::DepthLimit { .. })
        ));
    }
}
