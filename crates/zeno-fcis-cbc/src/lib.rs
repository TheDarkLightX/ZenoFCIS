//! First-class correctness-by-construction laws for ZenoFCIS transitions.
//!
//! Structural validity is necessary but not sufficient for value-moving
//! correctness. A schema can admit a transition that debits one amount and emits
//! a transfer for another amount. This crate binds project law definitions to the
//! profile `Claim` registry, reconstructs one complete validated transition
//! subject, executes project-specific pure checks, verifies exact proof artifacts,
//! and returns a nominal [`LawVerifiedTransition`] only when every applicable law
//! succeeds.
//!
//! The library does not know project economics. Projects supply a pure
//! [`LawChecker`] and, when required, an independent [`LawEvidenceVerifier`].
//! Production authority must own those implementations and refuse raw transition
//! decisions that lack the nominal law-verified witness.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_catalog::{CatalogError, ProjectCatalog};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::{Decision, DecisionKind};
use zeno_fcis_patch::{CanonicalPatch, PatchError};
use zeno_fcis_plan::{CommitPlan, OutboxPlan};
use zeno_fcis_project::{RegistryKind, SemanticId, StableName};
use zeno_fcis_receipt::SealError;
use zeno_fcis_transition::{TransitionDecision, TransitionError, validate_transition_decision};
use zeno_fcis_value::Value;

/// Canonical format version for law sets, subjects, claims, evidence, and reports.
pub const CBC_FORMAT_VERSION: u16 = 1;
/// Maximum law definitions in one required law set.
pub const MAX_CBC_LAWS: usize = 65_536;
/// Maximum evidence items supplied to one evaluation.
pub const MAX_CBC_EVIDENCE: usize = 65_536;
/// Maximum blockers retained in one evaluation report.
pub const MAX_CBC_BLOCKERS: usize = 65_536;

/// Semantic class of one project law.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LawKind {
    /// Predicate over every admitted authoritative state.
    StateInvariant = 0,
    /// Relation between pre-state, command/context, and post-state.
    TransitionRelation = 1,
    /// Conservation of one or more value domains.
    Conservation = 2,
    /// Mint/burn and total-supply relationship.
    MintBurn = 3,
    /// Debit/credit/effect equality.
    DebitCreditEffect = 4,
    /// Fee, fixed-point, dust, and rounding relationship.
    FeeRounding = 5,
    /// Capability, signature, principal, or authority derivation.
    Authority = 6,
    /// Ordinary rejection purity and unchanged authority.
    RejectPurity = 7,
    /// Permitted committed-failure state/effect relationship.
    CommittedFailure = 8,
    /// Cross-component or sequential/parallel composition relationship.
    Composition = 9,
    /// Project-defined law outside the common registry.
    Custom = 10,
}

impl CanonicalEncode for LawKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Decision classes to which one law applies.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum DecisionScope {
    /// Accepted, rejected, and committed-failure decisions.
    All = 0,
    /// Accepted decisions only.
    Accept = 1,
    /// Ordinary rejections only.
    Reject = 2,
    /// Committed failures only.
    CommittedFailure = 3,
    /// Accepted and committed-failure decisions.
    Committed = 4,
}

impl DecisionScope {
    /// Returns whether the law applies to a decision class.
    #[must_use]
    pub const fn applies(self, kind: DecisionKind) -> bool {
        match self {
            Self::All => true,
            Self::Accept => matches!(kind, DecisionKind::Accept),
            Self::Reject => matches!(kind, DecisionKind::Reject),
            Self::CommittedFailure => matches!(kind, DecisionKind::CommittedFailure),
            Self::Committed => {
                matches!(kind, DecisionKind::Accept | DecisionKind::CommittedFailure)
            }
        }
    }
}

impl CanonicalEncode for DecisionScope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Required checking mode for one law.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LawRequirement {
    /// A project-owned pure executable checker must succeed.
    Executable = 0,
    /// An independent evidence verifier must accept the exact claim.
    Evidence = 1,
    /// Both executable and independent evidence checks must succeed.
    ExecutableAndEvidence = 2,
}

impl LawRequirement {
    const fn requires_executable(self) -> bool {
        matches!(self, Self::Executable | Self::ExecutableAndEvidence)
    }

    const fn requires_evidence(self) -> bool {
        matches!(self, Self::Evidence | Self::ExecutableAndEvidence)
    }
}

impl CanonicalEncode for LawRequirement {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// One stable, profile-registered project law.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawDefinition {
    id: SemanticId,
    name: StableName,
    kind: LawKind,
    scope: DecisionScope,
    requirement: LawRequirement,
    statement_hash: Hash32,
}

impl LawDefinition {
    /// Creates a non-placeholder law definition.
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        kind: LawKind,
        scope: DecisionScope,
        requirement: LawRequirement,
        statement_hash: Hash32,
    ) -> Result<Self, CbcError> {
        if statement_hash == Hash32::ZERO {
            return Err(CbcError::ZeroHash);
        }
        Ok(Self {
            id,
            name,
            kind,
            scope,
            requirement,
            statement_hash,
        })
    }

    /// Returns the stable claim identifier.
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable readable name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the semantic law class.
    #[must_use]
    pub const fn kind(&self) -> LawKind {
        self.kind
    }

    /// Returns the decision scope.
    #[must_use]
    pub const fn scope(&self) -> DecisionScope {
        self.scope
    }

    /// Returns the required checker/evidence mode.
    #[must_use]
    pub const fn requirement(&self) -> LawRequirement {
        self.requirement
    }

    /// Returns the reviewed mathematical/specification statement commitment.
    #[must_use]
    pub const fn statement_hash(&self) -> Hash32 {
        self.statement_hash
    }
}

impl CanonicalEncode for LawDefinition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        self.kind.encode_to(output)?;
        self.scope.encode_to(output)?;
        self.requirement.encode_to(output)?;
        output.extend_from_slice(self.statement_hash.as_bytes());
        Ok(())
    }
}

/// Complete required law set for one exact catalog and transition build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawSet {
    profile_hash: Hash32,
    schema_hash: Hash32,
    catalog_hash: Hash32,
    policy_hash: Hash32,
    transition_build_hash: Hash32,
    laws: Box<[LawDefinition]>,
}

impl LawSet {
    /// Reconstructs and binds every profile `Claim` registry entry as a required law.
    pub fn try_new<H: CommitmentHasher>(
        catalog: &ProjectCatalog,
        transition_build_hash: Hash32,
        mut laws: Vec<LawDefinition>,
    ) -> Result<Self, CbcError> {
        if transition_build_hash == Hash32::ZERO {
            return Err(CbcError::ZeroHash);
        }
        if laws.is_empty() || laws.len() > MAX_CBC_LAWS {
            return Err(CbcError::LawCardinality);
        }
        laws.sort_by_key(LawDefinition::id);
        if laws.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(CbcError::DuplicateLaw);
        }

        let registered: Vec<_> = catalog
            .profile()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == RegistryKind::Claim)
            .collect();
        if registered.len() != laws.len() {
            return Err(CbcError::ClaimRegistryMismatch);
        }
        for law in &laws {
            let Some(entry) = catalog.profile().entry(RegistryKind::Claim, law.id) else {
                return Err(CbcError::UnknownClaim(law.id));
            };
            if entry.name() != &law.name || entry.definition_hash() != law.statement_hash {
                return Err(CbcError::ClaimRegistryMismatch);
            }
        }

        let profile_hash = catalog.profile_hash();
        let schema_hash = catalog.schema_hash();
        let catalog_hash = catalog.commitment::<H>()?;
        let policy_hash = catalog.profile().bindings().policy_hash;
        if [profile_hash, schema_hash, catalog_hash, policy_hash].contains(&Hash32::ZERO) {
            return Err(CbcError::ZeroHash);
        }
        Ok(Self {
            profile_hash,
            schema_hash,
            catalog_hash,
            policy_hash,
            transition_build_hash,
            laws: laws.into_boxed_slice(),
        })
    }

    /// Returns the exact profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the exact schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the exact catalog commitment.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the profile policy/invariant commitment.
    #[must_use]
    pub const fn policy_hash(&self) -> Hash32 {
        self.policy_hash
    }

    /// Returns the exact reviewed transition-build commitment.
    #[must_use]
    pub const fn transition_build_hash(&self) -> Hash32 {
        self.transition_build_hash
    }

    /// Returns required laws in stable identifier order.
    #[must_use]
    pub const fn laws(&self) -> &[LawDefinition] {
        &self.laws
    }

    /// Computes the complete law-set identity.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CbcError> {
        hash_canonical::<H>("zeno-fcis/cbc-law-set", self)
    }
}

impl CanonicalEncode for LawSet {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CBC-LAW-SET\0");
        output.extend_from_slice(&CBC_FORMAT_VERSION.to_be_bytes());
        for hash in [
            self.profile_hash,
            self.schema_hash,
            self.catalog_hash,
            self.policy_hash,
            self.transition_build_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_u32_length(output, self.laws.len())?;
        for law in &self.laws {
            put_blob(output, &law.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Complete immutable relation subject reconstructed from one validated transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawSubject {
    profile_hash: Hash32,
    schema_hash: Hash32,
    catalog_hash: Hash32,
    transition_build_hash: Hash32,
    decision_kind: DecisionKind,
    reason_id: Option<SemanticId>,
    command_hash: Hash32,
    context_hash: Hash32,
    pre_root: Hash32,
    post_root: Hash32,
    candidate_id: Option<Hash32>,
    pre_state: Value,
    post_state: Value,
    patch: Option<CanonicalPatch>,
    commit_plan: CommitPlan,
    outbox_plan: OutboxPlan,
}

impl LawSubject {
    /// Validates one complete transition and reconstructs its relational subject.
    pub fn from_transition<H: CommitmentHasher>(
        law_set: &LawSet,
        catalog: &ProjectCatalog,
        pre_state: &Value,
        state_domain: Domain<'_>,
        decision: &TransitionDecision,
    ) -> Result<Self, CbcError> {
        let catalog_hash = catalog.commitment::<H>()?;
        if law_set.profile_hash != catalog.profile_hash()
            || law_set.schema_hash != catalog.schema_hash()
            || law_set.catalog_hash != catalog_hash
        {
            return Err(CbcError::LawSetBindingMismatch);
        }
        validate_transition_decision::<H>(decision, catalog, pre_state, state_domain)?;

        match decision {
            Decision::Accept(accepted) => Self::from_committed::<H>(
                law_set,
                pre_state,
                state_domain,
                DecisionKind::Accept,
                None,
                accepted.candidate(),
            ),
            Decision::CommittedFailure(failed) => Self::from_committed::<H>(
                law_set,
                pre_state,
                state_domain,
                DecisionKind::CommittedFailure,
                Some(*failed.reason()),
                failed.candidate(),
            ),
            Decision::Reject(rejected) => {
                let rejected = rejected.reason();
                let receipt = rejected.receipt();
                let bindings = receipt.bindings();
                Ok(Self {
                    profile_hash: law_set.profile_hash,
                    schema_hash: law_set.schema_hash,
                    catalog_hash: law_set.catalog_hash,
                    transition_build_hash: law_set.transition_build_hash,
                    decision_kind: DecisionKind::Reject,
                    reason_id: Some(rejected.reason_id()),
                    command_hash: bindings.command_hash,
                    context_hash: bindings.context_hash,
                    pre_root: receipt.pre_root(),
                    post_root: receipt.post_root(),
                    candidate_id: None,
                    pre_state: pre_state.clone(),
                    post_state: pre_state.clone(),
                    patch: None,
                    commit_plan: CommitPlan::empty(),
                    outbox_plan: OutboxPlan::empty(),
                })
            }
        }
    }

    fn from_committed<H: CommitmentHasher>(
        law_set: &LawSet,
        pre_state: &Value,
        state_domain: Domain<'_>,
        decision_kind: DecisionKind,
        reason_id: Option<SemanticId>,
        artifacts: &zeno_fcis_transition::TransitionArtifacts,
    ) -> Result<Self, CbcError> {
        let bundle = artifacts.bundle();
        let body = bundle.body();
        let bindings = body.bindings();
        let applied = bundle.validate_and_apply::<H>(pre_state, state_domain)?;
        Ok(Self {
            profile_hash: law_set.profile_hash,
            schema_hash: law_set.schema_hash,
            catalog_hash: law_set.catalog_hash,
            transition_build_hash: law_set.transition_build_hash,
            decision_kind,
            reason_id,
            command_hash: bindings.command_hash,
            context_hash: bindings.context_hash,
            pre_root: body.pre_root(),
            post_root: body.post_root(),
            candidate_id: Some(bundle.candidate_id().hash()),
            pre_state: pre_state.clone(),
            post_state: applied.state().clone(),
            patch: Some(bundle.patch().clone()),
            commit_plan: bundle.commit_plan().clone(),
            outbox_plan: bundle.outbox_plan().clone(),
        })
    }

    /// Returns the decision class.
    #[must_use]
    pub const fn decision_kind(&self) -> DecisionKind {
        self.decision_kind
    }

    /// Returns the stable reject/failure reason, when present.
    #[must_use]
    pub const fn reason_id(&self) -> Option<SemanticId> {
        self.reason_id
    }

    /// Returns the admitted pre-state.
    #[must_use]
    pub const fn pre_state(&self) -> &Value {
        &self.pre_state
    }

    /// Returns the authoritative post-state.
    #[must_use]
    pub const fn post_state(&self) -> &Value {
        &self.post_state
    }

    /// Returns the canonical patch for a committed decision.
    #[must_use]
    pub const fn patch(&self) -> Option<&CanonicalPatch> {
        self.patch.as_ref()
    }

    /// Returns the authoritative effect plan.
    #[must_use]
    pub const fn commit_plan(&self) -> &CommitPlan {
        &self.commit_plan
    }

    /// Returns the committed outbox plan.
    #[must_use]
    pub const fn outbox_plan(&self) -> &OutboxPlan {
        &self.outbox_plan
    }

    /// Returns the authenticated command commitment.
    #[must_use]
    pub const fn command_hash(&self) -> Hash32 {
        self.command_hash
    }

    /// Returns the complete authenticated-context commitment.
    #[must_use]
    pub const fn context_hash(&self) -> Hash32 {
        self.context_hash
    }

    /// Returns the pre-state root.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the post-state root.
    #[must_use]
    pub const fn post_root(&self) -> Hash32 {
        self.post_root
    }

    /// Returns the candidate identity for committed decisions.
    #[must_use]
    pub const fn candidate_id(&self) -> Option<Hash32> {
        self.candidate_id
    }

    /// Computes the complete law-subject identity.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CbcError> {
        hash_canonical::<H>("zeno-fcis/cbc-subject", self)
    }
}

impl CanonicalEncode for LawSubject {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CBC-SUBJECT\0");
        output.extend_from_slice(&CBC_FORMAT_VERSION.to_be_bytes());
        for hash in [
            self.profile_hash,
            self.schema_hash,
            self.catalog_hash,
            self.transition_build_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        output.push(decision_tag(self.decision_kind));
        put_optional_semantic_id(output, self.reason_id);
        for hash in [
            self.command_hash,
            self.context_hash,
            self.pre_root,
            self.post_root,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_optional_hash(output, self.candidate_id);
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
        put_blob(output, &self.post_state.canonical_bytes()?)?;
        match &self.patch {
            None => output.push(0),
            Some(patch) => {
                output.push(1);
                put_blob(output, &patch.canonical_bytes()?)?;
            }
        }
        put_blob(output, &self.commit_plan.canonical_bytes()?)?;
        put_blob(output, &self.outbox_plan.canonical_bytes()?)
    }
}

/// Result returned by a project-specific pure executable law checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawCheck {
    /// The relation holds for the exact subject.
    Satisfied,
    /// A concrete counterexample or violation was found.
    Violated {
        /// Content commitment of the retained counterexample.
        counterexample_hash: Hash32,
    },
    /// The checker could not decide the law under its declared bounds.
    Indeterminate {
        /// Content commitment explaining the incomplete result.
        reason_hash: Hash32,
    },
}

/// Project-specific pure executable law checker.
pub trait LawChecker {
    /// Evaluates one exact law definition against the complete transition subject.
    fn check(&self, law: &LawDefinition, subject: &LawSubject) -> LawCheck;
}

/// Independently retained evidence for one law and exact coverage/toolchain.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LawEvidence {
    law_id: SemanticId,
    coverage_hash: Hash32,
    toolchain_hash: Hash32,
    artifact_hash: Hash32,
}

impl LawEvidence {
    /// Creates one non-placeholder evidence binding.
    pub fn try_new(
        law_id: SemanticId,
        coverage_hash: Hash32,
        toolchain_hash: Hash32,
        artifact_hash: Hash32,
    ) -> Result<Self, CbcError> {
        if [coverage_hash, toolchain_hash, artifact_hash].contains(&Hash32::ZERO) {
            return Err(CbcError::ZeroHash);
        }
        Ok(Self {
            law_id,
            coverage_hash,
            toolchain_hash,
            artifact_hash,
        })
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn law_id(self) -> SemanticId {
        self.law_id
    }

    /// Returns the domain/coverage commitment.
    #[must_use]
    pub const fn coverage_hash(self) -> Hash32 {
        self.coverage_hash
    }

    /// Returns the exact verifier/toolchain commitment.
    #[must_use]
    pub const fn toolchain_hash(self) -> Hash32 {
        self.toolchain_hash
    }

    /// Returns the retained artifact commitment.
    #[must_use]
    pub const fn artifact_hash(self) -> Hash32 {
        self.artifact_hash
    }
}

impl CanonicalEncode for LawEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.law_id.encode_to(output)?;
        output.extend_from_slice(self.coverage_hash.as_bytes());
        output.extend_from_slice(self.toolchain_hash.as_bytes());
        output.extend_from_slice(self.artifact_hash.as_bytes());
        Ok(())
    }
}

/// Exact proof statement presented to an independent law-evidence verifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawClaim {
    law_set_hash: Hash32,
    subject_hash: Hash32,
    profile_hash: Hash32,
    schema_hash: Hash32,
    catalog_hash: Hash32,
    transition_build_hash: Hash32,
    law_id: SemanticId,
    law_kind: LawKind,
    law_scope: DecisionScope,
    statement_hash: Hash32,
    decision_kind: DecisionKind,
    pre_root: Hash32,
    post_root: Hash32,
    command_hash: Hash32,
    context_hash: Hash32,
    candidate_id: Option<Hash32>,
    coverage_hash: Hash32,
    toolchain_hash: Hash32,
}

impl LawClaim {
    /// Constructs the exact claim for one law, subject, and evidence context.
    pub fn for_subject<H: CommitmentHasher>(
        law_set: &LawSet,
        law: &LawDefinition,
        subject: &LawSubject,
        evidence: LawEvidence,
    ) -> Result<Self, CbcError> {
        if evidence.law_id != law.id {
            return Err(CbcError::EvidenceLawMismatch);
        }
        Ok(Self {
            law_set_hash: law_set.commitment::<H>()?,
            subject_hash: subject.commitment::<H>()?,
            profile_hash: law_set.profile_hash,
            schema_hash: law_set.schema_hash,
            catalog_hash: law_set.catalog_hash,
            transition_build_hash: law_set.transition_build_hash,
            law_id: law.id,
            law_kind: law.kind,
            law_scope: law.scope,
            statement_hash: law.statement_hash,
            decision_kind: subject.decision_kind,
            pre_root: subject.pre_root,
            post_root: subject.post_root,
            command_hash: subject.command_hash,
            context_hash: subject.context_hash,
            candidate_id: subject.candidate_id,
            coverage_hash: evidence.coverage_hash,
            toolchain_hash: evidence.toolchain_hash,
        })
    }

    /// Computes the exact proof-statement commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CbcError> {
        hash_canonical::<H>("zeno-fcis/cbc-law-claim", self)
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn law_id(&self) -> SemanticId {
        self.law_id
    }
}

impl CanonicalEncode for LawClaim {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CBC-LAW-CLAIM\0");
        output.extend_from_slice(&CBC_FORMAT_VERSION.to_be_bytes());
        for hash in [
            self.law_set_hash,
            self.subject_hash,
            self.profile_hash,
            self.schema_hash,
            self.catalog_hash,
            self.transition_build_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        self.law_id.encode_to(output)?;
        self.law_kind.encode_to(output)?;
        self.law_scope.encode_to(output)?;
        output.extend_from_slice(self.statement_hash.as_bytes());
        output.push(decision_tag(self.decision_kind));
        for hash in [
            self.pre_root,
            self.post_root,
            self.command_hash,
            self.context_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_optional_hash(output, self.candidate_id);
        output.extend_from_slice(self.coverage_hash.as_bytes());
        output.extend_from_slice(self.toolchain_hash.as_bytes());
        Ok(())
    }
}

/// Independent verifier for exact law statements and retained artifacts.
pub trait LawEvidenceVerifier {
    /// Returns true only when the artifact establishes the complete claim under
    /// the exact coverage and toolchain identities contained in the claim.
    fn verify(&self, claim: &LawClaim, evidence: LawEvidence) -> bool;
}

/// One fail-closed law-evaluation blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LawBlocker {
    /// Subject profile/catalog/build bindings differ from the required law set.
    SubjectBindingMismatch,
    /// Two evidence items use the same law identifier.
    DuplicateEvidence {
        /// Duplicated law identifier.
        law_id: SemanticId,
    },
    /// Evidence was supplied for an unknown or inapplicable law.
    UnexpectedEvidence {
        /// Unexpected law identifier.
        law_id: SemanticId,
    },
    /// A required executable check found a violation.
    ExecutableViolation {
        /// Violated law.
        law_id: SemanticId,
        /// Retained counterexample commitment.
        counterexample_hash: Hash32,
    },
    /// A required executable check was incomplete or indeterminate.
    ExecutableIndeterminate {
        /// Undecided law.
        law_id: SemanticId,
        /// Retained explanation commitment.
        reason_hash: Hash32,
    },
    /// A checker returned a zero placeholder commitment.
    InvalidCheckerResult {
        /// Affected law.
        law_id: SemanticId,
    },
    /// Required independent evidence is absent.
    MissingEvidence {
        /// Affected law.
        law_id: SemanticId,
    },
    /// The independent evidence verifier rejected the exact claim.
    EvidenceRejected {
        /// Affected law.
        law_id: SemanticId,
    },
    /// No required law applies to the current decision class.
    NoApplicableLaw,
}

impl CanonicalEncode for LawBlocker {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::SubjectBindingMismatch => output.push(0),
            Self::DuplicateEvidence { law_id } => {
                output.push(1);
                law_id.encode_to(output)?;
            }
            Self::UnexpectedEvidence { law_id } => {
                output.push(2);
                law_id.encode_to(output)?;
            }
            Self::ExecutableViolation {
                law_id,
                counterexample_hash,
            } => {
                output.push(3);
                law_id.encode_to(output)?;
                output.extend_from_slice(counterexample_hash.as_bytes());
            }
            Self::ExecutableIndeterminate {
                law_id,
                reason_hash,
            } => {
                output.push(4);
                law_id.encode_to(output)?;
                output.extend_from_slice(reason_hash.as_bytes());
            }
            Self::InvalidCheckerResult { law_id } => {
                output.push(5);
                law_id.encode_to(output)?;
            }
            Self::MissingEvidence { law_id } => {
                output.push(6);
                law_id.encode_to(output)?;
            }
            Self::EvidenceRejected { law_id } => {
                output.push(7);
                law_id.encode_to(output)?;
            }
            Self::NoApplicableLaw => output.push(8),
        }
        Ok(())
    }
}

/// Complete deterministic result of law evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawEvaluationReport {
    law_set_hash: Hash32,
    subject_hash: Hash32,
    applicable_laws: u32,
    blockers: Box<[LawBlocker]>,
}

impl LawEvaluationReport {
    /// Returns true only when every applicable required check succeeded.
    #[must_use]
    pub fn is_verified(&self) -> bool {
        self.applicable_laws != 0 && self.blockers.is_empty()
    }

    /// Returns the exact law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the exact transition-subject identity.
    #[must_use]
    pub const fn subject_hash(&self) -> Hash32 {
        self.subject_hash
    }

    /// Returns the number of applicable laws evaluated.
    #[must_use]
    pub const fn applicable_laws(&self) -> u32 {
        self.applicable_laws
    }

    /// Returns fail-closed blockers in deterministic evaluation order.
    #[must_use]
    pub const fn blockers(&self) -> &[LawBlocker] {
        &self.blockers
    }

    /// Computes the complete report commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CbcError> {
        hash_canonical::<H>("zeno-fcis/cbc-evaluation-report", self)
    }
}

impl CanonicalEncode for LawEvaluationReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CBC-EVALUATION\0");
        output.extend_from_slice(&CBC_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.subject_hash.as_bytes());
        output.extend_from_slice(&self.applicable_laws.to_be_bytes());
        put_u32_length(output, self.blockers.len())?;
        for blocker in &self.blockers {
            put_blob(output, &blocker.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Evaluates one already reconstructed transition subject.
pub fn evaluate_subject<H, C, V>(
    law_set: &LawSet,
    subject: &LawSubject,
    evidence: &[LawEvidence],
    checker: &C,
    verifier: &V,
) -> Result<LawEvaluationReport, CbcError>
where
    H: CommitmentHasher,
    C: LawChecker,
    V: LawEvidenceVerifier,
{
    if evidence.len() > MAX_CBC_EVIDENCE {
        return Err(CbcError::EvidenceCardinality);
    }
    let law_set_hash = law_set.commitment::<H>()?;
    let subject_hash = subject.commitment::<H>()?;
    let mut blockers = Vec::new();
    if subject.profile_hash != law_set.profile_hash
        || subject.schema_hash != law_set.schema_hash
        || subject.catalog_hash != law_set.catalog_hash
        || subject.transition_build_hash != law_set.transition_build_hash
    {
        push_blocker(&mut blockers, LawBlocker::SubjectBindingMismatch);
    }

    let mut evidence = evidence.to_vec();
    evidence.sort_by_key(|item| item.law_id);
    for pair in evidence.windows(2) {
        if pair[0].law_id == pair[1].law_id {
            push_blocker(
                &mut blockers,
                LawBlocker::DuplicateEvidence {
                    law_id: pair[0].law_id,
                },
            );
        }
    }

    for item in &evidence {
        let known = law_set
            .laws
            .binary_search_by_key(&item.law_id, LawDefinition::id)
            .ok()
            .is_some_and(|index| law_set.laws[index].scope.applies(subject.decision_kind));
        if !known {
            push_blocker(
                &mut blockers,
                LawBlocker::UnexpectedEvidence {
                    law_id: item.law_id,
                },
            );
        }
    }

    let mut applicable = 0_u32;
    for law in law_set
        .laws
        .iter()
        .filter(|law| law.scope.applies(subject.decision_kind))
    {
        applicable = applicable.checked_add(1).ok_or(CbcError::LengthOverflow)?;
        if law.requirement.requires_executable() {
            match checker.check(law, subject) {
                LawCheck::Satisfied => {}
                LawCheck::Violated {
                    counterexample_hash,
                } => {
                    if counterexample_hash == Hash32::ZERO {
                        push_blocker(
                            &mut blockers,
                            LawBlocker::InvalidCheckerResult { law_id: law.id },
                        );
                    } else {
                        push_blocker(
                            &mut blockers,
                            LawBlocker::ExecutableViolation {
                                law_id: law.id,
                                counterexample_hash,
                            },
                        );
                    }
                }
                LawCheck::Indeterminate { reason_hash } => {
                    if reason_hash == Hash32::ZERO {
                        push_blocker(
                            &mut blockers,
                            LawBlocker::InvalidCheckerResult { law_id: law.id },
                        );
                    } else {
                        push_blocker(
                            &mut blockers,
                            LawBlocker::ExecutableIndeterminate {
                                law_id: law.id,
                                reason_hash,
                            },
                        );
                    }
                }
            }
        }
        if law.requirement.requires_evidence() {
            match evidence.binary_search_by_key(&law.id, |item| item.law_id) {
                Err(_) => push_blocker(
                    &mut blockers,
                    LawBlocker::MissingEvidence { law_id: law.id },
                ),
                Ok(index) => {
                    let item = evidence[index];
                    let claim = LawClaim::for_subject::<H>(law_set, law, subject, item)?;
                    if !verifier.verify(&claim, item) {
                        push_blocker(
                            &mut blockers,
                            LawBlocker::EvidenceRejected { law_id: law.id },
                        );
                    }
                }
            }
        }
    }
    if applicable == 0 {
        push_blocker(&mut blockers, LawBlocker::NoApplicableLaw);
    }
    Ok(LawEvaluationReport {
        law_set_hash,
        subject_hash,
        applicable_laws: applicable,
        blockers: blockers.into_boxed_slice(),
    })
}

/// Nominal witness that the complete required law set accepted one transition.
pub struct LawVerifiedTransition {
    decision: TransitionDecision,
    law_set_hash: Hash32,
    subject_hash: Hash32,
    report_hash: Hash32,
}

impl LawVerifiedTransition {
    /// Returns the exact underlying transition decision.
    #[must_use]
    pub const fn decision(&self) -> &TransitionDecision {
        &self.decision
    }

    /// Returns the exact required law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the exact relation-subject identity.
    #[must_use]
    pub const fn subject_hash(&self) -> Hash32 {
        self.subject_hash
    }

    /// Returns the successful evaluation-report identity.
    #[must_use]
    pub const fn report_hash(&self) -> Hash32 {
        self.report_hash
    }

    /// Consumes the nominal witness into the underlying decision.
    #[must_use]
    pub fn into_decision(self) -> TransitionDecision {
        self.decision
    }
}

/// Failed law verification retaining the original transition and exact report.
pub struct LawEvaluationFailure {
    decision: TransitionDecision,
    report: LawEvaluationReport,
}

impl LawEvaluationFailure {
    /// Returns the rejected transition decision.
    #[must_use]
    pub const fn decision(&self) -> &TransitionDecision {
        &self.decision
    }

    /// Returns the complete fail-closed report.
    #[must_use]
    pub const fn report(&self) -> &LawEvaluationReport {
        &self.report
    }

    /// Consumes the failure into its decision and report.
    #[must_use]
    pub fn into_parts(self) -> (TransitionDecision, LawEvaluationReport) {
        (self.decision, self.report)
    }
}

/// Outcome of consuming a transition through complete law verification.
pub enum LawVerificationOutcome {
    /// Every applicable required law succeeded.
    Verified(LawVerifiedTransition),
    /// At least one required law failed or was incomplete.
    Rejected(LawEvaluationFailure),
}

/// Complete borrowed inputs selected by a law-verification authority.
pub struct LawVerificationContext<'a, 'd> {
    law_set: &'a LawSet,
    catalog: &'a ProjectCatalog,
    pre_state: &'a Value,
    state_domain: Domain<'d>,
    evidence: &'a [LawEvidence],
}

impl<'a, 'd> LawVerificationContext<'a, 'd> {
    /// Binds the exact law set, catalog, pre-state, state domain, and evidence.
    #[must_use]
    pub const fn new(
        law_set: &'a LawSet,
        catalog: &'a ProjectCatalog,
        pre_state: &'a Value,
        state_domain: Domain<'d>,
        evidence: &'a [LawEvidence],
    ) -> Self {
        Self {
            law_set,
            catalog,
            pre_state,
            state_domain,
            evidence,
        }
    }
}

/// Validates, reconstructs, evaluates, and nominally seals one transition.
pub fn verify_transition_laws<H, C, V>(
    context: &LawVerificationContext<'_, '_>,
    decision: TransitionDecision,
    checker: &C,
    verifier: &V,
) -> Result<LawVerificationOutcome, CbcError>
where
    H: CommitmentHasher,
    C: LawChecker,
    V: LawEvidenceVerifier,
{
    let subject = LawSubject::from_transition::<H>(
        context.law_set,
        context.catalog,
        context.pre_state,
        context.state_domain,
        &decision,
    )?;
    let report = evaluate_subject::<H, C, V>(
        context.law_set,
        &subject,
        context.evidence,
        checker,
        verifier,
    )?;
    if report.is_verified() {
        let report_hash = report.commitment::<H>()?;
        Ok(LawVerificationOutcome::Verified(LawVerifiedTransition {
            decision,
            law_set_hash: report.law_set_hash,
            subject_hash: report.subject_hash,
            report_hash,
        }))
    } else {
        Ok(LawVerificationOutcome::Rejected(LawEvaluationFailure {
            decision,
            report,
        }))
    }
}

fn push_blocker(blockers: &mut Vec<LawBlocker>, blocker: LawBlocker) {
    if blockers.len() < MAX_CBC_BLOCKERS {
        blockers.push(blocker);
    }
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, CbcError> {
    let bytes = value.canonical_bytes().map_err(CbcError::Encode)?;
    let domain = Domain::new(domain_name, CBC_FORMAT_VERSION).map_err(CbcError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(CbcError::Encode)
}

fn decision_tag(kind: DecisionKind) -> u8 {
    match kind {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn put_optional_semantic_id(output: &mut Vec<u8>, value: Option<SemanticId>) {
    match value {
        None => output.push(0),
        Some(id) => {
            output.push(1);
            output.extend_from_slice(&id.get().to_be_bytes());
        }
    }
}

fn put_optional_hash(output: &mut Vec<u8>, value: Option<Hash32>) {
    match value {
        None => output.push(0),
        Some(hash) => {
            output.push(1);
            output.extend_from_slice(hash.as_bytes());
        }
    }
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

/// Correctness-by-construction model or evaluation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CbcError {
    /// A required commitment was zero.
    ZeroHash,
    /// The required law set is empty or exceeds the hard bound.
    LawCardinality,
    /// Two law definitions share one stable identifier.
    DuplicateLaw,
    /// Law definitions do not exactly reconstruct profile `Claim` entries.
    ClaimRegistryMismatch,
    /// A law identifier is absent from the profile `Claim` registry.
    UnknownClaim(SemanticId),
    /// The law set and supplied catalog/profile/schema differ.
    LawSetBindingMismatch,
    /// Evidence names another law.
    EvidenceLawMismatch,
    /// Evidence count exceeds the hard bound.
    EvidenceCardinality,
    /// A canonical count or arithmetic conversion overflowed.
    LengthOverflow,
    /// Catalog validation or commitment failed.
    Catalog(CatalogError),
    /// Transition validation failed.
    Transition(TransitionError),
    /// Patch application failed.
    Patch(PatchError),
    /// Sealed bundle validation or reconstruction failed.
    Seal(SealError),
    /// Canonical encoding or commitment construction failed.
    Encode(EncodeError),
}

impl From<CatalogError> for CbcError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<TransitionError> for CbcError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

impl From<PatchError> for CbcError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

impl From<SealError> for CbcError {
    fn from(error: SealError) -> Self {
        Self::Seal(error)
    }
}

impl From<EncodeError> for CbcError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for CbcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroHash => formatter.write_str("CBC commitment is zero"),
            Self::LawCardinality => formatter.write_str("CBC law-set cardinality is invalid"),
            Self::DuplicateLaw => formatter.write_str("CBC law identifier is duplicated"),
            Self::ClaimRegistryMismatch => {
                formatter.write_str("CBC law set differs from the profile claim registry")
            }
            Self::UnknownClaim(id) => write!(formatter, "CBC claim {} is unregistered", id.get()),
            Self::LawSetBindingMismatch => {
                formatter.write_str("CBC law-set catalog/profile/schema binding differs")
            }
            Self::EvidenceLawMismatch => formatter.write_str("CBC evidence names another law"),
            Self::EvidenceCardinality => formatter.write_str("CBC evidence count exceeds bound"),
            Self::LengthOverflow => formatter.write_str("CBC length or counter overflowed"),
            Self::Catalog(error) => write!(formatter, "CBC catalog failed: {error}"),
            Self::Transition(error) => write!(formatter, "CBC transition failed: {error}"),
            Self::Patch(error) => write!(formatter, "CBC patch failed: {error}"),
            Self::Seal(error) => write!(formatter, "CBC sealed bundle failed: {error}"),
            Self::Encode(error) => write!(formatter, "CBC encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CbcError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use zeno_fcis_project::ProfileError;

    #[derive(Clone, Copy, Debug)]
    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test-only/1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(*byte)
                    .rotate_left((index % 8) as u32);
            }
            Hash32::new(output)
        }
    }

    struct PassChecker;

    impl LawChecker for PassChecker {
        fn check(&self, _law: &LawDefinition, _subject: &LawSubject) -> LawCheck {
            LawCheck::Satisfied
        }
    }

    struct FailChecker;

    impl LawChecker for FailChecker {
        fn check(&self, law: &LawDefinition, _subject: &LawSubject) -> LawCheck {
            LawCheck::Violated {
                counterexample_hash: Hash32::new([law.id().get() as u8; 32]),
            }
        }
    }

    struct ExactVerifier;

    impl LawEvidenceVerifier for ExactVerifier {
        fn verify(&self, claim: &LawClaim, evidence: LawEvidence) -> bool {
            claim.commitment::<TestHasher>().ok() == Some(evidence.artifact_hash())
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn semantic_id(value: u32) -> SemanticId {
        SemanticId::try_new(value)
            .unwrap_or_else(|error: ProfileError| panic!("semantic id: {error}"))
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value)
            .unwrap_or_else(|error: ProfileError| panic!("stable name: {error}"))
    }

    fn law(requirement: LawRequirement, scope: DecisionScope) -> LawDefinition {
        LawDefinition::try_new(
            semantic_id(1),
            name("value-conservation"),
            LawKind::Conservation,
            scope,
            requirement,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("law: {error}"))
    }

    fn set(definition: LawDefinition) -> LawSet {
        LawSet {
            profile_hash: hash(1),
            schema_hash: hash(2),
            catalog_hash: hash(3),
            policy_hash: hash(4),
            transition_build_hash: hash(5),
            laws: vec![definition].into_boxed_slice(),
        }
    }

    fn subject(value: u128) -> LawSubject {
        LawSubject {
            profile_hash: hash(1),
            schema_hash: hash(2),
            catalog_hash: hash(3),
            transition_build_hash: hash(5),
            decision_kind: DecisionKind::Accept,
            reason_id: None,
            command_hash: hash(6),
            context_hash: hash(7),
            pre_root: hash(8),
            post_root: hash(9),
            candidate_id: Some(hash(11)),
            pre_state: Value::U128(value),
            post_state: Value::U128(value),
            patch: None,
            commit_plan: CommitPlan::empty(),
            outbox_plan: OutboxPlan::empty(),
        }
    }

    fn exact_evidence(
        law_set: &LawSet,
        definition: &LawDefinition,
        subject: &LawSubject,
        coverage: Hash32,
        toolchain: Hash32,
    ) -> LawEvidence {
        let placeholder = LawEvidence::try_new(definition.id(), coverage, toolchain, hash(99))
            .unwrap_or_else(|error| panic!("placeholder evidence: {error}"));
        let claim = LawClaim::for_subject::<TestHasher>(law_set, definition, subject, placeholder)
            .unwrap_or_else(|error| panic!("claim: {error}"));
        let artifact = claim
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("claim commitment: {error}"));
        LawEvidence::try_new(definition.id(), coverage, toolchain, artifact)
            .unwrap_or_else(|error| panic!("evidence: {error}"))
    }

    #[test]
    fn executable_and_evidence_law_produces_verified_report() {
        let definition = law(
            LawRequirement::ExecutableAndEvidence,
            DecisionScope::Committed,
        );
        let laws = set(definition.clone());
        let transition = subject(100);
        let evidence = exact_evidence(&laws, &definition, &transition, hash(30), hash(31));
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &transition,
            &[evidence],
            &PassChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(report.is_verified());
        assert_eq!(report.applicable_laws(), 1);
    }

    #[test]
    fn evidence_is_bound_to_the_complete_subject() {
        let definition = law(LawRequirement::Evidence, DecisionScope::Accept);
        let laws = set(definition.clone());
        let original = subject(100);
        let evidence = exact_evidence(&laws, &definition, &original, hash(30), hash(31));
        let mutated = subject(101);
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &mutated,
            &[evidence],
            &PassChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(matches!(
            report.blockers(),
            [LawBlocker::EvidenceRejected { law_id }] if *law_id == semantic_id(1)
        ));
    }

    #[test]
    fn executable_violation_fails_closed() {
        let definition = law(LawRequirement::Executable, DecisionScope::Accept);
        let laws = set(definition);
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &subject(100),
            &[],
            &FailChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(matches!(
            report.blockers(),
            [LawBlocker::ExecutableViolation { .. }]
        ));
    }

    #[test]
    fn missing_evidence_fails_closed() {
        let definition = law(LawRequirement::Evidence, DecisionScope::Accept);
        let laws = set(definition);
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &subject(100),
            &[],
            &PassChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(matches!(
            report.blockers(),
            [LawBlocker::MissingEvidence { .. }]
        ));
    }

    #[test]
    fn no_law_for_decision_class_fails_closed() {
        let definition = law(LawRequirement::Executable, DecisionScope::Reject);
        let laws = set(definition);
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &subject(100),
            &[],
            &PassChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(matches!(report.blockers(), [LawBlocker::NoApplicableLaw]));
    }

    #[test]
    fn changed_coverage_or_toolchain_invalidates_artifact() {
        let definition = law(LawRequirement::Evidence, DecisionScope::Accept);
        let laws = set(definition.clone());
        let transition = subject(100);
        let original = exact_evidence(&laws, &definition, &transition, hash(30), hash(31));
        let changed = LawEvidence::try_new(
            definition.id(),
            hash(32),
            original.toolchain_hash(),
            original.artifact_hash(),
        )
        .unwrap_or_else(|error| panic!("changed evidence: {error}"));
        let report = evaluate_subject::<TestHasher, _, _>(
            &laws,
            &transition,
            &[changed],
            &PassChecker,
            &ExactVerifier,
        )
        .unwrap_or_else(|error| panic!("evaluation: {error}"));
        assert!(matches!(
            report.blockers(),
            [LawBlocker::EvidenceRejected { .. }]
        ));
    }
}
