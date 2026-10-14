//! Canonical SMT-LIB numerals, including the complete signed i128 range.
//! The production printer and proof harness use these same executable bodies.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[path = "numeral/spec.rs"]
pub(crate) mod spec;

/// Appends the unsigned decimal digits without leading zeroes.
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures final(out)@ == old(out)@ + spec::decimal(value as nat),
    decreases value,
))]
pub(crate) fn append_unsigned(out: &mut String, value: u128) {
    if value >= 10 {
        append_unsigned(out, value / 10);
    }
    out.push((b'0' + (value % 10) as u8) as char);
}

/// Negative SMT numerals use `(- magnitude)`, rather than a signed token.
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures final(out)@ == old(out)@ + spec::signed(value as int),
))]
pub(crate) fn append_signed(out: &mut String, value: i128) {
    #[cfg(verus_keep_ghost)]
    proof! {
        broadcast use vstd::seq::group_seq_lemmas;
    }
    if value < 0 {
        out.push('(');
        out.push('-');
        out.push(' ');
        // Adding one before negating also handles i128::MIN without overflow.
        let magnitude = (-(value + 1)) as u128 + 1;
        #[cfg(verus_keep_ghost)]
        proof! { assert(magnitude as int == -(value as int)); }
        append_unsigned(out, magnitude);
        out.push(')');
    } else {
        append_unsigned(out, value as u128);
    }
}
