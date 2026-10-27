//! Finite universal replay for the withdrawal temporal pilot; authority: none.
//! Enumerates every distinct observation word of lengths 1 through a bound in
//! the language of a supplied finite observation automaton, and judges each
//! word with the private current temporal evaluator, as the parity harness does.
//! `inventory-lean ROOT` prints the portable tree identity of a Lean closure.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
extern crate zeno_fcis_spec as spec_authoring;
#[path = "../src/model_diagnostics.rs"]
#[allow(dead_code)]
mod model_diagnostics;
use model_diagnostics::{
    DiagnosticLimits, ModelObservation, ModelStep, TemporalDiagnosis, check_trace,
};
use serde_json::{Value, json};
use spec_authoring::{
    ClaimFormula, ProjectLimits, ProjectionPath, SourceLimits, elaborate_project, parse_project,
};

const MAX_WORDS: u64 = 50_000_000;
const MAX_STATES: usize = 4096;

fn path(source: &str) -> Result<ProjectionPath, String> {
    let (root, rest) = source.split_once('.').ok_or("pilot:projection-path")?;
    if root != "pre" {
        return Err("pilot:projection-root".into());
    }
    let segments = rest
        .split('.')
        .map(|part| {
            let number = part.parse().map_err(|_| "pilot:projection-id")?;
            spec_authoring::StableId::new(number).ok_or("pilot:projection-id")
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProjectionPath::try_new(spec_authoring::ProjectionRoot::Pre, segments)
        .ok_or_else(|| "pilot:projection-path".into())
}

fn integer(value: &Value) -> Result<i128, String> {
    value
        .as_i64()
        .map(i128::from)
        .ok_or_else(|| "pilot:integer".into())
}

fn index(value: &Value, states: usize) -> Result<usize, String> {
    let index = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("pilot:state")?;
    if index < states {
        Ok(index)
    } else {
        Err("pilot:state-range".into())
    }
}

type Letter = Vec<i128>;

struct Automaton {
    initial: BTreeSet<usize>,
    edges: Vec<Vec<(Letter, usize)>>,
}

fn automaton(value: &Value, width: usize) -> Result<Automaton, String> {
    let states = value["states"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| (1..=MAX_STATES).contains(v))
        .ok_or("pilot:states")?;
    let initial = value["initial"]
        .as_array()
        .ok_or("pilot:initial")?
        .iter()
        .map(|v| index(v, states))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if initial.is_empty() {
        return Err("pilot:initial-empty".into());
    }
    let mut edges = vec![Vec::new(); states];
    for edge in value["edges"].as_array().ok_or("pilot:edges")? {
        let [from, letter, to] = edge.as_array().map(Vec::as_slice).ok_or("pilot:edge")? else {
            return Err("pilot:edge-shape".into());
        };
        let letter = letter
            .as_array()
            .ok_or("pilot:letter")?
            .iter()
            .map(integer)
            .collect::<Result<Letter, _>>()?;
        if letter.len() != width {
            return Err("pilot:letter-width".into());
        }
        edges[index(from, states)?].push((letter, index(to, states)?));
    }
    Ok(Automaton { initial, edges })
}

struct Report {
    by_length: Vec<u64>,
    satisfied: u64,
    counterexamples: u64,
    indeterminate: u64,
    shown: Vec<Value>,
    show: usize,
}

fn step(paths: &[ProjectionPath], letter: &Letter) -> Result<ModelStep, String> {
    let observations = paths
        .iter()
        .zip(letter)
        .map(|(path, value)| ModelObservation::new(path.clone(), *value))
        .collect();
    ModelStep::try_new(observations).ok_or_else(|| "pilot:distinct-paths".into())
}

#[allow(clippy::too_many_arguments)]
fn visit(
    automaton: &Automaton,
    states: &BTreeSet<usize>,
    word: &mut Vec<Letter>,
    steps: &mut Vec<ModelStep>,
    paths: &[ProjectionPath],
    judge: &dyn Fn(&[ModelStep]) -> TemporalDiagnosis,
    bound: usize,
    report: &mut Report,
) -> Result<(), String> {
    if !word.is_empty() {
        let total = report.satisfied + report.counterexamples + report.indeterminate;
        if total >= MAX_WORDS {
            return Err("UNKNOWN:pilot-word-budget".into());
        }
        report.by_length[word.len() - 1] += 1;
        match judge(steps) {
            TemporalDiagnosis::Satisfied => report.satisfied += 1,
            TemporalDiagnosis::Counterexample { step } => {
                report.counterexamples += 1;
                if report.shown.len() < report.show {
                    report
                        .shown
                        .push(json!({"word": word.clone(), "step": step}));
                }
            }
            other => {
                report.indeterminate += 1;
                if report.shown.len() < report.show {
                    report
                        .shown
                        .push(json!({"word": word.clone(), "indeterminate": format!("{other:?}")}));
                }
            }
        }
    }
    if word.len() == bound {
        return Ok(());
    }
    // Determinize lazily: one child per distinct letter, in letter order.
    let mut next: BTreeMap<&Letter, BTreeSet<usize>> = BTreeMap::new();
    for state in states {
        for (letter, to) in &automaton.edges[*state] {
            next.entry(letter).or_default().insert(*to);
        }
    }
    for (letter, successors) in next {
        word.push(letter.clone());
        steps.push(step(paths, letter)?);
        visit(
            automaton,
            &successors,
            word,
            steps,
            paths,
            judge,
            bound,
            report,
        )?;
        steps.pop();
        word.pop();
    }
    Ok(())
}

fn replay(request: &Value) -> Result<Value, String> {
    let source = request["source"].as_str().ok_or("pilot:source")?;
    let parsed = parse_project(source, SourceLimits::default())
        .map_err(|error| format!("pilot:native-parse: {error:?}"))?;
    let project = elaborate_project(parsed, ProjectLimits::default())
        .map_err(|error| format!("pilot:native-elaboration: {error:?}"))?;
    if project.claims().len() != 1 {
        return Err("pilot:one-claim-required".into());
    }
    let claim = &project.claims()[0];
    let ClaimFormula::Temporal(formula) = claim.formula() else {
        return Err("pilot:temporal-claim-required".into());
    };
    let paths = request["paths"]
        .as_array()
        .ok_or("pilot:paths")?
        .iter()
        .map(|v| path(v.as_str().ok_or("pilot:projection-path")?))
        .collect::<Result<Vec<_>, _>>()?;
    let automaton = automaton(&request["automaton"], paths.len())?;
    let bound = request["max_length"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| (1..=256).contains(v))
        .ok_or("pilot:max-length")?;
    let show = request["show"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(3);
    let limits = DiagnosticLimits::default();
    let mode = claim.mode();
    let judge = move |steps: &[ModelStep]| check_trace(formula, mode, steps, limits);
    let mut report = Report {
        by_length: vec![0; bound],
        satisfied: 0,
        counterexamples: 0,
        indeterminate: 0,
        shown: Vec::new(),
        show,
    };
    visit(
        &automaton,
        &automaton.initial,
        &mut Vec::new(),
        &mut Vec::new(),
        &paths,
        &judge,
        bound,
        &mut report,
    )?;
    Ok(json!({
        "authority": "none",
        "mode": format!("{mode:?}"),
        "words_by_length": report.by_length,
        "words": report.satisfied + report.counterexamples + report.indeterminate,
        "satisfied": report.satisfied,
        "counterexamples": report.counterexamples,
        "indeterminate": report.indeterminate,
        "shown": report.shown,
    }))
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let [command, root] = arguments.as_slice()
        && command == "inventory-lean"
    {
        match zeno_fcis_formal_tools::inspect_lean_toolchain(std::path::Path::new(root)) {
            Ok(inventory) => println!("lean tree_sha256 {}", inventory.tree_sha256()),
            Err(error) => println!("lean inventory refused: {error:?}"),
        }
        return;
    }
    const INPUT_LIMIT: u64 = 64 * 1024 * 1024;
    let mut input = String::new();
    let output = io::stdin()
        .take(INPUT_LIMIT + 1)
        .read_to_string(&mut input)
        .map_err(|error| format!("pilot:input: {error}"))
        .and_then(|size| {
            if size as u64 > INPUT_LIMIT {
                Err("pilot:input-budget".into())
            } else {
                serde_json::from_str::<Value>(&input)
                    .map_err(|error| format!("pilot:json: {error}"))
            }
        })
        .and_then(|request| replay(&request))
        .unwrap_or_else(|refusal| json!({"refusal": refusal, "authority": "none"}));
    println!("{output}");
}
