//! `zeno-fcis loop decompose --compose` through the real binary: a
//! decomposition with case tables is exported as a composed-domain
//! composition, simulated with the composition checker's own joint step on
//! every input of the core's domain, and checked; the exported manifest is
//! then checked again by `contract check-composition`.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-loop-compose-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn json(&self, name: &str, value: &Value) -> String {
        let path = self.0.join(name);
        fs::write(&path, value.to_string()).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path.to_string_lossy().into_owned()
    }

    fn contract(&self, family: &str, parameters: &[(&str, i64)]) -> String {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../core-components")
            .join(family);
        let label: String = parameters
            .iter()
            .map(|(name, value)| format!("{}{value}", name.to_lowercase()))
            .collect::<Vec<_>>()
            .join("-");
        let app = self.0.join(format!("{family}-{label}"));
        fs::create_dir_all(app.join("v2")).unwrap_or_else(|error| panic!("{error}"));
        for (from, to) in [
            ("project.zeno.in", "project.zeno"),
            ("policy.json.in", "v2/policy.json"),
        ] {
            let mut text =
                fs::read_to_string(base.join(from)).unwrap_or_else(|error| panic!("{error}"));
            for (name, value) in parameters {
                text = text.replace(&format!("${{{name}}}"), &value.to_string());
            }
            text = text.replace("${INSTANCE}", &label);
            fs::write(app.join(to), text).unwrap_or_else(|error| panic!("{error}"));
        }
        app.to_string_lossy().into_owned()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(arguments: &[&str]) -> (Option<i32>, Value) {
    let output = Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn architecture(components: &[Value]) -> Value {
    json!({"schema": "zeno-fcis/loop-architecture/1", "components": components})
}

#[test]
fn a_source_with_deliveries_or_channels_is_refused_without_export() {
    // C=1: all eight class/reason/successor projections agree with the
    // delivery-stripped core, but both accepted moves lose a channel-300 notice.
    // Even an unused channel is outside this no-outbox composition profile.
    for has_deliveries in [true, false] {
        let directory = Directory::new();
        let core = directory.0.join("core");
        fs::create_dir_all(core.join("v2"))
            .unwrap_or_else(|error| panic!("create fixture directory: {error}"));
        fs::write(
            core.join("project.zeno"),
            include_str!("fixtures/loop-compose-deliveries/project.zeno"),
        )
        .unwrap_or_else(|error| panic!("write fixture project: {error}"));
        let mut policy: Value = serde_json::from_str(include_str!(
            "fixtures/loop-compose-deliveries/v2/policy.json"
        ))
        .unwrap_or_else(|error| panic!("parse fixture policy: {error}"));
        if !has_deliveries {
            for case in policy["cases"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("fixture cases must be an array"))
            {
                case["outbox"] = json!([]);
            }
        }
        fs::write(core.join("v2/policy.json"), policy.to_string())
            .unwrap_or_else(|error| panic!("write fixture policy: {error}"));
        let skeleton: Value = serde_json::from_str(include_str!(
            "fixtures/loop-compose-deliveries/architecture.json"
        ))
        .unwrap_or_else(|error| panic!("parse fixture architecture: {error}"));
        let path = directory.json("architecture.json", &skeleton);
        let export = directory.0.join("export");
        let (exit, report) = run(&[
            "loop",
            "decompose",
            "--contract",
            core.to_str()
                .unwrap_or_else(|| panic!("fixture path must be UTF-8")),
            "--architecture",
            &path,
            "--compose",
            "--export",
            export
                .to_str()
                .unwrap_or_else(|| panic!("export path must be UTF-8")),
        ]);
        assert_eq!(exit, Some(1), "{report}");
        assert_eq!(report["status"], "refused", "{report}");
        assert_eq!(
            report["detail"]["reason"], "compose-core-deliveries-unsupported",
            "{report}"
        );
        assert!(!export.exists(), "a refused source must export nothing");
    }
}

#[test]
fn a_projected_decomposition_composes_to_the_core_and_its_manifest_checks() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let path = directory.json(
        "skeleton.json",
        &architecture(&[
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"]}),
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity", "at_zero"]}),
        ]),
    );
    let export = directory.0.join("export");
    let export_text = export.to_string_lossy().into_owned();
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
        "--compose",
        "--export",
        &export_text,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "accepted", "{report}");
    let composed = &report["detail"]["compose"];
    assert_eq!(composed["inputs"], 16);
    assert_eq!(composed["differ"], 0);
    assert_eq!(composed["obligations"]["verdict"], "proved", "{composed}");
    // the exported manifest is checked again by the composition command
    let manifest = export.join("composition.json");
    let manifest = manifest.to_string_lossy();
    let (exit, checked) = run(&[
        "contract",
        "check-composition",
        &manifest,
        "--format",
        "json",
    ]);
    assert_eq!(exit, Some(0), "{checked}");
}

#[test]
fn a_planted_wrong_component_makes_the_composed_check_fail() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    // the counter rejects at capacity with the guard's reason
    let path = directory.json(
        "wrong.json",
        &architecture(&[
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"],
                   "cases": [{"when": "!authorized", "class": "Reject", "reason": "not_authorized"},
                             {"when": "true", "class": "Accept"}]}),
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity", "at_zero"],
                   "cases": [{"when": "action == Inc && value == 3", "class": "Reject", "reason": "at_zero"},
                             {"when": "action == Dec && value == 0", "class": "Reject", "reason": "at_zero"},
                             {"when": "action == Inc", "class": "Accept", "sets": {"value": "value + 1"}},
                             {"when": "true", "class": "Accept", "sets": {"value": "value - 1"}}]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
        "--compose",
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refuted", "{report}");
    let composed = &report["detail"]["compose"];
    assert_eq!(composed["differ"], 1, "{composed}");
    assert_eq!(composed["first_difference"]["input"]["value"], 3);
    assert_eq!(
        composed["first_difference"]["composed"]["outcome"]["reason"],
        202
    );
    // and the step-3 blame names the same component
    assert_eq!(report["detail"]["blame"][0]["component"], "counter");
}

#[test]
fn an_architecture_without_case_tables_is_not_composed() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let path = directory.json(
        "blind.json",
        &architecture(&[
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"]}),
            json!({"name": "counter", "owns": ["value"], "decides": ["at_capacity", "at_zero"]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
        "--compose",
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "unrealizable");
    assert_eq!(report["detail"]["compose"]["status"], "not-run");
}

#[test]
fn a_last_guard_that_never_fires_is_kept_and_the_decomposition_is_never_accepted() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    // the last guard misses an authorized Dec above zero: step 3 blames the
    // component there, and the composed check must not rewrite the guard
    let path = directory.json(
        "never.json",
        &architecture(&[json!({"name": "all", "owns": ["value"], "reads": ["action", "authorized"],
            "decides": ["not_authorized", "at_capacity", "at_zero"],
            "cases": [{"when": "!authorized", "class": "Reject", "reason": "not_authorized"},
                      {"when": "action == Inc && value == 3", "class": "Reject", "reason": "at_capacity"},
                      {"when": "action == Dec && value == 0", "class": "Reject", "reason": "at_zero"},
                      {"when": "action == Inc", "class": "Accept", "sets": {"value": "value + 1"}},
                      {"when": "action == Dec && !authorized", "class": "Accept", "sets": {"value": "value - 1"}}]})]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
        "--compose",
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refuted", "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["steps_verdict"], "refuted");
    assert_eq!(detail["blame"][0]["component"], "all");
    assert_eq!(detail["blame"][0]["wrong"], 3);
    let composed = &detail["compose"];
    assert_eq!(composed["decided_by"], "composition-checker");
    assert_eq!(composed["catch_all"], json!(["all"]));
    // the appended catch-all rejects with a reason the core never gives
    assert_eq!(composed["differ"], 3, "{composed}");
    assert_eq!(detail["compose_verdict"], "refuted");
}

#[test]
fn export_refuses_an_existing_directory() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let path = directory.json(
        "skeleton.json",
        &architecture(&[
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"]}),
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity", "at_zero"]}),
        ]),
    );
    let export = directory.0.join("existing");
    fs::create_dir(&export).unwrap_or_else(|error| panic!("{error}"));
    fs::write(export.join("composition.json"), "keep").unwrap_or_else(|error| panic!("{error}"));
    let export_text = export.to_string_lossy().into_owned();
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
        "--compose",
        "--export",
        &export_text,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused", "{report}");
    assert!(
        report["detail"]["reason"]
            .as_str()
            .unwrap_or_default()
            .starts_with("compose-export-needs-a-new-directory"),
        "{report}"
    );
    assert_eq!(
        fs::read_to_string(export.join("composition.json")).unwrap_or_default(),
        "keep"
    );
}

#[test]
fn a_command_root_without_fields_composes_under_its_own_name() {
    let directory = Directory::new();
    // the rate limiter's command is one variant of a sum type: a request
    // position named after its root, which a generated contract renames
    let limiter = directory.contract("rate-limiter", &[("N", 1), ("W", 1)]);
    let path = directory.json(
        "one.json",
        &architecture(&[json!({"name": "all", "owns": ["start", "used"],
            "reads": ["command", "now", "authorized"],
            "decides": ["not_authorized", "before_window", "quota_exhausted"]})]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &limiter,
        "--architecture",
        &path,
        "--compose",
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "accepted", "{report}");
    assert_eq!(report["detail"]["compose"]["differ"], 0);
}

#[test]
fn a_renamed_root_that_meets_a_real_field_of_that_name_is_refused() {
    let directory = Directory::new();
    // the rate limiter with its context field `now` renamed `command_value`:
    // the fieldless command root's generated name would collide with it
    let limiter = directory.contract("rate-limiter", &[("N", 1), ("W", 1)]);
    for file in ["project.zeno", "v2/policy.json"] {
        let path = Path::new(&limiter).join(file);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}"));
        let renamed = text
            .replace(" now ", " command_value ")
            .replace("\"now\"", "\"command_value\"")
            .replace("now >=", "command_value >=")
            .replace("now <", "command_value <")
            .replace("now -", "command_value -")
            .replace("now\"", "command_value\"");
        fs::write(&path, renamed).unwrap_or_else(|error| panic!("{error}"));
    }
    let path = directory.json(
        "one.json",
        &architecture(&[json!({"name": "all", "owns": ["start", "used"],
            "reads": ["command", "command_value", "authorized"],
            "decides": ["not_authorized", "before_window", "quota_exhausted"]})]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &limiter,
        "--architecture",
        &path,
        "--compose",
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused", "{report}");
    assert!(
        report["detail"]["reason"]
            .as_str()
            .unwrap_or_default()
            .starts_with("compose-renamed-name-collides"),
        "{report}"
    );
}

#[test]
fn exported_planted_retry_architecture_retains_its_refuted_verdict() {
    let directory = Directory::new();
    let core = directory.contract("retry-budget", &[("C", 3)]);
    let architecture: Value = serde_json::from_str(include_str!(
        "../../zeno-fcis-loop-doors/tests/data/rb-arch-cases-planted.json"
    ))
    .unwrap_or_else(|error| panic!("fixture: {error:?}"));
    let path = directory.json("planted.json", &architecture);
    let export = directory.0.join("planted-export");
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &core,
        "--architecture",
        &path,
        "--compose",
        "--export",
        &export.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refuted");
    let verdict: Value = serde_json::from_slice(
        &fs::read(export.join("verdict.json"))
            .unwrap_or_else(|error| panic!("export verdict: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("JSON: {error:?}"));
    assert_eq!(verdict["status"], "refuted");
    assert_eq!(verdict["differ"], report["detail"]["compose"]["differ"]);
    assert_eq!(
        verdict["first_difference"],
        report["detail"]["compose"]["first_difference"]
    );
    assert_eq!(
        verdict["core_digest"]
            .as_str()
            .unwrap_or_else(|| panic!("digest"))
            .len(),
        64
    );
    let digest: String = RustCryptoSha256::hash(
        &fs::read(export.join("composition.json"))
            .unwrap_or_else(|error| panic!("manifest: {error:?}")),
    )
    .as_bytes()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
    assert_eq!(verdict["manifest_sha256"], digest);
    assert_eq!(verdict["authority"], "none");
}
