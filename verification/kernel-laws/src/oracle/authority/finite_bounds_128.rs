//! Exact i128 interval width for the private retained authority oracle.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Returns the cardinality of a nonempty signed interval when it fits `u64`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        match result {
            Some(width) => min <= max
                && width as int == max as int - min as int + 1,
            None => min > max || max as int - min as int + 1 > u64::MAX,
        },
))]
pub(crate) fn interval_width(min: i128, max: i128) -> Option<u64> {
    match max.checked_sub(min) {
        Some(difference) => {
            if difference < 0 || difference >= u64::MAX as i128 {
                None
            } else {
                Some((difference + 1) as u64)
            }
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::interval_width;

    #[test]
    fn interval_boundaries_match_the_mathematical_domain() {
        assert_eq!(interval_width(i128::MIN, i128::MAX), None);
        assert_eq!(
            interval_width(i128::MIN, i128::MIN + u64::MAX as i128 - 1),
            Some(u64::MAX)
        );
        assert_eq!(
            interval_width(i128::MAX - u64::MAX as i128 + 1, i128::MAX),
            Some(u64::MAX)
        );
        assert_eq!(interval_width(i128::MIN, i128::MIN), Some(1));
        assert_eq!(interval_width(i128::MAX, i128::MAX), Some(1));
        assert_eq!(interval_width(i64::MIN as i128, i64::MAX as i128), None);
        assert_eq!(
            interval_width(i64::MIN as i128, i64::MAX as i128 - 1),
            Some(u64::MAX)
        );
        assert_eq!(
            interval_width(i64::MIN as i128 + 1, i64::MAX as i128),
            Some(u64::MAX)
        );
        assert_eq!(
            interval_width(i128::MIN, i128::MIN + u64::MAX as i128),
            None
        );
        assert_eq!(
            interval_width(i128::MAX - u64::MAX as i128, i128::MAX),
            None
        );
        assert_eq!(interval_width(0, 5), Some(6));
        assert_eq!(interval_width(1, 0), None);
        assert_eq!(interval_width(i128::MAX, i128::MIN), None);
    }
}
