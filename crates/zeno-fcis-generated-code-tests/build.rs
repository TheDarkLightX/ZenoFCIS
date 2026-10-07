//! Builds the generated Rust and Python test code from the canonical
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

mod checked_catalog;
mod test_catalog;

use zeno_fcis_bootstrap::{BootstrapLimits, BootstrapSpec, generate_project};
use zeno_fcis_codegen::{fixture_schema, fixture_spec, generate};
use zeno_fcis_crypto::RustCryptoSha256;

use test_catalog::test_catalog;

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

    let catalog = test_catalog(schema);
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

    write_additional_checked_fixture(&out_dir);
    println!("cargo::rerun-if-changed=checked_catalog.rs");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=test_catalog.rs");
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

// This independently named supported workflow adds actual Authority coverage.
// It leaves the original compound fixture and its obligations intact.
fn write_additional_checked_fixture(out_dir: &std::path::Path) {
    let schema = checked_catalog::checked_schema();
    let spec = zeno_fcis_codegen::GenerationSpec::try_new(
        "additional_checked_fixture",
        "additional_checked_fixture",
    )
    .unwrap_or_else(|error| panic!("additional codegen spec: {error}"));
    let bundle =
        generate(&schema, &spec).unwrap_or_else(|error| panic!("additional codegen: {error}"));
    let rust = bundle
        .files()
        .iter()
        .find(|file| file.path() == "rust/additional_checked_fixture.rs")
        .unwrap_or_else(|| panic!("additional codegen source"));
    fs::write(out_dir.join("checked_schema.rs"), rust.bytes())
        .unwrap_or_else(|error| panic!("additional schema write: {error}"));
    let catalog = checked_catalog::checked_catalog(schema);
    let spec = BootstrapSpec::try_new(
        "additional-checked-fixture",
        "additional_checked_fixture",
        "additional_checked_fixture",
        BootstrapLimits::default(),
    )
    .unwrap_or_else(|error| panic!("additional bootstrap spec: {error}"));
    let project = generate_project::<RustCryptoSha256>(&catalog, &spec)
        .unwrap_or_else(|error| panic!("additional bootstrap: {error}"));
    write_bootstrap_file(out_dir, &project, "rust/project.rs", "checked_project.rs");
}
