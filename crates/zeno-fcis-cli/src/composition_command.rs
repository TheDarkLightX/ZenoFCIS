//! `zeno-fcis contract check-composition`: the command over the library's
//! `zeno_fcis_contract::checked_composition`. This module only reads the
//! manifest, each leaf's files, an aggregate contract to validate and a
//! receipt to replay, writes the receipt and the generated aggregate where
//! asked, and prints the result. A checked composition grants no runtime
//! authority.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::{Value, json};

use crate::contract_files::read_input;
use crate::{FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, atomic_replace, print_json};
use zeno_fcis_contract::checked_composition::{
    self, AggregateSources, CheckedComposition, CompositionSources, LeafSources, RECEIPT_SCHEMA,
    RESULT_SCHEMA, Refusal, ReplayRefusal, leaf_paths,
};

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";

#[derive(Subcommand)]
pub(super) enum Command {
    /// Check a checked-composition manifest: every leaf's bound Authority on its complete local domain, every interface implication, each group's outer assumption and guarantee and, with an aggregate, its assembly. Grants no runtime authority.
    CheckComposition {
        /// The manifest, `zeno-fcis/checked-composition/1` or `/2` with groups; leaf paths are relative to its directory.
        manifest: PathBuf,
        /// Validate this aggregate contract directory (`project.zeno` and `v2/policy.json`) against the composition.
        #[arg(long, conflicts_with = "write_aggregate")]
        aggregate: Option<PathBuf>,
        /// Generate the composition's aggregate contract into this directory, then validate it.
        #[arg(long, conflicts_with = "replay")]
        write_aggregate: Option<PathBuf>,
        /// Write the result's canonical receipt to this file.
        #[arg(long, conflicts_with = "replay")]
        receipt: Option<PathBuf>,
        /// Run every check again and require this receipt, byte for byte.
        #[arg(long)]
        replay: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::CheckComposition {
            manifest,
            aggregate,
            write_aggregate,
            receipt,
            replay,
            format,
        } => {
            let read = read_sources(&manifest).and_then(|(text, leaves)| {
                let aggregate = aggregate.as_deref().map(read_aggregate).transpose()?;
                Ok((text, leaves, aggregate))
            });
            let (text, leaves, aggregate) = match read {
                Ok(read) => read,
                Err(message) => return read_failed(&manifest, &message, format),
            };
            let sources = CompositionSources {
                manifest: &text,
                leaves: &leaves,
                aggregate: aggregate.as_ref(),
            };
            if let Some(recorded) = replay {
                return replay_receipt(&manifest, &recorded, sources, format);
            }
            // The aggregate is generated and validated in the same check,
            // and written only after it passes, never over a leaf.
            let outcome = match &write_aggregate {
                Some(directory) => {
                    if let Some(leaf) = holds_a_leaf(&manifest, &text, directory) {
                        return directory_refused(&manifest, directory, &leaf, format);
                    }
                    checked_composition::check_generated(sources)
                        .map(|(checked, generated)| (checked, Some(generated)))
                }
                None => checked_composition::check(sources).map(|checked| (checked, None)),
            };
            check(
                &manifest,
                outcome,
                receipt.as_deref(),
                write_aggregate.as_deref(),
                format,
            )
        }
    }
}

/// The manifest and every leaf's files it names.
fn read_sources(path: &Path) -> Result<(String, Vec<LeafSources>), String> {
    let manifest = read_text(path)?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let Ok(paths) = leaf_paths(&manifest) else {
        // The check refuses the manifest itself.
        return Ok((manifest, Vec::new()));
    };
    let leaves = paths
        .iter()
        .map(|leaf| {
            Ok(LeafSources {
                project: read_text(&directory.join(leaf).join(PROJECT))?,
                rules: read_text(&directory.join(leaf).join(RULES))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((manifest, leaves))
}

fn read_aggregate(directory: &Path) -> Result<AggregateSources, String> {
    Ok(AggregateSources {
        project: read_text(&directory.join(PROJECT))?,
        rules: read_text(&directory.join(RULES))?,
    })
}

fn write_aggregate_files(directory: &Path, aggregate: &AggregateSources) -> Result<(), String> {
    let write = |name: &str, bytes: &str| {
        let path = directory.join(name);
        path.parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| atomic_replace(&path, bytes.as_bytes()))
            .map_err(|error| format!("write {}: {error}", path.display()))
    };
    write(PROJECT, &aggregate.project)?;
    write(RULES, &aggregate.rules)
}

/// The leaf directory `directory` is or contains, if any; paths are
/// compared after resolving what exists of them.
fn holds_a_leaf(manifest: &Path, text: &str, directory: &Path) -> Option<String> {
    let base = manifest.parent().unwrap_or_else(|| Path::new("."));
    let target = resolved(directory);
    leaf_paths(text)
        .ok()?
        .into_iter()
        .find(|leaf| resolved(&base.join(leaf)).starts_with(&target))
}

/// A path made absolute, with its longest existing prefix canonicalized.
fn resolved(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = absolute.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => break,
        }
    }
    let mut out = fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for name in rest.iter().rev() {
        out.push(name);
    }
    out
}

fn directory_refused(path: &Path, directory: &Path, leaf: &str, format: OutputFormat) -> u8 {
    let message = format!(
        "{} is or contains the leaf directory {leaf}; the aggregate is not written over a leaf",
        directory.display()
    );
    match format {
        OutputFormat::Human => eprintln!("{message}"),
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "error", "path": path.display().to_string(),
            "authority": "none",
            "error": {"code": "aggregate-directory-refused", "message": message},
        })),
    }
    FAILURE
}

fn read_failed(path: &Path, message: &str, format: OutputFormat) -> u8 {
    match format {
        OutputFormat::Human => eprintln!("{message}"),
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "error", "path": path.display().to_string(),
            "authority": "none",
            "error": {"code": "composition-read-failed", "message": message},
        })),
    }
    FAILURE
}

fn refused(path: &Path, refusal: &Value, text: &str, format: OutputFormat) -> u8 {
    match format {
        OutputFormat::Human => eprintln!("refused: {text}"),
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "refused", "path": path.display().to_string(),
            "authority": "none", "refusal": refusal,
        })),
    }
    INVALID
}

fn check(
    path: &Path,
    outcome: Result<(CheckedComposition, Option<AggregateSources>), Refusal>,
    receipt: Option<&Path>,
    written: Option<&Path>,
    format: OutputFormat,
) -> u8 {
    match outcome {
        Ok((checked, generated)) => {
            if let (Some(directory), Some(generated)) = (written, &generated)
                && let Err(message) = write_aggregate_files(directory, generated)
            {
                return read_failed(path, &message, format);
            }
            if let Some(out) = receipt {
                let result = out
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| atomic_replace(out, checked.receipt().as_bytes()));
                if let Err(error) = result {
                    return read_failed(path, &format!("write {}: {error}", out.display()), format);
                }
            }
            match format {
                OutputFormat::Human => {
                    for line in checked.lines() {
                        println!("{line}");
                    }
                    if let Some(directory) = written {
                        println!("aggregate: {}", directory.display());
                    }
                    if let Some(out) = receipt {
                        println!("receipt: {}", out.display());
                    }
                }
                OutputFormat::Json => {
                    let mut report = json!({
                        "schema": JSON_SCHEMA, "status": "checked", "path": path.display().to_string(),
                        "authority": "none", "evidence": "checked-composition",
                        "result_schema": RESULT_SCHEMA, "result": checked.json(),
                    });
                    if let Some(directory) = written {
                        report["aggregate"] = json!(directory.display().to_string());
                    }
                    if let Some(out) = receipt {
                        report["receipt"] = json!(out.display().to_string());
                        report["receipt_schema"] = json!(RECEIPT_SCHEMA);
                    }
                    print_json(&report);
                }
            }
            OK
        }
        Err(refusal) => refused(path, &refusal.json(), &refusal.to_string(), format),
    }
}

fn replay_receipt(
    path: &Path,
    recorded: &Path,
    sources: CompositionSources<'_>,
    format: OutputFormat,
) -> u8 {
    let receipt = match read_text(recorded) {
        Ok(receipt) => receipt,
        Err(message) => return read_failed(path, &message, format),
    };
    match checked_composition::replay(&receipt, sources) {
        Ok(checked) => {
            match format {
                OutputFormat::Human => {
                    for line in checked.lines() {
                        println!("{line}");
                    }
                    println!("replayed: {}", recorded.display());
                }
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": "replayed", "path": path.display().to_string(),
                    "authority": "none", "evidence": "checked-composition",
                    "receipt": recorded.display().to_string(), "receipt_schema": RECEIPT_SCHEMA,
                    "result_schema": RESULT_SCHEMA, "result": checked.json(),
                })),
            }
            OK
        }
        Err(refusal) => {
            let status = match refusal {
                ReplayRefusal::Check(_) => "refused",
                _ => "replay-mismatch",
            };
            match format {
                OutputFormat::Human => eprintln!("{status}: {refusal}"),
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": status, "path": path.display().to_string(),
                    "authority": "none", "receipt": recorded.display().to_string(),
                    "replay": refusal.json(),
                })),
            }
            INVALID
        }
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = read_input(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    String::from_utf8(bytes).map_err(|error| format!("read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;
    use zeno_fcis_contract::checked_composition::{self, CompositionSources, LeafSources};

    /// Until one canonical JSON writer serves both, a checked-composition
    /// receipt is written byte for byte as a transform receipt would be.
    #[test]
    fn the_receipt_is_written_as_transform_receipts_are() {
        let ring = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2");
        let read = |path: PathBuf| {
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        };
        let leaves: Vec<LeafSources> = ["register-0", "register-1"]
            .iter()
            .map(|leaf| LeafSources {
                project: read(ring.join(leaf).join(super::PROJECT)),
                rules: read(ring.join(leaf).join(super::RULES)),
            })
            .collect();
        let committed = read(ring.join("composition.json"));
        // A name with quotes, a backslash, non-ASCII text and a control
        // character, so that both writers escape the same strings.
        let mut renamed: Value =
            serde_json::from_str(&committed).unwrap_or_else(|error| panic!("{error}"));
        renamed["name"] = Value::from("ring \"two\" \\ \u{e9} \u{2713} \u{1}");
        let renamed =
            serde_json::to_string_pretty(&renamed).unwrap_or_else(|error| panic!("{error}"));
        for manifest in [committed, renamed] {
            let checked = checked_composition::check(CompositionSources {
                manifest: &manifest,
                leaves: &leaves,
                aggregate: None,
            })
            .unwrap_or_else(|refusal| panic!("{refusal}"));
            let receipt = checked.receipt();
            let parsed: Value =
                serde_json::from_str(&receipt).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(crate::transform::canonical_json(&parsed), receipt);
            assert!(!receipt.contains('\u{1}'), "a control character is escaped");
        }
    }
}
