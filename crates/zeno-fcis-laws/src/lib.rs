//! Tool-neutral project laws for production transition authorization.
//!
//! Structural validity does not establish relational properties between a
//! pre-state, an invocation, a successor, and authoritative plans. This crate
//! makes those properties closed, profile-bound values and requires a reviewed
//! checker to evaluate every applicable law before authorization.
//!
//! Formal tools remain optional adapters. Lean, SMT solvers, Flux, Kani, a
//! private ESSO deployment, or another checker can validate retained evidence
//! through [`LawEvidenceVerifier`]. No particular tool is protocol authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use zeno_fcis_catalog::{CatalogError, ProjectCatalog};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_evidence::{CoverageDeclaration, EvidenceEnvelope, SourceBindings};
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, OutboxPlan};
use zeno_fcis_project::{ProfileError, RegistryEntry, RegistryKind, SemanticId, StableName};
use zeno_fcis_value::Value;

/// Canonical project-law manifest format version.
pub const LAW_MANIFEST_FORMAT_VERSION: u16 = 2;
/// Canonical verified law-set format version.
pub const LAW_SET_FORMAT_VERSION: u16 = 1;
/// Canonical per-invocation evaluation format version.
pub const LAW_EVALUATION_FORMAT_VERSION: u16 = 1;
/// Canonical genesis-law evaluation format version.
pub const GENESIS_LAW_EVALUATION_FORMAT_VERSION: u16 = 1;
/// Hard maximum number of definitions, evidence items, or observations.
pub const MAX_PROJECT_LAWS: usize = 4_096;

/// Closed relational-law families understood by the authorization boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LawKind {
    /// Invariants over every committing successor state.
    StateInvariant = 0,
    /// Aggregate asset conservation.
    AssetConservation = 1,
    /// Minting and burning authority and supply relations.
    MintBurnAuthorization = 2,
    /// Equality between semantic debits/credits and authoritative effects.
    DebitCreditEffectEquality = 3,
    /// Fee, scale, dust, and rounding-remainder relations.
    FeeAndRounding = 4,
    /// Authority, subject, asset, and recipient relationships.
    AuthoritySubjectRecipient = 5,
    /// Ordinary rejection carries no successor or authority-bearing plan.
    RejectNoAuthority = 6,
    /// Committed failure changes only the explicitly admitted failure surface.
    CommittedFailureEffects = 7,
}

impl LawKind {
    /// Every law family in stable protocol order.
    pub const ALL: [Self; 8] = [
        Self::StateInvariant,
        Self::AssetConservation,
        Self::MintBurnAuthorization,
        Self::DebitCreditEffectEquality,
        Self::FeeAndRounding,
        Self::AuthoritySubjectRecipient,
        Self::RejectNoAuthority,
        Self::CommittedFailureEffects,
    ];

    const fn mandatory(self) -> bool {
        matches!(
            self,
            Self::StateInvariant | Self::RejectNoAuthority | Self::CommittedFailureEffects
        )
    }
}

impl CanonicalEncode for LawKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Whether one complete law family is required by this project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawFamilyDisposition {
    /// At least one definition in this family is required.
    Required,
    /// The family is inapplicable under an exact reviewed rationale.
    NotApplicable {
        /// Nonzero commitment to the reviewed rationale.
        rationale_hash: Hash32,
    },
}

impl CanonicalEncode for LawFamilyDisposition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Required => output.push(0),
            Self::NotApplicable { rationale_hash } => {
                output.push(1);
                output.extend_from_slice(rationale_hash.as_bytes());
            }
        }
        Ok(())
    }
}

/// Complete policy for one closed law family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawFamilyPolicy {
    kind: LawKind,
    disposition: LawFamilyDisposition,
}

impl LawFamilyPolicy {
    /// Requires at least one law in `kind`.
    #[must_use]
    pub const fn required(kind: LawKind) -> Self {
        Self {
            kind,
            disposition: LawFamilyDisposition::Required,
        }
    }

    /// Marks a non-mandatory family inapplicable under a nonzero rationale.
    pub fn not_applicable(kind: LawKind, rationale_hash: Hash32) -> Result<Self, LawError> {
        if kind.mandatory() {
            return Err(LawError::MandatoryFamilyNotApplicable(kind));
        }
        if rationale_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Rationale));
        }
        Ok(Self {
            kind,
            disposition: LawFamilyDisposition::NotApplicable { rationale_hash },
        })
    }

    /// Returns the family.
    #[must_use]
    pub const fn kind(self) -> LawKind {
        self.kind
    }

    /// Returns its required-or-inapplicable policy.
    #[must_use]
    pub const fn disposition(self) -> LawFamilyDisposition {
        self.disposition
    }
}

impl CanonicalEncode for LawFamilyPolicy {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        self.disposition.encode_to(output)
    }
}

/// Decisions for which one law definition applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DecisionScope {
    /// Accept, Reject, and CommittedFailure.
    Always = 0,
    /// Accept only.
    Accept = 1,
    /// Reject only.
    Reject = 2,
    /// CommittedFailure only.
    CommittedFailure = 3,
    /// Accept and CommittedFailure.
    Committing = 4,
}

impl DecisionScope {
    const fn applies(self, decision: DecisionKind) -> bool {
        match self {
            Self::Always => true,
            Self::Accept => matches!(decision, DecisionKind::Accept),
            Self::Reject => matches!(decision, DecisionKind::Reject),
            Self::CommittedFailure => matches!(decision, DecisionKind::CommittedFailure),
            Self::Committing => !matches!(decision, DecisionKind::Reject),
        }
    }
}

impl CanonicalEncode for DecisionScope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Whether one project law participates in the separately authorized genesis ceremony.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenesisApplicability {
    /// The law must be evaluated for the exact initial state and policy.
    Required,
    /// The law is inapplicable to genesis under a reviewed nonzero rationale.
    NotApplicable {
        /// Commitment to the reviewed inapplicability rationale.
        rationale_hash: Hash32,
    },
}

impl CanonicalEncode for GenesisApplicability {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Required => output.push(0),
            Self::NotApplicable { rationale_hash } => {
                output.push(1);
                output.extend_from_slice(rationale_hash.as_bytes());
            }
        }
        Ok(())
    }
}

/// Tool-neutral evidence coverage required by one law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawEvidenceRequirement {
    /// The reviewed deterministic runtime checker evaluates every invocation.
    RuntimeOnly,
    /// A complete finite domain is retained under an exact domain identity.
    ExhaustiveFinite {
        /// Exact finite-domain definition commitment.
        domain_hash: Hash32,
        /// Exact number of domain members.
        cardinality: u64,
    },
    /// A retained proof establishes the exact theorem claim.
    ProofAssisted {
        /// Exact theorem statement commitment.
        theorem_claim: Hash32,
    },
}

impl LawEvidenceRequirement {
    fn validate(self, claim_hash: Hash32) -> Result<(), LawError> {
        match self {
            Self::RuntimeOnly => Ok(()),
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => {
                if domain_hash == Hash32::ZERO {
                    return Err(LawError::ZeroBinding(LawField::Domain));
                }
                if cardinality == 0 {
                    return Err(LawError::ZeroCardinality);
                }
                Ok(())
            }
            Self::ProofAssisted { theorem_claim } => {
                if theorem_claim == Hash32::ZERO {
                    return Err(LawError::ZeroBinding(LawField::Theorem));
                }
                if theorem_claim != claim_hash {
                    return Err(LawError::TheoremClaimMismatch);
                }
                Ok(())
            }
        }
    }

    const fn requires_retained_evidence(self) -> bool {
        !matches!(self, Self::RuntimeOnly)
    }
}

impl CanonicalEncode for LawEvidenceRequirement {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::RuntimeOnly => output.push(0),
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => {
                output.push(1);
                output.extend_from_slice(domain_hash.as_bytes());
                output.extend_from_slice(&cardinality.to_be_bytes());
            }
            Self::ProofAssisted { theorem_claim } => {
                output.push(2);
                output.extend_from_slice(theorem_claim.as_bytes());
            }
        }
        Ok(())
    }
}

/// One stable project law committed by the profile claim registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawDefinition {
    id: SemanticId,
    name: StableName,
    kind: LawKind,
    scope: DecisionScope,
    genesis: GenesisApplicability,
    claim_hash: Hash32,
    checker_profile_hash: Hash32,
    evidence: LawEvidenceRequirement,
}

impl LawDefinition {
    /// Constructs one bounded tool-neutral law definition.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        kind: LawKind,
        scope: DecisionScope,
        genesis: GenesisApplicability,
        claim_hash: Hash32,
        checker_profile_hash: Hash32,
        evidence: LawEvidenceRequirement,
    ) -> Result<Self, LawError> {
        if claim_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Claim));
        }
        if checker_profile_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::CheckerProfile));
        }
        evidence.validate(claim_hash)?;
        validate_scope(kind, scope)?;
        validate_genesis_applicability(kind, genesis)?;
        Ok(Self {
            id,
            name,
            kind,
            scope,
            genesis,
            claim_hash,
            checker_profile_hash,
            evidence,
        })
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable law name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the law family.
    #[must_use]
    pub const fn kind(&self) -> LawKind {
        self.kind
    }

    /// Returns the decision scope.
    #[must_use]
    pub const fn scope(&self) -> DecisionScope {
        self.scope
    }

    /// Returns whether this law must be checked for genesis.
    #[must_use]
    pub const fn genesis_applicability(&self) -> GenesisApplicability {
        self.genesis
    }

    /// Returns the exact claim commitment.
    #[must_use]
    pub const fn claim_hash(&self) -> Hash32 {
        self.claim_hash
    }

    /// Returns the reviewed checker semantics commitment.
    #[must_use]
    pub const fn checker_profile_hash(&self) -> Hash32 {
        self.checker_profile_hash
    }

    /// Returns the required coverage class.
    #[must_use]
    pub const fn evidence_requirement(&self) -> LawEvidenceRequirement {
        self.evidence
    }

    fn registry_entry<H: CommitmentHasher>(&self) -> Result<RegistryEntry, LawError> {
        RegistryEntry::try_new(
            RegistryKind::Claim,
            self.id,
            self.name.clone(),
            hash_canonical::<H>("zeno-fcis/law-definition", self)?,
        )
        .map_err(LawError::Profile)
    }
}

impl CanonicalEncode for LawDefinition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        self.kind.encode_to(output)?;
        self.scope.encode_to(output)?;
        self.genesis.encode_to(output)?;
        output.extend_from_slice(self.claim_hash.as_bytes());
        output.extend_from_slice(self.checker_profile_hash.as_bytes());
        self.evidence.encode_to(output)
    }
}

/// Complete required-or-inapplicable policy and all stable project laws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawManifest {
    families: Box<[LawFamilyPolicy]>,
    definitions: Box<[LawDefinition]>,
}

impl LawManifest {
    /// Validates completeness, mandatory families, identifiers, names, and scopes.
    pub fn try_new(
        mut families: Vec<LawFamilyPolicy>,
        mut definitions: Vec<LawDefinition>,
    ) -> Result<Self, LawError> {
        if definitions.len() > MAX_PROJECT_LAWS {
            return Err(LawError::ResourceLimit);
        }
        families.sort_by_key(|policy| policy.kind);
        let actual = families
            .iter()
            .map(|policy| policy.kind)
            .collect::<Vec<_>>();
        if actual != LawKind::ALL {
            return Err(LawError::IncompleteFamilyPolicy);
        }
        for family in &families {
            if family.kind.mandatory()
                && matches!(
                    family.disposition,
                    LawFamilyDisposition::NotApplicable { .. }
                )
            {
                return Err(LawError::MandatoryFamilyNotApplicable(family.kind));
            }
            if let LawFamilyDisposition::NotApplicable { rationale_hash } = family.disposition
                && rationale_hash == Hash32::ZERO
            {
                return Err(LawError::ZeroBinding(LawField::Rationale));
            }
        }
        definitions.sort_by_key(LawDefinition::id);
        if definitions.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(LawError::DuplicateLawId);
        }
        let mut names = BTreeSet::new();
        for definition in &definitions {
            if !names.insert(definition.name.clone()) {
                return Err(LawError::DuplicateLawName);
            }
        }
        for family in &families {
            let count = definitions
                .iter()
                .filter(|definition| definition.kind == family.kind)
                .count();
            match family.disposition {
                LawFamilyDisposition::Required if count == 0 => {
                    return Err(LawError::MissingRequiredFamily(family.kind));
                }
                LawFamilyDisposition::NotApplicable { .. } if count != 0 => {
                    return Err(LawError::DefinitionForInapplicableFamily(family.kind));
                }
                _ => {}
            }
        }
        Ok(Self {
            families: families.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
        })
    }

    /// Returns family policies in stable `LawKind` order.
    #[must_use]
    pub const fn families(&self) -> &[LawFamilyPolicy] {
        &self.families
    }

    /// Returns definitions in stable ID order.
    #[must_use]
    pub const fn definitions(&self) -> &[LawDefinition] {
        &self.definitions
    }

    /// Computes the exact policy commitment bound by `ProjectProfile`.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, LawError> {
        hash_canonical::<H>("zeno-fcis/law-manifest", self)
    }

    /// Derives the complete stable claim registry for this manifest.
    pub fn registry_entries<H: CommitmentHasher>(&self) -> Result<Vec<RegistryEntry>, LawError> {
        self.definitions
            .iter()
            .map(LawDefinition::registry_entry::<H>)
            .collect()
    }
}

impl CanonicalEncode for LawManifest {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-LAW-MANIFEST\0");
        output.extend_from_slice(&LAW_MANIFEST_FORMAT_VERSION.to_be_bytes());
        put_u32_length(output, self.families.len())?;
        for family in &self.families {
            family.encode_to(output)?;
        }
        put_u32_length(output, self.definitions.len())?;
        for definition in &self.definitions {
            put_blob(output, &definition.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Untrusted retained formal evidence and the exact artifact bytes to replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawEvidenceInput {
    law_id: SemanticId,
    envelope: EvidenceEnvelope,
    artifact: Box<[u8]>,
}

impl LawEvidenceInput {
    /// Binds an envelope and retained artifact to one stable law.
    #[must_use]
    pub fn new(law_id: SemanticId, envelope: EvidenceEnvelope, artifact: Vec<u8>) -> Self {
        Self {
            law_id,
            envelope,
            artifact: artifact.into_boxed_slice(),
        }
    }

    /// Returns the law identifier.
    #[must_use]
    pub const fn law_id(&self) -> SemanticId {
        self.law_id
    }

    /// Returns the complete retained evidence envelope.
    #[must_use]
    pub const fn envelope(&self) -> &EvidenceEnvelope {
        &self.envelope
    }

    /// Returns the exact retained artifact bytes supplied to the checker.
    #[must_use]
    pub const fn artifact(&self) -> &[u8] {
        &self.artifact
    }
}

/// Exact proof subject independently checked for one project law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawProofSubject {
    source_bindings: SourceBindings,
    catalog_hash: Hash32,
    manifest_hash: Hash32,
    law_id: SemanticId,
    claim_hash: Hash32,
    checker_profile_hash: Hash32,
    query_id_hash: Hash32,
    assumptions_hash: Hash32,
    coverage_hash: Hash32,
    engine_build_hash: Hash32,
}

impl LawProofSubject {
    /// Returns exact source/profile/schema/algorithm bindings.
    #[must_use]
    pub const fn source_bindings(self) -> SourceBindings {
        self.source_bindings
    }

    /// Returns the complete project catalog identity.
    #[must_use]
    pub const fn catalog_hash(self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the complete law manifest identity.
    #[must_use]
    pub const fn manifest_hash(self) -> Hash32 {
        self.manifest_hash
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn law_id(self) -> SemanticId {
        self.law_id
    }

    /// Returns the exact law claim.
    #[must_use]
    pub const fn claim_hash(self) -> Hash32 {
        self.claim_hash
    }

    /// Returns the reviewed checker semantics commitment.
    #[must_use]
    pub const fn checker_profile_hash(self) -> Hash32 {
        self.checker_profile_hash
    }

    /// Returns the exact query identifier commitment.
    #[must_use]
    pub const fn query_id_hash(self) -> Hash32 {
        self.query_id_hash
    }

    /// Returns the exact ordered assumption-set commitment.
    #[must_use]
    pub const fn assumptions_hash(self) -> Hash32 {
        self.assumptions_hash
    }

    /// Returns the exact coverage declaration commitment.
    #[must_use]
    pub const fn coverage_hash(self) -> Hash32 {
        self.coverage_hash
    }

    /// Returns the exact runtime law-engine build commitment.
    #[must_use]
    pub const fn engine_build_hash(self) -> Hash32 {
        self.engine_build_hash
    }
}

impl CanonicalEncode for LawProofSubject {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_source_bindings(output, self.source_bindings);
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.manifest_hash.as_bytes());
        self.law_id.encode_to(output)?;
        output.extend_from_slice(self.claim_hash.as_bytes());
        output.extend_from_slice(self.checker_profile_hash.as_bytes());
        output.extend_from_slice(self.query_id_hash.as_bytes());
        output.extend_from_slice(self.assumptions_hash.as_bytes());
        output.extend_from_slice(self.coverage_hash.as_bytes());
        output.extend_from_slice(self.engine_build_hash.as_bytes());
        Ok(())
    }
}

/// Typed independent checker decision for one exact proof subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawProofDecision {
    /// The retained artifact establishes the exact subject.
    Attested {
        /// Nonzero independent verification claim.
        verification_claim: Hash32,
    },
    /// The subject is refuted by a retained counterexample.
    Refuted {
        /// Nonzero counterexample commitment.
        counterexample_hash: Hash32,
    },
    /// The checker cannot decide; grants no authority.
    Indeterminate,
}

/// Pluggable independent verifier for retained project-law evidence.
///
/// A public Lean/SMT/Flux adapter or a private ESSO adapter can implement this
/// interface. The production authority owns the selected concrete verifier.
pub trait LawEvidenceVerifier {
    /// Returns the exact verifier binary/configuration/environment identity.
    fn verifier_identity(&self) -> Hash32;

    /// Replays the exact retained artifact against the complete proof subject.
    fn verify(
        &self,
        subject: &LawProofSubject,
        envelope: &EvidenceEnvelope,
        artifact: &[u8],
    ) -> LawProofDecision;
}

/// Independently checked evidence retained in one verified law set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedLawEvidence {
    law_id: SemanticId,
    envelope: EvidenceEnvelope,
    subject_hash: Hash32,
    verifier_identity: Hash32,
    verification_claim: Hash32,
}

impl VerifiedLawEvidence {
    /// Returns the stable law identifier.
    #[must_use]
    pub const fn law_id(&self) -> SemanticId {
        self.law_id
    }

    /// Returns the exact producer envelope.
    #[must_use]
    pub const fn envelope(&self) -> &EvidenceEnvelope {
        &self.envelope
    }

    /// Returns the complete checked subject identity.
    #[must_use]
    pub const fn subject_hash(&self) -> Hash32 {
        self.subject_hash
    }

    /// Returns the independent checker identity.
    #[must_use]
    pub const fn verifier_identity(&self) -> Hash32 {
        self.verifier_identity
    }

    /// Returns the independent verification claim.
    #[must_use]
    pub const fn verification_claim(&self) -> Hash32 {
        self.verification_claim
    }
}

impl CanonicalEncode for VerifiedLawEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.law_id.encode_to(output)?;
        put_blob(output, &self.envelope.canonical_bytes()?)?;
        output.extend_from_slice(self.subject_hash.as_bytes());
        output.extend_from_slice(self.verifier_identity.as_bytes());
        output.extend_from_slice(self.verification_claim.as_bytes());
        Ok(())
    }
}

/// Deterministic law-set resource bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawLimits {
    /// Maximum manifest definitions.
    pub max_definitions: u32,
    /// Maximum retained formal evidence envelopes.
    pub max_evidence: u32,
    /// Maximum retained artifact bytes across the complete law set.
    pub max_artifact_bytes: u64,
    /// Maximum per-invocation observations.
    pub max_observations: u32,
}

impl Default for LawLimits {
    fn default() -> Self {
        Self {
            max_definitions: 4_096,
            max_evidence: 4_096,
            max_artifact_bytes: 64 * 1024 * 1024,
            max_observations: 4_096,
        }
    }
}

impl LawLimits {
    fn validate(self) -> Result<(), LawError> {
        let hard = u32::try_from(MAX_PROJECT_LAWS).map_err(|_| LawError::ResourceLimit)?;
        if self.max_definitions == 0
            || self.max_evidence == 0
            || self.max_observations == 0
            || self.max_artifact_bytes == 0
            || self.max_definitions > hard
            || self.max_evidence > hard
            || self.max_observations > hard
        {
            return Err(LawError::InvalidLimits);
        }
        Ok(())
    }
}

impl CanonicalEncode for LawLimits {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.max_definitions.to_be_bytes());
        output.extend_from_slice(&self.max_evidence.to_be_bytes());
        output.extend_from_slice(&self.max_artifact_bytes.to_be_bytes());
        output.extend_from_slice(&self.max_observations.to_be_bytes());
        Ok(())
    }
}

/// Full decision surface presented to relational checkers.
pub enum LawDecisionView<'a> {
    /// An accepted successor and its exact authority-bearing plans.
    Accept {
        /// Admitted successor state.
        post_state: &'a Value,
        /// Exact state patch.
        patch: &'a CanonicalPatch,
        /// Exact authoritative effects.
        commit_plan: &'a CommitPlan,
        /// Exact external obligations.
        outbox_plan: &'a OutboxPlan,
    },
    /// Ordinary rejection; authority-bearing fields are unrepresentable.
    Reject {
        /// Stable rejection reason.
        reason_id: u32,
    },
    /// Intentional committing failure and its exact plans.
    CommittedFailure {
        /// Stable committed-failure reason.
        reason_id: u32,
        /// Admitted successor state.
        post_state: &'a Value,
        /// Exact state patch.
        patch: &'a CanonicalPatch,
        /// Exact authoritative effects.
        commit_plan: &'a CommitPlan,
        /// Exact external obligations.
        outbox_plan: &'a OutboxPlan,
    },
}

impl LawDecisionView<'_> {
    /// Returns the exact three-way decision kind.
    #[must_use]
    pub const fn kind(&self) -> DecisionKind {
        match self {
            Self::Accept { .. } => DecisionKind::Accept,
            Self::Reject { .. } => DecisionKind::Reject,
            Self::CommittedFailure { .. } => DecisionKind::CommittedFailure,
        }
    }
}

impl CanonicalEncode for LawDecisionView<'_> {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Accept {
                post_state,
                patch,
                commit_plan,
                outbox_plan,
            } => {
                output.push(0);
                encode_committing(output, post_state, patch, commit_plan, outbox_plan)?;
            }
            Self::Reject { reason_id } => {
                output.push(1);
                output.extend_from_slice(&reason_id.to_be_bytes());
            }
            Self::CommittedFailure {
                reason_id,
                post_state,
                patch,
                commit_plan,
                outbox_plan,
            } => {
                output.push(2);
                output.extend_from_slice(&reason_id.to_be_bytes());
                encode_committing(output, post_state, patch, commit_plan, outbox_plan)?;
            }
        }
        Ok(())
    }
}

/// Exact invocation and complete decision supplied to the reviewed law checker.
pub struct LawCheckInput<'a> {
    catalog_hash: Hash32,
    invocation_id: Hash32,
    pre_state: &'a Value,
    command: &'a Value,
    context: &'a Value,
    decision: LawDecisionView<'a>,
}

impl<'a> LawCheckInput<'a> {
    /// Constructs one exact law-check input.
    pub fn try_new(
        catalog_hash: Hash32,
        invocation_id: Hash32,
        pre_state: &'a Value,
        command: &'a Value,
        context: &'a Value,
        decision: LawDecisionView<'a>,
    ) -> Result<Self, LawError> {
        if catalog_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Catalog));
        }
        if invocation_id == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Invocation));
        }
        Ok(Self {
            catalog_hash,
            invocation_id,
            pre_state,
            command,
            context,
            decision,
        })
    }

    /// Returns the exact catalog commitment.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the externally bound invocation identity.
    #[must_use]
    pub const fn invocation_id(&self) -> Hash32 {
        self.invocation_id
    }

    /// Returns the admitted pre-state value.
    #[must_use]
    pub const fn pre_state(&self) -> &'a Value {
        self.pre_state
    }

    /// Returns the admitted command value.
    #[must_use]
    pub const fn command(&self) -> &'a Value {
        self.command
    }

    /// Returns the admitted context value.
    #[must_use]
    pub const fn context(&self) -> &'a Value {
        self.context
    }

    /// Returns the complete decision view.
    #[must_use]
    pub const fn decision(&self) -> &LawDecisionView<'a> {
        &self.decision
    }
}

impl CanonicalEncode for LawCheckInput<'_> {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.invocation_id.as_bytes());
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
        put_blob(output, &self.command.canonical_bytes()?)?;
        put_blob(output, &self.context.canonical_bytes()?)?;
        put_blob(output, &self.decision.canonical_bytes()?)
    }
}

/// Exact policy and initial state supplied to the reviewed genesis-law checker.
pub struct GenesisLawCheckInput<'a> {
    catalog_hash: Hash32,
    policy_id: Hash32,
    genesis_binding_hash: Hash32,
    initial_state: &'a Value,
}

impl<'a> GenesisLawCheckInput<'a> {
    /// Constructs one exact genesis-law input.
    pub fn try_new(
        catalog_hash: Hash32,
        policy_id: Hash32,
        genesis_binding_hash: Hash32,
        initial_state: &'a Value,
    ) -> Result<Self, LawError> {
        if catalog_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Catalog));
        }
        if policy_id == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Policy));
        }
        if genesis_binding_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Genesis));
        }
        Ok(Self {
            catalog_hash,
            policy_id,
            genesis_binding_hash,
            initial_state,
        })
    }

    /// Returns the exact catalog commitment.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the complete authorization-policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the reviewed genesis-policy binding commitment.
    #[must_use]
    pub const fn genesis_binding_hash(&self) -> Hash32 {
        self.genesis_binding_hash
    }

    /// Returns the exact schema-admitted initial semantic state.
    #[must_use]
    pub const fn initial_state(&self) -> &'a Value {
        self.initial_state
    }
}

impl CanonicalEncode for GenesisLawCheckInput<'_> {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-GENESIS-LAW-INPUT\0");
        output.extend_from_slice(&GENESIS_LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        output.extend_from_slice(self.genesis_binding_hash.as_bytes());
        put_blob(output, &self.initial_state.canonical_bytes()?)
    }
}

/// Closed checker result for one applicable law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LawStatus {
    /// The law holds for this exact input.
    Satisfied = 0,
    /// A retained counterexample violates the law.
    Violated = 1,
    /// The checker cannot decide within the admitted semantics or bounds.
    Indeterminate = 2,
}

/// One content-bound law result returned by a reviewed checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawObservation {
    law_id: SemanticId,
    status: LawStatus,
    witness_hash: Hash32,
}

impl LawObservation {
    /// Creates a nonzero content-bound result.
    pub fn try_new(
        law_id: SemanticId,
        status: LawStatus,
        witness_hash: Hash32,
    ) -> Result<Self, LawError> {
        if witness_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Observation));
        }
        Ok(Self {
            law_id,
            status,
            witness_hash,
        })
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn law_id(self) -> SemanticId {
        self.law_id
    }

    /// Returns the three-way checker result.
    #[must_use]
    pub const fn status(self) -> LawStatus {
        self.status
    }

    /// Returns the retained result/counterexample commitment.
    #[must_use]
    pub const fn witness_hash(self) -> Hash32 {
        self.witness_hash
    }
}

impl CanonicalEncode for LawObservation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.law_id.encode_to(output)?;
        output.push(self.status as u8);
        output.extend_from_slice(self.witness_hash.as_bytes());
        Ok(())
    }
}

/// Closed failures from one deterministic project law checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawEngineFailure {
    /// The input is outside the checker's reviewed model.
    Unsupported,
    /// Logical fuel or another deterministic bound was exhausted.
    Incomplete,
    /// The checker emitted malformed or contradictory output.
    InvalidOutput,
}

/// Reviewed deterministic runtime checker for one project law manifest.
///
/// Concrete process adapters can call Lean, SMT, Flux, Kani, or a private ESSO
/// checker from a `std` shell. Tool absence, timeout, crash, and `unknown` must
/// map to a non-authoritative failure or indeterminate observation.
pub trait ProjectLawEngine {
    /// Evaluates every non-framework law applicable to this exact invocation.
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure>;

    /// Evaluates every law explicitly registered as applicable to genesis.
    fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure>;
}

/// Verified, profile-bound project laws and their exact reviewed checker.
pub struct VerifiedProjectLaws<H, L>
where
    H: CommitmentHasher,
    L: ProjectLawEngine,
{
    manifest: LawManifest,
    evidence: Box<[VerifiedLawEvidence]>,
    limits: LawLimits,
    source_bindings: SourceBindings,
    catalog_hash: Hash32,
    engine_build_hash: Hash32,
    evidence_verifier_hash: Hash32,
    law_set_hash: Hash32,
    engine: L,
    marker: PhantomData<fn() -> H>,
}

impl<H, L> VerifiedProjectLaws<H, L>
where
    H: CommitmentHasher,
    L: ProjectLawEngine,
{
    /// Returns the complete law manifest.
    #[must_use]
    pub const fn manifest(&self) -> &LawManifest {
        &self.manifest
    }

    /// Returns retained tool evidence in stable law-ID order.
    #[must_use]
    pub const fn evidence(&self) -> &[VerifiedLawEvidence] {
        &self.evidence
    }

    /// Returns the deterministic resource envelope.
    #[must_use]
    pub const fn limits(&self) -> LawLimits {
        self.limits
    }

    /// Returns exact source/profile/schema/algorithm bindings.
    #[must_use]
    pub const fn source_bindings(&self) -> SourceBindings {
        self.source_bindings
    }

    /// Returns the exact project catalog commitment checked for this law set.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the exact reviewed runtime checker build identity.
    #[must_use]
    pub const fn engine_build_hash(&self) -> Hash32 {
        self.engine_build_hash
    }

    /// Returns the independently mounted formal-evidence verifier identity.
    #[must_use]
    pub const fn evidence_verifier_hash(&self) -> Hash32 {
        self.evidence_verifier_hash
    }

    /// Returns the exact reviewed law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Evaluates every applicable law and fails closed on any incomplete result.
    pub fn evaluate(&self, input: &LawCheckInput<'_>) -> Result<LawEvaluation, LawError> {
        if input.catalog_hash != self.catalog_hash {
            return Err(LawError::CatalogMismatch);
        }
        let input_hash = hash_canonical::<H>("zeno-fcis/law-input", input)?;
        let decision = input.decision.kind();
        let mut expected = self
            .manifest
            .definitions
            .iter()
            .filter(|definition| definition.scope.applies(decision))
            .map(LawDefinition::id)
            .collect::<Vec<_>>();
        let framework = self
            .manifest
            .definitions
            .iter()
            .filter(|definition| {
                definition.kind == LawKind::RejectNoAuthority && definition.scope.applies(decision)
            })
            .map(|definition| {
                LawObservation::try_new(definition.id, LawStatus::Satisfied, input_hash)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut observations = self
            .engine
            .evaluate(input, self.limits)
            .map_err(LawError::Engine)?;
        observations.extend(framework);
        if observations.len()
            > usize::try_from(self.limits.max_observations).map_err(|_| LawError::ResourceLimit)?
        {
            return Err(LawError::ResourceLimit);
        }
        expected.sort_unstable();
        observations.sort_by_key(|observation| observation.law_id);
        if observations
            .windows(2)
            .any(|pair| pair[0].law_id == pair[1].law_id)
        {
            return Err(LawError::DuplicateObservation);
        }
        let actual = observations
            .iter()
            .map(|observation| observation.law_id)
            .collect::<Vec<_>>();
        if actual != expected {
            return Err(LawError::ObservationSetMismatch);
        }
        if let Some(observation) = observations
            .iter()
            .find(|observation| observation.status != LawStatus::Satisfied)
        {
            return Err(LawError::LawNotSatisfied {
                law_id: observation.law_id,
                status: observation.status,
                witness_hash: observation.witness_hash,
            });
        }
        LawEvaluation::try_new::<H>(self.law_set_hash, input_hash, decision, observations)
    }

    /// Evaluates the exact complete genesis-applicable law set.
    pub fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
    ) -> Result<GenesisLawEvaluation, LawError> {
        if input.catalog_hash != self.catalog_hash {
            return Err(LawError::CatalogMismatch);
        }
        let input_hash = hash_canonical::<H>("zeno-fcis/genesis-law-input", input)?;
        let mut expected = self
            .manifest
            .definitions
            .iter()
            .filter(|definition| matches!(definition.genesis, GenesisApplicability::Required))
            .map(LawDefinition::id)
            .collect::<Vec<_>>();
        let mut observations = self
            .engine
            .evaluate_genesis(input, self.limits)
            .map_err(LawError::Engine)?;
        if observations.len()
            > usize::try_from(self.limits.max_observations).map_err(|_| LawError::ResourceLimit)?
        {
            return Err(LawError::ResourceLimit);
        }
        expected.sort_unstable();
        observations.sort_by_key(|observation| observation.law_id);
        if observations
            .windows(2)
            .any(|pair| pair[0].law_id == pair[1].law_id)
        {
            return Err(LawError::DuplicateObservation);
        }
        let actual = observations
            .iter()
            .map(|observation| observation.law_id)
            .collect::<Vec<_>>();
        if actual != expected {
            return Err(LawError::ObservationSetMismatch);
        }
        if let Some(observation) = observations
            .iter()
            .find(|observation| observation.status != LawStatus::Satisfied)
        {
            return Err(LawError::LawNotSatisfied {
                law_id: observation.law_id,
                status: observation.status,
                witness_hash: observation.witness_hash,
            });
        }
        GenesisLawEvaluation::try_new::<H>(self.law_set_hash, input_hash, observations)
    }
}

/// Validates a complete law manifest and independently checks retained evidence.
#[allow(clippy::too_many_arguments)]
pub fn verify_project_laws<H, L, V>(
    catalog: &ProjectCatalog,
    manifest: LawManifest,
    source_commit: Hash32,
    mut evidence: Vec<LawEvidenceInput>,
    limits: LawLimits,
    engine_build_hash: Hash32,
    engine: L,
    evidence_verifier: &V,
) -> Result<VerifiedProjectLaws<H, L>, LawError>
where
    H: CommitmentHasher,
    L: ProjectLawEngine,
    V: LawEvidenceVerifier,
{
    limits.validate()?;
    if manifest.definitions.len()
        > usize::try_from(limits.max_definitions).map_err(|_| LawError::ResourceLimit)?
        || evidence.len()
            > usize::try_from(limits.max_evidence).map_err(|_| LawError::ResourceLimit)?
    {
        return Err(LawError::ResourceLimit);
    }
    let evidence_verifier_hash = evidence_verifier.verifier_identity();
    for (hash, field) in [
        (source_commit, LawField::SourceCommit),
        (engine_build_hash, LawField::EngineBuild),
        (evidence_verifier_hash, LawField::EvidenceVerifier),
    ] {
        if hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(field));
        }
    }
    let manifest_hash = manifest.commitment::<H>()?;
    if manifest_hash != catalog.profile().bindings().policy_hash {
        return Err(LawError::PolicyBindingMismatch);
    }
    let expected_registry = manifest.registry_entries::<H>()?;
    let actual_registry = catalog
        .profile()
        .entries()
        .iter()
        .filter(|entry| entry.kind() == RegistryKind::Claim)
        .cloned()
        .collect::<Vec<_>>();
    if expected_registry != actual_registry {
        return Err(LawError::ClaimRegistryMismatch);
    }
    let catalog_hash = catalog.commitment::<H>().map_err(LawError::Catalog)?;
    let bindings = catalog.profile().bindings();
    let source_bindings = SourceBindings::try_new(
        source_commit,
        catalog.profile_hash(),
        catalog.schema_hash(),
        bindings.algorithm_hash,
    )
    .map_err(|_| LawError::EvidenceBindingMismatch)?;
    let artifact_bytes = evidence.iter().try_fold(0_u64, |total, item| {
        let length = u64::try_from(item.artifact.len()).map_err(|_| LawError::ResourceLimit)?;
        total.checked_add(length).ok_or(LawError::ResourceLimit)
    })?;
    if artifact_bytes > limits.max_artifact_bytes {
        return Err(LawError::ResourceLimit);
    }
    evidence.sort_by_key(LawEvidenceInput::law_id);
    if evidence
        .windows(2)
        .any(|pair| pair[0].law_id == pair[1].law_id)
    {
        return Err(LawError::DuplicateLawEvidence);
    }
    let required = manifest
        .definitions
        .iter()
        .filter(|definition| definition.evidence.requires_retained_evidence())
        .map(LawDefinition::id)
        .collect::<Vec<_>>();
    let actual = evidence
        .iter()
        .map(LawEvidenceInput::law_id)
        .collect::<Vec<_>>();
    if actual != required {
        return Err(LawError::EvidenceSetMismatch);
    }
    let mut checked_evidence = Vec::with_capacity(evidence.len());
    for item in evidence {
        let definition = manifest
            .definitions
            .binary_search_by_key(&item.law_id, LawDefinition::id)
            .ok()
            .map(|index| &manifest.definitions[index])
            .ok_or(LawError::EvidenceSetMismatch)?;
        validate_law_evidence(definition, &item.envelope, source_bindings)?;
        validate_assumption_order(&item.envelope)?;
        if H::hash(&item.artifact) != item.envelope.artifact_digest() {
            return Err(LawError::ArtifactDigestMismatch(item.law_id));
        }
        let subject = build_proof_subject::<H>(
            source_bindings,
            catalog_hash,
            manifest_hash,
            definition,
            &item.envelope,
            engine_build_hash,
        )?;
        let subject_hash = hash_canonical::<H>("zeno-fcis/law-proof-subject", &subject)?;
        let verification_claim =
            match evidence_verifier.verify(&subject, &item.envelope, &item.artifact) {
                LawProofDecision::Attested { verification_claim }
                    if verification_claim != Hash32::ZERO =>
                {
                    verification_claim
                }
                LawProofDecision::Attested { .. } => {
                    return Err(LawError::ZeroBinding(LawField::VerificationClaim));
                }
                LawProofDecision::Refuted {
                    counterexample_hash,
                } if counterexample_hash != Hash32::ZERO => {
                    return Err(LawError::EvidenceRefuted {
                        law_id: item.law_id,
                        counterexample_hash,
                    });
                }
                LawProofDecision::Refuted { .. } | LawProofDecision::Indeterminate => {
                    return Err(LawError::EvidenceIndeterminate(item.law_id));
                }
            };
        checked_evidence.push(VerifiedLawEvidence {
            law_id: item.law_id,
            envelope: item.envelope,
            subject_hash,
            verifier_identity: evidence_verifier_hash,
            verification_claim,
        });
    }
    let binding = LawSetBinding {
        catalog_hash,
        manifest_hash,
        source_bindings,
        limits,
        engine_build_hash,
        evidence_verifier_hash,
        evidence: &checked_evidence,
    };
    let law_set_hash = hash_canonical::<H>("zeno-fcis/verified-law-set", &binding)?;
    Ok(VerifiedProjectLaws {
        manifest,
        evidence: checked_evidence.into_boxed_slice(),
        limits,
        source_bindings,
        catalog_hash,
        engine_build_hash,
        evidence_verifier_hash,
        law_set_hash,
        engine,
        marker: PhantomData,
    })
}

/// Successful complete per-invocation law evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawEvaluation {
    law_set_hash: Hash32,
    input_hash: Hash32,
    decision: DecisionKind,
    observations: Box<[LawObservation]>,
    evaluation_hash: Hash32,
}

impl LawEvaluation {
    fn try_new<H: CommitmentHasher>(
        law_set_hash: Hash32,
        input_hash: Hash32,
        decision: DecisionKind,
        observations: Vec<LawObservation>,
    ) -> Result<Self, LawError> {
        let mut value = Self {
            law_set_hash,
            input_hash,
            decision,
            observations: observations.into_boxed_slice(),
            evaluation_hash: Hash32::ZERO,
        };
        value.evaluation_hash = hash_canonical::<H>("zeno-fcis/law-evaluation", &value)?;
        Ok(value)
    }

    /// Returns the verified law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the exact invocation/decision input identity.
    #[must_use]
    pub const fn input_hash(&self) -> Hash32 {
        self.input_hash
    }

    /// Returns the decision kind.
    #[must_use]
    pub const fn decision(&self) -> DecisionKind {
        self.decision
    }

    /// Returns all successful observations in stable law-ID order.
    #[must_use]
    pub const fn observations(&self) -> &[LawObservation] {
        &self.observations
    }

    /// Returns the complete evaluation identity.
    #[must_use]
    pub const fn evaluation_hash(&self) -> Hash32 {
        self.evaluation_hash
    }
}

impl CanonicalEncode for LawEvaluation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-LAW-EVALUATION\0");
        output.extend_from_slice(&LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.input_hash.as_bytes());
        output.push(decision_tag(self.decision));
        put_u32_length(output, self.observations.len())?;
        for observation in &self.observations {
            observation.encode_to(output)?;
        }
        output.extend_from_slice(self.evaluation_hash.as_bytes());
        Ok(())
    }
}

/// Successful complete evaluation of every genesis-applicable project law.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenesisLawEvaluation {
    law_set_hash: Hash32,
    input_hash: Hash32,
    observations: Box<[LawObservation]>,
    evaluation_hash: Hash32,
}

impl GenesisLawEvaluation {
    fn try_new<H: CommitmentHasher>(
        law_set_hash: Hash32,
        input_hash: Hash32,
        observations: Vec<LawObservation>,
    ) -> Result<Self, LawError> {
        let mut value = Self {
            law_set_hash,
            input_hash,
            observations: observations.into_boxed_slice(),
            evaluation_hash: Hash32::ZERO,
        };
        value.evaluation_hash = hash_canonical::<H>("zeno-fcis/genesis-law-evaluation", &value)?;
        Ok(value)
    }

    /// Returns the verified law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the exact genesis-law input identity.
    #[must_use]
    pub const fn input_hash(&self) -> Hash32 {
        self.input_hash
    }

    /// Returns all successful observations in stable law-ID order.
    #[must_use]
    pub const fn observations(&self) -> &[LawObservation] {
        &self.observations
    }

    /// Returns the complete genesis-law evaluation identity.
    #[must_use]
    pub const fn evaluation_hash(&self) -> Hash32 {
        self.evaluation_hash
    }
}

impl CanonicalEncode for GenesisLawEvaluation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-GENESIS-LAW-EVALUATION\0");
        output.extend_from_slice(&GENESIS_LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.input_hash.as_bytes());
        put_u32_length(output, self.observations.len())?;
        for observation in &self.observations {
            observation.encode_to(output)?;
        }
        output.extend_from_slice(self.evaluation_hash.as_bytes());
        Ok(())
    }
}

struct LawSetBinding<'a> {
    catalog_hash: Hash32,
    manifest_hash: Hash32,
    source_bindings: SourceBindings,
    limits: LawLimits,
    engine_build_hash: Hash32,
    evidence_verifier_hash: Hash32,
    evidence: &'a [VerifiedLawEvidence],
}

impl CanonicalEncode for LawSetBinding<'_> {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-VERIFIED-LAW-SET\0");
        output.extend_from_slice(&LAW_SET_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.manifest_hash.as_bytes());
        encode_source_bindings(output, self.source_bindings);
        self.limits.encode_to(output)?;
        output.extend_from_slice(self.engine_build_hash.as_bytes());
        output.extend_from_slice(self.evidence_verifier_hash.as_bytes());
        put_u32_length(output, self.evidence.len())?;
        for evidence in self.evidence {
            put_blob(output, &evidence.canonical_bytes()?)?;
        }
        Ok(())
    }
}

fn validate_scope(kind: LawKind, scope: DecisionScope) -> Result<(), LawError> {
    match (kind, scope) {
        (LawKind::StateInvariant, DecisionScope::Committing)
        | (LawKind::RejectNoAuthority, DecisionScope::Reject)
        | (LawKind::CommittedFailureEffects, DecisionScope::CommittedFailure) => Ok(()),
        (
            LawKind::StateInvariant | LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            _,
        ) => Err(LawError::InvalidMandatoryScope(kind)),
        _ => Ok(()),
    }
}

fn validate_genesis_applicability(
    kind: LawKind,
    genesis: GenesisApplicability,
) -> Result<(), LawError> {
    match (kind, genesis) {
        (LawKind::StateInvariant, GenesisApplicability::Required)
        | (
            LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            GenesisApplicability::NotApplicable { .. },
        ) => {}
        (
            LawKind::StateInvariant | LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            _,
        ) => return Err(LawError::InvalidGenesisApplicability(kind)),
        _ => {}
    }
    if let GenesisApplicability::NotApplicable { rationale_hash } = genesis
        && rationale_hash == Hash32::ZERO
    {
        return Err(LawError::ZeroBinding(LawField::Rationale));
    }
    Ok(())
}

fn validate_law_evidence(
    definition: &LawDefinition,
    envelope: &EvidenceEnvelope,
    bindings: SourceBindings,
) -> Result<(), LawError> {
    if envelope.bindings() != bindings
        || envelope.claim_hash() != definition.claim_hash
        || envelope.query_id() != definition.name.as_str()
    {
        return Err(LawError::EvidenceBindingMismatch);
    }
    match (definition.evidence, envelope.coverage()) {
        (
            LawEvidenceRequirement::ExhaustiveFinite {
                domain_hash: expected_domain,
                cardinality: expected_cardinality,
            },
            CoverageDeclaration::ExhaustiveFinite {
                domain_hash: actual_domain,
                cardinality: actual_cardinality,
            },
        ) if expected_domain == actual_domain && expected_cardinality == actual_cardinality => {
            Ok(())
        }
        (
            LawEvidenceRequirement::ProofAssisted {
                theorem_claim: expected,
            },
            CoverageDeclaration::ProofAssisted {
                theorem_claim: actual,
            },
        ) if expected == actual => Ok(()),
        _ => Err(LawError::EvidenceCoverageMismatch),
    }
}

fn validate_assumption_order(envelope: &EvidenceEnvelope) -> Result<(), LawError> {
    if envelope.assumptions().windows(2).any(|pair| {
        pair[0].label() >= pair[1].label() || pair[0].statement_hash() == pair[1].statement_hash()
    }) {
        return Err(LawError::NonCanonicalAssumptions);
    }
    Ok(())
}

fn build_proof_subject<H: CommitmentHasher>(
    source_bindings: SourceBindings,
    catalog_hash: Hash32,
    manifest_hash: Hash32,
    definition: &LawDefinition,
    envelope: &EvidenceEnvelope,
    engine_build_hash: Hash32,
) -> Result<LawProofSubject, LawError> {
    let query_id_hash = hash_bytes::<H>("zeno-fcis/law-query-id", envelope.query_id().as_bytes())?;
    let mut assumptions = Vec::new();
    put_u32_length(&mut assumptions, envelope.assumptions().len()).map_err(LawError::Encode)?;
    for assumption in envelope.assumptions() {
        put_blob(&mut assumptions, assumption.label().as_bytes()).map_err(LawError::Encode)?;
        assumptions.extend_from_slice(assumption.statement_hash().as_bytes());
    }
    let assumptions_hash = hash_bytes::<H>("zeno-fcis/law-assumptions", &assumptions)?;
    let mut coverage = Vec::new();
    encode_coverage(&mut coverage, envelope.coverage());
    let coverage_hash = hash_bytes::<H>("zeno-fcis/law-coverage", &coverage)?;
    Ok(LawProofSubject {
        source_bindings,
        catalog_hash,
        manifest_hash,
        law_id: definition.id,
        claim_hash: definition.claim_hash,
        checker_profile_hash: definition.checker_profile_hash,
        query_id_hash,
        assumptions_hash,
        coverage_hash,
        engine_build_hash,
    })
}

fn encode_coverage(output: &mut Vec<u8>, coverage: CoverageDeclaration) {
    match coverage {
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash,
            cardinality,
        } => {
            output.push(0);
            output.extend_from_slice(domain_hash.as_bytes());
            output.extend_from_slice(&cardinality.to_be_bytes());
        }
        CoverageDeclaration::Bounded { case_budget } => {
            output.push(1);
            output.extend_from_slice(&case_budget.to_be_bytes());
        }
        CoverageDeclaration::ProofAssisted { theorem_claim } => {
            output.push(2);
            output.extend_from_slice(theorem_claim.as_bytes());
        }
        CoverageDeclaration::Unbounded => output.push(3),
    }
}

fn encode_committing(
    output: &mut Vec<u8>,
    post_state: &Value,
    patch: &CanonicalPatch,
    commit_plan: &CommitPlan,
    outbox_plan: &OutboxPlan,
) -> Result<(), EncodeError> {
    put_blob(output, &post_state.canonical_bytes()?)?;
    put_blob(output, &patch.canonical_bytes()?)?;
    put_blob(output, &commit_plan.canonical_bytes()?)?;
    put_blob(output, &outbox_plan.canonical_bytes()?)
}

fn encode_source_bindings(output: &mut Vec<u8>, bindings: SourceBindings) {
    output.extend_from_slice(bindings.source_commit().as_bytes());
    output.extend_from_slice(bindings.profile_hash().as_bytes());
    output.extend_from_slice(bindings.schema_hash().as_bytes());
    output.extend_from_slice(bindings.algorithm_hash().as_bytes());
}

fn decision_tag(decision: DecisionKind) -> u8 {
    match decision {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, LawError> {
    let bytes = value.canonical_bytes().map_err(LawError::Encode)?;
    let domain = Domain::new(domain_name, 1).map_err(LawError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(LawError::Encode)
}

fn hash_bytes<H: CommitmentHasher>(
    domain_name: &'static str,
    bytes: &[u8],
) -> Result<Hash32, LawError> {
    let domain = Domain::new(domain_name, 1).map_err(LawError::Encode)?;
    commitment::<H>(domain, bytes).map_err(LawError::Encode)
}

fn put_u32_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_u32_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Exact field rejected for a zero authority binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawField {
    /// Family inapplicability rationale.
    Rationale,
    /// Declarative law claim.
    Claim,
    /// Reviewed checker semantics.
    CheckerProfile,
    /// Exhaustive finite domain.
    Domain,
    /// Proof theorem.
    Theorem,
    /// Project catalog.
    Catalog,
    /// Complete authorization policy.
    Policy,
    /// Reviewed genesis-policy binding.
    Genesis,
    /// Exact invocation.
    Invocation,
    /// Law observation or counterexample.
    Observation,
    /// Source revision.
    SourceCommit,
    /// Runtime law-engine build.
    EngineBuild,
    /// Independent evidence-verifier build.
    EvidenceVerifier,
    /// Independent verification claim.
    VerificationClaim,
}

/// Fail-closed law manifest, evidence, or evaluation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LawError {
    /// A required content binding was zero.
    ZeroBinding(LawField),
    /// A mandatory family was marked inapplicable.
    MandatoryFamilyNotApplicable(LawKind),
    /// The family policy does not contain each closed family exactly once.
    IncompleteFamilyPolicy,
    /// A required family has no definition.
    MissingRequiredFamily(LawKind),
    /// An inapplicable family contains a hidden definition.
    DefinitionForInapplicableFamily(LawKind),
    /// Two definitions share one stable identifier.
    DuplicateLawId,
    /// Two definitions share one stable name.
    DuplicateLawName,
    /// A mandatory family uses a scope that weakens its meaning.
    InvalidMandatoryScope(LawKind),
    /// A mandatory framework law has invalid genesis applicability.
    InvalidGenesisApplicability(LawKind),
    /// A proof-assisted theorem does not equal the law claim.
    TheoremClaimMismatch,
    /// Exhaustive coverage declared zero members.
    ZeroCardinality,
    /// Resource limits are zero or exceed hard bounds.
    InvalidLimits,
    /// A deterministic hard limit was exceeded.
    ResourceLimit,
    /// The manifest does not equal the profile policy commitment.
    PolicyBindingMismatch,
    /// Generated claim entries do not equal the complete profile claim registry.
    ClaimRegistryMismatch,
    /// Retained evidence does not equal the exact required law set.
    EvidenceSetMismatch,
    /// Two retained envelopes target one law.
    DuplicateLawEvidence,
    /// Evidence source, claim, or query identity differs.
    EvidenceBindingMismatch,
    /// Evidence coverage differs from the law requirement.
    EvidenceCoverageMismatch,
    /// Retained assumptions are not strictly ordered and duplicate-free.
    NonCanonicalAssumptions,
    /// Retained artifact bytes do not match the envelope digest.
    ArtifactDigestMismatch(SemanticId),
    /// The independently mounted checker refuted the exact law subject.
    EvidenceRefuted {
        /// Stable law identifier.
        law_id: SemanticId,
        /// Retained counterexample commitment.
        counterexample_hash: Hash32,
    },
    /// The independently mounted checker could not attest the exact law.
    EvidenceIndeterminate(SemanticId),
    /// An invocation was checked against another catalog.
    CatalogMismatch,
    /// The checker returned one law more than once.
    DuplicateObservation,
    /// Missing or hidden extra observations were returned.
    ObservationSetMismatch,
    /// One exact law was violated or indeterminate.
    LawNotSatisfied {
        /// Stable law identifier.
        law_id: SemanticId,
        /// Checker result.
        status: LawStatus,
        /// Result or counterexample commitment.
        witness_hash: Hash32,
    },
    /// The reviewed runtime checker failed closed.
    Engine(LawEngineFailure),
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Catalog validation or commitment failed.
    Catalog(CatalogError),
    /// Stable registry construction failed.
    Profile(ProfileError),
}

impl fmt::Display for LawError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "project law validation failed: {self:?}")
    }
}

impl core::error::Error for LawError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_catalog::{CatalogLimits, CatalogManifest};
    use zeno_fcis_evidence::{Assumption, EvidenceResult, ToolIdentity};
    use zeno_fcis_project::{DomainPrefix, ProfileBindings, ProjectProfile};
    use zeno_fcis_refine::ToolKind;
    use zeno_fcis_schema::{Schema, SchemaLimits, TypeDef, TypeId, TypeKind};

    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/law-hash";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_mul(31)
                    .wrapping_add(*byte)
                    .wrapping_add(u8::try_from(slot).unwrap_or(0));
            }
            if output == [0; 32] {
                output[0] = 1;
            }
            Hash32::new(output)
        }
    }

    fn hash(label: &[u8]) -> Hash32 {
        TestHasher::hash(label)
    }

    fn id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("id: {error}"))
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
    }

    fn law(
        raw_id: u32,
        label: &str,
        kind: LawKind,
        scope: DecisionScope,
        requirement: LawEvidenceRequirement,
    ) -> LawDefinition {
        LawDefinition::try_new(
            id(raw_id),
            name(label),
            kind,
            scope,
            match kind {
                LawKind::StateInvariant | LawKind::AssetConservation => {
                    GenesisApplicability::Required
                }
                _ => GenesisApplicability::NotApplicable {
                    rationale_hash: hash(format!("no-genesis-{label}").as_bytes()),
                },
            },
            hash(label.as_bytes()),
            hash(format!("checker-{label}").as_bytes()),
            requirement,
        )
        .unwrap_or_else(|error| panic!("law: {error}"))
    }

    fn manifest() -> LawManifest {
        let proof_claim = hash(b"asset-conservation");
        LawManifest::try_new(
            LawKind::ALL
                .iter()
                .copied()
                .map(|kind| {
                    if matches!(
                        kind,
                        LawKind::StateInvariant
                            | LawKind::AssetConservation
                            | LawKind::RejectNoAuthority
                            | LawKind::CommittedFailureEffects
                    ) {
                        LawFamilyPolicy::required(kind)
                    } else {
                        LawFamilyPolicy::not_applicable(kind, hash(&[kind as u8, 0xa5]))
                            .unwrap_or_else(|error| panic!("family: {error}"))
                    }
                })
                .collect(),
            vec![
                law(
                    100,
                    "state-invariant",
                    LawKind::StateInvariant,
                    DecisionScope::Committing,
                    LawEvidenceRequirement::RuntimeOnly,
                ),
                law(
                    101,
                    "asset-conservation",
                    LawKind::AssetConservation,
                    DecisionScope::Committing,
                    LawEvidenceRequirement::ProofAssisted {
                        theorem_claim: proof_claim,
                    },
                ),
                law(
                    102,
                    "reject-no-authority",
                    LawKind::RejectNoAuthority,
                    DecisionScope::Reject,
                    LawEvidenceRequirement::RuntimeOnly,
                ),
                law(
                    103,
                    "committed-failure-effects",
                    LawKind::CommittedFailureEffects,
                    DecisionScope::CommittedFailure,
                    LawEvidenceRequirement::RuntimeOnly,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"))
    }

    fn schema() -> Schema {
        let type_def = |raw, label| {
            TypeDef::try_new(
                TypeId::new(raw),
                label,
                TypeKind::Bool,
                SchemaLimits::default(),
            )
            .unwrap_or_else(|error| panic!("type: {error}"))
        };
        Schema::try_new(
            "LawFixture",
            1,
            TypeId::new(1),
            vec![
                type_def(1, "State"),
                type_def(2, "Command"),
                type_def(3, "Context"),
            ],
            SchemaLimits::default(),
        )
        .unwrap_or_else(|error| panic!("schema: {error}"))
    }

    fn root_entry(kind: RegistryKind, raw_id: u32, label: &str) -> RegistryEntry {
        RegistryEntry::try_new(kind, id(raw_id), name(label), hash(label.as_bytes()))
            .unwrap_or_else(|error| panic!("entry: {error}"))
    }

    fn catalog(manifest: &LawManifest) -> ProjectCatalog {
        let schema = schema();
        let catalog_manifest =
            CatalogManifest::try_new::<TestHasher>(Vec::new(), Vec::new(), Vec::new())
                .unwrap_or_else(|error| panic!("catalog manifest: {error}"));
        let mut entries = vec![
            root_entry(RegistryKind::StateType, 1, "state"),
            root_entry(RegistryKind::CommandType, 2, "command"),
            root_entry(RegistryKind::ContextType, 3, "context"),
        ];
        entries.extend_from_slice(catalog_manifest.registry_entries());
        entries.extend(
            manifest
                .registry_entries::<TestHasher>()
                .unwrap_or_else(|error| panic!("law entries: {error}")),
        );
        let profile = ProjectProfile::try_new(
            name("fixture"),
            name("laws"),
            id(900),
            1,
            id(1),
            id(2),
            id(3),
            DomainPrefix::try_new("fixture/laws").unwrap_or_else(|error| panic!("domain: {error}")),
            ProfileBindings {
                schema_hash: schema
                    .schema_hash::<TestHasher>()
                    .unwrap_or_else(|error| panic!("schema hash: {error}")),
                precedence_hash: catalog_manifest.precedence_hash(),
                algorithm_hash: hash(b"algorithm"),
                codec_hash: hash(b"codec"),
                effect_registry_hash: catalog_manifest.effect_registry_hash(),
                channel_registry_hash: catalog_manifest.channel_registry_hash(),
                policy_hash: manifest
                    .commitment::<TestHasher>()
                    .unwrap_or_else(|error| panic!("policy hash: {error}")),
            },
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"));
        ProjectCatalog::try_new::<TestHasher>(
            profile,
            schema,
            catalog_manifest,
            CatalogLimits::default(),
        )
        .unwrap_or_else(|error| panic!("catalog: {error}"))
    }

    fn proof_input(catalog: &ProjectCatalog, artifact: &[u8]) -> LawEvidenceInput {
        let source = SourceBindings::try_new(
            hash(b"source"),
            catalog.profile_hash(),
            catalog.schema_hash(),
            catalog.profile().bindings().algorithm_hash,
        )
        .unwrap_or_else(|error| panic!("source: {error}"));
        let envelope = EvidenceEnvelope::try_new(
            ToolIdentity::try_new("cvc5", "fixture", hash(b"cvc5-binary"))
                .unwrap_or_else(|error| panic!("tool: {error}")),
            ToolKind::Cvc5,
            source,
            "asset-conservation",
            hash(b"asset-conservation"),
            Vec::new(),
            EvidenceResult::Proven,
            TestHasher::hash(artifact),
            CoverageDeclaration::ProofAssisted {
                theorem_claim: hash(b"asset-conservation"),
            },
        )
        .unwrap_or_else(|error| panic!("envelope: {error}"));
        LawEvidenceInput::new(id(101), envelope, artifact.to_vec())
    }

    struct AttestingVerifier;

    impl LawEvidenceVerifier for AttestingVerifier {
        fn verifier_identity(&self) -> Hash32 {
            hash(b"independent-verifier")
        }

        fn verify(
            &self,
            subject: &LawProofSubject,
            _envelope: &EvidenceEnvelope,
            artifact: &[u8],
        ) -> LawProofDecision {
            if subject.law_id() == id(101) && artifact == b"checked-proof" {
                LawProofDecision::Attested {
                    verification_claim: hash(b"verified-proof"),
                }
            } else {
                LawProofDecision::Indeterminate
            }
        }
    }

    #[derive(Clone, Copy)]
    enum EngineMode {
        Pass,
        Missing,
        Violate,
    }

    struct FixtureEngine(EngineMode);

    impl ProjectLawEngine for FixtureEngine {
        fn evaluate(
            &self,
            input: &LawCheckInput<'_>,
            _limits: LawLimits,
        ) -> Result<Vec<LawObservation>, LawEngineFailure> {
            if matches!(input.decision().kind(), DecisionKind::Reject) {
                return Ok(Vec::new());
            }
            let mut observations = vec![
                LawObservation::try_new(id(100), LawStatus::Satisfied, hash(b"state-ok"))
                    .unwrap_or_else(|error| panic!("observation: {error}")),
            ];
            if !matches!(self.0, EngineMode::Missing) {
                observations.push(
                    LawObservation::try_new(
                        id(101),
                        if matches!(self.0, EngineMode::Violate) {
                            LawStatus::Violated
                        } else {
                            LawStatus::Satisfied
                        },
                        hash(b"conservation-result"),
                    )
                    .unwrap_or_else(|error| panic!("observation: {error}")),
                );
            }
            Ok(observations)
        }

        fn evaluate_genesis(
            &self,
            _: &GenesisLawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, LawEngineFailure> {
            let mut observations = vec![
                LawObservation::try_new(id(100), LawStatus::Satisfied, hash(b"genesis-state"))
                    .unwrap_or_else(|error| panic!("observation: {error}")),
            ];
            if !matches!(self.0, EngineMode::Missing) {
                observations.push(
                    LawObservation::try_new(
                        id(101),
                        if matches!(self.0, EngineMode::Violate) {
                            LawStatus::Violated
                        } else {
                            LawStatus::Satisfied
                        },
                        hash(b"genesis-conservation"),
                    )
                    .unwrap_or_else(|error| panic!("observation: {error}")),
                );
            }
            Ok(observations)
        }
    }

    fn verified(mode: EngineMode) -> VerifiedProjectLaws<TestHasher, FixtureEngine> {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        verify_project_laws::<TestHasher, _, _>(
            &catalog,
            manifest,
            hash(b"source"),
            vec![proof_input(&catalog, b"checked-proof")],
            LawLimits::default(),
            hash(b"engine-build"),
            FixtureEngine(mode),
            &AttestingVerifier,
        )
        .unwrap_or_else(|error| panic!("verified laws: {error}"))
    }

    fn accept_input<'a>(
        catalog: &ProjectCatalog,
        state: &'a Value,
        command: &'a Value,
        context: &'a Value,
        patch: &'a CanonicalPatch,
        commit_plan: &'a CommitPlan,
        outbox_plan: &'a OutboxPlan,
    ) -> LawCheckInput<'a> {
        LawCheckInput::try_new(
            catalog
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("catalog hash: {error}")),
            hash(b"invocation"),
            state,
            command,
            context,
            LawDecisionView::Accept {
                post_state: state,
                patch,
                commit_plan,
                outbox_plan,
            },
        )
        .unwrap_or_else(|error| panic!("input: {error}"))
    }

    fn genesis_input<'a>(catalog: &ProjectCatalog, state: &'a Value) -> GenesisLawCheckInput<'a> {
        GenesisLawCheckInput::try_new(
            catalog
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("catalog hash: {error}")),
            hash(b"policy"),
            hash(b"genesis-binding"),
            state,
        )
        .unwrap_or_else(|error| panic!("genesis input: {error}"))
    }

    #[test]
    fn proof_and_runtime_evaluation_both_bind_acceptance() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let laws = verified(EngineMode::Pass);
        let state = Value::Bool(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let input = accept_input(
            &catalog,
            &state,
            &Value::Bool(true),
            &Value::Bool(false),
            &patch,
            &commit_plan,
            &outbox_plan,
        );
        let evaluation = laws
            .evaluate(&input)
            .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert_eq!(evaluation.decision(), DecisionKind::Accept);
        assert_eq!(evaluation.observations().len(), 2);
    }

    #[test]
    fn formal_certificate_does_not_override_runtime_violation() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let laws = verified(EngineMode::Violate);
        let state = Value::Bool(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let input = accept_input(
            &catalog,
            &state,
            &Value::Bool(true),
            &Value::Bool(false),
            &patch,
            &commit_plan,
            &outbox_plan,
        );
        assert!(matches!(
            laws.evaluate(&input),
            Err(LawError::LawNotSatisfied { law_id, .. }) if law_id == id(101)
        ));
    }

    #[test]
    fn every_applicable_law_is_evaluated_exactly_once() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let laws = verified(EngineMode::Missing);
        let state = Value::Bool(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let input = accept_input(
            &catalog,
            &state,
            &Value::Bool(true),
            &Value::Bool(false),
            &patch,
            &commit_plan,
            &outbox_plan,
        );
        assert!(matches!(
            laws.evaluate(&input),
            Err(LawError::ObservationSetMismatch)
        ));
    }

    #[test]
    fn genesis_evaluates_the_exact_required_law_set() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let state = Value::Bool(false);
        let input = genesis_input(&catalog, &state);
        let evaluation = verified(EngineMode::Pass)
            .evaluate_genesis(&input)
            .unwrap_or_else(|error| panic!("genesis evaluation: {error}"));

        assert_eq!(evaluation.observations().len(), 2);
        assert_eq!(evaluation.observations()[0].law_id(), id(100));
        assert_eq!(evaluation.observations()[1].law_id(), id(101));
    }

    #[test]
    fn genesis_missing_or_violated_law_fails_closed() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let state = Value::Bool(false);
        let input = genesis_input(&catalog, &state);

        assert!(matches!(
            verified(EngineMode::Missing).evaluate_genesis(&input),
            Err(LawError::ObservationSetMismatch)
        ));
        assert!(matches!(
            verified(EngineMode::Violate).evaluate_genesis(&input),
            Err(LawError::LawNotSatisfied { law_id, .. }) if law_id == id(101)
        ));
    }

    #[test]
    fn mandatory_genesis_applicability_cannot_be_weakened() {
        let state_result = LawDefinition::try_new(
            id(200),
            name("invalid-state-genesis"),
            LawKind::StateInvariant,
            DecisionScope::Committing,
            GenesisApplicability::NotApplicable {
                rationale_hash: hash(b"invalid"),
            },
            hash(b"state-claim"),
            hash(b"state-checker"),
            LawEvidenceRequirement::RuntimeOnly,
        );
        assert!(matches!(
            state_result,
            Err(LawError::InvalidGenesisApplicability(
                LawKind::StateInvariant
            ))
        ));

        let rejection_result = LawDefinition::try_new(
            id(201),
            name("invalid-reject-genesis"),
            LawKind::RejectNoAuthority,
            DecisionScope::Reject,
            GenesisApplicability::Required,
            hash(b"reject-claim"),
            hash(b"reject-checker"),
            LawEvidenceRequirement::RuntimeOnly,
        );
        assert!(matches!(
            rejection_result,
            Err(LawError::InvalidGenesisApplicability(
                LawKind::RejectNoAuthority
            ))
        ));
    }

    #[test]
    fn artifact_mutation_fails_before_checker_attestation() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let mut input = proof_input(&catalog, b"checked-proof");
        input.artifact[0] ^= 1;
        let result = verify_project_laws::<TestHasher, _, _>(
            &catalog,
            manifest,
            hash(b"source"),
            vec![input],
            LawLimits::default(),
            hash(b"engine-build"),
            FixtureEngine(EngineMode::Pass),
            &AttestingVerifier,
        );
        assert!(matches!(
            result,
            Err(LawError::ArtifactDigestMismatch(law_id)) if law_id == id(101)
        ));
    }

    #[test]
    fn noncanonical_assumptions_fail_closed() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let source = SourceBindings::try_new(
            hash(b"source"),
            catalog.profile_hash(),
            catalog.schema_hash(),
            catalog.profile().bindings().algorithm_hash,
        )
        .unwrap_or_else(|error| panic!("source: {error}"));
        let artifact = b"checked-proof";
        let envelope = EvidenceEnvelope::try_new(
            ToolIdentity::try_new("cvc5", "fixture", hash(b"cvc5-binary"))
                .unwrap_or_else(|error| panic!("tool: {error}")),
            ToolKind::Cvc5,
            source,
            "asset-conservation",
            hash(b"asset-conservation"),
            vec![
                Assumption::try_new("z-last", hash(b"z"))
                    .unwrap_or_else(|error| panic!("assumption: {error}")),
                Assumption::try_new("a-first", hash(b"a"))
                    .unwrap_or_else(|error| panic!("assumption: {error}")),
            ],
            EvidenceResult::Proven,
            TestHasher::hash(artifact),
            CoverageDeclaration::ProofAssisted {
                theorem_claim: hash(b"asset-conservation"),
            },
        )
        .unwrap_or_else(|error| panic!("envelope: {error}"));
        let result = verify_project_laws::<TestHasher, _, _>(
            &catalog,
            manifest,
            hash(b"source"),
            vec![LawEvidenceInput::new(id(101), envelope, artifact.to_vec())],
            LawLimits::default(),
            hash(b"engine-build"),
            FixtureEngine(EngineMode::Pass),
            &AttestingVerifier,
        );
        assert!(matches!(result, Err(LawError::NonCanonicalAssumptions)));
    }

    #[test]
    fn ordinary_reject_needs_no_engine_observation_for_framework_purity_law() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let laws = verified(EngineMode::Pass);
        let state = Value::Bool(false);
        let input = LawCheckInput::try_new(
            catalog
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("catalog hash: {error}")),
            hash(b"reject-invocation"),
            &state,
            &Value::Bool(true),
            &Value::Bool(false),
            LawDecisionView::Reject { reason_id: 7 },
        )
        .unwrap_or_else(|error| panic!("input: {error}"));
        let evaluation = laws
            .evaluate(&input)
            .unwrap_or_else(|error| panic!("reject evaluation: {error}"));
        assert_eq!(evaluation.observations().len(), 1);
        assert_eq!(evaluation.observations()[0].law_id(), id(102));
    }
}
