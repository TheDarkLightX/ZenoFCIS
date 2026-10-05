use crate::oracle::templates::durable_counter::normal::{authority, create, generated::*, invoke, journey};
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
            "durable-counter-{}-{}",
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
fn complete_durable_journey() {
    let temp = Temp::new();
    let path = temp.0.join("counter.sqlite");
    assert!(
        journey(&path)
            .unwrap()
            .contains("count=1 failures=1 bundles=2 deliveries=2 pending=0")
    );
    assert!(
        journey(&path).is_err(),
        "existing history must not be replaced by genesis"
    );
}

#[test]
fn both_capacity_boundaries_reject_without_publication() {
    let temp = Temp::new();
    let contract = crate::oracle::templates::durable_counter::normal::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor).unwrap();
    let mut shell = create(&temp.0.join("counter.sqlite"), &authority).unwrap();
    for n in 0..3 {
        assert_eq!(
            invoke(
                &mut shell,
                &authority,
                CounterCommand::Increment,
                true,
                &format!("inc-{n}")
            )
            .unwrap(),
            "Accept"
        );
        assert_eq!(
            invoke(
                &mut shell,
                &authority,
                CounterCommand::RecordFailure,
                true,
                &format!("fail-{n}")
            )
            .unwrap(),
            "CommittedFailure"
        );
    }
    let before = shell.snapshot().unwrap();
    for command in [CounterCommand::Increment, CounterCommand::RecordFailure] {
        assert_eq!(
            invoke(&mut shell, &authority, command, true, "full").unwrap(),
            "Reject"
        );
        assert_eq!(shell.snapshot().unwrap(), before);
    }
    assert_eq!(before.version(), 6);
    assert_eq!(before.pending(), 6);
}
