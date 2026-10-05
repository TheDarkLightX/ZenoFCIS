//! Independent original-wire inventory corpus through the actual producer/law bridge.
use std::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    Domain as ScalarDomain, Op as ScalarOp, V2InputField as InputField, V2InputLeaf as InputLeaf,
    V2InputVariant as Variant, V2Limits as Limits, V2Resource as Resource,
    V2ScalarProgram as ScalarProgram, v2_composition as c, v2_laws as l, v2_zero_limits,
};

const INPUTS: [ScalarDomain; 5] = [
    ScalarDomain::Int { min: 0, max: 5 },
    ScalarDomain::Int { min: 0, max: 5 },
    ScalarDomain::Int { min: 0, max: 3 },
    ScalarDomain::Int { min: 1, max: 3 },
    ScalarDomain::Bool,
];
const OUTPUTS: [ScalarDomain; 4] = [
    ScalarDomain::Int { min: 0, max: 4 },
    ScalarDomain::Int { min: 0, max: 5 },
    ScalarDomain::Int { min: 0, max: 5 },
    ScalarDomain::Bool,
];
// Exact original inventory program. The byte equality below checks this copied
// graph against the retained canonical artifact before it enters the real core.
const NODES: [ScalarOp; 46] = [
    ScalarOp::Input(0),
    ScalarOp::Input(1),
    ScalarOp::Input(2),
    ScalarOp::Input(3),
    ScalarOp::Int(5),
    ScalarOp::Int(1),
    ScalarOp::Int(0),
    ScalarOp::Eq(2, 6),
    ScalarOp::Int(1),
    ScalarOp::Eq(2, 8),
    ScalarOp::Int(2),
    ScalarOp::Eq(2, 10),
    ScalarOp::Lt(0, 3),
    ScalarOp::Lt(1, 3),
    ScalarOp::Add(0, 3),
    ScalarOp::Add(1, 3),
    ScalarOp::Sub(0, 3),
    ScalarOp::Sub(1, 3),
    ScalarOp::Lt(4, 15),
    ScalarOp::Lt(4, 14),
    ScalarOp::Int(3),
    ScalarOp::Select(18, 10, 20),
    ScalarOp::Select(12, 6, 21),
    ScalarOp::Select(19, 10, 20),
    ScalarOp::Select(13, 5, 23),
    ScalarOp::Select(13, 5, 20),
    ScalarOp::Select(11, 25, 23),
    ScalarOp::Select(9, 24, 26),
    ScalarOp::Select(7, 22, 27),
    ScalarOp::Eq(28, 20),
    ScalarOp::Select(11, 0, 14),
    ScalarOp::Select(9, 14, 30),
    ScalarOp::Select(7, 16, 31),
    ScalarOp::Select(11, 17, 1),
    ScalarOp::Select(9, 17, 33),
    ScalarOp::Select(7, 15, 34),
    ScalarOp::Select(29, 32, 0),
    ScalarOp::Select(29, 35, 1),
    ScalarOp::And(29, 11),
    ScalarOp::Input(4),
    ScalarOp::Int(4),
    ScalarOp::Not(39),
    ScalarOp::Select(41, 40, 28),
    ScalarOp::Select(41, 0, 36),
    ScalarOp::Select(41, 1, 37),
    ScalarOp::And(39, 38),
];
const ROOTS: [u16; 4] = [42, 43, 44, 45];

fn integer(v: i128) -> Vec<u8> {
    let mut b = vec![4];
    b.extend(v.to_be_bytes());
    b
}
fn tuple(items: Vec<Vec<u8>>) -> Vec<u8> {
    let mut b = vec![8];
    b.extend((items.len() as u32).to_be_bytes());
    for v in items {
        b.extend(v);
    }
    b
}
fn check_original_graph() {
    fn domains(values: &[ScalarDomain]) -> Vec<u8> {
        tuple(
            values
                .iter()
                .map(|v| {
                    let (flag, min, max) = match *v {
                        ScalarDomain::Bool => (2, 0, 1),
                        ScalarDomain::Int { min, max } => (1, min, max),
                        _ => panic!("unsupported future domain in the original graph oracle"),
                    };
                    tuple(vec![vec![flag], integer(min.into()), integer(max.into())])
                })
                .collect(),
        )
    }
    let ops = NODES
        .iter()
        .map(|op| {
            let fields: Vec<i128> = match *op {
                ScalarOp::Input(a) => vec![0, a.into()],
                ScalarOp::Int(a) => vec![1, a.into()],
                ScalarOp::Bool(a) => vec![2, i128::from(a)],
                ScalarOp::Add(a, b) => vec![3, a.into(), b.into()],
                ScalarOp::Sub(a, b) => vec![4, a.into(), b.into()],
                ScalarOp::Eq(a, b) => vec![5, a.into(), b.into()],
                ScalarOp::Lt(a, b) => vec![6, a.into(), b.into()],
                ScalarOp::And(a, b) => vec![7, a.into(), b.into()],
                ScalarOp::Not(a) => vec![8, a.into()],
                ScalarOp::Select(a, b, c) => vec![9, a.into(), b.into(), c.into()],
                _ => panic!("unsupported future operation in the original graph oracle"),
            };
            tuple(fields.into_iter().map(integer).collect())
        })
        .collect();
    let mut tag = vec![6];
    tag.extend(22u32.to_be_bytes());
    tag.extend(b"zeno-fcis/finite-i64/1");
    let roots = ROOTS
        .iter()
        .map(|n| {
            let mut b = vec![3];
            b.extend(u128::from(*n).to_be_bytes());
            b
        })
        .collect();
    assert_eq!(
        tuple(vec![
            tag,
            tuple(vec![domains(&INPUTS), domains(&OUTPUTS)]),
            tuple(ops),
            tuple(roots)
        ]),
        include_bytes!(
            "../../zeno-fcis-cli/templates/inventory-reservation/synthesized/program.zcve"
        )
    );
}
fn limits() -> Limits {
    let mut cap = v2_zero_limits();
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
        cap = cap.with_limit(r, u64::MAX);
    }
    cap
}
fn int(id: u16, min: i64, max: i64) -> InputField {
    InputField {
        id,
        leaf: InputLeaf::I128 { min, max },
    }
}
fn record(fields: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![9];
    b.extend((fields.len() as u32).to_be_bytes());
    for (id, value) in fields {
        b.extend(id.to_be_bytes());
        b.extend(value);
    }
    b
}
fn command(action: u16, quantity: i128) -> Vec<u8> {
    let mut sum = vec![10];
    sum.extend(107u32.to_be_bytes());
    sum.extend(action.to_be_bytes());
    sum.push(0);
    record(&[(120, sum), (121, integer(quantity))])
}
fn laws<'a>(truth: &'a [l::Op<'a>], conditional: &'a [l::Op<'a>], root: usize) -> Vec<l::Law<'a>> {
    let families = [
        (l::Kind::StateInvariant, l::Scope::Committing, true),
        (l::Kind::RejectNoAuthority, l::Scope::Reject, false),
        (
            l::Kind::CommittedFailureEffects,
            l::Scope::CommittedFailure,
            false,
        ),
        (l::Kind::DecisionConformance, l::Scope::Always, false),
        (l::Kind::InitialCondition, l::Scope::Always, true),
    ];
    let mut laws: Vec<_> = families
        .into_iter()
        .enumerate()
        .map(|(i, (kind, scope, genesis))| l::Law {
            id: i as u32 + 1,
            kind,
            scope,
            genesis,
            program: l::Program {
                nodes: truth,
                root: 0,
            },
        })
        .collect();
    laws.push(l::Law {
        id: 60,
        kind: l::Kind::DebitCreditEffectEquality,
        scope: l::Scope::Accept,
        genesis: false,
        program: l::Program {
            nodes: conditional,
            root,
        },
    });
    laws
}

fn with_inventory(run: impl FnOnce(&mut c::Descriptor<'_>)) {
    check_original_graph();
    let state = [int(110, 0, 5), int(111, 0, 5)];
    let command = [
        InputField {
            id: 120,
            leaf: InputLeaf::Sum {
                type_id: 107,
                min: 0,
                max: 3,
                variants: (150..=153)
                    .enumerate()
                    .map(|(i, id)| Variant { id, code: i as i64 })
                    .collect(),
            },
        },
        int(121, 1, 3),
    ];
    let context = [InputField {
        id: 130,
        leaf: InputLeaf::Bool,
    }];
    let bindings = [
        (c::Source::State, 110),
        (c::Source::State, 111),
        (c::Source::Command, 120),
        (c::Source::Command, 121),
        (c::Source::Context, 130),
    ]
    .map(|(source, id)| c::Binding {
        source,
        selector: c::Selector::Field(id),
    });
    let outputs = [
        InputLeaf::I128 { min: 0, max: 4 },
        InputLeaf::I128 { min: 0, max: 5 },
        InputLeaf::I128 { min: 0, max: 5 },
        InputLeaf::Bool,
    ];
    let assignments = [(110, 1), (111, 2)].map(|(field, i)| c::Assignment {
        field,
        value: c::Expr::Output(i),
        domain: c::Domain::I128 { min: 0, max: 5 },
    });
    let payload = [c::PayloadField {
        field: 140,
        value: c::Expr::Input(c::Source::Command, 121),
    }];
    let delivery = [c::DeliveryPlan {
        ordinal: 0,
        channel: 300,
        when: c::Expr::Output(3),
        destination: c::Expr::Constant(c::Atom::Text(b"warehouse")),
        payload: &payload,
        idempotency: c::Expr::Input(c::Source::Command, 121),
    }];
    let branches = [Some(201), Some(202), Some(203), None, Some(200)]
        .into_iter()
        .enumerate()
        .map(|(i, reason)| c::Branch {
            code: i as i128,
            class: if reason.is_some() {
                c::Class::Reject
            } else {
                c::Class::Accept
            },
            reason,
            assignments: if reason.is_none() { &assignments } else { &[] },
            effects: &[],
            outbox: if reason.is_none() { &delivery } else { &[] },
        })
        .collect::<Vec<_>>();
    let reasons = [200, 201, 202, 203].map(|id| c::Reason {
        id,
        class: c::Class::Reject,
    });
    let payload_types = [c::TypedField {
        field: 140,
        domain: c::Domain::I128 { min: 1, max: 3 },
    }];
    let channels = [c::Channel {
        id: 300,
        destination: c::Domain::Text,
        payload: &payload_types,
        idempotency: c::Domain::I128 { min: 1, max: 3 },
    }];
    let truth = [l::Op::Literal(l::Atom::Bool(true))];
    let conditional = [
        l::Op::Observe(l::Observation::Command(120)),
        l::Op::ToI128(0),
        l::Op::Literal(l::Atom::I128(152)),
        l::Op::Eq(1, 2), // ship guard
        l::Op::Observe(l::Observation::Command(121)),
        l::Op::ObserveWhen(3, l::Observation::OutboxPayload(0, 140), l::Atom::I128(0)),
        l::Op::Eq(4, 5),
        l::Op::ObserveWhen(3, l::Observation::OutboxChannel(0), l::Atom::I128(300)),
        l::Op::Literal(l::Atom::I128(300)),
        l::Op::Eq(7, 8),
        l::Op::ObserveWhen(
            3,
            l::Observation::OutboxDestination(0),
            l::Atom::Text(b"warehouse"),
        ),
        l::Op::Literal(l::Atom::Text(b"warehouse")),
        l::Op::Eq(10, 11),
        l::Op::And(6, 9),
        l::Op::And(13, 12),
        l::Op::Not(14),
        l::Op::And(3, 15),
        l::Op::Not(16),
    ];
    let definitions = laws(&truth, &conditional, 17);
    let mut d = c::Descriptor {
        state: c::Schema::Record(&state),
        command: c::Schema::Record(&command),
        context: c::Schema::Record(&context),
        program: ScalarProgram {
            inputs: &INPUTS,
            outputs: &OUTPUTS,
            nodes: &NODES,
            roots: &ROOTS,
        },
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &definitions,
        required: &[1, 2, 3, 4, 5, 60],
        limits: limits(),
    };
    run(&mut d);
}

#[test]
fn guarded_original_inventory_864_actual_frames_and_conditional_delivery_law() {
    with_inventory(|d| {
        let Ok(core) = c::bind(d) else {
            panic!("original descriptor must bind");
        };
        let mut count = 0;
        for available in 0..=5 {
            for reserved in 0..=5 {
                for action in 150..=153 {
                    for quantity in 1..=3 {
                        for authorized in [false, true] {
                            let reason = if !authorized {
                                Some(200)
                            } else if action == 150 && quantity > available {
                                Some(201)
                            } else if (action == 151 || action == 152) && quantity > reserved {
                                Some(202)
                            } else if (action == 150 && reserved + quantity > 5)
                                || ((action == 151 || action == 153) && available + quantity > 5)
                            {
                                Some(203)
                            } else {
                                None
                            };
                            let accepted = reason.is_none();
                            let ship = accepted && action == 152;
                            let (post_a, post_r) = if !accepted {
                                (available, reserved)
                            } else {
                                match action {
                                    150 => (available - quantity, reserved + quantity),
                                    151 => (available + quantity, reserved - quantity),
                                    152 => (available, reserved - quantity),
                                    _ => (available + quantity, reserved),
                                }
                            };
                            let s = record(&[(110, integer(available)), (111, integer(reserved))]);
                            let cmd = command(action, quantity);
                            let ctx = record(&[(130, vec![if authorized { 2 } else { 1 }])]);
                            let original = (s.clone(), cmd.clone(), ctx.clone());
                            let out = core.execute(c::Raw {
                                state: &s,
                                command: &cmd,
                                context: &ctx,
                            });
                            let Ok(candidate) = out.result() else {
                                panic!("original inventory decision refused its conditional law");
                            };
                            assert_eq!(
                                candidate.class(),
                                if accepted {
                                    c::Class::Accept
                                } else {
                                    c::Class::Reject
                                }
                            );
                            assert_eq!(candidate.reason(), reason);
                            assert_eq!(candidate.outbox().len(), usize::from(ship));
                            assert!(candidate.effects().is_empty());
                            if accepted {
                                assert_eq!(
                                    candidate.post(),
                                    &[
                                        c::Field {
                                            id: 110,
                                            value: c::Atom::I128(post_a)
                                        },
                                        c::Field {
                                            id: 111,
                                            value: c::Atom::I128(post_r)
                                        }
                                    ]
                                );
                            } else {
                                assert!(
                                    candidate.post().is_empty() && candidate.patch().is_empty()
                                );
                            }
                            if ship {
                                let delivered = &candidate.outbox()[0];
                                assert_eq!(
                                    (
                                        delivered.ordinal,
                                        delivered.channel,
                                        delivered.destination,
                                        delivered.idempotency
                                    ),
                                    (0, 300, c::Atom::Text(b"warehouse"), c::Atom::I128(quantity))
                                );
                                assert_eq!(
                                    delivered.payload,
                                    [c::Field {
                                        id: 140,
                                        value: c::Atom::I128(quantity)
                                    }]
                                );
                            }
                            let mut expected = Vec::new();
                            if accepted {
                                expected.push(l::ReadAttempt {
                                    law: 60,
                                    node: 0,
                                    observation: l::Observation::Command(120),
                                    permitted: true,
                                });
                                expected.push(l::ReadAttempt {
                                    law: 60,
                                    node: 4,
                                    observation: l::Observation::Command(121),
                                    permitted: true,
                                });
                                if ship {
                                    for (node, observation) in [
                                        (5, l::Observation::OutboxPayload(0, 140)),
                                        (7, l::Observation::OutboxChannel(0)),
                                        (10, l::Observation::OutboxDestination(0)),
                                    ] {
                                        expected.push(l::ReadAttempt {
                                            law: 60,
                                            node,
                                            observation,
                                            permitted: true,
                                        });
                                    }
                                }
                            }
                            assert_eq!(out.law_reads(), expected);
                            let Some(before) = out.decision_usage() else {
                                panic!("missing actual decision usage");
                            };
                            assert_eq!(
                                out.usage().used(Resource::Step),
                                before.used(Resource::Step) + if accepted { 20 } else { 2 }
                            );
                            assert_eq!(
                                out.usage().used(Resource::Read),
                                before.used(Resource::Read) + expected.len() as u64
                            );
                            for resource in [
                                Resource::Write,
                                Resource::Candidate,
                                Resource::Effect,
                                Resource::Byte,
                                Resource::WitnessByte,
                                Resource::Depth,
                            ] {
                                assert_eq!(out.usage().used(resource), before.used(resource));
                            }
                            assert_eq!(
                                out.diagnostics()[5].verdict,
                                if accepted {
                                    l::Verdict::Satisfied
                                } else {
                                    l::Verdict::Skipped
                                }
                            );
                            assert_eq!(
                                (out.raw().state, out.raw().command, out.raw().context),
                                (
                                    original.0.as_slice(),
                                    original.1.as_slice(),
                                    original.2.as_slice()
                                )
                            );
                            assert_eq!((s, cmd, ctx), original);
                            count += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(count, 864);
    });
}

#[test]
fn guarded_default_admission_matches_original_literal_text_standard_even_when_inactive() {
    with_inventory(|d| {
        let truth = [l::Op::Literal(l::Atom::Bool(true))];
        for byte in 0..=255 {
            let bytes = [byte];
            for text in [false, true] {
                let value = if text {
                    l::Atom::Text(&bytes)
                } else {
                    l::Atom::Bytes(&bytes)
                };
                let guarded = [
                    l::Op::Literal(l::Atom::Bool(false)),
                    l::Op::ObserveWhen(0, l::Observation::InitialRoot, value),
                    l::Op::Literal(l::Atom::Bool(true)),
                ];
                let literal = [l::Op::Literal(value), l::Op::Literal(l::Atom::Bool(true))];
                for nodes in [&guarded[..], &literal[..]] {
                    let definitions = laws(&truth, nodes, nodes.len() - 1);
                    let probe = c::Descriptor {
                        laws: &definitions,
                        ..c::Descriptor {
                            state: d.state,
                            command: d.command,
                            context: d.context,
                            program: ScalarProgram {
                                inputs: d.program.inputs,
                                outputs: d.program.outputs,
                                nodes: d.program.nodes,
                                roots: d.program.roots,
                            },
                            bindings: d.bindings,
                            output_types: d.output_types,
                            decision_output: d.decision_output,
                            branches: d.branches,
                            reasons: d.reasons,
                            channels: d.channels,
                            laws: d.laws,
                            required: d.required,
                            limits: d.limits,
                        }
                    };
                    assert_eq!(c::bind(&probe).is_ok(), !text || byte < 128);
                }
            }
        }
    });
}
