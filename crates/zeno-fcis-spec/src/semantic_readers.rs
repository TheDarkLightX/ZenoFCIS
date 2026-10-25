//! Pure semantic readers shared by the typed syntax fold and Verus.
//!
//! Signed magnitudes retain their mathematical value or are refused. A
//! projection reader consumes its complete input: every suffix token belongs
//! to a dot/nonzero-u32 pair, and every pair contributes exactly one segment.

// The remainder expression is shared directly with the arithmetic proof.
#![allow(clippy::manual_is_multiple_of)]

use alloc::vec::Vec;

use crate::lexical_core::LexemeKind;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
    pub(crate) open spec fn signed_value(negative: bool, magnitude: u128) -> int {
        if negative { -(magnitude as int) } else { magnitude as int }
    }

    pub(crate) open spec fn signed_representable(negative: bool, magnitude: u128) -> bool {
        i128::MIN <= signed_value(negative, magnitude) <= i128::MAX
    }

    pub(crate) open spec fn projection_pair(kinds: Seq<LexemeKind>, index: int) -> bool {
        0 <= index && 2 * index + 1 < kinds.len()
        && kinds[2 * index] == LexemeKind::Dot
        && match kinds[2 * index + 1] {
            LexemeKind::Number(value) => 0 < value <= u32::MAX,
            _ => false,
        }
    }

    pub(crate) open spec fn projection_tail_valid(kinds: Seq<LexemeKind>, max_segments: usize) -> bool {
        kinds.len() % 2 == 0 && kinds.len() / 2 <= max_segments
        && forall|index: int| 0 <= index < kinds.len() / 2 ==> projection_pair(kinds, index)
    }

    pub(crate) open spec fn projection_values(kinds: Seq<LexemeKind>, values: Seq<u32>) -> bool {
        values.len() * 2 == kinds.len()
        && forall|index: int| 0 <= index < values.len() ==>
            0 < values[index]
            && kinds[2 * index] == LexemeKind::Dot
            && kinds[2 * index + 1] == LexemeKind::Number(values[index] as u128)
    }
}

/// Interpret a separate sign and unsigned magnitude without wrapping.
///
/// Both zero spellings map to zero. The negative endpoint 2^127 maps to
/// `i128::MIN`; the same positive magnitude and all larger magnitudes refuse.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result.is_some() == signed_representable(negative, magnitude),
        match result {
            Some(value) => value as int == signed_value(negative, magnitude),
            None => !signed_representable(negative, magnitude),
        },
))]
pub(crate) fn signed_magnitude(negative: bool, magnitude: u128) -> Option<i128> {
    if magnitude <= i128::MAX as u128 {
        let value = magnitude as i128;
        Some(if negative { -value } else { value })
    } else if negative && magnitude == i128::MAX as u128 + 1 {
        Some(i128::MIN)
    } else {
        None
    }
}

/// Read all tokens after a root as zero or more `.` / nonzero-u32 pairs.
///
/// Empty input is the variable case and returns an empty vector, including
/// when `max_segments` is zero. Nonempty input can never return an empty
/// vector. Malformed, out-of-range, or over-budget suffixes refuse in full.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result.is_some() == projection_tail_valid(kinds@, max_segments),
        match result {
            Some(values) => projection_values(kinds@, values@)
                && values@.len() <= max_segments
                && (values@.len() == 0) == (kinds@.len() == 0),
            None => !projection_tail_valid(kinds@, max_segments),
        },
))]
pub(crate) fn read_projection_tail(kinds: &[LexemeKind], max_segments: usize) -> Option<Vec<u32>> {
    if kinds.len() % 2 != 0 || kinds.len() / 2 > max_segments {
        return None;
    }
    let count = kinds.len() / 2;
    let mut values = Vec::new();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            kinds@.len() % 2 == 0,
            count == kinds@.len() / 2,
            count <= max_segments,
            index <= count,
            values@.len() == index,
            forall|previous: int| 0 <= previous < index ==>
                0 < values@[previous]
                && kinds@[2 * previous] == LexemeKind::Dot
                && kinds@[2 * previous + 1] == LexemeKind::Number(values@[previous] as u128),
        decreases count - index,
    ))]
    while index < count {
        let offset = index * 2;
        match kinds[offset] {
            LexemeKind::Dot => (),
            _ => {
                #[cfg(verus_keep_ghost)]
                proof! { assert(!projection_pair(kinds@, index as int)); }
                return None;
            }
        }
        let magnitude = match kinds[offset + 1] {
            LexemeKind::Number(value) => value,
            _ => {
                #[cfg(verus_keep_ghost)]
                proof! { assert(!projection_pair(kinds@, index as int)); }
                return None;
            }
        };
        if magnitude == 0 || magnitude > u32::MAX as u128 {
            #[cfg(verus_keep_ghost)]
            proof! { assert(!projection_pair(kinds@, index as int)); }
            return None;
        }
        values.push(magnitude as u32);
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|position: int| 0 <= position < kinds@.len() / 2 implies
            projection_pair(kinds@, position) by {
            assert(0 < values@[position]);
            assert(kinds@[2 * position] == LexemeKind::Dot);
            assert(kinds@[2 * position + 1] == LexemeKind::Number(values@[position] as u128));
        }
    }
    Some(values)
}
