//! Private pilot meta-test. This entire module is compiled only for tests.

use crate::internals::review::evaluate::{digest, hex};
use crate::refinement::{self, Sources, TestComparison};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path};

const LIMIT: usize = 1 << 24;
const NAMES: [&str; 8] = [
    "reject-all",
    "forgotten-content",
    "missing-operation",
    "extrema-only",
    "wrong-tie",
    "wrong-reason",
    "invalid-encoding",
    "source-substitution",
];
const DISABLED: [&str; 8] = [
    "valid-row-relation",
    "successor",
    "successor",
    "valid-row-relation",
    "valid-row-relation",
    "reason",
    "null-law",
    "source-pin",
];
fn sha(bytes: &[u8]) -> String {
    hex(&digest(bytes))
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    if !fs::metadata(path).map_err(|e| e.to_string())?.is_file() {
        return Err("not a regular evidence file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("evidence exceeds its bound".into());
    }
    Ok(bytes)
}
fn json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut b = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    b.push(b'\n');
    Ok(b)
}
fn value(path: &Path, limit: usize) -> Result<Value, String> {
    let bytes = read(path, limit)?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    // Canonical equality also refuses duplicate properties and trailing data.
    if json_bytes(&v)? != bytes {
        return Err("noncanonical or duplicate-key artifact".into());
    }
    Ok(v)
}
fn put(path: &Path, v: &Value) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("output parent")?).map_err(|e| e.to_string())?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(&json_bytes(v).map_err(std::io::Error::other)?))
        .map_err(|e| e.to_string())
}
fn evidence_ref(root: &Path, path: &Path) -> Result<Value, String> {
    Ok(
        json!({"path":path.strip_prefix(root).map_err(|e|e.to_string())?.to_str().ok_or("path UTF-8")?,"sha256":sha(&read(path,LIMIT)?)}),
    )
}
fn referenced(root: &Path, reference: &Value, limit: usize) -> Result<Vec<u8>, String> {
    let object = reference.as_object().ok_or("reference object")?;
    if object.len() != 2 || !object.contains_key("path") || !object.contains_key("sha256") {
        return Err("closed evidence reference".into());
    }
    let path = Path::new(reference["path"].as_str().ok_or("reference path")?);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("relative evidence path".into());
    }
    let bytes = read(&root.join(path), limit)?;
    if reference["sha256"].as_str() != Some(sha(&bytes).as_str()) {
        return Err("evidence reference hash".into());
    }
    Ok(bytes)
}
#[derive(Default)]
struct Work {
    calls: u64,
    rows: u64,
    usage: [u128; 8],
}
impl Work {
    fn add(&mut self, report: &Value) -> Result<(), String> {
        self.calls += report["authority_evaluations"]
            .as_u64()
            .ok_or("native calls")?;
        self.rows += report["checked_rows"].as_u64().ok_or("native rows")?;
        for (j, total) in self.usage.iter_mut().enumerate() {
            *total += report["authority_usage"][j]
                .as_str()
                .ok_or("native usage")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn json(&self) -> Value {
        json!({"authority_evaluations":self.calls,"checked_rows":self.rows,"authority_usage":self.usage.map(|v|v.to_string())})
    }
}
fn execute(
    sources: Sources<'_>,
    max: u64,
    disabled: Option<TestComparison>,
    work: &mut Work,
) -> Result<Value, String> {
    let result = match disabled {
        Some(disabled) => refinement::check_with_disabled_for_test(sources, max, disabled),
        None => refinement::check(sources, max),
    };
    match result {
        Ok(report) => {
            let report = report.json();
            work.add(&report)?;
            Ok(
                json!({"exit":match report["verdict"].as_str(){Some("refines")=>0,Some("counterexample")=>1,_=>2},"report":report,"error":Value::Null}),
            )
        }
        Err(error) => Ok(json!({"exit":65,"report":Value::Null,"error":error.to_string()})),
    }
}
fn subject(project: &[u8], policy: &[u8], abstraction: &[u8]) -> Result<String, String> {
    Ok(sha(&json_bytes(&json!([
        ["abstraction.json", sha(abstraction)],
        ["project.zeno", sha(project)],
        ["v2/policy.json", sha(policy)]
    ]))?))
}
fn first(layout: &str, index: usize) -> Option<(u64, u64)> {
    let table = if layout == "T" {
        [
            (0, 0),
            (109, 10),
            (56, 10),
            (56, 2),
            (108, 15),
            (108, 4),
            (7, 0),
        ]
    } else {
        [
            (0, 0),
            (295, 10),
            (126, 14),
            (126, 0),
            (258, 15),
            (258, 0),
            (25, 0),
        ]
    };
    table.get(index).copied()
}
fn detected(result: &Value, layout: &str, index: usize) -> bool {
    if let Some((s, q)) = first(layout, index) {
        result["exit"] == 1
            && result["report"]["witness"]["state"] == s
            && result["report"]["witness"]["input"] == q
    } else {
        result["exit"] == 65
            && result["report"].is_null()
            && result["error"]
                .as_str()
                .is_some_and(|e| e.contains("source SHA-256 differs from its pin"))
    }
}
fn selector(index: usize) -> TestComparison {
    match index {
        0 | 3 | 4 => TestComparison::ValidRelation,
        1 | 2 => TestComparison::Successor,
        5 => TestComparison::Reason,
        6 => TestComparison::NullLaw,
        _ => TestComparison::SourcePin,
    }
}
fn positive(report: &Value, layout: &str) -> bool {
    report["verdict"] == "refines"
        && report["checked_rows"] == if layout == "T" { 2352 } else { 6000 }
        && report["authority_evaluations"] == if layout == "T" { 3074 } else { 6722 }
}

fn run(root: &Path, work: &mut Work) -> Result<(), String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if root.starts_with(&repository) {
        return Err("meta evidence must be outside source".into());
    }
    let root = root.as_path();
    let bundle = root.join("bundle");
    let controls = root.join("controls");
    if !controls
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root)
    {
        return Err("controls path escapes evidence".into());
    }
    let source = read(&bundle.join("source.sha256"), LIMIT)?;
    let source_hash = sha(&source);
    for line in std::str::from_utf8(&source)
        .map_err(|e| e.to_string())?
        .lines()
    {
        let (hash, path) = line.split_once("  ").ok_or("source manifest row")?;
        if sha(&read(&repository.join(path), LIMIT)?) != hash {
            return Err(format!("changed source: {path}"));
        }
    }
    let binding = value(&bundle.join("binding.json"), 1 << 16)?;
    let f51_bytes = referenced(root, &binding["f51_evidence"], 1 << 16)?;
    let f51: Value = serde_json::from_slice(&f51_bytes).map_err(|e| e.to_string())?;
    if f51["status"] != "qualified" || f51["source_manifest_sha256"] != source_hash {
        return Err("current F51 qualification required".into());
    }
    let checks = f51["checks"].as_array().ok_or("F51 checks")?;
    let f02 = checks
        .iter()
        .rev()
        .find(|c| c["gate"] == "F02")
        .ok_or("F02 evidence")?;
    let field_controls = f02["log"].clone();
    let field_log = referenced(root, &field_controls, LIMIT)?;
    if !String::from_utf8_lossy(&field_log).contains("14 passed; 0 failed; 0 ignored") {
        return Err("F02 field controls incomplete".into());
    }
    let controls_report = value(&controls.join("controls.json"), LIMIT)?;
    if controls_report["status"] != "controls-caught"
        || controls_report["detail"]["control_count"] != 16
    {
        return Err("sixteen native controls required".into());
    }
    let records = controls_report["detail"]["controls"]
        .as_array()
        .ok_or("control records")?;
    if records.len() != 16 {
        return Err("control record count".into());
    }
    if controls.join("meta.json").exists() {
        return Err("refuse meta evidence overwrite".into());
    }
    fs::create_dir(controls.join("meta-results")).map_err(|e| e.to_string())?;
    let abstract_project = String::from_utf8(read(&bundle.join("abstract/project.zeno"), 1 << 20)?)
        .map_err(|e| e.to_string())?;
    let abstract_policy =
        String::from_utf8(read(&bundle.join("abstract/v2/policy.json"), 1 << 20)?)
            .map_err(|e| e.to_string())?;
    let mut positives = BTreeMap::new();
    let mut meta = Vec::new();
    for (layout_index, layout) in ["T", "I"].iter().enumerate() {
        let run = value(&root.join(layout).join("run.json"), LIMIT)?;
        let final_path = run["detail"]["final_attempt"]
            .as_str()
            .ok_or("parent final attempt")?;
        let dir = root.join(layout).join(final_path);
        let parent = [
            read(&dir.join("candidate/project.zeno"), 1 << 20)?,
            read(&dir.join("candidate/v2/policy.json"), 1 << 20)?,
            read(&dir.join("abstraction.json"), 1 << 20)?,
        ];
        let parent_sha = subject(&parent[0], &parent[1], &parent[2])?;
        let text = parent
            .iter()
            .map(|v| std::str::from_utf8(v).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let sources = Sources {
            abstract_project: &abstract_project,
            abstract_policy: &abstract_policy,
            concrete_project: text[0],
            concrete_policy: text[1],
            abstraction: text[2],
        };
        let max = if *layout == "T" { 2352 } else { 6000 };
        let checked = execute(sources, max, None, work)?;
        if checked["exit"] != 0 || !positive(&checked["report"], layout) {
            return Err("parent native positive failed".into());
        }
        let positive_path = controls.join(layout).join("positive.json");
        if value(&positive_path, LIMIT)? != checked["report"] {
            return Err("parent native result differs".into());
        }
        positives.insert(*layout, evidence_ref(root, &positive_path)?);
        for (index, name) in NAMES.iter().enumerate() {
            let record = &records[layout_index * 8 + index];
            if record["layout"] != *layout
                || record["control"] != *name
                || record["disabled_predicate"] != DISABLED[index]
                || record["parent_sha256"] != parent_sha
                || record["expected_first"] != json!(first(layout, index))
                || record["expected_exit"] != if index == 7 { 65 } else { 1 }
            {
                return Err("frozen control table differs".into());
            }
            let bytes = [
                referenced(root, &record["project"], 1 << 20)?,
                referenced(root, &record["policy"], 1 << 20)?,
                referenced(root, &record["abstraction"], 1 << 20)?,
            ];
            let mutant_sha = subject(&bytes[0], &bytes[1], &bytes[2])?;
            if record["mutant_sha256"] != mutant_sha {
                return Err("mutant subject pin".into());
            }
            let text = bytes
                .iter()
                .map(|v| std::str::from_utf8(v).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let sources = Sources {
                abstract_project: &abstract_project,
                abstract_policy: &abstract_policy,
                concrete_project: text[0],
                concrete_policy: text[1],
                abstraction: text[2],
            };
            let normal = execute(sources, max, None, work)?;
            let stored = referenced(root, &record["normal_result"], LIMIT)?;
            if json_bytes(&normal)? != stored || !detected(&normal, layout, index) {
                return Err(format!("normal detection failed: {layout}/{name}"));
            }
            let disabled = execute(sources, max, Some(selector(index)), work)?;
            if disabled["exit"] != 0 || !positive(&disabled["report"], layout) {
                return Err(format!(
                    "isolated predicate did not defeat detection: {layout}/{name}"
                ));
            }
            let path = controls.join(format!("meta-results/{layout}/{index:02}-{name}.json"));
            put(&path, &disabled)?;
            let assertion_failed = std::panic::catch_unwind(|| {
                assert!(
                    detected(&disabled, layout, index),
                    "isolated control escaped: {layout}/{name}"
                )
            })
            .is_err();
            if !assertion_failed {
                return Err("detection assertion unexpectedly passed".into());
            }
            let item = json!({"layout":layout,"control":name,"parent_sha256":parent_sha,"mutant_sha256":mutant_sha,"disabled_predicate":DISABLED[index],
                "normal_result":record["normal_result"],"disabled_result":evidence_ref(root,&path)?,"detection_assertion_failed":assertion_failed});
            println!(
                "{}",
                json!({"schema":"zeno-fcis/priority-controls-meta-row/1","row":item})
            );
            meta.push(item);
        }
    }
    put(
        &controls.join("meta.json"),
        &json!({"schema":"zeno-fcis/priority-controls-meta/1","revision":"construction-r2","source_manifest_sha256":source_hash,
        "f51_field_controls":field_controls,"positives":positives,"controls":meta}),
    )?;
    Ok(())
}

#[test]
fn all_eight_controls_and_predicate_stubs() {
    let mut work = Work::default();
    let outcome = std::env::var_os("ZENO_FCIS_CONSTRUCTION_EVIDENCE")
        .ok_or_else(|| "ZENO_FCIS_CONSTRUCTION_EVIDENCE is required; no skip".to_string())
        .and_then(|root| run(Path::new(&root), &mut work));
    println!(
        "{}",
        json!({"schema":"zeno-fcis/priority-controls-meta-work/1","status":if outcome.is_ok(){"passed"}else{"failed"},"counts":work.json(),"failure":outcome.as_ref().err()})
    );
    if let Err(error) = outcome {
        panic!("{error}");
    }
}
