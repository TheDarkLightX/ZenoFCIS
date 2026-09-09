use alloc::{vec, vec::Vec};
use zeno_fcis_value::Value;

use super::Error;

/// Versioned eager, acyclic, checked-i64 semantic profile.
pub const PROFILE: &str = "zeno-fcis/finite-i64/1";
/// Maximum nodes in either an implementation or a relation.
pub const MAX_NODES: usize = 256;
/// Maximum fields on either side of a synthesis relation.
pub const MAX_FIELDS: usize = 16;

/// Closed scalar domain. Boolean wire values are exactly the integers 0 and 1.
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
    pub const fn bounds(self) -> (i64, i64) {
        match self {
            Self::Bool => (0, 1),
            Self::Int { min, max } => (min, max),
        }
    }
    pub(super) fn valid(self) -> bool {
        let (min, max) = self.bounds();
        min <= max
    }
    pub(super) fn boolean(self) -> bool {
        matches!(self, Self::Bool)
    }
    /// Checks an exact wire scalar against this domain.
    #[must_use]
    pub fn contains(self, value: i64) -> bool {
        let (min, max) = self.bounds();
        value >= min && value <= max
    }
    pub(super) fn value(self) -> Value {
        let (min, max) = self.bounds();
        tuple(vec![
            Value::Bool(self.boolean()),
            Value::I128(min.into()),
            Value::I128(max.into()),
        ])
    }
}

/// One typed instruction. Node operands refer strictly to earlier instructions.
/// Evaluation is eager, including both arms of `Select`; every add/sub is checked.
#[derive(Clone, Debug, Eq, PartialEq)]
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

impl Op {
    pub(super) fn kind(&self, inputs: &[Domain], previous: &[bool]) -> Result<bool, Error> {
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
    pub(super) fn value(&self) -> Value {
        let ints = |tag: i128, args: &[i128]| {
            tuple(
                core::iter::once(Value::I128(tag))
                    .chain(args.iter().copied().map(Value::I128))
                    .collect(),
            )
        };
        match *self {
            Self::Input(a) => ints(0, &[a.into()]),
            Self::Int(a) => ints(1, &[a.into()]),
            Self::Bool(a) => ints(2, &[i128::from(a)]),
            Self::Add(a, b) => ints(3, &[a.into(), b.into()]),
            Self::Sub(a, b) => ints(4, &[a.into(), b.into()]),
            Self::Eq(a, b) => ints(5, &[a.into(), b.into()]),
            Self::Lt(a, b) => ints(6, &[a.into(), b.into()]),
            Self::And(a, b) => ints(7, &[a.into(), b.into()]),
            Self::Not(a) => ints(8, &[a.into()]),
            Self::Select(c, a, b) => ints(9, &[c.into(), a.into(), b.into()]),
        }
    }
}

/// Structurally validated, closed expression graph; construction is not a proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    nodes: Vec<Op>,
    roots: Vec<u16>,
}

impl Program {
    /// Validates bounds, types, topological references, and complete result shape.
    pub fn try_new(
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        nodes: Vec<Op>,
        roots: Vec<u16>,
    ) -> Result<Self, Error> {
        validate_shape(&inputs, &outputs, nodes.len(), &roots)?;
        let mut kinds = Vec::with_capacity(nodes.len());
        for op in &nodes {
            kinds.push(op.kind(&inputs, &kinds)?);
        }
        validate_roots(&outputs, &roots, &kinds)?;
        Ok(Self {
            inputs,
            outputs,
            nodes,
            roots,
        })
    }
    /// Input domains in ABI order.
    #[must_use]
    pub fn inputs(&self) -> &[Domain] {
        &self.inputs
    }
    /// Output domains in ABI order.
    #[must_use]
    pub fn outputs(&self) -> &[Domain] {
        &self.outputs
    }
    /// Typed topological instructions.
    #[must_use]
    pub fn nodes(&self) -> &[Op] {
        &self.nodes
    }
    /// Result nodes in ABI order.
    #[must_use]
    pub fn roots(&self) -> &[u16] {
        &self.roots
    }
    /// Evaluates explicit inputs without any ambient state or effects.
    pub fn evaluate(&self, input: &[i64]) -> Result<Vec<i64>, Error> {
        let mut values = Vec::new();
        let mut output = Vec::new();
        self.evaluate_into(input, &mut values, &mut output)?;
        Ok(output)
    }
    /// Evaluates into caller-owned buffers, reusing their existing capacity.
    ///
    /// Both buffers are cleared before use and again on every failure, so a
    /// rejected input, a trapped operation, or a rejected output tuple cannot
    /// leave a partial row visible to the next evaluation.
    pub(super) fn evaluate_into(
        &self,
        input: &[i64],
        values: &mut Vec<i64>,
        output: &mut Vec<i64>,
    ) -> Result<(), Error> {
        let result = self.evaluate_nodes(input, values, output);
        if result.is_err() {
            values.clear();
            output.clear();
        }
        result
    }
    fn evaluate_nodes(
        &self,
        input: &[i64],
        values: &mut Vec<i64>,
        output: &mut Vec<i64>,
    ) -> Result<(), Error> {
        values.clear();
        output.clear();
        if !admitted(&self.inputs, input) {
            return Err(Error::Invalid("input-domain"));
        }
        values.reserve_exact(self.nodes.len());
        for op in &self.nodes {
            let at = |id: u16| values[usize::from(id)];
            let value = match *op {
                Op::Input(id) => input[usize::from(id)],
                Op::Int(v) => v,
                Op::Bool(v) => i64::from(v),
                Op::Add(a, b) => at(a).checked_add(at(b)).ok_or(Error::Arithmetic)?,
                Op::Sub(a, b) => at(a).checked_sub(at(b)).ok_or(Error::Arithmetic)?,
                Op::Eq(a, b) => i64::from(at(a) == at(b)),
                Op::Lt(a, b) => i64::from(at(a) < at(b)),
                Op::And(a, b) => i64::from(at(a) == 1 && at(b) == 1),
                Op::Not(a) => i64::from(at(a) == 0),
                Op::Select(c, a, b) => {
                    if at(c) == 1 {
                        at(a)
                    } else {
                        at(b)
                    }
                }
            };
            values.push(value);
        }
        output.reserve_exact(self.roots.len());
        for id in &self.roots {
            output.push(values[usize::from(*id)]);
        }
        if !admitted(&self.outputs, output) {
            return Err(Error::Invalid("output-domain"));
        }
        Ok(())
    }
    /// Canonical, language-neutral program data, including the semantic profile.
    #[must_use]
    pub fn value(&self) -> Value {
        tuple(vec![
            Value::Text(PROFILE.into()),
            schema_value(&self.inputs, &self.outputs),
            tuple(self.nodes.iter().map(Op::value).collect()),
            tuple(
                self.roots
                    .iter()
                    .map(|id| Value::U128((*id).into()))
                    .collect(),
            ),
        ])
    }
}

pub(super) fn admitted(domains: &[Domain], values: &[i64]) -> bool {
    domains.len() == values.len() && domains.iter().zip(values).all(|(d, v)| d.contains(*v))
}
pub(super) fn validate_shape(
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
pub(super) fn validate_roots(
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
pub(super) fn schema_value(inputs: &[Domain], outputs: &[Domain]) -> Value {
    tuple(vec![
        tuple(inputs.iter().map(|d| d.value()).collect()),
        tuple(outputs.iter().map(|d| d.value()).collect()),
    ])
}
pub(super) fn tuple(values: Vec<Value>) -> Value {
    Value::Tuple(values.into_boxed_slice())
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod evaluation_tests;
