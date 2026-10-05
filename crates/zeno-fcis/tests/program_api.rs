//! External consumer of the normal umbrella route; no direct synthesis imports.
#[path = "../examples/support/program_fixture.rs"]
mod support;

use zeno_fcis::prelude::*;
use zeno_fcis::program::{declaration::Class, law::Verdict, report::ExecutionRefusal};

fn with_program(limits: Limits, conforming: bool, run: impl FnOnce(&Program<'_>)) {
    support::with_material(limits, conforming, |d| {
        let catalog = bind_catalog(
            d.original,
            d.description,
            support::catalog_limits(),
            d.policy,
            d.descriptor,
            d.framing,
            d.links,
        )
        .unwrap_or_else(|e| panic!("complete declaration admission: {e:?}"));
        let program =
            bind_program(&catalog).unwrap_or_else(|e| panic!("actual library authority: {e:?}"));
        run(&program);
    });
}

#[test]
fn all_decision_classes_use_original_wires_and_actual_reports() {
    with_program(support::generous(), true, |program| {
        for (code, class, reason) in [
            (0, Class::Accept, None),
            (1, Class::Reject, Some(7)),
            (2, Class::CommittedFailure, Some(9)),
        ] {
            let (state, command, context) = support::originals(code, true);
            let original = Invocation {
                state: &state,
                command: &command,
                context: &context,
            };
            let outcome = program.publish(original);
            let evaluated = outcome.evaluation();
            let decision = evaluated
                .result()
                .unwrap_or_else(|e| panic!("actual decision: {e:?}"));
            assert_eq!(decision.class(), class);
            assert_eq!(decision.reason(), reason);
            assert_eq!(evaluated.raw().state, state);
            assert_eq!(evaluated.raw().command, command);
            assert_eq!(evaluated.raw().context, context);
            assert!(evaluated.usage().used(Resource::Step) > 0);
            assert!(evaluated.usage().used(Resource::Read) > 0);
            assert_eq!(evaluated.diagnostics().len(), 5);
            if class == Class::Reject {
                let PublicationOutcome::Reject(audit) = &outcome else {
                    panic!("Reject gained a capability")
                };
                assert!(decision.post().is_empty());
                assert!(decision.patch().is_empty());
                assert!(decision.effects().is_empty());
                assert!(decision.outbox().is_empty());
                let subject = audit
                    .subject()
                    .unwrap_or_else(|e| panic!("Reject audit: {e:?}"));
                assert!(matches!(
                    program.replay_publication(original, subject),
                    PublicationOutcome::Reject(_)
                ));
            } else {
                let PublicationOutcome::Commit(publication) = outcome else {
                    panic!("missing actual capability")
                };
                assert_eq!(publication.identity(), program.identity());
                assert_eq!(
                    publication.poststate(),
                    support::frame(100, &[9, 0, 0, 0, 1, 0, 0, 1])
                );
                assert_eq!(publication.effects().len(), 1);
                assert_eq!(publication.outbox().len(), 1);
                let mut payload = vec![9, 0, 0, 0, 1, 0, 0, 4];
                payload.extend_from_slice(&code.to_be_bytes());
                for (delivery, ordinal, destination, key) in [
                    (
                        &publication.effects()[0],
                        5,
                        b"effect".as_slice(),
                        b"ekey".as_slice(),
                    ),
                    (
                        &publication.outbox()[0],
                        9,
                        b"outbox".as_slice(),
                        b"okey".as_slice(),
                    ),
                ] {
                    assert_eq!((delivery.ordinal(), delivery.channel()), (ordinal, 3));
                    assert_eq!(
                        (delivery.destination_root(), delivery.payload_root()),
                        (200, 201)
                    );
                    assert_eq!(delivery.payload(), payload);
                    let mut encoded_destination = vec![6, 0, 0, 0, 6];
                    encoded_destination.extend_from_slice(destination);
                    let mut encoded_key = vec![5, 0, 0, 0, 4];
                    encoded_key.extend_from_slice(key);
                    assert_eq!(delivery.destination(), encoded_destination);
                    assert_eq!(delivery.idempotency(), encoded_key);
                }
                let PublicationOutcome::Commit(replayed) =
                    program.replay_publication(original, publication.subject())
                else {
                    panic!("complete replay refused")
                };
                assert_eq!(replayed.subject(), publication.subject());
            }
            assert_eq!(state.last(), Some(&2), "caller prestate remains immutable");
        }
    });
}

#[test]
fn genesis_is_a_real_initial_law_gate_and_replay() {
    with_program(support::generous(), true, |program| {
        let (valid, _, _) = support::originals(0, true);
        let PublicationOutcome::Commit(publication) = program.publish_genesis(&valid) else {
            panic!("valid genesis refused")
        };
        assert_eq!(publication.poststate(), valid);
        assert_eq!(publication.identity(), program.identity());
        let PublicationOutcome::Commit(replay) =
            program.replay_genesis_publication(&valid, publication.subject())
        else {
            panic!("genesis replay refused")
        };
        assert_eq!(replay.subject(), publication.subject());
        let (invalid, _, _) = support::originals(0, false);
        let refused = program.publish_genesis(&invalid);
        assert!(matches!(refused, PublicationOutcome::Refused { .. }));
        assert!(refused.evaluation().result().is_err());
        assert!(refused.evaluation().subject().is_err());
        assert!(
            refused
                .evaluation()
                .diagnostics()
                .iter()
                .any(|d| d.id == 50 && matches!(d.verdict, Verdict::Refused(_)))
        );
    });
}

#[test]
fn publication_replay_rejects_mutated_subject_segments() {
    with_program(support::generous(), true, |program| {
        let (state, command, context) = support::originals(0, true);
        let raw = Invocation {
            state: &state,
            command: &command,
            context: &context,
        };
        let PublicationOutcome::Commit(publication) = program.publish(raw) else {
            panic!("baseline publication")
        };
        // The subject embeds the source-bound identity; exhaustively replaying each
        // byte would be quadratic in that large subject. Mutate every boundary
        // byte and a deterministic spread through the full retained subject.
        let length = publication.subject().len();
        let mut positions = vec![0, 1, 16, 17, length - 2, length - 1];
        positions.extend((1..32).map(|i| i * length / 32));
        for wire in [
            state.as_slice(),
            command.as_slice(),
            context.as_slice(),
            publication.poststate(),
            publication.effects()[0].destination(),
            publication.effects()[0].payload(),
            publication.outbox()[0].destination(),
            publication.outbox()[0].idempotency(),
        ] {
            let offset = publication
                .subject()
                .windows(wire.len())
                .position(|w| w == wire)
                .unwrap_or_else(|| panic!("wire missing from complete subject"));
            positions.extend([offset, offset + wire.len() - 1]);
        }
        positions.sort_unstable();
        positions.dedup();
        for at in positions {
            let mut changed = publication.subject().to_vec();
            changed[at] ^= 1;
            let refused = program.replay_publication(raw, &changed);
            assert!(
                matches!(
                    refused,
                    PublicationOutcome::Refused {
                        error: Refusal::ReplayMismatch,
                        ..
                    }
                ),
                "byte {at}"
            );
        }
        assert!(matches!(
            program.replay_publication(raw, &publication.subject()[1..]),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
        let (_, changed_command, _) = support::originals(2, true);
        let changed = Invocation {
            command: &changed_command,
            ..raw
        };
        assert!(matches!(
            program.replay_publication(changed, publication.subject()),
            PublicationOutcome::Refused {
                error: Refusal::ReplayMismatch,
                ..
            }
        ));
    });
}

#[test]
fn law_failure_retains_audit_without_a_successful_subject() {
    with_program(support::generous(), false, |program| {
        let (state, command, context) = support::originals(0, true);
        let refused = program.publish(Invocation {
            state: &state,
            command: &command,
            context: &context,
        });
        assert!(matches!(
            refused,
            PublicationOutcome::Refused {
                error: Refusal::Core(ExecutionRefusal::Law(_)),
                ..
            }
        ));
        assert!(refused.evaluation().result().is_err());
        assert!(refused.evaluation().subject().is_err());
        assert!(refused.evaluation().usage().used(Resource::Step) > 0);
        assert!(
            refused
                .evaluation()
                .diagnostics()
                .iter()
                .any(|d| d.id == 40 && matches!(d.verdict, Verdict::Refused(_)))
        );
    });
}

#[test]
fn the_library_meter_refuses_before_an_unfunded_instruction() {
    with_program(
        support::generous().with_limit(Resource::Step, 0),
        true,
        |program| {
            let (state, command, context) = support::originals(0, true);
            let refused = program.publish(Invocation {
                state: &state,
                command: &command,
                context: &context,
            });
            assert!(matches!(
                refused,
                PublicationOutcome::Refused {
                    error: Refusal::Core(ExecutionRefusal::Execution(_)),
                    ..
                }
            ));
            assert_eq!(refused.evaluation().usage().used(Resource::Step), 0);
            assert!(refused.evaluation().usage().used(Resource::Read) > 0);
            assert!(refused.evaluation().result().is_err());
            assert!(refused.evaluation().subject().is_err());
        },
    );
}

#[test]
fn framing_and_original_state_are_not_a_host_supplied_view() {
    with_program(support::generous(), true, |program| {
        let (state, command, context) = support::originals(0, true);
        for at in [0, 8, 12, 44] {
            let mut changed = state.clone();
            changed[at] ^= 1;
            let refused = program.publish(Invocation {
                state: &changed,
                command: &command,
                context: &context,
            });
            assert!(matches!(
                refused,
                PublicationOutcome::Refused {
                    error: Refusal::Core(ExecutionRefusal::Frame(0, _)),
                    ..
                }
            ));
        }
        let refused = program.publish(Invocation {
            state: &state[48..],
            command: &command,
            context: &context,
        });
        assert!(matches!(refused, PublicationOutcome::Refused { .. }));
        let (_, outside, _) = support::originals(3, true);
        assert!(matches!(
            program.publish(Invocation {
                state: &state,
                command: &outside,
                context: &context
            }),
            PublicationOutcome::Refused { .. }
        ));
    });
}

#[test]
fn complete_original_policy_and_schema_are_mandatory() {
    support::with_material(support::generous(), true, |d| {
        let bind = |original, policy, links| {
            bind_catalog(
                original,
                d.description,
                support::catalog_limits(),
                policy,
                d.descriptor,
                d.framing,
                links,
            )
        };
        assert!(bind(d.original, d.policy, d.links).is_ok());
        let mut changed_policy = d.policy.to_vec();
        changed_policy[0] ^= 1;
        assert!(matches!(
            bind(d.original, &changed_policy, d.links),
            Err(CatalogRefusal::Policy)
        ));
        let mut changed_schema = d.original.to_vec();
        changed_schema[0] ^= 1;
        assert!(matches!(
            bind(&changed_schema, d.policy, d.links),
            Err(CatalogRefusal::Schema(_))
        ));
        assert!(bind(d.original, d.policy, &[(3, 200, 300)]).is_err());
    });
}

#[test]
fn all_supported_full_width_declarations_remain_on_the_normal_route() {
    use zeno_fcis::program::{declaration as d, law, schema};
    let wide = d::Domain::U128 {
        min: 0,
        max: u128::MAX,
    };
    assert!(matches!(wide, d::Domain::U128 { max: u128::MAX, .. }));
    let signed = schema::Kind::I128 {
        min: i128::MIN,
        max: i128::MAX,
    };
    assert!(matches!(
        signed,
        schema::Kind::I128 {
            min: i128::MIN,
            max: i128::MAX
        }
    ));
    let guarded = law::Op::ObserveWhen(0, law::Observation::EffectLength, law::Atom::U128(0));
    assert!(matches!(guarded, law::Op::ObserveWhen(0, _, _)));
    // These are declaration/value-domain checks; full-width compound execution
    // remains outside the current V2 runtime profile.
}
