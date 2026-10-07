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
    profile: b"inventory_reservation",
    version: 1,
    root: 100,
    definitions: &[
        s::Definition {
            id: 100,
            name: b"Stock",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 110,
                    name: b"available",
                    type_id: 105,
                },
                s::Field {
                    id: 111,
                    name: b"reserved",
                    type_id: 105,
                },
            ]),
        },
        s::Definition {
            id: 101,
            name: b"StockCommand",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 120,
                    name: b"action",
                    type_id: 107,
                },
                s::Field {
                    id: 121,
                    name: b"quantity",
                    type_id: 106,
                },
            ]),
        },
        s::Definition {
            id: 102,
            name: b"StockContext",
            kind: s::Kind::Record(&[s::Field {
                id: 130,
                name: b"authorized",
                type_id: 108,
            }]),
        },
        s::Definition {
            id: 103,
            name: b"WarehouseDestination",
            kind: s::Kind::Text { min: 1, max: 32 },
        },
        s::Definition {
            id: 104,
            name: b"ShipmentRequest",
            kind: s::Kind::Record(&[s::Field {
                id: 140,
                name: b"shipped_units",
                type_id: 106,
            }]),
        },
        s::Definition {
            id: 105,
            name: b"Units",
            kind: s::Kind::I128 { min: 0, max: 5 },
        },
        s::Definition {
            id: 106,
            name: b"Quantity",
            kind: s::Kind::I128 { min: 1, max: 3 },
        },
        s::Definition {
            id: 107,
            name: b"StockAction",
            kind: s::Kind::Sum(&[
                s::Variant {
                    id: 150,
                    name: b"Reserve",
                },
                s::Variant {
                    id: 151,
                    name: b"Release",
                },
                s::Variant {
                    id: 152,
                    name: b"Ship",
                },
                s::Variant {
                    id: 153,
                    name: b"Restock",
                },
            ]),
        },
        s::Definition {
            id: 108,
            name: b"OperatorFlag",
            kind: s::Kind::Bool,
        },
    ],
};
/// Original root and schema commitments and complete wire-size limits.
pub const FRAMING: c::Framing = c::Framing {
    state: c::FrameBinding {
        root: 100,
        schema: [
            1, 133, 11, 78, 105, 86, 1, 239, 0, 182, 150, 41, 7, 223, 205, 208, 78, 230, 240, 64,
            162, 82, 114, 35, 180, 23, 142, 148, 151, 209, 132, 108,
        ],
        max_bytes: 91,
    },
    command: c::FrameBinding {
        root: 101,
        schema: [
            1, 133, 11, 78, 105, 86, 1, 239, 0, 182, 150, 41, 7, 223, 205, 208, 78, 230, 240, 64,
            162, 82, 114, 35, 180, 23, 142, 148, 151, 209, 132, 108,
        ],
        max_bytes: 82,
    },
    context: c::FrameBinding {
        root: 102,
        schema: [
            1, 133, 11, 78, 105, 86, 1, 239, 0, 182, 150, 41, 7, 223, 205, 208, 78, 230, 240, 64,
            162, 82, 114, 35, 180, 23, 142, 148, 151, 209, 132, 108,
        ],
        max_bytes: 56,
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
        class: c::Class::Reject,
        reason: Some(202),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 3,
        class: c::Class::Reject,
        reason: Some(203),
        assignments: &[],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 4,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Output(1),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Output(2),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
        ],
        effects: &[],
        outbox: &[],
    },
    c::Branch {
        code: 5,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Output(3),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Output(4),
                domain: c::Domain::I128 { min: 0, max: 5 },
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
                value: c::Expr::Input(c::Source::State, 110),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Output(4),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
        ],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"warehouse")),
            payload: &[c::PayloadField {
                field: 140,
                value: c::Expr::Input(c::Source::Command, 121),
            }],
            idempotency: c::Expr::Constant(c::Atom::U128(0)),
        }],
    },
    c::Branch {
        code: 7,
        class: c::Class::Accept,
        reason: None,
        assignments: &[
            c::Assignment {
                field: 110,
                value: c::Expr::Output(3),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
            c::Assignment {
                field: 111,
                value: c::Expr::Input(c::Source::State, 111),
                domain: c::Domain::I128 { min: 0, max: 5 },
            },
        ],
        effects: &[],
        outbox: &[],
    },
];
const LAW_500: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Lt(1, 2),
    l::Op::Not(3),
    l::Op::Literal(l::Atom::I128(5)),
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
    l::Op::Observe(l::Observation::Command(120)),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(150)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(4),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(6),
    l::Op::Add(5, 7),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(9),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(11),
    l::Op::Add(10, 12),
    l::Op::Eq(8, 13),
    l::Op::Not(3),
    l::Op::Not(15),
    l::Op::Not(14),
    l::Op::And(16, 17),
    l::Op::Not(18),
    l::Op::Literal(l::Atom::I128(151)),
    l::Op::Eq(1, 20),
    l::Op::Not(21),
    l::Op::Not(22),
    l::Op::And(23, 17),
    l::Op::Not(24),
    l::Op::And(19, 25),
    l::Op::Literal(l::Atom::I128(152)),
    l::Op::Eq(1, 27),
    l::Op::Observe(l::Observation::Command(121)),
    l::Op::ToI128(29),
    l::Op::Add(8, 30),
    l::Op::Eq(31, 13),
    l::Op::Not(28),
    l::Op::Not(33),
    l::Op::Not(32),
    l::Op::And(34, 35),
    l::Op::Not(36),
    l::Op::And(26, 37),
    l::Op::Literal(l::Atom::I128(153)),
    l::Op::Eq(1, 39),
    l::Op::Add(13, 30),
    l::Op::Eq(8, 41),
    l::Op::Not(40),
    l::Op::Not(43),
    l::Op::Not(42),
    l::Op::And(44, 45),
    l::Op::Not(46),
    l::Op::And(38, 47),
];
const LAW_502: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::Context(130)),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::Command(120)),
    l::Op::ToI128(4),
    l::Op::Literal(l::Atom::I128(150)),
    l::Op::Eq(5, 6),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(8),
    l::Op::Observe(l::Observation::Command(121)),
    l::Op::ToI128(10),
    l::Op::Add(9, 11),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(13),
    l::Op::Eq(12, 14),
    l::Op::Observe(l::Observation::Post(111)),
    l::Op::ToI128(16),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(18),
    l::Op::Add(19, 11),
    l::Op::Eq(17, 20),
    l::Op::And(15, 21),
    l::Op::Not(7),
    l::Op::Not(23),
    l::Op::Not(22),
    l::Op::And(24, 25),
    l::Op::Not(26),
    l::Op::And(3, 27),
    l::Op::Literal(l::Atom::I128(151)),
    l::Op::Eq(5, 29),
    l::Op::Add(14, 11),
    l::Op::Eq(9, 31),
    l::Op::Add(17, 11),
    l::Op::Eq(33, 19),
    l::Op::And(32, 34),
    l::Op::Not(30),
    l::Op::Not(36),
    l::Op::Not(35),
    l::Op::And(37, 38),
    l::Op::Not(39),
    l::Op::And(28, 40),
    l::Op::Literal(l::Atom::I128(152)),
    l::Op::Eq(5, 42),
    l::Op::Eq(9, 14),
    l::Op::And(44, 34),
    l::Op::Not(43),
    l::Op::Not(46),
    l::Op::Not(45),
    l::Op::And(47, 48),
    l::Op::Not(49),
    l::Op::And(41, 50),
    l::Op::Literal(l::Atom::I128(153)),
    l::Op::Eq(5, 52),
    l::Op::Eq(17, 19),
    l::Op::And(32, 54),
    l::Op::Not(53),
    l::Op::Not(56),
    l::Op::Not(55),
    l::Op::And(57, 58),
    l::Op::Not(59),
    l::Op::And(51, 60),
];
const LAW_508: &[l::Op<'static>] = &[l::Op::Literal(l::Atom::Bool(false))];
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
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::And(5, 2),
    l::Op::And(6, 4),
];
const LAW_991: &[l::Op<'static>] = &[
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::Observe(l::Observation::Context(130)),
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
    l::Op::Observe(l::Observation::Command(120)),
    l::Op::ToI128(34),
    l::Op::Literal(l::Atom::I128(150)),
    l::Op::Eq(35, 36),
    l::Op::Observe(l::Observation::Command(121)),
    l::Op::ToI128(38),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(40),
    l::Op::Lt(41, 39),
    l::Op::And(37, 42),
    l::Op::And(8, 43),
    l::Op::Not(43),
    l::Op::And(8, 45),
    l::Op::ObserveWhen(44, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(47),
    l::Op::Literal(l::Atom::I128(201)),
    l::Op::Eq(48, 49),
    l::Op::And(28, 50),
    l::Op::And(51, 20),
    l::Op::And(52, 23),
    l::Op::And(53, 26),
    l::Op::Select(44, 54, 0),
    l::Op::Literal(l::Atom::I128(151)),
    l::Op::Eq(35, 56),
    l::Op::Literal(l::Atom::I128(152)),
    l::Op::Eq(35, 58),
    l::Op::Not(57),
    l::Op::Not(59),
    l::Op::And(60, 61),
    l::Op::Not(62),
    l::Op::Observe(l::Observation::Pre(111)),
    l::Op::ToI128(64),
    l::Op::Lt(65, 39),
    l::Op::And(63, 66),
    l::Op::And(46, 67),
    l::Op::Not(67),
    l::Op::And(46, 69),
    l::Op::ObserveWhen(68, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(71),
    l::Op::Literal(l::Atom::I128(202)),
    l::Op::Eq(72, 73),
    l::Op::And(28, 74),
    l::Op::And(75, 20),
    l::Op::And(76, 23),
    l::Op::And(77, 26),
    l::Op::Select(68, 78, 0),
    l::Op::Add(65, 39),
    l::Op::Literal(l::Atom::I128(5)),
    l::Op::Lt(81, 80),
    l::Op::And(37, 82),
    l::Op::Literal(l::Atom::I128(153)),
    l::Op::Eq(35, 84),
    l::Op::Not(85),
    l::Op::And(60, 86),
    l::Op::Not(87),
    l::Op::Add(41, 39),
    l::Op::Lt(81, 89),
    l::Op::And(88, 90),
    l::Op::Not(83),
    l::Op::Not(91),
    l::Op::And(92, 93),
    l::Op::Not(94),
    l::Op::And(70, 95),
    l::Op::Not(95),
    l::Op::And(70, 97),
    l::Op::ObserveWhen(96, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(99),
    l::Op::Literal(l::Atom::I128(203)),
    l::Op::Eq(100, 101),
    l::Op::And(28, 102),
    l::Op::And(103, 20),
    l::Op::And(104, 23),
    l::Op::And(105, 26),
    l::Op::Select(96, 106, 0),
    l::Op::And(98, 37),
    l::Op::Not(37),
    l::Op::And(98, 109),
    l::Op::Eq(10, 19),
    l::Op::Not(12),
    l::Op::Literal(l::Atom::I128(2)),
    l::Op::Eq(25, 113),
    l::Op::ObserveWhen(108, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(115),
    l::Op::Sub(41, 39),
    l::Op::Eq(116, 117),
    l::Op::Select(108, 118, 0),
    l::Op::ObserveWhen(108, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(120),
    l::Op::Eq(121, 80),
    l::Op::Select(108, 122, 0),
    l::Op::And(0, 111),
    l::Op::And(124, 112),
    l::Op::And(125, 20),
    l::Op::And(126, 23),
    l::Op::And(127, 114),
    l::Op::And(128, 119),
    l::Op::And(129, 123),
    l::Op::Select(108, 130, 0),
    l::Op::And(110, 57),
    l::Op::And(110, 60),
    l::Op::ObserveWhen(132, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(134),
    l::Op::Eq(135, 89),
    l::Op::Select(132, 136, 0),
    l::Op::ObserveWhen(132, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(138),
    l::Op::Sub(65, 39),
    l::Op::Eq(139, 140),
    l::Op::Select(132, 141, 0),
    l::Op::And(128, 137),
    l::Op::And(143, 142),
    l::Op::Select(132, 144, 0),
    l::Op::And(133, 59),
    l::Op::And(133, 61),
    l::Op::Eq(22, 3),
    l::Op::ObserveWhen(146, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(149),
    l::Op::Eq(150, 41),
    l::Op::Select(146, 151, 0),
    l::Op::ObserveWhen(146, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(153),
    l::Op::Eq(154, 140),
    l::Op::Select(146, 155, 0),
    l::Op::ObserveWhen(146, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(157),
    l::Op::Eq(158, 19),
    l::Op::ObserveWhen(146, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(160),
    l::Op::Literal(l::Atom::I128(300)),
    l::Op::Eq(161, 162),
    l::Op::ObserveWhen(
        146,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"warehouse"),
    ),
    l::Op::Literal(l::Atom::Text(b"warehouse")),
    l::Op::Eq(164, 165),
    l::Op::ObserveWhen(146, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Literal(l::Atom::U128(0)),
    l::Op::Eq(167, 168),
    l::Op::ObserveWhen(146, l::Observation::OutboxPayload(0, 140), l::Atom::I128(0)),
    l::Op::ToI128(170),
    l::Op::Eq(171, 39),
    l::Op::Select(146, 172, 0),
    l::Op::And(126, 148),
    l::Op::And(174, 114),
    l::Op::And(175, 152),
    l::Op::And(176, 156),
    l::Op::And(177, 159),
    l::Op::And(178, 163),
    l::Op::And(179, 166),
    l::Op::And(180, 169),
    l::Op::And(181, 173),
    l::Op::Select(146, 182, 0),
    l::Op::And(147, 0),
    l::Op::Not(0),
    l::Op::And(147, 185),
    l::Op::ObserveWhen(184, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(187),
    l::Op::Eq(188, 89),
    l::Op::Select(184, 189, 0),
    l::Op::ObserveWhen(184, l::Observation::Post(111), l::Atom::I128(0)),
    l::Op::ToI128(191),
    l::Op::Eq(192, 65),
    l::Op::Select(184, 193, 0),
    l::Op::And(128, 190),
    l::Op::And(195, 194),
    l::Op::Select(184, 196, 0),
    l::Op::Not(186),
    l::Op::And(0, 33),
    l::Op::And(199, 55),
    l::Op::And(200, 79),
    l::Op::And(201, 107),
    l::Op::And(202, 131),
    l::Op::And(203, 145),
    l::Op::And(204, 183),
    l::Op::And(205, 197),
    l::Op::And(206, 198),
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
        kind: l::Kind::AssetConservation,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_501,
            root: 48,
        },
    },
    l::Law {
        id: 502,
        kind: l::Kind::DebitCreditEffectEquality,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_502,
            root: 61,
        },
    },
    l::Law {
        id: 508,
        kind: l::Kind::CommittedFailureEffects,
        scope: l::Scope::CommittedFailure,
        genesis: false,
        program: l::Program {
            nodes: LAW_508,
            root: 0,
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
            root: 207,
        },
    },
];
/// No declared law is optional at descriptor admission.
pub const REQUIRED: &[u32] = &[500, 501, 502, 508, 509, 990, 991];
/// Owns declarative input/output type tables; evaluation stays in the library.
pub struct Contract {
    state: Vec<InputField>,
    command: Vec<InputField>,
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
                    leaf: InputLeaf::I128 { min: 0, max: 5 },
                },
                InputField {
                    id: 111,
                    leaf: InputLeaf::I128 { min: 0, max: 5 },
                },
            ],
            command: vec![
                InputField {
                    id: 120,
                    leaf: InputLeaf::Sum {
                        type_id: 107,
                        min: 150,
                        max: 153,
                        variants: vec![
                            InputVariant { id: 150, code: 150 },
                            InputVariant { id: 151, code: 151 },
                            InputVariant { id: 152, code: 152 },
                            InputVariant { id: 153, code: 153 },
                        ],
                    },
                },
                InputField {
                    id: 121,
                    leaf: InputLeaf::I128 { min: 1, max: 3 },
                },
            ],
            context: vec![InputField {
                id: 130,
                leaf: InputLeaf::Bool,
            }],
            output_types: vec![
                InputLeaf::I128 { min: 0, max: 7 },
                InputLeaf::I128 {
                    min: -9223372036854775808,
                    max: 9223372036854775807,
                },
                InputLeaf::I128 {
                    min: -9223372036854775808,
                    max: 9223372036854775807,
                },
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
            command: c::Schema::Record(&self.command),
            context: c::Schema::Record(&self.context),
            program: V2ScalarProgram {
                inputs: &[
                    ScalarDomain::Int { min: 0, max: 5 },
                    ScalarDomain::Int { min: 0, max: 5 },
                    ScalarDomain::Int { min: 150, max: 153 },
                    ScalarDomain::Int { min: 1, max: 3 },
                    ScalarDomain::Bool,
                ],
                outputs: &[
                    ScalarDomain::Int { min: 0, max: 7 },
                    ScalarDomain::Int {
                        min: -9223372036854775808,
                        max: 9223372036854775807,
                    },
                    ScalarDomain::Int {
                        min: -9223372036854775808,
                        max: 9223372036854775807,
                    },
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
                    Op::Not(0),
                    Op::Int(0),
                    Op::Input(2),
                    Op::Int(150),
                    Op::Eq(3, 4),
                    Op::Input(3),
                    Op::Input(0),
                    Op::Lt(7, 6),
                    Op::And(5, 8),
                    Op::Int(1),
                    Op::Int(151),
                    Op::Eq(3, 11),
                    Op::Int(152),
                    Op::Eq(3, 13),
                    Op::Not(12),
                    Op::Not(14),
                    Op::And(15, 16),
                    Op::Not(17),
                    Op::Input(1),
                    Op::Lt(19, 6),
                    Op::And(18, 20),
                    Op::Int(2),
                    Op::Add(19, 6),
                    Op::Int(5),
                    Op::Lt(24, 23),
                    Op::And(5, 25),
                    Op::Int(153),
                    Op::Eq(3, 27),
                    Op::Not(28),
                    Op::And(15, 29),
                    Op::Not(30),
                    Op::Add(7, 6),
                    Op::Lt(24, 32),
                    Op::And(31, 33),
                    Op::Not(26),
                    Op::Not(34),
                    Op::And(35, 36),
                    Op::Not(37),
                    Op::Int(3),
                    Op::Int(4),
                    Op::Int(6),
                    Op::Int(7),
                    Op::Select(14, 41, 42),
                    Op::Select(12, 24, 43),
                    Op::Select(5, 40, 44),
                    Op::Select(38, 39, 45),
                    Op::Select(21, 22, 46),
                    Op::Select(9, 10, 47),
                    Op::Select(1, 2, 48),
                    Op::Sub(7, 6),
                    Op::Sub(19, 6),
                ],
                roots: &[49, 50, 23, 32, 51],
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
                    selector: c::Selector::Field(120),
                },
                c::Binding {
                    source: c::Source::Command,
                    selector: c::Selector::Field(121),
                },
                c::Binding {
                    source: c::Source::Context,
                    selector: c::Selector::Field(130),
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
                    class: c::Class::Reject,
                },
            ],
            channels: &[c::Channel {
                id: 300,
                destination: c::Domain::Text,
                payload: &[c::TypedField {
                    field: 140,
                    domain: c::Domain::I128 { min: 1, max: 3 },
                }],
                idempotency: c::Domain::U128 { min: 0, max: 0 },
            }],
            laws: LAWS,
            required: REQUIRED,
            limits: v2_zero_limits()
                .with_limit(Resource::Read, 127)
                .with_limit(Resource::Write, 2)
                .with_limit(Resource::Candidate, 1)
                .with_limit(Resource::Effect, 1)
                .with_limit(Resource::Byte, 229)
                .with_limit(Resource::WitnessByte, 0)
                .with_limit(Resource::Depth, 0)
                .with_limit(Resource::Step, 420),
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
                types: 9,
                fields: 2,
                variants: 4,
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
/// Position in this application's contract lineage: 1 before any adoption.
pub const VERSION: u32 = 1;
/// Every contract version's checked catalog, oldest first and this one last,
/// for a store upgrade or a lineage open. Each binding checks that version's
/// complete retained schema and policy bytes.
pub fn with_lineage<R>(
    f: impl FnOnce(&[&catalog::BoundCatalog<'_>]) -> R,
) -> Result<R, catalog::Failure> {
    let contract = Contract::new();
    let descriptor = contract.descriptor();
    let catalog = checked_catalog(&descriptor)?;
    Ok(f(&[&catalog]))
}
