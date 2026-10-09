//! `zeno-fcis contract diff`: the command over the pure
//! `crate::contract::diff`. It reads two contract directories, prints the
//! kind of change from the first to the second and the account of every
//! changed item, and writes nothing.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::json;

use crate::contract::diff::{self, Refused, Side};
use crate::contract_files::{Failure, Inputs};
use crate::{FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, print_json};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Classify the change from one contract to another as identical, program-successor, rename, layout-change, rule-change or unrelated, name every changed item and the admission path the kind needs. Writes nothing.
    Diff {
        /// The old contract: an application or contract directory holding project.zeno, v2/policy.json and any adoptions.
        old: PathBuf,
        /// The new contract, in the same form.
        new: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Diff { old, new, format } => compare(&old, &new, format),
    }
}

/// Reads both contracts, then classifies the change through the pure
/// `contract::diff`. Exit 0 whatever the kind; 1 for an invalid contract and
/// 3 for an unreadable file, naming the side.
fn compare(old: &Path, new: &Path, format: OutputFormat) -> u8 {
    let old_inputs = match Inputs::read(old) {
        Ok(inputs) => inputs,
        Err(failure) => return report_failure(Side::Old, old, failure, format),
    };
    let new_inputs = match Inputs::read(new) {
        Ok(inputs) => inputs,
        Err(failure) => return report_failure(Side::New, new, failure, format),
    };
    let (old_adoptions, new_adoptions) =
        (old_inputs.adoption_sources(), new_inputs.adoption_sources());
    let (old_evolutions, new_evolutions) = (
        old_inputs.evolution_sources(),
        new_inputs.evolution_sources(),
    );
    let classified = diff::diff(
        old_inputs.sources(&old_inputs.rules, &old_adoptions, &old_evolutions),
        new_inputs.sources(&new_inputs.rules, &new_adoptions, &new_evolutions),
    );
    match classified {
        Ok(diff) => {
            match format {
                OutputFormat::Json => print_json(&diff.json()),
                OutputFormat::Human => {
                    for line in diff.lines() {
                        println!("{line}");
                    }
                }
            }
            OK
        }
        Err(Refused { side, error }) => {
            let dir = match side {
                Side::Old => old,
                Side::New => new,
            };
            report_failure(side, dir, Failure::from(error), format)
        }
    }
}

fn report_failure(side: Side, dir: &Path, failure: Failure, format: OutputFormat) -> u8 {
    let (code, place, message, exit) = match failure {
        Failure::Read(message) => ("contract-read-failed", None, message, FAILURE),
        Failure::Invalid { place, reason } => ("contract-invalid", Some(place), reason, INVALID),
        Failure::Binding(refusal) => ("contract-invalid", None, format!("{refusal:?}"), INVALID),
    };
    match format {
        OutputFormat::Human => match &place {
            Some(place) => eprintln!("{} contract: {place}: {message}", side.name()),
            None => eprintln!("{} contract: {message}", side.name()),
        },
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "error", "side": side.name(),
            "path": dir.display().to_string(), "authority": "none",
            "error": {"code": code, "place": place, "message": message}
        })),
    }
    exit
}
