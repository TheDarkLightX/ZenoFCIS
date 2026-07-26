//! Runs the exact pinned Python/Rust zUSD mount and writes bounded refinement evidence.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use zeno_fcis_adapter_zenodex::zusd::{
    MAX_NATIVE_LINE_BYTES, PINNED_PYTHON_ENTRYPOINT, PINNED_RUST_SUBCOMMAND, PINNED_ZENODEX_COMMIT,
    ZusdMountBindingsV1, ZusdMountInputV1, decode_zusd_native_decision_line_v1,
    encode_zusd_native_request_line_v1, normalize_zusd_native_decision_v1, zusd_mount_case_id_v1,
};
use zeno_fcis_adapter_zenodex::{compare_case, decision_commitment};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_core::DecisionKind;
use zeno_fcis_profile_zenodex::{ZusdCommandV1, ZusdStateV1};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_STDERR_BYTES: usize = 64 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mount-zenodex-zusd: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = Arguments::parse()?;
    verify_source(&arguments.zenodex_root)?;
    fs::create_dir_all(&arguments.output_dir)
        .map_err(|error| format!("create output directory: {error}"))?;
    let python = arguments.zenodex_root.join(PINNED_PYTHON_ENTRYPOINT);
    if !python.is_file() {
        return Err(format!(
            "missing pinned Python entry point: {}",
            python.display()
        ));
    }
    if !arguments.rust_binary.is_file() {
        return Err(format!(
            "missing pinned Rust binary: {}",
            arguments.rust_binary.display()
        ));
    }

    let fixed = ZusdMountBindingsV1::pinned().map_err(|error| error.to_string())?;
    let mut state = ZusdStateV1::reference_initial().map_err(|error| error.to_string())?;
    let mut accepted = 0_u32;
    let mut rejected = 0_u32;
    let mut cases = Vec::new();

    for (index, command) in corpus().into_iter().enumerate() {
        let input = ZusdMountInputV1::new(state.clone(), command);
        let request = encode_zusd_native_request_line_v1(&input)
            .map_err(|error| format!("case {index} request: {error}"))?;
        let python_output = run_process(
            "python",
            &arguments.python_binary,
            &[python.as_os_str()],
            &arguments.zenodex_root,
            &request,
        )?;
        let rust_output = run_process(
            "rust",
            &arguments.rust_binary,
            &[
                std::ffi::OsStr::new(PINNED_RUST_SUBCOMMAND),
                std::ffi::OsStr::new("-"),
            ],
            &arguments.zenodex_root,
            &request,
        )?;

        let python_native = match decode_zusd_native_decision_line_v1(&python_output, &input) {
            Ok(decision) => decision,
            Err(error) => {
                persist_transport_failure(
                    &arguments.output_dir,
                    index,
                    &request,
                    &python_output,
                    &rust_output,
                )?;
                return Err(format!("case {index} Python output: {error}"));
            }
        };
        let rust_native = match decode_zusd_native_decision_line_v1(&rust_output, &input) {
            Ok(decision) => decision,
            Err(error) => {
                persist_transport_failure(
                    &arguments.output_dir,
                    index,
                    &request,
                    &python_output,
                    &rust_output,
                )?;
                return Err(format!("case {index} Rust output: {error}"));
            }
        };
        let python_decision = normalize_zusd_native_decision_v1(&input, &python_native)
            .map_err(|error| format!("case {index} Python normalize: {error}"))?;
        let rust_decision = normalize_zusd_native_decision_v1(&input, &rust_native)
            .map_err(|error| format!("case {index} Rust normalize: {error}"))?;
        let case_id = zusd_mount_case_id_v1(&input).map_err(|error| error.to_string())?;
        let compared = compare_case(case_id, &request, &python_decision, &rust_decision)
            .map_err(|error| format!("case {index} compare: {error}"))?;
        if !compared.report().is_exact() || python_output != rust_output {
            persist_divergence(
                &arguments.output_dir,
                index,
                &request,
                &python_output,
                &rust_output,
                compared.replay(),
            )?;
            return Err(format!(
                "case {index} tool disagreement: {:?}",
                compared.report().mismatches()
            ));
        }

        let artifacts = python_decision.artifacts();
        match artifacts.kind {
            DecisionKind::Accept => {
                accepted = accepted.saturating_add(1);
                state = python_native.post_state().clone();
            }
            DecisionKind::Reject => rejected = rejected.saturating_add(1),
            DecisionKind::CommittedFailure => {
                return Err(format!("case {index} unexpectedly committed a failure"));
            }
        }
        cases.push(CaseReport {
            index: u32::try_from(index).map_err(|_| "case index overflow".to_owned())?,
            case_id: case_id.to_string(),
            command: command.tag().native_name(),
            kind: kind_label(artifacts.kind),
            reason: artifacts.reason_code.as_deref().map(str::to_owned),
            pre_root: artifacts.pre_root.to_string(),
            post_root: artifacts.post_root.to_string(),
            candidate_id: artifacts.candidate_id.map(|value| value.to_string()),
            decision_commitment: decision_commitment(&python_decision)
                .map_err(|error| error.to_string())?
                .to_string(),
        });
    }

    let final_root = state
        .root::<zeno_fcis_crypto::RustCryptoSha256>(fixed.profile())
        .map_err(|error| error.to_string())?;
    let report = MountReport {
        schema: "zeno-fcis.mounted-zenodex-zusd.v1",
        exact: true,
        zenodex_commit: PINNED_ZENODEX_COMMIT,
        python_entrypoint: PINNED_PYTHON_ENTRYPOINT,
        rust_subcommand: PINNED_RUST_SUBCOMMAND,
        rust_toolchain: "1.89.0",
        zeno_fcis_toolchain: "1.97.1",
        profile_hash: fixed.profile_hash().to_string(),
        schema_hash: fixed.schema_hash().to_string(),
        precedence_hash: fixed.precedence_hash().to_string(),
        algorithm_hash: fixed.algorithm_hash().to_string(),
        context_hash: fixed.context_hash().to_string(),
        budget_hash: fixed.budget_hash().to_string(),
        cases,
        accepted,
        rejected,
        final_root: final_root.to_string(),
        nonclaims: [
            "bounded executable refinement evidence, not an unbounded proof",
            "single-vault zUSD profile only",
            "empty commit/outbox plans do not authorize production effects",
            "no production authority or release status",
        ],
    };
    let mut bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| format!("encode report: {error}"))?;
    bytes.push(b'\n');
    fs::write(arguments.output_dir.join("report.json"), bytes)
        .map_err(|error| format!("write report: {error}"))?;
    println!(
        "mounted {} exact cases ({} accept, {} reject)",
        accepted.saturating_add(rejected),
        accepted,
        rejected
    );
    Ok(())
}

fn corpus() -> [ZusdCommandV1; 17] {
    [
        ZusdCommandV1::MintZusd {
            amount_e8: 20_000_000_000,
        },
        ZusdCommandV1::BootstrapOracle {
            auth_ok: false,
            price_e8: 100_000_000,
        },
        ZusdCommandV1::BootstrapOracle {
            auth_ok: true,
            price_e8: 100_000_000,
        },
        ZusdCommandV1::BootstrapOracle {
            auth_ok: true,
            price_e8: 100_000_000,
        },
        ZusdCommandV1::MintZusd {
            amount_e8: 20_000_000_000,
        },
        ZusdCommandV1::DepositCollateral {
            amount_e8: 100_000_000_000,
        },
        ZusdCommandV1::MintZusd { amount_e8: 1 },
        ZusdCommandV1::MintZusd {
            amount_e8: 20_000_000_000,
        },
        ZusdCommandV1::DepositStabilityPool {
            amount_e8: 5_000_000_000,
        },
        ZusdCommandV1::WithdrawStabilityPool {
            amount_e8: 1_000_000_000,
        },
        ZusdCommandV1::RepayZusd {
            amount_e8: 5_000_000_000,
        },
        ZusdCommandV1::RedeemZusd {
            amount_e8: 1_000_000_000,
        },
        ZusdCommandV1::WithdrawCollateral {
            amount_e8: 100_000_000_000,
        },
        ZusdCommandV1::OracleReport {
            auth_ok: true,
            price_e8: 90_000_000,
        },
        ZusdCommandV1::OracleReport {
            auth_ok: true,
            price_e8: 200_000_000,
        },
        ZusdCommandV1::MintZusd {
            amount_e8: 20_000_000_000,
        },
        ZusdCommandV1::OracleCommit { auth_ok: true },
    ]
}

fn verify_source(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("read ZenoDEX revision: {error}"))?;
    if !output.status.success() {
        return Err("ZenoDEX revision command failed".to_owned());
    }
    let revision = String::from_utf8(output.stdout)
        .map_err(|_| "ZenoDEX revision was not UTF-8".to_owned())?;
    let status = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all", "--"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("read ZenoDEX worktree status: {error}"))?;
    if !status.status.success() {
        return Err("ZenoDEX worktree status command failed".to_owned());
    }
    validate_source_identity(revision.trim(), &status.stdout)
}

fn validate_source_identity(revision: &str, porcelain_status: &[u8]) -> Result<(), String> {
    if revision != PINNED_ZENODEX_COMMIT {
        return Err(format!(
            "ZenoDEX revision mismatch: expected {PINNED_ZENODEX_COMMIT}, got {}",
            revision
        ));
    }
    if !porcelain_status.is_empty() {
        return Err(
            "ZenoDEX worktree is dirty; pinned evidence requires no tracked or untracked changes"
                .to_owned(),
        );
    }
    Ok(())
}

fn run_process(
    label: &str,
    program: &Path,
    args: &[&std::ffi::OsStr],
    cwd: &Path,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn {label}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("{label} stdin unavailable"))?
        .write_all(request)
        .map_err(|error| format!("write {label} request: {error}"))?;
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if child
            .try_wait()
            .map_err(|error| format!("poll {label}: {error}"))?
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| format!("kill {label}: {error}"))?;
            let _ = child.wait();
            return Err(format!(
                "{label} timed out after {}s",
                PROCESS_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("collect {label}: {error}"))?;
    if output.stdout.len() > MAX_NATIVE_LINE_BYTES || output.stderr.len() > MAX_STDERR_BYTES {
        return Err(format!("{label} exceeded its output bound"));
    }
    if !output.status.success() {
        return Err(format!(
            "{label} exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !output.stderr.is_empty() {
        return Err(format!("{label} produced unexpected stderr"));
    }
    Ok(output.stdout)
}

fn persist_transport_failure(
    output: &Path,
    index: usize,
    request: &[u8],
    python: &[u8],
    rust: &[u8],
) -> Result<(), String> {
    persist_bytes(output, index, request, python, rust, None)
}

fn persist_divergence(
    output: &Path,
    index: usize,
    request: &[u8],
    python: &[u8],
    rust: &[u8],
    replay: Option<&zeno_fcis_adapter_zenodex::ReplayFixture>,
) -> Result<(), String> {
    let replay = replay
        .map(CanonicalEncode::canonical_bytes)
        .transpose()
        .map_err(|error| format!("encode replay: {error}"))?;
    persist_bytes(output, index, request, python, rust, replay.as_deref())
}

fn persist_bytes(
    output: &Path,
    index: usize,
    request: &[u8],
    python: &[u8],
    rust: &[u8],
    replay: Option<&[u8]>,
) -> Result<(), String> {
    let directory = output.join(format!("counterexample-{index:04}"));
    fs::create_dir_all(&directory).map_err(|error| format!("create counterexample: {error}"))?;
    for (name, bytes) in [
        ("request.jsonl", request),
        ("python.jsonl", python),
        ("rust.jsonl", rust),
    ] {
        fs::write(directory.join(name), bytes)
            .map_err(|error| format!("write counterexample {name}: {error}"))?;
    }
    if let Some(bytes) = replay {
        fs::write(directory.join("replay.zcve"), bytes)
            .map_err(|error| format!("write counterexample replay: {error}"))?;
    }
    Ok(())
}

const fn kind_label(kind: DecisionKind) -> &'static str {
    match kind {
        DecisionKind::Accept => "accept",
        DecisionKind::Reject => "reject",
        DecisionKind::CommittedFailure => "committed_failure",
    }
}

#[derive(Serialize)]
struct CaseReport {
    index: u32,
    case_id: String,
    command: &'static str,
    kind: &'static str,
    reason: Option<String>,
    pre_root: String,
    post_root: String,
    candidate_id: Option<String>,
    decision_commitment: String,
}

#[derive(Serialize)]
struct MountReport<'a> {
    schema: &'a str,
    exact: bool,
    zenodex_commit: &'a str,
    python_entrypoint: &'a str,
    rust_subcommand: &'a str,
    rust_toolchain: &'a str,
    zeno_fcis_toolchain: &'a str,
    profile_hash: String,
    schema_hash: String,
    precedence_hash: String,
    algorithm_hash: String,
    context_hash: String,
    budget_hash: String,
    cases: Vec<CaseReport>,
    accepted: u32,
    rejected: u32,
    final_root: String,
    nonclaims: [&'a str; 4],
}

struct Arguments {
    zenodex_root: PathBuf,
    rust_binary: PathBuf,
    python_binary: PathBuf,
    output_dir: PathBuf,
}

impl Arguments {
    fn parse() -> Result<Self, String> {
        let mut values = env::args_os().skip(1);
        let supplied_root = values.next().map(PathBuf::from).ok_or_else(|| {
            "usage: mount-zenodex-zusd <zenodex-root> <rust-bin> <output-dir>".to_owned()
        })?;
        let supplied_rust_binary = values
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing Rust runtime binary".to_owned())?;
        let zenodex_root = fs::canonicalize(&supplied_root).map_err(|error| {
            format!("resolve ZenoDEX root {}: {error}", supplied_root.display())
        })?;
        let rust_binary = fs::canonicalize(&supplied_rust_binary).map_err(|error| {
            format!(
                "resolve Rust binary {}: {error}",
                supplied_rust_binary.display()
            )
        })?;
        let output_dir = values
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing output directory".to_owned())?;
        if values.next().is_some() {
            return Err("unexpected extra argument".to_owned());
        }
        Ok(Self {
            zenodex_root,
            rust_binary,
            python_binary: PathBuf::from("python3"),
            output_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_revision_requires_a_clean_worktree() {
        assert!(validate_source_identity(PINNED_ZENODEX_COMMIT, b"").is_ok());
        assert!(
            validate_source_identity(PINNED_ZENODEX_COMMIT, b" M tools/runtime/zusd_fcis_op.py\n")
                .is_err()
        );
        assert!(
            validate_source_identity(PINNED_ZENODEX_COMMIT, b"?? src/core/shadow.py\n").is_err()
        );
    }

    #[test]
    fn clean_worktree_cannot_mask_the_wrong_revision() {
        assert!(validate_source_identity("0000000000000000000000000000000000000000", b"").is_err());
    }
}
