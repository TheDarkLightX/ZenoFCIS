//! A closed one-step theory with exhaustive relational checking.
//!
//! Inputs and outputs are positional tuples. Bool is distinct during type
//! checking and encoded as 0/1 at the language boundary. This profile proves
//! neither unbounded temporal properties nor adequacy of the reviewed contract.

pub mod emit;
mod ir;

pub use ir::{Domain, MAX_FIELDS, MAX_NODES, Op, PROFILE, Program};

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

/// Hard input-space ceiling, checked before any enumeration.
pub const MAX_INPUTS: u64 = 65_536;
/// Hard output-space ceiling for the realizability check.
pub const MAX_OUTPUTS: u64 = 4_096;
/// Hard bound on the conservative total number of evaluated graph nodes.
pub const MAX_STEPS: u64 = 100_000_000;

/// Failure never usable as positive synthesis evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Checked i64 arithmetic trapped.
    Arithmetic,
    /// The reviewed relation is not total on an admitted pair.
    ContractTrap {
        /// Exact input witness.
        input: Vec<i64>,
        /// Exact output witness.
        output: Vec<i64>,
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
                                op.value()
                                    .canonical_bytes()
                                    .map_err(SynthesisError::Encode)?,
                                op.clone(),
                            ))
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    keyed.sort_by(|left, right| left.0.cmp(&right.0));
                    *alternatives = keyed.into_iter().map(|(_, op)| op).collect();
                    let hole =
                        Hole::try_new(hole_id, alternatives.iter().map(Op::value).collect())?;
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
                    let value = assignment
                        .get(HoleId::try_new(*id)?)
                        .ok_or(Error::Invalid("missing-assignment"))?;
                    alternatives
                        .iter()
                        .find(|op| op.value() == *value)
                        .cloned()
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
    fn value(&self) -> Value {
        tuple(vec![
            schema_value(&self.inputs, &self.outputs),
            tuple(
                self.nodes
                    .iter()
                    .map(|slot| match slot {
                        Slot::Fixed(op) => tuple(vec![Value::U128(0), op.value()]),
                        Slot::Choice { id, alternatives } => tuple(vec![
                            Value::U128(1),
                            Value::U128((*id).into()),
                            tuple(alternatives.iter().map(Op::value).collect()),
                        ]),
                    })
                    .collect(),
            ),
            tuple(
                self.roots
                    .iter()
                    .map(|id| Value::U128((*id).into()))
                    .collect(),
            ),
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
    /// Rechecks a target's actual complete output tuple against the relation.
    pub fn holds(&self, input: &[i64], output: &[i64]) -> Result<bool, Error> {
        if !ir::admitted(&self.inputs, input) || !ir::admitted(&self.outputs, output) {
            return Err(Error::Invalid("contract-domain"));
        }
        Ok(self.relation.evaluate(&[input, output].concat())? == [1])
    }
    /// Enumerates every admitted input, independently of any candidate or target.
    pub fn input_space(&self) -> Result<Space, Error> {
        Space::try_new(&self.inputs, MAX_INPUTS)
    }
    /// Canonical contract identity, including its full schema and semantics.
    pub fn commitment(&self) -> Result<Hash32, Error> {
        Ok(hash_canonical(
            "zeno-fcis/finite-contract",
            &tuple(vec![
                schema_value(&self.inputs, &self.outputs),
                self.relation.value(),
            ]),
        )?)
    }
}

/// Complete, bounded lexicographic Cartesian space. Zero fields gives one tuple.
#[derive(Clone, Debug)]
pub struct Space {
    domains: Vec<Domain>,
    current: Option<Vec<i64>>,
    count: u64,
}
impl Space {
    fn try_new(domains: &[Domain], limit: u64) -> Result<Self, Error> {
        let mut count = 1u64;
        for d in domains {
            let (min, max) = d.bounds();
            let width = u64::try_from(i128::from(max) - i128::from(min) + 1)
                .map_err(|_| Error::Limit("space"))?;
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
}
impl Iterator for Space {
    type Item = Vec<i64>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = self.current.clone()?;
        let mut next = result.clone();
        for i in (0..next.len()).rev() {
            let (min, max) = self.domains[i].bounds();
            if next[i] < max {
                next[i] += 1;
                self.current = Some(next);
                return Some(result);
            }
            next[i] = min;
        }
        self.current = None;
        Some(result)
    }
}

/// A replay vector, generated only after the candidate satisfies the relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    /// Explicit current inputs.
    pub input: Vec<i64>,
    /// Exact full output tuple.
    pub output: Vec<i64>,
}
/// First concrete refutation retained for agent repair; hashes retain all refutations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Witness {
    /// Input at which a candidate failed.
    pub input: Vec<i64>,
    /// Candidate output, or absent if evaluation trapped.
    pub output: Option<Vec<i64>>,
}
/// Results with distinct contract, grammar, and selected-program meanings.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
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
        input: Vec<i64>,
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
    for input in input_space.clone() {
        let mut found = false;
        for output in output_space.clone() {
            found |= contract
                .holds(&input, &output)
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
            "zeno-fcis/finite-schema",
            &schema_value(contract.inputs(), contract.outputs()),
        )?,
        contract_hash: contract.commitment()?,
        grammar_hash: hash_canonical("zeno-fcis/finite-grammar", &sketch.value())?,
        algorithm_hash: hash_bytes(
            "zeno-fcis/finite-algorithm",
            b"canonical-search/1; eager-finite-i64/1; exhaustive-total-relation/1",
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
        "zeno-fcis/finite-checker",
        &tuple(vec![
            Value::Text(PROFILE.into()),
            schema_value(contract.inputs(), contract.outputs()),
            contract.relation.value(),
            Value::U128(budget.max_steps.into()),
            Value::Bytes(
                hash_bytes("zeno-fcis/finite-checker-source", include_bytes!("mod.rs"))?
                    .as_bytes()
                    .to_vec()
                    .into_boxed_slice(),
            ),
            Value::Bytes(
                hash_bytes("zeno-fcis/finite-ir-source", include_bytes!("ir.rs"))?
                    .as_bytes()
                    .to_vec()
                    .into_boxed_slice(),
            ),
        ]),
    )?;
    let mut checker = Checker {
        contract,
        sketch,
        checker_hash,
        first: None,
        accepted: None,
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
            let cases = input_space
                .map(|input| {
                    Ok(Case {
                        output: program.evaluate(&input)?,
                        input,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
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
struct Checker<'a> {
    contract: &'a Contract,
    sketch: &'a Sketch,
    checker_hash: Hash32,
    first: Option<Witness>,
    accepted: Option<Program>,
}
impl CandidateChecker for Checker<'_> {
    fn checker_hash(&self) -> Hash32 {
        self.checker_hash
    }
    fn check(&mut self, assignment: &Assignment) -> CheckResult {
        let Ok(program) = self.sketch.close(assignment) else {
            return CheckResult::Indeterminate;
        };
        let Ok(space) = self.contract.input_space() else {
            return CheckResult::Indeterminate;
        };
        let count = space.cardinality();
        for input in space {
            let output = program.evaluate(&input).ok();
            let holds = match &output {
                Some(output) => match self.contract.holds(&input, output) {
                    Ok(holds) => holds,
                    Err(_) => return CheckResult::Indeterminate,
                },
                None => false,
            };
            if !holds {
                let counterexample = tuple(vec![
                    tuple(input.iter().map(|v| Value::I128((*v).into())).collect()),
                    output.as_ref().map_or(Value::Unit, |out| {
                        tuple(out.iter().map(|v| Value::I128((*v).into())).collect())
                    }),
                ]);
                if self.first.is_none() {
                    self.first = Some(Witness { input, output });
                }
                return CheckResult::Rejected { counterexample };
            }
        }
        let compiled = program.value();
        let evidence = tuple(vec![
            self.contract.relation.value(),
            compiled.clone(),
            Value::U128(count.into()),
        ]);
        let (Ok(reference_claim), Ok(composition_claim)) = (
            hash_canonical("zeno-fcis/finite-contract-coverage", &evidence),
            hash_canonical("zeno-fcis/finite-closed-totality", &evidence),
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
