#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-authority/src/finite_bounds.rs"]
mod finite_bounds;

#[cfg(test)]
mod tests {
    use super::finite_bounds::{bounded_product, interval_width};

    #[test]
    fn interval_boundaries_match_the_mathematical_domain() {
        assert_eq!(interval_width(i64::MIN, i64::MAX), None);
        assert_eq!(interval_width(i64::MIN, i64::MAX - 1), Some(u64::MAX));
        assert_eq!(interval_width(i64::MIN + 1, i64::MAX), Some(u64::MAX));
        assert_eq!(interval_width(i64::MIN, i64::MIN), Some(1));
        assert_eq!(interval_width(i64::MAX, i64::MAX), Some(1));
        assert_eq!(interval_width(0, 5), Some(6));
        assert_eq!(interval_width(1, 0), None);
        assert_eq!(interval_width(i64::MAX, i64::MIN), None);
    }

    #[test]
    fn products_refuse_overflow_and_excess_work() {
        assert_eq!(bounded_product(u64::MAX, 2, u64::MAX), None);
        assert_eq!(bounded_product(u64::MAX, u64::MAX, u64::MAX), None);
        assert_eq!(bounded_product(u64::MAX, 1, u64::MAX), Some(u64::MAX));
        assert_eq!(bounded_product(0, u64::MAX, 0), Some(0));
        assert_eq!(bounded_product(u64::MAX, 0, 0), Some(0));
        assert_eq!(bounded_product(216, 4, 864), Some(864));
        assert_eq!(bounded_product(216, 4, 863), None);
        assert_eq!(bounded_product(1_000, 1_000, 1_000_000), Some(1_000_000));
        assert_eq!(bounded_product(1_000, 1_001, 1_000_000), None);
    }
}
