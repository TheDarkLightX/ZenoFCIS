//! `zeno-fcis contract diff` through the real binary: every planted pair of
//! `tests/fixtures/contract-diff/pairs.json`, written to new directories,
//! gets its kind and names exactly its changes; the document is the same
//! for the same two contracts wherever they are; an adoption of the
//! spend-approval fixture made with `contract export-program`, `optimize`
//! and `contract adopt` alone is a program successor that the new lineage
//! holds; and a refusal names the side and writes nothing.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
const SCHEMA: &str = "zeno-fcis/contract-diff/1";

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-diff-{label}-{}-{}",
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

fn manifest(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn write(path: impl AsRef<Path>, bytes: &[u8]) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {parent:?}: {error}"));
    }
    fs::write(path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

/// Every file under `dir`, by relative path.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("{error}")) {
            let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or_else(|error| panic!("{error}"))
                    .to_string_lossy()
                    .into_owned();
                files.insert(relative, read(&path));
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(dir, dir, &mut files);
    files
}

/// A contract directory at `dir`: the declarations, the rules and the
/// retained adoptions of `side["base"]`, with `side["edits"]` applied.
fn contract(dir: &Path, side: &Value) {
    let base = manifest(side["base"].as_str().unwrap_or_default());
    let mut files: BTreeMap<String, Vec<u8>> = snapshot(&base)
        .into_iter()
        .filter(|(name, _)| {
            name == "project.zeno" || name == "v2/policy.json" || name.starts_with("v2/adoptions/")
        })
        .collect();
    for edit in side["edits"].as_array().cloned().unwrap_or_default() {
        let field = |key: &str| edit[key].as_str().unwrap_or_default().to_owned();
        let file = files
            .get_mut(&field("file"))
            .unwrap_or_else(|| panic!("{edit}: unknown file"));
        let text = String::from_utf8(file.clone()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            text.matches(&field("from")).count() as u64,
            edit["count"].as_u64().unwrap_or(0),
            "{edit}"
        );
        *file = text.replace(&field("from"), &field("to")).into_bytes();
    }
    for (name, bytes) in files {
        write(dir.join(name), &bytes);
    }
}

fn zeno(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn diff(old: &Path, new: &Path, format: &str) -> Output {
    zeno(&[
        "contract".as_ref(),
        "diff".as_ref(),
        old.as_os_str(),
        new.as_os_str(),
        "--format".as_ref(),
        format.as_ref(),
    ])
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON report: {error}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn keys(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap_or_else(|| panic!("entries"))
        .iter()
        .map(|entry| {
            let field = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
            format!("{}:{}:{}", field("item"), field("id"), field("change"))
        })
        .collect()
}

fn pairs() -> Vec<Value> {
    let table: Value =
        serde_json::from_slice(&read(manifest("tests/fixtures/contract-diff/pairs.json")))
            .unwrap_or_else(|error| panic!("{error}"));
    table["pairs"].as_array().cloned().unwrap_or_default()
}

#[test]
fn every_planted_pair_is_classified_through_the_binary() {
    let root = TempRoot::new("pairs");
    let mut kinds = Vec::new();
    for (index, pair) in pairs().iter().enumerate() {
        let name = pair["name"].as_str().unwrap_or_default();
        let (old, new) = (
            root.path().join(format!("{index}-old")),
            root.path().join(format!("{index}-new")),
        );
        contract(&old, &pair["old"]);
        contract(&new, &pair["new"]);
        let before = (snapshot(&old), snapshot(&new));
        let output = diff(&old, &new, "json");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json(&output);
        assert_eq!(report["schema"], SCHEMA, "{name}");
        assert_eq!(report["kind"], pair["kind"], "{name}");
        for list in ["changes", "notes"] {
            if pair[list].is_array() {
                assert_eq!(json!(keys(&report[list])), pair[list], "{name}: {list}");
            }
        }
        assert_eq!(
            (snapshot(&old), snapshot(&new)),
            before,
            "{name}: diff writes nothing"
        );
        kinds.push(report["kind"].as_str().unwrap_or_default().to_owned());
    }
    for kind in [
        "identical",
        "program-successor",
        "rename",
        "layout-change",
        "rule-change",
        "unrelated",
    ] {
        assert!(kinds.iter().any(|found| found == kind), "no planted {kind}");
    }
}

#[test]
fn the_document_is_the_same_for_the_same_contracts_wherever_they_are() {
    let pairs = pairs();
    let pair = pairs
        .iter()
        .find(|pair| pair["name"] == "renamed-and-rule-changed")
        .unwrap_or_else(|| panic!("renamed-and-rule-changed"));
    let (first, second) = (TempRoot::new("here"), TempRoot::new("there"));
    let mut outputs = Vec::new();
    for root in [&first, &second] {
        let (old, new) = (root.path().join("old"), root.path().join("new"));
        contract(&old, &pair["old"]);
        contract(&new, &pair["new"]);
        for format in ["json", "human"] {
            let output = diff(&old, &new, format);
            assert_eq!(output.status.code(), Some(0));
            outputs.push((format, output.stdout));
        }
        // Again in the same place: byte-identical.
        assert_eq!(
            diff(&old, &new, "json").stdout,
            outputs[outputs.len() - 2].1
        );
    }
    assert_eq!(outputs[0], outputs[2]);
    assert_eq!(outputs[1], outputs[3]);
    let human = String::from_utf8(outputs[1].1.clone()).unwrap_or_default();
    let lines: Vec<&str> = human.lines().collect();
    assert_eq!(
        lines[..3],
        [
            "unrelated: spend-approval version 1 -> spend-approval version 1",
            "Names changed together with other parts of the contract, which no single \
             admission path takes.",
            "admission: refused: no admission path takes this change. Make the renaming one \
             version and the other change the next.",
        ]
    );
    assert!(human.contains("- state field 120 `status` was renamed `phase`\n"));
    assert!(human.contains(
        "- case 12 \"execute: tier 1 and above need the CFO\" changed: its condition \
         `action == 172 && tier >= 1 && !cfo_ok` became `action == 172 && tier >= 2 && !cfo_ok`\n"
    ));
    assert!(human.contains("not part of the contract:\n- variable `status` was renamed `phase`\n"));
    // The JSON document carries the same lines as its summary.
    let report: Value = serde_json::from_slice(&outputs[0].1).unwrap_or_default();
    assert_eq!(report["summary"], json!(lines));
}

#[test]
fn an_adoption_made_through_the_command_lines_is_a_program_successor() {
    let root = TempRoot::new("adopt");
    let (old, new) = (root.path().join("v1"), root.path().join("v2"));
    let spend = json!({"base": "tests/fixtures/spend-approval"});
    contract(&old, &spend);
    contract(&new, &spend);
    let (program, candidate, receipt) = (
        root.path().join("program.zcve"),
        root.path().join("candidate.zcve"),
        root.path().join("receipt.json"),
    );
    let exported = zeno(&[
        "contract".as_ref(),
        "export-program".as_ref(),
        new.as_os_str(),
        "--out".as_ref(),
        program.as_os_str(),
    ]);
    assert_eq!(exported.status.code(), Some(0), "{exported:?}");
    let optimized = zeno(&[
        "optimize".as_ref(),
        "--program".as_ref(),
        program.as_os_str(),
        "--candidate-out".as_ref(),
        candidate.as_os_str(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
    ]);
    assert_eq!(json(&optimized)["status"], "improved");
    let adopted = zeno(&[
        "contract".as_ref(),
        "adopt".as_ref(),
        new.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--usage".as_ref(),
        "new-version".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
    ]);
    assert_eq!(
        adopted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    let adoption = json(&adopted)["adoption"].clone();
    let report = json(&diff(&old, &new, "json"));
    assert_eq!(report["kind"], "program-successor");
    assert_eq!(report["parts"], json!(["program", "step-limit"]));
    assert_eq!(
        keys(&report["changes"]),
        ["program::changed", "limit:step:changed"]
    );
    assert_eq!(report["old"]["version"], 1);
    assert_eq!(report["new"]["version"], 2);
    assert_eq!(
        report["lineage"],
        json!({"old_in_new": 1, "receipts": [adoption["receipt_sha256"]], "new_in_old": null})
    );
    assert_eq!(report["admission"]["ready"], true);
    assert_eq!(
        report["admission"]["paths"],
        json!([{"id": "f6.1-program-successor", "feature": "F6.1", "exists_today": true,
                "when": "an F3 equivalence receipt compares the two programs"}])
    );
    assert_eq!(
        report["changes"][0]["old"],
        json!(adoption["program_nodes"]["before"])
    );
    assert_eq!(
        report["changes"][0]["new"],
        json!(adoption["program_nodes"]["after"])
    );
    // Back again: the same kind, but no store returns to an earlier version.
    let back = json(&diff(&new, &old, "json"));
    assert_eq!(back["kind"], "program-successor");
    assert_eq!(back["lineage"]["new_in_old"], 1);
    assert_eq!(back["admission"]["ready"], false);
}

#[test]
fn a_refusal_names_the_side_and_writes_nothing() {
    let root = TempRoot::new("refusals");
    let spend = json!({"base": "tests/fixtures/spend-approval"});
    let (good, bad) = (root.path().join("good"), root.path().join("bad"));
    contract(&good, &spend);
    contract(
        &bad,
        &json!({"base": "tests/fixtures/spend-approval", "edits": [{
            "file": "v2/policy.json", "from": "\"class\": \"Reject\", \"reason\": 200",
            "to": "\"class\": \"Accept\", \"reason\": 200", "count": 1
        }]}),
    );
    let before = snapshot(root.path());
    let missing = root.path().join("missing");
    let unreadable = diff(&missing, &good, "json");
    assert_eq!(unreadable.status.code(), Some(3));
    let report = json(&unreadable);
    assert_eq!(
        (&report["status"], &report["side"], &report["error"]["code"]),
        (
            &json!("error"),
            &json!("old"),
            &json!("contract-read-failed")
        )
    );
    let invalid = diff(&good, &bad, "json");
    assert_eq!(invalid.status.code(), Some(1));
    let report = json(&invalid);
    assert_eq!(report["side"], "new");
    assert_eq!(report["error"]["code"], "contract-invalid");
    assert_eq!(report["error"]["place"], "v2/policy.json cases[0].post");
    let human = diff(&good, &bad, "human");
    assert_eq!(human.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&human.stderr)
            .starts_with("new contract: v2/policy.json cases[0].post: "),
        "{human:?}"
    );
    let usage = zeno(&["contract".as_ref(), "diff".as_ref(), good.as_os_str()]);
    assert_eq!(usage.status.code(), Some(64));
    assert_eq!(
        snapshot(root.path()),
        before,
        "a refused diff writes nothing"
    );
}

#[test]
fn describe_declares_contract_diff_effects() {
    let output = zeno(&["describe".as_ref(), "contract".as_ref(), "diff".as_ref()]);
    assert_eq!(output.status.code(), Some(0));
    let command = &json(&output)["command"];
    assert_eq!(
        command["effects"],
        json!({
            "classification": "declared", "executes_tools": false, "read_only_flag": null,
            "reads": ["application-contract", "adopted-artifacts"], "writes": []
        })
    );
    let arguments: Vec<&str> = command["arguments"]
        .as_array()
        .map(|arguments| {
            arguments
                .iter()
                .filter_map(|argument| argument["id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(arguments, ["format", "help", "new", "old"]);
}
