use durable_counter::{
    authority, bindings::GeneratedProject, create, delivery::Destination, generated::*, invoke,
    journey, profile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;

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
    let authority = authority().unwrap();
    let mut shell = create(
        &temp.0.join("counter.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
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
    assert_eq!(before.bundle_count(), 6);
    assert_eq!(before.pending_outbox(), 6);
}

#[test]
fn every_bounded_input_obeys_the_independent_decision_table() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    for count in 0..=3 {
        for failures in 0..=3 {
            for command in [CounterCommand::Increment, CounterCommand::RecordFailure] {
                for allowed in [false, true] {
                    let pre = project
                        .admit_root::<RustCryptoSha256>(
                            &CounterState {
                                count: CounterValue(count),
                                failures: CounterValue(failures),
                            },
                            ValidationLimits::default(),
                        )
                        .unwrap();
                    let cmd = project
                        .admit_command::<RustCryptoSha256>(&command, ValidationLimits::default())
                        .unwrap();
                    let ctx = project
                        .admit_context::<RustCryptoSha256>(
                            &CounterContext(allowed),
                            ValidationLimits::default(),
                        )
                        .unwrap();
                    let binding = profile::digest(
                        "example/test/input",
                        format!("{count},{failures},{command:?},{allowed}").as_bytes(),
                    );
                    let invocation = authority
                        .admit_invocation(
                            pre,
                            cmd.admitted().clone(),
                            ctx.admitted().clone(),
                            binding,
                            binding,
                            binding,
                        )
                        .unwrap();
                    let selected = match command {
                        CounterCommand::Increment => count,
                        CounterCommand::RecordFailure => failures,
                    };
                    let decision = authority.execute(invocation).unwrap();
                    match decision {
                        Decision::Reject(reject) => {
                            assert!(!allowed || selected == 3);
                            assert_eq!(
                                reject.reason().rejection().reason_id().get(),
                                if allowed { 201 } else { 200 }
                            );
                        }
                        Decision::Accept(_) => {
                            assert!(allowed && count < 3);
                            assert!(matches!(command, CounterCommand::Increment));
                        }
                        Decision::CommittedFailure(failure) => {
                            assert!(allowed && failures < 3);
                            assert!(matches!(command, CounterCommand::RecordFailure));
                            assert_eq!(failure.reason().get(), 202);
                        }
                    }
                }
            }
        }
    }
}
