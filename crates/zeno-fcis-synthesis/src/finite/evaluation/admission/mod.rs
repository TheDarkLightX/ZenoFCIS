//! Shared structural admission for the eager finite scalar profile.
//!
//! These executable bodies are checked against erased mathematical admission
//! results, including refusal diagnostics and their original evaluation order.

use super::{
    Domain, MAX_EXTENDED_INPUTS, MAX_EXTENDED_NODES, MAX_EXTENDED_OUTPUTS, MAX_FIELDS, MAX_NODES,
    Op,
};
use alloc::vec::Vec;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(in super::super) mod spec;

/// Exact structural refusal, independent of the frontend's diagnostic strings.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionFailure {
    /// Invalid field counts, node count, root count, or domain interval.
    Shape,
    /// An input operand names an undeclared input.
    InputReference,
    /// A node operand names a missing preceding node.
    NodeReference,
    /// Declared scalar kinds disagree with an instruction's requirements.
    TypeMismatch,
    /// A projected root is absent or has the wrong output scalar kind.
    OutputType,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::node_kind(previous@, id),
))]
fn node_kind(previous: &[bool], id: u16) -> Result<bool, AdmissionFailure> {
    let index = id as usize;
    if index < previous.len() {
        Ok(previous[index])
    } else {
        Err(AdmissionFailure::NodeReference)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::required_node(previous@, id, expected),
))]
fn require_node(previous: &[bool], id: u16, expected: bool) -> Result<(), AdmissionFailure> {
    match node_kind(previous, id) {
        Err(error) => Err(error),
        Ok(actual) => {
            if actual == expected {
                Ok(())
            } else {
                Err(AdmissionFailure::TypeMismatch)
            }
        }
    }
}

/// Determines one instruction's scalar kind with exact diagnostic precedence.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::kind(*op, inputs@, previous@),
))]
pub(crate) fn kind(
    op: &Op,
    inputs: &[Domain],
    previous: &[bool],
) -> Result<bool, AdmissionFailure> {
    match *op {
        Op::Input(id) => {
            let index = id as usize;
            if index < inputs.len() {
                Ok(inputs[index].boolean())
            } else {
                Err(AdmissionFailure::InputReference)
            }
        }
        Op::Int(_) => Ok(false),
        Op::Bool(_) => Ok(true),
        Op::Add(a, b) | Op::Sub(a, b) | Op::Lt(a, b) => {
            require_node(previous, a, false)?;
            require_node(previous, b, false)?;
            Ok(matches!(op, Op::Lt(..)))
        }
        Op::And(a, b) => {
            require_node(previous, a, true)?;
            require_node(previous, b, true)?;
            Ok(true)
        }
        Op::Eq(a, b) => {
            let left = node_kind(previous, a)?;
            let right = node_kind(previous, b)?;
            if left == right {
                Ok(true)
            } else {
                Err(AdmissionFailure::TypeMismatch)
            }
        }
        Op::Not(a) => {
            require_node(previous, a, true)?;
            Ok(true)
        }
        Op::Select(c, a, b) => {
            require_node(previous, c, true)?;
            let yes = node_kind(previous, a)?;
            let no = node_kind(previous, b)?;
            if yes == no {
                Ok(yes)
            } else {
                Err(AdmissionFailure::TypeMismatch)
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::valid_domains(domains@),
))]
fn valid_domains(domains: &[Domain]) -> bool {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= domains.len(),
            forall|i: int| 0 <= i < index ==>
                super::spec::valid_domain(domains@[i]),
        decreases domains.len() - index,
    ))]
    while index < domains.len() {
        if !domains[index].valid() {
            return false;
        }
        index += 1;
    }
    true
}

/// Checks the original profile's complete field, node, root, and domain shape.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::shape(inputs@, outputs@, nodes as nat, roots@),
))]
pub(crate) fn validate_shape(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: usize,
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    if inputs.len() > 2 * MAX_FIELDS
        || outputs.is_empty()
        || outputs.len() > MAX_FIELDS
        || nodes == 0
        || nodes > MAX_NODES
        || outputs.len() != roots.len()
        || !valid_domains(inputs)
        || !valid_domains(outputs)
    {
        Err(AdmissionFailure::Shape)
    } else {
        Ok(())
    }
}

/// Checks the larger profile without changing the original profile's shape.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::extended_shape(inputs@, outputs@, nodes as nat, roots@),
))]
pub(crate) fn validate_extended_shape(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: usize,
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    if inputs.len() > MAX_EXTENDED_INPUTS
        || outputs.is_empty()
        || outputs.len() > MAX_EXTENDED_OUTPUTS
        || nodes == 0
        || nodes > MAX_EXTENDED_NODES
        || outputs.len() != roots.len()
        || !valid_domains(inputs)
        || !valid_domains(outputs)
    {
        Err(AdmissionFailure::Shape)
    } else {
        Ok(())
    }
}

/// Checks roots over the overlap of the output/root slices, preserving zip.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::roots(outputs@, roots@, kinds@),
))]
pub(crate) fn validate_roots(
    outputs: &[Domain],
    roots: &[u16],
    kinds: &[bool],
) -> Result<(), AdmissionFailure> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= outputs.len(),
            index <= roots.len(),
            forall|i: int| 0 <= i < index ==>
                spec::root_valid(outputs@[i], roots@[i], kinds@),
        decreases outputs.len() - index,
    ))]
    while index < outputs.len() && index < roots.len() {
        let root = roots[index] as usize;
        if root >= kinds.len() || kinds[root] != outputs[index].boolean() {
            return Err(AdmissionFailure::OutputType);
        }
        index += 1;
    }
    Ok(())
}

/// Admits shape first, then every topological instruction, then result roots.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::program(inputs@, outputs@, nodes@, roots@),
))]
pub(crate) fn validate_program(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    validate_shape(inputs, outputs, nodes.len(), roots)?;
    validate_graph(inputs, outputs, nodes, roots)
}

/// Admits the larger shape, then the identical typed topological graph.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::extended_program(inputs@, outputs@, nodes@, roots@),
))]
pub(crate) fn validate_extended_program(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    validate_extended_shape(inputs, outputs, nodes.len(), roots)?;
    validate_graph(inputs, outputs, nodes, roots)
}

/// Shared instruction/root admission with unchanged first-error precedence.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::graph(inputs@, outputs@, nodes@, roots@),
))]
fn validate_graph(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    let mut kinds = Vec::with_capacity(nodes.len());
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            index <= nodes.len(),
            kinds@.len() == index,
            spec::prefix(inputs@, nodes@, index as nat) == Ok(kinds@),
        decreases nodes.len() - index,
    ))]
    while index < nodes.len() {
        match kind(&nodes[index], inputs, &kinds) {
            Ok(value) => kinds.push(value),
            Err(error) => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    spec::prefix_failure_persists(inputs@, nodes@,
                        (index + 1) as nat, nodes@.len(), error);
                }
                return Err(error);
            }
        }
        index += 1;
    }
    validate_roots(outputs, roots, &kinds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn chain(count: usize) -> Vec<Op> {
        let mut nodes = vec![Op::Int(1)];
        for i in 1..count {
            let previous = u16::try_from(i - 1).unwrap_or_else(|_| panic!("test node index"));
            nodes.push(Op::Add(previous, 0));
        }
        nodes
    }

    #[test]
    fn larger_graph_admission_preserves_the_original_boundary() {
        let outputs = [Domain::Int { min: 0, max: 4096 }];
        for count in [256, 257, 294, 4096] {
            let nodes = chain(count);
            let roots = [u16::try_from(count - 1).unwrap_or_else(|_| panic!("test root"))];
            assert_eq!(
                validate_extended_program(&[], &outputs, &nodes, &roots),
                Ok(())
            );
            assert_eq!(
                validate_program(&[], &outputs, &nodes, &roots),
                if count <= 256 {
                    Ok(())
                } else {
                    Err(AdmissionFailure::Shape)
                }
            );
        }
        // Shape still precedes malformed references and roots above the cap.
        let nodes: Vec<_> = (0..4097).map(|_| Op::Input(99)).collect();
        assert_eq!(
            validate_extended_program(&[], &outputs, &nodes, &[u16::MAX]),
            Err(AdmissionFailure::Shape)
        );
    }

    #[test]
    fn larger_field_admission_is_complete_and_bounded() {
        let inputs = [Domain::Bool; 64];
        let outputs = [Domain::Bool; 64];
        let nodes: Vec<_> = (0..64).map(Op::Input).collect();
        let roots: Vec<_> = (0..64).rev().collect();
        assert_eq!(
            validate_extended_program(&inputs, &outputs, &nodes, &roots),
            Ok(())
        );
        assert_eq!(
            validate_program(&inputs, &outputs, &nodes, &roots),
            Err(AdmissionFailure::Shape)
        );
        assert_eq!(
            validate_extended_shape(&[Domain::Bool; 65], &outputs, 64, &roots),
            Err(AdmissionFailure::Shape)
        );
        assert_eq!(
            validate_extended_shape(&inputs, &[Domain::Bool; 65], 64, &[0; 65]),
            Err(AdmissionFailure::Shape)
        );
        assert_eq!(
            validate_extended_shape(&inputs, &outputs, 64, &roots[..63]),
            Err(AdmissionFailure::Shape)
        );
        assert_eq!(
            validate_extended_shape(&[Domain::Int { min: 1, max: 0 }], &outputs, 64, &roots),
            Err(AdmissionFailure::Shape)
        );
    }

    #[test]
    fn a_large_graph_keeps_topological_and_type_failure_precedence() {
        let outputs = [Domain::Int { min: 0, max: 4096 }];
        let mut nodes = chain(294);
        nodes[257] = Op::Input(0);
        nodes[258] = Op::Add(u16::MAX, 0);
        assert_eq!(
            validate_extended_program(&[], &outputs, &nodes, &[u16::MAX]),
            Err(AdmissionFailure::InputReference)
        );
        nodes[257] = Op::Add(257, 0);
        assert_eq!(
            validate_extended_program(&[], &outputs, &nodes, &[u16::MAX]),
            Err(AdmissionFailure::NodeReference)
        );
        nodes[257] = Op::Bool(true);
        nodes[258] = Op::Add(257, u16::MAX);
        assert_eq!(
            validate_extended_program(&[], &outputs, &nodes, &[u16::MAX]),
            Err(AdmissionFailure::TypeMismatch)
        );
        nodes[258] = Op::Int(0);
        assert_eq!(
            validate_extended_program(&[], &outputs, &nodes, &[u16::MAX]),
            Err(AdmissionFailure::OutputType)
        );
    }
}
