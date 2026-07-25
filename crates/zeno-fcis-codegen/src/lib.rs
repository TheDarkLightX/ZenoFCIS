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
}
