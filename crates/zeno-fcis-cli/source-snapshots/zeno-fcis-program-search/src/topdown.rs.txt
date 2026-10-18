//! Top-down synthesis of each output afresh: case splits on input predicates,
//! arithmetic inversion (`t = x - r` means synthesize `r`), equality inversion
//! and two-valued selection, all over the tuples being considered.
//!
//! Above `cegis_tuples` live tuples the synthesis is counterexample-guided: it
//! runs on a seeded, stratified sample, the candidate is evaluated on every
//! live tuple by the library evaluator, and disagreeing tuples join the
//! sample for the next round. Plans are untrusted; the portfolio checks every
//! result on the complete domain before admitting it.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use crate::ir::{Graph, Kind, Node, domain_kind};
use crate::pool::{Pool, canonical};
use crate::space::Meter;
use crate::util::{
    FNV_BASIS, FNV_BASIS_2, Rng, distinct_count, distinct_up_to, fingerprint, single_value,
};
use crate::{Env, MAX_DEPTH, Stop};

/// One plan: a node over arena ids, its value on every considered tuple,
/// its size as a shared DAG, and the inputs it reads (as a bit mask).
struct Entry {
    node: Node,
    value: Vec<i128>,
    cost: usize,
    inputs: u64,
}

type MemoKey = (Kind, u32, u64, u64, u64, usize);

struct TopDown<'a> {
    original: &'a Graph,
    columns: Vec<Vec<i128>>,
    size: usize,
    depth: u32,
    width: usize,
    solve_limit: u32,
    two_valued: bool,
    max_cells: u64,
    cells: u128,
    arena: Vec<Entry>,
    index: BTreeMap<Node, usize>,
    memo: BTreeMap<MemoKey, Option<usize>>,
    atoms_int: Vec<usize>,
    atoms_bool: Vec<usize>,
    work: u32,
}

/// Most values an input may take before its point predicates are skipped.
const PREDICATE_DOMAIN: u64 = 64;

impl<'a> TopDown<'a> {
    fn new(
        original: &'a Graph,
        ordinals: &[u32],
        depth: u32,
        env: &Env<'_>,
        meter: &mut Meter,
    ) -> Result<Self, Stop> {
        let shape = &env.settings.shape;
        let columns = (0..original.inputs.len())
            .map(|input| env.space.column(input, ordinals))
            .collect();
        let mut search = Self {
            original,
            columns,
            size: ordinals.len(),
            depth,
            width: shape.width,
            solve_limit: shape.solve_limit,
            two_valued: env.settings.proposers.two_valued,
            max_cells: env.settings.budget.max_cells,
            cells: 0,
            arena: Vec::new(),
            index: BTreeMap::new(),
            memo: BTreeMap::new(),
            atoms_int: Vec::new(),
            atoms_bool: Vec::new(),
            work: 0,
        };
        search.atoms(meter)?;
        Ok(search)
    }

    /// Inputs, then point and threshold predicates on small integer inputs,
    /// then comparisons between integer inputs.
    fn atoms(&mut self, meter: &mut Meter) -> Result<(), Stop> {
        let inputs = self.original.inputs.clone();
        let mut ints = Vec::new();
        for (k, domain) in inputs.iter().enumerate() {
            let Ok(position) = u16::try_from(k) else {
                continue;
            };
            let Some(id) = self.make(Node::Input(position), meter)? else {
                continue;
            };
            match domain_kind(*domain) {
                Some(Kind::Int) => {
                    self.atoms_int.push(id);
                    ints.push(id);
                }
                Some(Kind::Bool) => self.atoms_bool.push(id),
                None => {}
            }
        }
        // Keep the source's arithmetic vocabulary available as exact atoms.
        // This is bounded by the source graph and the existing cells/work
        // budgets, rather than a Cartesian product of six-factor candidates.
        if self.original.nodes.iter().any(|node| {
            matches!(
                node,
                Node::Mul(..)
                    | Node::DivFloor(..)
                    | Node::DivCeil(..)
                    | Node::MulDivFloor(..)
                    | Node::MulDivCeil(..)
                    | Node::ProductsGe(..)
            )
        }) {
            let kinds = self.original.kinds();
            let mut ids: Vec<Option<usize>> = Vec::with_capacity(self.original.nodes.len());
            let original_nodes = self.original.nodes.clone();
            for (k, node) in original_nodes.into_iter().enumerate() {
                let (operands, count) = node.operands();
                if operands[..count]
                    .iter()
                    .any(|id| ids.get(*id).copied().flatten().is_none())
                {
                    ids.push(None);
                    continue;
                }
                let renamed = node.map(|id| ids[id].unwrap_or(usize::MAX));
                let id = self.make(renamed, meter)?;
                ids.push(id);
                if matches!(
                    node,
                    Node::Mul(..)
                        | Node::DivFloor(..)
                        | Node::DivCeil(..)
                        | Node::MulDivFloor(..)
                        | Node::MulDivCeil(..)
                        | Node::ProductsGe(..)
                ) {
                    if let Some(id) = id {
                        if kinds[k] == Kind::Bool {
                            self.atoms_bool.push(id);
                        } else {
                            self.atoms_int.push(id);
                        }
                    }
                }
            }
        }
        let mut predicates = Vec::new();
        for (k, domain) in inputs.iter().enumerate() {
            let (min, max) = domain.bounds();
            let Some(span) = max.checked_sub(min).and_then(|span| span.checked_add(1)) else {
                continue;
            };
            if domain_kind(*domain) != Some(Kind::Int) || span > i128::from(PREDICATE_DOMAIN) {
                continue;
            }
            let Ok(position) = u16::try_from(k) else {
                continue;
            };
            let Some(input) = self.make(Node::Input(position), meter)? else {
                continue;
            };
            for value in min..=max {
                predicates.push((input, value, true));
            }
            if min < max {
                for value in min + 1..=max {
                    predicates.push((input, value, false));
                }
            }
        }
        for (input, value, equal) in predicates {
            let Some(constant) = self.make(Node::Int(value), meter)? else {
                continue;
            };
            let node = if equal {
                Node::Eq(input, constant)
            } else {
                Node::Lt(input, constant)
            };
            if let Some(id) = self.make(node, meter)? {
                self.atoms_bool.push(id);
            }
        }
        for (i, a) in ints.iter().enumerate() {
            for b in &ints[i + 1..] {
                for node in [Node::Eq(*a, *b), Node::Lt(*a, *b), Node::Lt(*b, *a)] {
                    if let Some(id) = self.make(node, meter)? {
                        self.atoms_bool.push(id);
                    }
                }
            }
        }
        Ok(())
    }

    /// Interns `node` (operands are arena ids). `None` when an addition or
    /// subtraction overflows on a considered tuple: under eager evaluation
    /// that plan would trap there.
    fn make(&mut self, node: Node, meter: &mut Meter) -> Result<Option<usize>, Stop> {
        if let Some(id) = self.index.get(&node) {
            return Ok(Some(*id));
        }
        let size = self.size;
        // Reserve the library probe's work before evaluating, including on
        // refusal. Old operators retain their existing work accounting.
        let extra = if matches!(
            node,
            Node::Mul(..)
                | Node::DivFloor(..)
                | Node::DivCeil(..)
                | Node::MulDivFloor(..)
                | Node::MulDivCeil(..)
                | Node::ProductsGe(..)
        ) {
            node.operands().1.saturating_add(1)
        } else {
            0
        };
        meter.charge(size.max(1).saturating_mul(extra))?;
        let value_of = |id: usize| self.arena.get(id).map(|entry| &entry.value);
        let pair = |a: usize, b: usize| Some((value_of(a)?, value_of(b)?));
        let value: Option<Vec<i128>> = match node {
            Node::Input(k) => self.columns.get(usize::from(k)).cloned(),
            Node::Int(value) => Some(vec![value; size]),
            Node::Bool(value) => Some(vec![i128::from(value); size]),
            Node::Add(a, b) => pair(a, b)
                .and_then(|(x, y)| x.iter().zip(y).map(|(p, q)| p.checked_add(*q)).collect()),
            Node::Sub(a, b) => pair(a, b)
                .and_then(|(x, y)| x.iter().zip(y).map(|(p, q)| p.checked_sub(*q)).collect()),
            Node::Mul(..)
            | Node::DivFloor(..)
            | Node::DivCeil(..)
            | Node::MulDivFloor(..)
            | Node::MulDivCeil(..)
            | Node::ProductsGe(..) => {
                let (operands, count) = node.operands();
                let operands = &operands[..count];
                let compute = || {
                    let columns = operands
                        .iter()
                        .map(|id| value_of(*id).map(Vec::as_slice))
                        .collect::<Option<Vec<_>>>()?;
                    let renamed =
                        node.map(|id| operands.iter().position(|v| *v == id).unwrap_or(usize::MAX));
                    crate::arithmetic::evaluate(renamed.library()?, &columns)
                };
                compute()
            }
            Node::Eq(a, b) => {
                pair(a, b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| i128::from(p == q)).collect())
            }
            Node::Lt(a, b) => {
                pair(a, b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| i128::from(p < q)).collect())
            }
            Node::And(a, b) => pair(a, b).map(|(x, y)| {
                x.iter()
                    .zip(y)
                    .map(|(p, q)| i128::from(*p == 1 && *q == 1))
                    .collect()
            }),
            Node::Not(a) => value_of(a).map(|x| x.iter().map(|p| i128::from(*p == 0)).collect()),
            Node::Select(c, a, b) => match (value_of(c), pair(a, b)) {
                (Some(cond), Some((x, y))) => Some(
                    cond.iter()
                        .zip(x.iter().zip(y))
                        .map(|(c, (p, q))| if *c == 1 { *p } else { *q })
                        .collect(),
                ),
                _ => None,
            },
        };
        meter.charge(size.max(1))?;
        let Some(value) = value else {
            return Ok(None);
        };
        self.cells = self.cells.saturating_add(size as u128);
        if self.cells > u128::from(self.max_cells) {
            return Err(Stop::CellsOverBudget {
                cells: self.cells,
                limit: self.max_cells,
            });
        }
        let (operands, count) = node.operands();
        let mut below: BTreeSet<usize> = BTreeSet::new();
        let mut stack: Vec<usize> = operands[..count].to_vec();
        while let Some(id) = stack.pop() {
            if below.insert(id)
                && let Some(entry) = self.arena.get(id)
            {
                let (more, used) = entry.node.operands();
                stack.extend_from_slice(&more[..used]);
            }
        }
        let inputs = match node {
            Node::Input(k) if k < 64 => 1u64 << k,
            _ => operands[..count]
                .iter()
                .filter_map(|id| self.arena.get(*id))
                .fold(0, |mask, entry| mask | entry.inputs),
        };
        let id = self.arena.len();
        self.arena.push(Entry {
            node,
            value,
            cost: 1 + below.len(),
            inputs,
        });
        self.index.insert(node, id);
        Ok(Some(id))
    }

    fn constant(
        &mut self,
        kind: Kind,
        value: i128,
        meter: &mut Meter,
    ) -> Result<Option<usize>, Stop> {
        match kind {
            Kind::Int => self.make(Node::Int(value), meter),
            Kind::Bool => self.make(Node::Bool(value != 0), meter),
        }
    }

    fn cost(&self, id: usize) -> usize {
        self.arena.get(id).map_or(usize::MAX, |entry| entry.cost)
    }

    fn allowed(&self, id: usize, exclude: u64) -> bool {
        self.arena
            .get(id)
            .is_some_and(|entry| entry.inputs & exclude == 0)
    }

    fn value(&self, id: usize, position: u32) -> i128 {
        self.arena
            .get(id)
            .and_then(|entry| entry.value.get(position as usize))
            .copied()
            .unwrap_or(i128::MIN)
    }

    fn matches(&self, id: usize, pos: &[u32], want: &[i128], negate: bool) -> bool {
        pos.iter().zip(want).all(|(p, w)| {
            let value = self.value(id, *p);
            let value = if negate {
                i128::from(value == 0)
            } else {
                value
            };
            value == *w
        })
    }

    fn offer(&self, best: &mut Option<usize>, plan: Option<usize>) {
        if let Some(plan) = plan
            && best.is_none_or(|current| self.cost(plan) < self.cost(current))
        {
            *best = Some(plan);
        }
    }

    /// The cheapest plan found whose value equals `want` at `pos`; cost counts
    /// distinct subplans, so shared structure is paid once.
    fn solve(
        &mut self,
        pos: &[u32],
        want: &[i128],
        kind: Kind,
        depth: u32,
        exclude: u64,
        meter: &mut Meter,
    ) -> Result<Option<usize>, Stop> {
        let positions = || {
            pos.iter()
                .map(|p| i128::from(*p))
                .chain(want.iter().copied())
        };
        let key = (
            kind,
            depth,
            exclude,
            fingerprint(FNV_BASIS, positions()),
            fingerprint(FNV_BASIS_2, positions()),
            pos.len(),
        );
        if let Some(found) = self.memo.get(&key) {
            return Ok(*found);
        }
        self.memo.insert(key, None); // cycle guard
        self.work = self.work.saturating_add(1);
        // Over the solve budget: constants and atoms only from here on.
        let depth = if self.work > self.solve_limit {
            0
        } else {
            depth
        };
        meter.charge(1 + pos.len())?;
        let mut best = None;
        if let Some(value) = single_value(want) {
            let plan = self.constant(kind, value, meter)?;
            self.offer(&mut best, plan);
        } else {
            let atoms = match kind {
                Kind::Int => self.atoms_int.clone(),
                Kind::Bool => self.atoms_bool.clone(),
            };
            meter.charge(atoms.len().saturating_mul(2))?;
            for atom in &atoms {
                if self.allowed(*atom, exclude) && self.matches(*atom, pos, want, false) {
                    self.offer(&mut best, Some(*atom));
                }
            }
            if kind == Kind::Bool {
                for atom in &atoms {
                    if self.allowed(*atom, exclude) && self.matches(*atom, pos, want, true) {
                        let plan = self.make(Node::Not(*atom), meter)?;
                        self.offer(&mut best, plan);
                    }
                }
            }
            if best.is_none() && depth > 0 {
                if kind == Kind::Int {
                    self.invert(pos, want, depth, exclude, &mut best, meter)?;
                    if self.two_valued
                        && let Some(values) = distinct_up_to(want, 2)
                        && let [a, b] = values.as_slice()
                    {
                        self.select_two(pos, want, (*a, *b), depth, exclude, &mut best, meter)?;
                    }
                } else {
                    self.invert_equality(pos, want, depth, exclude, &mut best, meter)?;
                }
                self.split(pos, want, kind, depth, exclude, &mut best, meter)?;
            }
        }
        self.memo.insert(key, best);
        Ok(best)
    }

    /// `t` takes exactly two values: synthesize the Boolean `t == a` and select.
    #[allow(clippy::too_many_arguments)]
    fn select_two(
        &mut self,
        pos: &[u32],
        want: &[i128],
        (a, b): (i128, i128),
        depth: u32,
        exclude: u64,
        best: &mut Option<usize>,
        meter: &mut Meter,
    ) -> Result<(), Stop> {
        for (first, second) in [(a, b), (b, a)] {
            let condition: Vec<i128> = want.iter().map(|v| i128::from(*v == first)).collect();
            let Some(cond) = self.solve(pos, &condition, Kind::Bool, depth - 1, exclude, meter)?
            else {
                continue;
            };
            let (Some(x), Some(y)) = (
                self.make(Node::Int(first), meter)?,
                self.make(Node::Int(second), meter)?,
            ) else {
                continue;
            };
            let plan = self.make(Node::Select(cond, x, y), meter)?;
            self.offer(best, plan);
        }
        Ok(())
    }

    /// `t = x - r` or `t = x + r`: reuse an atom equal to `r`, or synthesize
    /// `r` when it takes at most three values.
    fn invert(
        &mut self,
        pos: &[u32],
        want: &[i128],
        depth: u32,
        exclude: u64,
        best: &mut Option<usize>,
        meter: &mut Meter,
    ) -> Result<(), Stop> {
        for x in self.atoms_int.clone() {
            if !self.allowed(x, exclude) {
                continue;
            }
            for subtract in [true, false] {
                meter.charge(pos.len())?;
                let rest: Option<Vec<i128>> = pos
                    .iter()
                    .zip(want)
                    .map(|(p, w)| {
                        let xv = self.value(x, *p);
                        if subtract {
                            xv.checked_sub(*w)
                        } else {
                            w.checked_sub(xv)
                        }
                    })
                    .collect();
                let Some(rest) = rest else {
                    continue;
                };
                let build = |r: usize| {
                    if subtract {
                        Node::Sub(x, r)
                    } else {
                        Node::Add(x, r)
                    }
                };
                let direct = self
                    .atoms_int
                    .iter()
                    .copied()
                    .find(|a| self.allowed(*a, exclude) && self.matches(*a, pos, &rest, false));
                if let Some(direct) = direct {
                    let plan = self.make(build(direct), meter)?;
                    self.offer(best, plan);
                } else if distinct_up_to(&rest, 3).is_some()
                    && let Some(sub) =
                        self.solve(pos, &rest, Kind::Int, depth - 1, exclude, meter)?
                {
                    let plan = self.make(build(sub), meter)?;
                    self.offer(best, plan);
                }
            }
        }
        Ok(())
    }

    /// `t == (x == y)`: `y` must equal `x` where `t` holds and differ
    /// elsewhere, so `y` is synthesized without reading `x`.
    fn invert_equality(
        &mut self,
        pos: &[u32],
        want: &[i128],
        depth: u32,
        exclude: u64,
        best: &mut Option<usize>,
        meter: &mut Meter,
    ) -> Result<(), Stop> {
        let on: Vec<u32> = pos
            .iter()
            .zip(want)
            .filter_map(|(p, w)| (*w == 1).then_some(*p))
            .collect();
        let off: Vec<u32> = pos
            .iter()
            .zip(want)
            .filter_map(|(p, w)| (*w == 0).then_some(*p))
            .collect();
        if on.is_empty() || off.is_empty() {
            return Ok(());
        }
        for x in self.atoms_int.clone() {
            if !self.allowed(x, exclude) {
                continue;
            }
            let reads = self.arena.get(x).map_or(0, |entry| entry.inputs);
            let target: Vec<i128> = on.iter().map(|p| self.value(x, *p)).collect();
            let Some(y) = self.solve(&on, &target, Kind::Int, depth - 1, exclude | reads, meter)?
            else {
                continue;
            };
            if off.iter().all(|p| self.value(y, *p) != self.value(x, *p)) {
                let plan = self.make(Node::Eq(x, y), meter)?;
                self.offer(best, plan);
            }
        }
        Ok(())
    }

    /// Case split on the `width` predicates that leave the fewest distinct
    /// target values on their two sides.
    #[allow(clippy::too_many_arguments)]
    fn split(
        &mut self,
        pos: &[u32],
        want: &[i128],
        kind: Kind,
        depth: u32,
        exclude: u64,
        best: &mut Option<usize>,
        meter: &mut Meter,
    ) -> Result<(), Stop> {
        let parts = |search: &Self, predicate: usize| {
            let mut on = (Vec::new(), Vec::new());
            let mut off = (Vec::new(), Vec::new());
            for (p, w) in pos.iter().zip(want) {
                let side = if search.value(predicate, *p) == 1 {
                    &mut on
                } else {
                    &mut off
                };
                side.0.push(*p);
                side.1.push(*w);
            }
            (on, off)
        };
        let mut scored = Vec::new();
        for predicate in self.atoms_bool.clone() {
            if !self.allowed(predicate, exclude) {
                continue;
            }
            meter.charge(pos.len())?;
            let (on, off) = parts(self, predicate);
            if on.0.is_empty() || off.0.is_empty() {
                continue;
            }
            let score = distinct_count(&on.1) + distinct_count(&off.1);
            scored.push((score, self.cost(predicate), predicate));
        }
        scored.sort_by_key(|(score, cost, _)| (*score, *cost));
        for (_, _, predicate) in scored.into_iter().take(self.width) {
            let (on, off) = parts(self, predicate);
            let Some(yes) = self.solve(&on.0, &on.1, kind, depth - 1, exclude, meter)? else {
                continue;
            };
            let Some(no) = self.solve(&off.0, &off.1, kind, depth - 1, exclude, meter)? else {
                continue;
            };
            let plan = self.make(Node::Select(predicate, yes, no), meter)?;
            self.offer(best, plan);
        }
        Ok(())
    }

    /// Fresh plans per output; an output with no plan keeps its original cone.
    fn program_for(
        &mut self,
        targets: &[(Vec<i128>, Kind)],
        meter: &mut Meter,
    ) -> Result<Option<Graph>, Stop> {
        let all: Vec<u32> = (0..self.size)
            .map(|position| u32::try_from(position).unwrap_or(u32::MAX))
            .collect();
        let mut plans = Vec::with_capacity(targets.len());
        for (target, kind) in targets {
            self.work = 0; // each output gets its own solve budget
            plans.push(self.solve(&all, target, *kind, self.depth, 0, meter)?);
        }
        if plans.iter().all(Option::is_none) {
            return Ok(None);
        }
        let mut pool = Pool::new(&self.original.inputs);
        let original = pool.load(self.original);
        let mut roots = Vec::with_capacity(plans.len());
        for (plan, root) in plans.iter().zip(&self.original.roots) {
            roots.push(match plan {
                Some(plan) => self.put(&mut pool, *plan),
                None => original.get(*root).copied().unwrap_or(usize::MAX),
            });
        }
        Ok(Some(canonical(&pool.emit(&self.original.outputs, &roots))))
    }

    fn put(&self, pool: &mut Pool, id: usize) -> usize {
        match self.arena.get(id) {
            Some(entry) => {
                let node = entry.node.map(|operand| self.put(pool, operand));
                pool.add(node)
            }
            None => usize::MAX,
        }
    }
}

/// Top-down synthesis of every output of `original`, whose node values on
/// the live tuples are `sig`. Depth adapts to the number of tuples.
pub(crate) fn top_down(
    original: &Graph,
    sig: &[Vec<i128>],
    env: &Env<'_>,
    meter: &mut Meter,
) -> Result<Option<Graph>, Stop> {
    let live = env.reference.live();
    let n = live.len();
    if n == 0 {
        return Ok(None);
    }
    let kinds = original.kinds();
    let targets: Vec<(Vec<i128>, Kind)> = original
        .roots
        .iter()
        .map(|root| (sig[*root].clone(), kinds[*root]))
        .collect();
    let shape = &env.settings.shape;
    if n > shape.cegis_tuples {
        return counterexample_guided(original, &targets, env, meter);
    }
    let depth = shape
        .depth
        .unwrap_or(if n <= 256 {
            8
        } else if n <= 4096 {
            6
        } else {
            5
        })
        .min(MAX_DEPTH);
    let mut search = TopDown::new(original, live, depth, env, meter)?;
    search.program_for(&targets, meter)
}

fn counterexample_guided(
    original: &Graph,
    targets: &[(Vec<i128>, Kind)],
    env: &Env<'_>,
    meter: &mut Meter,
) -> Result<Option<Graph>, Stop> {
    let shape = &env.settings.shape;
    let live = env.reference.live();
    let positions: Vec<u32> = (0..live.len())
        .map(|position| u32::try_from(position).unwrap_or(u32::MAX))
        .collect();
    let mut rng = Rng::new(shape.seed);
    let mut chosen: BTreeSet<u32> = rng.sample(&positions, shape.sample).into_iter().collect();
    // Stratified: rare output values are well represented, so spurious split
    // predicates cannot mimic the real one.
    let per_value = (shape.sample / 8).max(64);
    for (target, _) in targets {
        meter.charge(positions.len())?;
        let mut groups: BTreeMap<i128, Vec<u32>> = BTreeMap::new();
        let mut too_many = false;
        for position in &positions {
            let value = target[*position as usize];
            if !groups.contains_key(&value) && groups.len() == 256 {
                too_many = true;
                break;
            }
            groups.entry(value).or_default().push(*position);
        }
        if too_many {
            continue;
        }
        for members in groups.values() {
            chosen.extend(rng.sample(members, per_value));
        }
    }
    for _ in 0..shape.cegis_rounds {
        let sample: Vec<u32> = chosen.iter().copied().collect();
        let ordinals: Vec<u32> = sample.iter().map(|p| live[*p as usize]).collect();
        let depth = shape
            .depth
            .unwrap_or(if sample.len() <= 4096 { 6 } else { 5 })
            .min(MAX_DEPTH);
        let local: Vec<(Vec<i128>, Kind)> = targets
            .iter()
            .map(|(values, kind)| (sample.iter().map(|p| values[*p as usize]).collect(), *kind))
            .collect();
        let mut search = TopDown::new(original, &ordinals, depth, env, meter)?;
        let Some(candidate) = search.program_for(&local, meter)? else {
            return Ok(None);
        };
        let Some(program) = candidate.to_program() else {
            return Ok(None);
        };
        let wrong = env.reference.differing(&program, env.space, live, meter)?;
        if wrong.is_empty() {
            return Ok(Some(candidate));
        }
        let wrong: Vec<u32> = wrong
            .into_iter()
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect();
        chosen.extend(rng.sample(&wrong, (shape.sample / 4).max(64)));
    }
    Ok(None)
}

/// One care-masked solve of an arbitrary target, for the public facade in
/// `target.rs`. `columns` hold each input's value on every considered tuple
/// (`size` of them); `pos` lists the cared-for positions and `want` the
/// target there. The atoms and the search are exactly those of [`top_down`];
/// the result is a one-output graph whose output domain is the kind's full
/// range, or `None` when no plan was found. A plan is untrusted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_target(
    inputs: &[zeno_fcis_synthesis::finite::Domain],
    columns: Vec<Vec<i128>>,
    size: usize,
    pos: &[u32],
    want: &[i128],
    kind: Kind,
    depth: u32,
    width: usize,
    solve_limit: u32,
    max_cells: u64,
    meter: &mut Meter,
) -> Result<Option<Graph>, Stop> {
    let shell = Graph {
        inputs: inputs.to_vec(),
        outputs: Vec::new(),
        nodes: Vec::new(),
        roots: Vec::new(),
    };
    let mut search = TopDown {
        original: &shell,
        columns,
        size,
        depth: depth.min(MAX_DEPTH),
        width,
        solve_limit,
        two_valued: true,
        max_cells,
        cells: 0,
        arena: Vec::new(),
        index: BTreeMap::new(),
        memo: BTreeMap::new(),
        atoms_int: Vec::new(),
        atoms_bool: Vec::new(),
        work: 0,
    };
    search.atoms(meter)?;
    let Some(plan) = search.solve(pos, want, kind, search.depth, 0, meter)? else {
        return Ok(None);
    };
    let output = match kind {
        Kind::Bool => zeno_fcis_synthesis::finite::Domain::Bool,
        Kind::Int => zeno_fcis_synthesis::finite::Domain::Int {
            min: i128::MIN,
            max: i128::MAX,
        },
    };
    let mut pool = Pool::new(inputs);
    let root = search.put(&mut pool, plan);
    Ok(Some(pool.emit(&[output], &[root])))
}
