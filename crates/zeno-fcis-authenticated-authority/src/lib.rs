//! Nominal candidate-bound authority for authenticated-state publication.
//!
//! This higher dependency ring joins four independently meaningful values:
//! an already catalog-authorized semantic transition, a setup-qualified state
//! projector, an exact reference authenticated-tree snapshot, and a required
//! per-transition projection-relation check. Only their successful conjunction
//! creates [`CatalogAuthorizedAuthenticatedCommit`].

#![forbid(unsafe_code)]

use core::fmt;
use core::marker::PhantomData;

use zeno_fcis_authenticated::{
    AuthDecodeError, AuthError, AuthenticatedDecodeLimits, AuthenticatedProfile,
    AuthenticatedStatePlanner, DecodedAuthenticatedPlan, PlannedAuthenticatedCommit, PlannedState,
    ReferenceSparseTree, StateProjector, TreeReader, TreeWriter, decode_authenticated_plan,
};
use zeno_fcis_authority::{
    AuthorityError, AuthorizationId, CatalogAuthorizedTransition, CatalogTransitionProgram,
    StateDomainBinding,
};
use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::{ApprovedCommitmentProvider, ApprovedProviderId, VerifiedProvider};
use zeno_fcis_laws::ProjectLawEngine;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_receipt::CandidateId;
use zeno_fcis_value::Value;

type CommitPortMarker<H, Program, Laws, Interpreter> =
    PhantomData<fn() -> (H, Program, Laws, Interpreter)>;

/// Canonical authenticated-authority format version.
pub const AUTHENTICATED_AUTHORITY_FORMAT_VERSION: u16 = 1;

/// Explicit setup bound for retained projector-qualification evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectorQualificationLimits {
    /// Maximum evidence artifact bytes admitted during authority setup.
    pub max_evidence_bytes: u64,
}

impl Default for ProjectorQualificationLimits {
    fn default() -> Self {
        Self {
            max_evidence_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Reviewable projector specification, implementation, and evidence claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectorQualificationClaim {
    profile: AuthenticatedProfile,
    specification_hash: Hash32,
    implementation_hash: Hash32,
    evidence_hash: Hash32,
    toolchain_hash: Hash32,
}

impl ProjectorQualificationClaim {
    /// Creates a nonzero claim for one exact authenticated profile.
    pub fn try_new(
        profile: AuthenticatedProfile,
        specification_hash: Hash32,
        implementation_hash: Hash32,
        evidence_hash: Hash32,
        toolchain_hash: Hash32,
    ) -> Result<Self, AuthenticatedAuthorityError> {
        for (field, value) in [
            (
                AuthenticatedAuthorityField::ProjectorSpecification,
                specification_hash,
            ),
            (
                AuthenticatedAuthorityField::ProjectorImplementation,
                implementation_hash,
            ),
            (
                AuthenticatedAuthorityField::ProjectorEvidence,
                evidence_hash,
            ),
            (
                AuthenticatedAuthorityField::ProjectorToolchain,
                toolchain_hash,
            ),
        ] {
            require_nonzero(field, value)?;
        }
        Ok(Self {
            profile,
            specification_hash,
            implementation_hash,
            evidence_hash,
            toolchain_hash,
        })
    }

    /// Returns the exact authenticated profile.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the reviewed projector specification commitment.
    #[must_use]
    pub const fn specification_hash(&self) -> Hash32 {
        self.specification_hash
    }

    /// Returns the concrete projector implementation commitment.
    #[must_use]
    pub const fn implementation_hash(&self) -> Hash32 {
        self.implementation_hash
    }

    /// Returns the retained qualification-evidence commitment.
    #[must_use]
    pub const fn evidence_hash(&self) -> Hash32 {
        self.evidence_hash
    }

    /// Returns the exact qualification toolchain commitment.
    #[must_use]
    pub const fn toolchain_hash(&self) -> Hash32 {
        self.toolchain_hash
    }
}

impl CanonicalEncode for ProjectorQualificationClaim {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECTOR-QUALIFICATION-CLAIM\0");
        output.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
        self.profile.encode_to(output)?;
        output.extend_from_slice(self.specification_hash.as_bytes());
        output.extend_from_slice(self.implementation_hash.as_bytes());
        output.extend_from_slice(self.evidence_hash.as_bytes());
        output.extend_from_slice(self.toolchain_hash.as_bytes());
        Ok(())
    }
}

/// Result returned by the setup-selected independent projector verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectorQualificationDecision {
    /// The exact retained evidence attests the complete qualification claim.
    Attested {
        /// Verifier-specific nonzero attestation commitment.
        verification_claim: Hash32,
    },
    /// Evidence refutes the claim.
    Rejected {
        /// Stable nonzero rejection commitment.
        reason_hash: Hash32,
    },
    /// The verifier could not decide the claim.
    Indeterminate {
        /// Stable nonzero failure commitment.
        reason_hash: Hash32,
    },
}

/// Setup-selected independent checker for one concrete projector implementation.
pub trait ProjectorQualificationVerifier<P> {
    /// Returns the exact verifier implementation and configuration commitment.
    fn verifier_hash(&self) -> Hash32;

    /// Checks the concrete projector against the exact claim and retained bytes.
    fn verify(
        &self,
        projector: &P,
        claim: &ProjectorQualificationClaim,
        evidence: &[u8],
    ) -> ProjectorQualificationDecision;
}

/// Inspectable successful projector qualification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectorQualification {
    id: Hash32,
    claim: ProjectorQualificationClaim,
    verifier_hash: Hash32,
    verification_claim: Hash32,
    provider_id: ApprovedProviderId,
}

impl ProjectorQualification {
    /// Returns the complete qualification identity.
    #[must_use]
    pub const fn id(&self) -> Hash32 {
        self.id
    }

    /// Returns the exact qualification claim.
    #[must_use]
    pub const fn claim(&self) -> &ProjectorQualificationClaim {
        &self.claim
    }

    /// Returns the independent verifier commitment.
    #[must_use]
    pub const fn verifier_hash(&self) -> Hash32 {
        self.verifier_hash
    }

    /// Returns the verifier's exact attestation commitment.
    #[must_use]
    pub const fn verification_claim(&self) -> Hash32 {
        self.verification_claim
    }

    /// Returns the nominal provider used for qualification identities.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }
}

impl CanonicalEncode for ProjectorQualification {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECTOR-QUALIFICATION\0");
        output.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
        put_blob(output, &self.claim.canonical_bytes()?)?;
        output.extend_from_slice(self.verifier_hash.as_bytes());
        output.extend_from_slice(self.verification_claim.as_bytes());
        output.extend_from_slice(&self.provider_id.code().to_be_bytes());
        Ok(())
    }
}

/// Complete relation passed to the setup-selected per-transition projector checker.
pub struct ProjectionRelationSubject<'a> {
    authorization_id: Hash32,
    policy_id: Hash32,
    candidate_id: Hash32,
    bundle_hash: Hash32,
    profile: AuthenticatedProfile,
    qualification_id: Hash32,
    state_domain: &'a StateDomainBinding,
    semantic_pre_state: &'a Value,
    semantic_post_state: &'a Value,
    semantic_patch: &'a CanonicalPatch,
    authenticated_plan: &'a PlannedAuthenticatedCommit,
}

impl<'a> ProjectionRelationSubject<'a> {
    /// Returns the semantic authorization identity.
    #[must_use]
    pub const fn authorization_id(&self) -> Hash32 {
        self.authorization_id
    }

    /// Returns the semantic shell-policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the exact semantic candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> Hash32 {
        self.candidate_id
    }

    /// Returns the complete semantic bundle commitment.
    #[must_use]
    pub const fn bundle_hash(&self) -> Hash32 {
        self.bundle_hash
    }

    /// Returns the exact authenticated profile.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the successful projector-qualification identity.
    #[must_use]
    pub const fn qualification_id(&self) -> Hash32 {
        self.qualification_id
    }

    /// Returns the exact semantic state domain.
    #[must_use]
    pub const fn state_domain(&self) -> &StateDomainBinding {
        self.state_domain
    }

    /// Returns the exact semantic pre-state.
    #[must_use]
    pub const fn semantic_pre_state(&self) -> &Value {
        self.semantic_pre_state
    }

    /// Returns the exact semantic post-state reconstructed from the patch.
    #[must_use]
    pub const fn semantic_post_state(&self) -> &Value {
        self.semantic_post_state
    }

    /// Returns the exact candidate patch.
    #[must_use]
    pub const fn semantic_patch(&self) -> &CanonicalPatch {
        self.semantic_patch
    }

    /// Returns the locally reconstructed authenticated plan.
    #[must_use]
    pub const fn authenticated_plan(&self) -> &PlannedAuthenticatedCommit {
        self.authenticated_plan
    }
}

/// Result of one complete per-transition projection-relation check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionRelationDecision {
    /// The project-specific projection relation is satisfied.
    Satisfied {
        /// Nonzero exact execution or proof witness commitment.
        witness_hash: Hash32,
    },
    /// The relation is violated.
    Violated {
        /// Nonzero normalized counterexample commitment.
        counterexample_hash: Hash32,
    },
    /// The checker could not decide the relation.
    Indeterminate {
        /// Nonzero stable failure commitment.
        reason_hash: Hash32,
    },
}

/// Setup-selected pure checker for project-specific projection completeness laws.
pub trait ProjectionRelationEngine {
    /// Returns the exact reviewed checker implementation commitment.
    fn engine_hash(&self) -> Hash32;

    /// Evaluates the complete semantic/authenticated relation.
    fn evaluate(&self, subject: &ProjectionRelationSubject<'_>) -> ProjectionRelationDecision;
}

/// Inspectable successful per-transition projection evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionRelationEvaluation {
    subject_hash: Hash32,
    engine_hash: Hash32,
    witness_hash: Hash32,
}

impl ProjectionRelationEvaluation {
    /// Returns the complete evaluated relation identity.
    #[must_use]
    pub const fn subject_hash(self) -> Hash32 {
        self.subject_hash
    }

    /// Returns the reviewed checker implementation commitment.
    #[must_use]
    pub const fn engine_hash(self) -> Hash32 {
        self.engine_hash
    }

    /// Returns the successful execution or proof witness commitment.
    #[must_use]
    pub const fn witness_hash(self) -> Hash32 {
        self.witness_hash
    }
}

impl CanonicalEncode for ProjectionRelationEvaluation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECTION-RELATION-EVALUATION\0");
        output.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.subject_hash.as_bytes());
        output.extend_from_slice(self.engine_hash.as_bytes());
        output.extend_from_slice(self.witness_hash.as_bytes());
        Ok(())
    }
}

/// Configured authority that owns one qualified projector and relation checker.
pub struct AuthenticatedCommitAuthority<H, P, R>
where
    H: ApprovedCommitmentProvider,
    P: StateProjector,
    R: ProjectionRelationEngine,
{
    configuration_id: Hash32,
    state_domain: StateDomainBinding,
    planner: AuthenticatedStatePlanner<P>,
    qualification: ProjectorQualification,
    relation_engine: R,
    relation_engine_hash: Hash32,
    provider_id: ApprovedProviderId,
    marker: PhantomData<fn() -> H>,
}

impl<H, P, R> AuthenticatedCommitAuthority<H, P, R>
where
    H: ApprovedCommitmentProvider,
    P: StateProjector,
    R: ProjectionRelationEngine,
{
    /// Qualifies and mounts one projector under authority-owned setup inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new<V: ProjectorQualificationVerifier<P>>(
        profile: AuthenticatedProfile,
        projector: P,
        state_domain: StateDomainBinding,
        claim: ProjectorQualificationClaim,
        evidence: &[u8],
        limits: ProjectorQualificationLimits,
        verifier: &V,
        relation_engine: R,
        provider: &VerifiedProvider<H>,
    ) -> Result<Self, AuthenticatedAuthorityError> {
        if claim.profile() != profile {
            return Err(AuthenticatedAuthorityError::ProfileMismatch);
        }
        if projector.declared_projector_hash() != profile.projector_hash() {
            return Err(AuthenticatedAuthorityError::ProjectorMismatch);
        }
        let evidence_len = u64::try_from(evidence.len())
            .map_err(|_| AuthenticatedAuthorityError::LengthOverflow)?;
        if evidence_len > limits.max_evidence_bytes {
            return Err(AuthenticatedAuthorityError::EvidenceLimit {
                limit: limits.max_evidence_bytes,
                actual: evidence_len,
            });
        }
        let evidence_hash = hash_bytes::<H>("zeno-fcis/projector-evidence", evidence)?;
        if evidence_hash != claim.evidence_hash() {
            return Err(AuthenticatedAuthorityError::EvidenceMismatch);
        }
        let verifier_hash = verifier.verifier_hash();
        require_nonzero(
            AuthenticatedAuthorityField::ProjectorVerifier,
            verifier_hash,
        )?;
        let verification_claim = match verifier.verify(&projector, &claim, evidence) {
            ProjectorQualificationDecision::Attested { verification_claim } => {
                require_nonzero(
                    AuthenticatedAuthorityField::ProjectorVerificationClaim,
                    verification_claim,
                )?;
                verification_claim
            }
            ProjectorQualificationDecision::Rejected { reason_hash } => {
                return Err(AuthenticatedAuthorityError::QualificationRejected(
                    reason_hash,
                ));
            }
            ProjectorQualificationDecision::Indeterminate { reason_hash } => {
                return Err(AuthenticatedAuthorityError::QualificationIndeterminate(
                    reason_hash,
                ));
            }
        };
        let relation_engine_hash = relation_engine.engine_hash();
        require_nonzero(
            AuthenticatedAuthorityField::ProjectionRelationEngine,
            relation_engine_hash,
        )?;
        let provider_id = provider.provider_id();
        let qualification = ProjectorQualification {
            id: Hash32::ZERO,
            claim,
            verifier_hash,
            verification_claim,
            provider_id,
        };
        let qualification_id =
            hash_canonical::<H>("zeno-fcis/projector-qualification", &qualification)?;
        let qualification = ProjectorQualification {
            id: qualification_id,
            ..qualification
        };
        let planner = AuthenticatedStatePlanner::try_new(profile, projector)?;
        let configuration_id = authenticated_configuration_id::<H>(
            &state_domain,
            &qualification,
            relation_engine_hash,
            provider_id,
        )?;
        Ok(Self {
            configuration_id,
            state_domain,
            planner,
            qualification,
            relation_engine,
            relation_engine_hash,
            provider_id,
            marker: PhantomData,
        })
    }

    /// Returns the complete authenticated-authority configuration identity.
    #[must_use]
    pub const fn configuration_id(&self) -> Hash32 {
        self.configuration_id
    }

    /// Returns the exact mounted authenticated profile.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.planner.profile()
    }

    /// Returns the exact qualified projector record.
    #[must_use]
    pub const fn qualification(&self) -> &ProjectorQualification {
        &self.qualification
    }

    /// Returns the exact semantic state-domain binding.
    #[must_use]
    pub const fn state_domain(&self) -> &StateDomainBinding {
        &self.state_domain
    }

    /// Returns the nominal approved provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }

    /// Reconstructs and authorizes an authenticated plan for one exact semantic authorization.
    pub fn authorize<Program, Laws, Interpreter>(
        &self,
        authorized: CatalogAuthorizedTransition<H, Program, Laws, Interpreter>,
        tree: &ReferenceSparseTree,
    ) -> Result<
        CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>,
        AuthenticatedAuthorityError,
    >
    where
        Program: CatalogTransitionProgram<H>,
        Laws: ProjectLawEngine,
    {
        let material = self.authorize_view(&authorized, tree)?;
        Ok(CatalogAuthorizedAuthenticatedCommit {
            authenticated_authorization_id: material.authenticated_authorization_id,
            configuration_id: self.configuration_id,
            semantic: authorized,
            planned: material.planned,
            projection_evaluation: material.projection_evaluation,
        })
    }

    /// Strictly decodes persisted plan bytes and requires exact local reconstruction.
    pub fn reauthorize_canonical_plan<Program, Laws, Interpreter>(
        &self,
        bytes: &[u8],
        limits: AuthenticatedDecodeLimits,
        authorized: CatalogAuthorizedTransition<H, Program, Laws, Interpreter>,
        tree: &ReferenceSparseTree,
    ) -> Result<
        CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>,
        AuthenticatedAuthorityError,
    >
    where
        Program: CatalogTransitionProgram<H>,
        Laws: ProjectLawEngine,
    {
        let decoded = decode_authenticated_plan(bytes, limits)?;
        let material = self.authorize_view(&authorized, tree)?;
        require_exact_decoded_plan(&decoded, material.planned.authenticated(), bytes)?;
        Ok(CatalogAuthorizedAuthenticatedCommit {
            authenticated_authorization_id: material.authenticated_authorization_id,
            configuration_id: self.configuration_id,
            semantic: authorized,
            planned: material.planned,
            projection_evaluation: material.projection_evaluation,
        })
    }

    fn authorize_view<A: SemanticAuthorizationView>(
        &self,
        authorized: &A,
        tree: &ReferenceSparseTree,
    ) -> Result<AuthorizedMaterial, AuthenticatedAuthorityError> {
        if tree.profile() != self.profile() {
            return Err(AuthenticatedAuthorityError::ProfileMismatch);
        }
        let state_domain = self.state_domain.domain()?;
        let planned = self.planner.plan(
            authorized.pre_state(),
            state_domain,
            authorized.patch(),
            tree,
        )?;
        let plan = planned.authenticated();
        if plan.semantic_pre_root() != authorized.pre_root()
            || plan.semantic_post_root() != authorized.post_root()
            || authorized.patch().expected_pre_root() != authorized.pre_root()
        {
            return Err(AuthenticatedAuthorityError::SemanticBindingMismatch);
        }
        let subject = ProjectionRelationSubject {
            authorization_id: authorized.authorization_hash(),
            policy_id: authorized.policy_id(),
            candidate_id: authorized.candidate_hash(),
            bundle_hash: authorized.bundle_hash(),
            profile: self.profile(),
            qualification_id: self.qualification.id(),
            state_domain: &self.state_domain,
            semantic_pre_state: authorized.pre_state(),
            semantic_post_state: planned.semantic_post_state(),
            semantic_patch: authorized.patch(),
            authenticated_plan: plan,
        };
        let engine_hash = self.relation_engine.engine_hash();
        if engine_hash != self.relation_engine_hash {
            return Err(AuthenticatedAuthorityError::RelationEngineMismatch);
        }
        let witness_hash = match self.relation_engine.evaluate(&subject) {
            ProjectionRelationDecision::Satisfied { witness_hash } => {
                require_nonzero(AuthenticatedAuthorityField::ProjectionWitness, witness_hash)?;
                witness_hash
            }
            ProjectionRelationDecision::Violated {
                counterexample_hash,
            } => {
                return Err(AuthenticatedAuthorityError::ProjectionViolated(
                    counterexample_hash,
                ));
            }
            ProjectionRelationDecision::Indeterminate { reason_hash } => {
                return Err(AuthenticatedAuthorityError::ProjectionIndeterminate(
                    reason_hash,
                ));
            }
        };
        let subject_hash = projection_subject_hash::<H>(&subject)?;
        let projection_evaluation = ProjectionRelationEvaluation {
            subject_hash,
            engine_hash,
            witness_hash,
        };
        let authenticated_authorization_id = authenticated_authorization_id::<H>(
            self.configuration_id,
            authorized.authorization_hash(),
            authorized.candidate_hash(),
            plan,
            &projection_evaluation,
        )?;
        Ok(AuthorizedMaterial {
            authenticated_authorization_id,
            planned,
            projection_evaluation,
        })
    }
}

struct AuthorizedMaterial {
    authenticated_authorization_id: Hash32,
    planned: PlannedState,
    projection_evaluation: ProjectionRelationEvaluation,
}

/// Nominal candidate-bound authenticated commit accepted by the production port.
///
/// This value has no public constructor or conversion from a raw authenticated
/// plan. It is created only by [`AuthenticatedCommitAuthority::authorize`] or
/// exact persisted-plan reauthorization.
///
/// ```compile_fail
/// use zeno_fcis_authenticated::PlannedState;
/// use zeno_fcis_authenticated_authority::CatalogAuthorizedAuthenticatedCommit;
/// use zeno_fcis_authority::CatalogTransitionProgram;
/// use zeno_fcis_crypto::ApprovedCommitmentProvider;
/// use zeno_fcis_laws::ProjectLawEngine;
///
/// fn raw_plan_is_not_authority<H, P, L, I>(
///     plan: PlannedState,
/// ) -> CatalogAuthorizedAuthenticatedCommit<H, P, L, I>
/// where
///     H: ApprovedCommitmentProvider,
///     P: CatalogTransitionProgram<H>,
///     L: ProjectLawEngine,
/// {
///     plan.into()
/// }
/// ```
pub struct CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>
where
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    authenticated_authorization_id: Hash32,
    configuration_id: Hash32,
    semantic: CatalogAuthorizedTransition<H, Program, Laws, Interpreter>,
    planned: PlannedState,
    projection_evaluation: ProjectionRelationEvaluation,
}

impl<H, Program, Laws, Interpreter>
    CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>
where
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    /// Returns the candidate-bound authenticated authorization identity.
    #[must_use]
    pub const fn authenticated_authorization_id(&self) -> Hash32 {
        self.authenticated_authorization_id
    }

    /// Returns the authority configuration identity.
    #[must_use]
    pub const fn configuration_id(&self) -> Hash32 {
        self.configuration_id
    }

    /// Returns the underlying exact semantic authorization.
    #[must_use]
    pub const fn semantic(&self) -> &CatalogAuthorizedTransition<H, Program, Laws, Interpreter> {
        &self.semantic
    }

    /// Returns the locally reconstructed semantic and authenticated successor.
    #[must_use]
    pub const fn planned(&self) -> &PlannedState {
        &self.planned
    }

    /// Returns the successful project-specific projection relation evaluation.
    #[must_use]
    pub const fn projection_evaluation(&self) -> ProjectionRelationEvaluation {
        self.projection_evaluation
    }
}

impl<H, Program, Laws, Interpreter> CanonicalEncode
    for CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>
where
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CATALOG-AUTHORIZED-AUTHENTICATED-COMMIT\0");
        output.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.authenticated_authorization_id.as_bytes());
        output.extend_from_slice(self.configuration_id.as_bytes());
        put_blob(output, &self.semantic.canonical_bytes()?)?;
        put_blob(output, &self.planned.authenticated().canonical_bytes()?)?;
        self.projection_evaluation.encode_to(output)
    }
}

/// Production-facing authenticated-tree port accepting only nominal authorization.
pub struct ProductionAuthenticatedCommitPort<T, H, Program, Laws, Interpreter>
where
    T: TreeWriter,
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    tree: T,
    configuration_id: Hash32,
    profile: AuthenticatedProfile,
    marker: CommitPortMarker<H, Program, Laws, Interpreter>,
}

impl<T, H, Program, Laws, Interpreter>
    ProductionAuthenticatedCommitPort<T, H, Program, Laws, Interpreter>
where
    T: TreeWriter,
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    /// Mounts one writer under the exact authenticated-authority configuration.
    pub fn try_new<P, R>(
        tree: T,
        authority: &AuthenticatedCommitAuthority<H, P, R>,
    ) -> Result<Self, AuthenticatedAuthorityError>
    where
        P: StateProjector,
        R: ProjectionRelationEngine,
    {
        if tree.profile() != authority.profile() {
            return Err(AuthenticatedAuthorityError::ProfileMismatch);
        }
        Ok(Self {
            tree,
            configuration_id: authority.configuration_id(),
            profile: authority.profile(),
            marker: PhantomData,
        })
    }

    /// Publishes one exact candidate-bound authenticated plan atomically in the writer.
    pub fn publish(
        &mut self,
        authorized: CatalogAuthorizedAuthenticatedCommit<H, Program, Laws, Interpreter>,
    ) -> Result<AuthenticatedPublication, AuthenticatedAuthorityError> {
        if authorized.configuration_id != self.configuration_id {
            return Err(AuthenticatedAuthorityError::ConfigurationMismatch);
        }
        if authorized.planned.authenticated().profile() != self.profile {
            return Err(AuthenticatedAuthorityError::ProfileMismatch);
        }
        self.tree.apply_plan(authorized.planned.authenticated())?;
        Ok(AuthenticatedPublication {
            authenticated_authorization_id: authorized.authenticated_authorization_id,
            semantic_authorization_id: authorized.semantic.authorization_id(),
            candidate_id: authorized.semantic.body().candidate_id(),
            configuration_id: self.configuration_id,
            profile: self.profile,
            version: self.tree.version(),
            root: self.tree.root(),
        })
    }

    /// Returns the mounted tree snapshot.
    #[must_use]
    pub const fn tree(&self) -> &T {
        &self.tree
    }

    /// Consumes the port and returns its tree.
    #[must_use]
    pub fn into_tree(self) -> T {
        self.tree
    }
}

/// Inspectable receipt for successful authenticated-tree publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedPublication {
    authenticated_authorization_id: Hash32,
    semantic_authorization_id: AuthorizationId,
    candidate_id: CandidateId,
    configuration_id: Hash32,
    profile: AuthenticatedProfile,
    version: u64,
    root: Hash32,
}

impl AuthenticatedPublication {
    /// Returns the candidate-bound authenticated authorization identity.
    #[must_use]
    pub const fn authenticated_authorization_id(self) -> Hash32 {
        self.authenticated_authorization_id
    }

    /// Returns the semantic authorization identity.
    #[must_use]
    pub const fn semantic_authorization_id(self) -> AuthorizationId {
        self.semantic_authorization_id
    }

    /// Returns the exact semantic candidate identity.
    #[must_use]
    pub const fn candidate_id(self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the authenticated-authority configuration identity.
    #[must_use]
    pub const fn configuration_id(self) -> Hash32 {
        self.configuration_id
    }

    /// Returns the exact authenticated profile.
    #[must_use]
    pub const fn profile(self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the committed tree version.
    #[must_use]
    pub const fn version(self) -> u64 {
        self.version
    }

    /// Returns the committed authenticated root.
    #[must_use]
    pub const fn root(self) -> Hash32 {
        self.root
    }
}

trait SemanticAuthorizationView {
    fn authorization_hash(&self) -> Hash32;
    fn policy_id(&self) -> Hash32;
    fn candidate_hash(&self) -> Hash32;
    fn bundle_hash(&self) -> Hash32;
    fn pre_root(&self) -> Hash32;
    fn post_root(&self) -> Hash32;
    fn pre_state(&self) -> &Value;
    fn patch(&self) -> &CanonicalPatch;
}

impl<H, Program, Laws, Interpreter> SemanticAuthorizationView
    for CatalogAuthorizedTransition<H, Program, Laws, Interpreter>
where
    H: ApprovedCommitmentProvider,
    Program: CatalogTransitionProgram<H>,
    Laws: ProjectLawEngine,
{
    fn authorization_hash(&self) -> Hash32 {
        self.authorization_id().hash()
    }

    fn policy_id(&self) -> Hash32 {
        self.body().policy_id()
    }

    fn candidate_hash(&self) -> Hash32 {
        self.body().candidate_id().hash()
    }

    fn bundle_hash(&self) -> Hash32 {
        self.body().bundle_hash()
    }

    fn pre_root(&self) -> Hash32 {
        self.body().pre_root()
    }

    fn post_root(&self) -> Hash32 {
        self.body().post_root()
    }

    fn pre_state(&self) -> &Value {
        self.invocation().pre_state().value().value()
    }

    fn patch(&self) -> &CanonicalPatch {
        self.bundle().patch()
    }
}

fn authenticated_configuration_id<H: ApprovedCommitmentProvider>(
    state_domain: &StateDomainBinding,
    qualification: &ProjectorQualification,
    relation_engine_hash: Hash32,
    provider_id: ApprovedProviderId,
) -> Result<Hash32, AuthenticatedAuthorityError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"ZFCIS-AUTHENTICATED-AUTHORITY-CONFIGURATION\0");
    bytes.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
    put_blob(&mut bytes, &state_domain.canonical_bytes()?)?;
    put_blob(&mut bytes, &qualification.canonical_bytes()?)?;
    bytes.extend_from_slice(relation_engine_hash.as_bytes());
    bytes.extend_from_slice(&provider_id.code().to_be_bytes());
    hash_bytes::<H>("zeno-fcis/authenticated-authority-config", &bytes)
}

fn projection_subject_hash<H: ApprovedCommitmentProvider>(
    subject: &ProjectionRelationSubject<'_>,
) -> Result<Hash32, AuthenticatedAuthorityError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"ZFCIS-PROJECTION-RELATION-SUBJECT\0");
    bytes.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
    bytes.extend_from_slice(subject.authorization_id.as_bytes());
    bytes.extend_from_slice(subject.policy_id.as_bytes());
    bytes.extend_from_slice(subject.candidate_id.as_bytes());
    bytes.extend_from_slice(subject.bundle_hash.as_bytes());
    subject.profile.encode_to(&mut bytes)?;
    bytes.extend_from_slice(subject.qualification_id.as_bytes());
    put_blob(&mut bytes, &subject.state_domain.canonical_bytes()?)?;
    put_blob(&mut bytes, &subject.semantic_pre_state.canonical_bytes()?)?;
    put_blob(&mut bytes, &subject.semantic_post_state.canonical_bytes()?)?;
    put_blob(&mut bytes, &subject.semantic_patch.canonical_bytes()?)?;
    put_blob(&mut bytes, &subject.authenticated_plan.canonical_bytes()?)?;
    hash_bytes::<H>("zeno-fcis/projection-relation-subject", &bytes)
}

fn authenticated_authorization_id<H: ApprovedCommitmentProvider>(
    configuration_id: Hash32,
    semantic_authorization_id: Hash32,
    candidate_id: Hash32,
    plan: &PlannedAuthenticatedCommit,
    evaluation: &ProjectionRelationEvaluation,
) -> Result<Hash32, AuthenticatedAuthorityError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"ZFCIS-AUTHENTICATED-AUTHORIZATION\0");
    bytes.extend_from_slice(&AUTHENTICATED_AUTHORITY_FORMAT_VERSION.to_be_bytes());
    bytes.extend_from_slice(configuration_id.as_bytes());
    bytes.extend_from_slice(semantic_authorization_id.as_bytes());
    bytes.extend_from_slice(candidate_id.as_bytes());
    put_blob(&mut bytes, &plan.canonical_bytes()?)?;
    put_blob(&mut bytes, &evaluation.canonical_bytes()?)?;
    hash_bytes::<H>("zeno-fcis/authenticated-authorization", &bytes)
}

fn require_exact_decoded_plan(
    decoded: &DecodedAuthenticatedPlan,
    planned: &PlannedAuthenticatedCommit,
    original: &[u8],
) -> Result<(), AuthenticatedAuthorityError> {
    if decoded.profile() != planned.profile()
        || decoded.expected_version() != planned.expected_version()
        || decoded.next_version() != planned.next_version()
        || decoded.semantic_pre_root() != planned.semantic_pre_root()
        || decoded.semantic_post_root() != planned.semantic_post_root()
        || decoded.patch_hash() != planned.patch_hash()
        || decoded.authenticated_pre_root() != planned.authenticated_pre_root()
        || decoded.authenticated_post_root() != planned.authenticated_post_root()
        || decoded.node_batch() != planned.node_batch()
        || decoded.stale_nodes() != planned.stale_nodes()
        || planned.canonical_bytes()?.as_slice() != original
    {
        return Err(AuthenticatedAuthorityError::PersistedPlanMismatch);
    }
    Ok(())
}

fn hash_canonical<H: ApprovedCommitmentProvider>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, AuthenticatedAuthorityError> {
    hash_bytes::<H>(domain_name, &value.canonical_bytes()?)
}

fn hash_bytes<H: ApprovedCommitmentProvider>(
    domain_name: &'static str,
    bytes: &[u8],
) -> Result<Hash32, AuthenticatedAuthorityError> {
    let domain = Domain::new(domain_name, AUTHENTICATED_AUTHORITY_FORMAT_VERSION)?;
    commitment::<H>(domain, bytes).map_err(AuthenticatedAuthorityError::Encode)
}

fn require_nonzero(
    field: AuthenticatedAuthorityField,
    value: Hash32,
) -> Result<(), AuthenticatedAuthorityError> {
    if value == Hash32::ZERO {
        Err(AuthenticatedAuthorityError::Zero(field))
    } else {
        Ok(())
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

/// Named authenticated-authority field used by fail-closed diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticatedAuthorityField {
    /// Reviewed projector specification.
    ProjectorSpecification,
    /// Concrete projector implementation.
    ProjectorImplementation,
    /// Retained projector evidence.
    ProjectorEvidence,
    /// Qualification toolchain.
    ProjectorToolchain,
    /// Independent qualification verifier.
    ProjectorVerifier,
    /// Independent verifier attestation.
    ProjectorVerificationClaim,
    /// Per-transition projection relation engine.
    ProjectionRelationEngine,
    /// Per-transition successful projection witness.
    ProjectionWitness,
}

/// Projector qualification, relation, admission, or publication failure.
#[derive(Debug)]
pub enum AuthenticatedAuthorityError {
    /// A required identity is zero.
    Zero(AuthenticatedAuthorityField),
    /// Complete evidence exceeds the setup-owned bound.
    EvidenceLimit {
        /// Configured limit.
        limit: u64,
        /// Actual evidence bytes.
        actual: u64,
    },
    /// Evidence bytes differ from the reviewed claim.
    EvidenceMismatch,
    /// Projector qualification was rejected.
    QualificationRejected(Hash32),
    /// Projector qualification was indeterminate.
    QualificationIndeterminate(Hash32),
    /// The claim, tree, plan, or port uses another authenticated profile.
    ProfileMismatch,
    /// The concrete projector declares another identity.
    ProjectorMismatch,
    /// The mounted relation engine changed its declared identity after setup.
    RelationEngineMismatch,
    /// The locally planned semantic roots differ from the exact authorization.
    SemanticBindingMismatch,
    /// The per-transition projection relation is violated.
    ProjectionViolated(Hash32),
    /// The per-transition projection relation is indeterminate.
    ProjectionIndeterminate(Hash32),
    /// Persisted plan bytes differ from exact local reconstruction.
    PersistedPlanMismatch,
    /// The nominal commit belongs to another authenticated authority.
    ConfigurationMismatch,
    /// A length conversion overflowed.
    LengthOverflow,
    /// Authenticated planning or publication failed.
    Authenticated(AuthError),
    /// Strict authenticated transport decoding failed.
    Decode(AuthDecodeError),
    /// Semantic authority-domain reconstruction failed.
    Authority(AuthorityError),
    /// Canonical encoding or commitment failed.
    Encode(EncodeError),
}

impl From<AuthError> for AuthenticatedAuthorityError {
    fn from(error: AuthError) -> Self {
        Self::Authenticated(error)
    }
}

impl From<AuthDecodeError> for AuthenticatedAuthorityError {
    fn from(error: AuthDecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<AuthorityError> for AuthenticatedAuthorityError {
    fn from(error: AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<EncodeError> for AuthenticatedAuthorityError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for AuthenticatedAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero(field) => {
                write!(formatter, "authenticated authority field {field:?} is zero")
            }
            Self::EvidenceLimit { limit, actual } => write!(
                formatter,
                "projector evidence bytes {actual} exceeds limit {limit}"
            ),
            Self::EvidenceMismatch => formatter.write_str("projector evidence commitment differs"),
            Self::QualificationRejected(reason) => {
                write!(formatter, "projector qualification rejected: {reason}")
            }
            Self::QualificationIndeterminate(reason) => {
                write!(formatter, "projector qualification indeterminate: {reason}")
            }
            Self::ProfileMismatch => formatter.write_str("authenticated profile differs"),
            Self::ProjectorMismatch => formatter.write_str("projector identity differs"),
            Self::RelationEngineMismatch => {
                formatter.write_str("projection relation engine identity changed")
            }
            Self::SemanticBindingMismatch => {
                formatter.write_str("authenticated plan differs from semantic authorization")
            }
            Self::ProjectionViolated(counterexample) => {
                write!(formatter, "projection relation violated: {counterexample}")
            }
            Self::ProjectionIndeterminate(reason) => {
                write!(formatter, "projection relation indeterminate: {reason}")
            }
            Self::PersistedPlanMismatch => {
                formatter.write_str("persisted authenticated plan differs from reconstruction")
            }
            Self::ConfigurationMismatch => {
                formatter.write_str("authenticated authority configuration differs")
            }
            Self::LengthOverflow => formatter.write_str("authenticated authority length overflow"),
            Self::Authenticated(error) => error.fmt(formatter),
            Self::Decode(error) => error.fmt(formatter),
            Self::Authority(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AuthenticatedAuthorityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_authenticated::LeafWrite;
    use zeno_fcis_codec::CommitmentHasher;
    use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
    use zeno_fcis_patch::{PatchOp, ValuePath, hash_value};

    struct RootProjector;

    struct OmittingProjector;

    impl StateProjector for RootProjector {
        fn declared_projector_hash(&self) -> Hash32 {
            hash(6)
        }

        fn project(&self, state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            Ok(vec![(hash(9), state.clone())])
        }
    }

    impl StateProjector for OmittingProjector {
        fn declared_projector_hash(&self) -> Hash32 {
            hash(6)
        }

        fn project(&self, _: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            Ok(Vec::new())
        }
    }

    struct TestVerifier;

    struct RejectingVerifier;

    struct IndeterminateVerifier;

    struct ZeroVerifier;

    struct ZeroAttestationVerifier;

    impl<P> ProjectorQualificationVerifier<P> for TestVerifier {
        fn verifier_hash(&self) -> Hash32 {
            hash(20)
        }

        fn verify(
            &self,
            _: &P,
            _: &ProjectorQualificationClaim,
            _: &[u8],
        ) -> ProjectorQualificationDecision {
            ProjectorQualificationDecision::Attested {
                verification_claim: hash(21),
            }
        }
    }

    impl<P> ProjectorQualificationVerifier<P> for RejectingVerifier {
        fn verifier_hash(&self) -> Hash32 {
            hash(22)
        }

        fn verify(
            &self,
            _: &P,
            _: &ProjectorQualificationClaim,
            _: &[u8],
        ) -> ProjectorQualificationDecision {
            ProjectorQualificationDecision::Rejected {
                reason_hash: hash(23),
            }
        }
    }

    impl<P> ProjectorQualificationVerifier<P> for IndeterminateVerifier {
        fn verifier_hash(&self) -> Hash32 {
            hash(24)
        }

        fn verify(
            &self,
            _: &P,
            _: &ProjectorQualificationClaim,
            _: &[u8],
        ) -> ProjectorQualificationDecision {
            ProjectorQualificationDecision::Indeterminate {
                reason_hash: hash(25),
            }
        }
    }

    impl<P> ProjectorQualificationVerifier<P> for ZeroVerifier {
        fn verifier_hash(&self) -> Hash32 {
            Hash32::ZERO
        }

        fn verify(
            &self,
            _: &P,
            _: &ProjectorQualificationClaim,
            _: &[u8],
        ) -> ProjectorQualificationDecision {
            ProjectorQualificationDecision::Attested {
                verification_claim: hash(26),
            }
        }
    }

    impl<P> ProjectorQualificationVerifier<P> for ZeroAttestationVerifier {
        fn verifier_hash(&self) -> Hash32 {
            hash(27)
        }

        fn verify(
            &self,
            _: &P,
            _: &ProjectorQualificationClaim,
            _: &[u8],
        ) -> ProjectorQualificationDecision {
            ProjectorQualificationDecision::Attested {
                verification_claim: Hash32::ZERO,
            }
        }
    }

    struct RequireOneWrite;

    struct IndeterminateRelation;

    struct ZeroEngine;

    struct ZeroWitnessRelation;

    impl ProjectionRelationEngine for RequireOneWrite {
        fn engine_hash(&self) -> Hash32 {
            hash(30)
        }

        fn evaluate(&self, subject: &ProjectionRelationSubject<'_>) -> ProjectionRelationDecision {
            if subject.authenticated_plan().node_batch().writes().len() == 1
                && matches!(
                    subject.authenticated_plan().node_batch().writes()[0],
                    LeafWrite::Put { .. }
                )
            {
                ProjectionRelationDecision::Satisfied {
                    witness_hash: hash(31),
                }
            } else {
                ProjectionRelationDecision::Violated {
                    counterexample_hash: hash(32),
                }
            }
        }
    }

    impl ProjectionRelationEngine for IndeterminateRelation {
        fn engine_hash(&self) -> Hash32 {
            hash(33)
        }

        fn evaluate(&self, _: &ProjectionRelationSubject<'_>) -> ProjectionRelationDecision {
            ProjectionRelationDecision::Indeterminate {
                reason_hash: hash(34),
            }
        }
    }

    impl ProjectionRelationEngine for ZeroEngine {
        fn engine_hash(&self) -> Hash32 {
            Hash32::ZERO
        }

        fn evaluate(&self, _: &ProjectionRelationSubject<'_>) -> ProjectionRelationDecision {
            ProjectionRelationDecision::Satisfied {
                witness_hash: hash(35),
            }
        }
    }

    impl ProjectionRelationEngine for ZeroWitnessRelation {
        fn engine_hash(&self) -> Hash32 {
            hash(36)
        }

        fn evaluate(&self, _: &ProjectionRelationSubject<'_>) -> ProjectionRelationDecision {
            ProjectionRelationDecision::Satisfied {
                witness_hash: Hash32::ZERO,
            }
        }
    }

    struct TestAuthorization {
        authorization_id: Hash32,
        policy_id: Hash32,
        candidate_id: Hash32,
        bundle_hash: Hash32,
        pre_root: Hash32,
        post_root: Hash32,
        pre_state: Value,
        patch: CanonicalPatch,
    }

    impl SemanticAuthorizationView for TestAuthorization {
        fn authorization_hash(&self) -> Hash32 {
            self.authorization_id
        }

        fn policy_id(&self) -> Hash32 {
            self.policy_id
        }

        fn candidate_hash(&self) -> Hash32 {
            self.candidate_id
        }

        fn bundle_hash(&self) -> Hash32 {
            self.bundle_hash
        }

        fn pre_root(&self) -> Hash32 {
            self.pre_root
        }

        fn post_root(&self) -> Hash32 {
            self.post_root
        }

        fn pre_state(&self) -> &Value {
            &self.pre_state
        }

        fn patch(&self) -> &CanonicalPatch {
            &self.patch
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn profile() -> AuthenticatedProfile {
        AuthenticatedProfile::try_new(hash(1), hash(2), hash(6))
            .unwrap_or_else(|error| panic!("profile: {error}"))
    }

    fn domain_binding() -> StateDomainBinding {
        StateDomainBinding::try_new("authority/authenticated/state", 1)
            .unwrap_or_else(|error| panic!("domain binding: {error}"))
    }

    fn qualification_claim(evidence: &[u8]) -> ProjectorQualificationClaim {
        qualification_claim_for(profile(), evidence)
    }

    fn qualification_claim_for(
        profile: AuthenticatedProfile,
        evidence: &[u8],
    ) -> ProjectorQualificationClaim {
        let evidence_hash =
            hash_bytes::<RustCryptoSha256>("zeno-fcis/projector-evidence", evidence)
                .unwrap_or_else(|error| panic!("evidence hash: {error}"));
        ProjectorQualificationClaim::try_new(profile, hash(10), hash(11), evidence_hash, hash(12))
            .unwrap_or_else(|error| panic!("claim: {error}"))
    }

    fn patch_to(pre: &Value, next: Value) -> CanonicalPatch {
        let binding = domain_binding();
        let domain = binding
            .domain()
            .unwrap_or_else(|error| panic!("domain: {error}"));
        let pre_root = hash_value::<RustCryptoSha256>(domain, pre)
            .unwrap_or_else(|error| panic!("pre root: {error}"));
        let old_hash = hash_value::<RustCryptoSha256>(
            Domain::new("zeno-fcis/value", 1)
                .unwrap_or_else(|error| panic!("value domain: {error}")),
            pre,
        )
        .unwrap_or_else(|error| panic!("old hash: {error}"));
        CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(Vec::new()),
                expected_old_hash: old_hash,
                value: next,
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"))
    }

    fn test_authorization() -> TestAuthorization {
        test_authorization_to(Value::U128(8))
    }

    fn test_authorization_to(next: Value) -> TestAuthorization {
        let pre_state = Value::U128(7);
        let patch = patch_to(&pre_state, next);
        let applied = patch
            .apply::<RustCryptoSha256>(
                &pre_state,
                domain_binding()
                    .domain()
                    .unwrap_or_else(|error| panic!("domain: {error}")),
            )
            .unwrap_or_else(|error| panic!("apply: {error}"));
        TestAuthorization {
            authorization_id: hash(40),
            policy_id: hash(41),
            candidate_id: hash(42),
            bundle_hash: hash(43),
            pre_root: patch.expected_pre_root(),
            post_root: applied.post_root(),
            pre_state,
            patch,
        }
    }

    #[test]
    fn candidate_bound_material_requires_exact_projection_relation() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let authority = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            RequireOneWrite,
            &provider,
        )
        .unwrap_or_else(|error| panic!("authority: {error}"));
        let authorization = test_authorization();
        let tree = ReferenceSparseTree::try_new(
            profile(),
            0,
            vec![(hash(9), authorization.pre_state.clone())],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let material = authority
            .authorize_view(&authorization, &tree)
            .unwrap_or_else(|error| panic!("authorize material: {error}"));
        assert_ne!(material.authenticated_authorization_id, Hash32::ZERO);
        assert_eq!(material.planned.authenticated().expected_version(), 0);
        assert_eq!(material.planned.authenticated().next_version(), 1);
    }

    #[test]
    fn projector_omitting_changed_state_fails_required_relation() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let authority = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            OmittingProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            RequireOneWrite,
            &provider,
        )
        .unwrap_or_else(|error| panic!("authority: {error}"));
        let tree = ReferenceSparseTree::try_new(profile(), 0, Vec::new())
            .unwrap_or_else(|error| panic!("tree: {error}"));
        assert!(matches!(
            authority.authorize_view(&test_authorization(), &tree),
            Err(AuthenticatedAuthorityError::ProjectionViolated(value)) if value == hash(32)
        ));
    }

    #[test]
    fn qualification_binds_exact_evidence_and_limits() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let claim = qualification_claim(evidence);
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                claim.clone(),
                b"other evidence",
                ProjectorQualificationLimits::default(),
                &TestVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::EvidenceMismatch)
        ));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                claim,
                evidence,
                ProjectorQualificationLimits {
                    max_evidence_bytes: 1,
                },
                &TestVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::EvidenceLimit { .. })
        ));
    }

    #[test]
    fn qualification_rejects_substitution_and_non_attestation() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let other_profile = AuthenticatedProfile::try_new(hash(90), hash(91), hash(6))
            .unwrap_or_else(|error| panic!("other profile: {error}"));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim_for(other_profile, evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &TestVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::ProfileMismatch)
        ));
        let wrong_projector_profile = AuthenticatedProfile::try_new(hash(1), hash(2), hash(92))
            .unwrap_or_else(|error| panic!("wrong projector profile: {error}"));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                wrong_projector_profile,
                RootProjector,
                domain_binding(),
                qualification_claim_for(wrong_projector_profile, evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &TestVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::ProjectorMismatch)
        ));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim(evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &RejectingVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::QualificationRejected(value)) if value == hash(23)
        ));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim(evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &IndeterminateVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::QualificationIndeterminate(value)) if value == hash(25)
        ));
    }

    #[test]
    fn qualification_rejects_zero_verifier_attestation_and_engine() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim(evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &ZeroVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::Zero(
                AuthenticatedAuthorityField::ProjectorVerifier
            ))
        ));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim(evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &ZeroAttestationVerifier,
                RequireOneWrite,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::Zero(
                AuthenticatedAuthorityField::ProjectorVerificationClaim
            ))
        ));
        assert!(matches!(
            AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
                profile(),
                RootProjector,
                domain_binding(),
                qualification_claim(evidence),
                evidence,
                ProjectorQualificationLimits::default(),
                &TestVerifier,
                ZeroEngine,
                &provider,
            ),
            Err(AuthenticatedAuthorityError::Zero(
                AuthenticatedAuthorityField::ProjectionRelationEngine
            ))
        ));
    }

    #[test]
    fn projection_relation_rejects_indeterminate_zero_witness_and_semantic_substitution() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let tree = ReferenceSparseTree::try_new(profile(), 0, vec![(hash(9), Value::U128(7))])
            .unwrap_or_else(|error| panic!("tree: {error}"));
        let indeterminate = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            IndeterminateRelation,
            &provider,
        )
        .unwrap_or_else(|error| panic!("indeterminate authority: {error}"));
        assert!(matches!(
            indeterminate.authorize_view(&test_authorization(), &tree),
            Err(AuthenticatedAuthorityError::ProjectionIndeterminate(value)) if value == hash(34)
        ));
        let zero_witness = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            ZeroWitnessRelation,
            &provider,
        )
        .unwrap_or_else(|error| panic!("zero-witness authority: {error}"));
        assert!(matches!(
            zero_witness.authorize_view(&test_authorization(), &tree),
            Err(AuthenticatedAuthorityError::Zero(
                AuthenticatedAuthorityField::ProjectionWitness
            ))
        ));
        let exact = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            RequireOneWrite,
            &provider,
        )
        .unwrap_or_else(|error| panic!("exact authority: {error}"));
        let mut substituted = test_authorization();
        substituted.post_root = hash(99);
        assert!(matches!(
            exact.authorize_view(&substituted, &tree),
            Err(AuthenticatedAuthorityError::SemanticBindingMismatch)
        ));
    }

    #[test]
    fn persisted_plan_requires_strict_decode_and_exact_reconstruction() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let authority = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            RequireOneWrite,
            &provider,
        )
        .unwrap_or_else(|error| panic!("authority: {error}"));
        let authorization = test_authorization();
        let tree = ReferenceSparseTree::try_new(
            profile(),
            0,
            vec![(hash(9), authorization.pre_state.clone())],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let material = authority
            .authorize_view(&authorization, &tree)
            .unwrap_or_else(|error| panic!("material: {error}"));
        let bytes = material
            .planned
            .authenticated()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("plan bytes: {error}"));
        let decoded = decode_authenticated_plan(&bytes, AuthenticatedDecodeLimits::default())
            .unwrap_or_else(|error| panic!("decode: {error}"));
        assert!(
            require_exact_decoded_plan(&decoded, material.planned.authenticated(), &bytes,).is_ok()
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            decode_authenticated_plan(&trailing, AuthenticatedDecodeLimits::default()),
            Err(AuthDecodeError::TrailingBytes { .. })
        ));
        let substituted = test_authorization_to(Value::U128(9));
        let substituted_material = authority
            .authorize_view(&substituted, &tree)
            .unwrap_or_else(|error| panic!("substituted material: {error}"));
        let substituted_bytes = substituted_material
            .planned
            .authenticated()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("substituted bytes: {error}"));
        let substituted_decoded =
            decode_authenticated_plan(&substituted_bytes, AuthenticatedDecodeLimits::default())
                .unwrap_or_else(|error| panic!("substituted decode: {error}"));
        assert!(matches!(
            require_exact_decoded_plan(
                &substituted_decoded,
                material.planned.authenticated(),
                &substituted_bytes,
            ),
            Err(AuthenticatedAuthorityError::PersistedPlanMismatch)
        ));
    }

    #[test]
    fn configuration_and_authorization_identities_bind_setup_and_candidate() {
        let evidence = b"qualified projector evidence";
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        let authority = AuthenticatedCommitAuthority::<RustCryptoSha256, _, _>::try_new(
            profile(),
            RootProjector,
            domain_binding(),
            qualification_claim(evidence),
            evidence,
            ProjectorQualificationLimits::default(),
            &TestVerifier,
            RequireOneWrite,
            &provider,
        )
        .unwrap_or_else(|error| panic!("authority: {error}"));
        let authorization = test_authorization();
        let tree = ReferenceSparseTree::try_new(
            profile(),
            0,
            vec![(hash(9), authorization.pre_state.clone())],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let first = authority
            .authorize_view(&authorization, &tree)
            .unwrap_or_else(|error| panic!("first: {error}"));
        let mut other = test_authorization();
        other.candidate_id = hash(98);
        let second = authority
            .authorize_view(&other, &tree)
            .unwrap_or_else(|error| panic!("second: {error}"));
        assert_ne!(
            first.authenticated_authorization_id,
            second.authenticated_authorization_id
        );
    }

    #[test]
    fn provider_hash_is_sha256() {
        assert_ne!(RustCryptoSha256::hash(b"test"), Hash32::ZERO);
    }
}
