//! Native controls for the actual composition and arbitrary private prefixes.
use super::super::{InputVariant, ScalarProgram};
use super::*;
use crate::finite::evaluation::Op;
use alloc::vec;

fn limits() -> Limits {
    Limits {
        counters: [u64::MAX; 8],
    }
}
fn wire(variant: u16) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut state = vec![9, 0, 0, 0, 1, 0, 0, 7];
    state.extend(40u32.to_be_bytes());
    state.extend(7u16.to_be_bytes());
    let mut command = vec![10];
    command.extend(41u32.to_be_bytes());
    command.extend(variant.to_be_bytes());
    command.push(0);
    (state, command, vec![2])
}
fn framed(bytes: &[u8], root: u32) -> Vec<u8> {
    let mut frame = b"ZFCISV1\0".to_vec();
    frame.extend(root.to_be_bytes());
    frame.extend([5; 32]);
    frame.extend((bytes.len() as u32).to_be_bytes());
    frame.extend(bytes);
    frame
}
fn with_descriptor(run: impl FnOnce(&Descriptor<'_>)) {
    let state = [InputField {
        id: 0,
        leaf: InputLeaf::Enum {
            type_id: 40,
            min: -1,
            max: 0,
            variants: vec![
                InputVariant { id: 8, code: 0 },
                InputVariant { id: 7, code: -1 },
            ],
        },
    }];
    let command = InputLeaf::Sum {
        type_id: 41,
        min: 0,
        max: 1,
        variants: vec![
            InputVariant { id: 12, code: 1 },
            InputVariant { id: 11, code: 0 },
        ],
    };
    let context = InputLeaf::Bool;
    let inputs = [
        ScalarDomain::Int { min: -1, max: 0 },
        ScalarDomain::Int { min: 0, max: 1 },
        ScalarDomain::Bool,
    ];
    let outputs = [
        ScalarDomain::Int { min: 0, max: 1 },
        ScalarDomain::Int { min: -1, max: 0 },
    ];
    let nodes = [Op::Input(1), Op::Int(0)];
    let types = [
        InputLeaf::I128 { min: 0, max: 1 },
        InputLeaf::Enum {
            type_id: 40,
            min: -1,
            max: 0,
            variants: vec![
                InputVariant { id: 8, code: 0 },
                InputVariant { id: 7, code: -1 },
            ],
        },
    ];
    let bindings = [
        Binding {
            source: Source::State,
            selector: Selector::Field(0),
        },
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        Binding {
            source: Source::Context,
            selector: Selector::Root,
        },
    ];
    let assignments = [Assignment {
        field: 0,
        value: Expr::Output(1),
        domain: Domain::Enum {
            type_id: 40,
            variants: &[7, 8],
        },
    }];
    let payload = [PayloadField {
        field: 3,
        value: Expr::Root(Source::Command),
    }];
    let effect = [DeliveryPlan {
        ordinal: 9,
        channel: 70,
        when: Expr::Root(Source::Context),
        destination: Expr::Constant(Atom::Text(b"destination")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(u128::MAX)),
    }];
    let outbox = [DeliveryPlan {
        ordinal: u32::MAX,
        ..effect[0]
    }];
    let branches = [
        Branch {
            code: 0,
            class: Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &effect,
            outbox: &outbox,
        },
        Branch {
            code: 1,
            class: Class::Reject,
            reason: Some(77),
            assignments: &[],
            effects: &[],
            outbox: &[],
        },
    ];
    let channel_fields = [TypedField {
        field: 3,
        domain: Domain::Sum {
            type_id: 41,
            variants: &[11, 12],
        },
    }];
    let channels = [Channel {
        id: 70,
        destination: Domain::Text,
        payload: &channel_fields,
        idempotency: Domain::U128 {
            min: 0,
            max: u128::MAX,
        },
    }];
    let truth = [laws::Op::Literal(laws::Atom::Bool(true))];
    let initial = [
        laws::Op::Observe(laws::Observation::Initial(0)),
        laws::Op::Literal(laws::Atom::Enum {
            type_id: 40,
            variant: 7,
        }),
        laws::Op::Eq(0, 1),
    ];
    // Every field below is independently fixed, so dropped bridge components fail.
    let mut observations = Vec::new();
    for (o, a) in [
        (
            laws::Observation::CommandRoot,
            laws::Atom::Sum {
                type_id: 41,
                variant: 11,
            },
        ),
        (laws::Observation::ContextRoot, laws::Atom::Bool(true)),
        (
            laws::Observation::Post(0),
            laws::Atom::Enum {
                type_id: 40,
                variant: 8,
            },
        ),
        (
            laws::Observation::PatchBefore(0),
            laws::Atom::Enum {
                type_id: 40,
                variant: 7,
            },
        ),
        (
            laws::Observation::PatchAfter(0),
            laws::Atom::Enum {
                type_id: 40,
                variant: 8,
            },
        ),
        (
            laws::Observation::EffectDestination(0),
            laws::Atom::Text(b"destination"),
        ),
        (
            laws::Observation::OutboxDestination(0),
            laws::Atom::Text(b"destination"),
        ),
        (
            laws::Observation::EffectPayload(0, 3),
            laws::Atom::Sum {
                type_id: 41,
                variant: 11,
            },
        ),
        (
            laws::Observation::OutboxPayload(0, 3),
            laws::Atom::Sum {
                type_id: 41,
                variant: 11,
            },
        ),
        (
            laws::Observation::EffectIdempotency(0),
            laws::Atom::U128(u128::MAX),
        ),
        (
            laws::Observation::OutboxIdempotency(0),
            laws::Atom::U128(u128::MAX),
        ),
        (laws::Observation::EffectOrdinal(0), laws::Atom::I128(9)),
        (
            laws::Observation::OutboxOrdinal(0),
            laws::Atom::I128(u32::MAX as i128),
        ),
        (laws::Observation::EffectChannel(0), laws::Atom::I128(70)),
        (laws::Observation::ReadId(1), laws::Atom::I128(65536)),
        (
            laws::Observation::EffectAttemptSource(0),
            laws::Atom::I128(2),
        ),
        (
            laws::Observation::EffectAttemptSource(1),
            laws::Atom::I128(3),
        ),
    ] {
        let n = observations.len();
        observations.extend([
            laws::Op::Observe(o),
            laws::Op::Literal(a),
            laws::Op::Eq(n, n + 1),
        ]);
        if n > 0 {
            observations.push(laws::Op::And(n - 1, n + 2));
        }
    }
    let rows = [
        (
            1,
            laws::Kind::StateInvariant,
            laws::Scope::Committing,
            true,
            &truth[..],
        ),
        (
            2,
            laws::Kind::RejectNoAuthority,
            laws::Scope::Reject,
            false,
            &truth[..],
        ),
        (
            3,
            laws::Kind::CommittedFailureEffects,
            laws::Scope::CommittedFailure,
            false,
            &truth[..],
        ),
        (
            4,
            laws::Kind::DecisionConformance,
            laws::Scope::Always,
            false,
            &truth[..],
        ),
        (
            5,
            laws::Kind::InitialCondition,
            laws::Scope::Always,
            true,
            &initial[..],
        ),
        (
            6,
            laws::Kind::AuthoritySubjectRecipient,
            laws::Scope::Accept,
            false,
            &observations[..],
        ),
    ];
    let laws: Vec<_> = rows
        .iter()
        .map(|&(id, kind, scope, genesis, nodes)| laws::Law {
            id,
            kind,
            scope,
            genesis,
            program: laws::Program {
                nodes,
                root: nodes.len() - 1,
            },
        })
        .collect();
    let reasons = [Reason {
        id: 77,
        class: Class::Reject,
    }];
    run(&Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Leaf(&context),
        program: ScalarProgram {
            inputs: &inputs,
            outputs: &outputs,
            nodes: &nodes,
            roots: &[0, 1],
        },
        bindings: &bindings,
        output_types: &types,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &laws,
        required: &[1, 2, 3, 4, 5, 6],
        limits: limits(),
    });
}
fn trace() -> producer::Trace {
    producer::Trace {
        reads: vec![ReadAttempt {
            source: 88,
            selector: Selector::Field(42),
            permitted: false,
        }],
        attempts: vec![Attempt::Write(42, false)],
        ingress: None,
        decision: None,
    }
}
fn copy_descriptor<'a>(d: &Descriptor<'a>) -> Descriptor<'a> {
    Descriptor {
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
}
fn execute_into<'a>(
    d: &'a Descriptor<'a>,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'a>, Failure> {
    match bind(d) {
        Ok(core) => outcome::run(
            &core,
            Kind::Transition,
            raw,
            meter,
            trace,
            diagnostics,
            law_reads,
        ),
        Err(failure) => Err(failure),
    }
}

#[test]
fn actual_complete_bridge_and_original_frame_custody() {
    with_descriptor(|d| {
        let core = bind(d).unwrap_or_else(|error| {
            panic!("actual_complete_bridge_and_original_frame_custody fixture failed: {error:?}")
        });
        for variant in [11, 12] {
            let (s, c, x) = wire(variant);
            let out = core.execute(Raw {
                state: &s,
                command: &c,
                context: &x,
            });
            let candidate = out.result().unwrap_or_else(|error| {
                panic!(
                    "actual_complete_bridge_and_original_frame_custody fixture failed: {error:?}"
                )
            });
            assert_eq!(
                candidate.pre(),
                &[Field {
                    id: 0,
                    value: Atom::Enum {
                        type_id: 40,
                        variant: 7
                    }
                }]
            );
            assert_eq!(
                candidate.class(),
                if variant == 11 {
                    Class::Accept
                } else {
                    Class::Reject
                }
            );
            assert_eq!(
                candidate.reason(),
                if variant == 11 { None } else { Some(77) }
            );
            assert_eq!(candidate.post().len(), usize::from(variant == 11));
            assert_eq!(
                out.ingress_usage()
                    .unwrap_or_else(|| panic!(
                        "missing fixture value in actual_complete_bridge_and_original_frame_custody"
                    ))
                    .used(Resource::Read),
                3
            );
            assert_eq!(
                out.decision_usage()
                    .unwrap_or_else(|| panic!(
                        "missing fixture value in actual_complete_bridge_and_original_frame_custody"
                    ))
                    .used(Resource::Step),
                2
            );
        }
        let (s, c, x) = wire(11);
        let frames = [framed(&s, 100), framed(&c, 101), framed(&x, 102)];
        let binding = |root| FrameBinding {
            root,
            schema: [5; 32],
            max_bytes: 4096,
        };
        let framing = Framing {
            state: binding(100),
            command: binding(101),
            context: binding(102),
        };
        let out = core.frame(
            Kind::Transition,
            Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            },
            &framing,
        );
        assert!(out.result().is_ok());
        assert_eq!(
            out.usage().used(Resource::Byte),
            (s.len() + c.len() + x.len() + 144) as u64
        );
        assert_eq!(out.raw().state, frames[0]);
        let completed = core.genesis(&s);
        let identity = completed
            .result()
            .unwrap_or_else(|error| panic!("genesis laws fixture failed: {error:?}"));
        assert_eq!(identity.class(), Class::Accept);
        assert_eq!(identity.pre(), identity.post());
        assert!(
            identity.patch().is_empty()
                && identity.effects().is_empty()
                && identity.outbox().is_empty()
        );
        let mut bad = s.clone();
        *bad.last_mut().unwrap_or_else(|| {
            panic!("missing fixture value in actual_complete_bridge_and_original_frame_custody")
        }) = 8;
        assert!(matches!(
            core.genesis(&bad).result(),
            Err(Failure::Law(laws::Failure::Violated))
        ));
    });
}

#[test]
fn private_prefixes_and_each_counter_overflow_are_retained() {
    with_descriptor(|d| {
        let (s, c, x) = wire(12); // Reject does not depend on the observation-law fixture.
        let raw = Raw {
            state: &s,
            command: &c,
            context: &x,
        };
        let mut meter = Meter {
            limits: d.limits,
            used: Usage {
                counters: [11, 12, 13, 14, 15, 16, 17, 18],
            },
        };
        let mut t = trace();
        let prefix_diag = laws::Diagnostic {
            id: 99,
            verdict: laws::Verdict::Skipped,
        };
        let prefix_read = laws::ReadAttempt {
            law: 99,
            node: 55,
            observation: laws::Observation::Class,
            permitted: false,
        };
        let mut diagnostics = vec![prefix_diag];
        let mut reads = vec![prefix_read];
        let candidate =
            execute_into(d, raw, &mut meter, &mut t, &mut diagnostics, &mut reads)
                .unwrap_or_else(|error| panic!("private_prefixes_and_each_counter_overflow_are_retained fixture failed: {error:?}"));
        assert_eq!(candidate.class(), Class::Reject);
        assert_eq!(meter.used.counters, [14, 12, 14, 14, 38, 16, 17, 22]);
        assert_eq!(t.reads[0], trace().reads[0]);
        assert_eq!(t.attempts[0], Attempt::Write(42, false));
        assert_eq!(diagnostics[0], prefix_diag);
        assert_eq!(reads[0], prefix_read);
        for resource in [
            Resource::Byte,
            Resource::Read,
            Resource::Step,
            Resource::Candidate,
            Resource::Write,
            Resource::Effect,
        ] {
            let (s, c, x) = wire(11);
            let raw = Raw {
                state: &s,
                command: &c,
                context: &x,
            };
            let mut meter = Meter {
                limits: d.limits,
                used: Usage { counters: [0; 8] },
            };
            meter.used.counters[resource.index()] = u64::MAX;
            let mut t = trace();
            let mut diagnostics = vec![prefix_diag];
            let mut reads = vec![prefix_read];
            let result = execute_into(d, raw, &mut meter, &mut t, &mut diagnostics, &mut reads);
            let e = match result {
                Err(Failure::Ingress(
                    _,
                    ingress::Failure::Record(input_view::Failure::Budget(e)),
                ))
                | Err(Failure::Ingress(_, ingress::Failure::Budget(e)))
                | Err(Failure::Execution(super::super::Failure::Budget(e)))
                | Err(Failure::Decision(decision::Failure::Budget(e))) => e,
                other => panic!("expected exact overflow for {resource:?}: {other:?}"),
            };
            assert_eq!(e.resource, resource);
            assert!(e.overflow);
            assert_eq!(e.attempted, u64::MAX);
            assert_eq!(meter.used.used(resource), u64::MAX);
            assert_eq!(t.reads[0], trace().reads[0]);
            assert_eq!(t.attempts[0], Attempt::Write(42, false));
            assert_eq!(diagnostics, [prefix_diag]);
            assert_eq!(reads, [prefix_read]);
        }
    });
}

#[test]
fn laws_observe_all_decision_counters_before_their_own_costs() {
    with_descriptor(|d| {
        let (s, c, x) = wire(12);
        let raw = Raw {
            state: &s,
            command: &c,
            context: &x,
        };
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
        // Three original leaves, 23 original bytes, two graph nodes, one
        // Reject candidate, and no writes/deliveries. No production counter
        // report is used to build the expected law observation.
        let decision_delta = [3, 0, 1, 0, 23, 0, 0, 2];
        for initial in [[0; 8], [11, 12, 13, 14, 15, 16, 17, 18]] {
            let expected = core::array::from_fn::<_, 8, _>(|i| initial[i] + decision_delta[i]);
            // Each wrong expected counter must cause a genuine law refusal.
            // The last case checks all eight correct values together.
            for wrong in 0..=8 {
                let mut nodes = Vec::new();
                for (i, resource) in resources.into_iter().enumerate() {
                    let n = nodes.len();
                    nodes.extend([
                        laws::Op::Observe(laws::Observation::Usage(resource)),
                        laws::Op::Literal(laws::Atom::U128(
                            (expected[i] + u64::from(wrong == i)) as u128,
                        )),
                        laws::Op::Eq(n, n + 1),
                    ]);
                    if n > 0 {
                        nodes.push(laws::Op::And(n - 1, n + 2));
                    }
                }
                let definitions: Vec<_> = d
                    .laws
                    .iter()
                    .map(|l| laws::Law {
                        id: l.id,
                        kind: l.kind,
                        scope: l.scope,
                        genesis: l.genesis,
                        program: if l.id == 4 {
                            laws::Program {
                                nodes: &nodes,
                                root: nodes.len() - 1,
                            }
                        } else {
                            laws::Program {
                                nodes: l.program.nodes,
                                root: l.program.root,
                            }
                        },
                    })
                    .collect();
                let changed = Descriptor {
                    laws: &definitions,
                    ..copy_descriptor(d)
                };
                assert!(bind(&changed).is_ok());
                let mut meter = Meter {
                    limits: d.limits,
                    used: Usage { counters: initial },
                };
                let mut t = trace();
                let mut diagnostics = Vec::new();
                let mut reads = Vec::new();
                let result = execute_into(
                    &changed,
                    raw,
                    &mut meter,
                    &mut t,
                    &mut diagnostics,
                    &mut reads,
                );
                if wrong == 8 {
                    assert_eq!(result.unwrap_or_else(|error| panic!("laws_observe_all_decision_counters_before_their_own_costs fixture failed: {error:?}")).class(), Class::Reject);
                } else {
                    assert!(matches!(result, Err(Failure::Law(laws::Failure::Violated))));
                }
                assert_eq!(t.decision.unwrap_or_else(|| panic!("missing fixture value in laws_observe_all_decision_counters_before_their_own_costs")).counters, expected);
                let mut final_usage = expected;
                final_usage[0] += 8; // Actual Usage observations.
                final_usage[7] += 32; // One earlier law node and this 31-node predicate.
                assert_eq!(meter.used.counters, final_usage);
                assert_eq!(reads.len(), 8);
            }
        }
    });
}

#[test]
fn bind_refuses_root_alias_and_all_unused_bad_branches() {
    with_descriptor(|d| {
        let mut bindings = d.bindings.to_vec();
        bindings[1].selector = Selector::Field(0);
        assert!(
            bind(&Descriptor {
                bindings: &bindings,
                ..copy_descriptor(d)
            })
            .is_err()
        );
        let mut branches = d.branches.to_vec();
        branches[1].reason = Some(999);
        assert!(
            bind(&Descriptor {
                branches: &branches,
                ..copy_descriptor(d)
            })
            .is_err()
        );
        let mut nodes = d.program.nodes.to_vec();
        nodes.push(Op::Input(99));
        let mut invalid = copy_descriptor(d);
        invalid.program.nodes = &nodes;
        assert!(bind(&invalid).is_err());
    });
}

#[test]
fn eager_unused_arithmetic_and_law_observations_refuse_exactly() {
    with_descriptor(|d| {
        let (s, c, x) = wire(11);
        let raw = Raw {
            state: &s,
            command: &c,
            context: &x,
        };
        let mut nodes = d.program.nodes.to_vec();
        nodes.extend([Op::Int(i64::MAX), Op::Int(1), Op::Add(2, 3)]);
        let mut changed = copy_descriptor(d);
        changed.program.nodes = &nodes;
        let core = bind(&changed).unwrap_or_else(|error| panic!("eager_unused_arithmetic_and_law_observations_refuse_exactly fixture failed: {error:?}"));
        let out = core.execute(raw);
        assert!(matches!(
            out.result(),
            Err(Failure::Execution(super::super::Failure::Arithmetic))
        ));
        assert_eq!(out.usage().used(Resource::Step), 5);
        assert!(out.decision_attempts().is_empty());
        assert!(out.diagnostics().is_empty());
        assert!(out.ingress_usage().is_some());
        assert!(out.decision_usage().is_none());
        let unavailable = [
            laws::Op::Literal(laws::Atom::Bool(true)),
            laws::Op::Observe(laws::Observation::Initial(0)),
        ];
        let mut laws: Vec<_> = d
            .laws
            .iter()
            .map(|l| laws::Law {
                id: l.id,
                kind: l.kind,
                scope: l.scope,
                genesis: l.genesis,
                program: laws::Program {
                    nodes: l.program.nodes,
                    root: l.program.root,
                },
            })
            .collect();
        laws[3].program = laws::Program {
            nodes: &unavailable,
            root: 0,
        };
        let changed = Descriptor {
            laws: &laws,
            ..copy_descriptor(d)
        };
        let core = bind(&changed).unwrap_or_else(|error| panic!("eager_unused_arithmetic_and_law_observations_refuse_exactly fixture failed: {error:?}"));
        let out = core.execute(raw);
        assert!(matches!(
            out.result(),
            Err(Failure::Law(laws::Failure::Undefined))
        ));
        assert_eq!(out.diagnostics().last().unwrap_or_else(|| panic!("missing fixture value in eager_unused_arithmetic_and_law_observations_refuse_exactly")).id, 4);
        assert_eq!(
            out.law_reads().last().unwrap_or_else(|| panic!("missing fixture value in eager_unused_arithmetic_and_law_observations_refuse_exactly")).observation,
            laws::Observation::Initial(0)
        );
        assert!(out.law_reads().last().unwrap_or_else(|| panic!("missing fixture value in eager_unused_arithmetic_and_law_observations_refuse_exactly")).permitted);
        assert_eq!(out.usage().used(Resource::Step), 5);
        assert!(out.decision_usage().is_some());
        laws[3].program = laws::Program {
            nodes: &[laws::Op::Literal(laws::Atom::Bool(false))],
            root: 0,
        };
        let changed = Descriptor {
            laws: &laws,
            ..copy_descriptor(d)
        };
        let core = bind(&changed).unwrap_or_else(|error| panic!("eager_unused_arithmetic_and_law_observations_refuse_exactly fixture failed: {error:?}"));
        assert!(matches!(
            core.execute(raw).result(),
            Err(Failure::Law(laws::Failure::Violated))
        ));
    });
}

#[test]
fn binary_variant_input_preserves_raw_sum_and_refuses_nonbinary_aliases() {
    with_descriptor(|d| {
        let mut inputs = d.program.inputs.to_vec();
        inputs[1] = ScalarDomain::Bool;
        let nodes = [
            Op::Input(1),
            Op::Int(0),
            Op::Int(1),
            Op::Select(0, 2, 1),
            Op::Int(0),
        ];
        let mut changed = copy_descriptor(d);
        changed.program.inputs = &inputs;
        changed.program.nodes = &nodes;
        changed.program.roots = &[3, 4];
        let core = bind(&changed).unwrap_or_else(|error| panic!("binary_variant_input_preserves_raw_sum_and_refuses_nonbinary_aliases fixture failed: {error:?}"));
        for variant in [11, 12] {
            let (s, c, x) = wire(variant);
            let out = core.execute(Raw {
                state: &s,
                command: &c,
                context: &x,
            });
            assert_eq!(
                out.result().unwrap_or_else(|error| panic!("binary_variant_input_preserves_raw_sum_and_refuses_nonbinary_aliases fixture failed: {error:?}")).class(),
                if variant == 11 {
                    Class::Accept
                } else {
                    Class::Reject
                }
            );
            assert_eq!(out.raw().command, c);
        }
        for leaf in [
            InputLeaf::I128 { min: 0, max: 1 },
            InputLeaf::Sum {
                type_id: 41,
                min: 0,
                max: 2,
                variants: vec![
                    InputVariant { id: 11, code: 0 },
                    InputVariant { id: 12, code: 1 },
                    InputVariant { id: 13, code: 2 },
                ],
            },
            InputLeaf::Sum {
                type_id: 41,
                min: -1,
                max: 0,
                variants: vec![
                    InputVariant { id: 11, code: -1 },
                    InputVariant { id: 12, code: 0 },
                ],
            },
            InputLeaf::Sum {
                type_id: 41,
                min: 0,
                max: 1,
                variants: vec![
                    InputVariant { id: 11, code: 0 },
                    InputVariant { id: 12, code: 0 },
                ],
            },
        ] {
            let invalid = Descriptor {
                command: Schema::Leaf(&leaf),
                ..copy_descriptor(&changed)
            };
            assert!(bind(&invalid).is_err());
        }
    });
}

#[test]
fn every_law_literal_including_unused_and_inapplicable_text_is_canonical() {
    with_descriptor(|d| {
        for index in 0..d.laws.len() {
            for text in [&b"ASCII\0\x7f"[..], &b"\x80"[..], &b"\xc3\xa9"[..]] {
                let mut nodes = d.laws[index].program.nodes.to_vec();
                nodes.push(laws::Op::Literal(laws::Atom::Text(text)));
                let mut definitions: Vec<_> = d
                    .laws
                    .iter()
                    .map(|l| laws::Law {
                        id: l.id,
                        kind: l.kind,
                        scope: l.scope,
                        genesis: l.genesis,
                        program: laws::Program {
                            nodes: l.program.nodes,
                            root: l.program.root,
                        },
                    })
                    .collect();
                definitions[index].program.nodes = &nodes;
                let changed = Descriptor {
                    laws: &definitions,
                    ..copy_descriptor(d)
                };
                assert_eq!(bind(&changed).is_ok(), text.is_ascii());
            }
        }
    });
}

#[test]
fn account_raw_facts_feed_actual_graph_and_preserve_seen_and_deadline() {
    let field = |id, max| InputField {
        id,
        leaf: InputLeaf::I128 { min: 0, max },
    };
    let state = [
        field(110, 2),
        field(111, 4102445700),
        field(112, 4102444800),
    ];
    let command = InputLeaf::Sum {
        type_id: 101,
        min: 120,
        max: 122,
        variants: vec![
            InputVariant { id: 120, code: 120 },
            InputVariant { id: 121, code: 121 },
            InputVariant { id: 122, code: 122 },
        ],
    };
    let context = [
        field(130, 4102444800),
        InputField {
            id: 131,
            leaf: InputLeaf::Bool,
        },
    ];
    let bindings = [
        Binding {
            source: Source::State,
            selector: Selector::Field(110),
        },
        Binding {
            source: Source::State,
            selector: Selector::Field(111),
        },
        Binding {
            source: Source::State,
            selector: Selector::Field(112),
        },
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        Binding {
            source: Source::Context,
            selector: Selector::Field(130),
        },
        Binding {
            source: Source::Context,
            selector: Selector::Field(131),
        },
    ];
    let types = [
        InputLeaf::I128 { min: 0, max: 6 },
        InputLeaf::I128 {
            min: i64::MIN,
            max: i64::MAX,
        },
        InputLeaf::I128 {
            min: i64::MIN,
            max: i64::MAX,
        },
    ];
    let third = [
        Assignment {
            field: 110,
            value: Expr::Constant(Atom::I128(0)),
            domain: Domain::I128 { min: 0, max: 2 },
        },
        Assignment {
            field: 111,
            value: Expr::Output(1),
            domain: Domain::I128 {
                min: 0,
                max: 4102445700,
            },
        },
        Assignment {
            field: 112,
            value: Expr::Input(Source::Context, 130),
            domain: Domain::I128 {
                min: 0,
                max: 4102444800,
            },
        },
    ];
    let branches: Vec<_> = (0..7)
        .map(|code| Branch {
            code,
            class: if code == 4 {
                Class::CommittedFailure
            } else {
                Class::Reject
            },
            reason: Some(100 + code as u32),
            assignments: if code == 4 { &third } else { &[] },
            effects: &[],
            outbox: &[],
        })
        .collect();
    let reasons: Vec<_> = (0..7)
        .map(|code| Reason {
            id: 100 + code,
            class: if code == 4 {
                Class::CommittedFailure
            } else {
                Class::Reject
            },
        })
        .collect();
    let truth = [laws::Op::Literal(laws::Atom::Bool(true))];
    let families = [
        (laws::Kind::StateInvariant, laws::Scope::Committing, true),
        (laws::Kind::RejectNoAuthority, laws::Scope::Reject, false),
        (
            laws::Kind::CommittedFailureEffects,
            laws::Scope::CommittedFailure,
            false,
        ),
        (laws::Kind::DecisionConformance, laws::Scope::Always, false),
        (laws::Kind::InitialCondition, laws::Scope::Always, true),
    ];
    let laws: Vec<_> = families
        .iter()
        .enumerate()
        .map(|(i, &(kind, scope, genesis))| laws::Law {
            id: i as u32 + 1,
            kind,
            scope,
            genesis,
            program: laws::Program {
                nodes: &truth,
                root: 0,
            },
        })
        .collect();
    let d = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Record(&context),
        program: account_tests::program(),
        bindings: &bindings,
        output_types: &types,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &[],
        laws: &laws,
        required: &[1, 2, 3, 4, 5],
        limits: limits(),
    };
    let core = bind(&d).unwrap_or_else(|error| panic!("account_raw_facts_feed_actual_graph_and_preserve_seen_and_deadline fixture failed: {error:?}"));
    for failed in 0..=2i128 {
        for until in [0, 1000, 4102445700i128] {
            for seen in [0, 1001, 4102444800i128] {
                for cmd in 120..=122u16 {
                    for now in [0, 1000, 4102444800i128] {
                        for admin in [false, true] {
                            let mut s = vec![9, 0, 0, 0, 3];
                            for (id, v) in [(110u16, failed), (111, until), (112, seen)] {
                                s.extend(id.to_be_bytes());
                                s.push(4);
                                s.extend(v.to_be_bytes());
                            }
                            let mut c = vec![10, 0, 0, 0, 101];
                            c.extend(cmd.to_be_bytes());
                            c.push(0);
                            let mut x = vec![9, 0, 0, 0, 2, 0, 130, 4];
                            x.extend(now.to_be_bytes());
                            x.extend([0, 131, if admin { 2 } else { 1 }]);
                            let expected = if now < seen {
                                0
                            } else if cmd != 122 && now < until {
                                1
                            } else if cmd == 122 && !admin {
                                2
                            } else if cmd == 120 {
                                3
                            } else if cmd == 121 && failed == 2 {
                                4
                            } else if cmd == 121 {
                                5
                            } else {
                                6
                            };
                            let out = core.execute(Raw {
                                state: &s,
                                command: &c,
                                context: &x,
                            });
                            let candidate = out.result().unwrap_or_else(|error| panic!("account_raw_facts_feed_actual_graph_and_preserve_seen_and_deadline fixture failed: {error:?}"));
                            assert_eq!(candidate.reason(), Some(100 + expected));
                            assert_eq!(candidate.pre()[2].value, Atom::I128(seen));
                            if expected == 4 {
                                assert_eq!(candidate.post()[1].value, Atom::I128(now + 900));
                                assert_eq!(candidate.post()[2].value, Atom::I128(now));
                            }
                            assert_eq!(out.ingress_usage().unwrap_or_else(|| panic!("missing fixture value in account_raw_facts_feed_actual_graph_and_preserve_seen_and_deadline")).used(Resource::Step), 0);
                            assert_eq!(out.decision_usage().unwrap_or_else(|| panic!("missing fixture value in account_raw_facts_feed_actual_graph_and_preserve_seen_and_deadline")).used(Resource::Step), d.program.nodes.len() as u64);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn bound_core_reuse_keeps_borrowed_descriptor_and_fresh_usage() {
    with_descriptor(|d| {
        let bound = bind(d).unwrap_or_else(|error| panic!("bound_core_reuse_keeps_borrowed_descriptor_and_fresh_usage fixture failed: {error:?}"));
        assert!(core::ptr::eq(bound.descriptor(), d));
        let (s, c, x) = wire(12);
        let raw = Raw {
            state: &s,
            command: &c,
            context: &x,
        };
        let first = bound.execute(raw);
        let second = bound.execute(raw);
        for out in [&first, &second] {
            assert!(matches!(out.result(), Ok(candidate) if candidate.class() == Class::Reject));
        }
        assert_eq!(first.usage().counters, second.usage().counters);
        assert_eq!(first.reads().len(), second.reads().len());
        assert_eq!(
            first.decision_attempts().len(),
            second.decision_attempts().len()
        );
        assert_eq!(first.diagnostics().len(), second.diagnostics().len());
        assert_eq!(first.law_reads().len(), second.law_reads().len());
    });
}
