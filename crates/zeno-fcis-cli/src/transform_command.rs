//! `zeno-fcis transform`: the command over the pure `transform` checker. This
//! module only reads and writes files and renders JSON; no result grants
//! application authority.

use crate::symbolic::equivalence::{self, Outcome};
use crate::symbolic::verdict::Judgment;
use crate::symbolic_command::Solvers;
use crate::transform::{self, Inconclusive, Limits, Observation, Refusal, Rejection, Replay, Side};
use clap::Subcommand;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};

const RESULT_SCHEMA: &str = "zeno-fcis/transform-result/1";
/// The library importer refuses canonical programs above 64 KiB. A larger file
/// is refused here before it is hashed or decoded.
const PROGRAM_LIMIT: u64 = 64 * 1024;
const RECEIPT_LIMIT: u64 = 64 * 1024;
mod certificates;

#[derive(Subcommand)]
pub(super) enum Command {
    /// Independently check complete certificates for every regenerated symbolic piece.
    CheckCertificates {
        #[arg(long)]
        original: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        tools: PathBuf,
        /// Pinned complete-root checker manifest (zeno-fcis/certificate-checker/1).
        #[arg(long)]
        checker: PathBuf,
        /// Certificate files 0.alethe, 1.alethe, ... in regenerated piece order.
        #[arg(long)]
        certificates: PathBuf,
        #[arg(long, default_value_t = transform::DEFAULT_STEP_LIMIT)]
        step_limit: u64,
        #[arg(long, default_value_t = transform::DEFAULT_MAX_INPUT_TUPLES)]
        max_input_tuples: u64,
        /// Create a new distinct checked receipt after every check succeeds.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Retain regenerated queries and solver transcripts in a new directory.
        #[arg(long)]
        queries: Option<PathBuf>,
    },
    /// Regenerate all bindings and recheck every certificate before matching a receipt.
    ReplayCertificates {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        original: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        tools: PathBuf,
        #[arg(long)]
        checker: PathBuf,
        #[arg(long)]
        certificates: PathBuf,
        #[arg(long, default_value_t = transform::DEFAULT_MAX_INPUT_TUPLES)]
        max_input_tuples: u64,
    },
    /// Compare a candidate program with the original on every declared input tuple.
    Check {
        /// Canonical original program (program.zcve encoding).
        #[arg(long)]
        original: PathBuf,
        /// Canonical candidate program with the same input and output ABI.
        #[arg(long)]
        candidate: PathBuf,
        /// Declared Step limit. The default equals the largest admitted node
        /// count, so it never binds.
        #[arg(long, default_value_t = transform::DEFAULT_STEP_LIMIT)]
        step_limit: u64,
        /// Largest input domain to enumerate; a larger domain is inconclusive.
        #[arg(long, default_value_t = transform::DEFAULT_MAX_INPUT_TUPLES)]
        max_input_tuples: u64,
        /// Create a new receipt file when the programs are equivalent.
        #[arg(long, conflicts_with = "symbolic")]
        receipt: Option<PathBuf>,
        /// For a domain above --max-input-tuples: one CVC5 query per case of the original instead of enumerating. The result is solver-attested, never an exhaustive equivalence.
        #[arg(long, requires = "tools")]
        symbolic: bool,
        /// Tools manifest (zeno-fcis/tools/2) naming the pinned CVC5 and, to corroborate it, Z3; for --symbolic.
        #[arg(long, requires = "symbolic")]
        tools: Option<PathBuf>,
        /// Create a new symbolic receipt file (zeno-fcis/transform-symbolic-receipt/1) when every piece holds. No command that needs an exhaustive receipt reads it.
        #[arg(long, requires = "symbolic")]
        symbolic_receipt: Option<PathBuf>,
        /// Write every query's SMT-LIB text and each solver's output to this new directory; for --symbolic.
        #[arg(long, requires = "symbolic")]
        queries: Option<PathBuf>,
        /// Use bounded parallel windows under the explicit offline profile.
        /// This profile is separate from the loop's 2s/64MiB checker profile.
        #[arg(long, conflicts_with = "symbolic")]
        offline_parallel: bool,
        /// Worker count (1..8) for --offline-parallel; default 4.
        #[arg(long, requires = "offline_parallel")]
        workers: Option<u64>,
        /// Tuples per range (1..4096); default 4096.
        #[arg(long, requires = "offline_parallel")]
        chunk_tuples: Option<u64>,
        /// Maximum number of ranges (1..65536); default 65536.
        #[arg(long, requires = "offline_parallel")]
        max_chunks: Option<u64>,
        /// Complete logical work reservation ceiling, distinct from Steps.
        #[arg(long, requires = "offline_parallel")]
        work_units: Option<u64>,
        /// Offline deadline in milliseconds (1..60000); default 60000.
        #[arg(long, requires = "offline_parallel")]
        deadline_ms: Option<u64>,
    },
    /// Recompute a receipt from both programs and compare it byte for byte.
    Replay {
        /// Receipt created by `transform check --receipt`.
        #[arg(long)]
        receipt: PathBuf,
        /// Canonical original program the receipt names.
        #[arg(long)]
        original: PathBuf,
        /// Canonical candidate program the receipt names.
        #[arg(long)]
        candidate: PathBuf,
        /// Largest input domain this replay may enumerate, whatever the
        /// receipt records.
        #[arg(long, default_value_t = transform::DEFAULT_MAX_INPUT_TUPLES)]
        max_input_tuples: u64,
    },
}

pub(super) fn run(command: Command) -> u8 {
    let (exit, report) = match command {
        Command::CheckCertificates {
            original,
            candidate,
            tools,
            checker,
            certificates,
            step_limit,
            max_input_tuples,
            out,
            queries,
        } => certificates::check(
            &original,
            &candidate,
            Limits {
                steps: step_limit,
                input_tuples: max_input_tuples,
            },
            &tools,
            &checker,
            &certificates,
            out.as_deref(),
            queries.as_deref(),
        ),
        Command::ReplayCertificates {
            receipt,
            original,
            candidate,
            tools,
            checker,
            certificates,
            max_input_tuples,
        } => certificates::replay(
            &receipt,
            &original,
            &candidate,
            max_input_tuples,
            &tools,
            &checker,
            &certificates,
        ),
        Command::Check {
            original,
            candidate,
            step_limit,
            max_input_tuples,
            receipt,
            symbolic,
            tools,
            symbolic_receipt,
            queries,
            offline_parallel,
            workers,
            chunk_tuples,
            max_chunks,
            work_units,
            deadline_ms,
        } => {
            let limits = Limits {
                steps: step_limit,
                input_tuples: max_input_tuples,
            };
            match tools.filter(|_| symbolic) {
                Some(tools) => symbolic_check(
                    &original,
                    &candidate,
                    limits,
                    &tools,
                    symbolic_receipt.as_deref(),
                    queries.as_deref(),
                ),
                None => {
                    let defaults = crate::transform_plan::RunLimits::offline();
                    let run = offline_parallel.then_some(crate::transform_plan::RunLimits {
                        workers: workers.unwrap_or(defaults.workers),
                        chunk_tuples: chunk_tuples.unwrap_or(defaults.chunk_tuples),
                        max_chunks: max_chunks.unwrap_or(defaults.max_chunks),
                        work_units: work_units.unwrap_or(defaults.work_units),
                        deadline_ms: deadline_ms.unwrap_or(defaults.deadline_ms),
                    });
                    check(&original, &candidate, limits, receipt.as_deref(), run)
                }
            }
        }
        Command::Replay {
            receipt,
            original,
            candidate,
            max_input_tuples,
        } => replay(&receipt, &original, &candidate, max_input_tuples),
    };
    crate::print_json(&report);
    exit
}

fn without_receipt(value: (u8, Value)) -> (u8, Value, Option<Vec<u8>>) {
    (value.0, value.1, None)
}

#[cfg(test)]
thread_local! {
    static CHECK_COMPLETION_DELAY: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

fn check(
    original: &Path,
    candidate: &Path,
    limits: Limits,
    receipt: Option<&Path>,
    run: Option<crate::transform_plan::RunLimits>,
) -> (u8, Value) {
    // The run's own deadline starts before its stage is requested, so it
    // passes before the supervisor's stage deadline and records progress.
    let started = std::time::Instant::now();
    if let Some(run) = run
        && let Err(reason) = crate::checker_process::start(
            std::time::Duration::from_millis(run.deadline_ms.clamp(1, 60_000)),
            false,
        )
    {
        return result(crate::BLOCKED, "inconclusive", json!({"reason":reason}));
    }
    let (mut exit, mut report, bytes) =
        check_value(original, candidate, limits, receipt, run, started);
    let staged = match (receipt, bytes) {
        (Some(path), Some(bytes)) => match crate::checker_process::StagedReceipt::new(path, &bytes)
        {
            Ok(staged) => Some(staged),
            Err(error) => {
                (exit, report) = retain_check_diagnostics(io_failure(&error), &report);
                None
            }
        },
        _ => None,
    };
    #[cfg(test)]
    CHECK_COMPLETION_DELAY.with(|slot| {
        let delay = slot.replace(std::time::Duration::ZERO);
        if !delay.is_zero() {
            assert_eq!(report["status"], "equivalent");
            assert!(staged.is_some());
            std::thread::sleep(delay);
        }
    });
    if run.is_some()
        && let Err(reason) = crate::checker_process::finish()
    {
        return retain_check_diagnostics(
            result(
                crate::BLOCKED,
                "inconclusive",
                json!({"reason":reason,"accounting_complete":false}),
            ),
            &report,
        );
    }
    // Staging and the finish acknowledgement are part of the checking stage.
    // Supervisor grace permits reporting lateness, never handing over a late receipt.
    if run.is_some_and(|run| {
        started.elapsed() >= std::time::Duration::from_millis(run.deadline_ms.clamp(1, 60_000))
    }) {
        return retain_check_diagnostics(
            result(crate::BLOCKED, "inconclusive", json!({"reason":"deadline"})),
            &report,
        );
    }
    publish_check(staged, (exit, report))
}

/// The final publication seam remains after actual checking and its permit.
/// Keeping it explicit lets controls force an I/O race after completed work.
/// A contained run hands the staged receipt to its supervisor, which links
/// it only after accepting how the whole run ended.
fn publish_check(
    staged: Option<crate::checker_process::StagedReceipt>,
    checked: (u8, Value),
) -> (u8, Value) {
    if let Some(staged) = staged
        && let Err(error) = staged.publish_after_supervision()
    {
        return retain_check_diagnostics(io_failure(&error), &checked.1);
    }
    checked
}

/// Retain spent-work diagnostics when terminal I/O or the publication fence
/// fails. Failure status/detail/authority remain those of the failed operation;
/// receipt, witness and verdict fields never cross this boundary.
fn retain_check_diagnostics(mut failure: (u8, Value), checked: &Value) -> (u8, Value) {
    for key in ["progress", "limits", "original_sha256", "candidate_sha256"] {
        if let Some(value) = checked.get(key) {
            failure.1[key] = value.clone();
        }
    }
    failure
}

fn check_value(
    original: &Path,
    candidate: &Path,
    limits: Limits,
    receipt: Option<&Path>,
    run: Option<crate::transform_plan::RunLimits>,
    started: std::time::Instant,
) -> (u8, Value, Option<Vec<u8>>) {
    // Unlike `exists`, `symlink_metadata` also sees a dangling link, which
    // exclusive creation would refuse after the whole check.
    if let Some(path) = receipt.filter(|path| path.symlink_metadata().is_ok()) {
        return without_receipt(result(
            crate::INVALID,
            "receipt-exists",
            json!({"path": path.display().to_string()}),
        ));
    }
    if let Some(limits) = run {
        if let Err(reason) = limits.validate() {
            return without_receipt(result(
                crate::BLOCKED,
                "inconclusive",
                json!({"reason":reason.code()}),
            ));
        }
        if limits.work_units < crate::transform_plan::ACQUISITION_WORK {
            return without_receipt(result(
                crate::BLOCKED,
                "inconclusive",
                json!({"reason":"work-exhausted","reserved_work":0}),
            ));
        }
    }
    let (original, candidate) = match read_programs(original, candidate) {
        Ok(pair) => pair,
        Err(failure) => return without_receipt(failure),
    };
    let (checked, diagnostics) = match run {
        Some(run) => {
            let outcome =
                crate::transform_parallel::check(&original, &candidate, limits, run, started);
            let p = outcome.progress;
            let diagnostics = json!({"profile":"offline-parallel-v1",
                "work_policy":crate::transform_plan::WORK_POLICY,
                "scheduling":crate::checker_process::scheduling_evidence(),
                "workers":run.workers,"chunk_tuples":run.chunk_tuples,
                "planned_chunks":p.planned_chunks,"active_workers":p.active_workers,
                "reserved_work":p.reserved,"setup_reserved":p.setup_reserved,
                "evaluated_pairs":p.evaluated,"equal_prefix":p.equal_prefix,
                "measured_steps":[p.measured_steps[0].to_string(),p.measured_steps[1].to_string()],
                "completed_chunks":p.completed_chunks,"accounting_complete":p.accounting_complete});
            (outcome.result, Some(diagnostics))
        }
        None => (
            transform::check(&original, &candidate, limits)
                .map_err(crate::transform_parallel::Failure::Semantic),
            None,
        ),
    };
    let mut receipt_bytes = None;
    let (exit, mut report) = match checked {
        Ok(equivalence) => {
            let bytes = equivalence.receipt();
            receipt_bytes = Some(bytes.clone());
            result(
                crate::OK,
                "equivalent",
                json!({
                    "receipt": equivalence.receipt_value(),
                    "receipt_sha256": transform::sha256_hex(&bytes),
                    "receipt_path": receipt.map(|path| path.display().to_string())
                }),
            )
        }
        Err(crate::transform_parallel::Failure::Semantic(rejection)) => {
            rejection_result(&rejection)
        }
        Err(crate::transform_parallel::Failure::Stopped(reason)) => result(
            crate::BLOCKED,
            "inconclusive",
            json!({"reason":reason.code()}),
        ),
    };
    if let Some(diagnostics) = diagnostics {
        report["progress"] = diagnostics;
    }
    report["limits"] = json!({"steps": limits.steps, "input_tuples": limits.input_tuples});
    report["original_sha256"] = json!(transform::sha256_hex(&original));
    report["candidate_sha256"] = json!(transform::sha256_hex(&candidate));
    (exit, report, receipt_bytes)
}

/// `transform check --symbolic`: the pure `symbolic::equivalence` check
/// with the solvers the manifest admits. Equivalence is solver-attested and
/// exits `BLOCKED`, as a CVC5 `unsat` does for `prove`; a replayed
/// counterexample exits `INVALID`.
fn symbolic_check(
    original: &Path,
    candidate: &Path,
    limits: Limits,
    tools: &Path,
    receipt: Option<&Path>,
    queries: Option<&Path>,
) -> (u8, Value) {
    if let Some(path) = receipt.filter(|path| path.symlink_metadata().is_ok()) {
        return result(
            crate::INVALID,
            "receipt-exists",
            json!({"path": path.display().to_string()}),
        );
    }
    let (original, candidate) = match read_programs(original, candidate) {
        Ok(pair) => pair,
        Err(failure) => return failure,
    };
    let solvers = match Solvers::open(Some(tools), queries) {
        Ok(solvers) if solvers.has_cvc5() => solvers,
        Ok(_) => {
            return result(
                crate::BLOCKED,
                "solvers-blocked",
                json!({"message": "the tools manifest configures no CVC5, which --symbolic needs"}),
            );
        }
        Err(message) => {
            return result(
                crate::BLOCKED,
                "solvers-blocked",
                json!({"message": message}),
            );
        }
    };
    let outcome = equivalence::check(&original, &candidate, limits, &mut |query| {
        solvers.solve(query)
    });
    if let Some(message) = solvers.retention_failure() {
        return result(crate::FAILURE, "io-error", json!({"message": message}));
    }
    let identities = solvers.identities();
    let (exit, mut report) = match outcome {
        Outcome::Refused(refusal) => result(crate::INVALID, "refused", refusal_json(&refusal)),
        Outcome::ExhaustiveFits { size, limit } => result(
            crate::INVALID,
            "refused",
            json!({
                "reason": "exhaustive-check-fits",
                "domain_size": size.to_string(),
                "max_input_tuples": limit,
                "message": "the domain fits the exhaustive check, which decides it and writes an exhaustive receipt; run transform check without --symbolic"
            }),
        ),
        Outcome::Equivalent(found) => {
            let bytes = found.receipt(&identities);
            if let Some(path) = receipt
                && let Err(error) = crate::atomic_create(path, &bytes)
            {
                return io_failure(&error);
            }
            result(
                crate::BLOCKED,
                "attested-equivalent",
                json!({
                    "evidence": "attested",
                    "receipt": found.receipt_value(&identities),
                    "receipt_sha256": transform::sha256_hex(&bytes),
                    "receipt_path": receipt.map(|path| path.display().to_string()),
                    "queries": found.compared.json(),
                }),
            )
        }
        Outcome::Counterexample(found) => {
            let piece = &found.compared.pieces[found.piece];
            let replayed = match &piece.judgment {
                Judgment::Refuted { witness, .. } => witness.json(),
                _ => Value::Null,
            };
            result(
                crate::INVALID,
                "counterexample",
                json!({
                    "evidence": "checked",
                    "piece": piece.label,
                    "replayed": replayed,
                    "queries": found.compared.json(),
                }),
            )
        }
        Outcome::Inconclusive(stop) => result(
            crate::BLOCKED,
            "inconclusive",
            json!({
                "cause": stop.cause,
                "queries": stop.compared.as_ref().map(equivalence::Compared::json),
            }),
        ),
    };
    report["mode"] = json!("symbolic");
    report["limits"] = json!({"steps": limits.steps, "input_tuples": limits.input_tuples});
    report["original_sha256"] = json!(transform::sha256_hex(&original));
    report["candidate_sha256"] = json!(transform::sha256_hex(&candidate));
    report["solvers"] = identities;
    (exit, report)
}

fn replay(receipt: &Path, original: &Path, candidate: &Path, max_input_tuples: u64) -> (u8, Value) {
    let recorded = match read_bounded(receipt, RECEIPT_LIMIT) {
        Ok(bytes) if over(&bytes, RECEIPT_LIMIT) => {
            return result(
                crate::INVALID,
                "invalid-receipt",
                json!({"message": "receipt is larger than 64 KiB"}),
            );
        }
        Ok(bytes) => bytes,
        Err(error) => return io_failure(&error),
    };
    let (original, candidate) = match read_programs(original, candidate) {
        Ok(pair) => pair,
        Err(failure) => return failure,
    };
    match transform::replay(&recorded, &original, &candidate, max_input_tuples) {
        Replay::Matched => result(
            crate::OK,
            "replayed",
            json!({"receipt_sha256": transform::sha256_hex(&recorded)}),
        ),
        Replay::Unreadable => result(
            crate::INVALID,
            "invalid-receipt",
            json!({"message": "not a transform receipt with readable bindings"}),
        ),
        Replay::OverCap { size, cap } => result(
            crate::BLOCKED,
            "inconclusive",
            json!({
                "cause": "domain-too-large",
                "domain_size": size.map(|size| size.to_string()),
                "max_input_tuples": cap
            }),
        ),
        Replay::NotEquivalent(rejection) => {
            let (_, recomputed) = rejection_result(&rejection);
            result(
                crate::INVALID,
                "replay-mismatch",
                json!({"recomputed": recomputed}),
            )
        }
        Replay::Differs(fields) => result(
            crate::INVALID,
            "replay-mismatch",
            json!({"differing_fields": fields}),
        ),
    }
}

/// Reads both programs, refusing one above the importer's size limit before
/// it is hashed.
fn read_programs(original: &Path, candidate: &Path) -> Result<(Vec<u8>, Vec<u8>), (u8, Value)> {
    let read = |path: &Path, side: Side| match read_bounded(path, PROGRAM_LIMIT) {
        Ok(bytes) if over(&bytes, PROGRAM_LIMIT) => Err(result(
            crate::INVALID,
            "refused",
            json!({
                "reason": refused_side(side),
                "code": "program-too-large",
                "max_bytes": PROGRAM_LIMIT
            }),
        )),
        Ok(bytes) => Ok(bytes),
        Err(error) => Err(io_failure(&error)),
    };
    Ok((
        read(original, Side::Original)?,
        read(candidate, Side::Candidate)?,
    ))
}

/// Reads at most `limit + 1` bytes from a regular file. A FIFO cannot block
/// the open. The loop command shares this reader.
pub(super) fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn over(bytes: &[u8], limit: u64) -> bool {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit
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

fn rejection_result(rejection: &Rejection) -> (u8, Value) {
    match rejection {
        Rejection::Counterexample(witness) => result(
            crate::INVALID,
            "counterexample",
            json!({
                "ordinal": witness.ordinal,
                "input": transform::values_json(&witness.input),
                "original": observation_json(&witness.original),
                "candidate": observation_json(&witness.candidate)
            }),
        ),
        Rejection::Inconclusive(stop) => {
            result(crate::BLOCKED, "inconclusive", inconclusive_json(stop))
        }
        Rejection::Refused(refusal) => result(crate::INVALID, "refused", refusal_json(refusal)),
    }
}

fn observation_json(observation: &Observation) -> Value {
    match &observation.result {
        Ok(outputs) => json!({"ok": transform::values_json(outputs), "steps": observation.steps}),
        Err(failure) => {
            json!({"error": transform::failure_tag(*failure), "steps": observation.steps})
        }
    }
}

fn inconclusive_json(stop: &Inconclusive) -> Value {
    match stop {
        Inconclusive::DomainTooLarge { size, limit } => json!({
            "cause": "domain-too-large",
            "domain_size": size.map(|size| size.to_string()),
            "max_input_tuples": limit
        }),
        Inconclusive::BudgetBoundary {
            over_limit,
            minimum_limit,
            usage,
        } => json!({
            "cause": "budget-boundary",
            "over_limit": {"original": over_limit[0], "candidate": over_limit[1]},
            "minimum_step_limit": minimum_limit,
            "usage": transform::usage_json(usage)
        }),
        Inconclusive::CoverageMismatch { expected, visited } => json!({
            "cause": "coverage-mismatch", "expected": expected, "visited": visited
        }),
    }
}

fn refused_side(side: Side) -> &'static str {
    match side {
        Side::Original => "original-not-admitted",
        Side::Candidate => "candidate-not-admitted",
    }
}

fn refusal_json(refusal: &Refusal) -> Value {
    match refusal {
        Refusal::NotAdmitted { side, code } => json!({"reason": refused_side(*side), "code": code}),
        Refusal::InputAbi {
            original,
            candidate,
        } => json!({
            "reason": "input-abi-mismatch",
            "original": transform::domains_json(original),
            "candidate": transform::domains_json(candidate)
        }),
        Refusal::OutputAbi {
            original,
            candidate,
        } => json!({
            "reason": "output-abi-mismatch",
            "original": transform::domains_json(original),
            "candidate": transform::domains_json(candidate)
        }),
        Refusal::EmptyInputDomain { position } => {
            json!({"reason": "empty-input-domain", "position": position})
        }
    }
}

#[cfg(test)]
mod terminal_io_tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn receipt_staging_failure_retains_actual_completed_work_without_verdict() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/benchmarks/withdrawal-queue/artifacts");
        let original = fixtures.join("boolean-kernel-original.zcve");
        let candidate = fixtures.join("boolean-kernel-candidate.zcve");
        let missing = std::env::temp_dir().join(format!(
            "zeno-missing-receipt-parent-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        assert!(!missing.exists());
        let receipt = missing.join("receipt.json");
        let (exit, report) = check(
            &original,
            &candidate,
            Limits {
                steps: 256,
                input_tuples: 1000,
            },
            Some(&receipt),
            Some(crate::transform_plan::RunLimits::offline()),
        );
        assert_eq!(exit, crate::FAILURE, "{report}");
        assert_eq!(report["status"], "io-error");
        assert_eq!(report["authority"], "none");
        assert_eq!(report["progress"]["evaluated_pairs"], 16);
        assert!(
            report["progress"]["reserved_work"]
                .as_u64()
                .unwrap_or_else(|| panic!("required test value"))
                > 0
        );
        assert!(
            report["progress"]["measured_steps"][0]
                .as_str()
                .unwrap_or_else(|| panic!("required test value"))
                .parse::<u128>()
                .unwrap_or_else(|error| panic!("{error:?}"))
                > 0
        );
        assert_eq!(report["progress"]["accounting_complete"], true);
        assert!(report["original_sha256"].is_string());
        assert!(report["candidate_sha256"].is_string());
        assert!(report["detail"].get("receipt").is_none());
        assert!(report["detail"].get("ordinal").is_none());
        assert!(!receipt.exists());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn a_receipt_staged_after_the_deadline_is_dropped_without_handover() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/benchmarks/withdrawal-queue/artifacts");
        let directory = std::env::temp_dir().join(format!(
            "zeno-late-receipt-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap_or_else(|error| panic!("{error}"));
        let receipt = directory.join("receipt.json");
        CHECK_COMPLETION_DELAY.with(|delay| delay.set(std::time::Duration::from_millis(150)));
        let (exit, report) = check(
            &fixtures.join("boolean-kernel-original.zcve"),
            &fixtures.join("boolean-kernel-candidate.zcve"),
            Limits {
                steps: 256,
                input_tuples: 1000,
            },
            Some(&receipt),
            Some(crate::transform_plan::RunLimits {
                deadline_ms: 100,
                ..crate::transform_plan::RunLimits::offline()
            }),
        );
        assert_eq!(exit, crate::BLOCKED, "{report}");
        assert_eq!(report["status"], "inconclusive");
        assert_eq!(report["detail"]["reason"], "deadline");
        assert_eq!(report["progress"]["evaluated_pairs"], 16);
        assert!(!receipt.exists());
        assert_eq!(
            std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("{error}"))
                .count(),
            0
        );
        std::fs::remove_dir(directory).unwrap_or_else(|error| panic!("{error}"));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn offline_parallel_refuses_without_containment_while_serial_check_remains_available() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/benchmarks/withdrawal-queue/artifacts");
        let directory = std::env::temp_dir().join(format!(
            "zeno-unsupported-containment-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir(&directory).unwrap_or_else(|error| panic!("{error}"));
        let receipt = directory.join("receipt.json");
        let original = fixtures.join("boolean-kernel-original.zcve");
        let candidate = fixtures.join("boolean-kernel-candidate.zcve");
        let limits = Limits {
            steps: 256,
            input_tuples: 1000,
        };
        let (exit, report) = check(
            &original,
            &candidate,
            limits,
            Some(&receipt),
            Some(crate::transform_plan::RunLimits::offline()),
        );
        assert_eq!(exit, crate::BLOCKED, "{report}");
        assert_eq!(report["status"], "inconclusive");
        assert_eq!(report["authority"], "none");
        assert_eq!(
            report["detail"]["reason"],
            "containment-unavailable:platform"
        );
        assert_eq!(
            std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("{error}"))
                .count(),
            0
        );
        let (exit, report) = check(&original, &candidate, limits, Some(&receipt), None);
        assert_eq!(exit, crate::OK, "{report}");
        assert_eq!(report["status"], "equivalent");
        assert!(receipt.is_file());
        std::fs::remove_dir_all(directory).unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn publication_collision_after_completed_check_retains_work_and_existing_bytes() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/benchmarks/withdrawal-queue/artifacts");
        let directory = std::env::temp_dir().join(format!(
            "zeno-post-check-collision-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir(&directory).unwrap_or_else(|error| panic!("{error:?}"));
        let receipt = directory.join("receipt.json");
        let (exit, report, bytes) = check_value(
            &fixtures.join("boolean-kernel-original.zcve"),
            &fixtures.join("boolean-kernel-candidate.zcve"),
            Limits {
                steps: 256,
                input_tuples: 1000,
            },
            Some(&receipt),
            Some(crate::transform_plan::RunLimits::offline()),
            std::time::Instant::now(),
        );
        assert_eq!(exit, crate::OK, "{report}");
        assert_eq!(report["status"], "equivalent");
        assert_eq!(report["progress"]["evaluated_pairs"], 16);
        let progress = report["progress"].clone();
        let staged = crate::checker_process::StagedReceipt::new(
            &receipt,
            &bytes.unwrap_or_else(|| panic!("completed receipt bytes")),
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
        // The collision occurs after evaluation and staging, so early
        // path checks cannot satisfy this failure-path regression.
        std::fs::write(&receipt, b"other complete receipt")
            .unwrap_or_else(|error| panic!("{error:?}"));
        let (exit, failed) = publish_check(Some(staged), (exit, report));
        assert_eq!(exit, crate::FAILURE);
        assert_eq!(failed["status"], "io-error");
        assert_eq!(failed["authority"], "none");
        assert_eq!(failed["progress"], progress);
        assert!(failed["detail"].get("receipt").is_none());
        assert_eq!(
            std::fs::read(&receipt).unwrap_or_else(|error| panic!("{error:?}")),
            b"other complete receipt"
        );
        assert_eq!(
            std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("{error:?}"))
                .count(),
            1
        );
        std::fs::remove_dir_all(directory).unwrap_or_else(|error| panic!("{error:?}"));
    }
}
