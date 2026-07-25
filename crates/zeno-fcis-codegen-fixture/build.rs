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

use zeno_fcis_codegen::{fixture_schema, fixture_spec, generate};

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
