//! Owned static assets with exact routes; no filesystem lookup at request time.
use crate::Refusal;

/// An immutable, owned public asset. Credentials must never be included.
pub struct Asset {
    pub(crate) route: String,
    pub(crate) content_type: &'static str,
    pub(crate) bytes: Vec<u8>,
}
impl Asset {
    /// Copy one asset, refusing ambiguous routes and individual files over 16 MiB.
    pub fn new(route: &str, content_type: &'static str, bytes: &[u8]) -> Result<Self, Refusal> {
        if !route.starts_with('/')
            || route.contains(['?', '#', '%', '\\'])
            || route.split('/').any(|part| part == "." || part == "..")
            || !route.is_ascii()
            || route.bytes().any(|b| b.is_ascii_whitespace())
            || bytes.len() > 16 * 1024 * 1024
            || ![
                "text/html; charset=utf-8",
                "text/css; charset=utf-8",
                "text/javascript; charset=utf-8",
                "application/json",
                "application/wasm",
            ]
            .contains(&content_type)
        {
            return Err(Refusal::Malformed);
        }
        Ok(Self {
            route: route.to_owned(),
            content_type,
            bytes: bytes.to_vec(),
        })
    }
}
