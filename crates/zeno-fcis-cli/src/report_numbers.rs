//! Lossless integers in door/judge reports. Numbers within +/- (2^53 - 1)
//! remain JSON numbers; larger magnitudes are canonical decimal strings.
//! This avoids serde_json::Value overflow and JavaScript Number rounding.
//! Program documents, receipts and compiler input documents keep their own
//! existing encodings. This formatter grants no decision authority.

use serde_json::Value;

pub(crate) const ENCODING: &str = "safe-number-or-decimal-string/1";
const SAFE: i128 = (1i128 << 53) - 1;

pub(crate) fn integer(value: i128) -> Value {
    match i64::try_from(value)
        .ok()
        .filter(|_| (-SAFE..=SAFE).contains(&value))
    {
        Some(number) => Value::from(number),
        None => Value::from(value.to_string()),
    }
}

pub(crate) fn integers(values: &[i128]) -> Value {
    Value::Array(values.iter().copied().map(integer).collect())
}

/// Read this encoding and exact integer numbers from legacy reports. Floats,
/// Boolean aliases and noncanonical/out-of-range decimal strings are refused.
pub(crate) fn read(value: &Value) -> Option<i128> {
    if let Some(text) = value.as_str() {
        let number = text.parse::<i128>().ok()?;
        return (number.to_string() == text).then_some(number);
    }
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_reports_round_trip_extremes_and_small_controls_without_float_numbers() {
        for value in [
            i128::MIN,
            i128::MAX,
            -(1 << 100),
            1 << 100,
            1 << 64,
            i128::from(i64::MIN),
            i128::from(i64::MAX),
            -SAFE - 1,
            -SAFE,
            -5,
            -1,
            0,
            1,
            5,
            SAFE,
            SAFE + 1,
        ] {
            let encoded = integer(value);
            let bytes = serde_json::to_vec(&encoded).unwrap();
            let received: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(read(&received), Some(value));
            assert_eq!(encoded.is_number(), (-SAFE..=SAFE).contains(&value));
            if encoded.is_string() {
                assert_eq!(encoded.as_str(), Some(value.to_string().as_str()));
            }
        }
        assert_eq!(integer(5), json!(5));
        assert_eq!(integers(&[5, i128::MIN]), json!([5, i128::MIN.to_string()]));
        assert_eq!(read(&json!(u64::MAX)), Some(i128::from(u64::MAX)));
    }

    #[test]
    fn report_reader_refuses_aliases_rounding_and_noncanonical_decimals() {
        for bad in [
            json!(true),
            json!(false),
            json!(1.0),
            json!(1.5),
            Value::Null,
            json!("+1"),
            json!("01"),
            json!("-0"),
            json!(" 1"),
            json!("1e2"),
            json!("170141183460469231731687303715884105728"),
            json!("-170141183460469231731687303715884105729"),
        ] {
            assert_eq!(read(&bad), None, "{bad}");
        }
    }
}
