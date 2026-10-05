//! Independent complete wire oracles from actual framed producer outcomes.
use super::super::super::super::evaluation::{Domain as ScalarDomain, Op};
use super::super::super::{
    InputField, InputLeaf, Limits, Resource, ScalarProgram, composition as c, laws,
};
use super::super::{Evaluation, Refusal, canonical, outcome};
use super::{b, generous, int, text, w};
use alloc::{vec, vec::Vec};

pub(in super::super) fn fixture(
    limits: Limits,
    conformance: bool,
    run: impl FnOnce(&mut c::Descriptor<'_>),
) {
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
pub(in super::super) fn framing() -> c::Framing {
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
pub(in super::super) fn originals(code: i128, initial: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let state = [9, 0, 0, 0, 1, 0, 0, if initial { 2 } else { 1 }];
    let mut command = vec![4];
    command.extend_from_slice(&code.to_be_bytes());
    (
        frame(100, &state),
        frame(101, &command),
        frame(102, &[9, 0, 0, 0, 0]),
    )
}
pub(in super::super) fn raw(values: &(Vec<u8>, Vec<u8>, Vec<u8>)) -> c::Raw<'_> {
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
pub(in super::super) fn transition_oracle(
    identity: &[u8],
    values: &(Vec<u8>, Vec<u8>, Vec<u8>),
    code: u128,
) -> Vec<u8> {
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
#[test]
fn exact_framing_metadata_includes_all_three_roots_schema_ids_and_limits() {
    let config = framing();
    let actual=super::super::bound::framing_bytes(&config).unwrap_or_else(|| panic!("missing fixture value in exact_framing_metadata_includes_all_three_roots_schema_ids_and_limits"));
    let mut expected = Vec::new();
    w(&mut expected, 0x5a465232);
    w(&mut expected, 1);
    for value in [config.state, config.command, config.context] {
        w(&mut expected, value.root as u128);
        b(&mut expected, &value.schema);
        w(&mut expected, value.max_bytes as u128);
    }
    assert_eq!(actual, expected);
    for index in 0..3 {
        for component in 0..3 {
            let mut changed = config;
            let selected = match index {
                0 => &mut changed.state,
                1 => &mut changed.command,
                _ => &mut changed.context,
            };
            match component {
                0 => selected.root += 1,
                1 => selected.schema[31] ^= 1,
                _ => selected.max_bytes -= 1,
            };
            assert_ne!(actual,super::super::bound::framing_bytes(&changed).unwrap_or_else(|| panic!("missing fixture value in exact_framing_metadata_includes_all_three_roots_schema_ids_and_limits")));
        }
    }
}
#[test]
fn full_subject_comparison_preserves_real_reports_but_hides_candidate_on_mismatch() {
    fixture(generous(), true, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("full_subject_comparison_preserves_real_reports_but_hides_candidate_on_mismatch fixture failed: {error:?}"));
        let config = framing();
        let values = originals(0, true);
        let completed = core.frame(c::Kind::Transition, raw(&values), &config);
        let actual=outcome::seal(&completed,b"unit-seal-identity").unwrap_or_else(|error| panic!("full_subject_comparison_preserves_real_reports_but_hides_candidate_on_mismatch fixture failed: {error:?}"));
        let mut changed = actual.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        let refused = super::super::bound::compare(Ok(actual.clone()), &changed);
        assert_eq!(refused, Err(Refusal::ReplayMismatch));
        let evaluation = Evaluation {
            outcome: completed,
            subject: refused,
        };
        assert_eq!(evaluation.result().err().unwrap_or_else(|| panic!("expected refusal in full_subject_comparison_preserves_real_reports_but_hides_candidate_on_mismatch")),Refusal::ReplayMismatch);
        assert_eq!(evaluation.subject(), Err(Refusal::ReplayMismatch));
        assert_eq!(evaluation.usage().used(Resource::Read), 3);
        assert_eq!(evaluation.diagnostics().len(), 5);
        assert_eq!(evaluation.law_reads().len(), 1);
        assert_eq!(evaluation.decision_attempts().len(), 4);
        assert_eq!(
            super::super::bound::compare(Ok(actual.clone()), &actual),
            Ok(actual)
        );
        assert_eq!(
            super::super::bound::compare(Err(Refusal::Core(c::Failure::Metadata)), &changed),
            Err(Refusal::Core(c::Failure::Metadata))
        );
    });
}
#[test]
fn actual_framed_three_class_complete_subject_matches_independent_oracle() {
    fixture(generous(), true, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("actual_framed_three_class_complete_subject_matches_independent_oracle fixture failed: {error:?}"));
        let config = framing();
        for code in 0..=2 {
            let values = originals(code, true);
            let completed = core.frame(c::Kind::Transition, raw(&values), &config);
            assert!(completed.result().is_ok());
            let subject=outcome::seal(&completed,b"unit-seal-identity").unwrap_or_else(|error| panic!("actual_framed_three_class_complete_subject_matches_independent_oracle fixture failed: {error:?}"));
            assert_eq!(
                subject,
                transition_oracle(b"unit-seal-identity", &values, code as u128)
            );
            let evaluation = Evaluation {
                outcome: completed,
                subject: Ok(subject.clone()),
            };
            assert!(evaluation.result().is_ok());
            assert_eq!(evaluation.subject().unwrap_or_else(|error| panic!("actual_framed_three_class_complete_subject_matches_independent_oracle fixture failed: {error:?}")),subject);
            assert_eq!(evaluation.raw().state, values.0);
            assert_eq!(evaluation.raw().command, values.1);
            assert_eq!(evaluation.raw().context, values.2);
            assert!(!canonical::exact(&subject, &subject[..subject.len() - 1]));
            for i in 0..subject.len() {
                let mut changed = subject.clone();
                changed[i] ^= 1;
                assert!(!canonical::exact(&subject, &changed));
            }
        }
    });
}
#[test]
fn actual_genesis_subject_has_only_real_initial_phase_and_reads() {
    fixture(generous(), true, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("actual_genesis_subject_has_only_real_initial_phase_and_reads fixture failed: {error:?}"));
        let config = framing();
        let values = originals(0, true);
        let completed = core.frame(
            c::Kind::Genesis,
            c::Raw {
                state: &values.0,
                command: &[],
                context: &[],
            },
            &config,
        );
        assert!(matches!(completed.result(),Ok(c) if c.class()==c::Class::Accept));
        let subject=outcome::seal(&completed,b"unit-seal-identity").unwrap_or_else(|error| panic!("actual_genesis_subject_has_only_real_initial_phase_and_reads fixture failed: {error:?}"));
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
        let mut expected = Vec::new();
        for v in [0x5a525032, 1, 0] {
            w(&mut expected, v);
        }
        for bytes in [&b"unit-seal-identity"[..], &values.0, &[], &[], &artifact] {
            b(&mut expected, bytes);
        }
        assert_eq!(subject, expected);
        let evaluation = Evaluation {
            outcome: completed,
            subject: Ok(subject),
        };
        assert!(
            matches!(evaluation.result(),Ok(c) if c.class()==c::Class::Accept && c.patch().is_empty() && c.effects().is_empty() && c.outbox().is_empty() && c.pre()==c.post())
        );
        assert_eq!(evaluation.raw().state, values.0);
        let false_initial = originals(0, false);
        let failed = core.frame(
            c::Kind::Genesis,
            c::Raw {
                state: &false_initial.0,
                command: &[],
                context: &[],
            },
            &config,
        );
        assert!(matches!(
            failed.result(),
            Err(c::Failure::Law(laws::Failure::Violated))
        ));
        let seal = outcome::seal(&failed, b"unit-seal-identity");
        assert_eq!(
            seal,
            Err(Refusal::Core(c::Failure::Law(laws::Failure::Violated)))
        );
        assert_eq!(failed.diagnostics().last().unwrap_or_else(|| panic!("missing fixture value in actual_genesis_subject_has_only_real_initial_phase_and_reads")).id,50);
        assert_eq!(failed.law_reads().len(), 1);
    });
}
#[test]
fn actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate() {
    fixture(generous(), true, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate fixture failed: {error:?}"));
        let config = framing();
        let mut values = originals(0, true);
        values.1[0] ^= 1;
        let failed = core.frame(c::Kind::Transition, raw(&values), &config);
        assert!(matches!(
            failed.result(),
            Err(c::Failure::Frame(1, c::FrameFailure::Envelope(_)))
        ));
        assert_eq!(failed.usage().used(Resource::Byte), 96);
        assert!(failed.reads().is_empty());
        assert!(failed.decision_attempts().is_empty());
        assert!(failed.diagnostics().is_empty());
        assert!(failed.ingress_usage().is_none());
        let seal = outcome::seal(&failed, b"unit-seal-identity");
        let evaluation = Evaluation {
            outcome: failed,
            subject: seal,
        };
        assert!(evaluation.result().is_err());
        assert!(evaluation.subject().is_err());
        assert_eq!(evaluation.raw().command, values.1);
    });
    fixture(generous().with_limit(Resource::Byte, 160), true, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate fixture failed: {error:?}"));
        let config = framing();
        let values = originals(0, true);
        let failed = core.frame(c::Kind::Transition, raw(&values), &config);
        assert!(failed.result().is_err());
        assert_eq!(failed.usage().used(Resource::Byte), 152);
        assert_eq!(failed.reads().len(), 1);
        assert!(failed.ingress_usage().is_none());
        assert!(failed.decision_usage().is_none());
    });
    fixture(generous(), false, |d| {
        let core=c::bind(d).unwrap_or_else(|error| panic!("actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate fixture failed: {error:?}"));
        let config = framing();
        let values = originals(0, true);
        let failed = core.frame(c::Kind::Transition, raw(&values), &config);
        assert_eq!(failed.result().err().unwrap_or_else(|| panic!("expected refusal in actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate")),c::Failure::Law(laws::Failure::Violated));
        assert!(failed.decision_usage().is_some());
        assert_eq!(failed.decision_attempts().len(), 4);
        assert_eq!(failed.diagnostics().last().unwrap_or_else(|| panic!("missing fixture value in actual_early_frame_and_late_law_refusals_preserve_reports_and_expose_no_candidate")).id,40);
        let seal = outcome::seal(&failed, b"unit-seal-identity");
        let evaluation = Evaluation {
            outcome: failed,
            subject: seal,
        };
        assert!(evaluation.result().is_err());
        assert!(evaluation.subject().is_err());
        assert_eq!(evaluation.decision_attempts().len(), 4);
    });
}

// This authored fixture oracle writes the wire directly. It never calls a
// production metadata projector, descriptor resolver, policy encoder or getter.
fn policy_oracle(schema: &[u8], links: &[(u32, u32, u32)]) -> Vec<u8> {
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
    words(
        &mut out,
        &[
            40, 8, 0, 0, 8, 1, 1, 0, 1, 2, 8, 0, 1, 7, 0, 1, 10, 2, 10, 3, 9, 4, 5, 10, 6, 7,
        ],
    );
    words(
        &mut out,
        &[50, 9, 0, 1, 1, 1, 9, 0, 0, 5, 10, 20, 30, 40, 50],
    );
    for _ in 0..8 {
        w(&mut out, u64::MAX as u128);
    }
    out
}

#[test]
fn exact_policy_matches_independent_complete_fixture_and_binds_every_link() {
    fixture(generous(), true, |d| {
        let config = framing();
        let links = [(3, 200, 201), (u32::MAX, 0, u32::MAX)];
        let schema = b"original schema bytes";
        for current in [&[][..], &links[..]] {
            let actual=super::super::policy_bytes(d,schema,&config,current).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link"));
            assert_eq!(actual, policy_oracle(schema, current));
        }
        let actual=super::super::policy_bytes(d,schema,&config,&links).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link"));
        for index in 0..links.len() {
            for component in 0..3 {
                let mut changed = links;
                let link = &mut changed[index];
                match component {
                    0 => link.0 ^= 1,
                    1 => link.1 ^= 1,
                    _ => link.2 ^= 1,
                };
                assert_ne!(actual,super::super::policy_bytes(d,schema,&config,&changed).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link")));
            }
        }
        let reversed = [links[1], links[0]];
        assert_ne!(actual,super::super::policy_bytes(d,schema,&config,&reversed).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link")));
        for index in 0..schema.len() {
            let mut changed = schema.to_vec();
            changed[index] ^= 1;
            assert_ne!(actual,super::super::policy_bytes(d,&changed,&config,&links).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link")));
        }
        d.decision_output = 1;
        assert_ne!(actual,super::super::policy_bytes(d,schema,&config,&links).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link")));
        d.decision_output = 0;
        d.limits = d.limits.with_limit(Resource::Byte, 511);
        assert_ne!(actual,super::super::policy_bytes(d,schema,&config,&links).unwrap_or_else(|| panic!("missing fixture value in exact_policy_matches_independent_complete_fixture_and_binds_every_link")));
    });
}
