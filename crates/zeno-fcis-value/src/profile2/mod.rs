//! Experimental closed ZCVE/2 values, fixed-width atoms and conversions.
//!
//! Arithmetic uses the pinned PR42 algorithms with an explicitly factored
//! comparison module shared by runtime callers and the proof harness.
//! The owned recursive grammar and explicit resource admission are separate
//! from legacy Value/schema/Authority admission. No native or proof qualification
//! is inferred, and the profile/tag registry remains experimental.

extern crate alloc;

use alloc::string::String;

// Preserve capsule formatting outside the reviewed comparison-module factor.
#[rustfmt::skip]
mod arithmetic256;
pub use arithmetic256::{Failure, I256, Rounding, U256, U512, U768};

mod value;
pub use value::{
    AdmittedValue, ConversionError, Field, MapEntry, Value, ValueError, ValueKind, ValueLimits,
    ValueMetrics, ValueRef,
};

/// Proposed leaf tags; they are never admitted by the ZCVE/1 decoder.
pub const TAG_U256: u8 = 0x0d;
pub const TAG_I256: u8 = 0x0e;

/// Explicit profile selection; there is no default/global width switch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueProfile {
    Zcve1,
    Zcve2,
}

impl ValueProfile {
    pub fn parse(value: &str) -> Result<Self, BoundaryError> {
        match value {
            "ZCVE/1" => Ok(Self::Zcve1),
            "ZCVE/2" => Ok(Self::Zcve2),
            _ => Err(BoundaryError::UnsupportedProfile),
        }
    }
}

/// Owned fixed-width leaves; signedness is not inferred from payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Integer256 {
    Unsigned(U256),
    Signed(I256),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryError {
    UnsupportedProfile,
    Length { expected: usize, actual: usize },
    UnknownTag(u8),
    DecimalSyntax,
    OutOfRange,
    Arithmetic(Failure),
}

fn range_failure(error: Failure) -> BoundaryError {
    match error {
        Failure::Overflow => BoundaryError::OutOfRange,
        other => BoundaryError::Arithmetic(other),
    }
}

/// Numeric big-endian payload, explicitly independent of limb memory layout.
#[must_use]
pub fn unsigned_payload(value: U256) -> [u8; 32] {
    let [low, high] = value.words_le();
    let mut output = [0; 32];
    output[..16].copy_from_slice(&high.to_be_bytes());
    output[16..].copy_from_slice(&low.to_be_bytes());
    output
}

#[must_use]
pub fn unsigned_from_payload(bytes: [u8; 32]) -> U256 {
    let mut high = [0; 16];
    let mut low = [0; 16];
    high.copy_from_slice(&bytes[..16]);
    low.copy_from_slice(&bytes[16..]);
    U256::from_words_le([u128::from_be_bytes(low), u128::from_be_bytes(high)])
}

/// New atom API only: legacy selection refuses before tag/length interpretation.
pub fn encode_atom(profile: ValueProfile, value: Integer256) -> Result<[u8; 33], BoundaryError> {
    if profile != ValueProfile::Zcve2 {
        return Err(BoundaryError::UnsupportedProfile);
    }
    let (tag, bits) = match value {
        Integer256::Unsigned(value) => (TAG_U256, value),
        Integer256::Signed(value) => (TAG_I256, value.bits()),
    };
    let mut output = [0; 33];
    output[0] = tag;
    output[1..].copy_from_slice(&unsigned_payload(bits));
    Ok(output)
}

/// Exactly33 bytes; no truncation, ignored suffix or two-word tuple alias.
pub fn decode_atom(profile: ValueProfile, bytes: &[u8]) -> Result<Integer256, BoundaryError> {
    if profile != ValueProfile::Zcve2 {
        return Err(BoundaryError::UnsupportedProfile);
    }
    if bytes.len() != 33 {
        return Err(BoundaryError::Length {
            expected: 33,
            actual: bytes.len(),
        });
    }
    let mut payload = [0; 32];
    payload.copy_from_slice(&bytes[1..]);
    let bits = unsigned_from_payload(payload);
    match bytes[0] {
        TAG_U256 => Ok(Integer256::Unsigned(bits)),
        TAG_I256 => Ok(Integer256::Signed(I256::from_bits(bits))),
        other => Err(BoundaryError::UnknownTag(other)),
    }
}

/// Canonical ASCII decimal only; all78 digits remain full-width throughout.
pub fn parse_unsigned_decimal(text: &str) -> Result<U256, BoundaryError> {
    let bytes = text.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 78
        || (bytes.len() > 1 && bytes[0] == b'0')
        || !bytes.iter().all(u8::is_ascii_digit)
    {
        return Err(BoundaryError::DecimalSyntax);
    }
    let mut value = U256::ZERO;
    for digit in bytes {
        value = value
            .checked_mul(U256::from_u128(10))
            .map_err(range_failure)?;
        value = value
            .checked_add(U256::from_u128(u128::from(*digit - b'0')))
            .map_err(range_failure)?;
    }
    Ok(value)
}

pub fn parse_signed_decimal(text: &str) -> Result<I256, BoundaryError> {
    if text.len() > 78 {
        return Err(BoundaryError::DecimalSyntax);
    }
    let (negative, magnitude_text) = match text.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, text),
    };
    if negative && magnitude_text == "0" {
        return Err(BoundaryError::DecimalSyntax);
    }
    let magnitude = parse_unsigned_decimal(magnitude_text)?;
    I256::from_magnitude(magnitude, negative).map_err(range_failure)
}

pub fn unsigned_decimal(mut value: U256) -> Result<String, BoundaryError> {
    if value == U256::ZERO {
        return Ok(String::from("0"));
    }
    let mut reversed = [0_u8; 78];
    let mut length = 0;
    while value != U256::ZERO {
        if length == reversed.len() {
            return Err(BoundaryError::Arithmetic(Failure::Invariant));
        }
        let (quotient, remainder) = value
            .div_rem(U256::from_u128(10))
            .map_err(BoundaryError::Arithmetic)?;
        let digit = u8::try_from(remainder.try_u128().map_err(BoundaryError::Arithmetic)?)
            .map_err(|_| BoundaryError::Arithmetic(Failure::Invariant))?;
        if digit > 9 {
            return Err(BoundaryError::Arithmetic(Failure::Invariant));
        }
        reversed[length] = b'0' + digit;
        length += 1;
        value = quotient;
    }
    let mut output = String::with_capacity(length);
    for digit in reversed[..length].iter().rev() {
        output.push(char::from(*digit));
    }
    Ok(output)
}

pub fn signed_decimal(value: I256) -> Result<String, BoundaryError> {
    let magnitude = unsigned_decimal(value.magnitude())?;
    if !value.is_negative() {
        return Ok(magnitude);
    }
    let mut output = String::with_capacity(magnitude.len() + 1);
    output.push('-');
    output.push_str(&magnitude);
    Ok(output)
}

#[cfg(test)]
mod boundary_tests;

#[cfg(test)]
mod value_tests;
