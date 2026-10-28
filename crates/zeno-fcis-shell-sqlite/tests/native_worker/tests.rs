//! Independent native process, actual SQLite dispatcher and receiver observations.
use super::*;
use std::{
    collections::BTreeMap,
    fs,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use zeno_fcis_shell_sqlite::effects::{
    channel::{Channel, Error as ChannelError, Status},
    native_worker::{self, Completion, PinnedFile, Runtime, Termination, Worker},
};

fn inputs() -> (Runtime, Vec<u8>) {
    if std::env::var("ZENO_NATIVE_REQUIRE_INHERITED_FD").as_deref() == Ok("1") {
        assert!(
            fs::metadata("/proc/self/fd/200").is_ok(),
            "test controller did not inherit FD 200"
        );
    }
    let text = ok(fs::read_to_string(ok(std::env::var(
        "ZENO_NATIVE_RUNTIME_PINS",
    ))));
    let mut roles = BTreeMap::new();
    let mut dependencies = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1].len(), 64);
        let mut hash = [0; 32];
        for (index, value) in hash.iter_mut().enumerate() {
            *value = ok(u8::from_str_radix(&fields[1][2 * index..2 * index + 2], 16));
        }
        let pin = ok(PinnedFile::new(
            fields[2],
            zeno_fcis_codec::Hash32::new(hash),
        ));
        if fields[0] == "dependency" {
            dependencies.push(pin);
        } else {
            assert!(roles.insert(fields[0], pin).is_none());
        }
    }
    let mut get = |name| ok(roles.remove(name).ok_or("missing runtime role"));
    let runtime = Runtime {
        python: get("python"),
        bwrap: get("bwrap"),
        prlimit: get("prlimit"),
        loader: get("loader"),
        libc: get("libc"),
        libgcc: get("libgcc"),
        libm: get("libm"),
        seccomp: get("seccomp"),
        dependencies,
    };
    assert!(roles.is_empty());
    let image = ok(fs::read(ok(std::env::var("ZENO_NATIVE_WORKER_IMAGE"))));
    (runtime, image)
}
fn launch<'h>(
    runtime: &Runtime,
    image: &[u8],
    mode: &str,
    path: &Path,
    handles: &'h [zeno_fcis_shell_sqlite::effects::dispatcher::IntentHandle],
    principal: u64,
) -> Worker<'h> {
    let channel = ok(Channel::new(key(20), key(principal), handles, 8));
    ok(Worker::spawn(
        runtime,
        image,
        RustCryptoSha256::hash(image),
        &[mode, ok(path.to_str().ok_or("path encoding"))],
        channel,
    ))
}
fn clean(report: &Completion) {
    assert_eq!(report.termination, Termination::Exited, "{report:?}");
    assert_eq!(report.worker_exit, 0, "{report:?}");
    assert_eq!(report.stderr_bytes, 0, "{report:?}");
}

fn clean_exit_after_dropped_reply(report: &Completion) {
    assert_eq!(report.termination, Termination::Exited, "{report:?}");
    assert_eq!(report.worker_exit, 0, "{report:?}");
    assert_eq!(report.stderr_bytes, 14);
    assert_eq!(
        report.stderr_prefix_sha256,
        RustCryptoSha256::hash(b"reader-closed\n")
    );
    assert_eq!((report.output_bytes, report.input_bytes), (16, 16));
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn useful_native_dispatch_rechecks_current_grants() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut dispatcher = create(&files, lineage, &calls, Mode::Success);
        let handles = [
            ok(dispatcher.grant(key(40), pending[0].id)),
            ok(dispatcher.grant(key(40), pending[1].id)),
        ];
        let before = database::snapshot(&files.store);
        let mut worker = launch(&runtime, &image, "valid", &files.store, &handles, 40);
        assert_eq!(
            ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
            Some(Status::Sent)
        );
        assert!(ok(dispatcher.revoke(&handles[1])));
        assert_eq!(
            ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
            Some(Status::PolicyRefused)
        );
        assert_eq!(
            ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
            Some(Status::DispatcherRefused(Refusal::Grant))
        );
        assert_eq!(ok(worker.exchange(&mut dispatcher)), None);
        let report = ok(worker.finish());
        clean(&report);
        assert_eq!((report.output_bytes, report.input_bytes), (48, 48));
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(calls.borrow()[0].delivery, pending[0]);
        assert_eq!(calls.borrow()[0].reservations, checkpoint(&files.receiver));
        assert_eq!(
            database::snapshot(&files.store),
            before,
            "native dispatch is not ACK"
        );
    });
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn native_peer_cannot_select_identity_slot_or_replay() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        for mode in ["wrong-principal", "forged", "slot", "replay"] {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut dispatcher = create(&files, lineage, &calls, Mode::Success);
            let handles = [ok(dispatcher.grant(key(40), pending[0].id))];
            let before = database::snapshot(&files.store);
            let reserved = checkpoint(&files.receiver);
            let mut worker = launch(
                &runtime,
                &image,
                mode,
                &files.store,
                &handles,
                if mode == "wrong-principal" { 41 } else { 40 },
            );
            if mode == "wrong-principal" {
                assert_eq!(
                    ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
                    Some(Status::DispatcherRefused(Refusal::Principal))
                );
                assert_eq!(ok(worker.exchange(&mut dispatcher)), None);
            } else {
                if mode == "replay" {
                    assert_eq!(
                        ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
                        Some(Status::Sent)
                    );
                }
                let expected = match mode {
                    "forged" => ChannelError::Format,
                    "slot" => ChannelError::Slot,
                    _ => ChannelError::Sequence,
                };
                assert!(
                    matches!(worker.exchange(&mut dispatcher),Err(native_worker::Error::Protocol(reason)) if reason==expected)
                );
                assert!(matches!(
                    worker.exchange(&mut dispatcher),
                    Err(native_worker::Error::Stopped)
                ));
            }
            let report = ok(worker.finish());
            if mode == "wrong-principal" {
                clean(&report);
            }
            assert_eq!(calls.borrow().len(), usize::from(mode == "replay"));
            assert_eq!(database::snapshot(&files.store), before);
            if mode != "replay" {
                assert_eq!(checkpoint(&files.receiver), reserved);
            }
        }
    });
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn native_unknown_and_lost_reply_preserve_durable_attempt() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        for mode in ["unknown", "lost-reply"] {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut dispatcher = create(
                &files,
                lineage,
                &calls,
                if mode == "unknown" {
                    Mode::Unknown
                } else {
                    Mode::Success
                },
            );
            let handles = [ok(dispatcher.grant(key(40), pending[0].id))];
            let mut worker = launch(&runtime, &image, mode, &files.store, &handles, 40);
            let first = ok(worker.exchange(&mut dispatcher));
            assert_eq!(
                first.map(|reply| reply.status),
                Some(if mode == "unknown" {
                    Status::Unknown
                } else {
                    Status::Sent
                })
            );
            if mode == "unknown" {
                assert_eq!(
                    ok(worker.exchange(&mut dispatcher)).map(|reply| reply.status),
                    Some(Status::PolicyRefused)
                );
            }
            assert_eq!(ok(worker.exchange(&mut dispatcher)), None);
            let report = ok(worker.finish());
            if mode == "unknown" {
                clean(&report);
            } else {
                // Bwrap init can retain a pipe reader. Queuing this reply is not
                // evidence that the native application received it.
                clean_exit_after_dropped_reply(&report);
            }
            assert_eq!(calls.borrow().len(), 1);
            let reserved = checkpoint(&files.receiver);
            drop(dispatcher);
            let mut dispatcher = reopen(&files, lineage, &calls);
            let replacement = ok(dispatcher.grant(key(40), pending[0].id));
            assert_eq!(
                ok(dispatcher.session(key(20), key(40)).dispatch(&replacement)),
                Sent::Refused(Code::PolicyDuplicate)
            );
            assert_eq!(checkpoint(&files.receiver), reserved);
            assert_eq!(calls.borrow().len(), 1);
        }
    });
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn native_truncation_stderr_pressure_cpu_and_wall_limits() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        for mode in ["truncated", "stderr", "spin", "wait", "spoof-control"] {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut dispatcher = create(&files, lineage, &calls, Mode::Success);
            let handles = [ok(dispatcher.grant(key(40), pending[0].id))];
            let reserved = checkpoint(&files.receiver);
            let mut worker = launch(&runtime, &image, mode, &files.store, &handles, 40);
            if mode == "truncated" {
                assert!(matches!(
                    worker.exchange(&mut dispatcher),
                    Err(native_worker::Error::Truncated)
                ));
            } else {
                assert_eq!(ok(worker.exchange(&mut dispatcher)), None);
            }
            let report = ok(worker.finish());
            match mode {
                "truncated" => {
                    assert_eq!(report.termination, Termination::Exited);
                    assert_eq!(report.worker_exit, 0);
                    assert_eq!(report.output_bytes, 7);
                }
                "stderr" => {
                    assert_eq!(report.termination, Termination::OutputLimit);
                    assert!(report.stderr_bytes > 2048);
                }
                "spin" => {
                    assert_eq!(report.termination, Termination::Exited);
                    assert_eq!(report.worker_exit, 137);
                }
                "wait" => assert_eq!(report.termination, Termination::WallTimeout),
                "spoof-control" => {
                    let mut spoof = [0; 64];
                    spoof[..8].copy_from_slice(b"ZFNEND01");
                    spoof[8] = 1;
                    assert_eq!(report.termination, Termination::Exited);
                    assert_eq!(report.worker_exit, 0);
                    assert_eq!(report.stderr_bytes, 64);
                    assert_eq!(report.stderr_prefix_sha256, RustCryptoSha256::hash(&spoof));
                    assert_eq!((report.output_bytes, report.input_bytes), (0, 0));
                }
                _ => unreachable!(),
            }
            assert!(calls.borrow().is_empty());
            assert_eq!(checkpoint(&files.receiver), reserved);
        }
    });
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn runtime_inventory_hash_and_image_refusals_precede_dispatch() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut dispatcher = create(&files, lineage, &calls, Mode::Success);
        let handles = [ok(dispatcher.grant(key(40), pending[0].id))];
        let reserved = checkpoint(&files.receiver);
        let channel = || ok(Channel::new(key(20), key(40), &handles, 1));
        let mut missing = runtime.clone();
        assert!(!missing.dependencies.is_empty());
        missing.dependencies.remove(0);
        assert!(matches!(
            Worker::spawn(
                &missing,
                &image,
                RustCryptoSha256::hash(&image),
                &["once", "/absent"],
                channel()
            ),
            Err(native_worker::Error::Bootstrap)
        ));
        let mut wrong = runtime.clone();
        wrong.libc = ok(PinnedFile::new(
            "/usr/lib/x86_64-linux-gnu/libc.so.6",
            key(999),
        ));
        assert!(matches!(
            Worker::spawn(
                &wrong,
                &image,
                RustCryptoSha256::hash(&image),
                &[],
                channel()
            ),
            Err(native_worker::Error::Runtime)
        ));
        let mut duplicate = runtime.clone();
        duplicate.dependencies.push(duplicate.python.clone());
        assert!(matches!(
            Worker::spawn(
                &duplicate,
                &image,
                RustCryptoSha256::hash(&image),
                &[],
                channel()
            ),
            Err(native_worker::Error::Configuration)
        ));
        assert!(matches!(
            Worker::spawn(&runtime, &image, key(999), &[], channel()),
            Err(native_worker::Error::Image)
        ));
        let mut bad = image.clone();
        bad[0] = 0;
        assert!(matches!(
            Worker::spawn(&runtime, &bad, RustCryptoSha256::hash(&bad), &[], channel()),
            Err(native_worker::Error::Image)
        ));
        assert!(calls.borrow().is_empty());
        assert_eq!(checkpoint(&files.receiver), reserved);
    });
}

struct Requester(Child);
impl Drop for Requester {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn process(pid: u32) -> Option<(u32, u64, char)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = text.get(text.rfind(')')? + 2..)?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    Some((
        pid,
        fields.get(19)?.parse().ok()?,
        fields.first()?.chars().next()?,
    ))
}
fn tree(pid: u32, out: &mut Vec<(u32, u64, char)>) {
    if let Some(identity) = process(pid) {
        out.push(identity);
        let mut children = std::collections::BTreeSet::new();
        if let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                if let Ok(text) = fs::read_to_string(task.path().join("children")) {
                    children.extend(
                        text.split_whitespace()
                            .filter_map(|value| value.parse::<u32>().ok()),
                    );
                }
            }
        }
        for child in children {
            tree(child, out);
        }
    }
}

#[test]
#[ignore = "invoked only by the requester-loss test with its own evidence path"]
fn native_requester_role() {
    let (runtime, image) = inputs();
    let channel = ok(Channel::new(key(20), key(40), &[], 1));
    let worker = ok(Worker::spawn(
        &runtime,
        &image,
        RustCryptoSha256::hash(&image),
        // A sleeping leaf does not cooperate with cleanup through stdin EOF or
        // the CPU budget. The observing test waits for this actual image.
        &["sleep", "/absent"],
        channel,
    ));
    let record = ok(std::env::var("ZENO_NATIVE_DEATH_RECORD"));
    ok(fs::write(
        record,
        format!(
            "{} {:?}\n",
            worker.supervisor_pid(),
            worker.container_pids()
        ),
    ));
    thread::sleep(Duration::from_secs(30));
    drop(worker);
}
#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn actual_requester_process_death_reaps_owned_native_tree() {
    let root = PathBuf::from(ok(std::env::var("ZENO_NATIVE_EVIDENCE_DIR")));
    let record = root.join(format!("requester-loss-{}.txt", std::process::id()));
    assert!(!record.exists());
    let child = ok(Command::new(ok(std::env::current_exe()))
        .args([
            "--exact",
            "native_worker_process::native_requester_role",
            "--ignored",
            "--nocapture",
        ])
        .env("ZENO_NATIVE_DEATH_RECORD", &record)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn());
    let mut child = Requester(child);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut before = Vec::new();
    loop {
        before.clear();
        tree(child.0.id(), &mut before);
        let native_sleeping_leaf = before.iter().any(|identity| {
            identity.2 != 'Z'
                && fs::read(format!("/proc/{}/cmdline", identity.0))
                    .is_ok_and(|args| args.as_slice() == b"/worker\0sleep\0/absent\0")
                && process(identity.0).is_some_and(|current| current.1 == identity.1)
        });
        if record.exists() && before.len() >= 5 && native_sleeping_leaf {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "requester did not expose its owned tree and actual sleeping /worker: {before:?}"
        );
        assert!(
            ok(child.0.try_wait()).is_none(),
            "requester exited before observation"
        );
        thread::sleep(Duration::from_millis(10));
    }
    println!("owned requester/native tree before kill: {before:?}");
    ok(child.0.kill());
    ok(child.0.wait());
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let survivors: Vec<_> = before
            .iter()
            .filter_map(|old| process(old.0).filter(|new| new.1 == old.1))
            .collect();
        if survivors.is_empty() {
            break;
        }
        assert!(
            Instant::now() < until,
            "owned process identities survived: {survivors:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    println!("owned requester/native tree after kill: empty");
}

#[test]
#[ignore = "required by the native-worker-host ATDD gate on a qualified Linux host"]
fn native_transport_unwind_stops_worker_and_retains_attempt() {
    let (runtime, image) = inputs();
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut dispatcher = create(&files, lineage, &calls, Mode::Panic);
        let handles = [ok(dispatcher.grant(key(40), pending[0].id))];
        let mut worker = launch(&runtime, &image, "once", &files.store, &handles, 40);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            worker.exchange(&mut dispatcher)
        }));
        assert!(result.is_err());
        assert!(matches!(
            worker.exchange(&mut dispatcher),
            Err(native_worker::Error::Stopped)
        ));
        let _report = ok(worker.finish());
        assert_eq!(calls.borrow().len(), 1);
        let reserved = checkpoint(&files.receiver);
        drop(dispatcher);
        let mut dispatcher = reopen(&files, lineage, &calls);
        let handle = ok(dispatcher.grant(key(40), pending[0].id));
        assert_eq!(
            ok(dispatcher.session(key(20), key(40)).dispatch(&handle)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(checkpoint(&files.receiver), reserved);
    });
}
