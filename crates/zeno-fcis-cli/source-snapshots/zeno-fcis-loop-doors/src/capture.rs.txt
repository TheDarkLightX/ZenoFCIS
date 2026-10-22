//! Proposal-side black-box capture helpers (LLM-generated).
//!
//! The table encoder, example filter, coverage and counterexample choices are
//! proposals. Only an existing full-domain checker with receipt replay can
//! decide capture. No I/O, retained scratch, clock or mutable escaping aliases.

use alloc::{collections::BTreeMap, format, vec, vec::Vec};
use zeno_fcis_synthesis::finite::{Domain, MAX_FIELDS, MAX_NODES, Op, Program};

use crate::subject::{columns, tuples};
use crate::{Refusal, Result};

/// Hard bound on complete reference acquisition and proposal enumeration.
pub const MAX_TUPLES: u64 = 4096;

/// An owned, complete, successful reference table. This is data, not a verdict.
#[derive(Clone, Debug)]
pub struct ReferenceTable {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    points: Vec<Vec<i128>>,
    rows: Vec<Vec<i128>>,
}

impl ReferenceTable {
    /// Capture a complete successful table from an immutable native reference.
    /// The caller reserves evaluation and table work before calling this pure
    /// helper. Failures are unsupported by this table profile, never omitted.
    pub fn from_program(reference: &Program, limit: u64) -> Result<Self> {
        let points = points(reference.inputs(), limit)?;
        let rows = points
            .iter()
            .map(|point| {
                reference
                    .evaluate(point)
                    .map_err(|e| Refusal::new(format!("reference-observation-unsupported:{e:?}")))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::new(
            reference.inputs().to_vec(),
            reference.outputs().to_vec(),
            rows,
        )
    }

    /// Owns all inputs and rejects partial, malformed or out-of-domain rows.
    pub fn new(inputs: Vec<Domain>, outputs: Vec<Domain>, rows: Vec<Vec<i128>>) -> Result<Self> {
        if inputs.len() > 2 * MAX_FIELDS || outputs.is_empty() || outputs.len() > MAX_FIELDS {
            return Err(Refusal::new("reference-abi-bound"));
        }
        // Library admission owns the ABI grammar and runs before enumeration.
        let nodes = outputs
            .iter()
            .map(|d| literal(*d, d.bounds().0))
            .collect::<Vec<_>>();
        let roots = (0..nodes.len())
            .map(|i| u16::try_from(i).map_err(|_| Refusal::new("reference-abi")))
            .collect::<Result<Vec<_>>>()?;
        Program::try_new(inputs.clone(), outputs.clone(), nodes, roots)
            .map_err(|e| Refusal::new(format!("reference-abi:{e:?}")))?;
        let points = points(&inputs, MAX_TUPLES)?;
        if rows.len() != points.len() {
            return Err(Refusal::new("reference-table-shape"));
        }
        if rows.iter().any(|r| {
            r.len() != outputs.len() || r.iter().zip(&outputs).any(|(v, d)| !d.contains(*v))
        }) {
            return Err(Refusal::new("reference-output-domain"));
        }
        Ok(Self {
            inputs,
            outputs,
            points,
            rows,
        })
    }

    /// ABI inputs, read-only and privately owned.
    #[must_use]
    pub fn inputs(&self) -> &[Domain] {
        &self.inputs
    }
    /// ABI outputs, read-only and privately owned.
    #[must_use]
    pub fn outputs(&self) -> &[Domain] {
        &self.outputs
    }
    /// Complete points in the existing checker's last-input-fastest order.
    #[must_use]
    pub fn points(&self) -> &[Vec<i128>] {
        &self.points
    }
    /// Successful outputs, in the same order as points.
    #[must_use]
    pub fn rows(&self) -> &[Vec<i128>] {
        &self.rows
    }

    /// Exact lookup encoding, bounded by the EXISTING checker's profile-1 ABI.
    /// A table that needs more instructions is a budget stop, not a capture.
    pub fn program(&self) -> Result<Program> {
        let mut builder = TableBuilder::default();
        let mut roots = Vec::new();
        for (j, domain) in self.outputs.iter().enumerate() {
            let values = self.rows.iter().map(|row| row[j]).collect::<Vec<_>>();
            roots.push(builder.branch(&self.inputs, 0, &values, *domain, None)?);
        }
        Program::try_new(
            self.inputs.clone(),
            self.outputs.clone(),
            builder.nodes,
            roots,
        )
        .map_err(|e| Refusal::new(format!("reference-program:{e:?}")))
    }
}

/// Enumerate only within an explicitly declared tuple budget. Nullary product = 1.
pub fn points(inputs: &[Domain], limit: u64) -> Result<Vec<Vec<i128>>> {
    if inputs.len() > 2 * MAX_FIELDS || inputs.iter().any(|d| crate::subject::domain_size(*d) == 0)
    {
        return Err(Refusal::new("reference-abi-bound"));
    }
    if tuples(inputs) > u128::from(limit.min(MAX_TUPLES)) {
        return Err(Refusal::new("reference-domain-budget"));
    }
    let cols = columns(inputs, limit.min(MAX_TUPLES))?;
    let size =
        usize::try_from(tuples(inputs)).map_err(|_| Refusal::new("reference-domain-budget"))?;
    Ok((0..size)
        .map(|i| cols.iter().map(|c| c[i]).collect())
        .collect())
}

fn literal(domain: Domain, value: i128) -> Op {
    if matches!(domain, Domain::Bool) {
        Op::Bool(value == 1)
    } else {
        Op::Int(value)
    }
}

#[derive(Default)]
struct TableBuilder {
    nodes: Vec<Op>,
    index: BTreeMap<(u8, i128, u16, u16, u16), u16>,
}

impl TableBuilder {
    fn add(&mut self, op: Op) -> Result<u16> {
        let key = match op {
            Op::Input(a) => (0, 0, a, 0, 0),
            Op::Int(v) => (1, v, 0, 0, 0),
            Op::Bool(v) => (2, i128::from(v), 0, 0, 0),
            Op::Lt(a, b) => (6, 0, a, b, 0),
            Op::Select(c, a, b) => (9, 0, c, a, b),
            _ => return Err(Refusal::new("reference-encoder-operation")),
        };
        if let Some(id) = self.index.get(&key) {
            return Ok(*id);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(Refusal::new("reference-encoding-budget"));
        }
        let id = u16::try_from(self.nodes.len())
            .map_err(|_| Refusal::new("reference-encoding-budget"))?;
        self.nodes.push(op);
        self.index.insert(key, id);
        Ok(id)
    }

    // Split a contiguous product block, avoiding ordinal arithmetic/overflow.
    // Equal leaves/subtrees share nodes; construction cannot introduce traps.
    fn branch(
        &mut self,
        inputs: &[Domain],
        dim: usize,
        values: &[i128],
        output: Domain,
        range: Option<(i128, usize)>,
    ) -> Result<u16> {
        let first = *values
            .first()
            .ok_or_else(|| Refusal::new("reference-empty-table"))?;
        if values.iter().all(|v| *v == first) {
            return self.add(literal(output, first));
        }
        let d = inputs
            .get(dim)
            .ok_or_else(|| Refusal::new("reference-table-shape"))?;
        let (lo, width) = match range {
            Some(r) => r,
            None => (
                d.bounds().0,
                usize::try_from(crate::subject::domain_size(*d))
                    .map_err(|_| Refusal::new("reference-domain-budget"))?,
            ),
        };
        if width == 1 {
            return self.branch(inputs, dim + 1, values, output, None);
        }
        let half = width / 2;
        let stride = values.len() / width;
        let pivot = lo
            .checked_add(i128::try_from(half).map_err(|_| Refusal::new("reference-domain-budget"))?)
            .ok_or_else(|| Refusal::new("reference-domain-budget"))?;
        let (left, right) = values.split_at(half * stride);
        let a = self.branch(inputs, dim, left, output, Some((lo, half)))?;
        let b = self.branch(inputs, dim, right, output, Some((pivot, width - half)))?;
        if a == b {
            return Ok(a);
        }
        let input = self.add(Op::Input(
            u16::try_from(dim).map_err(|_| Refusal::new("reference-abi"))?,
        ))?;
        let c = if matches!(d, Domain::Bool) {
            // The only nontrivial Boolean split is 0 | 1.
            return self.add(Op::Select(input, b, a));
        } else {
            let threshold = self.add(Op::Int(pivot))?;
            self.add(Op::Lt(input, threshold))?
        };
        self.add(Op::Select(c, a, b))
    }
}

/// An advisory comparison over exposed examples. Never an acceptance witness.
pub fn precheck(
    table: &ReferenceTable,
    candidate: &Program,
    examples: &[usize],
) -> Result<Option<usize>> {
    abi(table, candidate)?;
    for i in examples {
        let point = table
            .points
            .get(*i)
            .ok_or_else(|| Refusal::new("example-ordinal"))?;
        if candidate.evaluate(point).ok().as_ref() != table.rows.get(*i) {
            return Ok(Some(*i));
        }
    }
    Ok(None)
}

/// Advisory full-table failing ordinals, used only AFTER checker refutation.
pub fn differences(table: &ReferenceTable, candidate: &Program) -> Result<Vec<usize>> {
    abi(table, candidate)?;
    Ok(table
        .points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            (candidate.evaluate(p).ok().as_ref() != table.rows.get(i)).then_some(i)
        })
        .collect())
}

fn abi(table: &ReferenceTable, p: &Program) -> Result<()> {
    if p.inputs() != table.inputs.as_slice() || p.outputs() != table.outputs.as_slice() {
        return Err(Refusal::new("capture-candidate-abi"));
    }
    Ok(())
}

/// Farthest failing input from the most recently added example; ordinal tie-break.
#[must_use]
pub fn choose_counterexample(
    table: &ReferenceTable,
    bad: &[usize],
    anchor: Option<usize>,
) -> Option<usize> {
    let p = anchor.and_then(|i| table.points.get(i));
    bad.iter()
        .copied()
        .filter(|i| *i < table.points.len())
        .max_by_key(|i| {
            let distance = p.map_or(0u128, |p| {
                p.iter()
                    .zip(&table.points[*i])
                    .map(|(a, b)| a.abs_diff(*b))
                    .sum()
            });
            (distance, core::cmp::Reverse(*i))
        })
}

/// A proposed reachable condition outcome. Prefix traps give no outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionOutcome {
    /// Select instruction index.
    pub node: usize,
    /// 0 or 1.
    pub outcome: i128,
    /// First point demonstrating the outcome, if reachable.
    pub example: Option<usize>,
    /// Whether a selected example already covers it.
    pub covered: bool,
}

/// Exact work reservation for the advisory eager-prefix coverage pass.
#[must_use]
pub fn coverage_visits(candidate: &Program, size: usize) -> u64 {
    candidate
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            matches!(n, Op::Select(..)).then_some(u64::try_from(i).unwrap_or(u64::MAX))
        })
        .fold(0u64, u64::saturating_add)
        .saturating_mul(u64::try_from(size).unwrap_or(u64::MAX))
}

/// Enumerate both reachable outcomes of every Select condition. Advisory only.
pub fn coverage(
    table: &ReferenceTable,
    candidate: &Program,
    examples: &[usize],
) -> Result<Vec<DecisionOutcome>> {
    abi(table, candidate)?;
    let mut result = Vec::new();
    for (j, op) in candidate.nodes().iter().enumerate() {
        let Op::Select(condition, _, _) = *op else {
            continue;
        };
        let prefix = Program::try_new(
            candidate.inputs().to_vec(),
            vec![Domain::Bool],
            candidate.nodes()[..j].to_vec(),
            vec![condition],
        )
        .map_err(|_| Refusal::new("coverage-prefix"))?;
        let observed = table
            .points
            .iter()
            .map(|p| prefix.evaluate(p).ok().and_then(|v| v.first().copied()))
            .collect::<Vec<_>>();
        for outcome in [0, 1] {
            let example = observed.iter().position(|v| *v == Some(outcome));
            let covered = examples
                .iter()
                .any(|i| observed.get(*i) == Some(&Some(outcome)));
            result.push(DecisionOutcome {
                node: j,
                outcome,
                example,
                covered,
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
