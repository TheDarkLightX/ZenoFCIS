//! Fixed-state matrix execution, rollback, and boundary tests.

use core::fmt::Debug;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Hash32};
use zeno_fcis_compose::{
    AccessPath, ComponentContract, ComponentId, CompositionSpec, Footprint, FrameRule, PathAtom,
    PathSet, Wiring,
};
use zeno_fcis_core::{
    Accepted, Budget, BudgetLimits, BudgetedDecision, Decision, Failed, Rejected, Resource,
};
use zeno_fcis_domain::{
    DomainError, DomainMachine, EnvelopeBinding, ExecutableComposition, MachineCandidate,
    MachineInterface, TypedPathBinding,
};
use zeno_fcis_project::SemanticId;
use zeno_fcis_schema::{
    Schema, SchemaAdmittedTypeEnvelope, SchemaLimits, TypeDef, TypeId, TypeKind, ValidationLimits,
};
use zeno_fcis_value::Value;

const SOURCE: ComponentId = ComponentId::new(1);
const SINK: ComponentId = ComponentId::new(2);
const TYPE_ID: TypeId = TypeId::new(7);

#[derive(Clone, Copy, Debug)]
struct TestHash;

impl CommitmentHasher for TestHash {
    const ALGORITHM_ID: &'static str = "test/fixed-domain-matrix/1";

    fn hash(bytes: &[u8]) -> Hash32 {
        let mut output = [0_u8; 32];
        for (index, byte) in bytes.iter().copied().enumerate() {
            let slot = index % output.len();
            output[slot] = output[slot]
                .wrapping_add(byte)
                .rotate_left((slot % 7) as u32);
        }
        Hash32::new(output)
    }
}

fn must<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn amount_schema(version: u16) -> Schema {
    let definition = must(TypeDef::try_new(
        TYPE_ID,
        "Amount",
        TypeKind::U128 { min: 0, max: 1_000 },
        SchemaLimits::default(),
    ));
    must(Schema::try_new(
        "FixedDomainMatrix",
        version,
        TYPE_ID,
        vec![definition],
        SchemaLimits::default(),
    ))
}

fn envelope(schema: &Schema, amount: u128) -> SchemaAdmittedTypeEnvelope {
    must(SchemaAdmittedTypeEnvelope::try_new::<TestHash>(
        schema,
        TYPE_ID,
        Value::U128(amount),
        ValidationLimits::default(),
    ))
}

fn schema_hash(schema: &Schema) -> Hash32 {
    must(schema.schema_hash::<TestHash>())
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn path(namespace: u32) -> AccessPath {
    must(AccessPath::try_new(namespace, vec![PathAtom::Field(1)]))
}

fn child_path(namespace: u32) -> AccessPath {
    must(AccessPath::try_new(
        namespace,
        vec![PathAtom::Field(1), PathAtom::Field(2)],
    ))
}

fn set(paths: Vec<AccessPath>) -> PathSet {
    must(PathSet::try_new(paths))
}

fn profile(component: ComponentId) -> Hash32 {
    let byte = u8::try_from(component.get()).unwrap_or(u8::MAX);
    hash(byte)
}

fn source_contract(
    source_state: AccessPath,
    source_context: AccessPath,
    output: AccessPath,
) -> ComponentContract {
    must(ComponentContract::try_new(
        SOURCE,
        profile(SOURCE),
        Footprint::new(
            set(vec![source_state.clone()]),
            set(vec![source_state]),
            set(vec![source_context]),
            set(vec![output]),
        ),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ))
}

fn sink_contract(
    sink_state: AccessPath,
    sink_context: AccessPath,
    input: AccessPath,
    authorize_source: bool,
    writable: bool,
) -> ComponentContract {
    let frames = if authorize_source {
        vec![must(FrameRule::try_new(input, vec![SOURCE], hash(91)))]
    } else {
        Vec::new()
    };
    let writes = if writable {
        set(vec![sink_state.clone()])
    } else {
        PathSet::empty()
    };
    must(ComponentContract::try_new(
        SINK,
        profile(SINK),
        Footprint::new(
            set(vec![sink_state.clone()]),
            writes,
            set(vec![sink_context]),
            PathSet::empty(),
        ),
        Vec::new(),
        Vec::new(),
        frames,
    ))
}

fn topology(
    binding: EnvelopeBinding,
    merge_order: [ComponentId; 2],
    include_wiring: bool,
    authorize_source: bool,
) -> Result<ExecutableComposition<2, 1, 1>, DomainError> {
    topology_version(
        binding,
        merge_order,
        include_wiring,
        authorize_source,
        true,
        1,
    )
}

fn topology_version(
    binding: EnvelopeBinding,
    merge_order: [ComponentId; 2],
    include_wiring: bool,
    authorize_source: bool,
    sink_writable: bool,
    version: u16,
) -> Result<ExecutableComposition<2, 1, 1>, DomainError> {
    let source_state = path(10);
    let sink_state = path(20);
    let output = path(30);
    let input = path(40);
    let source_context = path(50);
    let sink_context = path(60);
    let source = source_contract(source_state.clone(), source_context.clone(), output.clone());
    let sink = sink_contract(
        sink_state.clone(),
        sink_context.clone(),
        input.clone(),
        authorize_source,
        sink_writable,
    );
    let wirings = if include_wiring {
        vec![Wiring::new(
            SOURCE,
            output.clone(),
            SINK,
            input.clone(),
            binding.schema_hash(),
        )]
    } else {
        Vec::new()
    };
    let spec = must(CompositionSpec::try_new(
        version,
        vec![source, sink],
        wirings,
        Vec::new(),
        merge_order.to_vec(),
    ));
    let source_interface = must(MachineInterface::try_new(
        SOURCE,
        profile(SOURCE),
        binding,
        must(TypedPathBinding::try_new(source_context, binding)),
        [must(TypedPathBinding::try_new(source_state, binding))],
        [None],
        [Some(must(TypedPathBinding::try_new(output, binding)))],
    ));
    let sink_interface = must(MachineInterface::try_new(
        SINK,
        profile(SINK),
        binding,
        must(TypedPathBinding::try_new(sink_context, binding)),
        [must(TypedPathBinding::try_new(sink_state, binding))],
        [Some(must(TypedPathBinding::try_new(input, binding)))],
        [None],
    ));
    ExecutableComposition::try_new(spec, [source_interface, sink_interface])
}

fn limits() -> BudgetLimits {
    BudgetLimits::zero()
        .with_limit(Resource::Read, 4)
        .with_limit(Resource::Write, 4)
        .with_limit(Resource::Effect, 4)
}

fn reason(value: u32) -> SemanticId {
    must(SemanticId::try_new(value))
}

struct SourceMachine {
    id: ComponentId,
    next: SchemaAdmittedTypeEnvelope,
    emitted: SchemaAdmittedTypeEnvelope,
}

impl DomainMachine<1, 1> for SourceMachine {
    fn component_id(&self) -> ComponentId {
        self.id
    }

    fn step(
        &self,
        _state: &[SchemaAdmittedTypeEnvelope; 1],
        _command: &SchemaAdmittedTypeEnvelope,
        _context: &SchemaAdmittedTypeEnvelope,
        _inputs: &[Option<SchemaAdmittedTypeEnvelope>; 1],
        limits: BudgetLimits,
    ) -> BudgetedDecision<MachineCandidate<1, 1>, SemanticId, SemanticId> {
        let mut budget = Budget::new(limits);
        if budget.charge(Resource::Read, 1).is_err()
            || budget.charge(Resource::Write, 1).is_err()
            || budget.charge(Resource::Effect, 1).is_err()
        {
            return budget.finish(Decision::Reject(Rejected::new(reason(90))));
        }
        budget.finish(Decision::Accept(Accepted::new(MachineCandidate::new(
            [self.next.clone()],
            [Some(self.emitted.clone())],
        ))))
    }
}

#[derive(Clone, Copy)]
enum SinkMode {
    Accept,
    Reject,
    CommittedFailure,
}

struct SinkMachine {
    id: ComponentId,
    mode: SinkMode,
}

impl DomainMachine<1, 1> for SinkMachine {
    fn component_id(&self) -> ComponentId {
        self.id
    }

    fn step(
        &self,
        _state: &[SchemaAdmittedTypeEnvelope; 1],
        _command: &SchemaAdmittedTypeEnvelope,
        _context: &SchemaAdmittedTypeEnvelope,
        inputs: &[Option<SchemaAdmittedTypeEnvelope>; 1],
        limits: BudgetLimits,
    ) -> BudgetedDecision<MachineCandidate<1, 1>, SemanticId, SemanticId> {
        let mut budget = Budget::new(limits);
        if budget.charge(Resource::Read, 1).is_err() {
            return budget.finish(Decision::Reject(Rejected::new(reason(90))));
        }
        if matches!(self.mode, SinkMode::Reject) {
            return budget.finish(Decision::Reject(Rejected::new(reason(2))));
        }
        let Some(input) = inputs[0].as_ref() else {
            return budget.finish(Decision::Reject(Rejected::new(reason(3))));
        };
        if budget.charge(Resource::Write, 1).is_err() {
            return budget.finish(Decision::Reject(Rejected::new(reason(90))));
        }
        let candidate = MachineCandidate::new([input.clone()], [None]);
        match self.mode {
            SinkMode::Accept => budget.finish(Decision::Accept(Accepted::new(candidate))),
            SinkMode::CommittedFailure => budget.finish(Decision::CommittedFailure(Failed::new(
                candidate,
                reason(4),
            ))),
            SinkMode::Reject => budget.finish(Decision::Reject(Rejected::new(reason(2)))),
        }
    }
}

fn admitted_execution_inputs(
    executable: &ExecutableComposition<2, 1, 1>,
    schema: &Schema,
) -> (
    zeno_fcis_domain::FixedStateMatrix<2, 1>,
    zeno_fcis_domain::FixedInvocationMatrix<2>,
) {
    let state = must(executable.admit_state([[envelope(schema, 1)], [envelope(schema, 2)]]));
    let invocation = must(executable.admit_invocation(
        [envelope(schema, 10), envelope(schema, 20)],
        [envelope(schema, 30), envelope(schema, 40)],
    ));
    (state, invocation)
}

fn single_topology(
    binding: EnvelopeBinding,
    reads: AccessPath,
    writes: AccessPath,
    contexts: AccessPath,
) -> Result<ExecutableComposition<1, 1, 0>, DomainError> {
    let state = path(10);
    let context = path(50);
    let contract = must(ComponentContract::try_new(
        SOURCE,
        profile(SOURCE),
        Footprint::new(
            set(vec![reads]),
            set(vec![writes]),
            set(vec![contexts]),
            PathSet::empty(),
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
        vec![SOURCE],
    ));
    let interface = must(MachineInterface::try_new(
        SOURCE,
        profile(SOURCE),
        binding,
        must(TypedPathBinding::try_new(context, binding)),
        [must(TypedPathBinding::try_new(state, binding))],
        [],
        [],
    ));
    ExecutableComposition::try_new(spec, [interface])
}

fn amount(value: &SchemaAdmittedTypeEnvelope) -> u128 {
    match value.value().value() {
        Value::U128(value) => *value,
        other => panic!("unexpected value: {other:?}"),
    }
}

#[test]
fn executes_fixed_rows_and_routes_one_exact_output() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology(binding, [SOURCE, SINK], true, true));
    let (state, invocation) = admitted_execution_inputs(&executable, &schema);
    let source = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 3),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::Accept,
    };

    let first =
        must(executable.execute([&source, &sink], &state, &invocation, [limits(), limits()]));
    let second =
        must(executable.execute([&source, &sink], &state, &invocation, [limits(), limits()]));
    assert_eq!(first, second);
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(
        executable.routes()[0][0].map(|route| route.machine()),
        Some(1)
    );
    assert_eq!(executable.routes()[0][0].map(|route| route.port()), Some(0));

    let Decision::Accept(accepted) = first.decision() else {
        panic!("global decision was not Accept");
    };
    assert_eq!(amount(&accepted.candidate().post_state().rows()[0][0]), 3);
    assert_eq!(amount(&accepted.candidate().post_state().rows()[1][0]), 7);
    let Some(emitted) = accepted.candidate().outputs().rows()[0][0].as_ref() else {
        panic!("source output was absent");
    };
    assert_eq!(amount(emitted), 7);
    assert!(first.reports().iter().all(Option::is_some));
}

#[test]
fn rejection_discards_every_provisional_candidate() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology(binding, [SOURCE, SINK], true, true));
    let (state, invocation) = admitted_execution_inputs(&executable, &schema);
    let source = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 999),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::Reject,
    };
    let execution =
        must(executable.execute([&source, &sink], &state, &invocation, [limits(), limits()]));
    let Decision::Reject(rejected) = execution.decision() else {
        panic!("global decision was not Reject");
    };
    assert_eq!(rejected.reason().component(), SINK);
    assert_eq!(rejected.reason().reason(), reason(2));
    assert_eq!(amount(&state.rows()[0][0]), 1);
    assert_eq!(amount(&state.rows()[1][0]), 2);
}

#[test]
fn committed_failure_preserves_state_through_the_failing_machine() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology(binding, [SOURCE, SINK], true, true));
    let (state, invocation) = admitted_execution_inputs(&executable, &schema);
    let source = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 3),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::CommittedFailure,
    };
    let execution =
        must(executable.execute([&source, &sink], &state, &invocation, [limits(), limits()]));
    let Decision::CommittedFailure(failed) = execution.decision() else {
        panic!("global decision was not CommittedFailure");
    };
    assert_eq!(failed.reason().component(), SINK);
    assert_eq!(failed.reason().reason(), reason(4));
    assert_eq!(amount(&failed.candidate().post_state().rows()[0][0]), 3);
    assert_eq!(amount(&failed.candidate().post_state().rows()[1][0]), 7);
}

#[test]
fn backward_wiring_is_rejected() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    assert!(matches!(
        topology(binding, [SINK, SOURCE], true, true),
        Err(DomainError::BackwardWiring)
    ));
}

#[test]
fn active_input_requires_one_exact_wiring() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    assert!(matches!(
        topology(binding, [SOURCE, SINK], false, true),
        Err(DomainError::UnboundInputPort)
    ));
}

#[test]
fn destination_frame_must_authorize_the_source() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    assert!(matches!(
        topology(binding, [SOURCE, SINK], true, false),
        Err(DomainError::UnauthorizedWiring)
    ));
}

#[test]
fn pre_state_and_invocation_must_match_exact_schema_bindings() {
    let schema = amount_schema(1);
    let other_schema = amount_schema(2);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology(binding, [SOURCE, SINK], true, true));
    assert!(matches!(
        executable.admit_state([[envelope(&other_schema, 1)], [envelope(&schema, 2)]]),
        Err(DomainError::StateEnvelopeMismatch {
            machine: 0,
            slot: 0
        })
    ));
    assert!(matches!(
        executable.admit_invocation(
            [envelope(&schema, 1), envelope(&other_schema, 2)],
            [envelope(&schema, 3), envelope(&schema, 4)]
        ),
        Err(DomainError::CommandEnvelopeMismatch { machine: 1 })
    ));
}

#[test]
fn machine_identity_and_candidate_output_fail_closed() {
    let schema = amount_schema(1);
    let other_schema = amount_schema(2);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology(binding, [SOURCE, SINK], true, true));
    let (state, invocation) = admitted_execution_inputs(&executable, &schema);
    let wrong_id = SourceMachine {
        id: SINK,
        next: envelope(&schema, 3),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::Accept,
    };
    assert!(matches!(
        executable.execute(
            [&wrong_id, &sink],
            &state,
            &invocation,
            [limits(), limits()]
        ),
        Err(DomainError::MachineIdentityMismatch { machine: 0 })
    ));

    let wrong_output = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 3),
        emitted: envelope(&other_schema, 7),
    };
    assert!(matches!(
        executable.execute(
            [&wrong_output, &sink],
            &state,
            &invocation,
            [limits(), limits()]
        ),
        Err(DomainError::CandidateOutputMismatch {
            machine: 0,
            port: 0
        })
    ));
}

#[test]
fn state_and_invocation_cannot_cross_composition_identity() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let first = must(topology_version(
        binding,
        [SOURCE, SINK],
        true,
        true,
        true,
        1,
    ));
    let second = must(topology_version(
        binding,
        [SOURCE, SINK],
        true,
        true,
        true,
        2,
    ));
    let (first_state, first_invocation) = admitted_execution_inputs(&first, &schema);
    let (second_state, second_invocation) = admitted_execution_inputs(&second, &schema);
    let source = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 3),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::Accept,
    };
    assert!(matches!(
        first.execute(
            [&source, &sink],
            &second_state,
            &first_invocation,
            [limits(), limits()]
        ),
        Err(DomainError::StateCompositionMismatch)
    ));
    assert!(matches!(
        first.execute(
            [&source, &sink],
            &first_state,
            &second_invocation,
            [limits(), limits()]
        ),
        Err(DomainError::InvocationCompositionMismatch)
    ));
}

#[test]
fn read_only_state_cell_cannot_change() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    let executable = must(topology_version(
        binding,
        [SOURCE, SINK],
        true,
        true,
        false,
        1,
    ));
    let (state, invocation) = admitted_execution_inputs(&executable, &schema);
    let source = SourceMachine {
        id: SOURCE,
        next: envelope(&schema, 3),
        emitted: envelope(&schema, 7),
    };
    let sink = SinkMachine {
        id: SINK,
        mode: SinkMode::Accept,
    };
    assert!(matches!(
        executable.execute([&source, &sink], &state, &invocation, [limits(), limits()]),
        Err(DomainError::ReadOnlyStateMutation {
            machine: 1,
            slot: 0
        })
    ));
}

#[test]
fn cell_and_context_boundaries_reject_partial_authority() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    assert!(matches!(
        single_topology(binding, child_path(10), path(10), path(50)),
        Err(DomainError::UndeclaredStateRead { component: SOURCE })
    ));
    assert!(matches!(
        single_topology(binding, path(10), child_path(10), path(50)),
        Err(DomainError::PartialStateWrite { component: SOURCE })
    ));
    assert!(matches!(
        single_topology(binding, path(10), path(10), child_path(50)),
        Err(DomainError::UndeclaredContextInput { component: SOURCE })
    ));
}

#[test]
fn fixed_interface_rejects_zero_shape_and_wildcards() {
    let schema = amount_schema(1);
    let binding = must(EnvelopeBinding::try_new(TYPE_ID, schema_hash(&schema)));
    assert!(matches!(
        MachineInterface::<0, 0>::try_new(
            SOURCE,
            profile(SOURCE),
            binding,
            must(TypedPathBinding::try_new(path(50), binding)),
            [],
            [],
            []
        ),
        Err(DomainError::InvalidShape)
    ));
    let wildcard = must(AccessPath::try_new(
        10,
        vec![PathAtom::Field(1), PathAtom::AnyDescendant],
    ));
    assert!(matches!(
        TypedPathBinding::try_new(wildcard, binding),
        Err(DomainError::WildcardInterfacePath)
    ));
}
