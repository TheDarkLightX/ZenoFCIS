//! LLM-generated tests of the learn command through the real, contained CLI.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_synthesis::finite_runtime::import_program;

const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-loop-learn-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("directory: {e}"));
        Self(path)
    }
    fn document(&self, name: &str, data: &Value) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, data.to_string()).unwrap_or_else(|e| panic!("document: {e}"));
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> Output {
    Command::new(CLI)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("CLI: {e}"))
}
fn text(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("UTF-8 path"))
}
fn report(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "report: {e}; stdout={}; stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}
fn declaration() -> Value {
    json!({"schema":"zeno-fcis/loop-learn/1", "inputs":[
        {"name":"x","domain":{"kind":"Int","min":0,"max":2}},
        {"name":"y","domain":{"kind":"Int","min":0,"max":2}}],
        "outputs":[{"kind":"Int","min":-100,"max":100}],
        "vocabulary":{"constants":[1],"arithmetic":["+","-"]},"max_size":3,"examples":[]})
}

// BEGIN L1/L2 regression controls.
fn reference(d: &Directory, program: &Value) -> PathBuf {
    let source = d.document("reference.json", program);
    let target = d.0.join("reference.zcve");
    assert_eq!(
        run(&[
            "loop",
            "encode",
            "--program",
            text(&source),
            "--out",
            text(&target)
        ])
        .status
        .code(),
        Some(0)
    );
    target
}

#[test]
fn learned_candidate_automatically_captures_checks_and_exports_replayable_receipts() {
    let d = Directory::new();
    let mut spec = declaration();
    spec["examples"] = json!(
        (0..=2)
            .flat_map(|x| (0..=2).map(move |y| json!({"input":[x,y],"output":[x+y]})))
            .collect::<Vec<_>>()
    );
    let path = d.document("learn.json", &spec);
    let original = reference(
        &d,
        &json!({"inputs":[{"kind":"Int","min":"0","max":"2"},{"kind":"Int","min":"0","max":"2"}],
        "outputs":[{"kind":"Int","min":"-100","max":"100"}],
        "nodes":[["Input",0],["Input",1],["Add",0,1]],"roots":[2]}),
    );
    let export = d.0.join("verified");
    let done = run(&[
        "loop",
        "learn",
        "--spec",
        text(&path),
        "--original",
        text(&original),
        "--out",
        text(&export),
    ]);
    let result = report(&done);
    assert_eq!(done.status.code(), Some(0), "{result}");
    assert_eq!(result["status"], "qualified");
    assert_eq!(
        result["detail"]["candidate"]["status"],
        "examples-reproduced"
    );
    assert_eq!(result["detail"]["learning_status"], "determined");
    assert_eq!(result["detail"]["capture"]["rows"], 9);
    assert_eq!(result["detail"]["capture"]["check"]["replay"], "matched");
    assert_eq!(result["detail"]["best_check"]["replay"], "matched");
    assert_eq!(result["detail"]["verification_work"]["check_requests"], 2);
    for source in ["original.zcve", "capture-reference.zcve"] {
        let receipt = export.join(format!("{source}.receipt.json"));
        let bound = export.join(source);
        let candidate = export.join("candidate.zcve");
        assert_eq!(
            run(&[
                "transform",
                "check",
                "--original",
                text(&bound),
                "--candidate",
                text(&candidate),
                "--receipt",
                text(&receipt)
            ])
            .status
            .code(),
            Some(0)
        );
        assert_eq!(
            run(&[
                "transform",
                "replay",
                "--original",
                text(&bound),
                "--candidate",
                text(&candidate),
                "--receipt",
                text(&receipt)
            ])
            .status
            .code(),
            Some(0)
        );
    }
}

#[test]
fn fitting_examples_is_refuted_by_automatic_capture_on_an_unseen_input() {
    let d = Directory::new();
    let spec = json!({"schema":"zeno-fcis/loop-learn/1","inputs":[{"name":"x","domain":{"kind":"Int","min":0,"max":2}}],
        "outputs":[{"kind":"Int","min":0,"max":2}],"max_size":1,
        "vocabulary":{"constants":[],"arithmetic":[]},"examples":[{"input":[0],"output":[0]}]});
    let path = d.document("learn.json", &spec);
    let original = reference(
        &d,
        &json!({"inputs":[{"kind":"Int","min":"0","max":"2"}],"outputs":[{"kind":"Int","min":"0","max":"2"}],
        "nodes":[["Input",0],["Int","2"],["Eq",0,1],["Int","1"],["Select",2,3,0]],"roots":[4]}),
    );
    let export = d.0.join("refuted");
    let done = run(&[
        "loop",
        "learn",
        "--spec",
        text(&path),
        "--original",
        text(&original),
        "--out",
        text(&export),
    ]);
    let result = report(&done);
    assert_eq!(done.status.code(), Some(1), "{result}");
    assert_eq!(result["status"], "refuted");
    assert_eq!(
        result["detail"]["candidate"]["examples_check"]["verdict"],
        "reproduced"
    );
    assert_eq!(
        result["detail"]["capture"]["check"]["verdict"],
        "counterexample"
    );
    assert_eq!(result["detail"]["best_check"]["ordinal"], 2);
    assert!(!export.join("candidate.zcve").exists());
}

#[test]
fn four_node_macro_construction_names_the_three_node_search_bound() {
    let d = Directory::new();
    let mut spec = json!({"schema":"zeno-fcis/loop-learn/1","inputs":[
        {"name":"x","domain":{"kind":"Int","min":-1,"max":1}},
        {"name":"y","domain":{"kind":"Int","min":-1,"max":1}}],
        "outputs":[{"kind":"Int","min":0,"max":2}],"max_size":3,
        "vocabulary":{"constants":[],"arithmetic":["+"],"macros":["abs"]},
        "examples":(-1i128..=1).flat_map(|x|(-1i128..=1).map(move|y|json!({"input":[x,y],"output":[(x+y).abs()]}))).collect::<Vec<_>>()});
    let small = d.document("small.json", &spec);
    let done = run(&["loop", "learn", "--spec", text(&small)]);
    let result = report(&done);
    assert_eq!(result["status"], "outside-vocabulary", "{result}");
    assert_eq!(
        result["detail"]["search_limit"]["bound"],
        "max_size-and-declared-vocabulary"
    );
    assert_eq!(
        result["detail"]["search_limit"]["search_bounds"]["max_size"],
        3
    );
    assert_eq!(
        result["detail"]["search_limit"]["action"],
        "request-better-primitive"
    );
    // A separate explicit increase demonstrates the missing construction. The
    // failed call itself cannot change the declared search or checker caps.
    spec["max_size"] = json!(4);
    let larger = d.document("larger.json", &spec);
    let result = report(&run(&["loop", "learn", "--spec", text(&larger)]));
    assert_eq!(result["status"], "determined", "{result}");
    assert_eq!(result["detail"]["representatives"][0]["size"], 4);
}
// END L1/L2 regression controls.

#[test]
fn learn_determined_lowers_and_reproduces_every_provided_example_with_existing_evaluator() {
    let d = Directory::new();
    let mut spec = declaration();
    spec["examples"] = json!(
        (0..=2)
            .flat_map(|x| (0..=2).map(move |y| json!({"input":[x,y],"output":[x+y]})))
            .collect::<Vec<_>>()
    );
    let path = d.document("declaration.json", &spec);
    let out = run(&["loop", "learn", "--spec", text(&path)]);
    assert_eq!(out.status.code(), Some(0));
    let r = report(&out);
    assert_eq!(r["status"], "determined");
    assert_eq!(r["detail"]["enumeration"], "proposal");
    assert_eq!(r["detail"]["counts"], json!([1]));
    assert_eq!(
        r["detail"]["candidate"]["examples_check"],
        json!({"decided_by":"library-evaluator", "checked":9,
        "vacuous":false, "verdict":"reproduced"})
    );
    let program = d.document("program.json", &r["detail"]["candidate"]["program"]);
    let canonical = d.0.join("program.zcve");
    let encoded = run(&[
        "loop",
        "encode",
        "--program",
        text(&program),
        "--out",
        text(&canonical),
    ]);
    assert_eq!(encoded.status.code(), Some(0));
    let bytes = fs::read(&canonical).unwrap_or_else(|e| panic!("canonical: {e}"));
    let p = import_program(&bytes).unwrap_or_else(|e| panic!("import: {e:?}"));
    for x in 0..=2 {
        for y in 0..=2 {
            assert_eq!(p.evaluate(&[x, y]), Ok(vec![x + y]));
        }
    }
    assert_eq!(r["detail"]["attempts_spent"], 0);
}

#[test]
fn learn_question_partition_is_a_deterministic_proposal() {
    let d = Directory::new();
    let path = d.document("declaration.json", &declaration());
    let out = run(&["loop", "learn", "--spec", text(&path)]);
    assert_eq!(out.status.code(), Some(3));
    let r = report(&out);
    assert_eq!(r["status"], "question");
    assert_eq!(r["detail"]["counts"], json!([16]));
    assert_eq!(r["detail"]["by_size"], json!([[3, 0], [0, 0], [13, 0]]));
    assert_eq!(r["detail"]["question"]["ordinal"], 8);
    assert_eq!(r["detail"]["question"]["worst"], "3");
    assert_eq!(r["detail"]["question"]["squares"], "44");
    assert_eq!(r, report(&run(&["loop", "learn", "--spec", text(&path)])));
}

#[test]
fn learn_every_budget_stop_is_exit_two_without_counts_program_or_attempt() {
    let d = Directory::new();
    let path = d.document("declaration.json", &declaration());
    for flag in [
        "--max-work",
        "--max-cells",
        "--max-meanings",
        "--max-tuples",
    ] {
        let out = run(&["loop", "learn", "--spec", text(&path), flag, "0"]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
        let r = report(&out);
        assert_eq!(r["status"], "inconclusive");
        for field in ["counts", "question", "candidate", "core"] {
            assert!(r["detail"][field].is_null());
        }
        assert_eq!(r["detail"]["attempts_spent"], 0);
    }
    let mut outside = declaration();
    outside["max_size"] = json!(1);
    outside["examples"] = json!([{"input":[0,0],"output":[7]}]);
    let path = d.document("outside.json", &outside);
    let out = run(&[
        "loop",
        "learn",
        "--spec",
        text(&path),
        "--max-conflict-subsets",
        "0",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let r = report(&out);
    assert_eq!(r["status"], "inconclusive");
    assert!(r["detail"]["counts"].is_null());
    assert!(r["detail"]["core"].is_null());
}

#[test]
fn learn_outside_vocabulary_has_a_minimum_conflict_and_no_accepted_program() {
    let d = Directory::new();
    let mut spec = declaration();
    spec["max_size"] = json!(1);
    spec["examples"] = json!([{"input":[0,0],"output":[0]}, {"input":[1,2],"output":[2]}, {"input":[2,1],"output":[2]}]);
    let path = d.document("outside.json", &spec);
    let out = run(&["loop", "learn", "--spec", text(&path)]);
    assert_eq!(out.status.code(), Some(1));
    let r = report(&out);
    assert_eq!(r["status"], "outside-vocabulary");
    assert_eq!(r["detail"]["core"], json!([1, 2]));
    assert_eq!(r["detail"]["enumeration"], "proposal");
    assert!(r["detail"]["candidate"].is_null());
}

#[test]
fn learn_refuses_unknown_primitives_and_has_described_effects() {
    let d = Directory::new();
    let mut spec = declaration();
    spec["vocabulary"]["arithmetic"] = json!(["opaque_product"]);
    let path = d.document("research.json", &spec);
    let out = run(&["loop", "learn", "--spec", text(&path)]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(report(&out)["status"], "refused");
    let out = run(&["describe", "loop", "learn"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("learn-declaration-and-examples"));
}

#[test]
fn learn_malformed_specimens_are_refused_before_enumeration() {
    let d = Directory::new();
    let mut specimens = Vec::new();
    let mut duplicate = declaration();
    duplicate["inputs"][1]["name"] = json!("x");
    specimens.push((duplicate, Some("vocabulary-duplicate-name:x")));
    let mut custom = declaration();
    custom["vocabulary"]["macros"] = json!([{"name":"custom", "body":"x*x"}]);
    specimens.push((custom, None));
    let mut misses = declaration();
    misses["misses"] = json!(1);
    specimens.push((misses, Some("learn-confirmed-examples-require-zero-misses")));
    for coordinate in [json!(true), json!(1.5), json!(1.0), json!("1")] {
        for field in ["input", "output"] {
            let mut spec = declaration();
            spec["examples"] = json!([{"input":[1,1], "output":[2]}]);
            spec["examples"][0][field][0] = coordinate.clone();
            specimens.push((spec, None));
        }
    }
    for (ordinal, (spec, expected)) in specimens.iter().enumerate() {
        let path = d.document(&format!("invalid-{ordinal}.json"), spec);
        let out = run(&["loop", "learn", "--spec", text(&path), "--max-work", "0"]);
        assert_eq!(out.status.code(), Some(1), "{spec}");
        let r = report(&out);
        assert_eq!(r["status"], "refused", "{spec}");
        if let Some(reason) = expected {
            assert!(
                r["detail"]["reason"]
                    .as_str()
                    .is_some_and(|s| s.contains(reason)),
                "{r}"
            );
        }
        assert_eq!(r["detail"]["usage"]["work"], 0);
        for field in ["counts", "question", "candidate", "core"] {
            assert!(r["detail"][field].is_null(), "{r}");
        }
    }
}

#[test]
fn learn_fused_arithmetic_checks_examples_and_qualifies_against_native_reference() {
    for (name, ceil) in [("MulDivFloor", false), ("MulDivCeil", true)] {
        let d = Directory::new();
        let spec = json!({"schema":"zeno-fcis/loop-learn/1",
            "inputs":[{"name":"x","domain":{"kind":"Int","min":0,"max":4}}],
            "outputs":[{"kind":"Int","min":0,"max":10}],
            "vocabulary":{"constants":[3,2],"arithmetic":[if ceil {"mul_div_ceil"} else {"mul_div_floor"}]},
            "max_size":4,"examples":(0..=4).map(|x| json!({"input":[x],
                "output":[(3*x + i64::from(ceil)) / 2]})).collect::<Vec<_>>()});
        let path = d.document("arithmetic.json", &spec);
        let original = reference(
            &d,
            &json!({"inputs":[{"kind":"Int","min":"0","max":"4"}],
            "outputs":[{"kind":"Int","min":"0","max":"10"}],
            "nodes":[["Input",0],["Int","3"],["Int","2"],[name,0,1,2]],"roots":[3]}),
        );
        let done = run(&[
            "loop",
            "learn",
            "--spec",
            text(&path),
            "--original",
            text(&original),
        ]);
        let r = report(&done);
        assert_eq!(done.status.code(), Some(0), "{r}");
        assert_eq!(r["status"], "qualified", "{r}");
        assert_eq!(r["detail"]["candidate"]["examples_check"]["checked"], 5);
        assert_eq!(
            r["detail"]["candidate"]["examples_check"]["verdict"],
            "reproduced"
        );
        assert_eq!(r["authority"], "none");
    }
}

#[test]
fn learn_product_comparison_stub_reference_cannot_qualify() {
    for stub in [false, true] {
        let d = Directory::new();
        let path = d.document("products.json", &json!({"schema":"zeno-fcis/loop-learn/1",
            "inputs":[{"name":"x","domain":{"kind":"Int","min":0,"max":4}}],
            "outputs":[{"kind":"Bool"}],"max_size":5,
            "vocabulary":{"constants":[1,2],"arithmetic":[],"compare":["products_ge"]},
            "examples":(0..=4).map(|x| json!({"input":[x],"output":[i64::from(x>=2)]})).collect::<Vec<_>>() }));
        let compare = if stub {
            json!(["Bool", true])
        } else {
            json!(["ProductsGe", 0, 1, 1, 2, 1, 1])
        };
        let original = reference(
            &d,
            &json!({"inputs":[{"kind":"Int","min":"0","max":"4"}],
            "outputs":[{"kind":"Bool"}],"nodes":[["Input",0],["Int","1"],["Int","2"],compare],"roots":[3]}),
        );
        let done = run(&[
            "loop",
            "learn",
            "--spec",
            text(&path),
            "--original",
            text(&original),
        ]);
        let r = report(&done);
        assert_eq!(
            r["status"],
            if stub { "refuted" } else { "qualified" },
            "{r}"
        );
        assert_eq!(done.status.code() == Some(0), !stub, "{r}");
    }
}
