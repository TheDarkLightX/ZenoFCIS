//! V2 finite execution with library-owned logical work accounting.

use super::evaluation::{self, Domain, Failure as ScalarFailure, Op};
use alloc::vec::Vec;
use meter::Meter;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub mod authority;
pub mod catalog;
pub mod composition;
pub mod continuation;
mod decision;
mod input_view;
pub mod laws;
mod meter;
#[cfg(verus_keep_ghost)]
mod spec;
mod util;

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
    result: Result<Vec<i64>, Failure>,
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
    pub fn into_parts(self) -> (Result<Vec<i64>, Failure>, Usage) {
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
    pub closed spec fn view(self) -> (Result<Seq<i64>, Failure>, Seq<u64>) {
        (match self.result {
            Ok(values) => Ok(values@), Err(error) => Err(error),
        }, self.usage.view())
    }
}
pub closed spec fn execution(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>,
    input: Seq<i64>, limits: Seq<u64>, used: Seq<u64>,
) -> (Result<Seq<i64>, Failure>, Seq<u64>) {
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
    input: &[i64],
    meter: &mut Meter,
    values: &mut Vec<i64>,
    output: &mut Vec<i64>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial_used = meter.used.counters@; }
    values.clear();
    output.clear();
    if !evaluation::admitted(inputs, input) {
        return Err(Failure::InputDomain);
    }
    values.reserve(nodes.len());
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            evaluation::spec::admitted(inputs@, input@),
            meter.limits == old(meter).limits,
            initial_used == old(meter).used.counters@, initial_used.len() == 8,
            index <= nodes.len(), values@.len() == index,
            output@ == Seq::<i64>::empty(),
            spec::prefix(nodes@, input@, index as nat, meter.limits.counters@,
                initial_used) == (Ok(values@), meter.used.counters@),
        decreases nodes.len() - index,
    ))]
    while index < nodes.len() {
        if let Err(error) = meter.charge(Resource::Step, 1) {
            #[cfg(verus_keep_ghost)]
            proof! { spec::failure_persists(nodes@, input@, (index + 1) as nat,
            nodes@.len() as nat, meter.limits.counters@, initial_used,
            Failure::Budget(error)); }
            return Err(Failure::Budget(error));
        }
        match evaluation::evaluate_node(&nodes[index], input, values) {
            Ok(value) => values.push(value),
            Err(error) => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::failure_persists(nodes@, input@, (index + 1) as nat,
                nodes@.len() as nat, meter.limits.counters@, initial_used,
                spec::scalar_failure(error)); }
                return Err(lift(error));
            }
        }
        index += 1;
    }
    output.reserve(roots.len());
    let mut root = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            evaluation::spec::admitted(inputs@, input@),
            meter.limits == old(meter).limits,
            initial_used == old(meter).used.counters@, initial_used.len() == 8,
            spec::prefix(nodes@, input@, nodes@.len() as nat, meter.limits.counters@,
                initial_used) == (Ok(values@), meter.used.counters@),
            root <= roots.len(), output@.len() == root,
            evaluation::spec::projection(values@, roots@, root as nat) == Ok(output@),
        decreases roots.len() - root,
    ))]
    while root < roots.len() {
        match evaluation::at(values, roots[root]) {
            Some(value) => output.push(value),
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { evaluation::spec::projection_failure_persists(values@, roots@,
                (root + 1) as nat, roots@.len() as nat); }
                return Err(Failure::Reference);
            }
        }
        root += 1;
    }
    if !evaluation::admitted(outputs, output) {
        return Err(Failure::OutputDomain);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@) == spec::execution(inputs@, outputs@, nodes@,
                roots@, input@, old(meter).limits.counters@, old(meter).used.counters@),
        result.is_err() ==> final(values)@ == Seq::<i64>::empty()
            && final(output)@ == Seq::<i64>::empty(),
))]
fn evaluate_into(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i64],
    meter: &mut Meter,
    values: &mut Vec<i64>,
    output: &mut Vec<i64>,
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
    input: &[i64],
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
