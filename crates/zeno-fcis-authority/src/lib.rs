//! Nominal authorization for catalogued ZenoFCIS production commits.
//!
//! A [`CommitBundle`] proves structural consistency. Production authority also
//! requires an externally admitted invocation, a shell-owned reviewed
//! transition implementation, a sealed known-answer-verified provider, and
//! exact interpreter, deployment, resource, and replay-policy bindings.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use zeno_fcis_catalog::{CatalogError, CatalogManifest, NonZeroHash, ProjectCatalog};
use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::{Accepted, Decision, DecisionKind, Failed, Rejected};
use zeno_fcis_crypto::{ApprovedCommitmentProvider, ApprovedProviderId, VerifiedProvider};
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_project::SemanticId;
use zeno_fcis_receipt::{CandidateId, CommitBundle};
use zeno_fcis_schema::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, TypeId};
use zeno_fcis_shell::{CommitStatus, ShellError, ShellState, apply_reference_bundle};
use zeno_fcis_transition::{
    ExpectedInvocationBindings, TransitionArtifacts, TransitionDecision, TransitionError,
    TransitionLimits, TransitionReject,
};

type AuthorityMarker<H, P, I> = PhantomData<fn() -> (H, P, I)>;

/// Canonical authorization-envelope format version.
pub const AUTHORIZATION_FORMAT_VERSION: u16 = 1;
/// Command and complete invocation-context commitment format version.
pub const INVOCATION_INPUT_FORMAT_VERSION: u16 = 1;

/// Deployment-specific identity of one authorized transition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthorizationId(Hash32);

impl AuthorizationId {
    /// Returns the underlying commitment.
    #[must_use]
    pub const fn hash(self) -> Hash32 {
        self.0
    }
}

impl fmt::Display for AuthorizationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Owned state-domain identity selected by a commit authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateDomainBinding {
    name: Box<str>,
    version: u16,
}

impl StateDomainBinding {
    /// Creates an owned canonical state-domain binding.
    pub fn try_new(name: impl Into<String>, version: u16) -> Result<Self, AuthorityError> {
        let name = name.into();
        Domain::new(&name, version)?;
        Ok(Self {
            name: name.into_boxed_str(),
            version,
        })
    }

    /// Returns the domain name.
    #[must_use]
    pub const fn name(&self) -> &str {
        &self.name
    }

    /// Returns the domain version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the borrowed codec domain.
    pub fn domain(&self) -> Result<Domain<'_>, AuthorityError> {
        Domain::new(&self.name, self.version).map_err(AuthorityError::Encode)
    }
}

impl CanonicalEncode for StateDomainBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u16_blob(output, self.name.as_bytes())?;
        output.extend_from_slice(&self.version.to_be_bytes());
        Ok(())
    }
}

/// Reviewed execution and deployment commitments outside the pure transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionBinding {
    transition_build_hash: NonZeroHash,
    provider_build_evidence_hash: NonZeroHash,
    interpreter_profile_hash: NonZeroHash,
    deployment_profile_hash: NonZeroHash,
    replay_policy_hash: NonZeroHash,
}

impl ExecutionBinding {
    /// Creates an execution binding from five required nonzero commitments.
    pub fn try_new(
        transition_build_hash: Hash32,
        provider_build_evidence_hash: Hash32,
        interpreter_profile_hash: Hash32,
        deployment_profile_hash: Hash32,
        replay_policy_hash: Hash32,
    ) -> Result<Self, AuthorityError> {
        Ok(Self {
            transition_build_hash: NonZeroHash::try_new(transition_build_hash)?,
            provider_build_evidence_hash: NonZeroHash::try_new(provider_build_evidence_hash)?,
            interpreter_profile_hash: NonZeroHash::try_new(interpreter_profile_hash)?,
            deployment_profile_hash: NonZeroHash::try_new(deployment_profile_hash)?,
            replay_policy_hash: NonZeroHash::try_new(replay_policy_hash)?,
        })
    }

    /// Returns the reviewed transition-build commitment.
    #[must_use]
    pub const fn transition_build_hash(self) -> Hash32 {
        self.transition_build_hash.get()
    }

    /// Returns the provider-build evidence commitment.
    #[must_use]
    pub const fn provider_build_evidence_hash(self) -> Hash32 {
        self.provider_build_evidence_hash.get()
    }

    /// Returns the effect-interpreter profile commitment.
    #[must_use]
    pub const fn interpreter_profile_hash(self) -> Hash32 {
        self.interpreter_profile_hash.get()
    }

    /// Returns the deployment profile commitment.
    #[must_use]
    pub const fn deployment_profile_hash(self) -> Hash32 {
        self.deployment_profile_hash.get()
    }

    /// Returns the replay-policy commitment.
    #[must_use]
    pub const fn replay_policy_hash(self) -> Hash32 {
        self.replay_policy_hash.get()
    }
}

impl CanonicalEncode for ExecutionBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.transition_build_hash.encode_to(output)?;
        self.provider_build_evidence_hash.encode_to(output)?;
        self.interpreter_profile_hash.encode_to(output)?;
        self.deployment_profile_hash.encode_to(output)?;
        self.replay_policy_hash.encode_to(output)
    }
}

/// Read-only input supplied to the shell-owned reviewed transition program.
pub struct ReviewedTransitionInput<'a> {
    catalog: &'a ProjectCatalog,
    pre_state: &'a SchemaAdmittedEnvelope,
    command: &'a SchemaAdmittedTypeEnvelope,
    context: &'a SchemaAdmittedTypeEnvelope,
    expected: ExpectedInvocationBindings,
    state_domain: Domain<'a>,
    limits: TransitionLimits,
}

impl<'a> ReviewedTransitionInput<'a> {
    /// Returns the exact approved catalog.
    #[must_use]
    pub const fn catalog(&self) -> &'a ProjectCatalog {
        self.catalog
    }

    /// Returns the exact admitted pre-state.
    #[must_use]
    pub const fn pre_state(&self) -> &'a SchemaAdmittedEnvelope {
        self.pre_state
    }

    /// Returns the exact admitted command.
    #[must_use]
    pub const fn command(&self) -> &'a SchemaAdmittedTypeEnvelope {
        self.command
    }

    /// Returns the exact admitted authenticated context.
    #[must_use]
    pub const fn context(&self) -> &'a SchemaAdmittedTypeEnvelope {
        self.context
    }

    /// Returns the command and complete invocation-context commitments.
    #[must_use]
    pub const fn expected_bindings(&self) -> ExpectedInvocationBindings {
        self.expected
    }

    /// Returns the shell-owned state domain.
    #[must_use]
    pub const fn state_domain(&self) -> Domain<'a> {
        self.state_domain
    }

    /// Returns the shell-owned exact transition limits.
    #[must_use]
    pub const fn limits(&self) -> TransitionLimits {
        self.limits
    }
}

/// Exact transition implementation owned by a production commit authority.
///
/// The concrete program type is carried nominally through policy, invocation,
/// authorization, and shell types. Callers submit admitted inputs; they cannot
/// submit a prebuilt decision to the authorization constructor.
pub trait CatalogTransitionProgram<H: ApprovedCommitmentProvider> {
    /// Program-specific execution failure.
    type Error;

    /// Executes the reviewed transition over exact shell-owned inputs.
    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error>;
}

/// Shell-owned catalog, provider, program, interpreter, deployment, limits, and replay policy.
pub struct AuthorizationPolicy<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    catalog: ProjectCatalog,
    catalog_hash: Hash32,
    state_domain: StateDomainBinding,
    execution: ExecutionBinding,
    transition_limits: TransitionLimits,
    provider_id: ApprovedProviderId,
    policy_id: Hash32,
    marker: AuthorityMarker<H, P, I>,
}

impl<H, P, I> AuthorizationPolicy<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn try_new(
        catalog: &ProjectCatalog,
        state_domain: StateDomainBinding,
        execution: ExecutionBinding,
        transition_limits: TransitionLimits,
        provider: &VerifiedProvider<H>,
    ) -> Result<Self, AuthorityError> {
        let approved_manifest = CatalogManifest::try_new::<H>(
            catalog.manifest().reasons().to_vec(),
            catalog.manifest().effects().to_vec(),
            catalog.manifest().channels().to_vec(),
        )?;
        let approved_catalog = ProjectCatalog::try_new::<H>(
            catalog.profile().clone(),
            catalog.schema().clone(),
            approved_manifest,
            catalog.limits(),
        )?;
        if &approved_catalog != catalog {
            return Err(AuthorityError::Mismatch(AuthorityField::Catalog));
        }
        let catalog_hash = approved_catalog.commitment::<H>()?;
        require_nonzero(catalog_hash, AuthorityField::Catalog)?;
        let mut policy = Self {
            catalog: approved_catalog,
            catalog_hash,
            state_domain,
            execution,
            transition_limits,
            provider_id: provider.provider_id(),
            policy_id: Hash32::ZERO,
            marker: PhantomData,
        };
        policy.policy_id = hash_canonical::<H>("zeno-fcis/authorization-policy", &policy)?;
        require_nonzero(policy.policy_id, AuthorityField::PolicyId)?;
        Ok(policy)
    }

    /// Returns the exact approved catalog.
    #[must_use]
    pub const fn catalog(&self) -> &ProjectCatalog {
        &self.catalog
    }

    /// Returns the complete catalog commitment.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the shell-owned state domain.
    #[must_use]
    pub const fn state_domain(&self) -> &StateDomainBinding {
        &self.state_domain
    }

    /// Returns the external execution and deployment binding.
    #[must_use]
    pub const fn execution(&self) -> ExecutionBinding {
        self.execution
    }

    /// Returns the exact transition resource envelope.
    #[must_use]
    pub const fn transition_limits(&self) -> TransitionLimits {
        self.transition_limits
    }

    /// Returns the sealed provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }

    /// Returns the complete policy identity used to pin a shell.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }
}

impl<H, P, I> CanonicalEncode for AuthorizationPolicy<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZATION-POLICY\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.catalog.profile_hash().as_bytes());
        output.extend_from_slice(self.catalog.schema_hash().as_bytes());
        output.extend_from_slice(self.catalog.manifest().precedence_hash().as_bytes());
        self.state_domain.encode_to(output)?;
        output.extend_from_slice(&self.provider_id.code().to_be_bytes());
        put_u16_blob(output, H::ALGORITHM_ID.as_bytes())?;
        self.execution.encode_to(output)?;
        self.transition_limits.encode_to(output)
    }
}

/// Owns the only transition program allowed to mint one nominal authorization type.
pub struct CatalogCommitAuthority<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    policy: AuthorizationPolicy<H, P, I>,
    program: P,
}

impl<H, P, I> CatalogCommitAuthority<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Pins one exact catalog, program type, interpreter type, and deployment policy.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        catalog: &ProjectCatalog,
        state_domain: StateDomainBinding,
        execution: ExecutionBinding,
        transition_limits: TransitionLimits,
        provider: &VerifiedProvider<H>,
        program: P,
    ) -> Result<Self, AuthorityError> {
        let policy = AuthorizationPolicy::try_new(
            catalog,
            state_domain,
            execution,
            transition_limits,
            provider,
        )?;
        Ok(Self { policy, program })
    }

    /// Returns the exact shell-owned policy.
    #[must_use]
    pub const fn policy(&self) -> &AuthorizationPolicy<H, P, I> {
        &self.policy
    }

    /// Binds one concrete interpreter instance to this exact policy.
    #[must_use]
    pub fn bind_interpreter(&self, interpreter: I) -> BoundInterpreter<H, P, I> {
        BoundInterpreter {
            policy_id: self.policy.policy_id,
            interpreter,
            marker: PhantomData,
        }
    }

    /// Admits one exact externally supplied invocation.
    #[allow(clippy::too_many_arguments)]
    pub fn admit_invocation(
        &self,
        pre_state: SchemaAdmittedEnvelope,
        command: SchemaAdmittedTypeEnvelope,
        context: SchemaAdmittedTypeEnvelope,
        principal_hash: Hash32,
        authentication_evidence_hash: Hash32,
        replay_id: Hash32,
    ) -> Result<InvocationWitness<H, P, I>, AuthorityError> {
        validate_envelope_bindings(&self.policy, &pre_state, &command, &context)?;
        let principal_hash = NonZeroHash::try_new(principal_hash)?;
        let authentication_evidence_hash = NonZeroHash::try_new(authentication_evidence_hash)?;
        let replay_id = NonZeroHash::try_new(replay_id)?;
        let command_hash = command_commitment::<H, P, I>(&self.policy, &command)?;
        let context_hash = context_commitment::<H, P, I>(
            &self.policy,
            &context,
            principal_hash,
            authentication_evidence_hash,
            replay_id,
        )?;
        let expected = ExpectedInvocationBindings::try_new(command_hash, context_hash)?;
        let pre_root = hash_value::<H>(
            self.policy.state_domain.domain()?,
            pre_state.value().value(),
        )?;
        let mut witness = InvocationWitness {
            policy_id: self.policy.policy_id,
            pre_state,
            command,
            context,
            expected,
            pre_root,
            principal_hash,
            authentication_evidence_hash,
            replay_id,
            invocation_id: Hash32::ZERO,
            marker: PhantomData,
        };
        witness.invocation_id =
            hash_canonical::<H>("zeno-fcis/authorization-invocation", &witness)?;
        require_nonzero(witness.invocation_id, AuthorityField::InvocationId)?;
        Ok(witness)
    }

    /// Executes the pinned program and authorizes its complete three-way decision.
    pub fn execute(
        &self,
        invocation: InvocationWitness<H, P, I>,
    ) -> Result<CatalogAuthorizationDecision<H, P, I>, CatalogExecutionError<P::Error>> {
        if invocation.policy_id != self.policy.policy_id {
            return Err(CatalogExecutionError::Authority(AuthorityError::Mismatch(
                AuthorityField::PolicyId,
            )));
        }
        let input = ReviewedTransitionInput {
            catalog: &self.policy.catalog,
            pre_state: &invocation.pre_state,
            command: &invocation.command,
            context: &invocation.context,
            expected: invocation.expected,
            state_domain: self
                .policy
                .state_domain
                .domain()
                .map_err(CatalogExecutionError::Authority)?,
            limits: self.policy.transition_limits,
        };
        let decision = self
            .program
            .execute(input)
            .map_err(CatalogExecutionError::Program)?;
        authorize_decision(&self.policy, invocation, decision)
            .map_err(CatalogExecutionError::Authority)
    }
}

/// Concrete interpreter instance nominally bound to one exact authorization policy.
///
/// Private fields prevent a same-type interpreter from being substituted at a
/// commit port without passing through the owning [`CatalogCommitAuthority`].
pub struct BoundInterpreter<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    policy_id: Hash32,
    interpreter: I,
    marker: AuthorityMarker<H, P, I>,
}

impl<H, P, I> BoundInterpreter<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Returns the exact authorization policy that bound this instance.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Consumes the token for use by a policy-checking interpreter owner.
    #[must_use]
    pub fn into_parts(self) -> (Hash32, I) {
        (self.policy_id, self.interpreter)
    }
}

/// Exact externally admitted pre-state, command, context, principal, and replay.
pub struct InvocationWitness<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    policy_id: Hash32,
    pre_state: SchemaAdmittedEnvelope,
    command: SchemaAdmittedTypeEnvelope,
    context: SchemaAdmittedTypeEnvelope,
    expected: ExpectedInvocationBindings,
    pre_root: Hash32,
    principal_hash: NonZeroHash,
    authentication_evidence_hash: NonZeroHash,
    replay_id: NonZeroHash,
    invocation_id: Hash32,
    marker: AuthorityMarker<H, P, I>,
}

impl<H, P, I> InvocationWitness<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Returns the policy identity that admitted this invocation.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the exact admitted pre-state.
    #[must_use]
    pub const fn pre_state(&self) -> &SchemaAdmittedEnvelope {
        &self.pre_state
    }

    /// Returns the exact admitted command.
    #[must_use]
    pub const fn command(&self) -> &SchemaAdmittedTypeEnvelope {
        &self.command
    }

    /// Returns the exact admitted authenticated context.
    #[must_use]
    pub const fn context(&self) -> &SchemaAdmittedTypeEnvelope {
        &self.context
    }

    /// Returns the externally derived command and invocation-context commitments.
    #[must_use]
    pub const fn expected_bindings(&self) -> ExpectedInvocationBindings {
        self.expected
    }

    /// Returns the exact admitted pre-state root.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the authenticated principal commitment.
    #[must_use]
    pub const fn principal_hash(&self) -> Hash32 {
        self.principal_hash.get()
    }

    /// Returns the ingress authentication-evidence commitment.
    #[must_use]
    pub const fn authentication_evidence_hash(&self) -> Hash32 {
        self.authentication_evidence_hash.get()
    }

    /// Returns the replay identity selected before execution.
    #[must_use]
    pub const fn replay_id(&self) -> Hash32 {
        self.replay_id.get()
    }

    /// Returns the complete invocation identity.
    #[must_use]
    pub const fn invocation_id(&self) -> Hash32 {
        self.invocation_id
    }
}

impl<H, P, I> CanonicalEncode for InvocationWitness<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZATION-INVOCATION\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        output.extend_from_slice(self.pre_root.as_bytes());
        put_blob(output, &self.pre_state.canonical_bytes()?)?;
        put_blob(output, &self.command.canonical_bytes()?)?;
        put_blob(output, &self.context.canonical_bytes()?)?;
        self.expected.encode_to(output)?;
        self.principal_hash.encode_to(output)?;
        self.authentication_evidence_hash.encode_to(output)?;
        self.replay_id.encode_to(output)
    }
}

/// Inspectable fixed-shape body of one deployment-specific commit authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationBody {
    policy_id: Hash32,
    invocation_id: Hash32,
    candidate_id: CandidateId,
    bundle_hash: Hash32,
    decision_kind: DecisionKind,
    reason_id: Option<SemanticId>,
    pre_root: Hash32,
    post_root: Hash32,
}

impl AuthorizationBody {
    /// Returns the pinned shell policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the exact invocation identity.
    #[must_use]
    pub const fn invocation_id(&self) -> Hash32 {
        self.invocation_id
    }

    /// Returns the implementation-neutral semantic candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the exact complete bundle commitment.
    #[must_use]
    pub const fn bundle_hash(&self) -> Hash32 {
        self.bundle_hash
    }

    /// Returns the decision class.
    #[must_use]
    pub const fn decision_kind(&self) -> DecisionKind {
        self.decision_kind
    }

    /// Returns the committed-failure reason, if present.
    #[must_use]
    pub const fn reason_id(&self) -> Option<SemanticId> {
        self.reason_id
    }

    /// Returns the expected pre-state root.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the committing successor root.
    #[must_use]
    pub const fn post_root(&self) -> Hash32 {
        self.post_root
    }
}

impl CanonicalEncode for AuthorizationBody {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZED-TRANSITION-BODY\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        output.extend_from_slice(self.invocation_id.as_bytes());
        output.extend_from_slice(self.candidate_id.hash().as_bytes());
        output.extend_from_slice(self.bundle_hash.as_bytes());
        output.push(decision_tag(self.decision_kind));
        match self.reason_id {
            None => output.push(0),
            Some(reason) => {
                output.push(1);
                reason.encode_to(output)?;
            }
        }
        output.extend_from_slice(self.pre_root.as_bytes());
        output.extend_from_slice(self.post_root.as_bytes());
        Ok(())
    }
}

/// Nominal authorization accepted by production commit ports.
///
/// It has no public constructor, decoder, `Deref`, `Default`, or conversion from
/// [`CommitBundle`] or a caller-supplied [`TransitionDecision`].
///
/// ```compile_fail
/// use zeno_fcis_authority::{
///     CatalogAuthorizedTransition, CatalogTransitionProgram,
/// };
/// use zeno_fcis_crypto::ApprovedCommitmentProvider;
/// use zeno_fcis_receipt::CommitBundle;
///
/// fn raw_bundle_is_not_authority<H, P, I>(
///     bundle: CommitBundle,
/// ) -> CatalogAuthorizedTransition<H, P, I>
/// where
///     H: ApprovedCommitmentProvider,
///     P: CatalogTransitionProgram<H>,
/// {
///     bundle.into()
/// }
/// ```
pub struct CatalogAuthorizedTransition<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    authorization_id: AuthorizationId,
    body: AuthorizationBody,
    invocation: InvocationWitness<H, P, I>,
    artifacts: TransitionArtifacts,
}

impl<H, P, I> CatalogAuthorizedTransition<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Returns the deployment-specific authorization identity.
    #[must_use]
    pub const fn authorization_id(&self) -> AuthorizationId {
        self.authorization_id
    }

    /// Returns the inspectable authorization body.
    #[must_use]
    pub const fn body(&self) -> &AuthorizationBody {
        &self.body
    }

    /// Returns the exact externally admitted invocation.
    #[must_use]
    pub const fn invocation(&self) -> &InvocationWitness<H, P, I> {
        &self.invocation
    }

    /// Returns the replay identity selected before execution.
    #[must_use]
    pub const fn replay_id(&self) -> Hash32 {
        self.invocation.replay_id()
    }

    /// Returns the complete structurally validated bundle.
    #[must_use]
    pub const fn bundle(&self) -> &CommitBundle {
        self.artifacts.bundle()
    }

    /// Returns the complete catalogued transition artifacts.
    #[must_use]
    pub const fn artifacts(&self) -> &TransitionArtifacts {
        &self.artifacts
    }
}

impl<H, P, I> CanonicalEncode for CatalogAuthorizedTransition<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_authorization_envelope(&self.body, &self.invocation, output)
    }
}

impl<H, P, I> fmt::Debug for CatalogAuthorizedTransition<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogAuthorizedTransition")
            .field("authorization_id", &self.authorization_id)
            .field("body", &self.body)
            .finish_non_exhaustive()
    }
}

/// Externally bound ordinary rejection with no candidate or commit authority.
pub struct CatalogAuthorizedReject<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    rejection_id: Hash32,
    policy_id: Hash32,
    invocation: InvocationWitness<H, P, I>,
    rejection: TransitionReject,
}

impl<H, P, I> CatalogAuthorizedReject<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Returns the deployment-specific rejection identity.
    #[must_use]
    pub const fn rejection_id(&self) -> Hash32 {
        self.rejection_id
    }

    /// Returns the pinned shell policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the exact externally admitted invocation.
    #[must_use]
    pub const fn invocation(&self) -> &InvocationWitness<H, P, I> {
        &self.invocation
    }

    /// Returns the unchanged-state rejection evidence.
    #[must_use]
    pub const fn rejection(&self) -> &TransitionReject {
        &self.rejection
    }
}

impl<H, P, I> CanonicalEncode for CatalogAuthorizedReject<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZED-REJECT\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        put_blob(output, &self.invocation.canonical_bytes()?)?;
        self.rejection.reason_id().encode_to(output)?;
        put_blob(output, &self.rejection.receipt().canonical_bytes()?)?;
        put_blob(output, &self.rejection.footprint().canonical_bytes()?)?;
        put_blob(output, &self.rejection.resources().canonical_bytes()?)
    }
}

impl<H, P, I> fmt::Debug for CatalogAuthorizedReject<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CatalogAuthorizedReject")
            .field("rejection_id", &self.rejection_id)
            .field("policy_id", &self.policy_id)
            .finish_non_exhaustive()
    }
}

/// Complete externally bound three-way authorization decision.
pub type CatalogAuthorizationDecision<H, P, I> =
    Decision<CatalogAuthorizedTransition<H, P, I>, CatalogAuthorizedReject<H, P, I>, SemanticId>;

/// Persisted exact production-authorization replay record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationRecord {
    replay_id: Hash32,
    authorization_id: AuthorizationId,
    candidate_id: CandidateId,
    authorization_bytes: Box<[u8]>,
}

impl AuthorizationRecord {
    /// Returns the replay identity.
    #[must_use]
    pub const fn replay_id(&self) -> Hash32 {
        self.replay_id
    }

    /// Returns the deployment-specific authorization identity.
    #[must_use]
    pub const fn authorization_id(&self) -> AuthorizationId {
        self.authorization_id
    }

    /// Returns the implementation-neutral candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the exact canonical authorization bytes.
    #[must_use]
    pub const fn authorization_bytes(&self) -> &[u8] {
        &self.authorization_bytes
    }
}

/// Immutable commit port pinned to one exact provider, program, interpreter, and policy.
pub struct AuthorizedShellState<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    policy_id: Hash32,
    state_domain: StateDomainBinding,
    inner: ShellState,
    records: Box<[AuthorizationRecord]>,
    marker: AuthorityMarker<H, P, I>,
}

impl<H, P, I> AuthorizedShellState<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Creates an empty authorized shell from one exact admitted initial state.
    pub fn new(
        authority: &CatalogCommitAuthority<H, P, I>,
        initial_state: &SchemaAdmittedEnvelope,
    ) -> Result<Self, AuthorizedShellError> {
        validate_root_envelope(authority.policy(), initial_state)?;
        let inner = ShellState::new::<H>(
            initial_state.value().value().clone(),
            authority.policy.state_domain.domain()?,
        )?;
        Ok(Self {
            policy_id: authority.policy.policy_id,
            state_domain: authority.policy.state_domain.clone(),
            inner,
            records: Vec::new().into_boxed_slice(),
            marker: PhantomData,
        })
    }

    /// Returns the pinned policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the structural reference-shell state.
    #[must_use]
    pub const fn reference_state(&self) -> &ShellState {
        &self.inner
    }

    /// Returns canonical replay-to-authorization records.
    #[must_use]
    pub const fn authorization_records(&self) -> &[AuthorizationRecord] {
        &self.records
    }

    /// Consumes one nominal authorization and atomically publishes its exact bundle.
    pub fn commit(
        self,
        authorized: CatalogAuthorizedTransition<H, P, I>,
    ) -> Result<AuthorizedCommitResult<H, P, I>, AuthorizedShellError> {
        if authorized.body.policy_id != self.policy_id {
            return Err(AuthorizedShellError::PolicyMismatch {
                expected: self.policy_id,
                actual: authorized.body.policy_id,
            });
        }
        let authorization_bytes = authorized.canonical_bytes()?;
        let replay_id = authorized.replay_id();
        let authorization_id = authorized.authorization_id;
        let candidate_id = authorized.body.candidate_id;
        if let Ok(index) = self
            .records
            .binary_search_by_key(&replay_id, AuthorizationRecord::replay_id)
        {
            let existing = &self.records[index];
            if existing.authorization_id != authorization_id
                || existing.candidate_id != candidate_id
                || existing.authorization_bytes.as_ref() != authorization_bytes.as_slice()
            {
                return Err(AuthorizedShellError::ReplayConflict { replay_id });
            }
        }
        if self.records.iter().any(|record| {
            record.candidate_id == candidate_id && record.authorization_id != authorization_id
        }) {
            return Err(AuthorizedShellError::CandidateAuthorizationConflict { candidate_id });
        }
        let result = apply_reference_bundle::<H>(
            &self.inner,
            self.state_domain.domain()?,
            replay_id,
            authorized.bundle(),
        )?;
        let status = result.status();
        let mut records = self.records.to_vec();
        if status == CommitStatus::Committed {
            let insertion = records
                .binary_search_by_key(&replay_id, AuthorizationRecord::replay_id)
                .unwrap_or_else(|index| index);
            records.insert(
                insertion,
                AuthorizationRecord {
                    replay_id,
                    authorization_id,
                    candidate_id,
                    authorization_bytes: authorization_bytes.into_boxed_slice(),
                },
            );
        }
        Ok(AuthorizedCommitResult {
            state: Self {
                policy_id: self.policy_id,
                state_domain: self.state_domain,
                inner: result.into_state(),
                records: records.into_boxed_slice(),
                marker: PhantomData,
            },
            status,
        })
    }
}

/// Result of one nominally authorized pure-shell commit.
pub struct AuthorizedCommitResult<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    state: AuthorizedShellState<H, P, I>,
    status: CommitStatus,
}

impl<H, P, I> AuthorizedCommitResult<H, P, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    /// Returns the immutable authorized successor state.
    #[must_use]
    pub const fn state(&self) -> &AuthorizedShellState<H, P, I> {
        &self.state
    }

    /// Returns whether publication committed or replayed idempotently.
    #[must_use]
    pub const fn status(&self) -> CommitStatus {
        self.status
    }

    /// Consumes the result and returns the successor state.
    #[must_use]
    pub fn into_state(self) -> AuthorizedShellState<H, P, I> {
        self.state
    }
}

fn authorize_decision<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    invocation: InvocationWitness<H, P, I>,
    decision: TransitionDecision,
) -> Result<CatalogAuthorizationDecision<H, P, I>, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    match decision {
        Decision::Accept(accepted) => {
            let artifacts = accepted.into_candidate();
            if artifacts.reason_id().is_some() {
                return Err(AuthorityError::Mismatch(AuthorityField::Reason));
            }
            let authorized = authorize_artifacts(policy, invocation, artifacts)?;
            Ok(Decision::Accept(Accepted::new(authorized)))
        }
        Decision::Reject(rejected) => {
            let rejection = rejected.into_reason();
            validate_transition_reject(policy, &invocation, &rejection)?;
            let mut authorized = CatalogAuthorizedReject {
                rejection_id: Hash32::ZERO,
                policy_id: policy.policy_id,
                invocation,
                rejection,
            };
            authorized.rejection_id =
                hash_canonical::<H>("zeno-fcis/authorized-reject", &authorized)?;
            require_nonzero(authorized.rejection_id, AuthorityField::RejectionId)?;
            Ok(Decision::Reject(Rejected::new(authorized)))
        }
        Decision::CommittedFailure(failed) => {
            let (artifacts, reason) = failed.into_parts();
            if artifacts.reason_id() != Some(reason) {
                return Err(AuthorityError::Mismatch(AuthorityField::Reason));
            }
            let authorized = authorize_artifacts(policy, invocation, artifacts)?;
            Ok(Decision::CommittedFailure(Failed::new(authorized, reason)))
        }
    }
}

fn authorize_artifacts<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    invocation: InvocationWitness<H, P, I>,
    artifacts: TransitionArtifacts,
) -> Result<CatalogAuthorizedTransition<H, P, I>, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    if artifacts.resources().limits() != policy.transition_limits {
        return Err(AuthorityError::Mismatch(AuthorityField::TransitionLimits));
    }
    artifacts.validate::<H>(
        policy.catalog(),
        invocation.expected,
        invocation.pre_state.value().value(),
        policy.state_domain.domain()?,
    )?;
    let bundle = artifacts.bundle();
    if bundle.body().pre_root() != invocation.pre_root {
        return Err(AuthorityError::Mismatch(AuthorityField::PreRoot));
    }
    let bundle_hash = hash_canonical::<H>("zeno-fcis/authorized-bundle", bundle)?;
    let body = AuthorizationBody {
        policy_id: policy.policy_id,
        invocation_id: invocation.invocation_id,
        candidate_id: bundle.candidate_id(),
        bundle_hash,
        decision_kind: bundle.body().decision_kind(),
        reason_id: artifacts.reason_id(),
        pre_root: bundle.body().pre_root(),
        post_root: bundle.body().post_root(),
    };
    let mut bytes = Vec::new();
    encode_authorization_envelope(&body, &invocation, &mut bytes)?;
    let domain = Domain::new(
        "zeno-fcis/authorized-transition",
        AUTHORIZATION_FORMAT_VERSION,
    )?;
    let authorization_hash = commitment::<H>(domain, &bytes)?;
    require_nonzero(authorization_hash, AuthorityField::AuthorizationId)?;
    Ok(CatalogAuthorizedTransition {
        authorization_id: AuthorizationId(authorization_hash),
        body,
        invocation,
        artifacts,
    })
}

fn validate_transition_reject<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    invocation: &InvocationWitness<H, P, I>,
    rejection: &TransitionReject,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    if rejection.resources().limits() != policy.transition_limits {
        return Err(AuthorityError::Mismatch(AuthorityField::TransitionLimits));
    }
    rejection.validate::<H>(
        policy.catalog(),
        invocation.expected,
        invocation.pre_state.value().value(),
        policy.state_domain.domain()?,
    )?;
    Ok(())
}

fn validate_envelope_bindings<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    pre_state: &SchemaAdmittedEnvelope,
    command: &SchemaAdmittedTypeEnvelope,
    context: &SchemaAdmittedTypeEnvelope,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    validate_root_envelope(policy, pre_state)?;
    for actual in [command.schema_hash(), context.schema_hash()] {
        if actual != policy.catalog.schema_hash() {
            return Err(AuthorityError::Mismatch(AuthorityField::Schema));
        }
    }
    if command.type_id() != TypeId::new(policy.catalog.profile().command_type().get()) {
        return Err(AuthorityError::Mismatch(AuthorityField::CommandType));
    }
    if context.type_id() != TypeId::new(policy.catalog.profile().context_type().get()) {
        return Err(AuthorityError::Mismatch(AuthorityField::ContextType));
    }
    Ok(())
}

fn validate_root_envelope<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    pre_state: &SchemaAdmittedEnvelope,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    if pre_state.schema_hash() != policy.catalog.schema_hash() {
        return Err(AuthorityError::Mismatch(AuthorityField::Schema));
    }
    if pre_state.root_type() != TypeId::new(policy.catalog.profile().state_type().get()) {
        return Err(AuthorityError::Mismatch(AuthorityField::StateType));
    }
    Ok(())
}

fn command_commitment<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    command: &SchemaAdmittedTypeEnvelope,
) -> Result<Hash32, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    let mut domain_name = policy
        .catalog
        .profile()
        .domain_prefix()
        .as_str()
        .to_string();
    domain_name.push_str("/command");
    let domain = Domain::new(&domain_name, INVOCATION_INPUT_FORMAT_VERSION)?;
    let value = commitment::<H>(domain, &command.canonical_bytes()?)?;
    require_nonzero(value, AuthorityField::Command)?;
    Ok(value)
}

fn context_commitment<H, P, I>(
    policy: &AuthorizationPolicy<H, P, I>,
    context: &SchemaAdmittedTypeEnvelope,
    principal_hash: NonZeroHash,
    authentication_evidence_hash: NonZeroHash,
    replay_id: NonZeroHash,
) -> Result<Hash32, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"ZFCIS-COMPLETE-INVOCATION-CONTEXT\0");
    bytes.extend_from_slice(&INVOCATION_INPUT_FORMAT_VERSION.to_be_bytes());
    bytes.extend_from_slice(policy.policy_id.as_bytes());
    put_blob(&mut bytes, &context.canonical_bytes()?)?;
    principal_hash.encode_to(&mut bytes)?;
    authentication_evidence_hash.encode_to(&mut bytes)?;
    replay_id.encode_to(&mut bytes)?;
    let domain = Domain::new(
        "zeno-fcis/complete-invocation-context",
        INVOCATION_INPUT_FORMAT_VERSION,
    )?;
    let value = commitment::<H>(domain, &bytes)?;
    require_nonzero(value, AuthorityField::Context)?;
    Ok(value)
}

fn encode_authorization_envelope<H, P, I>(
    body: &AuthorizationBody,
    invocation: &InvocationWitness<H, P, I>,
    output: &mut Vec<u8>,
) -> Result<(), EncodeError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
{
    output.extend_from_slice(b"ZFCIS-AUTHORIZED-TRANSITION\0");
    output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
    put_blob(output, &body.canonical_bytes()?)?;
    put_blob(output, &invocation.canonical_bytes()?)
}

fn hash_canonical<H: ApprovedCommitmentProvider>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, AuthorityError> {
    let bytes = value.canonical_bytes()?;
    let domain = Domain::new(domain_name, AUTHORIZATION_FORMAT_VERSION)?;
    commitment::<H>(domain, &bytes).map_err(AuthorityError::Encode)
}

fn require_nonzero(value: Hash32, field: AuthorityField) -> Result<(), AuthorityError> {
    if value == Hash32::ZERO {
        Err(AuthorityError::Zero(field))
    } else {
        Ok(())
    }
}

const fn decision_tag(kind: DecisionKind) -> u8 {
    match kind {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn put_u16_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u16::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

/// Exact field selected by a stable local authority-validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityField {
    /// Rebuilt complete catalog identity.
    Catalog,
    /// Complete policy identity.
    PolicyId,
    /// Complete invocation identity.
    InvocationId,
    /// Deployment-specific authorization identity.
    AuthorizationId,
    /// Deployment-specific rejection identity.
    RejectionId,
    /// Schema commitment.
    Schema,
    /// Root state type.
    StateType,
    /// Command type.
    CommandType,
    /// Authenticated-context type.
    ContextType,
    /// Command commitment.
    Command,
    /// Complete invocation-context commitment.
    Context,
    /// Expected pre-state root.
    PreRoot,
    /// Decision reason.
    Reason,
    /// Shell-owned transition limits.
    TransitionLimits,
}

/// Authorization admission or construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorityError {
    /// A required derived or supplied commitment was zero.
    Zero(AuthorityField),
    /// An exact externally expected field differed.
    Mismatch(AuthorityField),
    /// Project catalog reconstruction or nonzero admission failed.
    Catalog(CatalogError),
    /// Canonical encoding or commitment construction failed.
    Encode(EncodeError),
    /// State-root construction failed.
    Patch(PatchError),
    /// Catalogued transition validation failed.
    Transition(TransitionError),
}

impl From<CatalogError> for AuthorityError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<EncodeError> for AuthorityError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<PatchError> for AuthorityError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

impl From<TransitionError> for AuthorityError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero(field) => write!(formatter, "authorization field {field:?} is zero"),
            Self::Mismatch(field) => write!(formatter, "authorization field {field:?} differs"),
            Self::Catalog(error) => write!(formatter, "authorization catalog failed: {error}"),
            Self::Encode(error) => write!(formatter, "authorization encoding failed: {error}"),
            Self::Patch(error) => write!(formatter, "authorization state root failed: {error}"),
            Self::Transition(error) => {
                write!(formatter, "authorization transition failed: {error}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AuthorityError {}

/// Shell-owned program execution or authority-validation failure.
#[derive(Debug)]
pub enum CatalogExecutionError<E> {
    /// The pinned transition implementation failed before producing a decision.
    Program(E),
    /// Invocation or produced artifacts failed the authority boundary.
    Authority(AuthorityError),
}

impl<E: fmt::Display> fmt::Display for CatalogExecutionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Program(error) => write!(formatter, "reviewed transition failed: {error}"),
            Self::Authority(error) => error.fmt(formatter),
        }
    }
}

/// Nominal pure-shell commit failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizedShellError {
    /// The authorization belongs to another shell policy.
    PolicyMismatch {
        /// Shell-owned policy.
        expected: Hash32,
        /// Authorization policy.
        actual: Hash32,
    },
    /// One replay identity was already bound to different authorization bytes.
    ReplayConflict {
        /// Colliding replay identity.
        replay_id: Hash32,
    },
    /// One candidate was presented under another deployment authorization.
    CandidateAuthorizationConflict {
        /// Colliding semantic candidate.
        candidate_id: CandidateId,
    },
    /// Invocation or policy validation failed.
    Authority(AuthorityError),
    /// Canonical authorization encoding failed.
    Encode(EncodeError),
    /// Structural reference-shell application failed.
    Shell(ShellError),
}

impl From<AuthorityError> for AuthorizedShellError {
    fn from(error: AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<EncodeError> for AuthorizedShellError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<ShellError> for AuthorizedShellError {
    fn from(error: ShellError) -> Self {
        Self::Shell(error)
    }
}

impl fmt::Display for AuthorizedShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PolicyMismatch { .. } => formatter.write_str("authorization policy mismatch"),
            Self::ReplayConflict { .. } => formatter.write_str("authorization replay conflict"),
            Self::CandidateAuthorizationConflict { .. } => {
                formatter.write_str("candidate belongs to another authorization")
            }
            Self::Authority(error) => error.fmt(formatter),
            Self::Encode(error) => write!(formatter, "authorization encoding failed: {error}"),
            Self::Shell(error) => write!(formatter, "reference shell failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AuthorizedShellError {}

#[cfg(all(test, feature = "rustcrypto"))]
mod tests {
    use alloc::vec;

    use super::*;
    use zeno_fcis_catalog::{CatalogLimits, ReasonDefinition, ReasonDisposition};
    use zeno_fcis_core::BudgetUsed;
    use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
    use zeno_fcis_project::{
        DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, StableName,
    };
    use zeno_fcis_schema::{Schema, SchemaLimits, TypeDef, TypeKind, ValidationLimits};
    use zeno_fcis_transition::CataloguedTransitionBuilder;
    use zeno_fcis_value::Value;

    #[derive(Clone, Copy, Debug)]
    struct AcceptProgram;

    impl CatalogTransitionProgram<RustCryptoSha256> for AcceptProgram {
        type Error = TransitionError;

        fn execute(
            &self,
            input: ReviewedTransitionInput<'_>,
        ) -> Result<TransitionDecision, Self::Error> {
            let expected = input.expected_bindings();
            CataloguedTransitionBuilder::<RustCryptoSha256>::try_new(
                input.catalog(),
                input.pre_state().value().value(),
                input.state_domain(),
                expected.command_hash(),
                expected.context_hash(),
                BudgetUsed::default(),
                input.limits(),
            )?
            .seal()
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct RejectProgram;

    impl CatalogTransitionProgram<RustCryptoSha256> for RejectProgram {
        type Error = TransitionError;

        fn execute(
            &self,
            input: ReviewedTransitionInput<'_>,
        ) -> Result<TransitionDecision, Self::Error> {
            let expected = input.expected_bindings();
            let mut builder = CataloguedTransitionBuilder::<RustCryptoSha256>::try_new(
                input.catalog(),
                input.pre_state().value().value(),
                input.state_domain(),
                expected.command_hash(),
                expected.context_hash(),
                BudgetUsed::default(),
                input.limits(),
            )?;
            builder.require(false, semantic_id(10))?;
            builder.seal()
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct WrongLimitsProgram;

    impl CatalogTransitionProgram<RustCryptoSha256> for WrongLimitsProgram {
        type Error = TransitionError;

        fn execute(
            &self,
            input: ReviewedTransitionInput<'_>,
        ) -> Result<TransitionDecision, Self::Error> {
            let expected = input.expected_bindings();
            CataloguedTransitionBuilder::<RustCryptoSha256>::try_new(
                input.catalog(),
                input.pre_state().value().value(),
                input.state_domain(),
                expected.command_hash(),
                expected.context_hash(),
                BudgetUsed::default(),
                TransitionLimits::default(),
            )?
            .seal()
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct TestInterpreter;

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn stable_name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("stable name: {error}"))
    }

    fn semantic_id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("semantic id: {error}"))
    }

    fn type_def(id: u32, label: &str) -> TypeDef {
        TypeDef::try_new(
            TypeId::new(id),
            label,
            TypeKind::Bool,
            SchemaLimits::default(),
        )
        .unwrap_or_else(|error| panic!("type definition: {error}"))
    }

    fn registry_entry(kind: RegistryKind, id: u32, label: &str) -> RegistryEntry {
        RegistryEntry::try_new(kind, semantic_id(id), stable_name(label), hash(id as u8))
            .unwrap_or_else(|error| panic!("registry entry: {error}"))
    }

    fn fixture_catalog() -> ProjectCatalog {
        let schema = Schema::try_new(
            "AuthorityFixture",
            1,
            TypeId::new(1),
            vec![
                type_def(1, "State"),
                type_def(2, "Command"),
                type_def(3, "Context"),
            ],
            SchemaLimits::default(),
        )
        .unwrap_or_else(|error| panic!("schema: {error}"));
        let reason = ReasonDefinition::try_new(
            semantic_id(10),
            stable_name("denied"),
            ReasonDisposition::Reject,
            0,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("reason: {error}"));
        let manifest =
            CatalogManifest::try_new::<RustCryptoSha256>(vec![reason], Vec::new(), Vec::new())
                .unwrap_or_else(|error| panic!("manifest: {error}"));
        let mut entries = vec![
            registry_entry(RegistryKind::StateType, 1, "state"),
            registry_entry(RegistryKind::CommandType, 2, "command"),
            registry_entry(RegistryKind::ContextType, 3, "context"),
        ];
        entries.extend_from_slice(manifest.registry_entries());
        let profile = ProjectProfile::try_new(
            stable_name("authority-fixture"),
            stable_name("core"),
            semantic_id(100),
            1,
            semantic_id(1),
            semantic_id(2),
            semantic_id(3),
            DomainPrefix::try_new("authority/fixture")
                .unwrap_or_else(|error| panic!("domain prefix: {error}")),
            ProfileBindings {
                schema_hash: schema
                    .schema_hash::<RustCryptoSha256>()
                    .unwrap_or_else(|error| panic!("schema hash: {error}")),
                precedence_hash: manifest.precedence_hash(),
                algorithm_hash: hash(40),
                codec_hash: hash(41),
                effect_registry_hash: manifest.effect_registry_hash(),
                channel_registry_hash: manifest.channel_registry_hash(),
                policy_hash: hash(42),
            },
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"));
        ProjectCatalog::try_new::<RustCryptoSha256>(
            profile,
            schema,
            manifest,
            CatalogLimits::default(),
        )
        .unwrap_or_else(|error| panic!("catalog: {error}"))
    }

    fn transition_limits() -> TransitionLimits {
        TransitionLimits::try_new(4, 4, 4, 64, 8, 64)
            .unwrap_or_else(|error| panic!("transition limits: {error}"))
    }

    fn execution(deployment_byte: u8) -> ExecutionBinding {
        ExecutionBinding::try_new(
            hash(50),
            hash(51),
            hash(52),
            hash(deployment_byte),
            hash(54),
        )
        .unwrap_or_else(|error| panic!("execution binding: {error}"))
    }

    fn accept_authority(
        catalog: &ProjectCatalog,
        deployment_byte: u8,
    ) -> CatalogCommitAuthority<RustCryptoSha256, AcceptProgram, TestInterpreter> {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(deployment_byte),
            transition_limits(),
            &provider,
            AcceptProgram,
        )
        .unwrap_or_else(|error| panic!("commit authority: {error}"))
    }

    fn reject_authority(
        catalog: &ProjectCatalog,
    ) -> CatalogCommitAuthority<RustCryptoSha256, RejectProgram, TestInterpreter> {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(53),
            transition_limits(),
            &provider,
            RejectProgram,
        )
        .unwrap_or_else(|error| panic!("reject authority: {error}"))
    }

    fn wrong_limits_authority(
        catalog: &ProjectCatalog,
    ) -> CatalogCommitAuthority<RustCryptoSha256, WrongLimitsProgram, TestInterpreter> {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(53),
            transition_limits(),
            &provider,
            WrongLimitsProgram,
        )
        .unwrap_or_else(|error| panic!("wrong-limits authority: {error}"))
    }

    fn root(catalog: &ProjectCatalog) -> SchemaAdmittedEnvelope {
        SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            Value::Bool(false),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("root envelope: {error}"))
    }

    fn command(catalog: &ProjectCatalog) -> SchemaAdmittedTypeEnvelope {
        SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            TypeId::new(2),
            Value::Bool(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("command envelope: {error}"))
    }

    fn context(catalog: &ProjectCatalog) -> SchemaAdmittedTypeEnvelope {
        SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            TypeId::new(3),
            Value::Bool(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("context envelope: {error}"))
    }

    fn admit<P>(
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, TestInterpreter>,
        catalog: &ProjectCatalog,
        principal: u8,
        replay: u8,
    ) -> InvocationWitness<RustCryptoSha256, P, TestInterpreter>
    where
        P: CatalogTransitionProgram<RustCryptoSha256>,
    {
        authority
            .admit_invocation(
                root(catalog),
                command(catalog),
                context(catalog),
                hash(principal),
                hash(61),
                hash(replay),
            )
            .unwrap_or_else(|error| panic!("invocation: {error}"))
    }

    fn accept(
        authority: &CatalogCommitAuthority<RustCryptoSha256, AcceptProgram, TestInterpreter>,
        catalog: &ProjectCatalog,
        principal: u8,
        replay: u8,
    ) -> CatalogAuthorizedTransition<RustCryptoSha256, AcceptProgram, TestInterpreter> {
        match authority
            .execute(admit(authority, catalog, principal, replay))
            .unwrap_or_else(|error| panic!("execute: {error}"))
        {
            Decision::Accept(accepted) => accepted.into_candidate(),
            Decision::Reject(_) | Decision::CommittedFailure(_) => {
                panic!("fixture program must accept")
            }
        }
    }

    #[test]
    fn exact_authorization_commits_and_replays_idempotently() {
        let catalog = fixture_catalog();
        let authority = accept_authority(&catalog, 53);
        let shell = AuthorizedShellState::new(&authority, &root(&catalog))
            .unwrap_or_else(|error| panic!("shell: {error}"));
        let first = shell
            .commit(accept(&authority, &catalog, 60, 62))
            .unwrap_or_else(|error| panic!("first commit: {error}"));
        assert_eq!(first.status(), CommitStatus::Committed);
        let second = first
            .into_state()
            .commit(accept(&authority, &catalog, 60, 62))
            .unwrap_or_else(|error| panic!("replay commit: {error}"));
        assert_eq!(second.status(), CommitStatus::IdempotentReplay);
        assert_eq!(second.state().authorization_records().len(), 1);
    }

    #[test]
    fn principal_and_replay_are_bound_into_candidate_and_authorization() {
        let catalog = fixture_catalog();
        let authority = accept_authority(&catalog, 53);
        let base = accept(&authority, &catalog, 60, 62);
        let other_principal = accept(&authority, &catalog, 63, 62);
        let other_replay = accept(&authority, &catalog, 60, 64);
        assert_ne!(
            base.invocation().expected_bindings().context_hash(),
            other_principal
                .invocation()
                .expected_bindings()
                .context_hash()
        );
        assert_ne!(
            base.body().candidate_id(),
            other_principal.body().candidate_id()
        );
        assert_ne!(base.authorization_id(), other_principal.authorization_id());
        assert_ne!(
            base.body().candidate_id(),
            other_replay.body().candidate_id()
        );
        assert_ne!(base.authorization_id(), other_replay.authorization_id());
    }

    #[test]
    fn shell_rejects_authorization_from_another_deployment_policy() {
        let catalog = fixture_catalog();
        let first_authority = accept_authority(&catalog, 53);
        let other_authority = accept_authority(&catalog, 55);
        assert_ne!(
            first_authority.policy().policy_id(),
            other_authority.policy().policy_id()
        );
        let shell = AuthorizedShellState::new(&first_authority, &root(&catalog))
            .unwrap_or_else(|error| panic!("shell: {error}"));
        let error = shell
            .commit(accept(&other_authority, &catalog, 60, 62))
            .err()
            .unwrap_or_else(|| panic!("other deployment must fail"));
        assert!(matches!(error, AuthorizedShellError::PolicyMismatch { .. }));
    }

    #[test]
    fn ordinary_reject_never_creates_commit_authority() {
        let catalog = fixture_catalog();
        let authority = reject_authority(&catalog);
        let decision = authority
            .execute(admit(&authority, &catalog, 60, 62))
            .unwrap_or_else(|error| panic!("execute reject: {error}"));
        match decision {
            Decision::Reject(rejected) => {
                assert_eq!(rejected.reason().rejection().reason_id(), semantic_id(10));
            }
            Decision::Accept(_) | Decision::CommittedFailure(_) => {
                panic!("fixture program must reject")
            }
        }
    }

    #[test]
    fn shell_owned_transition_limits_cannot_be_replaced_by_program() {
        let catalog = fixture_catalog();
        let authority = wrong_limits_authority(&catalog);
        let error = authority
            .execute(admit(&authority, &catalog, 60, 62))
            .err()
            .unwrap_or_else(|| panic!("wrong limits must fail"));
        assert!(matches!(
            error,
            CatalogExecutionError::Authority(AuthorityError::Mismatch(
                AuthorityField::TransitionLimits
            ))
        ));
    }
}
