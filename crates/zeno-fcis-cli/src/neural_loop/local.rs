//! The deterministic local proposer: a fixed set of algebraic rewrites,
//! hash-consing and dead-node elimination over the finite IR, applied to a
//! fixpoint. It is a proposer like any other: its candidates are checked by
//! `transform::check` and nothing here is trusted. Its rewrites are chosen to
//! preserve eager checked semantics (an `Add`/`Sub` that may trap is never
//! removed unless the aggressive variant asks for it), so the local-only arm
//! has a meaningful success rate; a mistaken rewrite would surface as a
//! replayed counterexample, not as an accepted candidate.

use super::program_json::encode;
use std::collections::BTreeMap;
use zeno_fcis_synthesis::finite::{Op, Program};

/// Proposer identity for provenance.
pub(crate) const IDENTITY: &str = "zeno-fcis/local-proposer/1";
const MAX_PASSES: usize = 8;

/// Yields distinct candidates in a fixed order, then reports exhaustion.
#[derive(Clone, Debug)]
pub(crate) struct LocalProposer {
    candidates: Vec<Vec<u8>>,
    next: usize,
}

impl LocalProposer {
    /// Precomputes the conservative simplification and, when it differs, the
    /// aggressive one (dead trapping nodes removed too). Candidates identical
    /// to the original are omitted.
    pub(crate) fn new(program: &Program, original: &[u8]) -> LocalProposer {
        let mut candidates: Vec<Vec<u8>> = Vec::new();
        for aggressive in [false, true] {
            if let Some(bytes) =
                simplify(program, aggressive).and_then(|program| encode(&program).ok())
                && bytes != original
                && !candidates.contains(&bytes)
            {
                candidates.push(bytes);
            }
        }
        LocalProposer {
            candidates,
            next: 0,
        }
    }

    /// The next candidate, or `None` when exhausted.
    pub(crate) fn next(&mut self) -> Option<Vec<u8>> {
        let candidate = self.candidates.get(self.next).cloned();
        self.next = self.next.saturating_add(1);
        candidate
    }

    pub(crate) fn remaining(&self) -> usize {
        self.candidates.len().saturating_sub(self.next)
    }
}

/// Rewrites to a fixpoint; `None` when the result would not admit.
pub(crate) fn simplify(program: &Program, aggressive: bool) -> Option<Program> {
    let mut current = program.clone();
    for _ in 0..MAX_PASSES {
        let next = pass(&current, aggressive)?;
        if next == current {
            return Some(next);
        }
        current = next;
    }
    Some(current)
}

/// One rewrite pass: simplify and hash-cons every node in order, then drop
/// dead nodes and renumber.
fn pass(program: &Program, aggressive: bool) -> Option<Program> {
    let nodes = program.nodes();
    let mut rebuilt: Vec<Op> = Vec::with_capacity(nodes.len());
    let mut canon: Vec<u16> = Vec::with_capacity(nodes.len());
    let mut interned: BTreeMap<Key, u16> = BTreeMap::new();
    for op in nodes {
        let remapped = remap(op, &canon)?;
        let simplified = simplify_op(remapped, &rebuilt);
        let id = match simplified {
            Simplified::Alias(id) => id,
            Simplified::Op(op) => {
                let key = Key::of(&op);
                match interned.get(&key) {
                    Some(id) => *id,
                    None => {
                        let id = u16::try_from(rebuilt.len()).ok()?;
                        rebuilt.push(op);
                        interned.insert(key, id);
                        id
                    }
                }
            }
        };
        canon.push(id);
    }
    let roots: Vec<u16> = program
        .roots()
        .iter()
        .map(|root| canon.get(usize::from(*root)).copied())
        .collect::<Option<Vec<_>>>()?;
    let (nodes, roots) = eliminate_dead(&rebuilt, &roots, aggressive)?;
    Program::try_new(
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        nodes,
        roots,
    )
    .ok()
}

fn remap(op: &Op, canon: &[u16]) -> Option<Op> {
    let id = |reference: u16| canon.get(usize::from(reference)).copied();
    Some(match *op {
        Op::Input(a) => Op::Input(a),
        Op::Int(a) => Op::Int(a),
        Op::Bool(a) => Op::Bool(a),
        Op::Add(a, b) => Op::Add(id(a)?, id(b)?),
        Op::Sub(a, b) => Op::Sub(id(a)?, id(b)?),
        Op::Eq(a, b) => Op::Eq(id(a)?, id(b)?),
        Op::Lt(a, b) => Op::Lt(id(a)?, id(b)?),
        Op::And(a, b) => Op::And(id(a)?, id(b)?),
        Op::Not(a) => Op::Not(id(a)?),
        Op::Select(c, a, b) => Op::Select(id(c)?, id(a)?, id(b)?),
        _ => return None,
    })
}

enum Simplified {
    /// The node is another, already built node.
    Alias(u16),
    /// A node to intern.
    Op(Op),
}

fn constant_bool(nodes: &[Op], id: u16) -> Option<bool> {
    match nodes.get(usize::from(id)) {
        Some(Op::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn constant_int(nodes: &[Op], id: u16) -> Option<i64> {
    match nodes.get(usize::from(id)) {
        Some(Op::Int(value)) => Some(*value),
        _ => None,
    }
}

fn negation_of(nodes: &[Op], id: u16) -> Option<u16> {
    match nodes.get(usize::from(id)) {
        Some(Op::Not(inner)) => Some(*inner),
        _ => None,
    }
}

/// Value-preserving rewrites. Integer folds keep a node that would trap.
fn simplify_op(op: Op, nodes: &[Op]) -> Simplified {
    use Simplified::{Alias, Op as Keep};
    match op {
        Op::Not(a) => match (constant_bool(nodes, a), negation_of(nodes, a)) {
            (Some(value), _) => Keep(Op::Bool(!value)),
            (None, Some(inner)) => Alias(inner),
            _ => Keep(Op::Not(a)),
        },
        Op::And(a, b) => {
            if a == b {
                return Alias(a);
            }
            match (constant_bool(nodes, a), constant_bool(nodes, b)) {
                (Some(false), _) | (_, Some(false)) => return Keep(Op::Bool(false)),
                (Some(true), _) => return Alias(b),
                (_, Some(true)) => return Alias(a),
                _ => {}
            }
            if negation_of(nodes, a) == Some(b) || negation_of(nodes, b) == Some(a) {
                return Keep(Op::Bool(false));
            }
            Keep(Op::And(a, b))
        }
        Op::Select(c, a, b) => {
            if a == b {
                return Alias(a);
            }
            match constant_bool(nodes, c) {
                Some(true) => return Alias(a),
                Some(false) => return Alias(b),
                None => {}
            }
            match (constant_bool(nodes, a), constant_bool(nodes, b)) {
                (Some(true), Some(false)) => Alias(c),
                (Some(false), Some(true)) => Keep(Op::Not(c)),
                _ => Keep(Op::Select(c, a, b)),
            }
        }
        Op::Add(a, b) => match (constant_int(nodes, a), constant_int(nodes, b)) {
            (Some(x), Some(y)) => x
                .checked_add(y)
                .map_or(Keep(Op::Add(a, b)), |sum| Keep(Op::Int(sum))),
            (Some(0), None) => Alias(b),
            (None, Some(0)) => Alias(a),
            _ => Keep(Op::Add(a, b)),
        },
        Op::Sub(a, b) => {
            if a == b {
                return Keep(Op::Int(0));
            }
            match (constant_int(nodes, a), constant_int(nodes, b)) {
                (Some(x), Some(y)) => x
                    .checked_sub(y)
                    .map_or(Keep(Op::Sub(a, b)), |difference| Keep(Op::Int(difference))),
                (None, Some(0)) => Alias(a),
                _ => Keep(Op::Sub(a, b)),
            }
        }
        Op::Eq(a, b) => {
            if a == b {
                return Keep(Op::Bool(true));
            }
            match (
                constant_int(nodes, a),
                constant_int(nodes, b),
                constant_bool(nodes, a),
                constant_bool(nodes, b),
            ) {
                (Some(x), Some(y), _, _) => Keep(Op::Bool(x == y)),
                (_, _, Some(x), Some(y)) => Keep(Op::Bool(x == y)),
                _ => Keep(Op::Eq(a, b)),
            }
        }
        Op::Lt(a, b) => {
            if a == b {
                return Keep(Op::Bool(false));
            }
            match (constant_int(nodes, a), constant_int(nodes, b)) {
                (Some(x), Some(y)) => Keep(Op::Bool(x < y)),
                _ => Keep(Op::Lt(a, b)),
            }
        }
        other => Keep(other),
    }
}

/// Structural key for hash-consing.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Key {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(u16, u16),
    Sub(u16, u16),
    Eq(u16, u16),
    Lt(u16, u16),
    And(u16, u16),
    Not(u16),
    Select(u16, u16, u16),
    Other,
}

impl Key {
    fn of(op: &Op) -> Key {
        match *op {
            Op::Input(a) => Key::Input(a),
            Op::Int(a) => Key::Int(a),
            Op::Bool(a) => Key::Bool(a),
            Op::Add(a, b) => Key::Add(a, b),
            Op::Sub(a, b) => Key::Sub(a, b),
            Op::Eq(a, b) => Key::Eq(a, b),
            Op::Lt(a, b) => Key::Lt(a, b),
            Op::And(a, b) => Key::And(a, b),
            Op::Not(a) => Key::Not(a),
            Op::Select(c, a, b) => Key::Select(c, a, b),
            _ => Key::Other,
        }
    }
}

fn operands(op: &Op) -> Vec<u16> {
    match *op {
        Op::Input(_) | Op::Int(_) | Op::Bool(_) => Vec::new(),
        Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => vec![a, b],
        Op::Not(a) => vec![a],
        Op::Select(c, a, b) => vec![c, a, b],
        _ => Vec::new(),
    }
}

/// Removes nodes no root reaches. Unless `aggressive`, every `Add` and `Sub`
/// stays live, because a dead checked operation still traps eagerly on the
/// inputs that overflow it.
fn eliminate_dead(nodes: &[Op], roots: &[u16], aggressive: bool) -> Option<(Vec<Op>, Vec<u16>)> {
    let mut live = vec![false; nodes.len()];
    let mut stack: Vec<u16> = roots.to_vec();
    if !aggressive {
        for (index, op) in nodes.iter().enumerate() {
            if matches!(op, Op::Add(..) | Op::Sub(..)) {
                stack.push(u16::try_from(index).ok()?);
            }
        }
    }
    while let Some(id) = stack.pop() {
        let index = usize::from(id);
        if *live.get(index)? {
            continue;
        }
        live[index] = true;
        stack.extend(operands(&nodes[index]));
    }
    let mut renumber = vec![u16::MAX; nodes.len()];
    let mut kept = Vec::new();
    for (index, op) in nodes.iter().enumerate() {
        if !live[index] {
            continue;
        }
        let id = |reference: u16| renumber.get(usize::from(reference)).copied();
        let rebuilt = match *op {
            Op::Input(a) => Op::Input(a),
            Op::Int(a) => Op::Int(a),
            Op::Bool(a) => Op::Bool(a),
            Op::Add(a, b) => Op::Add(id(a)?, id(b)?),
            Op::Sub(a, b) => Op::Sub(id(a)?, id(b)?),
            Op::Eq(a, b) => Op::Eq(id(a)?, id(b)?),
            Op::Lt(a, b) => Op::Lt(id(a)?, id(b)?),
            Op::And(a, b) => Op::And(id(a)?, id(b)?),
            Op::Not(a) => Op::Not(id(a)?),
            Op::Select(c, a, b) => Op::Select(id(c)?, id(a)?, id(b)?),
            _ => return None,
        };
        renumber[index] = u16::try_from(kept.len()).ok()?;
        kept.push(rebuilt);
    }
    let roots = roots
        .iter()
        .map(|root| renumber.get(usize::from(*root)).copied())
        .collect::<Option<Vec<_>>>()?;
    Some((kept, roots))
}
