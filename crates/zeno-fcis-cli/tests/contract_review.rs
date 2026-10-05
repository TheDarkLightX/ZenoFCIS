//! `zeno-fcis contract review`: the packet is byte-identical on repeat, the
//! summary reports the review, a planted wrong constant contradicts an owner
//! example, and the command is described with its effects.
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
            "zeno-fcis-review-{label}-{}-{}",
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

fn dual_approval() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval")
}

fn zeno(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn review(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["contract", "review"])
        .arg(dir)
        .args(extra)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|error| panic!("JSON: {error}: {}", String::from_utf8_lossy(bytes)))
}

#[test]
fn the_packet_is_byte_identical_on_repeat_and_the_summary_reports_it() {
    let root = TempRoot::new("repeat");
    let first = root.path().join("first.json");
    let second = root.path().join("nested/second.json");
    let output = review(
        &dual_approval(),
        &[
            "--out",
            first.to_str().unwrap_or_default(),
            "--format",
            "json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = json(&output.stdout);
    assert_eq!(summary["schema"], "zeno-fcis/cli/1");
    assert_eq!(summary["status"], "reviewed");
    assert_eq!(summary["authority"], "none");
    assert_eq!(summary["packet_schema"], "zeno-fcis/contract-review/1");
    assert_eq!(summary["summary"]["application"], "dual-approval");
    assert_eq!(summary["summary"]["inputs"]["construction"], "full-domain");
    assert_eq!(summary["summary"]["inputs"]["count"], 384);
    assert_eq!(summary["summary"]["examples"]["compared"], 12);
    assert_eq!(summary["summary"]["examples"]["disagreements"], 0);
    assert_eq!(
        summary["summary"]["mutants"]["not_distinguished_within_boundary_set"],
        0
    );
    let output = review(
        &dual_approval(),
        &["--out", second.to_str().unwrap_or_default()],
    );
    assert_eq!(output.status.code(), Some(0));
    let human = String::from_utf8_lossy(&output.stdout);
    assert!(
        human.starts_with("reviewed dual-approval: 384 inputs (full-domain)"),
        "{human}"
    );
    let first = fs::read(&first).unwrap_or_else(|error| panic!("{error}"));
    let second = fs::read(&second).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(first, second);
    let packet = json(&first);
    assert_eq!(packet["schema"], "zeno-fcis/contract-review/1");
    assert_eq!(packet["application"], "dual-approval");
    assert_eq!(
        packet["mutants"]["catalog"],
        "zeno-fcis/contract-review-mutants/1"
    );
    // Without --out the packet itself is the output.
    let output = review(&dual_approval(), &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, first);
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("reviewed dual-approval"));
}

#[test]
fn a_planted_wrong_constant_is_a_disagreement() {
    let root = TempRoot::new("planted");
    let app = root.path().join("account-lockout");
    for file in [
        "project.zeno",
        "v2/policy.json",
        "tests/decision-examples.txt",
    ] {
        let target = app.join(file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        fs::copy(template("account-lockout").join(file), &target)
            .unwrap_or_else(|error| panic!("copy {file}: {error}"));
    }
    let rules = app.join("v2/policy.json");
    let text = fs::read_to_string(&rules).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text.matches("\"now + 900\"").count(), 1);
    fs::write(&rules, text.replace("\"now + 900\"", "\"now + 901\""))
        .unwrap_or_else(|error| panic!("{error}"));
    let packet = root.path().join("packet.json");
    let output = review(
        &app,
        &[
            "--out",
            packet.to_str().unwrap_or_default(),
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let summary = json(&output.stdout);
    assert_eq!(summary["status"], "disagreement");
    assert!(summary["summary"]["examples"]["disagreements"].as_u64() > Some(0));
    let packet = json(&fs::read(&packet).unwrap_or_else(|error| panic!("{error}")));
    let findings = packet["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("findings"));
    assert!(
        findings
            .iter()
            .any(|finding| finding["kind"] == "example-disagrees")
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding["kind"] == "example-agrees-with-mutant"),
        "{findings:?}"
    );
    // The application directory holds exactly the three files it started with.
    let mut files = Vec::new();
    for entry in walkdir(&app) {
        files.push(entry);
    }
    files.sort();
    assert_eq!(
        files,
        [
            "project.zeno",
            "tests/decision-examples.txt",
            "v2/policy.json"
        ]
    );
}

fn walkdir(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap_or_else(|error| panic!("{error}")) {
            let path = entry.unwrap_or_else(|error| panic!("{error}")).path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(
                    path.strip_prefix(root)
                        .unwrap_or_else(|error| panic!("{error}"))
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    found
}

#[test]
fn failures_use_the_exit_classes() {
    let root = TempRoot::new("failures");
    let output = review(&root.path().join("missing"), &["--format", "json"]);
    assert_eq!(output.status.code(), Some(3));
    let report = json(&output.stdout);
    assert_eq!(report["status"], "error");
    assert_eq!(report["error"]["code"], "review-read-failed");
    let output = review(&dual_approval(), &["--max-tuples", "0"]);
    assert_eq!(output.status.code(), Some(64));
    // An invalid contract names the entry at fault and writes no packet.
    let app = root.path().join("broken");
    fs::create_dir_all(app.join("v2")).unwrap_or_else(|error| panic!("{error}"));
    fs::copy(
        dual_approval().join("project.zeno"),
        app.join("project.zeno"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let rules = fs::read_to_string(dual_approval().join("v2/policy.json"))
        .unwrap_or_else(|error| panic!("{error}"));
    fs::write(
        app.join("v2/policy.json"),
        rules.replace(
            "\"when\": \"status != 150\"",
            "\"when\": \"status != nothing\"",
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let packet = root.path().join("none.json");
    let output = review(
        &app,
        &[
            "--out",
            packet.to_str().unwrap_or_default(),
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let report = json(&output.stdout);
    assert_eq!(report["error"]["code"], "contract-invalid");
    assert_eq!(report["error"]["place"], "v2/policy.json cases[0].when");
    assert!(!packet.exists());
}

#[test]
fn the_command_is_described_with_its_effects() {
    let output = zeno(&["describe", "contract", "review"]);
    assert_eq!(output.status.code(), Some(0));
    let description = json(&output.stdout);
    assert_eq!(description["schema"], "zeno-fcis/cli-description/1");
    let description = &description["command"];
    assert_eq!(description["name"], "review");
    assert_eq!(description["effects"]["classification"], "declared");
    assert_eq!(
        description["effects"]["reads"],
        serde_json::json!(["application-contract", "decision-examples"])
    );
    assert_eq!(
        description["effects"]["writes"],
        serde_json::json!(["optional-review-packet"])
    );
    assert_eq!(description["effects"]["executes_tools"], false);
    let output = zeno(&["describe", "contract"]);
    assert_eq!(
        json(&output.stdout)["command"]["effects"]["classification"],
        "command-group"
    );
}
