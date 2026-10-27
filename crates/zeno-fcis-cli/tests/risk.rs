//! Public quantitative judge packets, determinism, effects and exit codes.
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};
fn run(args: &[&str], exit: i32) -> Result<Value, Box<dyn std::error::Error>> {
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/quantitative-risk/swap-pool");
    let out = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "risk"])
        .arg(app)
        .args(args)
        .args(["--format", "json"])
        .output()?;
    assert_eq!(
        out.status.code(),
        Some(exit),
        "{} {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    Ok(serde_json::from_slice(&out.stdout)?)
}
#[test]
fn risk_cli_exact_answers_and_deterministic_packets() -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        vec!["--where", "post.100.112 == 1"],
        vec!["--states", "--steps", "3", "--where", "post.100.112 == 1"],
        vec!["--reward", "610", "--steps", "3"],
    ] {
        let first = run(&args, 0)?;
        assert_eq!(first, run(&args, 0)?);
        assert_eq!(first["schema"], "zeno-fcis/contract-risk/1");
        assert_eq!(first["status"], "checked");
        assert_eq!(first["authority"], "none");
    }
    Ok(())
}
#[test]
fn risk_cli_unknown_and_malformed_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        vec!["--reward", "610", "--steps", "65"],
        vec!["--reward", "610", "--steps", "3", "--max-work", "0"],
        vec!["--where", "post.100.112 == 1", "--max-tuples", "119"],
    ] {
        let p = run(&args, 2)?;
        assert_eq!(p["status"], "UNKNOWN");
        assert!(p["claim"].is_null());
    }
    let p = run(&["--where", "unknown == 1"], 65)?;
    assert_eq!(p["error"]["code"], "judge-input-invalid");
    let p = run(&["--reward", "9999", "--steps", "0"], 65)?;
    assert_eq!(p["error"]["code"], "judge-input-invalid");
    Ok(())
}
#[test]
fn risk_describe_declares_read_only_judge_effects() -> Result<(), Box<dyn std::error::Error>> {
    let o = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["describe", "contract", "risk"])
        .output()?;
    assert!(o.status.success());
    let p: Value = serde_json::from_slice(&o.stdout)?;
    assert_eq!(p["command"]["effects"]["classification"], "declared");
    assert_eq!(
        p["command"]["effects"]["reads"],
        json!(["application-contract"])
    );
    assert_eq!(
        p["command"]["effects"]["writes"],
        json!(["optional-judge-packet"])
    );
    Ok(())
}
#[test]
fn risk_out_packet_preserves_answer() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("risk-packet-{}.json", std::process::id()));
    let saved = run(
        &[
            "--reward",
            "610",
            "--steps",
            "3",
            "--out",
            path.to_str().ok_or("path")?,
        ],
        0,
    )?;
    assert_eq!(saved["packet_schema"], "zeno-fcis/contract-risk/1");
    let p: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(p["result"]["worst"]["value"]["numerator"], "6");
    std::fs::remove_file(path)?;
    Ok(())
}
