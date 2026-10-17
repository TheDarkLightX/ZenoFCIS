//! `zeno-fcis contract check-composition` on composed-domain (`/3`)
//! manifests, through the binary: the ten-core ladder is proved without
//! its product, the wide coupling and a lowered total are refused before
//! anything is evaluated, every planted fault is violated or refused, a
//! read-only guard is accepted under `/3` and refused under `/1`, a receipt
//! replays byte for byte, aggregates are refused, and the joint table is
//! printed.
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
            "zeno-fcis-composed-domain-{label}-{}-{}",
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

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-contract/tests/fixtures/checked-composition-3")
}

fn run(manifest: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "check-composition"])
        .arg(manifest)
        .args(extra)
        .args(["--format", "json"])
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON: {error}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|error| panic!("{error}"));
    for entry in fs::read_dir(from).unwrap_or_else(|error| panic!("{error}")) {
        let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap_or_else(|error| panic!("{error}"));
        }
    }
}

#[test]
fn the_ten_core_ladder_is_proved_without_enumerating_its_product() {
    let output = run(&fixtures().join("ladder-10/composition.json"), &[]);
    assert_eq!(output.status.code(), Some(0), "{}", json(&output));
    let report = json(&output);
    assert_eq!(report["status"], "proved");
    assert_eq!(report["authority"], "none");
    let result = &report["result"];
    assert_eq!(result["schema"], "zeno-fcis/composed-domain-result/1");
    assert_eq!(result["verdict"], "proved");
    let pairs: u128 = result["monolithic"]["pairs"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(pairs > 1_000_000_000_000);
    assert_eq!(result["monolithic"]["enumerated"], false);
    for obligation in result["obligations"].as_array().unwrap_or(&Vec::new()) {
        assert_eq!(
            obligation["evaluated"].to_string(),
            obligation["size"].as_str().unwrap_or_default(),
            "{obligation}"
        );
    }
}

#[test]
fn the_wide_coupling_is_refused_before_evaluation() {
    let output = run(&fixtures().join("ladder-10/wide.json"), &[]);
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["status"], "refused");
    let result = &report["result"];
    assert_eq!(result["verdict"], "refused");
    assert_eq!(result["evaluated"], 0);
    assert_eq!(result["work"]["authority_decisions"], 0);
    assert_eq!(result["work"]["formula_values"], 0);
    assert_eq!(result["refusal"]["obligation"]["subject"], "wide");
    assert_eq!(result["refusal"]["factor"], "20691");
    let lowered = json(&run(&fixtures().join("ladder-10/lowered-total.json"), &[]));
    assert_eq!(lowered["result"]["refusal"]["reason"], "total-over-budget");
    assert_eq!(lowered["result"]["evaluated"], 0);
}

#[test]
fn every_planted_fault_is_violated_or_refused() {
    let faults = fixtures().join("faults");
    for (manifest, status, kind) in [
        ("coupling-false-at-genesis.json", "violated", "couple"),
        ("coupling-broken-by-step.json", "violated", "couple"),
        ("binding-out-of-domain.json", "violated", "assume"),
        ("invariant-not-preserved.json", "violated", "local"),
        ("read-only-changes.json", "violated", "local"),
    ] {
        let output = run(&faults.join(manifest), &[]);
        assert_eq!(output.status.code(), Some(1), "{manifest}");
        let report = json(&output);
        assert_eq!(report["status"], status, "{manifest}");
        assert_eq!(
            report["result"]["witness"]["obligation"]["kind"], kind,
            "{manifest}"
        );
    }
    for (manifest, obligation) in [
        ("binding-reads-later.json", "closed manifest form"),
        ("two-owners.json", "disjoint ownership"),
    ] {
        let output = run(&faults.join(manifest), &[]);
        assert_eq!(output.status.code(), Some(1), "{manifest}");
        let report = json(&output);
        assert_eq!(report["status"], "refused", "{manifest}");
        assert_eq!(report["refusal"]["obligation"], obligation, "{manifest}");
        assert!(report.get("result").is_none(), "{manifest}");
    }
    let output = run(&faults.join("ticker-tripwire.json"), &[]);
    assert_eq!(json(&output)["status"], "proved");
}

#[test]
fn a_read_only_guard_is_accepted_under_three_and_refused_under_one() {
    let guarded = fixtures().join("guarded");
    let output = run(&guarded.join("composition.json"), &[]);
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    assert_eq!(report["status"], "proved");
    assert_eq!(report["result"]["components"][0]["state"], "read-only");
    let output = run(&guarded.join("v1-guard.json"), &[]);
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["status"], "refused");
    assert_eq!(report["refusal"]["place"], "leaves[0].owns");
    assert_eq!(
        report["refusal"]["reason"],
        "must list each owned state field once, and at least one"
    );
}

#[test]
fn a_receipt_replays_byte_for_byte_and_a_changed_one_or_source_does_not() {
    let root = TempRoot::new("receipt");
    copy(&fixtures().join("guarded"), root.path());
    let manifest = root.path().join("composition.json");
    let receipt = root.path().join("receipt.json");
    let written = run(
        &manifest,
        &["--receipt", receipt.to_str().unwrap_or_default()],
    );
    assert_eq!(written.status.code(), Some(0));
    assert_eq!(
        json(&written)["receipt_schema"],
        "zeno-fcis/composed-domain-receipt/1"
    );
    let bytes = fs::read(&receipt).unwrap_or_else(|error| panic!("{error}"));
    let replayed = run(
        &manifest,
        &["--replay", receipt.to_str().unwrap_or_default()],
    );
    assert_eq!(replayed.status.code(), Some(0), "{}", json(&replayed));
    assert_eq!(json(&replayed)["status"], "replayed");
    let again = root.path().join("again.json");
    run(
        &manifest,
        &["--receipt", again.to_str().unwrap_or_default()],
    );
    assert_eq!(fs::read(&again).unwrap_or_default(), bytes);
    let tampered = root.path().join("tampered.json");
    let text = String::from_utf8(bytes.clone()).unwrap_or_default();
    fs::write(
        &tampered,
        text.replacen("\"evaluated\":40", "\"evaluated\":41", 1),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let refused = run(
        &manifest,
        &["--replay", tampered.to_str().unwrap_or_default()],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(json(&refused)["status"], "replay-mismatch");
    let policy = root.path().join("budget/v2/policy.json");
    let mut changed = fs::read_to_string(&policy).unwrap_or_default();
    changed.push('\n');
    fs::write(&policy, changed).unwrap_or_else(|error| panic!("{error}"));
    let refused = run(
        &manifest,
        &["--replay", receipt.to_str().unwrap_or_default()],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(json(&refused)["status"], "refused");
}

#[test]
fn aggregates_are_refused_for_the_composed_domain_profile() {
    let root = TempRoot::new("aggregate");
    let manifest = fixtures().join("guarded/composition.json");
    for flag in ["--aggregate", "--write-aggregate"] {
        let output = run(
            &manifest,
            &[flag, root.path().join("out").to_str().unwrap_or_default()],
        );
        assert_eq!(output.status.code(), Some(1), "{flag}");
        let report = json(&output);
        assert_eq!(
            report["error"]["code"], "composed-domain-aggregate-refused",
            "{flag}"
        );
        assert!(!root.path().join("out").exists());
    }
}

#[test]
fn the_joint_table_lists_every_reachable_pair() {
    let output = run(
        &fixtures().join("guarded/composition.json"),
        &["--joint-table", "1000"],
    );
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    assert_eq!(report["status"], "explored");
    assert_eq!(report["state_layout"][0], "budget.remaining");
    assert_eq!(report["request_layout"][1], "authorized");
    let rows = report["rows"].as_array().cloned().unwrap_or_default();
    // Budget 3, 2, 1 and 0 are reachable, each with 4 x 2 requests.
    assert_eq!(rows.len(), 32);
    assert!(rows.iter().any(|row| row["outcome"]["kind"] == "Reject"
        && row["outcome"]["component"] == "guard"));
    let refused = run(
        &fixtures().join("guarded/composition.json"),
        &["--joint-table", "10"],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(json(&refused)["error"]["code"], "joint-table-over-budget");
    let ring = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2/composition.json");
    let output = run(&ring, &["--joint-table", "10"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(json(&output)["error"]["code"], "composition-read-failed");
}

#[test]
fn a_replay_carries_the_receipts_verdict() {
    let root = TempRoot::new("replay-verdict");
    for (manifest, status) in [
        ("faults/invariant-not-preserved.json", "replayed-violated"),
        ("ladder-10/wide.json", "replayed-refused"),
    ] {
        let receipt = root.path().join("receipt.json");
        let receipt_text = receipt.to_str().unwrap_or_default();
        let written = run(&fixtures().join(manifest), &["--receipt", receipt_text]);
        assert_eq!(written.status.code(), Some(1), "{manifest}");
        let replayed = run(&fixtures().join(manifest), &["--replay", receipt_text]);
        assert_eq!(replayed.status.code(), Some(1), "{manifest}");
        assert_eq!(json(&replayed)["status"], status, "{manifest}");
    }
}

#[test]
fn the_joint_table_refuses_instead_of_failing_on_wide_domains() {
    let limits = fixtures().join("limits");
    for manifest in ["huge/composition.json", "medium/composition.json"] {
        let output = run(&limits.join(manifest), &["--joint-table", "10"]);
        assert_eq!(output.status.code(), Some(1), "{manifest}");
        assert_eq!(
            json(&output)["error"]["code"],
            "joint-table-over-budget",
            "{manifest}"
        );
    }
    for manifest in ["ovf/composition.json", "widehuge.json"] {
        let output = run(&limits.join(manifest), &["--joint-table", "10"]);
        assert_eq!(output.status.code(), Some(1), "{manifest}");
        let report = json(&output);
        assert_eq!(report["status"], "refused", "{manifest}");
        assert_eq!(
            report["refusal"]["obligation"], "complete enumeration",
            "{manifest}"
        );
    }
}

/// A flat chain of 200,000 conjuncts and parentheses nested past the
/// parser's depth are refused with a typed refusal, through the check and
/// the joint table, in a separate process: no signal, panic or abort.
#[test]
fn long_and_deeply_nested_formulas_are_refused_in_a_subprocess() {
    let root = TempRoot::new("long-formula");
    copy(&fixtures().join("guarded"), root.path());
    let path = root.path().join("composition.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap_or_default())
        .unwrap_or_else(|error| panic!("{error}"));
    let mut chain = "true && ".repeat(199_999);
    chain.push_str("true");
    let nested = format!("{}true{}", "(".repeat(100), ")".repeat(100));
    for (formula, obligation) in [
        (chain, "closed manifest form"),
        (nested, "defined predicate"),
    ] {
        let mut manifest = original.clone();
        manifest["components"][0]["participates"] = Value::from(formula);
        fs::write(&path, manifest.to_string()).unwrap_or_else(|error| panic!("{error}"));
        for extra in [&[][..], &["--joint-table", "1"][..]] {
            let output = run(&path, extra);
            assert_eq!(output.status.code(), Some(1), "{extra:?}");
            let report = json(&output);
            assert_eq!(report["status"], "refused", "{extra:?}");
            assert_eq!(report["refusal"]["obligation"], obligation, "{extra:?}");
        }
    }
}
