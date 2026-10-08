//! Adoption as consuming transitions, each a pure function of file contents:
//! a candidate whose receipt replayed against the application's current
//! program ([`CheckedCandidate`], holding F3's `Equivalence`), then the
//! complete set of files the adoption writes ([`AdoptionPlan`]), then, once
//! the command line wrote them, the adoption it reports ([`Adopted`]). The
//! command line reads and writes the files; nothing here does I/O.

use crate::contract_files::{ADOPTED_PROGRAM, ADOPTED_RECEIPT};
use crate::transform::{self, Replayed};

use super::{
    Adoption, AdoptionSources, ContractError, ContractSources, GeneratedContract, Usage,
    adoption_directory, diff, generate_contract, replay_reason, unchanged, with_adoption,
};

/// A candidate program whose receipt replayed against the application's
/// current decision program, with the application's current contract.
pub(crate) struct CheckedCandidate {
    current: GeneratedContract,
    candidate: Vec<u8>,
    receipt: Vec<u8>,
    replayed: Replayed,
}

impl CheckedCandidate {
    /// Generates the application's current contract from `sources`, then
    /// replays `receipt` against its decision program and `candidate`.
    ///
    /// # Errors
    /// The current contract's refusal; a candidate that is the current
    /// program; or a receipt that does not replay, at the entry the
    /// adoption would take in `v2/policy.json`.
    pub(crate) fn check(
        sources: ContractSources<'_>,
        candidate: Vec<u8>,
        receipt: Vec<u8>,
    ) -> Result<Self, ContractError> {
        let current = generate_contract(sources)?;
        let version = sources.adoptions.len() + 1;
        let place = format!("v2/policy.json adoptions[{}]", version - 1);
        let directory = adoption_directory(version);
        if candidate == current.program() {
            return Err(unchanged(&place, &directory, version));
        }
        let replayed = transform::replayed(
            &receipt,
            current.program(),
            &candidate,
            transform::DEFAULT_MAX_INPUT_TUPLES,
        )
        .map_err(|refused| {
            ContractError::new(
                &place,
                format!(
                    "{directory}/receipt.json does not replay against version {version}'s program and the candidate: {}",
                    replay_reason(&refused)
                ),
            )
        })?;
        Ok(Self {
            current,
            candidate,
            receipt,
            replayed,
        })
    }

    /// The rules with this adoption appended, binding the policy of the
    /// version it supersedes, and the whole lineage generated from them. No
    /// receipt is enumerated again: generation reuses this command's replays.
    /// The new version must be a program successor of the one it supersedes,
    /// as `contract diff` classifies the two.
    ///
    /// # Errors
    /// The rules' or the generator's refusal, such as `preserved` usage
    /// that the receipt contradicts, or a lineage that would repeat a version;
    /// or a new version of any other kind than a program successor, naming
    /// the kind.
    pub(crate) fn plan(
        self,
        sources: ContractSources<'_>,
        usage: Usage,
    ) -> Result<AdoptionPlan, ContractError> {
        let adoption = Adoption {
            candidate_sha256: transform::sha256_hex(&self.candidate),
            receipt_sha256: transform::sha256_hex(&self.receipt),
            usage,
            superseded_policy_sha256: transform::sha256_hex(self.current.policy()),
        };
        let rules = with_adoption(sources.rules, &adoption)?;
        let mut adoptions = sources.adoptions.to_vec();
        adoptions.push(AdoptionSources {
            candidate: &self.candidate,
            receipt: &self.receipt,
        });
        let mut replayed = self.current.replayed().to_vec();
        replayed.push(self.replayed.clone());
        let planned = ContractSources {
            rules: &rules,
            adoptions: &adoptions,
            replayed: &replayed,
            ..sources
        };
        let generated = generate_contract(planned)?;
        let ordinal = adoptions.len();
        // Only a program successor is admitted at any state by the F6.1
        // upgrade; an adoption replaces nothing but the decision program, so
        // this cross-checks the generator against the classifier.
        let change = diff::between(sources, &self.current, planned, &generated)
            .map_err(|refused| refused.error)?;
        diff::require_successor(
            &change,
            &format!("v2/policy.json adoptions[{}]", ordinal - 1),
            ordinal,
        )?;
        Ok(AdoptionPlan {
            ordinal,
            directory: adoption_directory(ordinal),
            candidate: self.candidate,
            receipt: self.receipt,
            rules,
            generated,
        })
    }
}

/// Everything an adoption writes, computed before any write.
pub(crate) struct AdoptionPlan {
    ordinal: usize,
    directory: String,
    candidate: Vec<u8>,
    receipt: Vec<u8>,
    rules: String,
    generated: GeneratedContract,
}

impl AdoptionPlan {
    /// `v2/adoptions/n`, the directory of the retained files.
    pub(crate) fn directory(&self) -> &str {
        &self.directory
    }

    /// The retained candidate and receipt, by path in the application.
    pub(crate) fn retained(&self) -> [(String, &[u8]); 2] {
        [
            (
                format!("{}/{ADOPTED_PROGRAM}", self.directory),
                &self.candidate,
            ),
            (
                format!("{}/{ADOPTED_RECEIPT}", self.directory),
                &self.receipt,
            ),
        ]
    }

    /// The whole lineage, generated with this adoption.
    pub(crate) fn generated(&self) -> &GeneratedContract {
        &self.generated
    }

    /// The new `v2/policy.json`.
    pub(crate) fn rules(&self) -> &str {
        &self.rules
    }

    /// The adoption, once the command line wrote every planned file.
    pub(crate) fn written(self, artifacts: Vec<String>) -> Adopted {
        Adopted {
            ordinal: self.ordinal,
            directory: self.directory,
            artifacts,
            generated: self.generated,
        }
    }
}

/// A written adoption, for the report.
pub(crate) struct Adopted {
    pub(crate) ordinal: usize,
    pub(crate) directory: String,
    /// Every file written, in write order.
    pub(crate) artifacts: Vec<String>,
    pub(crate) generated: GeneratedContract,
}
