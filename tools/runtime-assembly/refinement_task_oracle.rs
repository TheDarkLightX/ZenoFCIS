//! Independent small reference relation, compared directly with the generated
//! task Authority. No generated decision table, review packet or certificate is
//! consumed. These are finite core checks, not transport or publication proofs.
use application::v2_contract;
use zeno_fcis_synthesis::finite::{v2_authority::Refusal, v2_composition as c, v2_laws as laws};

#[derive(Debug, PartialEq, Eq)]
struct Decision<'a> {
    class: c::Class,
    reason: Option<u32>,
    pre: Vec<c::Field<'a>>,
    post: Vec<c::Field<'a>>,
    patch: Vec<laws::Patch<'a>>,
    effects: Vec<laws::Delivery<'a>>,
    outbox: Vec<laws::Delivery<'a>>,
}

fn state(status: i128, attempt: i128) -> Vec<c::Field<'static>> {
    vec![
        c::Field {
            id: 110,
            value: c::Atom::Sum {
                type_id: 107,
                variant: status as u16,
            },
        },
        c::Field {
            id: 111,
            value: c::Atom::I128(attempt),
        },
    ]
}

// This match-based oracle is authored independently of the policy expression
// compiler. Err means specifically state-invariant law 500 refuses a candidate;
// it is never a business rejection or an omitted input.
fn reference(
    [status, attempt, action, observed, artifact, role]: [i128; 6],
) -> Result<Decision<'static>, ()> {
    let actor = match action {
        160 | 165 | 166 => 170,
        161..=164 => 171,
        _ => panic!("outside declared action domain"),
    };
    let stage = match action {
        160 => status == 150,
        161 | 165 => status == 151,
        162 | 163 => matches!(status, 152 | 155),
        164 => status == 152,
        166 => status == 154,
        _ => unreachable!(),
    };
    let reason = if role != actor {
        Some(200)
    } else if artifact != 1 {
        Some(201)
    } else if observed != attempt {
        Some(202)
    } else if !stage {
        Some(203)
    } else if action == 166 && attempt == 2 {
        Some(204)
    } else {
        None
    };
    let mut decision = Decision {
        class: c::Class::Reject,
        reason,
        pre: state(status, attempt),
        post: vec![],
        patch: vec![],
        effects: vec![],
        outbox: vec![],
    };
    if reason.is_some() {
        return Ok(decision);
    }
    let (next_status, next_attempt) = match action {
        160 => (151, 1),
        161 => (152, attempt),
        162 => (153, attempt),
        163 => (154, attempt),
        164 => (155, attempt),
        165 => (156, attempt),
        166 => (151, attempt + 1),
        _ => unreachable!(),
    };
    if next_attempt == 0 {
        return Err(());
    }
    decision.class = c::Class::Accept;
    decision.post = state(next_status, next_attempt);
    decision.patch = decision
        .pre
        .iter()
        .zip(&decision.post)
        .filter(|(before, after)| before != after)
        .map(|(before, after)| laws::Patch {
            field: before.id,
            before: before.value,
            after: after.value,
        })
        .collect();
    if action == 161 {
        decision.outbox.push(laws::Delivery {
            ordinal: 0,
            channel: 300,
            destination: c::Atom::Text(b"local-test"),
            payload: vec![c::Field {
                id: 140,
                value: c::Atom::I128(attempt),
            }],
            idempotency: c::Atom::U128(0),
        });
    }
    Ok(decision)
}

fn observe<'a>(candidate: &c::Candidate<'a>) -> Decision<'a> {
    let lane = |deliveries: &[laws::Delivery<'a>]| {
        deliveries
            .iter()
            .map(|d| laws::Delivery {
                ordinal: d.ordinal,
                channel: d.channel,
                destination: d.destination,
                payload: d.payload.clone(),
                idempotency: d.idempotency,
            })
            .collect()
    };
    Decision {
        class: candidate.class(),
        reason: candidate.reason(),
        pre: candidate.pre().to_vec(),
        post: candidate.post().to_vec(),
        patch: candidate.patch().to_vec(),
        effects: lane(candidate.effects()),
        outbox: lane(candidate.outbox()),
    }
}

#[test]
fn refinement_task_all_2646_actual_authority_decisions_match_independent_oracle() {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap();
    let mut counts = [0_usize; 3];
    for status in 150..=156 {
        for attempt in 0..=2 {
            for action in 160..=166 {
                for observed in 0..=2 {
                    for artifact in 1..=2 {
                        for role in 170..=172 {
                            let input = [status, attempt, action, observed, artifact, role];
                            let raw = application::wire(&descriptor, &input).unwrap();
                            let evaluation = authority.evaluate(c::Raw {
                                state: &raw[0],
                                command: &raw[1],
                                context: &raw[2],
                            });
                            match reference(input) {
                                Ok(expected) => {
                                    let actual = evaluation.result().unwrap_or_else(|e| {
                                        panic!("{input:?}: unexpected refusal {e:?}")
                                    });
                                    assert_eq!(
                                        observe(actual),
                                        expected,
                                        "complete candidate mismatch: {input:?}"
                                    );
                                    counts[usize::from(expected.class == c::Class::Reject)] += 1;
                                }
                                Err(()) => {
                                    assert!(
                                        matches!(
                                            evaluation.result(),
                                            Err(Refusal::Core(c::Failure::Law(
                                                laws::Failure::Violated
                                            )))
                                        ),
                                        "{input:?}: wrong technical refusal"
                                    );
                                    let refused: Vec<_> = evaluation
                                        .diagnostics()
                                        .iter()
                                        .filter(|d| matches!(d.verdict, laws::Verdict::Refused(_)))
                                        .collect();
                                    assert_eq!(
                                        refused.len(),
                                        1,
                                        "{input:?}: unexpected refusal diagnostics"
                                    );
                                    assert_eq!(refused[0].id, 500, "{input:?}: wrong refusing law");
                                    counts[2] += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(counts, [19, 2620, 7]);
    assert_eq!(counts.iter().sum::<usize>(), 2646);
}

#[test]
fn refinement_task_comparator_detects_complete_observation_substitutions() {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap();
    let input = [151, 1, 161, 1, 1, 171];
    let raw = application::wire(&descriptor, &input).unwrap();
    let evaluation = authority.evaluate(c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    });
    let candidate = evaluation.result().unwrap();
    let expected = reference(input).unwrap();
    assert_eq!(observe(candidate), expected);
    // Mutate only copied observations: private Candidate constructors remain
    // private. Each planted mismatch must be detected by the exact comparison.
    for mutation in 0..12 {
        let mut wrong = observe(candidate);
        match mutation {
            0 => wrong.outbox[0].destination = c::Atom::Text(b"other-receiver"),
            1 => wrong.outbox[0].payload[0].value = c::Atom::I128(2),
            2 => wrong.outbox[0].channel = 301,
            3 => wrong.outbox[0].ordinal = 1,
            4 => wrong.outbox[0].idempotency = c::Atom::U128(1),
            5 => wrong.reason = Some(200),
            6 => wrong.pre[1].value = c::Atom::I128(2),
            7 => wrong.patch.clear(),
            // An acknowledgement cannot be silently reported as domain success.
            8 => {
                wrong.post[0].value = c::Atom::Sum {
                    type_id: 107,
                    variant: 153,
                }
            }
            9 => wrong.effects.push(wrong.outbox.remove(0)),
            10 => wrong.class = c::Class::CommittedFailure,
            11 => wrong.outbox.push(observe(candidate).outbox.remove(0)),
            _ => unreachable!(),
        }
        assert_ne!(
            wrong, expected,
            "undetected observation mutation {mutation}"
        );
    }
    // Real core invocations with a wrong role, attempt or artifact must not be
    // accepted by reusing the original accepting reference observation.
    for altered in [
        [151, 1, 161, 1, 1, 170],
        [151, 1, 161, 0, 1, 171],
        [151, 1, 161, 1, 2, 171],
    ] {
        let raw = application::wire(&descriptor, &altered).unwrap();
        let evaluation = authority.evaluate(c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        });
        assert_ne!(
            observe(evaluation.result().unwrap()),
            expected,
            "substituted input {altered:?}"
        );
    }
}
