//! Mathematical V2 charges and eager instruction attempts; erased from Rust.

use super::super::evaluation::spec as scalar;
use super::super::evaluation::{Domain, Failure as ScalarFailure, Op};
use super::Failure;
use super::meter::{MeterFailure, Resource};
use vstd::prelude::*;

pub use crate::resource::resource_index;

verus! {

pub(super) open spec fn charge(
    limits: Seq<u64>, used: Seq<u64>, resource: Resource, amount: u64,
) -> (Result<(), MeterFailure>, Seq<u64>)
    recommends limits.len() == 8, used.len() == 8,
{
    let index = resource_index(resource) as int;
    let total = used[index] as int + amount as int;
    if total > u64::MAX {
        (Err(MeterFailure { resource, limit: limits[index], attempted: u64::MAX, overflow: true }), used)
    } else if total > limits[index] {
        (Err(MeterFailure { resource, limit: limits[index], attempted: total as u64, overflow: false }), used)
    } else {
        (Ok(()), used.update(index, total as u64))
    }
}

pub(super) open spec fn scalar_failure(error: ScalarFailure) -> Failure {
    match error {
        ScalarFailure::InputDomain => Failure::InputDomain,
        ScalarFailure::Reference => Failure::Reference,
        ScalarFailure::Arithmetic => Failure::Arithmetic,
        ScalarFailure::OutputDomain => Failure::OutputDomain,
    }
}

pub(super) open spec fn prefix(
    nodes: Seq<Op>, input: Seq<i64>, count: nat, limits: Seq<u64>, used: Seq<u64>,
) -> (Result<Seq<i64>, Failure>, Seq<u64>)
    recommends count <= nodes.len(), limits.len() == 8, used.len() == 8,
    decreases count,
{
    if count == 0 { (Ok(Seq::empty()), used) }
    else {
        let previous = prefix(nodes, input, (count - 1) as nat, limits, used);
        match previous.0 {
            Err(error) => previous,
            Ok(values) => {
                let charged = charge(limits, previous.1, Resource::Step, 1);
                match charged.0 {
                    Err(error) => (Err(Failure::Budget(error)), charged.1),
                    Ok(()) => match scalar::node(nodes[count as int - 1], input, values) {
                        Err(error) => (Err(scalar_failure(error)), charged.1),
                        Ok(value) => (Ok(values.push(value)), charged.1),
                    },
                }
            },
        }
    }
}

pub(super) proof fn failure_persists(
    nodes: Seq<Op>, input: Seq<i64>, failed: nat, count: nat,
    limits: Seq<u64>, used: Seq<u64>, error: Failure,
)
    requires failed <= count <= nodes.len(),
        prefix(nodes, input, failed, limits, used).0 == Err(error),
    ensures prefix(nodes, input, count, limits, used)
        == prefix(nodes, input, failed, limits, used),
    decreases count - failed,
{
    if count > failed {
        failure_persists(nodes, input, failed, (count - 1) as nat, limits, used, error);
    }
}

pub(super) open spec fn execution(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>,
    input: Seq<i64>, limits: Seq<u64>, used: Seq<u64>,
) -> (Result<Seq<i64>, Failure>, Seq<u64>) {
    if !scalar::admitted(inputs, input) { (Err(Failure::InputDomain), used) }
    else {
        let evaluated = prefix(nodes, input, nodes.len(), limits, used);
        match evaluated.0 {
            Err(error) => evaluated,
            Ok(values) => match scalar::projection(values, roots, roots.len()) {
                Err(error) => (Err(scalar_failure(error)), evaluated.1),
                Ok(output) => if scalar::admitted(outputs, output) {
                    (Ok(output), evaluated.1)
                } else { (Err(Failure::OutputDomain), evaluated.1) },
            },
        }
    }
}

}
