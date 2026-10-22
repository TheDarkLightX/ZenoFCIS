//! Probes: exact answers to questions about a contract's decisions, asked
//! between proposals so that a proposer need not learn the semantics by
//! being wrong.
//!
//! A condition is a `project.zeno` relation over `pre.`, `command.`,
//! `context.` and `post.` fields, compiled as a law of the contract declared
//! `on any` and run by the library's law evaluator on the transition frame
//! of the contract's own decision. A condition that reads a value the frame
//! lacks, such as a successor field of a reject, is undefined there and
//! counted apart; tuples the contract refuses have no decision and are
//! counted apart too. Every answer except `explain` enumerates the whole
//! declared domain, and a domain over the tuple limit is inconclusive.
//!
//! - `explain`: the case that decides one input, the cases before it (whose
//!   `when` the decision program found false), and what the Authority does.
//! - `reachable`: a shortest trace from genesis to a state that satisfies a
//!   state formula, or that no reachable state does.
//! - `count`: on how many tuples a condition holds.
//! - `implies`: the first tuple where one condition holds and another does
//!   not, or that there is none; `vacuous` when the first holds nowhere.
//! - `decisions`: the distinct outcomes, with counts, where a condition holds.
//! - `depends`: two inputs that differ only in one position and are decided
//!   differently, or that the decision never depends on it; `vacuous` when
//!   no two selected inputs differ only there.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::model::Contract;
use super::super::rules::LawKind;
use super::domain::LeafDomain;
use super::evaluate::{self, LawSet, Outcome, Proposed};
use super::examples;
use super::lemma;
use super::machine::{self, Inconclusive, Loaded, Machine, Observed};
use super::mutants;
use super::parallel;
use zeno_fcis_synthesis::finite::v2_laws::{Failure, Verdict};

/// A question.
#[derive(Clone, Debug)]
pub(crate) enum Question {
    Explain {
        input: Vec<i128>,
    },
    Reachable {
        state: String,
    },
    Count {
        condition: String,
    },
    Implies {
        premise: String,
        conclusion: String,
    },
    Decisions {
        condition: Option<String>,
    },
    Depends {
        position: String,
        condition: Option<String>,
    },
}

impl Question {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Explain { .. } => "explain",
            Self::Reachable { .. } => "reachable",
            Self::Count { .. } => "count",
            Self::Implies { .. } => "implies",
            Self::Decisions { .. } => "decisions",
            Self::Depends { .. } => "depends",
        }
    }
}

/// An answer, or why there is none.
#[derive(Clone, Debug)]
pub(crate) enum Answer {
    Answered(Value),
    NoStartingState(Value),
    Inconclusive(Inconclusive),
}

/// A condition's value on one tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Truth {
    /// The contract refuses the tuple; there is no decision to read.
    Refused,
    Holds,
    Fails,
    /// The condition reads a value the frame lacks.
    Undefined,
}

/// Answers one question.
///
/// # Errors
/// A malformed input or formula, a declaration or rule with no contract
/// form, or the library's refusal of the contract.
pub(crate) fn probe(
    project: &str,
    rules: &str,
    question: &Question,
    max_tuples: u64,
) -> Result<Answer, ContractError> {
    if let Question::Count { condition } = question
        && super::super::rules::Rules::read_for_project(project, rules)?.unsigned
    {
        return super::unsigned::count_probe(project, rules, condition, max_tuples);
    }
    let loaded = Loaded::read(project, rules)?;
    if let Question::Explain { input } = question {
        return explain(&loaded, input).map(Answer::Answered);
    }
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(observation) => observation,
        Err(inconclusive) => return Ok(Answer::Inconclusive(inconclusive)),
    };
    let machine = &observation.machine;
    if matches!(question, Question::Reachable { .. })
        && let Err(refusal) = &machine.starting_state
    {
        return Ok(Answer::NoStartingState(refusal.packet()));
    }
    let space = &machine.space;
    let condition =
        |what: &str, formula: &str| conditions(project, &loaded, machine, what, formula);
    let answer = match question {
        Question::Explain { .. } => unreachable!("answered above"),
        Question::Reachable { state } => {
            let holds = lemma::state_holds(project, &loaded, machine, "--state", state)?;
            let (reachable, parent) = machine.reachable();
            let reachable_count = reachable.iter().filter(|seen| **seen).count();
            match (0..space.states).find(|state| holds[*state] && reachable[*state]) {
                None => json!({
                    "answer": "no",
                    "proof": format!("exhaustive over the {reachable_count} reachable states"),
                }),
                Some(found) => json!({
                    "answer": "yes",
                    "state": crate::report_numbers::integers(&space.state(found)),
                    "trace_from_genesis": Machine::trace(&parent, found)
                        .into_iter()
                        .map(|input| crate::report_numbers::integers(&space.input(input)))
                        .collect::<Vec<_>>(),
                }),
            }
        }
        Question::Count { condition: formula } => {
            let truths = condition("--where", formula)?;
            let count = |wanted: Truth| truths.iter().filter(|truth| **truth == wanted).count();
            json!({
                "answer": count(Truth::Holds),
                "fails": count(Truth::Fails),
                "undefined": count(Truth::Undefined),
                "refused": count(Truth::Refused),
                "of": space.tuples(),
            })
        }
        Question::Implies {
            premise,
            conclusion,
        } => {
            let premises = condition("--if", premise)?;
            let conclusions = condition("--then", conclusion)?;
            let broken: Vec<usize> = (0..space.tuples())
                .filter(|ordinal| {
                    premises[*ordinal] == Truth::Holds && conclusions[*ordinal] != Truth::Holds
                })
                .collect();
            let holding = premises
                .iter()
                .filter(|truth| **truth == Truth::Holds)
                .count();
            match broken.first() {
                None if holding == 0 => json!({
                    "answer": "vacuous",
                    "proof": "the first condition holds on no tuple the contract decides",
                }),
                None => json!({
                    "answer": "yes",
                    "proof": format!(
                        "exhaustive over the {holding} decided tuples where the first condition holds"
                    ),
                }),
                Some(ordinal) => json!({
                    "answer": "no",
                    "count": broken.len(),
                    "input": crate::report_numbers::integers(&space.at(*ordinal)),
                    "contract": machine.observed[*ordinal].json(space),
                    "conclusion": truth_name(conclusions[*ordinal]),
                }),
            }
        }
        Question::Decisions { condition: formula } => {
            let selected = select(formula.as_deref(), space.tuples(), |formula| {
                condition("--where", formula)
            })?;
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut inputs = 0;
            for ordinal in (0..space.tuples()).filter(|ordinal| selected[*ordinal]) {
                inputs += 1;
                *counts
                    .entry(outcome_name(machine.observed[ordinal]))
                    .or_default() += 1;
            }
            json!({"answer": counts, "distinct": counts.len(), "inputs": inputs})
        }
        Question::Depends {
            position,
            condition: formula,
        } => {
            let selected = select(formula.as_deref(), space.tuples(), |formula| {
                condition("--where", formula)
            })?;
            depends(&loaded, machine, position, &selected)?
        }
    };
    Ok(Answer::Answered(answer))
}

/// Which tuples a condition selects: every tuple without one, else those
/// where it holds.
fn select(
    formula: Option<&str>,
    tuples: usize,
    truths: impl Fn(&str) -> Result<Vec<Truth>, ContractError>,
) -> Result<Vec<bool>, ContractError> {
    Ok(match formula {
        None => vec![true; tuples],
        Some(formula) => truths(formula)?
            .into_iter()
            .map(|truth| truth == Truth::Holds)
            .collect(),
    })
}

fn truth_name(truth: Truth) -> &'static str {
    match truth {
        Truth::Refused => "refused",
        Truth::Holds => "holds",
        Truth::Fails => "fails",
        Truth::Undefined => "undefined",
    }
}

/// An outcome as a counting key: `Accept`, `Reject 201`, `Refused law 500`.
fn outcome_name(outcome: Observed) -> String {
    match outcome {
        Observed::Decided { class, reason, .. } => match reason {
            Some(reason) => format!("{} {reason}", class.name()),
            None => class.name().to_owned(),
        },
        Observed::Refused { class, law } => match law {
            Some(law) => format!("Refused {} {law}", class.name()),
            None => format!("Refused {}", class.name()),
        },
    }
}

/// A condition on every tuple, read on the contract's own decision.
pub(super) fn conditions(
    project: &str,
    loaded: &Loaded,
    machine: &Machine,
    what: &str,
    formula: &str,
) -> Result<Vec<Truth>, ContractError> {
    if formula.contains("outbox.") || formula.contains("verdict.") || formula.contains("delivery.")
    {
        let expression = super::observations::Expression::compile(
            formula,
            &super::observations::names(loaded),
            true,
        )?;
        return Ok(parallel(machine.space.tuples(), |ordinal| {
            if matches!(machine.observed[ordinal], Observed::Refused { .. }) {
                return Truth::Refused;
            }
            match super::observations::values(loaded, machine, ordinal)
                .and_then(|v| expression.evaluate(&v))
            {
                Some(1) => Truth::Holds,
                Some(0) => Truth::Fails,
                _ => Truth::Undefined,
            }
        }));
    }
    let (extended, id) = machine::with_law(
        project,
        &loaded.rules,
        what,
        "any",
        formula,
        LawKind::AuthoritySubjectRecipient,
    )?;
    let space = &machine.space;
    extended.bind(&extended.rules, |authority, framer| {
        let laws = LawSet::new(authority.descriptor(), &[id]);
        let states: Vec<Vec<i128>> = (0..space.states).map(|state| space.state(state)).collect();
        Ok(parallel(space.tuples(), |ordinal| {
            let Observed::Decided {
                class,
                reason,
                next,
            } = machine.observed[ordinal]
            else {
                return Truth::Refused;
            };
            let (_, diagnostics) = laws.on_transition(
                framer,
                &space.at(ordinal),
                &Proposed {
                    class,
                    reason,
                    post: &states[next],
                },
            );
            match diagnostics.first().map(|diagnostic| diagnostic.verdict) {
                Some(Verdict::Satisfied) => Truth::Holds,
                Some(Verdict::Refused(Failure::Violated)) => Truth::Fails,
                _ => Truth::Undefined,
            }
        }))
    })
}

/// Two inputs that differ only in `position` and are decided differently.
fn depends(
    loaded: &Loaded,
    machine: &Machine,
    position: &str,
    selected: &[bool],
) -> Result<Value, ContractError> {
    let index = loaded
        .positions
        .iter()
        .position(|candidate| candidate.name == position)
        .ok_or_else(|| {
            ContractError::new(
                "--position",
                format!(
                    "`{position}` is not an input position; positions are {}",
                    loaded
                        .positions
                        .iter()
                        .map(|position| position.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
    let space = &machine.space;
    let sizes = space.sizes();
    let stride: usize = sizes[index + 1..].iter().product();
    let size = sizes[index];
    // A decision as a person compares two: its class and reason, the
    // successor when it commits, and its deliveries; or the refusal. A
    // reject keeps whatever pre-state it had, so its state is no difference.
    let outcome = |ordinal: usize| {
        let observed = machine.observed[ordinal];
        let committed = observed.commits().then(|| observed.after(0));
        (observed.signature(), committed, machine.outboxes[ordinal])
    };
    // Pairs of selected inputs that differ only in the position.
    let mut pairs = 0usize;
    for base in (0..space.tuples()).filter(|ordinal| (ordinal / stride).is_multiple_of(size)) {
        let members: Vec<usize> = (0..size)
            .map(|step| base + step * stride)
            .filter(|ordinal| selected[*ordinal])
            .collect();
        pairs += members.len() * members.len().saturating_sub(1) / 2;
        let Some(first) = members.first() else {
            continue;
        };
        if let Some(other) = members
            .iter()
            .find(|ordinal| outcome(**ordinal) != outcome(*first))
        {
            let witness: Vec<Value> = [*first, *other]
                .iter()
                .map(|ordinal| {
                    json!({
                        "input": crate::report_numbers::integers(&space.at(*ordinal)),
                        "contract": machine.observed[*ordinal].json(space),
                    })
                })
                .collect();
            return Ok(json!({"answer": "yes", "witness": witness}));
        }
    }
    if pairs == 0 {
        return Ok(json!({
            "answer": "vacuous",
            "proof": "no two selected inputs differ only in this position",
        }));
    }
    Ok(json!({
        "answer": "no",
        "proof": format!("exhaustive over {pairs} pairs of inputs that differ only in this position"),
    }))
}

/// The case that decides `input` and what the Authority does with it.
fn explain(loaded: &Loaded, input: &[i128]) -> Result<Value, ContractError> {
    if input.len() != loaded.positions.len() {
        return Err(ContractError::new(
            "--input",
            format!(
                "needs {} numbers, one per position: {}",
                loaded.positions.len(),
                loaded
                    .positions
                    .iter()
                    .map(|position| position.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        ));
    }
    for (position, value) in loaded.positions.iter().zip(input) {
        let inside = match &position.domain {
            LeafDomain::Bool => matches!(value, 0 | 1),
            LeafDomain::Int { min, max } => (min..=max).contains(&value),
            LeafDomain::Sum { variants, .. } => variants
                .iter()
                .any(|variant| i128::from(*variant) == *value),
        };
        if !inside {
            return Err(ContractError::new(
                "--input",
                format!("{value} is outside the domain of {}", position.name),
            ));
        }
    }
    let contract = Contract::build(&loaded.declarations, &loaded.rules, loaded.commitment)?;
    let selected = contract
        .program()
        .ok()
        .and_then(|program| program.evaluate(input).ok())
        .and_then(|outputs| usize::try_from(*outputs.first()?).ok());
    let outcome = loaded.bind(&loaded.rules, |authority, framer| {
        Ok(evaluate::evaluate(authority, framer, input))
    })?;
    let cases: Vec<Value> = loaded
        .rules
        .cases
        .iter()
        .enumerate()
        .take(selected.map_or(0, |index| index + 1))
        .map(|(index, case)| {
            json!({
                "case": index,
                "when": mutants::render(&case.when),
                "class": case.class.name(),
                "reason": case.reason,
                "holds": Some(index) == selected,
            })
        })
        .collect();
    let authority = match &outcome {
        Outcome::Decision(decision) => json!({
            "class": decision.class.name(),
            "reason": decision.reason,
            "example": examples::render(input, decision, loaded.state_width),
        }),
        Outcome::Refused(refusal) => json!({
            "class": "Refused",
            "refusal": refusal.class.name(),
            "law": refusal.law,
            "text": refusal.text,
        }),
    };
    Ok(json!({
        "input": loaded
            .positions
            .iter()
            .zip(input)
            .map(|(position, value)| (position.name.clone(), crate::report_numbers::integer(*value)))
            .collect::<serde_json::Map<_, _>>(),
        "case": selected,
        "cases_tried": cases,
        "authority": authority,
    }))
}
