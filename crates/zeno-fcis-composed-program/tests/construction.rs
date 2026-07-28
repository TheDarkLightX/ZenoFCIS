//! Public construction, identity-binding, and fail-closed projection tests.

use core::fmt::Debug;

use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, EffectDefinition, HashRequirement, ProjectCatalog,
    ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_compose::{
    AccessPath, ComponentContract, ComponentId, CompositionSpec, Footprint, PathSet,
};
use zeno_fcis_composed_program::{
    ComposedDomainProgram, ComposedProgramError, ExternalOutput, ProjectionPlan,
    derive_semantic_program_hash,
};
use zeno_fcis_core::{
    Accepted, Budget, BudgetLimits, BudgetedDecision, Decision, Rejected, Resource,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_domain::{
    DomainMachine, EnvelopeBinding, ExecutableComposition, MachineCandidate, MachineInterface,
    TypedPathBinding,
};
use zeno_fcis_patch::{PathSegment, ValuePath};
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::{
    Schema, SchemaAdmittedTypeEnvelope, SchemaLimits, TypeDef, TypeId, TypeKind, ValidationLimits,
};
use zeno_fcis_value::Value;

const COMPONENT: ComponentId = ComponentId::new(1);
const STATE_TYPE: TypeId = TypeId::new(1);
const COMMAND_TYPE: TypeId = TypeId::new(2);
const CONTEXT_TYPE: TypeId = TypeId::new(3);
const PAYLOAD_TYPE: TypeId = TypeId::new(4);

fn must<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn id(value: u32) -> SemanticId {
    must(SemanticId::try_new(value))
}

fn name(value: &str) -> StableName {
    must(StableName::try_new(value))
}

fn path(namespace: u32) -> AccessPath {
    must(AccessPath::try_new(namespace, Vec::new()))
}

fn set(paths: Vec<AccessPath>) -> PathSet {
    must(PathSet::try_new(paths))
}

fn schema() -> Schema {
    let types = [
        (STATE_TYPE, "State"),
        (COMMAND_TYPE, "Command"),
        (CONTEXT_TYPE, "Context"),
        (PAYLOAD_TYPE, "Payload"),
    ]
    .map(|(type_id, label)| {
        must(TypeDef::try_new(
            type_id,
            label,
            TypeKind::Bool,
            SchemaLimits::default(),
        ))
    });
    must(Schema::try_new(
        "ComposedProgramFixture",
        1,
        STATE_TYPE,
        types.into_iter().collect(),
        SchemaLimits::default(),
    ))
}

fn manifest() -> CatalogManifest {
    manifest_with_requirements(HashRequirement::Any, HashRequirement::Any)
}

fn manifest_with_requirements(
    authority: HashRequirement,
    subject: HashRequirement,
) -> CatalogManifest {
    let reasons = vec![
        must(ReasonDefinition::try_new(
            id(10),
            name("machine-reject"),
            ReasonDisposition::Reject,
            0,
            hash(10),
        )),
        must(ReasonDefinition::try_new(
            id(11),
            name("machine-failed"),
            ReasonDisposition::CommittedFailure,
            1,
            hash(11),
        )),
    ];
    let effects = vec![must(EffectDefinition::try_new(
        id(20),
        name("publish"),
        PAYLOAD_TYPE,
        authority,
        subject,
        hash(20),
    ))];
    must(CatalogManifest::try_new::<RustCryptoSha256>(
        reasons,
        effects,
        Vec::new(),
    ))
}

fn executable(schema: &Schema) -> ExecutableComposition<1, 1, 1> {
    let schema_hash = must(schema.schema_hash::<RustCryptoSha256>());
    let state = path(STATE_TYPE.get());
    let context = path(CONTEXT_TYPE.get());
    let output = path(20);
    let contract = must(ComponentContract::try_new(
        COMPONENT,
        hash(40),
        Footprint::new(
            set(vec![state.clone()]),
            set(vec![state.clone()]),
            set(vec![context.clone()]),
            set(vec![output.clone()]),
        ),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ));
    let spec = must(CompositionSpec::try_new(
        1,
        vec![contract],
        Vec::new(),
        Vec::new(),
        vec![COMPONENT],
    ));
    let interface = must(MachineInterface::try_new(
        COMPONENT,
        hash(40),
        must(EnvelopeBinding::try_new(COMMAND_TYPE, schema_hash)),
        must(TypedPathBinding::try_new(
            context,
            must(EnvelopeBinding::try_new(CONTEXT_TYPE, schema_hash)),
        )),
        [must(TypedPathBinding::try_new(
            state,
            must(EnvelopeBinding::try_new(STATE_TYPE, schema_hash)),
        ))],
        [None],
        [Some(must(TypedPathBinding::try_new(
            output,
            must(EnvelopeBinding::try_new(PAYLOAD_TYPE, schema_hash)),
        )))],
    ));
    must(ExecutableComposition::try_new(spec, [interface]))
}

fn projection(output: ExternalOutput) -> ProjectionPlan<1, 1, 1> {
    must(ProjectionPlan::try_new(
        [[ValuePath::new(Vec::new())]],
        [ValuePath::new(Vec::new())],
        [ValuePath::new(Vec::new())],
        [vec![id(10), id(11)]],
        [[output]],
    ))
}

fn machine_limits() -> [BudgetLimits; 1] {
    [BudgetLimits::zero()
        .with_limit(Resource::Read, 1)
        .with_limit(Resource::Write, 1)
        .with_limit(Resource::Effect, 1)]
}

fn projection_limits() -> BudgetLimits {
    BudgetLimits::zero()
        .with_limit(Resource::Read, 3)
        .with_limit(Resource::Write, 1)
        .with_limit(Resource::Effect, 1)
        .with_limit(Resource::Byte, 4_096)
}

fn root_entry(kind: RegistryKind, id_value: u32, label: &str) -> RegistryEntry {
    must(RegistryEntry::try_new(
        kind,
        id(id_value),
        name(label),
        hash(id_value as u8),
    ))
}

fn catalog(schema: &Schema, manifest: CatalogManifest, algorithm_hash: Hash32) -> ProjectCatalog {
    let mut registry = vec![
        root_entry(RegistryKind::StateType, STATE_TYPE.get(), "state"),
        root_entry(RegistryKind::CommandType, COMMAND_TYPE.get(), "command"),
        root_entry(RegistryKind::ContextType, CONTEXT_TYPE.get(), "context"),
    ];
    registry.extend_from_slice(manifest.registry_entries());
    let profile = must(ProjectProfile::try_new(
        name("composed-fixture"),
        name("core"),
        id(100),
        1,
        id(STATE_TYPE.get()),
        id(COMMAND_TYPE.get()),
        id(CONTEXT_TYPE.get()),
        must(DomainPrefix::try_new("composed/fixture")),
        ProfileBindings {
            schema_hash: must(schema.schema_hash::<RustCryptoSha256>()),
            precedence_hash: manifest.precedence_hash(),
            algorithm_hash,
            codec_hash: hash(41),
            effect_registry_hash: manifest.effect_registry_hash(),
            channel_registry_hash: manifest.channel_registry_hash(),
            policy_hash: hash(42),
        },
        registry,
    ));
    must(ProjectCatalog::try_new::<RustCryptoSha256>(
        profile,
        schema.clone(),
        manifest,
        CatalogLimits::default(),
    ))
}

#[derive(Clone)]
struct FixtureMachine {
    output: SchemaAdmittedTypeEnvelope,
}

fn fixture_machine(schema: &Schema) -> FixtureMachine {
    FixtureMachine {
        output: must(SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
            schema,
            PAYLOAD_TYPE,
            Value::Bool(true),
            ValidationLimits::default(),
        )),
    }
}

impl DomainMachine<1, 1> for FixtureMachine {
    fn component_id(&self) -> ComponentId {
        COMPONENT
    }

    fn step(
        &self,
        state: &[zeno_fcis_schema::SchemaAdmittedTypeEnvelope; 1],
        _command: &zeno_fcis_schema::SchemaAdmittedTypeEnvelope,
        _context: &zeno_fcis_schema::SchemaAdmittedTypeEnvelope,
        _inputs: &[Option<zeno_fcis_schema::SchemaAdmittedTypeEnvelope>; 1],
        limits: BudgetLimits,
    ) -> BudgetedDecision<MachineCandidate<1, 1>, SemanticId, SemanticId> {
        let mut budget = Budget::new(limits);
        if budget.charge(Resource::Read, 1).is_err() {
            return budget.finish(Decision::Reject(Rejected::new(id(10))));
        }
        budget.finish(Decision::Accept(Accepted::new(MachineCandidate::new(
            [state[0].clone()],
            [Some(self.output.clone())],
        ))))
    }
}

fn effect_output() -> ExternalOutput {
    ExternalOutput::Effect {
        operation: id(20),
        authority: Hash32::ZERO,
        subject: Hash32::ZERO,
    }
}

#[test]
fn exact_semantic_program_identity_is_constructible() {
    let schema = schema();
    let manifest = manifest();
    let executable = executable(&schema);
    let projection = projection(effect_output());
    let machine_hashes = [hash(70)];
    let algorithm_hash = must(derive_semantic_program_hash::<RustCryptoSha256, 1, 1, 1>(
        &executable,
        &projection,
        &machine_hashes,
        &machine_limits(),
        projection_limits(),
    ));
    let catalog = catalog(&schema, manifest, algorithm_hash);
    let program = must(
        ComposedDomainProgram::<RustCryptoSha256, _, 1, 1, 1>::try_new(
            &catalog,
            executable,
            [fixture_machine(&schema)],
            projection,
            machine_hashes,
            machine_limits(),
            projection_limits(),
        ),
    );
    assert_eq!(program.semantic_program_hash(), algorithm_hash);
    assert_eq!(program.machine_build_hashes(), &[hash(70)]);
}

#[test]
fn machine_identity_substitution_changes_the_required_program_hash() {
    let schema = schema();
    let manifest = manifest();
    let executable = executable(&schema);
    let projection = projection(effect_output());
    let approved = must(derive_semantic_program_hash::<RustCryptoSha256, 1, 1, 1>(
        &executable,
        &projection,
        &[hash(70)],
        &machine_limits(),
        projection_limits(),
    ));
    let catalog = catalog(&schema, manifest, approved);
    assert!(matches!(
        ComposedDomainProgram::<RustCryptoSha256, _, 1, 1, 1>::try_new(
            &catalog,
            executable,
            [fixture_machine(&schema)],
            projection,
            [hash(71)],
            machine_limits(),
            projection_limits(),
        ),
        Err(ComposedProgramError::AlgorithmBindingMismatch)
    ));
}

#[test]
fn routed_or_inactive_output_roles_cannot_replace_an_external_effect() {
    let schema = schema();
    let manifest = manifest();
    let executable = executable(&schema);
    let projection = projection(ExternalOutput::Internal);
    let algorithm_hash = must(derive_semantic_program_hash::<RustCryptoSha256, 1, 1, 1>(
        &executable,
        &projection,
        &[hash(70)],
        &machine_limits(),
        projection_limits(),
    ));
    let catalog = catalog(&schema, manifest, algorithm_hash);
    assert!(matches!(
        ComposedDomainProgram::<RustCryptoSha256, _, 1, 1, 1>::try_new(
            &catalog,
            executable,
            [fixture_machine(&schema)],
            projection,
            [hash(70)],
            machine_limits(),
            projection_limits(),
        ),
        Err(ComposedProgramError::OutputRuleMismatch {
            machine: 0,
            port: 0
        })
    ));
}

#[test]
fn fixed_effect_authority_must_satisfy_the_catalog() {
    let schema = schema();
    let manifest = manifest_with_requirements(HashRequirement::Present, HashRequirement::Any);
    let executable = executable(&schema);
    let projection = projection(effect_output());
    let algorithm_hash = must(derive_semantic_program_hash::<RustCryptoSha256, 1, 1, 1>(
        &executable,
        &projection,
        &[hash(70)],
        &machine_limits(),
        projection_limits(),
    ));
    let catalog = catalog(&schema, manifest, algorithm_hash);
    assert!(matches!(
        ComposedDomainProgram::<RustCryptoSha256, _, 1, 1, 1>::try_new(
            &catalog,
            executable,
            [fixture_machine(&schema)],
            projection,
            [hash(70)],
            machine_limits(),
            projection_limits(),
        ),
        Err(ComposedProgramError::OutputHashRequirementMismatch {
            machine: 0,
            port: 0
        })
    ));
}

#[test]
fn direct_projection_rejects_map_keys_and_overlapping_state() {
    let map_path = ValuePath::new(vec![PathSegment::MapKey(vec![1_u8].into_boxed_slice())]);
    assert!(matches!(
        ProjectionPlan::<1, 1, 1>::try_new(
            [[map_path]],
            [ValuePath::new(Vec::new())],
            [ValuePath::new(Vec::new())],
            [vec![id(10)]],
            [[effect_output()]],
        ),
        Err(ComposedProgramError::MapKeyProjectionUnsupported)
    ));

    assert!(matches!(
        ProjectionPlan::<1, 2, 0>::try_new(
            [[
                ValuePath::new(Vec::new()),
                ValuePath::new(vec![PathSegment::Field(1)]),
            ]],
            [ValuePath::new(Vec::new())],
            [ValuePath::new(Vec::new())],
            [vec![id(10)]],
            [[]],
        ),
        Err(ComposedProgramError::OverlappingRootStatePaths)
    ));
}

#[test]
fn duplicate_or_empty_machine_reason_domains_fail_closed() {
    assert!(matches!(
        ProjectionPlan::<1, 1, 0>::try_new(
            [[ValuePath::new(Vec::new())]],
            [ValuePath::new(Vec::new())],
            [ValuePath::new(Vec::new())],
            [Vec::new()],
            [[]],
        ),
        Err(ComposedProgramError::EmptyReasonDomain)
    ));
    assert!(matches!(
        ProjectionPlan::<1, 1, 0>::try_new(
            [[ValuePath::new(Vec::new())]],
            [ValuePath::new(Vec::new())],
            [ValuePath::new(Vec::new())],
            [vec![id(10), id(10)]],
            [[]],
        ),
        Err(ComposedProgramError::DuplicateMachineReason)
    ));
}
