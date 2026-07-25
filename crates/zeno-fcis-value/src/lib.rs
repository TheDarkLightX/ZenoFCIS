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
pub enum TextError {
    /// The initial profile accepts ASCII only.
    NonAscii,
    /// The encoded text exceeds its bound.
    TooLong(LengthError),
}

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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MapEntry {
    encoded_key: Box<[u8]>,
    key: Value,
    value: Value,
}

impl MapEntry {
    /// Creates an entry. The enclosing map validates order and uniqueness.
    #[must_use]
    pub fn new(encoded_key: Vec<u8>, key: Value, value: Value) -> Self {
        Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        }
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

/// The closed reference value algebra used at protocol boundaries.
///
/// Domain crates should prefer generated strongly typed structures for hot
/// paths. This dynamic algebra is the reference, inspection, patching, and
/// cross-language boundary representation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Value {
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
    /// Creates owned bytes.
    #[must_use]
    pub fn bytes(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes.into_boxed_slice())
    }

    /// Creates ASCII text.
    pub fn text_ascii(text: String) -> Result<Self, TextError> {
        if !text.is_ascii() {
            return Err(TextError::NonAscii);
        }
        Ok(Self::Text(text.into_boxed_str()))
    }

    /// Creates a tuple.
    #[must_use]
    pub fn tuple(items: Vec<Self>) -> Self {
        Self::Tuple(items.into_boxed_slice())
    }

    /// Creates a vector.
    #[must_use]
    pub fn vector(items: Vec<Self>) -> Self {
        Self::Vector(items.into_boxed_slice())
    }

    /// Creates a record only when field identifiers are strictly increasing.
    pub fn record_canonical(fields: Vec<Field>) -> Result<Self, ValueError> {
        ensure_strict_fields(&fields)?;
        Ok(Self::Record(fields.into_boxed_slice()))
    }

    /// Sorts record fields by identifier and rejects duplicates.
    pub fn normalize_record(mut fields: Vec<Field>) -> Result<Self, ValueError> {
        fields.sort_by_key(Field::id);
        Self::record_canonical(fields)
    }

    /// Creates a map only when encoded keys are strictly increasing.
    pub fn map_canonical(entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        ensure_strict_map_keys(&entries)?;
        Ok(Self::Map(entries.into_boxed_slice()))
    }

    /// Sorts entries by encoded key and rejects duplicates.
    pub fn normalize_map(mut entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        entries.sort_by(|left, right| left.encoded_key.cmp(&right.encoded_key));
        Self::map_canonical(entries)
    }

    /// Returns the structural value kind.
    #[must_use]
    pub const fn kind(&self) -> ValueKind {
        match self {
            Self::Unit => ValueKind::Unit,
            Self::Bool(_) => ValueKind::Bool,
            Self::U128(_) => ValueKind::U128,
            Self::I128(_) => ValueKind::I128,
            Self::Bytes(_) => ValueKind::Bytes,
            Self::Text(_) => ValueKind::Text,
            Self::Enum { .. } => ValueKind::Enum,
            Self::Tuple(_) => ValueKind::Tuple,
            Self::Record(_) => ValueKind::Record,
            Self::Sum { .. } => ValueKind::Sum,
            Self::Vector(_) => ValueKind::Vector,
            Self::Map(_) => ValueKind::Map,
        }
    }

    /// Validates deterministic structural limits.
    pub fn validate_limits(&self, limits: ValueLimits) -> Result<ValueMetrics, ValueError> {
        let mut metrics = ValueMetrics::default();
        validate_value(self, 0, limits, &mut metrics)?;
        Ok(metrics)
    }
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
    /// Maximum aggregate owned byte and text payload bytes.
    pub max_payload_bytes: u64,
    /// Maximum children in any single collection.
    pub max_collection_len: u32,
}

impl Default for ValueLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 1_000_000,
            max_payload_bytes: 64 * 1024 * 1024,
            max_collection_len: 1_000_000,
        }
    }
}

/// Exact structural metrics observed during limit validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValueMetrics {
    /// Total value nodes.
    pub nodes: u64,
    /// Aggregate bytes and text bytes.
    pub payload_bytes: u64,
    /// Maximum observed depth.
    pub depth: u32,
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

    match value {
        Value::Bytes(bytes) => add_payload(bytes.len(), limits, metrics)?,
        Value::Text(text) => {
            if !text.is_ascii() {
                return Err(ValueError::NonAsciiText);
            }
            add_payload(text.len(), limits, metrics)?;
        }
        Value::Tuple(items) | Value::Vector(items) => {
            validate_collection_len(items.len(), limits)?;
            for child in items {
                validate_value(child, depth + 1, limits, metrics)?;
            }
        }
        Value::Record(fields) => {
            validate_collection_len(fields.len(), limits)?;
            ensure_strict_fields(fields)?;
            for field in fields {
                validate_value(field.value(), depth + 1, limits, metrics)?;
            }
        }
        Value::Sum { payload, .. } => {
            if let Some(child) = payload {
                validate_value(child, depth + 1, limits, metrics)?;
            }
        }
        Value::Map(entries) => {
            validate_collection_len(entries.len(), limits)?;
            ensure_strict_map_keys(entries)?;
            for entry in entries {
                add_payload(entry.encoded_key().len(), limits, metrics)?;
                validate_value(entry.key(), depth + 1, limits, metrics)?;
                validate_value(entry.value(), depth + 1, limits, metrics)?;
            }
        }
        Value::Unit | Value::Bool(_) | Value::U128(_) | Value::I128(_) | Value::Enum { .. } => {}
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
    fn records_require_stable_field_order() {
        let fields = vec![Field::new(2, Value::U128(2)), Field::new(1, Value::U128(1))];
        assert!(Value::record_canonical(fields.clone()).is_err());
        let normalized = Value::normalize_record(fields);
        assert!(normalized.is_ok());
    }

    #[test]
    fn maps_reject_duplicate_encoded_keys() {
        let entries = vec![
            MapEntry::new(vec![1], Value::U128(1), Value::Bool(true)),
            MapEntry::new(vec![1], Value::U128(1), Value::Bool(false)),
        ];
        assert_eq!(Value::map_canonical(entries), Err(ValueError::MapKeyOrder));
    }

    #[test]
    fn structural_limits_are_checked_transitively() {
        let value = Value::vector(vec![Value::vector(vec![Value::bytes(vec![1, 2, 3])])]);
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
