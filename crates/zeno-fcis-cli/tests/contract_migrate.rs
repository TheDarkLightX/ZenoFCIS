//! `zeno-fcis contract evolve --migration` through the real binary: the
//! spend-approval application takes the urgent flag, a layout change, with a
//! migration admitted by forward simulation; generation simulates it again
//! and refuses an edited migration; every kind of invalid migration is
//! refused with nothing written; a rename evolves without a migration; and a
//! shortcut must agree with the composed route of two consecutive
//! migrations. Consumed state fields split through the same closed
//! programs; an explicit unused-field drop preserves every observation,
//! and invalid drops or programs cannot write to the application.
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
    application_from(dir, &manifest("tests/fixtures/spend-approval"));
}

fn application_from(dir: &Path, source: &Path) {
    let output = zeno(&[
        "new".as_ref(),
        dir.as_os_str(),
        "--contract".as_ref(),
        source.as_os_str(),
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

/// Split both the state's tier observation and the future payment payload;
/// the old pending payment remains a tier under its original Authority.
fn split_contract(dir: &Path) -> PathBuf {
    let source = contract(
        dir,
        "tests/fixtures/spend-approval",
        |project| {
            project
                .replace(
                    "field 123 100 ceo_ok 108;",
                    "field 123 100 ceo_ok 108;\nfield 124 100 high_tier 108;",
                )
                .replace(
                    "field 145 104 payment_tier 105;",
                    "field 145 104 tier_low 108;\nfield 147 104 tier_high 108;",
                )
        },
        |rules| {
            rules["genesis"]["124"] = json!(false);
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if let Some(tier) = case["post"]["121"].as_str() {
                    let expression = format!("{tier} >= 2");
                    case["post"]["124"] = json!(expression);
                }
                for delivery in case["outbox"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("outbox"))
                {
                    if delivery["channel"] == 300 {
                        delivery["payload"] =
                            json!({"145": "tier == 1 || tier == 3", "147": "tier >= 2"});
                    }
                }
            }
        },
    );
    write(
        source.join("tests/decision-examples.txt"),
        b"150 0 0 0 0 170 1 160 0 1 | reject 200 150 0 0 0 0 | -\n",
    );
    source
}

fn split_mapping() -> Value {
    json!({"schema": "zeno-fcis/migration/2",
    "state": carried(json!({"124": {"expr": "old.121 >= 2"}})),
    "payloads": {"300": {
        "145": {"expr": "old.145 == 1 || old.145 == 3"},
        "147": {"expr": "old.145 >= 2"}
    }}})
}

fn document(path: &Path, value: &Value) -> PathBuf {
    write(
        path,
        &serde_json::to_vec_pretty(value).unwrap_or_else(|error| panic!("{error}")),
    );
    path.to_path_buf()
}

#[test]
fn payload_and_state_splits_use_bound_programs_and_are_rechecked_on_generation() {
    let root = TempRoot::new("payload-split");
    let app = root.path().join("app");
    application(&app);
    let split = split_contract(&root.path().join("split"));
    let map = document(&root.path().join("split.json"), &split_mapping());
    let report = admitted(&evolve(&app, &split, &[&map]));
    assert_eq!(report["kind"], "layout-change");
    assert_eq!(report["evolution"]["migration"]["tuples_compared"], 14_592);
    let generated = text(app.join("src/v2_contract.rs"));
    assert!(generated.contains("pub const RECORD_STEPS"));
    assert!(generated.contains("Op::Input"));
    admitted(&check(&app));

    // Even after regenerating the file digest, a changed payload expression
    // must fail simulation before any generated source is overwritten.
    let kept = app.join("v2/evolutions/1/migration.json");
    let mut erased = split_mapping();
    erased["payloads"]["300"]["147"] = json!({"expr": "old.145 < 0"});
    document(&kept, &erased);
    let policy = app.join("v2/policy.json");
    let mut rules: Value =
        serde_json::from_slice(&read(&policy)).unwrap_or_else(|error| panic!("{error}"));
    rules["evolutions"][0]["migration_sha256"] = json!(sha256(&read(&kept)));
    document(&policy, &rules);
    let before = snapshot(&app);
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json_of(&output)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("payload")
    );
    assert_eq!(snapshot(&app), before);
}

#[test]
fn missing_partial_lossy_and_trapped_payload_maps_write_nothing() {
    let root = TempRoot::new("payload-refusals");
    let app = root.path().join("app");
    application(&app);
    let split = split_contract(&root.path().join("split"));
    let good = split_mapping();
    let mut missing = good.clone();
    missing["payloads"] = json!({});
    let mut partial = good.clone();
    partial["payloads"]["300"]["145"] =
        json!({"from": 145, "map": {"0": false, "1": true, "2": false}});
    let mut lossy = good.clone();
    lossy["payloads"]["300"]["147"] = json!({"expr": "old.145 < 0"});
    let mut trapped = good.clone();
    trapped["payloads"]["300"]["147"] =
        json!({"expr": "old.145 + 170141183460469231731687303715884105727 > 0"});
    let mut wrong = good.clone();
    wrong["payloads"]["300"]["145"] = json!({"expr": "old.145 == 0 || old.145 == 2"});
    let mut unsupported = good.clone();
    unsupported["schema"] = json!("zeno-fcis/migration/3");
    for (label, map, words) in [
        ("missing", missing, vec!["payload"]),
        ("partial", partial, vec!["map gives no value"]),
        ("lossy", lossy, vec!["payload"]),
        ("trap", trapped, vec!["payload"]),
        ("wrong-but-injective", wrong, vec!["`deliveries` differs"]),
        ("unknown-version", unsupported, vec!["migration/2"]),
    ] {
        let path = document(&root.path().join(format!("{label}.json")), &map);
        refused(&app, &split, &[&path], &words);
    }
}

#[test]
fn payload_shortcuts_match_the_entire_composed_representation_route() {
    let root = TempRoot::new("payload-shortcut");
    let app = root.path().join("app");
    // The old payload declares both Boolean values, but its actual
    // publications always emit false. Route equality must still check true.
    let original = contract(
        &root.path().join("original"),
        "tests/fixtures/spend-approval",
        |text| {
            text.replace(
                "field 145 104 payment_tier 105;",
                "field 145 104 payment_tier 105;\nfield 148 104 unused_flag 108;",
            )
        },
        |rules| {
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                for delivery in case["outbox"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("outbox"))
                {
                    if delivery["channel"] == 300 {
                        delivery["payload"]["148"] = json!(false);
                    }
                }
            }
        },
    );
    write(
        original.join("tests/decision-examples.txt"),
        b"150 0 0 0 170 1 160 0 1 | reject 200 150 0 0 0 | -\n",
    );
    let output = zeno(&[
        "new".as_ref(),
        app.as_os_str(),
        "--contract".as_ref(),
        original.as_os_str(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let split = split_contract(&root.path().join("split"));
    let second = contract(
        &root.path().join("second"),
        split.to_str().unwrap_or_else(|| panic!("path")),
        |text| {
            text.replace(
                "field 147 104 tier_high 108;",
                "field 147 104 tier_high 108;\nfield 148 104 unused_flag 108;",
            )
        },
        |rules| {
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                for delivery in case["outbox"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("outbox"))
                {
                    if delivery["channel"] == 300 {
                        delivery["payload"]["148"] = json!(false);
                    }
                }
            }
        },
    );
    let mut first_mapping = split_mapping();
    first_mapping["payloads"]["300"]["148"] = json!({"from":148});
    let first = document(&root.path().join("first.json"), &first_mapping);
    admitted(&evolve(&app, &second, &[&first]));
    let third = contract(
        &root.path().join("third"),
        second.to_str().unwrap_or_else(|| panic!("path")),
        |text| {
            text.replace(
                "field 147 104 tier_high 108;",
                "field 147 104 tier_high 108;\nfield 149 104 reserved 108;",
            )
        },
        |rules| {
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                for delivery in case["outbox"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("outbox"))
                {
                    if delivery["channel"] == 300 {
                        delivery["payload"]["149"] = json!(false);
                    }
                }
            }
        },
    );
    let next = document(
        &root.path().join("next.json"),
        &json!({"schema":"zeno-fcis/migration/2",
        "state":carried(json!({"124":{"from":124}})), "payloads":{"300":{
            "145":{"from":145}, "147":{"from":147}, "148":{"from":148}, "149":{"default":false}}}}),
    );
    let mut shortcut = first_mapping;
    shortcut["from_version"] = json!(1);
    shortcut["payloads"]["300"]["149"] = json!({"expr":"old.148"});
    let wrong = document(&root.path().join("wrong.json"), &shortcut);
    refused(
        &app,
        &third,
        &[&next, &wrong],
        &["the shortcut disagrees with the composed route", "payload"],
    );
    shortcut["payloads"]["300"]["149"] = json!({"default":false});
    let right = document(&root.path().join("right.json"), &shortcut);
    admitted(&evolve(&app, &third, &[&next, &right]));
    admitted(&check(&app));
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
    // Legacy contract/lineage bytes stay the committed fixture's. Shared
    // session scaffolding follows the current additive metadata adapter.
    let fixture = manifest("tests/fixtures/spend-approval-migrated");
    for (name, bytes) in snapshot(&fixture) {
        if name == "src/session.rs" {
            assert_eq!(
                read(app.join(&name)),
                read(manifest("contract-app/src/session.rs")),
                "{name}"
            );
        } else if name != "README.md" {
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
    let before = snapshot(&app);
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    let message = json_of(&output)["error"].to_string();
    assert!(message.contains("migration_sha256"), "{message}");
    assert_eq!(snapshot(&app), before);
    // So is a rebound one, which the simulation then refuses.
    let rebound =
        text(app.join("v2/policy.json")).replace(&sha256(&original), &sha256(&read(&edited)));
    write(app.join("v2/policy.json"), rebound.as_bytes());
    let before = snapshot(&app);
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    let message = json_of(&output)["error"].to_string();
    assert!(
        message.contains("does not map the old genesis state"),
        "{message}"
    );
    assert_eq!(snapshot(&app), before);
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
    let before = snapshot(&app);
    let output = evolve(&app, &law, &[&good]);
    assert_eq!(
        json_of(&output)["error"]["place"],
        format!("--migration {}", good.display()),
    );
    assert_eq!(snapshot(&app), before);

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
    refused(
        &app,
        &priority,
        &[&unknown],
        &["requires zeno-fcis/migration/2"],
    );

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
    let before = snapshot(&app);
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json_of(&output)["error"]
            .to_string()
            .contains("must not be edited")
    );
    assert_eq!(snapshot(&app), before);
}

fn simulation_counts(report: &Value, expected: (u64, u64, u64, u64)) {
    let simulated = &report["evolution"]["migration"];
    assert_eq!(
        (
            simulated["states"].as_u64(),
            simulated["states_satisfying_state_laws"].as_u64(),
            simulated["genesis_states"].as_u64(),
            simulated["tuples_compared"].as_u64(),
        ),
        (
            Some(expected.0),
            Some(expected.1),
            Some(expected.2),
            Some(expected.3)
        )
    );
}

/// Mechanically re-encode retained fixture labels; this does not decide a
/// policy or supply new expected outcomes.
fn mapped_examples(
    source: &str,
    old_width: usize,
    map: impl Fn(&[String]) -> Vec<String>,
) -> String {
    let mut result = String::from(
        "# Mechanical state mapping of retained source labels; no new owner labels.\n",
    );
    for line in source.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        assert_eq!(parts.len(), 3);
        let input: Vec<String> = parts[0].split_whitespace().map(str::to_owned).collect();
        let expected: Vec<String> = parts[1].split_whitespace().map(str::to_owned).collect();
        let mut mapped = map(&input[..old_width]);
        mapped.extend_from_slice(&input[old_width..]);
        let mut post = expected[..2].to_vec();
        post.extend(map(&expected[2..]));
        result.push_str(&format!(
            "{} | {} | {}\n",
            mapped.join(" "),
            post.join(" "),
            parts[2]
        ));
    }
    result
}

/// Consume tier into high/odd Boolean ranges, retaining every actual rule,
/// law and expected class/reason. The optional urgent field stays before
/// the two new bits in ID order.
fn range_contract(dir: &Path, base: &str) -> PathBuf {
    let source = contract(
        dir,
        base,
        |project| {
            let mut project = project.replace(
                "field 121 100 tier 105;",
                "field 125 100 high_tier 108;\nfield 126 100 odd_tier 108;",
            );
            // .zeno laws compare scalar paths explicitly; policy JSON below
            // supports choose(), but it is not a .zeno scalar operator.
            for (old, new) in [
                (
                    "post.100.121 >= 1",
                    "(post.100.125 == 1 || post.100.126 == 1)",
                ),
                ("post.100.121 >= 2", "post.100.125 == 1"),
                (
                    "post.100.121 == 0",
                    "(post.100.125 == 0 && post.100.126 == 0)",
                ),
                (
                    "post.100.121 == 3",
                    "(post.100.125 == 1 && post.100.126 == 1)",
                ),
            ] {
                assert_eq!(project.matches(old).count(), 1, "{old}");
                project = project.replace(old, new);
            }
            assert!(!project.contains(".100.121"));
            assert!(!project.contains("choose("));
            project
        },
        |rules| {
            rules["variables"]["tier"] =
                json!("choose(pre.100.125, 2, 0) + choose(pre.100.126, 1, 0)");
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if let Some(value) = case["post"]
                    .as_object_mut()
                    .unwrap_or_else(|| panic!("post"))
                    .remove("121")
                {
                    let expression = value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned);
                    case["post"]["125"] = json!(format!("({expression}) >= 2"));
                    case["post"]["126"] =
                        json!(format!("({expression}) == 1 || ({expression}) == 3"));
                }
            }
            let genesis = rules["genesis"]
                .as_object_mut()
                .unwrap_or_else(|| panic!("genesis"));
            assert_eq!(genesis.remove("121"), Some(json!(0)));
            genesis.insert("125".to_owned(), json!(false));
            genesis.insert("126".to_owned(), json!(false));
        },
    );
    let old_width = if base.contains("priority") { 5 } else { 4 };
    let examples = mapped_examples(
        &text(manifest(base).join("tests/decision-examples.txt")),
        old_width,
        |state| {
            let tier = state[1]
                .parse::<u8>()
                .unwrap_or_else(|error| panic!("tier: {error}"));
            let mut mapped = vec![state[0].clone()];
            mapped.extend_from_slice(&state[2..]);
            mapped.push(u8::from(tier >= 2).to_string());
            mapped.push(u8::from(tier == 1 || tier == 3).to_string());
            mapped
        },
    );
    write(
        source.join("tests/decision-examples.txt"),
        examples.as_bytes(),
    );
    source
}

fn range_mapping(priority: bool) -> Value {
    let mut state = json!({
        "120": {"from": 120}, "122": {"from": 122}, "123": {"from": 123},
        "125": {"expr": "old.121 >= 2"},
        "126": {"expr": "old.121 == 1 || old.121 == 3"}
    });
    if priority {
        state["124"] = json!({"from": 124});
    }
    json!({"schema": "zeno-fcis/migration/2", "state": state})
}

#[test]
fn shipped_order_status_is_consumed_by_a_split_and_generation_rechecks_it() {
    let root = TempRoot::new("order-split");
    let app = root.path().join("app");
    let template = manifest("templates/order-fulfillment");
    let target = manifest("tests/fixtures/order-fulfillment-split");
    application_from(&app, &template);
    let report = admitted(&evolve(&app, &target, &[&target.join("migration.json")]));
    assert_eq!(report["kind"], "layout-change");
    simulation_counts(&report, (24, 20, 1, 1_440));
    assert!(report["evolution"]["migration"].get("dropped").is_none());
    admitted(&check(&app));

    // Rebinding the digest cannot authorize a split that merges Shipped and
    // Delivered. Generation checks the same complete forward simulation.
    let kept = app.join("v2/evolutions/1/migration.json");
    let mut mapping: Value =
        serde_json::from_slice(&read(&kept)).unwrap_or_else(|error| panic!("{error}"));
    mapping["state"]["123"] = json!({"expr":
        "choose(old.120 == 164, 1, choose(old.120 == 160 || old.120 == 162, 0, choose(old.120 == 161 || old.120 == 163, 1, 2)))"});
    document(&kept, &mapping);
    let policy = app.join("v2/policy.json");
    let mut rules: Value =
        serde_json::from_slice(&read(&policy)).unwrap_or_else(|error| panic!("{error}"));
    rules["evolutions"][0]["migration_sha256"] = json!(sha256(&read(&kept)));
    document(&policy, &rules);
    let before = snapshot(&app);
    let output = check(&app);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        json_of(&output)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("differs")
    );
    assert_eq!(snapshot(&app), before);
}

#[test]
fn a_consumed_tier_splits_into_two_ranges() {
    let root = TempRoot::new("range-split");
    let app = root.path().join("app");
    application(&app);
    let target = range_contract(&root.path().join("ranges"), "tests/fixtures/spend-approval");
    let map = document(&root.path().join("ranges.json"), &range_mapping(false));
    let report = admitted(&evolve(&app, &target, &[&map]));
    assert_eq!(report["kind"], "layout-change");
    simulation_counts(&report, (80, 57, 1, 14_592));
    admitted(&check(&app));
}

#[test]
fn two_consumed_approval_fields_use_one_multi_input_split_program() {
    let root = TempRoot::new("multi-input-split");
    let app = root.path().join("app");
    application(&app);
    let target = contract(
        &root.path().join("approval-basis"),
        "tests/fixtures/spend-approval",
        |project| {
            let mut project = project
                .replace(
                    "field 122 100 cfo_ok 108;",
                    "field 124 100 approval_parity 108;",
                )
                .replace(
                    "field 123 100 ceo_ok 108;",
                    "field 125 100 ceo_approved 108;",
                );
            let kept = "post.100.122 == pre.100.122";
            assert_eq!(project.matches(kept).count(), 1);
            // CFO is parity XOR CEO. Its value is kept exactly when both
            // encoded bits are kept or both change; this remains a Boolean
            // formula rather than a comparison of parenthesized formulas.
            project = project.replace(
                kept,
                "((post.100.124 == pre.100.124 && post.100.123 == pre.100.123) || (post.100.124 != pre.100.124 && post.100.123 != pre.100.123))",
            );
            for (frame, ones, zeros, ceo_paths) in [("pre", 1, 0, 5), ("post", 2, 1, 9)] {
                let one = format!("{frame}.100.122 == 1");
                let zero = format!("{frame}.100.122 == 0");
                assert_eq!(project.matches(&one).count(), ones, "{one}");
                assert_eq!(project.matches(&zero).count(), zeros, "{zero}");
                project = project
                    .replace(&one, &format!("({frame}.100.124 != {frame}.100.123)"))
                    .replace(&zero, &format!("({frame}.100.124 == {frame}.100.123)"));
                let old_ceo = format!("{frame}.100.123");
                assert_eq!(project.matches(&old_ceo).count(), ceo_paths, "{old_ceo}");
                project = project.replace(&old_ceo, &format!("{frame}.100.125"));
            }
            assert!(!project.contains(".100.122"));
            assert!(!project.contains(".100.123"));
            project
        },
        |rules| {
            rules["variables"]["cfo_ok"] = json!("pre.100.124 != pre.100.125");
            rules["variables"]["ceo_ok"] = json!("pre.100.125");
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                let post = case["post"]
                    .as_object_mut()
                    .unwrap_or_else(|| panic!("post"));
                if let Some(cfo) = post.remove("122") {
                    let ceo = post.remove("123").unwrap_or_else(|| panic!("ceo"));
                    let expression = |value: &Value| {
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned)
                    };
                    post.insert(
                        "124".to_owned(),
                        json!(format!("({}) != ({})", expression(&cfo), expression(&ceo))),
                    );
                    post.insert("125".to_owned(), ceo);
                }
            }
            rules["genesis"] = json!({"120":150, "121":0, "124":false, "125":false});
        },
    );
    let examples = mapped_examples(
        &text(manifest(
            "tests/fixtures/spend-approval/tests/decision-examples.txt",
        )),
        4,
        |state| {
            vec![
                state[0].clone(),
                state[1].clone(),
                u8::from(state[2] != state[3]).to_string(),
                state[3].clone(),
            ]
        },
    );
    write(
        target.join("tests/decision-examples.txt"),
        examples.as_bytes(),
    );
    let map = document(
        &root.path().join("basis.json"),
        &json!({"schema":"zeno-fcis/migration/2", "state":{
            "120":{"from":120}, "121":{"from":121},
            "124":{"expr":"old.122 != old.123"}, "125":{"expr":"old.123"}
        }}),
    );
    let report = admitted(&evolve(&app, &target, &[&map]));
    simulation_counts(&report, (80, 57, 1, 14_592));
    let generated = text(app.join("src/v2_contract.rs"));
    let record = generated
        .split_once("pub const RECORD_STEPS")
        .unwrap_or_else(|| panic!("compiled migration programs"))
        .1
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(record.contains("Some((&[122,123],"));
    admitted(&check(&app));
}

fn drop_mapping() -> Value {
    json!({"schema":"zeno-fcis/migration/2", "state":carried(json!({})), "drop":[124]})
}

#[test]
fn an_explicit_unused_field_drop_is_admitted_and_named_in_the_review() {
    let root = TempRoot::new("drop");
    let app = root.path().join("app");
    application_from(&app, &manifest("tests/fixtures/spend-approval-priority"));
    let map = document(&root.path().join("drop.json"), &drop_mapping());
    let report = admitted(&evolve(
        &app,
        &manifest("tests/fixtures/spend-approval"),
        &[&map],
    ));
    assert_eq!(report["kind"], "layout-change");
    simulation_counts(&report, (160, 114, 1, 29_184));
    assert_eq!(report["evolution"]["migration"]["dropped"], json!([124]));
    let review = text(app.join("v2/evolutions/1/review.txt"));
    assert!(
        review.contains("field 124") && review.contains("urgent") && review.contains("removed")
    );
    assert_eq!(read(app.join("v2/evolutions/1/migration.json")), read(&map));
    let checked = admitted(&check(&app));
    assert_eq!(
        checked["summary"]["evolutions"][0]["migration"]["dropped"],
        json!([124])
    );
}

#[test]
fn a_drop_that_changes_a_decision_is_refused_without_writes() {
    let root = TempRoot::new("harmful-drop");
    let source = contract(
        &root.path().join("uses-urgent"),
        "tests/fixtures/spend-approval-priority",
        |project| project,
        |rules| {
            for case in rules["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("cases"))
            {
                if case["rule"] == "create: only the clerk" {
                    case["when"] = json!("action == 170 && (role != 161 || urgent)");
                }
            }
        },
    );
    let app = root.path().join("app");
    application_from(&app, &source);
    let map = document(&root.path().join("drop.json"), &drop_mapping());
    refused(
        &app,
        &manifest("tests/fixtures/spend-approval"),
        &[&map],
        &["decision-class-and-reason", "behaviour change"],
    );
}

#[test]
fn undeclared_unknown_duplicate_and_still_used_drops_write_nothing() {
    let root = TempRoot::new("drop-refusals");
    let app = root.path().join("app");
    application_from(&app, &manifest("tests/fixtures/spend-approval-priority"));
    let target = manifest("tests/fixtures/spend-approval");
    let good = drop_mapping();
    let mut undeclared = good.clone();
    undeclared
        .as_object_mut()
        .unwrap_or_else(|| panic!("mapping"))
        .remove("drop");
    let mut legacy_undeclared = undeclared.clone();
    legacy_undeclared["schema"] = json!("zeno-fcis/migration/1");
    let mut unknown = good.clone();
    unknown["drop"] = json!([124, 999]);
    let mut duplicate = good.clone();
    duplicate["drop"] = json!([124, 124]);
    let mut still_used = good.clone();
    still_used["drop"] = json!([124, 122]);
    let mut legacy = good.clone();
    legacy["schema"] = json!("zeno-fcis/migration/1");
    let mut payload_drop = good.clone();
    payload_drop["payloads"] = json!({"300":{"145":{"from":145}, "drop":[145]}});
    for (label, map, words) in [
        (
            "undeclared-v2",
            undeclared,
            vec!["does not carry old state field 124", "drop"],
        ),
        (
            "undeclared-v1",
            legacy_undeclared,
            vec!["does not carry old state field 124"],
        ),
        (
            "unknown",
            unknown,
            vec!["dropped old state field 999 does not exist"],
        ),
        (
            "duplicate",
            duplicate,
            vec!["lists dropped field 124 twice"],
        ),
        (
            "still-used",
            still_used,
            vec!["dropped field 122", "still uses it"],
        ),
        (
            "legacy-drop",
            legacy,
            vec!["requires zeno-fcis/migration/2"],
        ),
        ("payload-drop", payload_drop, vec!["not a decimal ID"]),
    ] {
        let path = document(&root.path().join(format!("{label}.json")), &map);
        refused(&app, &target, &[&path], &words);
    }
}

#[test]
fn non_closed_state_expressions_and_invalid_source_domains_write_nothing() {
    let root = TempRoot::new("state-expr-refusals");
    let app = root.path().join("app");
    application(&app);
    let target = manifest("tests/fixtures/spend-approval-priority");
    for (index, (expression, word)) in [
        ("pre.100.121 >= 2", "not old.<field ID>"),
        ("command.101.130 == 170", "not old.<field ID>"),
        ("tier", "not old.<field ID>"),
        ("old.0121 >= 2", "not a canonical old record field"),
        ("old.999 >= 2", "not a canonical old record field"),
        ("old.121 * 2 > 0", "expr"),
        ("div_floor(old.121, 2) > 0", "expr"),
        ("div_ceil(old.121, 2) > 0", "expr"),
    ]
    .into_iter()
    .enumerate()
    {
        let map = document(
            &root.path().join(format!("closed-{index}.json")),
            &json!({"schema":"zeno-fcis/migration/2",
                "state":carried(json!({"124":{"expr":expression}}))}),
        );
        refused(&app, &target, &[&map], &[word]);
    }
    // The two excluded-* programs agree on every admitted old state. Empty requests
    // with a positive tier are excluded by law 500, but the declared source
    // domain still includes them and must be checked before simulation.
    for (label, field, expression) in [
        ("range", "121", "old.121 + 1"),
        (
            "excluded-range",
            "121",
            "choose(old.120 == 150 && old.121 > 0, 4, old.121)",
        ),
        (
            "excluded-trap",
            "124",
            "choose(old.120 == 150 && old.121 > 0, 170141183460469231731687303715884105727, 0) + old.121 < 0",
        ),
        (
            "trap",
            "124",
            "old.121 + 170141183460469231731687303715884105727 < 0",
        ),
    ] {
        let mut state = carried(json!({"124":{"default":false}}));
        state[field] = json!({"expr":expression});
        let map = document(
            &root.path().join(format!("{label}.json")),
            &json!({"schema":"zeno-fcis/migration/2", "state":state}),
        );
        refused(
            &app,
            &target,
            &[&map],
            &["expression of new field", "trap or an out-of-domain result"],
        );
    }
}

#[test]
fn shortcuts_must_agree_with_a_consumed_split_then_an_explicit_drop() {
    let root = TempRoot::new("split-drop-route");
    let app = root.path().join("app");
    application_from(&app, &manifest("tests/fixtures/spend-approval-priority"));
    let split_priority = range_contract(
        &root.path().join("priority-ranges"),
        "tests/fixtures/spend-approval-priority",
    );
    let split = document(&root.path().join("split.json"), &range_mapping(true));
    let first = admitted(&evolve(&app, &split_priority, &[&split]));
    simulation_counts(&first, (160, 114, 1, 29_184));
    let target = range_contract(
        &root.path().join("base-ranges"),
        "tests/fixtures/spend-approval",
    );
    let step = document(
        &root.path().join("drop.json"),
        &json!({"schema":"zeno-fcis/migration/2", "drop":[124], "state":{
            "120":{"from":120}, "122":{"from":122}, "123":{"from":123},
            "125":{"from":125}, "126":{"from":126}
        }}),
    );
    let mut shortcut = range_mapping(false);
    shortcut["from_version"] = json!(1);
    shortcut["drop"] = json!([124]);
    let mut changed = shortcut.clone();
    changed["state"]["125"] = json!({"expr":"old.121 == 3"});
    let wrong = document(&root.path().join("wrong.json"), &changed);
    refused(
        &app,
        &target,
        &[&step, &wrong],
        &["the shortcut disagrees with the composed route"],
    );
    let right = document(&root.path().join("right.json"), &shortcut);
    let report = admitted(&evolve(&app, &target, &[&step, &right]));
    assert_eq!(report["evolution"]["migration"]["dropped"], json!([124]));
    assert_eq!(
        report["evolution"]["migration"]["shortcuts"],
        json!([{"from_version":1, "tuples_compared":29_184}])
    );
    admitted(&check(&app));
}
