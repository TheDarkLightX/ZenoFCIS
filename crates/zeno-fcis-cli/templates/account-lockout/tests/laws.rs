//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use account_lockout::{context, generated::*, laws::AccountLaws, profile};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::Value;

fn account(failed: i128, until: i128, seen: i128) -> Account {
    Account {
        failed_attempts: Attempts(failed),
        locked_until: LockDeadline(until),
        last_seen: UnixTime(seen),
    }
}

fn alert(destination: &str, alert_kind: AlertKind, until: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        300,
        AlertDestination(destination.into()).to_value().unwrap(),
        SecurityAlert {
            alert_kind,
            alert_until: LockDeadline(until),
        }
        .to_value()
        .unwrap(),
    )
}

/// A committing decision, as the checker sees it.
struct Decided {
    pre: Account,
    post: Account,
    command: AccountCommand,
    now: i128,
    admin: bool,
    alerts: Vec<OutboxEntry>,
    failure: Option<u32>,
    effect: bool,
}

fn holds(decided: Decided) -> bool {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = decided.pre.to_value().unwrap();
    let post = decided.post.to_value().unwrap();
    let command = decided.command.to_value().unwrap();
    let context = context(decided.now, decided.admin).to_value().unwrap();
    let outbox = OutboxPlan::try_new(decided.alerts).unwrap();
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
    AccountLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

fn login(pre: Account, post: Account, now: i128) -> Decided {
    Decided {
        pre,
        post,
        command: AccountCommand::LoginSucceeded,
        now,
        admin: false,
        alerts: vec![],
        failure: None,
        effect: false,
    }
}

#[test]
fn successful_logins_are_checked() {
    assert!(holds(login(account(2, 0, 1000), account(0, 0, 1010), 1010)));
    // The count must be cleared and the time recorded.
    assert!(!holds(login(
        account(2, 0, 1000),
        account(2, 0, 1010),
        1010
    )));
    assert!(!holds(login(
        account(2, 0, 1000),
        account(0, 0, 1000),
        1010
    )));
    // A login during a lock, or with a clock moving backwards, must not commit.
    assert!(!holds(login(
        account(0, 1900, 1000),
        account(0, 1900, 1500),
        1500
    )));
    assert!(!holds(login(account(0, 0, 1000), account(0, 0, 999), 999)));
    // A stray alert or effect is refused.
    let mut noisy = login(account(0, 0, 1000), account(0, 0, 1010), 1010);
    noisy.alerts = vec![alert("security-team", AlertKind::Unlocked, 0)];
    assert!(!holds(noisy));
    let mut effect = login(account(0, 0, 1000), account(0, 0, 1010), 1010);
    effect.effect = true;
    assert!(!holds(effect));
    // A failed login can never be accepted.
    let mut accepted_failure = login(account(0, 0, 1000), account(0, 0, 1010), 1010);
    accepted_failure.command = AccountCommand::LoginFailed;
    assert!(!holds(accepted_failure));
}

fn failure(pre: Account, post: Account, now: i128, alerts: Vec<OutboxEntry>) -> Decided {
    Decided {
        pre,
        post,
        command: AccountCommand::LoginFailed,
        now,
        admin: false,
        alerts,
        failure: Some(203),
        effect: false,
    }
}

#[test]
fn failed_logins_and_locks_are_checked() {
    assert!(holds(failure(
        account(0, 0, 1000),
        account(1, 0, 1010),
        1010,
        vec![]
    )));
    assert!(!holds(failure(
        account(0, 0, 1000),
        account(0, 0, 1010),
        1010,
        vec![]
    )));
    let lock = || vec![alert("security-team", AlertKind::Locked, 1910)];
    assert!(holds(failure(
        account(2, 0, 1000),
        account(0, 1910, 1010),
        1010,
        lock()
    )));
    // The third failure must lock for exactly 900 seconds and alert once.
    assert!(!holds(failure(
        account(2, 0, 1000),
        account(0, 1909, 1010),
        1010,
        lock()
    )));
    assert!(!holds(failure(
        account(2, 0, 1000),
        account(0, 1910, 1010),
        1010,
        vec![]
    )));
    assert!(!holds(failure(
        account(2, 0, 1000),
        account(0, 1910, 1010),
        1010,
        vec![alert("someone-else", AlertKind::Locked, 1910)]
    )));
    assert!(!holds(failure(
        account(2, 0, 1000),
        account(0, 1910, 1010),
        1010,
        vec![alert("security-team", AlertKind::Unlocked, 1910)]
    )));
    // A failure recorded under another reason is refused.
    let mut wrong_reason = failure(account(0, 0, 1000), account(1, 0, 1010), 1010, vec![]);
    wrong_reason.failure = Some(202);
    assert!(!holds(wrong_reason));
}

fn unlock(pre: Account, post: Account, now: i128, admin: bool) -> Decided {
    Decided {
        pre,
        post,
        command: AccountCommand::AdminUnlock,
        now,
        admin,
        alerts: vec![alert("security-team", AlertKind::Unlocked, 0)],
        failure: None,
        effect: false,
    }
}

#[test]
fn unlocks_are_checked() {
    assert!(holds(unlock(
        account(0, 1900, 1000),
        account(0, 0, 1100),
        1100,
        true
    )));
    assert!(!holds(unlock(
        account(0, 1900, 1000),
        account(0, 0, 1100),
        1100,
        false
    )));
    assert!(!holds(unlock(
        account(0, 1900, 1000),
        account(0, 1900, 1100),
        1100,
        true
    )));
    let mut silent = unlock(account(0, 1900, 1000), account(0, 0, 1100), 1100, true);
    silent.alerts = vec![];
    assert!(!holds(silent));
}

/// Decisions that only a formula refuses: the Rust checks accept each one,
/// because its reason, alert, and effects are right and no rejection rule
/// applies, so a weakened formula would let it through.
#[test]
fn formulas_refuse_what_the_rust_checks_accept() {
    // Law 501: a login that moves the deadline.
    assert!(!holds(login(
        account(2, 0, 1000),
        account(0, 1910, 1010),
        1010
    )));
    // Law 502: an unlock that keeps the count, or does not record the time.
    assert!(!holds(unlock(
        account(2, 0, 1000),
        account(2, 0, 1100),
        1100,
        true
    )));
    assert!(!holds(unlock(
        account(0, 1900, 1000),
        account(0, 0, 1000),
        1100,
        true
    )));
    // Law 503: a failure under another command, one that does not record the
    // time, one that moves the deadline before the third failure, and a
    // third failure whose lock and alert agree on the wrong length.
    let mut succeeded = failure(account(0, 0, 1000), account(1, 0, 1010), 1010, vec![]);
    succeeded.command = AccountCommand::LoginSucceeded;
    assert!(!holds(succeeded));
    assert!(!holds(failure(
        account(0, 0, 1000),
        account(1, 0, 1000),
        1010,
        vec![]
    )));
    assert!(!holds(failure(
        account(0, 0, 1000),
        account(1, 500, 1010),
        1010,
        vec![]
    )));
    assert!(!holds(failure(
        account(2, 0, 1000),
        account(0, 1909, 1010),
        1010,
        vec![alert("security-team", AlertKind::Locked, 1909)]
    )));
}

#[test]
fn rejections_must_carry_the_rule_that_applies() {
    let hash = profile::digest("example/test/reject", b"independent decision view");
    let pre = account(0, 1900, 1000).to_value().unwrap();
    let cases = [
        (AccountCommand::LoginSucceeded, 1500, false, 201, true),
        (AccountCommand::LoginSucceeded, 1500, false, 200, false),
        (AccountCommand::LoginSucceeded, 999, false, 200, true),
        (AccountCommand::AdminUnlock, 1500, false, 202, true),
        (AccountCommand::AdminUnlock, 1500, true, 202, false),
        (AccountCommand::LoginFailed, 1900, false, 201, false),
    ];
    for (command, now, admin, reason_id, right) in cases {
        let command = command.to_value().unwrap();
        let context = context(now, admin).to_value().unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = AccountLaws::default().evaluate(&input, LawLimits::default());
        assert_eq!(
            result.is_ok(),
            right,
            "reason {reason_id} at {now}: {result:?}"
        );
    }
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    for (state, expected) in [(account(0, 0, 0), true), (account(0, 0, 5), false)] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = AccountLaws::default()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected
        );
    }
    let value = account(0, 0, 0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        AccountLaws::default().evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}
