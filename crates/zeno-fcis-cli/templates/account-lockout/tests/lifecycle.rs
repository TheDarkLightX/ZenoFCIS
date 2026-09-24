use account_lockout::{
    authority, context, create, delivery::Destination, generated::*, invoke, journey,
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
            "account-lockout-{}-{}",
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
fn complete_account_journey() {
    let temp = Temp::new();
    let path = temp.0.join("account.sqlite");
    let summary = journey(&path).unwrap();
    assert!(summary.contains("\"status\":\"passed\""), "{summary}");
    assert!(
        summary.contains("\"bundles\":6,\"pending\":0,\"deliveries\":2"),
        "{summary}"
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn a_lock_survives_a_restart() {
    let temp = Temp::new();
    let path = temp.0.join("account.sqlite");
    let authority = authority().unwrap();
    let destination = Destination::default();
    let mut shell = create(&path, &authority, destination.clone()).unwrap();
    for (n, now) in [100, 110, 120].into_iter().enumerate() {
        assert_eq!(
            invoke(
                &mut shell,
                &authority,
                AccountCommand::LoginFailed,
                context(now, false),
                &format!("fail-{n}")
            )
            .unwrap(),
            "CommittedFailure"
        );
    }
    drop(shell);
    // Reopening re-executes every persisted decision before admitting new ones.
    let mut shell = account_lockout::Shell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(destination),
    )
    .unwrap();
    let before = shell.snapshot().unwrap();
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            AccountCommand::LoginSucceeded,
            context(1019, false),
            "during-lock"
        )
        .unwrap(),
        "Reject"
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    assert_eq!(
        invoke(
            &mut shell,
            &authority,
            AccountCommand::LoginSucceeded,
            context(1020, false),
            "after-lock"
        )
        .unwrap(),
        "Accept"
    );
    assert_eq!(shell.snapshot().unwrap().bundle_count(), 4);
}
