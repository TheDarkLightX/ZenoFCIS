//! `zeno-fcis generate contract`: every template is current, generation into
//! a fresh directory reproduces both committed files, and drift names each
//! differing file.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const TEMPLATES: [&str; 8] = [
    "durable-counter",
    "inventory-reservation",
    "order-fulfillment",
    "account-lockout",
    "withdrawal-queue",
    "agent-treasury-guard",
    "prepared-counter",
    "compliance-gateway",
];

/// The files the generator reads, then the three it writes.
const INPUTS: [&str; 3] = ["project.zeno", "v2/policy.json", "v2/schema-origin.json"];
const OUTPUTS: [&str; 3] = ["v2/schema.zcve", "src/v2_contract.rs", "v2/policy.zcve"];

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-contract-{label}-{}-{}",
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

fn template(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(name)
}

fn contract(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["generate", "contract"])
        .arg(dir)
        .args(extra)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON report: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    let path = path.as_ref();
    fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// A fresh application directory holding only a template's inputs.
fn inputs_only(name: &str, root: &TempRoot) -> PathBuf {
    let dir = root.path().join(name);
    for file in INPUTS {
        let target = dir.join(file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {parent:?}: {error}"));
        }
        fs::write(&target, read(template(name).join(file)))
            .unwrap_or_else(|error| panic!("copy {file}: {error}"));
    }
    dir
}

#[test]
fn every_template_contract_is_current() {
    for name in TEMPLATES {
        let output = contract(&template(name), &["--check", "--format", "json"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json(&output);
        assert_eq!(report["status"], "current", "{name}");
        assert_eq!(report["drift"], serde_json::json!([]), "{name}");
        assert_eq!(report["catalog_binding"], "checked", "{name}");
        assert_eq!(report["summary"]["application"], name);
    }
}

#[test]
fn generation_reproduces_every_template_from_its_inputs() {
    let root = TempRoot::new("reproduce");
    for name in TEMPLATES {
        let dir = inputs_only(name, &root);
        let output = contract(&dir, &[]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for file in OUTPUTS {
            assert!(
                read(dir.join(file)) == read(template(name).join(file)),
                "{name}: generated {file} differs from the committed file"
            );
        }
    }
}

#[test]
fn check_names_each_drifted_file_and_changes_nothing() {
    let root = TempRoot::new("drift");
    let dir = inputs_only("durable-counter", &root);
    let missing = contract(&dir, &["--check"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr).trim_end(),
        format!(
            "contract drift in {}: v2/schema.zcve, src/v2_contract.rs, v2/policy.zcve",
            dir.display()
        )
    );
    assert!(!dir.join("src").exists(), "--check must not write");

    assert_eq!(contract(&dir, &[]).status.code(), Some(0));
    let source = dir.join("src/v2_contract.rs");
    let mut edited = read(&source);
    edited.extend_from_slice(b"// edited\n");
    fs::write(&source, &edited).unwrap_or_else(|error| panic!("edit source: {error}"));
    let drift = contract(&dir, &["--check", "--format", "json"]);
    assert_eq!(drift.status.code(), Some(1));
    assert_eq!(
        json(&drift)["drift"],
        serde_json::json!(["src/v2_contract.rs"])
    );
    assert_eq!(read(&source), edited, "--check must not repair");

    // A rule change moves both the source and the policy bytes.
    let rules = dir.join("v2/policy.json");
    let text = String::from_utf8(read(&rules)).unwrap_or_else(|error| panic!("{error}"));
    fs::write(
        &rules,
        text.replacen("\"when\": \"full\"", "\"when\": \"count == 2\"", 1),
    )
    .unwrap_or_else(|error| panic!("edit rules: {error}"));
    let drift = contract(&dir, &["--check", "--format", "json"]);
    assert_eq!(
        json(&drift)["drift"],
        serde_json::json!(["src/v2_contract.rs", "v2/policy.zcve"])
    );
    assert_eq!(contract(&dir, &[]).status.code(), Some(0));
    assert_eq!(contract(&dir, &["--check"]).status.code(), Some(0));
}

#[test]
fn invalid_rules_and_missing_files_are_reported() {
    let root = TempRoot::new("invalid");
    let dir = inputs_only("durable-counter", &root);
    let rules = dir.join("v2/policy.json");
    let text = String::from_utf8(read(&rules)).unwrap_or_else(|error| panic!("{error}"));
    fs::write(
        &rules,
        text.replacen("\"when\": \"full\"", "\"when\": \"full &&\"", 1),
    )
    .unwrap_or_else(|error| panic!("edit rules: {error}"));
    let invalid = contract(&dir, &["--format", "json"]);
    assert_eq!(invalid.status.code(), Some(1));
    let report = json(&invalid);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "contract-invalid");
    assert_eq!(report["error"]["place"], "v2/policy.json cases[1].when");
    assert!(
        !dir.join("src").exists(),
        "a refused contract writes nothing"
    );

    fs::remove_file(dir.join("project.zeno")).unwrap_or_else(|error| panic!("{error}"));
    let missing = contract(&dir, &["--format", "json"]);
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(json(&missing)["error"]["code"], "contract-read-failed");
}

/// What `new --contract` writes: the contract, its generated files, the
/// shared application source and the binding to this source tree.
const APPLICATION_FILES: [&str; 15] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "README.md",
    "project.zeno",
    "v2/policy.json",
    "v2/schema.zcve",
    "v2/policy.zcve",
    "src/v2_contract.rs",
    "src/lib.rs",
    "src/examples.rs",
    "src/session.rs",
    "src/main.rs",
    "tests/decisions.rs",
    "tests/decision-examples.txt",
];

fn new_application(dir: &Path, contract: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg("new")
        .arg(dir)
        .arg("--contract")
        .arg(contract)
        .args(extra)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn files(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap_or_else(|error| panic!("list {next:?}: {error}")) {
            let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                found.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn an_application_built_from_a_contract_is_current_and_complete() {
    let root = TempRoot::new("scaffold");
    let contract = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    let app = root.path().join("app");
    let created = new_application(&app, &contract, &[]);
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let mut expected: Vec<String> = APPLICATION_FILES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    assert_eq!(files(&app), expected);
    for name in [
        "project.zeno",
        "v2/policy.json",
        "tests/decision-examples.txt",
    ] {
        assert!(read(app.join(name)) == read(contract.join(name)), "{name}");
    }
    let manifest = String::from_utf8(read(app.join("Cargo.toml"))).unwrap_or_default();
    assert!(manifest.contains("name = \"dual-approval\""));
    assert!(manifest.contains(&format!("\"={}\"", env!("CARGO_PKG_VERSION"))));
    assert_eq!(contract_check(&app), Some(0));
    // The binding: this tree's packages, lock and toolchain.
    let tree = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .unwrap_or_else(|error| panic!("{error}"));
    let (_, patch) = manifest
        .split_once("\n[patch.crates-io]\n")
        .unwrap_or_else(|| panic!("no binding in {manifest}"));
    for name in [
        "zeno-fcis-shell-sqlite",
        "zeno-fcis-synthesis",
        "zeno-fcis-value",
    ] {
        let entry = format!(
            "{name} = {{ path = {:?} }}\n",
            tree.join("crates").join(name).display().to_string()
        );
        assert!(patch.contains(&entry), "{entry}");
    }
    assert!(!patch.contains("zeno-fcis-cli "));
    assert!(read(app.join("Cargo.lock")) == read(tree.join("Cargo.lock")));
    assert!(read(app.join("rust-toolchain.toml")) == read(tree.join("rust-toolchain.toml")));
    let readme = String::from_utf8(read(app.join("README.md"))).unwrap_or_default();
    assert!(
        readme
            .contains("```sh\ncargo test --offline\ncargo run --offline -- NEW_DATABASE_PATH\n```")
    );

    // An existing application is never overwritten, and a template and a
    // contract cannot both be chosen.
    let again = new_application(&app, &contract, &[]);
    assert_eq!(again.status.code(), Some(1));
    let both = new_application(
        &root.path().join("both"),
        &contract,
        &["--template", "durable-counter"],
    );
    assert_eq!(both.status.code(), Some(64));
    assert!(!root.path().join("both").exists());
}

#[test]
fn a_contract_without_examples_still_builds_its_application() {
    let root = TempRoot::new("no-examples");
    let contract = root.path().join("contract");
    for name in ["project.zeno", "v2/policy.json"] {
        let target = contract.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/dual-approval")
            .join(name);
        fs::write(&target, read(source)).unwrap_or_else(|error| panic!("{error}"));
    }
    let app = root.path().join("app");
    assert_eq!(new_application(&app, &contract, &[]).status.code(), Some(0));
    let examples =
        String::from_utf8(read(app.join("tests/decision-examples.txt"))).unwrap_or_default();
    assert!(examples.lines().all(|line| line.starts_with('#')));
    assert_eq!(contract_check(&app), Some(0));
}

fn contract_check(app: &Path) -> Option<i32> {
    contract(app, &["--check"]).status.code()
}

#[test]
fn a_source_tree_that_cannot_bind_the_application_is_refused_writing_nothing() {
    let root = TempRoot::new("unbound");
    let contract = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    let empty = root.path().join("not-a-tree");
    fs::create_dir(&empty).unwrap_or_else(|error| panic!("{error}"));
    for source in [root.path().join("absent"), empty] {
        let app = root.path().join("app");
        let refused = new_application(&app, &contract, &["--source", &source.to_string_lossy()]);
        assert_eq!(refused.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(stderr.contains(&source.display().to_string()), "{stderr}");
        // Nothing is written: the directory `new` created stays empty.
        assert_eq!(fs::read_dir(&app).map(Iterator::count).ok(), Some(0));
        fs::remove_dir(&app).unwrap_or_else(|error| panic!("{error}"));
    }
    // A template application is bound the same way.
    let refused = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg("new")
        .arg(root.path().join("counter"))
        .args(["--template", "durable-counter", "--source"])
        .arg(root.path().join("absent"))
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        fs::read_dir(root.path().join("counter"))
            .map(Iterator::count)
            .ok(),
        Some(0)
    );
    // The tree itself, named explicitly, binds as the default does.
    let tree = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let named = root.path().join("named");
    let bound = new_application(&named, &contract, &["--source", &tree.to_string_lossy()]);
    assert_eq!(
        bound.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&bound.stderr)
    );
    let default = root.path().join("default");
    assert_eq!(
        new_application(&default, &contract, &[]).status.code(),
        Some(0)
    );
    assert!(read(named.join("Cargo.toml")) == read(default.join("Cargo.toml")));
}
