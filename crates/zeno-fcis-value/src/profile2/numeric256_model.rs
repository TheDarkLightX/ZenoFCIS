//! Integer specifications and refinements of the actual numeric declarations.
pub(crate) use super::number256::ordering_model::{half, modulus, radix, signed, unsigned};
use super::{Failure, I256, U256, U512, U768};
use vstd::prelude::*;

verus! {
impl U512 {
    pub closed spec fn numeric_words(&self) -> [u128; 4] { self.0 }
}
impl U768 {
    pub closed spec fn numeric_words(&self) -> [u128; 6] { self.0 }
}
pub(super) proof fn wide_representation()
    ensures
        forall|value: U512| #[trigger] value.numeric_words() == value.0,
        forall|value: U768| #[trigger] value.numeric_words() == value.0,
{
    reveal(U512::numeric_words);
    reveal(U768::numeric_words);
}
pub open spec fn unsigned512(value: U512) -> int {
    let w = value.numeric_words();
    w@[0] as int + radix() * (w@[1] as int + radix() *
        (w@[2] as int + radix() * w@[3] as int))
}
pub open spec fn unsigned768(value: U768) -> int {
    let w = value.numeric_words();
    w@[0] as int + radix() * (w@[1] as int + radix() *
        (w@[2] as int + radix() * (w@[3] as int + radix() *
        (w@[4] as int + radix() * w@[5] as int))))
}

/// The result must succeed on every fitting input and otherwise be Overflow.
pub open spec fn unsigned_result(result: Result<U256, Failure>, value: int) -> bool {
    match result {
        Ok(actual) => 0 <= value < modulus() && unsigned(actual) == value,
        Err(error) => error == Failure::Overflow && !(0 <= value < modulus()),
    }
}
pub open spec fn signed_result(result: Result<I256, Failure>, value: int) -> bool {
    match result {
        Ok(actual) => -half() <= value < half() && signed(actual) == value,
        Err(error) => error == Failure::Overflow && !(-half() <= value < half()),
    }
}
pub open spec fn word_result(result: Result<u128, Failure>, value: int) -> bool {
    match result {
        Ok(actual) => 0 <= value < radix() && actual as int == value,
        Err(error) => error == Failure::Overflow && !(0 <= value < radix()),
    }
}

/// Prefix interpretation for the selected two-limb helper instantiations.
pub open spec fn prefix2(words: Seq<u128>, count: int) -> int {
    if count == 0 { 0 } else if count == 1 { words[0] as int }
    else { words[0] as int + radix() * words[1] as int }
}
pub open spec fn place2(count: int) -> int {
    if count == 0 { 1 } else if count == 1 { radix() } else { modulus() }
}

pub(super) proof fn complement(word: u128)
    ensures (!word) as int == u128::MAX as int - word as int,
{
    assert(!word == u128::MAX - word) by(bit_vector);
}

pub(super) proof fn unsigned_word(word: u128)
    ensures unsigned(U256([word, 0])) == word as int,
{
    conversion_representation();
    let value = U256([word, 0]);
    assert(value.0@[0] == word);
    assert(value.0@[1] == 0);
    assert(unsigned(value) == value.0@[0] as int + radix() * value.0@[1] as int);
}

pub(super) proof fn signed_word_cast(word: i128)
    ensures
        (word as u128) as int == if word < 0 { word as int + radix() } else { word as int },
        signed(I256(U256([word as u128, if word < 0 { u128::MAX } else { 0 }]))) == word as int,
{
    super::number256::numeric_representation();
    assert((word as u128) as int ==
        if word < 0 { word as int + 340282366920938463463374607431768211456int }
        else { word as int }) by(bit_vector);
    let bits = U256([word as u128, if word < 0 { u128::MAX } else { 0 }]);
    if word < 0 {
        assert(unsigned(bits) == word as int + modulus());
        assert(unsigned(bits) >= half());
    } else {
        assert(unsigned(bits) == word as int);
        assert(unsigned(bits) < half());
    }
}

pub(super) proof fn conversion_representation()
    ensures
        forall|value: U256| #[trigger] value.ordering_words() == value.0,
        forall|value: I256| #[trigger] value.ordering_bits() == value.0,
        forall|value: U256| #[trigger] unsigned(value)
            == value.0@[0] as int + radix() * value.0@[1] as int,
        forall|value: I256| #[trigger] signed(value)
            == if unsigned(value.0) >= half() {
                unsigned(value.0) - modulus()
            } else { unsigned(value.0) },
        forall|value: U512| #[trigger] value.numeric_words() == value.0,
        forall|value: U768| #[trigger] value.numeric_words() == value.0,
{
    super::number256::numeric_representation();
    wide_representation();
}

pub(super) proof fn unsigned_bounds(value: U256)
    ensures 0 <= unsigned(value) < modulus(),
{
    super::number256::ordering_model::unsigned_bounds(value);
}

pub(super) proof fn signed_bounds(value: I256)
    ensures -half() <= signed(value) < half(),
{
    conversion_representation();
    super::number256::ordering_model::signed_bounds(value.0);
}

pub(super) proof fn minimum_bits()
    ensures forall|value: I256| #[trigger] signed(value) == -half()
        ==> value == I256(U256([0, 170141183460469231731687303715884105728u128])),
{
    conversion_representation();
    assert forall|value: I256| #[trigger] signed(value) == -half()
        implies value == I256(U256([0, 170141183460469231731687303715884105728u128])) by {
        unsigned_bounds(value.0);
        assert(unsigned(value.0) == half());
        assert(value.0.0@[1] == 170141183460469231731687303715884105728u128);
        assert(value.0.0@[0] == 0);
        assert(value.0.0 =~= [0u128, 170141183460469231731687303715884105728u128]);
    }
}

pub(super) proof fn constant_bits()
    ensures
        unsigned(U256([0, 0])) == 0,
        unsigned(U256([1, 0])) == 1,
        unsigned(U256([u128::MAX, u128::MAX])) == modulus() - 1,
        signed(I256(U256([0, 0]))) == 0,
        signed(I256(U256([0, 170141183460469231731687303715884105728u128]))) == -half(),
        signed(I256(U256([u128::MAX, 170141183460469231731687303715884105727u128]))) == half() - 1,
        forall|value: U256| unsigned(value) == 0 ==> #[trigger] signed(I256(value)) == 0,
{
    super::number256::numeric_representation();
    assert(radix() == 340282366920938463463374607431768211456int);
    assert(modulus() == 115792089237316195423570985008687907853269984665640564039457584007913129639936int);
    assert(half() == 57896044618658097711785492504343953926634992332820282019728792003956564819968int);
    assert(unsigned(U256([0, 0])) == 0);
    assert(unsigned(U256([1, 0])) == 1);
    assert(unsigned(U256([u128::MAX, u128::MAX])) == 115792089237316195423570985008687907853269984665640564039457584007913129639935int);
    assert(unsigned(U256([0, 170141183460469231731687303715884105728u128])) == 57896044618658097711785492504343953926634992332820282019728792003956564819968int);
    assert(unsigned(U256([u128::MAX, 170141183460469231731687303715884105727u128])) == 57896044618658097711785492504343953926634992332820282019728792003956564819967int);
}

}
