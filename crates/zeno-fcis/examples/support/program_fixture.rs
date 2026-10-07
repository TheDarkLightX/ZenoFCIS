//! Small complete declaration shared by the runnable example and external API tests.
//! Original schema and invocation bytes are manually authored fixtures.
use zeno_fcis::prelude::*;
use zeno_fcis::program::scalar::{
    Domain as ScalarDomain, InputField, InputLeaf, Op, ScalarProgram,
};
use zeno_fcis::program::{declaration as c, law as laws, schema};

pub(super) fn generous() -> Limits {
    let mut limits = zero_limits();
    for r in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ] {
        limits = limits.with_limit(r, u64::MAX);
    }
    limits
}
fn fixture(limits: Limits, conformance: bool, run: impl FnOnce(&mut ProgramDefinition<'_>)) {
    let state = [InputField {
        id: 0,
        leaf: InputLeaf::Bool,
    }];
    let command = InputLeaf::I128 { min: 0, max: 2 };
    let inputs = [ScalarDomain::Bool, ScalarDomain::Int { min: 0, max: 2 }];
    let outputs = [ScalarDomain::Int { min: 0, max: 2 }];
    let nodes = [Op::Input(1)];
    let roots = [0];
    let bindings = [
        c::Binding {
            source: c::Source::State,
            selector: c::Selector::Field(0),
        },
        c::Binding {
            source: c::Source::Command,
            selector: c::Selector::Root,
        },
    ];
    let output_types = [InputLeaf::I128 { min: 0, max: 2 }];
    let assignments = [c::Assignment {
        field: 0,
        value: c::Expr::Constant(c::Atom::Bool(false)),
        domain: c::Domain::Bool,
    }];
    let payload = [c::PayloadField {
        field: 0,
        value: c::Expr::Root(c::Source::Command),
    }];
    let effects = [c::DeliveryPlan {
        ordinal: 5,
        channel: 3,
        when: c::Expr::Constant(c::Atom::Bool(true)),
        destination: c::Expr::Constant(c::Atom::Text(b"effect")),
        payload: &payload,
        idempotency: c::Expr::Constant(c::Atom::Bytes(b"ekey")),
    }];
    let outbox = [c::DeliveryPlan {
        ordinal: 9,
        channel: 3,
        when: c::Expr::Constant(c::Atom::Bool(true)),
        destination: c::Expr::Constant(c::Atom::Text(b"outbox")),
        payload: &payload,
        idempotency: c::Expr::Constant(c::Atom::Bytes(b"okey")),
    }];
    let branches = [
        c::Branch {
            code: 0,
            class: c::Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &effects,
            outbox: &outbox,
        },
        c::Branch {
            code: 1,
            class: c::Class::Reject,
            reason: Some(7),
            assignments: &[],
            effects: &[],
            outbox: &[],
        },
        c::Branch {
            code: 2,
            class: c::Class::CommittedFailure,
            reason: Some(9),
            assignments: &assignments,
            effects: &effects,
            outbox: &outbox,
        },
    ];
    let reasons = [
        c::Reason {
            id: 7,
            class: c::Class::Reject,
        },
        c::Reason {
            id: 9,
            class: c::Class::CommittedFailure,
        },
    ];
    let channel_payload = [c::TypedField {
        field: 0,
        domain: c::Domain::I128 { min: 0, max: 2 },
    }];
    let channels = [c::Channel {
        id: 3,
        destination: c::Domain::Text,
        payload: &channel_payload,
        idempotency: c::Domain::Bytes,
    }];
    let truth = [laws::Op::Literal(laws::Atom::Bool(true))];
    let initial = [laws::Op::Observe(laws::Observation::Initial(0))];
    let conform = [
        laws::Op::Observe(laws::Observation::CommandRoot),
        laws::Op::Literal(laws::Atom::I128(2)),
        laws::Op::Lt(0, 1),
        laws::Op::Eq(0, 1),
        laws::Op::Not(2),
        laws::Op::Not(3),
        laws::Op::And(4, 5),
        laws::Op::Not(6),
    ];
    let falsity = [laws::Op::Literal(laws::Atom::Bool(false))];
    let declarations = [
        laws::Law {
            id: 10,
            kind: laws::Kind::StateInvariant,
            scope: laws::Scope::Committing,
            genesis: true,
            program: laws::Program {
                nodes: &truth,
                root: 0,
            },
        },
        laws::Law {
            id: 20,
            kind: laws::Kind::RejectNoAuthority,
            scope: laws::Scope::Reject,
            genesis: false,
            program: laws::Program {
                nodes: &truth,
                root: 0,
            },
        },
        laws::Law {
            id: 30,
            kind: laws::Kind::CommittedFailureEffects,
            scope: laws::Scope::CommittedFailure,
            genesis: false,
            program: laws::Program {
                nodes: &truth,
                root: 0,
            },
        },
        laws::Law {
            id: 40,
            kind: laws::Kind::DecisionConformance,
            scope: laws::Scope::Always,
            genesis: false,
            program: laws::Program {
                nodes: if conformance { &conform } else { &falsity },
                root: if conformance { 7 } else { 0 },
            },
        },
        laws::Law {
            id: 50,
            kind: laws::Kind::InitialCondition,
            scope: laws::Scope::Always,
            genesis: true,
            program: laws::Program {
                nodes: &initial,
                root: 0,
            },
        },
    ];
    let required = [10, 20, 30, 40, 50];
    let mut descriptor = ProgramDefinition {
        state: c::Schema::Record(&state),
        command: c::Schema::Leaf(&command),
        context: c::Schema::Record(&[]),
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
        laws: &declarations,
        required: &required,
        limits,
    };
    run(&mut descriptor)
}
fn framing() -> Framing {
    Framing {
        state: FrameBinding {
            root: 100,
            schema: [23; 32],
            max_bytes: 512,
        },
        command: FrameBinding {
            root: 101,
            schema: [23; 32],
            max_bytes: 512,
        },
        context: FrameBinding {
            root: 102,
            schema: [23; 32],
            max_bytes: 512,
        },
    }
}
pub(super) fn frame(root: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"ZFCISV1\0".to_vec();
    bytes.extend_from_slice(&root.to_be_bytes());
    bytes.extend_from_slice(&[23; 32]);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
pub(super) fn originals(code: i128, initial: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let state = [9, 0, 0, 0, 1, 0, 0, if initial { 2 } else { 1 }];
    let mut command = vec![4];
    command.extend_from_slice(&code.to_be_bytes());
    (
        frame(100, &state),
        frame(101, &command),
        frame(102, &[9, 0, 0, 0, 0]),
    )
}
fn original_schema(profile: &[u8]) -> Vec<u8> {
    fn name(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    fn def(out: &mut Vec<u8>, id: u32, s: &[u8], tag: u8) {
        out.extend_from_slice(&id.to_be_bytes());
        name(out, s);
        out.push(tag);
    }
    fn field(out: &mut Vec<u8>, name_bytes: &[u8], type_id: u32) {
        out.extend_from_slice(&0u16.to_be_bytes());
        name(out, name_bytes);
        out.extend_from_slice(&type_id.to_be_bytes());
    }
    let mut out = b"ZFCISSCHEMA1\0".to_vec();
    name(&mut out, profile);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&100u32.to_be_bytes());
    out.extend_from_slice(&7u32.to_be_bytes());
    def(&mut out, 1, b"Bool", 1);
    def(&mut out, 100, b"State", 8);
    out.extend_from_slice(&1u32.to_be_bytes());
    field(&mut out, b"Flag", 1);
    def(&mut out, 101, b"Command", 3);
    out.extend_from_slice(&0i128.to_be_bytes());
    out.extend_from_slice(&2i128.to_be_bytes());
    def(&mut out, 102, b"Context", 8);
    out.extend_from_slice(&0u32.to_be_bytes());
    def(&mut out, 200, b"Destination", 5);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&32u32.to_be_bytes());
    def(&mut out, 201, b"Payload", 8);
    out.extend_from_slice(&1u32.to_be_bytes());
    field(&mut out, b"Code", 101);
    def(&mut out, 300, b"Unused", 0);
    out
}

/// Unadmitted example declarations. Only the library constructors can admit them.
pub(super) struct Material<'a> {
    pub original: &'a [u8],
    pub description: &'a schema::Description<'a>,
    pub descriptor: &'a ProgramDefinition<'a>,
    pub framing: &'a Framing,
    pub links: &'a [(u32, u32, u32)],
    pub policy: &'a [u8],
}

pub(super) fn catalog_limits() -> CatalogLimits {
    CatalogLimits {
        schema: schema::Limits {
            bytes: 4096,
            types: 32,
            fields: 16,
            variants: 16,
        },
        contract_bytes: 65536,
    }
}

/// Example declaration data only; no admission or execution is hidden here.
pub(super) fn with_material(limits: Limits, conformance: bool, run: impl FnOnce(Material<'_>)) {
    let profile = b"program_api_example";
    fixture(limits, conformance, |descriptor| {
        let fields = [schema::Field {
            id: 0,
            name: b"Flag",
            type_id: 1,
        }];
        let payload = [schema::Field {
            id: 0,
            name: b"Code",
            type_id: 101,
        }];
        let defs = [
            schema::Definition {
                id: 1,
                name: b"Bool",
                kind: schema::Kind::Bool,
            },
            schema::Definition {
                id: 100,
                name: b"State",
                kind: schema::Kind::Record(&fields),
            },
            schema::Definition {
                id: 101,
                name: b"Command",
                kind: schema::Kind::I128 { min: 0, max: 2 },
            },
            schema::Definition {
                id: 102,
                name: b"Context",
                kind: schema::Kind::Record(&[]),
            },
            schema::Definition {
                id: 200,
                name: b"Destination",
                kind: schema::Kind::Text { min: 0, max: 32 },
            },
            schema::Definition {
                id: 201,
                name: b"Payload",
                kind: schema::Kind::Record(&payload),
            },
            schema::Definition {
                id: 300,
                name: b"Unused",
                kind: schema::Kind::Unit,
            },
        ];
        let description = schema::Description {
            profile,
            version: 1,
            root: 100,
            definitions: &defs,
        };
        let original = original_schema(profile);
        let framed = framing();
        let links = [(3, 200, 201)];
        let policy = policy_bytes(descriptor, &original, &framed, &links)
            .unwrap_or_else(|| panic!("reviewed fixture policy"));

        run(Material {
            original: &original,
            description: &description,
            descriptor,
            framing: &framed,
            links: &links,
            policy: &policy,
        });
    });
}
