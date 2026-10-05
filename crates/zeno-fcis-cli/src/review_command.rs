//! `zeno-fcis contract review`: the command over the pure
//! `crate::contract::review`. This module only reads the application's
//! files, writes the packet where asked and prints a summary. A review grants
//! no authority and changes no application file.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::json;

use crate::contract::review::{self, DEFAULT_MAX_TUPLES, PACKET_SCHEMA, Review, ReviewSources};
use crate::contract_files::read_input;
use crate::{FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, USAGE, atomic_replace, print_json};

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";
const EXAMPLES: &str = "tests/decision-examples.txt";

#[derive(Subcommand)]
pub(super) enum Command {
    /// Review a contract: its decisions over the input domain or a boundary set, their agreement with the decision examples, and the rule mutants those inputs distinguish. Advisory; the application is not changed.
    Review {
        /// Application directory holding project.zeno, v2/policy.json and, optionally, tests/decision-examples.txt.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Write the review packet to this file. Without it the packet goes to stdout and the summary to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Largest input set to enumerate or probe. A domain of at most this many tuples is enumerated in full.
        #[arg(long, default_value_t = DEFAULT_MAX_TUPLES)]
        max_tuples: u64,
        /// Summary format, used with --out.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Review {
            dir,
            out,
            max_tuples,
            format,
        } => review_directory(&dir, out.as_deref(), max_tuples, format),
    }
}

/// Why no review was made.
enum Failure {
    /// An input could not be read: exit `FAILURE`.
    Read(String),
    /// The contract or an example is invalid: exit `INVALID`.
    Invalid { place: String, reason: String },
}

fn review_directory(dir: &Path, out: Option<&Path>, max_tuples: u64, format: OutputFormat) -> u8 {
    if max_tuples == 0 {
        eprintln!("--max-tuples must be at least 1");
        return USAGE;
    }
    let result = (|| {
        let project = read_text(dir, PROJECT)?;
        let rules = read_text(dir, RULES)?;
        let examples = match read_input(&dir.join(EXAMPLES)) {
            Ok(bytes) => Some(text(EXAMPLES, bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(format!("read {EXAMPLES}: {error}"))),
        };
        review::review(
            ReviewSources {
                project: &project,
                rules: &rules,
                examples: examples.as_deref(),
            },
            max_tuples,
        )
        .map_err(|error| Failure::Invalid {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        })
    })();
    let review = match result {
        Ok(review) => review,
        Err(failure) => return report_failure(dir, failure, format),
    };
    let summary = review.summary();
    let status = if summary.disagreements == 0 {
        "reviewed"
    } else {
        "disagreement"
    };
    match out {
        Some(path) => {
            let written = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| atomic_replace(path, review.packet().as_bytes()));
            if let Err(error) = written {
                let message = format!("write {}: {error}", path.display());
                return report_failure(dir, Failure::Read(message), format);
            }
            match format {
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
                    "authority": "none", "evidence": "advisory-review",
                    "packet": path.display().to_string(), "packet_schema": PACKET_SCHEMA,
                    "summary": summary.json(),
                })),
                OutputFormat::Human => {
                    for line in lines(&review, status) {
                        println!("{line}");
                    }
                    println!("packet: {}", path.display());
                }
            }
        }
        None => {
            print!("{}", review.packet());
            for line in lines(&review, status) {
                eprintln!("{line}");
            }
        }
    }
    if summary.disagreements == 0 {
        OK
    } else {
        INVALID
    }
}

/// The human summary.
fn lines(review: &Review, status: &str) -> Vec<String> {
    let summary = review.summary();
    vec![
        format!(
            "{status} {}: {} inputs ({}), {} examples compared, {} disagreements",
            summary.application,
            summary.inputs,
            summary.construction,
            summary.examples,
            summary.disagreements
        ),
        format!(
            "mutants {}: distinguished {}, refused by generator {}, refused by library {}, equivalent over the full domain {}, not distinguished within the boundary set {}",
            summary.mutants,
            summary.distinguished,
            summary.refused_by_generator,
            summary.refused_by_library,
            summary.equivalent,
            summary.undistinguished
        ),
        format!("findings {}", summary.findings),
    ]
}

fn report_failure(dir: &Path, failure: Failure, format: OutputFormat) -> u8 {
    let (code, place, message, exit) = match failure {
        Failure::Read(message) => ("review-read-failed", None, message, FAILURE),
        Failure::Invalid { place, reason } => ("contract-invalid", Some(place), reason, INVALID),
    };
    match format {
        OutputFormat::Human => match &place {
            Some(place) => eprintln!("{place}: {message}"),
            None => eprintln!("{message}"),
        },
        OutputFormat::Json => print_json(&json!({
            "schema": JSON_SCHEMA, "status": "error", "path": dir.display().to_string(),
            "authority": "none", "error": {"code": code, "place": place, "message": message}
        })),
    }
    exit
}

fn read_text(dir: &Path, name: &str) -> Result<String, Failure> {
    let bytes = read_input(&dir.join(name))
        .map_err(|error| Failure::Read(format!("read {name}: {error}")))?;
    text(name, bytes)
}

fn text(name: &str, bytes: Vec<u8>) -> Result<String, Failure> {
    String::from_utf8(bytes).map_err(|error| Failure::Read(format!("read {name}: {error}")))
}
