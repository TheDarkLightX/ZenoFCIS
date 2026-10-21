//! Permanent witnesses for the shell-effects conformance review.
use std::num::NonZeroU64;
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::{
    CommitStatus,
    effects::{
        driver::{Binding, Driver, Reply},
        explore::{self, Layers, Order, Scenario},
        handler::{Handled, Terminal},
        laws::{self, Laws},
        port::{CommitMode, CommitReceipt, Fault, OpId, Operation, Outcome, Response, Sent},
        program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram},
        record::Manifest,
        registry::REGISTRY,
        simulation::{self, Durable, Profile, Script, Simulation, World},
    },
};

#[test]
fn exact_outbox_rejects_wrong_recipient() {
    let old = Durable::genesis();
    let key = Hash32::new([1; 32]);
    let data = simulation::publication(&old.state, 1);
    let mut wrong = laws::committed(&old, key, 1);
    wrong.outbox[0].0.destination = b"wrong-recipient".to_vec();
    let operation = Operation::Commit(simulation::store(1), key, data, CommitMode::Unversioned);
    let response = Handled::Answer(Response::Commit(Outcome::Ok(CommitReceipt {
        version: 1,
        chain: wrong.chain,
        status: CommitStatus::Committed,
    })));
    assert!(
        Laws::default()
            .check(
                &Profile::frozen(0),
                &World::new(old),
                &Binding::of(OpId::first(1), &operation),
                &operation,
                &response,
                &World::new(wrong)
            )
            .is_err(),
        "wrong recipient accepted by independent commit law"
    );
}

fn delivery(responses: Vec<Response>, retries: u8) -> Option<Outcome<u64>> {
    let mut driver = Driver::<DeliveryProgram>::start(
        REGISTRY,
        1,
        DeliveryProgram::start(
            DeliveryInvocation {
                epoch: 1,
                store: simulation::store(1),
            },
            DeliveryPolicy {
                retries,
                backoff: 1,
                jitter: NonZeroU64::MIN,
            },
        ),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    for response in responses {
        let binding = driver
            .request()
            .unwrap_or_else(|| panic!("missing request"))
            .0
            .clone();
        driver
            .reply(Reply { binding, response })
            .unwrap_or_else(|error| panic!("{error:?}"));
    }
    driver.outcome().cloned()
}

#[test]
fn lost_send_remains_unknown_after_clock_failure() {
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    assert_eq!(
        delivery(
            vec![
                Response::ReadOutbox(Outcome::Ok(durable.pending())),
                Response::Send(Sent::Unknown(Fault::Io)),
                Response::Clock(Err(Fault::Io))
            ],
            1
        ),
        Some(Outcome::Unknown(Fault::Io)),
        "send uncertainty was erased"
    );
}

#[test]
fn send_progress_survives_acknowledgement_and_later_record_failures() {
    use zeno_fcis_shell::effects::port::{EpochTick, Report, WaitError};
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let first = durable.pending()[0].clone();
    let outbox = Response::ReadOutbox(Outcome::Ok(durable.pending()));
    let report = Response::Send(Sent::Sent(Report {
        delivery_id: first.id,
        payload_sha256: first.payload_sha256,
    }));
    for fault in [Fault::Busy, Fault::Full, Fault::Io] {
        assert_eq!(
            delivery(
                vec![
                    outbox.clone(),
                    report.clone(),
                    Response::Acknowledge(Outcome::NotApplied(fault))
                ],
                0
            ),
            Some(Outcome::Unknown(fault)),
            "ack exhaustion erased send"
        );
        let start = vec![outbox.clone(), Response::Send(Sent::Unknown(Fault::Io))];
        let clock = Response::Clock(Ok(EpochTick { epoch: 1, tick: 0 }));
        for suffix in [
            vec![Response::Clock(Err(fault))],
            vec![clock.clone(), Response::Random(Err(fault))],
            vec![
                clock,
                Response::Random(Ok(0)),
                Response::Wait(Err(WaitError::Failed(fault))),
            ],
        ] {
            let mut trace = start.clone();
            trace.extend(suffix);
            assert_eq!(
                delivery(trace, 1),
                Some(Outcome::Unknown(fault)),
                "later adapter failure erased uncertainty"
            );
        }
    }
    assert_eq!(
        delivery(
            vec![
                outbox,
                report,
                Response::Acknowledge(Outcome::Ok(())),
                Response::Send(Sent::NotSent(
                    zeno_fcis_shell::effects::port::Code::Delivery
                ))
            ],
            0
        ),
        Some(Outcome::Unknown(Fault::Io)),
        "later record erased partial progress"
    );
}

fn responses(operation: &Operation) -> Vec<Handled> {
    let mut answers = Vec::new();
    for choice in 0..32 {
        let mut sim = Simulation::new(
            World::new(Durable::genesis()),
            Profile::frozen(1),
            simulation::ALL_PORTS,
            Script::new(vec![choice]),
        );
        let answer = sim.handle(&Binding::of(OpId::first(1), operation), operation);
        if answer == Handled::Suspend {
            break;
        }
        answers.push(answer);
        if sim.script.made().is_empty() {
            break;
        }
    }
    answers
}

#[test]
fn frozen_alphabet_contains_required_failures() {
    assert!(
        responses(&Operation::ReadState(simulation::store(1))).contains(&Handled::Answer(
            Response::ReadState(Outcome::NotApplied(Fault::Full))
        )),
        "Full missing from frozen alphabet"
    );
}

#[test]
fn frozen_alphabet_contains_clock_failures() {
    assert!(
        responses(&Operation::Clock).contains(&Handled::Answer(Response::Clock(Err(Fault::Io)))),
        "Clock failure missing"
    );
}

#[test]
fn frozen_alphabet_contains_random_failures() {
    assert!(
        responses(&Operation::Random(
            zeno_fcis_shell::effects::port::Purpose::RetryJitter,
            NonZeroU64::MIN
        ))
        .contains(&Handled::Answer(Response::Random(Err(Fault::Io)))),
        "Random failure missing"
    );
}

#[test]
fn frozen_alphabet_contains_wait_io_failure() {
    assert!(
        responses(&Operation::Wait(
            zeno_fcis_shell::effects::port::EpochTick { epoch: 1, tick: 1 }
        ))
        .contains(&Handled::Answer(Response::Wait(Err(
            zeno_fcis_shell::effects::port::WaitError::Failed(Fault::Io)
        )))),
        "Wait I/O failure missing"
    );
}

#[test]
fn incomplete_retains_frontier_and_binding() {
    let report = explore::explore(
        Scenario::Commit,
        Profile::frozen(0),
        Layers::Record,
        Order::Depth,
        0,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    let retained = format!("{report:?}");
    assert!(
        retained.contains("checkpoint: Some(") && retained.contains("frontier:"),
        "incomplete exploration discarded its checkpoint"
    );
}

#[test]
fn manifest_binds_handler_sources_and_seed() {
    let manifest = Manifest::current(vec!["record", "simulation"], vec![], Hash32::ZERO, vec![]);
    let bound = format!("{manifest:?}");
    assert!(
        bound.contains("implementations:")
            && bound.contains("seed:")
            && bound.contains("configuration:"),
        "handler provenance and seed missing from manifest"
    );
}
