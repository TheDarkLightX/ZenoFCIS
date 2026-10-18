//! Scenarios: expected runs written from intent and replayed exactly.
//!
//! A scenario file is a decision-examples file whose example lines are
//! grouped into runs by header comments, `# scenario NAME`. Every line is
//! read with the decision-examples grammar the application compiles, so
//! each step is also an ordinary decision example. A scenario's steps are
//! consecutive decisions: it starts at the pre-state its first step writes,
//! and every later step must write, as its pre-state, the state the
//! replayed decisions left. Laws are universal statements; a scenario is a
//! concrete one, and one scenario that expects a commit already tells a
//! core that rejects everything apart from the contract.
//!
//! The judge replays every scenario through the library Authority of the
//! contract and reports the first step whose decision, or written
//! pre-state, differs. When every scenario passes on the contract, it
//! replays them through each mutant of the extended catalog and through the
//! reject-everything core, each bound as the application binds it, and
//! credits a kill to the first scenario that fails there; when one fails on
//! the contract, those sections are withheld.

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::model::Contract;
use super::super::rules::{Class, Constant, Rules};
use super::evaluate::{self, Framer, Outcome};
use super::examples::{self, Example};
use super::machine::{self, Loaded};
use super::mutants;
use zeno_fcis_synthesis::finite::v2_authority::Authority;

/// The default scenario file of an application.
pub(crate) const FILE: &str = "tests/decision-scenarios.txt";

/// One scenario: its name, header line and steps.
#[derive(Clone, Debug)]
struct Scenario {
    name: String,
    line: usize,
    steps: Vec<Example>,
}

/// Where a replay stopped.
#[derive(Clone, Debug)]
struct Failure {
    step: usize,
    line: usize,
    reason: String,
    outcome: Option<Outcome>,
}

/// The judge's result: the packet and whether every scenario passed on the
/// contract.
#[derive(Clone, Debug)]
pub(crate) struct Replayed {
    pub(crate) packet: Value,
    pub(crate) failed: usize,
}

/// Replays every scenario of `text`.
///
/// # Errors
/// A malformed scenario file, a declaration or rule with no contract form,
/// or the library's refusal of the contract.
pub(crate) fn replay(
    project: &str,
    rules: &str,
    text: &str,
    label: &str,
) -> Result<Replayed, ContractError> {
    let loaded = Loaded::read(project, rules)?;
    let width = loaded.state_width;
    let genesis = genesis_values(&loaded);
    let contract = Contract::build(&loaded.declarations, &loaded.rules, loaded.commitment)?;
    let program = contract.program().ok();
    let (scenarios, results) = loaded.bind(&loaded.rules, |authority, framer| {
        let parsed = examples::parse(text, authority.descriptor()).map_err(|error| {
            ContractError::new(
                error.place().replacen(examples::FILE, label, 1),
                error.reason(),
            )
        })?;
        let scenarios = group(text, parsed, label)?;
        let results: Vec<Option<Failure>> = scenarios
            .iter()
            .map(|scenario| run(authority, framer, scenario, width))
            .collect();
        Ok((scenarios, results))
    })?;
    let mut failed = 0;
    let mut list = Vec::new();
    for (scenario, result) in scenarios.iter().zip(&results) {
        failed += usize::from(result.is_some());
        let start = &scenario.steps[0].inputs[..width];
        let mut value = json!({
            "name": scenario.name,
            "line": scenario.line,
            "steps": scenario.steps.len(),
            "start": if genesis.as_deref() == Some(start) { "genesis" } else { "stated" },
            "verdict": if result.is_some() { "failed" } else { "passed" },
        });
        if let Some(failure) = result {
            let mut detail = failure_json(failure);
            let step = &scenario.steps[failure.step];
            // Naming the case is a courtesy; without the program it is omitted.
            if let Some(index) = program
                .as_ref()
                .and_then(|program| program.evaluate(&step.inputs).ok())
                .and_then(|outputs| usize::try_from(*outputs.first()?).ok())
            {
                detail["case"] = json!(index);
                if let Some(case) = loaded.rules.cases.get(index) {
                    detail["when"] = json!(mutants::render(&case.when));
                }
            }
            value["failure"] = detail;
        }
        list.push(value);
    }
    // A kill is credited only to scenarios the contract itself passes: a
    // scenario that fails on the contract tells a mutant apart for a reason
    // the contract shares. With any failing scenario both sections are
    // withheld.
    if failed > 0 {
        let withheld = json!({
            "status": "withheld",
            "reason": "a scenario fails on the contract; kills are credited only when every scenario passes on it",
        });
        return Ok(Replayed {
            packet: json!({
                "scenarios": list,
                "count": scenarios.len(),
                "failed_on_contract": failed,
                "mutants": withheld,
                "reject_everything": withheld,
            }),
            failed,
        });
    }
    let mutation = mutants_killed(&loaded, &scenarios)?;
    let degenerate = match machine::reject_everything(&loaded.rules) {
        None => json!({"status": "not-applicable", "reason": "no case rejects"}),
        Some((core, reason)) => match bind_and_run(&loaded, &core, &scenarios) {
            Err(error) => json!({
                "status": "not-bound",
                "reject_reason": reason,
                "place": error.place(),
                "reason": error.reason(),
            }),
            Ok(failures) => {
                let killer = failures.iter().position(Option::is_some);
                json!({
                    "status": "bound",
                    "reject_reason": reason,
                    "killed": killer.is_some(),
                    "first_failing_scenario": killer.map(|index| scenarios[index].name.clone()),
                })
            }
        },
    };
    Ok(Replayed {
        packet: json!({
            "scenarios": list,
            "count": scenarios.len(),
            "failed_on_contract": failed,
            "mutants": mutation,
            "reject_everything": degenerate,
        }),
        failed,
    })
}

/// The genesis state's values, when every state field has one.
fn genesis_values(loaded: &Loaded) -> Option<Vec<i128>> {
    loaded.positions[..loaded.state_width]
        .iter()
        .map(
            |position| match loaded.rules.genesis.get(&position.field?)? {
                Constant::U128(_) => None,
                Constant::Bool(value) => Some(i128::from(*value)),
                Constant::Int(value) => Some(*value),
            },
        )
        .collect()
}

/// Groups the parsed examples under the scenario headers of `text`.
fn group(text: &str, parsed: Vec<Example>, label: &str) -> Result<Vec<Scenario>, ContractError> {
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut headers: Vec<(usize, String)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(comment) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let comment = comment.trim();
        let Some(name) = comment.strip_prefix("scenario") else {
            continue;
        };
        if !name.is_empty() && !name.starts_with(char::is_whitespace) {
            // Another comment, such as `# scenarios follow`.
            continue;
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(ContractError::new(
                format!("{label} line {}", index + 1),
                "a scenario header is `# scenario NAME`",
            ));
        }
        headers.push((index + 1, name.to_owned()));
    }
    let mut next = 0;
    for example in parsed {
        while next < headers.len() && headers[next].0 < example.line {
            let (line, name) = headers[next].clone();
            if scenarios.iter().any(|scenario| scenario.name == name) {
                return Err(ContractError::new(
                    format!("{label} line {line}"),
                    format!("repeats the scenario name `{name}`"),
                ));
            }
            scenarios.push(Scenario {
                name,
                line,
                steps: Vec::new(),
            });
            next += 1;
        }
        let Some(scenario) = scenarios.last_mut() else {
            return Err(ContractError::new(
                format!("{label} line {}", example.line),
                "is a step before any `# scenario NAME` header",
            ));
        };
        scenario.steps.push(example);
    }
    if next < headers.len() {
        let (line, _) = &headers[next];
        return Err(ContractError::new(
            format!("{label} line {line}"),
            "is a scenario with no steps",
        ));
    }
    if let Some(empty) = scenarios.iter().find(|scenario| scenario.steps.is_empty()) {
        return Err(ContractError::new(
            format!("{label} line {}", empty.line),
            "is a scenario with no steps",
        ));
    }
    if scenarios.is_empty() {
        return Err(ContractError::new(label, "holds no scenario"));
    }
    Ok(scenarios)
}

/// Replays one scenario through a bound Authority; `None` when every step
/// agrees.
fn run(
    authority: &Authority<'_>,
    framer: &Framer,
    scenario: &Scenario,
    width: usize,
) -> Option<Failure> {
    let mut state: Vec<i128> = scenario.steps[0].inputs[..width].to_vec();
    for (index, step) in scenario.steps.iter().enumerate() {
        if step.inputs[..width] != state[..] {
            return Some(Failure {
                step: index,
                line: step.line,
                reason: format!(
                    "the step's pre-state {} is not the replayed state {}",
                    examples::join(&step.inputs[..width]),
                    examples::join(&state)
                ),
                outcome: None,
            });
        }
        let outcome = evaluate::evaluate(authority, framer, &step.inputs);
        if let Err(reason) = examples::agrees(step, &outcome, width) {
            return Some(Failure {
                step: index,
                line: step.line,
                reason,
                outcome: Some(outcome),
            });
        }
        if let Outcome::Decision(decision) = &outcome
            && decision.class != Class::Reject
        {
            // The examples agree, so the successor is the written one.
            state = step.post.to_vec();
        }
    }
    None
}

fn failure_json(failure: &Failure) -> Value {
    json!({
        "step": failure.step + 1,
        "line": failure.line,
        "reason": failure.reason,
        "outcome": failure.outcome.as_ref().map(outcome_json),
    })
}

fn outcome_json(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Decision(decision) => json!({
            "class": decision.class.name(),
            "reason": decision.reason,
            "post": decision.post.iter().map(|(_, value)| value.number().map(|number| number.to_string())).collect::<Vec<_>>(),
            "deliveries": decision.outbox.len(),
        }),
        Outcome::Refused(refusal) => json!({
            "class": "Refused",
            "refusal": refusal.class.name(),
            "law": refusal.law,
            "text": refusal.text,
        }),
    }
}

/// Binds `rules` over the contract's declarations and replays every
/// scenario.
fn bind_and_run(
    loaded: &Loaded,
    rules: &Rules,
    scenarios: &[Scenario],
) -> Result<Vec<Option<Failure>>, ContractError> {
    loaded.bind(rules, |authority, framer| {
        Ok(scenarios
            .iter()
            .map(|scenario| run(authority, framer, scenario, loaded.state_width))
            .collect())
    })
}

/// Which mutants of the extended catalog some scenario tells apart.
fn mutants_killed(loaded: &Loaded, scenarios: &[Scenario]) -> Result<Value, ContractError> {
    let catalog = mutants::extended_catalog(&loaded.rules, &loaded.declarations);
    let mut killed = 0;
    let mut survived = Vec::new();
    let mut refused = 0;
    let mut list = Vec::new();
    for mutant in &catalog {
        if Contract::build(&loaded.declarations, &mutant.rules, loaded.commitment).is_err() {
            refused += 1;
            list.push(json!({"id": mutant.id, "classification": "refused-by-generator"}));
            continue;
        }
        let entry = match bind_and_run(loaded, &mutant.rules, scenarios) {
            Err(_) => {
                refused += 1;
                json!({"id": mutant.id, "classification": "refused-by-library"})
            }
            Ok(failures) => match failures.iter().position(Option::is_some) {
                Some(index) => {
                    killed += 1;
                    let failure = failures[index].as_ref().map(|failure| failure.step + 1);
                    json!({
                        "id": mutant.id,
                        "classification": "killed",
                        "scenario": scenarios[index].name,
                        "step": failure,
                    })
                }
                None => {
                    survived.push(mutant.id.clone());
                    json!({"id": mutant.id, "classification": "survived"})
                }
            },
        };
        list.push(entry);
    }
    let mut value = json!({
        "catalog": mutants::CATALOG_V2,
        "count": catalog.len(),
        "killed": killed,
        "survived": survived,
        "refused": refused,
        "list": list,
    });
    value["operators"] = json!(
        catalog
            .iter()
            .map(|mutant| (
                mutant.id.clone(),
                json!([mutant.operator, mutant.site, mutant.change])
            ))
            .collect::<serde_json::Map<_, _>>()
    );
    Ok(value)
}
