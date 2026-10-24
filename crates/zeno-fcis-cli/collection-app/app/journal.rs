//! Bounded JSON framing. serde_json still validates each complete value.
//! This reader owns only one page, never the enclosing entries array.
use serde_json::Value;
use std::io::BufRead;

pub(super) const VALUE_LIMIT: usize = 64 * 1024 * 1024;

pub(super) struct Reader<R> {
    input: R,
}
impl<R: BufRead> Reader<R> {
    pub(super) fn new(input: R) -> Self {
        Self { input }
    }
    fn peek(&mut self) -> Result<Option<u8>, String> {
        self.input
            .fill_buf()
            .map(|b| b.first().copied())
            .map_err(|e| e.to_string())
    }
    fn whitespace(&mut self) -> Result<(), String> {
        while self
            .peek()?
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.input.consume(1);
        }
        Ok(())
    }
    pub(super) fn token(&mut self, byte: u8) -> Result<(), String> {
        self.whitespace()?;
        if self.peek()? != Some(byte) {
            return Err("journal delimiter".into());
        }
        self.input.consume(1);
        Ok(())
    }
    /// Read the first element, or require a comma before each later element.
    /// In particular a trailing comma cannot be mistaken for an empty element.
    pub(super) fn more(&mut self, close: u8, first: bool) -> Result<bool, String> {
        self.whitespace()?;
        if self.peek()? == Some(close) {
            self.input.consume(1);
            return Ok(false);
        }
        if !first {
            self.token(b',')?;
        }
        Ok(true)
    }
    pub(super) fn end(&mut self) -> Result<(), String> {
        self.whitespace()?;
        if self.peek()?.is_some() {
            return Err("journal trailing data".into());
        }
        Ok(())
    }
    pub(super) fn value(&mut self, limit: usize) -> Result<Value, String> {
        self.whitespace()?;
        let mut bytes = Vec::new();
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        while let Some(byte) = self.peek()? {
            if !quoted
                && depth == 0
                && matches!(byte, b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t')
            {
                break;
            }
            if bytes.len() == limit {
                return Err("journal value exceeds page limit".into());
            }
            self.input.consume(1);
            bytes.push(byte);
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
            } else {
                match byte {
                    b'"' => quoted = true,
                    b'{' | b'[' => {
                        depth += 1;
                        if depth > 128 {
                            return Err("journal nesting".into());
                        }
                    }
                    b'}' | b']' => {
                        depth = depth.checked_sub(1).ok_or("journal nesting")?;
                    }
                    _ => {}
                }
            }
            if !quoted && depth == 0 && matches!(byte, b'"' | b'}' | b']') {
                break;
            }
        }
        serde_json::from_slice(&bytes).map_err(|_| "journal value encoding".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn journal_values_are_bounded_before_buffering() {
        let data = format!("\"{}\"", "x".repeat(1024));
        let mut reader = Reader::new(Cursor::new(data.as_bytes()));
        let result = reader.value(128);
        assert!(result.is_err(), "oversize journal value was buffered");
        assert_eq!(reader.input.position(), 128, "oversize input was consumed");
    }

    #[test]
    fn journal_framing_handles_escapes_nesting_and_chunk_boundaries() {
        for value in [
            serde_json::json!({"a":["x\\\"}]", 42, null]}),
            serde_json::json!("λ😀"),
            serde_json::json!(false),
            serde_json::json!(123),
        ] {
            let data = format!("[{},{}]", value, value);
            for capacity in 1..=8 {
                let mut reader = Reader::new(BufReader::with_capacity(capacity, data.as_bytes()));
                reader.token(b'[').unwrap_or_else(|e| panic!("{e}"));
                assert!(reader.more(b']', true).unwrap_or(false));
                assert_eq!(reader.value(1024).ok(), Some(value.clone()));
                assert!(reader.more(b']', false).unwrap_or(false));
                assert_eq!(reader.value(1024).ok(), Some(value.clone()));
                assert!(!reader.more(b']', false).unwrap_or(true));
                assert!(reader.end().is_ok());
            }
        }
    }
}
