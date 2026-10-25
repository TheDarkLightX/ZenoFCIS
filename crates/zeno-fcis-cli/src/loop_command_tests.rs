//! Shell tests: the supervisor, the session store, the driver with each
//! proposer, crash recovery and the hosted adapter's silence.

#[cfg(target_os = "linux")]
use super::drive;
use super::{OptimizeEngine, Store, strategy_engine, supervise};
#[cfg(target_os = "linux")]
use crate::loop_proposers::{Fake, Hosted, HostedConfig, Local, Proposer, SCRIPT_SCHEMA};
use crate::neural_loop::acquisition::{Acquisition, Mode as AcquisitionMode};
#[cfg(target_os = "linux")]
use crate::neural_loop::incumbent::Incumbent;
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
use crate::neural_loop::request::{Policy, Request};
#[cfg(target_os = "linux")]
use crate::neural_loop::session::{ResumePlan, ResumeRefusal, Resumed, Status, StopReason};
use crate::neural_loop::session::{Session, WorkerFailure};
#[cfg(target_os = "linux")]
use crate::neural_loop::strategy::NoEngine;
use crate::neural_loop::strategy::{SearchReport, Strategy, StrategyEngine};
use crate::optimize::strategy::{DEFAULT_STRATEGY_JSON, Phase};
use serde_json::{Value, json};
#[cfg(target_os = "linux")]
use std::convert::Infallible;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Successful replay requires the Linux containment backend.
#[cfg(target_os = "linux")]
#[path = "loop_command_resume_tests.rs"]
mod resume;

#[path = "loop_command_intake_tests.rs"]
mod intake;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Directory {
        let path = std::env::temp_dir().join(format!(
            "zeno-loop-shell-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Directory(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ORIGINAL: &[u8] =
    include_bytes!("../tests/fixtures/i128-programs/boolean-kernel-original.zcve");
#[cfg(target_os = "linux")]
const CANDIDATE: &[u8] =
    include_bytes!("../tests/fixtures/i128-programs/boolean-kernel-candidate.zcve");
#[cfg(target_os = "linux")]
const PADDED: &[u8] = include_bytes!("../tests/fixtures/i128-programs/boolean-kernel-padded.zcve");

fn request() -> Request {
    Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"))
}

#[cfg(target_os = "linux")]
fn engine() -> Arc<dyn StrategyEngine + Send + Sync> {
    Arc::new(NoEngine)
}

fn open_store(dir: &Path) -> (Store, Session) {
    let session = Session::open(request());
    let mut store = Store::create(dir, session.request()).unwrap_or_else(|error| panic!("{error}"));
    store
        .persist(session.ledger())
        .unwrap_or_else(|error| panic!("{error}"));
    (store, session)
}

/// Resumes a stored session in memory. The replay reservation goes to a
/// write that keeps nothing, so the directory stays exactly as the test left
/// it; `load_session` is the durable route, which `resume_tests` covers.
#[cfg(target_os = "linux")]
fn reopen(dir: &Path) -> Result<Session, ResumeRefusal> {
    let mut acquisition = Acquisition::reserve(AcquisitionMode::Resume);
    let (_, loaded) =
        Store::acquire(dir, &mut acquisition).unwrap_or_else(|error| panic!("{error}"));
    match loaded.prepare(0) {
        ResumePlan::Replay(pending) => {
            let reserved = pending
                .reserve(|_| Ok::<(), Infallible>(()))
                .unwrap_or_else(|never| match never {});
            match reserved.replay() {
                Resumed::Session(session) => Ok(*session),
                Resumed::Refused(reason) => Err(reason),
            }
        }
        ResumePlan::Refused(reason) | ResumePlan::Inconclusive(reason) => Err(reason),
    }
}

#[cfg(target_os = "linux")]
fn resumed(dir: &Path) -> Session {
    reopen(dir).unwrap_or_else(|reason| panic!("expected a session: {reason:?}"))
}

#[cfg(target_os = "linux")]
fn script(dir: &Directory, steps: Value) -> Fake {
    let value = json!({"schema": SCRIPT_SCHEMA, "steps": steps});
    Fake::from_script(&value, &dir.0).unwrap_or_else(|error| panic!("{error:?}"))
}

#[test]
fn the_supervisor_reports_panics_and_deadlines_without_propagating() {
    assert_eq!(supervise(Duration::from_secs(1), || 7), Ok(7));
    assert_eq!(
        supervise(Duration::from_secs(1), || -> u8 {
            panic!("planted worker panic")
        }),
        Err(WorkerFailure::Panicked)
    );
    let started = Instant::now();
    assert_eq!(
        supervise(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(400));
            1
        }),
        Err(WorkerFailure::Timeout)
    );
    // Cooperative checking retains the worker until it finishes; actual return-time
    // enforcement is tested through the contained production process.
    assert!(started.elapsed() >= Duration::from_millis(400));
}

#[cfg(target_os = "linux")]
#[test]
fn a_panicking_or_slow_check_worker_leaves_the_incumbent_unchanged() {
    let directory = Directory::new();
    let (mut store, mut session) = open_store(&directory.0.join("session"));
    let mut fake = script(
        &directory,
        json!([{"candidate": "candidate.zcve"}, {"candidate": "padded.zcve"}]),
    );
    directory.write("candidate.zcve", CANDIDATE);
    directory.write("padded.zcve", PADDED);
    drive(
        &mut session,
        &mut store,
        &mut fake,
        &engine(),
        Instant::now(),
        Some(1),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let before = session
        .incumbent()
        .replacement()
        .map(|r| r.receipt_sha256().to_owned());
    assert!(before.is_some());
    // The shell's supervisor turns a panic into a failed report; the core
    // settles the attempt and the incumbent stands.
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let super::Prepared::Check(job) = session.prepare(
        ticket,
        crate::neural_loop::session::Proposal::Candidate(PADDED.to_vec()),
    ) else {
        panic!("expected a check job");
    };
    let report = supervise(
        Duration::from_secs(1),
        || -> crate::neural_loop::session::CheckReport {
            panic!("planted panic inside the check stage")
        },
    )
    .unwrap_or_else(crate::neural_loop::session::CheckReport::Failed);
    let outcome = session.conclude(job, report);
    assert_eq!(outcome.name(), "check-failed:panicked");
    assert_eq!(
        session
            .incumbent()
            .replacement()
            .map(|r| r.receipt_sha256().to_owned()),
        before
    );
    store
        .persist(session.ledger())
        .unwrap_or_else(|error| panic!("{error}"));
    let resumed = resumed(&directory.0.join("session"));
    assert_eq!(
        resumed
            .incumbent()
            .replacement()
            .map(|r| r.receipt_sha256().to_owned()),
        before
    );
    assert_eq!(resumed.ledger().accounting().attempts, 2);
}

#[cfg(target_os = "linux")]
#[test]
fn the_store_round_trips_a_session_and_keeps_crash_pending_charges() {
    let directory = Directory::new();
    let dir = directory.0.join("session");
    let (mut store, mut session) = open_store(&dir);
    for name in [
        "request.json",
        "original.zcve",
        "ledger.jsonl",
        "ledger.head",
    ] {
        assert!(dir.join(name).is_file(), "{name}");
    }
    assert_eq!(
        fs::read(dir.join("original.zcve")).ok().as_deref(),
        Some(ORIGINAL)
    );
    directory.write("candidate.zcve", CANDIDATE);
    let mut fake = script(
        &directory,
        json!([{"candidate": "candidate.zcve", "claims": {"passed": true}}]),
    );
    drive(
        &mut session,
        &mut store,
        &mut fake,
        &engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::ProposerExhausted)
    );
    let replacement = session
        .incumbent()
        .replacement()
        .unwrap_or_else(|| panic!("improvement"));
    let artifact = dir
        .join("artifacts")
        .join(format!("{}.zcve", replacement.sha256()));
    assert_eq!(fs::read(&artifact).ok().as_deref(), Some(CANDIDATE));
    assert_eq!(
        fs::read(dir.join("receipts/0.json")).ok().as_deref(),
        Some(replacement.receipt())
    );
    let transcript = fs::read_to_string(dir.join("transcript.jsonl")).unwrap_or_default();
    assert!(transcript.contains("\"provider_asserted\":{\"passed\":true}"));
    let reopened = resumed(&dir);
    assert_eq!(reopened.incumbent(), session.incumbent());
    assert_eq!(
        *reopened.status(),
        Status::Closed(StopReason::ProposerExhausted)
    );
    let totals = reopened.ledger().accounting();
    assert_eq!(
        (
            totals.attempts,
            totals.model_calls,
            totals.checks,
            totals.replays
        ),
        (1, 1, 1, 1)
    );

    // A fresh session that dies between the check reservation and the reply.
    let dir = directory.0.join("crash");
    let (mut store, mut session) = open_store(&dir);
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    store
        .persist(session.ledger())
        .unwrap_or_else(|error| panic!("{error}"));
    let super::Prepared::Check(job) = session.prepare(
        ticket,
        crate::neural_loop::session::Proposal::Candidate(CANDIDATE.to_vec()),
    ) else {
        panic!("expected a check job");
    };
    store
        .persist(session.ledger())
        .unwrap_or_else(|error| panic!("{error}"));
    store.store_artifact(job.candidate()).ok();
    drop((store, session, job));
    let recovered = resumed(&dir);
    let totals = recovered.ledger().accounting();
    assert_eq!(totals.unresolved, vec![0]);
    assert_eq!((totals.attempts, totals.checks), (1, 1));
    assert_eq!(*recovered.incumbent(), Incumbent::OriginalAdmitted);
    assert_eq!(recovered.report()["accounting"]["complete"], false);
}

#[cfg(target_os = "linux")]
#[test]
fn a_rolled_back_or_edited_ledger_is_refused_on_reopen() {
    let directory = Directory::new();
    let dir = directory.0.join("session");
    let (mut store, mut session) = open_store(&dir);
    directory.write("candidate.zcve", CANDIDATE);
    let mut fake = script(&directory, json!([{"candidate": "candidate.zcve"}]));
    drive(
        &mut session,
        &mut store,
        &mut fake,
        &engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let ledger = fs::read_to_string(dir.join("ledger.jsonl")).unwrap_or_default();
    let head = fs::read(dir.join("ledger.head")).unwrap_or_default();
    // Drop the last two lines: the head no longer matches.
    let lines: Vec<&str> = ledger.lines().collect();
    let shorter = lines[..lines.len() - 2].join("\n") + "\n";
    fs::write(dir.join("ledger.jsonl"), shorter).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        reopen(&dir),
        Err(ResumeRefusal::RollbackSuspected { .. })
    ));
    fs::write(dir.join("ledger.jsonl"), &ledger).unwrap_or_else(|error| panic!("{error}"));
    assert!(reopen(&dir).is_ok());
    // A missing head is unverifiable.
    fs::remove_file(dir.join("ledger.head")).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(reopen(&dir), Err(ResumeRefusal::Unverifiable)));
    fs::write(dir.join("ledger.head"), &head).unwrap_or_else(|error| panic!("{error}"));
    // An edited receipt file no longer matches the ledger's digest.
    let receipt = dir.join("receipts/0.json");
    let mut bytes = fs::read(&receipt).unwrap_or_default();
    bytes[12] ^= 1;
    fs::write(&receipt, &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        reopen(&dir),
        Err(ResumeRefusal::ReplacementDigest)
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn the_local_proposer_drives_to_exhaustion_and_the_hosted_adapter_stays_silent() {
    let directory = Directory::new();
    let (mut store, mut session) = open_store(&directory.0.join("local"));
    let mut local = Local::new(session.request().program(), session.request().original());
    assert_eq!(local.identity(), "zeno-fcis/local-proposer/1");
    assert!(!local.uses_model_calls());
    drive(
        &mut session,
        &mut store,
        &mut local,
        &engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::ProposerExhausted)
    );
    assert_eq!(session.report()["incumbent"]["cost"]["nodes"], 14);
    assert_eq!(session.ledger().accounting().model_calls, 0);

    // The hosted adapter: one unavailable answer, one attempt and one call
    // charged, no network, no credential read, nothing leaked.
    let secret = "SECRET-7f3a-do-not-leak";
    // SAFETY-free: the standard library's set_var is safe in Rust 2024 only
    // when single-threaded; the test harness may run threads, so we avoid it
    // and simply name a variable the adapter would never read.
    let config = HostedConfig {
        enabled: false,
        provider: Some("example-provider".to_owned()),
        credential_env: Some("ZENO_FCIS_TEST_SECRET".to_owned()),
        money_micros: 0,
    };
    assert_eq!(
        HostedConfig::from_json(&json!({
            "schema": "zeno-fcis/hosted-provider-config/1", "enabled": false,
            "provider": "example-provider", "credential_env": "ZENO_FCIS_TEST_SECRET", "money_micros": 0
        })),
        Some(config.clone())
    );
    assert_eq!(
        HostedConfig::from_json(
            &json!({"schema": "zeno-fcis/hosted-provider-config/1", "credential": secret})
        ),
        None
    );
    let dir = directory.0.join("hosted");
    let (mut store, mut session) = open_store(&dir);
    let mut hosted = Hosted::new(config);
    assert!(hosted.is_hosted());
    drive(
        &mut session,
        &mut store,
        &mut hosted,
        &engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        *session.status(),
        Status::Closed(StopReason::ProposerExhausted)
    );
    let report = session.report();
    assert_eq!(
        report["attempts"][0]["outcome"],
        "proposal-failed:unavailable:hosted-provider-disabled"
    );
    let totals = session.ledger().accounting();
    assert_eq!(
        (totals.attempts, totals.model_calls, totals.money_reserved),
        (1, 1, 0)
    );
    let transcript = fs::read_to_string(dir.join("transcript.jsonl")).unwrap_or_default();
    assert!(transcript.contains("\"credential\":\"never-read\""));
    for text in [report.to_string(), transcript] {
        assert!(!text.contains(secret));
        assert!(!text.contains("ZENO_FCIS_TEST_SECRET") || text.contains("never-read"));
    }
    let enabled = Hosted::new(HostedConfig {
        enabled: true,
        ..HostedConfig::DISABLED
    });
    assert_eq!(enabled.unavailable_reason(), "hosted-provider-unapproved");
}

#[cfg(target_os = "linux")]
#[test]
fn the_fake_provider_is_metered_as_a_model_and_its_claims_are_ignored() {
    let directory = Directory::new();
    directory.write("candidate.zcve", CANDIDATE);
    directory.write("padded.zcve", PADDED);
    let steps = json!([
        {"malformed": "not a program", "claims": {"passed": true}},
        {"timeout": true},
        {"candidate": "padded.zcve", "claims": {"equivalent": true, "improvement": true}},
        {"candidate": "candidate.zcve"},
        {"late": true},
        {"candidate": "candidate.zcve"}
    ]);
    let mut fake = script(&directory, steps);
    assert!(fake.uses_model_calls());
    let dir = directory.0.join("session");
    let (mut store, mut session) = open_store(&dir);
    drive(
        &mut session,
        &mut store,
        &mut fake,
        &engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let report = session.report();
    let outcomes: Vec<&str> = report["attempts"]
        .as_array()
        .unwrap_or_else(|| panic!("attempts"))
        .iter()
        .map(|attempt| attempt["outcome"].as_str().unwrap_or_default())
        .collect();
    // Four calls are reservable; the fifth attempt cannot reserve a call.
    assert_eq!(
        outcomes,
        [
            "refused",
            "proposal-failed:timeout",
            "equivalent-without-improvement:original-cost-guard",
            "checked-improvement",
            "proposal-failed:unavailable:model-calls-exhausted",
        ]
    );
    assert_eq!(report["session"]["stop_reason"], "model-calls-exhausted");
    assert_eq!(report["status"], "best-checked-so-far");
    assert_eq!(report["incumbent"]["cost"]["nodes"], 7);
    let totals = session.ledger().accounting();
    assert_eq!(
        (totals.attempts, totals.model_calls, totals.tokens_reserved),
        (5, 4, 24_576)
    );
    assert_eq!(totals.checks, 2);
    // Scripts outside the grammar are refused.
    assert!(Fake::from_script(&json!({"schema": "other", "steps": []}), &directory.0).is_err());
    assert!(
        Fake::from_script(
            &json!({"schema": SCRIPT_SCHEMA, "steps": [{"shell": "rm -rf /"}]}),
            &directory.0
        )
        .is_err()
    );
    assert!(
        Fake::from_script(
            &json!({"schema": SCRIPT_SCHEMA, "steps": [{"candidate": "a", "timeout": true}]}),
            &directory.0
        )
        .is_err()
    );
}

#[test]
fn the_transcript_is_bounded_and_labels_its_truncation() {
    let directory = Directory::new();
    let dir = directory.0.join("session");
    let (mut store, _session) = open_store(&dir);
    let line = json!({"filler": "x".repeat(4_000)});
    for _ in 0..200 {
        store.transcript(&line);
    }
    let transcript = fs::read_to_string(dir.join("transcript.jsonl")).unwrap_or_default();
    assert!(transcript.len() as u64 <= u64::from(Limits::CEILING.transcript_bytes) + 128);
    assert!(transcript.ends_with("{\"advisory_history_omitted\":true,\"truncated\":true}\n"));
    assert_eq!(transcript.matches("truncated").count(), 1);
}

#[test]
fn the_engine_preserves_an_unsupported_original_stop() {
    use zeno_fcis_codec::CanonicalEncode;
    use zeno_fcis_synthesis::finite::{Domain, Op, Program};
    let program = Program::try_new(
        vec![Domain::Int { min: 1, max: 2 }],
        vec![Domain::Int { min: 1, max: 2 }],
        vec![Op::Input(0), Op::Int(1), Op::MulDivCeil(0, 1, 1)],
        vec![2],
    )
    .unwrap_or_else(|error| panic!("witness program: {error}"));
    let bytes = program
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("witness encoding: {error}"));
    let document: Value = serde_json::from_str(DEFAULT_STRATEGY_JSON)
        .unwrap_or_else(|error| panic!("default strategy: {error}"));
    let strategy = Strategy::from_json(&document).unwrap_or_else(|error| panic!("{error:?}"));
    let report = OptimizeEngine.run(&bytes, &strategy, Profile::CheckedI128V1, &[]);
    assert!(
        matches!(&report, SearchReport::Unavailable { reason }
            if reason.contains("unsupported-original") && reason.contains("original-not-typable")),
        "an unrepresentable original did not run a completed search: {report:?}"
    );
}

#[test]
fn the_wired_engine_is_the_optimizer_and_admits_exactly_its_phases() {
    let engine = strategy_engine();
    assert_eq!(engine.identity(), "zeno-fcis/optimize/1");
    let phases = engine.phases();
    // Exhaustive on purpose: a new optimizer phase fails to compile here
    // until the loop's phase table lists it.
    let listed = |phase: Phase| match phase {
        Phase::Boolean
        | Phase::SemanticMerge
        | Phase::Select
        | Phase::Fold
        | Phase::Share
        | Phase::CutRewrite => phases.contains(&phase.name()),
    };
    for phase in [
        Phase::Boolean,
        Phase::SemanticMerge,
        Phase::Select,
        Phase::Fold,
        Phase::Share,
        Phase::CutRewrite,
    ] {
        assert!(listed(phase), "{}", phase.name());
    }
    assert_eq!(phases.len(), 6);
}

#[cfg(target_os = "linux")]
#[test]
fn an_optimizer_strategy_improves_the_kernel_only_through_the_loops_own_check() {
    let document: Value = serde_json::from_str(DEFAULT_STRATEGY_JSON)
        .unwrap_or_else(|error| panic!("default strategy: {error}"));
    // The engine's own verdict is provenance; the bytes are what it emits.
    let strategy = Strategy::from_json(&document).unwrap_or_else(|refusal| panic!("{refusal:?}"));
    let report = OptimizeEngine.run(ORIGINAL, &strategy, Profile::FunctionalBoolV1, &[]);
    let SearchReport::Candidate { bytes, extraction } = report else {
        panic!("expected a candidate, got {report:?}");
    };
    assert!(
        extraction.starts_with("engine-accepted:phase-"),
        "{extraction}"
    );
    let program = zeno_fcis_synthesis::finite_runtime::import_program(&bytes)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(program.nodes().len(), 7);

    // Through the driver: a model proposes the strategy, the engine searches,
    // and only the loop's own check lets the result replace the incumbent.
    let directory = Directory::new();
    let mut fake = script(&directory, json!([{ "strategy": document }]));
    let dir = directory.0.join("session");
    let (mut store, mut session) = open_store(&dir);
    drive(
        &mut session,
        &mut store,
        &mut fake,
        &strategy_engine(),
        Instant::now(),
        None,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let report = session.report();
    let outcomes: Vec<&str> = report["attempts"]
        .as_array()
        .unwrap_or_else(|| panic!("attempts"))
        .iter()
        .map(|attempt| attempt["outcome"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(outcomes, ["checked-improvement"]);
    assert_eq!(report["incumbent"]["cost"]["nodes"], 7);
    assert_eq!(session.ledger().accounting().checks, 1);
}

#[test]
fn the_engine_searches_within_the_requests_profile() {
    use zeno_fcis_codec::CanonicalEncode;
    use zeno_fcis_synthesis::finite::{Domain, Op, Program};
    // Parity of two inputs written with And and Not (the B06 seed's shape).
    let parity = Program::try_new(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Not(1),
            Op::And(0, 2),
            Op::Not(0),
            Op::And(4, 1),
            Op::Not(3),
            Op::Not(5),
            Op::And(6, 7),
            Op::Not(8),
        ],
        vec![9],
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let original = parity
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("{error}"));
    let document: Value = serde_json::from_str(DEFAULT_STRATEGY_JSON)
        .unwrap_or_else(|error| panic!("default strategy: {error}"));
    let strategy = Strategy::from_json(&document).unwrap_or_else(|refusal| panic!("{refusal:?}"));
    for profile in Profile::ALL {
        let SearchReport::Candidate { bytes, .. } =
            OptimizeEngine.run(&original, &strategy, profile, &[])
        else {
            panic!("expected a candidate under {}", profile.name());
        };
        let program = zeno_fcis_synthesis::finite_runtime::import_program(&bytes)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(program.nodes().len(), 4, "{:?}", program.nodes());
        // The request's profile is what the loop will admit the bytes under.
        assert!(profile.admit(&program).is_ok(), "{}", profile.name());
    }
    // A strategy that names another profile than the request's is unavailable.
    let mut named = document.clone();
    named["profile"] = json!("checked-i128-v1");
    let named = Strategy::from_json(&named).unwrap_or_else(|refusal| panic!("{refusal:?}"));
    assert_eq!(
        OptimizeEngine.run(&original, &named, Profile::FunctionalBoolV1, &[]),
        SearchReport::Unavailable {
            reason: String::from("engine:profile-mismatch:checked-i128-v1:functional-bool-v1")
        }
    );
}

#[test]
fn the_engine_fuses_the_loops_checked_candidates() {
    use zeno_fcis_codec::CanonicalEncode;
    use zeno_fcis_synthesis::finite::{Domain, Op, Program};
    let wide = Domain::Int {
        min: 0,
        max: 99_999,
    };
    let full = Domain::Int {
        min: i128::MIN,
        max: i128::MAX,
    };
    let encode = |nodes: Vec<Op>, roots: Vec<u16>| -> Vec<u8> {
        Program::try_new(vec![wide], vec![full, full], nodes, roots)
            .unwrap_or_else(|error| panic!("{error}"))
            .value()
            .and_then(|value| value.canonical_bytes())
            .unwrap_or_else(|error| panic!("{error}"))
    };
    // Outputs (x - 7) + 7 and (x - 9) + 9; each checked candidate simplifies
    // one of them.
    let original = encode(
        vec![
            Op::Input(0),
            Op::Int(7),
            Op::Sub(0, 1),
            Op::Add(2, 1),
            Op::Int(9),
            Op::Sub(0, 4),
            Op::Add(5, 4),
        ],
        vec![3, 6],
    );
    let first = encode(
        vec![Op::Input(0), Op::Int(9), Op::Sub(0, 1), Op::Add(2, 1)],
        vec![0, 3],
    );
    let second = encode(
        vec![Op::Input(0), Op::Int(7), Op::Sub(0, 1), Op::Add(2, 1)],
        vec![3, 0],
    );
    let document: Value = serde_json::from_str(DEFAULT_STRATEGY_JSON)
        .unwrap_or_else(|error| panic!("default strategy: {error}"));
    let strategy = Strategy::from_json(&document).unwrap_or_else(|refusal| panic!("{refusal:?}"));
    // With one checked candidate nothing beats it, so the engine has nothing
    // new for the loop.
    assert_eq!(
        OptimizeEngine.run(
            &original,
            &strategy,
            Profile::CheckedI128V1,
            std::slice::from_ref(&first)
        ),
        SearchReport::Failed {
            reason: String::from(
                "engine:no-checked-improvement; bounded-search:Limits { max_enodes: 20000, max_classes: 10000, max_rewrites_per_round: 10000, max_extraction_rounds: 8, max_work: 100000 }; request a supported lowerable primitive or sketch for the same requirements"
            )
        }
    );
    // With both, fusion makes each output x.
    let SearchReport::Candidate { bytes, extraction } = OptimizeEngine.run(
        &original,
        &strategy,
        Profile::CheckedI128V1,
        &[first, second],
    ) else {
        panic!("expected a fused candidate");
    };
    assert!(
        extraction.starts_with("engine-accepted:phase-"),
        "{extraction}"
    );
    let program = zeno_fcis_synthesis::finite_runtime::import_program(&bytes)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(program.nodes(), &[Op::Input(0)]);
}

#[test]
fn malformed_bootstrap_retains_separate_acquisition_charge() {
    let directory = Directory::new();
    open_store(&directory.0);
    fs::write(directory.0.join("request.json"), b"malformed request")
        .unwrap_or_else(|error| panic!("{error:?}"));
    let (exit, report) = super::resume_command(&directory.0);
    assert_eq!(exit, crate::BLOCKED, "{report}");
    assert_eq!(report["status"], "resume-refused");
    assert!(
        report["acquisition"]["reserved_units"]
            .as_u64()
            .unwrap_or_else(|| panic!("required test value"))
            > 0
    );
    assert!(
        report["acquisition"]["observed_intake"]["bytes"]
            .as_u64()
            .unwrap_or_else(|| panic!("required test value"))
            > 0
    );
    assert_eq!(
        report["acquisition"]["separate_from_session_check_work"],
        true
    );
    assert!(report["detail"].get("report").is_none());
}

#[test]
fn bootstrap_directory_cap_refuses_before_unbounded_collection() {
    let directory = Directory::new();
    open_store(&directory.0);
    for index in 0..=crate::neural_loop::acquisition::DIRECTORY_ENTRIES {
        fs::write(
            directory.0.join("witnesses").join(format!("extra-{index}")),
            b"",
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let ledger_before =
        fs::read(directory.0.join("ledger.jsonl")).unwrap_or_else(|error| panic!("{error:?}"));
    let (exit, report) = super::resume_command(&directory.0);
    assert_eq!(exit, crate::BLOCKED, "{report}");
    assert_eq!(report["status"], "inconclusive");
    assert_eq!(
        report["detail"]["reason"],
        "invocation-acquisition-exhausted"
    );
    assert_eq!(report["acquisition"]["exhausted"], true);
    assert_eq!(
        report["acquisition"]["observed_intake"]["directory_entries"],
        crate::neural_loop::acquisition::DIRECTORY_ENTRIES + 1
    );
    assert_eq!(
        fs::read(directory.0.join("ledger.jsonl")).unwrap_or_else(|error| panic!("{error:?}")),
        ledger_before
    );
}

#[cfg(target_os = "linux")]
#[test]
fn leftover_checker_staging_is_skipped_reported_and_bounded() {
    let directory = Directory::new();
    open_store(&directory.0);
    // Interrupted contained runs can leave their private staging directories.
    for index in 0..crate::neural_loop::acquisition::STAGING_LEFTOVERS + 2 {
        fs::create_dir(directory.0.join("receipts").join(format!(
            "{}1-{index}",
            crate::checker_process::STAGING_PREFIX
        )))
        .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let (exit, report) = super::resume_command(&directory.0);
    assert_eq!(exit, crate::OK, "{report}");
    assert_eq!(report["status"], "resumed");
    assert_eq!(
        report["acquisition"]["leftover_checker_staging"],
        crate::neural_loop::acquisition::STAGING_LEFTOVERS
    );
    // Beyond the fixed allowance they count as ordinary entries, so enough
    // of them still meet the directory cap instead of an unbounded listing.
    for index in 0..crate::neural_loop::acquisition::DIRECTORY_ENTRIES {
        fs::create_dir(directory.0.join("receipts").join(format!(
            "{}2-{index}",
            crate::checker_process::STAGING_PREFIX
        )))
        .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let (exit, report) = super::resume_command(&directory.0);
    assert_eq!(exit, crate::BLOCKED, "{report}");
    assert_eq!(
        report["detail"]["reason"],
        "invocation-acquisition-exhausted"
    );
}

#[test]
fn acquisition_regular_file_eof_and_sentinel_preserve_input_and_accounting() {
    let directory = Directory::new();
    for (index, input) in [b"".as_slice(), b"abc", b"abcd", b"abcde"]
        .into_iter()
        .enumerate()
    {
        let path = directory.write(&format!("input-{index}"), input);
        let mut acquisition = Acquisition::reserve(AcquisitionMode::Open);
        let reserved = acquisition.json()["reserved_units"].clone();
        let bytes = super::acquire_read(&mut acquisition, &path, 3)
            .unwrap_or_else(|error| panic!("read test input: {error}"));
        assert_eq!(bytes, input[..input.len().min(4)]);
        assert_eq!(
            fs::read(path).unwrap_or_else(|error| panic!("reread test input: {error}")),
            input
        );
        let report = acquisition.json();
        assert_eq!(report["reserved_units"], reserved);
        assert_eq!(report["exhausted"], false);
        assert_eq!(report["observed_intake"]["attempted_reads"], 1);
        assert_eq!(report["observed_intake"]["bytes"], bytes.len());
        let calls = report["observed_intake"]["read_calls"]
            .as_u64()
            .unwrap_or_else(|| panic!("missing read-call count"));
        assert!((1..=4).contains(&calls));
    }
}

#[test]
fn acquisition_exhaustion_precedes_file_open_and_failed_open_can_be_followed() {
    let directory = Directory::new();
    let missing = directory.0.join("missing");
    let mut exhausted = Acquisition::reserve(AcquisitionMode::Resume);
    exhausted.stop();
    let before = exhausted.json();
    let error = match super::acquire_read(&mut exhausted, &missing, 3) {
        Err(error) => error,
        Ok(_) => panic!("exhausted acquisition must refuse before open"),
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.to_string(), "invocation-acquisition-exhausted");
    assert_eq!(exhausted.json(), before);

    let mut acquisition = Acquisition::reserve(AcquisitionMode::Resume);
    let error = match super::acquire_read(&mut acquisition, &missing, 3) {
        Err(error) => error,
        Ok(_) => panic!("missing input must fail"),
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    let path = directory.write("empty", b"");
    assert!(
        super::acquire_read(&mut acquisition, &path, 3)
            .unwrap_or_else(|error| panic!("read after missing optional file: {error}"))
            .is_empty()
    );
    let report = acquisition.json();
    assert_eq!(report["exhausted"], false);
    assert_eq!(report["observed_intake"]["attempted_reads"], 2);
    assert_eq!(report["observed_intake"]["read_calls"], 1);
    assert_eq!(report["observed_intake"]["bytes"], 0);
}

#[test]
fn retained_file_comparison_stops_at_the_payload_and_one_sentinel() {
    struct PrefixReader<'a> {
        expected: &'a [u8],
        position: usize,
        calls: usize,
        bytes: usize,
    }
    impl std::io::Read for PrefixReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            assert!(!output.is_empty());
            output[0] = self.expected.get(self.position).copied().unwrap_or(0xff);
            self.position += 1;
            self.calls += 1;
            self.bytes += 1;
            Ok(1)
        }
    }
    let expected = vec![0xa5; 8193];
    let mut reader = PrefixReader {
        expected: &expected,
        position: 0,
        calls: 0,
        bytes: 0,
    };
    assert!(
        !super::retained_contents_match(&mut reader, &expected)
            .unwrap_or_else(|error| panic!("{error}"))
    );
    assert_eq!(reader.bytes, expected.len() + 1);
    assert_eq!(reader.calls, expected.len() + 1);
}

#[test]
fn retained_file_comparison_requires_exact_bytes_and_propagates_interrupts() {
    for (input, expected, matches) in [
        (b"".as_slice(), b"".as_slice(), true),
        (b"abc".as_slice(), b"abc".as_slice(), true),
        (b"ab".as_slice(), b"abc".as_slice(), false),
        (b"abd".as_slice(), b"abc".as_slice(), false),
        (b"abcd".as_slice(), b"abc".as_slice(), false),
        (b"a".as_slice(), b"".as_slice(), false),
    ] {
        assert_eq!(
            super::retained_contents_match(&mut std::io::Cursor::new(input), expected)
                .unwrap_or_else(|error| panic!("{error}")),
            matches,
        );
    }
    struct Interrupted;
    impl std::io::Read for Interrupted {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::Interrupted.into())
        }
    }
    assert_eq!(
        super::retained_contents_match(&mut Interrupted, b"abc")
            .err()
            .unwrap_or_else(|| panic!("expected interrupted read"))
            .kind(),
        std::io::ErrorKind::Interrupted,
    );
}

#[test]
fn retained_receipt_idempotency_and_oversized_collision_preserve_the_store() {
    let directory = Directory::new();
    let (mut store, _) = open_store(&directory.0);
    store
        .store_receipt(0, b"receipt")
        .unwrap_or_else(|error| panic!("{error}"));
    let stored_before = store.stored_bytes;
    store
        .store_receipt(0, b"receipt")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(store.stored_bytes, stored_before);
    let path = directory.0.join("receipts/0.json");
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    let oversized = 16 * 1024 * 1024;
    file.set_len(oversized)
        .unwrap_or_else(|error| panic!("{error}"));
    drop(file);
    let error = store
        .store_receipt(0, b"receipt")
        .err()
        .unwrap_or_else(|| panic!("expected collision"));
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::metadata(path)
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        oversized
    );
    assert_eq!(store.stored_bytes, stored_before);
}

#[cfg(target_os = "linux")]
#[test]
fn a_check_finalized_after_its_deadline_cannot_update_the_incumbent() {
    let directory = Directory::new();
    let (mut store, mut session) = open_store(&directory.0.join("session"));
    let ticket = session
        .reserve_attempt(0)
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    let super::Prepared::Check(job) = session.prepare(
        ticket,
        crate::neural_loop::session::Proposal::Candidate(CANDIDATE.to_vec()),
    ) else {
        panic!("expected check job");
    };
    super::CHECK_COMPLETION_DELAY.with(|delay| delay.set(Duration::from_millis(150)));
    let report = super::run_check(&job, Duration::from_millis(100), &mut store);
    let outcome = session.conclude(job, report);
    assert_eq!(outcome.name(), "check-failed:timeout");
    assert!(session.incumbent().replacement().is_none());
}

// Entry-point refusal is exercised through the production binary with injected
// missing capabilities on every host in tests/loop_containment.rs. It includes
// the former resume/candidate/run ledger and artifact no-change assertions.
