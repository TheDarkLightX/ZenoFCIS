//! Shell tests: the supervisor, the session store, the driver with each
//! proposer, crash recovery and the hosted adapter's silence.

use super::{OptimizeEngine, Store, drive, strategy_engine, supervise};
use crate::loop_proposers::{Fake, Hosted, HostedConfig, Local, Proposer, SCRIPT_SCHEMA};
use crate::neural_loop::incumbent::Incumbent;
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
use crate::neural_loop::request::{Policy, Request};
use crate::neural_loop::session::{Resumed, Session, Status, StopReason, WorkerFailure};
use crate::neural_loop::strategy::{NoEngine, SearchReport, Strategy, StrategyEngine};
use crate::optimize::strategy::{DEFAULT_STRATEGY_JSON, Phase};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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

const ORIGINAL: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-original.zcve"
);
const CANDIDATE: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-candidate.zcve"
);
const PADDED: &[u8] = include_bytes!(
    "../../../docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-padded.zcve"
);

fn request() -> Request {
    Request::admit(
        ORIGINAL,
        Profile::FunctionalBoolV1,
        Limits::CEILING,
        Policy::DISABLED,
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"))
}

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

fn reopen(dir: &Path) -> Resumed {
    let (_, loaded) = Store::open(dir).unwrap_or_else(|error| panic!("{error}"));
    Session::resume(loaded.stored(), 0)
}

fn resumed(dir: &Path) -> Session {
    match reopen(dir) {
        Resumed::Session(session) => *session,
        other => panic!("expected a session: {other:?}"),
    }
}

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
    assert!(started.elapsed() < Duration::from_millis(350));
}

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
        Resumed::Refused(
            crate::neural_loop::session::ResumeRefusal::RollbackSuspected { .. },
            None
        )
    ));
    fs::write(dir.join("ledger.jsonl"), &ledger).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(reopen(&dir), Resumed::Session(_)));
    // A missing head is unverifiable.
    fs::remove_file(dir.join("ledger.head")).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        reopen(&dir),
        Resumed::Refused(
            crate::neural_loop::session::ResumeRefusal::Unverifiable,
            None
        )
    ));
    fs::write(dir.join("ledger.head"), &head).unwrap_or_else(|error| panic!("{error}"));
    // An edited receipt file no longer matches the ledger's digest.
    let receipt = dir.join("receipts/0.json");
    let mut bytes = fs::read(&receipt).unwrap_or_default();
    bytes[12] ^= 1;
    fs::write(&receipt, &bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        reopen(&dir),
        Resumed::Refused(
            crate::neural_loop::session::ResumeRefusal::ReplacementDigest,
            None
        )
    ));
}

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
fn the_wired_engine_is_the_optimizer_and_admits_exactly_its_phases() {
    let engine = strategy_engine();
    assert_eq!(engine.identity(), "zeno-fcis/optimize/1");
    let phases = engine.phases();
    // Exhaustive on purpose: a new optimizer phase fails to compile here
    // until the loop's phase table lists it.
    let listed = |phase: Phase| match phase {
        Phase::Boolean | Phase::SemanticMerge | Phase::Select | Phase::Fold | Phase::Share => {
            phases.contains(&phase.name())
        }
    };
    for phase in [
        Phase::Boolean,
        Phase::SemanticMerge,
        Phase::Select,
        Phase::Fold,
        Phase::Share,
    ] {
        assert!(listed(phase), "{}", phase.name());
    }
    assert_eq!(phases.len(), 5);
}

#[test]
fn an_optimizer_strategy_improves_the_kernel_only_through_the_loops_own_check() {
    let document: Value = serde_json::from_str(DEFAULT_STRATEGY_JSON)
        .unwrap_or_else(|error| panic!("default strategy: {error}"));
    // The engine's own verdict is provenance; the bytes are what it emits.
    let strategy = Strategy::from_json(&document).unwrap_or_else(|refusal| panic!("{refusal:?}"));
    let report = OptimizeEngine.run(ORIGINAL, &strategy);
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
