//! V2 finite execution with library-owned logical work accounting.

use super::evaluation::{Domain, Failure as ScalarFailure, Op};
use alloc::vec::Vec;
use meter::Meter;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub mod authority;
pub mod catalog;
pub mod composition;
pub(super) mod compound;
pub mod continuation;
mod decision;
mod input_view;
pub mod laws;
pub(super) mod meter;
#[cfg(verus_keep_ghost)]
pub(crate) mod nonbinding_step;
pub(crate) mod numeric;
/// Nonpublishing native oracle relations; authenticated integration is separately gated.
#[cfg(not(verus_keep_ghost))]
pub mod oracle;
#[cfg(verus_keep_ghost)]
mod spec;
#[cfg(not(verus_keep_ghost))]
pub mod tables;
pub(crate) mod util;

pub use input_view::{
    AccessAttempt, Failure as RecordFailure, Field as InputField, Leaf as InputLeaf,
    Projection as RecordProjection, Variant as InputVariant, project as project_record,
};
pub use meter::{Limits, MeterFailure, Resource, Usage, zero_limits};
/// Borrowed eager scalar graph used by the library-owned producer.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct ScalarProgram<'a> {
    /// Exact scalar domains in declared ABI order.
    pub inputs: &'a [Domain],
    /// Exact result domains in root order.
    pub outputs: &'a [Domain],
    /// Eager instruction sequence, including unused and unselected nodes.
    pub nodes: &'a [Op],
    /// Result references, in declared order.
    pub roots: &'a [u16],
}

/// Refusal by scalar execution or the V2 logical meter.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The initial tuple is not in its declared domains.
    InputDomain,
    /// An instruction or output refers to an unavailable scalar.
    Reference,
    /// Checked signed arithmetic overflowed.
    Arithmetic,
    /// The output tuple is not in its declared domains.
    OutputDomain,
    /// A charge refused before the associated instruction attempt.
    Budget(MeterFailure),
}

/// A complete scalar result and library-computed usage, including on refusal.
/// This record does not authorize a transition.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Outcome {
    result: Result<Vec<i128>, Failure>,
    usage: Usage,
}

impl Outcome {
    /// Reads usage without allowing its replacement or mutation.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.view() == self.view().1,
    ))]
    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// Returns both result and usage; failed execution retains its report.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures (match result.0 {
            Ok(values) => Ok(values@), Err(error) => Err(error),
        }, result.1.view()) == self.view(),
    ))]
    pub fn into_parts(self) -> (Result<Vec<i128>, Failure>, Usage) {
        (self.result, self.usage)
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl Limits {
    pub closed spec fn view(self) -> Seq<u64> { self.counters@ }
}
impl Usage {
    pub closed spec fn view(self) -> Seq<u64> { self.counters@ }
}
impl Outcome {
    pub closed spec fn view(self) -> (Result<Seq<i128>, Failure>, Seq<u64>) {
        (match self.result {
            Ok(values) => Ok(values@), Err(error) => Err(error),
        }, self.usage.view())
    }
}
pub closed spec fn execution(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>,
    input: Seq<i128>, limits: Seq<u64>, used: Seq<u64>,
) -> (Result<Seq<i128>, Failure>, Seq<u64>) {
    spec::execution(inputs, outputs, nodes, roots, input, limits, used)
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::scalar_failure(error),
))]
fn lift(error: ScalarFailure) -> Failure {
    match error {
        ScalarFailure::InputDomain => Failure::InputDomain,
        ScalarFailure::Reference => Failure::Reference,
        ScalarFailure::Arithmetic => Failure::Arithmetic,
        ScalarFailure::OutputDomain => Failure::OutputDomain,
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@) == spec::execution(inputs@, outputs@, nodes@,
                roots@, input@, old(meter).limits.counters@, old(meter).used.counters@),
))]
fn evaluate_nodes(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i128],
    meter: &mut Meter,
    values: &mut Vec<i128>,
    output: &mut Vec<i128>,
) -> Result<(), Failure> {
    use super::evaluation::typed::Values;
    values.clear();
    output.clear();
    let program = numeric::ProgramView::Legacy {
        inputs,
        outputs,
        nodes,
        roots,
    };
    let mut atoms = Vec::new();
    let mut results = Vec::new();
    #[cfg(verus_keep_ghost)]
    proof! { numeric::spec::legacy_execution_embedding(program, input@, meter.limits.counters@, meter.used.counters@); }
    match numeric::evaluate(
        program,
        Values::Legacy(input),
        meter,
        &mut atoms,
        &mut results,
    ) {
        Err(numeric::Failure::InputDomain) => return Err(lift(ScalarFailure::InputDomain)),
        Err(numeric::Failure::Reference) => return Err(lift(ScalarFailure::Reference)),
        Err(numeric::Failure::Arithmetic | numeric::Failure::Kind) => {
            return Err(lift(ScalarFailure::Arithmetic));
        }
        Err(numeric::Failure::OutputDomain) => return Err(lift(ScalarFailure::OutputDomain)),
        Err(numeric::Failure::Budget(error)) => return Err(Failure::Budget(error)),
        Ok(()) => {}
    }
    if !numeric::project_legacy(&atoms, values) || !numeric::project_legacy(&results, output) {
        return Err(Failure::Arithmetic);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@) == spec::execution(inputs@, outputs@, nodes@,
                roots@, input@, old(meter).limits.counters@, old(meter).used.counters@),
        result.is_err() ==> final(values)@ == Seq::<i128>::empty()
            && final(output)@ == Seq::<i128>::empty(),
))]
fn evaluate_into(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i128],
    meter: &mut Meter,
    values: &mut Vec<i128>,
    output: &mut Vec<i128>,
) -> Result<(), Failure> {
    let result = evaluate_nodes(inputs, outputs, nodes, roots, input, meter, values, output);
    if result.is_err() {
        values.clear();
        output.clear();
    }
    result
}

/// Executes eager scalar IR with a fresh, private V2 meter.
///
/// Only instruction attempts consume Step. Scalar admission and output projection
/// are outside this cost profile. Other resources remain zero in this unit.
/// Malformed graphs are handled defensively; no caller usage can be supplied.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.view() == execution(inputs@, outputs@, nodes@, roots@,
        input@, limits.view(), Seq::new(8, |_: int| 0u64)),
))]
pub fn execute(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i128],
    limits: Limits,
) -> Outcome {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Outcome::view); reveal(Limits::view); reveal(Usage::view); reveal(execution); }
    let mut meter = meter::new(limits);
    let mut values = Vec::new();
    let mut output = Vec::new();
    let result = match evaluate_into(
        inputs,
        outputs,
        nodes,
        roots,
        input,
        &mut meter,
        &mut values,
        &mut output,
    ) {
        Ok(()) => Ok(output),
        Err(error) => Err(error),
    };
    Outcome {
        result,
        usage: meter.used,
    }
}

#[cfg(test)]
#[path = "../../../../zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs"]
mod order_fixture;

#[cfg(test)]
/// Unchanged emitted order fixture for complete-decision test comparisons.
pub use order_fixture::transition as order_fixture_transition;

#[cfg(test)]
mod tests;
