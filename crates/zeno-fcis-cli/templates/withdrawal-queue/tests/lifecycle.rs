use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use withdrawal_queue::{
    Authority, Shell, authority, create, delivery::Destination, deposit, generated::*, invoke,
    journey, request, tick,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "withdrawal-queue-{}-{}",
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

/// Sends one command and returns the outcome.
fn send(
    shell: &mut Shell,
    authority: &Authority,
    command: VaultCommand,
    caller: Caller,
    alarm: bool,
    replay: &str,
) -> &'static str {
    invoke(shell, authority, command, caller, alarm, replay).unwrap()
}

fn state(shell: &Shell) -> Vault {
    Vault::try_from_value(shell.snapshot().unwrap().state().clone()).unwrap()
}

#[test]
fn complete_vault_journey() {
    let temp = Temp::new();
    let path = temp.0.join("vault.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"balance\":0,\"deposited\":4,\"paid\":4,\"payout_ticks\":[4,8]"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn a_sustained_alarm_delays_each_payout_by_three_ticks_but_never_stops_them() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("vault.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            deposit(2),
            Caller::Operator,
            false,
            "fund"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            deposit(2),
            Caller::Operator,
            false,
            "fund-2"
        ),
        "Accept"
    );
    let mut payout_ticks = Vec::new();
    let mut ticks = 0;
    // Lane A asks twice in a row while the alarm never clears. Each request
    // is paid on the fourth tick after it is presented: three ticks of
    // pause, then the must-serve tick, which ignores the alarm.
    for round in 0..2 {
        assert_eq!(
            send(
                shell,
                &authority,
                request(Lane::A, 2),
                Caller::OwnerA,
                false,
                &format!("request-{round}")
            ),
            "Accept"
        );
        for _ in 0..4 {
            let before = shell.snapshot().unwrap().pending_outbox();
            assert_eq!(
                send(
                    shell,
                    &authority,
                    tick(),
                    Caller::Keeper,
                    true,
                    &format!("tick-{ticks}")
                ),
                "Accept"
            );
            ticks += 1;
            if shell.snapshot().unwrap().pending_outbox() > before {
                payout_ticks.push(ticks);
            }
        }
    }
    assert_eq!(payout_ticks, [4, 8]);
    let vault = state(shell);
    assert_eq!(
        (vault.balance.0, vault.lane_a, vault.pause.0),
        (0, LaneStatus::Empty, 0)
    );
}

#[test]
fn rejections_leave_the_vault_and_outbox_unchanged() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let shell = &mut create(
        &temp.0.join("vault.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    assert_eq!(
        send(
            shell,
            &authority,
            deposit(2),
            Caller::Operator,
            false,
            "fund"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            deposit(1),
            Caller::Operator,
            false,
            "fund-2"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            shell,
            &authority,
            request(Lane::A, 2),
            Caller::OwnerA,
            false,
            "request"
        ),
        "Accept"
    );
    let before = shell.snapshot().unwrap();
    for (n, (command, caller)) in [
        // The wrong caller for each command.
        (deposit(1), Caller::OwnerA),
        (request(Lane::B, 1), Caller::OwnerA),
        (tick(), Caller::Operator),
        // Lane A is occupied; only one of the three units is unreserved; two
        // more units would take the balance above four.
        (request(Lane::A, 1), Caller::OwnerA),
        (request(Lane::B, 2), Caller::OwnerB),
        (deposit(2), Caller::Operator),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            send(
                shell,
                &authority,
                command,
                caller,
                true,
                &format!("refused-{n}")
            ),
            "Reject",
            "case {n}"
        );
        assert_eq!(shell.snapshot().unwrap(), before, "case {n}");
    }
}

#[test]
fn a_pause_survives_a_database_reopen() {
    let temp = Temp::new();
    let path = temp.0.join("vault.sqlite");
    let authority = authority().unwrap();
    let destination = Destination::default();
    let mut shell = create(&path, &authority, destination.clone()).unwrap();
    assert_eq!(
        send(
            &mut shell,
            &authority,
            deposit(2),
            Caller::Operator,
            false,
            "fund"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            &mut shell,
            &authority,
            request(Lane::B, 2),
            Caller::OwnerB,
            false,
            "request"
        ),
        "Accept"
    );
    assert_eq!(
        send(
            &mut shell,
            &authority,
            tick(),
            Caller::Keeper,
            true,
            "alarm"
        ),
        "Accept"
    );
    let paused = state(&shell);
    assert_eq!((&paused.lane_b, paused.pause.0), (&LaneStatus::Pending, 2));
    drop(shell);
    // The reopened store re-executes its history, and the pause holds.
    let mut shell = Shell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(destination),
    )
    .unwrap();
    assert_eq!(state(&shell), paused);
    assert_eq!(
        send(
            &mut shell,
            &authority,
            tick(),
            Caller::Keeper,
            false,
            "tick-2"
        ),
        "Accept"
    );
    let vault = state(&shell);
    assert_eq!((vault.pause.0, vault.must_serve.0), (1, false));
    assert_eq!(shell.snapshot().unwrap().pending_outbox(), 0);
    assert_eq!(
        send(
            &mut shell,
            &authority,
            tick(),
            Caller::Keeper,
            false,
            "tick-3"
        ),
        "Accept"
    );
    assert!(state(&shell).must_serve.0);
    assert_eq!(
        send(
            &mut shell,
            &authority,
            tick(),
            Caller::Keeper,
            true,
            "tick-4"
        ),
        "Accept"
    );
    let vault = state(&shell);
    assert_eq!(
        (vault.balance.0, vault.lane_b, vault.must_serve.0),
        (0, LaneStatus::Empty, false)
    );
    assert_eq!(shell.snapshot().unwrap().pending_outbox(), 1);
}
