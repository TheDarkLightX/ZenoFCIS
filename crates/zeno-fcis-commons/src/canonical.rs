use serde_json::Value;

use crate::CommonsError;

/// Serialize JSON with Python `json.dumps` key ordering and compact separators.
pub fn canonical_json(value: &Value) -> Result<String, CommonsError> {
    let mut output = String::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn write_value(value: &Value, output: &mut String) -> Result<(), CommonsError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => {
            if let Some(float) = number.as_f64().filter(|_| number.is_f64()) {
                output.push_str(&python_float(float));
            } else {
                output.push_str(&number.to_string());
            }
        }
        Value::String(value) => output.push_str(
            &serde_json::to_string(value).map_err(|error| CommonsError::new(error.to_string()))?,
        ),
        Value::Array(values) => {
            output.push('[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(item, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by_key(|(key, _)| *key);
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key)
                        .map_err(|error| CommonsError::new(error.to_string()))?,
                );
                output.push(':');
                write_value(item, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn python_float(value: f64) -> String {
    let representation = format!("{value:?}");
    let Some((mantissa, exponent)) = representation.split_once('e') else {
        return representation;
    };
    let parsed_exponent = exponent.parse::<i32>().unwrap_or(0);
    let sign = if parsed_exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", parsed_exponent.unsigned_abs())
}
