//! Process-level adopter journeys for the published `zeno-fcis` command.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-cli-{label}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create temp root: {error}"));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> &'static str {
    env!("CARGO_BIN_EXE_zeno-fcis")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("CLI crate is outside the repository"))
        .to_path_buf()
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("run CLI: {error}"))
}
fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read test file: {error}"))
}

fn json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("decode CLI JSON: {error}"))
}

#[test]
fn rc3_cli_mini_determinator_check_json_contract() {
    let project = repository_root().join("examples/mini-determinator/project.zeno");
    let invoke = || {
        run(Command::new(cli())
            .arg("check")
            .arg(&project)
            .args(["--format", "json"]))
    };
    let first = invoke();
    let second = invoke();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    let document = json_stdout(&first);
    assert_eq!(document["schema"], "zeno-fcis/cli/1");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["project_id"], 2);
    assert_eq!(document["components"], 2);
    assert_eq!(document["claims"], 2);
    let keys = document
        .as_object()
        .unwrap_or_else(|| panic!("valid result is not an object"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn rc3_cli_generate_check_is_read_only_and_detects_drift() {
    let root = TempRoot::new("drift");
    let project = root.path().join("project.zeno");
    let generated = root.path().join("generated");
    fs::copy(
        repository_root().join("examples/mini-determinator/project.zeno"),
        &project,
    )
    .unwrap_or_else(|error| panic!("copy project: {error}"));

    let initial = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated));
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let manifest_before = fs::read(generated.join("PROJECT_MANIFEST.zfcis"))
        .unwrap_or_else(|error| panic!("read manifest: {error}"));

    let current = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated)
        .arg("--check"));
    assert!(current.status.success());

    let tampered = b"reviewed local edit\n";
    fs::write(generated.join("generated.rs"), tampered)
        .unwrap_or_else(|error| panic!("write drift: {error}"));
    let drift = run(Command::new(cli())
        .arg("generate")
        .arg(&project)
        .arg("--out")
        .arg(&generated)
        .arg("--check"));
    assert_eq!(drift.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&drift.stderr).contains("generated drift: generated.rs"));
    assert_eq!(read(generated.join("generated.rs")), tampered);
    assert_eq!(
        read(generated.join("PROJECT_MANIFEST.zfcis")),
        manifest_before
    );
}

#[test]
fn rc3_cli_invalid_json_diagnostics_are_versioned_and_ordered() {
    let root = TempRoot::new("diagnostics");
    let project = root.path().join("project.zeno");
    fs::write(&project, "zeno 2; project 1 broken;\n")
        .unwrap_or_else(|error| panic!("write invalid project: {error}"));
    let output = run(Command::new(cli())
        .arg("check")
        .arg(&project)
        .args(["--format", "json"]));
    assert_eq!(output.status.code(), Some(1));
    let document = json_stdout(&output);
    assert_eq!(document["schema"], "zeno-fcis/cli/1");
    assert_eq!(document["status"], "invalid");
    assert_eq!(document["truncated"], false);
    assert!(
        document["diagnostics"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    let keys = document
        .as_object()
        .unwrap_or_else(|| panic!("invalid result is not an object"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn rc3_cli_new_refuses_existing_content_and_usage_is_64() {
    let root = TempRoot::new("new");
    let target = root.path().join("target");
    fs::create_dir(&target).unwrap_or_else(|error| panic!("create target: {error}"));
    let sentinel = target.join("keep.txt");
    fs::write(&sentinel, b"keep me").unwrap_or_else(|error| panic!("write sentinel: {error}"));
    let output = run(Command::new(cli())
        .arg("new")
        .arg(&target)
        .args(["--template", "minimal"]));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(read(&sentinel), b"keep me");
    assert!(!target.join("project.zeno").exists());

    let usage = run(Command::new(cli()).arg("unknown-command"));
    assert_eq!(usage.status.code(), Some(64));
}
