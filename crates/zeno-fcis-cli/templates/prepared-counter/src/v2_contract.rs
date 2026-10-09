// Generated declarative data. Review v2/policy.json and project.zeno.
// Regenerate or check with `zeno-fcis generate contract`; no runtime mapper.
extern crate alloc;
use alloc::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{Domain as ScalarDomain, Op, V2ScalarProgram};
use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2Resource as Resource,
    canonical_v2::schema as s, v2_authority as authority, v2_catalog as catalog,
    v2_composition as c, v2_laws as l, v2_zero_limits,
};
/// Exact actual original schema bytes.
pub const ORIGINAL_SCHEMA: &[u8] = include_bytes!("../v2/schema.zcve");
/// Complete reviewed library-encoded policy.
pub const ORIGINAL_POLICY: &[u8] = include_bytes!("../v2/policy.zcve");
/// Complete original named schema description.
pub const DESCRIPTION: s::Description<'static> = s::Description {
    profile: b"prepared_counter",
    version: 1,
    root: 100,
    definitions: &[
        s::Definition {
            id: 100,
            name: b"CounterState",
            kind: s::Kind::Record(&[s::Field {
                id: 110,
                name: b"count",
                type_id: 105,
            }]),
        },
        s::Definition {
            id: 101,
            name: b"CounterCommand",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 120,
                    name: b"first",
                    type_id: 106,
                },
                s::Field {
                    id: 121,
                    name: b"second",
                    type_id: 106,
                },
                s::Field {
                    id: 122,
                    name: b"third",
                    type_id: 106,
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
            kind: s::Kind::Record(&[s::Field {
                id: 112,
                name: b"notified_count",
                type_id: 105,
            }]),
        },
        s::Definition {
            id: 105,
            name: b"CounterValue",
            kind: s::Kind::I128 { min: 0, max: 3 },
        },
        s::Definition {
            id: 106,
            name: b"CounterDelta",
            kind: s::Kind::I128 { min: -1, max: 1 },
        },
    ],
};
/// Original root and schema commitments and complete wire-size limits.
pub const FRAMING: c::Framing = c::Framing {
    state: c::FrameBinding {
        root: 100,
        schema: [
            158, 188, 189, 103, 171, 32, 156, 187, 41, 217, 245, 101, 51, 246, 219, 160, 233, 123,
            146, 64, 224, 203, 142, 55, 82, 159, 193, 35, 240, 185, 116, 181,
        ],
        max_bytes: 72,
    },
    command: c::FrameBinding {
        root: 101,
        schema: [
            158, 188, 189, 103, 171, 32, 156, 187, 41, 217, 245, 101, 51, 246, 219, 160, 233, 123,
            146, 64, 224, 203, 142, 55, 82, 159, 193, 35, 240, 185, 116, 181,
        ],
        max_bytes: 110,
    },
    context: c::FrameBinding {
        root: 102,
        schema: [
            158, 188, 189, 103, 171, 32, 156, 187, 41, 217, 245, 101, 51, 246, 219, 160, 233, 123,
            146, 64, 224, 203, 142, 55, 82, 159, 193, 35, 240, 185, 116, 181,
        ],
        max_bytes: 49,
    },
};
/// Exact channel to original destination and payload type links.
pub const CHANNEL_ROOTS: &[(u32, u32, u32)] = &[(300, 103, 104)];
/// The genesis state law 990 requires, field by field.
pub const GENESIS: &[c::Field<'static>] = &[c::Field {
    id: 110,
    value: c::Atom::I128(0),
}];
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
        assignments: &[c::Assignment {
            field: 110,
            value: c::Expr::Output(1),
            domain: c::Domain::I128 { min: 0, max: 3 },
        }],
        effects: &[],
        outbox: &[c::DeliveryPlan {
            ordinal: 0,
            channel: 300,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::Text(b"local-observer")),
            payload: &[c::PayloadField {
                field: 112,
                value: c::Expr::Output(1),
            }],
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
];
const LAW_501: &[l::Op<'static>] = &[
    l::Op::Observe(l::Observation::ContextRoot),
    l::Op::ToI128(0),
    l::Op::Literal(l::Atom::I128(1)),
    l::Op::Eq(1, 2),
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(4),
    l::Op::Observe(l::Observation::Command(120)),
    l::Op::ToI128(6),
    l::Op::Add(5, 7),
    l::Op::Literal(l::Atom::I128(0)),
    l::Op::Lt(8, 9),
    l::Op::Not(10),
    l::Op::And(3, 11),
    l::Op::Literal(l::Atom::I128(3)),
    l::Op::Lt(13, 8),
    l::Op::Not(14),
    l::Op::And(12, 15),
    l::Op::Observe(l::Observation::Command(121)),
    l::Op::ToI128(17),
    l::Op::Add(8, 18),
    l::Op::Lt(19, 9),
    l::Op::Not(20),
    l::Op::And(16, 21),
    l::Op::Lt(13, 19),
    l::Op::Not(23),
    l::Op::And(22, 24),
    l::Op::Observe(l::Observation::Post(110)),
    l::Op::ToI128(26),
    l::Op::Observe(l::Observation::Command(122)),
    l::Op::ToI128(28),
    l::Op::Add(19, 29),
    l::Op::Eq(27, 30),
    l::Op::And(25, 31),
];
const LAW_502: &[l::Op<'static>] = &[l::Op::Literal(l::Atom::Bool(false))];
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
    l::Op::Literal(l::Atom::Bool(true)),
    l::Op::And(3, 2),
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
    l::Op::Observe(l::Observation::Pre(110)),
    l::Op::ToI128(34),
    l::Op::Observe(l::Observation::Command(120)),
    l::Op::ToI128(36),
    l::Op::Add(35, 37),
    l::Op::Lt(38, 19),
    l::Op::Literal(l::Atom::I128(3)),
    l::Op::Lt(40, 38),
    l::Op::Not(39),
    l::Op::Not(41),
    l::Op::And(42, 43),
    l::Op::Not(44),
    l::Op::Observe(l::Observation::Command(121)),
    l::Op::ToI128(46),
    l::Op::Add(38, 47),
    l::Op::Lt(48, 19),
    l::Op::Not(45),
    l::Op::Not(49),
    l::Op::And(50, 51),
    l::Op::Not(52),
    l::Op::Lt(40, 48),
    l::Op::Not(53),
    l::Op::Not(54),
    l::Op::And(55, 56),
    l::Op::Not(57),
    l::Op::Observe(l::Observation::Command(122)),
    l::Op::ToI128(59),
    l::Op::Add(48, 60),
    l::Op::Lt(61, 19),
    l::Op::Not(58),
    l::Op::Not(62),
    l::Op::And(63, 64),
    l::Op::Not(65),
    l::Op::Lt(40, 61),
    l::Op::Not(66),
    l::Op::Not(67),
    l::Op::And(68, 69),
    l::Op::Not(70),
    l::Op::And(8, 71),
    l::Op::Not(71),
    l::Op::And(8, 73),
    l::Op::ObserveWhen(72, l::Observation::Reason, l::Atom::I128(0)),
    l::Op::ToI128(75),
    l::Op::Literal(l::Atom::I128(201)),
    l::Op::Eq(76, 77),
    l::Op::And(28, 78),
    l::Op::And(79, 20),
    l::Op::And(80, 23),
    l::Op::And(81, 26),
    l::Op::Select(72, 82, 0),
    l::Op::And(74, 0),
    l::Op::Not(0),
    l::Op::And(74, 85),
    l::Op::Eq(10, 19),
    l::Op::Not(12),
    l::Op::Eq(22, 3),
    l::Op::Eq(25, 3),
    l::Op::ObserveWhen(84, l::Observation::Post(110), l::Atom::I128(0)),
    l::Op::ToI128(91),
    l::Op::Eq(92, 61),
    l::Op::Select(84, 93, 0),
    l::Op::ObserveWhen(84, l::Observation::OutboxOrdinal(0), l::Atom::I128(0)),
    l::Op::ToI128(95),
    l::Op::Eq(96, 19),
    l::Op::ObserveWhen(84, l::Observation::OutboxChannel(0), l::Atom::I128(0)),
    l::Op::ToI128(98),
    l::Op::Literal(l::Atom::I128(300)),
    l::Op::Eq(99, 100),
    l::Op::ObserveWhen(
        84,
        l::Observation::OutboxDestination(0),
        l::Atom::Text(b"local-observer"),
    ),
    l::Op::Literal(l::Atom::Text(b"local-observer")),
    l::Op::Eq(102, 103),
    l::Op::ObserveWhen(84, l::Observation::OutboxIdempotency(0), l::Atom::U128(0)),
    l::Op::Literal(l::Atom::U128(0)),
    l::Op::Eq(105, 106),
    l::Op::ObserveWhen(84, l::Observation::OutboxPayload(0, 112), l::Atom::I128(0)),
    l::Op::ToI128(108),
    l::Op::Eq(109, 61),
    l::Op::Select(84, 110, 0),
    l::Op::And(0, 87),
    l::Op::And(112, 88),
    l::Op::And(113, 20),
    l::Op::And(114, 89),
    l::Op::And(115, 90),
    l::Op::And(116, 94),
    l::Op::And(117, 97),
    l::Op::And(118, 101),
    l::Op::And(119, 104),
    l::Op::And(120, 107),
    l::Op::And(121, 111),
    l::Op::Select(84, 122, 0),
    l::Op::Not(86),
    l::Op::And(0, 33),
    l::Op::And(125, 83),
    l::Op::And(126, 123),
    l::Op::And(127, 124),
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
            root: 8,
        },
    },
    l::Law {
        id: 501,
        kind: l::Kind::AuthoritySubjectRecipient,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: LAW_501,
            root: 32,
        },
    },
    l::Law {
        id: 502,
        kind: l::Kind::CommittedFailureEffects,
        scope: l::Scope::CommittedFailure,
        genesis: false,
        program: l::Program {
            nodes: LAW_502,
            root: 0,
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
            root: 4,
        },
    },
    l::Law {
        id: 991,
        kind: l::Kind::DecisionConformance,
        scope: l::Scope::Always,
        genesis: false,
        program: l::Program {
            nodes: LAW_991,
            root: 128,
        },
    },
];
/// No declared law is optional at descriptor admission.
pub const REQUIRED: &[u32] = &[500, 501, 502, 503, 909, 990, 991];
/// Owns declarative input/output type tables; evaluation stays in the library.
pub struct Contract {
    state: Vec<InputField>,
    command: Vec<InputField>,
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
            state: vec![InputField {
                id: 110,
                leaf: InputLeaf::I128 { min: 0, max: 3 },
            }],
            command: vec![
                InputField {
                    id: 120,
                    leaf: InputLeaf::I128 { min: -1, max: 1 },
                },
                InputField {
                    id: 121,
                    leaf: InputLeaf::I128 { min: -1, max: 1 },
                },
                InputField {
                    id: 122,
                    leaf: InputLeaf::I128 { min: -1, max: 1 },
                },
            ],
            context: InputLeaf::Bool,
            output_types: vec![
                InputLeaf::I128 { min: 0, max: 2 },
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
            context: c::Schema::Leaf(&self.context),
            program: V2ScalarProgram {
                inputs: &[
                    ScalarDomain::Int { min: 0, max: 3 },
                    ScalarDomain::Int { min: -1, max: 1 },
                    ScalarDomain::Int { min: -1, max: 1 },
                    ScalarDomain::Int { min: -1, max: 1 },
                    ScalarDomain::Bool,
                ],
                outputs: &[
                    ScalarDomain::Int { min: 0, max: 2 },
                    ScalarDomain::Int {
                        min: -9223372036854775808,
                        max: 9223372036854775807,
                    },
                ],
                nodes: &[
                    Op::Input(4),
                    Op::Not(0),
                    Op::Int(0),
                    Op::Input(0),
                    Op::Input(1),
                    Op::Add(3, 4),
                    Op::Lt(5, 2),
                    Op::Int(3),
                    Op::Lt(7, 5),
                    Op::Not(6),
                    Op::Not(8),
                    Op::And(9, 10),
                    Op::Not(11),
                    Op::Input(2),
                    Op::Add(5, 13),
                    Op::Lt(14, 2),
                    Op::Not(12),
                    Op::Not(15),
                    Op::And(16, 17),
                    Op::Not(18),
                    Op::Lt(7, 14),
                    Op::Not(19),
                    Op::Not(20),
                    Op::And(21, 22),
                    Op::Not(23),
                    Op::Input(3),
                    Op::Add(14, 25),
                    Op::Lt(26, 2),
                    Op::Not(24),
                    Op::Not(27),
                    Op::And(28, 29),
                    Op::Not(30),
                    Op::Lt(7, 26),
                    Op::Not(31),
                    Op::Not(32),
                    Op::And(33, 34),
                    Op::Not(35),
                    Op::Int(1),
                    Op::Int(2),
                    Op::Select(36, 37, 38),
                    Op::Select(1, 2, 39),
                ],
                roots: &[40, 26],
            },
            bindings: &[
                c::Binding {
                    source: c::Source::State,
                    selector: c::Selector::Field(110),
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
                    source: c::Source::Command,
                    selector: c::Selector::Field(122),
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
            ],
            channels: &[c::Channel {
                id: 300,
                destination: c::Domain::Text,
                payload: &[c::TypedField {
                    field: 112,
                    domain: c::Domain::I128 { min: 0, max: 3 },
                }],
                idempotency: c::Domain::U128 { min: 0, max: 0 },
            }],
            laws: LAWS,
            required: REQUIRED,
            limits: v2_zero_limits()
                .with_limit(Resource::Read, 109)
                .with_limit(Resource::Write, 1)
                .with_limit(Resource::Candidate, 1)
                .with_limit(Resource::Effect, 1)
                .with_limit(Resource::Byte, 231)
                .with_limit(Resource::WitnessByte, 0)
                .with_limit(Resource::Depth, 0)
                .with_limit(Resource::Step, 242),
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
                types: 7,
                fields: 3,
                variants: 0,
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
/// Original schema labels for a lineage version, for historical reports.
pub fn schema_description(version: usize) -> Option<&'static s::Description<'static>> {
    match version {
        1 => Some(&DESCRIPTION),
        _ => None,
    }
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
