//! Real CLI checks for requirements-only questions, proposal labels and final
//! replay. Unlike Python shell tests, these invoke the compiled checker.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zeno-loop-versions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("{e}"));
        Self(p)
    }
    fn file(&self, name: &str, text: &str) -> String {
        let p = self.0.join(name);
        fs::write(&p, text).unwrap_or_else(|e| panic!("{e}"));
        p.to_string_lossy().into_owned()
    }
    fn json(&self, name: &str, v: &Value) -> String {
        self.file(name, &v.to_string())
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> (Option<i32>, Value) {
    let o = Command::new(CLI)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    let v = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {o:?}"));
    (o.status.code(), v)
}
fn spec(sketch: Value, requirements: Value) -> Value {
    json!({"schema":"zeno-fcis/loop-version-space/1","inputs":[{"name":"x","domain":{"kind":"Bool"}}],
        "outputs":[{"kind":"Bool"}],"sketch":sketch,"requirements":requirements})
}
#[test]
fn requirements_only_counts_and_questions_are_proposals() {
    let d = Directory::new();
    let p = d.json("spec.json", &spec(json!(["x ? ?c : ?c"]), json!([])));
    let (code, r) = run(&["loop", "question", "--specification", &p]);
    assert_eq!(code, Some(2), "{r}");
    assert_eq!(r["status"], "question");
    assert_eq!(r["detail"]["count"], 4);
    assert_eq!(r["detail"]["claims"], "proposal");
    assert_eq!(
        r["detail"]["question"]["answers"],
        json!([{"output":[0],"remaining":2},{"output":[1],"remaining":2}])
    );
    assert_eq!(r["authority"], "none");
    assert_eq!(r["detail"]["attempts_spent"], 0);
}
#[test]
fn a_single_meaning_checks_every_requirement_and_replays_every_receipt() {
    let d = Directory::new();
    let p = d.json(
        "spec.json",
        &spec(
            json!(["?c"]),
            json!([{"id":"law","law":"out0 == false"},{"id":"example","input":[1],"output":[0]}]),
        ),
    );
    let (code, r) = run(&["loop", "question", "--specification", &p]);
    assert_eq!(code, Some(0), "{r}");
    assert_eq!(r["status"], "determined");
    let c = &r["detail"]["candidate"];
    assert_eq!(c["status"], "checked");
    assert_eq!(c["domain"], 2);
    assert_eq!(c["requirements"], json!(["law", "example"]));
    assert_eq!(c["law_usage"]["calls"], 4);
    let checks = c["checks"].as_array().unwrap_or_else(|| panic!("{r}"));
    assert_eq!(checks.len(), 3);
    for (k, check) in checks.iter().enumerate() {
        assert_eq!(check["replay"], "matched");
        fn bytes(v: &Value) -> Vec<u8> {
            v.as_str()
                .unwrap_or_else(|| panic!("hex"))
                .as_bytes()
                .chunks(2)
                .map(|b| {
                    u8::from_str_radix(std::str::from_utf8(b).unwrap_or_else(|e| panic!("{e}")), 16)
                        .unwrap_or_else(|e| panic!("{e}"))
                })
                .collect()
        }
        let original = d.0.join(format!("reference-{k}.zcve"));
        let predicate = d.0.join(format!("obligation-{k}.zcve"));
        fs::write(&original, bytes(&check["reference"])).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&predicate, bytes(&check["obligation"])).unwrap_or_else(|e| panic!("{e}"));
        let receipt = d.file(
            &format!("receipt-{k}.json"),
            &format!("{}\n", check["receipt"]),
        );
        let (exit, replayed) = run(&[
            "transform",
            "replay",
            "--receipt",
            &receipt,
            "--original",
            &original.to_string_lossy(),
            "--candidate",
            &predicate.to_string_lossy(),
        ]);
        assert_eq!(exit, Some(0), "{replayed}");
    }
}
#[test]
fn budget_stops_exit_two_without_count_question_candidate_or_attempt() {
    let d = Directory::new();
    let p = d.json("spec.json", &spec(json!(["?c"]), json!([])));
    for flag in [
        "--max-work",
        "--max-domain",
        "--max-meanings",
        "--max-cells",
    ] {
        let (code, r) = run(&["loop", "question", "--specification", &p, flag, "0"]);
        assert_eq!(code, Some(2), "{r}");
        assert_eq!(r["status"], "inconclusive");
        for key in ["count", "question", "candidate"] {
            assert!(r["detail"][key].is_null(), "{r}");
        }
        assert_eq!(r["detail"]["attempts_spent"], 0);
    }
}
#[test]
fn conflicts_are_minimum_relative_to_the_sketch_and_unknown_fields_refuse() {
    let d = Directory::new();
    let p = d.json(
        "spec.json",
        &spec(
            json!(["?c"]),
            json!([{"id":"b","input":[0],"output":[1]},{"id":"a","input":[1],"output":[0]}]),
        ),
    );
    let (code, r) = run(&["loop", "question", "--specification", &p]);
    assert_eq!(code, Some(1), "{r}");
    assert_eq!(r["status"], "no-completion");
    assert_eq!(r["detail"]["conflict"]["requirements"], json!(["a", "b"]));
    let (code, r) = run(&[
        "loop",
        "question",
        "--specification",
        &p,
        "--max-conflict-subsets",
        "0",
    ]);
    assert_eq!(code, Some(2), "{r}");
    assert!(r["detail"]["count"].is_null());
    let duplicate = d.file("duplicate.json", "{\"schema\":\"a\",\"schema\":\"b\"}");
    let (code, r) = run(&["loop", "question", "--specification", &duplicate]);
    assert_ne!(code, Some(0));
    assert_eq!(r["status"], "refused");
}
#[test]
fn exact_final_phase_budget_boundaries_are_reported() {
    let d = Directory::new();
    let p = d.json(
        "spec.json",
        &spec(
            json!(["false"]),
            json!([{"id":"zero","law":"out0 == false"}]),
        ),
    );
    let (code, r) = run(&["loop", "question", "--specification", &p]);
    assert_eq!(code, Some(0), "{r}");
    let used = r["detail"]["budget"]["work"]
        .as_u64()
        .unwrap_or_else(|| panic!("{r}"));
    let (code, exact) = run(&[
        "loop",
        "question",
        "--specification",
        &p,
        "--max-work",
        &used.to_string(),
    ]);
    assert_eq!(code, Some(0), "{exact}");
    let (code, short) = run(&[
        "loop",
        "question",
        "--specification",
        &p,
        "--max-work",
        &(used - 1).to_string(),
    ]);
    assert_eq!(code, Some(2), "{short}");
    assert!(short["detail"]["count"].is_null());
    assert_eq!(
        short["detail"]["law_usage"],
        r["detail"]["candidate"]["law_usage"]
    );
}

#[test]
fn version_space_cpmm_rounding_answers_reach_checked_singletons() {
    let d = Directory::new();
    for bps in [0, 30, 100] {
        // A declared amount slice for x=y=40; the proposal crate separately
        // covers all reserve slices through 40. No full-cube CLI claim here.
        let fee = format!("?{{mul_div_floor(a, {bps}, 10000) | mul_div_ceil(a, {bps}, 10000)}}");
        let net = format!("(a - mul_div_ceil(a, {bps}, 10000))");
        let output = format!(
            "?{{mul_div_floor({net}, 40, 40 + {net}) | mul_div_ceil({net}, 40, 40 + {net})}}"
        );
        for (kind, sketch) in [("fee", fee), ("output", output)] {
            let mut specification = json!({"schema":"zeno-fcis/loop-version-space/1",
                "inputs":[{"name":"a","domain":{"kind":"Int","min":0,"max":40}}],
                "outputs":[{"kind":"Int","min":0,"max":40}],"sketch":[sketch],"requirements":[]});
            let mut finished = false;
            for round in 0..=1 {
                let p = d.json("cpmm.json", &specification);
                let (code, r) = run(&["loop", "question", "--specification", &p]);
                assert_eq!(
                    code,
                    Some(if r["status"] == "determined" { 0 } else { 2 }),
                    "{r}"
                );
                if r["status"] == "determined" {
                    assert_eq!(r["detail"]["candidate"]["status"], "checked");
                    assert!(
                        r["detail"]["candidate"]["checks"]
                            .as_array()
                            .is_some_and(|checks| checks
                                .iter()
                                .all(|c| c["verdict"] == "equivalent" && c["replay"] == "matched"))
                    );
                    finished = true;
                    break;
                }
                assert_eq!(r["status"], "question", "{r}");
                let question = &r["detail"]["question"];
                let a = question["input"][0]
                    .as_i64()
                    .unwrap_or_else(|| panic!("{r}"));
                let fee = (a * bps + 9999) / 10000;
                let net = a - fee;
                let answer = if kind == "fee" {
                    fee
                } else {
                    net * 40 / (40 + net)
                };
                specification["requirements"]
                    .as_array_mut()
                    .unwrap_or_else(|| panic!("requirements"))
                    .push(json!({"id":format!("answer_{round}"),"input":[a],"output":[answer]}));
            }
            assert!(
                finished,
                "two rounding meanings must yield a checked singleton"
            );
        }
    }
}
