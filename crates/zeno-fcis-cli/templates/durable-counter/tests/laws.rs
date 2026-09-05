//! Calls the checker directly; authorization separately validates patch consistency.
use durable_counter::{generated::*, laws::CounterLaws, profile};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::Value;

#[derive(Clone, Copy)]
enum Delivery {
    Exact,
    Missing,
    WrongPayload,
    WrongDestination,
}

fn state(count: i128, failures: i128) -> CounterState {
    CounterState {
        count: CounterValue(count),
        failures: CounterValue(failures),
    }
}

fn check(
    post: CounterState,
    command: CounterCommand,
    allowed: bool,
    delivery: Delivery,
    failure: Option<u32>,
    effect: bool,
) -> Vec<LawObservation> {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = state(0, 0).to_value().unwrap();
    let payload = if matches!(delivery, Delivery::WrongPayload) {
        state(0, 0)
    } else {
        post.clone()
    };
    let payload = Notification {
        notified_count: payload.count,
        notified_failures: payload.failures,
    }
    .to_value()
    .unwrap();
    let destination = if matches!(delivery, Delivery::WrongDestination) {
        "another-observer"
    } else {
        "local-observer"
    };
    let entries = if matches!(delivery, Delivery::Missing) {
        vec![]
    } else {
        vec![OutboxEntry::new(
            0,
            300,
            NotificationDestination(destination.into())
                .to_value()
                .unwrap(),
            payload,
        )]
    };
    let outbox = OutboxPlan::try_new(entries).unwrap();
    let commit = CommitPlan::try_new(if effect {
        vec![Effect::new(0, 999, hash, hash, Value::I128(0))]
    } else {
        vec![]
    })
    .unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let post = post.to_value().unwrap();
    let command = command.to_value().unwrap();
    let context = CounterContext(allowed).to_value().unwrap();
    let decision = match failure {
        None => LawDecisionView::Accept {
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
        Some(reason_id) => LawDecisionView::CommittedFailure {
            reason_id,
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
    };
    let input = LawCheckInput::try_new(hash, hash, &pre, &command, &context, decision).unwrap();
    CounterLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
}

fn holds(observations: &[LawObservation]) -> bool {
    observations
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

#[test]
fn checker_rejects_wrong_relations_authority_and_complete_plans() {
    assert!(holds(&check(
        state(1, 0),
        CounterCommand::Increment,
        true,
        Delivery::Exact,
        None,
        false
    )));
    for bad_post in [state(0, 0), state(2, 0), state(1, 1)] {
        assert!(!holds(&check(
            bad_post,
            CounterCommand::Increment,
            true,
            Delivery::Exact,
            None,
            false
        )));
    }
    assert!(!holds(&check(
        state(1, 0),
        CounterCommand::Increment,
        false,
        Delivery::Exact,
        None,
        false
    )));
    assert!(!holds(&check(
        state(1, 0),
        CounterCommand::RecordFailure,
        true,
        Delivery::Exact,
        None,
        false
    )));
    for delivery in [
        Delivery::Missing,
        Delivery::WrongPayload,
        Delivery::WrongDestination,
    ] {
        assert!(!holds(&check(
            state(1, 0),
            CounterCommand::Increment,
            true,
            delivery,
            None,
            false
        )));
    }
    assert!(!holds(&check(
        state(1, 0),
        CounterCommand::Increment,
        true,
        Delivery::Exact,
        None,
        true
    )));
    assert!(holds(&check(
        state(0, 1),
        CounterCommand::RecordFailure,
        true,
        Delivery::Exact,
        Some(202),
        false
    )));
    assert!(!holds(&check(
        state(0, 1),
        CounterCommand::RecordFailure,
        true,
        Delivery::Exact,
        Some(201),
        false
    )));
    assert!(!holds(&check(
        state(1, 1),
        CounterCommand::RecordFailure,
        true,
        Delivery::Exact,
        Some(202),
        false
    )));
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    let nonzero = state(1, 0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &nonzero).unwrap();
    assert!(!holds(
        &CounterLaws::default()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap()
    ));
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        CounterLaws::default().evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}
