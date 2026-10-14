//! `zeno-fcis contract review`: the command over the pure
//! `crate::contract::review`. This module only reads the application's
//! files, writes the packet where asked and prints a summary. A review grants
//! no authority and changes no application file.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::{Value, json};

use crate::contract::review::{self, DEFAULT_MAX_TUPLES, PACKET_SCHEMA, Review, ReviewSources};
use crate::contract_files::read_input;
use crate::transform::sha256_hex;
use crate::{
    FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, USAGE, atomic_create, atomic_replace,
    print_json,
};

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
        /// Write the decision program of every mutant found equivalent over the full domain into
        /// this new directory, one canonical `<id>.zcve` each, as seed proposals for `optimize
        /// --with-candidate` and `loop run --seed`; each is marked with the transform checker's verdict.
        #[arg(long)]
        mutant_programs: Option<PathBuf>,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Review {
            dir,
            out,
            max_tuples,
            format,
            mutant_programs,
        } => review_directory(
            &dir,
            out.as_deref(),
            max_tuples,
            format,
            mutant_programs.as_deref(),
        ),
    }
}

/// Why no review was made.
enum Failure {
    /// An input could not be read: exit `FAILURE`.
    Read(String),
    /// The contract or an example is invalid: exit `INVALID`.
    Invalid { place: String, reason: String },
}

fn review_directory(
    dir: &Path,
    out: Option<&Path>,
    max_tuples: u64,
    format: OutputFormat,
    mutant_programs: Option<&Path>,
) -> u8 {
    if max_tuples == 0 {
        eprintln!("--max-tuples must be at least 1");
        return USAGE;
    }
    if let Some(path) = mutant_programs
        && path.symlink_metadata().is_ok()
    {
        let message = format!("{} exists; seeds go to a new directory", path.display());
        return report_failure(dir, Failure::Read(message), format);
    }
    let result = (|| {
        let project = read_text(dir, PROJECT)?;
        let rules = read_text(dir, RULES)?;
        let examples = match read_input(&dir.join(EXAMPLES)) {
            Ok(bytes) => Some(text(EXAMPLES, bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(format!("read {EXAMPLES}: {error}"))),
        };
        let sources = ReviewSources {
            project: &project,
            rules: &rules,
            examples: examples.as_deref(),
        };
        let invalid = |error: crate::contract::ContractError| Failure::Invalid {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        };
        let review = review::review(sources, max_tuples).map_err(invalid)?;
        let seeds = match mutant_programs {
            Some(path) => Some(write_seeds(
                path,
                review::seeds::equivalent_mutant_programs(sources, review.packet(), max_tuples)
                    .map_err(invalid)?,
            )?),
            None => None,
        };
        Ok((review, seeds))
    })();
    let (review, seeds) = match result {
        Ok(reviewed) => reviewed,
        Err(failure) => return report_failure(dir, failure, format),
    };
    let summary = review.summary();
    let status = if summary.disagreements > 0 {
        "disagreement"
    } else if summary.law_refusals > 0 {
        "law-refusal"
    } else {
        "reviewed"
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
                OutputFormat::Json => {
                    let mut report = json!({
                        "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
                        "authority": "none", "evidence": "advisory-review",
                        "packet": path.display().to_string(), "packet_schema": PACKET_SCHEMA,
                        "summary": summary.json(),
                    });
                    if let Some(seeds) = &seeds {
                        report["mutant_programs"] = seeds.clone();
                    }
                    print_json(&report);
                }
                OutputFormat::Human => {
                    for line in lines(&review, status) {
                        println!("{line}");
                    }
                    println!("packet: {}", path.display());
                    for line in seed_lines(seeds.as_ref()) {
                        println!("{line}");
                    }
                }
            }
        }
        None => {
            print!("{}", review.packet());
            for line in lines(&review, status)
                .into_iter()
                .chain(seed_lines(seeds.as_ref()))
            {
                eprintln!("{line}");
            }
        }
    }
    if summary.findings == 0 { OK } else { INVALID }
}

/// Writes each seed as `<id>.zcve` into the new directory `path` and
/// describes them; the directory is created whole or the command fails.
fn write_seeds(path: &Path, seeds: Vec<review::seeds::Seed>) -> Result<Value, Failure> {
    fs::create_dir_all(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .and_then(|()| fs::create_dir(path))
    .map_err(|error| Failure::Read(format!("create {}: {error}", path.display())))?;
    let mut programs = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let file = path.join(format!("{}.zcve", seed.id));
        atomic_create(&file, &seed.program)
            .map_err(|error| Failure::Read(format!("write {}: {error}", file.display())))?;
        programs.push(json!({
            "id": seed.id, "path": file.display().to_string(), "bytes": seed.program.len(),
            "sha256": sha256_hex(&seed.program), "program_equivalent": seed.program_equivalent,
        }));
    }
    Ok(json!({
        "directory": path.display().to_string(),
        "kind": "equivalent-over-full-domain mutants, as seed proposals; never evidence",
        "programs": programs,
    }))
}

/// One line per written seed.
fn seed_lines(seeds: Option<&Value>) -> Vec<String> {
    seeds
        .and_then(|seeds| seeds["programs"].as_array())
        .into_iter()
        .flatten()
        .map(|seed| {
            format!(
                "seed: {} ({}program-equivalent)",
                seed["path"].as_str().unwrap_or_default(),
                if seed["program_equivalent"] == true {
                    ""
                } else {
                    "not "
                }
            )
        })
        .collect()
}

/// The human summary.
fn lines(review: &Review, status: &str) -> Vec<String> {
    let summary = review.summary();
    let mut lines = vec![
        format!(
            "{status} {}: {} inputs ({}), {} examples compared, {} disagreements",
            summary.application,
            summary.inputs,
            summary.construction,
            summary.examples,
            summary.disagreements
        ),
        summary.refusals_line(),
        format!(
            "mutants {}: distinguished {}, refused by generator {}, refused by library {}, equivalent over the full domain {}, not distinguished within the boundary set {}",
            summary.mutants,
            summary.distinguished,
            summary.refused_by_generator,
            summary.refused_by_library,
            summary.equivalent,
            summary.undistinguished
        ),
    ];
    lines.extend(review.law_refusal_lines());
    lines.push(format!("findings {}", summary.findings));
    lines
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
