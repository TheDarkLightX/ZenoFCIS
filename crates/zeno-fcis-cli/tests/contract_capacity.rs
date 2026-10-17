//! The larger decision-program limits of 4,096 nodes, 64 outputs and 64
//! inputs each admit their bound and refuse one more, and the program
//! commands refuse the larger profile. `tools/check_g3_app.py` builds and
//! runs an application from a contract past the original limits.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-capacity-{label}-{}-{}",
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

fn zeno(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON report: {error}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn text(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path {path:?}"))
}

/// Sums distinct constants as a balanced tree: `terms` constants and
/// `terms - 1` additions, nested only logarithmically deep.
fn balanced_sum(low: usize, high: usize) -> String {
    if high - low == 1 {
        return (100_000 + low).to_string();
    }
    let middle = (low + high) / 2;
    format!(
        "({} + {})",
        balanced_sum(low, middle),
        balanced_sum(middle, high)
    )
}

/// A probe contract whose size the caller chooses: `fields` counters read
/// with the command's `amount` and the context's `caller`; one accept case
/// adds `amount` to the first `adds` counters and the last subtracts it from
/// the first `subs`. `terms` distinct constants, and `amount` once more when
/// `extra`, extend the first counter's sum, two nodes per constant.
struct Probe {
    fields: usize,
    adds: usize,
    subs: usize,
    terms: usize,
    extra: bool,
}

impl Probe {
    fn write(&self, dir: &Path) {
        fs::create_dir_all(dir.join("v2")).unwrap_or_else(|error| panic!("{error}"));
        let mut project = vec![
            "zeno 1;".to_owned(),
            "project 1 capacity_probe;".to_owned(),
            "namespace 2 probe;".to_owned(),
            "type 100 state Counters;".to_owned(),
            "type 101 command Bump;".to_owned(),
            "type 102 context Caller;".to_owned(),
            "type 103 int Count in 0..=1000;".to_owned(),
            "type 104 int Amount in 0..=10;".to_owned(),
        ];
        project.extend((0..self.fields).map(|i| format!("field {} 100 c{i} 103;", 1000 + i)));
        project.extend(
            [
                "field 2000 101 amount 104;",
                "field 2001 102 caller 104;",
                "reason 300 zero_amount precedence 0;",
                "component 400 probe {",
                "  owns 100;",
                "  reads pre.100;",
                "  writes post.100;",
                "  contexts context.102;",
                "  budget steps 64;",
                "}",
                "merge [400];",
                "law 500 counters_are_counts on commit, genesis = post.100.1000 >= 0;",
            ]
            .map(str::to_owned),
        );
        fs::write(dir.join("project.zeno"), project.join("\n") + "\n")
            .unwrap_or_else(|error| panic!("{error}"));
        let mut adds = serde_json::Map::new();
        for i in 0..self.adds {
            let field = 1000 + i;
            let mut sum = format!("pre.100.{field} + command.101.2000");
            if i == 0 && self.extra {
                sum.push_str(" + command.101.2000");
            }
            if i == 0 && self.terms > 0 {
                sum = format!("{sum} + {}", balanced_sum(0, self.terms));
            }
            adds.insert(field.to_string(), json!(sum));
        }
        let subs: serde_json::Map<String, Value> = (0..self.subs)
            .map(|i| {
                let field = 1000 + i;
                (
                    field.to_string(),
                    json!(format!("pre.100.{field} - command.101.2000")),
                )
            })
            .collect();
        let genesis: serde_json::Map<String, Value> = (0..self.fields)
            .map(|i| ((1000 + i).to_string(), json!(0)))
            .collect();
        let policy = json!({
            "schema": "zeno-fcis/template-declarative-policy/2",
            "template": "capacity-probe",
            "roots": {"state": 100, "command": 101, "context": 102},
            "leaf_bindings": {},
            "variables": {},
            "cases": [
                {"when": "command.101.2000 == 0", "class": "Reject", "reason": 300,
                 "post": {}, "outbox": []},
                {"when": "context.102.2001 == 1", "class": "Accept", "reason": null,
                 "post": adds, "outbox": []},
                {"when": "true", "class": "Accept", "reason": null, "post": subs, "outbox": []}
            ],
            "genesis": genesis,
            "law_kinds": {"500": "StateInvariant"}
        });
        fs::write(dir.join("v2/policy.json"), policy.to_string())
            .unwrap_or_else(|error| panic!("{error}"));
    }

    /// Generates the probe's contract in a new directory.
    fn generate(&self, root: &TempRoot, label: &str) -> (Option<i32>, Value, PathBuf) {
        let dir = root.path().join(label);
        self.write(&dir);
        let output = zeno(&["generate", "contract", text(&dir), "--format", "json"]);
        (output.status.code(), json(&output), dir)
    }
}

const GENERATED: [&str; 3] = ["v2/schema.zcve", "src/v2_contract.rs", "v2/policy.zcve"];

fn admitted(outcome: &(Option<i32>, Value, PathBuf)) -> (u64, u64) {
    let (code, report, dir) = outcome;
    assert_eq!(*code, Some(0), "{report}");
    assert_eq!(report["status"], "generated", "{report}");
    assert!(GENERATED.iter().all(|file| dir.join(file).is_file()));
    let summary = &report["summary"];
    (
        summary["program_nodes"].as_u64().unwrap_or_default(),
        summary["outputs"].as_u64().unwrap_or_default(),
    )
}

fn refused(outcome: &(Option<i32>, Value, PathBuf), message: &str) {
    let (code, report, dir) = outcome;
    assert_eq!(*code, Some(1), "{report}");
    assert_eq!(report["status"], "error", "{report}");
    assert_eq!(report["error"]["code"], "contract-invalid", "{report}");
    assert_eq!(report["error"]["message"], message, "{report}");
    assert!(GENERATED.iter().all(|file| !dir.join(file).exists()));
}

#[test]
fn the_node_limit_admits_4096_and_refuses_4097() {
    let root = TempRoot::new("nodes");
    let probe = |terms, extra| Probe {
        fields: 4,
        adds: 2,
        subs: 2,
        terms,
        extra,
    };
    // Each constant adds two nodes and the extra `amount` one.
    let (base, outputs) = admitted(&probe(1, false).generate(&root, "one"));
    assert_eq!(
        admitted(&probe(2, false).generate(&root, "two")).0,
        base + 2
    );
    assert_eq!(
        admitted(&probe(1, true).generate(&root, "extra")).0,
        base + 1
    );
    let sized = |nodes: u64| {
        let more = nodes - base;
        probe(
            usize::try_from(more / 2 + 1).unwrap_or(usize::MAX),
            more % 2 == 1,
        )
    };
    assert_eq!(
        admitted(&sized(4096).generate(&root, "at")),
        (4096, outputs)
    );
    refused(
        &sized(4097).generate(&root, "past"),
        &format!("{outputs} outputs and 4097 program nodes exceed 64 and 4096"),
    );
    // `new` refuses the same contract and writes no application.
    let app = root.path().join("app");
    let created = zeno(&[
        "new",
        text(&app),
        "--contract",
        text(&root.path().join("past")),
    ]);
    assert_eq!(created.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&created.stderr);
    assert!(
        stderr.contains("4097 program nodes exceed 64 and 4096"),
        "{stderr}"
    );
    if let Ok(mut entries) = fs::read_dir(&app) {
        assert!(entries.next().is_none());
    }
}

#[test]
fn the_output_limit_admits_64_and_refuses_65() {
    let root = TempRoot::new("outputs");
    // The case selection and one output per distinct computed value.
    let probe = |subs| Probe {
        fields: 40,
        adds: 40,
        subs,
        terms: 0,
        extra: false,
    };
    let (nodes, outputs) = admitted(&probe(23).generate(&root, "at"));
    assert_eq!(outputs, 64);
    refused(
        &probe(24).generate(&root, "past"),
        &format!(
            "65 outputs and {} program nodes exceed 64 and 4096",
            nodes + 1
        ),
    );
}

#[test]
fn the_input_limit_admits_64_and_refuses_65() {
    let root = TempRoot::new("inputs");
    // The state fields, the command's amount and the context's caller.
    let probe = |fields| Probe {
        fields,
        adds: 1,
        subs: 1,
        terms: 0,
        extra: false,
    };
    admitted(&probe(62).generate(&root, "at"));
    refused(&probe(63).generate(&root, "past"), "65 inputs exceed 64");
}

#[test]
fn the_program_commands_refuse_the_larger_profile() {
    let root = TempRoot::new("profile");
    // Past 256 nodes, so the decision program takes the larger profile.
    let probe = Probe {
        fields: 4,
        adds: 2,
        subs: 2,
        terms: 200,
        extra: false,
    };
    let (nodes, outputs) = admitted(&probe.generate(&root, "large"));
    assert!(nodes > 256, "{nodes}");
    let program = root.path().join("program.zcve");
    let exported = zeno(&[
        "contract",
        "export-program",
        text(&root.path().join("large")),
        "--out",
        text(&program),
        "--format",
        "json",
    ]);
    let report = json(&exported);
    assert_eq!(exported.status.code(), Some(0), "{report}");
    assert_eq!(report["status"], "exported", "{report}");
    assert_eq!(report["program"]["nodes"], nodes, "{report}");
    assert_eq!(report["program"]["outputs"], outputs, "{report}");
    let bytes = fs::read(&program).unwrap_or_else(|error| panic!("{error}"));
    let profile = b"zeno-fcis/finite-i128/2";
    assert!(bytes.windows(profile.len()).any(|window| window == profile));

    // Every command that reads a program file imports profile 1 only.
    let not_admitted = json!({"code": "program-encoding", "reason": "original-not-admitted"});
    let path = text(&program);
    let checked = zeno(&[
        "transform",
        "check",
        "--original",
        path,
        "--candidate",
        path,
    ]);
    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(json(&checked)["status"], "refused");
    assert_eq!(json(&checked)["detail"], not_admitted);
    let optimized = zeno(&["optimize", "--program", path]);
    assert_eq!(optimized.status.code(), Some(1));
    assert_eq!(json(&optimized)["status"], "refused");
    assert_eq!(json(&optimized)["detail"], not_admitted);
}
