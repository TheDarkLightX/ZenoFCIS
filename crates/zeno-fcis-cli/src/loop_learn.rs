//! LLM-generated shell for the fixed-vocabulary learn door. The enumeration
//! proposes; `Program::evaluate` decides every lowered example comparison.

use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;
use zeno_fcis_loop_doors::expr::print_symbolic;
use zeno_fcis_loop_doors::learn::{
    self, Budget, Example, ExampleCheck, Limits, Observation, Operation, Specification, Status,
    Stop,
};
use zeno_fcis_synthesis::finite::Domain;

use super::{STATELESS_DEADLINE, Stages, accepted, doors_result, pipeline, read_bytes};
use crate::neural_loop::program_json::{encode, program_to_json};
use crate::transform::sha256_hex;

const SCHEMA: &str = "zeno-fcis/loop-learn/1";

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum DomainDocument {
    Bool {},
    Int { min: i64, max: i64 },
}
impl DomainDocument {
    fn domain(&self) -> Domain {
        match self {
            Self::Bool {} => Domain::Bool,
            Self::Int { min, max } => Domain::Int {
                min: *min,
                max: *max,
            },
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDocument {
    name: String,
    domain: DomainDocument,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExampleDocument {
    input: Vec<i64>,
    output: Vec<i64>,
}

fn constants() -> Vec<i64> {
    vec![1, 2, 3]
}
fn arithmetic() -> Vec<String> {
    vec!["+".into(), "-".into()]
}
fn size() -> usize {
    7
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VocabularyDocument {
    #[serde(default = "constants")]
    constants: Vec<i64>,
    #[serde(default = "arithmetic")]
    arithmetic: Vec<String>,
    #[serde(default)]
    macros: Vec<String>,
    #[serde(default)]
    compare: Vec<String>,
    #[serde(default)]
    boolean: Vec<String>,
    #[serde(default)]
    conditional: bool,
}
impl Default for VocabularyDocument {
    fn default() -> Self {
        Self {
            constants: constants(),
            arithmetic: arithmetic(),
            macros: Vec::new(),
            compare: Vec::new(),
            boolean: Vec::new(),
            conditional: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    inputs: Vec<InputDocument>,
    outputs: Vec<DomainDocument>,
    #[serde(default)]
    vocabulary: VocabularyDocument,
    #[serde(default = "size")]
    max_size: usize,
    #[serde(default)]
    misses: usize,
    #[serde(default)]
    examples: Vec<ExampleDocument>,
}

fn specification(bytes: &[u8]) -> Result<Specification, String> {
    if !bytes.is_ascii() {
        return Err("learn-document-ascii".into());
    }
    let doc: Document = serde_json::from_slice(bytes).map_err(|e| format!("learn-document:{e}"))?;
    if doc.schema != SCHEMA {
        return Err("learn-schema".into());
    }
    if doc.misses != 0 {
        return Err("learn-confirmed-examples-require-zero-misses".into());
    }
    let vocab = doc.vocabulary;
    let mut operations = Vec::new();
    for s in vocab.arithmetic {
        operations.push(match s.as_str() {
            "+" => Operation::Add,
            "-" => Operation::Sub,
            _ => return Err("learn-arithmetic-operator".into()),
        });
    }
    for s in vocab.macros {
        operations.push(match s.as_str() {
            "min" => Operation::Min,
            "max" => Operation::Max,
            "abs" => Operation::Abs,
            _ => return Err("learn-standard-macro-required".into()),
        });
    }
    for s in vocab.compare {
        operations.push(match s.as_str() {
            "<" => Operation::Lt,
            "<=" => Operation::Le,
            "==" => Operation::Eq,
            "!=" => Operation::Ne,
            _ => return Err("learn-comparison-operator".into()),
        });
    }
    for s in vocab.boolean {
        operations.push(match s.as_str() {
            "&&" => Operation::And,
            "||" => Operation::Or,
            "!" => Operation::Not,
            _ => return Err("learn-boolean-operator".into()),
        });
    }
    if vocab.conditional {
        operations.push(Operation::Select);
    }
    let spec = Specification {
        names: doc.inputs.iter().map(|d| d.name.clone()).collect(),
        inputs: doc.inputs.iter().map(|d| d.domain.domain()).collect(),
        outputs: doc.outputs.iter().map(DomainDocument::domain).collect(),
        constants: vocab.constants,
        operations,
        max_size: doc.max_size,
        examples: doc
            .examples
            .into_iter()
            .map(|e| Example {
                input: e.input,
                output: e.output,
            })
            .collect(),
    };
    spec.vocabulary().map_err(|e| format!("{e:?}"))?;
    Ok(spec)
}

fn usage(budget: &Budget, bytes: usize) -> Value {
    let u = budget.usage();
    json!({"input_bytes": bytes, "work": u.work, "cells": u.cells, "conflict_subsets": u.conflict_subsets})
}
fn limits_data(limits: Limits, max_size: Option<usize>) -> Value {
    json!({"max_work":limits.work,"max_tuples":limits.tuples,
        "max_meanings":limits.meanings,"max_cells":limits.cells,
        "max_conflict_subsets":limits.conflict_subsets,"max_size":max_size})
}
fn stop(status: &str, reason: &str, budget: &Budget, bytes: usize) -> (u8, Value) {
    doors_result(
        if status == "inconclusive" {
            2
        } else {
            crate::INVALID
        },
        "learn",
        status,
        json!({"reason": reason, "enumeration": "proposal", "counts": null, "question": null,
            "candidate": null, "core": null, "usage": usage(budget, bytes),
            "limits":limits_data(budget.limits(),None),"attempts_spent": 0}),
    )
}
fn stopped(e: Stop, budget: &Budget, bytes: usize) -> (u8, Value) {
    match e {
        Stop::Inconclusive(reason) => stop("inconclusive", &reason, budget, bytes),
        Stop::Refused(reason) => stop("refused", &reason, budget, bytes),
    }
}
fn observation(o: Observation) -> Value {
    match o {
        Observation::Value(v) => json!({"value": v}),
        Observation::Arithmetic => json!({"failure": "arithmetic"}),
        Observation::OutputDomain => json!({"failure": "output-domain"}),
    }
}

/// Stateless, contained search. Caller flags may only reduce the default caps.
pub(super) fn run(
    path: &Path,
    original: Option<&Path>,
    out: Option<&Path>,
    limits: Limits,
) -> (u8, Value) {
    let begun = Instant::now();
    let mut budget = Budget::new(limits);
    let ceiling = Limits::default();
    if out.is_some_and(Path::exists) {
        return stop("refused", "output-must-be-new-directory", &budget, 0);
    }
    if limits.work > ceiling.work
        || limits.tuples > ceiling.tuples
        || limits.meanings > ceiling.meanings
        || limits.cells > ceiling.cells
        || limits.conflict_subsets > ceiling.conflict_subsets
    {
        return stop("refused", "learn-limit-ceiling", &budget, 0);
    }
    let bytes = match read_bytes(path) {
        Ok(bytes) => bytes,
        Err((_, report)) => {
            return stop(
                "refused",
                report["detail"]["reason"]
                    .as_str()
                    .unwrap_or("learn-document-read"),
                &budget,
                0,
            );
        }
    };
    let input_bytes = bytes.len();
    let spec = match specification(&bytes) {
        Ok(spec) => spec,
        Err(reason) => return stop("refused", &reason, &budget, input_bytes),
    };
    // BEGIN L2: bind the complete reference before proposal enumeration.
    let reference = match original {
        None => None,
        Some(path) => match pipeline::read_program(path) {
            Err(result) => return result,
            Ok(bytes) => {
                let program = match zeno_fcis_synthesis::finite_runtime::import_program(&bytes) {
                    Ok(program) => program,
                    Err(_) => {
                        return stop("refused", "learn-reference-admission", &budget, input_bytes);
                    }
                };
                if program.inputs() != spec.inputs.as_slice()
                    || program.outputs() != spec.outputs.as_slice()
                {
                    return stop("refused", "learn-reference-abi", &budget, input_bytes);
                }
                Some(bytes)
            }
        },
    };
    // END L2.
    let mut stages = Stages::new(begun, STATELESS_DEADLINE, 0);
    let analysis = match stages.run(true, || learn::analyze(&spec, &mut budget)) {
        Err(reason) => return stop("inconclusive", &reason, &budget, input_bytes),
        Ok(Err(e)) => return stopped(e, &budget, input_bytes),
        Ok(Ok(analysis)) => analysis,
    };
    let mut candidate = Value::Null;
    if let Some(program) = &analysis.candidate {
        let check = match stages.run(false, || learn::check_examples(&spec, program, &mut budget)) {
            Err(reason) => return stop("inconclusive", &reason, &budget, input_bytes),
            Ok(Err(e)) => return stopped(e, &budget, input_bytes),
            Ok(Ok(check)) => check,
        };
        match check {
            ExampleCheck::Reproduced { checked } => {
                let canonical = match encode(program) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        return stop(
                            "refused",
                            &format!("learn-candidate-encoding:{e:?}"),
                            &budget,
                            input_bytes,
                        );
                    }
                };
                candidate = json!({"status": "examples-reproduced", "sha256": sha256_hex(&canonical),
                    "program": program_to_json(program), "examples_check": {
                        "decided_by": "library-evaluator", "checked": checked,
                        "vacuous": checked == 0, "verdict": "reproduced"}});
            }
            ExampleCheck::Mismatch {
                example,
                checked,
                actual,
            } => {
                let (exit, mut result) = stop(
                    "refused",
                    "learn-lowered-example-mismatch",
                    &budget,
                    input_bytes,
                );
                result["detail"]["examples_check"] = json!({"decided_by": "library-evaluator", "verdict": "mismatch",
                    "example": example, "checked": checked, "actual": actual,
                    "expected": spec.examples[example].output});
                return (exit, result);
            }
        }
    }
    let question = analysis.question.as_ref().map(|q| json!({
        "input": q.input, "ordinal": q.ordinal, "worst": q.worst.to_string(), "squares": q.squares.to_string(),
        "buckets": q.buckets.iter().map(|bs| bs.iter().map(|b| json!({"observation": observation(b.observation),
            "remaining": b.remaining})).collect::<Vec<_>>()).collect::<Vec<_>>()
    }));
    let representatives: Vec<Value> = analysis.representatives.iter().enumerate().map(|(j, r)|
        json!({"output": j, "size": r.size, "text": r.text, "expanded": print_symbolic(&r.expression),
            "smallest": "proposal under declared vocabulary cost"})).collect();
    let (learning_status, exit) = match analysis.status {
        Status::Determined => ("determined", 0),
        Status::Question => ("question", 3),
        Status::OutsideVocabulary => ("outside-vocabulary", 1),
        Status::Ambiguous => ("ambiguous", 1),
    };
    let mut detail = json!({"enumeration": "proposal", "complete": true,
        "scope": "declared vocabulary, maximum expression size and finite domain only",
        "specification_sha256": sha256_hex(&bytes), "counts": analysis.counts,
        "before": analysis.before, "by_size": analysis.by_size, "question": question,
        "core": analysis.core, "representatives": representatives, "candidate": candidate,
        "usage": usage(&budget, input_bytes), "attempts_spent": 0,
        "learning_status":learning_status,"limits":limits_data(limits,Some(spec.max_size)),
        "search_bounds":{"max_size":spec.max_size,"operations":format!("{:?}",spec.operations)},
        "verification":{"status":"unsupported","reason":"complete-native-reference-required"}});
    // Every lowered fitting candidate flows to capture when a reference exists,
    // including a candidate whose vocabulary status remains `question`.
    if let (Some(reference), Some(program)) = (reference.as_deref(), analysis.candidate.as_ref()) {
        let mut verification = pipeline::Budget::new(pipeline::SearchLimits {
            max_tuples: limits.tuples,
            max_gate_pairs: 1_000_000,
            max_search_work: limits.work,
            max_cells: limits.cells.saturating_sub(budget.usage().cells),
            max_assignments: 2_000,
            max_terms: 128,
            max_checks: 24,
            deadline_ms: 22_000,
        });
        // Keep the original invocation's clock and spent frames across phases.
        verification.stages = stages;
        let check = verification.capture_and_gate(reference, program, budget.usage().work);
        let status = if accepted(&check) {
            "qualified"
        } else if check["verdict"] == "counterexample" {
            "refuted"
        } else if check["verdict"] == "refused" {
            "refused"
        } else {
            "inconclusive"
        };
        detail["status"] = json!(status);
        detail["best"] = if status == "qualified" {
            super::program_json(program)
        } else {
            Value::Null
        };
        detail["capture"] = check["capture"].clone();
        detail["best_check"] = check.clone();
        detail["verification"] = json!({"status":status,"check":check,
            "reference_sha256":sha256_hex(reference)});
        detail["verification_work"] = verification.work();
        detail["reason"] = check["reason"].clone();
        return pipeline::finish("learn", detail, status, out, Some(reference), Some(program));
    }
    doors_result(exit, "learn", learning_status, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Value {
        json!({"schema": SCHEMA, "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 2}}],
            "outputs": [{"kind": "Int", "min": 0, "max": 2}], "max_size": 1,
            "vocabulary": {"constants": [], "arithmetic": []}, "examples": []})
    }

    #[test]
    fn learn_document_refuses_research_primitives_noise_and_aliases() {
        let mut specimens = Vec::new();
        let mut value = declaration();
        value["vocabulary"]["arithmetic"] = json!(["*"]);
        specimens.push(value);
        let mut value = declaration();
        value["vocabulary"]["opaque"] = json!([]);
        specimens.push(value);
        let mut value = declaration();
        value["vocabulary"]["macros"] = json!([{"name":"custom", "body":"x*x"}]);
        specimens.push(value);
        let mut value = declaration();
        value["misses"] = json!(1);
        specimens.push(value);
        let mut value = declaration();
        value["inputs"][0]["domain"]["min"] = json!(false);
        specimens.push(value);
        let mut value = declaration();
        value["examples"] = json!([{"input":[true],"output":[1]}]);
        specimens.push(value);
        let mut value = declaration();
        value["inputs"][0]["domain"]["min"] = json!("0");
        specimens.push(value);
        let mut value = declaration();
        value["vocabulary"]["arithmetic"] = json!(["+", "+"]);
        specimens.push(value);
        let mut value = declaration();
        value["inputs"][0]["domain"] = json!({"kind":"Bool","min":0});
        specimens.push(value);
        for value in specimens {
            assert!(
                specification(value.to_string().as_bytes()).is_err(),
                "{value}"
            );
        }
        assert!(specification(declaration().to_string().as_bytes()).is_ok());
        assert!(
            specification(
                br#"{"schema":"zeno-fcis/loop-learn/1","schema":"zeno-fcis/loop-learn/1"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn learn_document_rejects_duplicate_names_before_enumeration_and_non_i64_examples() {
        let mut duplicate = declaration();
        duplicate["inputs"] = json!([
            duplicate["inputs"][0].clone(),
            duplicate["inputs"][0].clone()
        ]);
        // No enumeration budget is involved in parsing a declaration.
        let reason = specification(duplicate.to_string().as_bytes())
            .err()
            .unwrap_or_else(|| panic!("duplicate name accepted"));
        assert!(reason.contains("vocabulary-duplicate-name:x"), "{reason}");
        for coordinate in [json!(true), json!(1.5), json!(1.0), json!("1")] {
            for field in ["input", "output"] {
                let mut value = declaration();
                value["examples"] = json!([{"input":[1], "output":[1]}]);
                value["examples"][0][field][0] = coordinate.clone();
                assert!(
                    specification(value.to_string().as_bytes()).is_err(),
                    "{field}: {coordinate}"
                );
            }
        }
        let mut valid = declaration();
        valid["examples"] = json!([{"input":[1], "output":[1]}]);
        assert!(specification(valid.to_string().as_bytes()).is_ok());
    }
}
