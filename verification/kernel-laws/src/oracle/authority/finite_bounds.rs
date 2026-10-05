//! Exact finite-domain bounds shared by the runtime and Verus verification.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Returns the cardinality of a nonempty signed interval when it fits `u64`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        match result {
            Some(width) => min <= max
                && width as int == max as int - min as int + 1,
            None => min > max || max as int - min as int + 1 > u64::MAX,
        },
))]
pub(crate) fn interval_width(min: i64, max: i64) -> Option<u64> {
    let width = (max as i128) - (min as i128) + 1;
    if width <= 0 || width > (u64::MAX as i128) {
        None
    } else {
        Some(width as u64)
    }
}

/// Returns the exact product when it is at most the supplied limit.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        match result {
            Some(product) => product as int == (left as int) * (right as int)
                && product <= limit,
            None => (left as int) * (right as int) > limit,
        },
))]
pub(crate) fn bounded_product(left: u64, right: u64, limit: u64) -> Option<u64> {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(0 <= (left as int) * (right as int) <= u128::MAX) by(nonlinear_arith);
    }
    let product = (left as u128) * (right as u128);
    if product > (limit as u128) {
        None
    } else {
        Some(product as u64)
    }
}
