//! The lemma judge: a candidate state invariant, judged by induction over
//! the declared domain rather than by exploring runs.
//!
//! A lemma is written as a state law is, a `project.zeno` relation over
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

/// The lemma judge's result.
#[derive(Clone, Debug)]
pub(crate) enum Lemmas {
    /// Each lemma's verdict, in the order given.
    Judged(Vec<Value>),
    Inconclusive(Inconclusive),
}

/// What a verdict says.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    Inductive,
    NotInductive,
    False,
}

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
    let loaded = Loaded::read(project, rules)?;
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(observation) => observation,
        Err(inconclusive) => return Ok(Lemmas::Inconclusive(inconclusive)),
    };
    let mut judged = Vec::new();
    for (index, formula) in lemmas.iter().enumerate() {
        let what = format!("lemma {}", index + 1);
        let holds = state_holds(project, &loaded, &observation.machine, &what, formula)?;
        judged.push(verdict(&observation, &holds, formula));
    }
    Ok(Lemmas::Judged(judged))
}

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
    let (reachable, parent) = machine.reachable();
    let admitted = holds.iter().filter(|holds| **holds).count();
    let reachable_count = reachable.iter().filter(|seen| **seen).count();
    let mut out = json!({
        "formula": formula,
        "holds_sha256": machine::table_digest(
            holds.iter().map(|holds| u8::from(*holds).to_string())
        ),
        "initiation": holds[machine.genesis],
        "states": {
            "declared": space.states,
            "admitted": admitted,
            "reachable": reachable_count,
        },
    });
    let mut first = None;
    let mut count = 0usize;
    for state in (0..space.states).filter(|state| holds[*state]) {
        for input in 0..space.inputs {
            let outcome = machine.of(state, input);
            if outcome.commits() && !holds[outcome.after(state)] {
                count += 1;
                first.get_or_insert((state, input));
            }
        }
    }
    // The nearest reachable state outside the lemma, nearest first, then
    // least: its shortest trace's last step leaves a reachable state the
    // lemma admits, since a nearer violation would have been chosen.
    let violation = (0..space.states)
        .filter(|state| reachable[*state] && !holds[*state])
        .map(|state| (Machine::trace(&parent, state), state))
        .min_by_key(|(trace, state)| (trace.len(), *state));
    let verdict = if !holds[machine.genesis] {
        out["reason"] = json!("genesis does not satisfy the lemma");
        out["genesis"] = json!(space.state(machine.genesis));
        Verdict::False
    } else if let Some((trace, state)) = violation {
        let last = *trace.last().unwrap_or(&0);
        let before = parent[state].map_or(machine.genesis, |(before, _)| before);
        out["reason"] = json!("a reachable state lies outside the lemma");
        out["counterexample_to_induction"] = json!({
            "state": space.state(before),
            "input": space.input(last),
            "successor": space.state(state),
            "count": count,
        });
        out["trace_from_genesis"] = json!(
            trace
                .into_iter()
                .map(|input| json!(space.input(input)))
                .collect::<Vec<_>>()
        );
        Verdict::False
    } else if let Some((state, input)) = first {
        out["reason"] = json!(
            "every reachable state satisfies the lemma, but an unreachable state it admits steps outside it: strengthen the lemma to exclude that state"
        );
        out["counterexample_to_induction"] = json!({
            "state": space.state(state),
            "input": space.input(input),
            "successor": space.state(machine.of(state, input).after(state)),
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
