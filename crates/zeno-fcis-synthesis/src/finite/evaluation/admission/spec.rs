//! Exact mathematical admission results, erased from application builds.

use super::super::spec as execution;
use super::super::{Domain, Op};
use super::AdmissionFailure;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn boolean(domain: Domain) -> bool {
    matches!(domain, Domain::Bool)
}

pub(crate) open spec fn node_kind(previous: Seq<bool>, id: u16)
    -> Result<bool, AdmissionFailure>
{
    if id < previous.len() { Ok(previous[id as int]) }
    else { Err(AdmissionFailure::NodeReference) }
}

pub(crate) open spec fn required_node(previous: Seq<bool>, id: u16, expected: bool)
    -> Result<(), AdmissionFailure>
{
    if id >= previous.len() { Err(AdmissionFailure::NodeReference) }
    else if previous[id as int] != expected { Err(AdmissionFailure::TypeMismatch) }
    else { Ok(()) }
}

pub(crate) open spec fn kind(op: Op, inputs: Seq<Domain>, previous: Seq<bool>)
    -> Result<bool, AdmissionFailure>
{
    match op {
        Op::Input(id) => if id < inputs.len() {
            Ok(boolean(inputs[id as int]))
        } else { Err(AdmissionFailure::InputReference) },
        Op::Int(_) => Ok(false),
        Op::Bool(_) => Ok(true),
        Op::Add(a, b) | Op::Sub(a, b) | Op::Lt(a, b) => {
            if a >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if previous[a as int] { Err(AdmissionFailure::TypeMismatch) }
            else if b >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if previous[b as int] { Err(AdmissionFailure::TypeMismatch) }
            else { Ok(matches!(op, Op::Lt(..))) }
        },
        Op::And(a, b) => {
            if a >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if !previous[a as int] { Err(AdmissionFailure::TypeMismatch) }
            else if b >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if !previous[b as int] { Err(AdmissionFailure::TypeMismatch) }
            else { Ok(true) }
        },
        Op::Eq(a, b) => {
            if a >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if b >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if previous[a as int] != previous[b as int] {
                Err(AdmissionFailure::TypeMismatch)
            } else { Ok(true) }
        },
        Op::Not(a) => {
            if a >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if !previous[a as int] { Err(AdmissionFailure::TypeMismatch) }
            else { Ok(true) }
        },
        Op::Select(c, a, b) => {
            if c >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if !previous[c as int] { Err(AdmissionFailure::TypeMismatch) }
            else if a >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if b >= previous.len() { Err(AdmissionFailure::NodeReference) }
            else if previous[a as int] != previous[b as int] {
                Err(AdmissionFailure::TypeMismatch)
            } else { Ok(previous[a as int]) }
        },
    }
}

pub(crate) open spec fn valid_domains(domains: Seq<Domain>) -> bool {
    forall|i: int| 0 <= i < domains.len() ==> execution::valid_domain(domains[i])
}

pub(crate) open spec fn shape_valid(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: nat, roots: Seq<u16>,
) -> bool {
    inputs.len() <= 32
        && 0 < outputs.len() <= 16
        && 0 < nodes <= 256
        && outputs.len() == roots.len()
        && valid_domains(inputs)
        && valid_domains(outputs)
}

pub(crate) open spec fn shape(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: nat, roots: Seq<u16>,
) -> Result<(), AdmissionFailure> {
    if shape_valid(inputs, outputs, nodes, roots) { Ok(()) }
    else { Err(AdmissionFailure::Shape) }
}

pub(crate) open spec fn root_valid(domain: Domain, root: u16, kinds: Seq<bool>) -> bool {
    root < kinds.len() && kinds[root as int] == boolean(domain)
}

pub(crate) open spec fn roots_valid(
    outputs: Seq<Domain>, roots: Seq<u16>, kinds: Seq<bool>,
) -> bool {
    forall|i: int| 0 <= i < outputs.len() && i < roots.len() ==>
        root_valid(outputs[i], roots[i], kinds)
}

pub(crate) open spec fn roots(
    outputs: Seq<Domain>, roots: Seq<u16>, kinds: Seq<bool>,
) -> Result<(), AdmissionFailure> {
    if roots_valid(outputs, roots, kinds) { Ok(()) }
    else { Err(AdmissionFailure::OutputType) }
}

pub(crate) open spec fn prefix(inputs: Seq<Domain>, nodes: Seq<Op>, count: nat)
    -> Result<Seq<bool>, AdmissionFailure>
    recommends count <= nodes.len(),
    decreases count,
{
    if count == 0 { Ok(Seq::empty()) }
    else {
        match prefix(inputs, nodes, (count - 1) as nat) {
            Err(error) => Err(error),
            Ok(previous) => match kind(nodes[count as int - 1], inputs, previous) {
                Err(error) => Err(error),
                Ok(value) => Ok(previous.push(value)),
            },
        }
    }
}

pub(crate) proof fn prefix_failure_persists(
    inputs: Seq<Domain>, nodes: Seq<Op>, failed: nat, count: nat, error: AdmissionFailure,
)
    requires
        failed <= count <= nodes.len(),
        prefix(inputs, nodes, failed) == Err(error),
    ensures prefix(inputs, nodes, count) == Err(error),
    decreases count - failed,
{
    if count > failed {
        prefix_failure_persists(inputs, nodes, failed, (count - 1) as nat, error);
    }
}

pub(crate) open spec fn program(
    inputs: Seq<Domain>, outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>,
) -> Result<(), AdmissionFailure> {
    match shape(inputs, outputs, nodes.len(), roots) {
        Err(error) => Err(error),
        Ok(()) => match prefix(inputs, nodes, nodes.len()) {
            Err(error) => Err(error),
            Ok(kinds) => self::roots(outputs, roots, kinds),
        },
    }
}

}
