//! `zeno-fcis contract evolve` through the real binary: the study's
//! dispute-window change evolves an escrow application, which keeps the
//! replaced contract and the owner's review under `v2/evolutions/1/` and
//! regenerates its lineage, and a second evolution numbers the lineage on;
//! generation refuses an edited review or replaced contract; and every
//! planted pair of `tests/fixtures/contract-diff/pairs.json` that is neither
//! a rule change nor a rename is refused without a migration, with its kind
//! named and nothing written. `tests/contract_migrate.rs` checks migrations
//! and renames.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-evolve-{label}-{}-{}",
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

fn text(path: impl AsRef<Path>) -> String {
    String::from_utf8(read(path)).unwrap_or_else(|error| panic!("{error}"))
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

/// The contract files of a fixture: declarations, rules, retained
/// adoptions and, when it has them, decision examples.
fn copy_contract(base: &Path, dir: &Path) {
    for (name, bytes) in snapshot(base) {
        if name == "project.zeno"
            || name == "v2/policy.json"
            || name == "v2/schema-origin.json"
            || name == "tests/decision-examples.txt"
            || name.starts_with("v2/adoptions/")
        {
            write(dir.join(name), &bytes);
        }
    }
}

/// A contract directory at `dir`: `side["base"]` with `side["edits"]`
/// applied, as the contract-diff test writes it.
fn planted(dir: &Path, side: &Value) {
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
        let source = String::from_utf8(file.clone()).unwrap_or_else(|error| panic!("{error}"));
        *file = source.replace(&field("from"), &field("to")).into_bytes();
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

fn evolve(app: &Path, to: &Path) -> Output {
    zeno(&[
        "contract".as_ref(),
        "evolve".as_ref(),
        app.as_os_str(),
        "--to".as_ref(),
        to.as_os_str(),
        "--format".as_ref(),
        "json".as_ref(),
    ])
}

fn check(app: &Path) -> Output {
    zeno(&[
        "generate".as_ref(),
        "contract".as_ref(),
        app.as_os_str(),
        "--check".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
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

fn sha256(bytes: &[u8]) -> String {
    use zeno_fcis_codec::CommitmentHasher;
    zeno_fcis_crypto::RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn the_dispute_window_change_evolves_an_escrow_application() {
    let root = TempRoot::new("escrow");
    let app = root.path().join("app");
    copy_contract(&manifest("tests/fixtures/escrow"), &app);
    let thirty = manifest("tests/fixtures/escrow-dispute-30");
    let output = evolve(&app, &thirty);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["status"], "evolved");
    assert_eq!(report["kind"], "rule-change");
    let evolution = &report["evolution"];
    assert_eq!(evolution["ordinal"], 1);
    assert_eq!(evolution["from_version"], 1);
    assert_eq!(evolution["version"], 2);
    assert_eq!(evolution["claims"], serde_json::json!([]));
    // The review is the classifier's account, kept beside the replaced
    // contract and bound by the rules.
    let review = text(app.join("v2/evolutions/1/review.txt"));
    assert!(review.starts_with("rule-change: escrow version 1 -> escrow version 2\n"));
    assert!(review.contains(
        "variable `dispute_deadline` changed from `shipped_at + 1209600` to `shipped_at + 2592000`"
    ));
    assert!(review.contains("Genesis exactness, law 990, applies only to new stores"));
    assert_eq!(evolution["review_sha256"], sha256(review.as_bytes()));
    let lines: Vec<&str> = review.lines().collect();
    assert_eq!(report["review"], serde_json::json!(lines));
    let rules: Value = serde_json::from_slice(&read(app.join("v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        rules["evolutions"][0]["review_sha256"],
        evolution["review_sha256"]
    );
    assert_eq!(
        rules["variables"]["dispute_deadline"],
        "shipped_at + 2592000"
    );
    assert_eq!(
        read(app.join("v2/evolutions/1/project.zeno")),
        read(manifest("tests/fixtures/escrow/project.zeno"))
    );
    let kept: Value = serde_json::from_slice(&read(app.join("v2/evolutions/1/v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        kept["variables"]["dispute_deadline"],
        "shipped_at + 1209600"
    );
    assert_eq!(
        read(app.join("tests/decision-examples.txt")),
        read(thirty.join("tests/decision-examples.txt"))
    );
    // The whole lineage is generated: version 1 is kept beside version 2.
    let source = text(app.join("src/v2_contract.rs"));
    assert!(source.contains("pub const VERSION: u32 = 2;"));
    assert!(source.contains("pub mod v1;"));
    assert!(source.contains("include_str!(\"../v2/evolutions/1/review.txt\")"));
    assert!(app.join("src/v2_contract_v1.rs").is_file());
    assert!(app.join("v2/policy_v1.zcve").is_file());
    let checked = check(&app);
    assert_eq!(checked.status.code(), Some(0));
    assert_eq!(json(&checked)["status"], "current");
    assert_eq!(
        json(&checked)["summary"]["evolutions"][0]["after_version"],
        1
    );

    // Generation recomputes the review and checks the replaced contract.
    let path = app.join("v2/evolutions/1/review.txt");
    write(&path, format!("{review}edited\n").as_bytes());
    let refused = check(&app);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        json(&refused)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("differs from the review these two contracts have"),
        "{}",
        String::from_utf8_lossy(&refused.stdout)
    );
    write(&path, review.as_bytes());
    let kept_path = app.join("v2/evolutions/1/v2/policy.json");
    let kept_rules = text(&kept_path);
    write(
        &kept_path,
        kept_rules
            .replace("shipped_at + 1209600", "shipped_at + 1209601")
            .as_bytes(),
    );
    let refused = check(&app);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        json(&refused)["error"]["place"],
        "v2/policy.json evolutions[0].superseded_policy_sha256"
    );
    write(&kept_path, kept_rules.as_bytes());
    assert_eq!(check(&app).status.code(), Some(0));

    // A second evolution, to 21 days, numbers the lineage on.
    let twenty_one = root.path().join("twenty-one");
    copy_contract(&thirty, &twenty_one);
    let rules = text(twenty_one.join("v2/policy.json"));
    write(
        twenty_one.join("v2/policy.json"),
        rules
            .replace("shipped_at + 2592000", "shipped_at + 1814400")
            .as_bytes(),
    );
    fs::remove_file(twenty_one.join("tests/decision-examples.txt"))
        .unwrap_or_else(|error| panic!("{error}"));
    let output = evolve(&app, &twenty_one);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json(&output);
    assert_eq!(report["evolution"]["ordinal"], 2);
    assert_eq!(report["evolution"]["from_version"], 2);
    assert_eq!(report["evolution"]["version"], 3);
    assert!(app.join("v2/evolutions/2/review.txt").is_file());
    assert!(app.join("src/v2_contract_v2.rs").is_file());
    assert_eq!(check(&app).status.code(), Some(0));
    let rules: Value = serde_json::from_slice(&read(app.join("v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rules["evolutions"].as_array().map(Vec::len), Some(2));
    // Back to 14 days would repeat version 1's policy, which no lineage
    // may: a store could not tell the two versions apart.
    let fourteen = root.path().join("fourteen");
    copy_contract(&manifest("tests/fixtures/escrow"), &fourteen);
    let before = snapshot(&app);
    let output = evolve(&app, &fourteen);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json(&output)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("would repeat version 1"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(snapshot(&app), before);
}

#[test]
fn only_a_rule_change_or_a_rename_evolves_without_a_migration_and_a_refusal_writes_nothing() {
    let table: Value =
        serde_json::from_slice(&read(manifest("tests/fixtures/contract-diff/pairs.json")))
            .unwrap_or_else(|error| panic!("{error}"));
    let root = TempRoot::new("pairs");
    let mut refused_kinds = Vec::new();
    for (index, pair) in table["pairs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let name = pair["name"].as_str().unwrap_or_default();
        let kind = pair["kind"].as_str().unwrap_or_default();
        let (app, to) = (
            root.path().join(format!("{index}-app")),
            root.path().join(format!("{index}-to")),
        );
        planted(&app, &pair["old"]);
        planted(&to, &pair["new"]);
        let before = (snapshot(&app), snapshot(&to));
        let output = evolve(&app, &to);
        if kind == "rule-change" || kind == "rename" {
            assert_eq!(
                output.status.code(),
                Some(0),
                "{name}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert_eq!(check(&app).status.code(), Some(0), "{name}");
            continue;
        }
        assert_eq!(output.status.code(), Some(1), "{name}");
        let message = json(&output)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            message.contains(&format!("the change is classified `{kind}` (changed: "))
                && message.contains(
                    "without --migration, `contract evolve` admits a rule change or a rename"
                ),
            "{name}: {message}"
        );
        assert_eq!((snapshot(&app), snapshot(&to)), before, "{name}");
        refused_kinds.push(kind.to_owned());
    }
    for kind in [
        "identical",
        "program-successor",
        "layout-change",
        "unrelated",
    ] {
        assert!(
            refused_kinds.iter().any(|found| found == kind),
            "no refused {kind}"
        );
    }
}

#[test]
fn a_new_contract_with_adoptions_is_refused_after_classification() {
    // The adopted withdrawal queue back to the template's contract is a
    // program successor; the template's own contract to the adopted one too,
    // and the adopted one carries an adoption.
    let root = TempRoot::new("adopted");
    let app = root.path().join("app");
    copy_contract(&manifest("templates/withdrawal-queue"), &app);
    let before = snapshot(&app);
    let output = evolve(&app, &manifest("tests/fixtures/withdrawal-queue-adopted"));
    assert_eq!(output.status.code(), Some(1));
    let message = json(&output)["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        message.contains("the change is classified `program-successor`"),
        "{message}"
    );
    assert!(message.contains("contract adopt"), "{message}");
    assert_eq!(snapshot(&app), before);
}
