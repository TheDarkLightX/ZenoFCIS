//! Reduced decision-diagram resynthesis over input variable orders.
//!
//! Each output that is Boolean or takes at most eight values is rebuilt as a
//! decision diagram over the inputs in one variable order: a cofactor that is
//! constant becomes a literal, one that equals an input becomes that input,
//! and otherwise the next variable splits it. A Boolean variable selects
//! between its two children; an integer variable tests equality against each
//! value except the most frequent child, which is the final arm. Outputs with
//! more values keep their original cone. Orders are tried in lexicographic
//! order up to a limit, and the smallest results are kept.
//!
//! Every declared tuple is a row, including invalid business states. The
//! proposer runs only when the original refuses no tuple with an arithmetic
//! trap (a diagram never traps), and on small domains. Its results are
//! untrusted and checked like every other proposal.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use crate::ir::{Graph, Kind, Node, domain_kind};
use crate::pool::{Pool, canonical};
use crate::space::Meter;
use crate::{Env, Stop, admit};

/// Most values an output may take and still be resynthesized.
const RESYNTHESIZED_VALUES: usize = 8;
/// Most terms one order's pool may hold (the profile's instruction bound).
const POOL_TERMS: usize = 256;

/// How the decision-diagram proposer went.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagramReport {
    /// Why the proposer did not run, if it did not.
    pub skipped: Option<&'static str>,
    /// Variable orders started.
    pub orders_tried: u32,
    /// Orders that produced a program.
    pub orders_completed: u32,
    /// Orders abandoned at the diagram or pool size limit.
    pub orders_over_size: u32,
    /// Outputs rebuilt as diagrams.
    pub resynthesized_outputs: Vec<usize>,
    /// Outputs whose original cone was copied.
    pub copied_outputs: Vec<usize>,
}

/// An order abandoned at a size limit.
struct OverSize;

struct Diagram<'a> {
    inputs: &'a [Vec<i128>],
    minimums: &'a [i128],
    kinds: &'a [Kind],
    order: &'a [usize],
    pool: Pool,
    cache: BTreeMap<(usize, Kind, Vec<i128>), usize>,
    built: usize,
    max_built: usize,
}

impl Diagram<'_> {
    fn add(&mut self, node: Node) -> Result<usize, OverSize> {
        let id = self.pool.add(node);
        if self.pool.len() > POOL_TERMS {
            return Err(OverSize);
        }
        Ok(id)
    }

    /// `condition ? yes : no` with the Boolean simplifications.
    fn select(&mut self, condition: usize, yes: usize, no: usize) -> Result<usize, OverSize> {
        if yes == no {
            return Ok(yes);
        }
        match (self.pool.term(yes), self.pool.term(no)) {
            (Some(Node::Bool(true)), Some(Node::Bool(_))) => Ok(condition),
            (Some(Node::Bool(false)), Some(Node::Bool(_))) => self.add(Node::Not(condition)),
            (_, Some(Node::Bool(false))) => self.add(Node::And(condition, yes)),
            (Some(Node::Bool(false)), _) => {
                let negated = self.add(Node::Not(condition))?;
                self.add(Node::And(negated, no))
            }
            _ => self.add(Node::Select(condition, yes, no)),
        }
    }

    /// `input == value ? child : rest`.
    fn equal_or(
        &mut self,
        input: usize,
        value: i128,
        child: usize,
        rest: usize,
    ) -> Result<usize, OverSize> {
        let constant = self.add(Node::Int(value))?;
        let atom = self.add(Node::Eq(input, constant))?;
        self.select(atom, child, rest)
    }

    fn build(
        &mut self,
        rows: &[u32],
        depth: usize,
        values: &[i128],
        kind: Kind,
        meter: &mut Meter,
    ) -> Result<Result<usize, OverSize>, Stop> {
        meter.charge(5 * rows.len() + 10)?;
        let local: Vec<i128> = rows.iter().map(|row| values[*row as usize]).collect();
        let key = (depth, kind, local);
        if let Some(id) = self.cache.get(&key) {
            return Ok(Ok(*id));
        }
        self.built += 1;
        if self.built > self.max_built {
            return Ok(Err(OverSize));
        }
        let local = &key.2;
        let result = match local.first() {
            Some(first) if local.iter().all(|value| value == first) => {
                let node = match kind {
                    Kind::Bool => Node::Bool(*first != 0),
                    Kind::Int => Node::Int(*first),
                };
                self.add(node)
            }
            _ => match self.nonconstant(rows, depth, values, kind, meter)? {
                Some(result) => result,
                None => return Ok(Err(OverSize)),
            },
        };
        meter.charge(key.2.len() + 2)?;
        if let Ok(id) = result {
            self.cache.insert(key, id);
        }
        Ok(result)
    }

    /// `None` when the rows cannot be split further (never on a full domain).
    fn nonconstant(
        &mut self,
        rows: &[u32],
        depth: usize,
        values: &[i128],
        kind: Kind,
        meter: &mut Meter,
    ) -> Result<Option<Result<usize, OverSize>>, Stop> {
        for (index, column) in self.inputs.iter().enumerate() {
            meter.charge(2 * rows.len() + 2)?;
            if self.kinds[index] == kind
                && rows
                    .iter()
                    .all(|row| column[*row as usize] == values[*row as usize])
                && let Ok(position) = u16::try_from(index)
            {
                return Ok(Some(self.add(Node::Input(position))));
            }
        }
        let Some(variable) = self.order.get(depth).copied() else {
            return Ok(None);
        };
        let column = &self.inputs[variable];
        let minimum = self.minimums[variable];
        let mut groups: BTreeMap<i128, Vec<u32>> = BTreeMap::new();
        meter.charge(4 * rows.len() + 8)?;
        for row in rows {
            groups.entry(column[*row as usize]).or_default().push(*row);
        }
        let mut children = Vec::with_capacity(groups.len());
        let mut expected = minimum;
        for (value, group) in &groups {
            if *value != expected {
                return Ok(None); // a missing value: not a complete domain
            }
            expected = expected.saturating_add(1);
            match self.build(group, depth + 1, values, kind, meter)? {
                Ok(child) => children.push(child),
                Err(OverSize) => return Ok(Some(Err(OverSize))),
            }
        }
        meter.charge(10 * children.len() + 20)?;
        let Some(first) = children.first().copied() else {
            return Ok(None);
        };
        if children.iter().all(|child| *child == first) {
            return Ok(Some(Ok(first)));
        }
        let Ok(position) = u16::try_from(variable) else {
            return Ok(None);
        };
        let input = match self.add(Node::Input(position)) {
            Ok(input) => input,
            Err(OverSize) => return Ok(Some(Err(OverSize))),
        };
        if self.kinds[variable] == Kind::Bool {
            return Ok(Some(match children.as_slice() {
                [no, yes] => self.select(input, *yes, *no),
                _ => return Ok(None),
            }));
        }
        // The most frequent child (the last of equally frequent ones) is the
        // unconditional final arm.
        meter.charge(children.len() * children.len() + 4 * children.len())?;
        let count = |child: usize| children.iter().filter(|other| **other == child).count();
        let mut fallback = first;
        for child in &children {
            if count(*child) >= count(fallback) {
                fallback = *child;
            }
        }
        let mut result = fallback;
        for (offset, child) in children.iter().enumerate().rev() {
            if *child == fallback {
                continue;
            }
            match self.equal_or(input, minimum + offset as i128, *child, result) {
                Ok(next) => result = next,
                Err(OverSize) => return Ok(Some(Err(OverSize))),
            }
        }
        Ok(Some(Ok(result)))
    }
}

/// Advances `order` to the next lexicographic permutation; false at the last.
fn next_permutation(order: &mut [usize]) -> bool {
    let Some(pivot) = (1..order.len()).rev().find(|i| order[i - 1] < order[*i]) else {
        return false;
    };
    let Some(swap) = (pivot..order.len())
        .rev()
        .find(|j| order[*j] > order[pivot - 1])
    else {
        return false;
    };
    order.swap(pivot - 1, swap);
    order[pivot..].reverse();
    true
}

/// Proposals for `original`, whose node values on the live tuples are `sig`:
/// the smallest distinct programs found, smallest first.
pub(crate) fn propose(
    original: &Graph,
    sig: &[Vec<i128>],
    env: &Env<'_>,
    meter: &mut Meter,
) -> Result<(Vec<Graph>, DiagramReport), Stop> {
    let shape = &env.settings.shape;
    let mut report = DiagramReport::default();
    let live = env.reference.live();
    if live.len() != env.space.count() {
        report.skipped = Some("original-traps");
        return Ok((Vec::new(), report));
    }
    if live.len() > shape.diagram_tuples {
        report.skipped = Some("domain-over-diagram-limit");
        return Ok((Vec::new(), report));
    }
    let mut minimums = Vec::with_capacity(original.inputs.len());
    let mut input_kinds = Vec::with_capacity(original.inputs.len());
    for domain in &original.inputs {
        let (min, max) = domain.bounds();
        if !max
            .checked_sub(min)
            .and_then(|span| span.checked_add(1))
            .is_some_and(|span| span <= i128::from(shape.diagram_cardinality))
        {
            report.skipped = Some("input-cardinality-over-diagram-limit");
            return Ok((Vec::new(), report));
        }
        minimums.push(min);
        input_kinds.push(domain_kind(*domain).unwrap_or(Kind::Int));
    }
    let columns: Vec<Vec<i128>> = (0..original.inputs.len())
        .map(|input| env.space.column(input, live))
        .collect();
    let kinds = original.kinds();
    let mut resynthesized = Vec::new();
    let mut copied = Vec::new();
    for (index, root) in original.roots.iter().enumerate() {
        meter.charge(3 * live.len() + 4)?;
        let few =
            sig[*root].iter().copied().collect::<BTreeSet<i128>>().len() <= RESYNTHESIZED_VALUES;
        if kinds[*root] == Kind::Bool || few {
            resynthesized.push(index);
        } else {
            copied.push(index);
        }
    }
    report.resynthesized_outputs = resynthesized.clone();
    report.copied_outputs = copied.clone();
    // The original nodes the copied outputs need, in their original order.
    let mut needed = vec![false; original.nodes.len()];
    let mut pending: Vec<usize> = copied.iter().map(|index| original.roots[*index]).collect();
    while let Some(node) = pending.pop() {
        if !needed[node] {
            needed[node] = true;
            let (operands, count) = original.nodes[node].operands();
            pending.extend_from_slice(&operands[..count]);
        }
    }
    let rows: Vec<u32> = (0..live.len())
        .map(|row| u32::try_from(row).unwrap_or(u32::MAX))
        .collect();
    let mut order: Vec<usize> = (0..original.inputs.len()).collect();
    let mut kept: Vec<(usize, usize, Vec<u8>, Graph)> = Vec::new();
    loop {
        if report.orders_tried as usize >= shape.diagram_orders {
            break;
        }
        report.orders_tried += 1;
        meter.charge(4 * order.len() + 10)?;
        let mut diagram = Diagram {
            inputs: &columns,
            minimums: &minimums,
            kinds: &input_kinds,
            order: &order,
            pool: Pool::new(&original.inputs),
            cache: BTreeMap::new(),
            built: 0,
            max_built: shape.diagram_nodes,
        };
        let mut roots = vec![usize::MAX; original.roots.len()];
        let mut ids = vec![usize::MAX; original.nodes.len()];
        meter.charge(20 * original.nodes.len() + 10)?;
        let mut over = false;
        for (index, node) in original.nodes.iter().enumerate() {
            if needed[index] {
                let renamed = node.map(|operand| ids[operand]);
                match diagram.add(renamed) {
                    Ok(id) => ids[index] = id,
                    Err(OverSize) => over = true,
                }
            }
        }
        for index in &copied {
            roots[*index] = ids[original.roots[*index]];
        }
        for index in &resynthesized {
            if over {
                break;
            }
            let root = original.roots[*index];
            match diagram.build(&rows, 0, &sig[root], kinds[root], meter)? {
                Ok(id) => roots[*index] = id,
                Err(OverSize) => over = true,
            }
        }
        if over {
            report.orders_over_size += 1;
        } else {
            let candidate = canonical(&diagram.pool.emit(&original.outputs, &roots));
            if let Some(((_, bytes), cost)) = admit(&candidate) {
                report.orders_completed += 1;
                meter.charge(bytes.len() + 10)?;
                if !kept.iter().any(|entry| entry.2 == bytes) {
                    kept.push((cost.0, cost.1, bytes, candidate));
                    kept.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
                    kept.truncate(shape.diagram_candidates);
                }
            } else {
                report.orders_over_size += 1;
            }
        }
        if !next_permutation(&mut order) {
            break;
        }
    }
    Ok((kept.into_iter().map(|entry| entry.3).collect(), report))
}
