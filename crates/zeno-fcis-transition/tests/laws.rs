//! Cross-layer laws for catalog-aware pure transition construction.

use std::string::String;

use zeno_fcis_catalog::{
    CatalogError, CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition,
    HashRequirement, ProjectCatalog, ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::{CommitmentHasher, Domain, Hash32};
use zeno_fcis_compose::{AccessPath, PathAtom};
use zeno_fcis_core::{Budget, BudgetLimits, Decision, Resource};
use zeno_fcis_patch::{PatchError, PathSegment, ValuePath, value_at};
use zeno_fcis_plan::{Effect, OutboxEntry, PlanError};
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::{
    FieldDef, FieldId, Schema, SchemaLimits, TypeDef, TypeId, TypeKind, ValueValidationError,
};
use zeno_fcis_transition::{
    ArtifactField, CataloguedTransitionBuilder, ExpectedInvocationBindings, LimitKind,
    MAX_TRANSITION_MAP_KEY_BYTES, MAX_TRANSITION_OBSERVED_PATHS, MAX_TRANSITION_PATCH_OPERATIONS,
    MAX_TRANSITION_REASONS, MAX_TRANSITION_STATE_DEPTH, MAX_TRANSITION_STATE_NODES,
    TransitionError, TransitionLimits, validate_transition_decision,
};
use zeno_fcis_value::{Field, Value};

#[derive(Clone, Copy, Debug)]
struct TestHasher;

impl CommitmentHasher for TestHasher {
    const ALGORITHM_ID: &'static str = "test/transition/1";

    fn hash(bytes: &[u8]) -> Hash32 {
        let mut output = [0_u8; 32];
        for (index, byte) in bytes.iter().copied().enumerate() {
            let slot = index % output.len();
            output[slot] = output[slot]
                .wrapping_add(byte)
                .rotate_left((index % 7) as u32);
        }
        if output == [0_u8; 32] {
            output[0] = 1;
        }
        Hash32::new(output)
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn name(value: &str) -> StableName {
    StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
}

fn id(value: u32) -> SemanticId {
    SemanticId::try_new(value).unwrap_or_else(|error| panic!("id: {error}"))
}

fn type_def(raw_id: u32, label: &str, kind: TypeKind) -> TypeDef {
    TypeDef::try_new(TypeId::new(raw_id), label, kind, SchemaLimits::default())
        .unwrap_or_else(|error| panic!("type: {error}"))
}

fn field_def(raw_id: u16, label: &str, type_id: u32) -> FieldDef {
    FieldDef::try_new(FieldId::new(raw_id), label, TypeId::new(type_id))
        .unwrap_or_else(|error| panic!("field: {error}"))
}

fn schema() -> Schema {
    Schema::try_new(
        "TransitionFixture",
        1,
        TypeId::new(1),
        vec![
            type_def(
                1,
                "State",
                TypeKind::Record {
                    fields: vec![field_def(1, "amount", 4), field_def(2, "enabled", 5)]
                        .into_boxed_slice(),
                },
            ),
            type_def(2, "Command", TypeKind::Bool),
            type_def(3, "Context", TypeKind::Bool),
            type_def(4, "Amount", TypeKind::U128 { min: 0, max: 100 }),
            type_def(5, "Flag", TypeKind::Bool),
            type_def(
                6,
                "Destination",
                TypeKind::Text {
                    min_len: 1,
                    max_len: 32,
                },
            ),
            type_def(7, "Notification", TypeKind::Bool),
        ],
        SchemaLimits::default(),
    )
    .unwrap_or_else(|error| panic!("schema: {error}"))
}

fn manifest() -> CatalogManifest {
    let reasons = vec![
        ReasonDefinition::try_new(
            id(11),
            name("late-denied"),
            ReasonDisposition::Reject,
            2,
            hash(11),
        )
        .unwrap_or_else(|error| panic!("reason: {error}")),
        ReasonDefinition::try_new(
            id(12),
            name("committed-denial"),
            ReasonDisposition::CommittedFailure,
            1,
            hash(12),
        )
        .unwrap_or_else(|error| panic!("reason: {error}")),
        ReasonDefinition::try_new(
            id(10),
            name("denied"),
            ReasonDisposition::Reject,
            0,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("reason: {error}")),
    ];
    let effects = vec![
        EffectDefinition::try_new(
            id(21),
            name("audit"),
            TypeId::new(4),
            HashRequirement::Present,
            HashRequirement::Absent,
            hash(21),
        )
        .unwrap_or_else(|error| panic!("effect: {error}")),
        EffectDefinition::try_new(
            id(20),
            name("write"),
            TypeId::new(4),
            HashRequirement::Present,
            HashRequirement::Absent,
            hash(20),
        )
        .unwrap_or_else(|error| panic!("effect: {error}")),
    ];
    let channels = vec![
        ChannelDefinition::try_new(
            id(31),
            name("audit-channel"),
            TypeId::new(6),
            TypeId::new(7),
            hash(31),
        )
        .unwrap_or_else(|error| panic!("channel: {error}")),
        ChannelDefinition::try_new(
            id(30),
            name("notify"),
            TypeId::new(6),
            TypeId::new(7),
            hash(30),
        )
        .unwrap_or_else(|error| panic!("channel: {error}")),
    ];
    CatalogManifest::try_new::<TestHasher>(reasons, effects, channels)
        .unwrap_or_else(|error| panic!("manifest: {error}"))
}

fn registry_entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
    RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
        .unwrap_or_else(|error| panic!("registry entry: {error}"))
}

fn catalog() -> ProjectCatalog {
    let schema = schema();
    let manifest = manifest();
    let mut entries = vec![
        registry_entry(RegistryKind::StateType, 1, "state", 1),
        registry_entry(RegistryKind::CommandType, 2, "command", 2),
        registry_entry(RegistryKind::ContextType, 3, "context", 3),
    ];
    entries.extend_from_slice(manifest.registry_entries());
    let profile = ProjectProfile::try_new(
        name("example"),
        name("transition"),
        id(100),
        1,
        id(1),
        id(2),
        id(3),
        DomainPrefix::try_new("example/transition")
            .unwrap_or_else(|error| panic!("domain prefix: {error}")),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<TestHasher>()
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
    let limits = CatalogLimits::try_new(8, 8, 16, 128, 512, 1_024, 32)
        .unwrap_or_else(|error| panic!("catalog limits: {error}"));
    ProjectCatalog::try_new::<TestHasher>(profile, schema, manifest, limits)
        .unwrap_or_else(|error| panic!("catalog: {error}"))
}

fn state(amount: u128, enabled: bool) -> Value {
    Value::record_canonical(vec![
        Field::new(1, Value::U128(amount)),
        Field::new(2, Value::Bool(enabled)),
    ])
    .unwrap_or_else(|error| panic!("state: {error}"))
}

fn state_domain() -> Domain<'static> {
    Domain::new("example/transition/state", 1)
        .unwrap_or_else(|error| panic!("state domain: {error}"))
}

fn budget_used() -> zeno_fcis_core::BudgetUsed {
    let limits = BudgetLimits::zero()
        .with_limit(Resource::Read, 8)
        .with_limit(Resource::Write, 8)
        .with_limit(Resource::Effect, 8)
        .with_limit(Resource::Byte, 256);
    let mut budget = Budget::new(limits);
    budget
        .charge(Resource::Read, 2)
        .unwrap_or_else(|error| panic!("read budget: {error}"));
    budget
        .charge(Resource::Write, 1)
        .unwrap_or_else(|error| panic!("write budget: {error}"));
    budget
        .charge(Resource::Effect, 1)
        .unwrap_or_else(|error| panic!("effect budget: {error}"));
    budget.used()
}

fn builder<'a>(
    catalog: &'a ProjectCatalog,
    state: &'a Value,
) -> CataloguedTransitionBuilder<'a, TestHasher> {
    CataloguedTransitionBuilder::try_new(
        catalog,
        state,
        state_domain(),
        hash(80),
        hash(81),
        budget_used(),
        TransitionLimits::default(),
    )
    .unwrap_or_else(|error| panic!("builder: {error}"))
}

fn expected_invocation() -> ExpectedInvocationBindings {
    ExpectedInvocationBindings::try_new(hash(80), hash(81))
        .unwrap_or_else(|error| panic!("expected invocation: {error}"))
}

fn field_path(raw_id: u16) -> ValuePath {
    ValuePath::new(vec![PathSegment::Field(raw_id)])
}

fn effect(ordinal: u32, operation: u32, amount: u128) -> Effect {
    Effect::new(
        ordinal,
        operation,
        hash(90),
        Hash32::ZERO,
        Value::U128(amount),
    )
}

fn text(value: &str) -> Value {
    Value::text_ascii(String::from(value)).unwrap_or_else(|error| panic!("text: {error}"))
}

fn outbox(ordinal: u32, channel: u32, destination: &str, payload: bool) -> OutboxEntry {
    OutboxEntry::new(ordinal, channel, text(destination), Value::Bool(payload))
}

#[test]
fn accepted_transition_binds_catalog_plans_footprint_budget_and_successor() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let mut transition = builder(&catalog, &pre_state);
    assert_eq!(transition.read(field_path(1)), Ok(&Value::U128(7)));
    transition
        .update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("update: {error}"));
    transition
        .observe_context(
            AccessPath::try_new(3, Vec::new())
                .unwrap_or_else(|error| panic!("context path: {error}")),
        )
        .unwrap_or_else(|error| panic!("context: {error}"));
    transition
        .emit(effect(1, 20, 8))
        .unwrap_or_else(|error| panic!("effect: {error}"));
    transition
        .enqueue(outbox(1, 30, "audit", true))
        .unwrap_or_else(|error| panic!("outbox: {error}"));

    let decision = transition
        .seal()
        .unwrap_or_else(|error| panic!("seal: {error}"));
    validate_transition_decision::<TestHasher>(
        &decision,
        &catalog,
        expected_invocation(),
        &pre_state,
        state_domain(),
    )
    .unwrap_or_else(|error| panic!("validate: {error}"));

    let Decision::Accept(accepted) = decision else {
        panic!("expected acceptance");
    };
    let artifacts = accepted.candidate();
    assert_eq!(artifacts.catalog_metrics().effects(), 1);
    assert_eq!(artifacts.catalog_metrics().outbox_entries(), 1);
    assert_eq!(artifacts.resources().budget_used(), budget_used());
    assert_eq!(artifacts.footprint().reads().paths().len(), 1);
    assert_eq!(artifacts.footprint().writes().paths().len(), 1);
    assert_eq!(artifacts.footprint().contexts().paths().len(), 1);
    assert_eq!(artifacts.footprint().effects().paths().len(), 1);
    let applied = artifacts
        .bundle()
        .validate_and_apply::<TestHasher>(&pre_state, state_domain())
        .unwrap_or_else(|error| panic!("apply: {error}"));
    assert_eq!(
        value_at(applied.state(), &field_path(1))
            .unwrap_or_else(|error| panic!("updated value: {error}")),
        &Value::U128(8)
    );
}

#[test]
fn staging_order_does_not_change_the_complete_decision() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let mut left = builder(&catalog, &pre_state);
    left.update(field_path(2), Value::Bool(true))
        .unwrap_or_else(|error| panic!("left update two: {error}"));
    left.update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("left update one: {error}"));
    left.emit(effect(2, 21, 2))
        .unwrap_or_else(|error| panic!("left effect two: {error}"));
    left.emit(effect(1, 20, 1))
        .unwrap_or_else(|error| panic!("left effect one: {error}"));
    left.enqueue(outbox(2, 31, "second", false))
        .unwrap_or_else(|error| panic!("left outbox two: {error}"));
    left.enqueue(outbox(1, 30, "first", true))
        .unwrap_or_else(|error| panic!("left outbox one: {error}"));

    let mut right = builder(&catalog, &pre_state);
    right
        .enqueue(outbox(1, 30, "first", true))
        .unwrap_or_else(|error| panic!("right outbox one: {error}"));
    right
        .enqueue(outbox(2, 31, "second", false))
        .unwrap_or_else(|error| panic!("right outbox two: {error}"));
    right
        .emit(effect(1, 20, 1))
        .unwrap_or_else(|error| panic!("right effect one: {error}"));
    right
        .emit(effect(2, 21, 2))
        .unwrap_or_else(|error| panic!("right effect two: {error}"));
    right
        .update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("right update one: {error}"));
    right
        .update(field_path(2), Value::Bool(true))
        .unwrap_or_else(|error| panic!("right update two: {error}"));

    assert_eq!(left.seal(), right.seal());
}

#[test]
fn total_reason_precedence_is_independent_of_call_order_and_rejects_without_a_candidate() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let mut left = builder(&catalog, &pre_state);
    left.update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("update: {error}"));
    left.emit(effect(1, 20, 8))
        .unwrap_or_else(|error| panic!("effect: {error}"));
    left.require(false, id(11))
        .unwrap_or_else(|error| panic!("late rejection: {error}"));
    left.fail_if(true, id(12))
        .unwrap_or_else(|error| panic!("committed failure: {error}"));
    left.require(false, id(10))
        .unwrap_or_else(|error| panic!("early rejection: {error}"));

    let mut right = builder(&catalog, &pre_state);
    right
        .require(false, id(10))
        .unwrap_or_else(|error| panic!("early rejection: {error}"));
    right
        .fail_if(true, id(12))
        .unwrap_or_else(|error| panic!("committed failure: {error}"));
    right
        .require(false, id(11))
        .unwrap_or_else(|error| panic!("late rejection: {error}"));
    right
        .emit(effect(1, 20, 8))
        .unwrap_or_else(|error| panic!("effect: {error}"));
    right
        .update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("update: {error}"));

    let left = left
        .seal()
        .unwrap_or_else(|error| panic!("left seal: {error}"));
    let right = right
        .seal()
        .unwrap_or_else(|error| panic!("right seal: {error}"));
    assert_eq!(left, right);
    let Decision::Reject(rejected) = left else {
        panic!("expected ordinary rejection");
    };
    assert_eq!(rejected.reason().reason_id(), id(10));
    assert_eq!(rejected.reason().footprint().reads().paths().len(), 1);
    assert!(rejected.reason().footprint().writes().paths().is_empty());
    assert!(rejected.reason().footprint().effects().paths().is_empty());
    assert_eq!(
        rejected.reason().receipt().pre_root(),
        rejected.reason().receipt().post_root()
    );
}

#[test]
fn committed_failure_carries_a_complete_catalogued_candidate() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let mut transition = builder(&catalog, &pre_state);
    transition
        .update(field_path(2), Value::Bool(true))
        .unwrap_or_else(|error| panic!("update: {error}"));
    transition
        .fail_if(true, id(12))
        .unwrap_or_else(|error| panic!("failure: {error}"));
    let decision = transition
        .seal()
        .unwrap_or_else(|error| panic!("seal: {error}"));
    validate_transition_decision::<TestHasher>(
        &decision,
        &catalog,
        expected_invocation(),
        &pre_state,
        state_domain(),
    )
    .unwrap_or_else(|error| panic!("validate: {error}"));
    let Decision::CommittedFailure(failed) = decision else {
        panic!("expected committed failure");
    };
    assert_eq!(*failed.reason(), id(12));
    assert_eq!(failed.candidate().reason_id(), Some(id(12)));
    assert_eq!(
        failed.candidate().bundle().body().post_root(),
        failed.candidate().bundle().receipt().body().post_root()
    );
}

#[test]
fn wrong_reason_disposition_fails_before_sealing() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let mut transition = builder(&catalog, &pre_state);
    assert!(matches!(
        transition.require(false, id(12)),
        Err(TransitionError::Catalog(
            CatalogError::ReasonDispositionMismatch { .. }
        ))
    ));
    assert!(matches!(
        transition.fail_if(true, id(10)),
        Err(TransitionError::Catalog(
            CatalogError::ReasonDispositionMismatch { .. }
        ))
    ));
}

#[test]
fn unknown_effect_wrong_payload_and_duplicate_ordinal_fail_closed() {
    let catalog = catalog();
    let pre_state = state(7, false);

    let mut unknown = builder(&catalog, &pre_state);
    unknown
        .emit(effect(1, 999, 1))
        .unwrap_or_else(|error| panic!("stage unknown: {error}"));
    assert_eq!(
        unknown.seal(),
        Err(TransitionError::Catalog(CatalogError::UnknownEffect(999)))
    );

    let mut wrong_payload = builder(&catalog, &pre_state);
    wrong_payload
        .emit(Effect::new(
            1,
            20,
            hash(90),
            Hash32::ZERO,
            Value::Bool(true),
        ))
        .unwrap_or_else(|error| panic!("stage wrong payload: {error}"));
    assert!(matches!(
        wrong_payload.seal(),
        Err(TransitionError::Catalog(_))
    ));

    let mut duplicate = builder(&catalog, &pre_state);
    duplicate
        .emit(effect(1, 20, 1))
        .unwrap_or_else(|error| panic!("first duplicate: {error}"));
    duplicate
        .emit(effect(1, 21, 2))
        .unwrap_or_else(|error| panic!("second duplicate: {error}"));
    assert_eq!(
        duplicate.seal(),
        Err(TransitionError::Plan(PlanError::DuplicateEffectOrdinal(1)))
    );
}

#[test]
fn overlapping_patch_and_invalid_successor_schema_fail_closed() {
    let catalog = catalog();
    let pre_state = state(7, false);

    let mut overlap = builder(&catalog, &pre_state);
    overlap
        .update(ValuePath::new(Vec::new()), state(8, true))
        .unwrap_or_else(|error| panic!("root update: {error}"));
    overlap
        .update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("field update: {error}"));
    assert_eq!(
        overlap.seal(),
        Err(TransitionError::Patch(PatchError::OverlappingPaths))
    );

    let mut invalid = builder(&catalog, &pre_state);
    invalid
        .update(field_path(1), Value::U128(101))
        .unwrap_or_else(|error| panic!("invalid update: {error}"));
    assert_eq!(
        invalid.seal(),
        Err(TransitionError::Schema(ValueValidationError::IntegerRange))
    );
}

#[test]
fn invalid_pre_state_stale_replay_and_invalid_context_paths_fail_closed() {
    let catalog = catalog();
    assert!(matches!(
        CataloguedTransitionBuilder::<TestHasher>::try_new(
            &catalog,
            &Value::Bool(false),
            state_domain(),
            hash(80),
            hash(81),
            budget_used(),
            TransitionLimits::default(),
        ),
        Err(TransitionError::Schema(ValueValidationError::TypeMismatch))
    ));

    let pre_state = state(7, false);
    let mut transition = builder(&catalog, &pre_state);
    transition
        .update(field_path(1), Value::U128(8))
        .unwrap_or_else(|error| panic!("update: {error}"));
    let decision = transition
        .seal()
        .unwrap_or_else(|error| panic!("seal: {error}"));
    assert!(
        validate_transition_decision::<TestHasher>(
            &decision,
            &catalog,
            expected_invocation(),
            &state(9, false),
            state_domain(),
        )
        .is_err()
    );
    let substituted_command = ExpectedInvocationBindings::try_new(hash(82), hash(81))
        .unwrap_or_else(|error| panic!("substituted command: {error}"));
    assert_eq!(
        validate_transition_decision::<TestHasher>(
            &decision,
            &catalog,
            substituted_command,
            &pre_state,
            state_domain(),
        ),
        Err(TransitionError::ArtifactMismatch(
            ArtifactField::CandidateBindings
        ))
    );
    let substituted_context = ExpectedInvocationBindings::try_new(hash(80), hash(82))
        .unwrap_or_else(|error| panic!("substituted context: {error}"));
    assert_eq!(
        validate_transition_decision::<TestHasher>(
            &decision,
            &catalog,
            substituted_context,
            &pre_state,
            state_domain(),
        ),
        Err(TransitionError::ArtifactMismatch(
            ArtifactField::CandidateBindings
        ))
    );

    let mut wrong_namespace = builder(&catalog, &pre_state);
    let wrong_path =
        AccessPath::try_new(4, Vec::new()).unwrap_or_else(|error| panic!("wrong path: {error}"));
    assert!(matches!(
        wrong_namespace.observe_context(wrong_path),
        Err(TransitionError::ContextNamespaceMismatch {
            expected: 3,
            actual: 4,
        })
    ));

    let mut wildcard = builder(&catalog, &pre_state);
    let wildcard_path = AccessPath::try_new(3, vec![PathAtom::AnyDescendant])
        .unwrap_or_else(|error| panic!("wildcard path: {error}"));
    assert!(matches!(
        wildcard.observe_context(wildcard_path),
        Err(TransitionError::ObservedWildcard)
    ));
}

#[test]
fn exact_transition_limits_pass_and_one_over_limits_fail_without_output() {
    assert!(
        TransitionLimits::try_new(
            MAX_TRANSITION_PATCH_OPERATIONS,
            MAX_TRANSITION_OBSERVED_PATHS,
            MAX_TRANSITION_REASONS,
            MAX_TRANSITION_MAP_KEY_BYTES,
            MAX_TRANSITION_STATE_DEPTH,
            MAX_TRANSITION_STATE_NODES,
        )
        .is_ok()
    );
    assert_eq!(
        TransitionLimits::try_new(
            MAX_TRANSITION_PATCH_OPERATIONS + 1,
            MAX_TRANSITION_OBSERVED_PATHS,
            MAX_TRANSITION_REASONS,
            MAX_TRANSITION_MAP_KEY_BYTES,
            MAX_TRANSITION_STATE_DEPTH,
            MAX_TRANSITION_STATE_NODES,
        ),
        Err(TransitionError::InvalidLimits)
    );

    let catalog = catalog();
    let pre_state = state(7, false);
    let limits = TransitionLimits::try_new(4, 1, 4, 1_024, 16, 64)
        .unwrap_or_else(|error| panic!("limits: {error}"));
    let mut transition = CataloguedTransitionBuilder::<TestHasher>::try_new(
        &catalog,
        &pre_state,
        state_domain(),
        hash(80),
        hash(81),
        budget_used(),
        limits,
    )
    .unwrap_or_else(|error| panic!("builder: {error}"));
    transition
        .read(field_path(1))
        .unwrap_or_else(|error| panic!("boundary read: {error}"));
    assert_eq!(
        transition.read(field_path(2)),
        Err(TransitionError::LimitExceeded {
            kind: LimitKind::Reads,
            limit: 1,
            attempted: 2,
        })
    );
}

#[test]
fn repeated_applicable_reason_is_idempotent_at_the_exact_reason_bound() {
    let catalog = catalog();
    let pre_state = state(7, false);
    let limits = TransitionLimits::try_new(4, 4, 1, 1_024, 16, 64)
        .unwrap_or_else(|error| panic!("limits: {error}"));
    let mut transition = CataloguedTransitionBuilder::<TestHasher>::try_new(
        &catalog,
        &pre_state,
        state_domain(),
        hash(80),
        hash(81),
        budget_used(),
        limits,
    )
    .unwrap_or_else(|error| panic!("builder: {error}"));
    transition
        .require(false, id(10))
        .unwrap_or_else(|error| panic!("first reason: {error}"));
    transition
        .require(false, id(10))
        .unwrap_or_else(|error| panic!("repeated reason: {error}"));
    let decision = transition
        .seal()
        .unwrap_or_else(|error| panic!("seal: {error}"));
    assert!(matches!(decision, Decision::Reject(_)));
}
