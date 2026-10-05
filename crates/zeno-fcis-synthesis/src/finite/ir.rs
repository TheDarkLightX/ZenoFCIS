use alloc::{vec, vec::Vec};
use zeno_fcis_codec::EncodeError;
use zeno_fcis_value::{Value, ValueLimits};

use super::Error;
use super::evaluation::admission::{self, AdmissionFailure};
use super::evaluation::{self, Failure};

pub use super::evaluation::{Domain, MAX_FIELDS, MAX_NODES, Op};

/// Versioned eager, acyclic, checked-i64 semantic profile.
pub const PROFILE: &str = "zeno-fcis/finite-i64/1";

impl Domain {
    pub(super) fn value(self) -> Result<Value, EncodeError> {
        let (min, max) = self.bounds();
        tuple(vec![
            Value::boolean(self.boolean()),
            Value::signed(min.into()),
            Value::signed(max.into()),
        ])
    }
}

impl Op {
    pub(super) fn kind(&self, inputs: &[Domain], previous: &[bool]) -> Result<bool, Error> {
        admission::kind(self, inputs, previous).map_err(admission_error)
    }
    pub(super) fn value(&self) -> Result<Value, EncodeError> {
        let ints = |tag: i128, args: &[i128]| {
            tuple(
                core::iter::once(Value::signed(tag))
                    .chain(args.iter().copied().map(Value::signed))
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
        admission::validate_program(&inputs, &outputs, &nodes, &roots).map_err(admission_error)?;
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
    /// Executes with a fresh library-owned V2 instruction meter.
    ///
    /// Returns usage on success and refusal. This convenience adapter delegates
    /// to the verified `execute_v2` entry point; the adapter itself is not in
    /// the current Verus proof subject. It grants no transition authority.
    #[must_use]
    pub fn execute_v2(&self, input: &[i64], limits: super::V2Limits) -> super::V2ExecutionOutcome {
        super::execution_v2::execute(
            &self.inputs,
            &self.outputs,
            &self.nodes,
            &self.roots,
            input,
            limits,
        )
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
        evaluation::evaluate_into(
            &self.inputs,
            &self.outputs,
            &self.nodes,
            &self.roots,
            input,
            values,
            output,
        )
        .map_err(|failure| match failure {
            Failure::InputDomain => Error::Invalid("input-domain"),
            Failure::Reference => Error::Invalid("node-reference"),
            Failure::Arithmetic => Error::Arithmetic,
            Failure::OutputDomain => Error::Invalid("output-domain"),
        })
    }
    /// Canonical, language-neutral program data, including the semantic profile.
    pub fn value(&self) -> Result<Value, EncodeError> {
        tuple(vec![
            Value::text_ascii_with_limits(PROFILE.into(), ValueLimits::default())
                .map_err(EncodeError::InvalidValue)?,
            schema_value(&self.inputs, &self.outputs)?,
            tuple(
                self.nodes
                    .iter()
                    .map(Op::value)
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
            tuple(
                self.roots
                    .iter()
                    .map(|id| Value::unsigned((*id).into()))
                    .collect(),
            )?,
        ])
    }
}

pub(super) fn admitted(domains: &[Domain], values: &[i64]) -> bool {
    evaluation::admitted(domains, values)
}

fn admission_error(failure: AdmissionFailure) -> Error {
    Error::Invalid(match failure {
        AdmissionFailure::Shape => "program-shape",
        AdmissionFailure::InputReference => "input-reference",
        AdmissionFailure::NodeReference => "node-reference",
        AdmissionFailure::TypeMismatch => "type-mismatch",
        AdmissionFailure::OutputType => "output-type",
    })
}

pub(super) fn validate_shape(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: usize,
    roots: &[u16],
) -> Result<(), Error> {
    admission::validate_shape(inputs, outputs, nodes, roots).map_err(admission_error)
}
pub(super) fn validate_roots(
    outputs: &[Domain],
    roots: &[u16],
    kinds: &[bool],
) -> Result<(), Error> {
    admission::validate_roots(outputs, roots, kinds).map_err(admission_error)
}
pub(super) fn schema_value(inputs: &[Domain], outputs: &[Domain]) -> Result<Value, EncodeError> {
    tuple(vec![
        tuple(
            inputs
                .iter()
                .map(|d| d.value())
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        tuple(
            outputs
                .iter()
                .map(|d| d.value())
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    ])
}
pub(super) fn tuple(values: Vec<Value>) -> Result<Value, EncodeError> {
    Value::tuple(values).map_err(EncodeError::InvalidValue)
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod evaluation_tests;
