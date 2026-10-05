//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use compliance_gateway::{context, generated::*, laws::GatewayLaws, profile, reinstate, screen};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::{Field, Value};

fn standing(strikes: i128) -> Standing {
    Standing {
        strikes: Strikes(strikes),
    }
}

fn ticket(destination: &str, rule: RuleId, amount_band: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        300,
        ReviewDesk(destination.into()).to_value().unwrap(),
        ReviewTicket {
            ticket_rule: rule,
            ticket_amount_band: AmountBand(amount_band),
        }
        .to_value()
        .unwrap(),
    )
}

fn alert(destination: &str, rule: RuleId, strikes: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        301,
        ComplianceDesk(destination.into()).to_value().unwrap(),
        BlockAlert {
            alert_rule: rule,
            alert_strikes: Strikes(strikes),
        }
        .to_value()
        .unwrap(),
    )
}

/// A committing decision, as the checker sees it.
struct Decided {
    pre: Standing,
    post: Standing,
    command: GatewayCommand,
    identity_tier: i128,
    reviewer: bool,
    notices: Vec<OutboxEntry>,
    failure: Option<u32>,
    effect: bool,
}

fn holds(decided: Decided) -> bool {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = decided.pre.to_value().unwrap();
    let post = decided.post.to_value().unwrap();
    let command = decided.command.to_value().unwrap();
    let context = context(decided.identity_tier, decided.reviewer)
        .to_value()
        .unwrap();
    let outbox = OutboxPlan::try_new(decided.notices).unwrap();
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
    GatewayLaws::try_new()
        .unwrap()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

/// A screening from `pre` strikes by a customer of `identity_tier`.
fn screening(
    pre: i128,
    post: i128,
    command: GatewayCommand,
    identity_tier: i128,
    notices: Vec<OutboxEntry>,
    failure: Option<u32>,
) -> Decided {
    Decided {
        pre: standing(pre),
        post: standing(post),
        command,
        identity_tier,
        reviewer: false,
        notices,
        failure,
        effect: false,
    }
}

#[test]
fn allowed_transfers_change_nothing_and_queue_nothing() {
    // A verified customer, an allowed region, band 1, low risk: default_allow.
    let allowed = || screen(Region::Allowed, 1, CounterpartyRisk::Low);
    assert!(holds(screening(1, 1, allowed(), 2, vec![], None)));
    // The strikes must not move either way.
    assert!(!holds(screening(1, 0, allowed(), 2, vec![], None)));
    assert!(!holds(screening(1, 2, allowed(), 2, vec![], None)));
    // A stray ticket, a stray alert, or an effect is refused.
    assert!(!holds(screening(
        1,
        1,
        allowed(),
        2,
        vec![ticket("review-queue", RuleId::DefaultAllow, 1)],
        None
    )));
    assert!(!holds(screening(
        1,
        1,
        allowed(),
        2,
        vec![alert("compliance-team", RuleId::DefaultAllow, 1)],
        None
    )));
    let mut effect = screening(1, 1, allowed(), 2, vec![], None);
    effect.effect = true;
    assert!(!holds(effect));
    // Blocking an allowed transfer is refused, however it is dressed.
    assert!(!holds(screening(
        1,
        2,
        allowed(),
        2,
        vec![alert("compliance-team", RuleId::DefaultAllow, 2)],
        Some(210)
    )));
}

#[test]
fn held_transfers_must_queue_exactly_their_ticket() {
    // A verified customer, a restricted region, band 2, low risk:
    // restricted_region holds it.
    let held = || screen(Region::Restricted, 2, CounterpartyRisk::Low);
    let right = || vec![ticket("review-queue", RuleId::RestrictedRegion, 2)];
    assert!(holds(screening(1, 1, held(), 2, right(), None)));
    assert!(!holds(screening(1, 1, held(), 2, vec![], None)));
    // The wrong rule, the wrong band, the wrong desk, or two tickets.
    assert!(!holds(screening(
        1,
        1,
        held(),
        2,
        vec![ticket("review-queue", RuleId::HighRiskCounterparty, 2)],
        None
    )));
    assert!(!holds(screening(
        1,
        1,
        held(),
        2,
        vec![ticket("review-queue", RuleId::RestrictedRegion, 1)],
        None
    )));
    assert!(!holds(screening(
        1,
        1,
        held(),
        2,
        vec![ticket("compliance-team", RuleId::RestrictedRegion, 2)],
        None
    )));
    let mut two = right();
    two.push(OutboxEntry::new(
        1,
        300,
        ReviewDesk("review-queue".into()).to_value().unwrap(),
        ReviewTicket {
            ticket_rule: RuleId::RestrictedRegion,
            ticket_amount_band: AmountBand(2),
        }
        .to_value()
        .unwrap(),
    ));
    assert!(!holds(screening(1, 1, held(), 2, two, None)));
    // An alert instead of a ticket, a strike added, or a failure recorded.
    assert!(!holds(screening(
        1,
        1,
        held(),
        2,
        vec![alert("compliance-team", RuleId::RestrictedRegion, 1)],
        None
    )));
    assert!(!holds(screening(1, 2, held(), 2, right(), None)));
    assert!(!holds(screening(1, 2, held(), 2, right(), Some(213))));
    // Allowing a held transfer is refused.
    assert!(!holds(screening(1, 1, held(), 2, vec![], None)));
}

#[test]
fn blocked_transfers_must_add_a_strike_and_alert_under_their_rule() {
    // Any customer, a sanctioned region: sanctioned_region blocks it.
    let sanctioned = || screen(Region::Sanctioned, 0, CounterpartyRisk::Low);
    let right = |strikes| vec![alert("compliance-team", RuleId::SanctionedRegion, strikes)];
    assert!(holds(screening(1, 2, sanctioned(), 3, right(2), Some(210))));
    // Another reason, no reason, or no strike.
    assert!(!holds(screening(
        1,
        2,
        sanctioned(),
        3,
        right(2),
        Some(211)
    )));
    assert!(!holds(screening(1, 2, sanctioned(), 3, right(2), None)));
    assert!(!holds(screening(
        1,
        1,
        sanctioned(),
        3,
        right(1),
        Some(210)
    )));
    assert!(!holds(screening(
        1,
        3,
        sanctioned(),
        3,
        right(3),
        Some(210)
    )));
    // The alert must name the rule and the strikes now on record.
    assert!(!holds(screening(1, 2, sanctioned(), 3, vec![], Some(210))));
    assert!(!holds(screening(
        1,
        2,
        sanctioned(),
        3,
        right(1),
        Some(210)
    )));
    assert!(!holds(screening(
        1,
        2,
        sanctioned(),
        3,
        vec![alert("compliance-team", RuleId::FrozenAccount, 2)],
        Some(210)
    )));
    assert!(!holds(screening(
        1,
        2,
        sanctioned(),
        3,
        vec![alert("review-queue", RuleId::SanctionedRegion, 2)],
        Some(210)
    )));
    assert!(!holds(screening(
        1,
        2,
        sanctioned(),
        3,
        vec![ticket("review-queue", RuleId::SanctionedRegion, 0)],
        Some(210)
    )));
    // A frozen account stays at three strikes, under frozen_account, unless
    // a higher rule names the block.
    let small = || screen(Region::Allowed, 0, CounterpartyRisk::Low);
    let frozen = |strikes| vec![alert("compliance-team", RuleId::FrozenAccount, strikes)];
    assert!(holds(screening(3, 3, small(), 3, frozen(3), Some(211))));
    assert!(!holds(screening(3, 2, small(), 3, frozen(2), Some(211))));
    assert!(!holds(screening(3, 3, small(), 3, frozen(3), Some(210))));
    assert!(holds(screening(3, 3, sanctioned(), 3, right(3), Some(210))));
    // The rule that fires is the highest in priority: an unverified customer
    // sending a large transfer to a restricted region with a high-risk
    // counterparty is blocked under unverified_high_risk, not restricted_large.
    let contested = || screen(Region::Restricted, 3, CounterpartyRisk::High);
    assert!(holds(screening(
        0,
        1,
        contested(),
        0,
        vec![alert("compliance-team", RuleId::UnverifiedHighRisk, 1)],
        Some(212)
    )));
    assert!(!holds(screening(
        0,
        1,
        contested(),
        0,
        vec![alert("compliance-team", RuleId::RestrictedLarge, 1)],
        Some(213)
    )));
    // Accepting a blocked transfer is refused, with or without a ticket.
    assert!(!holds(screening(1, 1, sanctioned(), 3, vec![], None)));
    assert!(!holds(screening(
        1,
        1,
        sanctioned(),
        3,
        vec![ticket("review-queue", RuleId::SanctionedRegion, 0)],
        None
    )));
}

fn reinstatement(pre: i128, post: i128, reviewer: bool) -> Decided {
    Decided {
        pre: standing(pre),
        post: standing(post),
        command: reinstate(),
        identity_tier: 3,
        reviewer,
        notices: vec![],
        failure: None,
        effect: false,
    }
}

#[test]
fn reinstatements_are_checked() {
    assert!(holds(reinstatement(2, 0, true)));
    assert!(holds(reinstatement(3, 0, true)));
    assert!(!holds(reinstatement(2, 0, false)));
    assert!(!holds(reinstatement(0, 0, true)));
    assert!(!holds(reinstatement(2, 1, true)));
    assert!(!holds(reinstatement(2, 2, true)));
    // A reinstatement queues nothing and never fails.
    let mut noisy = reinstatement(2, 0, true);
    noisy.notices = vec![alert("compliance-team", RuleId::FrozenAccount, 0)];
    assert!(!holds(noisy));
    let mut failed = reinstatement(2, 0, true);
    failed.failure = Some(211);
    assert!(!holds(failed));
    let mut effect = reinstatement(2, 0, true);
    effect.effect = true;
    assert!(!holds(effect));
}

#[test]
fn rejections_must_carry_the_rule_that_applies() {
    let hash = profile::digest("example/test/reject", b"independent decision view");
    let cases = [
        (2, reinstate(), false, 200, true),
        (2, reinstate(), false, 201, false),
        (0, reinstate(), true, 201, true),
        (0, reinstate(), true, 200, false),
        // Without a reviewer, the reviewer rule applies first.
        (0, reinstate(), false, 200, true),
        (0, reinstate(), false, 201, false),
        (2, reinstate(), true, 200, false),
        // A screening is never rejected: it is allowed, held, or blocked.
        (
            0,
            screen(Region::Sanctioned, 4, CounterpartyRisk::High),
            false,
            210,
            false,
        ),
        (
            3,
            screen(Region::Allowed, 0, CounterpartyRisk::Low),
            true,
            200,
            false,
        ),
    ];
    for (strikes, command, reviewer, reason_id, right) in cases {
        let pre = standing(strikes).to_value().unwrap();
        let command = command.to_value().unwrap();
        let context = context(3, reviewer).to_value().unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = GatewayLaws::try_new()
            .unwrap()
            .evaluate(&input, LawLimits::default());
        assert_eq!(
            result.is_ok(),
            right,
            "reason {reason_id} from {strikes} strikes, reviewer {reviewer}: {result:?}"
        );
    }
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    for (state, expected) in [(standing(0), true), (standing(1), false)] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = GatewayLaws::try_new()
            .unwrap()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected
        );
    }
    let value = standing(0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        GatewayLaws::try_new()
            .unwrap()
            .evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}

/// A standing as a raw record at field 120, which unlike the typed binding
/// can hold a value the schema never admits.
fn raw_standing(strikes: i128) -> Value {
    Value::record_canonical(vec![Field::new(120, Value::I128(strikes))]).unwrap()
}

#[test]
fn strikes_outside_their_bounds_are_refused_before_any_law_is_evaluated() {
    // The schema never admits four strikes, and the typed bindings refuse to
    // encode them. A raw decision that carries them is refused outright:
    // the checker does not evaluate a law over a standing it cannot read.
    let hash = profile::digest("example/test/bounds", b"independent decision view");
    let laws = GatewayLaws::try_new().unwrap();
    let command = screen(Region::Sanctioned, 0, CounterpartyRisk::Low)
        .to_value()
        .unwrap();
    let context = context(3, false).to_value().unwrap();
    let outbox =
        OutboxPlan::try_new(vec![alert("compliance-team", RuleId::SanctionedRegion, 3)]).unwrap();
    let commit = CommitPlan::try_new(vec![]).unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let evaluate = |pre: Value, post: Value| {
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::CommittedFailure {
                reason_id: 210,
                post_state: &post,
                patch: &patch,
                commit_plan: &commit,
                outbox_plan: &outbox,
            },
        )
        .unwrap();
        laws.evaluate(&input, LawLimits::default())
    };
    // The block itself is right: two strikes become three.
    let right = evaluate(raw_standing(2), raw_standing(3)).unwrap();
    assert!(
        right
            .iter()
            .all(|observation| observation.status() == LawStatus::Satisfied)
    );
    // Three becoming four, or a negative count, is refused before evaluation.
    for (pre, post) in [(3, 4), (0, -1), (4, 4)] {
        assert_eq!(
            evaluate(raw_standing(pre), raw_standing(post)),
            Err(LawEngineFailure::Unsupported),
            "{pre} strikes becoming {post}"
        );
    }
}
