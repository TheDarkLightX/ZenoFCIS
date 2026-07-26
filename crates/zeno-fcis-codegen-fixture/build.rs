//! Builds the compiled generated Rust and Python fixtures from the canonical
//! codegen schema.
//!
//! Runs the deterministic generator at build time and writes the generated Rust
//! adapter file into `OUT_DIR` so the `generated` module can include it
//! verbatim. The Python adapter and ZCVE codec modules are written into a
//! `python/` subdirectory of `CARGO_MANIFEST_DIR` for replay by the Python
//! integration test.

use std::env;
use std::fs;
use std::path::PathBuf;

use zeno_fcis_bootstrap::{BootstrapLimits, BootstrapSpec, generate_project};
use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition, HashRequirement,
    ProjectCatalog, ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_codegen::{fixture_schema, fixture_spec, generate};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::TypeId;

fn main() {
    let schema = match fixture_schema() {
        Ok(value) => value,
        Err(error) => panic!("fixture schema rejected: {error}"),
    };
    let spec = match fixture_spec() {
        Ok(value) => value,
        Err(error) => panic!("fixture spec rejected: {error}"),
    };
    let bundle = match generate(&schema, &spec) {
        Ok(value) => value,
        Err(error) => panic!("fixture generation failed: {error}"),
    };

    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").unwrap_or_else(|| panic!("OUT_DIR not set")));

    let rust_file = bundle
        .files()
        .iter()
        .find(|file| file.path() == format!("rust/{}.rs", spec.rust_module()))
        .unwrap_or_else(|| panic!("generated bundle missing rust fixture"));
    let rust_path = out_dir.join("codegen_fixture.rs");
    fs::write(&rust_path, rust_file.bytes())
        .unwrap_or_else(|_| panic!("write generated rust fixture"));

    let catalog = fixture_catalog(schema);
    let bootstrap_spec = BootstrapSpec::try_new(
        "codegen-bootstrap-fixture",
        "codegen_fixture",
        "codegen_fixture",
        BootstrapLimits::default(),
    )
    .unwrap_or_else(|error| panic!("bootstrap spec rejected: {error}"));
    let bootstrap = generate_project::<RustCryptoSha256>(&catalog, &bootstrap_spec)
        .unwrap_or_else(|error| panic!("bootstrap generation failed: {error}"));
    write_bootstrap_file(
        &out_dir,
        &bootstrap,
        "rust/project.rs",
        "bootstrap_project.rs",
    );
    write_bootstrap_file(
        &out_dir,
        &bootstrap,
        "rust/runtime.rs",
        "bootstrap_runtime.rs",
    );

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR not set")),
    );
    let python_dir = manifest_dir.join("python");
    fs::create_dir_all(&python_dir).unwrap_or_else(|_| panic!("create python output dir"));

    for file in bundle.files() {
        let path = file.path();
        if let Some(file_name) = path.strip_prefix("python/") {
            let out_path = python_dir.join(file_name);
            fs::write(&out_path, file.bytes())
                .unwrap_or_else(|_| panic!("write generated python fixture {file_name}"));
        }
    }

    println!("cargo::rerun-if-changed=build.rs");
}

fn fixture_catalog(schema: zeno_fcis_schema::Schema) -> ProjectCatalog {
    let reasons = vec![
        ReasonDefinition::try_new(
            id(10),
            name("denied"),
            ReasonDisposition::Reject,
            0,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("fixture reason rejected: {error}")),
    ];
    let effects = vec![
        EffectDefinition::try_new(
            id(20),
            name("write"),
            TypeId::new(1),
            HashRequirement::Present,
            HashRequirement::Absent,
            hash(20),
        )
        .unwrap_or_else(|error| panic!("fixture effect rejected: {error}")),
    ];
    let channels = vec![
        ChannelDefinition::try_new(
            id(30),
            name("notify"),
            TypeId::new(3),
            TypeId::new(9),
            hash(30),
        )
        .unwrap_or_else(|error| panic!("fixture channel rejected: {error}")),
    ];
    let manifest = CatalogManifest::try_new::<RustCryptoSha256>(reasons, effects, channels)
        .unwrap_or_else(|error| panic!("fixture manifest rejected: {error}"));
    let mut entries = vec![
        root_entry(RegistryKind::StateType, 12, "state", 1),
        root_entry(RegistryKind::CommandType, 9, "command", 2),
        root_entry(RegistryKind::ContextType, 5, "context", 3),
    ];
    entries.extend_from_slice(manifest.registry_entries());
    let profile = ProjectProfile::try_new(
        name("codegen-fixture"),
        name("core"),
        id(100),
        1,
        id(12),
        id(9),
        id(5),
        DomainPrefix::try_new("codegen-fixture/core")
            .unwrap_or_else(|error| panic!("fixture domain rejected: {error}")),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("fixture schema hash failed: {error}")),
            precedence_hash: manifest.precedence_hash(),
            algorithm_hash: hash(40),
            codec_hash: hash(41),
            effect_registry_hash: manifest.effect_registry_hash(),
            channel_registry_hash: manifest.channel_registry_hash(),
            policy_hash: hash(42),
        },
        entries,
    )
    .unwrap_or_else(|error| panic!("fixture profile rejected: {error}"));
    ProjectCatalog::try_new::<RustCryptoSha256>(profile, schema, manifest, CatalogLimits::default())
        .unwrap_or_else(|error| panic!("fixture catalog rejected: {error}"))
}

fn write_bootstrap_file(
    out_dir: &std::path::Path,
    bundle: &zeno_fcis_bootstrap::BootstrapBundle,
    generated_path: &str,
    output_name: &str,
) {
    let file = bundle
        .files()
        .iter()
        .find(|file| file.path() == generated_path)
        .unwrap_or_else(|| panic!("bootstrap bundle missing {generated_path}"));
    fs::write(out_dir.join(output_name), file.bytes())
        .unwrap_or_else(|_| panic!("write bootstrap fixture {output_name}"));
}

fn root_entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
    RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
        .unwrap_or_else(|error| panic!("fixture registry entry rejected: {error}"))
}

fn name(value: &str) -> StableName {
    StableName::try_new(value).unwrap_or_else(|error| panic!("fixture name rejected: {error}"))
}

fn id(value: u32) -> SemanticId {
    SemanticId::try_new(value).unwrap_or_else(|error| panic!("fixture id rejected: {error}"))
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}
