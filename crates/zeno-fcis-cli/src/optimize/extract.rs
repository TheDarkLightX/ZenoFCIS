//! Deterministic extraction of a candidate program from the e-graph.
//!
//! Cost is the pair (instruction count, canonical byte length); byte length
//! is exact per instruction because the canonical encoding is fixed-width.
//! The tree extractor picks the cheapest tree per class bottom up. The
//! `dag-greedy` extractor then tries, class by class, every alternative e-node
//! and keeps a switch only when the whole shared candidate becomes strictly
//! cheaper, so sharing is counted once and no cycle can be chosen. Every
//! pinned (may-trap) e-node is emitted whether or not an output needs it. An
//! e-node outside the search's profile has no finite cost and is never
//! chosen.

use std::collections::{BTreeMap, BTreeSet};

use zeno_fcis_synthesis::finite::{Domain, Op, Program};

use super::egraph::{ClassId, EGraph, ENode, NodeId};
use super::strategy::Extractor;

/// Lexicographic cost of a (partial) candidate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Cost {
    pub(crate) nodes: u64,
    pub(crate) bytes: u64,
}

impl Cost {
    fn of(node: ENode) -> Self {
        Self {
            nodes: 1,
            bytes: node.bytes(),
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            nodes: self.nodes.saturating_add(other.nodes),
            bytes: self.bytes.saturating_add(other.bytes),
        }
    }
}

/// Why no candidate could be extracted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExtractError {
    /// A root or pinned class has no finite-cost e-node; impossible for a
    /// graph built from an admitted program.
    Unextractable(ClassId),
    /// The emitted program failed the library's admission; `code` names it.
    NotAdmitted(&'static str),
}

/// An extracted candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Extraction {
    pub(crate) program: Program,
    pub(crate) cost: Cost,
    /// Improvement rounds the `dag-greedy` extractor ran.
    pub(crate) rounds_run: u32,
    /// Whether it stopped at its round limit while still improving.
    pub(crate) limit_hit: bool,
}

/// Choices of one e-node per class; pinned classes are forced.
pub(crate) type Choice = BTreeMap<ClassId, NodeId>;

/// Cheapest tree per live class, bottom up, with pinned classes forced to
/// their lowest pinned e-node. Classes with no finite tree are absent; an
/// e-node outside the search's profile has no finite cost.
pub(crate) fn tree_costs(egraph: &EGraph) -> BTreeMap<ClassId, (Cost, NodeId)> {
    costs(egraph, true)
}

/// [`tree_costs`] over every e-node, the profile ignored: a structure to
/// derive facts from, never a candidate.
pub(crate) fn structure_costs(egraph: &EGraph) -> BTreeMap<ClassId, (Cost, NodeId)> {
    costs(egraph, false)
}

fn costs(egraph: &EGraph, profiled: bool) -> BTreeMap<ClassId, (Cost, NodeId)> {
    let classes = egraph.classes();
    let mut forced: BTreeMap<ClassId, NodeId> = BTreeMap::new();
    for pinned in egraph.pinned() {
        forced.entry(egraph.node_class(pinned)).or_insert(pinned);
    }
    let mut best: BTreeMap<ClassId, (Cost, NodeId)> = BTreeMap::new();
    for _ in 0..=classes.len() {
        let mut changed = false;
        for &class in &classes {
            let candidates = match forced.get(&class) {
                Some(&pinned) => vec![pinned],
                None => egraph.class_nodes(class),
            };
            for id in candidates {
                let node = egraph.node(id);
                if profiled && !egraph.admits(node) {
                    continue;
                }
                let Some(cost) = node
                    .children()
                    .iter()
                    .try_fold(Cost::of(node), |cost, child| {
                        best.get(&egraph.find(*child))
                            .map(|(child, _)| cost.plus(*child))
                    })
                else {
                    continue;
                };
                let better = match best.get(&class) {
                    Some(current) => (cost, id) < *current,
                    None => true,
                };
                if better {
                    best.insert(class, (cost, id));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    best
}

/// The e-nodes a choice needs for the roots and every pinned e-node, with
/// their total cost. `None` when a needed class has no choice.
fn needed(
    egraph: &EGraph,
    choice: &Choice,
    roots: &[ClassId],
    pinned: &[NodeId],
) -> Option<(BTreeSet<NodeId>, Cost)> {
    let mut set: BTreeSet<NodeId> = BTreeSet::new();
    let mut stack: Vec<NodeId> = pinned.to_vec();
    for root in roots {
        stack.push(*choice.get(&egraph.find(*root))?);
    }
    while let Some(id) = stack.pop() {
        if !set.insert(id) {
            continue;
        }
        for child in egraph.node(id).children() {
            stack.push(*choice.get(&egraph.find(child))?);
        }
    }
    let cost = set.iter().fold(Cost::default(), |cost, id| {
        cost.plus(Cost::of(egraph.node(*id)))
    });
    Some((set, cost))
}

/// Whether `class` is reachable from the children of `node` under `choice`.
fn reaches(egraph: &EGraph, choice: &Choice, node: ENode, class: ClassId) -> bool {
    let mut seen: BTreeSet<ClassId> = BTreeSet::new();
    let mut stack: Vec<ClassId> = node
        .children()
        .into_iter()
        .map(|c| egraph.find(c))
        .collect();
    while let Some(current) = stack.pop() {
        if current == class {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        if let Some(id) = choice.get(&current) {
            stack.extend(
                egraph
                    .node(*id)
                    .children()
                    .into_iter()
                    .map(|c| egraph.find(c)),
            );
        }
    }
    false
}

/// Extracts a candidate for `roots` over the declared ABI.
pub(crate) fn extract(
    egraph: &EGraph,
    roots: &[ClassId],
    inputs: &[Domain],
    outputs: &[Domain],
    extractor: Extractor,
    max_rounds: u32,
) -> Result<Extraction, ExtractError> {
    let pinned = egraph.pinned();
    extract_with(
        egraph, roots, inputs, outputs, extractor, max_rounds, &pinned,
    )
}

/// Extraction with an explicit pinned set. Production always passes every
/// pinned e-node; a test may drop them to show the judge refusing the result.
pub(crate) fn extract_with(
    egraph: &EGraph,
    roots: &[ClassId],
    inputs: &[Domain],
    outputs: &[Domain],
    extractor: Extractor,
    max_rounds: u32,
    pinned: &[NodeId],
) -> Result<Extraction, ExtractError> {
    let mut choice: Choice = tree_costs(egraph)
        .into_iter()
        .map(|(class, (_, id))| (class, id))
        .collect();
    let forced: BTreeSet<ClassId> = pinned.iter().map(|id| egraph.node_class(*id)).collect();
    let (mut set, mut cost) = needed(egraph, &choice, roots, pinned).ok_or_else(|| {
        let missing = roots
            .iter()
            .map(|root| egraph.find(*root))
            .find(|root| !choice.contains_key(root))
            .unwrap_or(0);
        ExtractError::Unextractable(missing)
    })?;
    let mut rounds_run = 0;
    let mut limit_hit = false;
    if extractor == Extractor::DagGreedy {
        loop {
            if rounds_run >= max_rounds {
                limit_hit = true;
                break;
            }
            rounds_run += 1;
            let mut improved = false;
            let classes: BTreeSet<ClassId> = set.iter().map(|id| egraph.node_class(*id)).collect();
            for class in classes {
                if forced.contains(&class) {
                    continue;
                }
                let Some(current) = choice.get(&class).copied() else {
                    continue;
                };
                for alternative in egraph.class_nodes(class) {
                    if alternative == current {
                        continue;
                    }
                    let node = egraph.node(alternative);
                    if !egraph.admits(node) || reaches(egraph, &choice, node, class) {
                        continue;
                    }
                    choice.insert(class, alternative);
                    match needed(egraph, &choice, roots, pinned) {
                        Some((new_set, new_cost)) if new_cost < cost => {
                            set = new_set;
                            cost = new_cost;
                            improved = true;
                            break;
                        }
                        _ => {
                            choice.insert(class, current);
                        }
                    }
                }
            }
            if !improved {
                limit_hit = false;
                break;
            }
        }
    }
    let program = emit(egraph, &choice, &set, roots, inputs, outputs)?;
    Ok(Extraction {
        program,
        cost,
        rounds_run,
        limit_hit,
    })
}

/// Emits the needed e-nodes in a stable topological order: among the ready
/// instructions, the one from the earliest original position comes first,
/// then the lowest e-node id.
fn emit(
    egraph: &EGraph,
    choice: &Choice,
    set: &BTreeSet<NodeId>,
    roots: &[ClassId],
    inputs: &[Domain],
    outputs: &[Domain],
) -> Result<Program, ExtractError> {
    let key = |id: NodeId| (egraph.node_origin(id).map_or(u32::MAX, u32::from), id);
    let mut remaining: BTreeSet<(u32, NodeId)> = set.iter().map(|id| key(*id)).collect();
    let mut emitted: BTreeMap<ClassId, u16> = BTreeMap::new();
    let mut ops: Vec<Op> = Vec::with_capacity(set.len());
    while !remaining.is_empty() {
        let ready = remaining.iter().copied().find(|(_, id)| {
            egraph
                .node(*id)
                .children()
                .iter()
                .all(|child| emitted.contains_key(&egraph.find(*child)))
        });
        let Some(entry) = ready else {
            // Impossible for an acyclic choice; refuse rather than loop.
            return Err(ExtractError::NotAdmitted("cyclic-choice"));
        };
        remaining.remove(&entry);
        let id = entry.1;
        let node = egraph.node(id);
        let index =
            u16::try_from(ops.len()).map_err(|_| ExtractError::NotAdmitted("program-shape"))?;
        ops.push(node.op(|child| {
            emitted
                .get(&egraph.find(child))
                .copied()
                .unwrap_or(u16::MAX)
        }));
        let class = egraph.node_class(id);
        // The class's chosen e-node is its representative; an unchosen pinned
        // e-node is emitted but never referenced.
        if choice.get(&class) == Some(&id) {
            emitted.insert(class, index);
        }
    }
    let root_indices = roots
        .iter()
        .map(|root| emitted.get(&egraph.find(*root)).copied())
        .collect::<Option<Vec<_>>>()
        .ok_or(ExtractError::NotAdmitted("missing-root"))?;
    Program::try_new(inputs.to_vec(), outputs.to_vec(), ops, root_indices).map_err(|error| {
        ExtractError::NotAdmitted(match error {
            zeno_fcis_synthesis::finite::Error::Invalid(code) => code,
            _ => "unclassified",
        })
    })
}
