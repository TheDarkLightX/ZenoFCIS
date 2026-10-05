//! `zeno-fcis loop`: the imperative shell around the pure
//! [`crate::neural_loop`] core. This module owns everything the core must
//! not: the session directory, durable ledger appends before each stage, the
//! clock, the supervised worker thread that runs a check under a deadline, the
//! proposers and the hosted adapter. It renders JSON and never grants
//! application authority.
//!
//! Durability order for every stage (NSR-005, NSR-007): the core appends a
//! reservation, the shell persists the ledger, and only then does the work
//! run. A crash in between leaves a reservation without a settlement; resume
//! keeps it charged as unresolved.

use crate::loop_proposers::{CONFIG_LIMIT, Fake, Hosted, HostedConfig, Local, Proposer};
use crate::neural_loop::canonical_json;
use crate::neural_loop::ledger::{Head, Ledger};
use crate::neural_loop::limits::Limits;
use crate::neural_loop::profiles::Profile;
use crate::neural_loop::program_json::{encode, program_from_json, program_to_json};
use crate::neural_loop::request::{Policy, Request};
use crate::neural_loop::session::{
    CheckReport, Outcome, Prepared, Proposal, ProposerFailure, Resumed, Session, Status,
    StopReason, Stored, WorkerFailure,
};
use crate::neural_loop::strategy::{SearchReport, Strategy, StrategyEngine};
use crate::optimize;
use crate::transform::sha256_hex;
use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use zeno_fcis_synthesis::finite_runtime::import_program;

const RESULT_SCHEMA: &str = "zeno-fcis/transform-loop/1";
/// NSR-002: a checker worker's external deadline.
const CHECK_DEADLINE: Duration = Duration::from_secs(2);
/// NSR-002: a search worker's external deadline.
const SEARCH_DEADLINE: Duration = Duration::from_secs(5);
/// Largest input domain the optimizer searches for the loop. The loop checks
/// every emitted candidate again under its own work caps, so a larger domain
/// could not be checked anyway, and the bound keeps an abandoned search short.
const ENGINE_MAX_INPUT_TUPLES: u64 = 1_000_000;
/// Provenance name of the wired engine (never assurance).
const ENGINE_IDENTITY: &str = "zeno-fcis/optimize/1";
/// The phase names of F4's grammar, which the driver checks a strategy against.
const ENGINE_PHASES: [&str; 6] = [
    "boolean",
    "semantic-merge",
    "select",
    "fold",
    "share",
    "cut-rewrite",
];
const PROGRAM_LIMIT: u64 = 64 * 1024;
const PROGRAM_JSON_LIMIT: u64 = 1024 * 1024;
const LEDGER_LIMIT: u64 = 4 * 1024 * 1024;
const HEAD_LIMIT: u64 = 4 * 1024;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum ProfileArg {
    #[value(name = "functional-bool-v1")]
    FunctionalBoolV1,
    #[value(name = "checked-i64-v1")]
    CheckedI64V1,
}

impl ProfileArg {
    fn profile(self) -> Profile {
        match self {
            ProfileArg::FunctionalBoolV1 => Profile::FunctionalBoolV1,
            ProfileArg::CheckedI64V1 => Profile::CheckedI64V1,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum ProposerArg {
    /// Deterministic local rewriting; no model call.
    Local,
    /// A scripted fake provider (`--script`), metered as a model.
    Fake,
    /// The hosted-model adapter; disabled in this build.
    Hosted,
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Admit an original program and open a new session directory.
    Open {
        /// Canonical original program (program.zcve encoding).
        #[arg(long)]
        original: PathBuf,
        /// New or empty directory holding the request, ledger and artifacts.
        #[arg(long)]
        session: PathBuf,
        #[arg(long, value_enum, default_value_t = ProfileArg::FunctionalBoolV1)]
        profile: ProfileArg,
        /// Attempts per session, at most 8.
        #[arg(long, default_value_t = Limits::CEILING.attempts)]
        attempts: u8,
        /// Candidate checks per session, at most 8.
        #[arg(long, default_value_t = Limits::CEILING.checks)]
        checks: u8,
        /// Model calls per session, at most 4.
        #[arg(long, default_value_t = Limits::CEILING.model_calls)]
        model_calls: u8,
        /// Deterministic checker work units per check, at most 1,000,000.
        #[arg(long, default_value_t = Limits::CEILING.check_work)]
        check_work: u64,
        /// Checker work units per session including replays, at most 8,000,000.
        #[arg(long, default_value_t = Limits::CEILING.session_work)]
        session_work: u64,
        /// Deadline for one run or candidate invocation, at most 20,000 ms.
        #[arg(long, default_value_t = Limits::CEILING.deadline_ms)]
        deadline_ms: u64,
    },
    /// Spend one attempt on a supplied candidate and report typed feedback.
    Candidate {
        #[arg(long)]
        session: PathBuf,
        /// Canonical candidate program.
        #[arg(
            long,
            conflicts_with = "candidate_json",
            required_unless_present = "candidate_json"
        )]
        candidate: Option<PathBuf>,
        /// Candidate in the fixture vocabulary (inputs, outputs, nodes, roots); encoded canonically before admission.
        #[arg(long)]
        candidate_json: Option<PathBuf>,
        /// Optional JSON recorded as the proposer's untrusted provenance.
        #[arg(long)]
        provenance: Option<PathBuf>,
    },
    /// Drive the loop with a proposer until it stops, then report.
    Run {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, value_enum)]
        proposer: ProposerArg,
        /// Fake provider script (zeno-fcis/fake-provider-script/1).
        #[arg(long, required_if_eq("proposer", "fake"))]
        script: Option<PathBuf>,
        /// Hosted adapter configuration (zeno-fcis/hosted-provider-config/1); disabled when absent.
        #[arg(long)]
        hosted_config: Option<PathBuf>,
    },
    /// Re-admit the request, verify the ledger, replay the incumbent and report.
    Resume {
        #[arg(long)]
        session: PathBuf,
    },
    /// Encode a program given in the fixture vocabulary as canonical bytes.
    Encode {
        /// JSON with inputs, outputs, nodes and roots.
        #[arg(long)]
        program: PathBuf,
        /// New file for the canonical bytes.
        #[arg(long)]
        out: PathBuf,
    },
}

pub(super) fn run(command: Command) -> u8 {
    let (exit, report) = match command {
        Command::Open {
            original,
            session,
            profile,
            attempts,
            checks,
            model_calls,
            check_work,
            session_work,
            deadline_ms,
        } => {
            let limits = Limits {
                attempts,
                checks,
                model_calls,
                check_work,
                session_work,
                deadline_ms,
                ..Limits::CEILING
            };
            open(&original, &session, profile.profile(), limits)
        }
        Command::Candidate {
            session,
            candidate,
            candidate_json,
            provenance,
        } => candidate_command(
            &session,
            candidate.as_deref(),
            candidate_json.as_deref(),
            provenance.as_deref(),
        ),
        Command::Run {
            session,
            proposer,
            script,
            hosted_config,
        } => run_command(
            &session,
            proposer,
            script.as_deref(),
            hosted_config.as_deref(),
        ),
        Command::Resume { session } => resume_command(&session),
        Command::Encode { program, out } => encode_command(&program, &out),
    };
    crate::print_json(&report);
    exit
}

/// The engine strategies run through: F4's e-graph optimizer.
fn strategy_engine() -> Arc<dyn StrategyEngine + Send + Sync> {
    Arc::new(OptimizeEngine)
}

/// F4's e-graph optimizer as the loop's strategy engine. The optimizer judges
/// its own candidates with the transform checker, but that verdict is only
/// provenance here: the bytes it emits go through the loop's admission and
/// its own check job before they can replace the incumbent (NSM-004/005).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OptimizeEngine;

impl StrategyEngine for OptimizeEngine {
    fn identity(&self) -> &str {
        ENGINE_IDENTITY
    }

    fn phases(&self) -> &[&str] {
        &ENGINE_PHASES
    }

    fn run(
        &self,
        original: &[u8],
        strategy: &Strategy,
        profile: Profile,
        checked: &[Vec<u8>],
    ) -> SearchReport {
        // The optimizer re-parses the document with its own strict grammar.
        let document = strategy.json().to_string();
        let mut strategy = match optimize::strategy::Strategy::parse(document.as_bytes()) {
            Ok(strategy) => strategy,
            Err(error) => {
                return SearchReport::Failed {
                    reason: format!("engine-strategy:{}", error.message),
                };
            }
        };
        // The search stays within the request's profile, so its candidate
        // can pass the loop's profile admission.
        if let Some(named) = strategy.profile
            && named != profile
        {
            return SearchReport::Unavailable {
                reason: format!(
                    "engine:profile-mismatch:{}:{}",
                    named.name(),
                    profile.name()
                ),
            };
        }
        strategy.profile = Some(profile);
        // The loop's checked candidates are fused into the search; the
        // optimizer judges them again before using them.
        let plan = optimize::strategy::Plan::single(strategy);
        match optimize::optimize_plan(original, &plan, checked, ENGINE_MAX_INPUT_TUPLES) {
            optimize::Outcome::Searched(search) => match search.best {
                Some(optimize::Accepted {
                    bytes,
                    source: optimize::Source::Phase { phase_index, .. },
                    ..
                }) => SearchReport::Candidate {
                    bytes,
                    extraction: format!("engine-accepted:phase-{phase_index}"),
                },
                // Nothing better than what the loop already holds.
                _ => SearchReport::Failed {
                    reason: String::from("engine:no-checked-improvement"),
                },
            },
            optimize::Outcome::Refused(refused) => SearchReport::Failed {
                reason: format!("engine-refused:{refused:?}"),
            },
            optimize::Outcome::Inconclusive(_) => SearchReport::Unavailable {
                reason: String::from("engine:domain-above-search-cap"),
            },
        }
    }
}

fn result(exit: u8, status: &str, detail: Value) -> (u8, Value) {
    (
        exit,
        json!({"schema": RESULT_SCHEMA, "status": status, "authority": "none", "detail": detail}),
    )
}

fn io_failure(error: &std::io::Error) -> (u8, Value) {
    result(
        crate::FAILURE,
        "io-error",
        json!({"message": error.to_string()}),
    )
}

fn open(original: &Path, session: &Path, profile: Profile, limits: Limits) -> (u8, Value) {
    let original = match crate::transform_command::read_bounded(original, PROGRAM_LIMIT) {
        Ok(bytes) if crate::transform_command::over(&bytes, PROGRAM_LIMIT) => {
            return result(
                crate::INVALID,
                "refused",
                json!({"reason": "program-too-large", "max_bytes": PROGRAM_LIMIT}),
            );
        }
        Ok(bytes) => bytes,
        Err(error) => return io_failure(&error),
    };
    // Hosted mode, disclosure and spending are fixed disabled in this build.
    let request = match Request::admit(&original, profile, limits, Policy::DISABLED) {
        Ok(request) => request,
        Err(refusal) => return result(crate::INVALID, "refused", refusal.json()),
    };
    let session_state = Session::open(request);
    let mut store = match Store::create(session, session_state.request()) {
        Ok(store) => store,
        Err(error) => return io_failure(&error),
    };
    if let Err(error) = store.persist(session_state.ledger()) {
        return io_failure(&error);
    }
    result(
        crate::OK,
        "opened",
        json!({
            "request_id": session_state.request().id(),
            "session": session.display().to_string(),
            "request": session_state.request().json(),
            "view": session_state.request().view(false),
        }),
    )
}

fn encode_command(program: &Path, out: &Path) -> (u8, Value) {
    if out.symlink_metadata().is_ok() {
        return result(
            crate::INVALID,
            "output-exists",
            json!({"path": out.display().to_string()}),
        );
    }
    let value: Value = match crate::transform_command::read_bounded(program, PROGRAM_JSON_LIMIT) {
        Ok(bytes) if crate::transform_command::over(&bytes, PROGRAM_JSON_LIMIT) => {
            return result(
                crate::INVALID,
                "refused",
                json!({"reason": "program-json-too-large", "max_bytes": PROGRAM_JSON_LIMIT}),
            );
        }
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                return result(
                    crate::INVALID,
                    "refused",
                    json!({"reason": "program-json-unreadable", "message": error.to_string()}),
                );
            }
        },
        Err(error) => return io_failure(&error),
    };
    let encoded = program_from_json(&value).and_then(|program| {
        let bytes = encode(&program)?;
        Ok((program, bytes))
    });
    let (program, bytes) = match encoded {
        Ok(pair) => pair,
        Err(error) => return result(crate::INVALID, "refused", error.json()),
    };
    if let Err(error) = crate::atomic_create(out, &bytes) {
        return io_failure(&error);
    }
    result(
        crate::OK,
        "encoded",
        json!({
            "path": out.display().to_string(),
            "sha256": sha256_hex(&bytes),
            "bytes": bytes.len(),
            "nodes": program.nodes().len(),
        }),
    )
}

/// Loads a session and resumes it; a refusal is rendered for the caller.
fn load_session(dir: &Path, started: Instant) -> Result<(Store, Session), (u8, Value)> {
    let (mut store, loaded) = Store::open(dir).map_err(|error| io_failure(&error))?;
    match Session::resume(loaded.stored(), elapsed_ms(started)) {
        Resumed::Session(session) => Ok((store, *session)),
        Resumed::Refused(reason, ledger) => {
            if let Some(ledger) = ledger {
                store.persist(&ledger).map_err(|error| io_failure(&error))?;
            }
            Err(result(
                crate::BLOCKED,
                "resume-refused",
                json!({"refusal": reason.json(), "trusted_incumbent": Value::Null}),
            ))
        }
        Resumed::Inconclusive(reason, ledger) => {
            if let Some(ledger) = ledger {
                store.persist(&ledger).map_err(|error| io_failure(&error))?;
            }
            Err(result(
                crate::BLOCKED,
                "resume-inconclusive",
                json!({"refusal": reason.json(), "trusted_incumbent": Value::Null}),
            ))
        }
    }
}

fn resume_command(dir: &Path) -> (u8, Value) {
    let started = Instant::now();
    let (mut store, session) = match load_session(dir, started) {
        Ok(loaded) => loaded,
        Err(failure) => return failure,
    };
    if let Err(error) = store.persist(session.ledger()) {
        return io_failure(&error);
    }
    result(crate::OK, "resumed", json!({"report": session.report()}))
}

fn candidate_command(
    dir: &Path,
    candidate: Option<&Path>,
    candidate_json: Option<&Path>,
    provenance: Option<&Path>,
) -> (u8, Value) {
    let started = Instant::now();
    let bytes = match (candidate, candidate_json) {
        (Some(path), _) => match crate::transform_command::read_bounded(path, PROGRAM_LIMIT) {
            Ok(bytes) => bytes,
            Err(error) => return io_failure(&error),
        },
        (None, Some(path)) => {
            match crate::transform_command::read_bounded(path, PROGRAM_JSON_LIMIT) {
                Ok(bytes) if crate::transform_command::over(&bytes, PROGRAM_JSON_LIMIT) => {
                    return result(
                        crate::INVALID,
                        "refused",
                        json!({"reason": "program-json-too-large", "max_bytes": PROGRAM_JSON_LIMIT}),
                    );
                }
                Ok(bytes) => {
                    let encoded = serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|value| program_from_json(&value).ok())
                        .and_then(|program| encode(&program).ok());
                    // An undecodable JSON candidate is a malformed proposal: it
                    // still consumes the attempt below.
                    encoded.unwrap_or_else(|| b"malformed program json".to_vec())
                }
                Err(error) => return io_failure(&error),
            }
        }
        (None, None) => {
            return (
                crate::USAGE,
                json!({"schema": RESULT_SCHEMA, "status": "usage"}),
            );
        }
    };
    let asserted = match provenance {
        Some(path) => match crate::transform_command::read_bounded(path, CONFIG_LIMIT) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            Err(error) => return io_failure(&error),
        },
        None => Value::Null,
    };
    let (mut store, mut session) = match load_session(dir, started) {
        Ok(loaded) => loaded,
        Err(failure) => return failure,
    };
    let mut proposer = Supplied {
        bytes: Some(bytes),
        asserted,
    };
    let engine = strategy_engine();
    let outcomes = match drive(
        &mut session,
        &mut store,
        &mut proposer,
        &engine,
        started,
        Some(1),
    ) {
        Ok(outcomes) => outcomes,
        Err(error) => return io_failure(&error),
    };
    let request = session.request();
    let inputs = request.program().inputs();
    let outputs = request.program().outputs();
    let (Some(record), Some(outcome)) = (
        session
            .attempts()
            .last()
            .filter(|_| proposer.bytes.is_none()),
        outcomes.last(),
    ) else {
        let stop = match session.status() {
            Status::Closed(reason) => reason.name(),
            Status::Open => "open".to_owned(),
        };
        return result(
            crate::BLOCKED,
            "session-closed",
            json!({"stop_reason": stop, "report": session.report()}),
        );
    };
    let exit = match outcome {
        Outcome::Equivalent { .. } => crate::OK,
        Outcome::Different(_) | Outcome::Refused(_) | Outcome::Duplicate(_) => crate::INVALID,
        _ => crate::BLOCKED,
    };
    // The incumbent's program, so an agent can continue from the best checked
    // result rather than from the original.
    let incumbent_program = match session.incumbent().replacement() {
        Some(replacement) => import_program(replacement.bytes())
            .map(|program| program_to_json(&program))
            .unwrap_or(Value::Null),
        None => program_to_json(request.program()),
    };
    result(
        exit,
        &record.outcome,
        json!({
            "attempt": record.attempt,
            "outcome": outcome.json(request),
            "feedback": record.feedback.json(inputs, outputs),
            "all_feedback": session.feedback().iter().map(|feedback| feedback.json(inputs, outputs)).collect::<Vec<_>>(),
            "incumbent": session.incumbent().json(request.original_sha256(), request.cost()),
            "incumbent_program": incumbent_program,
            "status": session.incumbent().status(),
            "accounting": session.ledger().accounting().json(),
            "session": match session.status() {
                Status::Open => json!({"state": "open"}),
                Status::Closed(reason) => json!({"state": "closed", "stop_reason": reason.name()}),
            },
        }),
    )
}

fn run_command(
    dir: &Path,
    proposer: ProposerArg,
    script: Option<&Path>,
    hosted_config: Option<&Path>,
) -> (u8, Value) {
    let started = Instant::now();
    let (mut store, mut session) = match load_session(dir, started) {
        Ok(loaded) => loaded,
        Err(failure) => return failure,
    };
    let mut proposer: Box<dyn Proposer> = match proposer {
        ProposerArg::Local => Box::new(Local::new(
            session.request().program(),
            session.request().original(),
        )),
        ProposerArg::Fake => {
            let Some(script) = script else {
                return (
                    crate::USAGE,
                    json!({"schema": RESULT_SCHEMA, "status": "usage"}),
                );
            };
            let value: Value = match crate::transform_command::read_bounded(script, CONFIG_LIMIT) {
                Ok(bytes) => match serde_json::from_slice(&bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        return result(
                            crate::INVALID,
                            "refused",
                            json!({"reason": "script-unreadable", "message": error.to_string()}),
                        );
                    }
                },
                Err(error) => return io_failure(&error),
            };
            let base = script.parent().unwrap_or_else(|| Path::new("."));
            match Fake::from_script(&value, base) {
                Ok(fake) => Box::new(fake),
                Err(error) => {
                    return result(
                        crate::INVALID,
                        "refused",
                        json!({"reason": "script-invalid", "error": format!("{error:?}")}),
                    );
                }
            }
        }
        ProposerArg::Hosted => {
            let config = match hosted_config {
                None => HostedConfig::DISABLED,
                Some(path) => match crate::transform_command::read_bounded(path, CONFIG_LIMIT) {
                    Ok(bytes) => match serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|value| HostedConfig::from_json(&value))
                    {
                        Some(config) => config,
                        None => {
                            return result(
                                crate::INVALID,
                                "refused",
                                json!({"reason": "hosted-config-invalid"}),
                            );
                        }
                    },
                    Err(error) => return io_failure(&error),
                },
            };
            Box::new(Hosted::new(config))
        }
    };
    let engine = strategy_engine();
    if let Err(error) = drive(
        &mut session,
        &mut store,
        proposer.as_mut(),
        &engine,
        started,
        None,
    ) {
        return io_failure(&error);
    }
    result(
        crate::OK,
        "completed",
        json!({"proposer": proposer.identity(), "report": session.report()}),
    )
}

/// The agent-supplied candidate of `loop candidate`: one proposal, no model
/// call metered (the agent's own tokens are outside this loop's accounting).
struct Supplied {
    bytes: Option<Vec<u8>>,
    asserted: Value,
}

impl Proposer for Supplied {
    fn identity(&self) -> &str {
        "zeno-fcis/supplied-candidate/1"
    }

    fn uses_model_calls(&self) -> bool {
        false
    }

    fn is_hosted(&self) -> bool {
        false
    }

    fn exhausted(&self) -> bool {
        self.bytes.is_none()
    }

    fn propose(&mut self, _view: &Value, _feedback: &[Value]) -> crate::loop_proposers::Proposed {
        let proposal = match self.bytes.take() {
            Some(bytes) => Proposal::Candidate(bytes),
            None => Proposal::Failed(ProposerFailure::Exhausted),
        };
        crate::loop_proposers::Proposed {
            proposal,
            provenance: json!({
                "provider": self.identity(),
                "provider_asserted": self.asserted,
            }),
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Runs a closure on a worker thread under a deadline. A panic is reported,
/// not propagated; a result after the deadline is dropped, because the stage
/// is closed by then (NSL-007). Memory caps are not installed by this shell:
/// the checker's allocation is bounded structurally by the admitted artifact
/// limits, and no search worker exists in this build (NSR-002, documented).
pub(crate) fn supervise<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, WorkerFailure> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("zeno-fcis-loop-worker".to_owned())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(work)).map_err(|_| ());
            let _ = sender.send(outcome);
        });
    if spawned.is_err() {
        return Err(WorkerFailure::Unavailable(
            "worker-thread-unavailable".to_owned(),
        ));
    }
    match receiver.recv_timeout(deadline) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(())) => Err(WorkerFailure::Panicked),
        Err(RecvTimeoutError::Timeout) => Err(WorkerFailure::Timeout),
        Err(RecvTimeoutError::Disconnected) => Err(WorkerFailure::Died),
    }
}

/// Drives attempts until the session stops or `max_attempts` were spent.
/// Every core transition is persisted before the work it authorizes.
pub(crate) fn drive(
    session: &mut Session,
    store: &mut Store,
    proposer: &mut dyn Proposer,
    engine: &Arc<dyn StrategyEngine + Send + Sync>,
    started: Instant,
    max_attempts: Option<u8>,
) -> std::io::Result<Vec<Outcome>> {
    let mut spent = 0_u8;
    let mut outcomes = Vec::new();
    loop {
        if max_attempts.is_some_and(|max| spent >= max) {
            store.persist(session.ledger())?;
            return Ok(outcomes);
        }
        if proposer.exhausted() {
            session.close(StopReason::ProposerExhausted);
            store.persist(session.ledger())?;
            return Ok(outcomes);
        }
        let ticket = match session.reserve_attempt(elapsed_ms(started)) {
            Ok(ticket) => ticket,
            Err(_) => {
                store.persist(session.ledger())?;
                return Ok(outcomes);
            }
        };
        spent = spent.saturating_add(1);
        store.persist(session.ledger())?;
        if proposer.uses_model_calls() {
            match session.reserve_model_call(&ticket, elapsed_ms(started)) {
                Ok(call) => store.transcript(&json!({
                    "attempt": call.attempt(),
                    "model_call_reserved": session.request().limits().call_tokens(),
                })),
                Err(_) => {
                    store.persist(session.ledger())?;
                    return Ok(outcomes);
                }
            }
            store.persist(session.ledger())?;
        }
        let request = session.request();
        let view = request.view(proposer.is_hosted());
        let inputs = request.program().inputs().to_vec();
        let outputs = request.program().outputs().to_vec();
        let feedback: Vec<Value> = session
            .feedback()
            .iter()
            .map(|feedback| feedback.json(&inputs, &outputs))
            .collect();
        let proposed = proposer.propose(&view, &feedback);
        store.transcript(&json!({
            "attempt": ticket.attempt(),
            "elapsed_ms": elapsed_ms(started),
            "provenance": proposed.provenance,
        }));
        let mut prepared = session.prepare(ticket, proposed.proposal);
        if let Prepared::Search(job) = prepared {
            store.persist(session.ledger())?;
            let strategy = job.strategy().clone();
            // NSL-003: every phase must name one of the engine's fixed phases.
            let unknown = strategy
                .phase_names()
                .find(|name| !engine.phases().contains(name));
            store.transcript(&json!({
                "engine": engine.identity(),
                "strategy": strategy.json(),
                "unknown_phase": unknown,
            }));
            let report = match unknown {
                Some(name) if !engine.phases().is_empty() => SearchReport::Unavailable {
                    reason: format!("unknown-phase:{name}"),
                },
                _ => {
                    let worker = Arc::clone(engine);
                    let original = session.request().original().to_vec();
                    let profile = session.request().profile();
                    let checked: Vec<Vec<u8>> = session
                        .incumbent()
                        .replacement()
                        .map(|replacement| replacement.bytes().to_vec())
                        .into_iter()
                        .collect();
                    supervise(SEARCH_DEADLINE, move || {
                        worker.run(&original, &strategy, profile, &checked)
                    })
                    .unwrap_or_else(|failure| SearchReport::Failed {
                        reason: format!("search-worker:{}", failure.name()),
                    })
                }
            };
            prepared = session.searched(job, report);
        }
        let outcome = match prepared {
            Prepared::Check(job) => {
                // Reservation and artifact custody before the check runs.
                store.persist(session.ledger())?;
                store.transcript(&json!({
                    "check": {
                        "attempt": job.attempt(),
                        "candidate_sha256": job.candidate_sha256(),
                        "cost": job.cost().json(),
                        "work_reserved": job.work(),
                    }
                }));
                let report = match store.store_artifact(job.candidate()) {
                    Ok(()) => {
                        let worker = job.clone();
                        supervise(CHECK_DEADLINE, move || worker.run())
                            .unwrap_or_else(CheckReport::Failed)
                    }
                    Err(StorageError::Exhausted) => CheckReport::Failed(
                        WorkerFailure::Unavailable("storage-exhausted".to_owned()),
                    ),
                    Err(StorageError::Io(error)) => return Err(error),
                };
                let attempt = job.attempt();
                let outcome = session.conclude(job, report);
                if let Outcome::Different(witness) = &outcome {
                    store.store_witness(attempt, &witness.json(&inputs, &outputs))?;
                }
                if let Some(replacement) = session.incumbent().replacement()
                    && replacement.attempt() == attempt
                {
                    store.store_receipt(attempt, replacement.receipt())?;
                }
                store.persist(session.ledger())?;
                outcome
            }
            Prepared::Settled(outcome) => {
                store.persist(session.ledger())?;
                outcome
            }
            Prepared::Search(_) => Outcome::NotPending,
        };
        if !outcome.settled() {
            store.transcript(&json!({"ignored": outcome.name()}));
        }
        store.transcript(&json!({"settled": outcome.json(session.request())}));
        outcomes.push(outcome);
        if let Status::Closed(_) = session.status() {
            store.persist(session.ledger())?;
            return Ok(outcomes);
        }
    }
}

/// Why an artifact could not be retained.
pub(crate) enum StorageError {
    /// NSR-004: the retained storage cap is reached; the attempt becomes
    /// inconclusive rather than truncating or omitting a required binding.
    Exhausted,
    Io(std::io::Error),
}

/// Everything `Store::open` read for `Session::resume`.
pub(crate) struct Loaded {
    request: Vec<u8>,
    original: Vec<u8>,
    ledger: Vec<u8>,
    head: Option<Head>,
    replacement: Option<(Vec<u8>, Vec<u8>)>,
    witnesses: Vec<(u8, Value, Vec<u8>)>,
}

impl Loaded {
    pub(crate) fn stored(&self) -> Stored<'_> {
        Stored {
            request: &self.request,
            original: &self.original,
            ledger: &self.ledger,
            head: self.head.clone(),
            replacement: self
                .replacement
                .as_ref()
                .map(|(candidate, receipt)| (candidate.as_slice(), receipt.as_slice())),
            witnesses: self.witnesses.clone(),
        }
    }
}

/// The session directory.
pub(crate) struct Store {
    dir: PathBuf,
    persisted: u64,
    stored_bytes: u64,
    storage_limit: u64,
    transcript_bytes: u64,
    transcript_limit: u64,
    transcript_truncated: bool,
}

impl Store {
    /// Creates the directory layout and retains the request and original.
    pub(crate) fn create(dir: &Path, request: &Request) -> std::io::Result<Store> {
        if dir.exists() {
            if fs::read_dir(dir)?.next().is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("session directory is not empty: {}", dir.display()),
                ));
            }
        } else {
            fs::create_dir(dir)?;
        }
        for sub in ["artifacts", "receipts", "witnesses"] {
            fs::create_dir(dir.join(sub))?;
        }
        crate::atomic_create(&dir.join("request.json"), request.canonical_bytes())?;
        crate::atomic_create(&dir.join("original.zcve"), request.original())?;
        Ok(Store {
            dir: dir.to_path_buf(),
            persisted: 0,
            stored_bytes: (request.canonical_bytes().len() + request.original().len()) as u64,
            storage_limit: u64::from(request.limits().storage_bytes),
            transcript_bytes: 0,
            transcript_limit: u64::from(request.limits().transcript_bytes),
            transcript_truncated: false,
        })
    }

    /// Reads a session directory. Faults in the ledger are left for
    /// `Session::resume` to classify; the replacement artifacts are loaded
    /// only when the ledger names them.
    pub(crate) fn open(dir: &Path) -> std::io::Result<(Store, Loaded)> {
        let read =
            |name: &str, limit: u64| crate::transform_command::read_bounded(&dir.join(name), limit);
        let request = read("request.json", PROGRAM_LIMIT)?;
        let original = read("original.zcve", PROGRAM_LIMIT)?;
        let ledger = read("ledger.jsonl", LEDGER_LIMIT)?;
        let head = match read("ledger.head", HEAD_LIMIT) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| Head::from_json(&value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let limits: Option<Limits> = serde_json::from_slice::<Value>(&request)
            .ok()
            .and_then(|value| value.get("limits").and_then(Limits::from_json));
        let limits = limits.unwrap_or(Limits::CEILING);
        let parsed = Ledger::parse_lines(&ledger)
            .ok()
            .and_then(|entries| Ledger::from_entries(entries).ok());
        let mut replacement = None;
        if let Some((attempt, candidate_sha256, _, _)) = parsed
            .as_ref()
            .and_then(|ledger| ledger.accounting().replaced)
        {
            let candidate = read(&format!("artifacts/{candidate_sha256}.zcve"), PROGRAM_LIMIT);
            let receipt = read(&format!("receipts/{attempt}.json"), PROGRAM_LIMIT);
            if let (Ok(candidate), Ok(receipt)) = (candidate, receipt) {
                replacement = Some((candidate, receipt));
            }
        }
        let mut witnesses = Vec::new();
        let mut names: Vec<PathBuf> = fs::read_dir(dir.join("witnesses"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        names.sort();
        for path in names {
            let attempt = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u8>().ok());
            let Some(attempt) = attempt else { continue };
            let Ok(bytes) = crate::transform_command::read_bounded(&path, PROGRAM_LIMIT) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let digest = value
                .get("candidate")
                .and_then(|side| side.get("sha256"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let candidate =
                read(&format!("artifacts/{digest}.zcve"), PROGRAM_LIMIT).unwrap_or_default();
            witnesses.push((attempt, value, candidate));
        }
        let mut stored_bytes = (request.len() + original.len()) as u64;
        for sub in ["artifacts", "receipts", "witnesses"] {
            for entry in fs::read_dir(dir.join(sub))?.filter_map(Result::ok) {
                stored_bytes =
                    stored_bytes.saturating_add(entry.metadata().map_or(0, |meta| meta.len()));
            }
        }
        let transcript_bytes =
            fs::metadata(dir.join("transcript.jsonl")).map_or(0, |meta| meta.len());
        let store = Store {
            dir: dir.to_path_buf(),
            persisted: parsed
                .as_ref()
                .map_or(0, |ledger| ledger.entries().len() as u64),
            stored_bytes,
            storage_limit: u64::from(limits.storage_bytes),
            transcript_bytes,
            transcript_limit: u64::from(limits.transcript_bytes),
            transcript_truncated: false,
        };
        Ok((
            store,
            Loaded {
                request,
                original,
                ledger,
                head,
                replacement,
                witnesses,
            },
        ))
    }

    /// Appends every entry not yet persisted, syncs, then rewrites the head.
    /// A crash between the two leaves a ledger ahead of its head, which
    /// resume refuses as rollback-suspected: fail closed, never silently.
    pub(crate) fn persist(&mut self, ledger: &Ledger) -> std::io::Result<()> {
        let pending: Vec<&crate::neural_loop::ledger::Entry> = ledger
            .entries()
            .iter()
            .filter(|entry| entry.sequence >= self.persisted)
            .collect();
        if pending.is_empty() {
            return Ok(());
        }
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.dir.join("ledger.jsonl"))?;
        for entry in &pending {
            file.write_all(&entry.line())?;
        }
        file.sync_all()?;
        drop(file);
        crate::atomic_replace(
            &self.dir.join("ledger.head"),
            &canonical_json(&ledger.head().json()),
        )?;
        self.persisted = ledger.entries().len() as u64;
        Ok(())
    }

    fn retain(&mut self, path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
        if path.symlink_metadata().is_ok() {
            // Content-addressed or attempt-numbered files are immutable: an
            // existing file must hold these exact bytes.
            let existing = fs::read(path).map_err(StorageError::Io)?;
            if existing == bytes {
                return Ok(());
            }
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("retained file differs: {}", path.display()),
            )));
        }
        let size = bytes.len() as u64;
        if self.stored_bytes.saturating_add(size) > self.storage_limit {
            return Err(StorageError::Exhausted);
        }
        crate::atomic_create(path, bytes).map_err(StorageError::Io)?;
        self.stored_bytes = self.stored_bytes.saturating_add(size);
        Ok(())
    }

    /// Retains candidate bytes under their digest before they are checked.
    pub(crate) fn store_artifact(&mut self, bytes: &[u8]) -> Result<(), StorageError> {
        let path = self
            .dir
            .join("artifacts")
            .join(format!("{}.zcve", sha256_hex(bytes)));
        self.retain(&path, bytes)
    }

    pub(crate) fn store_receipt(&mut self, attempt: u8, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.dir.join("receipts").join(format!("{attempt}.json"));
        self.retain(&path, bytes).map_err(storage_io)
    }

    pub(crate) fn store_witness(&mut self, attempt: u8, witness: &Value) -> std::io::Result<()> {
        let path = self.dir.join("witnesses").join(format!("{attempt}.json"));
        self.retain(&path, &canonical_json(witness))
            .map_err(storage_io)
    }

    /// Appends advisory provenance within the transcript cap (NSR-004).
    /// Overflow writes one truncation marker and drops later lines.
    pub(crate) fn transcript(&mut self, value: &Value) {
        let line = canonical_json(value);
        let path = self.dir.join("transcript.jsonl");
        let append = |bytes: &[u8]| -> std::io::Result<()> {
            let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
            file.write_all(bytes)
        };
        if self.transcript_bytes.saturating_add(line.len() as u64) <= self.transcript_limit {
            if append(&line).is_ok() {
                self.transcript_bytes = self.transcript_bytes.saturating_add(line.len() as u64);
            }
        } else if !self.transcript_truncated {
            self.transcript_truncated = true;
            let marker =
                canonical_json(&json!({"truncated": true, "advisory_history_omitted": true}));
            let _ = append(&marker);
        }
    }
}

fn storage_io(error: StorageError) -> std::io::Error {
    match error {
        StorageError::Exhausted => std::io::Error::other("retained storage cap reached"),
        StorageError::Io(error) => error,
    }
}

#[cfg(test)]
#[path = "loop_command_tests.rs"]
mod tests;
