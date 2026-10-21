//! Exact integer byte reads for the V2 canonical-input proof chain.
//!
//! These primitives neither admit a schema nor authorize a decision. They do
//! not charge a meter; callers must establish their ingress/access cost first.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub mod envelope;
pub mod output;
pub mod schema;

#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Reads at most 16 bytes as an unsigned big-endian integer.
///
/// Returns the integer and first unread offset. Width zero returns zero at any
/// offset through the end of the input. Excessive width, missing bytes or an
/// offset beyond the input refuse with `None`, without reading a payload.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::read_unsigned(bytes@, offset, width),
))]
pub fn read_big_endian(bytes: &[u8], offset: usize, width: usize) -> Option<(u128, usize)> {
    if width > 16 || offset > bytes.len() || width > bytes.len() - offset {
        return None;
    }
    let mut index = 0usize;
    let mut value = 0u128;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= width <= 16,
            offset + width <= bytes@.len(),
            offset + width <= usize::MAX,
            value as int == spec::unsigned(bytes@, offset as int, index as nat),
        decreases width - index,
    ))]
    while index < width {
        #[cfg(verus_keep_ghost)]
        proof! {
            spec::unsigned_bound(bytes@, offset as int, (index + 1) as nat);
            spec::machine_bound((index + 1) as nat);
            assert(spec::unsigned(bytes@, offset as int, (index + 1) as nat)
                == value as int * 256 + bytes@[offset as int + index as int] as int);
        }
        let shifted = value.checked_mul(256)?;
        let next = shifted.checked_add(bytes[offset + index] as u128)?;
        value = next;
        index += 1;
    }
    Some((value, offset + width))
}

/// Reads one signed, two's-complement i128 in the existing ZCVE byte order.
///
/// Every 16-byte value is supported, including both signed extremes. Missing
/// bytes and invalid offsets refuse with `None`; trailing bytes are untouched.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::read_signed(bytes@, offset),
))]
pub fn read_signed_128(bytes: &[u8], offset: usize) -> Option<(i128, usize)> {
    match read_big_endian(bytes, offset, 16) {
        Some((value, end)) => {
            let signed = if value <= i128::MAX as u128 {
                value as i128
            } else {
                -((u128::MAX - value) as i128) - 1
            };
            Some((signed, end))
        }
        None => None,
    }
}
