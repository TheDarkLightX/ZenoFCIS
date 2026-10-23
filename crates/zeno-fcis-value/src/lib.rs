//! Transitively immutable, bounded value types for ZenoFCIS.
//!
//! Values expose no interior mutation. Variable-sized children are owned by
//! boxed slices or boxed strings so committed values cannot retain mutable
//! caller aliases.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(verus_keep_ghost)]
use decoder_model::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub mod decoder_model;
#[cfg(verus_keep_ghost)]
include!("decoder_refinement.rs");

mod decoder_bytes;
use decoder_bytes::{append_word, bytes_before, signed_to_bits};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// ZCVE/1 value tags shared by construction and decoding.
///
/// These identifiers are protocol meaning. They are exposed so the codec can
/// decode the exact byte format whose encoder lives with [`Value`].
pub mod zcve {
    #[cfg(verus_keep_ghost)]
    use vstd::prelude::*;
    /// Unit value tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_UNIT: u8 = 0x00;
    /// Boolean false tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_BOOL_FALSE: u8 = 0x01;
    /// Boolean true tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_BOOL_TRUE: u8 = 0x02;
    /// Unsigned 128-bit integer tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_U128: u8 = 0x03;
    /// Signed 128-bit integer tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_I128: u8 = 0x04;
    /// Byte string tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_BYTES: u8 = 0x05;
    /// ASCII text tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_TEXT: u8 = 0x06;
    /// Closed enum tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_ENUM: u8 = 0x07;
    /// Tuple tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_TUPLE: u8 = 0x08;
    /// Record tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_RECORD: u8 = 0x09;
    /// Closed sum tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_SUM: u8 = 0x0a;
    /// Vector tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_VECTOR: u8 = 0x0b;
    /// Map tag.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const TAG_MAP: u8 = 0x0c;
}

/// A length-bound violation.
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
pub struct LengthError {
    minimum: usize,
    maximum: usize,
    actual: usize,
}

#[cfg(not(verus_keep_ghost))]
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

#[cfg(not(verus_keep_ghost))]
impl core::error::Error for LengthError {}

#[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct BoundedVec<T, const MIN: usize, const MAX: usize> {
    items: Box<[T]>,
}

#[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct OwnedBytes<const MAX: usize> {
    bytes: Box<[u8]>,
}

#[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct AsciiText<const MAX: usize> {
    text: Box<str>,
}

#[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
#[non_exhaustive]
pub enum TextError {
    /// The initial profile accepts ASCII only.
    NonAscii,
    /// The encoded text exceeds its bound.
    TooLong(LengthError),
}

#[cfg(not(verus_keep_ghost))]
impl core::error::Error for TextError {}

#[cfg(not(verus_keep_ghost))]
impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAscii => formatter.write_str("protocol text must be ASCII"),
            Self::TooLong(error) => error.fmt(formatter),
        }
    }
}

/// A record field ordered by stable numeric field identifier.
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct Field {
    id: u16,
    value: Value,
}

#[cfg(verus_keep_ghost)]
verus! {
impl Field {
    pub closed spec fn spec_id(&self) -> u16 { self.id }
    pub closed spec fn spec_value(&self) -> Value { self.value }
}
impl MapEntry {
    pub closed spec fn spec_key(&self) -> Value { self.key }
    pub closed spec fn spec_value(&self) -> Value { self.value }
    pub closed spec fn spec_encoded_key(&self) -> Seq<u8> { self.encoded_key@ }
}
}

impl Field {
    /// Creates a field.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.spec_id() == id && result.spec_value() == value,))]
    pub const fn new(id: u16, value: Value) -> Self {
        Self { id, value }
    }

    /// Returns the stable field identifier.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.spec_id(),))]
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// Returns the field value.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures *result == self.spec_value(),))]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the field and returns its parts.
    #[must_use]
    #[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
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
    #[cfg(not(verus_keep_ghost))]
    pub fn try_new(key: Value, value: Value) -> Result<Self, ValueError> {
        Self::try_new_with_limits(key, value, ValueLimits::default())
    }

    /// Derives key bytes using exactly the supplied value limits.
    pub fn try_new_with_limits(
        key: Value,
        value: Value,
        limits: ValueLimits,
    ) -> Result<Self, ValueError> {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            key.representation_valid() && value.representation_valid() ==>
                (result.is_ok() == (wf(key.model(), limits.model(0)) && spec_encode(key.model()).len() <= u32::MAX))
                && (result matches Ok(entry) ==> entry.representation_valid()
                    && entry.spec_key().model() == key.model() && entry.spec_value().model() == value.model()
                    && entry.spec_encoded_key() == spec_encode(key.model()))));
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(MapEntry::representation_valid); reveal(MapEntry::storage_valid);
            reveal(Value::representation_valid); reveal(Value::storage_valid);
        }
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
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.spec_encoded_key(),))]
    pub fn encoded_key(&self) -> &[u8] {
        &self.encoded_key
    }

    /// Returns the semantic key.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures *result == self.spec_key(),))]
    pub fn key(&self) -> &Value {
        &self.key
    }

    /// Returns the semantic value.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures *result == self.spec_value(),))]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry and returns its parts.
    #[must_use]
    #[cfg(not(verus_keep_ghost))]
    pub fn into_parts(self) -> (Box<[u8]>, Value, Value) {
        (self.encoded_key, self.key, self.value)
    }
}

/// An immutable borrowed inspection of a value, never a construction witness.
#[non_exhaustive]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
pub struct Value {
    repr: ValueRepr,
}

#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)
)]
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
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.model() == ModelValue::Unit && result.representation_valid(),))]
    pub const fn unit() -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }

        Self {
            repr: ValueRepr::Unit,
        }
    }
    /// Creates a Boolean.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.model() == ModelValue::Bool(value) && result.representation_valid(),))]
    pub const fn boolean(value: bool) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }

        Self {
            repr: ValueRepr::Bool(value),
        }
    }
    /// Creates a full-width unsigned integer.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.model() == ModelValue::Unsigned(value) && result.representation_valid(),))]
    pub const fn unsigned(value: u128) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }

        Self {
            repr: ValueRepr::U128(value),
        }
    }
    /// Creates a full-width signed integer.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.model() == ModelValue::Signed(value) && result.representation_valid(),))]
    pub const fn signed(value: i128) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }

        Self {
            repr: ValueRepr::I128(value),
        }
    }
    /// Creates a generic enum; zero and all full-width identifiers are legal.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, allow(unused, verus_impl_method_marker))]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.model() == ModelValue::Enum(type_id, variant) && result.representation_valid(),))]
    pub const fn enumeration(type_id: u32, variant: u16) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }

        Self {
            repr: ValueRepr::Enum { type_id, variant },
        }
    }
    /// Owns a sum payload; scalar and payload-bearing variants remain distinct.
    #[must_use]
    pub fn sum(type_id: u32, variant: u16, payload: Option<Self>) -> Self {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Self| result.model() == ModelValue::Sum(type_id, variant, optional_model(&payload))
            && (optional_valid(&payload) ==> result.representation_valid())));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }
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
    #[cfg(not(verus_keep_ghost))]
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
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            result.is_ok() == wf(ModelValue::Bytes(bytes@), limits.model(0))
                && (result matches Ok(value) ==> value.model() == ModelValue::Bytes(bytes@)
                    && value.representation_valid())));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }
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
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            result.is_ok() == wf(ModelValue::Text(vstd::utf8::encode_utf8(text@)), limits.model(0))
                && (result matches Ok(value) ==> value.model() == ModelValue::Text(vstd::utf8::encode_utf8(text@))
                    && value.representation_valid())));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); }
        let text_str = text.as_str();
        let bytes = text_str.as_bytes();
        if !bytes.is_ascii() {
            return Err(ValueError::NonAsciiText);
        }
        check_wire_length(bytes.len())?;
        let value = Self {
            repr: ValueRepr::Text(text.into_boxed_str()),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(value.model() == ModelValue::Text(vstd::utf8::encode_utf8(text@)));
            assert(ascii(vstd::utf8::encode_utf8(text@)));
            assert(vstd::utf8::encode_utf8(text@).len() <= u32::MAX);
            assert(structurally_valid(value.model()));
            assert(value.storage_valid());
            assert(value.representation_valid());
        }
        value.validate_limits(limits)?;
        Ok(value)
    }

    /// Creates owned bytes admitted by the default payload ceiling.
    ///
    /// A single byte leaf cannot exceed
    /// [`ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`]. Enclosing values must still
    /// call [`Self::validate_limits`] because several individually admitted
    /// leaves can exceed the aggregate payload budget.
    #[cfg(not(verus_keep_ghost))]
    pub fn bytes(bytes: Vec<u8>) -> Result<Self, LengthError> {
        Self::bytes_with_max(bytes, ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES)
    }

    #[cfg(not(verus_keep_ghost))]
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
    #[cfg(not(verus_keep_ghost))]
    pub fn text_ascii(text: String) -> Result<Self, TextError> {
        Self::text_ascii_with_max(text, ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES)
    }

    #[cfg(not(verus_keep_ghost))]
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
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            (result.is_ok() == (items.len() <= u32::MAX)) &&
            (result matches Ok(value) ==> value.model() == ModelValue::Tuple(values_model(items@))
                && (values_valid(items@) ==> value.representation_valid()))));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); values_model_properties(items@);
        if values_valid(items@) { values_valid_parts(items@); } }

        check_wire_length(items.len())?;
        Ok(Self {
            repr: ValueRepr::Tuple(items.into_boxed_slice()),
        })
    }

    /// Creates a vector after checking its canonical u32 item count.
    pub fn vector(items: Vec<Self>) -> Result<Self, ValueError> {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            (result.is_ok() == (items.len() <= u32::MAX)) &&
            (result matches Ok(value) ==> value.model() == ModelValue::Vector(values_model(items@))
                && (values_valid(items@) ==> value.representation_valid()))));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid); values_model_properties(items@);
        if values_valid(items@) { values_valid_parts(items@); } }

        check_wire_length(items.len())?;
        Ok(Self {
            repr: ValueRepr::Vector(items.into_boxed_slice()),
        })
    }

    /// Creates a record only when field identifiers are strictly increasing.
    pub fn record_canonical(fields: Vec<Field>) -> Result<Self, ValueError> {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            result.is_ok() == (fields.len() <= u32::MAX && fields_ordered(fields_model(fields@)))
            && (result matches Ok(value) ==> value.model() == ModelValue::Record(fields_model(fields@))
                && (fields_valid(fields@) ==> value.representation_valid()))));
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid);
            fields_model_properties(fields@);
            if fields_valid(fields@) { fields_valid_parts(fields@); }
        }
        check_wire_length(fields.len())?;
        ensure_strict_fields(&fields)?;
        Ok(Self {
            repr: ValueRepr::Record(fields.into_boxed_slice()),
        })
    }

    /// Sorts record fields by identifier and rejects duplicates.
    #[cfg(not(verus_keep_ghost))]
    pub fn normalize_record(mut fields: Vec<Field>) -> Result<Self, ValueError> {
        fields.sort_by_key(Field::id);
        Self::record_canonical(fields)
    }

    /// Creates a map only when encoded keys are strictly increasing.
    pub fn map_canonical(entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Result<Self, ValueError>|
            entries_valid(entries@) ==> (result.is_ok() == (entries.len() <= u32::MAX
                && entries_ordered(entries_model(entries@)) && entry_values_fit(entries@)))
                && (result matches Ok(value) ==> value.model() == ModelValue::Map(entries_model(entries@))
                    && value.representation_valid())));
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid);
            entries_model_properties(entries@);
            if entries_valid(entries@) { entries_valid_parts(entries@); }
        }
        check_wire_length(entries.len())?;
        ensure_strict_map_keys(&entries)?;
        let mut index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= entries.len(),
                entries.len() <= u32::MAX,
                entries_valid(entries@) ==> entries_ordered(entries_model(entries@)),
                entries_valid(entries@) ==> forall|i: int| 0 <= i < index ==>
                    spec_encode((#[trigger] entries@[i]).spec_value().model()).len() <= u32::MAX,
            decreases entries.len() - index,
        ))]
        while index < entries.len() {
            let entry = &entries[index];
            #[cfg(verus_keep_ghost)]
            proof! { if entries_valid(entries@) { entry_children_valid(*entry); } }
            check_wire_length(entry.value().encoded_length()?)?;
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            entries_model_properties(entries@);
            if entries_valid(entries@) {
                map_model_valid(entries@);
            }
        }
        let value = Self {
            repr: ValueRepr::Map(entries.into_boxed_slice()),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            if entries_valid(entries@) {
                assert(value.model() == ModelValue::Map(entries_model(entries@)));
                assert(structurally_valid(value.model()));
                reveal_with_fuel(Value::storage_valid, 2);
                reveal(MapEntry::storage_valid);
                match &value.repr {
                    ValueRepr::Map(boxed) => {
                        assert(boxed@ == entries@);
                        assert forall|i: int| 0 <= i < boxed@.len() implies
                            (#[trigger] boxed@[i]).storage_valid() by {
                            assert(entries@[i].storage_valid());
                        }
                    },
                    _ => { assert(false); },
                }
                assert(value.storage_valid());
                assert(value.representation_valid());
            }
        }
        Ok(value)
    }

    /// Sorts entries by encoded key and rejects duplicates.
    #[cfg(not(verus_keep_ghost))]
    pub fn normalize_map(mut entries: Vec<MapEntry>) -> Result<Self, ValueError> {
        entries.sort_by(|left, right| left.encoded_key.cmp(&right.encoded_key));
        Self::map_canonical(entries)
    }

    /// Returns the exact ZCVE/1 bytes for this value.
    ///
    /// This low-level primitive is shared with `zeno-fcis-codec` so map-entry
    /// construction and public canonical encoding cannot drift. It validates
    /// the default closed-value limits before emitting any bytes.
    #[cfg(not(verus_keep_ghost))]
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
    #[cfg(not(verus_keep_ghost))]
    pub fn encode_zcve_to(&self, output: &mut Vec<u8>) -> Result<(), ValueError> {
        self.encode_zcve_to_with_limits(output, ValueLimits::default())
    }

    /// Returns exact canonical bytes admitted under the supplied limits.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures self.representation_valid() ==> result.is_ok() == (wf(self.model(), limits.model(0))
            && spec_encode(self.model()).len() <= usize::MAX),
            self.representation_valid() && result.is_ok() ==> result->Ok_0@ == spec_encode(self.model()),
    ))]
    pub fn zcve_bytes_with_limits(&self, limits: ValueLimits) -> Result<Vec<u8>, ValueError> {
        let mut output = Vec::new();
        self.encode_zcve_to_with_limits(&mut output, limits)?;
        Ok(output)
    }

    /// Validates exactly the supplied limits before appending a complete encoding.
    /// Every refusal preserves the original output prefix.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures self.representation_valid() ==> result.is_ok() == (wf(self.model(), limits.model(0))
            && old(output)@.len() + spec_encode(self.model()).len() <= usize::MAX),
            self.representation_valid() && result.is_ok() ==>
                final(output)@ == old(output)@ + spec_encode(self.model()),
            result.is_err() ==> final(output)@ == old(output)@,
    ))]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures self.representation_valid() ==> match result {
            Ok(length) => length == spec_encode(self.model()).len(),
            Err(_) => spec_encode(self.model()).len() > usize::MAX,
        },
    ))]
    pub fn encoded_length(&self) -> Result<usize, ValueError> {
        encoded_length(self)
    }

    /// Returns the structural value kind.
    #[must_use]
    #[cfg(not(verus_keep_ghost))]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures self.representation_valid() ==> result.is_ok() == wf(self.model(), limits.model(0)),
            result matches Ok(metrics) ==> metrics.nodes <= limits.max_nodes
                && metrics.payload_bytes <= limits.max_payload_bytes,
            self.representation_valid() && result.is_ok() ==>
                result->Ok_0.nodes == nodes(self.model())
                && result->Ok_0.payload_bytes == payload_bytes(self.model()),
    ))]
    pub fn validate_limits(&self, limits: ValueLimits) -> Result<ValueMetrics, ValueError> {
        let mut metrics = ValueMetrics {
            nodes: 0,
            payload_bytes: 0,
            depth: 0,
        };
        validate_value(self, 0, limits, &mut metrics)?;
        Ok(metrics)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures value.representation_valid() ==> result.is_ok()
        && final(output)@ == old(output)@ + spec_encode(value.model()),
    decreases value.model(),
))]
fn encode_zcve_value(value: &Value, output: &mut Vec<u8>) -> Result<(), ValueError> {
    use zcve::{
        TAG_BOOL_FALSE, TAG_BOOL_TRUE, TAG_BYTES, TAG_ENUM, TAG_I128, TAG_MAP, TAG_RECORD, TAG_SUM,
        TAG_TEXT, TAG_TUPLE, TAG_U128, TAG_UNIT, TAG_VECTOR,
    };

    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Value::model); reveal(Value::representation_valid);
        if value.representation_valid() { value_children_valid(*value); }
    }
    match &value.repr {
        ValueRepr::Unit => output.push(TAG_UNIT),
        ValueRepr::Bool(false) => output.push(TAG_BOOL_FALSE),
        ValueRepr::Bool(true) => output.push(TAG_BOOL_TRUE),
        ValueRepr::U128(integer) => {
            output.push(TAG_U128);
            append_word(output, *integer, 16);
        }
        ValueRepr::I128(integer) => {
            output.push(TAG_I128);
            append_word(output, signed_to_bits(*integer), 16);
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
            append_word(output, u128::from(*type_id), 4);
            append_word(output, u128::from(*variant), 2);
        }
        ValueRepr::Tuple(items) => {
            #[cfg(verus_keep_ghost)]
            proof! { values_model_properties(items@); }
            encode_zcve_items(TAG_TUPLE, items, output)?;
        }
        ValueRepr::Record(fields) => {
            #[cfg(verus_keep_ghost)]
            proof! { fields_model_properties(fields@); }
            encode_zcve_fields(TAG_RECORD, fields, output)?;
        }
        ValueRepr::Sum {
            type_id,
            variant,
            payload,
        } => {
            output.push(TAG_SUM);
            append_word(output, u128::from(*type_id), 4);
            append_word(output, u128::from(*variant), 2);
            match payload {
                None => output.push(0),
                Some(child) => {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        assert(value.model() == ModelValue::Sum(*type_id, *variant, seq![child.model()]));
                        vstd::seq::lemma_seq_index_decreases(seq![child.model()], 0);
                        assert(decreases_to!(value.model() => child.model()));
                    }
                    output.push(1);
                    encode_zcve_value(child, output)?;
                }
            }
        }
        ValueRepr::Vector(items) => {
            #[cfg(verus_keep_ghost)]
            proof! { values_model_properties(items@); }
            encode_zcve_items(TAG_VECTOR, items, output)?;
        }
        ValueRepr::Map(entries) => {
            #[cfg(verus_keep_ghost)]
            proof! { entries_model_properties(entries@); }
            encode_zcve_entries(TAG_MAP, entries, output)?;
        }
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures values_valid(items@) && items.len() <= u32::MAX ==> result.is_ok()
        && final(output)@ == old(output)@ + seq![tag] + length_prefix(items@.len()) + encode_items(values_model(items@)),
    decreases values_model(items@),
))]
fn encode_zcve_items(tag: u8, items: &[Value], output: &mut Vec<u8>) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        values_model_properties(items@);
        assert(values_model(items@).subrange(0, items@.len() as int) == values_model(items@));
    }
    output.push(tag);
    put_zcve_length(output, items.len())?;
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= items.len(),
            values_valid(items@) ==> values_valid(items@),
            values_valid(items@) ==> output@ + encode_items(values_model(items@).subrange(index as int, items@.len() as int))
                == old(output)@ + seq![tag] + length_prefix(items@.len()) + encode_items(values_model(items@)),
        decreases items.len() - index,
    ))]
    while index < items.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            values_model_properties(items@);
            vstd::seq::lemma_seq_index_decreases(values_model(items@), index as int);
            encode_items_step(values_model(items@), index as int);
        }
        let item = &items[index];
        encode_zcve_value(item, output)?;
        index += 1;
    }

    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures fields_valid(fields@) && fields.len() <= u32::MAX ==> result.is_ok()
        && final(output)@ == old(output)@ + seq![tag] + length_prefix(fields@.len()) + encode_fields(fields_model(fields@)),
    decreases fields_model(fields@),
))]
fn encode_zcve_fields(tag: u8, fields: &[Field], output: &mut Vec<u8>) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        fields_model_properties(fields@);
        assert(fields_model(fields@).subrange(0, fields@.len() as int) == fields_model(fields@));
    }
    output.push(tag);
    put_zcve_length(output, fields.len())?;
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= fields.len(),
            fields_valid(fields@) ==> fields_valid(fields@),
            fields_valid(fields@) ==> output@ + encode_fields(fields_model(fields@).subrange(index as int, fields@.len() as int))
                == old(output)@ + seq![tag] + length_prefix(fields@.len()) + encode_fields(fields_model(fields@)),
        decreases fields.len() - index,
    ))]
    while index < fields.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            fields_model_properties(fields@);
            vstd::seq::lemma_seq_index_decreases(fields_model(fields@), index as int);
            encode_fields_step(fields_model(fields@), index as int);
        }
        let field = &fields[index];
        append_word(output, u128::from(field.id()), 2);
        encode_zcve_value(field.value(), output)?;
        index += 1;
    }

    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (entries_valid(entries@) && entry_values_fit(entries@)) && entries.len() <= u32::MAX ==> result.is_ok()
        && final(output)@ == old(output)@ + seq![tag] + length_prefix(entries@.len()) + encode_entries(entries_model(entries@)),
    decreases entries_model(entries@),
))]
fn encode_zcve_entries(
    tag: u8,
    entries: &[MapEntry],
    output: &mut Vec<u8>,
) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        entries_model_properties(entries@);
        assert(entries_model(entries@).subrange(0, entries@.len() as int) == entries_model(entries@));
    }
    output.push(tag);
    put_zcve_length(output, entries.len())?;
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= entries.len(),
            (entries_valid(entries@) && entry_values_fit(entries@)) ==> entries_valid(entries@),
            (entries_valid(entries@) && entry_values_fit(entries@)) ==> output@ + encode_entries(entries_model(entries@).subrange(index as int, entries@.len() as int))
                == old(output)@ + seq![tag] + length_prefix(entries@.len()) + encode_entries(entries_model(entries@)),
        decreases entries.len() - index,
    ))]
    while index < entries.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            entries_model_properties(entries@);
            vstd::seq::lemma_seq_index_decreases(entries_model(entries@), index as int);
            encode_entries_step(entries_model(entries@), index as int);
        }
        let entry = &entries[index];
        #[cfg(verus_keep_ghost)]
        proof! {
            if (entries_valid(entries@) && entry_values_fit(entries@)) {
                entry_children_valid(*entry);
                assert(spec_encode(entry.spec_value().model()).len() <= u32::MAX);
            }
        }
        put_zcve_blob(output, entry.encoded_key())?;
        let mut encoded_value = Vec::new();
        encode_zcve_value(entry.value(), &mut encoded_value)?;
        put_zcve_blob(output, &encoded_value)?;
        index += 1;
    }

    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (length <= u32::MAX),
))]
fn check_wire_length(length: usize) -> Result<(), ValueError> {
    u32::try_from(length)
        .map(|_| ())
        .map_err(|_| ValueError::ArithmeticOverflow)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result { Ok(sum) => sum == left + right, Err(_) => left + right > usize::MAX },
))]
fn checked_encoded_length_add(left: usize, right: usize) -> Result<usize, ValueError> {
    left.checked_add(right)
        .ok_or(ValueError::ArithmeticOverflow)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures value.representation_valid() ==> match result {
        Ok(length) => length == spec_encode(value.model()).len(),
        Err(_) => spec_encode(value.model()).len() > usize::MAX,
    },
    decreases value,
))]
fn encoded_length(value: &Value) -> Result<usize, ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Value::model); reveal(Value::representation_valid); reveal(Value::storage_valid);
        reveal(MapEntry::storage_valid); reveal(MapEntry::representation_valid);
        if value.representation_valid() { value_children_valid(*value); }
        broadcast use decoder_model::big_endian_length;
    }
    match &value.repr {
        ValueRepr::Unit | ValueRepr::Bool(_) => Ok(1),
        ValueRepr::U128(_) | ValueRepr::I128(_) => Ok(17),
        ValueRepr::Bytes(bytes) => {
            check_wire_length(bytes.len())?;
            checked_encoded_length_add(5, bytes.len())
        }
        ValueRepr::Text(text) => {
            check_wire_length(text.len())?;
            checked_encoded_length_add(5, text.len())
        }
        ValueRepr::Enum { .. } => Ok(7),
        ValueRepr::Tuple(items) | ValueRepr::Vector(items) => {
            #[cfg(verus_keep_ghost)]
            proof! {
                values_model_properties(items@);
                assert(values_model(items@).subrange(0, items@.len() as int) == values_model(items@));
            }
            check_wire_length(items.len())?;
            let mut length = 5;
            let mut index = 0;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant index <= items.len(),
                    value.representation_valid() ==> values_valid(items@),
                    value.representation_valid() ==> length + encode_items(values_model(items@).subrange(index as int, items@.len() as int)).len()
                        == spec_encode(value.model()).len(), decreases_to!(*value => *items),
                    (value.repr == ValueRepr::Tuple(*items) || value.repr == ValueRepr::Vector(*items)),
                decreases items.len() - index,
            ))]
            while index < items.len() {
                #[cfg(verus_keep_ghost)]
                proof! {
                    vstd::slice::lemma_slice_index_decreases(&**items, index as int);
                    encode_items_step(values_model(items@), index as int);
                }
                let child = &items[index];
                length = checked_encoded_length_add(length, encoded_length(child)?)?;
                index += 1;
            }
            Ok(length)
        }
        ValueRepr::Record(fields) => {
            #[cfg(verus_keep_ghost)]
            proof! {
                fields_model_properties(fields@);
                assert(fields_model(fields@).subrange(0, fields@.len() as int) == fields_model(fields@));
            }
            check_wire_length(fields.len())?;
            let mut length = 5;
            let mut index = 0;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant index <= fields.len(),
                    value.representation_valid() ==> fields_valid(fields@),
                    value.representation_valid() ==> length + encode_fields(fields_model(fields@).subrange(index as int, fields@.len() as int)).len()
                        == spec_encode(value.model()).len(), decreases_to!(*value => *fields), value.repr == ValueRepr::Record(*fields),
                decreases fields.len() - index,
            ))]
            while index < fields.len() {
                #[cfg(verus_keep_ghost)]
                proof! {
                    vstd::slice::lemma_slice_index_decreases(&**fields, index as int);
                    encode_fields_step(fields_model(fields@), index as int);
                }
                let field = &fields[index];
                length = checked_encoded_length_add(
                    checked_encoded_length_add(length, 2)?,
                    encoded_length(field.value())?,
                )?;
                index += 1;
            }
            Ok(length)
        }
        ValueRepr::Sum { payload, .. } => match payload {
            None => Ok(8),
            Some(child) => checked_encoded_length_add(8, encoded_length(child)?),
        },
        ValueRepr::Map(entries) => {
            #[cfg(verus_keep_ghost)]
            proof! {
                entries_model_properties(entries@);
                assert(entries_model(entries@).subrange(0, entries@.len() as int) == entries_model(entries@));
            }
            check_wire_length(entries.len())?;
            let mut length = 5;
            let mut index = 0;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant index <= entries.len(),
                    value.representation_valid() ==> entries_valid(entries@),
                    value.representation_valid() ==> length + encode_entries(entries_model(entries@).subrange(index as int, entries@.len() as int)).len()
                        == spec_encode(value.model()).len(), decreases_to!(*value => *entries), value.repr == ValueRepr::Map(*entries),
                decreases entries.len() - index,
            ))]
            while index < entries.len() {
                #[cfg(verus_keep_ghost)]
                proof! {
                    vstd::slice::lemma_slice_index_decreases(&**entries, index as int);
                    encode_entries_step(entries_model(entries@), index as int);
                }
                let entry = &entries[index];
                #[cfg(verus_keep_ghost)]
                proof! { if value.representation_valid() { entry_children_valid(*entry); } }
                let key = entry.encoded_key().len();
                let value = encoded_length(entry.value())?;
                check_wire_length(key)?;
                check_wire_length(value)?;
                length = checked_encoded_length_add(
                    checked_encoded_length_add(checked_encoded_length_add(length, 8)?, key)?,
                    value,
                )?;
                index += 1;
            }
            Ok(length)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (length <= u32::MAX),
        result.is_ok() ==> final(output)@ == old(output)@ + length_prefix(length as nat),
        result.is_err() ==> final(output)@ == old(output)@,
))]
fn put_zcve_length(output: &mut Vec<u8>, length: usize) -> Result<(), ValueError> {
    let length = u32::try_from(length).map_err(|_| ValueError::ArithmeticOverflow)?;
    append_word(output, u128::from(length), 4);
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (bytes.len() <= u32::MAX),
        result.is_ok() ==> final(output)@ == old(output)@ + blob(bytes@),
        result.is_err() ==> final(output)@ == old(output)@,
))]
fn put_zcve_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ValueError> {
    put_zcve_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == fields_ordered(fields_model(fields@)),
))]
fn ensure_strict_fields(fields: &[Field]) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! { fields_order_agrees(fields@); }
    let mut index = 1;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant 1 <= index, index <= fields.len() || fields.len() == 0,
            fields_ordered(fields_model(fields@)) == field_ids_ordered(fields@),
            forall|i: int| 0 <= i && i + 1 < index && i + 1 < fields.len() ==>
                (#[trigger] fields@[i]).id < fields@[i + 1].id,
        decreases fields.len() - index,
    ))]
    while index < fields.len() {
        if fields[index - 1].id >= fields[index].id {
            #[cfg(verus_keep_ghost)]
            proof! { assert(!field_ids_ordered(fields@)); }
            return Err(ValueError::RecordFieldOrder {
                previous: fields[index - 1].id,
                current: fields[index].id,
            });
        }
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == stored_keys_ordered(entries@),
        entries_valid(entries@) ==> result.is_ok() == entries_ordered(entries_model(entries@)),
))]
fn ensure_strict_map_keys(entries: &[MapEntry]) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! { if entries_valid(entries@) { entries_valid_parts(entries@); } }
    let mut index = 1;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant 1 <= index, index <= entries.len() || entries.len() == 0,
            entries_valid(entries@) ==> entries_ordered(entries_model(entries@)) == stored_keys_ordered(entries@),
            forall|i: int| 0 <= i && i + 1 < index && i + 1 < entries.len() ==>
                lex_less((#[trigger] entries@[i]).spec_encoded_key(), entries@[i + 1].spec_encoded_key()),
        decreases entries.len() - index,
    ))]
    while index < entries.len() {
        if !bytes_before(&entries[index - 1].encoded_key, &entries[index].encoded_key) {
            #[cfg(verus_keep_ghost)]
            proof! { assert(!stored_keys_ordered(entries@)); }
            return Err(ValueError::MapKeyOrder);
        }
        index += 1;
    }
    Ok(())
}

/// Structural kind of a closed value.
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)
)]
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
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, Eq, PartialEq))]
#[cfg_attr(verus_keep_ghost, derive(Clone, Copy))]
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

#[cfg(verus_keep_ghost)]
verus! {
impl ValueLimits {
    pub open spec fn model(self, max_input_bytes: u64) -> ModelLimits {
        ModelLimits { max_depth: self.max_depth, max_nodes: self.max_nodes,
            max_payload_bytes: self.max_payload_bytes,
            max_collection_len: self.max_collection_len, max_input_bytes }
    }
}
}

impl ValueLimits {
    /// Default maximum aggregate byte and text payload bytes.
    ///
    /// [`Value::text_ascii`] and [`Value::bytes`] also use this ceiling for one
    /// leaf so their successful outputs are admissible under the default
    /// payload budget.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

    /// Largest nesting depth any limits admit; a larger `max_depth` is
    /// capped here. It equals the catalog's value-depth ceiling.
    ///
    /// Validation, encoding and decoding recurse once per level. At this
    /// depth, decoding needs at most 1 MiB of stack in an optimized x86-64
    /// build (Rust 1.97.1), within a default 2 MiB thread; an unoptimized
    /// build needs up to 8 MiB. Without the cap, 60,000 levels exhausted an
    /// 8 MiB stack.
    #[cfg_attr(verus_keep_ghost, verus_verify)]
    pub const MAX_SUPPORTED_DEPTH: u32 = 1_024;

    /// The depth limit enforced: `max_depth`, capped at
    /// [`Self::MAX_SUPPORTED_DEPTH`].
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result as nat == decoder_model::depth_limit(self.model(u64::MAX)),
    ))]
    pub const fn effective_max_depth(self) -> u32 {
        if self.max_depth < Self::MAX_SUPPORTED_DEPTH {
            self.max_depth
        } else {
            Self::MAX_SUPPORTED_DEPTH
        }
    }
}

#[cfg(not(verus_keep_ghost))]
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
#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Copy, Debug, Default, Eq, PartialEq)
)]
#[cfg_attr(verus_keep_ghost, derive(Clone, Copy))]
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
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub struct AdmittedValue {
    value: Value,
    metrics: ValueMetrics,
    limits: ValueLimits,
}

#[cfg(not(verus_keep_ghost))]
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
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures *result == self.spec_value(),))]
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

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(metrics).nodes <= limits.max_nodes,
        old(metrics).payload_bytes <= limits.max_payload_bytes,
    ensures result.is_ok() ==> final(metrics).nodes <= limits.max_nodes
        && final(metrics).payload_bytes <= limits.max_payload_bytes,
        value.representation_valid() ==> result.is_ok() == admitted(value.model(), limits.model(0),
            depth as nat, old(metrics).nodes as nat, old(metrics).payload_bytes as nat),
        value.representation_valid() && result.is_ok() ==>
            final(metrics).nodes == old(metrics).nodes + nodes(value.model())
            && final(metrics).payload_bytes == old(metrics).payload_bytes + payload_bytes(value.model()),
    decreases value.model(),
))]
fn validate_value(
    value: &Value,
    depth: u32,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Value::model); reveal(Value::representation_valid);
        nodes_positive(value.model());
        if value.representation_valid() { value_children_valid(*value); }
    }
    if depth > limits.effective_max_depth() {
        return Err(ValueError::DepthLimit {
            limit: limits.effective_max_depth(),
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
            if !text.as_bytes().is_ascii() {
                return Err(ValueError::NonAsciiText);
            }
            add_payload(text.len(), limits, metrics)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                if value.representation_valid() {
                    assert(metrics.nodes == old(metrics).nodes + nodes(value.model()));
                    assert(metrics.payload_bytes == old(metrics).payload_bytes + payload_bytes(value.model()));
                    assert(within_shape(value.model(), limits.model(0), depth as nat));
                }
            }
        }
        ValueRepr::Tuple(items) | ValueRepr::Vector(items) => {
            #[cfg(verus_keep_ghost)]
            proof! { values_model_properties(items@); }
            validate_collection_len(items.len(), limits)?;
            validate_items(
                items,
                depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                limits,
                metrics,
            )?;
        }
        ValueRepr::Record(fields) => {
            #[cfg(verus_keep_ghost)]
            proof! { fields_model_properties(fields@); }
            validate_collection_len(fields.len(), limits)?;
            ensure_strict_fields(fields)?;
            validate_fields(
                fields,
                depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                limits,
                metrics,
            )?;
        }
        ValueRepr::Sum { payload, .. } => {
            #[cfg(verus_keep_ghost)]
            proof! {
                match value.model() {
                    ModelValue::Sum(id, variant, children) => { sum_costs(id, variant, children); },
                    _ => { assert(false); },
                }
            }
            if let Some(child) = payload {
                #[cfg(verus_keep_ghost)]
                proof! {
                    match value.model() {
                        ModelValue::Sum(_, _, children) => {
                            assert(children == seq![child.model()]);
                            vstd::seq::lemma_seq_index_decreases(children, 0);
                            assert(decreases_to!(value.model() => child.model()));
                        },
                        _ => { assert(false); },
                    }
                }
                validate_value(
                    child,
                    depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                    limits,
                    metrics,
                )?;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                if value.representation_valid() {
                    assert(metrics.nodes == old(metrics).nodes + nodes(value.model()));
                    assert(metrics.payload_bytes == old(metrics).payload_bytes + payload_bytes(value.model()));
                    assert(within_shape(value.model(), limits.model(0), depth as nat));
                }
            }
        }
        ValueRepr::Map(entries) => {
            #[cfg(verus_keep_ghost)]
            proof! { entries_model_properties(entries@); }
            validate_collection_len(entries.len(), limits)?;
            ensure_strict_map_keys(entries)?;
            validate_entries(
                entries,
                depth.checked_add(1).ok_or(ValueError::ArithmeticOverflow)?,
                limits,
                metrics,
            )?;
        }
        ValueRepr::Unit
        | ValueRepr::Bool(_)
        | ValueRepr::U128(_)
        | ValueRepr::I128(_)
        | ValueRepr::Enum { .. } => {}
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(metrics).nodes <= limits.max_nodes,
        old(metrics).payload_bytes <= limits.max_payload_bytes,
    ensures result.is_ok() ==> final(metrics).nodes <= limits.max_nodes
        && final(metrics).payload_bytes <= limits.max_payload_bytes,
        values_valid(items@) ==> result.is_ok() == items_admitted(values_model(items@), limits.model(0),
            depth as nat, old(metrics).nodes as nat, old(metrics).payload_bytes as nat),
        values_valid(items@) && result.is_ok() ==>
            final(metrics).nodes == old(metrics).nodes + item_nodes(values_model(items@))
            && final(metrics).payload_bytes == old(metrics).payload_bytes + item_payload(values_model(items@)),
    decreases values_model(items@),
))]
fn validate_items(
    items: &[Value],
    depth: u32,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        values_model_properties(items@);
        assert(values_model(items@).subrange(0, items@.len() as int) == values_model(items@));
    }
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= items.len(),
            metrics.nodes <= limits.max_nodes, metrics.payload_bytes <= limits.max_payload_bytes,
            values_valid(items@) ==> metrics.nodes + item_nodes(values_model(items@).subrange(index as int, items@.len() as int))
                == old(metrics).nodes + item_nodes(values_model(items@)),
            values_valid(items@) ==> metrics.payload_bytes + item_payload(values_model(items@).subrange(index as int, items@.len() as int))
                == old(metrics).payload_bytes + item_payload(values_model(items@)),
            values_valid(items@) ==> forall|i: int| 0 <= i < index ==> within_shape(values_model(items@)[i], limits.model(0), depth as nat),
        decreases items.len() - index,
    ))]
    while index < items.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            values_model_properties(items@);
            item_costs_step(values_model(items@), index as int);
            vstd::seq::lemma_seq_index_decreases(values_model(items@), index as int);
        }
        validate_value(&items[index], depth, limits, metrics)?;
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(metrics).nodes <= limits.max_nodes,
        old(metrics).payload_bytes <= limits.max_payload_bytes,
    ensures result.is_ok() ==> final(metrics).nodes <= limits.max_nodes
        && final(metrics).payload_bytes <= limits.max_payload_bytes,
        fields_valid(fields@) ==> result.is_ok() == fields_admitted(fields_model(fields@), limits.model(0),
            depth as nat, old(metrics).nodes as nat, old(metrics).payload_bytes as nat),
        fields_valid(fields@) && result.is_ok() ==>
            final(metrics).nodes == old(metrics).nodes + field_nodes(fields_model(fields@))
            && final(metrics).payload_bytes == old(metrics).payload_bytes + field_payload(fields_model(fields@)),
    decreases fields_model(fields@),
))]
fn validate_fields(
    fields: &[Field],
    depth: u32,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        fields_model_properties(fields@);
        assert(fields_model(fields@).subrange(0, fields@.len() as int) == fields_model(fields@));
    }
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= fields.len(),
            metrics.nodes <= limits.max_nodes, metrics.payload_bytes <= limits.max_payload_bytes,
            fields_valid(fields@) ==> metrics.nodes + field_nodes(fields_model(fields@).subrange(index as int, fields@.len() as int))
                == old(metrics).nodes + field_nodes(fields_model(fields@)),
            fields_valid(fields@) ==> metrics.payload_bytes + field_payload(fields_model(fields@).subrange(index as int, fields@.len() as int))
                == old(metrics).payload_bytes + field_payload(fields_model(fields@)),
            fields_valid(fields@) ==> forall|i: int| 0 <= i < index ==> within_shape(fields_model(fields@)[i].1, limits.model(0), depth as nat),
        decreases fields.len() - index,
    ))]
    while index < fields.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            fields_model_properties(fields@);
            field_costs_step(fields_model(fields@), index as int);
            vstd::seq::lemma_seq_index_decreases(fields_model(fields@), index as int);
        }
        validate_value(fields[index].value(), depth, limits, metrics)?;
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires old(metrics).nodes <= limits.max_nodes,
        old(metrics).payload_bytes <= limits.max_payload_bytes,
    ensures result.is_ok() ==> final(metrics).nodes <= limits.max_nodes
        && final(metrics).payload_bytes <= limits.max_payload_bytes,
        (entries_valid(entries@) && entry_values_fit(entries@)) ==> result.is_ok() == entries_admitted(entries_model(entries@), limits.model(0),
            depth as nat, old(metrics).nodes as nat, old(metrics).payload_bytes as nat),
        (entries_valid(entries@) && entry_values_fit(entries@)) && result.is_ok() ==>
            final(metrics).nodes == old(metrics).nodes + entry_nodes(entries_model(entries@))
            && final(metrics).payload_bytes == old(metrics).payload_bytes + entry_payload(entries_model(entries@)),
    decreases entries_model(entries@),
))]
fn validate_entries(
    entries: &[MapEntry],
    depth: u32,
    limits: ValueLimits,
    metrics: &mut ValueMetrics,
) -> Result<(), ValueError> {
    #[cfg(verus_keep_ghost)]
    proof! {
        entries_model_properties(entries@);
        assert(entries_model(entries@).subrange(0, entries@.len() as int) == entries_model(entries@));
    }
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= entries.len(),
            metrics.nodes <= limits.max_nodes, metrics.payload_bytes <= limits.max_payload_bytes,
            (entries_valid(entries@) && entry_values_fit(entries@)) ==> metrics.nodes + entry_nodes(entries_model(entries@).subrange(index as int, entries@.len() as int))
                == old(metrics).nodes + entry_nodes(entries_model(entries@)),
            (entries_valid(entries@) && entry_values_fit(entries@)) ==> metrics.payload_bytes + entry_payload(entries_model(entries@).subrange(index as int, entries@.len() as int))
                == old(metrics).payload_bytes + entry_payload(entries_model(entries@)),
            (entries_valid(entries@) && entry_values_fit(entries@)) ==> forall|i: int| 0 <= i < index ==> within_shape(entries_model(entries@)[i].0, limits.model(0), depth as nat) && within_shape(entries_model(entries@)[i].1, limits.model(0), depth as nat),
        decreases entries.len() - index,
    ))]
    while index < entries.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            entries_model_properties(entries@);
            entry_costs_step(entries_model(entries@), index as int);
            vstd::seq::lemma_seq_index_decreases(entries_model(entries@), index as int);
            if entries_valid(entries@) { entry_children_valid(entries@[index as int]); }
        }
        let entry = &entries[index];
        add_payload(entry.encoded_key().len(), limits, metrics)?;
        add_payload(entry.value().encoded_length()?, limits, metrics)?;
        validate_value(entry.key(), depth, limits, metrics)?;
        validate_value(entry.value(), depth, limits, metrics)?;
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (length <= limits.max_collection_len),
))]
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

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_ok() == (old(metrics).payload_bytes + length <= limits.max_payload_bytes),
        final(metrics).nodes == old(metrics).nodes,
        final(metrics).depth == old(metrics).depth,
        result.is_ok() ==> final(metrics).payload_bytes == old(metrics).payload_bytes + length,
))]
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
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
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

#[cfg(not(verus_keep_ghost))]
impl core::error::Error for ValueError {}

#[cfg(not(verus_keep_ghost))]
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
    fn depth_limits_are_capped_at_the_supported_ceiling() {
        let limits = |max_depth| ValueLimits {
            max_depth,
            ..ValueLimits::default()
        };
        assert_eq!(limits(u32::MAX).effective_max_depth(), 1_024);
        assert_eq!(limits(1_025).effective_max_depth(), 1_024);
        assert_eq!(limits(1_024).effective_max_depth(), 1_024);
        assert_eq!(limits(1_023).effective_max_depth(), 1_023);
        assert_eq!(ValueLimits::default().effective_max_depth(), 64);
        let mut value = Value::unit();
        for _ in 0..3 {
            value =
                Value::vector(vec![value]).unwrap_or_else(|error| panic!("collection: {error}"));
        }
        assert_eq!(
            value.validate_limits(limits(2)).err(),
            Some(ValueError::DepthLimit {
                limit: 2,
                attempted: 3,
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
