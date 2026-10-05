// Generated declarative data. Review v2/policy.json and project.zeno.
// Regenerate or check with `zeno-fcis generate contract`; no runtime mapper.
extern crate alloc;
use alloc::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{Domain as ScalarDomain, Op, V2ScalarProgram};
use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2InputVariant as InputVariant,
    V2Resource as Resource, canonical_v2::schema as s, v2_authority as authority,
    v2_catalog as catalog, v2_composition as c, v2_laws as l, v2_zero_limits,
};
/// Exact actual original schema bytes.
pub const ORIGINAL_SCHEMA: &[u8] = include_bytes!("../v2/schema.zcve");
/// Complete reviewed library-encoded policy.
pub const ORIGINAL_POLICY: &[u8] = include_bytes!("../v2/policy.zcve");
/// Complete original named schema description.
pub const DESCRIPTION: s::Description<'static> = s::Description {
    profile: b"durable_counter",
    version: 1,
    root: 100,
    definitions: &[
        s::Definition {
            id: 100,
            name: b"CounterState",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 110,
                    name: b"count",
                    type_id: 105,
                },
                s::Field {
                    id: 111,
                    name: b"failures",
                    type_id: 105,
                },
            ]),
        },
        s::Definition {
            id: 101,
            name: b"CounterCommand",
            kind: s::Kind::Sum(&[
                s::Variant {
                    id: 120,
                    name: b"Increment",
                },
                s::Variant {
                    id: 121,
                    name: b"RecordFailure",
                },
            ]),
        },
        s::Definition {
            id: 102,
            name: b"CounterContext",
            kind: s::Kind::Bool,
        },
        s::Definition {
            id: 103,
            name: b"NotificationDestination",
            kind: s::Kind::Text { min: 1, max: 32 },
        },
        s::Definition {
            id: 104,
            name: b"Notification",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 112,
                    name: b"notified_count",
                    type_id: 105,
                },
                s::Field {
                    id: 113,
                    name: b"notified_failures",
                    type_id: 105,
                },
            ]),
        },
        s::Definition {
            id: 105,
            name: b"CounterValue",
            kind: s::Kind::I128 { min: 0, max: 3 },
        },
    ],
};
/// Original root and schema commitments and complete wire-size limits.
pub const FRAMING: c::Framing = c::Framing {
    state: c::FrameBinding {
        root: 100,
        schema: [
            61, 248, 20, 218, 7, 0, 29, 54, 23, 120, 178, 228, 232, 246, 171, 211, 245, 86, 144,
            183, 163, 3, 7, 138, 166, 224, 186, 192, 60, 156, 49, 113,
        ],
        max_bytes: 91,
    },
    command: c::FrameBinding {
        root: 101,
        schema: [
            61, 248, 20, 218, 7, 0, 29, 54, 23, 120, 178, 228, 232, 246, 171, 211, 245, 86, 144,
            183, 163, 3, 7, 138, 166, 224, 186, 192, 60, 156, 49, 113,
        ],
        max_bytes: 56,
    },
    context: c::FrameBinding {
        root: 102,
        schema: [
            61, 248, 20, 218, 7, 0, 29, 54, 23, 120, 178, 228, 232, 246, 171, 211, 245, 86, 144,
            183, 163, 3, 7, 138, 166, 224, 186, 192, 60, 156, 49, 113,
        ],
        max_bytes: 49,
    },
};
/// Exact channel to original destination and payload type links.
pub const CHANNEL_ROOTS: &[(u32, u32, u32)] = &[(300, 103, 104)];
/// The genesis state law 990 requires, field by field.
pub const GENESIS: &[c::Field<'static>] = &[
    c::Field {
        id: 110,
        value: c::Atom::I128(0),
    },
    c::Field {
        id: 111,
        value: c::Atom::I128(0),
    },
];
/// Complete ordered decisions selected only by actual graph output.
pub const BRANCHES: &[c::Branch<'static>] = &[
    c::Branch {
        code: 0,
        class: c::Class::Reject,
        reason: Some(200),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 1,
        class: c::Class::Reject,
        reason: Some(201),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 2,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Output(1),
                domain: c::Domain::I128 { min: 0, max: 3 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Input(c::Source::State, 111),
                domain: c::Domain::I128 { min: 0, max: 3 },
            },
        ],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"local-observer")),
            payload: &[
                c::PayloadField {
                    field: 112,
                    value: c::Expr::Output(1),
                },
                c::PayloadField {
                    field: 113,
                    value: c::Expr::Input(c::Source::State, 111),
                },
            ],
            idempotency: c::Expr::Constant(c::Atom::U128(0)),
        }],
    },
    c::Branch {
        code: 3,
        class: c::Class::CommittedFailure,
        reason: Some(202),
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Input(c::Source::State, 110),
                domain: c::Domain::I128 { min: 0, max: 3 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Output(2),
                domain: c::Domain::I128 { min: 0, max: 3 },
            },
        ],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"local-observer")),
            payload: &[
                c::PayloadField {
                    field: 112,
                    value: c::Expr::Input(c::Source::State, 110),
                },
                c::PayloadField {
                    field: 113,
                    value: c::Expr::Output(2),
                },
            ],
            idempotency: c::Expr::Constant(c::Atom::U128(0)),
        }],
    },
];
const LAW_500: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Lt(1, 2),
    l::Op::Not(3),
    l::Op::Literal(l::Atom::I128(3)),
    l::Op::Lt(5, 1),
    l::Op::Not(6),
    l::Op::And(4, 7),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(9),
    l::Op::Lt(10, 2),
    l::Op::Not(11),
    l::Op::And(8, 12),
    l::Op::Lt(5, 10),
    l::Op::Not(14),
    l::Op::And(13, 15),
];
const LAW_501: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::ContextRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(4),
    l::Op::Literal(l::Atom::I128(120)),
    l::Op::Eq(5, 6),
    l::Op::And(3, 7),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(9),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(11),
    l::Op::Add(12, 2),
    l::Op::Eq(10, 13),
    l::Op::And(8, 14),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(16),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(18),
    l::Op::Eq(17, 19),
    l::Op::And(15, 20),
];
const LAW_502: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::ContextRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(4),
    l::Op::Literal(l::Atom::I128(121)),
    l::Op::Eq(5, 6),
    l::Op::And(3, 7),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(9),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(11),
    l::Op::Eq(10, 12),
    l::Op::And(8, 13),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(15),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(17),
    l::Op::Add(18, 2),
    l::Op::Eq(16, 19),
    l::Op::And(14, 20),
];
const LAW_503: &[l::Op<'static>] = &[l::Op::Literal(l::Atom::Bool(true))];
const LAW_909: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::PostLength),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::PatchLength),
    l::Op::ToI128(4),
    l::Op::Eq(5, 2),
    l::Op::Observe(l::Observation::EffectLength),
    l::Op::ToI128(7),
    l::Op::Eq(8, 2),
    l::Op::Observe(l::Observation::OutboxLength),
    l::Op::ToI128(10),
    l::Op::Eq(11, 2),
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::And(13, 3),
    l::Op::And(14, 6),
    l::Op::And(15, 9),
    l::Op::And(16, 12),
];
const LAW_990: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Initial(110)),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(0, 1),
    l::Op::Observe(l::Observation::Initial(111)),
    l::Op::Eq(3, 1),
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::And(5, 2),
    l::Op::And(6, 4),
];
const LAW_991: &[l::Op<'static>] = &[
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::Observe(l::Observation::ContextRoot),
    l::Op::ToI128(1),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(2, 3),
    l::Op::Not(4),
    l::Op::And(0, 5),
    l::Op::Not(5),
    l::Op::And(0, 7),
    l::Op::Observe(l::Observation::Class),
    l::Op::ToI128(9),
    l::Op::Eq(10, 3),
    l::Op::Observe(l::Observation::HasReason),
    l::Op::ObserveWhen(6, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(13),
    l::Op::Literal(l::Atom::I128(200)),
    l::Op::Eq(14, 15),
    l::Op::Observe(l::Observation::EffectLength),
    l::Op::ToI128(17),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(18, 19),
    l::Op::Observe(l::Observation::OutboxLength),
    l::Op::ToI128(21),
    l::Op::Eq(22, 19),
    l::Op::Observe(l::Observation::PostLength),
    l::Op::ToI128(24),
    l::Op::Eq(25, 19),
    l::Op::And(0, 11),
    l::Op::And(27, 12),
    l::Op::And(28, 16),
    l::Op::And(29, 20),
    l::Op::And(30, 23),
    l::Op::And(31, 26),
    l::Op::Select(6, 32, 0),
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(34),
    l::Op::Literal(l::Atom::I128(120)),
    l::Op::Eq(35, 36),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(38),
    l::Op::Literal(l::Atom::I128(3)),
    l::Op::Eq(39, 40),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(42),
    l::Op::Eq(43, 40),
    l::Op::Select(37, 41, 44),
    l::Op::And(8, 45),
    l::Op::Not(45),
    l::Op::And(8, 47),
    l::Op::ObserveWhen(46, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(49),
    l::Op::Literal(l::Atom::I128(201)),
    l::Op::Eq(50, 51),
    l::Op::And(28, 52),
    l::Op::And(53, 20),
    l::Op::And(54, 23),
    l::Op::And(55, 26),
    l::Op::Select(46, 56, 0),
    l::Op::And(48, 37),
    l::Op::Not(37),
    l::Op::And(48, 59),
    l::Op::Eq(10, 19),
    l::Op::Not(12),
    l::Op::Eq(22, 3),
    l::Op::Literal(l::Atom::I128(2)),
    l::Op::Eq(25, 64),
    l::Op::ObserveWhen(58, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(66),
    l::Op::Add(39, 3),
    l::Op::Eq(67, 68),
    l::Op::Select(58, 69, 0),
    l::Op::ObserveWhen(58, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(71),
    l::Op::Eq(72, 43),
    l::Op::Select(58, 73, 0),
    l::Op::ObserveWhen(58, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(75),
    l::Op::Eq(76, 19),
    l::Op::ObserveWhen(58, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(78),
    l::Op::Literal(l::Atom::I128(300)),
    l::Op::Eq(79, 80),
    l::Op::ObserveWhen(
        58,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"local-observer"),
    ),
    l::Op::Literal(l::Atom::Text(b"local-observer")),
    l::Op::Eq(82, 83),
    l::Op::ObserveWhen(58, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Literal(l::Atom::U128(0)),
    l::Op::Eq(85, 86),
    l::Op::ObserveWhen(58, l::Observation::OutboxPayload(0, 112), l::Atom::I128(0)),
    l::Op::ToI128(88),
    l::Op::Eq(89, 68),
    l::Op::Select(58, 90, 0),
    l::Op::ObserveWhen(58, l::Observation::OutboxPayload(0, 113), l::Atom::I128(0)),
    l::Op::ToI128(92),
    l::Op::Eq(93, 43),
    l::Op::Select(58, 94, 0),
    l::Op::And(0, 61),
    l::Op::And(96, 62),
    l::Op::And(97, 20),
    l::Op::And(98, 63),
    l::Op::And(99, 65),
    l::Op::And(100, 70),
    l::Op::And(101, 74),
    l::Op::And(102, 77),
    l::Op::And(103, 81),
    l::Op::And(104, 84),
    l::Op::And(105, 87),
    l::Op::And(106, 91),
    l::Op::And(107, 95),
    l::Op::Select(58, 108, 0),
    l::Op::And(60, 0),
    l::Op::Not(0),
    l::Op::And(60, 111),
    l::Op::Eq(10, 64),
    l::Op::ObserveWhen(110, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(114),
    l::Op::Literal(l::Atom::I128(202)),
    l::Op::Eq(115, 116),
    l::Op::ObserveWhen(110, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(118),
    l::Op::Eq(119, 39),
    l::Op::Select(110, 120, 0),
    l::Op::ObserveWhen(110, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(122),
    l::Op::Add(43, 3),
    l::Op::Eq(123, 124),
    l::Op::Select(110, 125, 0),
    l::Op::ObserveWhen(110, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(127),
    l::Op::Eq(128, 19),
    l::Op::ObserveWhen(110, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(130),
    l::Op::Eq(131, 80),
    l::Op::ObserveWhen(
        110,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"local-observer"),
    ),
    l::Op::Eq(133, 83),
    l::Op::ObserveWhen(110, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Eq(135, 86),
    l::Op::ObserveWhen(110, l::Observation::OutboxPayload(0, 112), l::Atom::I128(0)),
    l::Op::ToI128(137),
    l::Op::Eq(138, 39),
    l::Op::Select(110, 139, 0),
    l::Op::ObserveWhen(110, l::Observation::OutboxPayload(0, 113), l::Atom::I128(0)),
    l::Op::ToI128(141),
    l::Op::Eq(142, 124),
    l::Op::Select(110, 143, 0),
    l::Op::And(0, 113),
    l::Op::And(145, 12),
    l::Op::And(146, 117),
    l::Op::And(147, 20),
    l::Op::And(148, 63),
    l::Op::And(149, 65),
    l::Op::And(150, 121),
    l::Op::And(151, 126),
    l::Op::And(152, 129),
    l::Op::And(153, 132),
    l::Op::And(154, 134),
    l::Op::And(155, 136),
    l::Op::And(156, 140),
    l::Op::And(157, 144),
    l::Op::Select(110, 158, 0),
    l::Op::Not(112),
    l::Op::And(0, 33),
    l::Op::And(161, 57),
    l::Op::And(162, 109),
    l::Op::And(163, 159),
    l::Op::And(164, 160),
];
/// Original scoped laws plus independent structural requirements.
pub const LAWS: &[l::Law<'static>] = &[
    l::Law {
        id: 500,
        kind: l::Kind::StateInvariant,
        scope: l::Scope::Committing,
        genesis: true,
        program: l::Program {
            nodes: LAW_500,
            root: 16,
        },
    },
    l::Law {
        id: 501,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_501,
            root: 21,
        },
    },
    l::Law {
        id: 502,
        kind: l::Kind::CommittedFailureEffects,
        scope: l::Scope::CommittedFailure,
        genesis: false,
        program: l::Program {
            nodes: LAW_502,
            root: 21,
        },
    },
    l::Law {
        id: 503,
        kind: l::Kind::RejectNoAuthority,
        scope: l::Scope::Reject,
        genesis: false,
        program: l::Program {
            nodes: LAW_503,
            root: 0,
        },
    },
    l::Law {
        id: 909,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Reject,
        genesis: false,
        program: l::Program {
            nodes: LAW_909,
            root: 17,
        },
    },
    l::Law {
        id: 990,
        kind: l::Kind::InitialCondition,
        scope: l::Scope::Always,
        genesis: true,
        program: l::Program {
            nodes: LAW_990,
            root: 7,
        },
    },
    l::Law {
        id: 991,
        kind: l::Kind::DecisionConformance,
        scope: l::Scope::Always,
        genesis: false,
        program: l::Program {
            nodes: LAW_991,
            root: 165,
        },
    },
];
/// No declared law is optional at descriptor admission.
pub const REQUIRED: &[u32] = &[500, 501, 502, 503, 909, 990, 991];
/// Owns declarative input/output type tables; evaluation stays in the library.
pub struct Contract {
    state: Vec<InputField>,
    command: InputLeaf,
    context: InputLeaf,
    output_types: Vec<InputLeaf>,
}
impl Default for Contract {
    fn default() -> Self {
        Self::new()
    }
}
impl Contract {
    /// Allocate only fixed declarative type tables.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: vec![
                InputField {
                    id: 110,
                    leaf: InputLeaf::I128 { min: 0, max: 3 },
                },
                InputField {
                    id: 111,
                    leaf: InputLeaf::I128 { min: 0, max: 3 },
                },
            ],
            command: InputLeaf::Sum {
                type_id: 101,
                min: 120,
                max: 121,
                variants: vec![
                    InputVariant { id: 120, code: 120 },
                    InputVariant { id: 121, code: 121 },
                ],
            },
            context: InputLeaf::Bool,
            output_types: vec![
                InputLeaf::I128 { min: 0, max: 3 },
                InputLeaf::I128 {
                    min: -9223372036854775808,
                    max: 9223372036854775807,
                },
                InputLeaf::I128 {
                    min: -9223372036854775808,
                    max: 9223372036854775807,
                },
            ],
        }
    }
    /// Borrow the full raw-input, complete-decision and law contract.
    #[must_use]
    pub fn descriptor(&self) -> c::Descriptor<'_> {
        c::Descriptor {
            state: c::Schema::Record(&self.state),
            command: c::Schema::Leaf(&self.command),
            context: c::Schema::Leaf(&self.context),
            program: V2ScalarProgram {
                inputs: &[
                    ScalarDomain::Int { min: 0, max: 3 },
                    ScalarDomain::Int { min: 0, max: 3 },
                    ScalarDomain::Int { min: 120, max: 121 },
                    ScalarDomain::Bool,
                ],
                outputs: &[
                    ScalarDomain::Int { min: 0, max: 3 },
                    ScalarDomain::Int {
                        min: -9223372036854775808,
                        max: 9223372036854775807,
                    },
                    ScalarDomain::Int {
                        min: -9223372036854775808,
                        max: 9223372036854775807,
                    },
                ],
                nodes: &[
                    Op::Input(3),
                    Op::Not(0),
                    Op::Int(0),
                    Op::Input(2),
                    Op::Int(120),
                    Op::Eq(3, 4),
                    Op::Input(0),
                    Op::Int(3),
                    Op::Eq(6, 7),
                    Op::Input(1),
                    Op::Eq(9, 7),
                    Op::Select(5, 8, 10),
                    Op::Int(1),
                    Op::Int(2),
                    Op::Select(5, 13, 7),
                    Op::Select(11, 12, 14),
                    Op::Select(1, 2, 15),
                    Op::Add(6, 12),
                    Op::Add(9, 12),
                ],
                roots: &[16, 17, 18],
            },
            bindings: &[
                c::Binding {
                    source: c::Source::State,
                    selector: c::Selector::Field(110),
                },
                c::Binding {
                    source: c::Source::State,
                    selector: c::Selector::Field(111),
                },
                c::Binding {
                    source: c::Source::Command,
                    selector: c::Selector::Root,
                },
                c::Binding {
                    source: c::Source::Context,
                    selector: c::Selector::Root,
                },
            ],
            output_types: &self.output_types,
            decision_output: 0,
            branches: BRANCHES,
            reasons: &[
                c::Reason {
                    id: 200,
                    class: c::Class::Reject,
                },
                c::Reason {
                    id: 201,
                    class: c::Class::Reject,
                },
                c::Reason {
                    id: 202,
                    class: c::Class::CommittedFailure,
                },
            ],
            channels: &[c::Channel {
                id: 300,
                destination: c::Domain::Text,
                payload: &[
                    c::TypedField {
                        field: 112,
                        domain: c::Domain::I128 { min: 0, max: 3 },
                    },
                    c::TypedField {
                        field: 113,
                        domain: c::Domain::I128 { min: 0, max: 3 },
                    },
                ],
                idempotency: c::Domain::U128 { min: 0, max: 0 },
            }],
            laws: LAWS,
            required: REQUIRED,
            limits: v2_zero_limits()
                .with_limit(Resource::Read, 124)
                .with_limit(Resource::Write, 2)
                .with_limit(Resource::Candidate, 1)
                .with_limit(Resource::Effect, 1)
                .with_limit(Resource::Byte, 196)
                .with_limit(Resource::WitnessByte, 0)
                .with_limit(Resource::Depth, 0)
                .with_limit(Resource::Step, 278),
        }
    }
}
/// Exact complete schema/policy correspondence precedes Authority construction.
pub fn checked_catalog<'a>(
    descriptor: &'a c::Descriptor<'a>,
) -> Result<catalog::BoundCatalog<'a>, catalog::Failure> {
    catalog::bind_original(
        ORIGINAL_SCHEMA,
        &DESCRIPTION,
        catalog::Limits {
            schema: s::Limits {
                bytes: ORIGINAL_SCHEMA.len() as u64,
                types: 6,
                fields: 2,
                variants: 2,
            },
            contract_bytes: ORIGINAL_POLICY.len() as u64,
        },
        ORIGINAL_POLICY,
        descriptor,
        &FRAMING,
        CHANNEL_ROOTS,
    )
}
/// Ordered refusal at checked catalog or private Authority construction.
#[derive(Debug)]
#[non_exhaustive]
pub enum BindFailure {
    /// Complete original schema/policy correspondence refused.
    Catalog(catalog::Failure),
    /// Private library source binding refused.
    Authority(authority::Refusal),
}
/// Sole supplied production constructor; identity comes from the checked library.
pub fn checked_authority<'a>(
    descriptor: &'a c::Descriptor<'a>,
) -> Result<authority::Authority<'a>, BindFailure> {
    let catalog = checked_catalog(descriptor).map_err(BindFailure::Catalog)?;
    authority::bind(&catalog).map_err(BindFailure::Authority)
}
