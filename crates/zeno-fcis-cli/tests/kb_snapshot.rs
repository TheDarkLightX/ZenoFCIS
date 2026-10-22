//! Real native replay comparisons for every approved runner shape.
#![cfg(unix)]
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fcis-kb-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn hash(bytes: &[u8]) -> String {
    use zeno_fcis_codec::CommitmentHasher;
    zeno_fcis_crypto::RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn input(root: &Path, path: &str) -> Value {
    json!({"path":path,"sha256":hash(&fs::read(root.join(path)).unwrap_or_else(|e| panic!("{e}")))})
}
fn cli(root: &Path, args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(root)
        .args(args)
        .env_remove("ZENO_FCIS_KB")
        .output()
        .unwrap_or_else(|e| panic!("{e}"))
}
fn value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)))
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|e| panic!("{e}"));
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("{e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("{e}"));
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
fn case(root: &Path, arguments: Vec<String>, inputs: Vec<Value>, pointer: &str) -> (Value, Value) {
    let output = cli(root, &arguments);
    let result = value(&output);
    let assertion = result
        .pointer(pointer)
        .unwrap_or_else(|| panic!("missing {pointer}: {result}"));
    (
        json!({"arguments":arguments,"inputs":inputs,"expected_exit":output.status.code(),"assertion":{"pointer":pointer,"equals":assertion}}),
        result,
    )
}
fn write_recipe(root: &Path, positive: Value, negative: Value) {
    let checker =
        hash(&fs::read(env!("CARGO_BIN_EXE_zeno-fcis")).unwrap_or_else(|e| panic!("{e}")));
    let recipe = json!({"schema":"zeno-fcis/kb-recipe/1","runner":"zeno-fcis-readonly-v1","lesson":"synthetic","family":"fixture","mode":"observation","checker_sha256":checker,
        "statement":[{"predicate":"lesson","arguments":["synthetic","sample"]}],
        "dependencies":[positive["inputs"][0].clone()],"positive":positive,"negative":negative});
    fs::write(
        root.join("recipe.json"),
        serde_json::to_vec(&recipe).unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
}
fn replay(root: &Path, positive: Value, negative: Value, expected: [Value; 2]) {
    write_recipe(root, positive, negative);
    let before = inventory(root);
    let output = cli(
        root,
        &[
            "kb".into(),
            "replay".into(),
            "--recipe".into(),
            "recipe.json".into(),
            "--out".into(),
            root.join("validation.json").to_string_lossy().into_owned(),
        ],
    );
    assert!(output.status.success(), "{}", value(&output));
    let receipt: Value =
        serde_json::from_slice(&fs::read(root.join("validation.json")).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(receipt["positive"]["output"], expected[0]);
    assert_eq!(receipt["negative"]["output"], expected[1]);
    assert_eq!(
        receipt["dependency_sha256"],
        value(&output)["dependency_sha256"]
    );
    fs::remove_file(root.join("validation.json")).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(before, inventory(root), "runner changed root files");
}
fn inventory(root: &Path) -> Vec<(PathBuf, String)> {
    fn visit(root: &Path, result: &mut Vec<(PathBuf, String)>) {
        for entry in fs::read_dir(root).unwrap_or_else(|e| panic!("{e}")) {
            let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
            if path.is_dir() {
                visit(&path, result);
            } else {
                result.push((path.clone(), hash(&fs::read(path).unwrap_or_default())));
            }
        }
    }
    let mut result = Vec::new();
    visit(root, &mut result);
    result.sort();
    result
}
#[test]
fn kb_snapshot_native_transform_fixtures_preserve_results() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transform-check");
    for version in 1..=4 {
        let d = Directory::new();
        for (source, target) in [
            ("program-1.zcve", "original.zcve"),
            ("program-2.zcve", "same.zcve"),
            ("program-7.zcve", "changed.zcve"),
        ] {
            fs::copy(
                fixtures.join(version.to_string()).join(source),
                d.0.join(target),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        }
        // Historical fixture encodings are refused by the current checker.
        // Pair each with a current counterexample to retain a genuine boundary.
        for (file, target) in [
            ("program-4.zcve", "boundary.zcve"),
            ("program-7.zcve", "changed.zcve"),
        ] {
            fs::copy(fixtures.join("4").join(file), d.0.join(target))
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let make = |original: &str, candidate: &str| {
            case(
                &d.0,
                [
                    "transform",
                    "check",
                    "--original",
                    original,
                    "--candidate",
                    candidate,
                ]
                .map(str::to_owned)
                .to_vec(),
                vec![input(&d.0, original), input(&d.0, candidate)],
                "/status",
            )
        };
        let (positive, first) = make("original.zcve", "same.zcve");
        let (negative, second) = make("boundary.zcve", "changed.zcve");
        replay(&d.0, positive, negative, [first, second]);
    }
}
#[test]
fn kb_snapshot_native_contract_and_gaps_preserve_undeclared_tree() {
    for gaps in [false, true] {
        let d = Directory::new();
        let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/knowledge-evidence");
        copy_tree(&from, &d.0.join("app"));
        copy_tree(&from, &d.0.join("boundary"));
        // A nearby negative: no examples are compared, unlike the positive.
        fs::remove_file(d.0.join("boundary/tests/decision-examples.txt"))
            .unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(d.0.join("app/undeclared/nested")).unwrap_or_else(|e| panic!("{e}"));
        fs::write(
            d.0.join("app/undeclared/nested/data.txt"),
            b"complete subject",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let make = |dir: &str, examples: bool| {
            let mut arguments = vec!["contract".into(), "review".into(), dir.into()];
            if gaps {
                arguments.push("--gaps".into());
            }
            let mut inputs = vec![
                input(&d.0, &format!("{dir}/project.zeno")),
                input(&d.0, &format!("{dir}/v2/policy.json")),
            ];
            if examples {
                inputs.push(input(&d.0, &format!("{dir}/tests/decision-examples.txt")));
            }
            case(&d.0, arguments, inputs, "/summary/examples/compared")
        };
        let (positive, first) = make("app", true);
        let (negative, second) = make("boundary", false);
        replay(&d.0, positive, negative, [first, second]);
    }
}

#[test]
fn kb_snapshot_native_receipt_inside_either_subject_refuses_and_outside_reloads() {
    use std::os::unix::fs::symlink;
    let d = Directory::new();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/knowledge-evidence");
    for name in ["app", "boundary"] {
        copy_tree(&fixture, &d.0.join(name));
    }
    fs::remove_file(d.0.join("boundary/tests/decision-examples.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    let make = |name: &str, examples: bool| {
        let mut inputs = vec![
            input(&d.0, &format!("{name}/project.zeno")),
            input(&d.0, &format!("{name}/v2/policy.json")),
        ];
        if examples {
            inputs.push(input(&d.0, &format!("{name}/tests/decision-examples.txt")));
        }
        case(
            &d.0,
            vec!["contract".into(), "review".into(), name.into()],
            inputs,
            "/summary/examples/compared",
        )
        .0
    };
    write_recipe(&d.0, make("app", true), make("boundary", false));
    symlink("app/v2", d.0.join("alias")).unwrap_or_else(|e| panic!("{e}"));
    let run = |out: &str| {
        cli(
            &d.0,
            &[
                "kb".into(),
                "replay".into(),
                "--recipe".into(),
                "recipe.json".into(),
                "--out".into(),
                out.into(),
            ],
        )
    };
    let before = inventory(&d.0);
    for destination in [
        "app/validation.json",
        "boundary/validation.json",
        "app/v2/validation.json",
        "alias/validation.json",
        "app/v2/../validation.json",
    ] {
        let result = run(d.0.join(destination).to_str().unwrap_or_default());
        assert_eq!(result.status.code(), Some(1), "{}", value(&result));
        assert_eq!(value(&result)["reason"], "kb-evidence-output-subject");
        assert!(!d.0.join(destination).exists());
        assert_eq!(inventory(&d.0), before, "refusal changed the source tree");
    }
    // A relative destination with an existing parent is allowed outside either
    // tree, including a nearby name sharing the positive subject's prefix.
    fs::create_dir(d.0.join("app-receipts")).unwrap_or_else(|e| panic!("{e}"));
    let result = run("app-receipts/validation.json");
    assert!(result.status.success(), "{}", value(&result));
    assert_eq!(value(&result)["status"], "validated");
    fs::write(d.0.join("knowledge.kb"), "").unwrap_or_else(|e| panic!("{e}"));
    let layer = json!({"schema":"zeno-fcis/kb-layer/1",
        "knowledge":input(&d.0, "knowledge.kb"),
        "validations":[input(&d.0, "app-receipts/validation.json")]});
    fs::write(
        d.0.join("layer.json"),
        serde_json::to_vec(&layer).unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let checked = cli(
        &d.0,
        &[
            "--kb".into(),
            d.0.join("layer.json").to_string_lossy().into_owned(),
            "kb".into(),
            "check".into(),
        ],
    );
    let report = value(&checked);
    assert!(checked.status.success(), "{report}");
    assert_eq!(report["validation_problems"], json!([]));
    assert!(
        report["validations"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row["lesson"] == "synthetic" && row["status"] == "Validated")),
        "{report}"
    );
    // A valid executable with different bytes must not reuse this receipt.
    // Appending a byte preserves the native executable while changing its hash.
    use std::io::Write;
    let changed_checker = d.0.join("changed-checker");
    fs::copy(env!("CARGO_BIN_EXE_zeno-fcis"), &changed_checker).unwrap_or_else(|e| panic!("{e}"));
    fs::OpenOptions::new()
        .append(true)
        .open(&changed_checker)
        .and_then(|mut file| file.write_all(b"changed-checker-identity"))
        .unwrap_or_else(|e| panic!("{e}"));
    let changed = Command::new(&changed_checker)
        .current_dir(&d.0)
        .args(["--kb", "layer.json", "kb", "check"])
        .env_remove("ZENO_FCIS_KB")
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    let changed_report = value(&changed);
    assert!(
        changed_report["validation_problems"]
            .as_array()
            .is_some_and(|rows| rows
                .iter()
                .any(|row| row["reason"] == "kb-evidence-checker-stale")),
        "{changed_report}"
    );
    assert!(
        changed_report["validations"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["lesson"] != "synthetic")),
        "{changed_report}"
    );
    let refused = Command::new(&changed_checker)
        .current_dir(&d.0)
        .args([
            "kb",
            "replay",
            "--recipe",
            "recipe.json",
            "--out",
            "./stale.json",
        ])
        .env_remove("ZENO_FCIS_KB")
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(refused.status.code(), Some(1), "{}", value(&refused));
    assert_eq!(value(&refused)["reason"], "kb-evidence-checker-stale");
    assert!(!d.0.join("stale.json").exists());
}

// ---- Re-review 2 regression witnesses ----
#[test]
fn rr2_native_dangling_final_symlink_and_reload_timing() {
    use std::os::unix::fs::symlink;
    let d = Directory::new();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/knowledge-evidence");
    for name in ["app", "boundary"] {
        copy_tree(&fixture, &d.0.join(name));
    }
    fs::remove_file(d.0.join("boundary/tests/decision-examples.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    let make = |name: &str, examples: bool| {
        let mut inputs = vec![
            input(&d.0, &format!("{name}/project.zeno")),
            input(&d.0, &format!("{name}/v2/policy.json")),
        ];
        if examples {
            inputs.push(input(&d.0, &format!("{name}/tests/decision-examples.txt")));
        }
        case(
            &d.0,
            vec!["contract".into(), "review".into(), name.into()],
            inputs,
            "/summary/examples/compared",
        )
        .0
    };
    write_recipe(&d.0, make("app", true), make("boundary", false));
    fs::create_dir(d.0.join("receipts")).unwrap_or_else(|e| panic!("{e}"));
    symlink("../app/validation.json", d.0.join("receipts/dangling"))
        .unwrap_or_else(|e| panic!("{e}"));
    let run = |out: &str| {
        cli(
            &d.0,
            &[
                "kb".into(),
                "replay".into(),
                "--recipe".into(),
                "recipe.json".into(),
                "--out".into(),
                out.into(),
            ],
        )
    };
    let result = run("receipts/dangling");
    eprintln!(
        "rr2 dangling: exit={:?} {}",
        result.status.code(),
        value(&result)
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(!d.0.join("app/validation.json").exists());
    let result = run("receipts/validation.json");
    assert!(result.status.success(), "{}", value(&result));
    fs::write(d.0.join("knowledge.kb"), "").unwrap_or_else(|e| panic!("{e}"));
    let layer = json!({"schema":"zeno-fcis/kb-layer/1","knowledge":input(&d.0, "knowledge.kb"),
        "validations":[input(&d.0, "receipts/validation.json")]});
    fs::write(
        d.0.join("layer.json"),
        serde_json::to_vec(&layer).unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let size = fs::metadata(env!("CARGO_BIN_EXE_zeno-fcis"))
        .map(|m| m.len())
        .unwrap_or(0);
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let checked = cli(
            &d.0,
            &[
                "--kb".into(),
                d.0.join("layer.json").to_string_lossy().into_owned(),
                "kb".into(),
                "check".into(),
            ],
        );
        times.push(start.elapsed().as_millis());
        assert!(checked.status.success(), "{}", value(&checked));
    }
    eprintln!("rr2 kb check wall ms (5 runs, binary {size} bytes): {times:?}");
}
#[test]
fn rr2_native_kb_check_time_split() {
    let d = Directory::new();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/knowledge-evidence");
    for name in ["app", "boundary"] {
        copy_tree(&fixture, &d.0.join(name));
    }
    fs::remove_file(d.0.join("boundary/tests/decision-examples.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    let make = |name: &str, examples: bool| {
        let mut inputs = vec![
            input(&d.0, &format!("{name}/project.zeno")),
            input(&d.0, &format!("{name}/v2/policy.json")),
        ];
        if examples {
            inputs.push(input(&d.0, &format!("{name}/tests/decision-examples.txt")));
        }
        case(
            &d.0,
            vec!["contract".into(), "review".into(), name.into()],
            inputs,
            "/summary/examples/compared",
        )
        .0
    };
    write_recipe(&d.0, make("app", true), make("boundary", false));
    fs::create_dir(d.0.join("receipts")).unwrap_or_else(|e| panic!("{e}"));
    let out = cli(
        &d.0,
        &[
            "kb".into(),
            "replay".into(),
            "--recipe".into(),
            "recipe.json".into(),
            "--out".into(),
            "receipts/validation.json".into(),
        ],
    );
    assert!(out.status.success(), "{}", value(&out));
    let mut bad: Value =
        serde_json::from_slice(&fs::read(d.0.join("receipts/validation.json")).unwrap_or_default())
            .unwrap_or_default();
    bad["schema"] = json!("wrong");
    fs::write(
        d.0.join("receipts/bad.json"),
        serde_json::to_vec(&bad).unwrap_or_default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    fs::write(d.0.join("knowledge.kb"), "").unwrap_or_else(|e| panic!("{e}"));
    for (name, validations) in [
        ("none", vec![]),
        ("hash-only", vec![input(&d.0, "receipts/bad.json")]),
        ("full-load", vec![input(&d.0, "receipts/validation.json")]),
    ] {
        let layer = json!({"schema":"zeno-fcis/kb-layer/1","knowledge":input(&d.0, "knowledge.kb"),"validations":validations});
        let file = format!("layer-{name}.json");
        fs::write(
            d.0.join(&file),
            serde_json::to_vec(&layer).unwrap_or_default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let mut times = Vec::new();
        for _ in 0..4 {
            let start = std::time::Instant::now();
            let checked = cli(
                &d.0,
                &[
                    "--kb".into(),
                    d.0.join(&file).to_string_lossy().into_owned(),
                    "kb".into(),
                    "check".into(),
                ],
            );
            times.push(start.elapsed().as_millis());
            let _ = checked;
        }
        eprintln!("rr2 split {name}: {times:?} ms");
    }
}
