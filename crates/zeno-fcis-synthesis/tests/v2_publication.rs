//! Actual bound publication compared with independent original Value/Envelope encoders.
//! Policy serialization is fixture construction only; output oracles never call it.
use std::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    self, Domain as ScalarDomain, Op, V2InputField as InputField, V2InputLeaf as InputLeaf,
    V2Limits as Limits, V2Resource as Resource, V2ScalarProgram as ScalarProgram,
    canonical_v2::schema, v2_authority as authority, v2_catalog as catalog, v2_composition as c,
    v2_laws as laws,
};

fn w(out: &mut Vec<u8>, v: u128) {
    out.push(0);
    out.extend_from_slice(&v.to_be_bytes());
}
fn b(out: &mut Vec<u8>, v: &[u8]) {
    out.push(1);
    out.extend_from_slice(&(v.len() as u128).to_be_bytes());
    out.extend_from_slice(v);
}
fn int(out: &mut Vec<u8>, v: i128) {
    w(out, 1);
    w(out, v as u128);
}
fn text(out: &mut Vec<u8>, v: &[u8]) {
    w(out, 6);
    b(out, v);
}
fn generous() -> Limits {
    let mut limits = finite::v2_zero_limits();
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
fn fixture(limits: Limits, conformance: bool, run: impl FnOnce(&mut c::Descriptor<'_>)) {
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
    let mut descriptor = c::Descriptor {
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
fn framing() -> c::Framing {
    c::Framing {
        state: c::FrameBinding {
            root: 100,
            schema: [23; 32],
            max_bytes: 512,
        },
        command: c::FrameBinding {
            root: 101,
            schema: [23; 32],
            max_bytes: 512,
        },
        context: c::FrameBinding {
            root: 102,
            schema: [23; 32],
            max_bytes: 512,
        },
    }
}
fn frame(root: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"ZFCISV1\0".to_vec();
    bytes.extend_from_slice(&root.to_be_bytes());
    bytes.extend_from_slice(&[23; 32]);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
fn originals(code: i128, initial: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let state = [9, 0, 0, 0, 1, 0, 0, if initial { 2 } else { 1 }];
    let mut command = vec![4];
    command.extend_from_slice(&code.to_be_bytes());
    (
        frame(100, &state),
        frame(101, &command),
        frame(102, &[9, 0, 0, 0, 0]),
    )
}
fn raw(values: &(Vec<u8>, Vec<u8>, Vec<u8>)) -> c::Raw<'_> {
    c::Raw {
        state: &values.0,
        command: &values.1,
        context: &values.2,
    }
}
fn counters(out: &mut Vec<u8>, used: [u128; 8]) {
    for v in used {
        w(out, v);
    }
}
fn read_oracle(out: &mut Vec<u8>, genesis: bool) {
    w(out, if genesis { 1 } else { 2 });
    for v in [0, 1, 0, 1] {
        w(out, v);
    }
    if !genesis {
        for v in [1, 0, 1] {
            w(out, v);
        }
    }
}
fn candidate_oracle(out: &mut Vec<u8>, code: u128) {
    for v in [0x5a434432, 1, code] {
        w(out, v);
    }
    if code == 0 {
        w(out, 0);
    } else {
        w(out, 1);
        w(out, if code == 1 { 7 } else { 9 });
    }
    for v in [1, 0, 0, 1] {
        w(out, v);
    }
    if code == 1 {
        for _ in 0..4 {
            w(out, 0);
        }
    } else {
        for v in [1, 0, 0, 0, 1, 0, 0, 1, 0, 0] {
            w(out, v);
        }
        for (ordinal, destination, key) in [
            (5, &b"effect"[..], &b"ekey"[..]),
            (9, &b"outbox"[..], &b"okey"[..]),
        ] {
            w(out, 1);
            w(out, ordinal);
            w(out, 3);
            text(out, destination);
            w(out, 1);
            w(out, 0);
            int(out, code as i128);
            w(out, 5);
            b(out, key);
        }
    }
}
fn transition_oracle(identity: &[u8], values: &(Vec<u8>, Vec<u8>, Vec<u8>), code: u128) -> Vec<u8> {
    let mut artifact = Vec::new();
    for v in [0x5a4f5532, 1, 1] {
        w(&mut artifact, v);
    }
    candidate_oracle(&mut artifact, code);
    w(&mut artifact, 1);
    counters(&mut artifact, [2, 0, 0, 0, 174, 0, 0, 0]);
    let decision = if code == 1 {
        [2, 0, 1, 0, 174, 0, 0, 1]
    } else {
        [2, 1, 1, 2, 174, 0, 0, 1]
    };
    w(&mut artifact, 1);
    counters(&mut artifact, decision);
    let mut final_used = decision;
    final_used[0] = 3;
    final_used[7] = if code == 2 { 11 } else { 10 };
    counters(&mut artifact, final_used);
    read_oracle(&mut artifact, false);
    w(&mut artifact, if code == 1 { 1 } else { 4 });
    w(&mut artifact, 0);
    w(&mut artifact, 1);
    if code != 1 {
        for v in [1, 0, 1, 2, 0, 5, 1, 2, 1, 9, 1] {
            w(&mut artifact, v);
        }
    }
    w(&mut artifact, 5);
    for (id, verdict) in [
        (10, if code == 1 { 0 } else { 1 }),
        (20, if code == 1 { 1 } else { 0 }),
        (30, if code == 2 { 1 } else { 0 }),
        (40, 1),
        (50, 0),
    ] {
        w(&mut artifact, id);
        w(&mut artifact, verdict);
    }
    for v in [1, 40, 0, 1, 1] {
        w(&mut artifact, v);
    }
    let mut subject = Vec::new();
    for v in [0x5a525032, 1, 1] {
        w(&mut subject, v);
    }
    for bytes in [identity, &values.0, &values.1, &values.2, &artifact] {
        b(&mut subject, bytes);
    }
    subject
}

use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_value::{Field as ValueField, Value};

fn record(fields: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(
        fields
            .into_iter()
            .map(|(id, value)| ValueField::new(id, value))
            .collect(),
    )
    .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
}
fn envelope(root: u32, value: Value) -> Vec<u8> {
    Envelope::new(root, Hash32::new([23; 32]), value)
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
}
fn publication_oracle(
    identity: &[u8],
    values: &(Vec<u8>, Vec<u8>, Vec<u8>),
    code: u128,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    for n in [0x5a505532, 1, 1] {
        w(&mut bytes, n);
    }
    b(&mut bytes, &transition_oracle(identity, values, code));
    b(
        &mut bytes,
        &envelope(100, record(vec![(0, Value::boolean(false))])),
    );
    for (ordinal, destination, key) in [(5, "effect", b"ekey"), (9, "outbox", b"okey")] {
        for n in [1, ordinal, 3, 200, 201] {
            w(&mut bytes, n);
        }
        b(
            &mut bytes,
            &Value::text_ascii(destination.into())
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        );
        b(
            &mut bytes,
            &record(vec![(0, Value::signed(code as i128))])
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        );
        b(
            &mut bytes,
            &Value::bytes(key.to_vec())
                .unwrap_or_else(|error| panic!("value fixture: {error}"))
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        );
    }
    bytes
}

#[test]
fn publication_original_codec_oracle_all_classes_and_reports() {
    with_catalog(b"Publication", u64::MAX, true, |catalog, _| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        for code in 0..=2 {
            let values = originals(code, true);
            let evaluated = a.evaluate(raw(&values));
            let result = a.publish(raw(&values));
            let audit = result.evaluation();
            assert_eq!(audit.subject(), evaluated.subject());
            assert_eq!(audit.usage(), evaluated.usage());
            assert_eq!(audit.reads(), evaluated.reads());
            assert_eq!(audit.decision_attempts(), evaluated.decision_attempts());
            assert_eq!(audit.diagnostics(), evaluated.diagnostics());
            assert_eq!(audit.law_reads(), evaluated.law_reads());
            assert_eq!(audit.raw().state, values.0);
            assert_eq!(audit.raw().command, values.1);
            assert_eq!(audit.raw().context, values.2);
            if code == 1 {
                let authority::PublicationOutcome::Reject(reject) = result else {
                    panic!("Reject acquired capability")
                };
                assert_eq!(
                    reject
                        .result()
                        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                        .reason(),
                    Some(7)
                );
                assert!(matches!(
                    a.replay_publication(
                        raw(&values),
                        reject
                            .subject()
                            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                    ),
                    authority::PublicationOutcome::Reject(_)
                ));
                assert!(matches!(
                    a.replay_publication(raw(&values), b"changed"),
                    authority::PublicationOutcome::Refused {
                        error: authority::Refusal::ReplayMismatch,
                        ..
                    }
                ));
                continue;
            }
            let authority::PublicationOutcome::Commit(p) = result else {
                panic!("committing decision refused")
            };
            assert_eq!(p.identity(), a.identity());
            assert_eq!(
                p.evaluation()
                    .result()
                    .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                    .class(),
                if code == 0 {
                    c::Class::Accept
                } else {
                    c::Class::CommittedFailure
                }
            );
            assert_eq!(
                p.evaluation()
                    .result()
                    .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                    .reason(),
                if code == 0 { None } else { Some(9) }
            );
            assert_eq!(
                p.poststate(),
                envelope(100, record(vec![(0, Value::boolean(false))]))
            );
            assert_eq!(
                p.subject(),
                publication_oracle(a.identity(), &values, code as u128)
            );
            assert_eq!(p.effects().len(), 1);
            assert_eq!(p.outbox().len(), 1);
            for (d, ordinal, destination, key) in [
                (&p.effects()[0], 5, "effect", b"ekey"),
                (&p.outbox()[0], 9, "outbox", b"okey"),
            ] {
                assert_eq!(
                    (
                        d.ordinal(),
                        d.channel(),
                        d.destination_root(),
                        d.payload_root()
                    ),
                    (ordinal, 3, 200, 201)
                );
                assert_eq!(
                    d.destination(),
                    Value::text_ascii(destination.into())
                        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                        .canonical_bytes()
                        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                );
                assert_eq!(
                    d.payload(),
                    record(vec![(0, Value::signed(code))])
                        .canonical_bytes()
                        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                );
                assert_eq!(
                    d.idempotency(),
                    Value::bytes(key.to_vec())
                        .unwrap_or_else(|error| panic!("value fixture: {error}"))
                        .canonical_bytes()
                        .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                );
            }
            assert_eq!(values, originals(code, true));
            let authority::PublicationOutcome::Commit(replayed) =
                a.replay_publication(raw(&values), p.subject())
            else {
                panic!("replay")
            };
            assert_eq!(replayed.subject(), p.subject());
            assert_eq!(replayed.poststate(), p.poststate());
        }
    });
}

#[test]
fn publication_replay_checks_complete_output_input_and_authority_identity() {
    with_catalog(b"Publication", u64::MAX, true, |catalog, _| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        let values = originals(0, true);
        let authority::PublicationOutcome::Commit(p) = a.publish(raw(&values)) else {
            panic!("commit")
        };
        let subject = p.subject();
        // Every original-wire tail byte, framing tag/count and selected identity bytes.
        let tail = subject.len() - 500;
        for i in (tail..subject.len()).chain([0, 16, 17, 33, 50, 51, 70, 100, subject.len() / 2]) {
            let mut changed = subject.to_vec();
            changed[i] ^= 1;
            let refused = a.replay_publication(raw(&values), &changed);
            assert!(
                matches!(
                    refused,
                    authority::PublicationOutcome::Refused {
                        error: authority::Refusal::ReplayMismatch,
                        ..
                    }
                ),
                "byte {i}"
            );
            assert_eq!(refused.evaluation().usage(), p.evaluation().usage());
        }
        for expected in [
            &[][..],
            &subject[..subject.len() - 1],
            p.evaluation()
                .subject()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        ] {
            assert!(matches!(
                a.replay_publication(raw(&values), expected),
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        let stale = originals(0, false);
        let different = originals(2, true);
        for changed in [&stale, &different] {
            assert!(matches!(
                a.replay_publication(raw(changed), subject),
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        with_catalog(b"ChangedPolicy", u64::MAX, true, |other, _| {
            let other = authority::bind(other)
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
            assert_ne!(other.identity(), a.identity());
            assert!(matches!(
                other.replay_publication(raw(&values), subject),
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::ReplayMismatch,
                    ..
                }
            ));
        });
    });
}

#[test]
fn publication_genesis_uses_actual_initial_laws_and_complete_bytes() {
    with_catalog(b"Publication", u64::MAX, true, |catalog, _| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        let values = originals(0, true);
        let authority::PublicationOutcome::Commit(p) = a.publish_genesis(&values.0) else {
            panic!("genesis")
        };
        assert_eq!(p.poststate(), values.0);
        assert_eq!(p.identity(), a.identity());
        assert_eq!(p.evaluation().usage().used(Resource::Byte), 56);
        assert!(
            p.evaluation()
                .diagnostics()
                .iter()
                .any(|d| d.id == 50 && d.verdict == laws::Verdict::Satisfied)
        );
        let mut expected = Vec::new();
        for n in [0x5a505532, 1, 0] {
            w(&mut expected, n);
        }
        b(
            &mut expected,
            a.genesis(&values.0)
                .subject()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        );
        b(&mut expected, &values.0);
        w(&mut expected, 0);
        w(&mut expected, 0);
        assert_eq!(p.subject(), expected);
        assert!(matches!(
            a.replay_genesis_publication(&values.0, p.subject()),
            authority::PublicationOutcome::Commit(_)
        ));
        for index in [0, 16, 50, p.subject().len() - 1] {
            let mut changed = p.subject().to_vec();
            changed[index] ^= 1;
            assert!(matches!(
                a.replay_genesis_publication(&values.0, &changed),
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        let bad = originals(0, false);
        let refused = a.replay_genesis_publication(&bad.0, p.subject());
        assert!(matches!(
            refused,
            authority::PublicationOutcome::Refused {
                error: authority::Refusal::Core(c::Failure::Law(laws::Failure::Violated)),
                ..
            }
        ));
        assert_eq!(
            refused
                .evaluation()
                .diagnostics()
                .last()
                .unwrap_or_else(|| panic!("missing actual diagnostic"))
                .id,
            50
        );
        let authority::PublicationOutcome::Commit(transition) = a.publish(raw(&values)) else {
            panic!("commit")
        };
        assert!(matches!(
            a.replay_genesis_publication(&values.0, transition.subject()),
            authority::PublicationOutcome::Refused {
                error: authority::Refusal::ReplayMismatch,
                ..
            }
        ));
        assert!(matches!(
            a.replay_publication(raw(&values), p.subject()),
            authority::PublicationOutcome::Refused {
                error: authority::Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}

#[test]
fn publication_technical_refusal_retains_work_and_has_no_wire_capability() {
    with_catalog(b"Publication", u64::MAX, true, |catalog, _| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        for index in [0, 8, 12, 44] {
            let mut values = originals(0, true);
            values.1[index] ^= 1;
            let result = a.publish(raw(&values));
            assert!(matches!(
                result,
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::Core(c::Failure::Frame(1, _)),
                    ..
                }
            ));
            assert_eq!(result.evaluation().usage().used(Resource::Byte), 96);
            assert!(result.evaluation().result().is_err());
        }
    });
    for (limit, conformance) in [(160, true), (u64::MAX, false)] {
        with_catalog(b"Publication", limit, conformance, |catalog, _| {
            let a = authority::bind(catalog)
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
            let values = originals(0, true);
            let result = a.publish(raw(&values));
            assert!(matches!(
                result,
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::Core(_),
                    ..
                }
            ));
            assert!(result.evaluation().usage().used(Resource::Byte) > 0);
            if !conformance {
                assert_eq!(
                    result
                        .evaluation()
                        .diagnostics()
                        .last()
                        .unwrap_or_else(|| panic!("missing actual diagnostic"))
                        .id,
                    40
                );
            }
        });
    }
}

#[test]
fn publication_every_entry_and_variant_retains_its_invocation_kind() {
    with_catalog(b"Publication", u64::MAX, true, |catalog, _| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        let values = originals(0, true);
        let rejected = originals(1, true);
        let invalid = originals(0, false);
        let mut corrupt = originals(0, true);
        corrupt.1[0] ^= 1;
        let subject = |outcome: &authority::PublicationOutcome<'_>| match outcome {
            authority::PublicationOutcome::Commit(p) => p.subject().to_vec(),
            other => panic!("committing publication required: {other:?}"),
        };
        let transition = subject(&a.publish(raw(&values)));
        let genesis = subject(&a.publish_genesis(&values.0));
        let reject = a.publish(raw(&rejected));
        let reject_subject = reject
            .evaluation()
            .subject()
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
            .to_vec();
        // Each variant retains its evaluation; a capability repeats the same tag.
        for (outcome, variant, kind) in [
            (a.publish(raw(&values)), "commit", c::Kind::Transition),
            (reject, "reject", c::Kind::Transition),
            (a.publish(raw(&corrupt)), "refused", c::Kind::Transition),
            (
                a.replay_publication(raw(&values), &transition),
                "commit",
                c::Kind::Transition,
            ),
            (
                a.replay_publication(raw(&rejected), &reject_subject),
                "reject",
                c::Kind::Transition,
            ),
            (
                a.replay_publication(raw(&values), &genesis),
                "mismatch",
                c::Kind::Transition,
            ),
            (a.publish_genesis(&values.0), "commit", c::Kind::Genesis),
            (a.publish_genesis(&invalid.0), "refused", c::Kind::Genesis),
            (
                a.replay_genesis_publication(&values.0, &genesis),
                "commit",
                c::Kind::Genesis,
            ),
            (
                a.replay_genesis_publication(&values.0, &transition),
                "mismatch",
                c::Kind::Genesis,
            ),
        ] {
            let actual = if let authority::PublicationOutcome::Commit(p) = &outcome {
                assert_eq!(p.evaluation().kind(), kind);
                "commit"
            } else if matches!(outcome, authority::PublicationOutcome::Reject(_)) {
                "reject"
            } else if matches!(
                outcome,
                authority::PublicationOutcome::Refused {
                    error: authority::Refusal::ReplayMismatch,
                    ..
                }
            ) {
                "mismatch"
            } else if matches!(outcome, authority::PublicationOutcome::Refused { .. }) {
                "refused"
            } else {
                panic!("unknown publication variant: {outcome:?}")
            };
            assert_eq!(actual, variant);
            assert_eq!(outcome.evaluation().kind(), kind);
            assert_eq!(outcome.evaluation().outcome().kind(), kind);
        }
        let audited = [a.evaluate(raw(&values)), a.genesis(&values.0)].map(|e| {
            e.subject()
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"))
                .to_vec()
        });
        for (evaluation, verdict, kind) in [
            (
                a.replay(raw(&values), &audited[0]),
                "accepted",
                c::Kind::Transition,
            ),
            (
                a.replay(raw(&values), &audited[1]),
                "mismatch",
                c::Kind::Transition,
            ),
            (
                a.replay(raw(&corrupt), &audited[0]),
                "refused",
                c::Kind::Transition,
            ),
            (
                a.replay_genesis(&values.0, &audited[1]),
                "accepted",
                c::Kind::Genesis,
            ),
            (
                a.replay_genesis(&values.0, &audited[0]),
                "mismatch",
                c::Kind::Genesis,
            ),
            (
                a.replay_genesis(&invalid.0, &audited[1]),
                "refused",
                c::Kind::Genesis,
            ),
        ] {
            let actual = match evaluation.result() {
                Ok(_) => "accepted",
                Err(authority::Refusal::ReplayMismatch) => "mismatch",
                Err(authority::Refusal::Core(_)) => "refused",
                Err(other) => panic!("unexpected refusal: {other:?}"),
            };
            assert_eq!(actual, verdict);
            assert_eq!(evaluation.kind(), kind);
            assert_eq!(evaluation.outcome().kind(), kind);
        }
        // Unframed core execution tags its outcome, including after refusal.
        let core = c::bind(a.descriptor())
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        let payloads = c::Raw {
            state: &values.0[48..],
            command: &values.1[48..],
            context: &values.2[48..],
        };
        for (outcome, accepted, kind) in [
            (core.execute(payloads), true, c::Kind::Transition),
            (core.execute(raw(&values)), false, c::Kind::Transition),
            (core.genesis(&values.0[48..]), true, c::Kind::Genesis),
            (core.genesis(&values.0), false, c::Kind::Genesis),
        ] {
            assert_eq!(outcome.result().is_ok(), accepted);
            assert_eq!(outcome.kind(), kind);
        }
    });
}
// Original wire is authored separately from the proposed schema description.
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
fn with_catalog(
    profile: &[u8],
    byte_limit: u64,
    conformance: bool,
    run: impl FnOnce(&catalog::BoundCatalog<'_>, &[u8]),
) {
    fixture(
        generous().with_limit(Resource::Byte, byte_limit),
        conformance,
        |descriptor| {
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
            let policy = authority::policy_bytes(descriptor, &original, &framed, &links)
                .unwrap_or_else(|| panic!("reviewed fixture policy"));
            let limits = catalog::Limits {
                schema: schema::Limits {
                    bytes: u64::MAX,
                    types: u32::MAX,
                    fields: u32::MAX,
                    variants: u32::MAX,
                },
                contract_bytes: u64::MAX,
            };
            let bound = catalog::bind_original(
                &original,
                &description,
                limits,
                &policy,
                descriptor,
                &framed,
                &links,
            )
            .unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
            assert_eq!(bound.original_schema(), original);
            run(&bound, &policy);
        },
    );
}

// Fixture schema bytes are authored here; original Value/Envelope implementations
// remain the independent output oracle for the admitted production publication.
fn schema_fixture(d: &schema::Description<'_>) -> Vec<u8> {
    fn name(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    let mut out = b"ZFCISSCHEMA1\0".to_vec();
    name(&mut out, d.profile);
    out.extend_from_slice(&d.version.to_be_bytes());
    out.extend_from_slice(&d.root.to_be_bytes());
    out.extend_from_slice(&(d.definitions.len() as u32).to_be_bytes());
    for def in d.definitions {
        out.extend_from_slice(&def.id.to_be_bytes());
        name(&mut out, def.name);
        match def.kind {
            schema::Kind::Unit => out.push(0),
            schema::Kind::Bool => out.push(1),
            schema::Kind::U128 { min, max } => {
                out.push(2);
                out.extend_from_slice(&min.to_be_bytes());
                out.extend_from_slice(&max.to_be_bytes());
            }
            schema::Kind::I128 { min, max } => {
                out.push(3);
                out.extend_from_slice(&min.to_be_bytes());
                out.extend_from_slice(&max.to_be_bytes());
            }
            schema::Kind::Bytes { min, max } | schema::Kind::Text { min, max } => {
                out.push(if matches!(def.kind, schema::Kind::Bytes { .. }) {
                    4
                } else {
                    5
                });
                out.extend_from_slice(&min.to_be_bytes());
                out.extend_from_slice(&max.to_be_bytes());
            }
            schema::Kind::Enum(vs) | schema::Kind::Sum(vs) => {
                let sum = matches!(def.kind, schema::Kind::Sum(_));
                out.push(if sum { 9 } else { 6 });
                out.extend_from_slice(&(vs.len() as u32).to_be_bytes());
                for v in vs {
                    out.extend_from_slice(&v.id.to_be_bytes());
                    name(&mut out, v.name);
                    if sum {
                        out.push(0);
                    }
                }
            }
            schema::Kind::Record(fs) => {
                out.push(8);
                out.extend_from_slice(&(fs.len() as u32).to_be_bytes());
                for f in fs {
                    out.extend_from_slice(&f.id.to_be_bytes());
                    name(&mut out, f.name);
                    out.extend_from_slice(&f.type_id.to_be_bytes());
                }
            }
            _ => panic!("unsupported future schema kind in the original-wire oracle"),
        }
    }
    out
}

fn wide_catalog(cap: u64, swap_roots: bool, run: impl FnOnce(&catalog::BoundCatalog<'_>)) {
    use finite::V2InputVariant as InputVariant;
    fixture(generous(), true, |d| {
        let variants = [0, u16::MAX];
        let codes = vec![
            InputVariant {
                id: 0,
                code: i64::MIN,
            },
            InputVariant {
                id: u16::MAX,
                code: i64::MIN + 1,
            },
        ];
        let state = [
            InputField {
                id: 0,
                leaf: InputLeaf::Enum {
                    type_id: 500,
                    min: i64::MIN,
                    max: i64::MIN + 1,
                    variants: codes.clone(),
                },
            },
            InputField {
                id: u16::MAX,
                leaf: InputLeaf::Sum {
                    type_id: 501,
                    min: i64::MIN,
                    max: i64::MIN + 1,
                    variants: codes,
                },
            },
        ];
        let domains = [
            ScalarDomain::Int {
                min: i64::MIN,
                max: i64::MIN + 1,
            },
            ScalarDomain::Int {
                min: i64::MIN,
                max: i64::MIN + 1,
            },
            ScalarDomain::Int { min: 0, max: 2 },
        ];
        let outputs = [ScalarDomain::Int { min: 0, max: 2 }];
        let nodes = [Op::Input(2)];
        let roots = [0];
        let bindings = [
            c::Binding {
                source: c::Source::State,
                selector: c::Selector::Field(0),
            },
            c::Binding {
                source: c::Source::State,
                selector: c::Selector::Field(u16::MAX),
            },
            c::Binding {
                source: c::Source::Command,
                selector: c::Selector::Root,
            },
        ];
        let assignments = [
            c::Assignment {
                field: 0,
                value: c::Expr::Constant(c::Atom::Enum {
                    type_id: 500,
                    variant: u16::MAX,
                }),
                domain: c::Domain::Enum {
                    type_id: 500,
                    variants: &variants,
                },
            },
            c::Assignment {
                field: u16::MAX,
                value: c::Expr::Constant(c::Atom::Sum {
                    type_id: 501,
                    variant: 0,
                }),
                domain: c::Domain::Sum {
                    type_id: 501,
                    variants: &variants,
                },
            },
        ];
        let payload = [
            c::PayloadField {
                field: 0,
                value: c::Expr::Constant(c::Atom::I128(i128::MIN)),
            },
            c::PayloadField {
                field: 1,
                value: c::Expr::Constant(c::Atom::I128(i128::MAX)),
            },
            c::PayloadField {
                field: 2,
                value: c::Expr::Constant(c::Atom::U128(u128::MAX)),
            },
            c::PayloadField {
                field: 3,
                value: c::Expr::Constant(c::Atom::Enum {
                    type_id: 500,
                    variant: 0,
                }),
            },
            c::PayloadField {
                field: 4,
                value: c::Expr::Constant(c::Atom::Sum {
                    type_id: 501,
                    variant: u16::MAX,
                }),
            },
            c::PayloadField {
                field: 5,
                value: c::Expr::Constant(c::Atom::Bytes(b"\0\xff")),
            },
            c::PayloadField {
                field: u16::MAX,
                value: c::Expr::Constant(c::Atom::Text(b"\0\x7f")),
            },
        ];
        let delivery = |ordinal, channel, destination, key| c::DeliveryPlan {
            ordinal,
            channel,
            when: c::Expr::Constant(c::Atom::Bool(true)),
            destination: c::Expr::Constant(c::Atom::U128(destination)),
            payload: &payload,
            idempotency: c::Expr::Constant(c::Atom::Bytes(key)),
        };
        let effects = [
            delivery(0, 3, 0, b""),
            delivery(u32::MAX, 4, u128::MAX, b"\xff"),
        ];
        let outbox = [
            delivery(1, 4, u128::MAX, b"out"),
            delivery(u32::MAX, 3, 0, b"last"),
        ];
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
        let typed = [
            c::TypedField {
                field: 0,
                domain: c::Domain::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
            },
            c::TypedField {
                field: 1,
                domain: c::Domain::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
            },
            c::TypedField {
                field: 2,
                domain: c::Domain::U128 {
                    min: 0,
                    max: u128::MAX,
                },
            },
            c::TypedField {
                field: 3,
                domain: c::Domain::Enum {
                    type_id: 500,
                    variants: &variants,
                },
            },
            c::TypedField {
                field: 4,
                domain: c::Domain::Sum {
                    type_id: 501,
                    variants: &variants,
                },
            },
            c::TypedField {
                field: 5,
                domain: c::Domain::Bytes,
            },
            c::TypedField {
                field: u16::MAX,
                domain: c::Domain::Text,
            },
        ];
        let channels = [
            c::Channel {
                id: 3,
                destination: c::Domain::U128 {
                    min: 0,
                    max: u128::MAX,
                },
                payload: &typed,
                idempotency: c::Domain::Bytes,
            },
            c::Channel {
                id: 4,
                destination: c::Domain::U128 {
                    min: 0,
                    max: u128::MAX,
                },
                payload: &typed,
                idempotency: c::Domain::Bytes,
            },
        ];
        let truth = [laws::Op::Literal(laws::Atom::Bool(true))];
        let declarations = d
            .laws
            .iter()
            .map(|law| laws::Law {
                program: laws::Program {
                    nodes: &truth,
                    root: 0,
                },
                ..*law
            })
            .collect::<Vec<_>>();
        let sf = [
            schema::Field {
                id: 0,
                name: b"Enum",
                type_id: 500,
            },
            schema::Field {
                id: u16::MAX,
                name: b"Sum",
                type_id: 501,
            },
        ];
        let pf = [
            schema::Field {
                id: 0,
                name: b"SignedMin",
                type_id: 400,
            },
            schema::Field {
                id: 1,
                name: b"SignedMax",
                type_id: 400,
            },
            schema::Field {
                id: 2,
                name: b"Unsigned",
                type_id: 200,
            },
            schema::Field {
                id: 3,
                name: b"Enum",
                type_id: 500,
            },
            schema::Field {
                id: 4,
                name: b"Sum",
                type_id: 501,
            },
            schema::Field {
                id: 5,
                name: b"Bytes",
                type_id: 401,
            },
            schema::Field {
                id: u16::MAX,
                name: b"Text",
                type_id: 402,
            },
        ];
        let vs = [
            schema::Variant {
                id: 0,
                name: b"Zero",
            },
            schema::Variant {
                id: u16::MAX,
                name: b"Max",
            },
        ];
        let defs = [
            schema::Definition {
                id: 100,
                name: b"State",
                kind: schema::Kind::Record(&sf),
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
                kind: schema::Kind::U128 {
                    min: 0,
                    max: u128::MAX,
                },
            },
            schema::Definition {
                id: 201,
                name: b"Payload",
                kind: schema::Kind::Record(&pf),
            },
            schema::Definition {
                id: 202,
                name: b"SecondDestination",
                kind: schema::Kind::U128 {
                    min: 0,
                    max: u128::MAX,
                },
            },
            schema::Definition {
                id: 203,
                name: b"SecondPayload",
                kind: schema::Kind::Record(&pf),
            },
            schema::Definition {
                id: 400,
                name: b"Signed",
                kind: schema::Kind::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
            },
            schema::Definition {
                id: 401,
                name: b"Bytes",
                kind: schema::Kind::Bytes { min: 0, max: 32 },
            },
            schema::Definition {
                id: 402,
                name: b"Text",
                kind: schema::Kind::Text { min: 0, max: 32 },
            },
            schema::Definition {
                id: 500,
                name: b"Enum",
                kind: schema::Kind::Enum(&vs),
            },
            schema::Definition {
                id: 501,
                name: b"Sum",
                kind: schema::Kind::Sum(&vs),
            },
        ];
        let description = schema::Description {
            profile: b"WidePublication",
            version: 1,
            root: 100,
            definitions: &defs,
        };
        let original = schema_fixture(&description);
        let mut framed = framing();
        framed.state.max_bytes = cap;
        let links = if swap_roots {
            [(3, 202, 203), (4, 200, 201)]
        } else {
            [(3, 200, 201), (4, 202, 203)]
        };
        let wide = c::Descriptor {
            state: c::Schema::Record(&state),
            command: d.command,
            context: d.context,
            program: ScalarProgram {
                inputs: &domains,
                outputs: &outputs,
                nodes: &nodes,
                roots: &roots,
            },
            bindings: &bindings,
            output_types: d.output_types,
            decision_output: d.decision_output,
            branches: &branches,
            reasons: d.reasons,
            channels: &channels,
            laws: &declarations,
            required: d.required,
            limits: d.limits,
        };
        let policy = authority::policy_bytes(&wide, &original, &framed, &links)
            .unwrap_or_else(|| panic!("wide policy serialization"));
        let limits = catalog::Limits {
            schema: schema::Limits {
                bytes: u64::MAX,
                types: u32::MAX,
                fields: u32::MAX,
                variants: u32::MAX,
            },
            contract_bytes: u64::MAX,
        };
        let catalog = catalog::bind_original(
            &original,
            &description,
            limits,
            &policy,
            &wide,
            &framed,
            &links,
        )
        .unwrap_or_else(|error| panic!("wide catalog: {error:?}"));
        run(&catalog);
    });
}

fn wide_originals(code: i128) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        envelope(
            100,
            record(vec![
                (0, Value::enumeration(500, 0)),
                (u16::MAX, Value::sum(501, u16::MAX, None)),
            ]),
        ),
        envelope(101, Value::signed(code)),
        envelope(102, record(vec![])),
    )
}

#[test]
fn publication_wide_original_values_ordered_channels_and_exact_caps() {
    let expected_post = envelope(
        100,
        record(vec![
            (0, Value::enumeration(500, u16::MAX)),
            (u16::MAX, Value::sum(501, 0, None)),
        ]),
    );
    let expected_payload = record(vec![
        (0, Value::signed(i128::MIN)),
        (1, Value::signed(i128::MAX)),
        (2, Value::unsigned(u128::MAX)),
        (3, Value::enumeration(500, 0)),
        (4, Value::sum(501, u16::MAX, None)),
        (
            5,
            Value::bytes(vec![0, 255]).unwrap_or_else(|error| panic!("value fixture: {error}")),
        ),
        (
            u16::MAX,
            Value::text_ascii("\0\x7f".into())
                .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
        ),
    ])
    .canonical_bytes()
    .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
    for cap in [expected_post.len() as u64, u64::MAX] {
        for swapped in [false, true] {
            wide_catalog(cap, swapped, |catalog| {
                let a = authority::bind(catalog)
                    .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
                for code in [0, 2] {
                    let values = wide_originals(code);
                    let authority::PublicationOutcome::Commit(p) = a.publish(raw(&values)) else {
                        panic!("wide publication: {:?}", a.publish(raw(&values)))
                    };
                    assert_eq!(p.poststate(), expected_post);
                    assert_eq!(p.effects().len(), 2);
                    assert_eq!(p.outbox().len(), 2);
                    let mut subject = Vec::new();
                    for n in [0x5a505532, 1, 1] {
                        w(&mut subject, n);
                    }
                    b(
                        &mut subject,
                        p.evaluation()
                            .subject()
                            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}")),
                    );
                    b(&mut subject, &expected_post);
                    for (lane, wanted) in [
                        (
                            p.effects(),
                            [(0, 3, 0, &b""[..]), (u32::MAX, 4, u128::MAX, &b"\xff"[..])],
                        ),
                        (
                            p.outbox(),
                            [
                                (1, 4, u128::MAX, &b"out"[..]),
                                (u32::MAX, 3, 0, &b"last"[..]),
                            ],
                        ),
                    ] {
                        w(&mut subject, 2);
                        for (actual, (ordinal, channel, destination, key)) in
                            lane.iter().zip(wanted)
                        {
                            let dr = if (channel == 3) ^ swapped { 200 } else { 202 };
                            let pr = dr + 1;
                            assert_eq!(
                                (
                                    actual.ordinal(),
                                    actual.channel(),
                                    actual.destination_root(),
                                    actual.payload_root()
                                ),
                                (ordinal, channel, dr, pr)
                            );
                            let db = Value::unsigned(destination)
                                .canonical_bytes()
                                .unwrap_or_else(|error| {
                                    panic!("unexpected test refusal: {error:?}")
                                });
                            let kb = Value::bytes(key.to_vec())
                                .unwrap_or_else(|error| panic!("value fixture: {error}"))
                                .canonical_bytes()
                                .unwrap_or_else(|error| {
                                    panic!("unexpected test refusal: {error:?}")
                                });
                            assert_eq!(actual.destination(), db);
                            assert_eq!(actual.payload(), expected_payload);
                            assert_eq!(actual.idempotency(), kb);
                            for n in [ordinal, channel, dr, pr] {
                                w(&mut subject, n as u128);
                            }
                            for bytes in [&db, &expected_payload, &kb] {
                                b(&mut subject, bytes);
                            }
                        }
                    }
                    assert_eq!(p.subject(), subject);
                    assert!(matches!(
                        a.replay_publication(raw(&values), p.subject()),
                        authority::PublicationOutcome::Commit(_)
                    ));
                    wide_catalog(cap, !swapped, |other| {
                        let other = authority::bind(other)
                            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
                        assert!(matches!(
                            other.replay_publication(raw(&values), p.subject()),
                            authority::PublicationOutcome::Refused {
                                error: authority::Refusal::ReplayMismatch,
                                ..
                            }
                        ));
                    });
                }
            });
        }
    }
    wide_catalog(expected_post.len() as u64 - 1, false, |catalog| {
        let a = authority::bind(catalog)
            .unwrap_or_else(|error| panic!("unexpected test refusal: {error:?}"));
        let values = wide_originals(0);
        assert!(matches!(
            a.publish(raw(&values)),
            authority::PublicationOutcome::Refused {
                error: authority::Refusal::Core(c::Failure::Frame(0, _)),
                ..
            }
        ));
    });
}
