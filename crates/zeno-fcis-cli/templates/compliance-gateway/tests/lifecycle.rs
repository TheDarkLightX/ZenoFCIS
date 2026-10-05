use compliance_gateway::{
    authority, context, create, generated::*, invoke, journey, reinstate, screen,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "compliance-gateway-{}-{}",
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

#[test]
fn complete_gateway_journey() {
    let temp = Temp::new();
    let path = temp.0.join("gateway.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"strikes\":1,\"allowed\":1,\"held\":3,\"blocked\":5"),
        "{summary}"
    );
    assert!(
        summary.contains("\"bundles\":10,\"pending\":0,\"deliveries\":8"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn a_frozen_account_survives_a_restart() {
    let temp = Temp::new();
    let path = temp.0.join("gateway.sqlite");
    let contract = compliance_gateway::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor).unwrap();
    let mut shell = create(&path, &authority).unwrap();
    for n in 0..3 {
        assert_eq!(
            invoke(
                &mut shell,
                &authority,
                screen(Region::Sanctioned, 1, CounterpartyRisk::Low),
                context(3, false),
                &format!("block-{n}")
            )
            .unwrap(),
            "CommittedFailure"
        );
    }
    drop(shell);
    // Reopening re-executes every persisted decision before admitting new ones.
    let mut shell = compliance_gateway::Shell::open(&path, &authority).unwrap();
    // Frozen: even a small, low-risk transfer from a verified customer is
    // blocked, and the strikes stay at three.
    let small = || screen(Region::Allowed, 0, CounterpartyRisk::Low);
    assert_eq!(
        invoke(&mut shell, &authority, small(), context(3, false), "frozen").unwrap(),
        "CommittedFailure"
    );
    let snapshot = shell.snapshot().unwrap();
    assert_eq!(
        compliance_gateway::decode_state(snapshot.state()).unwrap(),
        Standing {
            strikes: Strikes(3)
        }
    );
    assert_eq!(snapshot.pending(), 4);
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            reinstate(),
            context(3, true),
            "reinstate"
        )
        .unwrap(),
        "Accept"
    );
    assert_eq!(
        invoke(&mut shell, &authority, small(), context(3, false), "after").unwrap(),
        "Accept"
    );
    let snapshot = shell.snapshot().unwrap();
    assert_eq!(snapshot.version(), 6);
    assert_eq!(
        compliance_gateway::decode_state(snapshot.state()).unwrap(),
        Standing {
            strikes: Strikes(0)
        }
    );
}

#[test]
fn rejections_leave_standing_and_outbox_unchanged() {
    let temp = Temp::new();
    let contract = compliance_gateway::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor).unwrap();
    let mut shell = create(&temp.0.join("gateway.sqlite"), &authority).unwrap();
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            screen(Region::Allowed, 4, CounterpartyRisk::Low),
            context(0, false),
            "unverified-large"
        )
        .unwrap(),
        "CommittedFailure"
    );
    let before = shell.snapshot().unwrap();
    assert_eq!(before.pending(), 1);
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            reinstate(),
            context(3, false),
            "not-reviewer"
        )
        .unwrap(),
        "Reject"
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            reinstate(),
            context(3, true),
            "reviewer"
        )
        .unwrap(),
        "Accept"
    );
    let cleared = shell.snapshot().unwrap();
    assert_ne!(cleared, before);
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            reinstate(),
            context(3, true),
            "nothing"
        )
        .unwrap(),
        "Reject"
    );
    assert_eq!(shell.snapshot().unwrap(), cleared);
}
