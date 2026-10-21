//! Seeded protocol simulation, process-abort recovery, and behavioural controls.
mod sim;
use sim::{
    database, fixture, model,
    schedule::{self, Schedule},
    *,
};

#[test]
fn random_and_schedule_vectors() {
    let mut r = schedule::Random(0);
    assert_eq!(
        [r.next(), r.next(), r.next()],
        [0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f],
        "published random vector"
    );
    let s = Schedule::seeded(123, 64);
    assert_eq!(Schedule::parse(&s.json()), s);
}
#[test]
fn default_corpus() {
    let start = std::time::Instant::now();
    // Process isolation avoids global SQLite shared-cache mutex contention
    // between independent worlds, and preserves each seed's sequential order.
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            ok(std::process::Command::new(ok(std::env::current_exe()))
                .args(["--exact", "process_corpus_worker", "--nocapture"])
                .env("SHELL_SIM_CORPUS_WORKER", worker.to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn())
        })
        .collect();
    let outputs: Vec<_> = workers
        .into_iter()
        .map(|worker| ok(worker.wait_with_output()))
        .collect();
    for output in outputs {
        assert!(
            output.status.success(),
            "corpus worker failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let elapsed = start.elapsed();
    eprintln!("256 seeds x 64 steps x 2 replays: {elapsed:?}");
    assert!(
        elapsed < std::time::Duration::from_secs(90),
        "default corpus exceeded 90 seconds: {elapsed:?}"
    );
}
#[test]
fn process_corpus_worker() {
    let Ok(worker) = std::env::var("SHELL_SIM_CORPUS_WORKER") else {
        return;
    };
    let worker: u64 = ok(worker.parse());
    assert!(worker < 4);
    fixture::with_authority(|a| {
        let cases = fixture::cases(a);
        let mut action_counts = [0_u64; 13];
        let mut crash_mask = 0_u16;
        for seed in (worker..256).step_by(4) {
            let s = Schedule::seeded(seed, 64);
            let first = replay_with_authority(a, &cases, &s);
            for event in &first.events {
                action_counts[event.action as usize] += 1;
                if let model::Outcome::Refused(model::Refusal::Crash(point)) = event.decision {
                    crash_mask |= 1 << point;
                }
            }
            assert_eq!(first, replay_with_authority(a, &cases, &s), "seed {seed}");
        }
        assert!(
            action_counts.iter().all(|n| *n > 0),
            "corpus action coverage {action_counts:?}"
        );
        assert_eq!(crash_mask, 0xfe, "corpus crash-point coverage");
    });
}
#[test]
#[ignore = "long deterministic campaign: 4096 seeds x 128 steps, twice"]
fn long_corpus() {
    for seed in 0..4096 {
        let s = Schedule::seeded(seed, 128);
        assert_eq!(replay(&s), replay(&s));
    }
}
#[test]
fn regression_schedules() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sim/fixtures");
    let mut paths = ok(std::fs::read_dir(dir))
        .map(ok)
        .map(|p| p.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let s = Schedule::parse(&ok(std::fs::read_to_string(path)));
        assert_eq!(replay(&s), replay(&s));
    }
}
#[test]
fn comparator_self_test() {
    let expected = model::Database(std::collections::BTreeMap::from([(
        "v2_state".into(),
        vec![vec![model::int(0)]],
    )]));
    let mut actual = expected.clone();
    actual
        .0
        .get_mut("v2_state")
        .unwrap_or_else(|| panic!("state"))[0][0] = model::int(1);
    assert!(
        database::compare(&expected, &actual).is_err(),
        "comparator accepted unequal states"
    );
}
#[test]
fn minimizer_self_test() {
    let steps = Schedule::seeded(21, 64).steps;
    let target = steps[17];
    let result = schedule::minimize(steps, |steps| steps.contains(&target));
    assert_eq!(
        result,
        vec![target],
        "minimizer must remove irrelevant steps"
    );
}

#[test]
fn outbox_exact() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let input = fixture::input(a, key(1), [0, 0], 120);
        let mut model = model::Model::new(fixture::model_genesis(a));
        assert_eq!(commit(&mut db, a, &input, 0), model.commit(input, 0, None));
        ok(database::compare(
            &model.database(),
            &database::snapshot(&files.store),
        ));
        assert_eq!(ok(db.export_pending()).len(), 2, "I3 exactly two entries");
    });
}
#[test]
fn changed_replay_is_refused() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let first = fixture::input(a, key(1), [0, 0], 120);
        commit(&mut db, a, &first, 0);
        let changed = fixture::input(a, key(1), [0, 0], 121);
        assert_eq!(
            commit(&mut db, a, &changed, 0),
            model::Outcome::Refused(model::Refusal::Replay),
            "I2 changed replay"
        );
        let before = database::snapshot(&files.store);
        commit(&mut db, a, &first, 0);
        assert_eq!(before, database::snapshot(&files.store));
    });
}
#[test]
fn acknowledgements_are_exact() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let input = fixture::input(a, key(1), [0, 0], 120);
        commit(&mut db, a, &input, 0);
        let mut model = model::Model::new(fixture::model_genesis(a));
        model.commit(input, 0, None);
        let d = model.deliveries[0].clone();
        // No intervening action: the second hash check is redundant on this
        // schedule. The control must remove both acknowledgement hash checks.
        assert_eq!(
            acknowledge(&mut db, d.id, key(999)),
            model.acknowledge(d.id, key(999)),
            "I4 wrong payload"
        );
        ok(database::compare(
            &model.database(),
            &database::snapshot(&files.store),
        ));
        assert_eq!(
            acknowledge(&mut db, d.id, payload_hash(&d.payload)),
            model.acknowledge(d.id, payload_hash(&d.payload))
        );
        assert_eq!(
            acknowledge(&mut db, d.id, payload_hash(&d.payload)),
            model.acknowledge(d.id, payload_hash(&d.payload)),
            "I4 duplicate acknowledgement"
        );
        assert_eq!(
            acknowledge(&mut db, key(999), key(999)),
            model::Outcome::Refused(model::Refusal::Unknown)
        );
        ok(database::compare(
            &model.database(),
            &database::snapshot(&files.store),
        ));
    });
}
#[test]
fn freshness_interleaving() {
    use zeno_fcis_shell_sqlite::CrashMode;
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut first = create(&files, a);
        let mut second = ok(zeno_fcis_shell_sqlite::v2::V2SqliteShell::open(
            &files.store,
            a,
        ));
        let input = fixture::input(a, key(1), [0, 0], 121);
        let other = fixture::input(a, key(2), [0, 0], 120);
        let pre = model::state(fixture::schema_hash(), input.before);
        let result = first.commit_with_test_controls(
            input.key,
            fixture::publication(a, &pre, &input.command, &input.context),
            None,
            CrashMode::Error,
            || {
                assert!(matches!(
                    commit(&mut second, a, &other, 0),
                    model::Outcome::Committed(1, _)
                ));
            },
        );
        assert!(
            matches!(result, Err(zeno_fcis_shell_sqlite::v2::Error::Concurrent)),
            "stale writer must return Concurrent: {result:?}"
        );
        let mut model = model::Model::new(fixture::model_genesis(a));
        model.commit(other, 0, None);
        ok(database::compare(
            &model.database(),
            &database::snapshot(&files.store),
        ));
        // synchronize makes a later ordinary call observe the committed head.
        assert_eq!(ok(first.snapshot()).version(), 1);
        let current = fixture::input(a, key(3), [1, 0], 121);
        let pre = model::state(fixture::schema_hash(), current.before);
        let result = first.commit_at(
            0,
            current.key,
            fixture::publication(a, &pre, &current.command, &current.context),
        );
        assert!(
            matches!(result, Err(zeno_fcis_shell_sqlite::v2::Error::Concurrent)),
            "expected-version guard"
        );
    });
}
#[test]
fn busy_is_atomic() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        ok(db.set_test_busy_timeout(std::time::Duration::ZERO));
        let before = database::snapshot(&files.store);
        let lock = ok(rusqlite::Connection::open(&files.store));
        ok(lock.execute_batch("BEGIN IMMEDIATE"));
        let input = fixture::input(a, key(1), [0, 0], 120);
        assert_eq!(
            commit(&mut db, a, &input, 8),
            model::Outcome::Refused(model::Refusal::Busy)
        );
        ok(lock.execute_batch("ROLLBACK"));
        assert_eq!(before, database::snapshot(&files.store));
        assert!(matches!(
            commit(&mut db, a, &input, 0),
            model::Outcome::Committed(1, _)
        ));
    });
}
#[test]
fn injected_error_rollback_and_unknown_outcome() {
    fixture::with_authority(|a| {
        for fault in 1..=7 {
            let files = database::Files::new();
            let mut db = create(&files, a);
            let input = fixture::input(a, key(1), [0, 0], 120);
            let mut model = model::Model::new(fixture::model_genesis(a));
            assert_eq!(
                commit(&mut db, a, &input, fault),
                model.commit(input.clone(), fault, None)
            );
            ok(database::compare(
                &model.database(),
                &database::snapshot(&files.store),
            ));
            drop(db);
            let mut db = ok(zeno_fcis_shell_sqlite::v2::V2SqliteShell::open(
                &files.store,
                a,
            ));
            ok(db.audit());
            model.audit();
            let recovered = commit(&mut db, a, &input, 0);
            assert_eq!(recovered, model.commit(input, 0, None));
            if fault == 7 {
                assert!(matches!(recovered, model::Outcome::Replay(1, _)));
            }
        }
    });
}
#[test]
fn lost_reply_receiver() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let input = fixture::input(a, key(1), [0, 0], 120);
        commit(&mut db, a, &input, 0);
        let mut model = model::Model::new(fixture::model_genesis(a));
        model.commit(input, 0, None);
        let d = &model.deliveries[0];
        let mut receiver = database::Receiver::open(&files.receiver);
        receiver.receive(d, true);
        drop(receiver);
        // Receiver committed but no acknowledgement reached the shell.
        assert_eq!(ok(db.export_pending()).len(), 2);
        let mut receiver = database::Receiver::open(&files.receiver);
        receiver.receive(d, false);
        receiver.invariants();
        assert_eq!(receiver.state().len(), 1);
        assert_eq!(
            acknowledge(&mut db, d.id, payload_hash(&d.payload)),
            model::Outcome::Observed
        );
    });
}
#[test]
fn at_least_once_and_new_key_limits() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let input = fixture::input(a, key(1), [0, 0], 120);
        let original = commit(&mut db, a, &input, 0);
        assert!(matches!(original, model::Outcome::Committed(1, _)));
        let replayed = commit(&mut db, a, &input, 0);
        assert!(matches!(replayed, model::Outcome::Replay(1, _)));
        // A new key and newly acquired state can repeat the logical command.
        let second = fixture::input(a, key(2), [1, 0], 120);
        assert!(matches!(
            commit(&mut db, a, &second, 0),
            model::Outcome::Committed(2, _)
        ));
        assert_eq!(
            ok(db.snapshot()).state(),
            model::state(fixture::schema_hash(), [2, 0])
        );
        let delivery = ok(db.export_pending()).remove(0);
        let mut receiver = database::Receiver::open(&files.receiver);
        receiver.unprotected_effect(delivery.delivery_id(), delivery.payload());
        drop(receiver);
        let delivery = ok(db.export_pending()).remove(0);
        let mut receiver = database::Receiver::open(&files.receiver);
        receiver.unprotected_effect(delivery.delivery_id(), delivery.payload());
        assert_eq!(
            receiver.effect_count(delivery.delivery_id()),
            2,
            "without receiver deduplication, redelivery can apply twice"
        );
    });
}
#[test]
fn stale_absence_is_not_noncommit() {
    fixture::with_authority(|a| {
        let files = database::Files::new();
        let mut db = create(&files, a);
        let stale = database::snapshot(&files.store);
        let input = fixture::input(a, key(1), [0, 0], 120);
        commit(&mut db, a, &input, 7);
        assert!(stale.0["v2_commits"].is_empty());
        assert_eq!(database::snapshot(&files.store).0["v2_commits"].len(), 1);
        assert!(matches!(
            commit(&mut db, a, &input, 0),
            model::Outcome::Replay(1, _)
        ));
    });
}

// The child is selected by the test harness; it is harmless in normal runs.
#[test]
fn process_child() {
    let Ok(role) = std::env::var("SHELL_SIM_CHILD") else {
        return;
    };
    let path = ok(std::env::var("SHELL_SIM_STORE"));
    fixture::with_authority(|a| {
        let mut db = ok(zeno_fcis_shell_sqlite::v2::V2SqliteShell::open(&path, a));
        let input = fixture::input(a, key(1), [0, 0], 120);
        let role: u8 = ok(role.parse());
        if role < 7 || role == 9 {
            let pre = model::state(fixture::schema_hash(), input.before);
            let _ = db.commit_with_test_controls(
                input.key,
                fixture::publication(a, &pre, &input.command, &input.context),
                Some(if role == 9 {
                    zeno_fcis_shell_sqlite::CrashPoint::AfterFirstOutboxWrite
                } else {
                    POINTS[role as usize]
                }),
                zeno_fcis_shell_sqlite::CrashMode::Abort,
                || {},
            );
        } else {
            commit(&mut db, a, &input, 0);
            if role == 8 {
                let mut model = model::Model::new(fixture::model_genesis(a));
                model.commit(input, 0, None);
                let mut receiver = database::Receiver::open(std::path::Path::new(&ok(
                    std::env::var("SHELL_SIM_RECEIVER"),
                )));
                receiver.receive(&model.deliveries[0], false);
            }
            std::process::abort();
        }
        panic!("child did not abort");
    });
}
#[test]
fn kill_and_reopen() {
    fixture::with_authority(|a| {
        // Seven real in-protocol aborts, abort immediately after return, and
        // abort after receiver delivery before acknowledgement.
        for role in 0..10 {
            eprintln!("process-abort role {role}");
            let files = database::Files::new();
            drop(create(&files, a));
            let status = ok(std::process::Command::new(ok(std::env::current_exe()))
                .args(["--exact", "process_child", "--nocapture"])
                .env("SHELL_SIM_CHILD", role.to_string())
                .env("SHELL_SIM_STORE", &files.store)
                .env("SHELL_SIM_RECEIVER", &files.receiver)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status());
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(
                    status.signal(),
                    Some(6),
                    "child must actually abort: {status}"
                );
            }
            #[cfg(not(unix))]
            assert!(!status.success());
            let input = fixture::input(a, key(1), [0, 0], 120);
            let mut model = model::Model::new(fixture::model_genesis(a));
            if (6..=8).contains(&role) {
                model.commit(input.clone(), 0, None);
            }
            ok(database::compare(
                &model.database(),
                &database::snapshot(&files.store),
            ));
            let mut db = ok(zeno_fcis_shell_sqlite::v2::V2SqliteShell::open(
                &files.store,
                a,
            ));
            ok(db.audit());
            model.audit();
            let recovered = commit(&mut db, a, &input, 0);
            assert_eq!(
                recovered,
                model.commit(input, 0, None),
                "original-key recovery role {role}"
            );
            if (6..=8).contains(&role) {
                assert!(matches!(recovered, model::Outcome::Replay(1, _)));
            }
            ok(database::compare(
                &model.database(),
                &database::snapshot(&files.store),
            ));
            if role == 8 {
                let mut receiver = database::Receiver::open(&files.receiver);
                receiver.receive(&model.deliveries[0], false);
                receiver.invariants();
                assert_eq!(receiver.state().len(), 1);
            }
        }
    });
}

#[test]
fn fixture_domain() {
    fixture::with_authority(|a| {
        let cases = fixture::cases(a);
        assert_eq!(cases.len(), 24);
        for count in 0..=3 {
            for failures in 0..=3 {
                for variant in [120, 121] {
                    let before = [count, failures];
                    if before[usize::from(variant == 121)] == 3 {
                        let state = model::state(fixture::schema_hash(), before);
                        assert!(matches!(
                            a.publish(zeno_fcis_synthesis::finite::v2_composition::Raw {
                                state: &state,
                                command: &fixture::command(variant),
                                context: &fixture::context()
                            }),
                            zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome::Reject(
                                _
                            )
                        ));
                    }
                }
            }
        }
        for input in cases {
            let pre = model::state(fixture::schema_hash(), input.before);
            let publication = fixture::publication(a, &pre, &input.command, &input.context);
            let mut after = input.before;
            after[usize::from(input.variant == 121)] += 1;
            assert_eq!(
                publication.poststate(),
                model::state(fixture::schema_hash(), after)
            );
            assert!(publication.effects().is_empty());
            assert_eq!(publication.outbox().len(), 2);
            let expected_payload = ok(zeno_fcis_value::Value::record_canonical(vec![
                zeno_fcis_value::Field::new(112, zeno_fcis_value::Value::signed(after[0])),
                zeno_fcis_value::Field::new(113, zeno_fcis_value::Value::signed(after[1])),
            ]));
            use zeno_fcis_codec::CanonicalEncode;
            for (ordinal, d) in publication.outbox().iter().enumerate() {
                assert_eq!(d.ordinal(), ordinal as u32);
                assert_eq!(
                    (d.channel(), d.destination_root(), d.payload_root()),
                    (300, 103, 104)
                );
                assert_eq!(d.payload(), ok(expected_payload.canonical_bytes()));
                assert_eq!(
                    d.destination(),
                    ok(
                        ok(zeno_fcis_value::Value::text_ascii("local-observer".into()))
                            .canonical_bytes()
                    )
                );
                assert_eq!(
                    d.idempotency(),
                    ok(zeno_fcis_value::Value::unsigned(0).canonical_bytes())
                );
            }
        }
    });
}
