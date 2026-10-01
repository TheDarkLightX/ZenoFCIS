// Admission oracle extracted from ir.rs at 4dd979b95433bc70dcdc3ce834ff6db90eef806f.
// Runtime comparisons preserve the old diagnostics; this is not a proof subject.
use crate::evaluation::{Domain, MAX_FIELDS, MAX_NODES, Op};
use alloc::vec::Vec;
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Error {
    Invalid(&'static str),
}
impl Op {
    pub(super) fn legacy_kind(&self, inputs: &[Domain], previous: &[bool]) -> Result<bool, Error> {
        let node = |id: u16| {
            previous
                .get(usize::from(id))
                .copied()
                .ok_or(Error::Invalid("node-reference"))
        };
        let require = |actual: bool, expected: bool| {
            if actual == expected {
                Ok(())
            } else {
                Err(Error::Invalid("type-mismatch"))
            }
        };
        match *self {
            Self::Input(id) => inputs
                .get(usize::from(id))
                .map(|d| d.boolean())
                .ok_or(Error::Invalid("input-reference")),
            Self::Int(_) => Ok(false),
            Self::Bool(_) => Ok(true),
            Self::Add(a, b) | Self::Sub(a, b) | Self::Lt(a, b) => {
                require(node(a)?, false)?;
                require(node(b)?, false)?;
                Ok(matches!(self, Self::Lt(..)))
            }
            Self::And(a, b) => {
                require(node(a)?, true)?;
                require(node(b)?, true)?;
                Ok(true)
            }
            Self::Eq(a, b) => {
                require(node(a)?, node(b)?)?;
                Ok(true)
            }
            Self::Not(a) => {
                require(node(a)?, true)?;
                Ok(true)
            }
            Self::Select(c, a, b) => {
                require(node(c)?, true)?;
                let kind = node(a)?;
                require(node(b)?, kind)?;
                Ok(kind)
            }
        }
    }
}
pub(super) fn legacy_validate_shape(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: usize,
    roots: &[u16],
) -> Result<(), Error> {
    if inputs.len() > 2 * MAX_FIELDS
        || outputs.is_empty()
        || outputs.len() > MAX_FIELDS
        || nodes == 0
        || nodes > MAX_NODES
        || outputs.len() != roots.len()
        || !inputs.iter().chain(outputs).all(|d| d.valid())
    {
        return Err(Error::Invalid("program-shape"));
    }
    Ok(())
}
pub(super) fn legacy_validate_roots(
    outputs: &[Domain],
    roots: &[u16],
    kinds: &[bool],
) -> Result<(), Error> {
    for (domain, id) in outputs.iter().zip(roots) {
        if kinds.get(usize::from(*id)) != Some(&domain.boolean()) {
            return Err(Error::Invalid("output-type"));
        }
    }
    Ok(())
}

pub(crate) fn legacy_program(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
) -> Result<(), Error> {
    legacy_validate_shape(inputs, outputs, nodes.len(), roots)?;
    let mut kinds = Vec::with_capacity(nodes.len());
    for op in nodes {
        kinds.push(op.legacy_kind(inputs, &kinds)?);
    }
    legacy_validate_roots(outputs, roots, &kinds)?;
    Ok(())
}
