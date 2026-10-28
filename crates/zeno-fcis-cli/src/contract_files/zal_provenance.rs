//! Optional inert app-provenance data, captured before scaffold writes.
//!
//! Hashes and matching declared revisions establish metadata correspondence;
//! they do not check ZAL semantics, authentication or native correspondence.

use std::path::Path;

use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

use super::{Failure, Inputs, invalid, read_input};
use crate::transform::sha256_hex;

const MAP: &str = "zal-source-map.json";
const BINDING: &str = "zal-app-binding.json";
const SCHEMA: &str = "zal/factory-source-map/3";
const TEXT_LIMIT: usize = 65_536;
const FILES: [&str; 5] = [
    "zal-original.zal",
    BINDING,
    MAP,
    "tests/zal-authority-examples.txt",
    "tests/zal_correspondence.rs",
];

fn optional(dir: &Path, name: &str) -> Result<Option<Vec<u8>>, Failure> {
    match read_input(&dir.join(name)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && dir
                    .join(name)
                    .symlink_metadata()
                    .is_err_and(|metadata_error| {
                        metadata_error.kind() == std::io::ErrorKind::NotFound
                    }) =>
        {
            Ok(None)
        }
        Err(error) => Err(Failure::Read(format!("read {name}: {error}"))),
    }
}

// Keep a top-level /3 marker once seen; later errors cannot downgrade it.
// The strict parser below decides admission after this format-selection step.
fn declares_app(bytes: &[u8]) -> bool {
    struct Marker<'a>(&'a mut bool);
    impl<'de> Visitor<'de> for Marker<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a source-map object")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<String>()? {
                if key == "schema" {
                    let value = map.next_value::<Value>()?;
                    if value.as_str() == Some(SCHEMA) {
                        *self.0 = true;
                    }
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(())
        }
    }
    let mut declared = false;
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let _ = serde::de::Deserializer::deserialize_map(&mut decoder, Marker(&mut declared));
    declared
}

fn json(bytes: &[u8], name: &str) -> Result<Value, Failure> {
    crate::integral_json::parse(bytes)
        .map_err(|_| invalid(name, "invalid app provenance JSON or duplicate key"))
}

fn pin(mapping: &Value, key: &str, bytes: &[u8], name: &str) -> Result<(), Failure> {
    if mapping.get(key).and_then(Value::as_str) != Some(sha256_hex(bytes).as_str()) {
        return Err(invalid(
            MAP,
            format!("app provenance hash mismatch: {name}"),
        ));
    }
    Ok(())
}

/// The two executable sources and UI are the caller's existing captured bytes.
/// Every additional read and check finishes before the native writer runs.
pub(super) fn capture(
    inputs: &Inputs,
    dir: &Path,
    ui: Option<&[u8]>,
) -> Result<Vec<(String, Vec<u8>)>, Failure> {
    let binding = optional(dir, BINDING)?;
    let mapping = optional(dir, MAP)?;
    if binding.is_none() && !mapping.as_deref().is_some_and(declares_app) {
        // Ordinary contracts and legacy finite /1,/2 exports keep their prior
        // behavior; auxiliary filenames alone do not declare an app bundle.
        return Ok(Vec::new());
    }
    let required = |name: &str, value: Option<Vec<u8>>| {
        value.ok_or_else(|| invalid(name, "declared app provenance bundle is incomplete"))
    };
    let raw = [
        required(FILES[0], optional(dir, FILES[0])?)?,
        required(BINDING, binding)?,
        required(MAP, mapping)?,
        required(FILES[3], optional(dir, FILES[3])?)?,
        required(FILES[4], optional(dir, FILES[4])?)?,
    ];
    for (name, bytes) in FILES.iter().zip(&raw) {
        if ((*name == FILES[0] || *name == BINDING || *name == MAP) && bytes.len() > TEXT_LIMIT)
            || std::str::from_utf8(bytes).is_err()
        {
            return Err(invalid(*name, "app provenance text is not bounded UTF-8"));
        }
    }
    let binding = json(&raw[1], BINDING)?;
    let mapping = json(&raw[2], MAP)?;
    if mapping.get("schema").and_then(Value::as_str) != Some(SCHEMA)
        || mapping.get("profile").and_then(Value::as_str) != Some("zal/finite-fsm/2")
        || binding.get("format").and_then(Value::as_str) != Some("zal/app-binding/1")
    {
        return Err(invalid(MAP, "unsupported app provenance declaration"));
    }
    let revision = mapping.get("revision").and_then(Value::as_str);
    if !revision.is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) || binding.get("model_revision").and_then(Value::as_str) != revision
    {
        return Err(invalid(MAP, "app provenance revision declarations differ"));
    }
    for (key, bytes, name) in [
        ("source_sha256", raw[0].as_slice(), FILES[0]),
        ("binding_sha256", raw[1].as_slice(), BINDING),
        ("reference_sha256", raw[3].as_slice(), FILES[3]),
        ("project_sha256", inputs.project.as_bytes(), super::PROJECT),
        ("policy_sha256", inputs.rules.as_bytes(), super::RULES),
    ] {
        pin(&mapping, key, bytes, name)?;
    }
    let ui =
        ui.ok_or_else(|| invalid("ui.json", "declared app provenance needs its captured UI"))?;
    pin(&mapping, "ui_sha256", ui, "ui.json")?;
    Ok(FILES
        .into_iter()
        .zip(raw)
        .map(|(name, bytes)| (format!("provenance/zal/{name}"), bytes))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn capture_uses_existing_subject_snapshot_and_returns_owned_raw_bytes() {
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let path =
            std::env::temp_dir().join(format!("zeno-provenance-owned-{}", std::process::id()));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("create owned fixture: {e}"));
        let root = Owned(path);
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zal-app-provenance/app");
        for name in FILES
            .into_iter()
            .chain([super::super::PROJECT, super::super::RULES, "ui.json"])
        {
            let target = root.0.join(name);
            fs::create_dir_all(target.parent().unwrap_or(&root.0))
                .unwrap_or_else(|e| panic!("fixture directory: {e}"));
            fs::copy(fixture.join(name), target).unwrap_or_else(|e| panic!("fixture copy: {e}"));
        }
        let inputs = Inputs::read(&root.0).unwrap_or_else(|_| panic!("capture executable inputs"));
        let ui = fs::read(root.0.join("ui.json")).unwrap_or_else(|e| panic!("capture UI: {e}"));
        let original =
            fs::read(root.0.join(FILES[0])).unwrap_or_else(|e| panic!("source bytes: {e}"));
        for name in [super::super::PROJECT, super::super::RULES, "ui.json"] {
            fs::write(root.0.join(name), b"later disk drift")
                .unwrap_or_else(|e| panic!("change uncaptured disk file: {e}"));
        }
        let retained = capture(&inputs, &root.0, Some(&ui))
            .unwrap_or_else(|_| panic!("must use already captured executable/UI bytes"));
        fs::write(root.0.join(FILES[0]), b"later source drift")
            .unwrap_or_else(|e| panic!("change source after capture: {e}"));
        let retained_source = retained
            .iter()
            .find(|(name, _)| name == "provenance/zal/zal-original.zal")
            .unwrap_or_else(|| panic!("retained source"));
        assert_eq!(retained_source.1, original);
        assert_eq!(retained.len(), 5);
    }
}
