//! The declared input domain, the original's observation table and node
//! signatures, all computed by the library evaluator.
//!
//! Observations come from `Program::execute_v2` with the full Step budget,
//! the evaluator and budget `transform check` uses. Node values come from
//! probe programs evaluated by `Program::evaluate`: the same instructions with
//! the probed nodes as results, so traps (including in unused nodes and
//! unselected `Select` arms) poison exactly the tuples the library says.

use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{
    Domain, MAX_NODES, Program, V2ExecutionFailure, V2Resource, v2_zero_limits,
};

use crate::Stop;
use crate::ir::Graph;

/// Probe results per library evaluation (the finite-i128/2 output bound).
const PROBE_WIDTH: usize = 64;

/// Work accounting shared by every stage of one search.
pub(crate) struct Meter {
    used: u64,
    limit: u64,
}

impl Meter {
    pub(crate) fn new(limit: u64) -> Self {
        Self { used: 0, limit }
    }

    pub(crate) fn charge(&mut self, units: usize) -> Result<(), Stop> {
        self.used = self
            .used
            .saturating_add(u64::try_from(units).unwrap_or(u64::MAX));
        if self.used > self.limit {
            Err(Stop::WorkBudget { limit: self.limit })
        } else {
            Ok(())
        }
    }

    pub(crate) fn used(&self) -> u64 {
        self.used
    }

    /// Units that can still be charged without stopping.
    pub(crate) fn remaining(&self) -> usize {
        usize::try_from(self.limit.saturating_sub(self.used)).unwrap_or(usize::MAX)
    }

    /// The stop for an exhausted budget.
    pub(crate) fn exhausted(&self) -> Stop {
        Stop::WorkBudget { limit: self.limit }
    }
}

/// One evaluation result, classified as the checker compares it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    Ok,
    Arithmetic,
    OutputDomain,
    /// Any other refusal; never treated as equal to anything.
    Other,
}

/// The declared domain in `ordered-product-last-input-fastest-v1` order.
pub(crate) struct Space {
    mins: Vec<i128>,
    sizes: Vec<u64>,
    strides: Vec<u64>,
    count: usize,
}

impl Space {
    pub(crate) fn new(inputs: &[Domain], max_tuples: u64) -> Result<Self, Stop> {
        let mut mins = Vec::with_capacity(inputs.len());
        let mut sizes = Vec::with_capacity(inputs.len());
        let mut total: u128 = 1;
        for domain in inputs {
            let (min, max) = domain.bounds();
            if min > max {
                return Err(Stop::EmptyInputDomain);
            }
            let size = max
                .checked_sub(min)
                .and_then(|span| span.checked_add(1))
                .and_then(|n| u128::try_from(n).ok())
                .ok_or(Stop::DomainOverBudget {
                    tuples: u128::MAX,
                    limit: max_tuples,
                })?;
            if size == 0 {
                return Err(Stop::EmptyInputDomain);
            }
            total = total.saturating_mul(size);
            mins.push(min);
            sizes.push(u64::try_from(size).unwrap_or(u64::MAX));
        }
        if total > u128::from(max_tuples) {
            return Err(Stop::DomainOverBudget {
                tuples: total,
                limit: max_tuples,
            });
        }
        let count = usize::try_from(total).map_err(|_| Stop::DomainOverBudget {
            tuples: total,
            limit: max_tuples,
        })?;
        let mut strides = vec![1u64; inputs.len()];
        for index in (0..inputs.len().saturating_sub(1)).rev() {
            strides[index] = strides[index + 1].saturating_mul(sizes[index + 1]);
        }
        Ok(Self {
            mins,
            sizes,
            strides,
            count,
        })
    }

    pub(crate) fn count(&self) -> usize {
        self.count
    }

    /// Writes the tuple at `ordinal` into `out`.
    pub(crate) fn tuple(&self, ordinal: usize, out: &mut Vec<i128>) {
        out.clear();
        let ordinal = u64::try_from(ordinal).unwrap_or(u64::MAX);
        for ((min, size), stride) in self.mins.iter().zip(&self.sizes).zip(&self.strides) {
            let offset = (ordinal / stride) % size;
            // The offset is below the domain size, so the sum stays in range.
            out.push(*min + i128::from(offset));
        }
    }

    /// One input's value at each listed ordinal.
    pub(crate) fn column(&self, input: usize, ordinals: &[u32]) -> Vec<i128> {
        let (Some(min), Some(size), Some(stride)) = (
            self.mins.get(input),
            self.sizes.get(input),
            self.strides.get(input),
        ) else {
            return Vec::new();
        };
        ordinals
            .iter()
            .map(|ordinal| {
                let offset = (u64::from(*ordinal) / stride) % size;
                *min + i128::from(offset)
            })
            .collect()
    }
}

fn observe(program: &Program, input: &[i128], outputs: &mut Vec<i128>) -> Status {
    let limits =
        v2_zero_limits().with_limit(V2Resource::Step, u64::try_from(MAX_NODES).unwrap_or(0));
    let (result, _) = program.execute_v2(input, limits).into_parts();
    outputs.clear();
    match result {
        Ok(values) => {
            outputs.extend_from_slice(&values);
            Status::Ok
        }
        Err(V2ExecutionFailure::Arithmetic) => Status::Arithmetic,
        Err(V2ExecutionFailure::OutputDomain) => Status::OutputDomain,
        Err(_) => Status::Other,
    }
}

/// The original's complete observation table.
pub(crate) struct Reference {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    status: Vec<Status>,
    values: Vec<i128>,
    width: usize,
    /// Ordinals where no instruction traps: node values are defined there.
    live: Vec<u32>,
}

/// The result of comparing a candidate with the original on every tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    Equal,
    /// Different input or output domains: `transform check` refuses the pair.
    Abi,
    /// The first tuple where the observations differ.
    Differs(u64),
}

impl Reference {
    pub(crate) fn new(
        original: &Program,
        space: &Space,
        max_cells: u64,
        meter: &mut Meter,
    ) -> Result<Self, Stop> {
        let width = original.outputs().len();
        let cells = (space.count() as u128).saturating_mul(width.max(1) as u128);
        if cells > u128::from(max_cells) {
            return Err(Stop::CellsOverBudget {
                cells,
                limit: max_cells,
            });
        }
        if u32::try_from(space.count()).is_err() {
            return Err(Stop::DomainOverBudget {
                tuples: space.count() as u128,
                limit: u64::from(u32::MAX),
            });
        }
        meter.charge(space.count().saturating_mul(original.nodes().len().max(1)))?;
        let mut status = Vec::with_capacity(space.count());
        let mut values = Vec::with_capacity(space.count().saturating_mul(width));
        let mut live = Vec::new();
        let mut tuple = Vec::new();
        let mut outputs = Vec::new();
        for ordinal in 0..space.count() {
            space.tuple(ordinal, &mut tuple);
            let observed = observe(original, &tuple, &mut outputs);
            outputs.resize(width, 0);
            values.extend_from_slice(&outputs);
            if matches!(observed, Status::Ok | Status::OutputDomain) {
                live.push(u32::try_from(ordinal).unwrap_or(u32::MAX));
            }
            status.push(observed);
        }
        Ok(Self {
            inputs: original.inputs().to_vec(),
            outputs: original.outputs().to_vec(),
            status,
            values,
            width,
            live,
        })
    }

    pub(crate) fn live(&self) -> &[u32] {
        &self.live
    }

    /// Tuples on which the original traps (Arithmetic) or refuses otherwise.
    pub(crate) fn trapping(&self) -> usize {
        self.status.len() - self.live.len()
    }

    /// Compares `candidate` with the original on the listed ordinals (all of
    /// them when `ordinals` is `None`), in order, with the checker's evaluator.
    pub(crate) fn compare(
        &self,
        candidate: &Program,
        space: &Space,
        ordinals: Option<&[u32]>,
        meter: &mut Meter,
    ) -> Result<Verdict, Stop> {
        if candidate.inputs() != self.inputs.as_slice()
            || candidate.outputs() != self.outputs.as_slice()
        {
            return Ok(Verdict::Abi);
        }
        let count = ordinals.map_or(space.count(), <[u32]>::len);
        let per_tuple = candidate.nodes().len().max(1);
        let mut tuple = Vec::new();
        let mut outputs = Vec::new();
        // Charge before evaluating each bounded chunk, not before the entire
        // domain. A first counterexample needs only its chunk's reserved work;
        // equality still requires enough budget to charge *every* tuple.
        // This deliberately overcharges at most one partially used chunk,
        // retaining fail-closed conservative metering and deterministic order.
        const CHECK_CHUNK: usize = 128;
        for start in (0..count).step_by(CHECK_CHUNK) {
            let end = start.saturating_add(CHECK_CHUNK).min(count);
            meter.charge((end - start).saturating_mul(per_tuple))?;
            for index in start..end {
                let ordinal = match ordinals {
                    Some(list) => list[index] as usize,
                    None => index,
                };
                if !self.agrees(candidate, space, ordinal, &mut tuple, &mut outputs) {
                    return Ok(Verdict::Differs(ordinal as u64));
                }
            }
        }
        Ok(Verdict::Equal)
    }

    /// Indices into `ordinals` where `candidate`'s observation differs. The
    /// caller has checked the ABI.
    pub(crate) fn differing(
        &self,
        candidate: &Program,
        space: &Space,
        ordinals: &[u32],
        meter: &mut Meter,
    ) -> Result<Vec<usize>, Stop> {
        meter.charge(
            ordinals
                .len()
                .saturating_mul(candidate.nodes().len().max(1)),
        )?;
        let mut tuple = Vec::new();
        let mut outputs = Vec::new();
        Ok(ordinals
            .iter()
            .enumerate()
            .filter(|(_, ordinal)| {
                !self.agrees(
                    candidate,
                    space,
                    **ordinal as usize,
                    &mut tuple,
                    &mut outputs,
                )
            })
            .map(|(index, _)| index)
            .collect())
    }

    /// Identical outputs, or the identical failure kind, at `ordinal`.
    fn agrees(
        &self,
        candidate: &Program,
        space: &Space,
        ordinal: usize,
        tuple: &mut Vec<i128>,
        outputs: &mut Vec<i128>,
    ) -> bool {
        space.tuple(ordinal, tuple);
        let observed = observe(candidate, tuple, outputs);
        let start = ordinal * self.width;
        observed == self.status[ordinal]
            && observed != Status::Other
            && (observed != Status::Ok
                || outputs.as_slice() == &self.values[start..start + self.width])
    }

    /// Every node's value on every live ordinal, read through library probes.
    /// `None` when `graph` traps on a live ordinal (it cannot then be
    /// equivalent to the original) or the library refuses a probe.
    pub(crate) fn signatures(
        &self,
        graph: &Graph,
        space: &Space,
        max_cells: u64,
        meter: &mut Meter,
    ) -> Result<Option<Vec<Vec<i128>>>, Stop> {
        let n = self.live.len();
        let cells = (graph.nodes.len() as u128).saturating_mul(n as u128);
        if cells > u128::from(max_cells) {
            return Err(Stop::CellsOverBudget {
                cells,
                limit: max_cells,
            });
        }
        let kinds = graph.kinds();
        let ids: Vec<usize> = (0..graph.nodes.len()).collect();
        let mut probes = Vec::new();
        for chunk in ids.chunks(PROBE_WIDTH) {
            match graph.probe(chunk, &kinds) {
                Some(program) => probes.push((chunk[0], program)),
                None => return Ok(None),
            }
        }
        meter.charge(
            n.saturating_mul(graph.nodes.len().max(1))
                .saturating_mul(probes.len().max(1)),
        )?;
        let mut signatures = vec![Vec::with_capacity(n); graph.nodes.len()];
        let mut tuple = Vec::new();
        for ordinal in &self.live {
            space.tuple(*ordinal as usize, &mut tuple);
            for (first, probe) in &probes {
                let Ok(values) = probe.evaluate(&tuple) else {
                    return Ok(None);
                };
                for (offset, value) in values.into_iter().enumerate() {
                    signatures[first + offset].push(value);
                }
            }
        }
        Ok(Some(signatures))
    }
}


#[cfg(test)]
mod compare_charge_tests {
    use super::*;
    use zeno_fcis_synthesis::finite::Op;

    fn admitted(nodes: Vec<Op>, roots: Vec<u16>) -> Program {
        Program::try_new(
            vec![Domain::Int { min: 0, max: 255 }],
            vec![Domain::Bool],
            nodes,
            roots,
        )
        .unwrap_or_else(|error| panic!("test program invalid: {error:?}"))
    }

    #[test]
    fn first_counterexample_does_not_precharge_the_whole_domain() {
        let original = admitted(vec![Op::Bool(false)], vec![0]);
        let wrong = admitted(vec![Op::Bool(true)], vec![0]);
        let space = Space::new(original.inputs(), 256)
            .unwrap_or_else(|error| panic!("test domain invalid: {error:?}"));
        let mut setup = Meter::new(u64::MAX);
        let reference = Reference::new(&original, &space, 1024, &mut setup)
            .unwrap_or_else(|error| panic!("reference refused: {error:?}"));

        let mut mismatch_budget = Meter::new(128);
        assert_eq!(
            reference.compare(&wrong, &space, None, &mut mismatch_budget),
            Ok(Verdict::Differs(0))
        );
        assert_eq!(mismatch_budget.used(), 128);

        let mut insufficient = Meter::new(127);
        assert_eq!(
            reference.compare(&wrong, &space, None, &mut insufficient),
            Err(Stop::WorkBudget { limit: 127 }),
            "without budget for even one chunk, no comparison verdict is given"
        );

        let mut insufficient_equal = Meter::new(255);
        assert_eq!(
            reference.compare(&original, &space, None, &mut insufficient_equal),
            Err(Stop::WorkBudget { limit: 255 }),
            "partial matching must never become an equivalence verdict"
        );
        let mut full = Meter::new(256);
        assert_eq!(
            reference.compare(&original, &space, None, &mut full),
            Ok(Verdict::Equal)
        );
        assert_eq!(full.used(), 256);

        let mut reordered = Meter::new(2);
        assert_eq!(
            reference.compare(&wrong, &space, Some(&[250, 0]), &mut reordered),
            Ok(Verdict::Differs(250)),
            "the first disagreement follows caller-supplied ordinal order"
        );
        assert_eq!(reordered.used(), 2);
    }

    #[test]
    fn last_counterexample_still_needs_the_full_budget() {
        let original = admitted(vec![Op::Bool(false)], vec![0]);
        let last_only = admitted(
            vec![Op::Input(0), Op::Int(255), Op::Eq(0, 1)],
            vec![2],
        );
        let space = Space::new(original.inputs(), 256)
            .unwrap_or_else(|error| panic!("test domain invalid: {error:?}"));
        let mut setup = Meter::new(u64::MAX);
        let reference = Reference::new(&original, &space, 1024, &mut setup)
            .unwrap_or_else(|error| panic!("reference refused: {error:?}"));
        let mut short = Meter::new(767);
        assert_eq!(
            reference.compare(&last_only, &space, None, &mut short),
            Err(Stop::WorkBudget { limit: 767 })
        );
        let mut enough = Meter::new(768);
        assert_eq!(
            reference.compare(&last_only, &space, None, &mut enough),
            Ok(Verdict::Differs(255))
        );
        assert_eq!(enough.used(), 768);
    }
}
