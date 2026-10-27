use zeno_fcis_value::Value;
use zenofcis_zusd_v1::{Accumulator256 as A, laws, *};
fn limits() -> Limits {
    Limits {
        steps: 1_000_000,
        reads: 1000,
        bytes: 1_000_000,
    }
}
fn acc(text: &str) -> A {
    let (h, l) = text.split_once(',').unwrap();
    A::from_limbs(h.parse().unwrap(), l.parse().unwrap())
}
#[test]
fn independent_256_add_and_384_product_boundaries() {
    for row in include_str!("test-data/accumulator_add.tsv").lines() {
        let c: Vec<_> = row.split('\t').collect();
        let a = acc(c[0]);
        let b = acc(c[1]);
        let expected = if c[2] == "refuse" {
            Err(Failure::Arithmetic)
        } else {
            Ok(acc(c[2]))
        };
        let (out, _) = a.checked_add(b, limits()).into_parts();
        assert_eq!(out, expected, "{row}");
        assert!(
            laws::check_accumulator_add(a, b, expected, limits())
                .result()
                .is_ok()
        );
        let wrong = Ok(if expected == Ok(A::ZERO) {
            A::from(1)
        } else {
            A::ZERO
        });
        assert!(
            laws::check_accumulator_add(a, b, wrong, limits())
                .result()
                .is_err()
        );
        for &reason in common::REASONS {
            assert!(
                laws::check_accumulator_add(a, b, Err(Failure::Reject(reason)), limits())
                    .result()
                    .is_err()
            );
        }
        if expected.is_ok() {
            assert!(
                laws::check_accumulator_add(a, b, Err(Failure::Arithmetic), limits())
                    .result()
                    .is_err()
            );
        }
    }
    for row in include_str!("test-data/accumulator_product.tsv").lines() {
        let c: Vec<_> = row.split('\t').collect();
        let expected = if c[3] == "refuse" {
            Err(Failure::Arithmetic)
        } else {
            Ok(c[3].parse().unwrap())
        };
        let (a, n, d) = (acc(c[0]), c[1].parse().unwrap(), c[2].parse().unwrap());
        assert!(
            laws::check_accumulator_product(a, n, d, expected, limits())
                .result()
                .is_ok()
        );
        let wrong = Ok(if expected == Ok(0) { 1 } else { 0 });
        assert!(
            laws::check_accumulator_product(a, n, d, wrong, limits())
                .result()
                .is_err()
        );
        for &reason in common::REASONS {
            assert!(
                laws::check_accumulator_product(a, n, d, Err(Failure::Reject(reason)), limits())
                    .result()
                    .is_err()
            );
        }
        if expected.is_ok() {
            assert!(
                laws::check_accumulator_product(a, n, d, Err(Failure::Arithmetic), limits())
                    .result()
                    .is_err()
            );
        }
        assert_eq!(
            acc(c[0])
                .multiply_divide(c[1].parse().unwrap(), c[2].parse().unwrap(), limits())
                .into_parts()
                .0,
            expected,
            "{row}"
        );
    }
}
#[test]
fn canonical_limbs_and_refusal_retry_preserve_values() {
    for a in [
        A::ZERO,
        A::from(u128::MAX),
        A::from_limbs(1, 0),
        A::from_limbs(1 << 127, 17),
        A::MAX,
    ] {
        assert_eq!(A::from_be_bytes(a.to_be_bytes()), a);
        assert_eq!(A::from_value(&a.to_value().unwrap()), Ok(a));
        let bytes = a.to_be_bytes();
        assert_eq!(&bytes[..16], &a.limbs().0.to_be_bytes());
        let out = a.multiply_divide(u128::MAX, u128::MAX, limits());
        let short = Limits {
            steps: out.usage().steps() - 1,
            ..limits()
        };
        let x = a.multiply_divide(u128::MAX, u128::MAX, short);
        let y = a.multiply_divide(u128::MAX, u128::MAX, short);
        assert!(matches!(x.result(), Err(Failure::Budget { .. })));
        assert_eq!(x.usage(), y.usage());
        assert_eq!(x.result(), y.result());
        assert_eq!(A::from_be_bytes(bytes), a);
    }
    assert_eq!(A::from_value(&Value::unsigned(0)), Err(Failure::Canonical));
    assert_eq!(
        A::from_value(&Value::tuple(vec![Value::unsigned(0)]).unwrap()),
        Err(Failure::Canonical)
    );
}
