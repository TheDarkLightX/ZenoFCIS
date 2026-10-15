//! Pinned original single-vault semantics, not a reduced replacement model.
//! Characterization is not a proof of equivalence over the full input domain.
#[path = "../../zeno-fcis-synthesis/tests/fixtures/g3-original-zusd/fixture.rs"]
mod fixture;
use fixture::*;
use zeno_fcis_synthesis::finite::{
    v2_authority::PublicationOutcome, v2_composition as c, v2_laws as l,
};
use zeno_fcis_value::Value;

/// Registered reasons the pinned single-vault Python never returns: none of its
/// error strings maps to them (`UNREACHABLE_REASONS` in
/// `tools/test_data/g3_original_zusd/compile_zusd.py`, recorded in SOURCE.json).
const UNREACHABLE_REASONS: [u32; 3] = [210, 239, 240];

#[test]
fn pinned_original_native_corpus_crosses_complete_authority_laws_and_replay() {
    with_authority(|authority, framing| {
        assert_eq!(NODE_DATA.len(), 1238);
        assert_eq!(ROOTS.len(), 46);
        assert_eq!(REASONS.len(), 46);
        let context = envelope(framing.context, Value::boolean(false));
        let mut count = 0;
        let mut accepted = [false; 12];
        let mut wide_deposits = 0;
        let mut exercised = std::collections::BTreeSet::new();
        for case in cases() {
            let pre = numbers(framing.state, 110, &case.inputs[..32]);
            let command = numbers(framing.command, 150, &case.inputs[32..]);
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let outcome = authority.publish(raw);
            let evaluation = outcome.evaluation();
            let candidate = evaluation
                .result()
                .unwrap_or_else(|e| panic!("case {count}: {e:?}"));
            assert!(
                evaluation
                    .outcome()
                    .diagnostics()
                    .iter()
                    .all(|d| matches!(d.verdict, l::Verdict::Satisfied | l::Verdict::Skipped)),
                "case {count}"
            );
            if case.code < 200 {
                assert_eq!(candidate.class(), c::Class::Accept, "case {count}");
                let publication = commit(outcome);
                assert_eq!(
                    publication.poststate(),
                    numbers(framing.state, 110, &case.post),
                    "case {count}"
                );
                assert_eq!(publication.effects().len(), 1);
                assert_eq!(publication.effects()[0].channel(), 400 + case.code);
                assert_eq!(
                    publication.effects()[0].payload(),
                    effect_payload(case.event, &case.effects)
                );
                assert!(publication.outbox().is_empty());
                let replay = commit(authority.replay_publication(raw, publication.subject()));
                assert_eq!(replay.poststate(), publication.poststate());
                assert_eq!(replay.subject(), publication.subject());
                accepted[ok(usize::try_from(case.code - 1))] = true;
                if case.code == 5 && case.inputs[33] == 1u128 << 63 {
                    wide_deposits += 1;
                }
            } else {
                assert_eq!(candidate.class(), c::Class::Reject, "case {count}");
                assert_eq!(candidate.reason(), Some(case.code), "case {count}");
                exercised.insert(case.code);
                assert!(
                    candidate.post().is_empty()
                        && candidate.patch().is_empty()
                        && candidate.effects().is_empty()
                        && candidate.outbox().is_empty()
                );
                let subject = ok(evaluation.subject()).to_vec();
                assert!(matches!(outcome, PublicationOutcome::Reject(_)));
                assert!(matches!(
                    authority.replay_publication(raw, &subject),
                    PublicationOutcome::Reject(_)
                ));
            }
            count += 1;
        }
        assert_eq!(count, 8064);
        assert_eq!(accepted, [true; 12]);
        assert_eq!(wide_deposits, 56);
        // Every reachable registered reason runs through the route.
        let reachable: std::collections::BTreeSet<u32> = REASONS
            .iter()
            .map(|reason| reason.id)
            .filter(|id| !UNREACHABLE_REASONS.contains(id))
            .collect();
        assert_eq!(reachable.len(), 43);
        assert_eq!(exercised, reachable);
    });
}

#[test]
fn a_changed_decision_is_withheld_by_complete_data_laws() {
    let mut descriptor = descriptor();
    let mut nodes = descriptor.program.nodes.to_vec();
    assert!(matches!(
        nodes[6],
        zeno_fcis_synthesis::finite::V2NumericOp::Input(6)
    ));
    nodes[6] = zeno_fcis_synthesis::finite::V2NumericOp::U128(0);
    descriptor.program.nodes = &nodes;
    with_descriptor(descriptor, |authority, framing| {
        let mut state = GENESIS.to_vec();
        state[6] = 10;
        let pre = numbers(framing.state, 110, &state);
        let command = numbers(framing.command, 150, &[5, 1, 0]);
        let context = envelope(framing.context, Value::boolean(false));
        let outcome = authority.publish(c::Raw {
            state: &pre,
            command: &command,
            context: &context,
        });
        assert!(matches!(outcome, PublicationOutcome::Refused { .. }));
        assert!(
            outcome
                .evaluation()
                .outcome()
                .diagnostics()
                .iter()
                .any(|d| d.id == 510 && matches!(d.verdict, l::Verdict::Refused(_)))
        );
    });
}

#[test]
fn original_genesis_and_raw_u128_boundary_are_enforced_without_authentication_claims() {
    with_authority(|authority, framing| {
        let initial = numbers(framing.state, 110, GENESIS);
        assert_eq!(
            commit(authority.publish_genesis(&initial)).poststate(),
            initial
        );
        let mut bad = GENESIS.to_vec();
        bad[0] = 1;
        assert!(matches!(
            authority.publish_genesis(&numbers(framing.state, 110, &bad)),
            PublicationOutcome::Refused { .. }
        ));
        let context = envelope(framing.context, Value::boolean(false));
        for (action, reason) in [(u128::MAX, 203), (5, 201)] {
            let command = numbers(framing.command, 150, &[action, u128::MAX, 0]);
            let result = authority.publish(c::Raw {
                state: &initial,
                command: &command,
                context: &context,
            });
            assert!(matches!(result, PublicationOutcome::Reject(_)));
            assert_eq!(ok(result.evaluation().result()).reason(), Some(reason));
        }
    });
}
