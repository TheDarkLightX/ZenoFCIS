//! Exact unsigned and two-complement meanings of actual closed numeric atoms.
//! Candidate proof bodies require actual Verus qualification.
pub(crate) use super::super::ordering256::ordering_model::{
    lex_gt, lex_lt, lex_result, suffix_equal,
};
use super::super::wide;
use super::{I256, U256};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {
// These definitions reuse the existing finite-wide mathematical interpretation.
// They do not invoke or replace wide's executable less_equal implementation.
pub open spec fn radix() -> int { wide::scale() }
pub open spec fn modulus() -> int { radix() * radix() }
pub open spec fn half() -> int { modulus() / 2 }
pub open spec fn unsigned(value: U256) -> int {
    wide::number2(value.ordering_words())
}
pub open spec fn signed_bits(bits: U256) -> int {
    if unsigned(bits) >= half() { unsigned(bits) - modulus() }
    else { unsigned(bits) }
}
pub open spec fn signed(value: I256) -> int {
    signed_bits(value.ordering_bits())
}
pub open spec fn integer_order(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

pub(crate) proof fn limbs_bounds(words: [u128; 2])
    ensures 0 <= wide::number2(words) < modulus(),
{
    assert(0 <= (words@[0] as int) < radix());
    assert(0 <= (words@[1] as int) < radix());
    assert(0 <= wide::number2(words) < modulus()) by(nonlinear_arith)
        requires
            0 <= (words@[0] as int) < radix(),
            0 <= (words@[1] as int) < radix(),
            radix() == u128::MAX as int + 1,
            wide::number2(words) == words@[1] as int * radix() + words@[0] as int,
            modulus() == radix() * radix();
}

/// The premise is produced by the actual compare body, not a caller assumption
/// on U256::cmp. This lemma itself must be checked before using its contract.
pub(crate) proof fn lex_two(a: [u128; 2], b: [u128; 2], result: Ordering)
    requires lex_result(a@, b@, result),
    ensures result == integer_order(wide::number2(a), wide::number2(b)),
{
    // Every ghost array lookup explicitly uses a Seq view and an int index.
    assert(a@.len() == 2 && b@.len() == 2);
    assert(suffix_equal(a@, b@, 2));
    assert(suffix_equal(a@, b@, 1) == (a@[1] == b@[1]));

    if a@[1] < b@[1] {
        assert(lex_lt(a@, b@)) by {
            assert(0 <= 1 < a@.len() && a@[1] < b@[1] && suffix_equal(a@, b@, 2));
        }
        assert(!lex_gt(a@, b@)) by {
            assert forall|j: int| 0 <= j < 2
                implies !(a@[j] > b@[j] && suffix_equal(a@, b@, j + 1)) by {
                if j == 0 { assert(!suffix_equal(a@, b@, 1)); }
            }
        }
        assert(a@ != b@);
        assert(wide::number2(a) < wide::number2(b));
    } else if a@[1] > b@[1] {
        assert(lex_gt(a@, b@)) by {
            assert(0 <= 1 < a@.len() && a@[1] > b@[1] && suffix_equal(a@, b@, 2));
        }
        assert(!lex_lt(a@, b@)) by {
            assert forall|j: int| 0 <= j < 2
                implies !(a@[j] < b@[j] && suffix_equal(a@, b@, j + 1)) by {
                if j == 0 { assert(!suffix_equal(a@, b@, 1)); }
            }
        }
        assert(a@ != b@);
        assert(wide::number2(a) > wide::number2(b));
    } else if a@[0] < b@[0] {
        assert(lex_lt(a@, b@)) by {
            assert(0 <= 0 < a@.len() && a@[0] < b@[0] && suffix_equal(a@, b@, 1));
        }
        assert(!lex_gt(a@, b@)) by {
            assert forall|j: int| 0 <= j < 2
                implies !(a@[j] > b@[j] && suffix_equal(a@, b@, j + 1)) by {}
        }
        assert(a@ != b@);
        assert(wide::number2(a) < wide::number2(b));
    } else if a@[0] > b@[0] {
        assert(lex_gt(a@, b@)) by {
            assert(0 <= 0 < a@.len() && a@[0] > b@[0] && suffix_equal(a@, b@, 1));
        }
        assert(!lex_lt(a@, b@)) by {
            assert forall|j: int| 0 <= j < 2
                implies !(a@[j] < b@[j] && suffix_equal(a@, b@, j + 1)) by {}
        }
        assert(a@ != b@);
        assert(wide::number2(a) > wide::number2(b));
    } else {
        assert(a@ =~= b@);
        assert(!lex_lt(a@, b@));
        assert(!lex_gt(a@, b@));
        assert(wide::number2(a) == wide::number2(b));
    }
}

pub(crate) proof fn sign_bit(word: u128)
    ensures (word >> 127 != 0) == (word >= 170141183460469231731687303715884105728u128),
{
    assert((word >> 127 != 0) == (word >= 170141183460469231731687303715884105728u128)) by(bit_vector);
}

pub(crate) proof fn high_half(words: [u128; 2])
    ensures (words@[1] >= 170141183460469231731687303715884105728u128)
        == (wide::number2(words) >= half()),
{
    assert(radix() == 2 * (170141183460469231731687303715884105728u128 as int));
    assert(half() == radix() * (170141183460469231731687303715884105728u128 as int));
    if words@[1] >= 170141183460469231731687303715884105728u128 {
        assert(wide::number2(words) >= half());
    } else {
        assert(wide::number2(words) < half());
    }
}

pub(crate) proof fn unsigned_bounds(bits: U256)
    ensures 0 <= unsigned(bits) < modulus(),
{
    reveal(U256::ordering_words);
    limbs_bounds(bits.ordering_words());
}

pub(crate) proof fn signed_bounds(bits: U256)
    ensures
        -half() <= signed_bits(bits) < half(),
        (signed_bits(bits) < 0) == (unsigned(bits) >= half()),
{
    unsigned_bounds(bits);
    assert(modulus() == 2 * half());
}

pub(crate) proof fn signed_order(a: U256, b: U256)
    ensures
        integer_order(signed_bits(a), signed_bits(b)) ==
        if unsigned(a) >= half() && unsigned(b) < half() { Ordering::Less }
        else if unsigned(a) < half() && unsigned(b) >= half() { Ordering::Greater }
        else { integer_order(unsigned(a), unsigned(b)) },
{
    signed_bounds(a);
    signed_bounds(b);
    if unsigned(a) >= half() {
        if unsigned(b) < half() {
            assert(signed_bits(a) < 0 <= signed_bits(b));
        }
    } else if unsigned(b) >= half() {
        assert(signed_bits(b) < 0 <= signed_bits(a));
    }
}

/// Concrete inhabited endpoints. This candidate is not a checked witness yet.
pub(crate) proof fn boundary_witnesses()
    ensures
        unsigned(U256([0, 0])) == 0,
        unsigned(U256([u128::MAX, u128::MAX])) == modulus() - 1,
        signed(I256(U256([0, 170141183460469231731687303715884105728u128]))) == -half(),
        signed(I256(U256([u128::MAX, 170141183460469231731687303715884105727u128]))) == half() - 1,
{
    reveal(U256::ordering_words);
    reveal(I256::ordering_bits);
    assert(radix() == 340282366920938463463374607431768211456int);
    assert(modulus() == 115792089237316195423570985008687907853269984665640564039457584007913129639936int);
    assert(half() == 57896044618658097711785492504343953926634992332820282019728792003956564819968int);
    assert(unsigned(U256([0, 0])) == 0);
    assert(unsigned(U256([u128::MAX, u128::MAX])) == 115792089237316195423570985008687907853269984665640564039457584007913129639935int);
    assert(unsigned(U256([0, 170141183460469231731687303715884105728u128])) == 57896044618658097711785492504343953926634992332820282019728792003956564819968int);
    assert(unsigned(U256([u128::MAX, 170141183460469231731687303715884105727u128])) == 57896044618658097711785492504343953926634992332820282019728792003956564819967int);
    assert(signed(I256(U256([0, 170141183460469231731687303715884105728u128]))) == -57896044618658097711785492504343953926634992332820282019728792003956564819968int);
    assert(signed(I256(U256([u128::MAX, 170141183460469231731687303715884105727u128]))) == 57896044618658097711785492504343953926634992332820282019728792003956564819967int);
}
}
