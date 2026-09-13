use prepared_counter::{
    Shell, authority, create,
    delivery::Destination,
    generated::*,
    journey,
    prepare::{MAX_PUBLICATION_BYTES, PreparedBatch, admit, command, publication_sizes},
    profile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell_sqlite::CrashPoint;
use zeno_fcis_synthesis::finite::completion::{
    CompletionLimits, CompletionProblem, find_completion, verify_completion,
};
use zeno_fcis_synthesis::finite::{Domain as FiniteDomain, Op, Program};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "prepared-counter-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn step_model() -> Program {
    use Op::*;
    let state = FiniteDomain::Int { min: 0, max: 3 };
    let delta = FiniteDomain::Int { min: -1, max: 1 };
    Program::try_new(
        vec![state, delta, delta, delta],
        vec![FiniteDomain::Bool, state],
        vec![
            Input(0),
            Input(1),
            Input(2),
            Input(3),
            Add(0, 1),
            Add(4, 2),
            Add(5, 3),
            Int(0),
            Int(3),
            Lt(4, 7),
            Lt(8, 4),
            Not(9),
            Not(10),
            And(11, 12),
            Lt(5, 7),
            Lt(8, 5),
            Not(14),
            Not(15),
            And(16, 17),
            Lt(6, 7),
            Lt(8, 6),
            Not(19),
            Not(20),
            And(21, 22),
            And(13, 18),
            And(24, 23),
            Select(25, 6, 0),
        ],
        vec![25, 26],
    )
    .unwrap()
}

#[test]
fn all_216_decisions_match_model_and_fit_complete_publication_envelope() {
    let authority = authority().unwrap();
    let model = step_model();
    let mut checked_cases = 0;
    let mut maximum = 0;
    for count in 0..=3 {
        for a in -1..=1 {
            for b in -1..=1 {
                for c in -1..=1 {
                    let expected = model.evaluate(&[count, a, b, c]).unwrap();
                    for allowed in [false, true] {
                        let state = CounterState {
                            count: CounterValue(i128::from(count)),
                        };
                        let command = command([i128::from(a), i128::from(b), i128::from(c)]);
                        let invocation = admit(
                            &authority,
                            &state,
                            &command,
                            allowed,
                            profile::digest(
                                "example/test/case",
                                format!("{count},{a},{b},{c},{allowed}").as_bytes(),
                            ),
                        )
                        .unwrap();
                        match authority.execute(invocation).unwrap() {
                            Decision::Reject(reject) => {
                                assert!(!allowed || expected[0] == 0);
                                assert_eq!(
                                    reject.reason().rejection().reason_id().get(),
                                    if allowed { 201 } else { 200 }
                                );
                            }
                            Decision::CommittedFailure(_) => {
                                panic!("this profile prohibits committed failures")
                            }
                            Decision::Accept(accept) => {
                                assert!(allowed && expected[0] == 1);
                                let candidate = accept.into_candidate();
                                let post = candidate
                                    .bundle()
                                    .validate_and_apply::<RustCryptoSha256>(
                                        &state.to_value().unwrap(),
                                        Domain::new("example/prepared-counter/state", 1).unwrap(),
                                    )
                                    .unwrap();
                                let expected_state = CounterState {
                                    count: CounterValue(i128::from(expected[1])),
                                }
                                .to_value()
                                .unwrap();
                                assert_eq!(post.state(), &expected_state);
                                assert!(candidate.bundle().commit_plan().effects().is_empty());
                                let entries = candidate.bundle().outbox_plan().entries();
                                assert_eq!(entries.len(), 1);
                                assert_eq!(entries[0].ordinal(), 0);
                                assert_eq!(entries[0].channel(), 300);
                                assert_eq!(
                                    entries[0].destination(),
                                    &NotificationDestination("local-observer".into())
                                        .to_value()
                                        .unwrap()
                                );
                                assert_eq!(
                                    entries[0].payload(),
                                    &Notification {
                                        notified_count: CounterValue(i128::from(expected[1]))
                                    }
                                    .to_value()
                                    .unwrap()
                                );
                                let sizes = publication_sizes(&candidate, post.state()).unwrap();
                                assert!(
                                    sizes.receipt > 0
                                        && sizes.outbox > 0
                                        && sizes.authorization > 0
                                );
                                assert!(sizes.bundle > sizes.receipt + sizes.outbox);
                                assert!(sizes.total() <= MAX_PUBLICATION_BYTES);
                                maximum = maximum.max(sizes.total());
                            }
                        }
                        checked_cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked_cases, 216);
    println!("complete_decisions={checked_cases} maximum_publication_bytes={maximum}");
}

#[test]
fn every_declared_state_has_a_checked_eligible_exit() {
    let state = FiniteDomain::Int { min: 0, max: 3 };
    let terminal = Program::try_new(
        vec![state],
        vec![FiniteDomain::Bool],
        vec![Op::Input(0), Op::Int(0), Op::Eq(0, 1)],
        vec![2],
    )
    .unwrap();
    let model = step_model();
    let problem =
        CompletionProblem::try_new(model.clone(), terminal, CompletionLimits::default()).unwrap();
    println!(
        "completion_problem={}",
        problem
            .problem_hash()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let checked = verify_completion(&problem, &find_completion(&problem).unwrap()).unwrap();
    assert_eq!(checked.next_command(&[0]).unwrap(), None);
    for count in 1..=3 {
        let exit = checked.next_command(&[count]).unwrap().unwrap();
        let mut input = vec![count];
        input.extend_from_slice(exit);
        assert_eq!(model.evaluate(&input).unwrap(), [1, 0]);
        // This command is included in the complete runtime/envelope corpus above.
        assert!(exit.iter().all(|value| (-1..=1).contains(value)));
    }
}

#[test]
fn every_partition_publishes_the_same_complete_state_once() {
    let authority = authority().unwrap();
    for partition in [vec![3], vec![1, 2], vec![2, 1], vec![1, 1, 1]] {
        let temp = Temp::new();
        let mut shell = create(
            &temp.0.join("state.sqlite"),
            &authority,
            Destination::default(),
        )
        .unwrap();
        let initial = shell.snapshot().unwrap();
        let mut work = PreparedBatch::start(
            &authority,
            &initial,
            &command([1, 1, 1]),
            true,
            "same-input",
        )
        .unwrap();
        let mut offset = 0;
        for count in partition {
            work.advance(offset, count).unwrap();
            offset += count;
            assert_eq!(shell.snapshot().unwrap(), initial);
        }
        work.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .unwrap();
        let final_state = shell.snapshot().unwrap();
        assert_eq!(
            CounterState::try_from_value(final_state.state().clone())
                .unwrap()
                .count
                .0,
            3
        );
        assert_eq!(
            (
                final_state.version(),
                final_state.bundle_count(),
                final_state.pending_outbox()
            ),
            (1, 1, 1)
        );
    }
}

#[test]
fn cancellation_failed_chunk_and_incomplete_finish_publish_nothing() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let mut shell = create(
        &temp.0.join("state.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let before = shell.snapshot().unwrap();
    let mut bad = PreparedBatch::start(
        &authority,
        &before,
        &command([1, -1, -1]),
        true,
        "invalid-prefix",
    )
    .unwrap();
    assert!(bad.advance(0, 3).is_err());
    assert_eq!(bad.processed_items(), 0);
    bad.advance(0, 1).unwrap();
    assert!(bad.advance(0, 1).is_err());
    assert!(
        bad.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    let mut cancelled =
        PreparedBatch::start(&authority, &before, &command([1, 1, 1]), true, "cancel").unwrap();
    cancelled.advance(0, 1).unwrap();
    drop(cancelled);
    assert!(
        PreparedBatch::start(&authority, &before, &command([-1, 0, 0]), false, "denied").is_err()
    );
    assert_eq!(shell.snapshot().unwrap(), before);
}

fn complete(
    shell: &Shell,
    authority: &prepared_counter::Authority,
    deltas: [i128; 3],
    replay: &str,
) -> PreparedBatch {
    let mut work = PreparedBatch::start(
        authority,
        &shell.snapshot().unwrap(),
        &command(deltas),
        true,
        replay,
    )
    .unwrap();
    work.advance(0, 3).unwrap();
    work
}

#[test]
fn stale_context_changed_head_and_recurrent_root_publish_nothing() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let mut shell = create(
        &temp.0.join("state.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let original = shell.snapshot().unwrap();
    let work = complete(&shell, &authority, [1, 0, 0], "changed-context");
    assert!(
        work.publish(&mut shell, &authority, false, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(shell.snapshot().unwrap(), original);
    let stale = complete(&shell, &authority, [1, 0, 0], "stale");
    let aba = complete(&shell, &authority, [1, 0, 0], "aba");
    complete(&shell, &authority, [1, 0, 0], "inc")
        .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let before = shell.snapshot().unwrap();
    assert!(
        stale
            .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(shell.snapshot().unwrap(), before);
    complete(&shell, &authority, [-1, 0, 0], "dec")
        .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let before = shell.snapshot().unwrap();
    assert_eq!(before.root(), original.root());
    assert_ne!(before.version(), original.version());
    assert!(
        aba.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(shell.snapshot().unwrap(), before);
}

#[test]
fn another_database_handle_cannot_commit_stale_preparation() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let path = temp.0.join("state.sqlite");
    let mut first = create(&path, &authority, Destination::default()).unwrap();
    let mut second = Shell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(Destination::default()),
    )
    .unwrap();
    let stale = complete(&second, &authority, [1, 0, 0], "stale");
    complete(&first, &authority, [1, 0, 0], "winner")
        .publish(&mut first, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let before = first.snapshot().unwrap();
    assert!(
        stale
            .publish(&mut second, &authority, true, MAX_PUBLICATION_BYTES, None)
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), before);
}

#[test]
fn exact_capacity_accepts_and_one_byte_less_does_not_publish() {
    let temp = Temp::new();
    let authority = authority().unwrap();
    let mut shell = create(
        &temp.0.join("measure.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let result = complete(&shell, &authority, [1, 1, 1], "same")
        .publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)
        .unwrap();
    let mut second = create(
        &temp.0.join("boundary.sqlite"),
        &authority,
        Destination::default(),
    )
    .unwrap();
    let before = second.snapshot().unwrap();
    let error = complete(&second, &authority, [1, 1, 1], "same")
        .publish(
            &mut second,
            &authority,
            true,
            result.sizes.total() - 1,
            None,
        )
        .err()
        .unwrap();
    assert!(error.starts_with("publication capacity:"));
    assert_eq!(second.snapshot().unwrap(), before);
    complete(&second, &authority, [1, 1, 1], "same")
        .publish(&mut second, &authority, true, result.sizes.total(), None)
        .unwrap();
    assert_eq!(shell.snapshot().unwrap(), second.snapshot().unwrap());
}

#[test]
fn every_commit_crash_recovers_all_or_none() {
    let authority = authority().unwrap();
    for crash in [
        CrashPoint::BeforeTransaction,
        CrashPoint::AfterValidation,
        CrashPoint::AfterStateWrite,
        CrashPoint::AfterReplayWrite,
        CrashPoint::AfterOutboxWrite,
        CrashPoint::BeforeCommit,
        CrashPoint::AfterCommit,
    ] {
        let temp = Temp::new();
        let path = temp.0.join("state.sqlite");
        let mut shell = create(&path, &authority, Destination::default()).unwrap();
        let before = shell.snapshot().unwrap();
        assert!(
            complete(&shell, &authority, [1, 1, 1], "crash")
                .publish(
                    &mut shell,
                    &authority,
                    true,
                    MAX_PUBLICATION_BYTES,
                    Some(crash)
                )
                .is_err()
        );
        drop(shell);
        let shell = Shell::open_existing(
            &path,
            &authority,
            authority.bind_delivery_interpreter(Destination::default()),
        )
        .unwrap();
        let after = shell.snapshot().unwrap();
        if crash == CrashPoint::AfterCommit {
            assert_eq!(
                (
                    after.version(),
                    after.bundle_count(),
                    after.replay_count(),
                    after.pending_outbox()
                ),
                (1, 1, 1, 1)
            );
            assert_eq!(
                CounterState::try_from_value(after.state().clone())
                    .unwrap()
                    .count
                    .0,
                3
            );
        } else {
            assert_eq!(after, before);
        }
    }
}

#[test]
fn complete_restart_replay_and_interrupted_delivery_journey() {
    let temp = Temp::new();
    let path = temp.0.join("state.sqlite");
    assert!(journey(&path).unwrap().contains("\"status\":\"passed\""));
    assert!(journey(&path).is_err());
}
