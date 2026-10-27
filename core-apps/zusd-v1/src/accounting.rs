//! Exact local accounting checks shared with the proof harness.
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// A valid recorded supply equals composite debt, without wrapping the sum.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (supply as int == active as int + default as int),
))]
pub(crate) fn debt_matches(supply: u128, active: u128, default: u128) -> bool {
    active.checked_add(default) == Some(supply)
}

/// An external asset movement needs both its debit and its credit.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (is_zusd || (has_from && has_to)),
        !is_zusd && result ==> has_from && has_to,
))]
pub(crate) fn movement_allowed(has_from: bool, has_to: bool, is_zusd: bool) -> bool {
    !((!has_from || !has_to) && !is_zusd)
}

/// Local change is exact, or refuses an out-of-range quantity.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(next) => next as int == old as int
            + if credit { amount as int } else { -(amount as int) },
        None => if credit { old as int + amount as int > u128::MAX }
            else { old < amount },
    },
))]
pub(crate) fn balance_change(old: u128, amount: u128, credit: bool) -> Option<u128> {
    if credit {
        old.checked_add(amount)
    } else {
        old.checked_sub(amount)
    }
}
