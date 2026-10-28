//! File/CLI shell for the 2.7 advisory postcondition prototype.

use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::json;

use crate::contract::review::postconditions as core;
use crate::{BLOCKED, INVALID, OK, print_json};

#[derive(Args)]
pub(crate) struct Inputs {
    /// Scalar application with project.zeno and v2/policy.json.
    dir: PathBuf,
    /// Written requirements; the exact bytes bind both prompt and check.
    #[arg(long)]
    intent: PathBuf,
    /// Complete domain cap, 1..=4096; must match the prompt.
    #[arg(long, default_value_t = core::MAX_TUPLES)]
    max_tuples: u64,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Emit a source-bound request for your AI agent as JSON on stdout. No model/network call.
    Prompt {
        #[command(flatten)]
        inputs: Inputs,
    },
    /// Check agent proposals on the frozen contract and optional mutant policies. No writes.
    /// Exits: 0 every proposal is checked-discriminating; 1 refused/refuted; 2 inconclusive, trivial or no discrimination.
    Check {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        proposals: PathBuf,
        /// Faulty policy JSON; repeat up to 8 times. Only cases/variables may differ.
        #[arg(long = "mutant")]
        mutants: Vec<PathBuf>,
    },
}

fn read(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("input is not a regular file".into());
    }
    file.take(core::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > core::MAX_BYTES {
        return Err("input exceeds 65536 bytes".into());
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

pub(crate) fn run(command: Command) -> u8 {
    let result = (|| {
        let inputs = match &command {
            Command::Prompt { inputs } | Command::Check { inputs, .. } => inputs,
        };
        let project = read(&inputs.dir.join("project.zeno"))?;
        let policy = read(&inputs.dir.join("v2/policy.json"))?;
        let intent = read(&inputs.intent)?;
        match command {
            Command::Prompt { inputs } => {
                core::request(&project, &policy, &intent, inputs.max_tuples)
                    .map(|v| (v, OK))
                    .map_err(|e| e.to_string())
            }
            Command::Check {
                inputs,
                proposals,
                mutants,
            } => {
                if mutants.len() > core::MAX_MUTANTS {
                    return Err("at most 8 mutants".into());
                }
                let proposals = read(&proposals)?;
                let mutants: Vec<String> =
                    mutants.iter().map(|p| read(p)).collect::<Result<_, _>>()?;
                let report = core::check(
                    &project,
                    &policy,
                    &intent,
                    &proposals,
                    &mutants,
                    inputs.max_tuples,
                )
                .map_err(|e| e.to_string())?;
                let rows = report["candidates"]
                    .as_array()
                    .ok_or("missing candidate report")?;
                let exit = if rows
                    .iter()
                    .any(|r| matches!(r["status"].as_str(), Some("refused" | "refuted")))
                {
                    INVALID
                } else if rows.iter().all(|r| r["status"] == "checked-discriminating") {
                    OK
                } else {
                    BLOCKED
                };
                Ok((report, exit))
            }
        }
    })();
    match result {
        Ok((report, exit)) => {
            print_json(&report);
            exit
        }
        Err(reason) => {
            print_json(
                &json!({"schema":"zeno-fcis/postcondition-report/1","authority":"none","status":"refused","reason":reason}),
            );
            INVALID
        }
    }
}
