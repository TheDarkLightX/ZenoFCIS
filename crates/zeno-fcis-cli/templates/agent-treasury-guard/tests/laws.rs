//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use agent_treasury_guard::{
    context, failed,
    generated::*,
    laws::{GuardLaws, trace_step},
    profile, propose, settled, treasury,
};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_spec::{
    EvalLimits, EvalOutcome, EvaluationContext, Identifier, PredicateProvider, evaluate_relational,
};
use zeno_fcis_value::Value;

use Caller::{Agent, Dex};
use Direction::{BuyBase, SellBase};
use ModelId::{TreasuryAgentV1 as V1, TreasuryAgentV2 as V2};
use PendingSwap::{NoSwap, PendingBuy, PendingSell};

fn request(
    destination: &str,
    intent_number: i128,
    direction: &Direction,
    amount_in: i128,
    min_amount_out: i128,
    deadline: i128,
) -> OutboxEntry {
    let (asset_in, asset_out) = match direction {
        BuyBase => (Asset::Quote, Asset::Base),
        SellBase => (Asset::Base, Asset::Quote),
    };
    OutboxEntry::new(
        0,
        300,
        DexDestination(destination.into()).to_value().unwrap(),
        SwapRequest {
            intent_number: Tick(intent_number),
            asset_in,
            asset_out,
            amount_in: TradeAmount(amount_in),
            min_amount_out: MinOut(min_amount_out),
            deadline: Deadline(deadline),
        }
        .to_value()
        .unwrap(),
    )
}

/// The same request queued again, at the next ordinal.
fn second_copy(entry: &OutboxEntry) -> OutboxEntry {
    OutboxEntry::new(
        1,
        entry.channel(),
        entry.destination().clone(),
        entry.payload().clone(),
    )
}

/// A committing decision, as the checker sees it.
struct Decided {
    pre: Treasury,
    post: Treasury,
    command: TreasuryCommand,
    context: GuardContext,
    requests: Vec<OutboxEntry>,
    failure: Option<u32>,
    effect: bool,
}

fn decided(
    pre: Treasury,
    post: Treasury,
    command: TreasuryCommand,
    context: GuardContext,
    requests: Vec<OutboxEntry>,
) -> Decided {
    Decided {
        pre,
        post,
        command,
        context,
        requests,
        failure: None,
        effect: false,
    }
}

fn holds(decided: Decided) -> bool {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = decided.pre.to_value().unwrap();
    let post = decided.post.to_value().unwrap();
    let command = decided.command.to_value().unwrap();
    let context = decided.context.to_value().unwrap();
    let outbox = OutboxPlan::try_new(decided.requests).unwrap();
    let commit = CommitPlan::try_new(if decided.effect {
        vec![Effect::new(0, 999, hash, hash, Value::I128(0))]
    } else {
        vec![]
    })
    .unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let decision = match decided.failure {
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
    GuardLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

#[test]
fn a_buy_must_debit_quote_and_queue_exactly_the_proposed_swap() {
    // From genesis at tick 1: buy 2 base for 2 quote at price 1, for at least
    // 2 base, valid until tick 3.
    let buy = |post, requests| {
        decided(
            treasury(6, 1, 0, 0, NoSwap, 0, 0),
            post,
            propose(BuyBase, 2, 2),
            context(Agent, 1, 1, 1, V2),
            requests,
        )
    };
    let right = treasury(4, 1, 2, 1, PendingBuy, 2, 2);
    let queued = || vec![request("zenodex", 1, &BuyBase, 2, 2, 3)];
    assert!(holds(buy(right.clone(), queued())));
    // The debit, the budget, the tick, and the held swap must all be exact.
    assert!(!holds(buy(
        treasury(5, 1, 2, 1, PendingBuy, 2, 2),
        queued()
    )));
    assert!(!holds(buy(
        treasury(4, 2, 2, 1, PendingBuy, 2, 2),
        queued()
    )));
    assert!(!holds(buy(
        treasury(4, 1, 1, 1, PendingBuy, 2, 2),
        queued()
    )));
    assert!(!holds(buy(
        treasury(4, 1, 2, 0, PendingBuy, 2, 2),
        queued()
    )));
    assert!(!holds(buy(treasury(4, 1, 2, 1, NoSwap, 2, 2), queued())));
    assert!(!holds(buy(
        treasury(4, 1, 2, 1, PendingSell, 2, 2),
        queued()
    )));
    assert!(!holds(buy(
        treasury(4, 1, 2, 1, PendingBuy, 1, 2),
        queued()
    )));
    assert!(!holds(buy(
        treasury(4, 1, 2, 1, PendingBuy, 2, 1),
        queued()
    )));
    // The request must be queued once, to the DEX, with the swap as proposed.
    assert!(!holds(buy(right.clone(), vec![])));
    assert!(!holds(buy(
        right.clone(),
        vec![request("someone-else", 1, &BuyBase, 2, 2, 3)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![request("zenodex", 2, &BuyBase, 2, 2, 3)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![request("zenodex", 1, &SellBase, 2, 2, 3)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![request("zenodex", 1, &BuyBase, 1, 2, 3)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![request("zenodex", 1, &BuyBase, 2, 1, 3)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![request("zenodex", 1, &BuyBase, 2, 2, 4)]
    )));
    assert!(!holds(buy(
        right.clone(),
        vec![
            request("zenodex", 1, &BuyBase, 2, 2, 3),
            second_copy(&request("zenodex", 1, &BuyBase, 2, 2, 3)),
        ]
    )));
    let mut effect = buy(right, queued());
    effect.effect = true;
    assert!(!holds(effect));
}

#[test]
fn a_proposal_is_accepted_only_under_the_readme_rules() {
    let accepted = |pre: Treasury, command: TreasuryCommand, context: GuardContext| {
        // The post-state a correct guard would produce for this buy at tick 1.
        let post = treasury(pre.quote.0 - 2, pre.base.0, 2, 1, PendingBuy, 2, 2);
        decided(
            pre,
            post,
            command,
            context,
            vec![request("zenodex", 1, &BuyBase, 2, 2, 3)],
        )
    };
    let idle = || treasury(6, 1, 0, 0, NoSwap, 0, 0);
    assert!(holds(accepted(
        idle(),
        propose(BuyBase, 2, 2),
        context(Agent, 1, 1, 1, V2)
    )));
    // Only the agent, under the approved model, on a fresh price, with no
    // swap outstanding.
    assert!(!holds(accepted(
        idle(),
        propose(BuyBase, 2, 2),
        context(Dex, 1, 1, 1, V2)
    )));
    assert!(!holds(accepted(
        idle(),
        propose(BuyBase, 2, 2),
        context(Agent, 1, 1, 1, V1)
    )));
    assert!(!holds(accepted(
        idle(),
        propose(BuyBase, 2, 2),
        context(Agent, 3, 1, 1, V2)
    )));
    assert!(!holds(accepted(
        treasury(6, 1, 0, 0, PendingSell, 1, 1),
        propose(BuyBase, 2, 2),
        context(Agent, 1, 1, 1, V2)
    )));
    // The clock must advance.
    assert!(!holds(accepted(
        treasury(6, 1, 0, 1, NoSwap, 0, 0),
        propose(BuyBase, 2, 2),
        context(Agent, 1, 1, 1, V2)
    )));
    // A buy of 2 at price 1 needs at least 2 back; at price 2, at least 1.
    assert!(!holds(decided(
        idle(),
        treasury(4, 1, 2, 1, PendingBuy, 2, 1),
        propose(BuyBase, 2, 1),
        context(Agent, 1, 1, 1, V2),
        vec![request("zenodex", 1, &BuyBase, 2, 1, 3)],
    )));
    assert!(holds(decided(
        idle(),
        treasury(4, 1, 2, 1, PendingBuy, 2, 1),
        propose(BuyBase, 2, 1),
        context(Agent, 1, 2, 1, V2),
        vec![request("zenodex", 1, &BuyBase, 2, 1, 3)],
    )));
    // The reserve: a buy of 3 from 4 quote would leave 1.
    assert!(!holds(decided(
        treasury(4, 1, 0, 0, NoSwap, 0, 0),
        treasury(1, 1, 3, 1, PendingBuy, 3, 3),
        propose(BuyBase, 3, 3),
        context(Agent, 1, 1, 1, V2),
        vec![request("zenodex", 1, &BuyBase, 3, 3, 3)],
    )));
    // The budget: 2 already spent today plus 3 exceeds 4.
    assert!(!holds(decided(
        treasury(6, 1, 2, 0, NoSwap, 0, 0),
        treasury(3, 1, 4, 1, PendingBuy, 3, 3),
        propose(BuyBase, 3, 3),
        context(Agent, 1, 1, 1, V2),
        vec![request("zenodex", 1, &BuyBase, 3, 3, 3)],
    )));
}

#[test]
fn a_sell_is_valued_at_the_oracle_price() {
    // Sell 1 base at price 2: 2 quote of value, at least 2 quote back.
    let sell = |post, min_out, price| {
        decided(
            treasury(4, 3, 2, 2, NoSwap, 0, 0),
            post,
            propose(SellBase, 1, min_out),
            context(Agent, 3, price, 3, V2),
            vec![request("zenodex", 3, &SellBase, 1, min_out, 5)],
        )
    };
    assert!(holds(sell(treasury(4, 2, 4, 3, PendingSell, 1, 2), 2, 2)));
    // The budget must count the value, not the amount.
    assert!(!holds(sell(treasury(4, 2, 3, 3, PendingSell, 1, 2), 2, 2)));
    // A minimum of 1 keeps less than three quarters of 2.
    assert!(!holds(sell(treasury(4, 2, 4, 3, PendingSell, 1, 1), 1, 2)));
    // At price 1 the same sell is worth 1 and needs 1 back.
    assert!(holds(sell(treasury(4, 2, 3, 3, PendingSell, 1, 1), 1, 1)));
    // Sell 2 at price 2: 4 of value, over the cap of 3.
    assert!(!holds(decided(
        treasury(6, 3, 0, 2, NoSwap, 0, 0),
        treasury(6, 1, 4, 3, PendingSell, 2, 3),
        propose(SellBase, 2, 3),
        context(Agent, 3, 2, 3, V2),
        vec![request("zenodex", 3, &SellBase, 2, 3, 5)],
    )));
    // A sell debits base, never quote.
    assert!(!holds(decided(
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        treasury(3, 3, 4, 3, PendingSell, 1, 2),
        propose(SellBase, 1, 2),
        context(Agent, 3, 2, 3, V2),
        vec![request("zenodex", 3, &SellBase, 1, 2, 5)],
    )));
}

#[test]
fn a_settlement_must_credit_exactly_and_clear_the_swap() {
    let settle = |pre, post, command, caller| {
        decided(pre, post, command, context(caller, 2, 1, 2, V2), vec![])
    };
    let pending = || treasury(4, 1, 2, 1, PendingBuy, 2, 2);
    assert!(holds(settle(
        pending(),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        Dex
    )));
    // Exactly `amount_out`, to the bought asset, from the DEX, for the
    // outstanding intent, delivering at least the minimum.
    assert!(!holds(settle(
        pending(),
        treasury(4, 2, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        Dex
    )));
    assert!(!holds(settle(
        pending(),
        treasury(6, 1, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        Dex
    )));
    assert!(!holds(settle(
        pending(),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        Agent
    )));
    assert!(!holds(settle(
        pending(),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        settled(0, 2),
        Dex
    )));
    assert!(!holds(settle(
        pending(),
        treasury(4, 2, 2, 2, NoSwap, 0, 0),
        settled(1, 1),
        Dex
    )));
    // The swap must be cleared, and the tick recorded.
    assert!(!holds(settle(
        pending(),
        treasury(4, 3, 2, 2, PendingBuy, 2, 2),
        settled(1, 2),
        Dex
    )));
    assert!(!holds(settle(
        pending(),
        treasury(4, 3, 2, 1, NoSwap, 0, 0),
        settled(1, 2),
        Dex
    )));
    // Nothing to settle.
    assert!(!holds(settle(
        treasury(4, 3, 2, 1, NoSwap, 0, 0),
        treasury(4, 5, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        Dex
    )));
    // A settlement queues nothing.
    assert!(!holds(decided(
        pending(),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        context(Dex, 2, 1, 2, V2),
        vec![request("zenodex", 2, &BuyBase, 2, 2, 4)],
    )));
    // A sell settles into quote.
    assert!(holds(decided(
        treasury(6, 0, 2, 1, PendingSell, 1, 2),
        treasury(9, 0, 2, 2, NoSwap, 0, 0),
        settled(1, 3),
        context(Dex, 2, 2, 2, V2),
        vec![],
    )));
    assert!(!holds(decided(
        treasury(6, 0, 2, 1, PendingSell, 1, 2),
        treasury(6, 3, 2, 2, NoSwap, 0, 0),
        settled(1, 3),
        context(Dex, 2, 2, 2, V2),
        vec![],
    )));
}

#[test]
fn the_day_accounting_is_checked_on_every_commit() {
    let settle_at = |now, post| {
        decided(
            treasury(4, 2, 4, 3, PendingSell, 1, 2),
            post,
            settled(3, 2),
            context(Dex, now, 1, now, V2),
            vec![],
        )
    };
    // Tick 4 starts day 1: the budget restarts at 0.
    assert!(holds(settle_at(4, treasury(6, 2, 0, 4, NoSwap, 0, 0))));
    assert!(!holds(settle_at(4, treasury(6, 2, 4, 4, NoSwap, 0, 0))));
    // At tick 3 the day has not changed, but the clock has not advanced.
    assert!(!holds(settle_at(3, treasury(6, 2, 4, 3, NoSwap, 0, 0))));
    // A settlement on the same day keeps the budget.
    assert!(holds(decided(
        treasury(4, 1, 2, 1, PendingBuy, 2, 2),
        treasury(4, 3, 2, 2, NoSwap, 0, 0),
        settled(1, 2),
        context(Dex, 2, 1, 2, V2),
        vec![],
    )));
    assert!(!holds(decided(
        treasury(4, 1, 2, 1, PendingBuy, 2, 2),
        treasury(4, 3, 0, 2, NoSwap, 0, 0),
        settled(1, 2),
        context(Dex, 2, 1, 2, V2),
        vec![],
    )));
}

#[test]
fn a_failure_must_refund_exactly_and_keep_the_budget_committed() {
    let fail = |pre, post, command, caller, now| {
        let mut decision = decided(pre, post, command, context(caller, now, 1, now, V2), vec![]);
        decision.failure = Some(212);
        decision
    };
    let pending = || treasury(2, 3, 2, 5, PendingBuy, 2, 2);
    assert!(holds(fail(
        pending(),
        treasury(4, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6
    )));
    // The refund goes to the asset the swap took from, exactly; the budget is
    // not restored.
    assert!(!holds(fail(
        pending(),
        treasury(2, 5, 2, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6
    )));
    assert!(!holds(fail(
        pending(),
        treasury(3, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6
    )));
    assert!(!holds(fail(
        pending(),
        treasury(4, 3, 0, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6
    )));
    // Only the DEX reports a failure, and only of the outstanding intent.
    assert!(!holds(fail(
        pending(),
        treasury(4, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        Agent,
        6
    )));
    assert!(!holds(fail(
        pending(),
        treasury(4, 3, 2, 6, NoSwap, 0, 0),
        failed(4),
        Dex,
        6
    )));
    // A failed sell refunds base, and a new day restarts the budget.
    assert!(holds(fail(
        treasury(4, 2, 4, 3, PendingSell, 1, 2),
        treasury(4, 3, 0, 4, NoSwap, 0, 0),
        failed(3),
        Dex,
        4
    )));
    // A failure is never accepted, and carries its own reason.
    let mut accepted = fail(
        pending(),
        treasury(4, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6,
    );
    accepted.failure = None;
    assert!(!holds(accepted));
    let mut wrong_reason = fail(
        pending(),
        treasury(4, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        Dex,
        6,
    );
    wrong_reason.failure = Some(211);
    assert!(!holds(wrong_reason));
    // A settlement is never a committed failure.
    let mut settlement = fail(
        pending(),
        treasury(2, 5, 2, 6, NoSwap, 0, 0),
        settled(5, 2),
        Dex,
        6,
    );
    settlement.failure = Some(212);
    assert!(!holds(settlement));
}

#[test]
fn rejections_must_carry_the_rule_that_applies() {
    let hash = profile::digest("example/test/reject", b"independent decision view");
    let idle = || treasury(4, 3, 2, 2, NoSwap, 0, 0);
    let pending = || treasury(4, 1, 2, 1, PendingBuy, 2, 2);
    let cases = [
        (
            pending(),
            settled(1, 2),
            context(Agent, 2, 1, 2, V2),
            200,
            true,
        ),
        (
            pending(),
            settled(1, 2),
            context(Agent, 2, 1, 2, V2),
            205,
            false,
        ),
        (
            idle(),
            propose(BuyBase, 1, 1),
            context(Agent, 2, 1, 2, V2),
            201,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 1, 1),
            context(Agent, 3, 1, 3, V1),
            202,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 1, 1),
            context(Agent, 3, 1, 1, V2),
            203,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 1, 1),
            context(Agent, 3, 1, 2, V2),
            203,
            false,
        ),
        (
            pending(),
            propose(BuyBase, 1, 1),
            context(Agent, 2, 1, 2, V2),
            204,
            true,
        ),
        (idle(), settled(2, 2), context(Dex, 3, 1, 3, V2), 205, true),
        (
            pending(),
            settled(0, 2),
            context(Dex, 2, 1, 2, V2),
            206,
            true,
        ),
        (
            pending(),
            settled(1, 1),
            context(Dex, 2, 1, 2, V2),
            207,
            true,
        ),
        (
            pending(),
            settled(1, 2),
            context(Dex, 2, 1, 2, V2),
            207,
            false,
        ),
        (
            idle(),
            propose(SellBase, 2, 3),
            context(Agent, 3, 2, 3, V2),
            208,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 3, 3),
            context(Agent, 3, 1, 3, V2),
            209,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 3, 3),
            context(Agent, 3, 1, 3, V2),
            208,
            false,
        ),
        (
            idle(),
            propose(BuyBase, 2, 1),
            context(Agent, 3, 1, 3, V2),
            210,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 3, 3),
            context(Agent, 5, 1, 5, V2),
            211,
            true,
        ),
        (
            idle(),
            propose(BuyBase, 2, 2),
            context(Agent, 5, 1, 5, V2),
            211,
            false,
        ),
    ];
    for (pre, command, ctx, reason_id, right) in cases {
        let pre = pre.to_value().unwrap();
        let command = command.to_value().unwrap();
        let ctx = ctx.to_value().unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &ctx,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = GuardLaws::default().evaluate(&input, LawLimits::default());
        assert_eq!(result.is_ok(), right, "reason {reason_id}: {result:?}");
    }
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    for (state, expected) in [
        (treasury(6, 1, 0, 0, NoSwap, 0, 0), true),
        (treasury(6, 1, 0, 1, NoSwap, 0, 0), false),
        (treasury(5, 1, 0, 0, NoSwap, 0, 0), false),
        (treasury(6, 1, 0, 0, PendingBuy, 1, 1), false),
    ] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = GuardLaws::default()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected
        );
    }
    let value = treasury(6, 1, 0, 0, NoSwap, 0, 0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        GuardLaws::default().evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}

/// The laws the checker reports violated for a committing decision.
fn violated(decided: Decided) -> Vec<u32> {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = decided.pre.to_value().unwrap();
    let post = decided.post.to_value().unwrap();
    let command = decided.command.to_value().unwrap();
    let context = decided.context.to_value().unwrap();
    let outbox = OutboxPlan::try_new(decided.requests).unwrap();
    let commit = CommitPlan::try_new(vec![]).unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let decision = match decided.failure {
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
    GuardLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .filter(|observation| observation.status() == LawStatus::Violated)
        .map(|observation| observation.law_id().get())
        .collect()
}

#[test]
fn each_law_refuses_a_decision_that_breaks_only_it() {
    // A correct buy from genesis at tick 1, and the request it queues.
    let buy = |post| {
        decided(
            treasury(6, 1, 0, 0, NoSwap, 0, 0),
            post,
            propose(BuyBase, 2, 2),
            context(Agent, 1, 1, 1, V2),
            vec![request("zenodex", 1, &BuyBase, 2, 2, 3)],
        )
    };
    assert_eq!(violated(buy(treasury(4, 1, 2, 1, PendingBuy, 2, 2))), []);
    // 500: the post-state breaks the reserve, and nothing else is wrong with
    // the decision that a formula can see.
    assert_eq!(
        violated(decided(
            treasury(3, 1, 0, 0, NoSwap, 0, 0),
            treasury(1, 1, 2, 1, PendingBuy, 2, 2),
            propose(BuyBase, 2, 2),
            context(Agent, 1, 1, 1, V2),
            vec![request("zenodex", 1, &BuyBase, 2, 2, 3)],
        )),
        [500, 501, 502, 503, 504, 505, 506, 507]
    );
    // 501: the tick is not recorded.
    assert_eq!(violated(buy(treasury(4, 1, 2, 0, PendingBuy, 2, 2))), [501]);
    // 502: the budget does not count the buy.
    assert_eq!(violated(buy(treasury(4, 1, 0, 1, PendingBuy, 2, 2))), [502]);
    // 503: an accepted proposal that leaves no swap pending; 500 and 506 see
    // the same field.
    assert_eq!(
        violated(buy(treasury(4, 1, 2, 1, NoSwap, 2, 2))),
        [500, 503, 506]
    );
    // 504: a settlement credits one base too many.
    assert_eq!(
        violated(decided(
            treasury(4, 1, 2, 1, PendingBuy, 2, 2),
            treasury(4, 4, 2, 2, NoSwap, 0, 0),
            settled(1, 2),
            context(Dex, 2, 1, 2, V2),
            vec![],
        )),
        [504]
    );
    // 505: the buy debits one quote too few.
    assert_eq!(violated(buy(treasury(5, 1, 2, 1, PendingBuy, 2, 2))), [505]);
    // 506: the recorded minimum is not the proposed one; 507 still holds
    // because 3 keeps three quarters too.
    assert_eq!(violated(buy(treasury(4, 1, 2, 1, PendingBuy, 2, 3))), [506]);
    // 507: the recorded minimum keeps less than three quarters. It differs
    // from the proposed minimum, which 506 sees; a proposed minimum that low
    // is a rejection, which every relation then refuses.
    assert_eq!(
        violated(buy(treasury(4, 1, 2, 1, PendingBuy, 2, 1))),
        [506, 507]
    );
    // 508: a failure refunds one quote too few.
    let mut short_refund = decided(
        treasury(2, 3, 2, 5, PendingBuy, 2, 2),
        treasury(3, 3, 2, 6, NoSwap, 0, 0),
        failed(5),
        context(Dex, 6, 1, 6, V2),
        vec![],
    );
    short_refund.failure = Some(212);
    assert_eq!(violated(short_refund), [508]);
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

/// Evaluates one law's formula, as written in `project.zeno`, on a decision,
/// through the law checker's own observer.
fn formula(
    law_id: u32,
    pre: &Treasury,
    post: &Treasury,
    command: &TreasuryCommand,
    context: &GuardContext,
) -> EvalOutcome {
    let project = profile::project();
    let law = project
        .laws()
        .iter()
        .find(|law| law.id().get() == law_id)
        .unwrap_or_else(|| panic!("law {law_id} is declared"));
    let step = trace_step(pre, post, command, context).unwrap();
    evaluate_relational(
        law.formula(),
        EvaluationContext::new(&step, &NoPredicates, EvalLimits::default()),
    )
}

#[test]
fn each_counterexample_to_induction_is_refused_by_the_law_that_now_states_the_rule() {
    // While claim 600 was being stated, CVC5 found six transitions that
    // satisfied every formula as then written and left law 500's invariant
    // (see the README). Each is a transition the README's rules forbid and
    // the Rust part of the checker refused, but no formula stated. The
    // formulas now refuse them.
    let at = |now, price| context(Agent, now, price, now, V2);
    let idle = |quote, base, spent| treasury(quote, base, spent, 0, NoSwap, 0, 0);
    // 1. A buy of 0 leaves a pending buy holding 0: a proposal sells at
    //    least 1 unit.
    assert_eq!(
        formula(
            505,
            &idle(2, 0, 0),
            &treasury(2, 0, 0, 1, PendingBuy, 0, 0),
            &propose(BuyBase, 0, 0),
            &at(1, 1)
        ),
        EvalOutcome::False
    );
    // 2. A buy of 1 from 2 quote leaves 1: a buy keeps the reserve.
    assert_eq!(
        formula(
            505,
            &idle(2, 0, 0),
            &treasury(1, 0, 1, 1, PendingBuy, 1, 1),
            &propose(BuyBase, 1, 1),
            &at(1, 1)
        ),
        EvalOutcome::False
    );
    // 3. A buy of 1 with 4 already committed takes the day to 5: a proposal
    //    fits the budget.
    assert_eq!(
        formula(
            507,
            &idle(3, 0, 4),
            &treasury(2, 0, 5, 1, PendingBuy, 1, 1),
            &propose(BuyBase, 1, 1),
            &at(1, 1)
        ),
        EvalOutcome::False
    );
    // 4. A sell of 1 base from 0 leaves -1: a sell keeps a non-negative base.
    assert_eq!(
        formula(
            505,
            &idle(2, 0, 0),
            &treasury(2, -1, 1, 1, PendingSell, 1, 1),
            &propose(SellBase, 1, 1),
            &at(1, 1)
        ),
        EvalOutcome::False
    );
    // 5. A sell at price -1 takes the day's value to -1: a proposal is
    //    valued at a positive price.
    assert_eq!(
        formula(
            507,
            &idle(2, 1, 0),
            &treasury(2, 0, -1, 1, PendingSell, 1, 1),
            &propose(SellBase, 1, 1),
            &at(1, -1)
        ),
        EvalOutcome::False
    );
    // 6. A pending buy holding a minimum of -1 would let a settlement of -1
    //    base through law 504: law 500 refuses the state that holds it.
    assert_eq!(
        formula(
            500,
            &idle(2, 0, 4),
            &treasury(2, 0, 4, 1, PendingBuy, 1, -1),
            &propose(BuyBase, 1, -1),
            &at(1, 1)
        ),
        EvalOutcome::False
    );
    // The same decisions with the rule kept satisfy the strengthened law.
    assert_eq!(
        formula(
            505,
            &idle(6, 1, 0),
            &treasury(4, 1, 2, 1, PendingBuy, 2, 2),
            &propose(BuyBase, 2, 2),
            &at(1, 1)
        ),
        EvalOutcome::True
    );
    assert_eq!(
        formula(
            507,
            &idle(6, 1, 2),
            &treasury(4, 1, 4, 1, PendingBuy, 2, 2),
            &propose(BuyBase, 2, 2),
            &at(1, 1)
        ),
        EvalOutcome::True
    );
}
