//! Don't-care resubstitution, in the style of logic synthesis (Mishchenko and
//! Brayton): a node only has to be right on the tuples where its value can
//! reach an output, so it can be replaced by an existing node, a constant, or
//! one new operation over existing nodes and new constants when that frees
//! more nodes than it adds. Full-table equality alone (the e-graph's merge rule) never
//! finds these replacements.
//!
//! Proposals are untrusted: the observability computed here is exact for
//! fanout-free cones and can be optimistic with reconvergent fanout, and new
//! operations can overflow on tuples where the replaced node was unobserved.
//! Every accepted rewrite is checked against the original by the library
//! evaluator when the domain is within `step_verify_tuples`, and every result
//! is checked again before the portfolio admits it.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Reverse;

use crate::ir::{Graph, Kind, Node};
use crate::pool::{Pool, canonical};
use crate::space::{Meter, Verdict};
use crate::util::{Bits, FNV_BASIS, distinct_up_to, fingerprint, single_value};
use crate::{Env, Stop, admit};

/// The operations a replacement may introduce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Code {
    Add,
    Sub,
    Eq,
    Lt,
    And,
    Not,
    Select,
}

/// A replacement for one node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Spec {
    Node(usize),
    Const(Kind, i128),
    Op(Code, Vec<Spec>),
}

/// One proposal: replace `node` by `spec`, expecting `gain` fewer nodes.
#[derive(Clone, Debug)]
pub(crate) struct Proposal {
    pub(crate) gain: i128,
    pub(crate) node: usize,
    pub(crate) spec: Spec,
}

/// Tuples (as positions in the live list) on which each node's value can
/// reach an output.
pub(crate) fn observability(
    graph: &Graph,
    sig: &[Vec<i128>],
    meter: &mut Meter,
) -> Result<Vec<Vec<bool>>, Stop> {
    let live = sig.first().map_or(0, Vec::len);
    meter.charge(graph.nodes.len().saturating_mul(live))?;
    let mut obs = vec![vec![false; live]; graph.nodes.len()];
    for root in &graph.roots {
        if let Some(mask) = obs.get_mut(*root) {
            mask.fill(true);
        }
    }
    for index in (0..graph.nodes.len()).rev() {
        let node = graph.nodes[index];
        if node.is_leaf() || !obs[index].iter().any(|seen| *seen) {
            continue;
        }
        let mask = obs[index].clone();
        match node {
            Node::Select(c, t, e) => {
                for (i, seen) in mask.iter().enumerate() {
                    if *seen {
                        if sig[c][i] == 1 {
                            obs[t][i] = true;
                        }
                        if sig[c][i] == 0 {
                            obs[e][i] = true;
                        }
                        if sig[t][i] != sig[e][i] {
                            obs[c][i] = true;
                        }
                    }
                }
            }
            Node::And(a, b) => {
                for (i, seen) in mask.iter().enumerate() {
                    if *seen {
                        if sig[b][i] == 1 {
                            obs[a][i] = true;
                        }
                        if sig[a][i] == 1 {
                            obs[b][i] = true;
                        }
                    }
                }
            }
            other => {
                let (operands, count) = other.operands();
                for operand in &operands[..count] {
                    for (i, seen) in mask.iter().enumerate() {
                        if *seen {
                            obs[*operand][i] = true;
                        }
                    }
                }
            }
        }
    }
    Ok(obs)
}

/// Uses of each node, counting every operand slot and every result.
pub(crate) fn reference_counts(graph: &Graph) -> Vec<usize> {
    let mut refs = vec![0usize; graph.nodes.len()];
    for node in &graph.nodes {
        let (operands, count) = node.operands();
        for operand in &operands[..count] {
            refs[*operand] += 1;
        }
    }
    for root in &graph.roots {
        refs[*root] += 1;
    }
    refs
}

/// Nodes freed if `node` is no longer used: its maximum fanout-free cone.
pub(crate) fn mffc(graph: &Graph, node: usize, refs: &[usize], roots: &Bits) -> Bits {
    let mut freed = Bits::new(graph.nodes.len());
    freed.insert(node);
    let mut count = refs.to_vec();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        let (operands, used) = graph.nodes[current].operands();
        for operand in &operands[..used] {
            count[*operand] = count[*operand].saturating_sub(1);
            if count[*operand] == 0 && !roots.contains(*operand) {
                freed.insert(*operand);
                stack.push(*operand);
            }
        }
    }
    freed
}

/// Each node with every node it reads, transitively.
fn transitive_fanins(graph: &Graph) -> Vec<Bits> {
    let mut fanins: Vec<Bits> = Vec::with_capacity(graph.nodes.len());
    for (index, node) in graph.nodes.iter().enumerate() {
        let mut set = Bits::new(graph.nodes.len());
        set.insert(index);
        let (operands, count) = node.operands();
        for operand in &operands[..count] {
            set.union_with(&fanins[*operand]);
        }
        fanins.push(set);
    }
    fanins
}

/// `node` and every node that reads it, transitively.
fn fanout_cone(graph: &Graph, node: usize) -> Bits {
    let mut cone = Bits::new(graph.nodes.len());
    cone.insert(node);
    for index in node + 1..graph.nodes.len() {
        let (operands, count) = graph.nodes[index].operands();
        if operands[..count]
            .iter()
            .any(|operand| cone.contains(*operand))
        {
            cone.insert(index);
        }
    }
    cone
}

/// Program constants, small integers, and offsets between integer nodes and
/// integer results that take at most three values; ordered by magnitude.
pub(crate) fn mined_constants(
    graph: &Graph,
    kinds: &[Kind],
    sig: &[Vec<i128>],
    limit: usize,
    meter: &mut Meter,
) -> Result<Vec<i128>, Stop> {
    let mut found: BTreeSet<i128> = [0, 1, 2, -1].into_iter().collect();
    for node in &graph.nodes {
        if let Node::Int(value) = node {
            found.insert(*value);
        }
    }
    let ints: Vec<usize> = (0..graph.nodes.len())
        .filter(|node| kinds[*node] == Kind::Int)
        .collect();
    let live = sig.first().map_or(0, Vec::len);
    for root in &graph.roots {
        if kinds[*root] != Kind::Int {
            continue;
        }
        for node in ints.iter().take(64) {
            meter.charge(live.saturating_mul(2))?;
            let forward: Option<Vec<i128>> = sig[*node]
                .iter()
                .zip(&sig[*root])
                .map(|(a, b)| a.checked_sub(*b))
                .collect();
            let backward: Option<Vec<i128>> = sig[*root]
                .iter()
                .zip(&sig[*node])
                .map(|(a, b)| a.checked_sub(*b))
                .collect();
            for diff in [forward, backward].into_iter().flatten() {
                if let Some(values) = distinct_up_to(&diff, 3) {
                    found.extend(values);
                }
            }
        }
    }
    let mut ordered: Vec<i128> = found.into_iter().collect();
    ordered.sort_by_key(|value| (value.unsigned_abs(), *value));
    ordered.truncate(limit + 8);
    Ok(ordered)
}

fn compare(code: Code, a: i128, b: i128) -> i128 {
    i128::from(match code {
        Code::Eq => a == b,
        _ => a < b,
    })
}

/// Counts examined elements against the work still available, checking the
/// limit before each element, so a node's loop never runs past the budget.
struct Tally {
    used: usize,
    room: usize,
    exhausted: bool,
}

impl Tally {
    fn new(room: usize) -> Self {
        Self {
            used: 0,
            room,
            exhausted: false,
        }
    }

    /// Reserves `count` units; false (and exhausted) when they do not fit.
    fn add(&mut self, count: usize) -> bool {
        match self.used.checked_add(count) {
            Some(total) if total <= self.room => {
                self.used = total;
                true
            }
            _ => {
                self.exhausted = true;
                false
            }
        }
    }

    /// True when `value(k) == want[k]` for every position, stopping early.
    /// False once the budget is exhausted; `settle` then stops the search.
    fn all(&mut self, want: &[i128], mut value: impl FnMut(usize) -> i128) -> bool {
        for (k, expected) in want.iter().enumerate() {
            if !self.add(1) || value(k) != *expected {
                return false;
            }
        }
        true
    }

    /// Charges the units used; a typed stop if the budget ran out.
    fn settle(&self, meter: &mut Meter) -> Result<(), Stop> {
        meter.charge(self.used)?;
        if self.exhausted {
            return Err(meter.exhausted());
        }
        Ok(())
    }
}

/// Every proposal for `graph`, in a fixed generation order.
pub(crate) fn candidates(
    graph: &Graph,
    sig: &[Vec<i128>],
    max_new: u32,
    complement: bool,
    meter: &mut Meter,
) -> Result<Vec<Proposal>, Stop> {
    let size = graph.nodes.len();
    let kinds = graph.kinds();
    let obs = observability(graph, sig, meter)?;
    let refs = reference_counts(graph);
    let mut roots = Bits::new(size);
    for root in &graph.roots {
        roots.insert(*root);
    }
    let fanins = transitive_fanins(graph);
    let freed_sets: Vec<Bits> = (0..size)
        .map(|node| mffc(graph, node, &refs, &roots))
        .collect();
    let constants = mined_constants(graph, &kinds, sig, 12, meter)?;
    let mut order: Vec<usize> = (0..size).collect();
    order.sort_by_key(|node| Reverse(freed_sets[*node].len()));
    let mut out = Vec::new();
    for node in order {
        if graph.nodes[node].is_leaf() {
            continue;
        }
        let mut tally = Tally::new(meter.remaining());
        let freed = &freed_sets[node];
        let gain0 = freed.len() as i128;
        let kept = |used: &[usize]| {
            let mut union = Bits::new(size);
            for item in used {
                union.union_with(&fanins[*item]);
            }
            union.intersection_len(freed) as i128
        };
        let mut push = |gain: i128, spec: Spec| out.push(Proposal { gain, node, spec });
        let kind = kinds[node];
        let pos: Vec<usize> = obs[node]
            .iter()
            .enumerate()
            .filter_map(|(i, seen)| seen.then_some(i))
            .collect();
        if pos.is_empty() {
            push(gain0 - 1, Spec::Const(kind, 0));
            continue;
        }
        let target = &sig[node];
        let want: Vec<i128> = pos.iter().map(|p| target[*p]).collect();
        if let Some(value) = single_value(&want) {
            push(gain0 - 1, Spec::Const(kind, value));
        }
        let cone = fanout_cone(graph, node);
        let divisors: Vec<usize> = (0..size).filter(|m| !cone.contains(*m)).collect();
        for m in &divisors {
            if kinds[*m] == kind && tally.all(&want, |k| sig[*m][pos[k]]) {
                push(gain0 - kept(&[*m]), Spec::Node(*m));
            }
        }
        if max_new < 1 {
            tally.settle(meter)?;
            continue;
        }
        let ints: Vec<usize> = divisors
            .iter()
            .copied()
            .filter(|m| kinds[*m] == Kind::Int)
            .collect();
        let bools: Vec<usize> = divisors
            .iter()
            .copied()
            .filter(|m| kinds[*m] == Kind::Bool)
            .collect();
        if !tally.add(divisors.len().saturating_mul(pos.len())) {
            return Err(meter.exhausted());
        }
        let mut table: BTreeMap<(Kind, u64), Vec<usize>> = BTreeMap::new();
        for m in &divisors {
            let key = fingerprint(FNV_BASIS, pos.iter().map(|p| sig[*m][*p]));
            table.entry((kinds[*m], key)).or_default().push(*m);
        }
        let lookup = |tally: &mut Tally, kind: Kind, values: &[i128]| -> Option<usize> {
            let key = fingerprint(FNV_BASIS, values.iter().copied());
            table
                .get(&(kind, key))?
                .iter()
                .copied()
                .find(|m| tally.all(values, |k| sig[*m][pos[k]]))
        };
        if kind == Kind::Bool {
            let negated: Vec<i128> = want.iter().map(|value| i128::from(*value == 0)).collect();
            if let Some(m) = lookup(&mut tally, Kind::Bool, &negated) {
                push(
                    gain0 - 1 - kept(&[m]),
                    Spec::Op(Code::Not, vec![Spec::Node(m)]),
                );
            }
            for (i, x) in bools.iter().enumerate() {
                for y in &bools[i + 1..] {
                    let both = |k: usize| i128::from(sig[*x][pos[k]] == 1 && sig[*y][pos[k]] == 1);
                    if tally.all(&want, both) {
                        push(
                            gain0 - 1 - kept(&[*x, *y]),
                            Spec::Op(Code::And, vec![Spec::Node(*x), Spec::Node(*y)]),
                        );
                    }
                }
            }
            for x in &ints {
                for value in &constants {
                    for code in [Code::Eq, Code::Lt] {
                        for node_first in [true, false] {
                            let got = |k: usize| {
                                let a = sig[*x][pos[k]];
                                if node_first {
                                    compare(code, a, *value)
                                } else {
                                    compare(code, *value, a)
                                }
                            };
                            if tally.all(&want, got) {
                                let pair = if node_first {
                                    vec![Spec::Node(*x), Spec::Const(Kind::Int, *value)]
                                } else {
                                    vec![Spec::Const(Kind::Int, *value), Spec::Node(*x)]
                                };
                                push(gain0 - 2 - kept(&[*x]), Spec::Op(code, pair));
                            }
                        }
                    }
                }
            }
            for x in &ints {
                for y in &ints {
                    if x == y {
                        continue;
                    }
                    for code in [Code::Eq, Code::Lt] {
                        if code == Code::Eq && x > y {
                            continue;
                        }
                        let got = |k: usize| compare(code, sig[*x][pos[k]], sig[*y][pos[k]]);
                        if tally.all(&want, got) {
                            push(
                                gain0 - 1 - kept(&[*x, *y]),
                                Spec::Op(code, vec![Spec::Node(*x), Spec::Node(*y)]),
                            );
                        }
                    }
                }
            }
        } else {
            for x in &ints {
                for code in [Code::Add, Code::Sub] {
                    if !tally.add(pos.len()) {
                        return Err(meter.exhausted());
                    }
                    let need: Option<Vec<i128>> = pos
                        .iter()
                        .zip(&want)
                        .map(|(p, w)| match code {
                            Code::Add => w.checked_sub(sig[*x][*p]),
                            _ => sig[*x][*p].checked_sub(*w),
                        })
                        .collect();
                    let Some(need) = need else {
                        continue;
                    };
                    if let Some(m) = lookup(&mut tally, Kind::Int, &need) {
                        push(
                            gain0 - 1 - kept(&[*x, m]),
                            Spec::Op(code, vec![Spec::Node(*x), Spec::Node(m)]),
                        );
                    }
                    if let Some(value) = single_value(&need) {
                        push(
                            gain0 - 2 - kept(&[*x]),
                            Spec::Op(code, vec![Spec::Node(*x), Spec::Const(Kind::Int, value)]),
                        );
                    }
                }
                // Complement: t = K - x, one subtraction instead of a
                // selection between mirrored branches.
                if complement {
                    if !tally.add(pos.len()) {
                        return Err(meter.exhausted());
                    }
                    let sum: Option<Vec<i128>> = pos
                        .iter()
                        .zip(&want)
                        .map(|(p, w)| w.checked_add(sig[*x][*p]))
                        .collect();
                    if let Some(value) = sum.as_deref().and_then(single_value) {
                        push(
                            gain0 - 2 - kept(&[*x]),
                            Spec::Op(
                                Code::Sub,
                                vec![Spec::Const(Kind::Int, value), Spec::Node(*x)],
                            ),
                        );
                    }
                }
            }
        }
        // Select over existing divisors: t = c ? x : y.
        let same_kind: &[usize] = if kind == Kind::Int { &ints } else { &bools };
        for c in &bools {
            if !tally.add(pos.len()) {
                return Err(meter.exhausted());
            }
            let (on, off): (Vec<usize>, Vec<usize>) =
                (0..pos.len()).partition(|k| sig[*c][pos[*k]] == 1);
            if on.is_empty() || off.is_empty() {
                continue;
            }
            let part = |tally: &mut Tally, side: &[usize]| -> Option<Spec> {
                let wanted: Vec<i128> = side.iter().map(|k| want[*k]).collect();
                if let Some(value) = single_value(&wanted) {
                    return Some(Spec::Const(kind, value));
                }
                same_kind
                    .iter()
                    .copied()
                    .find(|m| tally.all(&wanted, |j| sig[*m][pos[side[j]]]))
                    .map(Spec::Node)
            };
            let Some(x) = part(&mut tally, &on) else {
                continue;
            };
            let Some(y) = part(&mut tally, &off) else {
                continue;
            };
            let constants_added = [&x, &y]
                .iter()
                .filter(|spec| matches!(spec, Spec::Const(..)))
                .count() as i128;
            let mut used = vec![*c];
            for spec in [&x, &y] {
                if let Spec::Node(m) = spec {
                    used.push(*m);
                }
            }
            push(
                gain0 - 1 - constants_added - kept(&used),
                Spec::Op(Code::Select, vec![Spec::Node(*c), x, y]),
            );
        }
        tally.settle(meter)?;
    }
    Ok(out)
}

/// Builds `graph` with `node` replaced by `spec`. Divisors may come after
/// `node` in the old order (they are outside its fanout cone), so nodes are
/// built on demand rather than in index order.
pub(crate) fn rebuild(graph: &Graph, node: usize, spec: &Spec) -> Graph {
    struct Builder<'a> {
        graph: &'a Graph,
        node: usize,
        spec: &'a Spec,
        pool: Pool,
        ids: Vec<Option<usize>>,
    }
    impl Builder<'_> {
        fn build(&mut self, index: usize) -> usize {
            if let Some(Some(id)) = self.ids.get(index) {
                return *id;
            }
            let id = if index == self.node {
                let spec = self.spec;
                self.materialize(spec)
            } else {
                let graph = self.graph;
                match graph.nodes.get(index) {
                    Some(node) => {
                        let renamed = node.map(|operand| self.build(operand));
                        self.pool.add(renamed)
                    }
                    None => usize::MAX,
                }
            };
            if let Some(slot) = self.ids.get_mut(index) {
                *slot = Some(id);
            }
            id
        }

        fn materialize(&mut self, spec: &Spec) -> usize {
            match spec {
                Spec::Node(m) => self.build(*m),
                Spec::Const(kind, value) => self.pool.constant(*kind, *value),
                Spec::Op(code, args) => {
                    let ids: Vec<usize> = args.iter().map(|arg| self.materialize(arg)).collect();
                    let node = match (code, ids.as_slice()) {
                        (Code::Add, [a, b]) => Node::Add(*a, *b),
                        (Code::Sub, [a, b]) => Node::Sub(*a, *b),
                        (Code::Eq, [a, b]) => Node::Eq(*a, *b),
                        (Code::Lt, [a, b]) => Node::Lt(*a, *b),
                        (Code::And, [a, b]) => Node::And(*a, *b),
                        (Code::Not, [a]) => Node::Not(*a),
                        (Code::Select, [c, a, b]) => Node::Select(*c, *a, *b),
                        _ => return usize::MAX,
                    };
                    self.pool.add(node)
                }
            }
        }
    }
    let mut builder = Builder {
        graph,
        node,
        spec,
        pool: Pool::new(&graph.inputs),
        ids: vec![None; graph.nodes.len()],
    };
    let roots: Vec<usize> = graph
        .roots
        .iter()
        .map(|root| builder.build(*root))
        .collect();
    builder.pool.emit(&graph.outputs, &roots)
}

/// Greedy best-gain resubstitution until no proposal shrinks the program or
/// the round budget is spent. Returns the last accepted graph and the verdict
/// on every rewrite the step check refused.
pub(crate) fn resubstitute(
    seed: &Graph,
    env: &Env<'_>,
    meter: &mut Meter,
) -> Result<(Graph, Vec<Verdict>), Stop> {
    let mut current = canonical(seed);
    let mut refused = Vec::new();
    let Some((_, mut current_cost)) = admit(&current) else {
        return Ok((current, refused));
    };
    let settings = env.settings;
    let verify_steps = env.space.count() as u64 <= settings.budget.step_verify_tuples;
    let mut rejected: BTreeSet<Vec<u8>> = BTreeSet::new();
    for _ in 0..settings.budget.resub_rounds {
        let Some(sig) =
            env.reference
                .signatures(&current, env.space, settings.budget.max_cells, meter)?
        else {
            break;
        };
        let mut proposals = candidates(
            &current,
            &sig,
            settings.shape.max_new,
            settings.proposers.complement,
            meter,
        )?;
        proposals.retain(|proposal| proposal.gain > 0);
        proposals.sort_by_key(|proposal| Reverse(proposal.gain));
        let mut improved = false;
        for proposal in proposals.iter().take(settings.shape.resub_choices) {
            let candidate = canonical(&rebuild(&current, proposal.node, &proposal.spec));
            let Some(((program, bytes), cost)) = admit(&candidate) else {
                continue;
            };
            if cost >= current_cost || rejected.contains(&bytes) {
                continue;
            }
            if verify_steps {
                let verdict = env.reference.compare(&program, env.space, None, meter)?;
                if verdict != Verdict::Equal {
                    rejected.insert(bytes);
                    refused.push(verdict);
                    continue;
                }
            }
            current = candidate;
            current_cost = cost;
            improved = true;
            break;
        }
        if !improved {
            break;
        }
    }
    Ok((current, refused))
}
