//! `zeno-fcis contract adopt`: records one more adopted candidate decision
//! program in an application's rules and regenerates its contract lineage.
//! The pure generator replays the receipt against the application's current
//! program, re-derived from its declarations and rules; this module only
//! reads and writes files, and writes nothing unless everything generated.

use std::fs;
use std::path::{Path, PathBuf};

use clap::{Subcommand, ValueEnum};
use serde_json::json;

use crate::contract::{
    Adoption, AdoptionSources, GeneratedContract, Usage, adoption_directory, generate_contract,
    with_adoption,
};
use crate::contract_files::{
    ADOPTED_PROGRAM, ADOPTED_RECEIPT, Failure, Inputs, RULES, invalid, outputs, read_input,
    report_failure, summary_json, write_output,
};
use crate::transform::sha256_hex;
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
    let Command::Adopt {
        dir,
        candidate,
        receipt,
        usage,
        format,
    } = command;
    adopt(&dir, &candidate, &receipt, usage.into(), format)
}

/// Everything an adoption writes, computed before any write.
struct Plan {
    ordinal: usize,
    directory: String,
    candidate: Vec<u8>,
    receipt: Vec<u8>,
    rules: String,
    generated: GeneratedContract,
}

fn plan(dir: &Path, candidate: &Path, receipt: &Path, usage: Usage) -> Result<Plan, Failure> {
    let inputs = Inputs::read(dir)?;
    let candidate = read_attachment(candidate, "--candidate")?;
    let receipt = read_attachment(receipt, "--receipt")?;
    let ordinal = inputs.adoptions.len() + 1;
    let directory = adoption_directory(ordinal);
    // Unlike `exists`, `symlink_metadata` also sees a dangling link.
    if dir.join(&directory).symlink_metadata().is_ok() {
        return Err(invalid(
            &directory,
            "already exists, but the rules list fewer adoptions",
        ));
    }
    let adoption = Adoption {
        candidate_sha256: sha256_hex(&candidate),
        receipt_sha256: sha256_hex(&receipt),
        usage,
    };
    let rules = with_adoption(&inputs.rules, &adoption)?;
    let generated = {
        let mut sources = inputs.adoption_sources();
        sources.push(AdoptionSources {
            candidate: &candidate,
            receipt: &receipt,
        });
        generate_contract(inputs.sources(&rules, &sources))?
    };
    Ok(Plan {
        ordinal,
        directory,
        candidate,
        receipt,
        rules,
        generated,
    })
}

fn adopt(dir: &Path, candidate: &Path, receipt: &Path, usage: Usage, format: OutputFormat) -> u8 {
    let plan = match plan(dir, candidate, receipt, usage) {
        Ok(plan) => plan,
        Err(failure) => return report_failure(dir, failure, format),
    };
    // Retained files first, then the generated lineage, then the rules that
    // name them, so an interrupted adoption leaves the rules describing the
    // files that exist and `generate contract` can finish it.
    let retained = dir.join(&plan.directory);
    let program = format!("{}/{ADOPTED_PROGRAM}", plan.directory);
    let receipt_file = format!("{}/{ADOPTED_RECEIPT}", plan.directory);
    let mut writes: Vec<(String, Result<(), std::io::Error>)> = Vec::new();
    writes.push((
        program.clone(),
        fs::create_dir_all(&retained)
            .and_then(|()| atomic_create(&retained.join(ADOPTED_PROGRAM), &plan.candidate)),
    ));
    if writes.iter().all(|(_, result)| result.is_ok()) {
        writes.push((
            receipt_file.clone(),
            atomic_create(&retained.join(ADOPTED_RECEIPT), &plan.receipt),
        ));
    }
    let files = outputs(&plan.generated);
    for (name, bytes) in &files {
        if writes.iter().all(|(_, result)| result.is_ok()) {
            writes.push((name.clone(), write_output(&dir.join(name), bytes)));
        }
    }
    if writes.iter().all(|(_, result)| result.is_ok()) {
        writes.push((
            RULES.to_owned(),
            atomic_replace(&dir.join(RULES), plan.rules.as_bytes()),
        ));
    }
    if let Some((name, Err(error))) = writes.iter().find(|(_, result)| result.is_err()) {
        let message = format!("write {name}: {error}");
        match format {
            OutputFormat::Human => eprintln!("{message}"),
            OutputFormat::Json => print_json(&json!({
                "schema": JSON_SCHEMA, "status": "error", "path": dir.display().to_string(),
                "authority": "none",
                "error": {"code": "adoption-write-failed", "place": name, "message": message}
            })),
        }
        return FAILURE;
    }
    let summary = plan.generated.summary();
    let adoption = summary
        .adoptions
        .last()
        .unwrap_or_else(|| unreachable!("the generator recorded the adoption"));
    let mut artifacts = vec![program, receipt_file];
    artifacts.extend(files.iter().map(|(name, _)| name.clone()));
    artifacts.push(RULES.to_owned());
    match format {
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "adopted", "path": dir.display().to_string(),
            "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
            "adoption": {
                "ordinal": plan.ordinal, "version": summary.version,
                "directory": plan.directory,
                "candidate_sha256": adoption.candidate_sha256,
                "receipt_sha256": adoption.receipt_sha256,
                "usage": adoption.usage.name(), "usage_preserved": adoption.usage_preserved,
                "program_nodes": {"before": adoption.program_nodes[0], "after": adoption.program_nodes[1]}
            },
            "artifacts": artifacts,
            "summary": summary_json(&plan.generated)
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
            artifacts.join(", ")
        ),
    }
    OK
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
