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
use zeno_fcis_laws::{
    GenesisLawCheckInput, GenesisLawEvaluation, LawCheckInput, LawDecisionView, LawError,
    LawEvaluation, ProjectLawEngine, VerifiedProjectLaws,
};
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_project::SemanticId;
use zeno_fcis_receipt::{CandidateId, CommitBundle};
use zeno_fcis_schema::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, TypeId};
use zeno_fcis_shell::{CommitStatus, ShellError, ShellState, apply_reference_bundle};
use zeno_fcis_transition::{
    ExpectedInvocationBindings, TransitionArtifacts, TransitionDecision, TransitionError,
    TransitionLimits, TransitionReject,
};

type AuthorityMarker<H, P, L, I> = PhantomData<fn() -> (H, P, L, I)>;

/// Canonical authorization-envelope format version.
pub const AUTHORIZATION_FORMAT_VERSION: u16 = 2;
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

/// Reviewed immutable initial-state and deployment-instance commitments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisPolicyBinding {
    expected_initial_root: NonZeroHash,
    source_hash: NonZeroHash,
    configuration_hash: NonZeroHash,
    evidence_hash: NonZeroHash,
    deployment_instance_hash: NonZeroHash,
}

impl GenesisPolicyBinding {
    /// Creates one exact genesis policy from five required commitments.
    pub fn try_new(
        expected_initial_root: Hash32,
        source_hash: Hash32,
        configuration_hash: Hash32,
        evidence_hash: Hash32,
        deployment_instance_hash: Hash32,
    ) -> Result<Self, AuthorityError> {
        Ok(Self {
            expected_initial_root: NonZeroHash::try_new(expected_initial_root)?,
            source_hash: NonZeroHash::try_new(source_hash)?,
            configuration_hash: NonZeroHash::try_new(configuration_hash)?,
            evidence_hash: NonZeroHash::try_new(evidence_hash)?,
            deployment_instance_hash: NonZeroHash::try_new(deployment_instance_hash)?,
        })
    }

    /// Returns the exact reviewed initial semantic root.
    #[must_use]
    pub const fn expected_initial_root(self) -> Hash32 {
        self.expected_initial_root.get()
    }

    /// Returns the reviewed genesis source/build commitment.
    #[must_use]
    pub const fn source_hash(self) -> Hash32 {
        self.source_hash.get()
    }

    /// Returns the reviewed initial configuration commitment.
    #[must_use]
    pub const fn configuration_hash(self) -> Hash32 {
        self.configuration_hash.get()
    }

    /// Returns the retained genesis evidence commitment.
    #[must_use]
    pub const fn evidence_hash(self) -> Hash32 {
        self.evidence_hash.get()
    }

    /// Returns the unique shell/deployment-instance commitment.
    #[must_use]
    pub const fn deployment_instance_hash(self) -> Hash32 {
        self.deployment_instance_hash.get()
    }

    /// Returns the complete genesis-policy binding identity.
    pub fn commitment<H: ApprovedCommitmentProvider>(&self) -> Result<Hash32, AuthorityError> {
        hash_canonical::<H>("zeno-fcis/genesis-policy-binding", self)
    }
}

impl CanonicalEncode for GenesisPolicyBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-GENESIS-POLICY\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        self.expected_initial_root.encode_to(output)?;
        self.source_hash.encode_to(output)?;
        self.configuration_hash.encode_to(output)?;
        self.evidence_hash.encode_to(output)?;
        self.deployment_instance_hash.encode_to(output)
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

    /// Returns the exact reviewed semantic program/build commitment.
    ///
    /// The owning authority requires this value to equal its independently
    /// supplied [`ExecutionBinding::transition_build_hash`]. Nominal Rust type
    /// identity alone does not bind configuration values stored inside `Self`.
    fn transition_build_hash(&self) -> Hash32;

    /// Executes the reviewed transition over exact shell-owned inputs.
    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error>;
}

/// Shell-owned catalog, provider, program, interpreter, deployment, limits, and replay policy.
pub struct AuthorizationPolicy<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    catalog: ProjectCatalog,
    catalog_hash: Hash32,
    state_domain: StateDomainBinding,
    execution: ExecutionBinding,
    genesis: GenesisPolicyBinding,
    genesis_binding_hash: Hash32,
    transition_limits: TransitionLimits,
    provider_id: ApprovedProviderId,
    law_set_hash: Hash32,
    law_engine_build_hash: Hash32,
    law_evidence_verifier_hash: Hash32,
    policy_id: Hash32,
    marker: AuthorityMarker<H, P, L, I>,
}

impl<H, P, L, I> AuthorizationPolicy<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    fn try_new(
        catalog: &ProjectCatalog,
        state_domain: StateDomainBinding,
        execution: ExecutionBinding,
        genesis: GenesisPolicyBinding,
        transition_limits: TransitionLimits,
        provider: &VerifiedProvider<H>,
        laws: &VerifiedProjectLaws<H, L>,
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
        if laws.catalog_hash() != catalog_hash {
            return Err(AuthorityError::Mismatch(AuthorityField::LawCatalog));
        }
        let source = laws.source_bindings();
        if source.profile_hash() != approved_catalog.profile_hash()
            || source.schema_hash() != approved_catalog.schema_hash()
            || source.algorithm_hash() != approved_catalog.profile().bindings().algorithm_hash
        {
            return Err(AuthorityError::Mismatch(AuthorityField::LawSourceBindings));
        }
        require_nonzero(laws.law_set_hash(), AuthorityField::LawSet)?;
        require_nonzero(laws.engine_build_hash(), AuthorityField::LawEngine)?;
        require_nonzero(
            laws.evidence_verifier_hash(),
            AuthorityField::LawEvidenceVerifier,
        )?;
        let genesis_binding_hash = genesis.commitment::<H>()?;
        require_nonzero(genesis_binding_hash, AuthorityField::GenesisPolicy)?;
        let mut policy = Self {
            catalog: approved_catalog,
            catalog_hash,
            state_domain,
            execution,
            genesis,
            genesis_binding_hash,
            transition_limits,
            provider_id: provider.provider_id(),
            law_set_hash: laws.law_set_hash(),
            law_engine_build_hash: laws.engine_build_hash(),
            law_evidence_verifier_hash: laws.evidence_verifier_hash(),
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

    /// Returns the exact reviewed genesis policy.
    #[must_use]
    pub const fn genesis(&self) -> GenesisPolicyBinding {
        self.genesis
    }

    /// Returns the complete genesis-policy binding commitment.
    #[must_use]
    pub const fn genesis_binding_hash(&self) -> Hash32 {
        self.genesis_binding_hash
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

    /// Returns the exact verified relational-law set owned by this policy.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the reviewed per-invocation law-engine build identity.
    #[must_use]
    pub const fn law_engine_build_hash(&self) -> Hash32 {
        self.law_engine_build_hash
    }

    /// Returns the independently mounted retained-evidence verifier identity.
    #[must_use]
    pub const fn law_evidence_verifier_hash(&self) -> Hash32 {
        self.law_evidence_verifier_hash
    }

    /// Returns the complete policy identity used to pin a shell.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }
}

impl<H, P, L, I> CanonicalEncode for AuthorizationPolicy<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZATION-POLICY\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.catalog.profile_hash().as_bytes());
        output.extend_from_slice(self.catalog.schema_hash().as_bytes());
        output.extend_from_slice(self.catalog.manifest().precedence_hash().as_bytes());
        self.state_domain.encode_to(output)?;
        self.genesis.encode_to(output)?;
        output.extend_from_slice(self.genesis_binding_hash.as_bytes());
        output.extend_from_slice(&self.provider_id.code().to_be_bytes());
        put_u16_blob(output, H::ALGORITHM_ID.as_bytes())?;
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.law_engine_build_hash.as_bytes());
        output.extend_from_slice(self.law_evidence_verifier_hash.as_bytes());
        self.execution.encode_to(output)?;
        self.transition_limits.encode_to(output)
    }
}

/// Deployment-specific identity of one authorized genesis ceremony.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenesisId(Hash32);

impl GenesisId {
    /// Returns the underlying commitment.
    #[must_use]
    pub const fn hash(self) -> Hash32 {
        self.0
    }
}

impl fmt::Display for GenesisId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Inspectable fixed-shape body of one law-verified genesis authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenesisAuthorizationBody {
    policy_id: Hash32,
    catalog_hash: Hash32,
    genesis_binding_hash: Hash32,
    initial_root: Hash32,
    law_set_hash: Hash32,
    law_evaluation_hash: Hash32,
}

impl GenesisAuthorizationBody {
    /// Returns the complete shell policy identity.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the exact project catalog commitment.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the reviewed genesis-policy commitment.
    #[must_use]
    pub const fn genesis_binding_hash(&self) -> Hash32 {
        self.genesis_binding_hash
    }

    /// Returns the exact admitted initial semantic root.
    #[must_use]
    pub const fn initial_root(&self) -> Hash32 {
        self.initial_root
    }

    /// Returns the complete verified law-set identity.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the complete genesis-law evaluation identity.
    #[must_use]
    pub const fn law_evaluation_hash(&self) -> Hash32 {
        self.law_evaluation_hash
    }
}

impl CanonicalEncode for GenesisAuthorizationBody {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-GENESIS-AUTHORIZATION-BODY\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        output.extend_from_slice(self.genesis_binding_hash.as_bytes());
        output.extend_from_slice(self.initial_root.as_bytes());
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.law_evaluation_hash.as_bytes());
        Ok(())
    }
}

/// Private-construction authority to initialize one exact production store.
///
/// Raw schema admission cannot construct this type:
///
/// ```compile_fail
/// use zeno_fcis_authority::CatalogAuthorizedGenesis;
/// use zeno_fcis_schema::SchemaAdmittedEnvelope;
///
/// fn raw_is_not_genesis(value: SchemaAdmittedEnvelope) {
///     let _: CatalogAuthorizedGenesis<(), (), (), ()> = value;
/// }
/// ```
pub struct CatalogAuthorizedGenesis<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    body: GenesisAuthorizationBody,
    initial_state: SchemaAdmittedEnvelope,
    law_evaluation: GenesisLawEvaluation,
    genesis_id: GenesisId,
    marker: AuthorityMarker<H, P, L, I>,
}

impl<H, P, L, I> CatalogAuthorizedGenesis<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    /// Returns the complete inspectable genesis body.
    #[must_use]
    pub const fn body(&self) -> &GenesisAuthorizationBody {
        &self.body
    }

    /// Returns the exact schema-admitted initial state.
    #[must_use]
    pub const fn initial_state(&self) -> &SchemaAdmittedEnvelope {
        &self.initial_state
    }

    /// Returns the complete successful genesis-law evaluation.
    #[must_use]
    pub const fn law_evaluation(&self) -> &GenesisLawEvaluation {
        &self.law_evaluation
    }

    /// Returns the deployment-specific genesis identity.
    #[must_use]
    pub const fn genesis_id(&self) -> GenesisId {
        self.genesis_id
    }
}

impl<H, P, L, I> CanonicalEncode for CatalogAuthorizedGenesis<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZED-GENESIS\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        put_blob(output, &self.body.canonical_bytes()?)?;
        put_blob(output, &self.initial_state.canonical_bytes()?)?;
        put_blob(output, &self.law_evaluation.canonical_bytes()?)
    }
}

/// Owns the only transition program allowed to mint one nominal authorization type.
pub struct CatalogCommitAuthority<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    policy: AuthorizationPolicy<H, P, L, I>,
    laws: VerifiedProjectLaws<H, L>,
    program: P,
}

impl<H, P, L, I> CatalogCommitAuthority<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    /// Pins one exact catalog, program type, interpreter type, and deployment policy.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        catalog: &ProjectCatalog,
        state_domain: StateDomainBinding,
        execution: ExecutionBinding,
        genesis: GenesisPolicyBinding,
        transition_limits: TransitionLimits,
        provider: &VerifiedProvider<H>,
        laws: VerifiedProjectLaws<H, L>,
        program: P,
    ) -> Result<Self, AuthorityError> {
        if program.transition_build_hash() != execution.transition_build_hash() {
            return Err(AuthorityError::Mismatch(AuthorityField::TransitionBuild));
        }
        let policy = AuthorizationPolicy::try_new(
            catalog,
            state_domain,
            execution,
            genesis,
            transition_limits,
            provider,
            &laws,
        )?;
        Ok(Self {
            policy,
            laws,
            program,
        })
    }

    /// Returns the exact shell-owned policy.
    #[must_use]
    pub const fn policy(&self) -> &AuthorizationPolicy<H, P, L, I> {
        &self.policy
    }

    /// Binds one concrete interpreter instance to this exact policy.
    #[must_use]
    pub fn bind_interpreter(&self, interpreter: I) -> BoundInterpreter<H, P, L, I> {
        BoundInterpreter {
            policy_id: self.policy.policy_id,
            interpreter,
            marker: PhantomData,
        }
    }

    /// Evaluates the exact initial state and mints nominal genesis authority.
    pub fn authorize_genesis(
        &self,
        initial_state: SchemaAdmittedEnvelope,
    ) -> Result<CatalogAuthorizedGenesis<H, P, L, I>, AuthorityError> {
        validate_root_envelope(&self.policy, &initial_state)?;
        let initial_root = hash_value::<H>(
            self.policy.state_domain.domain()?,
            initial_state.value().value(),
        )?;
        if initial_root != self.policy.genesis.expected_initial_root() {
            return Err(AuthorityError::Mismatch(AuthorityField::GenesisRoot));
        }
        let input = GenesisLawCheckInput::try_new(
            self.policy.catalog_hash,
            self.policy.policy_id,
            self.policy.genesis_binding_hash,
            initial_state.value().value(),
        )?;
        let law_evaluation = self.laws.evaluate_genesis(&input)?;
        let body = GenesisAuthorizationBody {
            policy_id: self.policy.policy_id,
            catalog_hash: self.policy.catalog_hash,
            genesis_binding_hash: self.policy.genesis_binding_hash,
            initial_root,
            law_set_hash: self.policy.law_set_hash,
            law_evaluation_hash: law_evaluation.evaluation_hash(),
        };
        let mut genesis = CatalogAuthorizedGenesis {
            body,
            initial_state,
            law_evaluation,
            genesis_id: GenesisId(Hash32::ZERO),
            marker: PhantomData,
        };
        let genesis_hash = hash_canonical::<H>("zeno-fcis/authorized-genesis", &genesis)?;
        require_nonzero(genesis_hash, AuthorityField::GenesisId)?;
        genesis.genesis_id = GenesisId(genesis_hash);
        Ok(genesis)
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
    ) -> Result<InvocationWitness<H, P, L, I>, AuthorityError> {
        validate_envelope_bindings(&self.policy, &pre_state, &command, &context)?;
        let principal_hash = NonZeroHash::try_new(principal_hash)?;
        let authentication_evidence_hash = NonZeroHash::try_new(authentication_evidence_hash)?;
        let replay_id = NonZeroHash::try_new(replay_id)?;
        let command_hash = command_commitment::<H, P, L, I>(&self.policy, &command)?;
        let context_hash = context_commitment::<H, P, L, I>(
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
        invocation: InvocationWitness<H, P, L, I>,
    ) -> Result<CatalogAuthorizationDecision<H, P, L, I>, CatalogExecutionError<P::Error>> {
        if self.program.transition_build_hash() != self.policy.execution.transition_build_hash() {
            return Err(CatalogExecutionError::Authority(AuthorityError::Mismatch(
                AuthorityField::TransitionBuild,
            )));
        }
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
        authorize_decision(&self.policy, &self.laws, invocation, decision)
            .map_err(CatalogExecutionError::Authority)
    }
}

/// Concrete interpreter instance nominally bound to one exact authorization policy.
///
/// Private fields prevent a same-type interpreter from being substituted at a
/// commit port without passing through the owning [`CatalogCommitAuthority`].
pub struct BoundInterpreter<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    policy_id: Hash32,
    interpreter: I,
    marker: AuthorityMarker<H, P, L, I>,
}

impl<H, P, L, I> BoundInterpreter<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
pub struct InvocationWitness<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
    marker: AuthorityMarker<H, P, L, I>,
}

impl<H, P, L, I> InvocationWitness<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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

impl<H, P, L, I> CanonicalEncode for InvocationWitness<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
    law_set_hash: Hash32,
    law_evaluation_hash: Hash32,
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

    /// Returns the verified project-law set used for authorization.
    #[must_use]
    pub const fn law_set_hash(&self) -> Hash32 {
        self.law_set_hash
    }

    /// Returns the complete per-invocation relational-law evaluation.
    #[must_use]
    pub const fn law_evaluation_hash(&self) -> Hash32 {
        self.law_evaluation_hash
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
        output.extend_from_slice(self.law_set_hash.as_bytes());
        output.extend_from_slice(self.law_evaluation_hash.as_bytes());
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
/// fn raw_bundle_is_not_authority<H, P, L, I>(
///     bundle: CommitBundle,
/// ) -> CatalogAuthorizedTransition<H, P, L, I>
/// where
///     H: ApprovedCommitmentProvider,
///     P: CatalogTransitionProgram<H>,
///     L: zeno_fcis_laws::ProjectLawEngine,
/// {
///     bundle.into()
/// }
/// ```
pub struct CatalogAuthorizedTransition<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    authorization_id: AuthorizationId,
    body: AuthorizationBody,
    invocation: InvocationWitness<H, P, L, I>,
    law_evaluation: LawEvaluation,
    artifacts: TransitionArtifacts,
}

impl<H, P, L, I> CatalogAuthorizedTransition<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
    pub const fn invocation(&self) -> &InvocationWitness<H, P, L, I> {
        &self.invocation
    }

    /// Returns the complete satisfied project-law evaluation.
    #[must_use]
    pub const fn law_evaluation(&self) -> &LawEvaluation {
        &self.law_evaluation
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

impl<H, P, L, I> CanonicalEncode for CatalogAuthorizedTransition<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_authorization_envelope(&self.body, &self.invocation, output)
    }
}

impl<H, P, L, I> fmt::Debug for CatalogAuthorizedTransition<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
pub struct CatalogAuthorizedReject<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    rejection_id: Hash32,
    policy_id: Hash32,
    invocation: InvocationWitness<H, P, L, I>,
    law_evaluation: LawEvaluation,
    rejection: TransitionReject,
}

impl<H, P, L, I> CatalogAuthorizedReject<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
    pub const fn invocation(&self) -> &InvocationWitness<H, P, L, I> {
        &self.invocation
    }

    /// Returns the complete satisfied project-law evaluation.
    #[must_use]
    pub const fn law_evaluation(&self) -> &LawEvaluation {
        &self.law_evaluation
    }

    /// Returns the unchanged-state rejection evidence.
    #[must_use]
    pub const fn rejection(&self) -> &TransitionReject {
        &self.rejection
    }
}

impl<H, P, L, I> CanonicalEncode for CatalogAuthorizedReject<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-AUTHORIZED-REJECT\0");
        output.extend_from_slice(&AUTHORIZATION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.policy_id.as_bytes());
        put_blob(output, &self.invocation.canonical_bytes()?)?;
        put_blob(output, &self.law_evaluation.canonical_bytes()?)?;
        self.rejection.reason_id().encode_to(output)?;
        put_blob(output, &self.rejection.receipt().canonical_bytes()?)?;
        put_blob(output, &self.rejection.footprint().canonical_bytes()?)?;
        put_blob(output, &self.rejection.resources().canonical_bytes()?)
    }
}

impl<H, P, L, I> fmt::Debug for CatalogAuthorizedReject<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
pub type CatalogAuthorizationDecision<H, P, L, I> = Decision<
    CatalogAuthorizedTransition<H, P, L, I>,
    CatalogAuthorizedReject<H, P, L, I>,
    SemanticId,
>;

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
pub struct AuthorizedShellState<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    policy_id: Hash32,
    genesis_id: GenesisId,
    state_domain: StateDomainBinding,
    inner: ShellState,
    records: Box<[AuthorizationRecord]>,
    marker: AuthorityMarker<H, P, L, I>,
}

impl<H, P, L, I> AuthorizedShellState<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    /// Creates an empty authorized shell from one exact law-verified genesis.
    pub fn new(
        authority: &CatalogCommitAuthority<H, P, L, I>,
        genesis: CatalogAuthorizedGenesis<H, P, L, I>,
    ) -> Result<Self, AuthorizedShellError> {
        if genesis.body.policy_id != authority.policy.policy_id {
            return Err(AuthorizedShellError::PolicyMismatch {
                expected: authority.policy.policy_id,
                actual: genesis.body.policy_id,
            });
        }
        let inner = ShellState::new::<H>(
            genesis.initial_state.value().value().clone(),
            authority.policy.state_domain.domain()?,
        )?;
        if inner.root() != genesis.body.initial_root {
            return Err(AuthorizedShellError::Authority(AuthorityError::Mismatch(
                AuthorityField::GenesisRoot,
            )));
        }
        Ok(Self {
            policy_id: authority.policy.policy_id,
            genesis_id: genesis.genesis_id,
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

    /// Returns the exact genesis authorization that initialized this shell.
    #[must_use]
    pub const fn genesis_id(&self) -> GenesisId {
        self.genesis_id
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
        authorized: CatalogAuthorizedTransition<H, P, L, I>,
    ) -> Result<AuthorizedCommitResult<H, P, L, I>, AuthorizedShellError> {
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
                genesis_id: self.genesis_id,
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
pub struct AuthorizedCommitResult<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    state: AuthorizedShellState<H, P, L, I>,
    status: CommitStatus,
}

impl<H, P, L, I> AuthorizedCommitResult<H, P, L, I>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    /// Returns the immutable authorized successor state.
    #[must_use]
    pub const fn state(&self) -> &AuthorizedShellState<H, P, L, I> {
        &self.state
    }

    /// Returns whether publication committed or replayed idempotently.
    #[must_use]
    pub const fn status(&self) -> CommitStatus {
        self.status
    }

    /// Consumes the result and returns the successor state.
    #[must_use]
    pub fn into_state(self) -> AuthorizedShellState<H, P, L, I> {
        self.state
    }
}

fn authorize_decision<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    laws: &VerifiedProjectLaws<H, L>,
    invocation: InvocationWitness<H, P, L, I>,
    decision: TransitionDecision,
) -> Result<CatalogAuthorizationDecision<H, P, L, I>, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    match decision {
        Decision::Accept(accepted) => {
            let artifacts = accepted.into_candidate();
            if artifacts.reason_id().is_some() {
                return Err(AuthorityError::Mismatch(AuthorityField::Reason));
            }
            let authorized = authorize_artifacts(policy, laws, invocation, artifacts)?;
            Ok(Decision::Accept(Accepted::new(authorized)))
        }
        Decision::Reject(rejected) => {
            let rejection = rejected.into_reason();
            validate_transition_reject(policy, &invocation, &rejection)?;
            let law_input = LawCheckInput::try_new(
                policy.catalog_hash,
                invocation.invocation_id,
                invocation.pre_state.value().value(),
                invocation.command.value().value(),
                invocation.context.value().value(),
                LawDecisionView::Reject {
                    reason_id: rejection.reason_id().get(),
                },
            )?;
            let law_evaluation = laws.evaluate(&law_input)?;
            let mut authorized = CatalogAuthorizedReject {
                rejection_id: Hash32::ZERO,
                policy_id: policy.policy_id,
                invocation,
                law_evaluation,
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
            let authorized = authorize_artifacts(policy, laws, invocation, artifacts)?;
            Ok(Decision::CommittedFailure(Failed::new(authorized, reason)))
        }
    }
}

fn authorize_artifacts<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    laws: &VerifiedProjectLaws<H, L>,
    invocation: InvocationWitness<H, P, L, I>,
    artifacts: TransitionArtifacts,
) -> Result<CatalogAuthorizedTransition<H, P, L, I>, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    if artifacts.resources().limits() != policy.transition_limits {
        return Err(AuthorityError::Mismatch(AuthorityField::TransitionLimits));
    }
    let applied = artifacts.validate_and_apply::<H>(
        policy.catalog(),
        invocation.expected,
        invocation.pre_state.value().value(),
        policy.state_domain.domain()?,
    )?;
    let bundle = artifacts.bundle();
    if bundle.body().pre_root() != invocation.pre_root {
        return Err(AuthorityError::Mismatch(AuthorityField::PreRoot));
    }
    let law_decision = match bundle.body().decision_kind() {
        DecisionKind::Accept => LawDecisionView::Accept {
            post_state: applied.state(),
            patch: bundle.patch(),
            commit_plan: bundle.commit_plan(),
            outbox_plan: bundle.outbox_plan(),
        },
        DecisionKind::CommittedFailure => LawDecisionView::CommittedFailure {
            reason_id: artifacts
                .reason_id()
                .ok_or(AuthorityError::Mismatch(AuthorityField::Reason))?
                .get(),
            post_state: applied.state(),
            patch: bundle.patch(),
            commit_plan: bundle.commit_plan(),
            outbox_plan: bundle.outbox_plan(),
        },
        DecisionKind::Reject => {
            return Err(AuthorityError::Mismatch(AuthorityField::Reason));
        }
    };
    let law_input = LawCheckInput::try_new(
        policy.catalog_hash,
        invocation.invocation_id,
        invocation.pre_state.value().value(),
        invocation.command.value().value(),
        invocation.context.value().value(),
        law_decision,
    )?;
    let law_evaluation = laws.evaluate(&law_input)?;
    let bundle_hash = hash_canonical::<H>("zeno-fcis/authorized-bundle", bundle)?;
    let body = AuthorizationBody {
        policy_id: policy.policy_id,
        invocation_id: invocation.invocation_id,
        law_set_hash: policy.law_set_hash,
        law_evaluation_hash: law_evaluation.evaluation_hash(),
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
        law_evaluation,
        artifacts,
    })
}

fn validate_transition_reject<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    invocation: &InvocationWitness<H, P, L, I>,
    rejection: &TransitionReject,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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

fn validate_envelope_bindings<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    pre_state: &SchemaAdmittedEnvelope,
    command: &SchemaAdmittedTypeEnvelope,
    context: &SchemaAdmittedTypeEnvelope,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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

fn validate_root_envelope<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    pre_state: &SchemaAdmittedEnvelope,
) -> Result<(), AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
{
    if pre_state.schema_hash() != policy.catalog.schema_hash() {
        return Err(AuthorityError::Mismatch(AuthorityField::Schema));
    }
    if pre_state.root_type() != TypeId::new(policy.catalog.profile().state_type().get()) {
        return Err(AuthorityError::Mismatch(AuthorityField::StateType));
    }
    Ok(())
}

fn command_commitment<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    command: &SchemaAdmittedTypeEnvelope,
) -> Result<Hash32, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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

fn context_commitment<H, P, L, I>(
    policy: &AuthorizationPolicy<H, P, L, I>,
    context: &SchemaAdmittedTypeEnvelope,
    principal_hash: NonZeroHash,
    authentication_evidence_hash: NonZeroHash,
    replay_id: NonZeroHash,
) -> Result<Hash32, AuthorityError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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

fn encode_authorization_envelope<H, P, L, I>(
    body: &AuthorizationBody,
    invocation: &InvocationWitness<H, P, L, I>,
    output: &mut Vec<u8>,
) -> Result<(), EncodeError>
where
    H: ApprovedCommitmentProvider,
    P: CatalogTransitionProgram<H>,
    L: ProjectLawEngine,
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
    /// Complete reviewed genesis policy.
    GenesisPolicy,
    /// Exact expected initial semantic root.
    GenesisRoot,
    /// Deployment-specific genesis authorization identity.
    GenesisId,
    /// Project catalog committed by the verified law set.
    LawCatalog,
    /// Source/profile/schema/algorithm bindings of the verified law set.
    LawSourceBindings,
    /// Complete verified relational-law set.
    LawSet,
    /// Reviewed per-invocation law-engine build.
    LawEngine,
    /// Independent retained-evidence verifier.
    LawEvidenceVerifier,
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
    /// Exact reviewed transition program/build identity.
    TransitionBuild,
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
    /// Project relational-law validation or evaluation failed.
    Laws(LawError),
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

impl From<LawError> for AuthorityError {
    fn from(error: LawError) -> Self {
        Self::Laws(error)
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
            Self::Laws(error) => write!(formatter, "authorization project laws failed: {error}"),
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
    use zeno_fcis_evidence::EvidenceEnvelope;
    use zeno_fcis_laws::{
        DecisionScope, GenesisApplicability, GenesisLawCheckInput, LawDefinition,
        LawEvidenceRequirement, LawEvidenceVerifier, LawFamilyPolicy, LawKind, LawLimits,
        LawObservation, LawProofDecision, LawProofSubject, LawStatus, ProjectLawEngine,
        VerifiedProjectLaws, verify_project_laws,
    };
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

        fn transition_build_hash(&self) -> Hash32 {
            hash(50)
        }

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

        fn transition_build_hash(&self) -> Hash32 {
            hash(50)
        }

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

        fn transition_build_hash(&self) -> Hash32 {
            hash(50)
        }

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

    #[derive(Clone, Copy, Debug)]
    struct TestLawEngine;

    impl ProjectLawEngine for TestLawEngine {
        fn evaluate(
            &self,
            input: &LawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            let witness = hash(91);
            match input.decision().kind() {
                DecisionKind::Accept => Ok(vec![
                    LawObservation::try_new(semantic_id(1_001), LawStatus::Satisfied, witness)
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                ]),
                DecisionKind::Reject => Ok(Vec::new()),
                DecisionKind::CommittedFailure => Ok(vec![
                    LawObservation::try_new(semantic_id(1_001), LawStatus::Satisfied, witness)
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                    LawObservation::try_new(semantic_id(1_003), LawStatus::Satisfied, witness)
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                ]),
            }
        }

        fn evaluate_genesis(
            &self,
            _: &GenesisLawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            Ok(vec![
                LawObservation::try_new(semantic_id(1_001), LawStatus::Satisfied, hash(91))
                    .unwrap_or_else(|error| panic!("genesis observation: {error}")),
            ])
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct ViolatingLawEngine;

    impl ProjectLawEngine for ViolatingLawEngine {
        fn evaluate(
            &self,
            _: &LawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            Ok(vec![
                LawObservation::try_new(semantic_id(1_001), LawStatus::Violated, hash(95))
                    .unwrap_or_else(|error| panic!("law observation: {error}")),
            ])
        }

        fn evaluate_genesis(
            &self,
            _: &GenesisLawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            Ok(vec![
                LawObservation::try_new(semantic_id(1_001), LawStatus::Violated, hash(95))
                    .unwrap_or_else(|error| panic!("genesis observation: {error}")),
            ])
        }
    }

    struct TestEvidenceVerifier;

    impl LawEvidenceVerifier for TestEvidenceVerifier {
        fn verifier_identity(&self) -> Hash32 {
            hash(92)
        }

        fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
            LawProofDecision::Attested {
                verification_claim: hash(93),
            }
        }
    }

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

    fn law_manifest() -> zeno_fcis_laws::LawManifest {
        let families = LawKind::ALL
            .into_iter()
            .map(|kind| {
                if matches!(
                    kind,
                    LawKind::StateInvariant
                        | LawKind::RejectNoAuthority
                        | LawKind::CommittedFailureEffects
                ) {
                    LawFamilyPolicy::required(kind)
                } else {
                    LawFamilyPolicy::not_applicable(kind, hash(94))
                        .unwrap_or_else(|error| panic!("law family: {error}"))
                }
            })
            .collect();
        let definitions = vec![
            LawDefinition::try_new(
                semantic_id(1_001),
                stable_name("state-invariant"),
                LawKind::StateInvariant,
                DecisionScope::Committing,
                GenesisApplicability::Required,
                hash(101),
                hash(111),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("state law: {error}")),
            LawDefinition::try_new(
                semantic_id(1_002),
                stable_name("reject-no-authority"),
                LawKind::RejectNoAuthority,
                DecisionScope::Reject,
                GenesisApplicability::NotApplicable {
                    rationale_hash: hash(121),
                },
                hash(102),
                hash(112),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("reject law: {error}")),
            LawDefinition::try_new(
                semantic_id(1_003),
                stable_name("committed-failure-effects"),
                LawKind::CommittedFailureEffects,
                DecisionScope::CommittedFailure,
                GenesisApplicability::NotApplicable {
                    rationale_hash: hash(122),
                },
                hash(103),
                hash(113),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("failure law: {error}")),
        ];
        zeno_fcis_laws::LawManifest::try_new(families, definitions)
            .unwrap_or_else(|error| panic!("law manifest: {error}"))
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
        let laws = law_manifest();
        let mut entries = vec![
            registry_entry(RegistryKind::StateType, 1, "state"),
            registry_entry(RegistryKind::CommandType, 2, "command"),
            registry_entry(RegistryKind::ContextType, 3, "context"),
        ];
        entries.extend_from_slice(manifest.registry_entries());
        entries.extend(
            laws.registry_entries::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("law registry: {error}")),
        );
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
                policy_hash: laws
                    .commitment::<RustCryptoSha256>()
                    .unwrap_or_else(|error| panic!("law commitment: {error}")),
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

    fn verified_laws(
        catalog: &ProjectCatalog,
    ) -> VerifiedProjectLaws<RustCryptoSha256, TestLawEngine> {
        verify_project_laws::<RustCryptoSha256, _, _>(
            catalog,
            law_manifest(),
            hash(90),
            Vec::new(),
            LawLimits::default(),
            hash(91),
            TestLawEngine,
            &TestEvidenceVerifier,
        )
        .unwrap_or_else(|error| panic!("verified laws: {error}"))
    }

    fn violating_laws(
        catalog: &ProjectCatalog,
    ) -> VerifiedProjectLaws<RustCryptoSha256, ViolatingLawEngine> {
        verify_project_laws::<RustCryptoSha256, _, _>(
            catalog,
            law_manifest(),
            hash(90),
            Vec::new(),
            LawLimits::default(),
            hash(96),
            ViolatingLawEngine,
            &TestEvidenceVerifier,
        )
        .unwrap_or_else(|error| panic!("verified laws: {error}"))
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

    fn genesis_policy(catalog: &ProjectCatalog, instance: u8) -> GenesisPolicyBinding {
        let domain = Domain::new("authority/fixture/state", 1)
            .unwrap_or_else(|error| panic!("state domain: {error}"));
        let initial_root = hash_value::<RustCryptoSha256>(domain, root(catalog).value().value())
            .unwrap_or_else(|error| panic!("initial root: {error}"));
        GenesisPolicyBinding::try_new(initial_root, hash(70), hash(71), hash(72), hash(instance))
            .unwrap_or_else(|error| panic!("genesis policy: {error}"))
    }

    fn accept_authority(
        catalog: &ProjectCatalog,
        deployment_byte: u8,
    ) -> CatalogCommitAuthority<RustCryptoSha256, AcceptProgram, TestLawEngine, TestInterpreter>
    {
        accept_authority_with_genesis(
            catalog,
            deployment_byte,
            genesis_policy(catalog, deployment_byte),
        )
    }

    fn accept_authority_with_genesis(
        catalog: &ProjectCatalog,
        deployment_byte: u8,
        genesis: GenesisPolicyBinding,
    ) -> CatalogCommitAuthority<RustCryptoSha256, AcceptProgram, TestLawEngine, TestInterpreter>
    {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(deployment_byte),
            genesis,
            transition_limits(),
            &provider,
            verified_laws(catalog),
            AcceptProgram,
        )
        .unwrap_or_else(|error| panic!("commit authority: {error}"))
    }

    fn violating_authority(
        catalog: &ProjectCatalog,
    ) -> CatalogCommitAuthority<RustCryptoSha256, AcceptProgram, ViolatingLawEngine, TestInterpreter>
    {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(53),
            genesis_policy(catalog, 53),
            transition_limits(),
            &provider,
            violating_laws(catalog),
            AcceptProgram,
        )
        .unwrap_or_else(|error| panic!("commit authority: {error}"))
    }

    fn reject_authority(
        catalog: &ProjectCatalog,
    ) -> CatalogCommitAuthority<RustCryptoSha256, RejectProgram, TestLawEngine, TestInterpreter>
    {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(53),
            genesis_policy(catalog, 53),
            transition_limits(),
            &provider,
            verified_laws(catalog),
            RejectProgram,
        )
        .unwrap_or_else(|error| panic!("reject authority: {error}"))
    }

    fn wrong_limits_authority(
        catalog: &ProjectCatalog,
    ) -> CatalogCommitAuthority<RustCryptoSha256, WrongLimitsProgram, TestLawEngine, TestInterpreter>
    {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            execution(53),
            genesis_policy(catalog, 53),
            transition_limits(),
            &provider,
            verified_laws(catalog),
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

    fn admit<P, L>(
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, TestInterpreter>,
        catalog: &ProjectCatalog,
        principal: u8,
        replay: u8,
    ) -> InvocationWitness<RustCryptoSha256, P, L, TestInterpreter>
    where
        P: CatalogTransitionProgram<RustCryptoSha256>,
        L: ProjectLawEngine,
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
        authority: &CatalogCommitAuthority<
            RustCryptoSha256,
            AcceptProgram,
            TestLawEngine,
            TestInterpreter,
        >,
        catalog: &ProjectCatalog,
        principal: u8,
        replay: u8,
    ) -> CatalogAuthorizedTransition<RustCryptoSha256, AcceptProgram, TestLawEngine, TestInterpreter>
    {
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
        let genesis = authority
            .authorize_genesis(root(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        let shell = AuthorizedShellState::new(&authority, genesis)
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
        assert_eq!(
            base.body().law_set_hash(),
            authority.policy().law_set_hash()
        );
        assert_eq!(
            base.body().law_evaluation_hash(),
            base.law_evaluation().evaluation_hash()
        );
    }

    #[test]
    fn violated_project_law_cannot_mint_commit_authority() {
        let catalog = fixture_catalog();
        let authority = violating_authority(&catalog);
        let error = authority
            .execute(admit(&authority, &catalog, 60, 62))
            .err()
            .unwrap_or_else(|| panic!("violated law must fail"));
        assert!(matches!(
            error,
            CatalogExecutionError::Authority(AuthorityError::Laws(
                LawError::LawNotSatisfied {
                    law_id,
                    status: LawStatus::Violated,
                    ..
                }
            )) if law_id == semantic_id(1_001)
        ));
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
        let genesis = first_authority
            .authorize_genesis(root(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        let shell = AuthorizedShellState::new(&first_authority, genesis)
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

    #[test]
    fn authority_rejects_transition_program_build_mismatch() {
        let catalog = fixture_catalog();
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("approved provider: {error}"));
        let wrong_execution =
            ExecutionBinding::try_new(hash(49), hash(51), hash(52), hash(53), hash(54))
                .unwrap_or_else(|error| panic!("execution binding: {error}"));
        let result = CatalogCommitAuthority::<
            RustCryptoSha256,
            AcceptProgram,
            TestLawEngine,
            TestInterpreter,
        >::try_new(
            &catalog,
            StateDomainBinding::try_new("authority/fixture/state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            wrong_execution,
            genesis_policy(&catalog, 53),
            transition_limits(),
            &provider,
            verified_laws(&catalog),
            AcceptProgram,
        );
        assert!(matches!(
            result,
            Err(AuthorityError::Mismatch(AuthorityField::TransitionBuild))
        ));
    }

    #[test]
    fn schema_valid_wrong_root_cannot_mint_genesis_authority() {
        let catalog = fixture_catalog();
        let authority = accept_authority(&catalog, 53);
        let other = SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            Value::Bool(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("other root: {error}"));
        assert!(matches!(
            authority.authorize_genesis(other),
            Err(AuthorityError::Mismatch(AuthorityField::GenesisRoot))
        ));
    }

    #[test]
    fn every_genesis_policy_field_changes_policy_and_genesis_identity() {
        let catalog = fixture_catalog();
        let base = accept_authority(&catalog, 53);
        let base_genesis = base
            .authorize_genesis(root(&catalog))
            .unwrap_or_else(|error| panic!("base genesis: {error}"));
        let initial_root = base.policy().genesis().expected_initial_root();
        let variants = [
            GenesisPolicyBinding::try_new(initial_root, hash(73), hash(71), hash(72), hash(53)),
            GenesisPolicyBinding::try_new(initial_root, hash(70), hash(73), hash(72), hash(53)),
            GenesisPolicyBinding::try_new(initial_root, hash(70), hash(71), hash(73), hash(53)),
            GenesisPolicyBinding::try_new(initial_root, hash(70), hash(71), hash(72), hash(73)),
        ];

        for variant in variants {
            let authority = accept_authority_with_genesis(
                &catalog,
                53,
                variant.unwrap_or_else(|error| panic!("variant genesis policy: {error}")),
            );
            let genesis = authority
                .authorize_genesis(root(&catalog))
                .unwrap_or_else(|error| panic!("variant genesis: {error}"));
            assert_ne!(authority.policy().policy_id(), base.policy().policy_id());
            assert_ne!(genesis.genesis_id(), base_genesis.genesis_id());
        }
    }

    #[test]
    fn violated_genesis_law_cannot_initialize_authorized_shell() {
        let catalog = fixture_catalog();
        let authority = violating_authority(&catalog);
        let error = authority
            .authorize_genesis(root(&catalog))
            .err()
            .unwrap_or_else(|| panic!("violated genesis law must fail"));
        assert!(matches!(
            error,
            AuthorityError::Laws(LawError::LawNotSatisfied {
                law_id,
                status: LawStatus::Violated,
                ..
            }) if law_id == semantic_id(1_001)
        ));
    }
}
