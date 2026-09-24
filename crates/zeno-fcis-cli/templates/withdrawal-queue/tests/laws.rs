//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use std::collections::BTreeMap;
use withdrawal_queue::{generated::*, laws::VaultLaws, profile};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::Value;

#[allow(clippy::too_many_arguments)]
fn vault(
    balance: i128,
    lane_a: LaneStatus,
    amount_a: i128,
    lane_b: LaneStatus,
    amount_b: i128,
    pause: i128,
    must_serve: bool,
    priority: Lane,
) -> Vault {
    Vault {
        balance: Money(balance),
        lane_a,
        amount_a: Money(amount_a),
        lane_b,
        amount_b: Money(amount_b),
        pause: PauseTicks(pause),
        must_serve: Flag(must_serve),
        priority,
    }
}

fn payout(destination: &str, lane: Lane, amount: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        300,
        PayoutDestination(destination.into()).to_value().unwrap(),
        PayoutRequest {
            paid_lane: lane,
            paid_amount: Amount(amount),
        }
        .to_value()
        .unwrap(),
    )
}

/// A committing decision, as the checker sees it.
struct Decided {
    pre: Vault,
    post: Vault,
    command: VaultCommand,
    caller: Caller,
    alarm: bool,
    payouts: Vec<OutboxEntry>,
    effect: bool,
}

fn decided(pre: Vault, post: Vault, command: VaultCommand, caller: Caller) -> Decided {
    Decided {
        pre,
        post,
        command,
        caller,
        alarm: false,
        payouts: Vec::new(),
        effect: false,
    }
}

fn tick(pre: Vault, post: Vault, alarm: bool, payouts: Vec<OutboxEntry>) -> Decided {
    Decided {
        alarm,
        payouts,
        ..decided(pre, post, withdrawal_queue::tick(), Caller::Keeper)
    }
}

/// The status the checker gives each law for an accepted decision, or `None`
/// when the decision cannot reach the laws at all: a state the schema cannot
/// hold does not even encode.
fn statuses(decided: Decided) -> Option<BTreeMap<u32, LawStatus>> {
    let hash = profile::digest("example/test/law", b"independent decision view").unwrap();
    let pre = decided.pre.to_value().ok()?;
    let post = decided.post.to_value().ok()?;
    let command = decided.command.to_value().unwrap();
    let context = TickContext {
        caller: decided.caller,
        alarm: Flag(decided.alarm),
    }
    .to_value()
    .unwrap();
    let outbox = OutboxPlan::try_new(decided.payouts).unwrap();
    let commit = CommitPlan::try_new(if decided.effect {
        vec![Effect::new(0, 999, hash, hash, Value::I128(0))]
    } else {
        vec![]
    })
    .unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let decision = LawDecisionView::Accept {
        post_state: &post,
        patch: &patch,
        commit_plan: &commit,
        outbox_plan: &outbox,
    };
    let input = LawCheckInput::try_new(hash, hash, &pre, &command, &context, decision).unwrap();
    let observations = VaultLaws::try_new()
        .unwrap()
        .evaluate(&input, LawLimits::default())
        .ok()?;
    Some(
        observations
            .iter()
            .map(|observation| (observation.law_id().get(), observation.status()))
            .collect(),
    )
}

/// Whether the checker accepts an accepted decision.
fn holds(decided: Decided) -> bool {
    statuses(decided).is_some_and(|statuses| {
        statuses
            .values()
            .all(|status| *status == LawStatus::Satisfied)
    })
}

use LaneStatus::{Arrived, Empty, Pending};

#[test]
fn each_law_refuses_the_decision_that_breaks_it() {
    let empty = |balance| vault(balance, Empty, 0, Empty, 0, 0, false, Lane::A);
    // Law 500: a request recorded without the balance to cover it.
    let uncovered = statuses(decided(
        vault(1, Empty, 0, Empty, 0, 0, false, Lane::A),
        vault(1, Arrived, 2, Empty, 0, 0, false, Lane::A),
        withdrawal_queue::request(Lane::A, 2),
        Caller::OwnerA,
    ))
    .unwrap();
    assert_eq!(uncovered[&500], LawStatus::Violated);
    // Law 501: a deposit that adds the wrong amount.
    let short = statuses(decided(
        empty(1),
        empty(2),
        withdrawal_queue::deposit(2),
        Caller::Operator,
    ))
    .unwrap();
    assert_eq!(short[&500], LawStatus::Satisfied);
    assert_eq!(short[&501], LawStatus::Violated);
    // Law 502: a deposit from the keeper.
    let keeper = statuses(decided(
        empty(1),
        empty(3),
        withdrawal_queue::deposit(2),
        Caller::Keeper,
    ))
    .unwrap();
    assert_eq!(keeper[&502], LawStatus::Violated);
    // Law 503: a tick that leaves an arrived lane unpresented.
    let unpresented = statuses(tick(
        vault(4, Arrived, 2, Empty, 0, 0, false, Lane::A),
        vault(4, Arrived, 2, Empty, 0, 2, false, Lane::A),
        true,
        vec![],
    ))
    .unwrap();
    assert_eq!(unpresented[&503], LawStatus::Violated);
    // A balance above the capacity cannot even be encoded, so no decision
    // can present it to the laws; the program refuses the deposit first.
    assert_eq!(
        statuses(decided(
            empty(3),
            empty(5),
            withdrawal_queue::deposit(2),
            Caller::Operator
        )),
        None
    );
    // The right decision satisfies every law.
    let right = statuses(tick(
        vault(4, Arrived, 2, Empty, 0, 0, false, Lane::A),
        vault(4, Pending, 2, Empty, 0, 2, false, Lane::A),
        true,
        vec![],
    ))
    .unwrap();
    assert_eq!(right.len(), 4);
    assert!(right.values().all(|status| *status == LawStatus::Satisfied));
}

#[test]
fn deposits_must_add_exactly_their_amount() {
    let empty = |balance| vault(balance, Empty, 0, Empty, 0, 0, false, Lane::A);
    let deposit = |pre, post, caller| decided(pre, post, withdrawal_queue::deposit(2), caller);
    assert!(holds(deposit(empty(1), empty(3), Caller::Operator)));
    assert!(holds(deposit(empty(2), empty(4), Caller::Operator)));
    // The wrong sum, a field the deposit does not name, the wrong caller.
    assert!(!holds(deposit(empty(1), empty(2), Caller::Operator)));
    assert!(!holds(deposit(
        empty(1),
        vault(3, Empty, 0, Empty, 0, 0, false, Lane::B),
        Caller::Operator
    )));
    assert!(!holds(deposit(empty(1), empty(3), Caller::Keeper)));
    // Over capacity: no accepted result is right, whether the sum is kept
    // or the balance is clipped to the capacity.
    assert!(!holds(deposit(empty(3), empty(5), Caller::Operator)));
    assert!(!holds(deposit(empty(3), empty(4), Caller::Operator)));
    // A deposit queues nothing and commits no effect.
    assert!(!holds(Decided {
        payouts: vec![payout("settlement", Lane::A, 2)],
        ..deposit(empty(1), empty(3), Caller::Operator)
    }));
    assert!(!holds(Decided {
        effect: true,
        ..deposit(empty(1), empty(3), Caller::Operator)
    }));
}

#[test]
fn requests_must_record_exactly_one_covered_arrival() {
    let pre = || vault(4, Empty, 0, Arrived, 2, 0, false, Lane::A);
    let request =
        |post, caller| decided(pre(), post, withdrawal_queue::request(Lane::A, 2), caller);
    assert!(holds(request(
        vault(4, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerA
    )));
    // Recorded as already presented, with the wrong amount, on the wrong
    // lane, or touching the balance.
    assert!(!holds(request(
        vault(4, Pending, 2, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerA
    )));
    assert!(!holds(request(
        vault(4, Arrived, 1, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerA
    )));
    assert!(!holds(request(
        vault(4, Empty, 0, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerA
    )));
    assert!(!holds(request(
        vault(2, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerA
    )));
    // The other lane's owner, or the operator.
    assert!(!holds(request(
        vault(4, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        Caller::OwnerB
    )));
    assert!(!holds(request(
        vault(4, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        Caller::Operator
    )));
    // An occupied lane, or a request the unreserved balance does not cover,
    // must not commit at all.
    assert!(!holds(decided(
        vault(4, Pending, 1, Empty, 0, 0, false, Lane::A),
        vault(4, Arrived, 2, Empty, 0, 0, false, Lane::A),
        withdrawal_queue::request(Lane::A, 2),
        Caller::OwnerA
    )));
    assert!(!holds(decided(
        vault(3, Empty, 0, Arrived, 2, 0, false, Lane::A),
        vault(3, Arrived, 2, Arrived, 2, 0, false, Lane::A),
        withdrawal_queue::request(Lane::A, 2),
        Caller::OwnerA
    )));
}

#[test]
fn ticks_must_follow_the_contract() {
    // Paused: the pause counts down and nothing is paid.
    let paused = vault(4, Pending, 2, Pending, 2, 2, false, Lane::A);
    assert!(holds(tick(
        paused.clone(),
        vault(4, Pending, 2, Pending, 2, 1, false, Lane::A),
        false,
        vec![]
    )));
    assert!(!holds(tick(
        paused.clone(),
        vault(2, Empty, 0, Pending, 2, 1, false, Lane::B),
        false,
        vec![payout("settlement", Lane::A, 2)]
    )));
    assert!(!holds(tick(
        paused.clone(),
        vault(4, Pending, 2, Pending, 2, 2, false, Lane::A),
        false,
        vec![]
    )));
    // An alarm outside must-serve is honored: nothing is paid, the pause
    // starts, and the arrived lane is presented.
    let arrived = vault(4, Arrived, 2, Empty, 0, 0, false, Lane::A);
    assert!(holds(tick(
        arrived.clone(),
        vault(4, Pending, 2, Empty, 0, 2, false, Lane::A),
        true,
        vec![]
    )));
    assert!(!holds(tick(
        arrived.clone(),
        vault(2, Empty, 0, Empty, 0, 0, false, Lane::B),
        true,
        vec![payout("settlement", Lane::A, 2)]
    )));
    assert!(!holds(tick(
        arrived.clone(),
        vault(4, Pending, 2, Empty, 0, 1, false, Lane::A),
        true,
        vec![]
    )));
    assert!(!holds(tick(
        arrived.clone(),
        vault(4, Arrived, 2, Empty, 0, 2, false, Lane::A),
        true,
        vec![]
    )));
    // A lane that is not due cannot be paid.
    assert!(!holds(tick(
        arrived.clone(),
        vault(4, Pending, 2, Empty, 0, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::B, 2)]
    )));
    // The pause ends: a due lane starts must-serve, an empty vault does not.
    assert!(holds(tick(
        vault(4, Pending, 2, Empty, 0, 1, false, Lane::A),
        vault(4, Pending, 2, Empty, 0, 0, true, Lane::A),
        true,
        vec![]
    )));
    assert!(!holds(tick(
        vault(4, Pending, 2, Empty, 0, 1, false, Lane::A),
        vault(4, Pending, 2, Empty, 0, 0, false, Lane::A),
        true,
        vec![]
    )));
    assert!(holds(tick(
        vault(4, Empty, 0, Empty, 0, 1, false, Lane::A),
        vault(4, Empty, 0, Empty, 0, 0, false, Lane::A),
        true,
        vec![]
    )));
    assert!(!holds(tick(
        vault(4, Empty, 0, Empty, 0, 1, false, Lane::A),
        vault(4, Empty, 0, Empty, 0, 0, true, Lane::A),
        true,
        vec![]
    )));
    // Nothing due and no alarm: nothing changes.
    assert!(holds(tick(
        vault(1, Empty, 0, Empty, 0, 0, false, Lane::B),
        vault(1, Empty, 0, Empty, 0, 0, false, Lane::B),
        false,
        vec![]
    )));
}

#[test]
fn ticks_must_follow_the_strategy_row() {
    // Both lanes due, priority A: lane A is paid and priority moves to B.
    let both = vault(4, Pending, 2, Pending, 2, 0, false, Lane::A);
    assert!(holds(tick(
        both.clone(),
        vault(2, Empty, 0, Pending, 2, 0, false, Lane::B),
        false,
        vec![payout("settlement", Lane::A, 2)]
    )));
    // Paying the other lane is allowed by the contract but not the row.
    assert!(!holds(tick(
        both.clone(),
        vault(2, Pending, 2, Empty, 0, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::B, 2)]
    )));
    // Waiting is allowed by the contract but not the row; keeping priority
    // is neither.
    assert!(!holds(tick(
        both.clone(),
        vault(4, Pending, 2, Pending, 2, 0, false, Lane::A),
        false,
        vec![]
    )));
    assert!(!holds(tick(
        both.clone(),
        vault(2, Empty, 0, Pending, 2, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::A, 2)]
    )));
    // In must-serve the alarm is ignored and the due lane is paid; waiting
    // would keep must-serve, which the contract allows but the row does not.
    let serving = vault(3, Pending, 1, Empty, 0, 0, true, Lane::A);
    assert!(holds(tick(
        serving.clone(),
        vault(2, Empty, 0, Empty, 0, 0, false, Lane::B),
        true,
        vec![payout("settlement", Lane::A, 1)]
    )));
    assert!(!holds(tick(
        serving.clone(),
        vault(3, Pending, 1, Empty, 0, 0, true, Lane::A),
        true,
        vec![]
    )));
    assert!(!holds(tick(
        serving.clone(),
        vault(3, Pending, 1, Empty, 0, 2, false, Lane::A),
        true,
        vec![]
    )));
}

#[test]
fn payouts_must_move_exactly_the_paid_amount() {
    let pre = vault(3, Pending, 2, Arrived, 1, 0, false, Lane::B);
    let paid = vault(2, Pending, 2, Empty, 0, 0, false, Lane::A);
    assert!(holds(tick(
        pre.clone(),
        paid.clone(),
        false,
        vec![payout("settlement", Lane::B, 1)]
    )));
    // The wrong amount, destination, lane, or count of requests; no request.
    assert!(!holds(tick(
        pre.clone(),
        paid.clone(),
        false,
        vec![payout("settlement", Lane::B, 2)]
    )));
    assert!(!holds(tick(
        pre.clone(),
        paid.clone(),
        false,
        vec![payout("elsewhere", Lane::B, 1)]
    )));
    assert!(!holds(tick(
        pre.clone(),
        paid.clone(),
        false,
        vec![payout("settlement", Lane::A, 1)]
    )));
    assert!(!holds(tick(
        pre.clone(),
        paid.clone(),
        false,
        vec![
            payout("settlement", Lane::B, 1),
            OutboxEntry::new(
                1,
                300,
                PayoutDestination("settlement".into()).to_value().unwrap(),
                PayoutRequest {
                    paid_lane: Lane::B,
                    paid_amount: Amount(1),
                }
                .to_value()
                .unwrap(),
            ),
        ]
    )));
    assert!(!holds(tick(pre.clone(), paid.clone(), false, vec![])));
    // The balance not debited, debited twice, or the lane's amount kept.
    assert!(!holds(tick(
        pre.clone(),
        vault(3, Pending, 2, Empty, 0, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::B, 1)]
    )));
    assert!(!holds(tick(
        pre.clone(),
        vault(1, Pending, 2, Empty, 0, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::B, 1)]
    )));
    assert!(!holds(tick(
        pre.clone(),
        vault(2, Pending, 2, Empty, 1, 0, false, Lane::A),
        false,
        vec![payout("settlement", Lane::B, 1)]
    )));
    // An effect is refused on any decision.
    assert!(!holds(Decided {
        effect: true,
        ..tick(pre, paid, false, vec![payout("settlement", Lane::B, 1)])
    }));
}

#[test]
fn rejections_must_carry_the_rule_that_applies_and_failures_never_commit() {
    let hash = profile::digest("example/test/reject", b"independent decision view").unwrap();
    let pre = vault(3, Arrived, 2, Empty, 0, 0, false, Lane::A);
    let cases = [
        (withdrawal_queue::deposit(1), Caller::Keeper, 200, true),
        (withdrawal_queue::deposit(1), Caller::Operator, 200, false),
        (withdrawal_queue::deposit(2), Caller::Operator, 203, true),
        (withdrawal_queue::deposit(1), Caller::Operator, 203, false),
        (
            withdrawal_queue::request(Lane::A, 1),
            Caller::OwnerA,
            201,
            true,
        ),
        (
            withdrawal_queue::request(Lane::A, 1),
            Caller::OwnerA,
            202,
            false,
        ),
        (
            withdrawal_queue::request(Lane::B, 2),
            Caller::OwnerB,
            202,
            true,
        ),
        (
            withdrawal_queue::request(Lane::B, 1),
            Caller::OwnerB,
            202,
            false,
        ),
        (
            withdrawal_queue::request(Lane::B, 2),
            Caller::OwnerA,
            200,
            true,
        ),
        (
            withdrawal_queue::request(Lane::B, 2),
            Caller::OwnerA,
            202,
            false,
        ),
        (withdrawal_queue::tick(), Caller::Operator, 200, true),
        (withdrawal_queue::tick(), Caller::Keeper, 200, false),
    ];
    for (command, caller, reason_id, right) in cases {
        let pre = pre.to_value().unwrap();
        let command = command.to_value().unwrap();
        let context = TickContext {
            caller,
            alarm: Flag(false),
        }
        .to_value()
        .unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = VaultLaws::try_new()
            .unwrap()
            .evaluate(&input, LawLimits::default());
        assert_eq!(result.is_ok(), right, "reason {reason_id}: {result:?}");
    }
    let post = pre.to_value().unwrap();
    let pre = pre.to_value().unwrap();
    let command = withdrawal_queue::request(Lane::A, 1).to_value().unwrap();
    let context = TickContext {
        caller: Caller::OwnerA,
        alarm: Flag(false),
    }
    .to_value()
    .unwrap();
    let outbox = OutboxPlan::try_new(vec![]).unwrap();
    let commit = CommitPlan::try_new(vec![]).unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let input = LawCheckInput::try_new(
        hash,
        hash,
        &pre,
        &command,
        &context,
        LawDecisionView::CommittedFailure {
            reason_id: 201,
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
    )
    .unwrap();
    assert_eq!(
        VaultLaws::try_new()
            .unwrap()
            .evaluate(&input, LawLimits::default()),
        Err(LawEngineFailure::InvalidOutput)
    );
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state").unwrap();
    let genesis = vault(0, Empty, 0, Empty, 0, 0, false, Lane::A);
    for (state, expected) in [
        (genesis.clone(), true),
        (vault(1, Empty, 0, Empty, 0, 0, false, Lane::A), false),
        (vault(0, Empty, 0, Empty, 0, 0, false, Lane::B), false),
        (vault(0, Empty, 0, Empty, 0, 1, false, Lane::A), false),
    ] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = VaultLaws::try_new()
            .unwrap()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected,
            "{state:?}"
        );
    }
    let value = genesis.to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        VaultLaws::try_new()
            .unwrap()
            .evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
    let command = withdrawal_queue::deposit(1).to_value().unwrap();
    let context = TickContext {
        caller: Caller::Operator,
        alarm: Flag(false),
    }
    .to_value()
    .unwrap();
    let post = vault(1, Empty, 0, Empty, 0, 0, false, Lane::A)
        .to_value()
        .unwrap();
    let outbox = OutboxPlan::try_new(vec![]).unwrap();
    let commit = CommitPlan::try_new(vec![]).unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let input = LawCheckInput::try_new(
        hash,
        hash,
        &value,
        &command,
        &context,
        LawDecisionView::Accept {
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
    )
    .unwrap();
    let limits = LawLimits {
        max_observations: 3,
        ..LawLimits::default()
    };
    assert_eq!(
        VaultLaws::try_new().unwrap().evaluate(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}
