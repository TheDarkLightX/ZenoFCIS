//! Process-level checks of inert completion cases and independent replay.
#![allow(clippy::unwrap_used)]

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-completion-cli-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, source: &Value) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, serde_json::to_vec(source).unwrap()).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"));
    command.args(["synth", "completion"]);
    command
}
fn run(command: &mut Command) -> (i32, Value) {
    let output = command.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)));
    assert_eq!(report["authority"], "none");
    (output.status.code().unwrap(), report)
}
fn source() -> Value {
    json!({
        "schema": "zeno-fcis/completion-problem/1",
        "profile": "zeno-fcis/finite-i64/1",
        "state": [{"name": "count", "type": {"kind": "int", "min": 0, "max": 3}}],
        "commands": [{"name": "exit", "type": {"kind": "bool"}}],
        "step": {"nodes": [["input", 0], ["input", 1], ["int", 0], ["int", 1],
            ["lt", 2, 0], ["and", 1, 4], ["sub", 0, 3], ["select", 5, 6, 0]], "roots": [5, 7]},
        "terminal": {"nodes": [["input", 0], ["int", 0], ["eq", 0, 1]], "roots": [2]}
    })
}
fn case(directory: &Directory) -> (PathBuf, PathBuf, Value) {
    let problem = directory.write("source.json", &source());
    let out = directory.path("case");
    let (exit, report) = run(cli().arg("find").arg(&problem).arg("--out").arg(&out));
    assert_eq!(exit, 0);
    (problem, out, report)
}
fn names(path: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn discovery_find_verify_and_replay_preserve_independent_problem_binding() {
    let (exit, discovery) = run(cli().arg("discover"));
    assert_eq!(exit, 0);
    assert_eq!(
        discovery["problem_schema"],
        "zeno-fcis/completion-problem/1"
    );
    assert_eq!(discovery["hard_limits"]["source_bytes"], 262_144);
    assert_eq!(discovery["instructions"].as_array().unwrap().len(), 10);
    let directory = Directory::new();
    let (problem, out, found) = case(&directory);
    assert_eq!(found["status"], "verified");
    assert_eq!(found["states_checked"], 4);
    assert_eq!(found["commands_per_state"], 2);
    assert_eq!(found["maximum_exit_steps"], 3);
    assert_eq!(names(&out), ["plan.zcve", "problem.json", "result.json"]);
    let recorded = fs::read(out.join("result.json")).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&recorded).unwrap(), found);
    let (exit, verified) = run(cli()
        .arg("verify")
        .arg(&problem)
        .arg("--plan")
        .arg(out.join("plan.zcve")));
    assert_eq!((exit, verified), (0, found.clone()));
    let (exit, replayed) = run(cli().arg("replay").arg(&problem).arg("--case").arg(&out));
    let mut expected = found;
    expected["replay"] = json!("matched");
    assert_eq!((exit, replayed), (0, expected));
    assert_eq!(fs::read(out.join("result.json")).unwrap(), recorded);
    let mut different = source();
    different["terminal"] = json!({"nodes": [["bool", true]], "roots": [0]});
    let different = directory.write("different.json", &different);
    let (exit, report) = run(cli()
        .arg("verify")
        .arg(&different)
        .arg("--plan")
        .arg(out.join("plan.zcve")));
    assert_eq!(exit, 1);
    assert_eq!(report["detail"]["reason"], "problem-binding");
    let (exit, report) = run(cli().arg("replay").arg(&different).arg("--case").arg(&out));
    assert_eq!(exit, 1);
    assert_eq!(report["status"], "artifact-drift");
}

#[test]
fn failure_cases_are_inert_portable_and_replay_the_actual_claim_exit_code() {
    let directory = Directory::new();
    let mut no_exit = source();
    no_exit["step"] = json!({"nodes": [["bool", true], ["input", 0]], "roots": [0, 1]});
    let mut trap = source();
    trap["step"]["nodes"].as_array_mut().unwrap().extend([
        json!(["int", i64::MAX]),
        json!(["int", 1]),
        json!(["add", 8, 9]),
    ]);
    let mut rejected_change = source();
    rejected_change["step"] = json!({"nodes": [["bool", false], ["int", 0]], "roots": [0, 1]});
    let mut limited = source();
    limited["limits"] = json!({"max_transitions": 0});
    for (index, (source, expected_exit, status)) in [
        (no_exit, 1, "no-exit"),
        (trap, 1, "evaluation-trap"),
        (rejected_change, 1, "rejected-state-change"),
        (limited, 2, "resource-limit"),
    ]
    .into_iter()
    .enumerate()
    {
        let problem = directory.write(&format!("source-{index}.json"), &source);
        let out = directory.path(&format!("case-{index}"));
        let (exit, found) = run(cli().arg("find").arg(&problem).arg("--out").arg(&out));
        assert_eq!(exit, expected_exit);
        assert_eq!(found["status"], status);
        assert_eq!(names(&out), ["problem.json", "result.json"]);
        assert!(
            !String::from_utf8(fs::read(out.join("result.json")).unwrap())
                .unwrap()
                .contains(directory.0.to_str().unwrap())
        );
        if status == "no-exit" {
            assert_eq!(found["detail"]["state"], json!([1]));
        }
        if status == "resource-limit" {
            assert_eq!(found["detail"]["required"], 8);
        }
        let (exit, replayed) = run(cli().arg("replay").arg(&problem).arg("--case").arg(&out));
        let mut expected = found;
        expected["replay"] = json!("matched");
        assert_eq!((exit, replayed), (expected_exit, expected));
        fs::write(out.join("plan.zcve"), b"claimed authority").unwrap();
        let (exit, report) = run(cli().arg("replay").arg(&problem).arg("--case").arg(&out));
        assert_eq!(exit, 1);
        assert_eq!(report["status"], "artifact-drift");
    }
}

#[test]
fn source_grammar_remains_closed_and_rejects_primitive_aliases() {
    let directory = Directory::new();
    let mut changes = Vec::new();
    for (key, value) in [
        ("schema", json!("wrong/1")),
        ("profile", json!("temporal/1")),
        ("extra", json!(true)),
        ("limits", Value::Null),
        ("limits", json!({"extra": 1})),
        ("limits", json!({"max_steps": true})),
        ("limits", json!({"max_steps": "100"})),
    ] {
        let mut changed = source();
        changed[key] = value;
        changes.push(changed);
    }
    for op in [
        json!(["input", true]),
        json!(["input", 0.0]),
        json!(["input", "0"]),
        json!(["input", 0, 1]),
        json!(["read_file", "/not-an-effect"]),
    ] {
        let mut changed = source();
        changed["step"]["nodes"][0] = op;
        changes.push(changed);
    }
    let mut changed = source();
    changed["state"][0]["type"]["min"] = json!(false);
    changes.push(changed);
    let mut changed = source();
    changed["state"][0]["name"] = json!("../../state");
    changes.push(changed);
    let mut changed = source();
    changed["commands"][0]["type"]["extra"] = json!(0);
    changes.push(changed);
    let mut changed = source();
    let duplicate = changed["state"][0].clone();
    changed["state"].as_array_mut().unwrap().push(duplicate);
    changes.push(changed);
    for (index, source) in changes.iter().enumerate() {
        let problem = directory.write(&format!("invalid-{index}.json"), source);
        let out = directory.path(&format!("invalid-{index}"));
        let (exit, report) = run(cli().arg("find").arg(&problem).arg("--out").arg(&out));
        assert_eq!(exit, 1, "{source}: {report}");
        assert_eq!(report["status"], "invalid-problem");
        assert!(!out.exists());
    }
    let raw = serde_json::to_string(&source()).unwrap();
    for (index, malformed) in [
        raw.replacen('{', "{\"schema\":\"duplicate\",", 1),
        raw.replacen("\"step\":{", "\"step\":{\"nodes\":[],", 1),
    ]
    .iter()
    .enumerate()
    {
        let problem = directory.path(&format!("duplicate-{index}.json"));
        fs::write(&problem, malformed).unwrap();
        assert_eq!(
            run(cli()
                .arg("find")
                .arg(&problem)
                .arg("--out")
                .arg(directory.path(&format!("duplicate-{index}"))))
            .0,
            1
        );
    }
    let problem = directory.path("oversized.json");
    fs::write(&problem, vec![b' '; 262_145]).unwrap();
    let (exit, report) = run(cli()
        .arg("find")
        .arg(&problem)
        .arg("--out")
        .arg(directory.path("oversized")));
    assert_eq!(exit, 2);
    assert_eq!(report["status"], "byte-limit");
}

#[test]
fn plan_file_import_rejects_hostile_bytes_before_they_can_become_evidence() {
    let directory = Directory::new();
    let (problem, out, _) = case(&directory);
    let bytes = fs::read(out.join("plan.zcve")).unwrap();
    let plan = directory.path("hostile.zcve");
    for length in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        fs::write(&plan, &bytes[..length]).unwrap();
        assert_eq!(
            run(cli().arg("verify").arg(&problem).arg("--plan").arg(&plan)).0,
            1
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    fs::write(&plan, trailing).unwrap();
    let (exit, report) = run(cli().arg("verify").arg(&problem).arg("--plan").arg(&plan));
    assert_eq!(exit, 1);
    assert_eq!(report["status"], "invalid-plan-bytes");
    let mut changed = bytes.clone();
    let profile = b"zeno-fcis/completion-plan/1";
    let position = changed
        .windows(profile.len())
        .position(|part| part == profile)
        .unwrap();
    changed[position + profile.len() - 1] = b'0';
    fs::write(&plan, changed).unwrap();
    let (exit, report) = run(cli().arg("verify").arg(&problem).arg("--plan").arg(&plan));
    assert_eq!(exit, 1);
    assert_eq!(report["detail"]["reason"], "plan-profile");
    fs::File::create(&plan)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let (exit, report) = run(cli().arg("verify").arg(&problem).arg("--plan").arg(&plan));
    assert_eq!(exit, 2);
    assert_eq!(report["status"], "byte-limit");
    let mut huge_collection = vec![0x08];
    huge_collection.extend_from_slice(&u32::MAX.to_be_bytes());
    fs::write(&plan, huge_collection).unwrap();
    let (exit, report) = run(cli().arg("verify").arg(&problem).arg("--plan").arg(&plan));
    assert_eq!(exit, 2);
    assert_eq!(report["detail"]["resource"], "collection-items");
    assert_eq!(fs::read(out.join("plan.zcve")).unwrap(), bytes);
}

#[test]
fn replay_compares_every_byte_and_the_complete_closed_file_set() {
    let directory = Directory::new();
    let (problem, out, _) = case(&directory);
    for name in ["problem.json", "plan.zcve", "result.json"] {
        let path = out.join(name);
        let original = fs::read(&path).unwrap();
        let mut altered = original.clone();
        altered.push(b'\n');
        fs::write(&path, altered).unwrap();
        let (exit, report) = run(cli().arg("replay").arg(&problem).arg("--case").arg(&out));
        assert_eq!(exit, 1);
        assert_eq!(report["status"], "artifact-drift");
        fs::write(&path, &original).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(
            run(cli().arg("replay").arg(&problem).arg("--case").arg(&out)).0,
            1
        );
        fs::write(&path, original).unwrap();
    }
    fs::create_dir(out.join("extra")).unwrap();
    let (exit, report) = run(cli().arg("replay").arg(&problem).arg("--case").arg(&out));
    assert_eq!(exit, 1);
    assert_eq!(report["detail"]["reason"], "file-set");
    fs::remove_dir(out.join("extra")).unwrap();
    assert_eq!(
        run(cli().arg("replay").arg(&problem).arg("--case").arg(&out)).0,
        0
    );
}

#[test]
fn existing_output_and_io_failures_preserve_files_and_hide_host_paths() {
    let directory = Directory::new();
    let (problem, out, _) = case(&directory);
    let original = fs::read(out.join("result.json")).unwrap();
    let (exit, report) = run(cli().arg("find").arg(&problem).arg("--out").arg(&out));
    assert_eq!(exit, 1);
    assert_eq!(report["status"], "output-exists");
    assert_eq!(fs::read(out.join("result.json")).unwrap(), original);
    let empty = directory.path("empty");
    fs::create_dir(&empty).unwrap();
    assert_eq!(
        run(cli().arg("find").arg(&problem).arg("--out").arg(&empty)).0,
        1
    );
    assert_eq!(names(&empty), Vec::<String>::new());
    let missing = directory.path("private-missing.json");
    for command in [
        cli()
            .arg("find")
            .arg(&missing)
            .arg("--out")
            .arg(directory.path("missing-case"))
            .output()
            .unwrap(),
        cli()
            .arg("verify")
            .arg(&problem)
            .arg("--plan")
            .arg(&missing)
            .output()
            .unwrap(),
    ] {
        assert_eq!(command.status.code(), Some(3));
        let report: Value = serde_json::from_slice(&command.stdout).unwrap();
        assert_eq!(report["status"], "io-error");
        assert!(
            !String::from_utf8(command.stdout)
                .unwrap()
                .contains(directory.0.to_str().unwrap())
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_inputs_outputs_and_case_members_cannot_redirect_the_file_boundary() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let (problem, out, _) = case(&directory);
    let linked_problem = directory.path("linked-source");
    symlink(&problem, &linked_problem).unwrap();
    assert_eq!(
        run(cli()
            .arg("find")
            .arg(&linked_problem)
            .arg("--out")
            .arg(directory.path("linked-find")))
        .0,
        1
    );
    let linked_plan = directory.path("linked-plan");
    symlink(out.join("plan.zcve"), &linked_plan).unwrap();
    assert_eq!(
        run(cli()
            .arg("verify")
            .arg(&problem)
            .arg("--plan")
            .arg(&linked_plan))
        .0,
        1
    );
    let linked_case = directory.path("linked-case");
    symlink(&out, &linked_case).unwrap();
    assert_eq!(
        run(cli()
            .arg("replay")
            .arg(&problem)
            .arg("--case")
            .arg(&linked_case))
        .0,
        1
    );
    assert_eq!(
        run(cli()
            .arg("find")
            .arg(&problem)
            .arg("--out")
            .arg(&linked_case))
        .0,
        1
    );
    let original = fs::read(out.join("result.json")).unwrap();
    let outside = directory.path("outside.json");
    fs::write(&outside, &original).unwrap();
    fs::remove_file(out.join("result.json")).unwrap();
    symlink(&outside, out.join("result.json")).unwrap();
    assert_eq!(
        run(cli().arg("replay").arg(&problem).arg("--case").arg(&out)).0,
        1
    );
    assert_eq!(fs::read(&outside).unwrap(), original);
}
