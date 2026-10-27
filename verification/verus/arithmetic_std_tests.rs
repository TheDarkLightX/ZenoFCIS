//! Native samples for the checked-integer standard-library contracts we trust.
#![allow(dead_code)]
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/scalar.rs"]
mod scalar;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/wide.rs"]
mod wide;

#[test]
fn checked_integer_contract_samples() {
    assert_eq!(i128::MAX.checked_add(1), None);
    assert_eq!(i128::MIN.checked_sub(1), None);
    assert_eq!(i128::MAX.checked_mul(2), None);
    assert_eq!(5i128.checked_mul(-2), Some(-10));
    assert_eq!(u128::MAX.checked_add(1), None);
    assert_eq!(0u128.checked_sub(1), None);
    assert_eq!(u128::MAX.checked_mul(2), None);
    assert_eq!(1u128.checked_div(0), None);
    assert_eq!(5u128.checked_div(2), Some(2));
    assert!((-2_i128..=3).contains(&3));
    assert!(!(-2_i128..=3).contains(&4));
    assert_eq!(i128::from(true), 1);
    assert_eq!(i128::from(false), 0);
}

#[test]
fn ceiling_division_witness() {
    assert_eq!(scalar::divide(true, 1, 2), Some(1));
}

#[test]
fn full_product_refuses_overflow() {
    assert_eq!(scalar::mul_div(false, i128::MAX, i128::MAX, 1), None);
    assert_eq!(wide::mul_div_floor(u128::MAX, u128::MAX, 1), None);
    assert_eq!(
        wide::mul_div_floor(u128::MAX, u128::MAX, u128::MAX),
        Some(u128::MAX)
    );
}
