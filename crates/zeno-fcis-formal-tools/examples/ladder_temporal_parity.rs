//! Diagnostic replay harness for temporal ladder fixtures; authority: none.
//! Reuses the private current evaluator and the public typed parser/exporter.
use std::io::{self, Read};
extern crate zeno_fcis_spec as spec_authoring;
#[path = "../src/model_diagnostics.rs"]
#[allow(dead_code)]
mod model_diagnostics;
use model_diagnostics::{DiagnosticLimits, ModelObservation, ModelStep, check_trace};
use serde_json::{Value, json};
use spec_authoring::{
    ClaimFormula, ProjectLimits, ProjectionPath, SourceLimits, elaborate_project, parse_project,
};
use zeno_fcis_formal_tools::{ExportLimits, ToolBackend, export_smt_with_limits};

fn path(source: &str) -> Result<ProjectionPath, String> {
    let (root, rest) = source.split_once('.').ok_or("replay:projection-path")?;
    let root = match root {
        "pre" => spec_authoring::ProjectionRoot::Pre,
        "post" => spec_authoring::ProjectionRoot::Post,
        _ => return Err("replay:projection-root".into()),
    };
    let segments = rest
        .split('.')
        .map(|part| {
            let number = part.parse().map_err(|_| "replay:projection-id")?;
            spec_authoring::StableId::new(number).ok_or("replay:projection-id")
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProjectionPath::try_new(root, segments).ok_or_else(|| "replay:projection-path".into())
}

fn array<'a>(value: &'a Value, refusal: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| refusal.into())
}

fn integer(value: &Value) -> Result<i128, String> {
    if let Some(text) = value.as_str() {
        text.parse().map_err(|_| "replay:integer".into())
    } else if let Some(boolean) = value.as_bool() {
        Ok(i128::from(boolean))
    } else {
        value
            .as_i64()
            .map(i128::from)
            .ok_or_else(|| "replay:integer".into())
    }
}

fn replay(case: &Value) -> Result<Value, String> {
    let source = case["source"].as_str().ok_or("replay:source")?;
    let parsed = parse_project(source, SourceLimits::default())
        .map_err(|error| format!("replay:native-parse: {error:?}"))?;
    let project = elaborate_project(parsed, ProjectLimits::default())
        .map_err(|error| format!("replay:native-elaboration: {error:?}"))?;
    if project.claims().len() != 1 {
        return Err("replay:one-claim-required".into());
    }
    let claim = &project.claims()[0];
    let ClaimFormula::Temporal(formula) = claim.formula() else {
        return Err("replay:temporal-claim-required".into());
    };
    let paths = array(&case["paths"], "replay:paths")?
        .iter()
        .map(|value| path(value.as_str().ok_or("replay:projection-path")?))
        .collect::<Result<Vec<_>, _>>()?;
    let positions = match case.get("positions") {
        None => false,
        Some(value) => value.as_bool().ok_or("replay:positions")?,
    };
    let limits = DiagnosticLimits::default();
    let mut verdicts = Vec::new();
    for word in array(&case["words"], "replay:words")? {
        let steps = array(word, "replay:word")?
            .iter()
            .map(|event| {
                let values = array(event, "replay:event")?;
                if values.len() != paths.len() {
                    return Err("replay:event-width".into());
                }
                let observations = values
                    .iter()
                    .zip(&paths)
                    .filter(|(value, _)| !value.is_null())
                    .map(|(value, path)| Ok(ModelObservation::new(path.clone(), integer(value)?)))
                    .collect::<Result<Vec<_>, String>>()?;
                ModelStep::try_new(observations).ok_or_else(|| "replay:distinct-paths".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if positions {
            let at_positions = (0..steps.len())
                .map(|position| {
                    format!(
                        "{:?}",
                        check_trace(formula, claim.mode(), &steps[position..], limits)
                    )
                })
                .collect::<Vec<_>>();
            verdicts.push(json!(at_positions));
        } else {
            verdicts.push(json!(format!(
                "{:?}",
                check_trace(formula, claim.mode(), &steps, limits)
            )));
        }
    }
    let export = export_smt_with_limits(claim, ToolBackend::Cvc5, ExportLimits::default());
    Ok(json!({"verdicts": verdicts, "finite_export": format!("{:?}",export.as_ref().map(|_| ())) }))
}

fn request() -> Result<Value, String> {
    const INPUT_LIMIT: u64 = 32 * 1024 * 1024;
    let mut input = String::new();
    io::stdin()
        .take(INPUT_LIMIT + 1)
        .read_to_string(&mut input)
        .map_err(|error| format!("replay:input: {error}"))?;
    if input.len() as u64 > INPUT_LIMIT {
        return Err("replay:input-budget".into());
    }
    let request: Value =
        serde_json::from_str(&input).map_err(|error| format!("replay:json: {error}"))?;
    let results = array(&request["cases"], "replay:cases")?
        .iter()
        .map(|case| replay(case).unwrap_or_else(|refusal| json!({"refusal":refusal})))
        .collect::<Vec<_>>();
    Ok(json!({"results":results,"authority":"none"}))
}

fn main() {
    let output = request().unwrap_or_else(|refusal| json!({"refusal":refusal,"authority":"none"}));
    println!("{output}");
}
