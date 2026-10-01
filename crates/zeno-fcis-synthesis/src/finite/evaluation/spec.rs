//! Mathematical eager execution; this module is erased from application builds.

use super::{Domain, Failure, Op};
use vstd::prelude::*;

verus! {

pub open spec fn valid_domain(domain: Domain) -> bool {
    match domain {
        Domain::Bool => true,
        Domain::Int { min, max } => min <= max,
    }
}

pub open spec fn contains(domain: Domain, value: i64) -> bool {
    match domain {
        Domain::Bool => 0 <= value <= 1,
        Domain::Int { min, max } => min <= value <= max,
    }
}

pub open spec fn admitted(domains: Seq<Domain>, values: Seq<i64>) -> bool {
    domains.len() == values.len()
        && forall|i: int| 0 <= i < domains.len() ==> contains(domains[i], values[i])
}

pub(crate) open spec fn node(op: Op, input: Seq<i64>, values: Seq<i64>) -> Result<i64, Failure> {
    match op {
        Op::Input(id) => if id < input.len() {
            Ok(input[id as int])
        } else { Err(Failure::Reference) },
        Op::Int(value) => Ok(value),
        Op::Bool(value) => Ok(if value { 1i64 } else { 0i64 }),
        Op::Not(id) => if id < values.len() {
            Ok(if values[id as int] == 0 { 1i64 } else { 0i64 })
        } else { Err(Failure::Reference) },
        Op::Select(c, a, b) => if c < values.len() && a < values.len() && b < values.len() {
            Ok(if values[c as int] == 1 { values[a as int] } else { values[b as int] })
        } else { Err(Failure::Reference) },
        Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
            if a >= values.len() || b >= values.len() {
                Err(Failure::Reference)
            } else {
                let left = values[a as int];
                let right = values[b as int];
                match op {
                    Op::Add(_, _) => {
                        let value = left as int + right as int;
                        if i64::MIN <= value <= i64::MAX { Ok(value as i64) }
                        else { Err(Failure::Arithmetic) }
                    },
                    Op::Sub(_, _) => {
                        let value = left as int - right as int;
                        if i64::MIN <= value <= i64::MAX { Ok(value as i64) }
                        else { Err(Failure::Arithmetic) }
                    },
                    Op::Eq(_, _) => Ok(if left == right { 1i64 } else { 0i64 }),
                    Op::Lt(_, _) => Ok(if left < right { 1i64 } else { 0i64 }),
                    _ => Ok(if left == 1 && right == 1 { 1i64 } else { 0i64 }),
                }
            }
        },
    }
}

pub(crate) open spec fn prefix(nodes: Seq<Op>, input: Seq<i64>, count: nat)
    -> Result<Seq<i64>, Failure>
    recommends count <= nodes.len(),
    decreases count,
{
    if count == 0 { Ok(Seq::empty()) }
    else {
        match prefix(nodes, input, (count - 1) as nat) {
            Err(error) => Err(error),
            Ok(previous) => match node(nodes[count as int - 1], input, previous) {
                Err(error) => Err(error),
                Ok(value) => Ok(previous.push(value)),
            },
        }
    }
}

pub(crate) proof fn prefix_failure_persists(
    nodes: Seq<Op>, input: Seq<i64>, failed: nat, count: nat, error: Failure,
)
    requires
        failed <= count <= nodes.len(),
        prefix(nodes, input, failed) == Err(error),
    ensures prefix(nodes, input, count) == Err(error),
    decreases count - failed,
{
    if count > failed {
        prefix_failure_persists(nodes, input, failed, (count - 1) as nat, error);
    }
}

pub(crate) open spec fn projection(values: Seq<i64>, roots: Seq<u16>, count: nat)
    -> Result<Seq<i64>, Failure>
    recommends count <= roots.len(),
    decreases count,
{
    if count == 0 { Ok(Seq::empty()) }
    else {
        match projection(values, roots, (count - 1) as nat) {
            Err(error) => Err(error),
            Ok(previous) => if roots[count as int - 1] < values.len() {
                Ok(previous.push(values[roots[count as int - 1] as int]))
            } else { Err(Failure::Reference) },
        }
    }
}

pub(crate) proof fn projection_failure_persists(
    values: Seq<i64>, roots: Seq<u16>, failed: nat, count: nat,
)
    requires
        failed <= count <= roots.len(),
        projection(values, roots, failed) == Err(Failure::Reference),
    ensures projection(values, roots, count) == Err(Failure::Reference),
    decreases count - failed,
{
    if count > failed {
        projection_failure_persists(values, roots, failed, (count - 1) as nat);
    }
}

pub(crate) open spec fn execution(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>, input: Seq<i64>,
) -> Result<Seq<i64>, Failure> {
    if !admitted(inputs, input) { Err(Failure::InputDomain) }
    else {
        match prefix(nodes, input, nodes.len()) {
            Err(error) => Err(error),
            Ok(values) => match projection(values, roots, roots.len()) {
                Err(error) => Err(error),
                Ok(output) => if admitted(outputs, output) { Ok(output) }
                    else { Err(Failure::OutputDomain) },
            },
        }
    }
}

}
