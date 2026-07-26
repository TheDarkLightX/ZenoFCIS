//! Deterministic, complete-within-bounds, verifier-gated synthesis.
//!
//! Search order is canonical protocol data. A proposer may populate reviewed
//! candidate domains, but only an external checker can accept an assignment.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_value::Value;

/// Maximum number of stable holes in one problem.
pub const MAX_HOLES: usize = 64;
/// Maximum closed candidate values per hole.
pub const MAX_VALUES_PER_HOLE: usize = 1_024;
/// Maximum exact Cartesian assignments in one certified search.
pub const MAX_ASSIGNMENTS: u64 = 1_000_000;

/// Stable synthesis-hole identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HoleId(u32);

impl HoleId {
    /// Creates a nonzero stable hole identifier.
    pub const fn try_new(value: u32) -> Result<Self, SynthesisError> {
        if value == 0 {
            Err(SynthesisError::ZeroHoleId)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One closed, canonically ordered synthesis domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hole {
    id: HoleId,
    values: Box<[CandidateValue]>,
}

impl Hole {
    /// Owns, validates, and canonicalizes the complete candidate domain.
    pub fn try_new(id: HoleId, values: Vec<Value>) -> Result<Self, SynthesisError> {
        if values.is_empty() {
            return Err(SynthesisError::EmptyCandidateDomain(id));
        }
        if values.len() > MAX_VALUES_PER_HOLE {
            return Err(SynthesisError::TooManyCandidateValues(id));
        }
        let mut candidates = values
            .into_iter()
            .map(CandidateValue::try_new)
            .collect::<Result<Vec<_>, _>>()?;
        candidates.sort_by(|left, right| left.bytes.cmp(&right.bytes));
        if candidates
            .windows(2)
            .any(|pair| pair[0].bytes == pair[1].bytes)
        {
            return Err(SynthesisError::DuplicateCandidateValue(id));
        }
        Ok(Self {
            id,
            values: candidates.into_boxed_slice(),
        })
    }

    /// Returns the stable hole identifier.
    #[must_use]
    pub const fn id(&self) -> HoleId {
        self.id
    }

    /// Returns candidate values in canonical encoding order.
    #[must_use]
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.values.iter().map(|candidate| &candidate.value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateValue {
    value: Value,
    bytes: Box<[u8]>,
}

impl CandidateValue {
    fn try_new(value: Value) -> Result<Self, SynthesisError> {
        let bytes = value
            .canonical_bytes()
            .map_err(SynthesisError::Encode)?
            .into_boxed_slice();
        Ok(Self { value, bytes })
    }
}

/// Reviewed identities that define one synthesis authority surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynthesisBindings {
    /// Closed schema identity.
    pub schema_hash: Hash32,
    /// Composition/transition contract identity.
    pub contract_hash: Hash32,
    /// Closed grammar and dependency-set identity.
    pub grammar_hash: Hash32,
    /// Deterministic search algorithm identity.
    pub algorithm_hash: Hash32,
}

impl SynthesisBindings {
    fn validate(self) -> Result<Self, SynthesisError> {
        if [
            self.schema_hash,
            self.contract_hash,
            self.grammar_hash,
            self.algorithm_hash,
        ]
        .contains(&Hash32::ZERO)
        {
            return Err(SynthesisError::ZeroBinding);
        }
        Ok(self)
    }
}

/// Exact logical search budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchBudget {
    /// Maximum assignments the reviewed run authorizes.
    pub max_assignments: u64,
}

/// Fully validated bounded synthesis problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthesisProblem {
    bindings: SynthesisBindings,
    holes: Box<[Hole]>,
    cardinality: u64,
    budget: SearchBudget,
    problem_hash: Hash32,
}

impl SynthesisProblem {
    /// Canonicalizes holes and proves the declared budget covers the full search.
    pub fn try_new(
        bindings: SynthesisBindings,
        mut holes: Vec<Hole>,
        budget: SearchBudget,
    ) -> Result<Self, SynthesisError> {
        let bindings = bindings.validate()?;
        if holes.is_empty() {
            return Err(SynthesisError::EmptyProblem);
        }
        if holes.len() > MAX_HOLES {
            return Err(SynthesisError::TooManyHoles);
        }
        holes.sort_by_key(Hole::id);
        if holes.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(SynthesisError::DuplicateHoleId);
        }
        let cardinality = holes.iter().try_fold(1_u64, |count, hole| {
            let width = u64::try_from(hole.values.len())
                .map_err(|_| SynthesisError::CardinalityOverflow)?;
            count
                .checked_mul(width)
                .ok_or(SynthesisError::CardinalityOverflow)
        })?;
        if cardinality > MAX_ASSIGNMENTS {
            return Err(SynthesisError::SearchSpaceTooLarge);
        }
        if budget.max_assignments < cardinality {
            return Err(SynthesisError::IncompleteSearch {
                required: cardinality,
                declared: budget.max_assignments,
            });
        }
        let problem_hash = hash_problem(bindings, &holes, cardinality, budget)?;
        Ok(Self {
            bindings,
            holes: holes.into_boxed_slice(),
            cardinality,
            budget,
            problem_hash,
        })
    }

    /// Returns the exact number of assignments that must be checked.
    #[must_use]
    pub const fn cardinality(&self) -> u64 {
        self.cardinality
    }

    /// Returns the complete content-derived problem identity.
    #[must_use]
    pub const fn problem_hash(&self) -> Hash32 {
        self.problem_hash
    }

    /// Returns holes in stable ID order.
    #[must_use]
    pub const fn holes(&self) -> &[Hole] {
        &self.holes
    }
}

/// One complete assignment in stable hole order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assignment {
    entries: Box<[(HoleId, Value)]>,
}

impl Assignment {
    /// Returns the value assigned to a stable hole.
    #[must_use]
    pub fn get(&self, id: HoleId) -> Option<&Value> {
        self.entries
            .binary_search_by_key(&id, |entry| entry.0)
            .ok()
            .map(|index| &self.entries[index].1)
    }

    /// Returns all assignments in stable hole order.
    #[must_use]
    pub const fn entries(&self) -> &[(HoleId, Value)] {
        &self.entries
    }

    /// Returns the assignment commitment.
    pub fn commitment(&self) -> Result<Hash32, SynthesisError> {
        hash_canonical("zeno-fcis/synthesis-assignment", self)
    }
}

impl CanonicalEncode for Assignment {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.entries.len())?;
        for (id, value) in &self.entries {
            output.extend_from_slice(&id.get().to_be_bytes());
            put_blob(output, &value.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Independent checker result for one complete assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckResult {
    /// Candidate passed both independent reference and composition checks.
    Accepted {
        /// Closed compiled candidate artifact.
        compiled: Value,
        /// Independent reference/SMT claim identity.
        reference_claim: Hash32,
        /// Composition/refinement claim identity.
        composition_claim: Hash32,
    },
    /// Candidate was refuted by a normalized counterexample.
    Rejected {
        /// Immutable counterexample value.
        counterexample: Value,
    },
    /// Checker timed out, disagreed, crashed, or could not decide.
    Indeterminate,
}

/// External exact-checker authority.
pub trait CandidateChecker {
    /// Returns the pinned checker/toolchain identity.
    fn checker_hash(&self) -> Hash32;
    /// Decides one immutable assignment.
    fn check(&mut self, assignment: &Assignment) -> CheckResult;
}

/// Content-bound rejected frontier point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CounterexampleRecord {
    /// Refuted assignment identity.
    pub assignment_hash: Hash32,
    /// Normalized counterexample identity.
    pub counterexample_hash: Hash32,
}

/// Certified bounded-search outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchResult {
    /// Canonically first checked candidate that passed every required checker.
    Selected {
        /// Accepted assignment.
        assignment: Assignment,
        /// Closed compiled candidate.
        compiled: Value,
        /// Complete certificate.
        certificate: SynthesisCertificate,
    },
    /// Every assignment in the declared complete space was refuted.
    NoSolution {
        /// Complete no-solution certificate.
        certificate: SynthesisCertificate,
    },
}

/// Content-addressed bounded synthesis certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynthesisCertificate {
    problem_hash: Hash32,
    bindings: SynthesisBindings,
    checker_hash: Hash32,
    cardinality: u64,
    evaluated: u64,
    trace_hash: Hash32,
    selected_assignment: Option<Hash32>,
    compiled_hash: Option<Hash32>,
    reference_claim: Option<Hash32>,
    composition_claim: Option<Hash32>,
    counterexamples: Box<[CounterexampleRecord]>,
}

impl SynthesisCertificate {
    /// Returns the exact number of assignments evaluated.
    #[must_use]
    pub const fn evaluated(&self) -> u64 {
        self.evaluated
    }

    /// Returns the canonical trace commitment.
    #[must_use]
    pub const fn trace_hash(&self) -> Hash32 {
        self.trace_hash
    }

    /// Returns the selected assignment, if any.
    #[must_use]
    pub const fn selected_assignment(&self) -> Option<Hash32> {
        self.selected_assignment
    }

    /// Returns retained verifier-labeled negative frontier points.
    #[must_use]
    pub const fn counterexamples(&self) -> &[CounterexampleRecord] {
        &self.counterexamples
    }

    /// Returns the certificate commitment.
    pub fn commitment(&self) -> Result<Hash32, SynthesisError> {
        hash_canonical("zeno-fcis/synthesis-certificate", self)
    }
}

impl CanonicalEncode for SynthesisCertificate {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.problem_hash.as_bytes());
        for hash in [
            self.bindings.schema_hash,
            self.bindings.contract_hash,
            self.bindings.grammar_hash,
            self.bindings.algorithm_hash,
            self.checker_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        output.extend_from_slice(&self.cardinality.to_be_bytes());
        output.extend_from_slice(&self.evaluated.to_be_bytes());
        output.extend_from_slice(self.trace_hash.as_bytes());
        put_optional_hash(output, self.selected_assignment);
        put_optional_hash(output, self.compiled_hash);
        put_optional_hash(output, self.reference_claim);
        put_optional_hash(output, self.composition_claim);
        put_length(output, self.counterexamples.len())?;
        for record in &self.counterexamples {
            output.extend_from_slice(record.assignment_hash.as_bytes());
            output.extend_from_slice(record.counterexample_hash.as_bytes());
        }
        Ok(())
    }
}

/// Exhaustively searches the complete declared space in canonical order.
pub fn search<C: CandidateChecker>(
    problem: &SynthesisProblem,
    checker: &mut C,
) -> Result<SearchResult, SynthesisError> {
    let checker_hash = checker.checker_hash();
    if checker_hash == Hash32::ZERO {
        return Err(SynthesisError::ZeroCheckerIdentity);
    }
    let mut indexes = vec![0_usize; problem.holes.len()];
    let mut trace_hash = problem.problem_hash;
    let mut counterexamples = Vec::new();
    for ordinal in 0..problem.cardinality {
        let assignment = assignment_at(problem, &indexes);
        let assignment_hash = assignment.commitment()?;
        let result = checker.check(&assignment);
        match result {
            CheckResult::Accepted {
                compiled,
                reference_claim,
                composition_claim,
            } => {
                if reference_claim == Hash32::ZERO || composition_claim == Hash32::ZERO {
                    return Err(SynthesisError::MissingAcceptanceEvidence);
                }
                let compiled_hash = hash_canonical("zeno-fcis/synthesis-compiled", &compiled)?;
                trace_hash = extend_trace(
                    trace_hash,
                    assignment_hash,
                    0,
                    &[compiled_hash, reference_claim, composition_claim],
                )?;
                let certificate = SynthesisCertificate {
                    problem_hash: problem.problem_hash,
                    bindings: problem.bindings,
                    checker_hash,
                    cardinality: problem.cardinality,
                    evaluated: ordinal + 1,
                    trace_hash,
                    selected_assignment: Some(assignment_hash),
                    compiled_hash: Some(compiled_hash),
                    reference_claim: Some(reference_claim),
                    composition_claim: Some(composition_claim),
                    counterexamples: counterexamples.into_boxed_slice(),
                };
                return Ok(SearchResult::Selected {
                    assignment,
                    compiled,
                    certificate,
                });
            }
            CheckResult::Rejected { counterexample } => {
                let counterexample_hash =
                    hash_canonical("zeno-fcis/synthesis-counterexample", &counterexample)?;
                trace_hash = extend_trace(trace_hash, assignment_hash, 1, &[counterexample_hash])?;
                counterexamples.push(CounterexampleRecord {
                    assignment_hash,
                    counterexample_hash,
                });
            }
            CheckResult::Indeterminate => return Err(SynthesisError::CheckerIndeterminate),
        }
        increment_indexes(problem, &mut indexes);
    }
    let certificate = SynthesisCertificate {
        problem_hash: problem.problem_hash,
        bindings: problem.bindings,
        checker_hash,
        cardinality: problem.cardinality,
        evaluated: problem.cardinality,
        trace_hash,
        selected_assignment: None,
        compiled_hash: None,
        reference_claim: None,
        composition_claim: None,
        counterexamples: counterexamples.into_boxed_slice(),
    };
    Ok(SearchResult::NoSolution { certificate })
}

fn assignment_at(problem: &SynthesisProblem, indexes: &[usize]) -> Assignment {
    let entries = problem
        .holes
        .iter()
        .zip(indexes)
        .map(|(hole, index)| (hole.id, hole.values[*index].value.clone()))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Assignment { entries }
}

fn increment_indexes(problem: &SynthesisProblem, indexes: &mut [usize]) {
    for position in (0..indexes.len()).rev() {
        indexes[position] += 1;
        if indexes[position] < problem.holes[position].values.len() {
            return;
        }
        indexes[position] = 0;
    }
}

fn hash_problem(
    bindings: SynthesisBindings,
    holes: &[Hole],
    cardinality: u64,
    budget: SearchBudget,
) -> Result<Hash32, SynthesisError> {
    let mut bytes = Vec::new();
    for hash in [
        bindings.schema_hash,
        bindings.contract_hash,
        bindings.grammar_hash,
        bindings.algorithm_hash,
    ] {
        bytes.extend_from_slice(hash.as_bytes());
    }
    bytes.extend_from_slice(&cardinality.to_be_bytes());
    bytes.extend_from_slice(&budget.max_assignments.to_be_bytes());
    put_length(&mut bytes, holes.len()).map_err(SynthesisError::Encode)?;
    for hole in holes {
        bytes.extend_from_slice(&hole.id.get().to_be_bytes());
        put_length(&mut bytes, hole.values.len()).map_err(SynthesisError::Encode)?;
        for value in &hole.values {
            put_blob(&mut bytes, &value.bytes).map_err(SynthesisError::Encode)?;
        }
    }
    hash_bytes("zeno-fcis/synthesis-problem", &bytes)
}

fn extend_trace(
    previous: Hash32,
    assignment: Hash32,
    outcome: u8,
    bindings: &[Hash32],
) -> Result<Hash32, SynthesisError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(previous.as_bytes());
    bytes.extend_from_slice(assignment.as_bytes());
    bytes.push(outcome);
    for hash in bindings {
        bytes.extend_from_slice(hash.as_bytes());
    }
    hash_bytes("zeno-fcis/synthesis-trace", &bytes)
}

fn hash_canonical(
    domain: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SynthesisError> {
    let bytes = value.canonical_bytes().map_err(SynthesisError::Encode)?;
    hash_bytes(domain, &bytes)
}

fn hash_bytes(domain: &'static str, bytes: &[u8]) -> Result<Hash32, SynthesisError> {
    let domain = Domain::new(domain, 1).map_err(SynthesisError::Encode)?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(SynthesisError::Encode)
}

fn put_optional_hash(output: &mut Vec<u8>, hash: Option<Hash32>) {
    match hash {
        None => output.push(0),
        Some(hash) => {
            output.push(1);
            output.extend_from_slice(hash.as_bytes());
        }
    }
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Synthesis construction, search, or certification failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SynthesisError {
    /// Stable hole ID used the forbidden zero sentinel.
    ZeroHoleId,
    /// One reviewed binding used the forbidden zero sentinel.
    ZeroBinding,
    /// Checker identity used the forbidden zero sentinel.
    ZeroCheckerIdentity,
    /// No synthesis holes were declared.
    EmptyProblem,
    /// Problem exceeds the stable hole bound.
    TooManyHoles,
    /// Two holes share one stable identifier.
    DuplicateHoleId,
    /// One hole has no candidate values.
    EmptyCandidateDomain(HoleId),
    /// One hole exceeds its candidate bound.
    TooManyCandidateValues(HoleId),
    /// One hole contains duplicate canonical values.
    DuplicateCandidateValue(HoleId),
    /// Exact search cardinality overflowed.
    CardinalityOverflow,
    /// Exact search space exceeds the global bound.
    SearchSpaceTooLarge,
    /// Declared budget would truncate the complete search.
    IncompleteSearch {
        /// Exact required assignment count.
        required: u64,
        /// Declared maximum assignment count.
        declared: u64,
    },
    /// Checker could not decide one candidate.
    CheckerIndeterminate,
    /// Accepted candidate omitted independent claim bindings.
    MissingAcceptanceEvidence,
    /// Canonical encoding or hashing failed.
    Encode(EncodeError),
}

impl fmt::Display for SynthesisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroHoleId => formatter.write_str("synthesis hole ID is zero"),
            Self::ZeroBinding => formatter.write_str("synthesis binding is zero"),
            Self::ZeroCheckerIdentity => formatter.write_str("checker identity is zero"),
            Self::EmptyProblem => formatter.write_str("synthesis problem has no holes"),
            Self::TooManyHoles => formatter.write_str("synthesis hole bound exceeded"),
            Self::DuplicateHoleId => formatter.write_str("duplicate synthesis hole ID"),
            Self::EmptyCandidateDomain(id) => write!(formatter, "hole {} is empty", id.get()),
            Self::TooManyCandidateValues(id) => {
                write!(formatter, "hole {} exceeds its value bound", id.get())
            }
            Self::DuplicateCandidateValue(id) => {
                write!(formatter, "hole {} contains duplicate values", id.get())
            }
            Self::CardinalityOverflow => formatter.write_str("search cardinality overflow"),
            Self::SearchSpaceTooLarge => formatter.write_str("search space exceeds global bound"),
            Self::IncompleteSearch { required, declared } => write!(
                formatter,
                "search budget {declared} does not cover {required} assignments"
            ),
            Self::CheckerIndeterminate => formatter.write_str("checker result is indeterminate"),
            Self::MissingAcceptanceEvidence => {
                formatter.write_str("accepted candidate lacks independent evidence")
            }
            Self::Encode(error) => write!(formatter, "synthesis encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
#[cfg(feature = "std")]
#[cfg(feature = "std")]
impl std::error::Error for SynthesisError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn bindings() -> SynthesisBindings {
        SynthesisBindings {
            schema_hash: hash(1),
            contract_hash: hash(2),
            grammar_hash: hash(3),
            algorithm_hash: hash(4),
        }
    }

    fn hole(id: u32, values: Vec<u128>) -> Hole {
        Hole::try_new(
            HoleId::try_new(id).unwrap_or_else(|error| panic!("id: {error}")),
            values.into_iter().map(Value::U128).collect(),
        )
        .unwrap_or_else(|error| panic!("hole: {error}"))
    }

    struct SumChecker;

    impl CandidateChecker for SumChecker {
        fn checker_hash(&self) -> Hash32 {
            hash(10)
        }

        fn check(&mut self, assignment: &Assignment) -> CheckResult {
            let sum = assignment
                .entries()
                .iter()
                .map(|(_, value)| match value {
                    Value::U128(value) => *value,
                    _ => 0,
                })
                .sum::<u128>();
            if sum == 5 {
                CheckResult::Accepted {
                    compiled: Value::U128(sum),
                    reference_claim: hash(11),
                    composition_claim: hash(12),
                }
            } else {
                CheckResult::Rejected {
                    counterexample: Value::U128(sum),
                }
            }
        }
    }

    #[test]
    fn declaration_order_does_not_change_certificate() {
        let left = SynthesisProblem::try_new(
            bindings(),
            vec![hole(2, vec![4, 2]), hole(1, vec![3, 1])],
            SearchBudget { max_assignments: 4 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        let right = SynthesisProblem::try_new(
            bindings(),
            vec![hole(1, vec![1, 3]), hole(2, vec![2, 4])],
            SearchBudget { max_assignments: 4 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        assert_eq!(left.problem_hash(), right.problem_hash());
        let first =
            search(&left, &mut SumChecker).unwrap_or_else(|error| panic!("search: {error}"));
        let second =
            search(&right, &mut SumChecker).unwrap_or_else(|error| panic!("search: {error}"));
        assert_eq!(first, second);
    }

    #[test]
    fn truncated_budget_is_not_no_solution() {
        let result = SynthesisProblem::try_new(
            bindings(),
            vec![hole(1, vec![1, 2, 3])],
            SearchBudget { max_assignments: 2 },
        );
        assert_eq!(
            result,
            Err(SynthesisError::IncompleteSearch {
                required: 3,
                declared: 2
            })
        );
    }

    #[test]
    fn no_solution_retains_every_counterexample() {
        struct RejectAll;
        impl CandidateChecker for RejectAll {
            fn checker_hash(&self) -> Hash32 {
                hash(20)
            }
            fn check(&mut self, assignment: &Assignment) -> CheckResult {
                CheckResult::Rejected {
                    counterexample: Value::Bytes(
                        assignment
                            .commitment()
                            .unwrap_or_else(|error| panic!("hash: {error}"))
                            .as_bytes()
                            .to_vec()
                            .into_boxed_slice(),
                    ),
                }
            }
        }
        let problem = SynthesisProblem::try_new(
            bindings(),
            vec![hole(1, vec![1, 2])],
            SearchBudget { max_assignments: 2 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        let SearchResult::NoSolution { certificate } =
            search(&problem, &mut RejectAll).unwrap_or_else(|error| panic!("search: {error}"))
        else {
            panic!("unexpected selection")
        };
        assert_eq!(certificate.evaluated(), 2);
        assert_eq!(certificate.counterexamples().len(), 2);
        assert_ne!(
            certificate
                .commitment()
                .unwrap_or_else(|error| panic!("certificate: {error}")),
            Hash32::ZERO
        );
    }

    #[test]
    fn indeterminate_checker_blocks_certificate() {
        struct Unknown;
        impl CandidateChecker for Unknown {
            fn checker_hash(&self) -> Hash32 {
                hash(30)
            }
            fn check(&mut self, _: &Assignment) -> CheckResult {
                CheckResult::Indeterminate
            }
        }
        let problem = SynthesisProblem::try_new(
            bindings(),
            vec![hole(1, vec![1])],
            SearchBudget { max_assignments: 1 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        assert_eq!(
            search(&problem, &mut Unknown),
            Err(SynthesisError::CheckerIndeterminate)
        );
    }
}
