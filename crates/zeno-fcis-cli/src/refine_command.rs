//! File-reading shell for the pure representation refinement checker.
// SPDX-License-Identifier: MIT OR Apache-2.0
// LLM-generated implementation; checked by the repository's qualification gates.

use std::path::PathBuf;

use clap::Subcommand;
use serde_json::json;
use zeno_fcis_contract::refinement::{self, Sources, Verdict};

use crate::contract_files::read_input;
use crate::{BLOCKED, FAILURE, INVALID, JSON_SCHEMA, MALFORMED, OK, OutputFormat, print_json};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Exhaustively compare concrete and abstract contracts through an abstraction table. Advisory; no application file changes. Exit 0 refines, 1 counterexample, 2 inconclusive, 65 malformed.
    RefineCheck {
        /// Directory of the abstract project.zeno and v2/policy.json.
        abstract_dir: PathBuf,
        /// Directory of the concrete project.zeno and v2/policy.json.
        concrete_dir: PathBuf,
        /// Source-pinned abstraction JSON file.
        #[arg(long)]
        abstraction: PathBuf,
        /// Maximum completed raw state/input comparisons; an exhausted budget is inconclusive.
        #[arg(long, default_value_t = refinement::MAX_CHECKS)]
        max_checks: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    let Command::RefineCheck {
        abstract_dir,
        concrete_dir,
        abstraction,
        max_checks,
        format,
    } = command;
    let paths = [
        abstract_dir.join("project.zeno"),
        abstract_dir.join("v2/policy.json"),
        concrete_dir.join("project.zeno"),
        concrete_dir.join("v2/policy.json"),
        abstraction,
    ];
    let text: Result<Vec<String>, String> = paths
        .iter()
        .map(|path| {
            let bytes = read_input(path).map_err(|e| format!("read {}: {e}", path.display()))?;
            String::from_utf8(bytes).map_err(|e| format!("read {}: {e}", path.display()))
        })
        .collect();
    let text = match text {
        Ok(text) => text,
        Err(error) => return failed("read-failed", &error, FAILURE, format),
    };
    let sources = Sources {
        abstract_project: &text[0],
        abstract_policy: &text[1],
        concrete_project: &text[2],
        concrete_policy: &text[3],
        abstraction: &text[4],
    };
    match refinement::check(sources, max_checks) {
        Ok(report) => {
            match format {
                OutputFormat::Json => print_json(&report.json()),
                OutputFormat::Human => {
                    println!(
                        "{}: {} raw state/input pairs checked",
                        report.verdict().name(),
                        report.checked_rows()
                    );
                    let value = report.json();
                    if !value["witness"].is_null() {
                        println!("counterexample: {}", value["witness"]);
                    }
                    if let Some(reason) = value["reason"].as_str() {
                        println!("reason: {reason}");
                    }
                    println!("Finite contract refinement only; authority: none.");
                }
            }
            match report.verdict() {
                Verdict::Refines => OK,
                Verdict::Counterexample => INVALID,
                Verdict::Inconclusive => BLOCKED,
            }
        }
        Err(error) => failed("malformed", &error.to_string(), MALFORMED, format),
    }
}

fn failed(status: &str, message: &str, code: u8, format: OutputFormat) -> u8 {
    match format {
        OutputFormat::Human => eprintln!("{status}: {message}"),
        OutputFormat::Json => print_json(
            &json!({ "schema": JSON_SCHEMA, "status": status, "authority": "none", "error": message }),
        ),
    }
    code
}
