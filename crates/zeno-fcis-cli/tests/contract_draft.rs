//! G7 source fixtures exercise supplied labels, never a real owner conversation.
#![forbid(unsafe_code)]
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-draft-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn path(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
}
fn draft(args: &[&str], success: bool) -> Value {
    let output = cli(&[&["contract", "draft"][..], args].concat());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(output.status.success(), success, "{report}");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["hosted_model"], "off");
    report
}
fn revision(report: &Value) -> &str {
    report["revision"]
        .as_str()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
}
fn start(session: &Path, name: &str, initial: bool, rounds: &str) -> Value {
    let template = root().join("templates").join(name);
    let intent = root()
        .join("tests/fixtures/contract-draft")
        .join(format!("{name}.intent.txt"));
    let examples = template.join("tests/decision-examples.txt");
    let project = template.join("project.zeno");
    let mut args = vec![
        "start",
        "--session",
        path(session),
        "--intent",
        path(&intent),
        "--project",
        path(&project),
        "--provenance",
        "prerecorded template examples: retain original source comments; no live owner",
        "--rounds",
        rounds,
    ];
    if initial {
        args.extend(["--examples", path(&examples)]);
    }
    draft(&args, true)
}
fn proposal(session: &Path, current: &Value, rules: &Path, success: bool) -> Value {
    draft(
        &[
            "propose",
            "--session",
            path(session),
            "--revision",
            revision(current),
            "--rules",
            path(rules),
            "--provenance",
            "complete fixture proposal from the retained prose description",
        ],
        success,
    )
}

#[test]
fn draft_refuses_missing_labels_invalid_exhausted_and_stale_proposals() {
    let temp = Temp::new();
    let session = temp.0.join("session");
    let initial = start(&session, "durable-counter", false, "2");
    let rules = root().join("tests/fixtures/contract-draft/durable-counter.rules.json");
    let proposed = proposal(&session, &initial, &rules, true);
    assert_eq!(proposed["detail"]["ready"], false);
    draft(&["check", "--session", path(&session)], false);
    let out = temp.0.join("unlabeled");
    draft(
        &[
            "finalize",
            "--session",
            path(&session),
            "--revision",
            revision(&proposed),
            "--out",
            path(&out),
        ],
        false,
    );
    assert!(!out.exists());
    let before = fs::read(session.join("draft.json"))
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let stale = proposal(&session, &initial, &rules, false);
    assert!(
        stale["reason"]
            .as_str()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .contains("stale")
    );
    assert_eq!(
        fs::read(session.join("draft.json"))
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
        before
    );
    let bad = temp.0.join("bad.json");
    fs::write(&bad, b"not rules")
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let refused = proposal(&session, &proposed, &bad, false);
    assert_eq!(refused["attempts"], 2);
    let exhausted = proposal(&session, &refused, &rules, false);
    assert!(
        exhausted["reason"]
            .as_str()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .contains("exhausted")
    );
    draft(
        &[
            "finalize",
            "--session",
            path(&session),
            "--revision",
            revision(&refused),
            "--out",
            path(&out),
        ],
        false,
    );
    assert!(!out.exists());
    let stored: Value = serde_json::from_slice(
        &fs::read(session.join("draft.json"))
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(stored["proposals"][1]["rules"], "not rules");
    assert_eq!(
        stored["proposals"][1]["assessment"]["status"],
        "proposal-refused"
    );
}

// Format library observations for a test fixture; this performs no policy evaluation.
fn number(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        s.to_owned()
    } else if let Some(b) = value.as_bool() {
        if b { "1" } else { "0" }.to_owned()
    } else {
        value["variant"]
            .as_u64()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .to_string()
    }
}
fn fields(value: &Value) -> String {
    value
        .as_array()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .iter()
        .map(|field| number(&field["value"]))
        .collect::<Vec<_>>()
        .join(" ")
}
fn numbers(input: &[Value]) -> String {
    input
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

// Only formats existing library observations; exact packet classes are closed.
fn observed_example(input: &[Value], width: usize, observed: &Value) -> String {
    let class = match observed["class"]
        .as_str()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
    {
        "Accept" => "accept",
        "Reject" => "reject",
        "CommittedFailure" => "failure",
        other => panic!("unknown F2 decision class: {other}"),
    };
    let reason = observed["reason"]
        .as_u64()
        .map_or("-".to_owned(), |r| r.to_string());
    let post = if class == "reject" {
        numbers(&input[..width])
    } else {
        fields(&observed["post"])
    };
    let deliveries: Vec<String> = observed["outbox"]
        .as_array()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .iter()
        .map(|d| {
            format!(
                "{} {}",
                d["channel"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("required fixture value is missing")),
                fields(&d["payload"])
            )
        })
        .collect();
    let deliveries = if deliveries.is_empty() {
        "-".to_owned()
    } else {
        deliveries.join(" ; ")
    };
    format!(
        "{} | {class} {reason} {post} | {deliveries}",
        numbers(input)
    )
}

/// The simulated supplier evaluates *original retained rules*, never the candidate.
/// Placeholder expectations are intentionally not labels; F2 returns the baseline
/// library observations, which we format as separately supplied test expectations.
fn simulated_labels(base: &Path, name: &str, inputs: &[Value], width: usize) -> String {
    let original = root().join("templates").join(name);
    let baseline = base.join("baseline");
    fs::create_dir_all(baseline.join("v2"))
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    fs::create_dir_all(baseline.join("tests"))
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    fs::copy(original.join("project.zeno"), baseline.join("project.zeno"))
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    fs::copy(
        original.join("v2/policy.json"),
        baseline.join("v2/policy.json"),
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let placeholders: Vec<String> = inputs
        .iter()
        .map(|input| {
            let input = input
                .as_array()
                .unwrap_or_else(|| panic!("required fixture value is missing"));
            format!(
                "{} | reject 200 {} | -",
                numbers(input),
                numbers(&input[..width])
            )
        })
        .collect();
    fs::write(
        baseline.join("tests/decision-examples.txt"),
        placeholders.join("\n"),
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let output = cli(&[
        "contract",
        "review",
        path(&baseline),
        "--max-tuples",
        "4096",
    ]);
    let review: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let disagreements = review["examples"]["disagreements"]
        .as_array()
        .unwrap_or_else(|| panic!("required fixture value is missing"));
    placeholders
        .iter()
        .enumerate()
        .map(|(index, placeholder)| {
            let Some(disagreement) = disagreements
                .iter()
                .find(|d| d["line"].as_u64() == Some(index as u64 + 1))
            else {
                return placeholder.clone();
            };
            let observed = &disagreement["library"];
            assert!(
                observed.get("refused").is_none(),
                "baseline cannot label this fixture question: {observed}"
            );
            let input = inputs[index]
                .as_array()
                .unwrap_or_else(|| panic!("required fixture value is missing"));
            observed_example(input, width, observed)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[ignore = "builds three generated applications; run under the single-heavy-work resource gate"]
fn draft_rederive_three_templates_and_test_generated_apps() {
    for (name, width) in [
        ("durable-counter", 2),
        ("inventory-reservation", 2),
        ("account-lockout", 3),
    ] {
        let temp = Temp::new();
        let session = temp.0.join("session");
        let mut current = start(&session, name, true, "3");
        let rules = root()
            .join("tests/fixtures/contract-draft")
            .join(format!("{name}.rules.json"));
        if name == "durable-counter" {
            let mut wrong: Value = serde_json::from_slice(
                &fs::read(&rules)
                    .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
            )
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
            // Keep both reasons used so generation reaches the label mismatch.
            wrong["cases"][0]["reason"] = json!(201);
            wrong["cases"][1]["reason"] = json!(200);
            let wrong_path = temp.0.join("wrong.json");
            fs::write(&wrong_path, wrong.to_string())
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
            current = proposal(&session, &current, &wrong_path, true);
            assert_eq!(current["detail"]["ready"], false);
            assert!(
                current["detail"]["labels_compared"]
                    .as_array()
                    .unwrap_or_else(|| panic!("required fixture value is missing"))
                    .iter()
                    .any(|label| !label["difference"].is_null())
            );
            let bad_out = temp.0.join("wrong-output");
            draft(
                &[
                    "finalize",
                    "--session",
                    path(&session),
                    "--revision",
                    revision(&current),
                    "--out",
                    path(&bad_out),
                ],
                false,
            );
            assert!(!bad_out.exists());
        }
        current = proposal(&session, &current, &rules, true);
        // Adding supplied examples can change F2's bounded probe bases. Keep the
        // simulated conversation explicitly bounded, never auto-label in production.
        for _ in 0..8 {
            let missing = current["detail"]["unlabeled_inputs"]
                .as_array()
                .unwrap_or_else(|| panic!("required fixture value is missing"));
            if missing.is_empty() {
                break;
            }
            let labels = simulated_labels(&temp.0, name, missing, width);
            let labels_path = temp.0.join("supplier.txt");
            fs::write(&labels_path, labels)
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
            current = draft(
                &[
                    "label",
                    "--session",
                    path(&session),
                    "--revision",
                    revision(&current),
                    "--examples",
                    path(&labels_path),
                    "--provenance",
                    "SIMULATED supplier fixture: retained original template library decisions; not live owner or human review",
                ],
                true,
            );
        }
        let checked = draft(&["check", "--session", path(&session)], true);
        assert_eq!(checked["detail"]["ready"], true);
        assert!(
            checked["detail"]["labels_compared"]
                .as_array()
                .unwrap_or_else(|| panic!("required fixture value is missing"))
                .iter()
                .all(|label| label["difference"].is_null())
        );
        assert!(
            checked["detail"]["questions"]
                .as_array()
                .unwrap_or_else(|| panic!("required fixture value is missing"))
                .iter()
                .all(|q| q["label_supplied"] == true)
        );
        let out = temp.0.join("contract");
        let finalized = draft(
            &[
                "finalize",
                "--session",
                path(&session),
                "--revision",
                revision(&checked),
                "--out",
                path(&out),
            ],
            true,
        );
        assert_eq!(finalized["status"], "draft-finalized");
        assert!(out.join("draft-transcript.json").is_file());
        let bytes = fs::read(out.join("draft-finalized.json"))
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        draft(
            &[
                "finalize",
                "--session",
                path(&session),
                "--revision",
                revision(&checked),
                "--out",
                path(&out),
            ],
            false,
        );
        assert_eq!(
            bytes,
            fs::read(out.join("draft-finalized.json"))
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
        );
        let app = temp.0.join("application");
        let tree = root().join("../..");
        let scaffold = cli(&[
            "new",
            path(&app),
            "--contract",
            path(&out),
            "--source",
            path(&tree),
        ]);
        assert!(
            scaffold.status.success(),
            "{}",
            String::from_utf8_lossy(&scaffold.stderr)
        );
        // Resolve only the generated package's copied graph before locked tests.
        let resolved = Command::new("cargo")
            .args([
                "+1.97.1",
                "metadata",
                "--offline",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(app.join("Cargo.toml"))
            .output()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        assert!(
            resolved.status.success(),
            "{}",
            String::from_utf8_lossy(&resolved.stderr)
        );
        let tested = Command::new("cargo")
            .args([
                "+1.97.1",
                "test",
                "--offline",
                "--locked",
                "-j",
                "1",
                "--manifest-path",
            ])
            .arg(app.join("Cargo.toml"))
            .args(["--", "--test-threads=1"])
            .output()
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        assert!(
            tested.status.success(),
            "{name}: {} {}",
            String::from_utf8_lossy(&tested.stdout),
            String::from_utf8_lossy(&tested.stderr)
        );
    }
}

#[test]
fn draft_supplier_formats_all_three_library_classes() {
    let input = [json!(2), json!(1), json!(120), json!(1)];
    let post = json!([{"field":110,"value":"3"},{"field":111,"value":"1"}]);
    assert_eq!(
        observed_example(
            &input,
            2,
            &json!({"class":"Accept","reason":null,"post":post,"outbox":[]})
        ),
        "2 1 120 1 | accept - 3 1 | -"
    );
    assert_eq!(
        observed_example(
            &input,
            2,
            &json!({"class":"Reject","reason":201,"post":[],"outbox":[]})
        ),
        "2 1 120 1 | reject 201 2 1 | -"
    );
    assert_eq!(
        observed_example(
            &input,
            2,
            &json!({"class":"CommittedFailure","reason":202,"post":post,"outbox":[]})
        ),
        "2 1 120 1 | failure 202 3 1 | -"
    );
}

#[test]
#[should_panic(expected = "unknown F2 decision class")]
fn draft_supplier_refuses_unknown_library_class() {
    observed_example(
        &[],
        0,
        &json!({"class":"accept","reason":null,"post":[],"outbox":[]}),
    );
}
