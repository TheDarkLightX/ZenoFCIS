//! `zeno-fcis contract review`: the command over the pure
//! `crate::contract::review`. This module only reads the application's
//! files, writes the packet where asked and prints a summary. A review grants
//! no authority and changes no application file.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::{Value, json};

use crate::contract::review::{
    self, DEFAULT_MAX_TUPLES, PACKET_SCHEMA, PACKET_SCHEMA_GAPS, Review, ReviewSources, gaps,
};
use crate::contract_files::read_input;
use crate::transform::sha256_hex;
use crate::{
    BLOCKED, FAILURE, INVALID, JSON_SCHEMA, MALFORMED, OK, OutputFormat, USAGE, atomic_create,
    atomic_replace, print_json,
};

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";
const EXAMPLES: &str = "tests/decision-examples.txt";

#[derive(Subcommand)]
pub(super) enum Command {
    /// Review decisions, owner examples and rule mutants. Small domains remain exhaustive; above the cap, full-domain law checking needs both pinned solvers. A concrete native example can refuse first. Advisory; the application is not changed.
    Review {
        /// Application directory holding project.zeno, v2/policy.json and, optionally, tests/decision-examples.txt.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Write the review packet to this file. Without it the packet goes to stdout and the summary to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Largest domain to enumerate in full. Larger scalar domains automatically use symbolic law checking with ZENO_FCIS_Z3 and ZENO_FCIS_CVC5.
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
        /// Also run the specification-gap judge over the whole declared domain and use the extended mutant catalog: packet version 3. A domain over --max-tuples, or law checks over --max-work, is inconclusive (exit 2).
        #[arg(long)]
        gaps: bool,
        /// With --gaps: a file of proposed laws, one `project.zeno` law declaration per line, checked against the contract's decisions and then measured.
        #[arg(long, requires = "gaps")]
        gaps_laws: Option<PathBuf>,
        /// With --gaps: the largest number of law checks, tuples times candidate decisions times declared laws.
        #[arg(long, requires = "gaps", default_value_t = gaps::DEFAULT_MAX_WORK)]
        max_work: u64,
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
            gaps,
            gaps_laws,
            max_work,
        } => {
            let gaps = gaps.then_some(Gaps {
                laws: gaps_laws,
                max_work,
            });
            review_directory(
                &dir,
                out.as_deref(),
                max_tuples,
                format,
                mutant_programs.as_deref(),
                gaps.as_ref(),
            )
        }
    }
}

/// Why no review was made.
enum Failure {
    /// An input could not be read: exit `FAILURE`.
    Read(String),
    /// The contract or an example is invalid: exit `INVALID`.
    Invalid { place: String, reason: String },
}

/// The specification-gap judge's settings.
struct Gaps {
    laws: Option<PathBuf>,
    max_work: u64,
}

fn review_directory(
    dir: &Path,
    out: Option<&Path>,
    max_tuples: u64,
    format: OutputFormat,
    mutant_programs: Option<&Path>,
    gap_settings: Option<&Gaps>,
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
    if let Ok(rules) = read_text(dir, RULES)
        && zeno_fcis_contract::internals::tables::recognizes(&rules).unwrap_or(false)
    {
        if mutant_programs.is_some()
            || gap_settings.is_some_and(|g| g.laws.is_some())
            || dir.join(EXAMPLES).exists()
        {
            return report_failure(dir, Failure::Invalid {place: "tables/1".into(), reason: "scalar examples, mutant programs and scalar proposed laws do not support table subjects".into()}, format);
        }
        let result = (|| {
            let project = read_text(dir, PROJECT)?;
            let compiled = zeno_fcis_contract::internals::tables::compile(&project, &rules)
                .map_err(|e| Failure::Invalid {
                    place: e.place().into(),
                    reason: e.reason().into(),
                })?;
            let report = zeno_fcis_contract::internals::tables_review::review(
                &compiled,
                max_tuples,
                gap_settings.map_or(0, |g| g.max_work),
                gap_settings.is_some(),
            )
            .map_err(|e| Failure::Invalid {
                place: e.place().into(),
                reason: e.reason().into(),
            })?;
            if let Some(path) = out {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    fs::create_dir_all(parent).map_err(|e| Failure::Read(e.to_string()))?;
                }
                atomic_replace(
                    path,
                    serde_json::to_string_pretty(&report)
                        .map_err(|e| Failure::Read(e.to_string()))?
                        .as_bytes(),
                )
                .map_err(|e| Failure::Read(e.to_string()))?;
            }
            Ok(report)
        })();
        return match result {
            Ok(report) => {
                let code = match report["status"].as_str() {
                    Some("finite-checked") => OK,
                    Some("inconclusive") => BLOCKED,
                    _ => INVALID,
                };
                print_json(&report);
                code
            }
            Err(failure) => report_failure(dir, failure, format),
        };
    }
    // Preserve the existing exhaustive route at and below its exact cap.
    // Solver acquisition belongs here in the file/process shell, never in
    // the pure review implementation.
    if let (Ok(project), Ok(rules)) = (read_text(dir, PROJECT), read_text(dir, RULES)) {
        match review::numeric_symbolic::over_cap(&project, &rules, max_tuples) {
            Ok(true) => {
                return symbolic_directory(
                    dir,
                    out,
                    format,
                    &project,
                    &rules,
                    mutant_programs.is_some() || gap_settings.is_some(),
                );
            }
            Ok(false) => {}
            Err(error) => {
                return report_failure(
                    dir,
                    Failure::Invalid {
                        place: error.place().into(),
                        reason: error.reason().into(),
                    },
                    format,
                );
            }
        }
    }
    let result = (|| {
        let project = read_text(dir, PROJECT)?;
        let rules = read_text(dir, RULES)?;
        let examples = match read_input(&dir.join(EXAMPLES)) {
            Ok(bytes) => Some(text(EXAMPLES, bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(format!("read {EXAMPLES}: {error}"))),
        };
        let invalid = |error: crate::contract::ContractError| Failure::Invalid {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        };
        // Admit the same immutable snapshot that the review receives.
        review::application(&project, &rules).map_err(invalid)?;
        let sources = ReviewSources {
            project: &project,
            rules: &rules,
            examples: examples.as_deref(),
        };
        let reviewed = match gap_settings {
            None => review::review(sources, max_tuples),
            Some(settings) => {
                let laws = match &settings.laws {
                    None => None,
                    Some(path) => {
                        let bytes = read_input(path).map_err(|error| {
                            Failure::Read(format!("read {}: {error}", path.display()))
                        })?;
                        Some(text(&path.display().to_string(), bytes)?)
                    }
                };
                review::review_with_gaps(
                    sources,
                    gaps::Options {
                        max_tuples,
                        max_work: settings.max_work,
                        proposals: laws.as_deref(),
                        mutants: true,
                    },
                )
            }
        };
        let review = reviewed.map_err(invalid)?;
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
    let gap_status = review.gaps().map(gaps::Gaps::status);
    let status = if summary.disagreements > 0 {
        "disagreement"
    } else if summary.law_refusals > 0 {
        "law-refusal"
    } else {
        review.gaps().map_or("reviewed", gaps::Gaps::review_status)
    };
    let packet_schema = if review.gaps().is_some() {
        PACKET_SCHEMA_GAPS
    } else {
        PACKET_SCHEMA
    };
    // Observe the completed measure before an optional packet write can fail.
    if std::env::var_os("ZENO_FCIS_TELEMETRY").is_some()
        && let Ok(packet) = serde_json::from_str(review.packet())
    {
        crate::telemetry::observe(&packet);
    }
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
                        "packet": path.display().to_string(), "packet_schema": packet_schema,
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
    if summary.findings > 0 {
        return INVALID;
    }
    match gap_status {
        Some("proposal-false-alarm") => INVALID,
        Some("inconclusive" | "out-of-scope") => BLOCKED,
        Some("withheld") => FAILURE,
        Some("invalid-proposals") => MALFORMED,
        _ => OK,
    }
}

fn symbolic_directory(
    dir: &Path,
    out: Option<&Path>,
    format: OutputFormat,
    project: &str,
    rules: &str,
    unsupported_options: bool,
) -> u8 {
    let result = (|| {
        let application =
            review::application(project, rules).map_err(|error| Failure::Invalid {
                place: error.place().to_owned(),
                reason: error.reason().to_owned(),
            })?;
        let examples = match read_input(&dir.join(EXAMPLES)) {
            Ok(b) => Some(text(EXAMPLES, b)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(Failure::Read(e.to_string())),
        };
        let mut report = if unsupported_options {
            json!({"schema":"zeno-fcis/numeric-law-review/1","authority":"none","mode":"symbolic","status":"UNKNOWN","reason":"symbolic gap counting and exhaustive mutant seed receipts are unsupported"})
        } else if let Some(report) = native_example_refutation(ReviewSources {
            project,
            rules,
            examples: examples.as_deref(),
        })? {
            report
        } else {
            match crate::symbolic_command::Solvers::from_environment() {
                Ok(solvers) => {
                    let mut report = review::numeric_symbolic::review(
                        ReviewSources {
                            project,
                            rules,
                            examples: examples.as_deref(),
                        },
                        &mut |q| solvers.solve(q),
                        true,
                    )
                    .map_err(|e| Failure::Invalid {
                        place: e.place().into(),
                        reason: e.reason().into(),
                    })?;
                    report["solver_identities"] = solvers.identities();
                    if let Some(reason) = solvers.retention_failure() {
                        report["status"] = json!("UNKNOWN");
                        report["evidence"] = json!("none");
                        report["reason"] = json!(reason);
                    }
                    report
                }
                Err(reason) => {
                    json!({"schema":"zeno-fcis/numeric-law-review/1","authority":"none","mode":"symbolic","status":"UNKNOWN","reason":reason})
                }
            }
        };
        report["application"] = json!(application);
        if let Some(path) = out {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent).map_err(|e| Failure::Read(e.to_string()))?;
            }
            atomic_replace(path, crate::transform::canonical_json(&report).as_bytes())
                .map_err(|e| Failure::Read(e.to_string()))?;
        }
        Ok(report)
    })();
    match result {
        Err(f) => report_failure(dir, f, format),
        Ok(report) => {
            let status = report["status"].as_str().unwrap_or("UNKNOWN");
            if out.is_none() || matches!(format, OutputFormat::Json) {
                print_json(&report)
            } else {
                println!(
                    "contract review: {status}; mode: {}",
                    report["mode"].as_str().unwrap_or("unknown")
                );
                if status == "solver-checked" {
                    println!("{}", review::numeric_symbolic::MODE);
                }
                if let Some(reason) = report["reason"].as_str() {
                    println!("{reason}");
                }
            }
            match status {
                "solver-checked" => OK,
                "refuted" | "disagreement" | "law-refusal" => INVALID,
                _ => BLOCKED,
            }
        }
    }
}

/// A concrete native example can refuse a review before global solver work.
/// Reuse the existing owner-example and mutant-witness checks with one probe;
/// its packet retains the boundary scope and issues no symbolic law verdict.
fn native_example_refutation(sources: ReviewSources<'_>) -> Result<Option<Value>, Failure> {
    if !sources.examples.is_some_and(|text| {
        text.lines()
            .any(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
    }) {
        return Ok(None);
    }
    let reviewed = review::review(sources, 1).map_err(|e| Failure::Invalid {
        place: e.place().into(),
        reason: e.reason().into(),
    })?;
    if reviewed.summary().findings == 0 {
        return Ok(None);
    }
    let mut packet: Value =
        serde_json::from_str(reviewed.packet()).map_err(|e| Failure::Read(e.to_string()))?;
    packet["mode"] = json!("native-examples");
    packet["status"] = json!(if reviewed.summary().disagreements > 0 {
        "disagreement"
    } else {
        "law-refusal"
    });
    packet["law_check"] =
        json!("symbolic checking not run: a concrete native finding already refuses the review");
    Ok(Some(packet))
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
    lines.push(format!(
        "original decisions: {} example disagreements, {} law refusals",
        summary.disagreements, summary.law_refusals
    ));
    if let Some(gaps) = review.gaps() {
        lines.push(gaps_line(gaps));
    }
    lines.push(
        "scenario coverage: run contract scenario to measure decision examples and mutant kills"
            .into(),
    );
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

fn measured_gaps_line(label: &str, summary: &Value) -> String {
    format!(
        "{label}: {} of {} tuples determined by the declared laws ({} of {} on valid pre-states); mutants admitted {}, caught {}; inactive laws {}; reject-everything core admitted where the contract decides: {}",
        summary["full"]["determined"],
        summary["full"]["tuples"],
        summary["valid"]["determined"],
        summary["valid"]["tuples"],
        summary["full"]["mutants_admitted"],
        summary["full"]["mutants_caught"],
        summary["inactive_laws"],
        summary["reject_everything_admitted"],
    )
}

/// Report both checked measures when a proposal was measured.
fn gaps_line(gaps: &gaps::Gaps) -> String {
    let summary = gaps.summary();
    match gaps {
        gaps::Gaps::Measured(_) => {
            let mut line = measured_gaps_line("gaps", &summary);
            if let Some(after) = summary.get("with_proposals") {
                line.push('\n');
                line.push_str(&measured_gaps_line("gaps with proposals", after));
            }
            line
        }
        gaps::Gaps::FalseAlarm(value) => format!(
            "gaps: proposed law {} fails on the contract's decision at input {}",
            value["false_alarm"]["law"], value["false_alarm"]["input"]
        ),
        gaps::Gaps::Inconclusive(inconclusive) => format!(
            "gaps: inconclusive, {} ({} over the limit {}); nothing is claimed",
            inconclusive.reason,
            inconclusive
                .size
                .map_or_else(|| "a size past 2^128".to_owned(), |size| size.to_string()),
            inconclusive.limit
        ),
        gaps::Gaps::OutOfScope(reason) => {
            format!("gaps: out of scope, {reason}; nothing is claimed")
        }
        gaps::Gaps::Withheld(reason) => format!("gaps: withheld, {reason}"),
        gaps::Gaps::InvalidProposals { place, reason } => {
            format!("gaps: the proposed laws have no contract form: {place}: {reason}")
        }
    }
}
