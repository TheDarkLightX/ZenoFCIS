//! Public checked-catalog construction and independent complete wire comparisons.
//! The expected policy, outcomes and source bytes never call production encoders.
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

fn policy_oracle(
    schema: &[u8],
    links: &[(u32, u32, u32)],
    byte_limit: u64,
    conformance: bool,
) -> Vec<u8> {
    fn words(out: &mut Vec<u8>, values: &[u128]) {
        for &v in values {
            w(out, v);
        }
    }
    let mut framed = Vec::new();
    words(&mut framed, &[0x5a465232, 1]);
    for root in [100, 101, 102] {
        w(&mut framed, root);
        b(&mut framed, &[23; 32]);
        w(&mut framed, 512);
    }
    let mut out = Vec::new();
    words(&mut out, &[0x5a504f32, 1]);
    b(&mut out, schema);
    b(&mut out, &framed);
    w(&mut out, links.len() as u128);
    for &(id, destination, payload) in links {
        words(
            &mut out,
            &[id as u128, destination as u128, payload as u128],
        );
    }
    // State record Bool(0), command I128[0,2], empty context.
    words(&mut out, &[1, 1, 0, 1, 0, 0, 0, 2, 1, 0]);
    // Complete scalar graph, its input ABI, output type and branch selector.
    words(
        &mut out,
        &[
            2, 0, 1, 0, 2, 1, 1, 0, 2, 1, 0, 1, 1, 0, 2, 0, 1, 0, 1, 0, 1, 0, 0, 2, 0,
        ],
    );
    w(&mut out, 3);
    for code in 0..=2 {
        words(&mut out, &[code, code]);
        if code == 0 {
            w(&mut out, 0);
        } else {
            words(&mut out, &[1, if code == 1 { 7 } else { 9 }]);
        }
        if code == 1 {
            words(&mut out, &[0, 0, 0]);
            continue;
        }
        words(&mut out, &[1, 0, 2, 0, 0, 0]);
        for (ordinal, destination, key) in [
            (5, &b"effect"[..], &b"ekey"[..]),
            (9, &b"outbox"[..], &b"okey"[..]),
        ] {
            words(&mut out, &[1, ordinal, 3, 2, 0, 1, 2, 6]);
            b(&mut out, destination);
            words(&mut out, &[1, 0, 3, 1, 2, 5]);
            b(&mut out, key);
        }
    }
    words(&mut out, &[2, 7, 1, 9, 2, 1, 3, 6, 1, 0, 1, 0, 2, 5]);
    w(&mut out, 5);
    for (id, kind, scope, genesis) in [(10, 0, 4, 1), (20, 6, 2, 0), (30, 7, 3, 0)] {
        words(&mut out, &[id, kind, scope, genesis, 1, 0, 0, 1, 0]);
    }
    if conformance {
        words(
            &mut out,
            &[
                40, 8, 0, 0, 8, 1, 1, 0, 1, 2, 8, 0, 1, 7, 0, 1, 10, 2, 10, 3, 9, 4, 5, 10, 6, 7,
            ],
        );
    } else {
        words(&mut out, &[40, 8, 0, 0, 1, 0, 0, 0, 0]);
    }
    words(
        &mut out,
        &[50, 9, 0, 1, 1, 1, 9, 0, 0, 5, 10, 20, 30, 40, 50],
    );
    for i in 0..8 {
        w(
            &mut out,
            if i == 4 {
                byte_limit as u128
            } else {
                u64::MAX as u128
            },
        );
    }
    out
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
            let policy = policy_oracle(&original, &links, byte_limit, conformance);
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
fn source_oracle() -> Vec<u8> {
    zeno_fcis_synthesis::finite::v2_authority::EVALUATOR.to_vec()
}
fn identity_oracle(policy: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    w(&mut out, 0x5a494432);
    w(&mut out, 1);
    b(&mut out, policy);
    b(&mut out, &source_oracle());
    out
}
fn genesis_oracle(identity: &[u8], state: &[u8]) -> Vec<u8> {
    let mut artifact = Vec::new();
    for v in [0x5a4f5532, 1, 0] {
        w(&mut artifact, v);
    }
    for v in [0x5a434432, 1, 0, 0] {
        w(&mut artifact, v);
    }
    for v in [1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0] {
        w(&mut artifact, v);
    }
    w(&mut artifact, 1);
    counters(&mut artifact, [1, 0, 0, 0, 56, 0, 0, 0]);
    w(&mut artifact, 0);
    counters(&mut artifact, [2, 0, 0, 0, 56, 0, 0, 2]);
    read_oracle(&mut artifact, true);
    w(&mut artifact, 0);
    w(&mut artifact, 5);
    for (id, verdict) in [(10, 1), (20, 0), (30, 0), (40, 0), (50, 1)] {
        w(&mut artifact, id);
        w(&mut artifact, verdict);
    }
    for v in [1, 50, 0, 9, 0, 1] {
        w(&mut artifact, v);
    }
    let mut out = Vec::new();
    for v in [0x5a525032, 1, 0] {
        w(&mut out, v);
    }
    for bytes in [identity, state, &[], &[], &artifact] {
        b(&mut out, bytes);
    }
    out
}

#[test]
fn public_constructor_binds_the_complete_actual_source_and_checked_policy() {
    with_catalog(b"Authority", u64::MAX, true, |catalog, policy| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        assert!(core::ptr::eq(bound.descriptor(), catalog.descriptor()));
        assert!(
            bound.identity() == identity_oracle(policy),
            "exact complete source/build/policy identity differs"
        );
        assert!(
            authority::bind(catalog)
                .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
                .identity()
                == bound.identity()
        );
    });
}

#[test]
fn public_invocation_all_classes_match_independent_complete_subjects() {
    with_catalog(b"Authority", u64::MAX, true, |catalog, policy| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let identity = identity_oracle(policy);
        for (code, class, reason) in [
            (0, c::Class::Accept, None),
            (1, c::Class::Reject, Some(7)),
            (2, c::Class::CommittedFailure, Some(9)),
        ] {
            let values = originals(code, true);
            let evaluated = bound.evaluate(raw(&values));
            let candidate = evaluated
                .result()
                .unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
            assert_eq!(candidate.class(), class);
            assert_eq!(candidate.reason(), reason);
            assert_eq!(
                candidate.pre(),
                &[c::Field {
                    id: 0,
                    value: c::Atom::Bool(true)
                }]
            );
            if code == 1 {
                assert!(candidate.post().is_empty());
                assert!(candidate.patch().is_empty());
                assert!(candidate.effects().is_empty());
                assert!(candidate.outbox().is_empty());
            } else {
                assert_eq!(
                    candidate.post(),
                    &[c::Field {
                        id: 0,
                        value: c::Atom::Bool(false)
                    }]
                );
                assert_eq!(candidate.patch().len(), 1);
                assert_eq!(candidate.effects()[0].ordinal, 5);
                assert_eq!(candidate.outbox()[0].ordinal, 9);
            }
            let expected = transition_oracle(&identity, &values, code as u128);
            assert!(
                evaluated
                    .subject()
                    .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
                    == expected,
                "complete independent transition subject differs for {code}"
            );
            let replayed = bound.replay(raw(&values), &expected);
            assert!(replayed.result().is_ok());
            assert!(
                replayed
                    .subject()
                    .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
                    == expected
            );
            assert_eq!(replayed.usage(), evaluated.usage());
            assert_eq!(replayed.reads(), evaluated.reads());
            assert_eq!(replayed.diagnostics(), evaluated.diagnostics());
        }
    });
}

#[test]
fn public_genesis_replays_genuine_initial_state_and_refuses_failed_laws() {
    with_catalog(b"Authority", u64::MAX, true, |catalog, policy| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let values = originals(0, true);
        let expected = genesis_oracle(&identity_oracle(policy), &values.0);
        let evaluated = bound.genesis(&values.0);
        assert!(matches!(evaluated.result(), Ok(c) if c.class()==c::Class::Accept));
        assert!(
            evaluated
                .subject()
                .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
                == expected,
            "complete independent genesis subject differs"
        );
        assert!(bound.replay_genesis(&values.0, &expected).result().is_ok());
        assert!(bound.replay(raw(&values), &expected).result().is_err());
        assert!(
            bound
                .replay_genesis(
                    &values.0,
                    bound
                        .evaluate(raw(&values))
                        .subject()
                        .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
                )
                .result()
                .is_err()
        );
        let wrong = originals(0, false);
        let refused = bound.replay_genesis(&wrong.0, &expected);
        assert!(matches!(
            refused.result(),
            Err(authority::Refusal::Core(c::Failure::Law(
                laws::Failure::Violated
            )))
        ));
        assert!(refused.subject().is_err());
        assert_eq!(refused.usage().used(Resource::Byte), 56);
        assert_eq!(refused.usage().used(Resource::Read), 2);
        assert_eq!(
            refused
                .diagnostics()
                .last()
                .unwrap_or_else(|| panic!("expected law diagnostic is absent"))
                .id,
            50
        );
    });
}

#[test]
fn public_replay_refuses_changed_inputs_policy_source_and_artifact() {
    let mut persisted = Vec::new();
    with_catalog(b"Authority", u64::MAX, true, |catalog, _| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let values = originals(0, true);
        let evaluated = bound.evaluate(raw(&values));
        persisted = evaluated
            .subject()
            .unwrap_or_else(|error| panic!("authority fixture: {error:?}"))
            .to_vec();
        // Magic/version/phase, policy/source identity, original inputs and trailing artifact.
        for index in [0, 17, 50, 100, persisted.len() / 2, persisted.len() - 1] {
            let mut altered = persisted.clone();
            altered[index] ^= 1;
            let refused = bound.replay(raw(&values), &altered);
            assert_eq!(
                refused.result().map_or_else(
                    |error| error,
                    |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
                ),
                authority::Refusal::ReplayMismatch
            );
            assert!(refused.subject().is_err());
            assert_eq!(refused.usage(), evaluated.usage());
        }
        for expected in [&[][..], &persisted[..persisted.len() - 1]] {
            assert_eq!(
                bound.replay(raw(&values), expected).result().map_or_else(
                    |error| error,
                    |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
                ),
                authority::Refusal::ReplayMismatch
            );
        }
        let changed = originals(1, true);
        assert_eq!(
            bound
                .replay(raw(&changed), &persisted)
                .result()
                .map_or_else(
                    |error| error,
                    |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
                ),
            authority::Refusal::ReplayMismatch
        );
        let changed = originals(0, false);
        assert_eq!(
            bound
                .replay(raw(&changed), &persisted)
                .result()
                .map_or_else(
                    |error| error,
                    |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
                ),
            authority::Refusal::ReplayMismatch
        );
        let mut bad = values.clone();
        bad.2[12] ^= 1;
        assert!(matches!(
            bound.replay(raw(&bad), &persisted).result(),
            Err(authority::Refusal::Core(c::Failure::Frame(2, _)))
        ));
    });
    // A newly checked original schema name changes policy identity even when decisions coincide.
    with_catalog(b"Renamed", u64::MAX, true, |catalog, _| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let values = originals(0, true);
        assert_eq!(
            bound.replay(raw(&values), &persisted).result().map_or_else(
                |error| error,
                |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
            ),
            authority::Refusal::ReplayMismatch
        );
    });
}

#[test]
fn public_refusals_retain_actual_work_and_never_expose_a_candidate() {
    with_catalog(b"Authority", 160, true, |catalog, _| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let values = originals(0, true);
        let refused = bound.evaluate(raw(&values));
        assert!(refused.result().is_err());
        assert!(refused.subject().is_err());
        assert_eq!(refused.usage().used(Resource::Byte), 152);
        assert_eq!(refused.reads().len(), 1);
        assert!(refused.decision_usage().is_none());
    });
    with_catalog(b"Authority", u64::MAX, false, |catalog, _| {
        let bound =
            authority::bind(catalog).unwrap_or_else(|error| panic!("authority fixture: {error:?}"));
        let values = originals(0, true);
        let refused = bound.evaluate(raw(&values));
        assert_eq!(
            refused.result().map_or_else(
                |error| error,
                |value| panic!("adversarial fixture unexpectedly succeeded: {value:?}")
            ),
            authority::Refusal::Core(c::Failure::Law(laws::Failure::Violated))
        );
        assert!(refused.subject().is_err());
        assert_eq!(refused.decision_attempts().len(), 4);
        assert!(refused.decision_usage().is_some());
        assert_eq!(
            refused
                .diagnostics()
                .last()
                .unwrap_or_else(|| panic!("expected law diagnostic is absent"))
                .id,
            40
        );
    });
}

#[test]
fn public_checked_authority_retains_complete_original_decisions_and_identity() {
    with_catalog(b"Authority", u64::MAX, true, |catalog, policy| {
        let bound = authority::bind(catalog).unwrap_or_else(|error| panic!("catalog: {error:?}"));
        assert_eq!(bound.identity(), identity_oracle(policy));
        for code in 0..=2 {
            let inputs = originals(code, true);
            let outcome = bound.publish(raw(&inputs));
            let e = outcome.evaluation();
            assert_eq!(
                e.subject()
                    .unwrap_or_else(|error| panic!("subject: {error:?}")),
                transition_oracle(bound.identity(), &inputs, code as u128)
            );
            let full = bound.evaluate(raw(&inputs));
            assert_eq!(
                e.result()
                    .unwrap_or_else(|error| panic!("candidate: {error:?}"))
                    .class(),
                full.result()
                    .unwrap_or_else(|error| panic!("candidate: {error:?}"))
                    .class()
            );
            assert_eq!(e.usage(), full.usage());
            assert_eq!(e.reads(), full.reads());
            assert_eq!(e.decision_attempts(), full.decision_attempts());
            assert_eq!(e.diagnostics(), full.diagnostics());
            assert_eq!(e.law_reads(), full.law_reads());
            let record = match &outcome {
                authority::PublicationOutcome::Commit(p) => {
                    assert_eq!(p.identity(), bound.identity());
                    assert_eq!(p.effects()[0].ordinal(), 5);
                    assert_eq!(p.outbox()[0].ordinal(), 9);
                    p.subject()
                }
                authority::PublicationOutcome::Reject(e) => e
                    .subject()
                    .unwrap_or_else(|error| panic!("rejection: {error:?}")),
                _ => panic!("required original decision"),
            };
            assert_eq!(
                bound
                    .replay_publication(raw(&inputs), record)
                    .evaluation()
                    .usage(),
                e.usage()
            );
            for identity in [&[][..], b"zeno-fcis/scoped-history/1"] {
                assert_eq!(
                    bound
                        .replay(
                            raw(&inputs),
                            &transition_oracle(identity, &inputs, code as u128)
                        )
                        .result()
                        .err(),
                    Some(authority::Refusal::ReplayMismatch)
                );
            }
            for index in [
                0,
                17,
                50,
                bound.identity().len() / 2,
                bound.identity().len() - 1,
            ] {
                let mut changed = bound.identity().to_vec();
                changed[index] ^= 1;
                assert_eq!(
                    bound
                        .replay(
                            raw(&inputs),
                            &transition_oracle(&changed, &inputs, code as u128)
                        )
                        .result()
                        .err(),
                    Some(authority::Refusal::ReplayMismatch)
                );
            }
        }
        let inputs = originals(0, true);
        let authority::PublicationOutcome::Commit(p) = bound.publish_genesis(&inputs.0) else {
            panic!("genuine genesis required")
        };
        assert_eq!(p.identity(), bound.identity());
        assert_eq!(p.poststate(), inputs.0);
        assert!(matches!(p.evaluation().result(),Ok(c) if c.class()==c::Class::Accept));
        assert_eq!(
            p.evaluation()
                .subject()
                .unwrap_or_else(|error| panic!("genesis: {error:?}")),
            genesis_oracle(bound.identity(), &inputs.0)
        );
        assert!(matches!(
            bound.replay_genesis_publication(&inputs.0, p.subject()),
            authority::PublicationOutcome::Commit(_)
        ));
        assert!(matches!(
            bound
                .replay_genesis(
                    &inputs.0,
                    &genesis_oracle(b"zeno-fcis/scoped-history/1", &inputs.0)
                )
                .result(),
            Err(authority::Refusal::ReplayMismatch)
        ));
    });
}
