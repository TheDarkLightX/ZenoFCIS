//! Mathematical base-256 and signed byte interpretation, without machine wrap.
use vstd::prelude::*;

verus! {
pub open spec fn power(count: nat) -> int
    decreases count,
{
    if count == 0 { 1 } else { 256 * power((count - 1) as nat) }
}

pub proof fn power_positive(count: nat)
    ensures power(count) > 0,
    decreases count,
{
    if count > 0 { power_positive((count - 1) as nat); }
}

pub proof fn power_monotone(left: nat, right: nat)
    requires left <= right,
    ensures 0 < power(left) <= power(right),
    decreases right - left,
{
    if left == right { power_positive(left); }
    else { power_monotone(left, (right - 1) as nat); }
}

pub open spec fn unsigned(bytes: Seq<u8>, offset: int, count: nat) -> int
    recommends 0 <= offset, offset + count <= bytes.len(),
    decreases count,
{
    if count == 0 { 0 }
    else {
        unsigned(bytes, offset, (count - 1) as nat) * 256
            + bytes[offset + count - 1] as int
    }
}

pub proof fn unsigned_bound(bytes: Seq<u8>, offset: int, count: nat)
    requires 0 <= offset, offset + count <= bytes.len(),
    ensures 0 <= unsigned(bytes, offset, count) < power(count),
    decreases count,
{
    if count > 0 { unsigned_bound(bytes, offset, (count - 1) as nat); }
}

pub proof fn machine_bound(count: nat)
    requires count <= 16,
    ensures power(count) <= u128::MAX as int + 1,
{
    power_monotone(count, 16);
    reveal_with_fuel(power, 17);
    assert(power(16) == u128::MAX as int + 1);
}

pub open spec fn read_unsigned(bytes: Seq<u8>, offset: usize, width: usize)
    -> Option<(u128, usize)>
{
    if width <= 16 && offset <= bytes.len() && width <= bytes.len() - offset {
        Some((unsigned(bytes, offset as int, width as nat) as u128, (offset + width) as usize))
    } else { None }
}

pub open spec fn signed(value: u128) -> i128 {
    (if value as int <= i128::MAX as int { value as int }
     else { value as int - (u128::MAX as int + 1) }) as i128
}

pub open spec fn read_signed(bytes: Seq<u8>, offset: usize) -> Option<(i128, usize)> {
    match read_unsigned(bytes, offset, 16) {
        Some((value, end)) => Some((signed(value), end)),
        None => None,
    }
}
}
