//! Integral JSON with unique object keys, read the same way in every build.
//!
//! Numbers are integer tokens in the `i64` or `u64` range, parsed from their text. The Commons
//! crate enables serde_json's `arbitrary_precision` feature, and Cargo unifies features, so in
//! this binary serde_json hands every number to a visitor as a private map instead of an
//! integer or a float. Reading each element as raw text keeps fractions, exponents and wider
//! integers refused whether or not that feature is enabled. Duplicate keys, trailing data and
//! nesting deeper than serde_json's own limit stay refused.
use serde::de::{self, Deserializer as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value, value::RawValue};
use std::fmt;

/// serde_json's own recursion limit.
const MAX_DEPTH: usize = 128;

pub(crate) fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let raw: Box<RawValue> = serde_json::from_slice(bytes)?;
    parse_raw(raw.get(), 0)
}

fn parse_raw(raw: &str, depth: usize) -> Result<Value, serde_json::Error> {
    if depth >= MAX_DEPTH {
        return Err(de::Error::custom("JSON nests too deeply"));
    }
    if raw.starts_with('-') || raw.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        // The raw text is one valid JSON number; only an integer token in range is accepted.
        if let Ok(value) = raw.parse::<i64>() {
            return Ok(Value::from(value));
        }
        return raw
            .parse::<u64>()
            .map(Value::from)
            .map_err(|_| de::Error::custom("expected an integer within the i64 or u64 range"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = decoder.deserialize_any(Integral { depth })?;
    decoder.end()?;
    Ok(value)
}

struct Integral {
    depth: usize,
}

impl<'de> Visitor<'de> for Integral {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("integral JSON without duplicate fields")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(raw) = items.next_element::<Box<RawValue>>()? {
            values.push(parse_raw(raw.get(), self.depth + 1).map_err(de::Error::custom)?);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut entries = Map::new();
        while let Some((key, raw)) = map.next_entry::<String, Box<RawValue>>()? {
            let value = parse_raw(raw.get(), self.depth + 1).map_err(de::Error::custom)?;
            if entries.insert(key, value).is_some() {
                return Err(de::Error::custom("duplicate field"));
            }
        }
        Ok(Value::Object(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn integers_strings_and_structure_round_trip_and_everything_else_refuses() {
        let accepted = parse(br#"{"a":[true,null,-1,0,18446744073709551615,"x"],"b":{}}"#);
        assert_eq!(
            accepted.ok(),
            Some(json!({"a": [true, null, -1, 0, u64::MAX, "x"], "b": {}}))
        );
        for refused in [
            br#"{"a":1,"a":2}"#.as_slice(),
            br#"{"a":1.0}"#,
            br#"[1e3]"#,
            br#"18446744073709551616"#,
            br#"-9223372036854775809"#,
            br#"{}{}"#,
            br#"[1,]"#,
        ] {
            assert!(
                parse(refused).is_err(),
                "{}",
                String::from_utf8_lossy(refused)
            );
        }
        let deep = format!("{}0{}", "[".repeat(200), "]".repeat(200));
        assert!(parse(deep.as_bytes()).is_err());
        // A key that looks like serde_json's private number token is ordinary data.
        assert_eq!(
            parse(br#"{"$serde_json::private::Number":"1.5"}"#).ok(),
            Some(json!({"$serde_json::private::Number": "1.5"}))
        );
    }
}
