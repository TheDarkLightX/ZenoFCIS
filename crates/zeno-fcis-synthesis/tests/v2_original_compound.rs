//! Retained original 12-type schema through catalog, Authority, laws and replay.
use std::vec;
#[path = "fixtures/g3-original-compound/fixture.rs"]
mod fixture;
use fixture::*;
use zeno_fcis_synthesis::finite::{v2_authority as a, v2_composition as c, v2_laws as l};

#[test]
fn original_compounds_patch_carry_forward_and_replay_without_reencoding_loss() {
    with_authority(|authority| {
        let initial = state(5, Some(12));
        assert_eq!(
            commit(authority.publish_genesis(&initial)).poststate(),
            initial
        );
        let invalid_initial = state(6, Some(12));
        assert!(matches!(
            authority.publish_genesis(&invalid_initial),
            a::PublicationOutcome::Refused { .. }
        ));
        let context = wire(5, &[2]);
        let mut pre = initial;
        for event_value in [None, Some(0), Some(1_000_000)] {
            let command = wire(9, &event(event_value));
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let publication = commit(authority.publish(raw));
            let expected = state(6, event_value);
            assert_eq!(publication.poststate(), expected);
            let candidate = ok(publication.evaluation().outcome().result());
            assert_eq!(candidate.pre().len(), 11);
            assert_eq!(candidate.post().len(), 11);
            for id in 2..=11 {
                if id != 9 {
                    assert_eq!(candidate.pre()[id - 1], candidate.post()[id - 1]);
                }
            }
            assert!(
                candidate
                    .patch()
                    .iter()
                    .all(|p| p.field == 1 || p.field == 9)
            );
            assert!(
                publication
                    .evaluation()
                    .outcome()
                    .diagnostics()
                    .iter()
                    .any(|d| d.id == 1 && d.verdict == l::Verdict::Satisfied)
            );
            let subject = publication.subject();
            assert_eq!(
                commit(authority.replay_publication(raw, subject)).subject(),
                subject
            );
            let mut altered = subject.to_vec();
            altered[0] ^= 1;
            assert!(matches!(
                authority.replay_publication(raw, &altered),
                a::PublicationOutcome::Refused {
                    error: a::Refusal::ReplayMismatch,
                    ..
                }
            ));
            pre = expected;
        }
    });
}

#[test]
fn malformed_unprojected_compounds_never_acquire_publication() {
    with_authority(|authority| {
        let context = wire(5, &[2]);
        let command = wire(9, &event(None));
        // Every field is parsed even though only amount is a graph input.
        let mutations = [
            (2, vec![6, 0, 0, 0, 1, 128]),
            (3, vec![5, 0, 0, 0, 33]),
            (5, vec![1]),
            (6, vec![7, 0, 0, 0, 7, 0, 9]),
            (7, vec![8, 0, 0, 0, 0]),
            (8, event(Some(1_000_001))),
            (9, vec![11, 0, 0, 0, 5]),
            (10, scores(&[(1, 4), (1, 5)])),
            (10, scores(&[(2, 4), (1, 5)])),
        ];
        for (index, bad) in mutations {
            let mut values = fields(5, Some(12));
            values[index] = bad;
            let state = wire(12, &record(&values));
            assert!(
                matches!(
                    authority.publish(c::Raw {
                        state: &state,
                        command: &command,
                        context: &context
                    }),
                    a::PublicationOutcome::Refused { .. }
                ),
                "field {}",
                index + 1
            );
        }
        let state = state(5, Some(12));
        for end in 0..command.len() {
            assert!(matches!(
                authority.publish(c::Raw {
                    state: &state,
                    command: &command[..end],
                    context: &context
                }),
                a::PublicationOutcome::Refused { .. }
            ));
        }
    });
}

#[test]
fn original_parser_denied_reads_and_steps_remain_refusals() {
    use zeno_fcis_synthesis::finite::V2Resource as R;
    let state = state(5, Some(12));
    let command = wire(9, &event(None));
    let context = wire(5, &[2]);
    let raw = c::Raw {
        state: &state,
        command: &command,
        context: &context,
    };
    for reads in 0..13 {
        with_limits(reads, 100_000, |authority| {
            let a::PublicationOutcome::Refused { evaluation, .. } = authority.publish(raw) else {
                panic!("denied read must withhold publication")
            };
            let outcome = evaluation.outcome();
            assert_eq!(outcome.usage().used(R::Read), reads);
            assert_eq!(outcome.reads().len(), reads as usize + 1);
            let denied = outcome
                .reads()
                .last()
                .unwrap_or_else(|| panic!("denied read retained"));
            assert!(!denied.permitted);
            assert_eq!(
                (denied.source, denied.selector),
                if reads < 11 {
                    (0, c::Selector::Field(reads as u16 + 1))
                } else {
                    (reads as u8 - 10, c::Selector::Root)
                }
            );
            let payload_bytes = state.len() - 48
                + if reads >= 11 { command.len() - 48 } else { 0 }
                + if reads >= 12 { context.len() - 48 } else { 0 };
            assert_eq!(outcome.usage().used(R::Byte), 144 + payload_bytes as u64);
            assert!(evaluation.subject().is_err());
        });
    }
    for steps in 0..48 {
        with_limits(100_000, steps, |authority| {
            let a::PublicationOutcome::Refused { evaluation, .. } = authority.publish(raw) else {
                panic!("denied step must withhold publication")
            };
            assert_eq!(evaluation.outcome().usage().used(R::Step), steps);
            assert_eq!(
                evaluation.outcome().usage().used(R::Byte),
                144 + (state.len() - 48) as u64
            );
            assert!(evaluation.subject().is_err());
        });
    }
}
