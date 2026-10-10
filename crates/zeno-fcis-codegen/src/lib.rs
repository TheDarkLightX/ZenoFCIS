//! Deterministic, inspectable generation from closed ZenoFCIS schemas.
//!
//! The generator emits ordinary Rust and Python constants, typed domain
//! adapters with strict `to_value`/`try_from_value` conversions, typed patch
//! path constructors, positive and negative codec vectors, exact canonical
//! schema bytes, and a content-addressed manifest. It also exposes deliberately
//! bounded Solidity and chain-neutral on-chain FCIS generators. Generated
//! artifacts contain no hidden procedural macros and can be diffed or
//! regenerated independently.

#![forbid(unsafe_code)]

mod adapters;
mod fixture;
mod model;
mod onchain;
mod python;
mod render;
mod root_envelope;
mod solana_anchor;
mod solidity;
mod solidity_advanced;
mod vectors;

pub use fixture::{fixture_schema, fixture_spec};
pub use model::{
    CodegenError, FORMATTER_ID, GeneratedBundle, GeneratedFile, GenerationSpec, VectorKind,
};
pub use onchain::{
    GeneratedOnchainBundle, GeneratedOnchainFile, MAX_ONCHAIN_CAPABILITIES,
    MAX_ONCHAIN_EVENT_FIELDS, MAX_ONCHAIN_EVENTS, MAX_ONCHAIN_FIELDS, MAX_ONCHAIN_PLAN_SLOTS,
    MAX_ONCHAIN_REASONS, ONCHAIN_MACHINE_FORMAT_VERSION, ObservationPolicy, OnchainCapability,
    OnchainCapabilityKind, OnchainEvent, OnchainField, OnchainListKind, OnchainMachineSpec,
    OnchainModelError, OnchainReason, OnchainScalar, RecipientPolicy,
};
pub use render::{GENERATOR_ID, generate};
pub use solana_anchor::{
    ANCHOR_VERSION, MAX_SOLANA_GENERATED_FILE_BYTES, SOLANA_ANCHOR_GENERATOR_ID,
    SOLANA_SHA256_HASHER_VERSION, SOLANA_TOOLCHAIN_VERSION, SolanaAnchorSpec,
    SolanaCoreSafetyFindingKind, SolanaFungibleBinding, SolanaSafetyFinding, SolanaSafetyReport,
    SolanaShellSafetyFindingKind, SolanaTokenProgram, generate_solana_anchor,
    inspect_solana_core_source, inspect_solana_shell_source,
};
pub use solidity::{
    GeneratedSolidity, MAX_SOLIDITY_FIELDS, MAX_SOLIDITY_REASONS, MAX_SOLIDITY_SOURCE_BYTES,
    SOLIDITY_GENERATOR_ID, SOLIDITY_PRAGMA, SolidityContractSpec, SolidityField,
    SolidityGenerationError, SolidityListKind, SoliditySafetyFinding, SoliditySafetyFindingKind,
    SoliditySafetyReport, SolidityScalar, generate_solidity, inspect_solidity_source,
};
pub use solidity_advanced::{
    OPENZEPPELIN_CONTRACTS_VERSION, SOLIDITY_ADVANCED_GENERATOR_ID, SOLIDITY_PINNED_VERSION,
    SolidityAdvancedSpec, SolidityFungibleBinding, generate_advanced_solidity,
};

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
        assert!(text.contains("pub fn zfcis_schema() -> Result<Schema, SchemaError>"));
        assert!(text.contains("pub fn to_root_envelope<H: CommitmentHasher>"));
        assert!(text.contains("SchemaAdmittedEnvelope::try_new::<H>"));
        assert!(text.contains("AdapterError::SchemaHashMismatch"));
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
    fn manifest_binds_schema_hash_and_vector_set_hash() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let manifest = bundle
            .files()
            .iter()
            .find(|file| file.path() == "MANIFEST.zfcis")
            .unwrap_or_else(|| panic!("manifest missing"));
        let text = core::str::from_utf8(manifest.bytes())
            .unwrap_or_else(|error| panic!("manifest not UTF-8: {error}"));
        let schema_hash_hex = format!("{}", bundle.schema_hash());
        let vector_set_hash_hex = format!("{}", bundle.vector_set_hash());
        assert!(
            text.contains(&format!("schema_hash={schema_hash_hex}")),
            "manifest must bind schema hash"
        );
        assert!(
            text.contains(&format!("vector_set_hash={vector_set_hash_hex}")),
            "manifest must bind vector set hash"
        );
    }

    #[test]
    fn generated_bundle_exposes_generator_and_formatter_identity() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        assert_eq!(bundle.generator_id(), GENERATOR_ID);
        assert_eq!(bundle.formatter_id(), FORMATTER_ID);
    }

    #[test]
    fn negative_vectors_cover_all_categories() {
        let schema = match crate::fixture_schema() {
            Ok(value) => value,
            Err(error) => panic!("canonical fixture schema failed: {error}"),
        };
        let bundle = match generate(&schema, &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let rust = bundle
            .files()
            .iter()
            .find(|file| file.path() == "rust/codegen_fixture.rs")
            .unwrap_or_else(|| panic!("rust output missing"));
        let text = core::str::from_utf8(rust.bytes())
            .unwrap_or_else(|error| panic!("rust not UTF-8: {error}"));
        assert!(
            text.contains("VectorKind::Malformed"),
            "missing malformed category"
        );
        assert!(
            text.contains("VectorKind::NonCanonical"),
            "missing noncanonical category"
        );
        assert!(
            text.contains("VectorKind::UnknownField"),
            "missing unknown-field category"
        );
        assert!(
            text.contains("VectorKind::UnknownVariant"),
            "missing unknown-variant category"
        );
        assert!(
            text.contains("VectorKind::TrailingBytes"),
            "missing trailing-bytes category"
        );
    }

    #[test]
    fn generated_python_module_contains_replay_function() {
        let bundle = match generate(&fixture_schema(), &spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let python = bundle
            .files()
            .iter()
            .find(|file| file.path() == "python/codegen_fixture.py")
            .unwrap_or_else(|| panic!("python output missing"));
        let text = core::str::from_utf8(python.bytes())
            .unwrap_or_else(|error| panic!("python not UTF-8: {error}"));
        assert!(
            text.contains("def replay()"),
            "python module must define replay()"
        );
        assert!(
            text.contains("VECTORS = ["),
            "python module must define VECTORS"
        );
    }
}
