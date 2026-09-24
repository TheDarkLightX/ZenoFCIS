//! Checks that a probed execution withholds the decision of a program that
//! behaves differently on repeated executions.
//!
//! The programs here keep state between calls on purpose. They live in an
//! integration test because the crate's own source may not contain interior
//! mutability (`tools/check_assurance.py` rejects it).

use std::cell::Cell;

use zeno_fcis_authority::{
    CatalogCommitAuthority, CatalogTransitionProgram, ExecutionBinding, GenesisPolicyBinding,
    InvocationWitness, ProbeError, ProbeRuns, ReviewedTransitionInput, StateDomainBinding,
};
use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, ProjectCatalog, ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::{Domain, Hash32};
use zeno_fcis_core::{BudgetUsed, DecisionKind};
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, GenesisLawCheckInput, LawCheckInput, LawDefinition,
    LawEngineFailure, LawEvidenceRequirement, LawEvidenceVerifier, LawFamilyPolicy, LawKind,
    LawLimits, LawManifest, LawObservation, LawProofDecision, LawProofSubject, LawStatus,
    ProjectLawEngine, VerifiedProjectLaws, verify_project_laws,
};
use zeno_fcis_patch::hash_value;
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::{
    Schema, SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, SchemaLimits, TypeDef, TypeId,
    TypeKind, ValidationLimits,
};
use zeno_fcis_transition::{
    CataloguedTransitionBuilder, TransitionDecision, TransitionError, TransitionLimits,
};
use zeno_fcis_value::Value;

/// How a test program misbehaves on its second and later calls.
#[derive(Clone, Copy, Debug)]
enum Behavior {
    /// Rejects on every second call and accepts otherwise.
    Alternate,
    /// Fails on every call after the first.
    FailAfterFirst,
}

#[derive(Debug)]
struct StatefulProgram {
    behavior: Behavior,
    calls: Cell<u32>,
}

#[derive(Debug)]
enum ProgramError {
    Transition(TransitionError),
    Planted,
}

impl std::fmt::Display for ProgramError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transition(error) => write!(formatter, "transition: {error}"),
            Self::Planted => formatter.write_str("planted failure after the first run"),
        }
    }
}

impl From<TransitionError> for ProgramError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

impl CatalogTransitionProgram<RustCryptoSha256> for StatefulProgram {
    type Error = ProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        hash(50)
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let calls = self.calls.get();
        self.calls.set(calls + 1);
        if matches!(self.behavior, Behavior::FailAfterFirst) && calls > 0 {
            return Err(ProgramError::Planted);
        }
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
        builder.require(calls.is_multiple_of(2), semantic_id(10))?;
        Ok(builder.seal()?)
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
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        match input.decision().kind() {
            DecisionKind::Reject => Ok(Vec::new()),
            DecisionKind::Accept | DecisionKind::CommittedFailure => Ok(vec![observation(1_001)]),
        }
    }

    fn evaluate_genesis(
        &self,
        _: &GenesisLawCheckInput<'_>,
        _: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        Ok(vec![observation(1_001)])
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

fn observation(law: u32) -> LawObservation {
    LawObservation::try_new(semantic_id(law), LawStatus::Satisfied, hash(91))
        .unwrap_or_else(|error| panic!("law observation: {error}"))
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
    let byte = u8::try_from(id).unwrap_or_else(|error| panic!("registry byte: {error}"));
    RegistryEntry::try_new(kind, semantic_id(id), stable_name(label), hash(byte))
        .unwrap_or_else(|error| panic!("registry entry: {error}"))
}

fn law_definition(
    id: u32,
    name: &str,
    kind: LawKind,
    scope: DecisionScope,
    genesis: GenesisApplicability,
    byte: u8,
) -> LawDefinition {
    LawDefinition::try_new(
        semantic_id(id),
        stable_name(name),
        kind,
        scope,
        genesis,
        hash(byte),
        hash(byte + 10),
        LawEvidenceRequirement::RuntimeOnly,
    )
    .unwrap_or_else(|error| panic!("law {name}: {error}"))
}

fn law_manifest() -> LawManifest {
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
    let not_at_genesis = |byte| GenesisApplicability::NotApplicable {
        rationale_hash: hash(byte),
    };
    let definitions = vec![
        law_definition(
            1_001,
            "state-invariant",
            LawKind::StateInvariant,
            DecisionScope::Committing,
            GenesisApplicability::Required,
            101,
        ),
        law_definition(
            1_002,
            "reject-no-authority",
            LawKind::RejectNoAuthority,
            DecisionScope::Reject,
            not_at_genesis(121),
            102,
        ),
        law_definition(
            1_003,
            "committed-failure-effects",
            LawKind::CommittedFailureEffects,
            DecisionScope::CommittedFailure,
            not_at_genesis(122),
            103,
        ),
    ];
    LawManifest::try_new(families, definitions)
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
    ProjectCatalog::try_new::<RustCryptoSha256>(profile, schema, manifest, CatalogLimits::default())
        .unwrap_or_else(|error| panic!("catalog: {error}"))
}

fn verified_laws(catalog: &ProjectCatalog) -> VerifiedProjectLaws<RustCryptoSha256, TestLawEngine> {
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

fn root(catalog: &ProjectCatalog) -> SchemaAdmittedEnvelope {
    SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
        catalog.schema(),
        Value::Bool(false),
        ValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("root envelope: {error}"))
}

fn typed(catalog: &ProjectCatalog, id: u32) -> SchemaAdmittedTypeEnvelope {
    SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
        catalog.schema(),
        TypeId::new(id),
        Value::Bool(true),
        ValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("typed envelope: {error}"))
}

type Authority =
    CatalogCommitAuthority<RustCryptoSha256, StatefulProgram, TestLawEngine, TestInterpreter>;

fn authority(catalog: &ProjectCatalog, behavior: Behavior) -> Authority {
    let provider = verify_approved_provider::<RustCryptoSha256>()
        .unwrap_or_else(|error| panic!("approved provider: {error}"));
    let domain =
        Domain::new("authority/fixture/state", 1).unwrap_or_else(|error| panic!("domain: {error}"));
    let initial_root = hash_value::<RustCryptoSha256>(domain, root(catalog).value().value())
        .unwrap_or_else(|error| panic!("initial root: {error}"));
    CatalogCommitAuthority::try_new(
        catalog,
        StateDomainBinding::try_new("authority/fixture/state", 1)
            .unwrap_or_else(|error| panic!("state domain: {error}")),
        ExecutionBinding::try_new(hash(50), hash(51), hash(52), hash(53), hash(54))
            .unwrap_or_else(|error| panic!("execution binding: {error}")),
        GenesisPolicyBinding::try_new(initial_root, hash(70), hash(71), hash(72), hash(53))
            .unwrap_or_else(|error| panic!("genesis policy: {error}")),
        TransitionLimits::try_new(4, 4, 4, 64, 8, 64)
            .unwrap_or_else(|error| panic!("transition limits: {error}")),
        &provider,
        verified_laws(catalog),
        StatefulProgram {
            behavior,
            calls: Cell::new(0),
        },
    )
    .unwrap_or_else(|error| panic!("authority: {error}"))
}

fn admit(
    authority: &Authority,
    catalog: &ProjectCatalog,
) -> InvocationWitness<RustCryptoSha256, StatefulProgram, TestLawEngine, TestInterpreter> {
    authority
        .admit_invocation(
            root(catalog),
            typed(catalog, 2),
            typed(catalog, 3),
            hash(60),
            hash(61),
            hash(62),
        )
        .unwrap_or_else(|error| panic!("invocation: {error}"))
}

fn runs(count: u8) -> ProbeRuns {
    ProbeRuns::try_new(count).unwrap_or_else(|| panic!("run count {count}"))
}

#[test]
fn probed_execution_withholds_a_decision_that_changes_between_runs() {
    let catalog = fixture_catalog();
    let authority = authority(&catalog, Behavior::Alternate);
    let Err(ProbeError::Diverged(divergence)) =
        authority.execute_probed(admit(&authority, &catalog), runs(4))
    else {
        panic!("an alternating program must diverge");
    };
    assert_eq!(divergence.run(), 1);
    assert_ne!(divergence.first(), divergence.other());
}

#[test]
fn probed_execution_withholds_a_decision_when_a_later_run_fails() {
    let catalog = fixture_catalog();
    let authority = authority(&catalog, Behavior::FailAfterFirst);
    let Err(ProbeError::FailedAfterDecision { run, .. }) =
        authority.execute_probed(admit(&authority, &catalog), runs(3))
    else {
        panic!("a program that fails after its first run must be withheld");
    };
    assert_eq!(run, 1);
}
