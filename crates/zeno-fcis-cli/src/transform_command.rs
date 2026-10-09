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

#[derive(Subcommand)]
pub(super) enum Command {
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
                None => check(&original, &candidate, limits, receipt.as_deref()),
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

fn check(original: &Path, candidate: &Path, limits: Limits, receipt: Option<&Path>) -> (u8, Value) {
    // Unlike `exists`, `symlink_metadata` also sees a dangling link, which
    // exclusive creation would refuse after the whole check.
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
    let (exit, mut report) = match transform::check(&original, &candidate, limits) {
        Ok(equivalence) => {
            let bytes = equivalence.receipt();
            if let Some(path) = receipt
                && let Err(error) = crate::atomic_create(path, &bytes)
            {
                return io_failure(&error);
            }
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
        Err(rejection) => rejection_result(&rejection),
    };
    report["limits"] = json!({"steps": limits.steps, "input_tuples": limits.input_tuples});
    report["original_sha256"] = json!(transform::sha256_hex(&original));
    report["candidate_sha256"] = json!(transform::sha256_hex(&candidate));
    (exit, report)
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
    if let Some(message) = solvers.retention_failure() {
        return result(crate::FAILURE, "io-error", json!({"message": message}));
    }
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
