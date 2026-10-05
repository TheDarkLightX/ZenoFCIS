//! Additional supported generated Authority fixture. The original compound fixture stays distinct.
#![allow(missing_docs, clippy::all, clippy::pedantic)]
extern crate alloc;

pub mod generated {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/checked_schema.rs"));
}
pub mod bootstrap_project {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/checked_project.rs"));
}
#[path = "../checked_catalog.rs"]
mod checked_catalog;

use bootstrap_project::{
    GeneratedDeclarations, GeneratedProgramProposal, GeneratedProject, GeneratedProjectError,
    SCHEMA_HASH,
};
use generated::{CheckedContext, CheckedPayload, CheckedState, Code, Ready};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_synthesis::finite::{
    Domain as D, Op, V2InputField as InputField, V2InputLeaf as InputLeaf, V2Limits as Limits,
    V2Resource as Resource, V2ScalarProgram as ScalarProgram, canonical_v2::schema as s,
    v2_authority as a, v2_catalog as k, v2_composition as c, v2_laws as l,
    v2_zero_limits as zero_limits,
};

const CHANNEL_ROOTS: &[(u32, u32, u32)] = &[(30, 3, 4)];
const REQUIRED: &[u32] = &[501, 502, 503, 504, 505];
const STATE_FIELDS: &[s::Field<'static>] = &[s::Field {
    id: 1,
    name: b"ready",
    type_id: 1,
}];
const PAYLOAD_FIELDS: &[s::Field<'static>] = &[s::Field {
    id: 1,
    name: b"code",
    type_id: 2,
}];
const CONTEXT_FIELDS: &[s::Field<'static>] = &[s::Field {
    id: 1,
    name: b"approved",
    type_id: 1,
}];
const DEFINITIONS: &[s::Definition<'static>] = &[
    s::Definition {
        id: 1,
        name: b"Ready",
        kind: s::Kind::Bool,
    },
    s::Definition {
        id: 2,
        name: b"Code",
        kind: s::Kind::I128 { min: 0, max: 2 },
    },
    s::Definition {
        id: 3,
        name: b"Destination",
        kind: s::Kind::Text { min: 1, max: 16 },
    },
    s::Definition {
        id: 4,
        name: b"CheckedPayload",
        kind: s::Kind::Record(PAYLOAD_FIELDS),
    },
    s::Definition {
        id: 10,
        name: b"CheckedState",
        kind: s::Kind::Record(STATE_FIELDS),
    },
    s::Definition {
        id: 11,
        name: b"CheckedContext",
        kind: s::Kind::Record(CONTEXT_FIELDS),
    },
];

fn project() -> GeneratedProject {
    GeneratedProject::try_new::<RustCryptoSha256>()
        .unwrap_or_else(|error| panic!("additional generated project: {error:?}"))
}
fn limits() -> Limits {
    [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .into_iter()
    .fold(zero_limits(), |limits, resource| {
        limits.with_limit(resource, 8192)
    })
}
fn with_definition(run: impl FnOnce(&s::Description<'_>, &mut c::Descriptor<'_>, &c::Framing)) {
    let description = s::Description {
        profile: b"AdditionalCheckedWorkflow",
        version: 1,
        root: 10,
        definitions: DEFINITIONS,
    };
    let state = [InputField {
        id: 1,
        leaf: InputLeaf::Bool,
    }];
    let context = [InputField {
        id: 1,
        leaf: InputLeaf::Bool,
    }];
    let command = InputLeaf::I128 { min: 0, max: 2 };
    let inputs = [D::Bool, D::Int { min: 0, max: 2 }, D::Bool];
    let outputs = [D::Int { min: 0, max: 2 }];
    // Accept/CommittedFailure require both exact original Boolean fields.
    let nodes = [
        Op::Input(0),
        Op::Input(1),
        Op::Input(2),
        Op::And(0, 2),
        Op::Int(1),
        Op::Select(3, 1, 4),
    ];
    let roots = [5];
    let bindings = [
        c::Binding {
            source: c::Source::State,
            selector: c::Selector::Field(1),
        },
        c::Binding {
            source: c::Source::Command,
            selector: c::Selector::Root,
        },
        GeneratedDeclarations::context_approved_binding(),
    ];
    let output_types = [InputLeaf::I128 { min: 0, max: 2 }];
    let assignments = [
        GeneratedDeclarations::update_ready(c::Expr::Constant(c::Atom::Bool(false)))
            .unwrap_or_else(|error| panic!("generated assignment: {error:?}")),
    ];
    let payload = [c::PayloadField {
        field: 1,
        value: c::Expr::Root(c::Source::Command),
    }];
    let outbox = [c::DeliveryPlan {
        ordinal: 8,
        channel: 30,
        when: c::Expr::Constant(c::Atom::Bool(true)),
        destination: c::Expr::Constant(c::Atom::Text(b"destination")),
        payload: &payload,
        idempotency: c::Expr::Constant(c::Atom::Bytes(b"key")),
    }];
    let branches = [
        c::Branch {
            code: 0,
            class: c::Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &[],
            outbox: &outbox,
        },
        c::Branch {
            code: 1,
            class: c::Class::Reject,
            reason: Some(10),
            assignments: &[],
            effects: &[],
            outbox: &[],
        },
        c::Branch {
            code: 2,
            class: c::Class::CommittedFailure,
            reason: Some(11),
            assignments: &assignments,
            effects: &[],
            outbox: &outbox,
        },
    ];
    let reasons = [
        c::Reason {
            id: 10,
            class: c::Class::Reject,
        },
        c::Reason {
            id: 11,
            class: c::Class::CommittedFailure,
        },
    ];
    let channel_payload = [c::TypedField {
        field: 1,
        domain: c::Domain::I128 { min: 0, max: 2 },
    }];
    let channels = [c::Channel {
        id: 30,
        destination: c::Domain::Text,
        payload: &channel_payload,
        idempotency: c::Domain::Bytes,
    }];
    let invariant = [
        l::Op::Observe(l::Observation::PostLength),
        l::Op::Literal(l::Atom::U128(1)),
        l::Op::Eq(0, 1),
    ];
    let reject = [
        l::Op::Observe(l::Observation::PatchLength),
        l::Op::Observe(l::Observation::EffectLength),
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::Literal(l::Atom::U128(0)),
        l::Op::Eq(0, 3),
        l::Op::Eq(1, 3),
        l::Op::Eq(2, 3),
        l::Op::And(4, 5),
        l::Op::And(7, 6),
    ];
    let failure = [
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::Literal(l::Atom::U128(1)),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::OutboxPayload(0, 1)),
        l::Op::Observe(l::Observation::CommandRoot),
        l::Op::Eq(3, 4),
        l::Op::And(2, 5),
    ];
    let conformance = [
        l::Op::Observe(l::Observation::Pre(1)),
        l::Op::Observe(l::Observation::Context(1)),
        l::Op::And(0, 1),
        l::Op::Observe(l::Observation::CommandRoot),
        l::Op::Literal(l::Atom::I128(1)),
        l::Op::Select(2, 3, 4),
        l::Op::Observe(l::Observation::Class),
        l::Op::ToI128(6),
        l::Op::Eq(5, 7),
    ];
    let initial = [l::Op::Observe(l::Observation::Initial(1))];
    let laws = [
        l::Law {
            id: 501,
            kind: l::Kind::StateInvariant,
            scope: l::Scope::Committing,
            genesis: true,
            program: l::Program {
                nodes: &invariant,
                root: 2,
            },
        },
        l::Law {
            id: 502,
            kind: l::Kind::RejectNoAuthority,
            scope: l::Scope::Reject,
            genesis: false,
            program: l::Program {
                nodes: &reject,
                root: 8,
            },
        },
        l::Law {
            id: 503,
            kind: l::Kind::CommittedFailureEffects,
            scope: l::Scope::CommittedFailure,
            genesis: false,
            program: l::Program {
                nodes: &failure,
                root: 6,
            },
        },
        l::Law {
            id: 504,
            kind: l::Kind::DecisionConformance,
            scope: l::Scope::Always,
            genesis: false,
            program: l::Program {
                nodes: &conformance,
                root: 8,
            },
        },
        l::Law {
            id: 505,
            kind: l::Kind::InitialCondition,
            scope: l::Scope::Always,
            genesis: true,
            program: l::Program {
                nodes: &initial,
                root: 0,
            },
        },
    ];
    let mut definition = c::Descriptor {
        state: c::Schema::Record(&state),
        command: c::Schema::Leaf(&command),
        context: c::Schema::Record(&context),
        program: ScalarProgram {
            inputs: &inputs,
            outputs: &outputs,
            nodes: &nodes,
            roots: &roots,
        },
        bindings: &bindings,
        output_types: &output_types,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &laws,
        required: REQUIRED,
        limits: limits(),
    };
    let frame = |root| c::FrameBinding {
        root,
        schema: *SCHEMA_HASH.as_bytes(),
        max_bytes: 512,
    };
    let framing = c::Framing {
        state: frame(10),
        command: frame(2),
        context: frame(11),
    };
    run(&description, &mut definition, &framing);
}
fn proposal<'a>(
    schema: &'a [u8],
    description: &'a s::Description<'a>,
    policy: &'a [u8],
    definition: &'a c::Descriptor<'a>,
    framing: &'a c::Framing,
) -> GeneratedProgramProposal<'a> {
    GeneratedProgramProposal {
        original_schema: schema,
        description,
        catalog_limits: k::Limits {
            schema: s::Limits {
                bytes: 8192,
                types: 32,
                fields: 32,
                variants: 32,
            },
            contract_bytes: 65536,
        },
        original_policy: policy,
        definition,
        framing,
        channel_roots: CHANNEL_ROOTS,
    }
}
fn invocation(
    project: &GeneratedProject,
    ready: bool,
    command: i128,
    approved: bool,
) -> bootstrap_project::GeneratedInvocation {
    let state = project
        .admit_root::<RustCryptoSha256>(
            &CheckedState {
                ready: Ready(ready),
            },
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("state: {error:?}"));
    let command = project
        .admit_command::<RustCryptoSha256>(&Code(command), ValidationLimits::default())
        .unwrap_or_else(|error| panic!("command: {error:?}"));
    let context = project
        .admit_context::<RustCryptoSha256>(
            &CheckedContext {
                approved: Ready(approved),
            },
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("context: {error:?}"));
    project
        .invocation::<RustCryptoSha256>(&state, &command, &context)
        .unwrap_or_else(|error| panic!("invocation: {error:?}"))
}
fn checked<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("fixture: {error:?}"))
}
fn bare(value: zeno_fcis_value::Value) -> Vec<u8> {
    checked(value.canonical_bytes())
}
fn counters(usage: zeno_fcis_synthesis::finite::V2Usage) -> [u64; 8] {
    [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .map(|resource| usage.used(resource))
}
// Independent publication framing oracle: no production canonical encoder.
fn word(out: &mut Vec<u8>, value: u128) {
    out.push(0);
    out.extend_from_slice(&value.to_be_bytes());
}
fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.push(1);
    out.extend_from_slice(&(value.len() as u128).to_be_bytes());
    out.extend_from_slice(value);
}
fn publication_bytes(
    evaluation: &[u8],
    post: &[u8],
    destination: &[u8],
    payload: &[u8],
    idempotency: &[u8],
) -> Vec<u8> {
    let mut result = Vec::new();
    for value in [0x5a505532, 1, 1] {
        word(&mut result, value);
    }
    bytes(&mut result, evaluation);
    bytes(&mut result, post);
    word(&mut result, 0);
    word(&mut result, 1);
    for value in [8, 30, 3, 4] {
        word(&mut result, value);
    }
    for value in [destination, payload, idempotency] {
        bytes(&mut result, value);
    }
    result
}

#[test]
fn additional_supported_generated_authority_evaluates_all_twelve_original_inputs_of_its_own_domain()
{
    with_definition(|description, definition, framing| {
        let project = project();
        let authored_catalog = checked_catalog::checked_catalog(checked_catalog::checked_schema());
        assert_eq!(
            checked(project.catalog().canonical_bytes()),
            checked(authored_catalog.canonical_bytes())
        );
        let schema = checked(project.catalog().schema().canonical_bytes());
        let policy = a::policy_bytes(definition, &schema, framing, CHANNEL_ROOTS)
            .unwrap_or_else(|| panic!("policy encoding"));
        let authority = checked(project.bind_program::<RustCryptoSha256>(proposal(
            &schema,
            description,
            &policy,
            definition,
            framing,
        )));
        let mut commits = 0;
        let mut rejects = 0;
        for ready in [false, true] {
            for command in 0..=2 {
                for approved in [false, true] {
                    let invocation = invocation(&project, ready, command, approved);
                    let expected = if ready && approved { command } else { 1 };
                    match authority.publish(invocation.original()) {
                        a::PublicationOutcome::Commit(publication) => {
                            commits += 1;
                            assert_ne!(expected, 1);
                            let evaluation = publication.evaluation();
                            let candidate = checked(evaluation.result());
                            assert_eq!(
                                candidate.class(),
                                if expected == 0 {
                                    c::Class::Accept
                                } else {
                                    c::Class::CommittedFailure
                                }
                            );
                            assert_eq!(
                                candidate.reason(),
                                if expected == 0 { None } else { Some(11) }
                            );
                            let post = checked(project.admit_root::<RustCryptoSha256>(
                                &CheckedState {
                                    ready: Ready(false),
                                },
                                ValidationLimits::default(),
                            ));
                            assert_eq!(
                                publication.poststate(),
                                checked(post.envelope().canonical_bytes())
                            );
                            assert_eq!(publication.identity(), authority.identity());
                            assert!(publication.effects().is_empty());
                            let delivery = &publication.outbox()[0];
                            assert_eq!(publication.outbox().len(), 1);
                            let inert = checked(bootstrap_project::channel_30(
                                8,
                                &generated::Destination("destination".into()),
                                &CheckedPayload {
                                    code: Code(command),
                                },
                            ));
                            assert_eq!(delivery.ordinal(), 8);
                            assert_eq!(delivery.channel(), 30);
                            assert_eq!(delivery.destination_root(), 3);
                            assert_eq!(delivery.payload_root(), 4);
                            assert_eq!(delivery.destination(), bare(inert.destination().clone()));
                            assert_eq!(delivery.payload(), bare(inert.payload().clone()));
                            assert_eq!(
                                delivery.idempotency(),
                                bare(checked(zeno_fcis_value::Value::bytes(b"key".to_vec())))
                            );
                            let expected_wire = publication_bytes(
                                checked(evaluation.subject()),
                                publication.poststate(),
                                delivery.destination(),
                                delivery.payload(),
                                delivery.idempotency(),
                            );
                            assert_eq!(publication.subject(), expected_wire);
                            let replay = authority
                                .replay_publication(invocation.original(), publication.subject());
                            match replay {
                                a::PublicationOutcome::Commit(actual) => {
                                    assert_eq!(actual.subject(), publication.subject());
                                    assert_eq!(actual.poststate(), publication.poststate());
                                    assert_eq!(
                                        counters(actual.evaluation().usage()),
                                        counters(evaluation.usage())
                                    );
                                }
                                actual => panic!("complete replay: {actual:?}"),
                            }
                            assert_eq!(
                                counters(evaluation.usage()),
                                if expected == 0 {
                                    [8, 1, 1, 1, 177, 0, 0, 18]
                                } else {
                                    [11, 1, 1, 1, 177, 0, 0, 25]
                                }
                            );
                        }
                        a::PublicationOutcome::Reject(evaluation) => {
                            rejects += 1;
                            assert_eq!(expected, 1);
                            let candidate = checked(evaluation.result());
                            assert_eq!(candidate.class(), c::Class::Reject);
                            assert_eq!(candidate.reason(), Some(10));
                            assert!(candidate.post().is_empty());
                            assert!(candidate.patch().is_empty());
                            assert!(candidate.effects().is_empty());
                            assert!(candidate.outbox().is_empty());
                            assert_eq!(evaluation.raw().state, invocation.original().state);
                            assert_eq!(counters(evaluation.usage()), [10, 0, 1, 0, 177, 0, 0, 24]);
                        }
                        actual => panic!("additional checked publication: {actual:?}"),
                    }
                }
            }
        }
        assert_eq!((commits, rejects), (2, 10));
    });
}

#[test]
fn additional_generated_publication_rejects_changed_complete_wire_and_inputs() {
    with_definition(|description, definition, framing| {
        let project = project();
        let schema = checked(project.catalog().schema().canonical_bytes());
        let policy = a::policy_bytes(definition, &schema, framing, CHANNEL_ROOTS)
            .unwrap_or_else(|| panic!("policy"));
        let authority = checked(project.bind_program::<RustCryptoSha256>(proposal(
            &schema,
            description,
            &policy,
            definition,
            framing,
        )));
        let original = invocation(&project, true, 0, true);
        let publication = match authority.publish(original.original()) {
            a::PublicationOutcome::Commit(value) => value,
            other => panic!("publication: {other:?}"),
        };
        let subject = publication.subject();
        // Exercise every framing word and every opaque-byte component without a quadratic megabyte copy loop.
        let mut offsets = vec![0, 1, 16, 17, 34, 50, 51, 67, subject.len() - 1];
        let mut cursor = 0;
        while cursor < subject.len() {
            offsets.push(cursor);
            match subject[cursor] {
                0 => {
                    offsets.push(cursor + 16);
                    cursor += 17;
                }
                1 => {
                    let mut length = [0; 16];
                    length.copy_from_slice(&subject[cursor + 1..cursor + 17]);
                    let length = u128::from_be_bytes(length) as usize;
                    offsets.push(cursor + 16);
                    if length > 0 {
                        offsets.push(cursor + 17);
                        offsets.push(cursor + 16 + length);
                    }
                    cursor += 17 + length;
                }
                other => panic!("publication token {other}"),
            }
        }
        offsets.sort_unstable();
        offsets.dedup();
        for offset in offsets {
            let mut altered = subject.to_vec();
            altered[offset] ^= 1;
            assert!(matches!(
                authority.replay_publication(original.original(), &altered),
                a::PublicationOutcome::Refused {
                    error: a::Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        for altered in [
            invocation(&project, false, 0, true),
            invocation(&project, true, 1, true),
            invocation(&project, true, 0, false),
        ] {
            assert!(matches!(
                authority.replay_publication(altered.original(), subject),
                a::PublicationOutcome::Refused {
                    error: a::Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        for which in 0..3 {
            for index in [6usize, 8, 12, 44, 47] {
                let raw = original.original();
                let mut state = raw.state.to_vec();
                let mut command = raw.command.to_vec();
                let mut context = raw.context.to_vec();
                [&mut state, &mut command, &mut context][which][index] ^= 1;
                assert!(matches!(
                    authority.publish(c::Raw {
                        state: &state,
                        command: &command,
                        context: &context
                    }),
                    a::PublicationOutcome::Refused { .. }
                ));
            }
        }
        assert!(matches!(
            authority.replay_publication(original.original(), &subject[..subject.len() - 1]),
            a::PublicationOutcome::Refused {
                error: a::Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}

#[test]
fn additional_generated_binding_checks_complete_schema_framing_catalog_policy_and_required_laws() {
    with_definition(|description, definition, framing| {
        let project = project();
        let schema = checked(project.catalog().schema().canonical_bytes());
        let policy = a::policy_bytes(definition, &schema, framing, CHANNEL_ROOTS)
            .unwrap_or_else(|| panic!("policy"));
        let mut changed_schema = schema.clone();
        changed_schema[0] ^= 1;
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &changed_schema,
                description,
                &policy,
                definition,
                framing
            )),
            Err(GeneratedProjectError::OriginalSchemaMismatch)
        ));
        let mut changed_frame = *framing;
        changed_frame.context.root = 10;
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &schema,
                description,
                &policy,
                definition,
                &changed_frame
            )),
            Err(GeneratedProjectError::FramingMismatch)
        ));
        let mut changed_policy = policy.clone();
        changed_policy[0] ^= 1;
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &schema,
                description,
                &changed_policy,
                definition,
                framing
            )),
            Err(GeneratedProjectError::CheckedCatalog(k::Failure::Policy))
        ));
        let reasons = definition.reasons;
        definition.reasons = &[];
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &schema,
                description,
                &policy,
                definition,
                framing
            )),
            Err(GeneratedProjectError::ReasonSetMismatch)
        ));
        definition.reasons = reasons;
        let mut missing_channel = proposal(&schema, description, &policy, definition, framing);
        missing_channel.channel_roots = &[];
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(missing_channel),
            Err(GeneratedProjectError::ChannelSetMismatch)
        ));
        let required = definition.required;
        definition.required = &[];
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &schema,
                description,
                &policy,
                definition,
                framing
            )),
            Err(GeneratedProjectError::CheckedCatalog(k::Failure::Policy))
        ));
        definition.required = &[501, 502, 503, 504, 506];
        assert!(matches!(
            project.bind_program::<RustCryptoSha256>(proposal(
                &schema,
                description,
                &policy,
                definition,
                framing
            )),
            Err(GeneratedProjectError::CheckedCatalog(
                k::Failure::Descriptor
            ))
        ));
        definition.required = required;
        let false_nodes = [l::Op::Literal(l::Atom::Bool(false))];
        let mut changed_laws: Vec<_> = definition
            .laws
            .iter()
            .map(|law| l::Law {
                id: law.id,
                kind: law.kind,
                scope: law.scope,
                genesis: law.genesis,
                program: l::Program {
                    nodes: law.program.nodes,
                    root: law.program.root,
                },
            })
            .collect();
        changed_laws[3].program = l::Program {
            nodes: &false_nodes,
            root: 0,
        };
        let changed_definition = c::Descriptor {
            state: definition.state,
            command: definition.command,
            context: definition.context,
            program: ScalarProgram {
                inputs: definition.program.inputs,
                outputs: definition.program.outputs,
                nodes: definition.program.nodes,
                roots: definition.program.roots,
            },
            bindings: definition.bindings,
            output_types: definition.output_types,
            decision_output: definition.decision_output,
            branches: definition.branches,
            reasons: definition.reasons,
            channels: definition.channels,
            laws: &changed_laws,
            required: definition.required,
            limits: definition.limits,
        };
        let policy = a::policy_bytes(&changed_definition, &schema, framing, CHANNEL_ROOTS)
            .unwrap_or_else(|| panic!("changed law policy"));
        let authority = checked(project.bind_program::<RustCryptoSha256>(proposal(
            &schema,
            description,
            &policy,
            &changed_definition,
            framing,
        )));
        let invocation = invocation(&project, true, 0, true);
        assert!(matches!(
            authority.publish(invocation.original()),
            a::PublicationOutcome::Refused {
                error: a::Refusal::Core(c::Failure::Law(l::Failure::Violated)),
                ..
            }
        ));
    });
}

#[test]
fn additional_generated_genesis_runs_real_initial_laws_and_phase_replay() {
    with_definition(|description, definition, framing| {
        let project = project();
        let schema = checked(project.catalog().schema().canonical_bytes());
        let policy = a::policy_bytes(definition, &schema, framing, CHANNEL_ROOTS)
            .unwrap_or_else(|| panic!("policy"));
        let authority = checked(project.bind_program::<RustCryptoSha256>(proposal(
            &schema,
            description,
            &policy,
            definition,
            framing,
        )));
        let original = invocation(&project, true, 0, true);
        let genesis = match authority.publish_genesis(original.original().state) {
            a::PublicationOutcome::Commit(value) => value,
            actual => panic!("genesis: {actual:?}"),
        };
        assert_eq!(genesis.poststate(), original.original().state);
        assert!(matches!(
            authority.replay_genesis_publication(genesis.poststate(), genesis.subject()),
            a::PublicationOutcome::Commit(_)
        ));
        let invalid = invocation(&project, false, 0, true);
        assert!(matches!(
            authority.publish_genesis(invalid.original().state),
            a::PublicationOutcome::Refused {
                error: a::Refusal::Core(c::Failure::Law(l::Failure::Violated)),
                ..
            }
        ));
        assert!(matches!(
            authority.replay_publication(original.original(), genesis.subject()),
            a::PublicationOutcome::Refused {
                error: a::Refusal::ReplayMismatch,
                ..
            }
        ));
        assert_eq!(
            genesis
                .evaluation()
                .diagnostics()
                .iter()
                .map(|d| d.id)
                .collect::<Vec<_>>(),
            REQUIRED
        );
    });
}

#[test]
fn additional_generated_accounting_preserves_exact_and_one_under_charges_for_every_class() {
    // Byte177 = three48-byte frame headers + Bool-record8 + I12817 + Bool-record8.
    // Step = control6 + applicable law nodes; Read = ingress3 + law observations.
    // Candidate1 always; committing assignments/effects each charge1.
    let resources = [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ];
    for (command, expected) in [
        (0, [8, 1, 1, 1, 177, 0, 0, 18]),
        (1, [10, 0, 1, 0, 177, 0, 0, 24]),
        (2, [11, 1, 1, 1, 177, 0, 0, 25]),
    ] {
        for (index, resource) in resources.into_iter().enumerate() {
            if expected[index] == 0 {
                continue;
            }
            for exact in [true, false] {
                with_definition(|description, definition, framing| {
                    let cap = expected[index] - u64::from(!exact);
                    definition.limits = limits().with_limit(resource, cap);
                    let project = project();
                    let schema = checked(project.catalog().schema().canonical_bytes());
                    let policy = a::policy_bytes(definition, &schema, framing, CHANNEL_ROOTS)
                        .unwrap_or_else(|| panic!("budget policy"));
                    let authority = checked(project.bind_program::<RustCryptoSha256>(proposal(
                        &schema,
                        description,
                        &policy,
                        definition,
                        framing,
                    )));
                    let invocation = invocation(&project, true, command, true);
                    match authority.publish(invocation.original()) {
                        a::PublicationOutcome::Commit(publication) => {
                            assert!(exact);
                            assert_ne!(command, 1);
                            assert_eq!(counters(publication.evaluation().usage()), expected);
                        }
                        a::PublicationOutcome::Reject(evaluation) => {
                            assert!(exact);
                            assert_eq!(command, 1);
                            assert_eq!(counters(evaluation.usage()), expected);
                        }
                        a::PublicationOutcome::Refused { evaluation, .. } => {
                            assert!(!exact);
                            assert!(evaluation.usage().used(resource) <= cap);
                            assert!(evaluation.result().is_err());
                        }
                        other => panic!("unknown future outcome: {other:?}"),
                    }
                });
            }
        }
    }
}
