//! Python replay integration test.
//!
//! Runs the generated Python fixture's `replay()` function via `python3` and
//! asserts that all vectors replay with the expected decode outcomes. The
//! Python files are generated at build time by `build.rs` into the `python/`
//! directory next to this crate's `Cargo.toml`.

use std::env;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn python_vector_replay_succeeds() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR not set")),
    );
    let python_dir = manifest_dir.join("python");
    let module_path = python_dir.join("codegen_fixture.py");
    assert!(
        module_path.exists(),
        "generated python fixture not found at {module_path:?}"
    );

    let output = Command::new("python3")
        .arg(&module_path)
        .current_dir(&python_dir)
        .output()
        .unwrap_or_else(|_| panic!("failed to run python3"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "python replay failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("vectors replayed:"),
        "missing replay confirmation in stdout: {stdout}"
    );
}
