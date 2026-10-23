//! Every contract loader rejects a genesis outside the declared field domain.
use std::{fs, path::Path, path::PathBuf, process::Command, time::SystemTime};

struct TestProject(PathBuf);

impl TestProject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|error| panic!("test clock: {error}"))
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("genesis-domain-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("test directory: {error}"));
        Self(root)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        // This handle owns the directory; every child finishes before cleanup.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_project(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap_or_else(|error| panic!("test project: {error}"));
    for entry in fs::read_dir(source).unwrap_or_else(|error| panic!("source: {error}")) {
        let entry = entry.unwrap_or_else(|error| panic!("source entry: {error}"));
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_project(&from, &to);
        } else {
            fs::copy(from, to).unwrap_or_else(|error| panic!("copy input: {error}"));
        }
    }
}

fn command(name: &str, project: &Path, output: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"));
    command.arg("contract");
    match name {
        "refresh-receipts" => {
            command.arg(name).arg(project);
        }
        "diff" => {
            command.arg(name).arg(project).arg(project);
        }
        "export" => {
            command
                .arg(name)
                .arg(project)
                .arg("--tau")
                .arg("--out")
                .arg(output)
                .args(["--max-tuples", "1024", "--samples", "4"]);
        }
        "export-program" => {
            command.arg(name).arg(project).arg("--out").arg(output);
        }
        "scenario" => {
            command
                .arg(name)
                .arg(project)
                .arg("--file")
                .arg(project.join("tests/decision-scenarios.txt"))
                .args(["--max-tuples", "1024"]);
        }
        "probe" => {
            command.args([name, "explain"]).arg(project).args([
                "--input",
                "0 150 1",
                "--max-tuples",
                "1024",
            ]);
        }
        "review" => {
            command
                .arg(name)
                .arg(project)
                .args(["--max-tuples", "1024"]);
        }
        _ => panic!("unknown regression command"),
    }
    command.args(["--no-kb", "--format", "json"]);
    command
}

#[test]
fn every_contract_loader_refuses_out_of_range_genesis() {
    let work = TestProject::new();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/guide-counter");
    let good = work.0.join("good");
    let bad = work.0.join("bad");
    copy_project(&source, &good);
    copy_project(&source, &bad);
    let policy = bad.join("v2/policy.json");
    let mut rules: serde_json::Value = serde_json::from_slice(
        &fs::read(&policy).unwrap_or_else(|error| panic!("read policy: {error}")),
    )
    .unwrap_or_else(|error| panic!("parse policy: {error}"));
    rules["genesis"]["110"] = serde_json::json!(3);
    fs::write(policy, rules.to_string())
        .unwrap_or_else(|error| panic!("write test policy: {error}"));
    for name in [
        "refresh-receipts",
        "diff",
        "export",
        "export-program",
        "scenario",
        "probe",
        "review",
    ] {
        let refusal = if matches!(name, "scenario" | "probe") {
            65
        } else {
            1
        };
        for (label, project, expected) in [("original", &good, 0), ("out-of-range", &bad, refusal)]
        {
            let output = work.0.join(format!("{name}-{label}"));
            let result = command(name, project, &output)
                .output()
                .unwrap_or_else(|error| panic!("run {name}: {error}"));
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                result.status.code(),
                Some(expected),
                "{name}, {label}: {text}"
            );
            if expected != 0 {
                assert!(
                    text.contains("110") && text.contains('3') && text.contains("0..=2"),
                    "{name}: refusal must name field, value and range: {text}"
                );
            }
        }
        eprintln!("{name}: original=0, out-of-range={refusal} with field/value/range");
    }
}
