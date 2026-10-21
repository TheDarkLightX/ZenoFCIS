//! A closed one-step theory with exhaustive relational checking.
//!
//! Inputs and outputs are positional tuples. Bool is distinct during type
//! checking and encoded as 0/1 at the language boundary. This profile proves
//! neither unbounded temporal properties nor adequacy of the reviewed contract.

/// Verified integer byte primitives; schema admission remains a separate stage.
pub mod canonical_v2;
/// Finite exit-path search and independent decreasing-rank verification.
pub mod completion;
pub mod emit;
mod evaluation;
mod execution_v2;
mod value;

pub use evaluation::admission::AdmissionFailure as V2NumericAdmissionFailure;
pub use evaluation::typed::NumericOp as V2NumericOp;
#[doc(hidden)]
pub use execution_v2::historical_i64;
pub use execution_v2::composition as v2_composition;
/// Immutable original-schema value witness; only checked ingress constructs it.
pub use execution_v2::compound::Value as V2CanonicalValue;
pub use execution_v2::continuation as v2_continuation;
pub use execution_v2::laws as v2_laws;
pub use execution_v2::numeric::{
    Failure as V2NumericFailure, MAX_NUMERIC_INPUTS, MAX_NUMERIC_NODES, MAX_NUMERIC_OUTPUTS,
    NumericProgram as V2NumericProgram, Outcome as V2NumericOutcome, execute as execute_numeric_v2,
};
/// Pure, nonpublishing oracle relations and checked full-width arithmetic.
#[cfg(not(verus_keep_ghost))]
pub use execution_v2::oracle as v2_oracle;
#[cfg(test)]
/// Unchanged emitted order fixture for complete-decision test comparisons.
pub use execution_v2::order_fixture_transition as v2_order_fixture_transition;
pub use value::{Atom as V2NumericAtom, Domain as V2NumericDomain, IntegerWidth as V2IntegerWidth};
mod ir;

#[cfg(test)]
mod choice_tests;

pub use execution_v2::{
    AccessAttempt as V2ReadAttempt, Failure as V2ExecutionFailure, InputField as V2InputField,
    InputLeaf as V2InputLeaf, InputVariant as V2InputVariant, Limits as V2Limits,
    MeterFailure as V2MeterFailure, Outcome as V2ExecutionOutcome,
    RecordFailure as V2RecordFailure, RecordProjection as V2RecordProjection,
    Resource as V2Resource, ScalarProgram as V2ScalarProgram, Usage as V2Usage,
    execute as execute_v2, project_record as project_record_v2, zero_limits as v2_zero_limits,
};
pub use ir::{
    ARITHMETIC_EXTENDED_PROFILE, ARITHMETIC_PROFILE, Domain, EXTENDED_PROFILE, MAX_EXTENDED_INPUTS,
    MAX_EXTENDED_NODES, MAX_EXTENDED_OUTPUTS, MAX_FIELDS, MAX_NODES, Op, PROFILE, Program,
};

/// V2 instruction-attempt accounting, separate from the scalar wire profile.
pub const V2_EXECUTION_PROFILE: &str = "zeno-fcis/finite-instruction-meter/2";

/// V2 ingress bytes and protected flat-record field attempts.
pub const V2_RECORD_PROFILE: &str = "zeno-fcis/finite-protected-record/2";

/// Every source file, relative to this directory, reachable from finite
/// program admission, evaluation and metered execution, with its
/// specifications and proofs, and its exact bytes.
///
/// The synthesis checker identity and `finite_runtime::evaluator_hash` both
/// bind this list. A test compares it with the module tree, so a new file
/// under these directories refuses until it is hashed or reviewed out.
const SEMANTIC_SOURCES: &[(&str, &[u8])] = &[
    (
        "canonical_v2/mod.rs",
        include_bytes!("canonical_v2/mod.rs") as &[u8],
    ),
    (
        "canonical_v2/schema.rs",
        include_bytes!("canonical_v2/schema.rs") as &[u8],
    ),
    (
        "canonical_v2/schema/admission.rs",
        include_bytes!("canonical_v2/schema/admission.rs") as &[u8],
    ),
    (
        "canonical_v2/schema/admission/spec.rs",
        include_bytes!("canonical_v2/schema/admission/spec.rs") as &[u8],
    ),
    (
        "canonical_v2/schema/spec.rs",
        include_bytes!("canonical_v2/schema/spec.rs") as &[u8],
    ),
    (
        "canonical_v2/spec.rs",
        include_bytes!("canonical_v2/spec.rs") as &[u8],
    ),
    (
        "evaluation/admission/mod.rs",
        include_bytes!("evaluation/admission/mod.rs") as &[u8],
    ),
    (
        "evaluation/admission/spec.rs",
        include_bytes!("evaluation/admission/spec.rs") as &[u8],
    ),
    (
        "evaluation/mod.rs",
        include_bytes!("evaluation/mod.rs") as &[u8],
    ),
    (
        "evaluation/spec.rs",
        include_bytes!("evaluation/spec.rs") as &[u8],
    ),
    (
        "evaluation/typed.rs",
        include_bytes!("evaluation/typed.rs") as &[u8],
    ),
    (
        "evaluation/typed/admission.rs",
        include_bytes!("evaluation/typed/admission.rs") as &[u8],
    ),
    (
        "evaluation/typed/admission/spec.rs",
        include_bytes!("evaluation/typed/admission/spec.rs") as &[u8],
    ),
    (
        "evaluation/typed/spec.rs",
        include_bytes!("evaluation/typed/spec.rs") as &[u8],
    ),
    (
        "execution_v2/compound.rs",
        include_bytes!("execution_v2/compound.rs") as &[u8],
    ),
    (
        "execution_v2/compound/spec.rs",
        include_bytes!("execution_v2/compound/spec.rs") as &[u8],
    ),
    (
        "execution_v2/decision.rs",
        include_bytes!("execution_v2/decision.rs") as &[u8],
    ),
    (
        "execution_v2/decision/spec.rs",
        include_bytes!("execution_v2/decision/spec.rs") as &[u8],
    ),
    (
        "execution_v2/input_view.rs",
        include_bytes!("execution_v2/input_view.rs") as &[u8],
    ),
    (
        "execution_v2/input_view/spec.rs",
        include_bytes!("execution_v2/input_view/spec.rs") as &[u8],
    ),
    (
        "execution_v2/input_view/typed.rs",
        include_bytes!("execution_v2/input_view/typed.rs") as &[u8],
    ),
    (
        "execution_v2/input_view/typed/spec.rs",
        include_bytes!("execution_v2/input_view/typed/spec.rs") as &[u8],
    ),
    (
        "execution_v2/meter.rs",
        include_bytes!("execution_v2/meter.rs") as &[u8],
    ),
    (
        "execution_v2/mod.rs",
        include_bytes!("execution_v2/mod.rs") as &[u8],
    ),
    (
        "execution_v2/nonbinding_step.rs",
        include_bytes!("execution_v2/nonbinding_step.rs") as &[u8],
    ),
    (
        "execution_v2/numeric.rs",
        include_bytes!("execution_v2/numeric.rs") as &[u8],
    ),
    (
        "execution_v2/numeric/spec.rs",
        include_bytes!("execution_v2/numeric/spec.rs") as &[u8],
    ),
    (
        "execution_v2/spec.rs",
        include_bytes!("execution_v2/spec.rs") as &[u8],
    ),
    (
        "execution_v2/util.rs",
        include_bytes!("execution_v2/util.rs") as &[u8],
    ),
    ("ir.rs", include_bytes!("ir.rs") as &[u8]),
    ("ir/program.rs", include_bytes!("ir/program.rs") as &[u8]),
    ("value/mod.rs", include_bytes!("value/mod.rs") as &[u8]),
    (
        "value/scalar.rs",
        include_bytes!("value/scalar.rs") as &[u8],
    ),
    ("value/spec.rs", include_bytes!("value/spec.rs") as &[u8]),
    ("value/wide.rs", include_bytes!("value/wide.rs") as &[u8]),
];

/// The row count, then each path and its bytes, each preceded by its u64
/// big-endian length, so distinct source lists cannot frame the same bytes.
pub(crate) fn semantic_source_bytes() -> Vec<u8> {
    let mut framed = Vec::new();
    framed.extend_from_slice(&(SEMANTIC_SOURCES.len() as u64).to_be_bytes());
    for (path, bytes) in SEMANTIC_SOURCES {
        framed.extend_from_slice(&(path.len() as u64).to_be_bytes());
        framed.extend_from_slice(path.as_bytes());
        framed.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        framed.extend_from_slice(bytes);
    }
    framed
}

use crate::{
    Assignment, CandidateChecker, CheckResult, Hole, HoleId, SearchBudget, SearchResult,
    SynthesisBindings, SynthesisCertificate, SynthesisError, SynthesisProblem, hash_bytes,
    hash_canonical, search,
};
use alloc::{vec, vec::Vec};
use core::fmt;
use ir::{schema_value, tuple, validate_roots, validate_shape};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_value::Value;

// All source commitments are fixed-width 32-byte leaves. Checked construction
// retains the original wire bytes without exposing a raw Value variant.
fn digest_value(hash: Hash32) -> Result<Value, Error> {
    Value::bytes(hash.as_bytes().to_vec()).map_err(|_| Error::Invalid("finite-source-digest"))
}

/// Canonical scalar tuple shared by the optional finite operation profiles.
/// Boolean positions retain the finite ABI's exact integer values 0 and 1.
fn finite_tuple(values: &[i128]) -> Result<Value, zeno_fcis_codec::EncodeError> {
    tuple(values.iter().map(|value| Value::signed(*value)).collect())
}

/// Hard input-space ceiling, checked before any enumeration.
pub const MAX_INPUTS: u64 = 65_536;
/// Hard output-space ceiling for the realizability check.
pub const MAX_OUTPUTS: u64 = 4_096;
/// Hard bound on the conservative total number of evaluated graph nodes.
pub const MAX_STEPS: u64 = 100_000_000;

/// Failure never usable as positive synthesis evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Invalid shape, scope, type, or domain with a stable diagnostic code.
    Invalid(&'static str),
    /// A hard resource ceiling would be exceeded.
    Limit(&'static str),
    /// The declared budget cannot cover the entire operation.
    Budget {
        /// Budget dimension.
        resource: &'static str,
        /// Exact conservative requirement.
        required: u64,
        /// Declared maximum.
        declared: u64,
    },
    /// Checked i128 arithmetic trapped.
    Arithmetic,
    /// The reviewed relation is not total on an admitted pair.
    ContractTrap {
        /// Exact input witness.
        input: Vec<i128>,
        /// Exact output witness.
        output: Vec<i128>,
    },
    /// Failure in the existing canonical synthesis kernel.
    Search(SynthesisError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<SynthesisError> for Error {
    fn from(error: SynthesisError) -> Self {
        Self::Search(error)
    }
}
impl From<zeno_fcis_codec::EncodeError> for Error {
    fn from(error: zeno_fcis_codec::EncodeError) -> Self {
        Self::Search(SynthesisError::Encode(error))
    }
}
#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Exact operation budgets, applied before any partial search.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    /// Must cover the entire candidate Cartesian product.
    pub max_assignments: u64,
    /// Must cover realizability and worst-case candidate checking graph work.
    pub max_steps: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_assignments: 100_000,
            max_steps: MAX_STEPS,
        }
    }
}

/// A fixed instruction or a stable typed hole with a closed instruction grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Slot {
    /// Reviewed instruction.
    Fixed(Op),
    /// Alternatives may reference inputs or strictly earlier typed nodes.
    Choice {
        /// Nonzero, unique stable hole identifier.
        id: u32,
        /// Complete reviewed alternatives; canonicalized at construction.
        alternatives: Vec<Op>,
    },
}

/// Owned, typed grammar. It cannot contain cycles, ambient inputs, or effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sketch {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    nodes: Vec<Slot>,
    roots: Vec<u16>,
    holes: Vec<Hole>,
}
impl Sketch {
    /// Validates every alternative, even alternatives never selected by search.
    pub fn try_new(
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        mut nodes: Vec<Slot>,
        roots: Vec<u16>,
    ) -> Result<Self, Error> {
        validate_shape(&inputs, &outputs, nodes.len(), &roots)?;
        if inputs.len() > MAX_FIELDS {
            return Err(Error::Limit("input-fields"));
        }
        let mut kinds = Vec::new();
        let mut holes = Vec::new();
        for slot in &mut nodes {
            let kind = match slot {
                Slot::Fixed(op) => op.kind(&inputs, &kinds)?,
                Slot::Choice { id, alternatives } => {
                    let hole_id = HoleId::try_new(*id)?;
                    if holes.len() >= crate::MAX_HOLES {
                        return Err(Error::Limit("holes"));
                    }
                    if alternatives.is_empty() {
                        return Err(Error::Search(SynthesisError::EmptyCandidateDomain(hole_id)));
                    }
                    if alternatives.len() > crate::MAX_VALUES_PER_HOLE {
                        return Err(Error::Search(SynthesisError::TooManyCandidateValues(
                            hole_id,
                        )));
                    }
                    let kind = alternatives[0].kind(&inputs, &kinds)?;
                    for op in alternatives.iter() {
                        if op.kind(&inputs, &kinds)? != kind {
                            return Err(Error::Invalid("hole-type"));
                        }
                    }
                    let mut keyed = alternatives
                        .iter()
                        .map(|op| {
                            Ok((
                                op.value()?
                                    .canonical_bytes()
                                    .map_err(SynthesisError::Encode)?,
                                op.clone(),
                            ))
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    keyed.sort_by(|left, right| left.0.cmp(&right.0));
                    *alternatives = keyed.into_iter().map(|(_, op)| op).collect();
                    let hole = Hole::try_new(
                        hole_id,
                        alternatives
                            .iter()
                            .map(Op::value)
                            .collect::<Result<Vec<_>, _>>()?,
                    )?;
                    if holes.iter().any(|h: &Hole| h.id() == hole.id()) {
                        return Err(Error::Invalid("duplicate-hole"));
                    }
                    holes.push(hole);
                    kind
                }
            };
            kinds.push(kind);
        }
        validate_roots(&outputs, &roots, &kinds)?;
        if holes.is_empty() {
            return Err(Error::Invalid("no-holes"));
        }
        if holes.len() > crate::MAX_HOLES {
            return Err(Error::Limit("holes"));
        }
        Ok(Self {
            inputs,
            outputs,
            nodes,
            roots,
            holes,
        })
    }
    fn close(&self, assignment: &Assignment) -> Result<Program, Error> {
        let nodes = self
            .nodes
            .iter()
            .map(|slot| match slot {
                Slot::Fixed(op) => Ok(op.clone()),
                Slot::Choice { id, alternatives } => {
                    let hole_id = HoleId::try_new(*id)?;
                    let value = assignment
                        .get(hole_id)
                        .ok_or(Error::Invalid("missing-assignment"))?;
                    // Construction gives these alternatives and the hole's values
                    // the same canonical order. Holes retain node order, so locate
                    // this hole by its stable ID.
                    self.holes
                        .iter()
                        .find(|hole| hole.id() == hole_id)
                        .and_then(|hole| {
                            hole.values()
                                .zip(alternatives.iter())
                                .find(|(candidate, _)| *candidate == value)
                        })
                        .map(|(_, op)| op.clone())
                        .ok_or(Error::Invalid("unknown-assignment"))
                }
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Program::try_new(
            self.inputs.clone(),
            self.outputs.clone(),
            nodes,
            self.roots.clone(),
        )
    }
    fn value(&self) -> Result<Value, zeno_fcis_codec::EncodeError> {
        tuple(vec![
            schema_value(&self.inputs, &self.outputs)?,
            tuple(
                self.nodes
                    .iter()
                    .map(|slot| match slot {
                        Slot::Fixed(op) => tuple(vec![Value::unsigned(0), op.value()?]),
                        Slot::Choice { id, alternatives } => tuple(vec![
                            Value::unsigned(1),
                            Value::unsigned((*id).into()),
                            tuple(
                                alternatives
                                    .iter()
                                    .map(Op::value)
                                    .collect::<Result<Vec<_>, _>>()?,
                            )?,
                        ]),
                    })
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

/// Independent reviewed relation over input followed by output fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    relation: Program,
}
impl Contract {
    /// Requires a closed Boolean relation with exactly the declared environment.
    pub fn try_new(
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        relation: Program,
    ) -> Result<Self, Error> {
        if inputs.len() > MAX_FIELDS
            || outputs.len() > MAX_FIELDS
            || outputs.is_empty()
            || relation.inputs() != [inputs.as_slice(), outputs.as_slice()].concat()
            || relation.outputs() != [Domain::Bool]
        {
            return Err(Error::Invalid("contract-shape"));
        }
        Ok(Self {
            inputs,
            outputs,
            relation,
        })
    }
    /// Complete admitted input domains in ABI order.
    #[must_use]
    pub fn inputs(&self) -> &[Domain] {
        &self.inputs
    }
    /// Complete output domains in ABI order.
    #[must_use]
    pub fn outputs(&self) -> &[Domain] {
        &self.outputs
    }
    /// The exact admitted relation over inputs followed by outputs.
    #[must_use]
    pub const fn relation(&self) -> &Program {
        &self.relation
    }
    /// Rechecks a target's actual complete output tuple against the relation.
    pub fn holds(&self, input: &[i128], output: &[i128]) -> Result<bool, Error> {
        let mut environment = Vec::new();
        let mut values = Vec::new();
        let mut result = Vec::new();
        self.holds_into(input, output, &mut environment, &mut values, &mut result)
    }
    /// Rechecks one admitted pair using caller-owned buffers.
    ///
    /// The environment is the input tuple followed by the output tuple, exactly
    /// as the relation declares its own inputs. Every buffer is rewritten from
    /// the start before evaluation, so an earlier rejected pair cannot influence
    /// an admitted pair.
    fn holds_into(
        &self,
        input: &[i128],
        output: &[i128],
        environment: &mut Vec<i128>,
        values: &mut Vec<i128>,
        result: &mut Vec<i128>,
    ) -> Result<bool, Error> {
        if !ir::admitted(&self.inputs, input) || !ir::admitted(&self.outputs, output) {
            return Err(Error::Invalid("contract-domain"));
        }
        environment.clear();
        environment.extend_from_slice(input);
        environment.extend_from_slice(output);
        self.relation.evaluate_into(environment, values, result)?;
        Ok(matches!(result.as_slice(), [1]))
    }
    /// Enumerates every admitted input, independently of any candidate or target.
    pub fn input_space(&self) -> Result<Space, Error> {
        Space::try_new(&self.inputs, MAX_INPUTS)
    }
    /// Canonical contract identity, including its full schema and semantics.
    pub fn commitment(&self) -> Result<Hash32, Error> {
        Ok(hash_canonical(
            zeno_fcis_codec::domains::FINITE_CONTRACT,
            tuple(vec![
                schema_value(&self.inputs, &self.outputs)?,
                self.relation.value()?,
            ])?
            .canonical_bytes(),
        )?)
    }
}

/// Complete, bounded lexicographic Cartesian space. Zero fields gives one tuple.
#[derive(Clone, Debug)]
pub struct Space {
    domains: Vec<Domain>,
    current: Option<Vec<i128>>,
    count: u64,
}
impl Space {
    fn try_new(domains: &[Domain], limit: u64) -> Result<Self, Error> {
        let mut count = 1u64;
        for d in domains {
            let (min, max) = d.bounds();
            let width = max
                .checked_sub(min)
                .and_then(|span| span.checked_add(1))
                .and_then(|width| u64::try_from(width).ok())
                .ok_or(Error::Limit("space"))?;
            count = count
                .checked_mul(width)
                .filter(|n| *n <= limit)
                .ok_or(Error::Limit("space"))?;
            if width == 0 {
                return Err(Error::Invalid("domain"));
            }
        }
        Ok(Self {
            domains: domains.to_vec(),
            current: Some(domains.iter().map(|d| d.bounds().0).collect()),
            count,
        })
    }
    /// Exact number of tuples in the original complete space.
    #[must_use]
    pub const fn cardinality(&self) -> u64 {
        self.count
    }
    /// Copies the current tuple into `tuple` and advances the space.
    ///
    /// Returns false once the space is exhausted. Zero fields yield exactly one
    /// empty tuple, matching the public iterator.
    fn next_into(&mut self, tuple: &mut Vec<i128>) -> bool {
        let Some(current) = self.current.as_mut() else {
            return false;
        };
        tuple.clear();
        tuple.extend_from_slice(current);
        if !advance(&self.domains, current) {
            self.current = None;
        }
        true
    }
    /// Returns the space to its first tuple, recreating it after exhaustion.
    fn restart(&mut self) {
        match &mut self.current {
            Some(tuple) => {
                tuple.clear();
                tuple.extend(self.domains.iter().map(|d| d.bounds().0));
            }
            None => self.current = Some(self.domains.iter().map(|d| d.bounds().0).collect()),
        }
    }
}
/// Steps one tuple to its lexicographic successor in place.
///
/// Returns false when the last field wrapped, which is also the answer for a
/// zero-field tuple: the singleton space is exhausted by its one visit. A field
/// is incremented only while it is strictly below its maximum, so the odometer
/// never overflows at an `i128` endpoint.
fn advance(domains: &[Domain], tuple: &mut [i128]) -> bool {
    for index in (0..tuple.len()).rev() {
        let (min, max) = domains[index].bounds();
        if tuple[index] < max {
            tuple[index] += 1;
            return true;
        }
        tuple[index] = min;
    }
    false
}
impl Iterator for Space {
    type Item = Vec<i128>;
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.as_mut()?;
        let result = current.clone();
        if !advance(&self.domains, current) {
            self.current = None;
        }
        Some(result)
    }
}

/// A replay vector, generated only after the candidate satisfies the relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    /// Explicit current inputs.
    pub input: Vec<i128>,
    /// Exact full output tuple.
    pub output: Vec<i128>,
}
/// First concrete refutation retained for agent repair; hashes retain all refutations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Witness {
    /// Input at which a candidate failed.
    pub input: Vec<i128>,
    /// Candidate output, or absent if evaluation trapped.
    pub output: Option<Vec<i128>>,
}
/// Results with distinct contract, grammar, and selected-program meanings.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum Outcome {
    /// Complete finite semantic acceptance. Target execution is still separate.
    Selected {
        /// Closed accepted implementation.
        program: Program,
        /// Existing canonical search evidence.
        certificate: SynthesisCertificate,
        /// Exhaustive portable replay corpus.
        cases: Vec<Case>,
        /// First refuted candidate, if any.
        first_counterexample: Option<Witness>,
    },
    /// Every candidate in the reviewed grammar was refuted.
    NoSolution {
        /// Complete existing no-solution search evidence.
        certificate: SynthesisCertificate,
        /// Concrete witness for the first candidate.
        first_counterexample: Option<Witness>,
    },
    /// The relation has no admitted output for this admitted input.
    Unrealizable {
        /// Exact input witness, independent of the candidate grammar.
        input: Vec<i128>,
    },
}

/// Checks total realizability, then reuses the existing canonical hole search.
/// Neither emission nor any commit/effect authority is part of this operation.
pub fn synthesize(contract: &Contract, sketch: &Sketch, budget: Budget) -> Result<Outcome, Error> {
    if contract.inputs != sketch.inputs || contract.outputs != sketch.outputs {
        return Err(Error::Invalid("schema-mismatch"));
    }
    let input_space = contract.input_space()?;
    let output_space = Space::try_new(contract.outputs(), MAX_OUTPUTS)?;
    let assignments = sketch.holes.iter().try_fold(1u64, |n, h| {
        n.checked_mul(h.values().len() as u64)
            .ok_or(Error::Limit("assignments"))
    })?;
    check_budget(
        "assignments",
        assignments,
        budget.max_assignments,
        crate::MAX_ASSIGNMENTS,
    )?;
    let realizability = input_space
        .cardinality()
        .checked_mul(output_space.cardinality())
        .and_then(|n| n.checked_mul(contract.relation.nodes().len() as u64))
        .ok_or(Error::Limit("steps"))?;
    let checking = assignments
        .checked_mul(input_space.cardinality())
        .and_then(|n| n.checked_mul((contract.relation.nodes().len() + sketch.nodes.len()) as u64))
        .ok_or(Error::Limit("steps"))?;
    let replay = input_space
        .cardinality()
        .checked_mul(sketch.nodes.len() as u64)
        .ok_or(Error::Limit("steps"))?;
    check_budget(
        "steps",
        realizability
            .checked_add(checking)
            .and_then(|n| n.checked_add(replay))
            .ok_or(Error::Limit("steps"))?,
        budget.max_steps,
        MAX_STEPS,
    )?;
    let mut inputs = input_space.clone();
    let mut outputs = output_space;
    let mut input = Vec::new();
    let mut output = Vec::new();
    let mut environment = Vec::new();
    let mut relation_nodes = Vec::new();
    let mut relation_output = Vec::new();
    while inputs.next_into(&mut input) {
        let mut found = false;
        outputs.restart();
        // Every output in the row is visited even after one satisfies the
        // relation, so a later trap still wins over the row's success, and an
        // earlier unrealizable row still wins over any later trap.
        while outputs.next_into(&mut output) {
            found |= contract
                .holds_into(
                    &input,
                    &output,
                    &mut environment,
                    &mut relation_nodes,
                    &mut relation_output,
                )
                .map_err(|_| Error::ContractTrap {
                    input: input.clone(),
                    output: output.clone(),
                })?;
        }
        if !found {
            return Ok(Outcome::Unrealizable { input });
        }
    }
    let bindings = SynthesisBindings {
        schema_hash: hash_canonical(
            zeno_fcis_codec::domains::FINITE_SCHEMA,
            schema_value(contract.inputs(), contract.outputs())?.canonical_bytes(),
        )?,
        contract_hash: contract.commitment()?,
        grammar_hash: hash_canonical(
            zeno_fcis_codec::domains::FINITE_GRAMMAR,
            sketch.value()?.canonical_bytes(),
        )?,
        algorithm_hash: hash_bytes(
            zeno_fcis_codec::domains::FINITE_ALGORITHM,
            b"canonical-search/1; eager-finite-i128/1; exhaustive-total-relation/1",
        )?,
    };
    let problem = SynthesisProblem::try_new(
        bindings,
        sketch.holes.clone(),
        SearchBudget {
            max_assignments: budget.max_assignments,
        },
    )?;
    let checker_hash = hash_canonical(
        zeno_fcis_codec::domains::FINITE_CHECKER,
        tuple(vec![
            Value::text_ascii(PROFILE.into()).map_err(|_| Error::Invalid("finite-profile-text"))?,
            schema_value(contract.inputs(), contract.outputs())?,
            contract.relation.value()?,
            Value::unsigned(budget.max_steps.into()),
            digest_value(hash_bytes(
                zeno_fcis_codec::domains::FINITE_CHECKER_SOURCE,
                include_bytes!("mod.rs"),
            )?)?,
            // Covers the IR, admission and evaluation sources and their
            // specifications, which earlier identities hashed file by file.
            digest_value(hash_bytes(
                zeno_fcis_codec::domains::FINITE_EXECUTION_SOURCE,
                &semantic_source_bytes(),
            )?)?,
            digest_value(Hash32::new(execution_v2::authority::EVALUATOR))?,
        ])?
        .canonical_bytes(),
    )?;
    let mut checker = Checker {
        contract,
        sketch,
        checker_hash,
        first: None,
        accepted: None,
        inputs: input_space.clone(),
        input: Vec::new(),
        program_nodes: Vec::new(),
        program_output: Vec::new(),
        environment: Vec::new(),
        relation_nodes: Vec::new(),
        relation_output: Vec::new(),
    };
    match search(&problem, &mut checker)? {
        SearchResult::NoSolution { certificate } => Ok(Outcome::NoSolution {
            certificate,
            first_counterexample: checker.first,
        }),
        SearchResult::Selected { certificate, .. } => {
            let program = checker
                .accepted
                .ok_or(Error::Invalid("missing-accepted-program"))?;
            let mut cases = Vec::new();
            let mut replay = input_space;
            let mut replay_input = Vec::new();
            let mut replay_nodes = Vec::new();
            let mut replay_output = Vec::new();
            while replay.next_into(&mut replay_input) {
                program.evaluate_into(&replay_input, &mut replay_nodes, &mut replay_output)?;
                cases.push(Case {
                    input: replay_input.clone(),
                    output: replay_output.clone(),
                });
            }
            Ok(Outcome::Selected {
                program,
                certificate,
                cases,
                first_counterexample: checker.first,
            })
        }
    }
}
fn check_budget(
    resource: &'static str,
    required: u64,
    declared: u64,
    hard: u64,
) -> Result<(), Error> {
    if required > hard {
        return Err(Error::Limit(resource));
    }
    if required > declared {
        return Err(Error::Budget {
            resource,
            required,
            declared,
        });
    }
    Ok(())
}
/// Candidate checking state, including the admitted input space and the
/// evaluation buffers reused across candidates and inputs.
struct Checker<'a> {
    contract: &'a Contract,
    sketch: &'a Sketch,
    checker_hash: Hash32,
    first: Option<Witness>,
    accepted: Option<Program>,
    inputs: Space,
    input: Vec<i128>,
    program_nodes: Vec<i128>,
    program_output: Vec<i128>,
    environment: Vec<i128>,
    relation_nodes: Vec<i128>,
    relation_output: Vec<i128>,
}
impl CandidateChecker for Checker<'_> {
    fn checker_hash(&self) -> Hash32 {
        self.checker_hash
    }
    fn check(&mut self, assignment: &Assignment) -> CheckResult {
        let Ok(program) = self.sketch.close(assignment) else {
            return CheckResult::Indeterminate;
        };
        // The input space was admitted before the search started. Every
        // candidate replays that same space from its first tuple.
        let count = self.inputs.cardinality();
        self.inputs.restart();
        while self.inputs.next_into(&mut self.input) {
            let evaluated = program
                .evaluate_into(
                    &self.input,
                    &mut self.program_nodes,
                    &mut self.program_output,
                )
                .is_ok();
            let holds = if evaluated {
                match self.contract.holds_into(
                    &self.input,
                    &self.program_output,
                    &mut self.environment,
                    &mut self.relation_nodes,
                    &mut self.relation_output,
                ) {
                    Ok(holds) => holds,
                    Err(_) => return CheckResult::Indeterminate,
                }
            } else {
                false
            };
            if !holds {
                let output = evaluated.then(|| self.program_output.clone());
                let counterexample = (|| {
                    tuple(vec![
                        tuple(self.input.iter().map(|v| Value::signed(*v)).collect())?,
                        output.as_ref().map_or(Ok(Value::unit()), |out| {
                            tuple(out.iter().map(|v| Value::signed(*v)).collect())
                        })?,
                    ])
                })();
                let Ok(counterexample) = counterexample else {
                    return CheckResult::Indeterminate;
                };
                if self.first.is_none() {
                    self.first = Some(Witness {
                        input: self.input.clone(),
                        output,
                    });
                }
                return CheckResult::Rejected { counterexample };
            }
        }
        let Ok(compiled) = program.value() else {
            return CheckResult::Indeterminate;
        };
        let evidence = (|| {
            tuple(vec![
                self.contract.relation.value()?,
                compiled.clone(),
                Value::unsigned(count.into()),
            ])
        })();
        let Ok(evidence) = evidence else {
            return CheckResult::Indeterminate;
        };
        let (Ok(reference_claim), Ok(composition_claim)) = (
            hash_canonical(
                zeno_fcis_codec::domains::FINITE_CONTRACT_COVERAGE,
                evidence.canonical_bytes(),
            ),
            hash_canonical(
                zeno_fcis_codec::domains::FINITE_CLOSED_TOTALITY,
                evidence.canonical_bytes(),
            ),
        ) else {
            return CheckResult::Indeterminate;
        };
        self.accepted = Some(program);
        CheckResult::Accepted {
            compiled,
            reference_claim,
            composition_claim,
        }
    }
}

pub use execution_v2::authority as v2_authority;

pub use execution_v2::catalog as v2_catalog;

#[cfg(all(test, not(miri)))]
mod semantic_source_tests {
    use super::SEMANTIC_SOURCES;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    /// Source-bound native relations and the Authority publication route.
    /// Oracle plans are source-bound here but have no publication constructor.
    /// Legacy scalar program admission/evaluation does not reach these modules.
    /// Both identities bind them through the
    /// generated `EVALUATOR` digest of the approved source closure.
    const AUTHORITY_ROUTE: &[&str] = &[
        "canonical_v2/envelope.rs",
        "canonical_v2/envelope/spec.rs",
        "canonical_v2/output.rs",
        "canonical_v2/output/spec.rs",
        "execution_v2/authority.rs",
        "execution_v2/authority/bound.rs",
        "execution_v2/authority/bound_spec.rs",
        "execution_v2/authority/candidate.rs",
        "execution_v2/authority/canonical.rs",
        "execution_v2/authority/descriptor.rs",
        "execution_v2/authority/descriptor_spec.rs",
        "execution_v2/authority/family_spec.rs",
        "execution_v2/authority/framing.rs",
        "execution_v2/authority/metadata.rs",
        "execution_v2/authority/metadata_spec.rs",
        "execution_v2/authority/numeric_metadata.rs",
        "execution_v2/authority/numeric_metadata_spec.rs",
        "execution_v2/authority/observations.rs",
        "execution_v2/authority/outcome.rs",
        "execution_v2/authority/outcome_spec.rs",
        "execution_v2/authority/policy.rs",
        "execution_v2/authority/policy_spec.rs",
        "execution_v2/authority/publication.rs",
        "execution_v2/authority/publication/spec.rs",
        "execution_v2/authority/refusal.rs",
        "execution_v2/authority/spec.rs",
        "execution_v2/catalog.rs",
        "execution_v2/catalog/family_spec.rs",
        "execution_v2/catalog/matching.rs",
        "execution_v2/catalog/spec.rs",
        "execution_v2/composition.rs",
        "execution_v2/composition/admission.rs",
        "execution_v2/composition/admission/numeric.rs",
        "execution_v2/composition/admission/numeric/spec.rs",
        "execution_v2/composition/admission/spec.rs",
        "execution_v2/composition/descriptor.rs",
        "execution_v2/composition/descriptor/spec.rs",
        "execution_v2/composition/framed.rs",
        "execution_v2/composition/framed/spec.rs",
        "execution_v2/composition/ingress.rs",
        "execution_v2/composition/ingress/original.rs",
        "execution_v2/composition/ingress/original/spec.rs",
        "execution_v2/composition/ingress/shared.rs",
        "execution_v2/composition/ingress/shared_spec.rs",
        "execution_v2/composition/ingress/spec.rs",
        "execution_v2/composition/outcome.rs",
        "execution_v2/composition/producer.rs",
        "execution_v2/composition/producer/spec.rs",
        "execution_v2/composition/schema_validation.rs",
        "execution_v2/composition/schema_validation/spec.rs",
        "execution_v2/composition/shared.rs",
        "execution_v2/composition/shared_spec.rs",
        "execution_v2/composition/spec.rs",
        "execution_v2/continuation.rs",
        "execution_v2/continuation/spec.rs",
        "execution_v2/laws.rs",
        "execution_v2/laws/atoms.rs",
        "execution_v2/laws/frame.rs",
        "execution_v2/laws/predicate.rs",
        "execution_v2/laws/spec.rs",
        "execution_v2/laws/types.rs",
        "execution_v2/oracle/account_law_tests.rs",
        "execution_v2/oracle/aggregation.rs",
        "execution_v2/oracle/arithmetic.rs",
        "execution_v2/oracle/bond_laws.rs",
        "execution_v2/oracle/bonds.rs",
        "execution_v2/oracle/finite_account_tests.rs",
        "execution_v2/oracle/finite_price_tests.rs",
        "execution_v2/oracle/laws.rs",
        "execution_v2/oracle/lifecycle.rs",
        "execution_v2/oracle/lifecycle_ingress.rs",
        "execution_v2/oracle/lifecycle_laws.rs",
        "execution_v2/oracle/lifecycle_schema.rs",
        "execution_v2/oracle/lifecycle_tests.rs",
        "execution_v2/oracle/mod.rs",
        "execution_v2/oracle/native_ports.rs",
        "execution_v2/oracle/native_rewards.rs",
        "execution_v2/oracle/prices.rs",
        "execution_v2/oracle/quorum_laws.rs",
        "execution_v2/oracle/reports.rs",
        "execution_v2/oracle/read.rs",
        "execution_v2/oracle/reward_laws.rs",
        "execution_v2/oracle/rewards.rs",
        "execution_v2/oracle/test_verifier.rs",
        "execution_v2/oracle/types.rs",
        "execution_v2/oracle/verification.rs",
        "execution_v2/oracle/wire.rs",
        "execution_v2/tables/authority.rs",
        "execution_v2/tables/collection.rs",
        "execution_v2/tables/expression.rs",
        "execution_v2/tables/mod.rs",
        "execution_v2/tables/policy.rs",
        "execution_v2/tables/schema.rs",
        "execution_v2/tables/ordered.rs",
        "execution_v2/tables/order_laws.rs",
        "execution_v2/tables/redemption.rs",
        "execution_v2/tables/redemption_laws.rs",
    ];
    /// Modules rooted in `AUTHORITY_ROUTE`, which hashed files must not use.
    const AUTHORITY_MODULES: &[&str] = &[
        "authority",
        "catalog",
        "composition",
        "continuation",
        "envelope",
        "laws",
        "output",
        "tables",
        "oracle",
    ];
    /// The generated digest, which both identities include by value.
    const GENERATED: &[&str] = &["execution_v2/authority/evaluator.rs"];
    /// Unit tests, compiled only under `cfg(test)`.
    const TESTS: &[&str] = &[
        "canonical_v2/envelope/tests.rs",
        "canonical_v2/output/tests.rs",
        "canonical_v2/schema/tests.rs",
        "execution_v2/authority/invocation_tests.rs",
        "execution_v2/authority/publication_tests.rs",
        "execution_v2/authority/sealing_tests.rs",
        "execution_v2/authority/tests.rs",
        "execution_v2/catalog/tests.rs",
        "execution_v2/composition/account_tests.rs",
        "execution_v2/composition/record_tests.rs",
        "execution_v2/composition/tests.rs",
        "execution_v2/compound/tests.rs",
        "execution_v2/continuation/tests.rs",
        "execution_v2/decision/tests.rs",
        "execution_v2/input_view/tests.rs",
        "execution_v2/laws/tests.rs",
        "execution_v2/numeric/tests.rs",
        "execution_v2/tables/tests.rs",
        "execution_v2/tables/ordered_tests.rs",
        "execution_v2/tests.rs",
        "value/tests.rs",
    ];

    fn collect(root: &Path, directory: &Path, files: &mut BTreeSet<String>) {
        for entry in fs::read_dir(directory).unwrap_or_else(|e| panic!("{directory:?}: {e}")) {
            let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
            if path.is_dir() {
                collect(root, &path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let relative = path.strip_prefix(root).unwrap_or_else(|e| panic!("{e}"));
                files.insert(
                    relative
                        .to_str()
                        .unwrap_or_else(|| panic!("{path:?}"))
                        .replace('\\', "/"),
                );
            }
        }
    }

    #[test]
    fn every_evaluation_and_execution_file_is_hashed_or_reviewed_out() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/finite");
        let mut tree = BTreeSet::from(["ir.rs".to_owned()]);
        for directory in ["canonical_v2", "evaluation", "execution_v2", "ir", "value"] {
            collect(&root, &root.join(directory), &mut tree);
        }
        assert!(
            SEMANTIC_SOURCES.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted and unique"
        );
        for (path, bytes) in SEMANTIC_SOURCES {
            assert_eq!(
                fs::read(root.join(path)).ok().as_deref(),
                Some(*bytes),
                "{path}"
            );
        }
        let mut reviewed = BTreeSet::new();
        let hashed = SEMANTIC_SOURCES.iter().map(|(path, _)| path);
        for path in hashed.chain(AUTHORITY_ROUTE).chain(GENERATED).chain(TESTS) {
            assert!(
                reviewed.insert((*path).to_owned()),
                "{path} is listed twice"
            );
        }
        let unreviewed: Vec<_> = tree.difference(&reviewed).collect();
        let missing: Vec<_> = reviewed.difference(&tree).collect();
        assert!(unreviewed.is_empty(), "hash or review out: {unreviewed:?}");
        assert!(missing.is_empty(), "stale entries: {missing:?}");
    }

    #[test]
    fn hashed_files_do_not_use_the_authority_route() {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let starts_word = |text: &str, at: usize| !text[..at].chars().next_back().is_some_and(word);
        for (path, bytes) in SEMANTIC_SOURCES {
            let text = core::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            let code: String = text
                .lines()
                .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
                .collect::<Vec<_>>()
                .join("\n");
            // Every `use` tree, including grouped and renamed imports.
            let trees: Vec<&str> = code
                .match_indices("use ")
                .filter(|(at, _)| starts_word(&code, *at))
                .filter_map(|(at, _)| code[at..].split_once(';').map(|(tree, _)| tree))
                .collect();
            for module in AUTHORITY_MODULES {
                let as_path = format!("{module}::");
                let path_use = code
                    .match_indices(&as_path)
                    .any(|(at, _)| starts_word(&code, at));
                let item_use = trees
                    .iter()
                    .any(|tree| tree.split(|c: char| !word(c)).any(|item| item == *module));
                assert!(
                    !path_use && !item_use,
                    "{path} uses {module}; hash its closure"
                );
            }
        }
    }
}
