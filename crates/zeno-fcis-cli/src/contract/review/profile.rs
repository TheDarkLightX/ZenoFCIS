//! The state, memory and robustness profile of a contract, exact over its
//! declared domain.
//!
//! - States: every declared state (the product of the state fields'
//!   domains); those that satisfy every state law, as `contract review`
//!   decides it; those reachable from genesis through committed decisions
//!   (an accept or a committed failure; a reject or a refusal keeps the
//!   state); and the reachable states left after merging those no sequence
//!   of inputs tells apart by its outcomes, class and reason or refusal:
//!   Moore partition refinement on the reachable machine. The successor is
//!   internal to that comparison, so fewer minimal than reachable states
//!   means the contract stores information its decisions never show. That
//!   is a design question for a person, not a change: an application
//!   persists and exposes its successor state.
//! - Memory: bits to store a state field by field as declared, as one of
//!   the reachable states, and as one of the minimal states.
//! - Robustness: what the Authority does on pre-states that break a state
//!   law (keeps the state, or commits into a state that satisfies the state
//!   laws or into one that does not), refused tuples over the domain and on
//!   reachable states, reachable states from which nothing commits, and
//!   whether genesis is reachable again from every reachable state.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::ContractError;
use super::machine::{self, Inconclusive, Loaded, Machine, Observation};

/// What the profile found.
#[derive(Clone, Debug)]
pub(crate) enum Profile {
    Measured(Value),
    Inconclusive(Inconclusive),
}

/// Profiles a contract.
///
/// # Errors
/// A declaration or rule with no contract form, or the library's refusal.
pub(crate) fn profile(
    project: &str,
    rules: &str,
    max_tuples: u64,
) -> Result<Profile, ContractError> {
    let loaded = Loaded::read(project, rules)?;
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(observation) => observation,
        Err(inconclusive) => return Ok(Profile::Inconclusive(inconclusive)),
    };
    Ok(Profile::Measured(measure(&loaded, &observation)))
}

fn measure(loaded: &Loaded, observation: &Observation) -> Value {
    let Observation {
        machine,
        valid,
        state_laws,
    } = observation;
    let space = &machine.space;
    let (reachable, _) = machine.reachable();
    let reachable_states: Vec<usize> = (0..space.states)
        .filter(|state| reachable[*state])
        .collect();
    let classes = classes(machine, &reachable_states);
    let minimal = classes.iter().flatten().max().map_or(0, |last| last + 1);
    let mut kept = 0;
    let mut into_valid = 0;
    let mut into_invalid = 0;
    let mut refused = 0;
    let mut refused_reachable = 0;
    for state in 0..space.states {
        for input in 0..space.inputs {
            let outcome = machine.of(state, input);
            let is_refused = !matches!(outcome, machine::Observed::Decided { .. });
            refused += usize::from(is_refused);
            refused_reachable += usize::from(is_refused && reachable[state]);
            if valid[state] {
                continue;
            }
            if !outcome.commits() {
                kept += 1;
            } else if valid[outcome.after(state)] {
                into_valid += 1;
            } else {
                into_invalid += 1;
            }
        }
    }
    let stuck = reachable_states
        .iter()
        .filter(|state| (0..space.inputs).all(|input| !machine.of(**state, input).commits()))
        .count();
    let returns = returns_to_genesis(machine, &reachable, &reachable_states);
    let declared_bits: u32 = space.state_sizes().into_iter().map(machine::bits).sum();
    let valid_count = valid.iter().filter(|valid| **valid).count();
    json!({
        "domain": {
            "states": space.states,
            "inputs": space.inputs,
            "tuples": space.tuples(),
        },
        "genesis": space.state(machine.genesis),
        "state_laws": state_laws,
        "states": {
            "schema": space.states,
            "valid": valid_count,
            "reachable": reachable_states.len(),
            "minimal": minimal,
        },
        "memory_bits": {
            "declared": declared_bits,
            "reachable": machine::bits(reachable_states.len()),
            "minimal": machine::bits(minimal),
        },
        "robustness": {
            "invalid_prestates": space.states - valid_count,
            "invalid_prestate_tuples": {
                "kept": kept,
                "committed_into_valid": into_valid,
                "committed_into_invalid": into_invalid,
            },
            "refused_tuples": {"all": refused, "reachable": refused_reachable},
            "reachable_states_committing_nothing": stuck,
            "genesis_reachable_from_every_reachable_state": returns,
        },
        "positions": loaded.positions.iter().map(super::domain::Position::json).collect::<Vec<_>>(),
        "tables": {
            "rule": "SHA-256 of one line per entry, each ending in a newline. decisions: per tuple in ordinal order, `A`, `R` or `F`, the reason or `-`, and the values of the state after the decision, or `X` for a refusal. states: per declared state in index order, 1 or 0 for valid, 1 or 0 for reachable, and the minimal class, numbered from 0 in order of each class's least state, or `-` when unreachable",
            "decisions_sha256": machine::table_digest((0..space.tuples()).map(|ordinal| {
                machine::decision_line(space, machine.observed[ordinal], ordinal / space.inputs)
            })),
            "states_sha256": machine::table_digest((0..space.states).map(|state| {
                format!(
                    "{} {} {}",
                    u8::from(valid[state]),
                    u8::from(reachable[state]),
                    classes[state].map_or_else(|| "-".to_owned(), |class| class.to_string())
                )
            })),
        },
    })
}

/// The class of each reachable state among the classes no input sequence
/// tells apart by its outcomes: Moore partition refinement, starting from
/// each state's outcome signature on every input and splitting by the
/// classes its successors fall in until the count no longer grows. Classes
/// are numbered from 0 in order of their least state; an unreachable state
/// has none.
pub(super) fn classes(machine: &Machine, reachable: &[usize]) -> Vec<Option<usize>> {
    let inputs = machine.space.inputs;
    let mut block = vec![usize::MAX; machine.space.states];
    let mut ids: BTreeMap<Vec<(u8, u8, Option<u32>)>, usize> = BTreeMap::new();
    for state in reachable {
        let signature: Vec<_> = (0..inputs)
            .map(|input| machine.of(*state, input).signature())
            .collect();
        let next = ids.len();
        block[*state] = *ids.entry(signature).or_insert(next);
    }
    let mut count = ids.len();
    loop {
        let mut ids: BTreeMap<(usize, Vec<usize>), usize> = BTreeMap::new();
        let mut refined = block.clone();
        for state in reachable {
            let successors: Vec<usize> = (0..inputs)
                .map(|input| block[machine.of(*state, input).after(*state)])
                .collect();
            let next = ids.len();
            refined[*state] = *ids.entry((block[*state], successors)).or_insert(next);
        }
        block = refined;
        if ids.len() == count {
            break;
        }
        count = ids.len();
    }
    // Number classes by their least state; `reachable` is ascending.
    let mut numbers: BTreeMap<usize, usize> = BTreeMap::new();
    let mut classes = vec![None; machine.space.states];
    for state in reachable {
        let next = numbers.len();
        classes[*state] = Some(*numbers.entry(block[*state]).or_insert(next));
    }
    classes
}

/// Whether genesis is reachable from every reachable state.
fn returns_to_genesis(machine: &Machine, reachable: &[bool], states: &[usize]) -> bool {
    let mut back = vec![false; machine.space.states];
    back[machine.genesis] = true;
    let mut changed = true;
    while changed {
        changed = false;
        for state in states {
            if back[*state] {
                continue;
            }
            if (0..machine.space.inputs).any(|input| back[machine.of(*state, input).after(*state)])
            {
                back[*state] = true;
                changed = true;
            }
        }
    }
    states.iter().all(|state| back[*state] && reachable[*state])
}
