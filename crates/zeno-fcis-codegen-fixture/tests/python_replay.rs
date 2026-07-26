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

#[test]
fn python_outgoing_schema_bounds_are_enforced() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR not set")),
    );
    let python_dir = manifest_dir.join("python");
    let script = r#"
from codegen_fixture import AdapterError, Amount, Signed, Blob, Label, Labels, Scores, ScoresEntry

def rejects(call, kind):
    try:
        call()
    except AdapterError as error:
        assert error.kind == kind, (error.kind, kind)
        return
    raise AssertionError(f"expected {kind}")

rejects(lambda: Amount(1_000_001).to_value(), "integer_range")
rejects(lambda: Signed(-1_001).to_value(), "integer_range")
rejects(lambda: Blob(bytes(33)).to_value(), "length")
rejects(lambda: Label("").to_value(), "length")
rejects(lambda: Label("é").to_value(), "non_ascii_text")
rejects(lambda: Labels([Label("a")] * 5).to_value(), "length")
rejects(lambda: Scores([ScoresEntry(Amount(i), Amount(i)) for i in range(5)]).to_value(), "length")
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .current_dir(&python_dir)
        .output()
        .unwrap_or_else(|_| panic!("failed to run python3"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "python outgoing-bound checks failed\nstdout: {stdout}\nstderr: {stderr}"
    );
}
