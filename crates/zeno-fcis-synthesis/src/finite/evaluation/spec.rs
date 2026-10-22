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

pub open spec fn contains(domain: Domain, value: i128) -> bool {
    match domain {
        Domain::Bool => 0 <= value <= 1,
        Domain::Int { min, max } => min <= value <= max,
    }
}

pub open spec fn admitted(domains: Seq<Domain>, values: Seq<i128>) -> bool {
    domains.len() == values.len()
        && forall|i: int| 0 <= i < domains.len() ==> contains(domains[i], values[i])
}

pub(crate) open spec fn new_arithmetic(op: Op) -> bool {
    matches!(op, Op::Mul(..) | Op::DivFloor(..) | Op::DivCeil(..)
        | Op::MulDivFloor(..) | Op::MulDivCeil(..) | Op::ProductsGe(..) | Op::SignedMulDivFloor(..))
}

pub(crate) open spec fn scalar_result(value: Option<i128>) -> Result<i128, Failure> {
    match value {Some(v)=>Ok(v),None=>Err(Failure::Arithmetic)}
}

pub(crate) open spec fn node(op: Op, input: Seq<i128>, values: Seq<i128>) -> Result<i128, Failure> {
    match op {
        Op::Input(id) => if id < input.len() {
            Ok(input[id as int])
        } else { Err(Failure::Reference) },
        Op::Int(value) => Ok(value),
        Op::Bool(value) => Ok(if value { 1i128 } else { 0i128 }),
        Op::Not(id) => if id < values.len() {
            Ok(if values[id as int] == 0 { 1i128 } else { 0i128 })
        } else { Err(Failure::Reference) },
        Op::Select(c, a, b) => if c < values.len() && a < values.len() && b < values.len() {
            Ok(if values[c as int] == 1 { values[a as int] } else { values[b as int] })
        } else { Err(Failure::Reference) },
        Op::Mul(a,b) | Op::DivFloor(a,b) | Op::DivCeil(a,b) => {
            if a>=values.len() || b>=values.len() {Err(Failure::Reference)} else {
                let x=values[a as int];let y=values[b as int];
                scalar_result(match op {
                    Op::Mul(..)=>super::super::value::scalar::multiply_value(x,y),
                    _=>super::super::value::scalar::division_value(matches!(op,Op::DivCeil(..)),x,y),
                })
            }
        },
        Op::MulDivFloor(a,b,d) | Op::MulDivCeil(a,b,d) => {
            if a>=values.len() || b>=values.len() || d>=values.len() {Err(Failure::Reference)} else {
                scalar_result(super::super::value::scalar::mul_div_value(matches!(op,Op::MulDivCeil(..)),
                    values[a as int],values[b as int],values[d as int]))
            }
        },
        Op::SignedMulDivFloor(a,b,d) => {
            if a>=values.len() || b>=values.len() || d>=values.len() {Err(Failure::Reference)} else {
                scalar_result(super::super::value::scalar::signed_mul_div_floor_value(
                    values[a as int],values[b as int],values[d as int]))
            }
        },
        Op::ProductsGe(a,b) => {
            if a[0]>=values.len() || a[1]>=values.len() || a[2]>=values.len()
                || b[0]>=values.len() || b[1]>=values.len() || b[2]>=values.len() {Err(Failure::Reference)}
            else { match super::super::value::scalar::products_ge_value(
                values[a[0] as int],values[a[1] as int],values[a[2] as int],
                values[b[0] as int],values[b[1] as int],values[b[2] as int]) {
                    Some(v)=>Ok(if v{1i128}else{0i128}),None=>Err(Failure::Arithmetic),
                }
            }
        },
        Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
            if a >= values.len() || b >= values.len() {
                Err(Failure::Reference)
            } else {
                let left = values[a as int];
                let right = values[b as int];
                match op {
                    Op::Add(_, _) => {
                        let value = left as int + right as int;
                        if i128::MIN <= value <= i128::MAX { Ok(value as i128) }
                        else { Err(Failure::Arithmetic) }
                    },
                    Op::Sub(_, _) => {
                        let value = left as int - right as int;
                        if i128::MIN <= value <= i128::MAX { Ok(value as i128) }
                        else { Err(Failure::Arithmetic) }
                    },
                    Op::Eq(_, _) => Ok(if left == right { 1i128 } else { 0i128 }),
                    Op::Lt(_, _) => Ok(if left < right { 1i128 } else { 0i128 }),
                    _ => Ok(if left == 1 && right == 1 { 1i128 } else { 0i128 }),
                }
            }
        },
    }
}

pub(crate) open spec fn prefix(nodes: Seq<Op>, input: Seq<i128>, count: nat)
    -> Result<Seq<i128>, Failure>
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
    nodes: Seq<Op>, input: Seq<i128>, failed: nat, count: nat, error: Failure,
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

pub(crate) open spec fn projection(values: Seq<i128>, roots: Seq<u16>, count: nat)
    -> Result<Seq<i128>, Failure>
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
    values: Seq<i128>, roots: Seq<u16>, failed: nat, count: nat,
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
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>, input: Seq<i128>,
) -> Result<Seq<i128>, Failure> {
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
