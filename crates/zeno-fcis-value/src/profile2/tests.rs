use super::*;

fn hex<const N: usize>(x: [u128; N]) -> String {
    x.iter()
        .rev()
        .map(|x| format!("{x:032x}"))
        .collect::<Vec<_>>()
        .join("")
}
fn parse<const N: usize>(s: &str) -> [u128; N] {
    assert_eq!(s.len(), N * 32);
    let mut words = [0; N];
    for (i, word) in words.iter_mut().enumerate() {
        let end = s.len() - i * 32;
        *word = u128::from_str_radix(&s[end - 32..end], 16)
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    }
    words
}
fn mode(s: &str) -> Rounding {
    match s {
        "F" => Rounding::Floor,
        "C" => Rounding::Ceil,
        "E" => Rounding::Exact,
        _ => panic!("mode"),
    }
}
fn u(s: &str) -> U256 {
    U256::from_words_le(parse(s))
}

#[test]
fn independent_full_width_vectors() {
    let mut count = 0;
    for line in include_str!("vectors_03.tsv").lines() {
        let c: Vec<_> = line.split('\t').collect();
        assert_eq!(c.len(), 9);
        let result = if c[0] == "sqrt" {
            U512::from_words_le(parse(c[2]))
                .sqrt(mode(c[1]))
                .map(|x| hex(x.words_le()))
        } else if c[0] == "ratio" {
            U512::from_words_le(parse(c[2]))
                .sqrt_ratio(U512::from_words_le(parse(c[3])), mode(c[1]))
                .map(|x| hex(x.words_le()))
        } else {
            let a = u(c[2]);
            let b = u(c[3]);
            let d = u(c[4]);
            let x = I256::from_bits(a);
            let y = I256::from_bits(b);
            match c[0] {
                "full" => a.full_product(b).map(|x| hex(x.words_le())),
                "add" => a.checked_add(b).map(|x| hex(x.words_le())),
                "sub" => a.checked_sub(b).map(|x| hex(x.words_le())),
                "mul" => a.checked_mul(b).map(|x| hex(x.words_le())),
                "div" => a.checked_div(b, mode(c[1])).map(|x| hex(x.words_le())),
                "md" => a.mul_div(b, d, mode(c[1])).map(|x| hex(x.words_le())),
                "half" => a.mul_div_half_up(b, d).map(|x| hex(x.words_le())),
                "iadd" => x.checked_add(y).map(|x| hex(x.bits().words_le())),
                "isub" => x.checked_sub(y).map(|x| hex(x.bits().words_le())),
                "imul" => x.checked_mul(y).map(|x| hex(x.bits().words_le())),
                "idiff" => I256::difference_unsigned(a, b).map(|x| hex(x.bits().words_le())),
                "idiv" => x
                    .checked_div(y, mode(c[1]))
                    .map(|x| hex(x.bits().words_le())),
                "smd" => x
                    .mul_div_positive(y, I256::from_bits(d), Rounding::Floor)
                    .map(|x| hex(x.bits().words_le())),
                "md3" => a
                    .mul3_div(b, d, u(c[5]), mode(c[1]))
                    .map(|x| hex(x.words_le())),
                "smd3" => x
                    .mul_unsigned_signed_div(b, I256::from_bits(d), u(c[5]), mode(c[1]))
                    .map(|x| hex(x.bits().words_le())),
                "triple" => U256::products_ge([a, b, d], [u(c[5]), u(c[6]), u(c[7])])
                    .map(|x| format!("bool:{}", u8::from(x))),
                _ => panic!("unknown vector opcode"),
            }
        };
        let actual = result.unwrap_or_else(|e| format!("err:{e:?}"));
        assert_eq!(actual, c[8], "vector {count}: {line}");
        count += 1;
    }
    assert!(count > 5000, "independent corpus must be present");
}

#[test]
fn actual_legacy_signed_128_helpers_keep_their_success_refusal_partition() {
    let values = [i128::MIN, i128::MIN + 1, -2, -1, 0, 1, 2, i128::MAX];
    for a in values {
        for b in values {
            let x = I256::from_i128(a);
            let y = I256::from_i128(b);
            assert_eq!(x.try_i128(), Ok(a));
            assert_eq!(x.cmp(y), a.cmp(&b));
            assert_eq!(
                x.checked_add(y).and_then(I256::try_i128).ok(),
                a.checked_add(b)
            );
            assert_eq!(
                x.checked_sub(y).and_then(I256::try_i128).ok(),
                a.checked_sub(b)
            );
            assert_eq!(
                x.checked_mul(y).and_then(I256::try_i128).ok(),
                legacy_scalar::multiply(a, b)
            );
            for up in [false, true] {
                let round = if up { Rounding::Ceil } else { Rounding::Floor };
                assert_eq!(
                    x.checked_div(y, round).and_then(I256::try_i128).ok(),
                    legacy_scalar::divide(up, a, b)
                );
                for den in [-1, 0, 1, 2, i128::MAX] {
                    let d = I256::from_i128(den);
                    assert_eq!(
                        x.nonnegative_mul_div(y, d, round)
                            .and_then(I256::try_i128)
                            .ok(),
                        legacy_scalar::mul_div(up, a, b, den)
                    );
                    assert_eq!(
                        x.mul_div_positive(y, d, Rounding::Floor)
                            .and_then(I256::try_i128)
                            .ok(),
                        legacy_scalar::signed_mul_div_floor(a, b, den)
                    );
                }
            }
        }
    }
}

#[test]
fn actual_legacy_unsigned_word_kernel_is_reused_exactly() {
    for a in [0, 1, 2, u64::MAX as u128, 1u128 << 127, u128::MAX] {
        for b in [0, 1, 2, u64::MAX as u128, 1u128 << 127, u128::MAX] {
            let x = U256::from_u128(a);
            let y = U256::from_u128(b);
            let old = wide::multiply(a, b);
            assert_eq!(
                x.full_product(y),
                Ok(U512::from_words_le([old[0], old[1], 0, 0]))
            );
            assert_eq!(
                x.checked_add(y).and_then(U256::try_u128).ok(),
                a.checked_add(b)
            );
            assert_eq!(
                x.checked_sub(y).and_then(U256::try_u128).ok(),
                a.checked_sub(b)
            );
            for den in [0, 1, 2, u128::MAX] {
                let d = U256::from_u128(den);
                assert_eq!(
                    x.mul_div(y, d, Rounding::Floor)
                        .and_then(U256::try_u128)
                        .ok(),
                    wide::mul_div_floor(a, b, den)
                );
                assert_eq!(
                    x.mul_div(y, d, Rounding::Ceil)
                        .and_then(U256::try_u128)
                        .ok(),
                    wide::mul_div_ceil(a, b, den)
                );
            }
        }
    }
}

#[test]
fn high_products_roots_and_differences_are_not_narrowed_early() {
    let two = U256::from_u128(2);
    assert_eq!(U256::MAX.mul_div(two, two, Rounding::Floor), Ok(U256::MAX));
    assert_eq!(U256::MAX.checked_mul(two), Err(Failure::Overflow));
    assert_eq!(
        U256::MAX.geometric_mean(U256::MAX, Rounding::Exact),
        Ok(U256::MAX)
    );
    let n = U512::from_words_le([0, 0, 0, 1u128 << 16]); // 2^400.
    let root = U256::from_words_le([0, 1u128 << 72]); // 2^200.
    assert_eq!(
        n.sqrt_ratio(U512::from_u256(U256::ONE), Rounding::Exact),
        Ok(root)
    );
    let lower = U256::MAX
        .checked_sub(U256::ONE)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(
        I256::difference_unsigned(U256::MAX, lower),
        Ok(I256::from_i128(1))
    );
    assert_eq!(I256::MIN.checked_neg(), Err(Failure::Overflow));
    assert_eq!(
        I256::MIN.checked_div(I256::from_i128(-1), Rounding::Floor),
        Err(Failure::Overflow)
    );
}

#[test]
fn unsigned_three_factor_comparison_keeps_all_768_bits() {
    assert_eq!(
        U256::products_ge([U256::MAX; 3], [U256::MAX, U256::MAX, U256::ONE]),
        Ok(true)
    );
    assert_eq!(
        U256::products_ge([U256::MAX, U256::MAX, U256::ONE], [U256::MAX; 3]),
        Ok(false)
    );
    assert_eq!(
        I256::nonnegative_products_ge([I256::from_i128(-1); 3], [I256::ZERO; 3]),
        Err(Failure::Domain)
    );
}

#[test]
fn all_mutation_is_local_and_arguments_remain_owned_values() {
    let a = U256::MAX;
    let b = U256::from_words_le([7, 9]);
    let saved = (a, b);
    let _ = a.full_product(b);
    let _ = a.div_rem(b);
    let _ = a.checked_add(b);
    assert_eq!((a, b), saved);
    assert_eq!(I256::from_magnitude(U256::ZERO, true), Ok(I256::ZERO));
}

#[test]
fn limb_helpers_preserve_the_previous_overflowing_operations() {
    fn previous_add<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
        let mut out = [0; N];
        let mut carry = false;
        for i in 0..N {
            let (x, p) = a[i].overflowing_add(b[i]);
            let (x, q) = x.overflowing_add(u128::from(carry));
            out[i] = x;
            carry = p || q;
        }
        (out, carry)
    }
    fn previous_subtract<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
        let mut out = [0; N];
        let mut borrow = false;
        for i in 0..N {
            let (x, p) = a[i].overflowing_sub(b[i]);
            let (x, q) = x.overflowing_sub(u128::from(borrow));
            out[i] = x;
            borrow = p || q;
        }
        (out, borrow)
    }
    fn compare<const N: usize>() {
        let check = |a, b| {
            assert_eq!(add(a, b), previous_add(a, b));
            assert_eq!(subtract(a, b), previous_subtract(a, b));
        };
        let boundaries = [
            0,
            1,
            2,
            u64::MAX as u128,
            1u128 << 127,
            u128::MAX - 1,
            u128::MAX,
        ];
        for a in boundaries {
            for b in boundaries {
                check([a; N], [b; N]);
            }
        }
        if N > 0 {
            let mut one = [0; N];
            one[0] = 1;
            check([u128::MAX; N], one);
            check([0; N], one);
        }
        let mut state = 0x4ed0_dfb2_7a54_6719_d24a_9c15_3351_740bu128;
        let mut next = || {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state
        };
        for _ in 0..512 {
            check(
                core::array::from_fn(|_| next()),
                core::array::from_fn(|_| next()),
            );
        }
    }
    compare::<0>();
    compare::<1>();
    compare::<2>();
    compare::<3>();
    compare::<4>();
    compare::<6>();
}

#[test]
fn wide_narrowing_preserves_every_previous_zero_pattern() {
    for nonzero in [1, 2, 1u128 << 127, u128::MAX] {
        for mask in 0..16 {
            let mut words = [u128::MAX, 7, 0, 0, 0, 0];
            for i in 0..4 {
                words[i + 2] = if mask & (1 << i) != 0 { nonzero } else { 0 };
            }
            let previous = if words[2..].iter().all(|x| *x == 0) {
                Ok(U256([words[0], words[1]]))
            } else {
                Err(Failure::Overflow)
            };
            assert_eq!(U768(words).narrow(), previous);
        }
    }
}

#[test]
fn checked_negation_preserves_the_minimum_refusal_and_exact_endpoints() {
    let minimum_plus_one = I256::from_bits(U256::from_words_le([1, 1u128 << 127]));
    assert_eq!(I256::MIN.checked_neg(), Err(Failure::Overflow));
    assert_eq!(I256::ZERO.checked_neg(), Ok(I256::ZERO));
    assert_eq!(I256::MAX.checked_neg(), Ok(minimum_plus_one));
    assert_eq!(minimum_plus_one.checked_neg(), Ok(I256::MAX));
    assert_eq!(I256::from_i128(-1).checked_neg(), Ok(I256::from_i128(1)));
    assert_eq!(I256::from_i128(1).checked_neg(), Ok(I256::from_i128(-1)));
    let words = [
        0,
        1,
        2,
        u64::MAX as u128,
        (1u128 << 127) - 1,
        1u128 << 127,
        (1u128 << 127) + 1,
        u128::MAX,
    ];
    for low in words {
        for high in words {
            let value = I256::from_bits(U256::from_words_le([low, high]));
            let previous = if value == I256::MIN {
                Err(Failure::Overflow)
            } else {
                Ok(I256(I256::negative_bits(value.0)))
            };
            assert_eq!(value.checked_neg(), previous);
            if let Ok(negative) = value.checked_neg() {
                assert_eq!(negative.checked_neg(), Ok(value));
            }
        }
    }
}

/// Verbatim copies of the iterator helpers that the indexed loops replaced.
mod previous_helpers {
    use super::super::{Failure, compare, subtract, wide};
    use core::cmp::Ordering;

    pub(super) fn add_word<const N: usize>(
        out: &mut [u128; N],
        at: usize,
        value: u128,
    ) -> Result<(), Failure> {
        if at >= N {
            return if value == 0 {
                Ok(())
            } else {
                Err(Failure::Invariant)
            };
        }
        let mut carry = value;
        for limb in &mut out[at..] {
            let (next, overflow) = limb.overflowing_add(carry);
            *limb = next;
            carry = u128::from(overflow);
            if carry == 0 {
                return Ok(());
            }
        }
        Err(Failure::Invariant)
    }
    pub(super) fn multiply<const A: usize, const B: usize, const O: usize>(
        a: [u128; A],
        b: [u128; B],
    ) -> Result<[u128; O], Failure> {
        let mut out = [0; O];
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                let product = wide::multiply(*x, *y);
                let at = i.checked_add(j).ok_or(Failure::Invariant)?;
                add_word(&mut out, at, product[0])?;
                add_word(
                    &mut out,
                    at.checked_add(1).ok_or(Failure::Invariant)?,
                    product[1],
                )?;
            }
        }
        Ok(out)
    }
    pub(super) fn divide<const N: usize>(
        n: [u128; N],
        d: [u128; N],
    ) -> Result<([u128; N], [u128; N]), Failure> {
        if d.iter().all(|x| *x == 0) {
            return Err(Failure::ZeroDivisor);
        }
        let bits = N.checked_mul(128).ok_or(Failure::Domain)?;
        let mut q = [0; N];
        let mut r = [0; N];
        for bit in (0..bits).rev() {
            let mut carry = (n[bit / 128] >> (bit % 128)) & 1;
            for limb in &mut r {
                let next = *limb >> 127;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            if carry != 0 || compare(&r, &d) != Ordering::Less {
                let (next, borrow) = subtract(r, d);
                if borrow != (carry != 0) {
                    return Err(Failure::Invariant);
                }
                r = next;
                q[bit / 128] |= 1u128 << (bit % 128);
            }
        }
        if compare(&r, &d) != Ordering::Less {
            return Err(Failure::Invariant);
        }
        Ok((q, r))
    }
}

/// Deterministic limb inputs: every boundary word in every limb, one-limb and
/// top-limb shapes, then xorshift words with random zero limbs and short values.
fn helper_inputs<const N: usize>(seed: u128, random: usize) -> Vec<[u128; N]> {
    let boundaries = [
        0,
        1,
        2,
        3,
        u64::MAX as u128,
        1u128 << 64,
        (1u128 << 127) - 1,
        1u128 << 127,
        u128::MAX - 1,
        u128::MAX,
    ];
    let mut inputs = Vec::new();
    for word in boundaries {
        inputs.push([word; N]);
        for at in 0..N {
            let mut single = [0; N];
            single[at] = word;
            inputs.push(single);
            let mut below = [u128::MAX; N];
            below[at] = word;
            inputs.push(below);
        }
    }
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 7;
        state ^= state >> 9;
        state ^= state << 8;
        state
    };
    for _ in 0..random {
        let shape = next();
        let mut words: [u128; N] = core::array::from_fn(|_| next());
        for (at, word) in words.iter_mut().enumerate() {
            if shape >> (at % 128) & 1 == 0 {
                *word = 0;
            } else if shape >> ((at + 64) % 128) & 1 == 0 {
                *word >>= next() % 128;
            }
        }
        inputs.push(words);
    }
    inputs
}

fn compare_divide<const N: usize>(seed: u128, random: usize) {
    let inputs = helper_inputs::<N>(seed, random);
    let mut count = 0;
    for (index, n) in inputs.iter().enumerate() {
        // Every divisor shape against a rotating window of numerators.
        for d in inputs.iter().skip(index % 7).step_by(7) {
            assert_eq!(
                divide(*n, *d),
                previous_helpers::divide(*n, *d),
                "{n:?} / {d:?}"
            );
            count += 1;
        }
    }
    assert!(count > 0 || N == 0);
}

fn compare_multiply<const A: usize, const B: usize, const O: usize>(seed: u128, random: usize) {
    let left = helper_inputs::<A>(seed, random);
    let right = helper_inputs::<B>(seed ^ 0x5a5a, random);
    for (index, a) in left.iter().enumerate() {
        for b in right.iter().skip(index % 5).step_by(5) {
            assert_eq!(
                multiply::<A, B, O>(*a, *b),
                previous_helpers::multiply::<A, B, O>(*a, *b),
                "{a:?} * {b:?}"
            );
        }
    }
}

fn compare_add_word<const N: usize>(seed: u128, random: usize) {
    let words = [
        0,
        1,
        2,
        (1u128 << 127) - 1,
        1u128 << 127,
        u128::MAX - 1,
        u128::MAX,
    ];
    for start in helper_inputs::<N>(seed, random) {
        for at in 0..N + 3 {
            for value in words {
                let mut actual = start;
                let mut previous = start;
                assert_eq!(
                    add_word(&mut actual, at, value),
                    previous_helpers::add_word(&mut previous, at, value),
                    "{start:?} + {value} at {at}"
                );
                // Refusals leave the same partially carried scratch as before.
                assert_eq!(actual, previous);
            }
        }
    }
}

#[test]
fn indexed_helpers_preserve_the_previous_iterator_helpers() {
    compare_divide::<0>(0x11, 4);
    compare_divide::<1>(0x2b7e_1516_28ae_d2a6, 400);
    compare_divide::<2>(0xabf7_1588_09cf_4f3c, 900);
    compare_divide::<3>(0x7623_8b7e_a85f_2041, 300);
    compare_divide::<4>(0x13c6_ef37_2fe9_4f82, 400);
    compare_divide::<6>(0xa54f_f53a_5f1d_36f1, 200);
    compare_multiply::<0, 2, 2>(0x51, 4);
    compare_multiply::<1, 1, 2>(0x9b05_688c_2b3e_6c1f, 400);
    compare_multiply::<2, 2, 4>(0x1f83_d9ab_fb41_bd6b, 900);
    compare_multiply::<4, 2, 6>(0x5be0_cd19_137e_2179, 400);
    // Too-small outputs reach the old Invariant refusal on the same inputs.
    compare_multiply::<2, 2, 3>(0x6a09_e667_f3bc_c908, 600);
    compare_multiply::<2, 2, 2>(0xbb67_ae85_84ca_a73b, 400);
    compare_multiply::<3, 2, 4>(0x3c6e_f372_fe94_f82b, 300);
    for seed in [0xa54f_f53a_5f1d_36f1u128, 0x510e_527f_ade6_82d1] {
        compare_add_word::<0>(seed, 4);
        compare_add_word::<1>(seed, 60);
        compare_add_word::<2>(seed, 60);
        compare_add_word::<4>(seed, 60);
        compare_add_word::<6>(seed, 60);
    }
}

#[test]
fn checked_division_remainder_test_matches_the_derived_equality() {
    let words = [0, 1, 2, u64::MAX as u128, 1u128 << 127, u128::MAX];
    for low in words {
        for high in words {
            let r = U256::from_words_le([low, high]);
            assert_eq!(r.0 != U256::ZERO.0, r != U256::ZERO);
        }
    }
    let seven = U256::from_u128(7);
    let two = U256::from_u128(2);
    assert_eq!(
        seven.checked_div(two, Rounding::Floor),
        Ok(U256::from_u128(3))
    );
    assert_eq!(
        seven.checked_div(two, Rounding::Ceil),
        Ok(U256::from_u128(4))
    );
    assert_eq!(
        seven.checked_div(two, Rounding::Exact),
        Err(Failure::Inexact)
    );
    assert_eq!(
        seven.checked_div(U256::ZERO, Rounding::Exact),
        Err(Failure::ZeroDivisor)
    );
    assert_eq!(
        U256::MAX.checked_div(U256::MAX, Rounding::Ceil),
        Ok(U256::ONE)
    );
    assert_eq!(
        U256::MAX.checked_div(two, Rounding::Ceil),
        U256::MAX.checked_sub(
            U256::MAX
                .checked_div(two, Rounding::Floor)
                .unwrap_or(U256::ZERO)
        )
    );
}
