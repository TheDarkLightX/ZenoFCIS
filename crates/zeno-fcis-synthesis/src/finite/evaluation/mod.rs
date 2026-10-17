//! Shared eager scalar execution for synthesis and the decision interpreter.
//!
//! The executable bodies in this file are also the Verus proof subject. Program
//! shape/type admission lives in the shared `admission` module. Canonical
//! import and complete decision construction remain separate proof obligations.

pub(crate) mod typed;
use alloc::vec::Vec;

pub(super) mod admission;

/// Maximum nodes in either an implementation or a relation.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_NODES: usize = 256;
/// Maximum fields on either side of a synthesis relation.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_FIELDS: usize = 16;

/// Maximum input positions in the larger finite-i128/2 profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_EXTENDED_INPUTS: usize = 64;
/// Maximum output positions in the larger finite-i128/2 profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_EXTENDED_OUTPUTS: usize = 64;
/// Maximum instructions in the larger finite-i128/2 profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_EXTENDED_NODES: usize = 4096;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Closed scalar domain. Boolean wire values are exactly the integers 0 and 1.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Domain {
    /// Logical values, distinct from integers during type checking.
    Bool,
    /// Inclusive integer bounds.
    Int {
        /// Smallest admitted value.
        min: i128,
        /// Largest admitted value.
        max: i128,
    },
}

impl Domain {
    /// Returns the inclusive wire bounds.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == match self {
            Self::Bool => (0i128, 1i128),
            Self::Int { min, max } => (min, max),
        },
    ))]
    pub const fn bounds(self) -> (i128, i128) {
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
    pub fn contains(self, value: i128) -> bool {
        let (min, max) = self.bounds();
        value >= min && value <= max
    }
}

/// One typed instruction with operands referring to earlier graph nodes.
/// Evaluation is eager, including both arms of `Select`; arithmetic is checked.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Op {
    /// Read a declared input by its zero-based position.
    Input(u16),
    /// An integer constant.
    Int(i128),
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
    /// Checked signed i128 product, independent of fused multiply-divide.
    Mul(u16, u16),
    /// Mathematical floor division; zero divisor and out-of-range result refuse.
    DivFloor(u16, u16),
    /// Mathematical ceiling division, including negative divisors.
    DivCeil(u16, u16),
    /// Nonnegative exact product, positive denominator, final checked i128 floor.
    MulDivFloor(u16, u16, u16),
    /// Nonnegative exact product, positive denominator, final checked i128 ceiling.
    MulDivCeil(u16, u16, u16),
    /// Nonnegative exact three-factor comparison; no intermediate narrowing.
    ProductsGe([u16; 3], [u16; 3]),
    /// Signed full-product floor; positive divisor and final i128 fit required.
    SignedMulDivFloor(u16, u16, u16),
}

impl Op {
    /// Whether this instruction needs the separately admitted arithmetic profile.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == matches!(*self,
        Self::Mul(..) | Self::DivFloor(..) | Self::DivCeil(..) | Self::MulDivFloor(..)
            | Self::MulDivCeil(..) | Self::ProductsGe(..) | Self::SignedMulDivFloor(..)),))]
    pub fn requires_arithmetic_profile(&self) -> bool {
        matches!(
            self,
            Self::Mul(..)
                | Self::DivFloor(..)
                | Self::DivCeil(..)
                | Self::MulDivFloor(..)
                | Self::MulDivCeil(..)
                | Self::ProductsGe(..)
                | Self::SignedMulDivFloor(..)
        )
    }
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
            Self::Mul(a, b) => Self::Mul(a, b),
            Self::DivFloor(a, b) => Self::DivFloor(a, b),
            Self::DivCeil(a, b) => Self::DivCeil(a, b),
            Self::MulDivFloor(a, b, d) => Self::MulDivFloor(a, b, d),
            Self::MulDivCeil(a, b, d) => Self::MulDivCeil(a, b, d),
            Self::ProductsGe(a, b) => Self::ProductsGe(a, b),
            Self::SignedMulDivFloor(a, b, d) => Self::SignedMulDivFloor(a, b, d),
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
pub(super) fn admitted(domains: &[Domain], values: &[i128]) -> bool {
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
pub(super) fn at(values: &[i128], id: u16) -> Option<i128> {
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
pub(super) fn evaluate_node(op: &Op, input: &[i128], values: &[i128]) -> Result<i128, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { typed::spec::legacy_node_embedding(*op, input@, values@); }
    match typed::node(
        typed::legacy_instruction(op),
        typed::Values::Legacy(input),
        typed::Values::Legacy(values),
    ) {
        Ok(value) => match typed::project(value) {
            Some(value) => Ok(value),
            None => Err(Failure::Arithmetic),
        },
        Err(typed::Failure::Reference) => Err(Failure::Reference),
        Err(typed::Failure::Arithmetic | typed::Failure::Kind) => Err(Failure::Arithmetic),
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
    input: &[i128],
    values: &mut Vec<i128>,
    output: &mut Vec<i128>,
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
            output@ == Seq::<i128>::empty(),
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
            && final(values)@ == Seq::<i128>::empty()
            && final(output)@ == Seq::<i128>::empty(),
    },
))]
pub(super) fn evaluate_into(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i128],
    values: &mut Vec<i128>,
    output: &mut Vec<i128>,
) -> Result<(), Failure> {
    let result = evaluate_nodes(inputs, outputs, nodes, roots, input, values, output);
    if result.is_err() {
        values.clear();
        output.clear();
    }
    result
}
