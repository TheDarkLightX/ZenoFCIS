//! `zeno-fcis loop` doors through the real binary: `candidate --expressions`,
//! `sketch`, `family` and `transfer`.
//!
//! Every program a door reports as accepted, completed or "checked repair"
//! is checked again here by a separate `transform check --receipt`, whose
//! receipt must be byte for byte the one the door reported (by digest), and
//! replayed by `transform replay`.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
const COUNTER_C8: &str = "authorized ? action - (value == (action == Inc ? 8 : 0) ? 149 : 147) : 0";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-loop-doors-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("mkdir {name}: {error}"));
        }
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path
    }

    fn json(&self, name: &str, value: &Value) -> PathBuf {
        self.write(name, value.to_string().as_bytes())
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// An exported core-component program (exported by this tree's
/// `contract export-program`; shared with the loop-doors crate's tests).
fn exported(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/i128-door-programs")
        .join(format!("{name}.original.zcve"));
    fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn run(arguments: &[&str]) -> (Option<i32>, Value) {
    let output: Output = Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn sha256(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn counter_vocabulary() -> Value {
    json!({"inputs": ["value", "action", "authorized"],
           "variants": {"action": {"Inc": 150, "Dec": 151}}, "cases": 5})
}

fn document(outputs: &[&str]) -> Value {
    json!({"schema": "zeno-fcis/loop-expressions/1", "vocabulary": counter_vocabulary(), "outputs": outputs})
}

fn decimal(value: &Value) -> i128 {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or_else(|| panic!("decimal {value}"))
}

/// Canonical bytes of a program given in the fixture vocabulary.
fn encode(program: &Value) -> Vec<u8> {
    let domain = |d: &Value| match d["kind"].as_str() {
        Some("Bool") => Domain::Bool,
        Some("Int") => Domain::Int {
            min: decimal(&d["min"]),
            max: decimal(&d["max"]),
        },
        _ => panic!("domain {d}"),
    };
    let list = |key: &str| {
        program[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key}"))
            .clone()
    };
    let id = |v: &Value| u16::try_from(v.as_u64().unwrap_or(u64::MAX)).unwrap_or(u16::MAX);
    let nodes = list("nodes")
        .iter()
        .map(|node| {
            let n = node.as_array().unwrap_or_else(|| panic!("node"));
            match n[0].as_str().unwrap_or_default() {
                "Input" => Op::Input(id(&n[1])),
                "Int" => Op::Int(decimal(&n[1])),
                "Bool" => Op::Bool(n[1].as_bool().unwrap_or_default()),
                "Add" => Op::Add(id(&n[1]), id(&n[2])),
                "Sub" => Op::Sub(id(&n[1]), id(&n[2])),
                "Mul" => Op::Mul(id(&n[1]), id(&n[2])),
                "DivFloor" => Op::DivFloor(id(&n[1]), id(&n[2])),
                "DivCeil" => Op::DivCeil(id(&n[1]), id(&n[2])),
                "MulDivFloor" => Op::MulDivFloor(id(&n[1]), id(&n[2]), id(&n[3])),
                "MulDivCeil" => Op::MulDivCeil(id(&n[1]), id(&n[2]), id(&n[3])),
                "ProductsGe" => Op::ProductsGe(
                    [id(&n[1]), id(&n[2]), id(&n[3])],
                    [id(&n[4]), id(&n[5]), id(&n[6])],
                ),
                "Eq" => Op::Eq(id(&n[1]), id(&n[2])),
                "Lt" => Op::Lt(id(&n[1]), id(&n[2])),
                "And" => Op::And(id(&n[1]), id(&n[2])),
                "Not" => Op::Not(id(&n[1])),
                "Select" => Op::Select(id(&n[1]), id(&n[2]), id(&n[3])),
                other => panic!("opcode {other}"),
            }
        })
        .collect();
    let program = Program::try_new(
        list("inputs").iter().map(domain).collect(),
        list("outputs").iter().map(domain).collect(),
        nodes,
        list("roots").iter().map(id).collect(),
    )
    .unwrap_or_else(|error| panic!("admit: {error:?}"));
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("encode: {error:?}"))
}

/// The independent confirmation: `transform check --receipt` must give the
/// receipt the door reported (same digest), and `transform replay` must
/// match it.
fn confirm(directory: &Directory, original: &[u8], program: &Value, check: &Value, label: &str) {
    assert_eq!(check["verdict"], "equivalent", "{label}: {check}");
    assert_eq!(check["replay"], "matched", "{label}: {check}");
    let candidate = encode(program);
    assert_eq!(
        check["candidate_sha256"],
        json!(sha256(&candidate)),
        "{label}"
    );
    let original_path = directory.write(&format!("{label}/original.zcve"), original);
    let candidate_path = directory.write(&format!("{label}/candidate.zcve"), &candidate);
    let receipt = directory.0.join(format!("{label}/receipt.json"));
    let (exit, report) = run(&[
        "transform",
        "check",
        "--original",
        &original_path.to_string_lossy(),
        "--candidate",
        &candidate_path.to_string_lossy(),
        "--receipt",
        &receipt.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{label}: {report}");
    let bytes = fs::read(&receipt).unwrap_or_else(|error| panic!("receipt: {error}"));
    assert_eq!(
        json!(sha256(&bytes)),
        check["receipt_sha256"],
        "{label}: the door's receipt is this receipt"
    );
    let (exit, report) = run(&[
        "transform",
        "replay",
        "--receipt",
        &receipt.to_string_lossy(),
        "--original",
        &original_path.to_string_lossy(),
        "--candidate",
        &candidate_path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{label}: {report}");
}

fn open(directory: &Directory, name: &str) -> (String, Vec<u8>) {
    open_with(directory, name, &[])
}

#[test]
fn main_route_arithmetic_reaches_exact_checks_replay_and_point_evaluation() {
    for (label, instruction, text, wrong, expected, boolean) in [
        (
            "floor",
            json!(["MulDivFloor", 0, 1, 2]),
            "mul_div_floor(x, 3, 2)",
            "mul_div_ceil(x, 3, 2)",
            "10",
            false,
        ),
        (
            "ceil",
            json!(["MulDivCeil", 0, 1, 2]),
            "mul_div_ceil(x, 3, 2)",
            "mul_div_floor(x, 3, 2)",
            "11",
            false,
        ),
        (
            "two",
            json!(["ProductsGe", 0, 2, 3, 1, 3, 3]),
            "products_ge(x, 2, 3, 1)",
            "products_ge(x, 1, 3, 1)",
            "1",
            true,
        ),
        (
            "three",
            json!(["ProductsGe", 0, 2, 3, 1, 3, 3]),
            "products_ge(x, 2, 1, 3, 1, 1)",
            "products_ge(x, 1, 1, 3, 1, 1)",
            "1",
            true,
        ),
    ] {
        let directory = Directory::new();
        let output = if boolean {
            json!({"kind": "Bool"})
        } else {
            json!({"kind": "Int", "min": "0", "max": "100"})
        };
        let nodes = if boolean {
            json!([
                ["Input", 0],
                ["Int", "3"],
                ["Int", "2"],
                ["Int", "1"],
                instruction
            ])
        } else {
            json!([["Input", 0], ["Int", "3"], ["Int", "2"], instruction])
        };
        let original = encode(
            &json!({"inputs": [{"kind": "Int", "min": "0", "max": "40"}],
            "outputs": [output], "nodes": nodes, "roots": [if boolean { 4 } else { 3 }]}),
        );
        let original_path = directory.write("original.zcve", &original);
        let session = directory.path("session");
        let (exit, report) = run(&[
            "loop",
            "open",
            "--original",
            &original_path.to_string_lossy(),
            "--session",
            &session,
            "--profile",
            "checked-i128-v1",
        ]);
        assert_eq!(exit, Some(0), "{label}: {report}");
        let make_document = |expression: &str| {
            json!({"schema": "zeno-fcis/loop-expressions/1",
            "vocabulary": {"inputs": ["x"]}, "outputs": [expression]})
        };
        let right = directory.json("right.json", &make_document(text));
        let (exit, report) = run(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--expressions",
            &right.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(0), "{label}: {report}");
        let door = &report["detail"]["door"];
        confirm(
            &directory,
            &original,
            &door["lowered"]["program"],
            &door["check"],
            label,
        );
        let (exit, observed) = run(&[
            "loop",
            "evaluate",
            "--original",
            &original_path.to_string_lossy(),
            "--expressions",
            &right.to_string_lossy(),
            "--input=7",
        ]);
        assert_eq!(exit, Some(0), "{observed}");
        assert_eq!(observed["detail"]["values"], json!([expected]));
        assert_eq!(observed["authority"], "none");
        let wrong = directory.json("wrong.json", &make_document(wrong));
        let (exit, report) = run(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--expressions",
            &wrong.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(1), "{label}: {report}");
        assert_eq!(report["status"], "different", "{report}");
        let comparison = |good: bool, bad: bool, compare: &dyn Fn(bool, bool) -> bool| {
            compare(good, true) && !compare(bad, true)
        };
        assert!(comparison(true, false, &|a, b| a == b));
        assert!(
            !comparison(true, false, &|_, _| true),
            "stubbed comparison must fail"
        );
        for refused in [
            "mul_div_floor(x, 1, 0)",
            "mul_div_ceil(x, -1, 1)",
            "products_ge(x, 1, -1, 1)",
        ] {
            let path = directory.json(
                &format!("refused-{}.json", sha256(refused.as_bytes())),
                &make_document(refused),
            );
            let (exit, report) = run(&[
                "loop",
                "evaluate",
                "--original",
                &original_path.to_string_lossy(),
                "--expressions",
                &path.to_string_lossy(),
                "--input=7",
            ]);
            assert_eq!(exit, Some(1), "{report}");
            assert_eq!(report["status"], "refused");
        }
    }
}

fn open_with(directory: &Directory, name: &str, extra: &[&str]) -> (String, Vec<u8>) {
    let original = exported(name);
    let path = directory.write(&format!("{name}.zcve"), &original);
    let session = directory.path(&format!("{name}-session"));
    let path = path.to_string_lossy();
    let mut arguments = vec![
        "loop",
        "open",
        "--original",
        &path,
        "--session",
        &session,
        "--profile",
        "checked-i128-v1",
    ];
    arguments.extend_from_slice(extra);
    let (exit, report) = run(&arguments);
    assert_eq!(exit, Some(0), "{report}");
    (session, original)
}

#[test]
fn a_wrong_expression_gets_its_region_sites_and_a_checked_repair() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "bounded-counter-c8");
    let wrong = COUNTER_C8.replace("? 8 :", "? 7 :");
    let path = directory.json("wrong.json", &document(&[&wrong, "value + 1", "value - 1"]));
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["authority"], "none");
    assert_eq!(report["status"], "different");
    let door = &report["detail"]["door"];
    assert_eq!(door["lowering"], "lowered");
    let explanation = &door["explanation"];
    assert_eq!(explanation["region"]["tuples"], 2);
    assert_eq!(explanation["region"]["of"], 36);
    let top = &explanation["sites"][0];
    assert_eq!(
        (top["subexpression"].clone(), top["replacement"].clone()),
        (json!("7"), json!("8"))
    );
    let repair = &explanation["repair"];
    assert_eq!(repair["label"], "checked repair", "{repair}");
    confirm(
        &directory,
        &original,
        &repair["program"]["program"],
        &repair["check"],
        "repair",
    );
    // the session spent exactly one attempt, like a JSON candidate
    assert_eq!(report["detail"]["attempt"], 0);
    // a correct expression is accepted through the session and confirmed
    let right = directory.json(
        "right.json",
        &document(&[COUNTER_C8, "value + 1", "value - 1"]),
    );
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &right.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "checked-improvement");
    let door = &report["detail"]["door"];
    confirm(
        &directory,
        &original,
        &door["lowered"]["program"],
        &door["check"],
        "accepted",
    );
}

#[test]
fn a_root_fault_is_repaired_by_its_one_step_edit() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "consumable-budget-c6");
    let path = directory.json(
        "root.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1",
                "vocabulary": {"inputs": ["remaining", "amount", "authorized"]},
                "outputs": ["authorized ? (remaining < amount ? 1 : 2) : 0", "remaining + amount"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    let explanation = &report["detail"]["door"]["explanation"];
    let top = &explanation["sites"][0];
    assert_eq!(top["path"], json!([]));
    assert_eq!(top["kind"], "edit");
    assert_eq!(top["replacement"], "remaining - amount");
    let repair = &explanation["repair"];
    assert_eq!(repair["label"], "checked repair");
    confirm(
        &directory,
        &original,
        &repair["program"]["program"],
        &repair["check"],
        "root",
    );
}

#[test]
fn a_sketch_completes_or_is_unrealizable_with_a_witness() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "bounded-counter-c8");
    let sketch = directory.json(
        "sketch.json",
        &document(&[
            "authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0",
            "keep",
            "keep",
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &sketch.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let door = &report["detail"]["door"];
    assert_eq!(door["status"], "completed");
    assert_eq!(door["holes"]["0"]["fill"], "8");
    assert_eq!(door["holes"]["1"]["fill"], "149");
    assert_eq!(door["holes"]["2"]["fill"], "147");
    confirm(
        &directory,
        &original,
        &door["completion"]["program"],
        &door["check"],
        "completion",
    );
    let none = directory.json(
        "none.json",
        &document(&["authorized ? action - 149 : ?c", "keep", "keep"]),
    );
    let (exit, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &none.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    let door = &report["detail"]["door"];
    assert_eq!(door["status"], "unrealizable");
    assert_eq!(door["evidence"]["kind"], "witness");
    assert_eq!(door["evidence"]["input"]["authorized"], true);
    assert_eq!(
        report["detail"]["outcome"]["outcome"],
        "proposal-failed:empty"
    );
    // an expression hole: completed by synthesis, or not found, never "impossible"
    let open_hole = directory.json(
        "e.json",
        &document(&["authorized ? ?e : 0", "keep", "keep"]),
    );
    let (_, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &open_hole.to_string_lossy(),
    ]);
    let status = report["detail"]["door"]["status"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        ["completed", "not-found"].contains(&status.as_str()),
        "{report}"
    );
}

fn counter_manifest(directory: &Directory, fitted: &[i128]) -> PathBuf {
    let mut instances = Vec::new();
    for c in 1..=8 {
        let name = format!("bounded-counter-c{c}");
        directory.write(&format!("family/{name}/original.zcve"), &exported(&name));
        instances.push(
            json!({"name": name, "parameters": {"C": c}, "program": format!("{name}/original.zcve"),
                              "fit": fitted.contains(&c)}),
        );
    }
    directory.json(
        "family/manifest.json",
        &json!({"schema": "zeno-fcis/loop-family-manifest/1", "family": "bounded-counter",
                "vocabulary": counter_vocabulary(), "instances": instances}),
    )
}

#[test]
fn a_family_program_is_checked_on_every_instance_and_its_fit_on_unfitted_ones() {
    let directory = Directory::new();
    let manifest = counter_manifest(&directory, &[1, 2, 3]);
    let program = directory.json(
        "family.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1",
                "outputs": [COUNTER_C8.replace("? 8 :", "? ?c :"), "value + 1", "value - 1"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &program.to_string_lossy(),
        "--fit",
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["schema"], "zeno-fcis/loop-doors/2");
    assert_eq!(report["authority"], "none");
    let detail = &report["detail"];
    assert_eq!(detail["accepted"], 8);
    assert_eq!(detail["coverage"], "checked on these 8 instances");
    assert_eq!(detail["fits"]["0"]["text"], "$C");
    let lifted = &detail["lifted_check"];
    assert_eq!(lifted["verdict"], "accepted");
    assert_eq!(lifted["unfitted_instances_checked"], 5);
    assert_eq!(lifted["unfitted_instances_accepted"], 5);
    for row in lifted["rows"].as_array().unwrap_or(&Vec::new()) {
        let name = row["instance"].as_str().unwrap_or_default();
        confirm(
            &directory,
            &exported(name),
            &row["program"]["program"],
            &row["check"],
            &format!("lifted-{name}"),
        );
    }
    // a wrong family program: refuted, with the smallest failing instance
    let wrong = directory.json(
        "wrong.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1",
                "outputs": [COUNTER_C8.replace("? 8 :", "? $C + 1 :"), "value + 1", "value - 1"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &wrong.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(
        report["detail"]["smallest_failing"]["instance"],
        "bounded-counter-c1"
    );
    assert_eq!(
        report["detail"]["smallest_failing"]["check"]["verdict"],
        "counterexample"
    );
    // an unrealizable sketch is the doors' claim, never a checker refutation
    let unrealizable = directory.json(
        "unrealizable.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1",
                "outputs": [COUNTER_C8.replace("? 8 :", "? ?c[100..101] :"), "value + 1", "value - 1"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &unrealizable.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(2), "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["verdict"], "unrealizable", "{report}");
    assert_eq!(detail["unrealizable"], 8, "{report}");
    assert_eq!(detail["smallest_failing"], Value::Null, "{report}");
    for row in detail["rows"].as_array().unwrap_or(&Vec::new()) {
        assert_eq!(row["verdict"], "unrealizable", "{row}");
        assert_eq!(row["sketch"]["status"], "unrealizable", "{row}");
        assert!(row["sketch"]["evidence"]["kind"].is_string(), "{row}");
        assert!(
            row.get("check").is_none(),
            "no checker was consulted: {row}"
        );
    }
}

#[test]
fn a_program_transfers_across_parameters_and_every_result_is_checked() {
    let directory = Directory::new();
    let manifest = counter_manifest(&directory, &[]);
    let source = directory.json(
        "source.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1", "vocabulary": counter_vocabulary(),
                "parameters": {"C": 3},
                "outputs": [COUNTER_C8.replace("? 8 :", "? 3 :"), "value + 1", "value - 1"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "transfer",
        "--from",
        &source.to_string_lossy(),
        "--to-instances",
        &manifest.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["transferred"], 8);
    assert_eq!(detail["by"]["direct"], 1);
    for row in detail["rows"].as_array().unwrap_or(&Vec::new()) {
        let name = row["instance"].as_str().unwrap_or_default();
        confirm(
            &directory,
            &exported(name),
            &row["program"]["program"],
            &row["check"],
            &format!("transfer-{name}"),
        );
    }
}

#[test]
fn malformed_doors_are_refused_before_any_work() {
    let directory = Directory::new();
    let manifest = counter_manifest(&directory, &[]);
    let escape = directory.json(
        "family/escape.json",
        &json!({"schema": "zeno-fcis/loop-family-manifest/1", "family": "bounded-counter",
                "vocabulary": counter_vocabulary(),
                "instances": [{"name": "x", "parameters": {"C": 1}, "program": "../outside.zcve"}]}),
    );
    let program = directory.json(
        "p.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1", "outputs": ["$C"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "family",
        "--instances",
        &escape.to_string_lossy(),
        "--program",
        &program.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refused");
    assert_eq!(report["schema"], "zeno-fcis/loop-doors/2");
    let unknown = directory.json(
        "u.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1", "outputs": ["$K", "keep", "keep"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &unknown.to_string_lossy(),
    ]);
    assert_eq!(
        (exit, report["status"].clone()),
        (Some(1), json!("refused")),
        "{report}"
    );
    let big = directory.write("big.json", &vec![b' '; 70_000]);
    let (session, _) = open(&directory, "bounded-counter-c8");
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &big.to_string_lossy(),
    ]);
    assert_eq!(
        (exit, report["status"].clone()),
        (Some(1), json!("refused")),
        "{report}"
    );
    // an unknown name is a malformed proposal: it consumes the attempt, like a JSON candidate
    let typo = directory.json("typo.json", &document(&["valeu + 1", "keep", "keep"]));
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &typo.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["door"]["lowering"], "refused");
    assert!(
        report["status"]
            .as_str()
            .unwrap_or_default()
            .starts_with("proposal-failed:malformed"),
        "{report}"
    );
    // so is an undecodable document, of either door: not JSON, or the wrong shape
    let not_json = directory.write("not.json", b"value + 1");
    let shape = directory.json("shape.json", &json!({"outputs": "value + 1"}));
    for (attempt, (door, path)) in [("--expressions", &not_json), ("--expressions", &shape)]
        .into_iter()
        .chain([("--sketch", &not_json)])
        .enumerate()
    {
        let command = if door == "--sketch" {
            "sketch"
        } else {
            "candidate"
        };
        let (exit, report) = run(&[
            "loop",
            command,
            "--session",
            &session,
            door,
            &path.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(1), "{report}");
        assert_eq!(report["detail"]["attempt"], json!(attempt + 1), "{report}");
        assert!(
            report["status"]
                .as_str()
                .unwrap_or_default()
                .starts_with("proposal-failed:malformed"),
            "{report}"
        );
    }
}

#[test]
fn a_door_that_cannot_confirm_in_time_is_inconclusive_and_spends_nothing() {
    let directory = Directory::new();
    // Too short for any contained stage: neither the confirming check nor
    // the sketch's solving can run.
    let (session, _) = open_with(&directory, "bounded-counter-c8", &["--deadline-ms", "20"]);
    let right = directory.json(
        "right.json",
        &document(&[COUNTER_C8, "value + 1", "value - 1"]),
    );
    let sketch = directory.json(
        "sketch.json",
        &document(&[
            "authorized ? action - (value == (action == Inc ? ?c : 0) ? 149 : 147) : 0",
            "keep",
            "keep",
        ]),
    );
    for (command, door, path) in [
        ("candidate", "--expressions", &right),
        ("sketch", "--sketch", &sketch),
    ] {
        let (exit, report) = run(&[
            "loop",
            command,
            "--session",
            &session,
            door,
            &path.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(2), "{report}");
        assert_eq!(report["status"], "inconclusive", "{report}");
        let detail = &report["detail"];
        assert_ne!(detail["door"]["check"]["verdict"], "equivalent", "{report}");
        // no attempt spent, the incumbent unchanged and the session still open
        assert_ne!(report["accounting_complete"], false, "{report}");
        assert!(
            report["acquisition"]["observed_intake"].is_object(),
            "{report}"
        );
        assert_eq!(detail["report"]["accounting"]["complete"], true, "{report}");
        assert_eq!(detail["report"]["attempts"], json!([]), "{report}");
        assert_eq!(
            detail["report"]["status"], "no-checked-improvement",
            "{report}"
        );
        assert_eq!(detail["report"]["session"]["state"], "open", "{report}");
    }
}

#[test]
fn a_sketch_search_stopped_by_its_budget_is_inconclusive_and_spends_nothing() {
    let directory = Directory::new();
    // The constant 16 as a sum of sixteen 0/1 holes: the only completion is
    // all ones, which the search over meanings reaches past its default
    // budget, so the solver itself returns inconclusive.
    let original = encode(&json!({
        "inputs": [{"kind": "Bool"}],
        "outputs": [{"kind": "Int", "min": "16", "max": "16"}],
        "nodes": [["Int", "16"]],
        "roots": [0]
    }));
    let path = directory.write("sixteen.zcve", &original);
    let session = directory.path("sixteen-session");
    let (exit, report) = run(&[
        "loop",
        "open",
        "--original",
        &path.to_string_lossy(),
        "--session",
        &session,
        "--profile",
        "checked-i128-v1",
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let sum = ["?c[0..1]"; 16].join(" + ");
    let sketch = directory.json(
        "sixteen.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1", "vocabulary": {"inputs": ["flag"]},
                "outputs": [sum]}),
    );
    let (exit, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &sketch.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "inconclusive", "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["reason"], "door-search-not-completed", "{report}");
    // the solver ran and stopped on its budget (not the stage deadline)
    assert_eq!(detail["door"]["status"], "inconclusive", "{report}");
    assert!(detail["door"]["searched"].is_array(), "{report}");
    assert_eq!(detail["report"]["attempts"], json!([]), "{report}");
    assert_eq!(detail["report"]["session"]["state"], "open", "{report}");
}

#[test]
fn stateless_doors_print_identical_reports_on_repeat() {
    let directory = Directory::new();
    let manifest = counter_manifest(&directory, &[2, 5, 8]);
    let program = directory.json(
        "family.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1",
                "outputs": [COUNTER_C8.replace("? 8 :", "? ?c :"), "value + 1", "value - 1"]}),
    );
    let family = [
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &program.to_string_lossy(),
        "--fit",
    ]
    .map(str::to_owned);
    let family: Vec<&str> = family.iter().map(String::as_str).collect();
    let first = Command::new(CLI)
        .args(&family)
        .output()
        .unwrap_or_else(|error| panic!("run: {error}"));
    let second = Command::new(CLI)
        .args(&family)
        .output()
        .unwrap_or_else(|error| panic!("run: {error}"));
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(first.stdout, second.stdout, "byte-identical family reports");
}

/// `loop meaning`'s key for an expressions document, with the lowered program.
fn meaning_of(directory: &Directory, original: &str, label: &str, outputs: &[&str]) -> Value {
    let path = directory.json(&format!("meaning-{label}.json"), &document(outputs));
    let (exit, report) = run(&[
        "loop",
        "meaning",
        "--original",
        original,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    assert_eq!(
        exit,
        Some(if report["detail"]["same_as_original"] == true {
            0
        } else {
            1
        }),
        "{report}"
    );
    assert_eq!(report["status"], "keyed", "{report}");
    report["detail"].clone()
}

#[test]
fn meaning_verdict_exits_distinguish_same_different_and_over_budget() {
    let directory = Directory::new();
    let domain = |max: &str| json!({"kind": "Int", "min": "0", "max": max});
    let identity = |max: &str| json!({"inputs": [domain(max)], "outputs": [domain(max)], "nodes": [["Input", 0]], "roots": [0]});
    let original = directory.write("identity.zcve", &encode(&identity("1")));
    let zero = json!({"inputs": [domain("1")], "outputs": [domain("1")], "nodes": [["Int", "0"]], "roots": [0]});
    let different = directory.write("zero.zcve", &encode(&zero));
    let large = directory.write("large.zcve", &encode(&identity("131072")));
    for (left, right, expected) in [
        (&original, &original, 0),
        (&original, &different, 1),
        (&large, &large, 2),
    ] {
        let (exit, report) = run(&[
            "loop",
            "meaning",
            "--original",
            &left.to_string_lossy(),
            "--candidate",
            &right.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(expected), "{report}");
        if expected == 2 {
            assert_eq!(report["status"], "inconclusive");
        } else {
            assert_eq!(report["detail"]["same_as_original"], expected == 0);
        }
    }
}

#[test]
fn meaning_keys_are_equal_exactly_when_transform_check_finds_equivalence() {
    let directory = Directory::new();
    let original = exported("bounded-counter-c8");
    let original_path = directory.write("original.zcve", &original);
    let original_path = original_path.to_string_lossy().into_owned();
    let wrong = COUNTER_C8.replace("? 8 :", "? 7 :");
    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "right",
            vec![COUNTER_C8.into(), "value + 1".into(), "value - 1".into()],
        ),
        (
            "right-respelled",
            vec![
                "!authorized ? 0 : action - ((action == Inc ? 8 : 0) == value ? 149 : 147)".into(),
                "1 + value".into(),
                "value - 1".into(),
            ],
        ),
        (
            "wrong",
            vec![wrong.clone(), "value + 1".into(), "value - 1".into()],
        ),
        (
            "wrong-respelled",
            vec![
                "!authorized ? 0 : action - (value == (action == Dec ? 0 : 7) ? 149 : 147)".into(),
                "value + 1".into(),
                "value - 1".into(),
            ],
        ),
        (
            "other",
            vec![COUNTER_C8.into(), "value + 2".into(), "value - 1".into()],
        ),
        (
            "trapping",
            vec![
                COUNTER_C8.into(),
                "value + 9223372036854775800".into(),
                "value - 1".into(),
            ],
        ),
    ];
    let mut keyed = Vec::new();
    for (label, outputs) in &cases {
        let outputs: Vec<&str> = outputs.iter().map(String::as_str).collect();
        let detail = meaning_of(&directory, &original_path, label, &outputs);
        let bytes = encode(&detail["program"]["program"]);
        let path = directory.write(&format!("{label}.zcve"), &bytes);
        keyed.push((*label, detail, path));
    }
    assert_eq!(keyed[0].1["same_as_original"], true);
    assert_eq!(keyed[1].1["same_as_original"], true);
    assert_eq!(keyed[2].1["same_as_original"], false);
    // every pair: equal keys exactly when transform check finds the two equivalent
    for (a, key_a, path_a) in &keyed {
        for (b, key_b, path_b) in &keyed {
            let (exit, report) = run(&[
                "transform",
                "check",
                "--original",
                &path_a.to_string_lossy(),
                "--candidate",
                &path_b.to_string_lossy(),
            ]);
            let equivalent = exit == Some(0);
            assert!(
                equivalent || report["status"] == "counterexample",
                "{a} {b}: {report}"
            );
            assert_eq!(
                key_a["key"] == key_b["key"],
                equivalent,
                "{a} against {b}: {report}"
            );
        }
    }
    // the canonical form of a candidate gets the same key as its expressions
    let (exit, report) = run(&[
        "loop",
        "meaning",
        "--original",
        &original_path,
        "--candidate",
        &keyed[3].2.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["key"], keyed[2].1["key"]);
}

#[test]
fn a_family_sketch_stopped_by_its_budget_is_never_unrealizable() {
    // x + y over 0..=360 twice (130,321 inputs); the sketch completes with
    // ?c = 0 and any values for the other holes, but its padded branch makes
    // the search run out of work before it can enumerate the classes.
    let directory = Directory::new();
    let int = |min: i128, max: i128| json!({"kind": "Int", "min": min.to_string(), "max": max.to_string()});
    let program = json!({
        "inputs": [int(0, 360), int(0, 360)],
        "outputs": [int(0, 720)],
        "nodes": [["Input", 0], ["Input", 1], ["Add", 0, 1]],
        "roots": [2],
    });
    directory.write("family/sum/original.zcve", &encode(&program));
    let manifest = directory.json(
        "family/manifest.json",
        &json!({"schema": "zeno-fcis/loop-family-manifest/1", "family": "sum",
                "vocabulary": {"inputs": ["x", "y"]},
                "instances": [{"name": "sum", "parameters": {"N": 360}, "program": "sum/original.zcve"}]}),
    );
    let pad = format!("x + y{}", " + 0".repeat(40));
    let sketch = format!(
        "x == 0 ? x + y + ?c : (x == 1 ? x + y + ?c : (((?c < x) == (?c < y)) ? {pad} : {pad}))"
    );
    let document = directory.json(
        "sketch.json",
        &json!({"schema": "zeno-fcis/loop-expressions/1", "outputs": [sketch]}),
    );
    let (_, report) = run(&[
        "loop",
        "family",
        "--instances",
        &manifest.to_string_lossy(),
        "--program",
        &document.to_string_lossy(),
    ]);
    assert_ne!(report["status"], "unrealizable", "{report}");
    for row in report["detail"]["rows"].as_array().unwrap_or(&Vec::new()) {
        assert_ne!(row["verdict"], "unrealizable", "{row}");
    }
}

fn fixture_document(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-loop-doors/tests/data")
        .join(name);
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|error| panic!("fixture: {error:?}")))
        .unwrap_or_else(|error| panic!("fixture JSON: {error:?}"))
}

#[test]
fn counter_repairs_keep_the_small_edit_and_two_greedy_steps() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "bounded-counter-c3");
    let mut wrong = fixture_document("bc-m1.json");
    wrong["outputs"][0] = json!(
        "!authorized ? 0 : action == Inc && value == 2 ? 1 : action == Dec && value == 0 ? 2 : action == Inc ? 3 : 4"
    );
    wrong["outputs"][1] = json!("value + 1");
    let path = directory.json("one-fault.json", &wrong);
    let (_, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    let repair = &report["detail"]["door"]["explanation"]["repair"];
    assert!(
        repair["expressions"][0]
            .as_str()
            .is_some_and(|text| text.contains("value == 3")),
        "{report}"
    );
    confirm(
        &directory,
        &original,
        &repair["program"]["program"],
        &repair["check"],
        "one-fault",
    );
    let path = directory.json("two-branch-faults.json", &fixture_document("bc-m1.json"));
    let (_, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    let repair = &report["detail"]["door"]["explanation"]["repair"];
    assert_eq!(
        repair["steps"]
            .as_array()
            .unwrap_or_else(|| panic!("steps"))
            .len(),
        2,
        "{report}"
    );
    let replacements: Vec<_> = repair["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("steps"))
        .iter()
        .map(|step| step["replacement"].clone())
        .collect();
    assert!(
        replacements.contains(&json!("3")) && replacements.contains(&json!("0")),
        "{report}"
    );
    confirm(
        &directory,
        &original,
        &repair["program"]["program"],
        &repair["check"],
        "two-faults",
    );
}

#[test]
fn shifted_rate_limiter_holes_complete_with_five_thousand() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "rate-limiter-n1-w2");
    let path = directory.json("shifted-holes.json", &fixture_document("rl-a2.json"));
    let (exit, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let door = &report["detail"]["door"];
    assert_eq!(door["status"], "completed");
    assert_eq!(door["holes"]["0"]["value"], 5000);
    assert_eq!(door["holes"]["1"]["value"], 5000);
    confirm(
        &directory,
        &original,
        &door["completion"]["program"],
        &door["check"],
        "shifted-holes",
    );
}

#[test]
fn checked_duplicate_sketches_and_expressions_complete_successfully() {
    let directory = Directory::new();
    let (session, original) = open(&directory, "idempotency-slot-c2");
    let path = directory.json("choice.json", &fixture_document("is-c2-s1.json"));
    let (exit, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let door = &report["detail"]["door"];
    assert_eq!(door["status"], "completed");
    assert_eq!(door["selection"], "duplicate");
    confirm(
        &directory,
        &original,
        &door["completion"]["program"],
        &door["check"],
        "duplicate-sketch",
    );
    let path = directory.json(
        "same.json",
        &json!({"schema":"zeno-fcis/loop-expressions/1", "vocabulary":{"inputs":["seen","key","value","command_key","command_value","authorized"],"cases":3}, "outputs":["keep"]}),
    );
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--expressions",
        &path.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["door"]["status"], "completed");
    assert_eq!(report["detail"]["door"]["selection"], "duplicate");
    assert_eq!(report["detail"]["accounting"]["attempts"], 2);
}

#[test]
fn boolean_vote_sums_and_explicit_selects_have_the_same_fault_region() {
    let mut regions = Vec::new();
    for name in ["aq-k1-bug.json", "aq-k1-bug2.json"] {
        let directory = Directory::new();
        let (session, _) = open(&directory, "approval-queue-k1");
        let path = directory.json("wrong.json", &fixture_document(name));
        let (exit, report) = run(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--expressions",
            &path.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(1), "{report}");
        assert_eq!(report["detail"]["door"]["lowering"], "lowered");
        regions.push(report["detail"]["door"]["explanation"]["region"].clone());
    }
    assert_eq!(regions[0], regions[1]);
    assert!(regions[0]["tuples"].as_u64().is_some_and(|n| n > 0));
}

#[test]
fn an_unrealizable_shared_constant_explains_its_conflicting_inputs() {
    let directory = Directory::new();
    let (session, _) = open(&directory, "retry-budget-c3");
    let path = directory.json("impossible.json", &fixture_document("rb-u1.json"));
    let (_, report) = run(&[
        "loop",
        "sketch",
        "--session",
        &session,
        "--sketch",
        &path.to_string_lossy(),
    ]);
    let door = &report["detail"]["door"];
    assert_eq!(door["status"], "unrealizable", "{report}");
    assert_eq!(door["evidence"]["kind"], "complete-space-exhausted");
    assert_eq!(
        door["evidence"]["scope"],
        "one-shared-assignment-over-the-declared-domain"
    );
    let pair = door["evidence"]["conflicting_inputs"]
        .as_array()
        .unwrap_or_else(|| panic!("conflict inputs"));
    assert_eq!(pair.len(), 2, "{report}");
    assert_ne!(pair[0]["required_output"], pair[1]["required_output"]);
    assert!(door["searched"][0]["witness"].is_null());
}
