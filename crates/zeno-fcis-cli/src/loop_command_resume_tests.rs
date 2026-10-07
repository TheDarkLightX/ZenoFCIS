//! Durable accounting of resume replays (review finding: resumed checker
//! work ran before its reservation was durable). Each test drives the real
//! shell entry points against a session directory on disk and interrupts the
//! resume replay through the test-only probe at the start of replay work.

use crate::loop_command::{ProposerArg, candidate_command, open, resume_command, run_command};
use crate::neural_loop::canonical_json;
use crate::neural_loop::incumbent::Cost;
use crate::neural_loop::ledger::{Accounting, Ledger};
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
use crate::neural_loop::program_json::encode;
use crate::neural_loop::request::{Policy, Request};
use crate::neural_loop::session::replay_probe;
use crate::transform::sha256_hex;
use serde_json::Value;
use std::cell::RefCell;
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_synthesis::finite::Program;
use zeno_fcis_synthesis::finite_runtime::import_program;

static NEXT: AtomicU64 = AtomicU64::new(0);

const ORIGINAL: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-original.zcve"
);
const CANDIDATE: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-candidate.zcve"
);
const PADDED: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-padded.zcve"
);

/// A scratch directory with the session's input files; removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let path = std::env::temp_dir().join(format!(
            "zeno-loop-resume-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        for (name, bytes) in [
            ("original.zcve", ORIGINAL.to_vec()),
            ("candidate.zcve", CANDIDATE.to_vec()),
            ("padded.zcve", PADDED.to_vec()),
            ("swapped.zcve", swapped()),
        ] {
            fs::write(path.join(name), bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        Scratch(path)
    }

    fn input(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn session(&self) -> PathBuf {
        self.0.join("session")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The improving candidate with its two output roots swapped: different
/// from the original, so the check stores a witness that resume replays.
fn swapped() -> Vec<u8> {
    let candidate = import_program(CANDIDATE).unwrap_or_else(|error| panic!("{error}"));
    let mut roots = candidate.roots().to_vec();
    roots.swap(0, 1);
    let program = Program::try_new(
        candidate.inputs().to_vec(),
        candidate.outputs().to_vec(),
        candidate.nodes().to_vec(),
        roots,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    encode(&program).unwrap_or_else(|error| panic!("{error:?}"))
}

/// The checker work one replay of the improving candidate reserves.
fn replay_work(limits: Limits) -> u64 {
    let request = Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        limits,
        Policy::DISABLED,
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"));
    let program = import_program(CANDIDATE).unwrap_or_else(|error| panic!("{error}"));
    request
        .check_work(Cost::measure(&program, CANDIDATE))
        .unwrap_or_else(|| panic!("work"))
}

/// Opens a session and makes the improving candidate its checked incumbent.
/// The session stays open, so `candidate` and `run` can still use it.
fn session_with_replacement(scratch: &Scratch, limits: Limits) {
    let (exit, report) = open(
        &scratch.input("original.zcve"),
        &scratch.session(),
        Profile::FunctionalBoolV1,
        limits,
    );
    assert_eq!(exit, crate::OK, "{report}");
    let (exit, report) = candidate_command(
        &scratch.session(),
        Some(&scratch.input("candidate.zcve")),
        None,
        None,
    );
    assert_eq!(exit, crate::OK, "{report}");
    assert_eq!(report["status"], "checked-improvement", "{report}");
}

/// The ledger exactly as it is on disk.
fn durable_accounting(session: &Path) -> Accounting {
    let bytes = fs::read(session.join("ledger.jsonl")).unwrap_or_default();
    Ledger::parse_lines(&bytes)
        .and_then(Ledger::from_entries)
        .unwrap_or_else(|fault| panic!("{fault:?}"))
        .accounting()
}

/// The durable ledger and head bytes.
fn durable_bytes(session: &Path) -> (Vec<u8>, Vec<u8>) {
    let read = |name: &str| fs::read(session.join(name)).unwrap_or_default();
    (read("ledger.jsonl"), read("ledger.head"))
}

/// Simulates the process dying once the next replay has started: the probe
/// panics at the start of replay work and the command never returns.
fn interrupted<T>(command: impl FnOnce() -> T) {
    replay_probe::arm(|| panic!("injected interruption: resume replay started"));
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(command));
    assert!(
        outcome.is_err(),
        "the injected interruption must stop the command"
    );
}

/// A command that loads the session, by name.
type EntryPoint = (&'static str, fn(&Scratch));

#[test]
fn an_interrupted_resume_replay_keeps_its_charge_on_every_entry_point() {
    let work = replay_work(Limits::CEILING);
    let entry_points: [EntryPoint; 3] = [
        ("resume", |scratch| {
            resume_command(&scratch.session());
        }),
        ("candidate", |scratch| {
            candidate_command(
                &scratch.session(),
                Some(&scratch.input("padded.zcve")),
                None,
                None,
            );
        }),
        ("run", |scratch| {
            run_command(&scratch.session(), ProposerArg::Local, None, None);
        }),
    ];
    for (name, entry_point) in entry_points {
        let scratch = Scratch::new();
        session_with_replacement(&scratch, Limits::CEILING);
        let session = scratch.session();
        let before = durable_accounting(&session);
        interrupted(|| entry_point(&scratch));
        let after = durable_accounting(&session);
        // The interrupted replay's reservation is durable and stays charged.
        assert_eq!(after.replays, before.replays + 1, "{name}");
        assert_eq!(after.work_reserved, before.work_reserved + work, "{name}");
        // Nothing else happened: no attempt began.
        assert_eq!(after.attempts, before.attempts, "{name}");
        // The next resume neither reuses the interrupted replay nor retries
        // on the old allowance: it reserves and pays for a new one.
        let (exit, report) = resume_command(&session);
        assert_eq!(exit, crate::OK, "{name}: {report}");
        let report = &report["detail"]["report"];
        assert_eq!(report["status"], "best-checked-so-far", "{name}");
        assert_eq!(
            report["accounting"]["replays"],
            before.replays + 2,
            "{name}"
        );
        let again = durable_accounting(&session);
        assert_eq!(again.replays, before.replays + 2, "{name}");
        assert_eq!(
            again.work_reserved,
            before.work_reserved + 2 * work,
            "{name}"
        );
    }
}

#[test]
fn a_run_refused_after_its_replay_keeps_the_replay_charge() {
    // `run` resumes before it reads its proposer configuration; a refused
    // configuration must not discard the replay it already paid for.
    let scratch = Scratch::new();
    session_with_replacement(&scratch, Limits::CEILING);
    let session = scratch.session();
    let before = durable_accounting(&session);
    let missing = scratch.input("missing-script.json");
    let (exit, report) = run_command(&session, ProposerArg::Fake, Some(&missing), None);
    assert_eq!(exit, crate::FAILURE, "{report}");
    let bad = scratch.input("candidate.zcve");
    let (exit, report) = run_command(&session, ProposerArg::Hosted, None, Some(&bad));
    assert_eq!(exit, crate::INVALID, "{report}");
    let after = durable_accounting(&session);
    assert_eq!(after.replays, before.replays + 2);
    assert_eq!(
        after.work_reserved,
        before.work_reserved + 2 * replay_work(Limits::CEILING)
    );
    assert_eq!(after.attempts, before.attempts);
}

#[test]
fn the_replay_reservation_is_durable_before_any_replay_work() {
    let scratch = Scratch::new();
    session_with_replacement(&scratch, Limits::CEILING);
    let session = scratch.session();
    let before = durable_accounting(&session);
    let seen: Rc<RefCell<Option<Accounting>>> = Rc::new(RefCell::new(None));
    let probe = Rc::clone(&seen);
    let at = session.clone();
    replay_probe::arm(move || *probe.borrow_mut() = Some(durable_accounting(&at)));
    let (exit, report) = resume_command(&session);
    assert_eq!(exit, crate::OK, "{report}");
    let seen = seen
        .borrow_mut()
        .take()
        .unwrap_or_else(|| panic!("the replay probe must run"));
    assert_eq!(seen.replays, before.replays + 1);
    assert_eq!(
        seen.work_reserved,
        before.work_reserved + replay_work(Limits::CEILING)
    );
    // The completed resume writes nothing beyond that reservation.
    assert_eq!(durable_accounting(&session), seen);
}

#[test]
fn a_resume_that_cannot_afford_a_new_replay_refuses_and_writes_nothing() {
    let work = replay_work(Limits::CEILING);
    let self_check = Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"))
    .check_work(
        Request::admit(
            ORIGINAL,
            Profile::FunctionalBoolV1,
            Limits::CEILING,
            Policy::DISABLED,
        )
        .unwrap_or_else(|refusal| panic!("{refusal:?}"))
        .cost(),
    )
    .unwrap_or_else(|| panic!("work"));
    // Room for the candidate check and exactly one replay of it.
    let limits = Limits {
        check_work: self_check,
        session_work: 2 * work,
        ..Limits::CEILING
    };
    assert!(work <= self_check && self_check <= limits.session_work);
    let scratch = Scratch::new();
    session_with_replacement(&scratch, limits);
    let session = scratch.session();
    assert_eq!(durable_accounting(&session).work_reserved, work);
    interrupted(|| resume_command(&session));
    assert_eq!(durable_accounting(&session).work_reserved, 2 * work);
    let durable = durable_bytes(&session);
    for (exit, report) in [
        resume_command(&session),
        candidate_command(&session, Some(&scratch.input("padded.zcve")), None, None),
        run_command(&session, ProposerArg::Local, None, None),
    ] {
        assert_eq!(exit, crate::BLOCKED, "{report}");
        assert_eq!(report["status"], "resume-inconclusive", "{report}");
        assert_eq!(
            report["detail"]["refusal"]["reason"],
            "insufficient-replay-allowance"
        );
        assert_eq!(report["detail"]["trusted_incumbent"], Value::Null);
        assert_eq!(durable_bytes(&session), durable, "nothing more is written");
    }
}

/// Digests of everything an uninterrupted scenario writes and reports,
/// recorded from the base commit (39bc5e1) before this repair: the repair
/// moves the replay reservation's write earlier and changes nothing else.
const BASE_LEDGER_SHA256: &str = "5724eb1aa68e3d0e2d504aeca528dee6414a1ab838d230ba53ca795f64a0f790";
const BASE_HEAD_SHA256: &str = "b005f1c035811baf99c20f065e8616fc095baf5c2765c3786e433eb97fafe4be";
const BASE_OUTPUTS_SHA256: &str =
    "caab6f05a3ff8c52ab2deb856fe72fc8ac1dec6aee6532bfa64a798a3f2d1131";

#[test]
fn an_uninterrupted_resume_writes_and_reports_what_the_base_commit_did() {
    let scratch = Scratch::new();
    let session = scratch.session();
    let (exit, report) = open(
        &scratch.input("original.zcve"),
        &session,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
    );
    assert_eq!(exit, crate::OK, "{report}");
    let supply = |name: &str| candidate_command(&session, Some(&scratch.input(name)), None, None);
    let outputs = [
        supply("swapped.zcve"),
        supply("candidate.zcve"),
        resume_command(&session),
        supply("padded.zcve"),
        resume_command(&session),
        run_command(&session, ProposerArg::Local, None, None),
        resume_command(&session),
    ];
    let statuses: Vec<&Value> = outputs.iter().map(|(_, value)| &value["status"]).collect();
    assert_eq!(
        statuses,
        [
            "different",
            "checked-improvement",
            "resumed",
            "equivalent-without-improvement:original-cost-guard",
            "resumed",
            "completed",
            "resumed",
        ],
        "{outputs:?}"
    );
    let mut transcript = Vec::new();
    for (exit, value) in &outputs {
        transcript.push(*exit);
        transcript.extend_from_slice(&canonical_json(value));
    }
    let (ledger, head) = durable_bytes(&session);
    let found = [
        sha256_hex(&ledger),
        sha256_hex(&head),
        sha256_hex(&transcript),
    ];
    eprintln!("uninterrupted scenario digests: {found:?}");
    assert_eq!(
        found,
        [BASE_LEDGER_SHA256, BASE_HEAD_SHA256, BASE_OUTPUTS_SHA256]
    );
}
