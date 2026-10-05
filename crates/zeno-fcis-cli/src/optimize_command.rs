//! `zeno-fcis optimize`: the command over the pure `optimize` module. This
//! module only reads and writes files and renders JSON; no result grants
//! application authority, and no candidate is reported as accepted unless the
//! transform checker accepted it.

use crate::optimize::strategy::Strategy;
use crate::optimize::{self, Outcome, Refused};
use crate::transform;
use clap::Args;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The library importer refuses canonical programs above 64 KiB. A larger
/// file is refused here before it is hashed or decoded.
const PROGRAM_LIMIT: u64 = 64 * 1024;
const STRATEGY_LIMIT: u64 = 64 * 1024;

#[derive(Args)]
pub(super) struct Arguments {
    /// Canonical original program (program.zcve encoding).
    #[arg(long)]
    program: PathBuf,
    /// Strategy document (JSON, schema zeno-fcis/optimize-strategy/1). The
    /// fixed default strategy applies when absent.
    #[arg(long)]
    strategy: Option<PathBuf>,
    /// Create a new file holding the best accepted candidate's canonical bytes.
    #[arg(long)]
    candidate_out: Option<PathBuf>,
    /// Create a new file holding the best accepted candidate's equivalence receipt.
    #[arg(long)]
    receipt: Option<PathBuf>,
    /// Largest input domain the checker may enumerate; a larger domain is
    /// inconclusive before any search.
    #[arg(long, default_value_t = transform::DEFAULT_MAX_INPUT_TUPLES)]
    max_input_tuples: u64,
}

pub(super) fn run(arguments: Arguments) -> u8 {
    let (exit, report) = optimize_files(
        &arguments.program,
        arguments.strategy.as_deref(),
        arguments.candidate_out.as_deref(),
        arguments.receipt.as_deref(),
        arguments.max_input_tuples,
    );
    crate::print_json(&report);
    exit
}

fn optimize_files(
    program: &Path,
    strategy: Option<&Path>,
    candidate_out: Option<&Path>,
    receipt: Option<&Path>,
    max_input_tuples: u64,
) -> (u8, Value) {
    // Unlike `exists`, `symlink_metadata` also sees a dangling link, which
    // exclusive creation would refuse after the whole search.
    for path in [candidate_out, receipt].into_iter().flatten() {
        if path.symlink_metadata().is_ok() {
            return result(
                crate::INVALID,
                "output-exists",
                json!({"path": path.display().to_string()}),
            );
        }
    }
    let original = match read_bounded(program, PROGRAM_LIMIT) {
        Ok(bytes) if over(&bytes, PROGRAM_LIMIT) => {
            return result(
                crate::INVALID,
                "refused",
                json!({
                    "reason": "original-not-admitted",
                    "code": "program-too-large",
                    "max_bytes": PROGRAM_LIMIT
                }),
            );
        }
        Ok(bytes) => bytes,
        Err(error) => return io_failure(&error),
    };
    let strategy_bytes = match strategy {
        Some(path) => match read_bounded(path, STRATEGY_LIMIT) {
            Ok(bytes) if over(&bytes, STRATEGY_LIMIT) => {
                return result(
                    crate::INVALID,
                    "invalid-strategy",
                    json!({"message": "strategy is larger than 64 KiB"}),
                );
            }
            Ok(bytes) => bytes,
            Err(error) => return io_failure(&error),
        },
        None => Vec::from(optimize::strategy::DEFAULT_STRATEGY_JSON.as_bytes()),
    };
    let strategy = match Strategy::parse(&strategy_bytes) {
        Ok(strategy) => strategy,
        Err(error) => {
            return result(
                crate::INVALID,
                "invalid-strategy",
                json!({"message": error.message}),
            );
        }
    };
    let (exit, mut report) = match optimize::optimize(&original, &strategy, max_input_tuples) {
        Outcome::Refused(Refused::NotAdmitted { code }) => result(
            crate::INVALID,
            "refused",
            json!({"reason": "original-not-admitted", "code": code}),
        ),
        Outcome::Refused(Refused::EmptyInputDomain) => result(
            crate::INVALID,
            "refused",
            json!({"reason": "empty-input-domain"}),
        ),
        Outcome::Inconclusive(stop) => result(
            crate::BLOCKED,
            "inconclusive",
            json!({
                "cause": "domain-too-large",
                "domain_size": stop.size.map(|size| size.to_string()),
                "max_input_tuples": stop.limit
            }),
        ),
        Outcome::Searched(search) => {
            let mut detail = search.json();
            match &search.best {
                Some(best) => {
                    let receipt_bytes = best.equivalence.receipt();
                    if let Some(path) = candidate_out
                        && let Err(error) = crate::atomic_create(path, &best.bytes)
                    {
                        return io_failure(&error);
                    }
                    if let Some(path) = receipt
                        && let Err(error) = crate::atomic_create(path, &receipt_bytes)
                    {
                        return io_failure(&error);
                    }
                    detail["candidate_path"] =
                        json!(candidate_out.map(|path| path.display().to_string()));
                    detail["receipt_path"] = json!(receipt.map(|path| path.display().to_string()));
                    result(crate::OK, "improved", detail)
                }
                None => result(crate::BLOCKED, "no-checked-improvement", detail),
            }
        }
    };
    report["original_sha256"] = json!(transform::sha256_hex(&original));
    report["strategy_sha256"] = json!(transform::sha256_hex(&strategy_bytes));
    report["limits"] = json!({
        "steps": transform::DEFAULT_STEP_LIMIT,
        "input_tuples": max_input_tuples
    });
    (exit, report)
}

/// Reads at most `limit + 1` bytes from a regular file. A FIFO cannot block
/// the open.
fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
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

fn over(bytes: &[u8], limit: u64) -> bool {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit
}

fn result(exit: u8, status: &str, detail: Value) -> (u8, Value) {
    (
        exit,
        json!({
            "schema": optimize::RESULT_SCHEMA,
            "status": status,
            "authority": "none",
            "detail": detail
        }),
    )
}

fn io_failure(error: &std::io::Error) -> (u8, Value) {
    result(
        crate::FAILURE,
        "io-error",
        json!({"message": error.to_string()}),
    )
}
