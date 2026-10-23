//! The lemma judge: state induction, or an explicit before/after relation
//! checked on actual admitted committed decisions of the finite machine.
//! Transition results are labelled `checked`, not `inductive` or proved.
//!
//! In the default state mode a lemma is a `project.zeno` relation over
//! `post.` state fields, and compiled as a law of the contract declared `on
//! commit, genesis`; the library's law evaluator runs its program on each
//! declared state as a genesis frame. Each question an inductive proof asks
//! is answered exactly over the declared domain:
//!
//! - initiation: does genesis satisfy the lemma?
//! - consecution: from every state that satisfies it, does every committed
//!   decision (an accept or a committed failure; a reject or a refusal keeps
//!   the state) lead to a state that satisfies it? If not, the first
//!   counterexample to induction: the state, the input and the successor,
//!   with how many there are.
//! - truth: when a state reachable from genesis lies outside the lemma, the
//!   lemma is false, and a shortest trace from genesis to the nearest such
//!   state is given, its last step a counterexample to induction from a
//!   reachable state. When every reachable state satisfies the lemma and a
//!   counterexample to induction remains, the lemma holds but is too weak to
//!   prove by induction: strengthen it to exclude that unreachable state.
//! - strength, for an inductive lemma: how many declared states it admits
//!   against the reachable and the valid states, whether it admits exactly
//!   the reachable states, and whether it implies the state laws.
//!
//! An inductive lemma with initiation holds on every reachable state of the
//! declared domain, and only there is it claimed.

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::rules::LawKind;
use super::evaluate::LawSet;
use super::machine::{self, Inconclusive, Loaded, Machine, Observation};
use super::probe::{self, Truth};

/// Shared initiation and reachable consecution check. Breadth-first parents
/// make the first failed obligation a shortest run, including a zero-step
/// genesis failure. The caller supplies bounded, owned product states.
pub(super) fn induction<S: Clone + Ord>(
    genesis: S,
    initial_holds: bool,
    inputs: usize,
    mut step: impl FnMut(&S, usize) -> Result<(S, bool), ContractError>,
) -> Result<(usize, Option<Vec<usize>>), ContractError> {
    if !initial_holds {
        return Ok((1, Some(Vec::new())));
    }
    let mut states = vec![genesis.clone()];
    let mut indices = std::collections::BTreeMap::from([(genesis, 0)]);
    let mut parents = vec![None];
    let mut at = 0;
    while at < states.len() {
        for input in 0..inputs {
            let (next, holds) = step(&states[at], input)?;
            if !holds {
                let mut trace = Machine::trace(&parents, at);
                trace.push(input);
                return Ok((states.len(), Some(trace)));
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = indices.entry(next.clone()) {
                entry.insert(states.len());
                states.push(next);
                parents.push(Some((at, input)));
            }
        }
        at += 1;
    }
    Ok((states.len(), None))
}

/// Which declared pre-states a lemma obligation covers. The finite and
/// symbolic transition judges must use the same scope, including repairs.
#[derive(Clone, Copy)]
pub(super) enum PreStates {
    Declared,
    Lawful,
}

impl PreStates {
    pub(super) const TRANSITION: Self = Self::Declared;

    /// Select either the whole declared domain or its lawful subset. The
    /// predicate can be a native Boolean or a symbolic Boolean term.
    pub(super) fn predicate<T>(self, declared: T, lawful: impl FnOnce() -> T) -> T {
        match self {
            Self::Declared => declared,
            Self::Lawful => lawful(),
        }
    }
}

/// The lemma judge's result.
#[derive(Clone, Debug)]
pub(crate) enum Lemmas {
    /// Each lemma's verdict, in the order given.
    Judged(Vec<Value>),
    NoStartingState(Value),
    Inconclusive(Inconclusive),
}

use super::lemma_kernel as kernel;

pub(crate) use kernel::Verdict;

impl Verdict {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Inductive => "inductive",
            Self::NotInductive => "not-inductive",
            Self::False => "false",
        }
    }
}

/// Judges each lemma.
///
/// # Errors
/// A declaration, rule or lemma with no contract form, or the library's
/// refusal of the contract.
pub(crate) fn judge(
    project: &str,
    rules: &str,
    lemmas: &[String],
    max_tuples: u64,
) -> Result<Lemmas, ContractError> {
    if zeno_fcis_contract::collections::judges::has_maps(project)? {
        return map_lemmas(project, rules, lemmas, max_tuples, false);
    }
    if super::super::rules::Rules::read_for_project(project, rules)?.unsigned {
        return super::unsigned::lemmas(project, rules, lemmas, max_tuples, false);
    }
    let loaded = Loaded::read(project, rules)?;
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(observation) => observation,
        Err(inconclusive) => return Ok(Lemmas::Inconclusive(inconclusive)),
    };
    if let Err(refusal) = &observation.machine.starting_state {
        return Ok(Lemmas::NoStartingState(refusal.packet()));
    }
    let mut judged = Vec::new();
    for (index, formula) in lemmas.iter().enumerate() {
        let what = format!("lemma {}", index + 1);
        let holds = state_holds(project, &loaded, &observation.machine, &what, formula)?;
        judged.push(verdict(&observation, &holds, formula));
    }
    Ok(Lemmas::Judged(judged))
}

/// Checks before/after relations on every admitted committed decision.
/// This is a finite relation check, not state induction or a reachability claim.
/// Rejects preserve state but have no post-law view; refusals have no admitted
/// decision. Both are counted outside this explicit transition scope. An
/// undefined relation or an empty admitted scope cannot receive `checked`.
pub(crate) fn judge_transitions(
    project: &str,
    rules: &str,
    lemmas: &[String],
    max_tuples: u64,
) -> Result<Lemmas, ContractError> {
    if zeno_fcis_contract::collections::judges::has_maps(project)? {
        return map_lemmas(project, rules, lemmas, max_tuples, true);
    }
    if lemmas.is_empty() {
        return Err(ContractError::new(
            "transition lemmas",
            "requires at least one relation",
        ));
    }
    if super::super::rules::Rules::read_for_project(project, rules)?.unsigned {
        return super::unsigned::lemmas(project, rules, lemmas, max_tuples, true);
    }
    let loaded = Loaded::read(project, rules)?;
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(observation) => observation,
        Err(inconclusive) => return Ok(Lemmas::Inconclusive(inconclusive)),
    };
    let machine = &observation.machine;
    let space = &machine.space;
    let mut judged = Vec::new();
    for (index, formula) in lemmas.iter().enumerate() {
        // The probe's shared law path reads actual Authority decisions. The
        // proposed relation is never installed as an admission law of that
        // Authority, which would hide the very counterexamples being sought.
        let truths = probe::conditions(
            project,
            &loaded,
            machine,
            &format!("transition lemma {}", index + 1),
            formula,
        )?;
        let in_scope = |ordinal: usize| {
            PreStates::TRANSITION.predicate(true, || observation.valid[ordinal / space.inputs])
                && machine.observed[ordinal].commits()
        };
        let count = |truth| {
            truths
                .iter()
                .enumerate()
                .filter(|(ordinal, value)| in_scope(*ordinal) && **value == truth)
                .count()
        };
        let failed = count(Truth::Fails);
        let undefined = count(Truth::Undefined);
        let refused = machine
            .observed
            .iter()
            .filter(|outcome| matches!(outcome, machine::Observed::Refused { .. }))
            .count();
        let admitted = machine
            .observed
            .iter()
            .enumerate()
            .filter(|(ordinal, _)| in_scope(*ordinal))
            .count();
        let verdict = if failed != 0 {
            "false"
        } else if undefined != 0 {
            "undefined"
        } else if admitted == 0 {
            "vacuous"
        } else {
            "checked"
        };
        let mut result = json!({
            "mode": "transition-relation", "formula": formula, "verdict": verdict,
            "scope": "all admitted committed decisions in the declared finite domain",
            "decided_by": if formula.contains("outbox.") || formula.contains("delivery.") || formula.contains("verdict.") { "library-authority-and-scalar-interpreter" } else { "library-authority-and-law-evaluator" },
            "tuples": space.tuples(), "admitted": admitted, "checked": count(Truth::Holds),
            "failed": failed, "undefined": undefined, "refused": refused,
            "rejected_without_state_change": space.tuples() - refused - admitted,
            "truths_sha256": machine::table_digest(truths.iter().map(|truth| format!("{truth:?}"))),
        });
        let witness = if failed != 0 {
            Truth::Fails
        } else {
            Truth::Undefined
        };
        if let Some(ordinal) = truths
            .iter()
            .enumerate()
            .position(|(ordinal, truth)| in_scope(ordinal) && *truth == witness)
        {
            result["counterexample"] = json!({
                "ordinal": ordinal,
                "state": crate::report_numbers::integers(&space.state(ordinal / space.inputs)),
                "input": crate::report_numbers::integers(&space.input(ordinal % space.inputs)),
                "outcome": machine.observed[ordinal].json(space),
                "relation": if truths[ordinal] == Truth::Fails { "fails" } else { "undefined" },
            });
        }
        judged.push(result);
    }
    Ok(Lemmas::Judged(judged))
}

fn map_lemmas(
    project: &str,
    rules: &str,
    formulas: &[String],
    max_tuples: u64,
    transition: bool,
) -> Result<Lemmas, ContractError> {
    use zeno_fcis_contract::collections::{Limits, Verdict, judges};
    if formulas.is_empty() {
        return Err(ContractError::new(
            "map lemma",
            "requires at least one formula",
        ));
    }
    let mut results = Vec::new();
    for formula in formulas {
        let result = judges::lemma(project, rules, formula, transition, Limits { max_tuples })?;
        if result.verdict() == Verdict::Unknown && result.packet()["verdict"].is_null() {
            return Ok(Lemmas::Inconclusive(Inconclusive {
                reason: "collection view domain exceeds the tuple limit",
                size: result.packet()["view_domain"]
                    .as_str()
                    .and_then(|s| s.parse().ok()),
                limit: max_tuples,
            }));
        }
        results.push(result.packet().clone());
    }
    Ok(Lemmas::Judged(results))
}

#[cfg(test)]
#[path = "transition_lemma_tests.rs"]
mod transition_tests;

/// Whether each declared state satisfies a state formula: its law program,
/// compiled as a law of the contract declared `on commit, genesis`, run by
/// the library's law evaluator on the state as a genesis frame.
///
/// # Errors
/// A formula with no contract form, named as `what`.
pub(super) fn state_holds(
    project: &str,
    loaded: &Loaded,
    machine: &Machine,
    what: &str,
    formula: &str,
) -> Result<Vec<bool>, ContractError> {
    let (extended, id) = machine::with_law(
        project,
        &loaded.rules,
        what,
        "commit, genesis",
        formula,
        LawKind::StateInvariant,
    )?;
    extended.bind(&extended.rules, |authority, framer| {
        let laws = LawSet::new(authority.descriptor(), &[id]);
        Ok(machine::valid_states(&machine.space, |tuple| {
            laws.on_state(framer, tuple)
        }))
    })
}

/// The verdict of one lemma.
fn verdict(observation: &Observation, holds: &[bool], formula: &str) -> Value {
    let machine = &observation.machine;
    let space = &machine.space;
    let (after, commits) = machine.kernel_tables();
    let decision = kernel::decide(space.inputs, machine.genesis, &after, &commits, holds)
        .unwrap_or_else(|| unreachable!("observed machine tables are bounded and complete"));
    let reachable = decision.search.seen;
    let admitted = holds.iter().filter(|holds| **holds).count();
    let reachable_count = reachable.iter().filter(|seen| **seen).count();
    let mut out = json!({
        "formula": formula,
        "holds_sha256": machine::table_digest(
            holds.iter().map(|holds| u8::from(*holds).to_string())
        ),
        "initiation": decision.initiation,
        "states": {
            "declared": space.states,
            "admitted": admitted,
            "reachable": reachable_count,
        },
    });
    let count = decision.count;
    let first = decision
        .counterexample
        .map(|(state, input, _)| (state, input));
    let violation = if decision.verdict == Verdict::False {
        decision.target.map(|state| (decision.trace, state))
    } else {
        None
    };
    let verdict = if !decision.initiation {
        out["reason"] = json!("genesis does not satisfy the lemma");
        out["genesis"] = crate::report_numbers::integers(&space.state(machine.genesis));
        Verdict::False
    } else if let Some((trace, state)) = violation {
        let last = *trace.last().unwrap_or(&0);
        let before = first.map_or(machine.genesis, |(before, _)| before);
        out["reason"] = json!("a reachable state lies outside the lemma");
        out["counterexample_to_induction"] = json!({
            "state": crate::report_numbers::integers(&space.state(before)),
            "input": crate::report_numbers::integers(&space.input(last)),
            "successor": crate::report_numbers::integers(&space.state(state)),
            "count": count,
        });
        out["trace_from_genesis"] = json!(
            trace
                .into_iter()
                .map(|input| crate::report_numbers::integers(&space.input(input)))
                .collect::<Vec<_>>()
        );
        Verdict::False
    } else if let Some((state, input)) = first {
        out["reason"] = json!(
            "every reachable state satisfies the lemma, but an unreachable state it admits steps outside it: strengthen the lemma to exclude that state"
        );
        out["counterexample_to_induction"] = json!({
            "state": crate::report_numbers::integers(&space.state(state)),
            "input": crate::report_numbers::integers(&space.input(input)),
            "successor": crate::report_numbers::integers(&space.state(machine.of(state, input).after(state))),
            "count": count,
        });
        Verdict::NotInductive
    } else {
        let unreachable_admitted = (0..space.states)
            .filter(|state| holds[*state] && !reachable[*state])
            .count();
        let valid = &observation.valid;
        out["exact"] = json!(unreachable_admitted == 0 && admitted == reachable_count);
        out["unreachable_admitted"] = json!(unreachable_admitted);
        out["states"]["valid"] = json!(valid.iter().filter(|valid| **valid).count());
        out["implies_state_laws"] =
            json!((0..space.states).all(|state| !holds[state] || valid[state]));
        Verdict::Inductive
    };
    out["verdict"] = json!(verdict.name());
    out
}
