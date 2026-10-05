//! Exact cut rewriting: DAG-aware local Boolean resynthesis in the style of
//! ABC's rewriting (Mishchenko, Chatterjee and Brayton, DAC 2006), over the
//! e-graph instead of a fixed network.
//!
//! A cut of a Boolean class is a set of at most three leaf classes and the
//! class's value as a function of them, derived through one chain of e-nodes;
//! it holds on every tuple, because each step is an equation the e-graph
//! already knows. For each cut whose leaves no trap can poison, the class is
//! proposed equal to the minimum circuit of that function from
//! [`super::cut_table`], built over the leaves; the e-graph's merge guard
//! decides, and extraction chooses among the alternatives.

use std::collections::{BTreeMap, BTreeSet};

use super::cut_table::{WITH_EQ, WITHOUT_EQ};
use super::egraph::{ClassId, EGraph, ENode, MergeReason, NodeId};
use super::extract::{Cost, structure_costs};
use super::rules::{Rewrite, Term};
use super::semantics::Kind;

/// Largest number of leaves in a cut.
pub(crate) const CUT_LEAVES: usize = 3;
/// Most cuts one class keeps besides itself.
const CUTS_PER_CLASS: usize = 16;
/// Volume of an e-node outside the search's profile: no circuit is too
/// costly to replace it.
const OUTSIDE_PROFILE: u32 = 1 << 16;

/// Position of the `vars`-input functions in a table: the tables list every
/// function of 0, 1, 2 and 3 inputs in turn.
fn offset(vars: usize) -> usize {
    [0, 2, 6, 22][vars]
}

/// One cut: sorted leaf classes, the function over them (bit `t` is the
/// value when leaf `i` has value `t >> i & 1`), and the number of e-nodes on
/// the chain it was derived through, counting shared ones once per use.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Cut {
    leaves: Vec<ClassId>,
    function: u8,
    volume: u32,
}

/// Mask of every assignment of `vars` inputs.
fn full(vars: usize) -> u8 {
    ((1_u16 << (1 << vars)) - 1) as u8
}

/// `function` over `from`, re-expressed over the superset `to`.
fn expand(function: u8, from: &[ClassId], to: &[ClassId]) -> u8 {
    let positions: Vec<usize> = from
        .iter()
        .map(|leaf| to.iter().position(|other| other == leaf).unwrap_or(0))
        .collect();
    (0..1_usize << to.len()).fold(0, |result, assignment| {
        let local = positions
            .iter()
            .enumerate()
            .fold(0, |local, (index, position)| {
                local | (assignment >> position & 1) << index
            });
        result | (function >> local & 1) << assignment
    })
}

/// The cuts of every Boolean class through its cheapest e-node, and the
/// number of cut combinations evaluated. As in ABC, cuts follow one
/// structure, the cheapest tree; it is acyclic, and classes are visited
/// children first, by ascending tree cost. The structure may use e-nodes
/// outside the search's profile: a cut's function holds whatever chain
/// derived it, and only the replacement circuit must stay in the profile.
fn enumerate(egraph: &EGraph) -> (BTreeMap<ClassId, Vec<Cut>>, u64) {
    let costs = structure_costs(egraph);
    let mut order: Vec<(Cost, ClassId, NodeId)> = costs
        .iter()
        .filter(|(class, _)| egraph.data(**class).kind == Kind::Bool)
        .map(|(class, (cost, id))| (*cost, *class, *id))
        .collect();
    order.sort_unstable();
    let mut cuts: BTreeMap<ClassId, Vec<Cut>> = BTreeMap::new();
    let mut evaluations = 0_u64;
    for (_, class, id) in order {
        let trivial = Cut {
            leaves: vec![class],
            function: 0b10,
            volume: 0,
        };
        if let Some(value) = egraph.data(class).constant() {
            cuts.insert(
                class,
                vec![Cut {
                    leaves: Vec::new(),
                    function: u8::from(value == 1),
                    volume: 0,
                }],
            );
            continue;
        }
        let node = egraph.node(id);
        let children = match node {
            ENode::Not(a) => vec![a],
            ENode::And(a, b) => vec![a, b],
            ENode::Eq(a, b) if egraph.data(a).kind == Kind::Bool => vec![a, b],
            ENode::Select(c, a, b) if egraph.data(a).kind == Kind::Bool => vec![c, a, b],
            _ => {
                cuts.insert(class, vec![trivial]);
                continue;
            }
        };
        let sets: Vec<&Vec<Cut>> = children
            .iter()
            .filter_map(|child| cuts.get(&egraph.find(*child)))
            .collect();
        if sets.len() != children.len() {
            cuts.insert(class, vec![trivial]);
            continue;
        }
        let own = if egraph.admits(node) {
            1
        } else {
            OUTSIDE_PROFILE
        };
        let mut found: BTreeMap<(Vec<ClassId>, u8), u32> = BTreeMap::new();
        combine(node, own, &sets, &mut |leaves, function, volume| {
            evaluations += 1;
            let entry = found.entry((leaves, function)).or_insert(volume);
            *entry = (*entry).min(volume);
        });
        let mut derived: Vec<Cut> = found
            .into_iter()
            .map(|((leaves, function), volume)| Cut {
                leaves,
                function,
                volume,
            })
            .filter(|cut| !cut.leaves.contains(&class))
            .collect();
        derived.sort_by_key(|cut| {
            (
                cut.leaves.len(),
                std::cmp::Reverse(cut.volume),
                cut.leaves.clone(),
                cut.function,
            )
        });
        derived.truncate(CUTS_PER_CLASS);
        let mut set = vec![trivial];
        set.extend(derived);
        cuts.insert(class, set);
    }
    (cuts, evaluations)
}

/// Every combination of one cut per child, as (leaves, function, volume);
/// the e-node itself adds `own` to the volume.
fn combine(
    node: ENode,
    own: u32,
    sets: &[&Vec<Cut>],
    emit: &mut impl FnMut(Vec<ClassId>, u8, u32),
) {
    let mut chosen: Vec<&Cut> = Vec::with_capacity(sets.len());
    pick(node, own, sets, &mut chosen, emit);
}

fn pick<'a>(
    node: ENode,
    own: u32,
    sets: &[&'a Vec<Cut>],
    chosen: &mut Vec<&'a Cut>,
    emit: &mut impl FnMut(Vec<ClassId>, u8, u32),
) {
    if chosen.len() == sets.len() {
        let leaves: BTreeSet<ClassId> = chosen
            .iter()
            .flat_map(|cut| cut.leaves.iter().copied())
            .collect();
        if leaves.len() > CUT_LEAVES {
            return;
        }
        let leaves: Vec<ClassId> = leaves.into_iter().collect();
        let mask = full(leaves.len());
        let inputs: Vec<u8> = chosen
            .iter()
            .map(|cut| expand(cut.function, &cut.leaves, &leaves))
            .collect();
        let function = match node {
            ENode::Not(_) => !inputs[0] & mask,
            ENode::And(..) => inputs[0] & inputs[1],
            ENode::Eq(..) => !(inputs[0] ^ inputs[1]) & mask,
            ENode::Select(..) => (inputs[0] & inputs[1]) | (!inputs[0] & inputs[2] & mask),
            _ => return,
        };
        let volume = chosen
            .iter()
            .fold(own, |volume, cut| volume.saturating_add(cut.volume));
        emit(leaves, function, volume);
        return;
    }
    for cut in sets[chosen.len()] {
        chosen.push(cut);
        pick(node, own, sets, chosen, emit);
        chosen.pop();
    }
}

/// The function over its essential leaves only.
fn essential(leaves: &[ClassId], function: u8) -> (Vec<ClassId>, u8) {
    let mask = full(leaves.len());
    let kept: Vec<usize> = (0..leaves.len())
        .filter(|index| {
            (0..1_usize << leaves.len())
                .filter(|assignment| assignment >> index & 1 == 0)
                .any(|assignment| {
                    (function >> assignment & 1) != (function >> (assignment | 1 << index) & 1)
                })
        })
        .collect();
    let new: Vec<ClassId> = kept.iter().map(|index| leaves[*index]).collect();
    let function = expand_onto(function & mask, leaves, &new);
    (new, function)
}

/// `function` over `from` restricted to the subset `to`; the dropped leaves
/// must not matter.
fn expand_onto(function: u8, from: &[ClassId], to: &[ClassId]) -> u8 {
    (0..1_usize << to.len()).fold(0, |result, assignment| {
        let wide = to.iter().enumerate().fold(0, |wide, (index, leaf)| {
            let position = from.iter().position(|other| other == leaf).unwrap_or(0);
            wide | (assignment >> index & 1) << position
        });
        result | (function >> wide & 1) << assignment
    })
}

/// The table circuit of a function of `vars` essential inputs.
pub(crate) fn circuit(with_eq: bool, vars: usize, function: u8) -> &'static str {
    let table = if with_eq { &WITH_EQ } else { &WITHOUT_EQ };
    table[offset(vars) + usize::from(function)]
}

/// Number of gates in a circuit; constants count, leaves do not.
pub(crate) fn cost(circuit: &str) -> u32 {
    circuit
        .split(' ')
        .filter(|token| !token.is_empty() && !token.starts_with('@'))
        .count() as u32
}

/// The circuit as a right-hand side over its leaf classes.
fn term(circuit: &str, leaves: &[ClassId]) -> Term {
    let mut operands: Vec<Term> = leaves.iter().map(|leaf| Term::Class(*leaf)).collect();
    let mut output = None;
    for token in circuit.split(' ').filter(|token| !token.is_empty()) {
        let mut chars = token.chars();
        let op = chars.next().unwrap_or('@');
        let digits: Vec<usize> = chars
            .map(|digit| digit.to_digit(10).unwrap_or(0) as usize)
            .collect();
        let at = |index: usize| Box::new(operands[digits[index]].clone());
        let gate = match op {
            'T' => Term::Bool(true),
            'F' => Term::Bool(false),
            'N' => Term::Not(at(0)),
            'A' => Term::And(at(0), at(1)),
            'E' => Term::Eq(at(0), at(1)),
            'S' => Term::Select(at(0), at(1), at(2)),
            _ => {
                output = Some(operands[digits[0]].clone());
                continue;
            }
        };
        operands.push(gate);
    }
    output.unwrap_or_else(|| operands.pop().unwrap_or(Term::Bool(false)))
}

/// What one round of cut enumeration proposed.
pub(crate) struct Proposals {
    pub(crate) rewrites: Vec<Rewrite>,
    /// Cut combinations evaluated.
    pub(crate) evaluations: u64,
    /// Whether more rewrites were found than `limit`.
    pub(crate) limit_hit: bool,
}

/// Proposes, for every Boolean class and every cut over unpoisonable leaves
/// whose table circuit has fewer gates than the chain it was derived
/// through, the class equal to that circuit.
pub(crate) fn proposals(egraph: &EGraph, limit: usize) -> Proposals {
    let with_eq = egraph.admits(ENode::Eq(0, 0));
    let (cuts, evaluations) = enumerate(egraph);
    let mut rewrites = Vec::new();
    let mut seen: BTreeSet<(ClassId, Vec<ClassId>, u8)> = BTreeSet::new();
    let mut limit_hit = false;
    'classes: for (class, set) in &cuts {
        for cut in set.iter().filter(|cut| cut.volume > 0) {
            if cut.leaves.iter().any(|leaf| egraph.data(*leaf).may_poison) {
                continue;
            }
            let (leaves, function) = essential(&cut.leaves, cut.function);
            let circuit = circuit(with_eq, leaves.len(), function);
            if cost(circuit) >= cut.volume || !seen.insert((*class, leaves.clone(), function)) {
                continue;
            }
            if rewrites.len() == limit {
                limit_hit = true;
                break 'classes;
            }
            rewrites.push(Rewrite {
                lhs: *class,
                rhs: term(circuit, &leaves),
                rule: "cut-rewrite",
                reason: MergeReason::Rule,
            });
        }
    }
    Proposals {
        rewrites,
        evaluations,
        limit_hit,
    }
}

#[cfg(test)]
pub(crate) mod generator {
    //! Test-only generator of the minimum-circuit tables: a breadth-first
    //! search over sets of computed functions, so the first circuit to reach
    //! a function has the fewest gates. Leaves are free; constants, `Not`,
    //! `And`, `Select` and, when allowed, `Eq` each cost one gate.

    use std::collections::BTreeSet;

    #[derive(Clone, Copy, Debug)]
    enum Gate {
        True,
        False,
        Not(usize),
        And(usize, usize),
        Eq(usize, usize),
        Select(usize, usize, usize),
    }

    impl Gate {
        fn token(self) -> String {
            match self {
                Gate::True => String::from("T"),
                Gate::False => String::from("F"),
                Gate::Not(a) => format!("N{a}"),
                Gate::And(a, b) => format!("A{a}{b}"),
                Gate::Eq(a, b) => format!("E{a}{b}"),
                Gate::Select(c, a, b) => format!("S{c}{a}{b}"),
            }
        }
    }

    /// Every gate over the available functions, in a fixed order, with the
    /// function it computes.
    fn options(available: &[u16], mask: u16, with_eq: bool) -> Vec<(Gate, u16)> {
        let mut options = vec![(Gate::True, mask), (Gate::False, 0)];
        let count = available.len();
        for (a, function) in available.iter().enumerate() {
            options.push((Gate::Not(a), !function & mask));
        }
        for a in 0..count {
            for b in a + 1..count {
                options.push((Gate::And(a, b), available[a] & available[b]));
            }
        }
        if with_eq {
            for a in 0..count {
                for b in a + 1..count {
                    options.push((Gate::Eq(a, b), !(available[a] ^ available[b]) & mask));
                }
            }
        }
        for c in 0..count {
            if available[c] == 0 || available[c] == mask {
                continue;
            }
            for a in 0..count {
                for b in 0..count {
                    if a != b {
                        let function =
                            (available[c] & available[a]) | (!available[c] & available[b] & mask);
                        options.push((Gate::Select(c, a, b), function));
                    }
                }
            }
        }
        options.retain(|(_, function)| !available.contains(function));
        options
    }

    /// Minimum circuits for every function of `vars` inputs, by function.
    pub(crate) fn generate(vars: usize, with_eq: bool) -> Vec<String> {
        let assignments = 1_usize << vars;
        let mask = ((1_u32 << assignments) - 1) as u16;
        let leaves: Vec<u16> = (0..vars)
            .map(|leaf| {
                (0..assignments)
                    .filter(|assignment| assignment >> leaf & 1 == 1)
                    .fold(0, |function, assignment| function | 1 << assignment)
            })
            .collect();
        let mut best: Vec<Option<String>> = vec![None; 1 << assignments];
        for (leaf, function) in leaves.iter().enumerate() {
            best[usize::from(*function)] = Some(format!("@{leaf}"));
        }
        // A state is a gate sequence and the functions it computes.
        let mut level: Vec<(Vec<Gate>, Vec<u16>)> = vec![(Vec::new(), Vec::new())];
        while best.iter().any(Option::is_none) {
            let tokens = |gates: &[Gate], gate: Gate| {
                gates
                    .iter()
                    .copied()
                    .chain([gate])
                    .map(Gate::token)
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            for (gates, functions) in &level {
                let available: Vec<u16> = leaves.iter().chain(functions).copied().collect();
                for (gate, function) in options(&available, mask, with_eq) {
                    let entry = &mut best[usize::from(function)];
                    if entry.is_none() {
                        *entry = Some(tokens(gates, gate));
                    }
                }
            }
            if best.iter().all(Option::is_some) {
                break;
            }
            let mut seen: BTreeSet<Vec<u16>> = BTreeSet::new();
            let mut next = Vec::new();
            for (gates, functions) in &level {
                let available: Vec<u16> = leaves.iter().chain(functions).copied().collect();
                for (gate, function) in options(&available, mask, with_eq) {
                    let mut key = functions.clone();
                    key.push(function);
                    key.sort_unstable();
                    if seen.insert(key) {
                        let mut gates = gates.clone();
                        gates.push(gate);
                        let mut functions = functions.clone();
                        functions.push(function);
                        next.push((gates, functions));
                    }
                }
            }
            level = next;
        }
        best.into_iter().flatten().collect()
    }

    /// Both tables: functions of 0, 1, 2 and 3 inputs in turn.
    pub(crate) fn tables() -> [Vec<String>; 2] {
        [true, false].map(|with_eq| (0..=3).flat_map(|vars| generate(vars, with_eq)).collect())
    }
}
