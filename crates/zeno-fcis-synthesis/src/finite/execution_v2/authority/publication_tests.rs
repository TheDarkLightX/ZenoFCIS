//! Migrated History-wrapper checks against the surviving full publication route.
//! Private fixture identities remain non-authorizing outside this test descendant.
use super::super::super::{Resource, composition as c};
use super::super::tests::{
    generous,
    sealing::{fixture, framing, originals, raw, transition_oracle},
};
use super::super::{Evaluation, PublicationOutcome, Refusal};
use super::Authority;
use alloc::vec::Vec;

fn authority<'p>(d: &'p c::Descriptor<'p>, identity: &[u8]) -> Authority<'p> {
    Authority {
        core: c::bind(d).unwrap_or_else(|e| panic!("fixture: {e:?}")),
        framing: framing(),
        identity: identity.to_vec(),
        channel_roots: &[(3, 200, 201)],
    }
}
fn bytes<'a>(outcome: &'a PublicationOutcome<'_>) -> &'a [u8] {
    match outcome {
        PublicationOutcome::Commit(p) => p.subject(),
        PublicationOutcome::Reject(e) => e.subject().unwrap_or_else(|e| panic!("reject: {e:?}")),
        _ => panic!("actual decision required"),
    }
}
fn reports(a: &Evaluation<'_>, b: &Evaluation<'_>) {
    assert_eq!(a.usage(), b.usage());
    assert_eq!(a.ingress_usage(), b.ingress_usage());
    assert_eq!(a.decision_usage(), b.decision_usage());
    assert_eq!(a.reads(), b.reads());
    assert_eq!(a.decision_attempts(), b.decision_attempts());
    assert_eq!(a.diagnostics(), b.diagnostics());
    assert_eq!(a.law_reads(), b.law_reads());
    assert_eq!(a.raw().state, b.raw().state);
    assert_eq!(a.raw().command, b.raw().command);
    assert_eq!(a.raw().context, b.raw().context);
}
fn replace_first_bytes(value: &[u8], replacement: &[u8]) -> Vec<u8> {
    assert_eq!(value[51], 1);
    let n = usize::try_from(u128::from_be_bytes(
        value[52..68]
            .try_into()
            .unwrap_or_else(|e| panic!("word: {e:?}")),
    ))
    .unwrap_or_else(|e| panic!("length: {e:?}"));
    let mut result = value[..52].to_vec();
    result.extend_from_slice(&(replacement.len() as u128).to_be_bytes());
    result.extend_from_slice(replacement);
    result.extend_from_slice(&value[68 + n..]);
    result
}
#[test]
fn full_identity_refuses_missing_changed_and_retired_record_markers() {
    fixture(generous(), true, |d| {
        let a = authority(d, b"history-fixture-A");
        let inputs = originals(0, true);
        let result = a.evaluate(raw(&inputs));
        let subject = result
            .subject()
            .unwrap_or_else(|e| panic!("subject: {e:?}"));
        for identity in [
            &[][..],
            &a.identity()[..a.identity().len() - 1],
            b"zeno-fcis/scoped-history/1",
        ] {
            assert_eq!(
                a.replay(raw(&inputs), &replace_first_bytes(subject, identity))
                    .result()
                    .err(),
                Some(Refusal::ReplayMismatch)
            );
        }
        for i in 0..a.identity().len() {
            let mut identity = a.identity().to_vec();
            identity[i] ^= 1;
            assert_eq!(
                a.replay(raw(&inputs), &replace_first_bytes(subject, &identity))
                    .result()
                    .err(),
                Some(Refusal::ReplayMismatch)
            );
        }
        let b = authority(d, b"history-fixture-B");
        let p = a.publish(raw(&inputs));
        assert_ne!(bytes(&p), bytes(&b.publish(raw(&inputs))));
        assert!(matches!(
            b.replay_publication(raw(&inputs), bytes(&p)),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}
#[test]
fn every_full_replay_entry_retains_its_original_identity() {
    fixture(generous(), true, |d| {
        let a = authority(d, b"history-fixture-A");
        let b = authority(d, b"different source history");
        let inputs = originals(0, true);
        let e = a.evaluate(raw(&inputs));
        let p = a.publish(raw(&inputs));
        let g = a.genesis(&inputs.0);
        let gp = a.publish_genesis(&inputs.0);
        assert_eq!(
            b.replay(
                raw(&inputs),
                e.subject().unwrap_or_else(|e| panic!("subject: {e:?}"))
            )
            .result()
            .err(),
            Some(Refusal::ReplayMismatch)
        );
        assert!(matches!(
            b.replay_publication(raw(&inputs), bytes(&p)),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
        assert!(matches!(
            b.replay_genesis(
                &inputs.0,
                g.subject().unwrap_or_else(|e| panic!("genesis: {e:?}"))
            )
            .result(),
            Err(Refusal::ReplayMismatch)
        ));
        let PublicationOutcome::Commit(gp) = gp else {
            panic!("genesis capability")
        };
        assert!(matches!(
            b.replay_genesis_publication(&inputs.0, gp.subject()),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}
#[test]
fn publication_preserves_all_three_original_classes_and_reports() {
    fixture(generous(), true, |d| {
        let a = authority(d, b"history-fixture-A");
        for code in 0..=2 {
            let inputs = originals(code, true);
            let p = a.publish(raw(&inputs));
            let e = a.evaluate(raw(&inputs));
            reports(p.evaluation(), &e);
            assert_eq!(
                e.subject().unwrap_or_else(|e| panic!("subject: {e:?}")),
                transition_oracle(a.identity(), &inputs, code as u128)
            );
            assert_eq!(
                p.evaluation()
                    .result()
                    .unwrap_or_else(|e| panic!("candidate: {e:?}"))
                    .class(),
                e.result()
                    .unwrap_or_else(|e| panic!("candidate: {e:?}"))
                    .class()
            );
            let replay = a.replay_publication(raw(&inputs), bytes(&p));
            reports(replay.evaluation(), p.evaluation());
            assert_eq!(bytes(&replay), bytes(&p));
            match (&p, &replay) {
                (PublicationOutcome::Commit(x), PublicationOutcome::Commit(y)) => {
                    assert_eq!(x.identity(), a.identity());
                    assert_eq!(x.poststate(), y.poststate());
                    assert_eq!(x.effects().len(), y.effects().len());
                    assert_eq!(x.outbox().len(), y.outbox().len());
                    for (left, right) in x
                        .effects()
                        .iter()
                        .chain(x.outbox())
                        .zip(y.effects().iter().chain(y.outbox()))
                    {
                        assert_eq!(left.ordinal(), right.ordinal());
                        assert_eq!(left.channel(), right.channel());
                        assert_eq!(left.destination_root(), right.destination_root());
                        assert_eq!(left.payload_root(), right.payload_root());
                        assert_eq!(left.destination(), right.destination());
                        assert_eq!(left.payload(), right.payload());
                        assert_eq!(left.idempotency(), right.idempotency());
                    }
                }
                (PublicationOutcome::Reject(_), PublicationOutcome::Reject(_)) => {}
                _ => panic!("classification differs"),
            }
        }
    });
}
#[test]
fn publication_replay_checks_every_byte_and_original_input_without_losing_work() {
    fixture(generous(), true, |d| {
        let a = authority(d, b"history-fixture-A");
        for code in 0..=2 {
            let inputs = originals(code, true);
            let p = a.publish(raw(&inputs));
            let r = bytes(&p);
            for i in 0..r.len() {
                let mut changed = r.to_vec();
                changed[i] ^= 1;
                let result = a.replay_publication(raw(&inputs), &changed);
                assert!(
                    matches!(
                        result,
                        PublicationOutcome::Refused {
                            error: Refusal::ReplayMismatch,
                            ..
                        }
                    ),
                    "byte {i}"
                );
                reports(result.evaluation(), p.evaluation());
            }
            for bad in [&[][..], &r[..r.len() - 1]] {
                assert!(matches!(
                    a.replay_publication(raw(&inputs), bad),
                    PublicationOutcome::Refused {
                        error: Refusal::ReplayMismatch,
                        ..
                    }
                ));
            }
            let changed = originals((code + 1) % 3, true);
            assert!(matches!(
                a.replay_publication(raw(&changed), r),
                PublicationOutcome::Refused {
                    error: Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
    });
}
#[test]
fn original_frame_law_and_budget_refusals_retain_their_reports() {
    for (limits, conforms) in [
        (generous(), true),
        (generous().with_limit(Resource::Byte, 160), true),
        (generous(), false),
    ] {
        fixture(limits, conforms, |d| {
            let a = authority(d, b"history-fixture-A");
            let mut inputs = originals(0, true);
            if conforms && d.limits.limit(Resource::Byte) == u64::MAX {
                inputs.1[0] ^= 1;
            }
            let p = a.publish(raw(&inputs));
            let e = a.evaluate(raw(&inputs));
            reports(p.evaluation(), &e);
            let PublicationOutcome::Refused { evaluation, error } = p else {
                panic!("original refusal required")
            };
            assert_eq!(Some(error), e.result().err());
            assert_eq!(evaluation.result().err(), e.result().err());
            assert!(evaluation.subject().is_err());
        });
    }
}
#[test]
fn genuine_genesis_retains_initial_laws_identity_and_every_byte() {
    fixture(generous(), true, |d| {
        let a = authority(d, b"history-fixture-A");
        let inputs = originals(0, true);
        let full = a.publish_genesis(&inputs.0);
        let e = a.genesis(&inputs.0);
        let PublicationOutcome::Commit(p) = full else {
            panic!("genuine genesis")
        };
        assert_eq!(p.identity(), a.identity());
        assert_eq!(p.poststate(), inputs.0);
        assert_eq!(p.evaluation().subject(), e.subject());
        assert_eq!(p.evaluation().usage(), e.usage());
        assert_eq!(p.evaluation().diagnostics(), e.diagnostics());
        assert_eq!(p.evaluation().law_reads(), e.law_reads());
        assert!(matches!(
            a.replay_genesis_publication(&inputs.0, p.subject()),
            PublicationOutcome::Commit(_)
        ));
        for i in 0..p.subject().len() {
            let mut changed = p.subject().to_vec();
            changed[i] ^= 1;
            assert!(matches!(
                a.replay_genesis_publication(&inputs.0, &changed),
                PublicationOutcome::Refused {
                    error: Refusal::ReplayMismatch,
                    ..
                }
            ));
        }
        assert!(matches!(
            a.replay_publication(raw(&inputs), p.subject()),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
        let bad = originals(0, false);
        let refusal = a.publish_genesis(&bad.0);
        assert!(matches!(
            refusal,
            PublicationOutcome::Refused {
                error: Refusal::Core(_),
                ..
            }
        ));
        assert_eq!(refusal.evaluation().usage(), a.genesis(&bad.0).usage());
    });
}
