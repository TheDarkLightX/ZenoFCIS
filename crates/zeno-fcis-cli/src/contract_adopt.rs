//! `zeno-fcis contract adopt`: records one more adopted candidate decision
//! program in an application's rules and regenerates its contract lineage.
//! The pure transitions in `crate::contract` check the candidate, by
//! replaying its receipt against the application's current program, and plan
//! every file; this module only reads and writes files, and writes nothing
//! unless everything generated.
//!
//! `zeno-fcis contract refresh-receipts` rebinds every adoption's receipt to
//! this checker's identity after a deliberate change of its semantics
//! version: each pair is checked again, and nothing but the checker identity
//! may change.

use std::fs;
use std::path::{Path, PathBuf};

use clap::{Subcommand, ValueEnum};
use serde_json::json;

use crate::contract::{
    Adopted, AdoptionPlan, CheckedCandidate, RefreshedReceipts, Usage, adoption_directory,
    refresh_receipts,
};
use crate::contract_files::{
    ADOPTED_PROGRAM, ADOPTED_RECEIPT, Failure, Inputs, RULES, invalid, outputs, read_input,
    report_failure, summary_json, write_output,
};
use crate::transform::CHECKER;
use crate::{FAILURE, JSON_SCHEMA, OK, OutputFormat, atomic_create, atomic_replace, print_json};

/// The library importer refuses programs above 64 KiB, and receipts are far
/// smaller; a larger file is refused before it is hashed.
const ATTACHMENT_LIMIT: usize = 64 * 1024;

#[derive(Subcommand)]
pub(super) enum Command {
    /// Adopt a checked candidate decision program as the application's next contract version.
    Adopt {
        /// Application directory holding project.zeno and v2/policy.json.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Canonical candidate program (program.zcve encoding), at most 64 KiB.
        #[arg(long)]
        candidate: PathBuf,
        /// Receipt from `transform check` that compares the candidate with the application's current decision program.
        #[arg(long)]
        receipt: PathBuf,
        /// `preserved` only when the receipt reports equal Step usage on every input; otherwise `new-version`.
        #[arg(long, value_enum)]
        usage: UsageArg,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Check every adopted candidate again with this checker and rebind each receipt to its identity; refused when anything but the checker identity would change.
    RefreshReceipts {
        /// Application directory holding project.zeno and v2/policy.json.
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum UsageArg {
    Preserved,
    NewVersion,
}

impl From<UsageArg> for Usage {
    fn from(value: UsageArg) -> Self {
        match value {
            UsageArg::Preserved => Self::Preserved,
            UsageArg::NewVersion => Self::NewVersion,
        }
    }
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Adopt {
            dir,
            candidate,
            receipt,
            usage,
            format,
        } => adopt(&dir, &candidate, &receipt, usage.into(), format),
        Command::RefreshReceipts { dir, format } => refresh(&dir, format),
    }
}

/// Whether a retained file already holds the planned bytes, from an
/// adoption that was interrupted before it wrote the rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Retained {
    Absent,
    Present,
}

fn adopt(dir: &Path, candidate: &Path, receipt: &Path, usage: Usage, format: OutputFormat) -> u8 {
    let (plan, state) = match plan(dir, candidate, receipt, usage) {
        Ok(planned) => planned,
        Err(failure) => return report_failure(dir, failure, format),
    };
    match write(dir, plan, state) {
        Ok(adopted) => report(dir, &adopted, format),
        Err((name, error)) => {
            let message = format!("write {name}: {error}");
            match format {
                OutputFormat::Human => eprintln!("{message}"),
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": "error", "path": dir.display().to_string(),
                    "authority": "none",
                    "error": {"code": "adoption-write-failed", "place": name, "message": message}
                })),
            }
            FAILURE
        }
    }
}

/// Reads the application, both attachments and the next retained
/// directory, then runs the pure transitions: the replayed check, then the
/// plan.
fn plan(
    dir: &Path,
    candidate: &Path,
    receipt: &Path,
    usage: Usage,
) -> Result<(AdoptionPlan, [Retained; 2]), Failure> {
    let inputs = Inputs::read(dir)?;
    let candidate = read_attachment(candidate, "--candidate")?;
    let receipt = read_attachment(receipt, "--receipt")?;
    let adoptions = inputs.adoption_sources();
    let sources = inputs.sources(&inputs.rules, &adoptions);
    // A directory holding other files is refused before any replay.
    let state = retained_state(
        dir,
        &adoption_directory(adoptions.len() + 1),
        &candidate,
        &receipt,
    )?;
    let checked = CheckedCandidate::check(sources, candidate, receipt)?;
    Ok((checked.plan(sources, usage)?, state))
}

/// The next adoption's directory must be absent, or hold only this
/// candidate and receipt, which an interrupted adoption of the same files
/// left before it wrote the rules; running it again then finishes it.
fn retained_state(
    dir: &Path,
    directory: &str,
    candidate: &[u8],
    receipt: &[u8],
) -> Result<[Retained; 2], Failure> {
    let refused = || {
        invalid(
            directory,
            "already exists, but the rules list fewer adoptions and it holds other files than this candidate and receipt",
        )
    };
    let path = dir.join(directory);
    // Unlike `exists`, `symlink_metadata` also sees a dangling link.
    let Ok(metadata) = path.symlink_metadata() else {
        return Ok([Retained::Absent; 2]);
    };
    if !metadata.is_dir() {
        return Err(refused());
    }
    let entries = fs::read_dir(&path)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| Failure::Read(format!("read {directory}: {error}")))?;
    if entries
        .iter()
        .any(|name| name != ADOPTED_PROGRAM && name != ADOPTED_RECEIPT)
    {
        return Err(refused());
    }
    let state = |name: &str, expected: &[u8]| {
        let file = path.join(name);
        match file.symlink_metadata() {
            Err(_) => Ok(Retained::Absent),
            Ok(_) if read_input(&file).ok().as_deref() == Some(expected) => Ok(Retained::Present),
            Ok(_) => Err(refused()),
        }
    };
    Ok([
        state(ADOPTED_PROGRAM, candidate)?,
        state(ADOPTED_RECEIPT, receipt)?,
    ])
}

/// Writes the retained files first, then the generated lineage, then the
/// rules that name them. An interruption leaves the rules unchanged:
/// `generate contract` then regenerates the previous version, and the same
/// `contract adopt` command finishes the adoption.
fn write(
    dir: &Path,
    plan: AdoptionPlan,
    retained: [Retained; 2],
) -> Result<Adopted, (String, std::io::Error)> {
    let mut artifacts = Vec::new();
    fs::create_dir_all(dir.join(plan.directory()))
        .map_err(|error| (plan.directory().to_owned(), error))?;
    for ((name, bytes), state) in plan.retained().into_iter().zip(retained) {
        if state == Retained::Absent {
            atomic_create(&dir.join(&name), bytes).map_err(|error| (name.clone(), error))?;
        }
        artifacts.push(name);
    }
    for (name, bytes) in outputs(plan.generated()) {
        write_output(&dir.join(&name), bytes).map_err(|error| (name.clone(), error))?;
        artifacts.push(name);
    }
    atomic_replace(&dir.join(RULES), plan.rules().as_bytes())
        .map_err(|error| (RULES.to_owned(), error))?;
    artifacts.push(RULES.to_owned());
    Ok(plan.written(artifacts))
}

fn report(dir: &Path, adopted: &Adopted, format: OutputFormat) -> u8 {
    let summary = adopted.generated.summary();
    let adoption = summary
        .adoptions
        .last()
        .unwrap_or_else(|| unreachable!("the generator recorded the adoption"));
    match format {
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "adopted", "path": dir.display().to_string(),
            "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
            "adoption": {
                "ordinal": adopted.ordinal, "version": summary.version,
                "directory": adopted.directory,
                "candidate_sha256": adoption.candidate_sha256,
                "receipt_sha256": adoption.receipt_sha256,
                "superseded_policy_sha256": adoption.superseded_policy_sha256,
                "usage": adoption.usage.name(), "usage_preserved": adoption.usage_preserved,
                "program_nodes": {"before": adoption.program_nodes[0], "after": adoption.program_nodes[1]}
            },
            "artifacts": adopted.artifacts,
            "summary": summary_json(&adopted.generated)
        })),
        OutputFormat::Human => println!(
            "adopted candidate {} as contract version {} of {} in {}: {} -> {} program nodes, usage {}; wrote {}",
            adoption.candidate_sha256,
            summary.version,
            summary.application,
            dir.display(),
            adoption.program_nodes[0],
            adoption.program_nodes[1],
            adoption.usage.name(),
            adopted.artifacts.join(", ")
        ),
    }
    OK
}

/// Rechecks every adoption and writes the rebound receipts, then the
/// regenerated lineage, then the rules that name them. An interrupted refresh
/// is finished by running it again. Nothing is written when every receipt is
/// already this checker's.
fn refresh(dir: &Path, format: OutputFormat) -> u8 {
    let planned = Inputs::read(dir).and_then(|inputs| {
        let adoptions = inputs.adoption_sources();
        let refreshed = refresh_receipts(inputs.sources(&inputs.rules, &adoptions))?;
        let rebound: Vec<usize> = inputs
            .adoptions
            .iter()
            .zip(refreshed.receipts())
            .enumerate()
            .filter(|(_, (files, receipt))| files.receipt != **receipt)
            .map(|(index, _)| index + 1)
            .collect();
        let current = rebound.is_empty() && inputs.rules == refreshed.rules();
        Ok((refreshed, rebound, current))
    });
    let (refreshed, rebound, current) = match planned {
        Ok(planned) => planned,
        Err(failure) => return report_failure(dir, failure, format),
    };
    let mut artifacts = Vec::new();
    if !current && let Err((name, error)) = write_refresh(dir, &refreshed, &mut artifacts) {
        let message = format!("write {name}: {error}");
        match format {
            OutputFormat::Human => eprintln!("{message}"),
            OutputFormat::Json => print_json(&json!({
                "schema": JSON_SCHEMA, "status": "error", "path": dir.display().to_string(),
                "authority": "none",
                "error": {"code": "refresh-write-failed", "place": name, "message": message}
            })),
        }
        return FAILURE;
    }
    let status = if current { "current" } else { "refreshed" };
    match format {
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
            "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
            "checker": CHECKER, "refreshed": rebound, "artifacts": artifacts,
            "summary": summary_json(refreshed.generated())
        })),
        OutputFormat::Human if current => println!(
            "every adoption receipt in {} is already this checker's ({CHECKER})",
            dir.display()
        ),
        OutputFormat::Human => println!(
            "rebound the receipts of adoptions {rebound:?} in {} to {CHECKER}; wrote {}",
            dir.display(),
            artifacts.join(", ")
        ),
    }
    OK
}

fn write_refresh(
    dir: &Path,
    refreshed: &RefreshedReceipts,
    artifacts: &mut Vec<String>,
) -> Result<(), (String, std::io::Error)> {
    for (index, receipt) in refreshed.receipts().iter().enumerate() {
        let name = format!("{}/{ADOPTED_RECEIPT}", adoption_directory(index + 1));
        atomic_replace(&dir.join(&name), receipt).map_err(|error| (name.clone(), error))?;
        artifacts.push(name);
    }
    for (name, bytes) in outputs(refreshed.generated()) {
        write_output(&dir.join(&name), bytes).map_err(|error| (name.clone(), error))?;
        artifacts.push(name);
    }
    atomic_replace(&dir.join(RULES), refreshed.rules().as_bytes())
        .map_err(|error| (RULES.to_owned(), error))?;
    artifacts.push(RULES.to_owned());
    Ok(())
}

/// A candidate or receipt file, bounded before it is hashed.
fn read_attachment(path: &Path, option: &str) -> Result<Vec<u8>, Failure> {
    let bytes = read_input(path)
        .map_err(|error| Failure::Read(format!("read {} ({option}): {error}", path.display())))?;
    if bytes.len() > ATTACHMENT_LIMIT {
        return Err(invalid(option, "is larger than 64 KiB"));
    }
    Ok(bytes)
}
