//! Integer specifications and refinements of the actual numeric declarations.
use super::{I256, U256, U512, U768};
pub(crate) use super::number256::ordering_model::{half, modulus, radix, signed, unsigned};
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
