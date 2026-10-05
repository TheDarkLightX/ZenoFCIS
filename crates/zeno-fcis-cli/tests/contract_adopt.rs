//! `zeno-fcis contract adopt` through the real binary: the withdrawal-queue
//! candidate becomes contract version 2 exactly as the committed fixture
//! records it, every refusal writes nothing, and an adopted contract scaffolds
//! an application carrying its whole lineage.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

/// The template's contract inputs: what an application owner writes.
const INPUTS: [&str; 3] = ["project.zeno", "v2/policy.json", "v2/schema-origin.json"];
/// Everything the fixture holds: the adopted contract, plus the template's
/// decision examples so that `new --contract` builds a checked application.
const FIXTURE_FILES: [&str; 11] = [
    "project.zeno",
    "src/v2_contract.rs",
    "src/v2_contract_v1.rs",
    "tests/decision-examples.txt",
    "v2/adoptions/1/program.zcve",
    "v2/adoptions/1/receipt.json",
    "v2/policy.json",
    "v2/policy.zcve",
    "v2/policy_v1.zcve",
    "v2/schema-origin.json",
    "v2/schema.zcve",
];
const EXAMPLES: &str = "tests/decision-examples.txt";

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-adopt-{label}-{}-{}",
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

fn template() -> PathBuf {
    manifest("templates/withdrawal-queue")
}

fn fixture() -> PathBuf {
    manifest("tests/fixtures/withdrawal-queue-adopted")
}

fn artifact(name: &str) -> PathBuf {
    manifest(&format!(
        "../../docs/benchmarks/withdrawal-queue/artifacts/{name}.zcve"
    ))
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

/// A fresh application directory holding only the template's inputs.
fn inputs_only(root: &TempRoot) -> PathBuf {
    let dir = root.path().join("app");
    for file in INPUTS {
        write(dir.join(file), &read(template().join(file)));
    }
    dir
}

fn zeno(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn adopt(dir: &Path, candidate: &Path, receipt: &Path, usage: &str) -> Output {
    zeno(&[
        "contract".as_ref(),
        "adopt".as_ref(),
        dir.as_os_str(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
        "--usage".as_ref(),
        usage.as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
    ])
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON report: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Every file under `dir` with its bytes.
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

#[test]
fn adopting_the_withdrawal_queue_candidate_reproduces_the_committed_fixture() {
    let root = TempRoot::new("fixture");
    let dir = inputs_only(&root);
    // The committed receipt is what `transform check` produces for the two
    // artifacts with this checker, so the fixture is reproducible from scratch.
    let receipt = root.path().join("receipt.json");
    let checked = zeno(&[
        "transform".as_ref(),
        "check".as_ref(),
        "--original".as_ref(),
        artifact("current-decision-scalars-original").as_os_str(),
        "--candidate".as_ref(),
        artifact("current-decision-scalars-candidate").as_os_str(),
        "--receipt".as_ref(),
        receipt.as_os_str(),
    ]);
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");
    assert_eq!(
        read(&receipt),
        read(fixture().join("v2/adoptions/1/receipt.json")),
        "the committed receipt is this checker's; regenerate the fixture after changing the checker"
    );

    let output = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &receipt,
        "new-version",
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["status"], "adopted");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["catalog_binding"], "checked");
    assert_eq!(report["adoption"]["ordinal"], 1);
    assert_eq!(report["adoption"]["version"], 2);
    assert_eq!(report["adoption"]["usage"], "new-version");
    assert_eq!(report["adoption"]["usage_preserved"], false);
    assert_eq!(
        report["adoption"]["program_nodes"],
        json!({"before": 106, "after": 100})
    );
    assert_eq!(
        report["artifacts"],
        json!([
            "v2/adoptions/1/program.zcve",
            "v2/adoptions/1/receipt.json",
            "v2/schema.zcve",
            "src/v2_contract.rs",
            "v2/policy.zcve",
            "src/v2_contract_v1.rs",
            "v2/policy_v1.zcve",
            "v2/policy.json"
        ])
    );
    assert_eq!(report["summary"]["version"], 2);
    assert_eq!(report["summary"]["program_nodes"], 100);

    let produced = snapshot(&dir);
    let mut committed = snapshot(&fixture());
    assert_eq!(
        committed.keys().cloned().collect::<Vec<_>>(),
        FIXTURE_FILES.map(str::to_owned)
    );
    assert_eq!(
        committed.remove(EXAMPLES),
        Some(read(template().join(EXAMPLES))),
        "the fixture keeps the template's examples; adoption does not write them"
    );
    for (name, bytes) in &committed {
        assert!(
            produced.get(name) == Some(bytes),
            "{name} differs from the committed fixture"
        );
    }
    assert_eq!(produced.len(), committed.len());
    // Version 1 is the template's contract reading its retained policy file.
    assert_eq!(
        String::from_utf8(produced["src/v2_contract_v1.rs"].clone()).unwrap_or_default(),
        String::from_utf8(read(template().join("src/v2_contract.rs")))
            .unwrap_or_default()
            .replace("../v2/policy.zcve", "../v2/policy_v1.zcve")
    );
    assert_eq!(
        produced["v2/policy_v1.zcve"],
        read(template().join("v2/policy.zcve"))
    );
    assert_ne!(
        produced["v2/policy.zcve"],
        read(template().join("v2/policy.zcve")),
        "the adopted program changes the policy bytes and so the identity"
    );
    // A second adoption needs a receipt against version 2; this one names version 1.
    let again = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &receipt,
        "new-version",
    );
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(
        json(&again)["error"]["place"],
        "v2/policy.json adoptions[1]"
    );
    assert_eq!(
        snapshot(&dir),
        produced,
        "a refused adoption writes nothing"
    );
}

#[test]
fn the_committed_fixture_is_current() {
    let output = zeno(&[
        "generate".as_ref(),
        "contract".as_ref(),
        fixture().as_os_str(),
        "--check".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["status"], "current");
    assert_eq!(
        report["artifacts"],
        json!([
            "v2/schema.zcve",
            "src/v2_contract.rs",
            "v2/policy.zcve",
            "src/v2_contract_v1.rs",
            "v2/policy_v1.zcve"
        ])
    );
    assert_eq!(report["summary"]["version"], 2);
    assert_eq!(
        report["summary"]["adoptions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        report["summary"]["adoptions"][0]["candidate_sha256"],
        "5df45c56b2f9d2e304bad481ccdbc2f5f3ede5f4327ab99a29efbfb3e8714e89"
    );
}

#[test]
fn preserved_is_refused_when_the_receipt_reports_different_usage() {
    let root = TempRoot::new("preserved");
    let dir = inputs_only(&root);
    let before = snapshot(&dir);
    let output = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &fixture().join("v2/adoptions/1/receipt.json"),
        "preserved",
    );
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "contract-invalid");
    assert_eq!(
        report["error"]["place"],
        "v2/policy.json adoptions[0].usage"
    );
    assert_eq!(
        report["error"]["message"],
        "the receipt reports that Step usage differs; `new-version` is required"
    );
    assert_eq!(snapshot(&dir), before, "a refused adoption writes nothing");
}

#[test]
fn an_unreplayable_receipt_or_another_candidate_is_refused_with_no_write() {
    let root = TempRoot::new("replay");
    let dir = inputs_only(&root);
    let before = snapshot(&dir);
    let committed = read(fixture().join("v2/adoptions/1/receipt.json"));
    // The receipt names the candidate; the padded program is not it.
    let other = adopt(
        &dir,
        &artifact("current-decision-scalars-padded"),
        &fixture().join("v2/adoptions/1/receipt.json"),
        "new-version",
    );
    assert_eq!(other.status.code(), Some(1));
    let report = json(&other);
    assert_eq!(report["error"]["place"], "v2/policy.json adoptions[0]");
    assert_eq!(
        report["error"]["message"],
        "v2/adoptions/1/receipt.json does not replay against version 1's program and the candidate: `candidate` differ from the record"
    );
    assert_eq!(snapshot(&dir), before);
    // An altered domain size is caught before any evaluation.
    let altered = root.path().join("altered.json");
    let text = String::from_utf8(committed.clone()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text.matches("\"size\":1296000").count(), 1);
    write(
        &altered,
        text.replace("\"size\":1296000", "\"size\":1295999")
            .as_bytes(),
    );
    let tampered = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &altered,
        "new-version",
    );
    assert_eq!(tampered.status.code(), Some(1));
    assert_eq!(
        json(&tampered)["error"]["message"],
        "v2/adoptions/1/receipt.json does not replay against version 1's program and the candidate: `domain` differ from the record"
    );
    assert_eq!(snapshot(&dir), before);
    // Not a receipt at all.
    let unreadable = root.path().join("unreadable.json");
    write(&unreadable, b"{\"schema\": \"other\"}\n");
    let refused = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &unreadable,
        "new-version",
    );
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        json(&refused)["error"]["message"],
        "v2/adoptions/1/receipt.json does not replay against version 1's program and the candidate: not a transform receipt with readable bindings"
    );
    assert_eq!(snapshot(&dir), before);
    // The retained directory of the next adoption must not exist yet.
    write(dir.join("v2/adoptions/1/note"), b"");
    let occupied = snapshot(&dir);
    let refused = adopt(
        &dir,
        &artifact("current-decision-scalars-candidate"),
        &fixture().join("v2/adoptions/1/receipt.json"),
        "new-version",
    );
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(json(&refused)["error"]["place"], "v2/adoptions/1");
    assert_eq!(snapshot(&dir), occupied);
    // A missing candidate file is a read failure.
    let missing = adopt(
        &dir,
        &root.path().join("absent.zcve"),
        &fixture().join("v2/adoptions/1/receipt.json"),
        "new-version",
    );
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(json(&missing)["error"]["code"], "contract-read-failed");
}

#[test]
fn an_adopted_contract_scaffolds_an_application_with_its_lineage() {
    let root = TempRoot::new("scaffold");
    let dir = root.path().join("app");
    let output = zeno(&[
        "new".as_ref(),
        dir.as_os_str(),
        "--contract".as_ref(),
        fixture().as_os_str(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let files = snapshot(&dir);
    let fixture_files = snapshot(&fixture());
    for (name, bytes) in &fixture_files {
        // The origin record describes the template's build; a scaffold has none.
        if name != "v2/schema-origin.json" {
            assert!(files.get(name) == Some(bytes), "{name} differs");
        }
    }
    for name in [
        "Cargo.toml",
        "README.md",
        "src/lib.rs",
        "src/session.rs",
        "src/main.rs",
        "tests/decisions.rs",
    ] {
        assert!(files.contains_key(name), "{name} missing");
    }
    assert_eq!(files.len(), fixture_files.len() - 1 + 6);
    let check = zeno(&[
        "generate".as_ref(),
        "contract".as_ref(),
        dir.as_os_str(),
        "--check".as_ref(),
    ]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn describe_declares_contract_adopt_effects() {
    let output = zeno(&["describe".as_ref(), "contract".as_ref()]);
    assert_eq!(output.status.code(), Some(0));
    let report = json(&output);
    let group = &report["command"];
    assert_eq!(group["effects"]["classification"], "command-group");
    let adopt = &group["subcommands"][0];
    assert_eq!(adopt["name"], "adopt");
    assert_eq!(
        adopt["effects"],
        json!({
            "classification": "declared", "executes_tools": false, "read_only_flag": null,
            "reads": ["application-contract", "candidate-program", "equivalence-receipt"],
            "writes": ["application-contract", "adopted-artifacts", "generated-artifacts"]
        })
    );
    let usage = adopt["arguments"]
        .as_array()
        .unwrap_or_else(|| panic!("adopt arguments"))
        .iter()
        .find(|argument| argument["id"] == "usage")
        .unwrap_or_else(|| panic!("usage argument"));
    assert_eq!(usage["required"], true);
    assert_eq!(usage["choices"], json!(["preserved", "new-version"]));
}
