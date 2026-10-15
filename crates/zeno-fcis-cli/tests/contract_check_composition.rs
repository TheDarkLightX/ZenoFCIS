//! `zeno-fcis contract check-composition`: the committed two-register ring
//! is checked through the binary with the same bytes on repeat, a changed
//! leaf file is refused against its pin, a written receipt replays and a
//! tampered one does not, the generated aggregate is written and validated
//! and a changed one refused, the ring under a root group is checked with
//! its claim, and the command is described with its effects.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-composition-{label}-{}-{}",
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

fn ring() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2")
}

fn zeno(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn check(manifest: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "check-composition"])
        .arg(manifest)
        .args(["--format", "json"])
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|error| panic!("JSON: {error}: {}", String::from_utf8_lossy(bytes)))
}

#[test]
fn the_committed_ring_is_checked_with_the_same_bytes_on_repeat() {
    let manifest = ring().join("composition.json");
    let first = check(&manifest);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let report = json(&first.stdout);
    assert_eq!(report["status"], "checked");
    assert_eq!(report["authority"], "none");
    let result = &report["result"];
    assert_eq!(result["schema"], "zeno-fcis/checked-composition-result/1");
    assert_eq!(result["evidence"], "checked");
    assert_eq!(result["totals"]["local_rows"], 640);
    assert_eq!(result["totals"]["interface_rows"], 80);
    assert_eq!(result["cycles"], json!([[1, 2]]));
    let second = check(&manifest);
    assert_eq!(first.stdout, second.stdout);
}

/// A copy of the committed ring under `root`.
fn copy_ring(root: &Path) {
    for leaf in ["register-0", "register-1"] {
        fs::create_dir_all(root.join(leaf).join("v2")).unwrap_or_else(|error| panic!("{error}"));
        for file in ["project.zeno", "v2/policy.json"] {
            fs::copy(ring().join(leaf).join(file), root.join(leaf).join(file))
                .unwrap_or_else(|error| panic!("copy {file}: {error}"));
        }
    }
    fs::copy(
        ring().join("composition.json"),
        root.join("composition.json"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn a_changed_leaf_file_is_refused_against_its_pin() {
    let root = TempRoot::new("changed");
    copy_ring(root.path());
    let project = root.path().join("register-1/project.zeno");
    let text = fs::read_to_string(&project).unwrap_or_else(|error| panic!("{error}"));
    fs::write(&project, text.replace("in 0..=7", "in 0..=8"))
        .unwrap_or_else(|error| panic!("{error}"));
    let output = check(&root.path().join("composition.json"));
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output.stdout);
    assert_eq!(report["status"], "refused");
    assert_eq!(report["refusal"]["obligation"], "source binding");
    assert_eq!(report["refusal"]["stage"], "binding");
}

#[test]
fn the_command_is_described_with_its_effects() {
    let output = zeno(&["describe", "contract", "check-composition"]);
    assert_eq!(output.status.code(), Some(0));
    let description = json(&output.stdout);
    let effects = &description["command"]["effects"];
    assert_eq!(effects["classification"], "declared");
    assert_eq!(
        effects["reads"],
        json!([
            "composition-manifest",
            "application-contract",
            "optional-aggregate-contract",
            "optional-composition-receipt"
        ])
    );
    assert_eq!(
        effects["writes"],
        json!([
            "optional-aggregate-contract",
            "optional-composition-receipt"
        ])
    );
    assert_eq!(effects["executes_tools"], false);
}

fn run_with(manifest: &Path, flag: &str, receipt: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "check-composition"])
        .arg(manifest)
        .arg(flag)
        .arg(receipt)
        .args(["--format", "json"])
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

#[test]
fn a_written_receipt_replays_and_a_tampered_one_does_not() {
    let root = TempRoot::new("receipt");
    let manifest = ring().join("composition.json");
    let receipt = root.path().join("receipts/ring.json");
    let written = run_with(&manifest, "--receipt", &receipt);
    assert_eq!(
        written.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    let report = json(&written.stdout);
    assert_eq!(report["status"], "checked");
    assert_eq!(
        report["receipt_schema"],
        "zeno-fcis/checked-composition-receipt/1"
    );
    let bytes = fs::read(&receipt).unwrap_or_else(|error| panic!("{error}"));
    let document = json(&bytes);
    assert_eq!(
        document["schema"],
        "zeno-fcis/checked-composition-receipt/1"
    );
    assert_eq!(document["result"], report["result"]);

    let replayed = run_with(&manifest, "--replay", &receipt);
    assert_eq!(
        replayed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replayed.stdout)
    );
    assert_eq!(json(&replayed.stdout)["status"], "replayed");

    // A changed count is found at its path.
    let text = String::from_utf8(bytes).unwrap_or_else(|error| panic!("{error}"));
    let forged = root.path().join("forged.json");
    fs::write(
        &forged,
        text.replacen("\"local_rows\":640", "\"local_rows\":641", 1),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let refused = run_with(&manifest, "--replay", &forged);
    assert_eq!(refused.status.code(), Some(1));
    let report = json(&refused.stdout);
    assert_eq!(report["status"], "replay-mismatch");
    assert_eq!(report["replay"]["kind"], "mismatch");
    assert_eq!(report["replay"]["path"], "result.totals.local_rows");
    assert_eq!(report["replay"]["receipt"], 641);
    assert_eq!(report["replay"]["recomputed"], 640);

    // A changed leaf file is refused by the check itself.
    let copy = TempRoot::new("receipt-sources");
    copy_ring(copy.path());
    let project = copy.path().join("register-0/project.zeno");
    let source = fs::read_to_string(&project).unwrap_or_else(|error| panic!("{error}"));
    fs::write(&project, source.replace("in 0..=7", "in 0..=6"))
        .unwrap_or_else(|error| panic!("{error}"));
    let refused = run_with(&copy.path().join("composition.json"), "--replay", &receipt);
    assert_eq!(refused.status.code(), Some(1));
    let report = json(&refused.stdout);
    assert_eq!(report["status"], "refused");
    assert_eq!(report["replay"]["kind"], "check");
    assert_eq!(report["replay"]["refusal"]["obligation"], "source binding");
}

#[test]
fn receipt_and_replay_are_exclusive() {
    let root = TempRoot::new("exclusive");
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "check-composition"])
        .arg(ring().join("composition.json"))
        .arg("--receipt")
        .arg(root.path().join("a.json"))
        .arg("--replay")
        .arg(root.path().join("b.json"))
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    // The usage exit status.
    assert_eq!(output.status.code(), Some(64));
    assert!(!root.path().join("a.json").exists());
}

#[test]
fn the_written_aggregate_is_validated_and_a_changed_one_refused() {
    let root = TempRoot::new("aggregate");
    let manifest = ring().join("composition.json");
    let directory = root.path().join("aggregate");
    let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "check-composition"])
        .arg(&manifest)
        .arg("--write-aggregate")
        .arg(&directory)
        .args(["--format", "json"])
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report = json(&output.stdout);
    let assembly = &report["result"]["assembly"];
    assert_eq!(assembly["routes"], 2);
    assert_eq!(assembly["mapped_cases"], 4);
    assert!(directory.join("project.zeno").is_file());
    let validate = |aggregate: &Path| {
        Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
            .args(["contract", "check-composition"])
            .arg(&manifest)
            .arg("--aggregate")
            .arg(aggregate)
            .args(["--format", "json"])
            .output()
            .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
    };
    let again = validate(&directory);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(json(&again.stdout)["result"]["assembly"], *assembly);
    // Leaf 0's Accept also clears register 1.
    let rules = directory.join("v2/policy.json");
    let mut policy = json(&fs::read(&rules).unwrap_or_else(|error| panic!("{error}")));
    policy["cases"][1]["post"]["1001"] = json!("0");
    fs::write(
        &rules,
        serde_json::to_string_pretty(&policy).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let refused = validate(&directory);
    assert_eq!(refused.status.code(), Some(1));
    let report = json(&refused.stdout);
    assert_eq!(report["refusal"]["stage"], "assembly");
    assert_eq!(
        report["refusal"]["obligation"],
        "unselected slots unchanged"
    );
}

#[test]
fn the_ring_under_a_root_group_is_checked_with_its_claim() {
    let root = TempRoot::new("groups");
    copy_ring(root.path());
    let path = root.path().join("composition.json");
    let mut manifest = json(&fs::read(&path).unwrap_or_else(|error| panic!("{error}")));
    manifest["schema"] = json!("zeno-fcis/checked-composition/2");
    manifest["groups"] = json!([{
        "id": 10, "name": "ring", "children": [1, 2], "inputs": [],
        "exports": [{"id": 1, "leaf": 1, "view": 1}, {"id": 2, "leaf": 2, "view": 1}],
        "assumption": [],
        "guarantee": [{"id": 1, "formula": "post.100.1 + post.100.2 <= 6"}]
    }]);
    manifest["root"] = json!(10);
    fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let output = check(&path);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result = &json(&output.stdout)["result"];
    assert_eq!(
        result["manifest"]["schema"],
        "zeno-fcis/checked-composition/2"
    );
    assert_eq!(result["hierarchy"]["claim"]["kind"], "unconditional");
    assert_eq!(result["totals"]["group_guarantee_rows"], 64);
}

#[test]
fn the_aggregate_is_not_written_over_a_leaf() {
    let root = TempRoot::new("over-leaf");
    copy_ring(root.path());
    let manifest = root.path().join("composition.json");
    let leaf_project = root.path().join("register-0/project.zeno");
    let before = fs::read(&leaf_project).unwrap_or_else(|error| panic!("{error}"));
    // A leaf's own directory, and a directory that contains the leaves.
    for directory in [root.path().join("register-0"), root.path().to_path_buf()] {
        let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
            .args(["contract", "check-composition"])
            .arg(&manifest)
            .arg("--write-aggregate")
            .arg(&directory)
            .args(["--format", "json"])
            .output()
            .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
        assert_eq!(
            output.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let report = json(&output.stdout);
        assert_eq!(report["error"]["code"], "aggregate-directory-refused");
    }
    assert_eq!(
        fs::read(&leaf_project).unwrap_or_else(|error| panic!("{error}")),
        before
    );
    assert!(!root.path().join("project.zeno").exists());
}
