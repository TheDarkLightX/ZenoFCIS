//! Public construction intake and effect boundaries.
#![forbid(unsafe_code)]
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "construct-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
    path
}
fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"));
    command
        .args(["construct", "--requirement"])
        .arg(root.join("bundle"))
        .arg("--owner-approval")
        .arg(root.join("approval.json"))
        .args(["--layout", "T", "--out"])
        .arg(root.join("result"));
    command
}
#[test]
fn unsupported_family_refuses_by_name_before_io() {
    let root = root();
    let output = command(&root)
        .args(["--family", "calendar"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.status.code(), Some(65));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unsupported construction family: calendar")
    );
    assert!(!root.join("result").exists());
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}
#[test]
fn existing_output_is_preserved_before_reading_requirements() {
    let root = root();
    std::fs::create_dir(root.join("result")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(root.join("result/keep"), b"keep").unwrap_or_else(|e| panic!("{e}"));
    let output = command(&root).output().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        std::fs::read(root.join("result/keep")).unwrap_or_else(|e| panic!("{e}")),
        b"keep"
    );
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}
#[test]
fn description_declares_construction_effects() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "construct"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "requirements-bundle",
        "owner-approval",
        "source-tree",
        "construction-artifacts",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert!(value.to_string().contains("\"executes_tools\":true"));
}

#[test]
fn relative_output_is_checked_as_an_existing_output() {
    let root = root();
    std::fs::create_dir(root.join("result")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir(root.join("child")).unwrap_or_else(|e| panic!("{e}"));
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(root.join("child"))
        .args([
            "construct",
            "--requirement",
            "../bundle",
            "--owner-approval",
            "../approval.json",
            "--layout",
            "I",
            "--out",
            "../result",
        ])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.status.code(), Some(3));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("output-outside-evidence"));
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn construction_respects_recorded_build_tree() {
    let root = root();
    let output = command(&root).output().unwrap_or_else(|e| panic!("{e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    if option_env!("ZENO_FCIS_BUILD_TREE") == Some("") {
        assert_eq!(output.status.code(), Some(65));
        assert!(
            stderr.contains("construction-build-tree-unrecorded"),
            "{stderr}"
        );
        assert!(!root.join("result").exists());
    } else {
        assert_eq!(output.status.code(), Some(3));
        assert!(
            !stderr.contains("construction-build-tree-unrecorded"),
            "{stderr}"
        );
    }
    std::fs::remove_dir_all(root).unwrap_or_else(|e| panic!("{e}"));
}
