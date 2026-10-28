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
        *word = u128::from_str_radix(&s[end - 32..end], 16).unwrap();
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
    let lower = U256::MAX.checked_sub(U256::ONE).unwrap();
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
