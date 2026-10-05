// Generated declarative data. Review v2/policy.json and project.zeno.
// Regenerate/check with tools/check_template_contracts_v2.py; no runtime mapper.
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
    profile: b"account_lockout",
    version: 1,
    root: 100,
    definitions: &[
        s::Definition {
            id: 100,
            name: b"Account",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 110,
                    name: b"failed_attempts",
                    type_id: 105,
                },
                s::Field {
                    id: 111,
                    name: b"locked_until",
                    type_id: 107,
                },
                s::Field {
                    id: 112,
                    name: b"last_seen",
                    type_id: 106,
                },
            ]),
        },
        s::Definition {
            id: 101,
            name: b"AccountCommand",
            kind: s::Kind::Sum(&[
                s::Variant {
                    id: 120,
                    name: b"LoginSucceeded",
                },
                s::Variant {
                    id: 121,
                    name: b"LoginFailed",
                },
                s::Variant {
                    id: 122,
                    name: b"AdminUnlock",
                },
            ]),
        },
        s::Definition {
            id: 102,
            name: b"RequestContext",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 130,
                    name: b"now",
                    type_id: 106,
                },
                s::Field {
                    id: 131,
                    name: b"admin",
                    type_id: 108,
                },
            ]),
        },
        s::Definition {
            id: 103,
            name: b"AlertDestination",
            kind: s::Kind::Text { min: 1, max: 32 },
        },
        s::Definition {
            id: 104,
            name: b"SecurityAlert",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 140,
                    name: b"alert_kind",
                    type_id: 109,
                },
                s::Field {
                    id: 141,
                    name: b"alert_until",
                    type_id: 107,
                },
            ]),
        },
        s::Definition {
            id: 105,
            name: b"Attempts",
            kind: s::Kind::I128 { min: 0, max: 2 },
        },
        s::Definition {
            id: 106,
            name: b"UnixTime",
            kind: s::Kind::I128 {
                min: 0,
                max: 4102444800,
            },
        },
        s::Definition {
            id: 107,
            name: b"LockDeadline",
            kind: s::Kind::I128 {
                min: 0,
                max: 4102445700,
            },
        },
        s::Definition {
            id: 108,
            name: b"AdminFlag",
            kind: s::Kind::Bool,
        },
        s::Definition {
            id: 109,
            name: b"AlertKind",
            kind: s::Kind::Sum(&[
                s::Variant {
                    id: 150,
                    name: b"Locked",
                },
                s::Variant {
                    id: 151,
                    name: b"Unlocked",
                },
            ]),
        },
    ],
};
/// Original root and schema commitments and complete wire-size limits.
pub const FRAMING: c::Framing = c::Framing {
    state: c::FrameBinding {
        root: 100,
        schema: [
            26, 42, 134, 226, 94, 136, 112, 207, 35, 227, 94, 60, 172, 73, 102, 213, 164, 158, 182,
            226, 181, 37, 134, 15, 253, 49, 109, 99, 21, 210, 101, 204,
        ],
        max_bytes: 110,
    },
    command: c::FrameBinding {
        root: 101,
        schema: [
            26, 42, 134, 226, 94, 136, 112, 207, 35, 227, 94, 60, 172, 73, 102, 213, 164, 158, 182,
            226, 181, 37, 134, 15, 253, 49, 109, 99, 21, 210, 101, 204,
        ],
        max_bytes: 56,
    },
    context: c::FrameBinding {
        root: 102,
        schema: [
            26, 42, 134, 226, 94, 136, 112, 207, 35, 227, 94, 60, 172, 73, 102, 213, 164, 158, 182,
            226, 181, 37, 134, 15, 253, 49, 109, 99, 21, 210, 101, 204,
        ],
        max_bytes: 75,
    },
};
/// Exact channel to original destination and payload type links.
pub const CHANNEL_ROOTS: &[(u32, u32, u32)] = &[(300, 103, 104)];
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
        class: c::Class::Reject,
        reason: Some(202),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 3,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Constant(c::Atom::I128(0)),
                domain: c::Domain::I128 { min: 0, max: 2 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Input(c::Source::State, 111),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102445700,
                },
            },
            c::Assignment {
                field: 112,
                value: c::Expr::Input(c::Source::Context, 130),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102444800,
                },
            },
        ],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 4,
        class: c::Class::CommittedFailure,
        reason: Some(203),
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Constant(c::Atom::I128(0)),
                domain: c::Domain::I128 { min: 0, max: 2 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Output(1),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102445700,
                },
            },
            c::Assignment {
                field: 112,
                value: c::Expr::Input(c::Source::Context, 130),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102444800,
                },
            },
        ],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"security-team")),
            payload: &[
                c::PayloadField {
                    field: 140,
                    value: c::Expr::Constant(c::Atom::Sum {
                        type_id: 109,
                        variant: 150,
                    }),
                },
                c::PayloadField {
                    field: 141,
                    value: c::Expr::Output(1),
                },
            ],
            idempotency: c::Expr::Constant(c::Atom::U128(0)),
        }],
    },
    c::Branch {
        code: 5,
        class: c::Class::CommittedFailure,
        reason: Some(203),
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Output(2),
                domain: c::Domain::I128 { min: 0, max: 2 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Input(c::Source::State, 111),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102445700,
                },
            },
            c::Assignment {
                field: 112,
                value: c::Expr::Input(c::Source::Context, 130),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102444800,
                },
            },
        ],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 6,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Constant(c::Atom::I128(0)),
                domain: c::Domain::I128 { min: 0, max: 2 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Constant(c::Atom::I128(0)),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102445700,
                },
            },
            c::Assignment {
                field: 112,
                value: c::Expr::Input(c::Source::Context, 130),
                domain: c::Domain::I128 {
                    min: 0,
                    max: 4102444800,
                },
            },
        ],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"security-team")),
            payload: &[
                c::PayloadField {
                    field: 140,
                    value: c::Expr::Constant(c::Atom::Sum {
                        type_id: 109,
                        variant: 151,
                    }),
                },
                c::PayloadField {
                    field: 141,
                    value: c::Expr::Constant(c::Atom::I128(0)),
                },
            ],
            idempotency: c::Expr::Constant(c::Atom::U128(0)),
        }],
    },
];
const LAW_500: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(0),
    l::Op::Observe(l::Observation::Post(112)),
    l::Op::ToI128(2),
    l::Op::Literal(l::Atom::I128(900)),
    l::Op::Add(3, 4),
    l::Op::Lt(5, 1),
    l::Op::Not(6),
    l::Op::Lt(3, 1),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(9),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(10, 11),
    l::Op::Not(8),
    l::Op::Not(13),
    l::Op::Not(12),
    l::Op::And(14, 15),
    l::Op::Not(16),
    l::Op::And(7, 17),
];
const LAW_501: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(121)),
    l::Op::Eq(1, 2),
    l::Op::Not(3),
    l::Op::Literal(l::Atom::I128(120)),
    l::Op::Eq(1, 5),
    l::Op::Observe(l::Observation::Context(130)),
    l::Op::ToI128(7),
    l::Op::Observe(l::Observation::Pre(112)),
    l::Op::ToI128(9),
    l::Op::Lt(8, 10),
    l::Op::Not(11),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(13),
    l::Op::Lt(8, 14),
    l::Op::Not(15),
    l::Op::And(12, 16),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(18),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(19, 20),
    l::Op::And(17, 21),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(23),
    l::Op::Eq(24, 14),
    l::Op::And(22, 25),
    l::Op::Observe(l::Observation::Post(112)),
    l::Op::ToI128(27),
    l::Op::Eq(28, 8),
    l::Op::And(26, 29),
    l::Op::Not(6),
    l::Op::Not(31),
    l::Op::Not(30),
    l::Op::And(32, 33),
    l::Op::Not(34),
    l::Op::And(4, 35),
];
const LAW_502: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(122)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::Context(131)),
    l::Op::ToI128(4),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(5, 6),
    l::Op::Observe(l::Observation::Context(130)),
    l::Op::ToI128(8),
    l::Op::Observe(l::Observation::Pre(112)),
    l::Op::ToI128(10),
    l::Op::Lt(9, 11),
    l::Op::Not(12),
    l::Op::And(7, 13),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(15),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(16, 17),
    l::Op::And(14, 18),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(20),
    l::Op::Eq(21, 17),
    l::Op::And(19, 22),
    l::Op::Observe(l::Observation::Post(112)),
    l::Op::ToI128(24),
    l::Op::Eq(25, 9),
    l::Op::And(23, 26),
    l::Op::Not(3),
    l::Op::Not(28),
    l::Op::Not(27),
    l::Op::And(29, 30),
    l::Op::Not(31),
];
const LAW_503: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(121)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::Context(130)),
    l::Op::ToI128(4),
    l::Op::Observe(l::Observation::Pre(112)),
    l::Op::ToI128(6),
    l::Op::Lt(5, 7),
    l::Op::Not(8),
    l::Op::And(3, 9),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(11),
    l::Op::Lt(5, 12),
    l::Op::Not(13),
    l::Op::And(10, 14),
    l::Op::Observe(l::Observation::Post(112)),
    l::Op::ToI128(16),
    l::Op::Eq(17, 5),
    l::Op::And(15, 18),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(20),
    l::Op::Literal(l::Atom::I128(2)),
    l::Op::Lt(21, 22),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(24),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Add(21, 26),
    l::Op::Eq(25, 27),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(29),
    l::Op::Eq(30, 12),
    l::Op::And(28, 31),
    l::Op::Not(23),
    l::Op::Not(33),
    l::Op::Not(32),
    l::Op::And(34, 35),
    l::Op::Not(36),
    l::Op::And(19, 37),
    l::Op::Eq(21, 22),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(25, 40),
    l::Op::Literal(l::Atom::I128(900)),
    l::Op::Add(5, 42),
    l::Op::Eq(30, 43),
    l::Op::And(41, 44),
    l::Op::Not(39),
    l::Op::Not(46),
    l::Op::Not(45),
    l::Op::And(47, 48),
    l::Op::Not(49),
    l::Op::And(38, 50),
];
const LAW_509: &[l::Op<'static>] = &[
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
    l::Op::Observe(l::Observation::Initial(112)),
    l::Op::Eq(5, 1),
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::And(7, 2),
    l::Op::And(8, 4),
    l::Op::And(9, 6),
];
const LAW_991: &[l::Op<'static>] = &[
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::Observe(l::Observation::Context(130)),
    l::Op::ToI128(1),
    l::Op::Observe(l::Observation::Pre(112)),
    l::Op::ToI128(3),
    l::Op::Lt(2, 4),
    l::Op::And(0, 5),
    l::Op::Not(5),
    l::Op::And(0, 7),
    l::Op::Observe(l::Observation::Class),
    l::Op::ToI128(9),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(10, 11),
    l::Op::Observe(l::Observation::HasReason),
    l::Op::ObserveWhen(6, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(14),
    l::Op::Literal(l::Atom::I128(200)),
    l::Op::Eq(15, 16),
    l::Op::Observe(l::Observation::EffectLength),
    l::Op::ToI128(18),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(19, 20),
    l::Op::Observe(l::Observation::OutboxLength),
    l::Op::ToI128(22),
    l::Op::Eq(23, 20),
    l::Op::Observe(l::Observation::PostLength),
    l::Op::ToI128(25),
    l::Op::Eq(26, 20),
    l::Op::And(0, 12),
    l::Op::And(28, 13),
    l::Op::And(29, 17),
    l::Op::And(30, 21),
    l::Op::And(31, 24),
    l::Op::And(32, 27),
    l::Op::Select(6, 33, 0),
    l::Op::Observe(l::Observation::CommandRoot),
    l::Op::ToI128(35),
    l::Op::Literal(l::Atom::I128(122)),
    l::Op::Eq(36, 37),
    l::Op::Not(38),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(40),
    l::Op::Lt(2, 41),
    l::Op::And(39, 42),
    l::Op::And(8, 43),
    l::Op::Not(43),
    l::Op::And(8, 45),
    l::Op::ObserveWhen(44, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(47),
    l::Op::Literal(l::Atom::I128(201)),
    l::Op::Eq(48, 49),
    l::Op::And(29, 50),
    l::Op::And(51, 21),
    l::Op::And(52, 24),
    l::Op::And(53, 27),
    l::Op::Select(44, 54, 0),
    l::Op::Observe(l::Observation::Context(131)),
    l::Op::ToI128(56),
    l::Op::Eq(57, 11),
    l::Op::Not(58),
    l::Op::And(38, 59),
    l::Op::And(46, 60),
    l::Op::Not(60),
    l::Op::And(46, 62),
    l::Op::ObserveWhen(61, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(64),
    l::Op::Literal(l::Atom::I128(202)),
    l::Op::Eq(65, 66),
    l::Op::And(29, 67),
    l::Op::And(68, 21),
    l::Op::And(69, 24),
    l::Op::And(70, 27),
    l::Op::Select(61, 71, 0),
    l::Op::Literal(l::Atom::I128(120)),
    l::Op::Eq(36, 73),
    l::Op::And(63, 74),
    l::Op::Not(74),
    l::Op::And(63, 76),
    l::Op::Eq(10, 20),
    l::Op::Not(13),
    l::Op::Literal(l::Atom::I128(3)),
    l::Op::Eq(26, 80),
    l::Op::ObserveWhen(75, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Eq(82, 83),
    l::Op::Select(75, 84, 0),
    l::Op::ObserveWhen(75, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(86),
    l::Op::Eq(87, 41),
    l::Op::Select(75, 88, 0),
    l::Op::ObserveWhen(75, l::Observation::Post(112), l::Atom::I128(0)),
    l::Op::ToI128(90),
    l::Op::Eq(91, 2),
    l::Op::Select(75, 92, 0),
    l::Op::And(0, 78),
    l::Op::And(94, 79),
    l::Op::And(95, 21),
    l::Op::And(96, 24),
    l::Op::And(97, 81),
    l::Op::And(98, 85),
    l::Op::And(99, 89),
    l::Op::And(100, 93),
    l::Op::Select(75, 101, 0),
    l::Op::Literal(l::Atom::I128(121)),
    l::Op::Eq(36, 103),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(105),
    l::Op::Literal(l::Atom::I128(2)),
    l::Op::Eq(106, 107),
    l::Op::And(104, 108),
    l::Op::And(77, 109),
    l::Op::Not(109),
    l::Op::And(77, 111),
    l::Op::Eq(10, 107),
    l::Op::ObserveWhen(110, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(114),
    l::Op::Literal(l::Atom::I128(203)),
    l::Op::Eq(115, 116),
    l::Op::Eq(23, 11),
    l::Op::ObserveWhen(110, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::Eq(119, 83),
    l::Op::Select(110, 120, 0),
    l::Op::ObserveWhen(110, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(122),
    l::Op::Literal(l::Atom::I128(900)),
    l::Op::Add(2, 124),
    l::Op::Eq(123, 125),
    l::Op::Select(110, 126, 0),
    l::Op::ObserveWhen(110, l::Observation::Post(112), l::Atom::I128(0)),
    l::Op::ToI128(128),
    l::Op::Eq(129, 2),
    l::Op::Select(110, 130, 0),
    l::Op::ObserveWhen(110, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(132),
    l::Op::Eq(133, 20),
    l::Op::ObserveWhen(110, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(135),
    l::Op::Literal(l::Atom::I128(300)),
    l::Op::Eq(136, 137),
    l::Op::ObserveWhen(
        110,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"security-team"),
    ),
    l::Op::Literal(l::Atom::Text(b"security-team")),
    l::Op::Eq(139, 140),
    l::Op::ObserveWhen(110, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Literal(l::Atom::U128(0)),
    l::Op::Eq(142, 143),
    l::Op::ObserveWhen(
        110,
        l::Observation::OutboxPayload(0, 140),
        l::Atom::Sum {
            type_id: 109,
            variant: 150,
        },
    ),
    l::Op::Literal(l::Atom::Sum {
        type_id: 109,
        variant: 150,
    }),
    l::Op::Eq(145, 146),
    l::Op::Select(110, 147, 0),
    l::Op::ObserveWhen(110, l::Observation::OutboxPayload(0, 141), l::Atom::I128(0)),
    l::Op::ToI128(149),
    l::Op::Eq(150, 125),
    l::Op::Select(110, 151, 0),
    l::Op::And(0, 113),
    l::Op::And(153, 13),
    l::Op::And(154, 117),
    l::Op::And(155, 21),
    l::Op::And(156, 118),
    l::Op::And(157, 81),
    l::Op::And(158, 121),
    l::Op::And(159, 127),
    l::Op::And(160, 131),
    l::Op::And(161, 134),
    l::Op::And(162, 138),
    l::Op::And(163, 141),
    l::Op::And(164, 144),
    l::Op::And(165, 148),
    l::Op::And(166, 152),
    l::Op::Select(110, 167, 0),
    l::Op::And(112, 104),
    l::Op::Not(104),
    l::Op::And(112, 170),
    l::Op::ObserveWhen(169, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(172),
    l::Op::Eq(173, 116),
    l::Op::ObserveWhen(169, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(175),
    l::Op::Add(106, 11),
    l::Op::Eq(176, 177),
    l::Op::Select(169, 178, 0),
    l::Op::ObserveWhen(169, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(180),
    l::Op::Eq(181, 41),
    l::Op::Select(169, 182, 0),
    l::Op::ObserveWhen(169, l::Observation::Post(112), l::Atom::I128(0)),
    l::Op::ToI128(184),
    l::Op::Eq(185, 2),
    l::Op::Select(169, 186, 0),
    l::Op::And(154, 174),
    l::Op::And(188, 21),
    l::Op::And(189, 24),
    l::Op::And(190, 81),
    l::Op::And(191, 179),
    l::Op::And(192, 183),
    l::Op::And(193, 187),
    l::Op::Select(169, 194, 0),
    l::Op::And(171, 0),
    l::Op::Not(0),
    l::Op::And(171, 197),
    l::Op::ObserveWhen(196, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::Eq(199, 83),
    l::Op::Select(196, 200, 0),
    l::Op::ObserveWhen(196, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::Eq(202, 83),
    l::Op::Select(196, 203, 0),
    l::Op::ObserveWhen(196, l::Observation::Post(112), l::Atom::I128(0)),
    l::Op::ToI128(205),
    l::Op::Eq(206, 2),
    l::Op::Select(196, 207, 0),
    l::Op::ObserveWhen(196, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(209),
    l::Op::Eq(210, 20),
    l::Op::ObserveWhen(196, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(212),
    l::Op::Eq(213, 137),
    l::Op::ObserveWhen(
        196,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"security-team"),
    ),
    l::Op::Eq(215, 140),
    l::Op::ObserveWhen(196, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Eq(217, 143),
    l::Op::ObserveWhen(
        196,
        l::Observation::OutboxPayload(0, 140),
        l::Atom::Sum {
            type_id: 109,
            variant: 150,
        },
    ),
    l::Op::Literal(l::Atom::Sum {
        type_id: 109,
        variant: 151,
    }),
    l::Op::Eq(219, 220),
    l::Op::Select(196, 221, 0),
    l::Op::ObserveWhen(196, l::Observation::OutboxPayload(0, 141), l::Atom::I128(0)),
    l::Op::Eq(223, 83),
    l::Op::Select(196, 224, 0),
    l::Op::And(96, 118),
    l::Op::And(226, 81),
    l::Op::And(227, 201),
    l::Op::And(228, 204),
    l::Op::And(229, 208),
    l::Op::And(230, 211),
    l::Op::And(231, 214),
    l::Op::And(232, 216),
    l::Op::And(233, 218),
    l::Op::And(234, 222),
    l::Op::And(235, 225),
    l::Op::Select(196, 236, 0),
    l::Op::Not(198),
    l::Op::And(0, 34),
    l::Op::And(239, 55),
    l::Op::And(240, 72),
    l::Op::And(241, 102),
    l::Op::And(242, 168),
    l::Op::And(243, 195),
    l::Op::And(244, 237),
    l::Op::And(245, 238),
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
            root: 18,
        },
    },
    l::Law {
        id: 501,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_501,
            root: 36,
        },
    },
    l::Law {
        id: 502,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_502,
            root: 32,
        },
    },
    l::Law {
        id: 503,
        kind: l::Kind::CommittedFailureEffects,
        scope: l::Scope::CommittedFailure,
        genesis: false,
        program: l::Program {
            nodes: LAW_503,
            root: 51,
        },
    },
    l::Law {
        id: 509,
        kind: l::Kind::RejectNoAuthority,
        scope: l::Scope::Reject,
        genesis: false,
        program: l::Program {
            nodes: LAW_509,
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
            root: 10,
        },
    },
    l::Law {
        id: 991,
        kind: l::Kind::DecisionConformance,
        scope: l::Scope::Always,
        genesis: false,
        program: l::Program {
            nodes: LAW_991,
            root: 246,
        },
    },
];
/// No declared law is optional at descriptor admission.
pub const REQUIRED: &[u32] = &[500, 501, 502, 503, 509, 990, 991];
/// Owns declarative input/output type tables; evaluation stays in the library.
pub struct Contract {
    state: Vec<InputField>,
    command: InputLeaf,
    context: Vec<InputField>,
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
                    leaf: InputLeaf::I128 { min: 0, max: 2 },
                },
                InputField {
                    id: 111,
                    leaf: InputLeaf::I128 {
                        min: 0,
                        max: 4102445700,
                    },
                },
                InputField {
                    id: 112,
                    leaf: InputLeaf::I128 {
                        min: 0,
                        max: 4102444800,
                    },
                },
            ],
            command: InputLeaf::Sum {
                type_id: 101,
                min: 120,
                max: 122,
                variants: vec![
                    InputVariant { id: 120, code: 120 },
                    InputVariant { id: 121, code: 121 },
                    InputVariant { id: 122, code: 122 },
                ],
            },
            context: vec![
                InputField {
                    id: 130,
                    leaf: InputLeaf::I128 {
                        min: 0,
                        max: 4102444800,
                    },
                },
                InputField {
                    id: 131,
                    leaf: InputLeaf::Bool,
                },
            ],
            output_types: vec![
                InputLeaf::I128 { min: 0, max: 6 },
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
            context: c::Schema::Record(&self.context),
            program: V2ScalarProgram {
                inputs: &[
                    ScalarDomain::Int { min: 0, max: 2 },
                    ScalarDomain::Int {
                        min: 0,
                        max: 4102445700,
                    },
                    ScalarDomain::Int {
                        min: 0,
                        max: 4102444800,
                    },
                    ScalarDomain::Int { min: 120, max: 122 },
                    ScalarDomain::Int {
                        min: 0,
                        max: 4102444800,
                    },
                    ScalarDomain::Bool,
                ],
                outputs: &[
                    ScalarDomain::Int { min: 0, max: 6 },
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
                    Op::Input(4),
                    Op::Input(2),
                    Op::Lt(0, 1),
                    Op::Int(0),
                    Op::Input(3),
                    Op::Int(122),
                    Op::Eq(4, 5),
                    Op::Not(6),
                    Op::Input(1),
                    Op::Lt(0, 8),
                    Op::And(7, 9),
                    Op::Int(1),
                    Op::Input(5),
                    Op::Not(12),
                    Op::And(6, 13),
                    Op::Int(2),
                    Op::Int(120),
                    Op::Eq(4, 16),
                    Op::Int(3),
                    Op::Int(121),
                    Op::Eq(4, 19),
                    Op::Input(0),
                    Op::Eq(21, 15),
                    Op::And(20, 22),
                    Op::Int(4),
                    Op::Int(5),
                    Op::Int(6),
                    Op::Select(20, 25, 26),
                    Op::Select(23, 24, 27),
                    Op::Select(17, 18, 28),
                    Op::Select(14, 15, 29),
                    Op::Select(10, 11, 30),
                    Op::Select(2, 3, 31),
                    Op::Int(900),
                    Op::Add(0, 33),
                    Op::Add(21, 11),
                ],
                roots: &[32, 34, 35],
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
                    source: c::Source::State,
                    selector: c::Selector::Field(112),
                },
                c::Binding {
                    source: c::Source::Command,
                    selector: c::Selector::Root,
                },
                c::Binding {
                    source: c::Source::Context,
                    selector: c::Selector::Field(130),
                },
                c::Binding {
                    source: c::Source::Context,
                    selector: c::Selector::Field(131),
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
                    class: c::Class::Reject,
                },
                c::Reason {
                    id: 203,
                    class: c::Class::CommittedFailure,
                },
            ],
            channels: &[c::Channel {
                id: 300,
                destination: c::Domain::Text,
                payload: &[
                    c::TypedField {
                        field: 140,
                        domain: c::Domain::Sum {
                            type_id: 109,
                            variants: &[150, 151],
                        },
                    },
                    c::TypedField {
                        field: 141,
                        domain: c::Domain::I128 {
                            min: 0,
                            max: 4102445700,
                        },
                    },
                ],
                idempotency: c::Domain::U128 { min: 0, max: 0 },
            }],
            laws: LAWS,
            required: REQUIRED,
            limits: v2_zero_limits()
                .with_limit(Resource::Read, 154)
                .with_limit(Resource::Write, 3)
                .with_limit(Resource::Candidate, 1)
                .with_limit(Resource::Effect, 1)
                .with_limit(Resource::Byte, 241)
                .with_limit(Resource::WitnessByte, 0)
                .with_limit(Resource::Depth, 0)
                .with_limit(Resource::Step, 458),
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
                types: 10,
                fields: 3,
                variants: 3,
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
