use agent_treasury_guard::{
    Authority, Shell, authority, context, create, delivery::Destination, failed, generated::*,
    invoke, journey, propose, settled, treasury,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use Caller::{Agent, Dex};
use Direction::{BuyBase, SellBase};
use ModelId::TreasuryAgentV2 as V2;
use PendingSwap::{NoSwap, PendingBuy, PendingSell};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agent-treasury-guard-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Sends one command at `now` with a fresh price and the approved model.
fn send(
    shell: &mut Shell,
    authority: &Authority,
    command: &TreasuryCommand,
    caller: Caller,
    now: i128,
    price: i128,
    message: &str,
) -> &'static str {
    invoke(
        shell,
        authority,
        command,
        &context(caller, now, price, now, V2),
        message,
    )
    .unwrap()
}

fn state(shell: &Shell) -> Treasury {
    Treasury::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap()
}

#[test]
fn complete_treasury_journey() {
    let temp = Temp::new();
    let path = temp.0.join("treasury.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"bundles\":10,\"pending\":0,\"deliveries\":5"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn a_pending_swap_survives_a_restart() {
    let temp = Temp::new();
    let path = temp.0.join("treasury.sqlite");
    let authority = authority().unwrap();
    let destination = Destination::default();
    let mut shell = create(&path, &authority, destination.clone()).unwrap();
    assert_eq!(
        send(
            &mut shell,
            &authority,
            &propose(BuyBase, 2, 2),
            Agent,
            1,
            1,
            "buy"
        ),
        "Accept"
    );
    drop(shell);
    // The reopened store re-executes the history; the swap is still
    // outstanding, so a second proposal is refused and the settlement lands.
    let mut shell = Shell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(destination),
    )
    .unwrap();
    assert_eq!(state(&shell), treasury(4, 1, 2, 1, PendingBuy, 2, 2));
    assert_eq!(
        send(
            &mut shell,
            &authority,
            &propose(BuyBase, 1, 1),
            Agent,
            2,
            1,
            "buy-again"
        ),
        "Reject"
    );
    assert_eq!(
        send(&mut shell, &authority, &settled(1, 2), Dex, 2, 1, "settle"),
        "Accept"
    );
    assert_eq!(state(&shell), treasury(4, 3, 2, 2, NoSwap, 0, 0));
}

#[test]
fn a_late_settlement_cannot_settle_a_newer_swap() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("treasury.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(BuyBase, 1, 1),
            Agent,
            1,
            1,
            "buy-1"
        ),
        "Accept"
    );
    assert_eq!(
        send(shell, &authority, &failed(1), Dex, 2, 1, "fail-1"),
        "CommittedFailure"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(SellBase, 1, 1),
            Agent,
            3,
            1,
            "sell-3"
        ),
        "Accept"
    );
    // The DEX delivers a settlement of intent 1 late. It names intent 1, so
    // it cannot settle intent 3, and nothing changes.
    let before = shell.snapshot().unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            &settled(1, 3),
            Dex,
            4,
            1,
            "settle-1-late"
        ),
        "Reject"
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    // The settlement of intent 3 then credits quote.
    assert_eq!(
        send(shell, &authority, &settled(3, 1), Dex, 4, 1, "settle-3"),
        "Accept"
    );
    assert_eq!(state(shell), treasury(7, 0, 0, 4, NoSwap, 0, 0));
}

#[test]
fn a_failure_refunds_but_does_not_restore_the_budget() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("treasury.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(BuyBase, 3, 3),
            Agent,
            1,
            1,
            "buy-3"
        ),
        "Accept"
    );
    assert_eq!(
        send(shell, &authority, &failed(1), Dex, 2, 1, "fail"),
        "CommittedFailure"
    );
    // The quote is back, but 3 of the day's 4 stay committed: a buy of 2 is
    // over budget and a buy of 1 is not.
    assert_eq!(state(shell), treasury(6, 1, 3, 2, NoSwap, 0, 0));
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(BuyBase, 2, 2),
            Agent,
            3,
            1,
            "buy-2"
        ),
        "Reject"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(BuyBase, 1, 1),
            Agent,
            3,
            1,
            "buy-1"
        ),
        "Accept"
    );
    assert_eq!(state(shell), treasury(5, 1, 4, 3, PendingBuy, 1, 1));
    // The next day starts a new budget once the swap settles.
    assert_eq!(
        send(shell, &authority, &settled(3, 1), Dex, 4, 1, "settle"),
        "Accept"
    );
    assert_eq!(state(shell), treasury(5, 2, 0, 4, NoSwap, 0, 0));
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(BuyBase, 3, 3),
            Agent,
            5,
            1,
            "buy-3-again"
        ),
        "Accept"
    );
    assert_eq!(state(shell), treasury(2, 2, 3, 5, PendingBuy, 3, 3));
}

#[test]
fn a_repeated_settlement_changes_nothing_and_queues_nothing() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("treasury.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            &propose(SellBase, 1, 2),
            Agent,
            1,
            2,
            "sell"
        ),
        "Accept"
    );
    assert_eq!(state(shell), treasury(6, 0, 2, 1, PendingSell, 1, 2));
    assert_eq!(
        send(shell, &authority, &settled(1, 3), Dex, 2, 2, "settle"),
        "Accept"
    );
    // The DEX retries under a new message ID. Nothing is outstanding, so the
    // retry is refused and no request is queued.
    assert_eq!(
        send(shell, &authority, &settled(1, 3), Dex, 3, 2, "settle-retry"),
        "Reject"
    );
    let snapshot = shell.snapshot().unwrap();
    assert_eq!((snapshot.bundle_count(), snapshot.pending_outbox()), (2, 1));
    assert_eq!(state(shell), treasury(9, 0, 2, 2, NoSwap, 0, 0));
}
