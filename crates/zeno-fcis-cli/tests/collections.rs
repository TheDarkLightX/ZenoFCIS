//! `contract collections`: exits, packets and declared effects.

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
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error:?}"))
}

#[test]
fn collections_cli_checks_the_examples_and_refutes_a_minting_self_transfer() {
    for (name, views) in [("token-balances", 2304), ("library", 144), ("escrow", 4500)] {
        let packet = collections(&example(name), &[], 0);
        assert_eq!(packet["status"], "checked");
        assert_eq!(packet["authority"], "none");
        assert_eq!(packet["measured"]["admissible_views"], views, "{name}");
        assert!(
            packet["claim"]
                .as_str()
                .unwrap_or_else(|| panic!("expected as_str"))
                .contains("every number of entries")
        );
    }
    let dir = std::env::temp_dir().join(format!("collections-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("v2")).unwrap_or_else(|error| panic!("{error:?}"));
    let source = example("token-balances");
    std::fs::copy(source.join("project.zeno"), dir.join("project.zeno"))
        .unwrap_or_else(|error| panic!("{error:?}"));
    let mut rules: Value = serde_json::from_slice(
        &std::fs::read(source.join("v2/policy.json")).unwrap_or_else(|error| panic!("{error:?}")),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    for case in rules["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("expected as_array_mut"))
    {
        if case["rule"] == "a transfer to the same account changes nothing" {
            case["post"] = serde_json::json!({"170": "1", "171": "receiver_balance + amount"});
        }
    }
    std::fs::write(dir.join("v2/policy.json"), rules.to_string())
        .unwrap_or_else(|error| panic!("{error:?}"));
    let packet = collections(&dir, &[], 1);
    assert_eq!(packet["result"]["obligation"], "summary delta");
    assert!(packet["claim"].is_null());
    let packet = collections(&example("token-balances"), &["--max-tuples", "100"], 2);
    assert_eq!(packet["status"], "UNKNOWN");
    std::fs::remove_dir_all(dir).unwrap_or_else(|error| panic!("{error:?}"));
}

#[test]
fn collections_cli_declares_its_effects() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "contract", "collections"])
        .output()
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(output.status.success());
    let result: Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error:?}"));
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

#[test]
fn map_transition_cli_matches_scalar_vacuity_and_undefined()
-> Result<(), Box<dyn std::error::Error>> {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/collections/deletion");
    let original = std::fs::read_to_string(root.join("project.zeno"))?;
    let project = original
        .lines()
        .filter(|line| !line.starts_with("law 500"))
        .collect::<Vec<_>>()
        .join("\n");
    for reject in [false, true] {
        let mut rules: Value =
            serde_json::from_slice(&std::fs::read(root.join("v2/policy.json"))?)?;
        rules["law_kinds"]
            .as_object_mut()
            .ok_or("law_kinds")?
            .remove("500");
        rules["cases"] = serde_json::json!([{"rule":"only","when":"true",
            "class":if reject {"Reject"} else {"Accept"},
            "reason":if reject {Some(200)} else {None},"post":{},"outbox":[]}]);
        let project = if reject {
            format!("{project}\nreason 200 no precedence 0;\n")
        } else {
            project.clone()
        };
        let scalar = zeno_fcis_spec::expand_tables(&project, Default::default())
            .map_err(|e| e.to_string())?
            .text;
        let formula = if reject {
            "false"
        } else {
            "div_floor(1,0) == 0"
        };
        let expected = if reject { "vacuous" } else { "undefined" };
        for (name, project) in [("map", project.as_str()), ("scalar", scalar.as_str())] {
            let directory = std::env::temp_dir().join(format!(
                "collection-lemma-{name}-{reject}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(directory.join("v2"))?;
            std::fs::write(directory.join("project.zeno"), project)?;
            std::fs::write(directory.join("v2/policy.json"), rules.to_string())?;
            let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
                .args(["--no-kb", "contract", "lemma"])
                .arg(&directory)
                .args(["--transition", "--lemma", formula])
                .output()?;
            assert_eq!(
                output.status.code(),
                Some(2),
                "{name}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let packet: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(packet["status"], "inconclusive", "{packet}");
            assert_eq!(packet["lemmas"][0]["verdict"], expected, "{packet}");
            assert!(packet["lemmas"][0]["claim"].is_null());
            std::fs::remove_dir_all(directory)?;
        }
    }
    Ok(())
}
