//! `contract collections`: exits, packets and declared effects.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/collections")
        .join(name)
}

fn collections(dir: &Path, more: &[&str], exit: i32) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "collections"])
        .arg(dir)
        .args(more)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn collections_cli_checks_the_examples_and_refutes_a_minting_self_transfer() {
    for (name, views) in [("token-balances", 2304), ("library", 160), ("escrow", 4500)] {
        let packet = collections(&example(name), &[], 0);
        assert_eq!(packet["status"], "checked");
        assert_eq!(packet["authority"], "none");
        assert_eq!(packet["measured"]["admissible_views"], views, "{name}");
        assert!(
            packet["claim"]
                .as_str()
                .unwrap()
                .contains("every number of entries")
        );
    }
    let dir = std::env::temp_dir().join(format!("collections-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("v2")).unwrap();
    let source = example("token-balances");
    std::fs::copy(source.join("project.zeno"), dir.join("project.zeno")).unwrap();
    let mut rules: Value =
        serde_json::from_slice(&std::fs::read(source.join("v2/policy.json")).unwrap()).unwrap();
    for case in rules["cases"].as_array_mut().unwrap() {
        if case["rule"] == "a transfer to the same account changes nothing" {
            case["post"] = serde_json::json!({"170": "1", "171": "receiver_balance + amount"});
        }
    }
    std::fs::write(dir.join("v2/policy.json"), rules.to_string()).unwrap();
    let packet = collections(&dir, &[], 1);
    assert_eq!(packet["result"]["obligation"], "summary delta");
    assert!(packet["claim"].is_null());
    let packet = collections(&example("token-balances"), &["--max-tuples", "100"], 2);
    assert_eq!(packet["status"], "UNKNOWN");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn collections_cli_declares_its_effects() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "contract", "collections"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let effects = &result["command"]["effects"];
    assert_eq!(
        effects["reads"],
        serde_json::json!(["application-contract"])
    );
    assert_eq!(
        effects["writes"],
        serde_json::json!(["optional-judge-packet"])
    );
    assert_eq!(effects["executes_tools"], false);
}
