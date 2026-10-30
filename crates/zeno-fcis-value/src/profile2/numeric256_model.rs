//! Integer specifications and refinements of the actual numeric declarations.
pub(crate) use super::number256::ordering_model::{half, modulus, radix, signed, unsigned};
use super::number256::ordering_model::{lex_result, suffix_equal};
use super::{Failure, I256, U256, U512, U768};
use core::cmp::Ordering;
use vstd::arithmetic::{div_mod::*, mul::*, power2::*};
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


// Generic limb meanings used by the multiply/divide helpers at every width.

// Limb meanings.

/// The weight 2^(128*count) of the limb at position `count`.
pub open spec fn place(count: nat) -> int
    decreases count,
{
    if count == 0 { 1 } else { radix() * place((count - 1) as nat) }
}

/// The meaning of the lowest `count` limbs, least significant limb first.
pub open spec fn limbs(words: Seq<u128>, count: nat) -> int
    decreases count,
{
    if count == 0 { 0 }
    else { limbs(words, (count - 1) as nat) + place((count - 1) as nat) * words[count - 1] as int }
}

/// The meaning of every limb.
pub open spec fn value(words: Seq<u128>) -> int {
    limbs(words, words.len())
}

/// The meaning of the limbs from `start` upward, divided by place(start).
pub open spec fn upper(words: Seq<u128>, start: nat) -> int
    decreases words.len() - start,
{
    if start >= words.len() { 0 }
    else { words[start as int] as int + radix() * upper(words, start + 1) }
}

/// The value with its lowest `low` bits removed.
pub open spec fn top(words: Seq<u128>, low: nat) -> int {
    value(words) / pow2(low) as int
}

// Exact contracts of the multiply/divide family.

/// Exact Euclidean division: the canonical quotient/remainder witness.
pub open spec fn quotient_witness(n: int, d: int, q: int, r: int) -> bool {
    0 < d && 0 <= q && 0 <= r < d && n == q * d + r
}

/// The private divider's exact contract at every width N.
pub open spec fn divide_result<const N: usize>(n: Seq<u128>, d: Seq<u128>,
    result: Result<([u128; N], [u128; N]), Failure>) -> bool {
    match result {
        Ok((q, r)) => quotient_witness(value(n), value(d), value(q@), value(r@)),
        Err(error) => (error == Failure::ZeroDivisor && value(d) == 0)
            || (error == Failure::Domain && value(d) != 0 && 128 * N > usize::MAX),
    }
}

/// U256::div_rem: an exact canonical witness, or ZeroDivisor exactly when den is zero.
pub open spec fn div_rem_result(n: U256, d: U256, result: Result<(U256, U256), Failure>) -> bool {
    match result {
        Ok((q, r)) => quotient_witness(unsigned(n), unsigned(d), unsigned(q), unsigned(r)),
        Err(error) => error == Failure::ZeroDivisor && unsigned(d) == 0,
    }
}

/// The private rounding step: Exact refuses a remainder; Ceil adds one with checked overflow.
pub open spec fn rounded_result(q: U256, remainder: bool, mode: super::Rounding,
    result: Result<U256, Failure>) -> bool {
    if mode == super::Rounding::Exact && remainder {
        result == Err::<U256, Failure>(Failure::Inexact)
    } else {
        unsigned_result(result, unsigned(q)
            + if mode == super::Rounding::Ceil && remainder { 1int } else { 0int })
    }
}

/// The integer selected by a rounding mode for an exact quotient and remainder.
pub open spec fn rounded_value(q: int, r: int, mode: super::Rounding) -> int {
    match mode {
        super::Rounding::Ceil => if r != 0 { q + 1 } else { q },
        _ => q,
    }
}

/// U256::checked_div: ZeroDivisor first, then Inexact for Exact with a remainder,
/// then the rounded quotient with the exact Overflow partition.
pub open spec fn checked_div_result(n: U256, d: U256, mode: super::Rounding,
    result: Result<U256, Failure>) -> bool {
    if unsigned(d) == 0 {
        result == Err::<U256, Failure>(Failure::ZeroDivisor)
    } else if mode == super::Rounding::Exact && unsigned(n) % unsigned(d) != 0 {
        result == Err::<U256, Failure>(Failure::Inexact)
    } else {
        unsigned_result(result, rounded_value(unsigned(n) / unsigned(d), unsigned(n) % unsigned(d), mode))
    }
}

/// add_word: adds word*2^(128*at) exactly, refusing with Invariant exactly on overflow.
pub open spec fn add_word_result(before: Seq<u128>, after: Seq<u128>, at: usize, word: u128,
    result: Result<(), Failure>) -> bool {
    let total = value(before) + place(at as nat) * word as int;
    match result {
        Ok(_) => total < place(before.len()) && value(after) == total,
        Err(error) => error == Failure::Invariant && total >= place(before.len()),
    }
}

/// The private multiplier returns the exact complete product when the output has room.
pub open spec fn product_result<const O: usize>(a: Seq<u128>, b: Seq<u128>,
    result: Result<[u128; O], Failure>) -> bool {
    match result {
        Ok(out) => value(out@) == value(a) * value(b),
        Err(_) => false,
    }
}

/// U256::full_product: always the exact complete product.
pub open spec fn wide_product_result(result: Result<U512, Failure>, value: int) -> bool {
    match result {
        Ok(product) => unsigned512(product) == value,
        Err(_) => false,
    }
}

/// U512::checked_add: the exact sum iff it is below 2^512, otherwise exactly Overflow.
pub open spec fn unsigned512_result(result: Result<U512, Failure>, value: int) -> bool {
    match result {
        Ok(actual) => 0 <= value < modulus() * modulus() && unsigned512(actual) == value,
        Err(error) => error == Failure::Overflow && !(0 <= value < modulus() * modulus()),
    }
}
/// U512::multiply_u256: always the exact complete 768-bit product.
pub open spec fn product768_result(result: Result<U768, Failure>, value: int) -> bool {
    match result {
        Ok(product) => unsigned768(product) == value,
        Err(_) => false,
    }
}
/// U512::div_rem: an exact canonical witness, or ZeroDivisor exactly when den is zero.
pub open spec fn div_rem512_result(n: U512, d: U512, result: Result<(U512, U512), Failure>) -> bool {
    match result {
        Ok((q, r)) => quotient_witness(unsigned512(n), unsigned512(d), unsigned512(q), unsigned512(r)),
        Err(error) => error == Failure::ZeroDivisor && unsigned512(d) == 0,
    }
}
/// U768::div_rem: an exact canonical witness, or ZeroDivisor exactly when den is zero.
pub open spec fn div_rem768_result(n: U768, d: U768, result: Result<(U768, U768), Failure>) -> bool {
    match result {
        Ok((q, r)) => quotient_witness(unsigned768(n), unsigned768(d), unsigned768(q), unsigned768(r)),
        Err(error) => error == Failure::ZeroDivisor && unsigned768(d) == 0,
    }
}

/// U256::mul_add_div_rem: ZeroDivisor first, then Overflow exactly when the
/// floor quotient of the complete numerator needs more than 256 bits.
pub open spec fn mul_add_div_rem_result(p: int, d: int, result: Result<(U256, U256), Failure>) -> bool {
    if d == 0 {
        result == Err::<(U256, U256), Failure>(Failure::ZeroDivisor)
    } else if p / d >= modulus() {
        result == Err::<(U256, U256), Failure>(Failure::Overflow)
    } else {
        match result {
            Ok((q, r)) => quotient_witness(p, d, unsigned(q), unsigned(r)),
            Err(_) => false,
        }
    }
}
/// U256::mul_div and mul3_div: ZeroDivisor, then quotient Overflow, then
/// Inexact for Exact with a remainder, then the rounded quotient with the exact
/// Overflow partition of that value.
pub open spec fn mul_div_result(p: int, d: int, mode: super::Rounding, result: Result<U256, Failure>) -> bool {
    if d == 0 {
        result == Err::<U256, Failure>(Failure::ZeroDivisor)
    } else if p / d >= modulus() {
        result == Err::<U256, Failure>(Failure::Overflow)
    } else if mode == super::Rounding::Exact && p % d != 0 {
        result == Err::<U256, Failure>(Failure::Inexact)
    } else {
        unsigned_result(result, rounded_value(p / d, p % d, mode))
    }
}
/// U256::mul_div_half_up: floor((p + floor(d/2)) / d), with the same refusals.
pub open spec fn half_up_result(p: int, d: int, result: Result<U256, Failure>) -> bool {
    if d == 0 {
        result == Err::<U256, Failure>(Failure::ZeroDivisor)
    } else if (p + d / 2) / d >= modulus() {
        result == Err::<U256, Failure>(Failure::Overflow)
    } else {
        match result {
            Ok(q) => unsigned(q) == (p + d / 2) / d,
            Err(_) => false,
        }
    }
}
/// The product of three unsigned factors.
pub open spec fn triple(x: [U256; 3]) -> int {
    unsigned(x@[0]) * unsigned(x@[1]) * unsigned(x@[2])
}

// Limb weights.

pub(super) proof fn place_positive(count: nat)
    ensures place(count) > 0,
    decreases count,
{
    if count > 0 {
        place_positive((count - 1) as nat);
        lemma_mul_strictly_positive(radix(), place((count - 1) as nat));
    }
}

pub(super) proof fn place_add(m: nat, n: nat)
    ensures place(m + n) == place(m) * place(n),
    decreases n,
{
    if n > 0 {
        place_add(m, (n - 1) as nat);
        assert(place(m + n) == radix() * place((m + n - 1) as nat));
        assert(place(n) == radix() * place((n - 1) as nat));
        lemma_mul_is_associative(radix(), place(m), place((n - 1) as nat));
        lemma_mul_is_commutative(radix(), place(m));
        lemma_mul_is_associative(place(m), radix(), place((n - 1) as nat));
    }
}

pub(super) proof fn place_monotone(low: nat, high: nat)
    requires low <= high,
    ensures place(low) <= place(high),
    decreases high,
{
    if low < high {
        place_monotone(low, (high - 1) as nat);
        place_positive((high - 1) as nat);
        assert(place(high) == radix() * place((high - 1) as nat));
        lemma_mul_inequality(1, radix(), place((high - 1) as nat));
    }
}

pub(super) proof fn radix_power()
    ensures pow2(128) == radix(), radix() == 340282366920938463463374607431768211456int,
{
    lemma2_to64();
    lemma2_to64_rest();
    lemma_pow2_adds(64, 64);
}

pub(super) proof fn place_power(count: nat)
    ensures place(count) == pow2(128 * count) as int,
    decreases count,
{
    if count == 0 {
        lemma2_to64();
    } else {
        place_power((count - 1) as nat);
        radix_power();
        lemma_pow2_adds(128, (128 * (count - 1)) as nat);
        assert(128 + 128 * (count - 1) == 128 * count);
    }
}

// Limb meanings of actual arrays.

pub(super) proof fn limbs_bounds(words: Seq<u128>, count: nat)
    requires count <= words.len(),
    ensures 0 <= limbs(words, count) < place(count),
    decreases count,
{
    if count > 0 {
        let c = (count - 1) as nat;
        limbs_bounds(words, c);
        place_positive(c);
        let w = words[c as int] as int;
        assert(0 <= limbs(words, c) + place(c) * w < radix() * place(c)) by(nonlinear_arith)
            requires 0 <= limbs(words, c) < place(c), 0 <= w < radix(), place(c) > 0;
    }
}

pub(super) proof fn value_bounds(words: Seq<u128>)
    ensures 0 <= value(words) < place(words.len()),
{
    limbs_bounds(words, words.len());
}

pub(super) proof fn limbs_monotone(words: Seq<u128>, low: nat, high: nat)
    requires low <= high <= words.len(),
    ensures limbs(words, low) <= limbs(words, high),
    decreases high,
{
    if low < high {
        let c = (high - 1) as nat;
        limbs_monotone(words, low, c);
        place_positive(c);
        lemma_mul_nonnegative(place(c), words[c as int] as int);
    }
}

/// Writing one limb changes the meaning by its weight times the word difference.
pub(super) proof fn limbs_update(words: Seq<u128>, index: int, word: u128, count: nat)
    requires 0 <= index < words.len(), count <= words.len(),
    ensures limbs(words.update(index, word), count) == limbs(words, count)
        + if index < count { place(index as nat) * (word as int - words[index] as int) } else { 0 },
    decreases count,
{
    if count > 0 {
        let c = (count - 1) as nat;
        limbs_update(words, index, word, c);
        if index == c {
            lemma_mul_is_distributive_sub(place(c), word as int, words[index] as int);
        }
    }
}

/// Writing the next limb extends the low meaning by exactly that limb.
pub(super) proof fn limbs_push(words: Seq<u128>, index: nat, word: u128)
    requires index < words.len(),
    ensures limbs(words.update(index as int, word), index + 1) == limbs(words, index) + place(index) * word as int,
{
    limbs_update(words, index as int, word, index);
}

pub(super) proof fn value_update(words: Seq<u128>, index: int, word: u128)
    requires 0 <= index < words.len(),
    ensures value(words.update(index, word)) == value(words)
        + place(index as nat) * (word as int - words[index] as int),
{
    limbs_update(words, index, word, words.len());
}

/// Equal limbs below `count` give equal meanings below `count`.
pub(super) proof fn limbs_equal(a: Seq<u128>, b: Seq<u128>, count: nat)
    requires count <= a.len(), count <= b.len(),
        forall|k: int| 0 <= k < count ==> a[k] == b[k],
    ensures limbs(a, count) == limbs(b, count),
    decreases count,
{
    if count > 0 {
        limbs_equal(a, b, (count - 1) as nat);
    }
}

pub(super) proof fn upper_nonnegative(words: Seq<u128>, start: nat)
    ensures upper(words, start) >= 0,
    decreases words.len() - start,
{
    if start < words.len() {
        upper_nonnegative(words, start + 1);
    }
}

pub(super) proof fn upper_equal(a: Seq<u128>, b: Seq<u128>, start: nat)
    requires a.len() == b.len(), forall|k: int| start <= k < a.len() ==> a[k] == b[k],
    ensures upper(a, start) == upper(b, start),
    decreases a.len() - start,
{
    if start < a.len() {
        upper_equal(a, b, start + 1);
    }
}

/// The value is its low limbs plus the weighted upper limbs.
pub(super) proof fn value_split(words: Seq<u128>, start: nat)
    requires start <= words.len(),
    ensures value(words) == limbs(words, start) + place(start) * upper(words, start),
    decreases words.len() - start,
{
    if start < words.len() {
        value_split(words, start + 1);
        let w = words[start as int] as int;
        let u = upper(words, start + 1);
        assert(limbs(words, start + 1) == limbs(words, start) + place(start) * w);
        assert(place(start + 1) == radix() * place(start));
        assert(place(start) * w + (radix() * place(start)) * u
            == place(start) * (w + radix() * u)) by(nonlinear_arith);
    }
}

/// Division by a limb weight selects the upper limbs; the remainder is the low limbs.
pub(super) proof fn value_div_place(words: Seq<u128>, start: nat)
    requires start <= words.len(),
    ensures
        value(words) / place(start) == upper(words, start),
        value(words) % place(start) == limbs(words, start),
{
    value_split(words, start);
    limbs_bounds(words, start);
    place_positive(start);
    lemma_mul_is_commutative(place(start), upper(words, start));
    lemma_fundamental_div_mod_converse(value(words), place(start), upper(words, start),
        limbs(words, start));
}

pub(super) proof fn zero_value(words: Seq<u128>, count: nat)
    requires count <= words.len(), forall|k: int| 0 <= k < count ==> words[k] == 0,
    ensures limbs(words, count) == 0,
    decreases count,
{
    if count > 0 {
        zero_value(words, (count - 1) as nat);
    }
}

pub(super) proof fn nonzero_value(words: Seq<u128>, index: int)
    requires 0 <= index < words.len(), words[index] != 0,
    ensures value(words) > 0,
{
    value_split(words, (index + 1) as nat);
    limbs_bounds(words, index as nat);
    limbs_monotone(words, 0, index as nat);
    upper_nonnegative(words, (index + 1) as nat);
    place_positive(index as nat);
    place_positive((index + 1) as nat);
    assert(limbs(words, (index + 1) as nat)
        == limbs(words, index as nat) + place(index as nat) * words[index] as int);
    lemma_mul_strictly_positive(place(index as nat), words[index] as int);
    lemma_mul_nonnegative(place((index + 1) as nat), upper(words, (index + 1) as nat));
}

/// The meaning of two limbs is the existing unsigned meaning.
pub(super) proof fn value_two(words: [u128; 2])
    ensures value(words@) == words@[0] as int + radix() * words@[1] as int,
{
    assert(words@.len() == 2);
    assert(place(0) == 1);
    assert(place(1) == radix());
    assert(limbs(words@, 0) == 0);
    assert(limbs(words@, 1) == words@[0] as int);
    assert(limbs(words@, 2) == words@[0] as int + radix() * words@[1] as int);
}

proof fn value_four(w: [u128; 4])
    ensures value(w@) == w@[0] as int + radix() * (w@[1] as int + radix() *
        (w@[2] as int + radix() * w@[3] as int)),
{
    let r = radix();
    assert(w@.len() == 4);
    assert(place(0) == 1);
    assert(place(1) == r);
    assert(place(2) == r * r);
    assert(place(3) == r * (r * r));
    assert(limbs(w@, 0) == 0);
    assert(limbs(w@, 1) == w@[0] as int);
    assert(limbs(w@, 2) == limbs(w@, 1) + r * w@[1] as int);
    assert(limbs(w@, 3) == limbs(w@, 2) + (r * r) * w@[2] as int);
    assert(limbs(w@, 4) == limbs(w@, 3) + (r * (r * r)) * w@[3] as int);
    let (a, b, c, d) = (w@[0] as int, w@[1] as int, w@[2] as int, w@[3] as int);
    assert(a + r * b + (r * r) * c + (r * (r * r)) * d == a + r * (b + r * (c + r * d)))
        by(nonlinear_arith);
}

proof fn value_six(w: [u128; 6])
    ensures value(w@) == w@[0] as int + radix() * (w@[1] as int + radix() *
        (w@[2] as int + radix() * (w@[3] as int + radix() *
        (w@[4] as int + radix() * w@[5] as int)))),
{
    let r = radix();
    assert(w@.len() == 6);
    assert(place(0) == 1);
    assert(place(1) == r);
    assert(place(2) == r * r);
    assert(place(3) == r * (r * r));
    assert(place(4) == r * (r * (r * r)));
    assert(place(5) == r * (r * (r * (r * r))));
    assert(limbs(w@, 0) == 0);
    assert(limbs(w@, 1) == w@[0] as int);
    assert(limbs(w@, 2) == limbs(w@, 1) + r * w@[1] as int);
    assert(limbs(w@, 3) == limbs(w@, 2) + (r * r) * w@[2] as int);
    assert(limbs(w@, 4) == limbs(w@, 3) + (r * (r * r)) * w@[3] as int);
    assert(limbs(w@, 5) == limbs(w@, 4) + (r * (r * (r * r))) * w@[4] as int);
    assert(limbs(w@, 6) == limbs(w@, 5) + (r * (r * (r * (r * r)))) * w@[5] as int);
    let (a, b, c, d, e, f) = (w@[0] as int, w@[1] as int, w@[2] as int, w@[3] as int,
        w@[4] as int, w@[5] as int);
    assert(a + r * b + (r * r) * c + (r * (r * r)) * d + (r * (r * (r * r))) * e
        + (r * (r * (r * (r * r)))) * f
        == a + r * (b + r * (c + r * (d + r * (e + r * f))))) by(nonlinear_arith);
}

/// The generic limb meaning is each existing fixed-width meaning.
pub(super) proof fn limb_representations()
    ensures
        forall|x: U256| #[trigger] unsigned(x) == value(x.0@),
        forall|x: U512| #[trigger] unsigned512(x) == value(x.0@),
        forall|x: U768| #[trigger] unsigned768(x) == value(x.0@),
{
    conversion_representation();
    assert forall|x: U256| #[trigger] unsigned(x) == value(x.0@) by { value_two(x.0); }
    assert forall|x: U512| #[trigger] unsigned512(x) == value(x.0@) by { value_four(x.0); }
    assert forall|x: U768| #[trigger] unsigned768(x) == value(x.0@) by { value_six(x.0); }
}

/// The 512-bit meaning, with the weight of its four limbs.
pub(super) proof fn wide_representations()
    ensures
        forall|x: U512| #[trigger] unsigned512(x) == value(x.0@),
        place(4) == modulus() * modulus(),
{
    limb_representations();
    place_add(2, 2);
    reveal_with_fuel(place, 3);
    assert(place(2) == modulus());
}

/// A U256 is zero exactly when both limbs are zero.
pub(super) proof fn zero_words()
    ensures forall|x: U256| #[trigger] unsigned(x) == 0 <==> (x.0@[0] == 0 && x.0@[1] == 0),
{
    conversion_representation();
    assert forall|x: U256| #[trigger] unsigned(x) == 0 <==> (x.0@[0] == 0 && x.0@[1] == 0) by {
        unsigned_bounds(x);
        if x.0@[1] != 0 {
            lemma_mul_inequality(1, x.0@[1] as int, radix());
        }
    }
}

/// A value fits two limbs exactly when every higher limb is zero.
pub(super) proof fn narrow_bounds(words: Seq<u128>)
    requires words.len() >= 2,
    ensures
        (forall|k: int| 2 <= k < words.len() ==> words[k] == 0)
            ==> value(words) == words[0] as int + radix() * words[1] as int,
        !(forall|k: int| 2 <= k < words.len() ==> words[k] == 0) ==> value(words) >= modulus(),
{
    value_split(words, 2);
    limbs_bounds(words, 2);
    prefix_two(words, 2);
    upper_nonnegative(words, 2);
    place_positive(2);
    if forall|k: int| 2 <= k < words.len() ==> words[k] == 0 {
        upper_zero(words, 2);
        let u = upper(words, 2);
        assert(u == 0);
        lemma_mul_by_zero_is_zero(place(2));
        assert(place(2) * u == 0);
        assert(prefix2(words, 2) == words[0] as int + radix() * words[1] as int);
    } else {
        let k = choose|k: int| 2 <= k < words.len() && words[k] != 0;
        value_split(words, (k + 1) as nat);
        limbs_bounds(words, k as nat);
        upper_nonnegative(words, (k + 1) as nat);
        place_positive(k as nat);
        place_positive((k + 1) as nat);
        place_monotone(2, k as nat);
        limbs_monotone(words, 0, k as nat);
        assert(limbs(words, (k + 1) as nat) == limbs(words, k as nat) + place(k as nat) * words[k] as int);
        lemma_mul_inequality(1, words[k] as int, place(k as nat));
        lemma_mul_is_commutative(words[k] as int, place(k as nat));
        assert(1 * place(k as nat) == place(k as nat));
        lemma_mul_nonnegative(place((k + 1) as nat), upper(words, (k + 1) as nat));
        assert(value(words) >= place(k as nat));
        if upper(words, 2) == 0 {
            assert(value(words) < place(2));
            assert(false);
        }
        lemma_mul_inequality(1, upper(words, 2), place(2));
    }
}

proof fn upper_zero(words: Seq<u128>, start: nat)
    requires forall|k: int| start <= k < words.len() ==> words[k] == 0,
    ensures upper(words, start) == 0,
    decreases words.len() - start,
{
    if start < words.len() {
        upper_zero(words, start + 1);
    }
}

/// A U768 is zero exactly when all six limbs are zero.
pub(super) proof fn zero_words768()
    ensures forall|x: U768| #[trigger] unsigned768(x) == 0
        <==> (forall|k: int| 0 <= k < 6 ==> x.0@[k] == 0),
{
    limb_representations();
    assert forall|x: U768| #[trigger] unsigned768(x) == 0
        <==> (forall|k: int| 0 <= k < 6 ==> x.0@[k] == 0) by {
        if forall|k: int| 0 <= k < 6 ==> x.0@[k] == 0 {
            zero_value(x.0@, 6);
        } else {
            let k = choose|k: int| 0 <= k < 6 && x.0@[k] != 0;
            nonzero_value(x.0@, k);
        }
    }
}

/// The complete product plus a 256-bit offset always fits 512 bits.
pub(super) proof fn product_offset_bounds(a: U256, b: U256, offset: U256)
    ensures 0 <= unsigned(a) * unsigned(b) + unsigned(offset) < modulus() * modulus(),
{
    unsigned_bounds(a);
    unsigned_bounds(b);
    unsigned_bounds(offset);
    let m = modulus();
    assert(0 <= unsigned(a) * unsigned(b) + unsigned(offset) < m * m) by(nonlinear_arith)
        requires 0 <= unsigned(a) < m, 0 <= unsigned(b) < m, 0 <= unsigned(offset) < m;
}

/// The actual one-bit right shift across both limbs halves the value.
pub(super) proof fn half_value(den: U256, half: U256)
    requires
        half.0@[0] == den.0@[0] >> 1 | (den.0@[1] << 127),
        half.0@[1] == den.0@[1] >> 1,
    ensures unsigned(half) == unsigned(den) / 2,
{
    conversion_representation();
    radix_power();
    let (lo, hi) = (den.0@[0], den.0@[1]);
    assert(((lo >> 1) | (hi << 127)) as int
        == (lo >> 1) as int + 170141183460469231731687303715884105728int * (hi & 1) as int) by(bit_vector);
    assert(lo as int == 2 * (lo >> 1) as int + (lo & 1) as int) by(bit_vector);
    assert(hi as int == 2 * (hi >> 1) as int + (hi & 1) as int) by(bit_vector);
    assert((lo & 1) <= 1) by(bit_vector);
    let low_bit = (lo & 1) as int;
    assert(unsigned(den) == 2 * unsigned(half) + low_bit) by(nonlinear_arith)
        requires
            unsigned(den) == lo as int + radix() * hi as int,
            unsigned(half) == half.0@[0] as int + radix() * half.0@[1] as int,
            half.0@[0] as int == (lo >> 1) as int + 170141183460469231731687303715884105728int * (hi & 1) as int,
            half.0@[1] as int == (hi >> 1) as int,
            lo as int == 2 * (lo >> 1) as int + low_bit,
            hi as int == 2 * (hi >> 1) as int + (hi & 1) as int,
            radix() == 340282366920938463463374607431768211456int;
    unsigned_bounds(half);
    lemma_fundamental_div_mod_converse(unsigned(den), 2, unsigned(half), low_bit);
}

/// The selected two-limb prefix meanings agree with the generic limb meaning.
pub(super) proof fn prefix_two(words: Seq<u128>, count: nat)
    requires count <= 2, count <= words.len(),
    ensures prefix2(words, count as int) == limbs(words, count), place2(count as int) == place(count),
{
    reveal_with_fuel(limbs, 3);
    reveal_with_fuel(place, 3);
    assert(place(0) == 1);
    assert(place(1) == radix());
    assert(place(2) == radix() * radix());
    assert(limbs(words, 0) == 0);
    if count >= 1 { assert(limbs(words, 1) == words[0] as int); }
    if count == 2 { assert(limbs(words, 2) == words[0] as int + radix() * words[1] as int); }
}

// Carry and borrow chains.

/// One addition limb preserves the low-limb carry equation.
pub(super) proof fn carry_step(out: Seq<u128>, a: Seq<u128>, b: Seq<u128>, i: nat, x: u128,
    before: bool, after: bool)
    requires
        i < out.len(), i < a.len(), i < b.len(),
        limbs(out, i) + place(i) * (if before { 1int } else { 0int }) == limbs(a, i) + limbs(b, i),
        x as int + radix() * (if after { 1int } else { 0int })
            == a[i as int] as int + b[i as int] as int + (if before { 1int } else { 0int }),
    ensures
        limbs(out.update(i as int, x), i + 1) + place(i + 1) * (if after { 1int } else { 0int })
            == limbs(a, i + 1) + limbs(b, i + 1),
{
    limbs_push(out, i, x);
    let p = place(i);
    let old = if before { 1int } else { 0int };
    let new = if after { 1int } else { 0int };
    let (wa, wb) = (a[i as int] as int, b[i as int] as int);
    assert(limbs(a, i + 1) == limbs(a, i) + p * wa);
    assert(limbs(b, i + 1) == limbs(b, i) + p * wb);
    assert(place(i + 1) == radix() * p);
    assert(p * (x as int) + (radix() * p) * new == p * wa + p * wb + p * old) by(nonlinear_arith)
        requires x as int + radix() * new == wa + wb + old;
}

/// One subtraction limb preserves the low-limb borrow equation.
pub(super) proof fn borrow_step(out: Seq<u128>, a: Seq<u128>, b: Seq<u128>, i: nat, x: u128,
    before: bool, after: bool)
    requires
        i < out.len(), i < a.len(), i < b.len(),
        limbs(out, i) - place(i) * (if before { 1int } else { 0int }) == limbs(a, i) - limbs(b, i),
        x as int - radix() * (if after { 1int } else { 0int })
            == a[i as int] as int - b[i as int] as int - (if before { 1int } else { 0int }),
    ensures
        limbs(out.update(i as int, x), i + 1) - place(i + 1) * (if after { 1int } else { 0int })
            == limbs(a, i + 1) - limbs(b, i + 1),
{
    limbs_push(out, i, x);
    let p = place(i);
    let old = if before { 1int } else { 0int };
    let new = if after { 1int } else { 0int };
    let (wa, wb) = (a[i as int] as int, b[i as int] as int);
    assert(limbs(a, i + 1) == limbs(a, i) + p * wa);
    assert(limbs(b, i + 1) == limbs(b, i) + p * wb);
    assert(place(i + 1) == radix() * p);
    assert(p * (x as int) - (radix() * p) * new == p * wa - p * wb - p * old) by(nonlinear_arith)
        requires x as int - radix() * new == wa - wb - old;
}

/// One carried limb addition preserves the whole-value carry equation.
pub(super) proof fn carry_word(out: Seq<u128>, i: nat, carry: u128, next: u128, overflow: bool)
    requires i < out.len(),
        next as int == if out[i as int] as int + carry as int > u128::MAX {
            out[i as int] as int + carry as int - radix() } else { out[i as int] as int + carry as int },
        overflow == (out[i as int] as int + carry as int > u128::MAX),
    ensures
        value(out.update(i as int, next)) + place(i + 1) * (if overflow { 1int } else { 0int })
            == value(out) + place(i) * carry as int,
{
    value_update(out, i as int, next);
    let p = place(i);
    let o = if overflow { 1int } else { 0int };
    assert(place(i + 1) == radix() * p);
    assert(next as int + radix() * o == out[i as int] as int + carry as int);
    assert(p * (next as int - out[i as int] as int) + (radix() * p) * o == p * carry as int)
        by(nonlinear_arith)
        requires next as int + radix() * o == out[i as int] as int + carry as int;
}

// The shared comparator.

/// The actual high-limb-first comparator orders the complete meanings.
pub(super) proof fn compare_order(a: Seq<u128>, b: Seq<u128>)
    ensures forall|result: Ordering| #[trigger] lex_result(a, b, result)
        ==> ((result == Ordering::Less) == (value(a) < value(b))),
{
    assert forall|result: Ordering| #[trigger] lex_result(a, b, result)
        implies ((result == Ordering::Less) == (value(a) < value(b))) by {
        if result == Ordering::Less {
            let j = choose|j: int| 0 <= j < a.len() && a[j] < b[j] && suffix_equal(a, b, j + 1);
            lex_value(a, b, j);
        } else if result == Ordering::Greater {
            let j = choose|j: int| 0 <= j < a.len() && a[j] > b[j] && suffix_equal(a, b, j + 1);
            lex_value(b, a, j);
        } else {
            assert(a == b);
        }
    }
}

proof fn lex_value(a: Seq<u128>, b: Seq<u128>, j: int)
    requires a.len() == b.len(), 0 <= j < a.len(), a[j] < b[j], suffix_equal(a, b, j + 1),
    ensures value(a) < value(b),
{
    let k = (j + 1) as nat;
    value_split(a, k);
    value_split(b, k);
    upper_equal(a, b, k);
    limbs_bounds(a, j as nat);
    limbs_bounds(b, j as nat);
    let p = place(j as nat);
    place_positive(j as nat);
    let (low_a, low_b) = (limbs(a, j as nat), limbs(b, j as nat));
    let (x, y) = (a[j] as int, b[j] as int);
    assert(limbs(a, k) == low_a + p * x);
    assert(limbs(b, k) == low_b + p * y);
    assert(low_a + p * x < low_b + p * y) by(nonlinear_arith)
        requires 0 <= low_a < p, 0 <= low_b, x < y, p > 0;
    let high = place(k) * upper(a, k);
    assert(place(k) * upper(b, k) == high);
    assert(value(a) == limbs(a, k) + high);
    assert(value(b) == limbs(b, k) + high);
}

// Bits of the limb meaning.

/// A shifted limb plus its outgoing high bit is exactly twice the limb plus the carry.
pub(super) proof fn shift_word(x: u128, carry: u128)
    requires carry <= 1,
    ensures
        ((x << 1) | carry) as int + radix() * (x >> 127) as int == 2 * x as int + carry as int,
        x >> 127 <= 1,
{
    radix_power();
    assert(((x << 1) | carry) as int
        + 340282366920938463463374607431768211456int * ((x >> 127) as int)
        == 2 * (x as int) + (carry as int)) by(bit_vector)
        requires carry <= 1;
    assert(x >> 127 <= 1) by(bit_vector);
}

pub(super) proof fn one_bit(x: u128, shift: usize)
    ensures (x >> shift) & 1 <= 1,
{
    assert((x >> shift) & 1 <= 1) by(bit_vector);
}

pub(super) proof fn bit_power(bit: usize)
    requires bit < 128,
    ensures (1u128 << bit) as int == pow2(bit as nat) as int,
    decreases bit,
{
    if bit == 0 {
        assert(1u128 << 0usize == 1u128) by(bit_vector);
        lemma2_to64();
    } else {
        bit_power((bit - 1) as usize);
        lemma_pow2_unfold(bit as nat);
        assert((1u128 << bit) as int == 2 * ((1u128 << ((bit - 1) as usize)) as int))
            by(bit_vector) requires 0 < bit < 128;
    }
}

/// The value without its lowest `bit` bits, and the actual limb bit selected by `bit`.
pub(super) proof fn bit_step(words: Seq<u128>, bit: usize)
    requires bit < 128 * words.len(),
    ensures
        top(words, bit as nat) == 2 * top(words, bit as nat + 1)
            + ((words[(bit / 128) as int] >> (bit % 128)) & 1) as int,
        top(words, bit as nat) % 2 == ((words[(bit / 128) as int] >> (bit % 128)) & 1) as int,
{
    let j = (bit / 128) as nat;
    let s = (bit % 128) as nat;
    let x = words[j as int];
    let n = value(words);
    let ps = pow2(s) as int;
    value_bounds(words);
    place_power(j);
    lemma_pow2_adds(128 * j, s);
    assert(128 * j + s == bit as nat);
    lemma_pow2_pos(s);
    place_positive(j);
    // top(bit) = (n / place(j)) / 2^s = upper(j) / 2^s.
    value_div_place(words, j);
    lemma_div_denominator(n, place(j), ps);
    // upper(j) = x + 2^s * (2^(128-s) * upper(j+1)), with 2^(128-s) even.
    let u = upper(words, j + 1);
    upper_nonnegative(words, j + 1);
    radix_power();
    lemma_pow2_adds(s, (128 - s) as nat);
    lemma_pow2_unfold((128 - s) as nat);
    let half = pow2((127 - s) as nat) as int;
    assert(upper(words, j) == x as int + radix() * u);
    assert(radix() * u == (2 * half * u) * ps) by(nonlinear_arith)
        requires radix() == ps * pow2((128 - s) as nat) as int,
            pow2((128 - s) as nat) as int == 2 * half;
    lemma_hoist_over_denominator(x as int, 2 * half * u, ps as nat);
    assert(top(words, bit as nat) == x as int / ps + 2 * (half * u)) by {
        lemma_mul_is_associative(2, half, u);
    }
    lemma_mod_multiples_vanish(half * u, x as int / ps, 2);
    vstd::bits::lemma_u128_shr_is_div(x, s as u128);
    assert(x >> (bit % 128) == x >> ((bit % 128) as u128)) by(bit_vector);
    assert(((x >> (bit % 128)) & 1) as int == ((x >> (bit % 128)) as int) % 2) by(bit_vector);
    // top(bit + 1) = top(bit) / 2.
    lemma_pow2_pos(bit as nat);
    lemma_pow2_unfold(bit as nat + 1);
    lemma_div_denominator(n, pow2(bit as nat) as int, 2);
    lemma_mul_is_commutative(2, pow2(bit as nat) as int);
    lemma_fundamental_div_mod(top(words, bit as nat), 2);
}

/// Setting a bit below which the value is divisible by 2^(bit+1) adds exactly 2^bit.
pub(super) proof fn set_bit(words: Seq<u128>, bit: usize)
    requires bit < 128 * words.len(), value(words) % pow2(bit as nat + 1) as int == 0,
    ensures
        value(words.update((bit / 128) as int, words[(bit / 128) as int] | (1u128 << (bit % 128))))
            == value(words) + pow2(bit as nat),
        value(words) % pow2(bit as nat) as int == 0,
{
    let j = (bit / 128) as int;
    let x = words[j];
    let p = pow2(bit as nat) as int;
    value_bounds(words);
    lemma_pow2_pos(bit as nat);
    lemma_pow2_unfold(bit as nat + 1);
    // Divisibility by 2p gives an even top and divisibility by p.
    lemma_fundamental_div_mod(value(words), 2 * p);
    let m = value(words) / (2 * p);
    assert(value(words) == (2 * m) * p) by(nonlinear_arith)
        requires value(words) == (2 * p) * m + 0;
    lemma_fundamental_div_mod_converse(value(words), p, 2 * m, 0);
    lemma_mod_multiples_vanish(m, 0, 2);
    bit_step(words, bit);
    assert(((x >> (bit % 128)) & 1) == 0);
    assert((x | (1u128 << (bit % 128))) as int == x as int + (1u128 << (bit % 128)) as int)
        by(bit_vector) requires bit % 128 < 128, ((x >> (bit % 128)) & 1) == 0;
    bit_power(bit % 128);
    value_update(words, j, x | (1u128 << (bit % 128)));
    place_power(j as nat);
    lemma_pow2_adds(128 * (j as nat), (bit % 128) as nat);
    assert(128 * (j as nat) + (bit % 128) as nat == bit as nat);
}

/// No value of N limbs has a bit at position 128*N.
pub(super) proof fn top_empty(words: Seq<u128>)
    ensures top(words, 128 * words.len()) == 0, top(words, 0) == value(words),
{
    value_bounds(words);
    place_power(words.len());
    lemma_pow2_pos(128 * words.len());
    lemma_basic_div(value(words), pow2(128 * words.len()) as int);
    lemma2_to64();
    lemma_div_basics(value(words));
}

// Restoring division.

/// One limb of the left shift: the low meaning doubles and absorbs the incoming bit.
pub(super) proof fn shift_step(r: Seq<u128>, before: Seq<u128>, i: nat, carry: u128, incoming: u128)
    requires
        i < r.len(), r.len() == before.len(), r[i as int] == before[i as int], carry <= 1,
        limbs(r, i) + place(i) * carry as int == 2 * limbs(before, i) + incoming as int,
    ensures
        limbs(r.update(i as int, (r[i as int] << 1) | carry), i + 1)
            + place(i + 1) * (r[i as int] >> 127) as int == 2 * limbs(before, i + 1) + incoming as int,
        r[i as int] >> 127 <= 1,
{
    let x = r[i as int];
    limbs_push(r, i, (x << 1) | carry);
    shift_word(x, carry);
    let p = place(i);
    let (low, high) = (((x << 1) | carry) as int, (x >> 127) as int);
    assert(limbs(before, i + 1) == limbs(before, i) + p * x as int);
    assert(place(i + 1) == radix() * p);
    assert(p * low + (radix() * p) * high == 2 * (p * x as int) + p * carry as int) by(nonlinear_arith)
        requires low + radix() * high == 2 * x as int + carry as int;
}

/// One restoring-division step from the remainder of the higher bits.
pub(super) proof fn quotient_step(t: int, bit: int, d: int, shifted: int)
    requires 0 <= t, 0 <= bit <= 1, d > 0, shifted == 2 * (t % d) + bit,
    ensures
        0 <= shifted < 2 * d,
        shifted >= d ==> (2 * t + bit) / d == 2 * (t / d) + 1 && (2 * t + bit) % d == shifted - d,
        shifted < d ==> (2 * t + bit) / d == 2 * (t / d) && (2 * t + bit) % d == shifted,
{
    lemma_fundamental_div_mod(t, d);
    lemma_mod_bound(t, d);
    let q = t / d;
    if shifted >= d {
        assert(2 * t + bit == (2 * q + 1) * d + (shifted - d)) by(nonlinear_arith)
            requires t == d * q + t % d, shifted == 2 * (t % d) + bit;
        lemma_fundamental_div_mod_converse(2 * t + bit, d, 2 * q + 1, shifted - d);
    } else {
        assert(2 * t + bit == (2 * q) * d + shifted) by(nonlinear_arith)
            requires t == d * q + t % d, shifted == 2 * (t % d) + bit;
        lemma_fundamental_div_mod_converse(2 * t + bit, d, 2 * q, shifted);
    }
}

/// The quotient bits chosen above `bit` occupy only multiples of 2^(bit+1).
pub(super) proof fn quotient_bits(q: Seq<u128>, bit: usize, chosen: int)
    requires bit < 128 * q.len(), chosen >= 0, value(q) == chosen * pow2(bit as nat + 1) as int,
    ensures
        value(q.update((bit / 128) as int, q[(bit / 128) as int] | (1u128 << (bit % 128))))
            == (2 * chosen + 1) * pow2(bit as nat) as int,
        value(q) == (2 * chosen) * pow2(bit as nat) as int,
{
    lemma_pow2_pos(bit as nat + 1);
    lemma_pow2_unfold(bit as nat + 1);
    lemma_mod_multiples_basic(chosen, pow2(bit as nat + 1) as int);
    set_bit(q, bit);
    let p = pow2(bit as nat) as int;
    assert(chosen * (2 * p) == (2 * chosen) * p) by(nonlinear_arith);
    assert((2 * chosen) * p + p == (2 * chosen + 1) * p) by(nonlinear_arith);
}

/// The final exact witness from the loop's floor quotient and remainder.
pub(super) proof fn divide_finish(n: Seq<u128>, d: Seq<u128>, q: Seq<u128>, r: Seq<u128>)
    requires value(d) > 0, value(r) == top(n, 0) % value(d),
        value(q) == top(n, 0) / value(d) * pow2(0) as int,
    ensures quotient_witness(value(n), value(d), value(q), value(r)),
{
    top_empty(n);
    lemma2_to64();
    value_bounds(n);
    lemma_fundamental_div_mod(value(n), value(d));
    lemma_mod_bound(value(n), value(d));
    lemma_div_pos_is_pos(value(n), value(d));
    lemma_mul_is_commutative(value(d), value(n) / value(d));
}

/// Every canonical witness is the floor quotient and its remainder.
pub(super) proof fn canonical_quotients()
    ensures forall|n: int, d: int, q: int, r: int| #[trigger] quotient_witness(n, d, q, r)
        ==> q == n / d && r == n % d,
{
    assert forall|n: int, d: int, q: int, r: int| #[trigger] quotient_witness(n, d, q, r)
        implies q == n / d && r == n % d by {
        canonical_quotient(n, d, q, r);
    }
}

/// The canonical witness is the floor quotient and its remainder.
pub(super) proof fn canonical_quotient(n: int, d: int, q: int, r: int)
    requires quotient_witness(n, d, q, r),
    ensures q == n / d, r == n % d,
{
    lemma_mul_is_commutative(q, d);
    lemma_fundamental_div_mod_converse(n, d, q, r);
}

// Schoolbook multiplication.

/// One partial product: both word additions stay below the output weight and
/// add exactly the weighted product.
pub(super) proof fn product_step(a: Seq<u128>, b: Seq<u128>, i: nat, j: nat, current: int,
    low: u128, high: u128, size: nat)
    requires i < a.len(), j < b.len(), a.len() + b.len() <= size,
        current == limbs(a, i) * value(b) + place(i) * a[i as int] as int * limbs(b, j),
        high as int * super::wide::scale() + low as int == a[i as int] as int * b[j as int] as int,
    ensures
        current + place(i + j) * (low as int) < place(size),
        current + place(i + j) * low as int + place(i + j + 1) * high as int
            == limbs(a, i) * value(b) + place(i) * a[i as int] as int * limbs(b, j + 1),
        limbs(a, i) * value(b) + place(i) * a[i as int] as int * limbs(b, j + 1) < place(size),
{
    let (x, y) = (a[i as int] as int, b[j as int] as int);
    let (pi, pj, pij) = (place(i), place(j), place(i + j));
    place_add(i, j);
    place_positive(i);
    place_positive(j);
    place_positive(i + j);
    assert(place(i + j + 1) == radix() * pij);
    assert(limbs(b, j + 1) == limbs(b, j) + pj * y);
    assert(limbs(a, i + 1) == limbs(a, i) + pi * x);
    // The step adds exactly place(i)*x*place(j)*y.
    assert(pij * low as int + (radix() * pij) * high as int == pij * (x * y)) by(nonlinear_arith)
        requires high as int * radix() + low as int == x * y;
    assert(pi * x * (limbs(b, j) + pj * y) == pi * x * limbs(b, j) + pij * (x * y)) by(nonlinear_arith)
        requires pij == pi * pj;
    // Upper bound: the next partial sum is at most value(a) * value(b) < place(size).
    limbs_bounds(b, j + 1);
    limbs_bounds(a, i);
    limbs_bounds(a, i + 1);
    value_bounds(a);
    value_bounds(b);
    limbs_monotone(b, j + 1, b.len());
    limbs_monotone(a, i + 1, a.len());
    let (vb, va) = (value(b), value(a));
    assert(limbs(a, i) * vb + pi * x * limbs(b, j + 1) <= va * vb) by(nonlinear_arith)
        requires 0 <= limbs(b, j + 1) <= vb, pi > 0, x >= 0, limbs(a, i) >= 0,
            limbs(a, i) + pi * x <= va;
    place_add(a.len(), b.len());
    place_monotone(a.len() + b.len(), size);
    assert(va * vb < place(a.len()) * place(b.len())) by(nonlinear_arith)
        requires 0 <= va < place(a.len()), 0 <= vb < place(b.len());
    assert(pij * low as int <= pij * low as int + (radix() * pij) * high as int) by(nonlinear_arith)
        requires pij > 0, high >= 0, radix() > 0;
}

/// A completed row adds its limb times the complete second operand.
pub(super) proof fn product_row(a: Seq<u128>, b: Seq<u128>, i: nat)
    requires i < a.len(),
    ensures limbs(a, i) * value(b) + place(i) * a[i as int] as int * value(b)
        == limbs(a, i + 1) * value(b),
{
    assert(limbs(a, i + 1) == limbs(a, i) + place(i) * a[i as int] as int);
    lemma_mul_is_distributive_add_other_way(value(b), limbs(a, i), place(i) * a[i as int] as int);
}

// Signed multiplication and division by magnitudes.

/// The absolute value of an integer.
pub open spec fn magnitude_of(x: int) -> int {
    if x < 0 { -x } else { x }
}
/// floor(n/d) for nonzero d; integer division by a positive divisor rounds toward
/// negative infinity, and n/d = (-n)/(-d).
pub open spec fn floor_div(n: int, d: int) -> int {
    if d > 0 { n / d } else { (-n) / (-d) }
}
/// ceil(n/d) for nonzero d.
pub open spec fn ceil_div(n: int, d: int) -> int {
    -floor_div(-n, d)
}
/// The signed integer selected by a rounding mode for the exact ratio n/d.
pub open spec fn signed_rounded_value(n: int, d: int, mode: super::Rounding) -> int {
    match mode {
        super::Rounding::Ceil => ceil_div(n, d),
        _ => floor_div(n, d),
    }
}
/// The unsigned rounding of |n|/|d| that realizes a signed rounding of n/d.
pub open spec fn magnitude_mode(mode: super::Rounding, negative: bool) -> super::Rounding {
    if mode == super::Rounding::Exact {
        super::Rounding::Exact
    } else if (mode == super::Rounding::Floor) == negative {
        super::Rounding::Ceil
    } else {
        super::Rounding::Floor
    }
}
/// I256::checked_div: ZeroDivisor first, then Inexact for Exact with a remainder,
/// then the rounded quotient with the exact signed Overflow partition.
pub open spec fn signed_div_result(n: int, d: int, mode: super::Rounding,
    result: Result<I256, Failure>) -> bool {
    if d == 0 {
        result == Err::<I256, Failure>(Failure::ZeroDivisor)
    } else if mode == super::Rounding::Exact && floor_div(n, d) * d != n {
        result == Err::<I256, Failure>(Failure::Inexact)
    } else {
        signed_result(result, signed_rounded_value(n, d, mode))
    }
}

/// Signed complete-product division by a nonnegative denominator: ZeroDivisor,
/// then Overflow when the magnitude quotient needs more than 256 bits, then
/// Inexact for Exact with a remainder, then the rounded quotient with the exact
/// signed Overflow partition.
pub open spec fn signed_mul_div_result(p: int, d: int, mode: super::Rounding,
    result: Result<I256, Failure>) -> bool {
    if d == 0 {
        result == Err::<I256, Failure>(Failure::ZeroDivisor)
    } else if magnitude_of(p) / d >= modulus() {
        result == Err::<I256, Failure>(Failure::Overflow)
    } else if mode == super::Rounding::Exact && floor_div(p, d) * d != p {
        result == Err::<I256, Failure>(Failure::Inexact)
    } else {
        signed_result(result, signed_rounded_value(p, d, mode))
    }
}
/// I256::mul_div_positive: Domain unless the denominator is positive, then the
/// signed complete-product contract for a*b/den.
pub open spec fn mul_div_positive_result(a: I256, b: I256, den: I256, mode: super::Rounding,
    result: Result<I256, Failure>) -> bool {
    if signed(den) <= 0 {
        result == Err::<I256, Failure>(Failure::Domain)
    } else {
        signed_mul_div_result(signed(a) * signed(b), signed(den), mode, result)
    }
}

/// Whether any of six signed factors is negative.
pub open spec fn any_negative(a: [I256; 3], b: [I256; 3]) -> bool {
    exists|k: int| 0 <= k < 3 && (signed(a@[k]) < 0 || signed(b@[k]) < 0)
}
/// The product of three signed factors.
pub open spec fn signed_triple(x: [I256; 3]) -> int {
    signed(x@[0]) * signed(x@[1]) * signed(x@[2])
}
/// I256::try_i128: the exact value iff it fits i128, otherwise exactly Overflow.
pub open spec fn i128_result(result: Result<i128, Failure>, value: int) -> bool {
    match result {
        Ok(actual) => i128::MIN <= value <= i128::MAX && actual as int == value,
        Err(error) => error == Failure::Overflow && !(i128::MIN <= value <= i128::MAX),
    }
}

/// Integer division of a negated nonnegative value by a positive divisor.
pub(super) proof fn negated_division(m: int, e: int)
    requires m >= 0, e > 0,
    ensures (-m) / e == -(m / e) - (if m % e == 0 { 0int } else { 1int }),
{
    lemma_fundamental_div_mod(m, e);
    lemma_mod_bound(m, e);
    let (q, r) = (m / e, m % e);
    if r == 0 {
        assert(-m == (-q) * e + 0) by(nonlinear_arith) requires m == e * q + r, r == 0;
        lemma_fundamental_div_mod_converse(-m, e, -q, 0);
    } else {
        assert(-m == (-q - 1) * e + (e - r)) by(nonlinear_arith) requires m == e * q + r;
        lemma_fundamental_div_mod_converse(-m, e, -q - 1, e - r);
    }
}

/// Signed rounding of n/d is the signed magnitude rounding of |n|/|d|.
pub(super) proof fn signed_rounding(n: int, d: int, mode: super::Rounding, negative: bool)
    requires d != 0, n == 0 || negative == ((n < 0) != (d < 0)),
    ensures
        (floor_div(n, d) * d == n) == (magnitude_of(n) % magnitude_of(d) == 0),
        mode != super::Rounding::Exact || magnitude_of(n) % magnitude_of(d) == 0 ==>
            signed_rounded_value(n, d, mode) == {
                let v = rounded_value(magnitude_of(n) / magnitude_of(d), magnitude_of(n) % magnitude_of(d),
                    magnitude_mode(mode, negative));
                if negative { -v } else { v }
            },
{
    let (m, e) = (magnitude_of(n), magnitude_of(d));
    lemma_fundamental_div_mod(m, e);
    lemma_mod_bound(m, e);
    let (q, r) = (m / e, m % e);
    let up = if r == 0 { 0int } else { 1int };
    negated_division(m, e);
    if n == 0 {
        lemma_small_mod(0, e as nat);
        lemma_basic_div(0, e);
        assert(q == 0 && r == 0 && up == 0);
        assert(floor_div(n, d) == 0);
        assert(floor_div(-n, d) == 0);
    } else if n > 0 && d > 0 {
        assert(floor_div(n, d) == q);
        assert(floor_div(-n, d) == -q - up);
        assert((q * d == n) == (r == 0)) by(nonlinear_arith)
            requires n == d * q + r, 0 <= r < d;
    } else if n < 0 && d > 0 {
        assert(floor_div(n, d) == -q - up);
        assert(floor_div(-n, d) == q);
        assert(((-q - up) * d == n) == (r == 0)) by(nonlinear_arith)
            requires -n == d * q + r, 0 <= r < d, up == if r == 0 { 0int } else { 1int };
    } else if n > 0 && d < 0 {
        assert(floor_div(n, d) == -q - up);
        assert(floor_div(-n, d) == q);
        assert(((-q - up) * d == n) == (r == 0)) by(nonlinear_arith)
            requires n == (-d) * q + r, 0 <= r < -d, up == if r == 0 { 0int } else { 1int };
    } else {
        assert(floor_div(n, d) == q);
        assert(floor_div(-n, d) == -q - up);
        assert((q * d == n) == (r == 0)) by(nonlinear_arith)
            requires -n == (-d) * q + r, 0 <= r < -d;
    }
    // In every case the two floor quotients are the signed magnitude quotients.
    assert(negative ==> floor_div(n, d) == -q - up && floor_div(-n, d) == q);
    assert(!negative ==> floor_div(n, d) == q && floor_div(-n, d) == -q - up);
    assert(ceil_div(n, d) == -floor_div(-n, d));
    if mode != super::Rounding::Exact || r == 0 {
        let mm = magnitude_mode(mode, negative);
        assert(rounded_value(q, r, super::Rounding::Ceil) == q + up);
        assert(rounded_value(q, r, super::Rounding::Floor) == q);
        assert(rounded_value(q, r, super::Rounding::Exact) == q);
        if mode == super::Rounding::Floor {
            assert(signed_rounded_value(n, d, mode) == floor_div(n, d));
            assert(mm == if negative { super::Rounding::Ceil } else { super::Rounding::Floor });
        } else if mode == super::Rounding::Ceil {
            assert(signed_rounded_value(n, d, mode) == -floor_div(-n, d));
            assert(mm == if negative { super::Rounding::Floor } else { super::Rounding::Ceil });
        } else {
            assert(r == 0 && up == 0);
            assert(signed_rounded_value(n, d, mode) == floor_div(n, d));
            assert(mm == super::Rounding::Exact);
        }
    }
}

/// The magnitude and sign of a product.
pub(super) proof fn signed_product(x: int, y: int)
    ensures
        magnitude_of(x * y) == magnitude_of(x) * magnitude_of(y),
        x * y != 0 ==> ((x * y < 0) == ((x < 0) != (y < 0))),
        (x * y == 0) == (x == 0 || y == 0),
{
    let (a, b) = (magnitude_of(x), magnitude_of(y));
    if x < 0 && y < 0 {
        assert(x * y == a * b) by(nonlinear_arith) requires a == -x, b == -y;
        lemma_mul_strictly_positive(a, b);
    } else if x < 0 && y > 0 {
        assert(x * y == -(a * b)) by(nonlinear_arith) requires a == -x, b == y;
        lemma_mul_strictly_positive(a, b);
    } else if x > 0 && y < 0 {
        assert(x * y == -(a * b)) by(nonlinear_arith) requires a == x, b == -y;
        lemma_mul_strictly_positive(a, b);
    } else if x > 0 && y > 0 {
        lemma_mul_strictly_positive(x, y);
    } else {
        assert(x * y == 0 && a * b == 0) by(nonlinear_arith)
            requires x == 0 || y == 0, a == 0 || b == 0;
    }
}

/// The magnitude computation used for x*k*y/d with k, d >= 0 and the sign of x and y.
pub(super) proof fn signed_magnitude_division(x: int, y: int, k: int, d: int, mode: super::Rounding,
    negative: bool)
    requires k >= 0, d >= 0, negative == ((x < 0) != (y < 0)),
    ensures
        magnitude_of(x * k * y) == magnitude_of(x) * k * magnitude_of(y),
        magnitude_of(x) * k * magnitude_of(y) >= 0,
        d > 0 ==> (floor_div(x * k * y, d) * d == x * k * y)
            == ((magnitude_of(x) * k * magnitude_of(y)) % d == 0),
        d > 0 && (mode != super::Rounding::Exact || (magnitude_of(x) * k * magnitude_of(y)) % d == 0) ==>
            signed_rounded_value(x * k * y, d, mode) == {
                let p = magnitude_of(x) * k * magnitude_of(y);
                let v = rounded_value(p / d, p % d, magnitude_mode(mode, negative));
                if negative { -v } else { v }
            },
        d > 0 && (mode != super::Rounding::Exact || (magnitude_of(x) * k * magnitude_of(y)) % d == 0)
            && rounded_value((magnitude_of(x) * k * magnitude_of(y)) / d,
                (magnitude_of(x) * k * magnitude_of(y)) % d, magnitude_mode(mode, negative)) >= modulus()
            ==> !(-half() <= signed_rounded_value(x * k * y, d, mode) < half()),
{
    signed_product(x, k);
    signed_product(x * k, y);
    assert(magnitude_of(k) == k);
    lemma_mul_nonnegative(magnitude_of(x), k);
    lemma_mul_nonnegative(magnitude_of(x) * k, magnitude_of(y));
    let p = x * k * y;
    if d > 0 {
        // The product's sign is the factors' sign whenever it is nonzero.
        if p != 0 {
            assert(x * k != 0 && y != 0);
            assert((x * k < 0) == (x < 0)) by {
                signed_product(x, k);
                assert(k != 0);
            }
        }
        signed_rounding(p, d, mode, negative);
        assert(magnitude_of(d) == d);
        let v = rounded_value((magnitude_of(x) * k * magnitude_of(y)) / d,
            (magnitude_of(x) * k * magnitude_of(y)) % d, magnitude_mode(mode, negative));
        if (mode != super::Rounding::Exact || (magnitude_of(x) * k * magnitude_of(y)) % d == 0)
            && v >= modulus() {
            outside_signed(signed_rounded_value(p, d, mode));
        }
    }
}

/// Six nonnegative signed factors have the unsigned meanings of their bits.
pub(super) proof fn nonnegative_triples(a: [I256; 3], b: [I256; 3])
    requires
        forall|k: int| 0 <= k < 3 ==> signed(a@[k]) >= 0,
        forall|k: int| 0 <= k < 3 ==> signed(b@[k]) >= 0,
    ensures
        !any_negative(a, b),
        signed_triple(a) == unsigned(a@[0].0) * unsigned(a@[1].0) * unsigned(a@[2].0),
        signed_triple(b) == unsigned(b@[0].0) * unsigned(b@[1].0) * unsigned(b@[2].0),
{
    conversion_representation();
    assert forall|x: I256| signed(x) >= 0 implies #[trigger] signed(x) == unsigned(x.0) by {
        signed_bounds(x);
        unsigned_bounds(x.0);
    }
    assert(signed(a@[0]) >= 0 && signed(a@[1]) >= 0 && signed(a@[2]) >= 0);
    assert(signed(b@[0]) >= 0 && signed(b@[1]) >= 0 && signed(b@[2]) >= 0);
}

/// The two-complement sign extension test used by try_i128.
pub(super) proof fn sign_extension(x: I256)
    ensures
        ((x.0.0@[0] as i128) as int)
            == if x.0.0@[0] < 170141183460469231731687303715884105728u128 { x.0.0@[0] as int }
               else { x.0.0@[0] as int - 340282366920938463463374607431768211456int },
        ((x.0.0@[0] as i128) < 0) == (x.0.0@[0] >= 170141183460469231731687303715884105728u128),
        (x.0.0@[1] == if (x.0.0@[0] as i128) < 0 { u128::MAX } else { 0u128 })
            == (i128::MIN <= signed(x) <= i128::MAX),
        i128::MIN <= signed(x) <= i128::MAX ==> signed(x) == ((x.0.0@[0] as i128) as int),
{
    conversion_representation();
    radix_power();
    let (lo, hi) = (x.0.0@[0], x.0.0@[1]);
    assert(((lo as i128) as int) == if lo < 170141183460469231731687303715884105728u128 { lo as int }
        else { lo as int - 340282366920938463463374607431768211456int }) by(bit_vector);
    let r = radix();
    let u = lo as int + r * hi as int;
    assert(unsigned(x.0) == u);
    assert(modulus() == r * r);
    assert(half() == r * 170141183460469231731687303715884105728int);
    if hi == 0 {
        assert(u == lo as int);
    } else if hi == u128::MAX {
        assert(u == lo as int + r * r - r) by(nonlinear_arith)
            requires u == lo as int + r * hi as int, hi as int == r - 1;
    } else if hi < 170141183460469231731687303715884105728u128 {
        assert(u >= r) by(nonlinear_arith) requires u == lo as int + r * hi as int, hi >= 1, r > 0, lo >= 0;
        assert(u < half()) by(nonlinear_arith)
            requires u == lo as int + r * hi as int, (hi as int) <= 170141183460469231731687303715884105727int,
                (lo as int) < r, r == 340282366920938463463374607431768211456int,
                half() == r * 170141183460469231731687303715884105728int;
    } else {
        assert(u >= half()) by(nonlinear_arith)
            requires u == lo as int + r * hi as int, hi as int >= 170141183460469231731687303715884105728int,
                lo >= 0, r > 0, half() == r * 170141183460469231731687303715884105728int;
        assert(u - modulus() <= -r - 1) by(nonlinear_arith)
            requires u == lo as int + r * hi as int, (hi as int) <= r - 2, (lo as int) <= r - 1,
                modulus() == r * r;
    }
}

/// A magnitude at least 2^256 is outside the signed range.
pub(super) proof fn outside_signed(value: int)
    requires magnitude_of(value) >= modulus(),
    ensures !(-half() <= value < half()),
{
    radix_power();
    assert(half() == modulus() / 2);
}
}
