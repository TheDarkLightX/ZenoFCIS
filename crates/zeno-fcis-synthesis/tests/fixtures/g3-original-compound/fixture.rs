//! Exact retained CodegenFixture original schema; no generated replacement types.
use std::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    V2InputField, V2NumericOp as N, V2NumericProgram, V2Resource as R,
    canonical_v2::{output, schema as s},
    v2_authority as a, v2_catalog as catalog, v2_composition as c, v2_laws as l, v2_zero_limits,
};
pub(crate) const ORIGINAL: &[u8] = include_bytes!("schema.zcve");
pub(crate) const DESCRIPTION: s::Description<'static> = s::Description {
    profile: b"CodegenFixture",
    version: 1,
    root: 12,
    definitions: &[
        s::Definition {
            id: 1,
            name: b"Amount",
            kind: s::Kind::U128 {
                min: 0,
                max: 1_000_000,
            },
        },
        s::Definition {
            id: 2,
            name: b"Signed",
            kind: s::Kind::I128 {
                min: -1_000,
                max: 1_000,
            },
        },
        s::Definition {
            id: 3,
            name: b"Label",
            kind: s::Kind::Text { min: 1, max: 16 },
        },
        s::Definition {
            id: 4,
            name: b"Blob",
            kind: s::Kind::Bytes { min: 0, max: 32 },
        },
        s::Definition {
            id: 5,
            name: b"Flag",
            kind: s::Kind::Bool,
        },
        s::Definition {
            id: 6,
            name: b"Nil",
            kind: s::Kind::Unit,
        },
        s::Definition {
            id: 7,
            name: b"Tag",
            kind: s::Kind::Enum(&[
                s::Variant {
                    id: 1,
                    name: b"Idle",
                },
                s::Variant {
                    id: 2,
                    name: b"Active",
                },
            ]),
        },
        s::Definition {
            id: 8,
            name: b"Point",
            kind: s::Kind::Tuple(&[1, 7]),
        },
        s::Definition {
            id: 9,
            name: b"Event",
            kind: s::Kind::SumPayload(&[
                s::SumVariant {
                    id: 1,
                    name: b"Stop",
                    payload: None,
                },
                s::SumVariant {
                    id: 2,
                    name: b"Move",
                    payload: Some(1),
                },
            ]),
        },
        s::Definition {
            id: 10,
            name: b"Labels",
            kind: s::Kind::Vector {
                element: 3,
                min: 0,
                max: 4,
            },
        },
        s::Definition {
            id: 11,
            name: b"Scores",
            kind: s::Kind::Map {
                key: 1,
                value: 1,
                min: 0,
                max: 4,
            },
        },
        s::Definition {
            id: 12,
            name: b"BalanceState",
            kind: s::Kind::Record(&[
                s::Field {
                    id: 1,
                    name: b"amount",
                    type_id: 1,
                },
                s::Field {
                    id: 2,
                    name: b"signed",
                    type_id: 2,
                },
                s::Field {
                    id: 3,
                    name: b"label",
                    type_id: 3,
                },
                s::Field {
                    id: 4,
                    name: b"blob",
                    type_id: 4,
                },
                s::Field {
                    id: 5,
                    name: b"flag",
                    type_id: 5,
                },
                s::Field {
                    id: 6,
                    name: b"nil",
                    type_id: 6,
                },
                s::Field {
                    id: 7,
                    name: b"tag",
                    type_id: 7,
                },
                s::Field {
                    id: 8,
                    name: b"point",
                    type_id: 8,
                },
                s::Field {
                    id: 9,
                    name: b"event",
                    type_id: 9,
                },
                s::Field {
                    id: 10,
                    name: b"labels",
                    type_id: 10,
                },
                s::Field {
                    id: 11,
                    name: b"scores",
                    type_id: 11,
                },
            ]),
        },
    ],
};
pub(crate) fn ok<T, E: core::fmt::Debug>(v: Result<T, E>) -> T {
    v.unwrap_or_else(|e| panic!("original compound fixture refused: {e:?}"))
}
pub(crate) fn schema_limits() -> s::Limits {
    s::Limits {
        bytes: 501,
        types: 12,
        fields: 11,
        variants: 2,
    }
}
pub(crate) const HASH: [u8; 32] = [
    0x83, 0x0a, 0x17, 0xee, 0x25, 0xb7, 0xb0, 0x6a, 0x3b, 0x33, 0x32, 0xd3, 0xe0, 0xe7, 0x20, 0x5c,
    0xea, 0x78, 0xb0, 0x57, 0x02, 0xb4, 0xfa, 0xe1, 0x6c, 0x96, 0xa4, 0xcd, 0x37, 0x78, 0x15, 0xc9,
];
pub(crate) fn framing() -> c::Framing {
    let binding = |root| c::FrameBinding {
        root,
        schema: HASH,
        max_bytes: 4096,
    };
    c::Framing {
        state: binding(12),
        command: binding(9),
        context: binding(5),
    }
}
pub(crate) fn wire(root: u32, value: &[u8]) -> Vec<u8> {
    ok(output::encode_envelope(root, &HASH, value, 4096))
}
pub(crate) fn unsigned(v: u128) -> Vec<u8> {
    let mut b = vec![3];
    b.extend(v.to_be_bytes());
    b
}
pub(crate) fn event(v: Option<u128>) -> Vec<u8> {
    let mut b = vec![
        10,
        0,
        0,
        0,
        9,
        0,
        if v.is_some() { 2 } else { 1 },
        u8::from(v.is_some()),
    ];
    if let Some(v) = v {
        b.extend(unsigned(v));
    }
    b
}
fn text(v: &[u8]) -> Vec<u8> {
    let mut b = vec![6];
    b.extend((v.len() as u32).to_be_bytes());
    b.extend(v);
    b
}
fn sequence(tag: u8, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![tag];
    b.extend((children.len() as u32).to_be_bytes());
    for c in children {
        b.extend(c);
    }
    b
}
pub(crate) fn scores(entries: &[(u128, u128)]) -> Vec<u8> {
    let mut b = vec![12];
    b.extend((entries.len() as u32).to_be_bytes());
    for (k, v) in entries {
        for child in [unsigned(*k), unsigned(*v)] {
            b.extend((child.len() as u32).to_be_bytes());
            b.extend(child);
        }
    }
    b
}
pub(crate) fn fields(amount: u128, ev: Option<u128>) -> Vec<Vec<u8>> {
    let mut signed = vec![4];
    signed.extend((-7i128).to_be_bytes());
    let tag = vec![7, 0, 0, 0, 7, 0, 2];
    vec![
        unsigned(amount),
        signed,
        text(b"label"),
        vec![5, 0, 0, 0, 3, 0, 127, 255],
        vec![2],
        vec![0],
        tag.clone(),
        sequence(8, &[unsigned(8), tag]),
        event(ev),
        sequence(11, &[text(b"one"), text(b"two")]),
        scores(&[(1, 4), (2, 6)]),
    ]
}
pub(crate) fn record(values: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![9];
    b.extend((values.len() as u32).to_be_bytes());
    for (i, v) in values.iter().enumerate() {
        b.extend(((i + 1) as u16).to_be_bytes());
        b.extend(v);
    }
    b
}
pub(crate) fn state(amount: u128, ev: Option<u128>) -> Vec<u8> {
    wire(12, &record(&fields(amount, ev)))
}
pub(crate) fn with_authority(run: impl FnOnce(&a::Authority<'_, c::NumericDescriptor<'_>>)) {
    with_limits(100_000, 100_000, run);
}
pub(crate) fn with_limits(
    read_limit: u64,
    step_limit: u64,
    run: impl FnOnce(&a::Authority<'_, c::NumericDescriptor<'_>>),
) {
    let checked = ok(s::admit(ORIGINAL, &DESCRIPTION, schema_limits()));
    let amount = c::Domain::U128 {
        min: 0,
        max: 1_000_000,
    };
    let signed = c::Domain::I128 {
        min: -1_000,
        max: 1_000,
    };
    let domains = [
        amount,
        signed,
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 3,
        },
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 4,
        },
        c::Domain::Bool,
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 6,
        },
        c::Domain::Enum {
            type_id: 7,
            variants: &[1, 2],
        },
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 8,
        },
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 9,
        },
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 10,
        },
        c::Domain::Canonical {
            schema: ORIGINAL,
            type_id: 11,
        },
    ];
    let assignments: Vec<_> = domains
        .iter()
        .enumerate()
        .map(|(i, d)| c::Assignment {
            field: (i + 1) as u16,
            domain: *d,
            value: if i == 0 {
                c::Expr::Output(1)
            } else if i == 8 {
                c::Expr::Root(c::Source::Command)
            } else {
                c::Expr::Input(c::Source::State, (i + 1) as u16)
            },
        })
        .collect();
    let branches = [c::Branch {
        code: 0,
        class: c::Class::Accept,
        reason: None,
        assignments: &assignments,
        effects: &[],
        outbox: &[],
    }];
    let projection = [V2InputField {
        id: 1,
        leaf: amount,
    }];
    let mut nodes = Vec::new();
    for id in 2..=11 {
        if id == 9 {
            continue;
        }
        let n = nodes.len();
        nodes.extend([
            l::Op::Observe(l::Observation::Pre(id)),
            l::Op::Observe(l::Observation::Post(id)),
            l::Op::Eq(n, n + 1),
        ]);
        if n > 0 {
            nodes.push(l::Op::And(n - 1, n + 2));
        }
    }
    let n = nodes.len();
    nodes.extend([
        l::Op::Observe(l::Observation::CommandRoot),
        l::Op::Observe(l::Observation::Post(9)),
        l::Op::Eq(n, n + 1),
        l::Op::And(n - 1, n + 2),
    ]);
    let n = nodes.len();
    nodes.extend([
        l::Op::Observe(l::Observation::Post(1)),
        l::Op::Literal(c::Atom::U128(6)),
        l::Op::Eq(n, n + 1),
        l::Op::And(n - 1, n + 2),
    ]);
    // These declarations exercise the same mandatory law inventory as real apps.
    // Post observations also select the actual initial state during genesis.
    let invariant = [
        l::Op::Observe(l::Observation::PostLength),
        l::Op::Literal(c::Atom::U128(11)),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::Post(1)),
        l::Op::Literal(c::Atom::U128(1_000_000)),
        l::Op::Lt(4, 3),
        l::Op::Not(5),
        l::Op::And(2, 6),
    ];
    let no_deliveries = [
        l::Op::Observe(l::Observation::EffectLength),
        l::Op::Literal(c::Atom::U128(0)),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::Eq(3, 1),
        l::Op::And(2, 4),
    ];
    let no_authority = [
        l::Op::Observe(l::Observation::EffectLength),
        l::Op::Literal(c::Atom::U128(0)),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::OutboxLength),
        l::Op::Eq(3, 1),
        l::Op::And(2, 4),
        l::Op::Observe(l::Observation::PostLength),
        l::Op::Eq(6, 1),
        l::Op::And(5, 7),
        l::Op::Observe(l::Observation::PatchLength),
        l::Op::Eq(9, 1),
        l::Op::And(8, 10),
    ];
    let initial = [
        l::Op::Observe(l::Observation::Initial(1)),
        l::Op::Literal(c::Atom::U128(5)),
        l::Op::Eq(0, 1),
        l::Op::Observe(l::Observation::Initial(2)),
        l::Op::Literal(c::Atom::I128(-7)),
        l::Op::Eq(3, 4),
        l::Op::And(2, 5),
        l::Op::Observe(l::Observation::Initial(5)),
        l::Op::Literal(c::Atom::Bool(true)),
        l::Op::Eq(7, 8),
        l::Op::And(6, 9),
    ];
    let laws = [
        l::Law {
            id: 1,
            kind: l::Kind::DecisionConformance,
            scope: l::Scope::Always,
            genesis: false,
            program: l::Program {
                nodes: &nodes,
                root: nodes.len() - 1,
            },
        },
        l::Law {
            id: 2,
            kind: l::Kind::StateInvariant,
            scope: l::Scope::Committing,
            genesis: true,
            program: l::Program {
                nodes: &invariant,
                root: invariant.len() - 1,
            },
        },
        l::Law {
            id: 3,
            kind: l::Kind::RejectNoAuthority,
            scope: l::Scope::Reject,
            genesis: false,
            program: l::Program {
                nodes: &no_authority,
                root: no_authority.len() - 1,
            },
        },
        l::Law {
            id: 4,
            kind: l::Kind::CommittedFailureEffects,
            scope: l::Scope::CommittedFailure,
            genesis: false,
            program: l::Program {
                nodes: &no_deliveries,
                root: no_deliveries.len() - 1,
            },
        },
        l::Law {
            id: 5,
            kind: l::Kind::InitialCondition,
            scope: l::Scope::Always,
            genesis: true,
            program: l::Program {
                nodes: &initial,
                root: initial.len() - 1,
            },
        },
    ];
    let mut limits = v2_zero_limits();
    for r in [
        R::Read,
        R::Write,
        R::Candidate,
        R::Effect,
        R::Byte,
        R::WitnessByte,
        R::Depth,
        R::Step,
    ] {
        limits = limits.with_limit(r, 100_000);
    }
    limits = limits
        .with_limit(R::Read, read_limit)
        .with_limit(R::Step, step_limit);
    let descriptor = c::NumericDescriptor {
        state: c::Schema::Original {
            checked: &checked,
            root: 12,
            projection: c::Projection::Fields(&projection),
        },
        command: c::Schema::Original {
            checked: &checked,
            root: 9,
            projection: c::Projection::None,
        },
        context: c::Schema::Original {
            checked: &checked,
            root: 5,
            projection: c::Projection::None,
        },
        program: V2NumericProgram {
            inputs: &[amount],
            outputs: &[c::Domain::I128 { min: 0, max: 0 }, amount],
            nodes: &[N::Input(0), N::I128(0), N::U128(6)],
            roots: &[1, 2],
        },
        bindings: &[c::Binding {
            source: c::Source::State,
            selector: c::Selector::Field(1),
        }],
        output_types: &[c::Domain::I128 { min: 0, max: 0 }, amount],
        decision_output: 0,
        branches: &branches,
        reasons: &[],
        channels: &[],
        laws: &laws,
        required: &[1, 2, 3, 4, 5],
        limits,
    };
    let framing = framing();
    let policy = a::numeric_policy_bytes(&descriptor, ORIGINAL, &framing, &[])
        .unwrap_or_else(|| panic!("policy encoding failed"));
    let catalog = ok(catalog::bind_original(
        ORIGINAL,
        &DESCRIPTION,
        catalog::Limits {
            schema: schema_limits(),
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &framing,
        &[],
    ));
    let authority = ok(a::bind(&catalog));
    run(&authority);
}
pub(crate) fn commit(p: a::PublicationOutcome<'_>) -> a::Publication<'_> {
    match p {
        a::PublicationOutcome::Commit(p) => p,
        other => panic!("expected original compound publication: {other:?}"),
    }
}
