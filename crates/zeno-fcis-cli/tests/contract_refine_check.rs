//! Refine-check CLI exits, read-only effects and independent semantic controls.
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/contract-refinement/reservation")
}

fn check(budget: u64) -> Output {
    let example = example();
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "refine-check"])
        .arg(example.join("abstract"))
        .arg(example.join("concrete"))
        .arg("--abstraction")
        .arg(example.join("abstraction.json"))
        .arg("--max-checks")
        .arg(budget.to_string())
        .args(["--format", "json"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn exhaustion_has_its_own_exit_and_no_refines_claim() {
    for (budget, exit, verdict) in [
        (0, 2, "inconclusive"),
        (383, 2, "inconclusive"),
        (384, 0, "refines"),
    ] {
        let output = check(budget);
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json["verdict"], verdict);
        assert_eq!(json["checked_rows"], budget);
        assert_eq!(json["authority"], "none");
    }
    assert_eq!(check(384).stdout, check(384).stdout);
    assert_eq!(check(262145).status.code(), Some(65));
}

#[test]
fn command_description_lists_reads_and_no_writes() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "contract", "refine-check"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}"));
    let effects = &value["command"]["effects"];
    assert_eq!(effects["classification"], "declared");
    assert_eq!(
        effects["reads"],
        serde_json::json!([
            "abstract-contract",
            "concrete-contract",
            "abstraction-table"
        ])
    );
    assert_eq!(effects["writes"], serde_json::json!([]));
    assert_eq!(effects["executes_tools"], false);
}

#[test]
fn independent_reference_and_fault_controls_agree_with_native_cli() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(root.join("tools/check_contract_refinement.py"))
        .arg("--cli")
        .arg(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["--controls", "--self-test"])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stub = Command::new("python3")
        .arg(root.join("tools/check_contract_refinement.py"))
        .args(["--self-test", "--stub-comparison"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(stub.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&stub.stdout).contains("disabled comparison"));
}

#[test]
fn every_invalid_raw_pair_is_exercised_by_the_native_control() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(root.join("tools/check_contract_refinement.py"))
        .arg("--cli")
        .arg(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["--all-invalid-rows"])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
