//! Public command exits, witnesses, limits and deterministic packets.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn run(example: &str, more: &[&str], exit: i32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/distribution-laws")
        .join(example);
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "distribution"])
        .arg(path)
        .args(more)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[test]
fn distribution_cli_checks_refutes_and_reports_unknown_with_the_premise()
-> Result<(), Box<dyn std::error::Error>> {
    for (example, extra, exit, status) in [
        ("reporter-choice", vec![], 0, "checked"),
        ("payout-tie", vec![], 0, "checked"),
        ("modulo-bias", vec![], 1, "refuted"),
        ("reporter-choice", vec!["--max-tuples", "15"], 2, "UNKNOWN"),
        ("reporter-choice", vec!["--max-work", "31"], 2, "UNKNOWN"),
    ] {
        let first = run(example, &extra, exit)?;
        assert_eq!(first, run(example, &extra, exit)?);
        let result: Value = serde_json::from_slice(&first)?;
        assert_eq!(result["status"], status);
        assert!(
            result["premise"]
                .as_str()
                .ok_or("missing textual randomness premise")?
                .contains("uniform independent random bits")
        );
        assert_eq!(result["authority"], "none");
        if status == "refuted" {
            assert_eq!(
                result["result"]["witness"]["random_values"],
                serde_json::json!([["0"], ["3"]])
            );
        }
        if status == "UNKNOWN" {
            assert!(result["claim"].is_null());
        }
    }
    Ok(())
}

#[test]
fn distribution_cli_saved_packet_and_both_summaries_retain_the_premise()
-> Result<(), Box<dyn std::error::Error>> {
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/distribution-laws/reporter-choice");
    let out =
        std::env::temp_dir().join(format!("distribution-summary-{}.json", std::process::id()));
    for format in ["human", "json"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
            .args(["contract", "distribution"])
            .arg(&app)
            .arg("--out")
            .arg(&out)
            .args(["--format", format])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let summary = String::from_utf8(output.stdout)?;
        assert!(summary.contains("given uniform independent random bits"));
        let packet: Value = serde_json::from_slice(&std::fs::read(&out)?)?;
        assert!(
            packet["premise"]
                .as_str()
                .ok_or("missing textual randomness premise")?
                .contains("uniform independent random bits")
        );
    }
    std::fs::remove_file(out)?;
    Ok(())
}

#[test]
fn distribution_cli_declares_its_effects() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "contract", "distribution"])
        .output()?;
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let effects = &result["command"]["effects"];
    assert_eq!(effects["classification"], "declared");
    assert_eq!(
        effects["reads"],
        serde_json::json!(["application-contract"])
    );
    assert_eq!(
        effects["writes"],
        serde_json::json!(["optional-judge-packet"])
    );
    assert_eq!(effects["executes_tools"], false);
    Ok(())
}

#[test]
fn reward_cli_examples_and_budget_preserve_exact_values_and_premise()
-> Result<(), Box<dyn std::error::Error>> {
    for (example, extra, exit, status) in [
        ("lottery-reward", vec![], 0, "checked"),
        ("shortfall-reward", vec![], 0, "checked"),
        ("biased-reward", vec![], 1, "refuted"),
        ("lottery-reward", vec!["--max-work", "8"], 2, "UNKNOWN"),
    ] {
        let first = run(example, &extra, exit)?;
        assert_eq!(first, run(example, &extra, exit)?);
        let packet: Value = serde_json::from_slice(&first)?;
        assert_eq!(packet["status"], status);
        assert_eq!(packet["premise"], zeno_fcis_contract::distribution::PREMISE);
        if status == "refuted" {
            assert_eq!(
                packet["result"]["actual"],
                serde_json::json!({"numerator":"3","denominator":"4"})
            );
            assert_eq!(packet["result"]["outcomes"][0]["outcome"], "3");
        }
    }
    Ok(())
}
