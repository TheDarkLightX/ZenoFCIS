//! Deterministic replay diagnostics with canonical minimal witnesses.
//!
//! A DivergenceForest owns equal-length observation-commitment traces,
//! canonically ordered by exact bounded implementation identifiers. Prefix
//! partitions only refine as steps are added, so later equal observations
//! cannot erase an earlier disagreement. The globally earliest divergence is
//! selected by step and then by the canonical implementation pair.
//!
//! These values are diagnostic evidence. They grant no transition, commit,
//! publication, or promotion authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod recovery;

pub use recovery::{
    ICRWT_VERSION, MAX_RECOVERY_ACTION_BYTES, MAX_RECOVERY_EVENT_ID_BYTES,
    MAX_RECOVERY_WORD_ID_BYTES, RecoveryAction, RecoveryBadPrefixWitness, RecoveryDefectKind,
    RecoveryError, RecoveryEvent, RecoveryEventId, RecoveryEventKind, RecoveryObservation,
    RecoveryPrefixKey, RecoverySnapshotCommitment, RecoveryTrieNode, RecoveryWord, RecoveryWordId,
    RecoveryWordTree, build_recovery_word_tree,
};

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError, Hash32};
use zeno_fcis_value::{AsciiText, TextError};

/// Maximum bytes in one implementation identifier.
pub const MAX_IMPLEMENTATION_ID_BYTES: usize = 64;

/// Exact bounded implementation identifier.
pub type ImplementationId = AsciiText<MAX_IMPLEMENTATION_ID_BYTES>;

/// One implementation's immutable ordered observation commitments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationTrace {
    implementation_id: ImplementationId,
    observations: Box<[Hash32]>,
}

impl ImplementationTrace {
    /// Owns an exact identifier and observation trace.
    pub fn try_new(
        implementation_id: &str,
        observations: Vec<Hash32>,
    ) -> Result<Self, DivergenceError> {
        if implementation_id.is_empty() {
            return Err(DivergenceError::EmptyImplementationId);
        }
        let implementation_id = ImplementationId::try_from_string(implementation_id.to_string())
            .map_err(DivergenceError::InvalidImplementationId)?;
        let _ = u32::try_from(observations.len()).map_err(|_| DivergenceError::StepOverflow)?;
        Ok(Self {
            implementation_id,
            observations: observations.into_boxed_slice(),
        })
    }

    /// Returns the exact implementation identifier.
    #[must_use]
    pub fn implementation_id(&self) -> &str {
        self.implementation_id.as_str()
    }

    /// Returns the ordered observation commitments.
    #[must_use]
    pub fn observations(&self) -> &[Hash32] {
        &self.observations
    }
}

impl CanonicalEncode for ImplementationTrace {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, self.implementation_id().as_bytes())?;
        put_length(output, self.observations.len())?;
        for observation in self.observations() {
            output.extend_from_slice(observation.as_bytes());
        }
        Ok(())
    }
}

/// Globally earliest, replay-checkable trace divergence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DivergenceWitness {
    step: u32,
    first_implementation: ImplementationId,
    second_implementation: ImplementationId,
    common_prefix: Box<[Hash32]>,
    first_observation: Hash32,
    second_observation: Hash32,
}

impl DivergenceWitness {
    /// Returns the zero-based divergent step.
    #[must_use]
    pub const fn step(&self) -> u32 {
        self.step
    }

    /// Returns the canonically first implementation identifier.
    #[must_use]
    pub fn first_implementation(&self) -> &str {
        self.first_implementation.as_str()
    }

    /// Returns the canonically second implementation identifier.
    #[must_use]
    pub fn second_implementation(&self) -> &str {
        self.second_implementation.as_str()
    }

    /// Returns the exact shared observation prefix before the divergent step.
    #[must_use]
    pub fn common_prefix(&self) -> &[Hash32] {
        &self.common_prefix
    }

    /// Returns the first implementation's divergent observation.
    #[must_use]
    pub const fn first_observation(&self) -> Hash32 {
        self.first_observation
    }

    /// Returns the second implementation's divergent observation.
    #[must_use]
    pub const fn second_observation(&self) -> Hash32 {
        self.second_observation
    }
}

impl CanonicalEncode for DivergenceWitness {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.step.to_be_bytes());
        put_blob(output, self.first_implementation().as_bytes())?;
        put_blob(output, self.second_implementation().as_bytes())?;
        put_length(output, self.common_prefix.len())?;
        for observation in self.common_prefix() {
            output.extend_from_slice(observation.as_bytes());
        }
        output.extend_from_slice(self.first_observation.as_bytes());
        output.extend_from_slice(self.second_observation.as_bytes());
        Ok(())
    }
}

/// Canonical immutable forest of implementation trace-prefix partitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DivergenceForest {
    traces: Box<[ImplementationTrace]>,
    steps: u32,
}

impl DivergenceForest {
    /// Canonically owns equal-length traces.
    pub fn try_new(mut traces: Vec<ImplementationTrace>) -> Result<Self, DivergenceError> {
        traces.sort_by(|left, right| left.implementation_id().cmp(right.implementation_id()));
        for pair in traces.windows(2) {
            if pair[0].implementation_id() == pair[1].implementation_id() {
                return Err(DivergenceError::DuplicateImplementationId(
                    pair[0].implementation_id().to_string(),
                ));
            }
        }

        let expected_steps = traces.first().map_or(0, |trace| trace.observations().len());
        for trace in &traces {
            if trace.observations().len() != expected_steps {
                return Err(DivergenceError::UnequalTraceLength {
                    implementation_id: trace.implementation_id().to_string(),
                    expected: expected_steps,
                    actual: trace.observations().len(),
                });
            }
        }
        let steps = u32::try_from(expected_steps).map_err(|_| DivergenceError::StepOverflow)?;
        Ok(Self {
            traces: traces.into_boxed_slice(),
            steps,
        })
    }

    /// Returns canonically ordered implementation traces.
    #[must_use]
    pub fn traces(&self) -> &[ImplementationTrace] {
        &self.traces
    }

    /// Returns the common trace length.
    #[must_use]
    pub const fn steps(&self) -> u32 {
        self.steps
    }

    /// Returns the globally earliest divergence, then the smallest identifier pair.
    #[must_use]
    pub fn first_divergence(&self) -> Option<DivergenceWitness> {
        let mut best: Option<(usize, usize, usize)> = None;
        for first_index in 0..self.traces.len() {
            for second_index in first_index.saturating_add(1)..self.traces.len() {
                let first = &self.traces[first_index];
                let second = &self.traces[second_index];
                let Some(step) = first
                    .observations()
                    .iter()
                    .zip(second.observations())
                    .position(|(left, right)| left != right)
                else {
                    continue;
                };
                let replace = best.is_none_or(|(best_step, best_first, best_second)| {
                    (step, first_index, second_index) < (best_step, best_first, best_second)
                });
                if replace {
                    best = Some((step, first_index, second_index));
                }
            }
        }

        let (step, first_index, second_index) = best?;
        let first = &self.traces[first_index];
        let second = &self.traces[second_index];
        let step_u32 = u32::try_from(step).ok()?;
        Some(DivergenceWitness {
            step: step_u32,
            first_implementation: first.implementation_id.clone(),
            second_implementation: second.implementation_id.clone(),
            common_prefix: first.observations()[..step].to_vec().into_boxed_slice(),
            first_observation: first.observations()[step],
            second_observation: second.observations()[step],
        })
    }

    /// Independently verifies that a witness is the globally canonical first divergence.
    #[must_use]
    pub fn verify_witness(&self, witness: &DivergenceWitness) -> bool {
        self.first_divergence().as_ref() == Some(witness)
    }

    /// Returns prefix-equivalence groups after exactly prefix_len steps.
    ///
    /// Zero steps returns one group containing every implementation. The
    /// returned groups and their members are canonically ordered.
    pub fn partition_after(&self, prefix_len: u32) -> Result<Vec<Box<[String]>>, DivergenceError> {
        if prefix_len > self.steps {
            return Err(DivergenceError::PrefixLengthOutOfRange {
                steps: self.steps,
                requested: prefix_len,
            });
        }
        let prefix_len = usize::try_from(prefix_len).map_err(|_| DivergenceError::StepOverflow)?;
        let mut groups: Vec<(Box<[Hash32]>, Vec<String>)> = Vec::new();
        for trace in self.traces() {
            let prefix = trace.observations()[..prefix_len]
                .to_vec()
                .into_boxed_slice();
            if let Some((_, members)) = groups
                .iter_mut()
                .find(|(known_prefix, _)| known_prefix.as_ref() == prefix.as_ref())
            {
                members.push(trace.implementation_id().to_string());
            } else {
                groups.push((prefix, vec![trace.implementation_id().to_string()]));
            }
        }
        let mut partitions = groups
            .into_iter()
            .map(|(_, mut members)| {
                members.sort();
                members.into_boxed_slice()
            })
            .collect::<Vec<_>>();
        partitions.sort();
        Ok(partitions)
    }

    /// Checks that every later prefix partition refines the preceding partition.
    #[must_use]
    pub fn verify_monotone_refinement(&self) -> bool {
        let Ok(mut previous) = self.partition_after(0) else {
            return false;
        };
        for prefix_len in 1..=self.steps {
            let Ok(current) = self.partition_after(prefix_len) else {
                return false;
            };
            for group in &current {
                let parent_count = previous
                    .iter()
                    .filter(|parent| group.iter().all(|member| parent.contains(member)))
                    .count();
                if parent_count != 1 {
                    return false;
                }
            }
            previous = current;
        }
        true
    }
}

impl CanonicalEncode for DivergenceForest {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.steps.to_be_bytes());
        put_length(output, self.traces.len())?;
        for trace in self.traces() {
            put_blob(output, &trace.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Divergence-structure construction or query failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DivergenceError {
    /// An implementation identifier is empty.
    EmptyImplementationId,
    /// An implementation identifier violates the bounded ASCII profile.
    InvalidImplementationId(TextError),
    /// Two traces use the same implementation identifier.
    DuplicateImplementationId(String),
    /// Traces do not share one exact step count.
    UnequalTraceLength {
        /// Identifier of the mismatched implementation.
        implementation_id: String,
        /// Required step count.
        expected: usize,
        /// Actual step count.
        actual: usize,
    },
    /// A step count cannot be represented by the canonical profile.
    StepOverflow,
    /// A requested prefix is longer than the traces.
    PrefixLengthOutOfRange {
        /// Available steps.
        steps: u32,
        /// Requested prefix length.
        requested: u32,
    },
}

impl fmt::Display for DivergenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImplementationId => {
                formatter.write_str("implementation identifier is empty")
            }
            Self::InvalidImplementationId(error) => error.fmt(formatter),
            Self::DuplicateImplementationId(id) => {
                write!(formatter, "duplicate implementation identifier {id}")
            }
            Self::UnequalTraceLength {
                implementation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "implementation {implementation_id} has {actual} steps; expected {expected}"
            ),
            Self::StepOverflow => formatter.write_str("trace step count overflow"),
            Self::PrefixLengthOutOfRange { steps, requested } => write!(
                formatter,
                "prefix length {requested} exceeds trace length {steps}"
            ),
        }
    }
}

impl core::error::Error for DivergenceError {}

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn hash(value: u8) -> Hash32 {
        Hash32::new([value; 32])
    }

    fn trace(id: &str, observations: &[u8]) -> ImplementationTrace {
        ImplementationTrace::try_new(id, observations.iter().copied().map(hash).collect())
            .unwrap_or_else(|error| panic!("trace construction failed: {error}"))
    }

    #[test]
    fn canonical_bytes_do_not_depend_on_input_order() {
        let first = DivergenceForest::try_new(vec![
            trace("rust", &[1, 2, 3]),
            trace("python", &[1, 2, 4]),
            trace("julia", &[1, 2, 3]),
        ])
        .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        let second = DivergenceForest::try_new(vec![
            trace("julia", &[1, 2, 3]),
            trace("rust", &[1, 2, 3]),
            trace("python", &[1, 2, 4]),
        ])
        .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn first_divergence_is_earliest_then_smallest_pair() {
        let forest = DivergenceForest::try_new(vec![
            trace("zeta", &[0, 1, 2]),
            trace("alpha", &[0, 9, 2]),
            trace("beta", &[0, 9, 3]),
        ])
        .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        let Some(witness) = forest.first_divergence() else {
            panic!("expected a divergence witness");
        };
        assert_eq!(witness.step(), 1);
        assert_eq!(witness.first_implementation(), "alpha");
        assert_eq!(witness.second_implementation(), "zeta");
        assert!(forest.verify_witness(&witness));
    }

    #[test]
    fn converged_outputs_do_not_remerge_prefix_partitions() {
        let forest = DivergenceForest::try_new(vec![
            trace("a", &[0, 1, 9]),
            trace("b", &[0, 2, 9]),
            trace("c", &[0, 1, 9]),
        ])
        .unwrap_or_else(|error| panic!("forest construction failed: {error}"));

        assert_eq!(
            forest.partition_after(1),
            Ok(vec![
                vec!["a".to_string(), "b".to_string(), "c".to_string()].into_boxed_slice()
            ])
        );
        assert_eq!(
            forest.partition_after(2),
            Ok(vec![
                vec!["a".to_string(), "c".to_string()].into_boxed_slice(),
                vec!["b".to_string()].into_boxed_slice(),
            ])
        );
        assert_eq!(forest.partition_after(2), forest.partition_after(3));
        assert!(forest.verify_monotone_refinement());
    }

    #[test]
    fn equal_traces_have_no_divergence() {
        let forest = DivergenceForest::try_new(vec![trace("a", &[1, 2]), trace("b", &[1, 2])])
            .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        assert_eq!(forest.first_divergence(), None);
    }

    #[test]
    fn verifier_rejects_a_valid_pair_that_is_not_globally_first() {
        let forest = DivergenceForest::try_new(vec![
            trace("a", &[0, 0]),
            trace("b", &[0, 1]),
            trace("c", &[1, 1]),
        ])
        .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        let nonminimal = DivergenceWitness {
            step: 1,
            first_implementation: ImplementationId::try_from_string("a".to_string())
                .unwrap_or_else(|error| panic!("identifier construction failed: {error}")),
            second_implementation: ImplementationId::try_from_string("b".to_string())
                .unwrap_or_else(|error| panic!("identifier construction failed: {error}")),
            common_prefix: vec![hash(0)].into_boxed_slice(),
            first_observation: hash(0),
            second_observation: hash(1),
        };
        assert!(!forest.verify_witness(&nonminimal));
        let canonical = forest
            .first_divergence()
            .unwrap_or_else(|| panic!("expected canonical divergence"));
        assert_eq!(canonical.step(), 0);
        assert_eq!(canonical.first_implementation(), "a");
        assert_eq!(canonical.second_implementation(), "c");
        assert!(forest.verify_witness(&canonical));
    }

    #[test]
    fn duplicate_ids_and_unequal_lengths_fail_closed() {
        assert!(matches!(
            DivergenceForest::try_new(vec![trace("a", &[1]), trace("a", &[1])]),
            Err(DivergenceError::DuplicateImplementationId(_))
        ));
        assert!(matches!(
            DivergenceForest::try_new(vec![trace("a", &[1]), trace("b", &[1, 2])]),
            Err(DivergenceError::UnequalTraceLength { .. })
        ));
    }

    #[test]
    fn identifier_profile_and_partition_bounds_are_exact() {
        assert_eq!(
            ImplementationTrace::try_new("", vec![]),
            Err(DivergenceError::EmptyImplementationId)
        );
        assert!(matches!(
            ImplementationTrace::try_new("not-ascii-\u{2603}", vec![]),
            Err(DivergenceError::InvalidImplementationId(
                TextError::NonAscii
            ))
        ));
        let forest = DivergenceForest::try_new(vec![trace("a", &[1])])
            .unwrap_or_else(|error| panic!("forest construction failed: {error}"));
        assert_eq!(
            forest.partition_after(2),
            Err(DivergenceError::PrefixLengthOutOfRange {
                steps: 1,
                requested: 2,
            })
        );
    }
}
