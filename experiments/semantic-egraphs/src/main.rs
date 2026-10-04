mod boolean;
mod evaluation;
mod optimizer;

use boolean::{Boolean, admit_total_boolean, lower, parse_outputs, program_json};
use egg::RecExpr;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    time::Instant,
};
use zeno_fcis_synthesis::finite::Program;

const EGG_HEAD: &str = "12997cd15a123156abd726155363701f13358040";
const FCIS_HEAD: &str = "cb1366cddc5c813f9977b377f541e64bcf94035c";

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    nvars: usize,
    family: String,
    outputs: Vec<String>,
    baseline_outputs: Vec<String>,
    truth_signatures: Vec<String>,
}
struct Config {
    input: PathBuf,
    output: PathBuf,
    iterations: usize,
    node_limit: usize,
    lp_seconds: f64,
    tree: bool,
}
fn config() -> Result<Config, String> {
    let mut args = std::env::args().skip(1);
    let input=args.next().ok_or("Usage: semantic-egraphs corpus.json results.json [--iterations6 --node-limit5000 --lp-seconds2 --tree]")?.into();
    let output = args.next().ok_or("Missing output JSON path")?.into();
    let mut config = Config {
        input,
        output,
        iterations: 6,
        node_limit: 5000,
        lp_seconds: 2.0,
        tree: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => {
                config.iterations = args
                    .next()
                    .ok_or("Missing iteration count")?
                    .parse()
                    .map_err(|_| "Invalid iteration count")?
            }
            "--node-limit" => {
                config.node_limit = args
                    .next()
                    .ok_or("Missing node limit")?
                    .parse()
                    .map_err(|_| "Invalid node limit")?
            }
            "--lp-seconds" => {
                config.lp_seconds = args
                    .next()
                    .ok_or("Missing LP seconds")?
                    .parse()
                    .map_err(|_| "Invalid LP seconds")?
            }
            "--tree" => config.tree = true,
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    if config.iterations == 0
        || config.iterations > 100
        || config.node_limit == 0
        || config.node_limit > 100_000
        || !config.lp_seconds.is_finite()
        || config.lp_seconds <= 0.0
        || config.lp_seconds > 600.0
    {
        return Err("Resource limits outside experiment admission bounds".into());
    }
    Ok(config)
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn panic_reason(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "Non-string panic".into())
}
fn failed(error: String, outputs: Vec<String>, elapsed: f64) -> Value {
    json!({"status":"error","outputs":outputs,"error":error,"timings_ms":{"total":elapsed},"evaluation":{"gate_passed":false}})
}

fn variant(
    case: &Case,
    source: &Program,
    original: &[RecExpr<Boolean>],
    outputs: Vec<String>,
    metadata: Value,
    failure: Option<String>,
    total_start: Instant,
) -> Value {
    let evaluation_start = Instant::now();
    let expressions = match parse_outputs(&outputs, case.nvars) {
        Ok(expressions) => expressions,
        Err(error) => return failed(error, outputs, total_start.elapsed().as_secs_f64() * 1000.0),
    };
    let candidate = match lower(&expressions, case.nvars) {
        Ok(program) => program,
        Err(error) => return failed(error, outputs, total_start.elapsed().as_secs_f64() * 1000.0),
    };
    if let Err(error) = admit_total_boolean(&candidate) {
        return failed(error, outputs, total_start.elapsed().as_secs_f64() * 1000.0);
    }
    let evaluation = evaluation::compare(
        source,
        &candidate,
        case.nvars,
        Some(original),
        Some(&expressions),
        Some(&case.truth_signatures),
    );
    let passed = evaluation["gate_passed"].as_bool() == Some(true) && failure.is_none();
    let program = program_json(&candidate);
    let error = failure.or_else(|| {
        if passed {
            None
        } else {
            Some("Complete FCIS/direct/corpus evaluation gate rejected candidate".into())
        }
    });
    json!({"status":if passed {"accepted"}else if metadata["solver_status"]=="time_limit"||metadata["solver_status"]=="gap_limit"||metadata["solver_status"]=="unknown" {"error"}else{"rejected"},
           "outputs":outputs,"fcis_nodes":candidate.nodes().len(),"program":program,
           "checksums":{"program_json_sha256":sha(&serde_json::to_vec(&program).unwrap()),
                        "truth_trace_sha256":sha(&serde_json::to_vec(&evaluation["truth_signatures"]).unwrap())},
           "evaluation":evaluation,"optimization":metadata,"error":error,
           "timings_ms":{"evaluation":evaluation_start.elapsed().as_secs_f64()*1000.0,"total":total_start.elapsed().as_secs_f64()*1000.0}})
}

fn run_case(case: &Case, config: &Config) -> Value {
    let started = Instant::now();
    let original = match parse_outputs(&case.outputs, case.nvars) {
        Ok(expressions) => expressions,
        Err(error) => {
            return json!({"id":case.id,"nvars":case.nvars,"family":case.family,"source_error":error,"variants":{}});
        }
    };
    let source = match lower(&original, case.nvars).and_then(|program| {
        admit_total_boolean(&program)?;
        Ok(program)
    }) {
        Ok(program) => program,
        Err(error) => {
            return json!({"id":case.id,"nvars":case.nvars,"family":case.family,"source_error":error,"variants":{}});
        }
    };
    let mut variants = BTreeMap::new();
    for (name, outputs) in [
        ("raw", case.outputs.clone()),
        ("local_baseline", case.baseline_outputs.clone()),
    ] {
        variants.insert(
            name.to_string(),
            variant(
                case,
                &source,
                &original,
                outputs,
                json!({"solver_status":"not_applicable"}),
                None,
                Instant::now(),
            ),
        );
    }
    let mut configurations = vec![("rewrite_egg", false, false), ("semantic_egg", true, false)];
    if config.tree {
        configurations.push(("tree_egg", false, true));
    }
    for (name, semantic, tree) in configurations {
        let start = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| {
            optimizer::optimize(
                &original,
                case.nvars,
                semantic,
                tree,
                config.iterations,
                config.node_limit,
                config.lp_seconds,
            )
        }));
        let value = match result {
            Ok(Ok(output)) => variant(
                case,
                &source,
                &original,
                output.outputs,
                output.metadata,
                output.failed,
                start,
            ),
            Ok(Err(error)) => failed(error, Vec::new(), start.elapsed().as_secs_f64() * 1000.0),
            Err(payload) => failed(
                format!("Optimizer panic: {}", panic_reason(payload)),
                Vec::new(),
                start.elapsed().as_secs_f64() * 1000.0,
            ),
        };
        variants.insert(name.to_string(), value);
    }
    let semantic = &variants["semantic_egg"];
    let baseline = &variants["local_baseline"];
    let decrease = semantic["status"] == "accepted"
        && baseline["status"] == "accepted"
        && semantic["fcis_nodes"].as_u64() < baseline["fcis_nodes"].as_u64();
    let chosen = if decrease {
        "semantic_egg"
    } else {
        "local_baseline"
    };
    json!({"id":case.id,"nvars":case.nvars,"family":case.family,"variants":variants,
           "accepted_policy":{"chosen_variant":chosen,"strict_actual_decrease":decrease,"fcis_nodes":variants[chosen]["fcis_nodes"],
                               "policy":"Fallback to checked local baseline unless candidate passes the complete gate and strictly reduces actual FCIS nodes; raw optimizer regressions remain recorded."},
           "total_ms":started.elapsed().as_secs_f64()*1000.0})
}

fn run() -> Result<bool, String> {
    let config = config()?;
    optimizer::initialize_logger()?;
    let input = fs::read(&config.input).map_err(|e| format!("Cannot read corpus: {e}"))?;
    let corpus: Corpus =
        serde_json::from_slice(&input).map_err(|e| format!("Invalid corpus: {e}"))?;
    if corpus.cases.is_empty() || corpus.cases.len() > 1000 {
        return Err("Corpus case resource limit".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for case in &corpus.cases {
        if !seen.insert(&case.id) {
            return Err(format!("Duplicate case identity {}", case.id));
        }
    }
    let controls = evaluation::controls()?;
    let started = Instant::now();
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for (index, case) in corpus.cases.iter().enumerate() {
        let result = run_case(case, &config);
        if let Some(error) = result.get("source_error") {
            failures.push(json!({"case":case.id,"variant":"source","error":error}));
        }
        if let Some(variants) = result["variants"].as_object() {
            for (name, variant) in variants {
                if variant["status"] != "accepted" {
                    failures.push(json!({"case":case.id,"variant":name,"error":variant["error"]}));
                }
            }
        }
        eprintln!(
            "case {}/{} {} {}",
            index + 1,
            corpus.cases.len(),
            case.id,
            if result["source_error"].is_null() {
                "evaluated"
            } else {
                "source-error"
            }
        );
        results.push(result);
    }
    for control in &controls {
        if control["passed"] != true {
            failures
                .push(json!({"control":control["id"],"error":"Expected rejection did not occur"}));
        }
    }
    let passed = failures.is_empty();
    let report = json!({"schema_version":1,"corpus_sha256":sha(&input),"config":{"iterations":config.iterations,"node_limit":config.node_limit,
                     "lp_seconds":config.lp_seconds,"tree_ablation":config.tree,"seeded_leaves":"true,false,and every declared vi in ascending index order","rewrite_rules":33},
        "provenance":{"egg_head":EGG_HEAD,"fcis_head":FCIS_HEAD,"egg_lp_extract_sha256":sha(include_bytes!("../vendor/egg/src/lp_extract.rs")),
                      "harness_cargo_lock_sha256":sha(include_bytes!("../Cargo.lock")),"rust_requirement":"1.97.1","good_lp_locked":"1.15.3",
                      "evaluator":"Actual zeno_fcis_synthesis::finite::Program.evaluate","truth_signature":"All2^n complete Boolean valuations, n<=6; rowmask bit i supplies vi","experiment_scope":"Total typed Bool regions only; no arithmetic, purity/authority changes or protocol behavior changes."},
        "cases":results,"controls":controls,"gate":{"passed":passed,"failed_variants":failures,"total_cases":corpus.cases.len(),"total_ms":started.elapsed().as_secs_f64()*1000.0},
        "limitations":["LP weights are a surrogate; Or4 lowering can share generated Not nodes, so backend optimality is not final FCIS optimality.",
                       "Both rewrite variants compute truth signatures to check rewrite soundness; only semantic_egg additionally unions equal signatures.",
                       "Binary rewrite patterns use commutation/reassociation to expose generalized factors; bounded exploration differs from the greedy flattened local baseline.",
                       "No benchmark success declaration is made here; independent analyzer applies the preregistered criterion to raw candidates and reports regressions.",
                       "Case structures are deterministic with pinned sources/lock/backend seeds on this environment; timings and finite numerical backend behavior are not formal guarantees."]});
    if let Some(parent) = config.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let temporary = config.output.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, &config.output).map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({"output":config.output,"gate_passed":passed,"cases":corpus.cases.len(),"failures":report["gate"]["failed_variants"].as_array().unwrap().len()})
    );
    Ok(passed)
}
fn main() {
    match run() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
