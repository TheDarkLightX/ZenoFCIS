//! Deterministic, inspectable generation from closed ZenoFCIS schemas.
//!
//! The generator emits ordinary Rust and Python constants, exact canonical
//! schema bytes, and a content-addressed manifest. Generated artifacts contain
//! no hidden procedural macros and can be diffed or regenerated independently.

#![forbid(unsafe_code)]

mod model;
mod render;

pub use model::{CodegenError, GeneratedBundle, GeneratedFile, GenerationSpec};
pub use render::{GENERATOR_ID, generate};

#[cfg(test)]
mod tests {
    use zeno_fcis_schema::{FieldDef, FieldId, Schema, SchemaLimits, TypeDef, TypeId, TypeKind};

    use super::*;
    use zeno_fcis_codec::CanonicalEncode;

    fn fixture_schema() -> Schema {
        let limits = SchemaLimits::default();
        let amount = match TypeDef::try_new(
            TypeId::new(1),
            "Amount",
            TypeKind::U128 {
                min: 0,
                max: 1_000_000,
            },
            limits,
        ) {
            Ok(value) => value,
            Err(error) => panic!("amount type rejected: {error}"),
        };
        let amount_field = match FieldDef::try_new(FieldId::new(7), "amount", TypeId::new(1)) {
            Ok(value) => value,
            Err(error) => panic!("field rejected: {error}"),
        };
        let state = match TypeDef::try_new(
            TypeId::new(2),
            "BalanceState",
            TypeKind::Record {
                fields: vec![amount_field].into_boxed_slice(),
            },
            limits,
        ) {
            Ok(value) => value,
            Err(error) => panic!("state type rejected: {error}"),
        };
        match Schema::try_new(
            "CodegenFixture",
            1,
            TypeId::new(2),
            vec![state, amount],
            limits,
        ) {
            Ok(value) => value,
            Err(error) => panic!("schema rejected: {error}"),
        }
    }

    fn spec() -> GenerationSpec {
        match GenerationSpec::try_new("codegen_fixture", "codegen_fixture") {
            Ok(value) => value,
            Err(error) => panic!("generation spec rejected: {error}"),
        }
    }

    #[test]
    fn repeated_generation_is_byte_identical() {
        let schema = fixture_schema();
        let left = generate(&schema, &spec());
        let right = generate(&schema, &spec());
        assert_eq!(left, right);
    }

    #[test]
    fn generated_sources_expose_stable_identifiers() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let rust = bundle
            .files()
            .iter()
            .find(|file| file.path() == "rust/codegen_fixture.rs");
        let Some(rust) = rust else {
            panic!("Rust output missing");
        };
        let text = match core::str::from_utf8(rust.bytes()) {
            Ok(value) => value,
            Err(error) => panic!("generated Rust is not UTF-8: {error}"),
        };
        assert!(text.contains("TYPE_BALANCESTATE"));
        assert!(text.contains("FIELD_BALANCESTATE_AMOUNT"));
        assert!(text.contains("SCHEMA_HASH_HEX"));
    }

    #[test]
    fn generated_files_are_in_canonical_path_order() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        assert!(
            bundle
                .files()
                .windows(2)
                .all(|pair| pair[0].path() < pair[1].path())
        );
    }

    #[test]
    fn spec_rejects_rust_keyword_module_name() {
        assert_eq!(
            GenerationSpec::try_new("fn", "valid_name"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_rejects_python_keyword_module_name() {
        assert_eq!(
            GenerationSpec::try_new("valid_name", "class"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_rejects_empty_module_name() {
        assert_eq!(
            GenerationSpec::try_new("", "valid_name"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_rejects_uppercase_module_name() {
        assert_eq!(
            GenerationSpec::try_new("ValidName", "valid_name"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_rejects_hyphenated_module_name() {
        assert_eq!(
            GenerationSpec::try_new("valid-name", "valid_name"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_rejects_oversized_module_name() {
        let long_name = "a".repeat(97);
        assert_eq!(
            GenerationSpec::try_new(&long_name, "valid_name"),
            Err(CodegenError::InvalidModuleName)
        );
    }

    #[test]
    fn spec_accepts_underscore_prefixed_name() {
        assert!(GenerationSpec::try_new("_internal", "_internal").is_ok());
    }

    #[test]
    fn spec_accepts_max_length_name() {
        let max_name = "a".repeat(96);
        assert!(GenerationSpec::try_new(&max_name, &max_name).is_ok());
    }

    #[test]
    fn generated_rust_contains_all_expected_constants() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let rust = bundle
            .files()
            .iter()
            .find(|file| file.path() == "rust/codegen_fixture.rs")
            .unwrap_or_else(|| panic!("Rust output missing"));
        let text = core::str::from_utf8(rust.bytes()).unwrap_or_else(|e| panic!("UTF-8: {e}"));
        assert!(text.contains("TYPE_AMOUNT"));
        assert!(text.contains("TYPE_BALANCESTATE"));
        assert!(text.contains("FIELD_BALANCESTATE_AMOUNT"));
        assert!(text.contains("GENERATOR_ID"));
        assert!(text.contains("SCHEMA_BYTES_HEX"));
    }

    #[test]
    fn generated_python_contains_all_expected_constants() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let python = bundle
            .files()
            .iter()
            .find(|file| file.path() == "python/codegen_fixture.py")
            .unwrap_or_else(|| panic!("Python output missing"));
        let text = core::str::from_utf8(python.bytes()).unwrap_or_else(|e| panic!("UTF-8: {e}"));
        assert!(text.contains("TYPE_AMOUNT"));
        assert!(text.contains("TYPE_BALANCESTATE"));
        assert!(text.contains("FIELD_BALANCESTATE_AMOUNT"));
        assert!(text.contains("GENERATOR_ID"));
    }

    #[test]
    fn generated_manifest_contains_schema_hash_and_files() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let manifest = bundle
            .files()
            .iter()
            .find(|file| file.path() == "MANIFEST.zfcis")
            .unwrap_or_else(|| panic!("Manifest missing"));
        let text = core::str::from_utf8(manifest.bytes()).unwrap_or_else(|e| panic!("UTF-8: {e}"));
        assert!(text.contains("generator=zeno-fcis-codegen/1"));
        assert!(text.contains("schema_hash="));
        assert!(text.contains("file=python/codegen_fixture.py"));
        assert!(text.contains("file=rust/codegen_fixture.rs"));
        assert!(text.contains("file=schema.zcve"));
        assert!(text.contains("sha256="));
    }

    #[test]
    fn generated_schema_bytes_match_canonical_encoding() {
        let schema = fixture_schema();
        let expected_bytes = schema
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("canonical bytes: {e}"));
        let bundle = match generate(&schema, &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let schema_file = bundle
            .files()
            .iter()
            .find(|file| file.path() == "schema.zcve")
            .unwrap_or_else(|| panic!("Schema file missing"));
        assert_eq!(schema_file.bytes(), expected_bytes.as_slice());
    }
}
