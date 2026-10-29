//! Existing finite-wide mathematical definitions, shared without executable helpers.
use vstd::prelude::*;
verus! {
pub open spec fn scale() -> int { u128::MAX as int + 1 }
pub open spec fn number2(v: [u128; 2]) -> int {
    v[1] as int * scale() + v[0] as int
}
pub(crate) open spec fn number3(v: [u128; 3]) -> int {
    v[2] as int * scale() * scale() + v[1] as int * scale() + v[0] as int
}
pub(crate) open spec fn floor_value(a: u128, b: u128, den: u128) -> Option<u128> {
    if den == 0 { None }
    else {
        let q = (a as int * b as int) / den as int;
        if q > u128::MAX { None } else { Some(q as u128) }
    }
}
pub(crate) open spec fn ceiling_value(a: u128, b: u128, den: u128) -> Option<u128> {
    if den == 0 { None }
    else {
        let p = a as int * b as int;
        let q = p / den as int + if p % den as int == 0 { 0int } else { 1int };
        if q > u128::MAX { None } else { Some(q as u128) }
    }
}
}
