//! Bounded shell intake and exact artifact bytes for the private pilot.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path};

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

pub(super) const REVISION: &str = "construction-r2";
pub(super) const SOURCE_LIMIT: usize = 1 << 20;
pub(super) const DOCUMENT_LIMIT: usize = 1 << 16;
pub(super) const LOG_LIMIT: usize = 1 << 24;
pub(super) type Result<T> = std::result::Result<T, Failure>;

#[derive(Debug)]
pub(super) struct Failure {
    pub(super) code: i32,
    pub(super) reason: String,
}
pub(super) fn fail<T>(code: i32, reason: impl Into<String>) -> Result<T> {
    Err(Failure {
        code,
        reason: reason.into(),
    })
}
pub(super) fn malformed(reason: impl Into<String>) -> Failure {
    Failure {
        code: 65,
        reason: reason.into(),
    }
}
pub(super) fn gate(reason: impl Into<String>) -> Failure {
    Failure {
        code: 1,
        reason: reason.into(),
    }
}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self {
            code: 3,
            reason: error.to_string(),
        }
    }
}
impl From<zeno_fcis_contract::ContractError> for Failure {
    fn from(error: zeno_fcis_contract::ContractError) -> Self {
        gate(error.to_string())
    }
}
pub(super) fn sha(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(super) fn canonical(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    // serde_json without preserve_order emits object properties in key order.
    let value = serde_json::to_value(value).map_err(|e| malformed(e.to_string()))?;
    let mut bytes = serde_json::to_vec(&value).map_err(|e| malformed(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(super) fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return fail(3, "input-not-regular-file");
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return fail(65, format!("input-too-large: {}", path.display()));
    }
    Ok(bytes)
}
pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}
pub(super) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    write(path, &canonical(value)?)
}

// Same serde Visitor pattern as the CLI's bounded strict JSON intake: numbers
// retain their kind, duplicate keys and floating point are refused recursively.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Reader;
        impl<'de> Visitor<'de> for Reader {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("integral JSON with unique properties")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Strict(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(de::Error::custom("duplicate JSON property"));
                    }
                    let Strict(v) = a.next_value()?;
                    values.insert(k, v);
                }
                Ok(Strict(Value::Object(values)))
            }
        }
        d.deserialize_any(Reader)
    }
}
pub(super) fn strict(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice::<Strict>(bytes)
        .map(|v| v.0)
        .map_err(|e| malformed(e.to_string()))
}
pub(super) fn closed(value: &Value, keys: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| malformed("object-required"))?;
    if object.len() != keys.len() || !keys.iter().all(|key| object.contains_key(*key)) {
        return fail(65, "closed-object-fields");
    }
    Ok(())
}
pub(super) fn number(value: &Value, lo: i128, hi: i128) -> Result<i128> {
    let n = value
        .as_i64()
        .map(i128::from)
        .ok_or_else(|| malformed("integer-kind"))?;
    if n < lo || n > hi {
        return fail(65, "integer-range");
    }
    Ok(n)
}
pub(super) fn relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return fail(65, "relative-evidence-path");
    }
    Ok(path)
}
pub(super) fn check_ref(root: &Path, reference: &Value, limit: usize) -> Result<Vec<u8>> {
    closed(reference, &["path", "sha256"])?;
    let path = reference["path"]
        .as_str()
        .ok_or_else(|| malformed("reference-path"))?;
    let bytes = read(&root.join(relative(path)?), limit)?;
    if reference["sha256"].as_str() != Some(sha(&bytes).as_str()) {
        return fail(65, "reference-sha256");
    }
    Ok(bytes)
}
