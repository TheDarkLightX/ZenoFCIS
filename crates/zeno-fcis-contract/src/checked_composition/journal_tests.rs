//! Deterministic journals of the generated aggregate through its own bound
//! Authority, from genesis, touching every leaf. Each publication replays
//! exactly, and each decision is the selected leaf's own Authority's
//! decision on the leaf's input read from the same pre-state.

use std::collections::BTreeMap;
use std::time::Instant;

use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition as c};

use super::aggregate::{AggregateSources, Layout, View, state_field};
use super::assembly::CompiledAggregate;
use super::leaf::{self, Compiled};
use super::manifest::Manifest;
use super::structure;
use super::tests::{Ring, fail};
use super::{CompositionSources, aggregate, check};
use crate::internals::declarations::Source;
use crate::internals::model::Contract;
use crate::internals::policy;
use crate::internals::review::domain::{self, Position};
use crate::internals::review::evaluate::Framer;
use crate::internals::rules::Constant;

/// One step: the values of command and context fields by ID; the rest take
/// their domain's least value.
#[derive(Clone, Debug)]
pub(super) struct Step {
    pub(super) command: BTreeMap<u16, i128>,
    pub(super) context: BTreeMap<u16, i128>,
}

/// One publication: its kind, its subject and its successor's bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Entry {
    pub(super) kind: &'static str,
    pub(super) subject: Vec<u8>,
    pub(super) poststate: Vec<u8>,
}

/// A journal: the genesis subject, each publication and each state.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Journal {
    pub(super) genesis: Vec<u8>,
    pub(super) entries: Vec<Entry>,
    /// The state fields after genesis and after each step, by ID.
    pub(super) states: Vec<BTreeMap<u16, i128>>,
}

fn number(atom: &c::Atom<'_>) -> i128 {
    match atom {
        c::Atom::Bool(value) => i128::from(*value),
        c::Atom::I128(value) => i128::try_from(*value).unwrap_or_else(fail),
        c::Atom::Sum { variant, .. } => i128::from(*variant),
        other => fail(format!("unexpected atom {other:?}")),
    }
}

fn least(position: &Position) -> i128 {
    position
        .domain
        .least()
        .unwrap_or_else(|| fail("an empty domain"))
}

/// The input tuple of a state and a step.
fn tuple(positions: &[Position], state: &BTreeMap<u16, i128>, step: &Step) -> Vec<i128> {
    positions
        .iter()
        .map(|position| {
            let field = position.field.unwrap_or_else(|| fail("a root input"));
            match position.source {
                Source::State => state[&field],
                Source::Command => step
                    .command
                    .get(&field)
                    .copied()
                    .unwrap_or_else(|| least(position)),
                Source::Context => step
                    .context
                    .get(&field)
                    .copied()
                    .unwrap_or_else(|| least(position)),
            }
        })
        .collect()
}

/// Publishes the aggregate's genesis and then each step, as a store does:
/// each commit's successor is the next pre-state. Every publication must
/// replay to the same subject.
pub(super) fn journal(files: &AggregateSources, steps: &[Step]) -> Journal {
    let built = CompiledAggregate::read(files).unwrap_or_else(fail);
    let contract = built.contract().unwrap_or_else(fail);
    let positions = domain::positions(&built.declarations).unwrap_or_else(fail);
    let framer = Framer::new(&positions, &contract);
    let mut state: BTreeMap<u16, i128> = positions
        .iter()
        .filter(|position| position.source == Source::State)
        .map(|position| {
            let field = position.field.unwrap_or_else(|| fail("a root state"));
            let value = match built.rules.genesis.get(&field) {
                Some(Constant::Bool(value)) => i128::from(*value),
                Some(Constant::Int(value)) => i128::try_from(*value).unwrap_or_else(fail),
                None => least(position),
            };
            (field, value)
        })
        .collect();
    policy::with_authority(&contract, &built.schema, |authority| {
        let empty = Step {
            command: BTreeMap::new(),
            context: BTreeMap::new(),
        };
        let frames = framer.frame(&tuple(&positions, &state, &empty));
        let genesis = match authority.publish_genesis(&frames[0]) {
            PublicationOutcome::Commit(publication) => publication.subject().to_vec(),
            other => fail(format!("genesis did not commit: {other:?}")),
        };
        let mut entries = Vec::with_capacity(steps.len());
        let mut states = vec![state.clone()];
        for step in steps {
            let frames = framer.frame(&tuple(&positions, &state, step));
            let raw = c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            };
            let outcome = authority.publish(raw);
            let entry = match &outcome {
                PublicationOutcome::Commit(publication) => {
                    let candidate = outcome.evaluation().result().unwrap_or_else(fail);
                    for field in candidate.post() {
                        state.insert(field.id, number(&field.value));
                    }
                    Entry {
                        kind: "commit",
                        subject: publication.subject().to_vec(),
                        poststate: publication.poststate().to_vec(),
                    }
                }
                PublicationOutcome::Reject(evaluation) => Entry {
                    kind: "reject",
                    subject: evaluation.subject().unwrap_or_else(fail).to_vec(),
                    poststate: Vec::new(),
                },
                PublicationOutcome::Refused { .. } => Entry {
                    kind: "refused",
                    subject: Vec::new(),
                    poststate: Vec::new(),
                },
                other => fail(format!("unknown outcome {other:?}")),
            };
            if entry.kind != "refused" {
                let replayed = match authority.replay_publication(raw, &entry.subject) {
                    PublicationOutcome::Commit(publication) => publication.subject().to_vec(),
                    PublicationOutcome::Reject(evaluation) => {
                        evaluation.subject().unwrap_or_else(fail).to_vec()
                    }
                    other => fail(format!("a publication did not replay: {other:?}")),
                };
                assert_eq!(replayed, entry.subject, "exact replay");
            }
            entries.push(entry);
            states.push(state.clone());
        }
        Journal {
            genesis,
            entries,
            states,
        }
    })
    .unwrap_or_else(fail)
}

/// The selected leaf's own Authority on the leaf's input read from the
/// aggregate's pre-state and step: its decision kind and, on a commit, its
/// successor's fields.
fn leaf_decision(
    leaf: &Compiled,
    layout: &Layout,
    position: usize,
    pre: &BTreeMap<u16, i128>,
    step: &Step,
) -> (&'static str, BTreeMap<u16, i128>) {
    let contract =
        Contract::build(&leaf.declarations, &leaf.rules, leaf.commitment).unwrap_or_else(fail);
    let framer = Framer::new(&leaf.positions, &contract);
    let input: Vec<i128> = leaf
        .positions
        .iter()
        .map(|at| {
            let field = at.field.unwrap_or_else(|| fail("a root input"));
            match at.source {
                Source::State => pre[&state_field(layout.owned[position][&field])],
                Source::Command => step
                    .command
                    .get(&field)
                    .copied()
                    .unwrap_or_else(|| least(at)),
                Source::Context => match layout.views[position][&field] {
                    View::Slot(slot) => pre[&state_field(slot)],
                    View::Environment(id) => {
                        step.context.get(&id).copied().unwrap_or_else(|| least(at))
                    }
                },
            }
        })
        .collect();
    policy::with_authority(&contract, &leaf.schema, |authority| {
        let frames = framer.frame(&input);
        let outcome = authority.publish(c::Raw {
            state: &frames[0],
            command: &frames[1],
            context: &frames[2],
        });
        match &outcome {
            PublicationOutcome::Commit(_) => {
                let candidate = outcome.evaluation().result().unwrap_or_else(fail);
                (
                    "commit",
                    candidate
                        .post()
                        .iter()
                        .map(|field| (field.id, number(&field.value)))
                        .collect(),
                )
            }
            PublicationOutcome::Reject(_) => ("reject", BTreeMap::new()),
            _ => ("refused", BTreeMap::new()),
        }
    })
    .unwrap_or_else(fail)
}

/// Each leaf in turn, three rounds, cycling through the actions: Set 0..3
/// and CopyNeighbor.
fn ring_steps(n: usize, target: u16) -> Vec<(usize, Step)> {
    let mut steps = Vec::new();
    for round in 0..3 {
        for route in 0..n {
            let action = i128::try_from((route + round) % 5).unwrap_or_else(fail);
            steps.push((
                route,
                Step {
                    command: BTreeMap::from([
                        (target, i128::try_from(route).unwrap_or_else(fail)),
                        (201, action),
                    ]),
                    context: BTreeMap::new(),
                },
            ));
        }
    }
    steps
}

#[test]
fn ring_journals_decide_as_the_selected_leaves() {
    for n in [4, 8, 16, 32] {
        let ring = Ring::new(n);
        let manifest_text = ring.manifest_text();
        let sources = CompositionSources {
            manifest: &manifest_text,
            leaves: &ring.leaves,
            aggregate: None,
        };
        let files = aggregate(sources).unwrap_or_else(fail);
        check(CompositionSources {
            aggregate: Some(&files),
            ..sources
        })
        .unwrap_or_else(fail);
        let manifest = Manifest::read(&manifest_text).unwrap_or_else(fail);
        let compiled: Vec<Compiled> = manifest
            .leaves
            .iter()
            .zip(&ring.leaves)
            .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
            .collect::<Result<_, _>>()
            .unwrap_or_else(fail);
        let judged = structure::judge(&manifest, &compiled).unwrap_or_else(fail);
        let layout = Layout::of(&manifest, &compiled, &judged.edges).unwrap_or_else(fail);
        let routed = ring_steps(n, layout.target.0);
        let steps: Vec<Step> = routed.iter().map(|(_, step)| step.clone()).collect();
        let start = Instant::now();
        let journal = journal(&files, &steps);
        let elapsed = start.elapsed();
        assert_eq!(
            journal,
            super::journal_tests::journal(&files, &steps),
            "the journal is deterministic"
        );
        let mut kinds = BTreeMap::new();
        for (at, ((route, step), entry)) in routed.iter().zip(&journal.entries).enumerate() {
            *kinds.entry(entry.kind).or_insert(0usize) += 1;
            let (pre, post) = (&journal.states[at], &journal.states[at + 1]);
            let (kind, leaf_post) = leaf_decision(&compiled[*route], &layout, *route, pre, step);
            assert_eq!(
                entry.kind, kind,
                "n={n} step {at}: the aggregate decides as leaf {route}"
            );
            for (slot, (owner, field)) in layout.slots.iter().enumerate() {
                let aggregate_field = state_field(slot);
                if *owner == *route && kind == "commit" {
                    assert_eq!(
                        post[&aggregate_field], leaf_post[field],
                        "n={n} step {at} slot {slot}"
                    );
                } else {
                    assert_eq!(
                        post[&aggregate_field], pre[&aggregate_field],
                        "n={n} step {at} frames slot {slot}"
                    );
                }
            }
        }
        assert!(
            kinds.get("commit").copied().unwrap_or(0) > 0
                && kinds.get("reject").copied().unwrap_or(0) > 0
        );
        assert_eq!(kinds.get("refused"), None);
        println!(
            "journal n={n}: {} steps, {} commits, {} rejects, every leaf touched, {} ms",
            steps.len(),
            kinds.get("commit").copied().unwrap_or(0),
            kinds.get("reject").copied().unwrap_or(0),
            elapsed.as_millis()
        );
    }
}
