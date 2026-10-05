//! Private original native law inputs, bindings, evaluation and executable oracles.
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::marker::PhantomData;
use zeno_fcis_catalog::ProjectCatalog;
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use crate::oracle::core::DecisionKind;
use zeno_fcis_evidence::{CoverageDeclaration, EvidenceEnvelope, SourceBindings};
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, OutboxPlan};
use zeno_fcis_project::{RegistryEntry, RegistryKind, SemanticId, StableName};
use zeno_fcis_spec::ProjectSpec;
use zeno_fcis_value::Value;

pub(crate) use zeno_fcis_laws::{
    AssumptionGap, DecisionScope, GENESIS_LAW_EVALUATION_FORMAT_VERSION, GenesisApplicability,
    LAW_EVALUATION_FORMAT_VERSION, LAW_SET_FORMAT_VERSION, LawDefinition, LawEngineFailure,
    LawError, LawEvidenceRequirement, LawFamilyPolicy, LawField, LawKind, LawLimits, LawManifest,
    LawStatus, ScopeMismatch, StepCase, validate_catalog_law_requirements,
};

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

impl LawProofSubject {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
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

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// External checker decision for one exact proof subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawProofDecision {
    /// The external checker attests that the artifact establishes the subject.
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

/// Pluggable external verifier for retained project-law evidence.
///
/// A public Lean/SMT/Flux adapter or a private ESSO adapter can implement this
/// interface. The legacy law owner selects the concrete verifier and trusts
/// its answer. This callback is not an admission route into V2 Authority.
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

/// Externally attested evidence with library-checked artifact and subject bindings.
/// The external verifier answer is not promoted to a kernel proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestedLawEvidence {
    law_id: SemanticId,
    envelope: EvidenceEnvelope,
    subject_hash: Hash32,
    verifier_identity: Hash32,
    verification_claim: Hash32,
}

impl AttestedLawEvidence {
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

impl AttestedLawEvidence {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.law_id.encode_to(output)?;
        put_blob(output, &self.envelope.canonical_bytes()?)?;
        output.extend_from_slice(self.subject_hash.as_bytes());
        output.extend_from_slice(self.verifier_identity.as_bytes());
        output.extend_from_slice(self.verification_claim.as_bytes());
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Full decision surface presented to relational checkers.
pub enum LawDecisionView<'a> {
    /// An accepted successor with exact evidence and delivery obligations.
    Accept {
        /// Admitted successor state.
        post_state: &'a Value,
        /// Exact state patch.
        patch: &'a CanonicalPatch,
        /// Exact non-executable commit evidence.
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
        /// Exact non-executable commit evidence.
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

impl LawDecisionView<'_> {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
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

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
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

impl LawCheckInput<'_> {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.invocation_id.as_bytes());
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
        put_blob(output, &self.command.canonical_bytes()?)?;
        put_blob(output, &self.context.canonical_bytes()?)?;
        put_blob(output, &self.decision.canonical_bytes()?)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
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

impl GenesisLawCheckInput<'_> {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-GENESIS-LAW-INPUT\0");
        output.extend_from_slice(&GENESIS_LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        output.extend_from_slice(self.genesis_binding_hash.as_bytes());
        put_blob(output, &self.initial_state.canonical_bytes()?)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
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

impl LawObservation {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.law_id.encode_to(output)?;
        output.push(self.status as u8);
        output.extend_from_slice(self.witness_hash.as_bytes());
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
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
    evidence: Box<[AttestedLawEvidence]>,
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
    pub const fn evidence(&self) -> &[AttestedLawEvidence] {
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
        let input_hash = hash_canonical::<H>(
            zeno_fcis_codec::domains::LAW_INPUT,
            (input).canonical_bytes(),
        )?;
        let decision = input.decision.kind();
        let mut expected = self
            .manifest
            .definitions()
            .iter()
            .filter(|definition| definition.scope().covers(decision.into_current()))
            .map(LawDefinition::id)
            .collect::<Vec<_>>();
        let framework = self
            .manifest
            .definitions()
            .iter()
            .filter(|definition| {
                definition.kind() == LawKind::RejectNoAuthority
                    && definition.scope().covers(decision.into_current())
            })
            .map(|definition| {
                LawObservation::try_new(definition.id(), LawStatus::Satisfied, input_hash)
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
        let input_hash = hash_canonical::<H>(
            zeno_fcis_codec::domains::GENESIS_LAW_INPUT,
            (input).canonical_bytes(),
        )?;
        let mut expected = self
            .manifest
            .definitions()
            .iter()
            .filter(|definition| {
                matches!(
                    definition.genesis_applicability(),
                    GenesisApplicability::Required
                )
            })
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
    validate_catalog_law_requirements(catalog, &manifest)?;
    if manifest.definitions().len()
        > usize::try_from(limits.max_definitions).map_err(|_| LawError::ResourceLimit)?
        || evidence.len()
            > usize::try_from(limits.max_evidence).map_err(|_| LawError::ResourceLimit)?
    {
        return Err(LawError::ResourceLimit);
    }
    let evidence_verifier_hash = evidence_verifier.verifier_identity();
    for (hash, field) in [
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
        .definitions()
        .iter()
        .filter(|definition| {
            definition
                .evidence_requirement()
                .requires_retained_evidence()
        })
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
            .definitions()
            .binary_search_by_key(&item.law_id, LawDefinition::id)
            .ok()
            .map(|index| &manifest.definitions()[index])
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
        let subject_hash = hash_canonical::<H>(
            zeno_fcis_codec::domains::LAW_PROOF_SUBJECT,
            subject.canonical_bytes(),
        )?;
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
        checked_evidence.push(AttestedLawEvidence {
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
    let law_set_hash = hash_canonical::<H>(
        zeno_fcis_codec::domains::VERIFIED_LAW_SET,
        binding.canonical_bytes(),
    )?;
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
        let mut bytes = Vec::new();
        encode_law_evaluation(
            law_set_hash,
            input_hash,
            decision,
            &observations,
            &mut bytes,
        )
        .map_err(LawError::Encode)?;
        let evaluation_hash = hash_bytes::<H>(zeno_fcis_codec::domains::LAW_EVALUATION, &bytes)?;
        Ok(Self {
            law_set_hash,
            input_hash,
            decision,
            observations: observations.into_boxed_slice(),
            evaluation_hash,
        })
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

impl LawEvaluation {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_law_evaluation(
            self.law_set_hash,
            self.input_hash,
            self.decision,
            &self.observations,
            output,
        )
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

fn encode_law_evaluation(
    law_set_hash: Hash32,
    input_hash: Hash32,
    decision: DecisionKind,
    observations: &[LawObservation],
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    output.extend_from_slice(b"ZFCIS-LAW-EVALUATION\0");
    output.extend_from_slice(&LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
    output.extend_from_slice(law_set_hash.as_bytes());
    output.extend_from_slice(input_hash.as_bytes());
    output.push(decision_tag(decision));
    put_u32_length(output, observations.len())?;
    for observation in observations {
        observation.encode_to(output)?;
    }
    Ok(())
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
        let mut bytes = Vec::new();
        encode_genesis_law_evaluation(law_set_hash, input_hash, &observations, &mut bytes)
            .map_err(LawError::Encode)?;
        let evaluation_hash =
            hash_bytes::<H>(zeno_fcis_codec::domains::GENESIS_LAW_EVALUATION, &bytes)?;
        Ok(Self {
            law_set_hash,
            input_hash,
            observations: observations.into_boxed_slice(),
            evaluation_hash,
        })
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

impl GenesisLawEvaluation {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_genesis_law_evaluation(
            self.law_set_hash,
            self.input_hash,
            &self.observations,
            output,
        )
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

fn encode_genesis_law_evaluation(
    law_set_hash: Hash32,
    input_hash: Hash32,
    observations: &[LawObservation],
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    output.extend_from_slice(b"ZFCIS-GENESIS-LAW-EVALUATION\0");
    output.extend_from_slice(&GENESIS_LAW_EVALUATION_FORMAT_VERSION.to_be_bytes());
    output.extend_from_slice(law_set_hash.as_bytes());
    output.extend_from_slice(input_hash.as_bytes());
    put_u32_length(output, observations.len())?;
    for observation in observations {
        observation.encode_to(output)?;
    }
    Ok(())
}

struct LawSetBinding<'a> {
    catalog_hash: Hash32,
    manifest_hash: Hash32,
    source_bindings: SourceBindings,
    limits: LawLimits,
    engine_build_hash: Hash32,
    evidence_verifier_hash: Hash32,
    evidence: &'a [AttestedLawEvidence],
}

impl LawSetBinding<'_> {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
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

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

fn validate_law_evidence(
    definition: &LawDefinition,
    envelope: &EvidenceEnvelope,
    bindings: SourceBindings,
) -> Result<(), LawError> {
    if envelope.bindings() != bindings
        || envelope.claim_hash() != definition.claim_hash()
        || envelope.query_id() != definition.name().as_str()
    {
        return Err(LawError::EvidenceBindingMismatch);
    }
    match (definition.evidence_requirement(), envelope.coverage()) {
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
    let query_id_hash = hash_bytes::<H>(
        zeno_fcis_codec::domains::LAW_QUERY_ID,
        envelope.query_id().as_bytes(),
    )?;
    let mut assumptions = Vec::new();
    put_u32_length(&mut assumptions, envelope.assumptions().len()).map_err(LawError::Encode)?;
    for assumption in envelope.assumptions() {
        put_blob(&mut assumptions, assumption.label().as_bytes()).map_err(LawError::Encode)?;
        assumptions.extend_from_slice(assumption.statement_hash().as_bytes());
    }
    let assumptions_hash =
        hash_bytes::<H>(zeno_fcis_codec::domains::LAW_ASSUMPTIONS, &assumptions)?;
    let mut coverage = Vec::new();
    encode_coverage(&mut coverage, envelope.coverage());
    let coverage_hash = hash_bytes::<H>(zeno_fcis_codec::domains::LAW_COVERAGE, &coverage)?;
    Ok(LawProofSubject {
        source_bindings,
        catalog_hash,
        manifest_hash,
        law_id: definition.id(),
        claim_hash: definition.claim_hash(),
        checker_profile_hash: definition.checker_profile_hash(),
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
    domain: Domain<'static>,
    value: Result<Vec<u8>, EncodeError>,
) -> Result<Hash32, LawError> {
    let bytes = value.map_err(LawError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(LawError::Encode)
}

fn hash_bytes<H: CommitmentHasher>(
    domain: Domain<'static>,
    bytes: &[u8],
) -> Result<Hash32, LawError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_catalog::{
        CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition, OperationSemantics,
        ValueFlow, ValueFlowKind,
    };
    use zeno_fcis_evidence::{Assumption, EvidenceResult, ToolIdentity};
    use zeno_fcis_project::{DomainPrefix, ProfileBindings, ProjectProfile};
    use zeno_fcis_refine::EvidenceKind;
    use zeno_fcis_schema::{Schema, SchemaLimits, TypeDef, TypeId, TypeKind};

    use zeno_fcis_codec::RustCryptoSha256 as TestHasher;

    struct OriginalFixtureHasher;

    impl OriginalFixtureHasher {
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

    #[test]
    fn step_assumptions_must_be_enforced_on_the_decisions_they_are_assumed_on() {
        let manifest = manifest();
        // 100 and 101 are checked on every committing decision; 103 only on
        // committed failures; 102 only on rejections.
        assert_eq!(
            manifest.check_step_assumptions(&[id(100), id(101)], &[], &[]),
            Ok(())
        );
        assert_eq!(
            manifest.check_step_assumptions(&[], &[id(100)], &[id(103)]),
            Ok(())
        );
        assert_eq!(
            manifest.check_step_assumptions(&[id(103)], &[id(102)], &[id(999)]),
            Err(vec![
                AssumptionGap::NotEnforced {
                    law: id(103),
                    case: StepCase::EveryCommit,
                    scope: DecisionScope::CommittedFailure,
                },
                AssumptionGap::NotEnforced {
                    law: id(102),
                    case: StepCase::Accepts,
                    scope: DecisionScope::Reject,
                },
                AssumptionGap::Missing {
                    law: id(999),
                    case: StepCase::CommittedFailures,
                },
            ])
        );
        assert!(DecisionScope::Committing.covers(DecisionKind::CommittedFailure.into_current()));
        assert!(!DecisionScope::Accept.covers(DecisionKind::CommittedFailure.into_current()));
    }

    /// A project whose laws 100 to 103 declare the scopes `manifest()`
    /// enforces, plus `extra`.
    fn declared(extra: &str) -> ProjectSpec {
        let source = format!(
            "zeno 1;\nproject 1 scopes;\n\
             type 10 state State;\ntype 11 command Command;\ntype 12 context Context;\n\
             type 13 int Count;\nfield 20 10 count 13;\n\
             reason 30 bad precedence 0;\n\
             component 40 machine {{ owns 10; reads pre.10; writes post.10; budget steps 10; }}\n\
             merge [40];\n{extra}"
        );
        let parsed =
            zeno_fcis_spec::parse_project(&source, zeno_fcis_spec::SourceLimits::default())
                .unwrap_or_else(|set| panic!("{set}"));
        zeno_fcis_spec::elaborate_project(parsed, zeno_fcis_spec::ProjectLimits::default())
            .unwrap_or_else(|set| panic!("{set}"))
    }

    #[test]
    fn a_manifest_must_enforce_the_scopes_its_project_declares() {
        let manifest = manifest();
        let matching = "law 100 state_invariant on commit, genesis = post.10.20 >= 0;\n\
                        law 101 asset_conservation on commit, genesis = post.10.20 >= 0;\n\
                        law 102 reject_no_authority on reject = post.10.20 == pre.10.20;\n\
                        law 103 committed_failure_effects on failure = post.10.20 >= 0;\n";
        assert_eq!(manifest.check_declared_scopes(&declared(matching)), Ok(()));
        // A law without a declared scope is not compared.
        let unscoped = "law 100 state_invariant = post.10.20 >= 0;\n";
        assert_eq!(manifest.check_declared_scopes(&declared(unscoped)), Ok(()));
        let differing = "law 100 state_invariant on accept, genesis = post.10.20 >= 0;\n\
                         law 101 asset_conservation on commit = post.10.20 >= 0;\n\
                         law 103 committed_failure_effects on failure, genesis = post.10.20 >= 0;\n\
                         law 104 undefined on any = post.10.20 >= 0;\n";
        assert_eq!(
            manifest.check_declared_scopes(&declared(differing)),
            Err(vec![
                ScopeMismatch::Scope {
                    law: id(100),
                    declared: DecisionScope::Accept,
                    enforced: DecisionScope::Committing,
                },
                ScopeMismatch::Genesis {
                    law: id(101),
                    declared: false,
                },
                ScopeMismatch::Genesis {
                    law: id(103),
                    declared: true,
                },
                ScopeMismatch::Missing { law: id(104) },
            ])
        );
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
        catalog_with_effects(manifest, Vec::new())
    }

    fn catalog_with_effects(
        manifest: &LawManifest,
        effects: Vec<EffectDefinition>,
    ) -> ProjectCatalog {
        catalog_with_operations(manifest, effects, Vec::new())
    }

    fn catalog_with_operations(
        manifest: &LawManifest,
        effects: Vec<EffectDefinition>,
        channels: Vec<ChannelDefinition>,
    ) -> ProjectCatalog {
        let schema = schema();
        let catalog_manifest =
            CatalogManifest::try_new::<TestHasher>(Vec::new(), effects, channels)
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

    fn complete_manifest(missing: Option<LawKind>, narrowed: Option<LawKind>) -> LawManifest {
        let families = LawKind::ALL
            .iter()
            .copied()
            .map(|kind| {
                if Some(kind) == missing {
                    LawFamilyPolicy::not_applicable(kind, hash(&[kind as u8, 0x55]))
                        .unwrap_or_else(|error| panic!("family: {error}"))
                } else {
                    LawFamilyPolicy::required(kind)
                }
            })
            .collect();
        let definitions = LawKind::ALL
            .iter()
            .copied()
            .filter(|kind| Some(*kind) != missing)
            .map(|kind| {
                let label = match kind {
                    LawKind::StateInvariant => "complete-state",
                    LawKind::AssetConservation => "complete-conservation",
                    LawKind::MintBurnAuthorization => "complete-mint-burn",
                    LawKind::DebitCreditEffectEquality => "complete-debit-credit",
                    LawKind::FeeAndRounding => "complete-fee-rounding",
                    LawKind::AuthoritySubjectRecipient => "complete-authority",
                    LawKind::RejectNoAuthority => "complete-reject",
                    LawKind::CommittedFailureEffects => "complete-failure",
                };
                let scope = match kind {
                    LawKind::StateInvariant => DecisionScope::Committing,
                    LawKind::RejectNoAuthority => DecisionScope::Reject,
                    LawKind::CommittedFailureEffects => DecisionScope::CommittedFailure,
                    _ if Some(kind) == narrowed => DecisionScope::Accept,
                    _ => DecisionScope::Committing,
                };
                law(
                    300 + kind as u32,
                    label,
                    kind,
                    scope,
                    LawEvidenceRequirement::RuntimeOnly,
                )
            })
            .collect();
        LawManifest::try_new(families, definitions)
            .unwrap_or_else(|error| panic!("complete manifest: {error}"))
    }

    fn value_channel(kind: ValueFlowKind) -> ChannelDefinition {
        let flow = ValueFlow::standard(kind, hash(b"asset-domain"))
            .unwrap_or_else(|error| panic!("flow: {error}"));
        ChannelDefinition::try_new(
            id(30),
            name("value-delivery"),
            TypeId::new(1),
            TypeId::new(1),
            OperationSemantics::value(vec![flow], hash(b"classification"))
                .unwrap_or_else(|error| panic!("semantics: {error}")),
            hash(b"delivery-policy"),
        )
        .unwrap_or_else(|error| panic!("channel: {error}"))
    }

    #[test]
    fn value_flows_mechanically_require_their_economic_law_families() {
        let cases = [
            (ValueFlowKind::Transfer, LawKind::AssetConservation),
            (ValueFlowKind::Transfer, LawKind::DebitCreditEffectEquality),
            (ValueFlowKind::Mint, LawKind::MintBurnAuthorization),
            (ValueFlowKind::FeeCharge, LawKind::FeeAndRounding),
            (
                ValueFlowKind::ExternalValueDelivery,
                LawKind::AuthoritySubjectRecipient,
            ),
        ];
        for (flow, missing) in cases {
            let manifest = complete_manifest(Some(missing), None);
            let catalog = catalog_with_operations(&manifest, Vec::new(), vec![value_channel(flow)]);
            assert_eq!(
                validate_catalog_law_requirements(&catalog, &manifest),
                Err(LawError::CatalogRequiredFamilyNotApplicable(missing))
            );
        }
    }

    #[test]
    fn settlement_requires_every_economic_family() {
        for missing in [
            LawKind::AssetConservation,
            LawKind::MintBurnAuthorization,
            LawKind::DebitCreditEffectEquality,
            LawKind::FeeAndRounding,
            LawKind::AuthoritySubjectRecipient,
        ] {
            let manifest = complete_manifest(Some(missing), None);
            let catalog = catalog_with_operations(
                &manifest,
                Vec::new(),
                vec![value_channel(ValueFlowKind::Settlement)],
            );
            assert_eq!(
                validate_catalog_law_requirements(&catalog, &manifest),
                Err(LawError::CatalogRequiredFamilyNotApplicable(missing))
            );
        }
    }

    #[test]
    fn economic_laws_must_cover_accept_and_committed_failure() {
        for narrowed in [
            LawKind::AssetConservation,
            LawKind::MintBurnAuthorization,
            LawKind::DebitCreditEffectEquality,
            LawKind::FeeAndRounding,
            LawKind::AuthoritySubjectRecipient,
        ] {
            let manifest = complete_manifest(None, Some(narrowed));
            let catalog = catalog_with_operations(
                &manifest,
                Vec::new(),
                vec![value_channel(ValueFlowKind::Settlement)],
            );
            assert_eq!(
                validate_catalog_law_requirements(&catalog, &manifest),
                Err(LawError::MissingCommittingCoverage(narrowed))
            );
        }
    }

    #[test]
    fn separate_accept_and_failure_definitions_may_cover_one_family() {
        let base = complete_manifest(None, None);
        let mut definitions = base.definitions().to_vec();
        definitions.retain(|definition| definition.kind() != LawKind::AssetConservation);
        definitions.push(law(
            401,
            "accept-conservation",
            LawKind::AssetConservation,
            DecisionScope::Accept,
            LawEvidenceRequirement::RuntimeOnly,
        ));
        definitions.push(law(
            402,
            "failure-conservation",
            LawKind::AssetConservation,
            DecisionScope::CommittedFailure,
            LawEvidenceRequirement::RuntimeOnly,
        ));
        let manifest = LawManifest::try_new(base.families().to_vec(), definitions)
            .unwrap_or_else(|error| panic!("split manifest: {error}"));
        let catalog = catalog_with_operations(
            &manifest,
            Vec::new(),
            vec![value_channel(ValueFlowKind::Settlement)],
        );
        assert_eq!(
            validate_catalog_law_requirements(&catalog, &manifest),
            Ok(())
        );
    }

    #[test]
    fn custom_value_flow_requires_exact_independently_evidenced_law() {
        let manifest = complete_manifest(None, None);
        let flow = ValueFlow::custom(
            hash(b"asset-domain"),
            id(301),
            hash(b"complete-conservation"),
        )
        .unwrap_or_else(|error| panic!("flow: {error}"));
        let channel = ChannelDefinition::try_new(
            id(30),
            name("custom-value-delivery"),
            TypeId::new(1),
            TypeId::new(1),
            OperationSemantics::value(vec![flow], hash(b"custom-classification"))
                .unwrap_or_else(|error| panic!("semantics: {error}")),
            hash(b"custom-delivery-policy"),
        )
        .unwrap_or_else(|error| panic!("channel: {error}"));
        let catalog = catalog_with_operations(&manifest, Vec::new(), vec![channel]);
        assert_eq!(
            validate_catalog_law_requirements(&catalog, &manifest),
            Err(LawError::CustomValueLawRequiresEvidence(id(301)))
        );
    }

    #[test]
    fn external_value_channel_derives_the_same_nonwaivable_laws() {
        let manifest = complete_manifest(Some(LawKind::DebitCreditEffectEquality), None);
        let flow = ValueFlow::standard(
            ValueFlowKind::ExternalValueDelivery,
            hash(b"channel-asset-domain"),
        )
        .unwrap_or_else(|error| panic!("flow: {error}"));
        let channel = ChannelDefinition::try_new(
            id(30),
            name("value-delivery"),
            TypeId::new(1),
            TypeId::new(1),
            OperationSemantics::value(vec![flow], hash(b"channel-classification"))
                .unwrap_or_else(|error| panic!("semantics: {error}")),
            hash(b"delivery-policy"),
        )
        .unwrap_or_else(|error| panic!("channel: {error}"));
        let catalog = catalog_with_operations(&manifest, Vec::new(), vec![channel]);
        assert_eq!(
            validate_catalog_law_requirements(&catalog, &manifest),
            Err(LawError::CatalogRequiredFamilyNotApplicable(
                LawKind::DebitCreditEffectEquality
            ))
        );
    }

    fn proof_input(catalog: &ProjectCatalog, artifact: &[u8]) -> LawEvidenceInput {
        let source = SourceBindings::try_new(
            catalog.profile_hash(),
            catalog.schema_hash(),
            catalog.profile().bindings().algorithm_hash,
        )
        .unwrap_or_else(|error| panic!("source: {error}"));
        let envelope = EvidenceEnvelope::try_new(
            ToolIdentity::try_new("cvc5", "fixture", hash(b"cvc5-binary"))
                .unwrap_or_else(|error| panic!("tool: {error}")),
            EvidenceKind::Cvc5,
            source,
            "asset-conservation",
            hash(b"asset-conservation"),
            Vec::new(),
            EvidenceResult::Attested,
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

    struct TestLawChecker(EngineMode);

    impl ProjectLawEngine for TestLawChecker {
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

    fn verified(mode: EngineMode) -> VerifiedProjectLaws<TestHasher, TestLawChecker> {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        verify_project_laws::<TestHasher, _, _>(
            &catalog,
            manifest,
            vec![proof_input(&catalog, b"checked-proof")],
            LawLimits::default(),
            hash(b"engine-build"),
            TestLawChecker(mode),
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
        let state = Value::boolean(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let command = Value::boolean(true);
        let context = Value::boolean(false);
        let input = accept_input(
            &catalog,
            &state,
            &command,
            &context,
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
        let state = Value::boolean(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let command = Value::boolean(true);
        let context = Value::boolean(false);
        let input = accept_input(
            &catalog,
            &state,
            &command,
            &context,
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
        let state = Value::boolean(false);
        let patch = CanonicalPatch::try_new(1, hash(b"pre-root"), Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let commit_plan = CommitPlan::empty();
        let outbox_plan = OutboxPlan::empty();
        let command = Value::boolean(true);
        let context = Value::boolean(false);
        let input = accept_input(
            &catalog,
            &state,
            &command,
            &context,
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
        let state = Value::boolean(false);
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
        let state = Value::boolean(false);
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
            vec![input],
            LawLimits::default(),
            hash(b"engine-build"),
            TestLawChecker(EngineMode::Pass),
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
            catalog.profile_hash(),
            catalog.schema_hash(),
            catalog.profile().bindings().algorithm_hash,
        )
        .unwrap_or_else(|error| panic!("source: {error}"));
        let artifact = b"checked-proof";
        let envelope = EvidenceEnvelope::try_new(
            ToolIdentity::try_new("cvc5", "fixture", hash(b"cvc5-binary"))
                .unwrap_or_else(|error| panic!("tool: {error}")),
            EvidenceKind::Cvc5,
            source,
            "asset-conservation",
            hash(b"asset-conservation"),
            vec![
                Assumption::try_new("z-last", hash(b"z"))
                    .unwrap_or_else(|error| panic!("assumption: {error}")),
                Assumption::try_new("a-first", hash(b"a"))
                    .unwrap_or_else(|error| panic!("assumption: {error}")),
            ],
            EvidenceResult::Attested,
            TestHasher::hash(artifact),
            CoverageDeclaration::ProofAssisted {
                theorem_claim: hash(b"asset-conservation"),
            },
        )
        .unwrap_or_else(|error| panic!("envelope: {error}"));
        let result = verify_project_laws::<TestHasher, _, _>(
            &catalog,
            manifest,
            vec![LawEvidenceInput::new(id(101), envelope, artifact.to_vec())],
            LawLimits::default(),
            hash(b"engine-build"),
            TestLawChecker(EngineMode::Pass),
            &AttestingVerifier,
        );
        assert!(matches!(result, Err(LawError::NonCanonicalAssumptions)));
    }

    #[test]
    fn ordinary_reject_needs_no_engine_observation_for_framework_purity_law() {
        let manifest = manifest();
        let catalog = catalog(&manifest);
        let laws = verified(EngineMode::Pass);
        let state = Value::boolean(false);
        let command = Value::boolean(true);
        let context = Value::boolean(false);
        let input = LawCheckInput::try_new(
            catalog
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("catalog hash: {error}")),
            hash(b"reject-invocation"),
            &state,
            &command,
            &context,
            LawDecisionView::Reject { reason_id: 7 },
        )
        .unwrap_or_else(|error| panic!("input: {error}"));
        let evaluation = laws
            .evaluate(&input)
            .unwrap_or_else(|error| panic!("reject evaluation: {error}"));
        assert_eq!(evaluation.observations().len(), 1);
        assert_eq!(evaluation.observations()[0].law_id(), id(102));
    }

    // This native oracle spells out the original domain preimage independently
    // of codec::commitment and the production evaluation encoders.
    fn independent_evaluation_digest(name: &str, payload: &[u8]) -> Hash32 {
        let mut preimage = Vec::from(&b"ZENOFCIS-HASH\0"[..]);
        preimage.extend_from_slice(&1_u16.to_be_bytes());
        let name_len =
            u16::try_from(name.len()).unwrap_or_else(|error| panic!("domain length: {error}"));
        preimage.extend_from_slice(&name_len.to_be_bytes());
        preimage.extend_from_slice(name.as_bytes());
        let payload_len =
            u64::try_from(payload.len()).unwrap_or_else(|error| panic!("payload length: {error}"));
        preimage.extend_from_slice(&payload_len.to_be_bytes());
        preimage.extend_from_slice(payload);
        TestHasher::hash(&preimage)
    }

    #[test]
    fn transition_evaluation_hash_commits_exact_canonical_bytes_without_itself() {
        for (decision, tag) in [
            (DecisionKind::Accept, 0),
            (DecisionKind::Reject, 1),
            (DecisionKind::CommittedFailure, 2),
        ] {
            let observation =
                LawObservation::try_new(id(17), LawStatus::Satisfied, Hash32::new([3; 32]))
                    .unwrap_or_else(|error| panic!("observation: {error}"));
            let evaluation = LawEvaluation::try_new::<TestHasher>(
                Hash32::new([1; 32]),
                Hash32::new([2; 32]),
                decision,
                vec![observation],
            )
            .unwrap_or_else(|error| panic!("evaluation: {error}"));
            let mut expected = Vec::from(&b"ZFCIS-LAW-EVALUATION\0"[..]);
            expected.extend_from_slice(&2_u16.to_be_bytes());
            expected.extend_from_slice(&[1; 32]);
            expected.extend_from_slice(&[2; 32]);
            expected.push(tag);
            expected.extend_from_slice(&1_u32.to_be_bytes());
            expected.extend_from_slice(&17_u32.to_be_bytes());
            expected.push(0);
            expected.extend_from_slice(&[3; 32]);
            assert_eq!(evaluation.canonical_bytes(), Ok(expected.clone()));
            assert_eq!(
                evaluation.evaluation_hash(),
                independent_evaluation_digest("zeno-fcis/law-evaluation", &expected)
            );
            let mut changed_cache = evaluation;
            changed_cache.evaluation_hash = Hash32::ZERO;
            assert_eq!(changed_cache.canonical_bytes(), Ok(expected));
        }
    }

    #[test]
    fn genesis_evaluation_hash_commits_exact_canonical_bytes_without_itself() {
        let observation =
            LawObservation::try_new(id(19), LawStatus::Satisfied, Hash32::new([6; 32]))
                .unwrap_or_else(|error| panic!("observation: {error}"));
        let evaluation = GenesisLawEvaluation::try_new::<TestHasher>(
            Hash32::new([4; 32]),
            Hash32::new([5; 32]),
            vec![observation],
        )
        .unwrap_or_else(|error| panic!("genesis evaluation: {error}"));
        let mut expected = Vec::from(&b"ZFCIS-GENESIS-LAW-EVALUATION\0"[..]);
        expected.extend_from_slice(&2_u16.to_be_bytes());
        expected.extend_from_slice(&[4; 32]);
        expected.extend_from_slice(&[5; 32]);
        expected.extend_from_slice(&1_u32.to_be_bytes());
        expected.extend_from_slice(&19_u32.to_be_bytes());
        expected.push(0);
        expected.extend_from_slice(&[6; 32]);
        assert_eq!(evaluation.canonical_bytes(), Ok(expected.clone()));
        assert_eq!(
            evaluation.evaluation_hash(),
            independent_evaluation_digest("zeno-fcis/genesis-law-evaluation", &expected)
        );
        let mut changed_cache = evaluation;
        changed_cache.evaluation_hash = Hash32::ZERO;
        assert_eq!(changed_cache.canonical_bytes(), Ok(expected));
    }

    #[test]
    fn original_private_fixture_hash_algorithm_is_retained() {
        for bytes in [
            &b""[..],
            &b"checked-proof"[..],
            &[0_u8; 96][..],
            &[255_u8; 97][..],
        ] {
            let mut expected = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % 32;
                expected[slot] = ((u16::from(expected[slot]) * 31
                    + u16::from(byte)
                    + u16::try_from(slot).unwrap())
                    % 256) as u8;
            }
            if expected == [0; 32] {
                expected[0] = 1;
            }
            assert_eq!(OriginalFixtureHasher::hash(bytes), Hash32::new(expected));
        }
    }

    fn independent_blob(output: &mut Vec<u8>, bytes: &[u8]) {
        let length = u32::try_from(bytes.len())
            .unwrap_or_else(|error| panic!("independent blob length: {error}"));
        output.extend_from_slice(&length.to_be_bytes());
        output.extend_from_slice(bytes);
    }

    #[test]
    fn original_law_input_bytes_match_independent_all_decision_frames() {
        let pre = Value::signed(i128::MIN);
        let command = Value::boolean(false);
        let context = Value::unsigned(u128::MAX);
        let post = Value::boolean(true);
        let patch = CanonicalPatch::try_new(42, Hash32::new([3; 32]), Vec::new())
            .unwrap_or_else(|error| panic!("independent patch: {error}"));
        let commit = CommitPlan::empty();
        let outbox = OutboxPlan::empty();
        let mut pre_bytes = vec![4];
        pre_bytes.extend_from_slice(&i128::MIN.to_be_bytes());
        let mut context_bytes = vec![3];
        context_bytes.extend_from_slice(&u128::MAX.to_be_bytes());
        let mut patch_bytes = Vec::from(42_u32.to_be_bytes());
        patch_bytes.extend_from_slice(&[3; 32]);
        patch_bytes.extend_from_slice(&0_u32.to_be_bytes());
        for (decision, tag) in [
            (
                LawDecisionView::Accept {
                    post_state: &post,
                    patch: &patch,
                    commit_plan: &commit,
                    outbox_plan: &outbox,
                },
                0_u8,
            ),
            (
                LawDecisionView::Reject {
                    reason_id: u32::MAX,
                },
                1,
            ),
            (
                LawDecisionView::CommittedFailure {
                    reason_id: u32::MAX,
                    post_state: &post,
                    patch: &patch,
                    commit_plan: &commit,
                    outbox_plan: &outbox,
                },
                2,
            ),
        ] {
            let input = LawCheckInput::try_new(
                Hash32::new([1; 32]),
                Hash32::new([2; 32]),
                &pre,
                &command,
                &context,
                decision,
            )
            .unwrap_or_else(|error| panic!("independent law input: {error}"));
            let mut view_bytes = vec![tag];
            if tag != 0 {
                view_bytes.extend_from_slice(&u32::MAX.to_be_bytes());
            }
            if tag != 1 {
                independent_blob(&mut view_bytes, &[2]);
                independent_blob(&mut view_bytes, &patch_bytes);
                independent_blob(&mut view_bytes, &[0; 4]);
                independent_blob(&mut view_bytes, &[0; 4]);
            }
            let mut expected = Vec::from([1_u8; 32]);
            expected.extend_from_slice(&[2; 32]);
            independent_blob(&mut expected, &pre_bytes);
            independent_blob(&mut expected, &[1]);
            independent_blob(&mut expected, &context_bytes);
            independent_blob(&mut expected, &view_bytes);
            assert_eq!(input.canonical_bytes(), Ok(expected));
            assert_eq!(input.pre_state(), &pre);
            assert_eq!(input.command(), &command);
            assert_eq!(input.context(), &context);
        }
    }

    #[test]
    fn original_genesis_input_bytes_match_independent_extreme_states() {
        for initial in [i128::MIN, -1, 0, 1, i128::MAX] {
            let state = Value::signed(initial);
            let input = GenesisLawCheckInput::try_new(
                Hash32::new([4; 32]),
                Hash32::new([5; 32]),
                Hash32::new([6; 32]),
                &state,
            )
            .unwrap_or_else(|error| panic!("independent genesis input: {error}"));
            let mut expected = Vec::from(&b"ZFCIS-GENESIS-LAW-INPUT\0"[..]);
            expected.extend_from_slice(&2_u16.to_be_bytes());
            expected.extend_from_slice(&[4; 32]);
            expected.extend_from_slice(&[5; 32]);
            expected.extend_from_slice(&[6; 32]);
            let mut state_bytes = vec![4];
            state_bytes.extend_from_slice(&initial.to_be_bytes());
            independent_blob(&mut expected, &state_bytes);
            assert_eq!(input.canonical_bytes(), Ok(expected));
            assert_eq!(input.initial_state(), &state);
        }
    }
}
