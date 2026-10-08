//! `zeno-fcis contract evolve --migration` through the real binary: the
//! spend-approval application takes the urgent flag, a layout change, with a
//! migration admitted by forward simulation; generation simulates it again
//! and refuses an edited migration; every kind of invalid migration is
//! refused with nothing written; a rename evolves without a migration; and a
//! shortcut must agree with the composed route of two consecutive
//! migrations.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-migrate-{label}-{}-{}",
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

/// A contract directory at `dir` from a fixture's declarations, rules and
/// examples, with `project` and `rules` applied to them.
fn contract(
    dir: &Path,
    base: &str,
    project: impl FnOnce(String) -> String,
    rules: impl FnOnce(&mut Value),
) -> PathBuf {
    let base = manifest(base);
    write(
        dir.join("project.zeno"),
        project(text(base.join("project.zeno"))).as_bytes(),
    );
    let mut policy: Value = serde_json::from_slice(&read(base.join("v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    rules(&mut policy);
    write(
        dir.join("v2/policy.json"),
        serde_json::to_string_pretty(&policy)
            .unwrap_or_else(|error| panic!("{error}"))
            .as_bytes(),
    );
    write(
        dir.join("tests/decision-examples.txt"),
        &read(base.join("tests/decision-examples.txt")),
    );
    dir.to_path_buf()
}

fn zeno(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

/// A new application at `dir` from the spend-approval contract.
fn application(dir: &Path) {
    let output = zeno(&[
        "new".as_ref(),
        dir.as_os_str(),
        "--contract".as_ref(),
        manifest("tests/fixtures/spend-approval").as_os_str(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn evolve(app: &Path, to: &Path, extra: &[&Path]) -> Output {
    let mut arguments: Vec<&std::ffi::OsStr> = vec![
        "contract".as_ref(),
        "evolve".as_ref(),
        app.as_os_str(),
        "--to".as_ref(),
        to.as_os_str(),
        "--format".as_ref(),
        "json".as_ref(),
    ];
    for (index, path) in extra.iter().enumerate() {
        arguments.push(if index == 0 {
            "--migration".as_ref()
        } else {
            "--shortcut".as_ref()
        });
        arguments.push(path.as_os_str());
    }
    zeno(&arguments)
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

fn json_of(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON report: {error}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[track_caller]
fn admitted(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    json_of(output)
}

fn sha256(bytes: &[u8]) -> String {
    use zeno_fcis_codec::CommitmentHasher;
    zeno_fcis_crypto::RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A migration file at `path`.
fn migration(path: &Path, state: Value, from_version: Option<u32>) -> PathBuf {
    let mut document = json!({"schema": "zeno-fcis/migration/1"});
    if let Some(version) = from_version {
        document["from_version"] = json!(version);
    }
    document["state"] = state;
    write(
        path,
        serde_json::to_string_pretty(&document)
            .unwrap_or_else(|error| panic!("{error}"))
            .as_bytes(),
    );
    path.to_path_buf()
}

/// The four old fields carried over, and `rest`.
fn carried(rest: Value) -> Value {
    let mut state = json!({
        "120": {"from": 120}, "121": {"from": 121}, "122": {"from": 122}, "123": {"from": 123}
    });
    if let (Value::Object(fields), Value::Object(more)) = (&mut state, rest) {
        fields.extend(more);
    }
    state
}

/// `contract evolve` refuses, its message holding every word, and nothing
/// under `app` changes.
#[track_caller]
fn refused(app: &Path, to: &Path, extra: &[&Path], words: &[&str]) -> String {
    let before = snapshot(app);
    let output = evolve(app, to, extra);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let message = json_of(&output)["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    for word in words {
        assert!(message.contains(word), "{word}: {message}");
    }
    assert_eq!(snapshot(app), before, "{message}");
    message
}

#[test]
fn a_layout_change_evolves_with_a_simulated_migration_and_generation_checks_it() {
    let root = TempRoot::new("layout");
    let app = root.path().join("app");
    application(&app);
    let priority = manifest("tests/fixtures/spend-approval-priority");
    let file = priority.join("migration.json");
    let report = admitted(&evolve(&app, &priority, &[&file]));
    assert_eq!(report["kind"], "layout-change");
    let evolution = &report["evolution"];
    assert_eq!(evolution["path"], "migration");
    assert_eq!(
        (
            evolution["from_version"].as_u64(),
            evolution["version"].as_u64()
        ),
        (Some(1), Some(2))
    );
    let simulated = &evolution["migration"];
    assert_eq!(simulated["sha256"], sha256(&read(&file)));
    assert_eq!(
        (
            simulated["states"].as_u64(),
            simulated["states_satisfying_state_laws"].as_u64(),
            simulated["genesis_states"].as_u64(),
            simulated["tuples_compared"].as_u64()
        ),
        (Some(80), Some(57), Some(1), Some(14_592))
    );
    // The migration is kept beside the replaced contract, and the rules bind
    // it; the replaced version keeps its own schema.
    let kept = app.join("v2/evolutions/1");
    assert_eq!(read(kept.join("migration.json")), read(&file));
    let rules: Value = serde_json::from_slice(&read(app.join("v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rules["evolutions"][0]["kind"], "migration");
    assert_eq!(
        rules["evolutions"][0]["migration_sha256"],
        sha256(&read(&file))
    );
    assert_eq!(
        read(app.join("v2/schema_v1.zcve")),
        read(manifest(
            "tests/fixtures/spend-approval-migrated/v2/schema_v1.zcve"
        ))
    );
    assert!(text(app.join("src/v2_contract_v1.rs")).contains("../v2/schema_v1.zcve"));
    assert!(text(app.join("src/v2_contract.rs")).contains("pub const STATE_STEPS"));
    // The application is the committed fixture, file for file.
    let fixture = manifest("tests/fixtures/spend-approval-migrated");
    for (name, bytes) in snapshot(&fixture) {
        if name != "README.md" {
            assert_eq!(read(app.join(&name)), bytes, "{name}");
        }
    }
    let checked = admitted(&check(&app));
    assert_eq!(checked["summary"]["evolutions"][0]["kind"], "migration");
    assert_eq!(
        checked["summary"]["evolutions"][0]["migration"]["tuples_compared"],
        14_592
    );
    assert_eq!(admitted(&check(&fixture))["drift"], json!([]));
    // An edited migration is refused by generation.
    let edited = app.join("v2/evolutions/1/migration.json");
    let original = read(&edited);
    write(&edited, text(&edited).replace("false", "true").as_bytes());
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    let message = json_of(&output)["error"].to_string();
    assert!(message.contains("migration_sha256"), "{message}");
    // So is a rebound one, which the simulation then refuses.
    let rebound =
        text(app.join("v2/policy.json")).replace(&sha256(&original), &sha256(&read(&edited)));
    write(app.join("v2/policy.json"), rebound.as_bytes());
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    let message = json_of(&output)["error"].to_string();
    assert!(
        message.contains("does not map the old genesis state"),
        "{message}"
    );
}

#[test]
fn every_kind_of_invalid_migration_is_refused_with_nothing_written() {
    let root = TempRoot::new("refused");
    let app = root.path().join("app");
    application(&app);
    let dir = root.path();
    let priority = manifest("tests/fixtures/spend-approval-priority");
    let good = priority.join("migration.json");

    // The new contract pays the tier a new, carried field holds; the
    // migration sets that field to 0, so a tier 1 payment pays tier 0.
    let pays = contract(
        &dir.join("pays-carried"),
        "tests/fixtures/spend-approval-priority",
        |project| project.replace("field 124 100 urgent 108;", "field 124 100 paid_tier 105;"),
        |rules| {
            let variables = rules["variables"]
                .as_object_mut()
                .unwrap_or_else(|| panic!("variables"));
            variables.remove("urgent");
            variables.insert("paid_tier".to_owned(), json!("pre.100.124"));
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if case["post"].get("124").is_some() {
                    case["post"]["124"] = json!("paid_tier");
                }
                for delivery in case["outbox"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("outbox"))
                {
                    if delivery["channel"] == 300 {
                        delivery["payload"] = json!({"145": "paid_tier"});
                    }
                }
            }
            rules["genesis"]["124"] = json!(0);
        },
    );
    let zero = migration(
        &dir.join("zero.json"),
        carried(json!({"124": {"default": 0}})),
        None,
    );
    refused(&app, &pays, &[&zero], &["`deliveries` differs", "tier 1"]);

    // A request created by anyone but the clerk is refused with another
    // reason.
    let reason = contract(
        &dir.join("other-reason"),
        "tests/fixtures/spend-approval-priority",
        |project| project,
        |rules| {
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if case["rule"] == "create: only the clerk" {
                    case["reason"] = json!(204);
                }
            }
        },
    );
    refused(
        &app,
        &reason,
        &[&good],
        &[
            "`decision-class-and-reason` differs",
            "this is a behaviour change",
        ],
    );

    // The new flag set: the old genesis state does not map.
    let urgent = migration(
        &dir.join("urgent.json"),
        carried(json!({"124": {"default": true}})),
        None,
    );
    refused(
        &app,
        &priority,
        &[&urgent],
        &["does not map the old genesis state"],
    );

    // A new state law that a state version 1's laws allow breaks once
    // migrated: a pending tier 0 request with the CEO's approval. No version 1
    // commit reaches it, but a behaviour change could have kept it, so the
    // simulation must refuse it rather than leave it to the upgraded head.
    let law = contract(
        &dir.join("new-state-law"),
        "tests/fixtures/spend-approval-priority",
        |project| {
            project.replace(
                "law 501 roles_are_respected",
                "law 505 pending_tier0_has_no_ceo on commit, genesis = (post.100.120 == 151 && post.100.121 == 0) -> post.100.123 == 0;\nlaw 501 roles_are_respected",
            )
        },
        |rules| {
            rules["law_kinds"]["505"] = json!("StateInvariant");
        },
    );
    let message = refused(
        &app,
        &law,
        &[&good],
        &[
            "breaks a state law of the new contract",
            "status 151, tier 0",
            "ceo_ok 1",
        ],
    );
    assert!(!message.contains("behaviour change"), "{message}");
    // The refusal names the file given, which nothing kept.
    let output = evolve(&app, &law, &[&good]);
    assert_eq!(
        json_of(&output)["error"]["place"],
        format!("--migration {}", good.display()),
    );

    // Mistakes the compiler names before any simulation.
    let missing = migration(&dir.join("missing.json"), carried(json!({})), None);
    refused(
        &app,
        &priority,
        &[&missing],
        &["gives new state field 124 `urgent` no value"],
    );
    let dropped = migration(
        &dir.join("dropped.json"),
        json!({"120": {"from": 120}, "121": {"from": 121}, "122": {"from": 122},
               "123": {"from": 122}, "124": {"default": false}}),
        None,
    );
    refused(
        &app,
        &priority,
        &[&dropped],
        &["does not carry old state field 123"],
    );
    let short_map = migration(
        &dir.join("short-map.json"),
        carried(json!({"124": {"from": 121, "map": {"0": false, "1": true}}})),
        None,
    );
    refused(
        &app,
        &priority,
        &[&short_map],
        &["map gives no value for old value 2"],
    );
    let shortcut_file = migration(
        &dir.join("from.json"),
        carried(json!({"124": {"default": false}})),
        Some(1),
    );
    refused(&app, &priority, &[&shortcut_file], &["marks a shortcut"]);
    let unknown = dir.join("unknown.json");
    write(
        &unknown,
        br#"{"schema": "zeno-fcis/migration/1", "state": {}, "payloads": {}}"#,
    );
    refused(&app, &priority, &[&unknown], &["unknown key `payloads`"]);

    // A layout change needs a migration; a rename takes none.
    refused(
        &app,
        &priority,
        &[],
        &["classified `layout-change`", "--migration"],
    );
    let renamed = contract(
        &dir.join("renamed"),
        "tests/fixtures/spend-approval",
        |project| project.replace("field 121 100 tier 105;", "field 121 100 level 105;"),
        |_| {},
    );
    refused(
        &app,
        &renamed,
        &[&good],
        &["classified `rename`", "without --migration"],
    );

    // The escrow's input domain is above the cap: inconclusive.
    let escrow = dir.join("escrow");
    let output = zeno(&[
        "new".as_ref(),
        escrow.as_os_str(),
        "--contract".as_ref(),
        manifest("tests/fixtures/escrow").as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    let identity = migration(
        &dir.join("identity.json"),
        Value::Object(
            (110..=116)
                .map(|field: u16| (field.to_string(), json!({"from": field})))
                .collect(),
        ),
        None,
    );
    refused(
        &escrow,
        &manifest("tests/fixtures/escrow-dispute-30"),
        &[&identity],
        &[
            "more than the cap of 1048576",
            "solver evidence is not accepted",
        ],
    );
}

#[test]
fn a_rename_evolves_without_a_migration() {
    let root = TempRoot::new("rename");
    let app = root.path().join("app");
    application(&app);
    let renamed = contract(
        &root.path().join("renamed"),
        "tests/fixtures/spend-approval",
        |project| project.replace("field 121 100 tier 105;", "field 121 100 level 105;"),
        |_| {},
    );
    let report = admitted(&evolve(&app, &renamed, &[]));
    assert_eq!(report["kind"], "rename");
    assert_eq!(report["evolution"]["path"], "rename");
    let rules: Value = serde_json::from_slice(&read(app.join("v2/policy.json")))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(rules["evolutions"][0]["kind"], "rename");
    assert!(rules["evolutions"][0].get("migration_sha256").is_none());
    assert!(text(app.join("src/v2_contract.rs")).contains("(1, None)"));
    assert_eq!(
        admitted(&check(&app))["summary"]["evolutions"][0]["kind"],
        "rename"
    );
}

#[test]
fn a_shortcut_must_agree_with_the_composed_route() {
    let root = TempRoot::new("shortcut");
    let app = root.path().join("app");
    application(&app);
    let dir = root.path();
    let priority = manifest("tests/fixtures/spend-approval-priority");
    admitted(&evolve(
        &app,
        &priority,
        &[&priority.join("migration.json")],
    ));
    let escalated = contract(
        &dir.join("escalated"),
        "tests/fixtures/spend-approval-priority",
        |project| {
            project.replace(
                "field 124 100 urgent 108;",
                "field 124 100 urgent 108;\nfield 125 100 escalated 108;",
            )
        },
        |rules| {
            rules["variables"]["escalated"] = json!("pre.100.125");
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if case["post"].get("124").is_some() {
                    case["post"]["125"] = json!("escalated");
                }
            }
            rules["genesis"]["125"] = json!(false);
        },
    );
    let step = migration(
        &dir.join("step.json"),
        carried(json!({"124": {"from": 124}, "125": {"default": false}})),
        None,
    );
    let wrong = migration(
        &dir.join("wrong.json"),
        carried(json!({"124": {"default": false}, "125": {"default": true}})),
        Some(1),
    );
    refused(
        &app,
        &escalated,
        &[&step, &wrong],
        &["the shortcut disagrees with the composed route"],
    );
    let late = migration(
        &dir.join("late.json"),
        carried(json!({"124": {"default": false}, "125": {"default": false}})),
        Some(2),
    );
    refused(
        &app,
        &escalated,
        &[&step, &late],
        &["no earlier migration or rename left"],
    );
    let right = migration(
        &dir.join("right.json"),
        carried(json!({"124": {"default": false}, "125": {"default": false}})),
        Some(1),
    );
    let report = admitted(&evolve(&app, &escalated, &[&step, &right]));
    assert_eq!(report["evolution"]["version"], 3);
    assert_eq!(
        report["evolution"]["migration"]["shortcuts"],
        json!([{"from_version": 1, "tuples_compared": 14_592}])
    );
    assert_eq!(
        read(app.join("v2/evolutions/2/shortcuts/from-1.json")),
        read(&right)
    );
    admitted(&check(&app));
    // An edited shortcut is refused by generation.
    let kept = app.join("v2/evolutions/2/shortcuts/from-1.json");
    write(
        &kept,
        text(&kept)
            .replace("\"from_version\": 1", "\"from_version\": 1 ")
            .as_bytes(),
    );
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json_of(&output)["error"]
            .to_string()
            .contains("must not be edited")
    );
}
