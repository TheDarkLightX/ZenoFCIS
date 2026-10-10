//! Shared eager scalar execution for synthesis and the decision interpreter.
//!
//! The executable bodies in this file are also the Verus proof subject. Program
//! shape/type admission lives in the shared `admission` module. Canonical
//! import and complete decision construction remain separate proof obligations.

use alloc::vec::Vec;

pub(super) mod admission;

/// Maximum nodes in either an implementation or a relation.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_NODES: usize = 256;
/// Maximum fields on either side of a synthesis relation.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_FIELDS: usize = 16;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Closed scalar domain. Boolean wire values are exactly the integers 0 and 1.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Domain {
    /// Logical values, distinct from integers during type checking.
    Bool,
    /// Inclusive integer bounds.
    Int {
        /// Smallest admitted value.
        min: i64,
        /// Largest admitted value.
        max: i64,
    },
}

impl Domain {
    /// Returns the inclusive wire bounds.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == match self {
            Self::Bool => (0i64, 1i64),
            Self::Int { min, max } => (min, max),
        },
    ))]
    pub const fn bounds(self) -> (i64, i64) {
        match self {
            Self::Bool => (0, 1),
            Self::Int { min, max } => (min, max),
        }
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == spec::valid_domain(self),
    ))]
    pub(super) fn valid(self) -> bool {
        let (min, max) = self.bounds();
        min <= max
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == matches!(self, Self::Bool),
    ))]
    pub(super) fn boolean(self) -> bool {
        matches!(self, Self::Bool)
    }

    /// Checks an exact wire scalar against this domain.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == spec::contains(self, value),
    ))]
    pub fn contains(self, value: i64) -> bool {
        let (min, max) = self.bounds();
        value >= min && value <= max
    }
}

/// One typed instruction with operands referring to earlier graph nodes.
/// Evaluation is eager, including both arms of `Select`; add/sub are checked.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub enum Op {
    /// Read a declared input by its zero-based position.
    Input(u16),
    /// An integer constant.
    Int(i64),
    /// A Boolean constant.
    Bool(bool),
    /// Checked signed addition.
    Add(u16, u16),
    /// Checked signed subtraction.
    Sub(u16, u16),
    /// Equality between values of the same scalar kind.
    Eq(u16, u16),
    /// Signed integer less-than.
    Lt(u16, u16),
    /// Boolean conjunction.
    And(u16, u16),
    /// Boolean negation.
    Not(u16),
    /// Select between equal-kind values using a Boolean condition.
    Select(u16, u16, u16),
}

impl Clone for Op {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == *self,
    ))]
    fn clone(&self) -> Self {
        match *self {
            Self::Input(a) => Self::Input(a),
            Self::Int(a) => Self::Int(a),
            Self::Bool(a) => Self::Bool(a),
            Self::Add(a, b) => Self::Add(a, b),
            Self::Sub(a, b) => Self::Sub(a, b),
            Self::Eq(a, b) => Self::Eq(a, b),
            Self::Lt(a, b) => Self::Lt(a, b),
            Self::And(a, b) => Self::And(a, b),
            Self::Not(a) => Self::Not(a),
            Self::Select(c, a, b) => Self::Select(c, a, b),
        }
    }
}

/// Exact execution failure, independent of the frontend's diagnostic format.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    InputDomain,
    Reference,
    Arithmetic,
    OutputDomain,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::admitted(domains@, values@),
))]
pub(super) fn admitted(domains: &[Domain], values: &[i64]) -> bool {
    if domains.len() != values.len() {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            domains.len() == values.len(),
            index <= domains.len(),
            forall|i: int| 0 <= i < index ==> spec::contains(domains@[i], values@[i]),
        decreases domains.len() - index,
    ))]
    while index < domains.len() {
        if !domains[index].contains(values[index]) {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == if (id as int) < values@.len() {
        Some(values@[id as int])
    } else {
        None
    },
))]
pub(super) fn at(values: &[i64], id: u16) -> Option<i64> {
    let index = id as usize;
    if index < values.len() {
        Some(values[index])
    } else {
        None
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::node(*op, input@, values@),
))]
pub(super) fn evaluate_node(op: &Op, input: &[i64], values: &[i64]) -> Result<i64, Failure> {
    match *op {
        Op::Input(id) => match at(input, id) {
            Some(value) => Ok(value),
            None => Err(Failure::Reference),
        },
        Op::Int(value) => Ok(value),
        Op::Bool(value) => Ok(value as i64),
        Op::Not(id) => match at(values, id) {
            Some(value) => Ok((value == 0) as i64),
            None => Err(Failure::Reference),
        },
        Op::Select(condition, yes, no) => {
            match (at(values, condition), at(values, yes), at(values, no)) {
                (Some(condition), Some(yes), Some(no)) => Ok(if condition == 1 { yes } else { no }),
                _ => Err(Failure::Reference),
            }
        }
        Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
            let (Some(left), Some(right)) = (at(values, a), at(values, b)) else {
                return Err(Failure::Reference);
            };
            match *op {
                Op::Add(_, _) => match left.checked_add(right) {
                    Some(value) => Ok(value),
                    None => Err(Failure::Arithmetic),
                },
                Op::Sub(_, _) => match left.checked_sub(right) {
                    Some(value) => Ok(value),
                    None => Err(Failure::Arithmetic),
                },
                Op::Eq(_, _) => Ok((left == right) as i64),
                Op::Lt(_, _) => Ok((left < right) as i64),
                _ => Ok((left == 1 && right == 1) as i64),
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Ok(()) => spec::execution(inputs@, outputs@, nodes@, roots@, input@)
            == Ok(final(output)@),
        Err(error) => spec::execution(inputs@, outputs@, nodes@, roots@, input@)
            == Err(error),
    },
))]
fn evaluate_nodes(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i64],
    values: &mut Vec<i64>,
    output: &mut Vec<i64>,
) -> Result<(), Failure> {
    values.clear();
    output.clear();
    if !admitted(inputs, input) {
        return Err(Failure::InputDomain);
    }
    // `reserve` preserves values and can overallocate. Allocation itself remains
    // part of the standard-library trusted base, outside the logical-step proof.
    values.reserve(nodes.len());
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            spec::admitted(inputs@, input@),
            index <= nodes.len(),
            values@.len() == index,
            spec::prefix(nodes@, input@, index as nat) == Ok(values@),
            output@ == Seq::<i64>::empty(),
        decreases nodes.len() - index,
    ))]
    while index < nodes.len() {
        match evaluate_node(&nodes[index], input, values) {
            Ok(value) => {
                values.push(value);
            }
            Err(error) => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    spec::prefix_failure_persists(nodes@, input@,
                        (index + 1) as nat, nodes@.len() as nat, error);
                }
                return Err(error);
            }
        }
        index += 1;
    }
    output.reserve(roots.len());
    let mut root = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            spec::admitted(inputs@, input@),
            spec::prefix(nodes@, input@, nodes@.len() as nat) == Ok(values@),
            root <= roots.len(),
            output@.len() == root,
            spec::projection(values@, roots@, root as nat) == Ok(output@),
        decreases roots.len() - root,
    ))]
    while root < roots.len() {
        match at(values, roots[root]) {
            Some(value) => {
                output.push(value);
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    spec::projection_failure_persists(values@, roots@,
                        (root + 1) as nat, roots@.len() as nat);
                }
                return Err(Failure::Reference);
            }
        }
        root += 1;
    }
    if !admitted(outputs, output) {
        return Err(Failure::OutputDomain);
    }
    Ok(())
}

/// Evaluates the complete eager graph and clears both buffers on every refusal.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Ok(()) => spec::execution(inputs@, outputs@, nodes@, roots@, input@)
            == Ok(final(output)@),
        Err(error) => spec::execution(inputs@, outputs@, nodes@, roots@, input@)
            == Err(error)
            && final(values)@ == Seq::<i64>::empty()
            && final(output)@ == Seq::<i64>::empty(),
    },
))]
pub(super) fn evaluate_into(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i64],
    values: &mut Vec<i64>,
    output: &mut Vec<i64>,
) -> Result<(), Failure> {
    let result = evaluate_nodes(inputs, outputs, nodes, roots, input, values, output);
    if result.is_err() {
        values.clear();
        output.clear();
    }
    result
}
