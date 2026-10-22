//! `zeno-fcis contract profile`, `lemma`, `scenario` and `probe`: the
//! commands over the pure judges of `crate::contract::review`. This module
//! only reads the application's files, writes the packet where asked and
//! prints a summary. No judge grants authority or changes an application
//! file.

use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::review::evaluate::{digest, hex};

use crate::contract::review::{
    self, DEFAULT_MAX_TUPLES, Inconclusive, lemma, probe, profile, run_properties, scenario,
};
use crate::contract_files::read_input;
use crate::{
    BLOCKED, FAILURE, INVALID, JSON_SCHEMA, MALFORMED, OK, OutputFormat, USAGE, atomic_replace,
    print_json,
};

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";

const ADVISORY: &str = "Advisory only. This packet grants no authority and changes no \
application file. Every decision in it is the library Authority's on the contract bound as \
the application binds it, and every law verdict is the library's law evaluator's. Results \
hold over the declared domain only, and an inconclusive result claims nothing.";

/// What every judge reads and writes.
#[derive(Args)]
pub(super) struct Common {
    /// Application directory holding project.zeno and v2/policy.json.
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Write the packet to this file. Without it the packet goes to stdout and the summary to stderr.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Largest declared domain, in tuples, to enumerate; a larger one is inconclusive (exit 2).
    #[arg(long, default_value_t = DEFAULT_MAX_TUPLES)]
    max_tuples: u64,
    /// Summary format, used with --out.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Check keyed collections by data independence: every admissible view of the named entries, with alias merges and summary deltas. Exit 0 checked, 1 refuted, 2 UNKNOWN; advisory only.
    Collections {
        #[command(flatten)]
        common: Common,
    },
    /// Check declared bounded observers and safety properties over runs of any length. Exhaustive finite domains only; advisory.
    Runs {
        /// Observer and property declarations; defaults to v2/run-properties.json.
        #[arg(long)]
        file: Option<PathBuf>,
        #[command(flatten)]
        common: Common,
    },
    /// Check finite distribution laws exactly, given uniform independent random bits. Exit 0 checked, 1 refuted, 2 UNKNOWN; advisory only.
    Distribution {
        /// Invocations plus outcome projection/fold operations; a stop is UNKNOWN.
        #[arg(long, default_value_t = zeno_fcis_contract::distribution::MAX_WORK)]
        max_work: u64,
        #[command(flatten)]
        common: Common,
    },
    /// Profile a contract's states, memory and robustness exactly over its declared domain: valid, reachable and Moore-minimal states, and what happens on pre-states that break a state law. Advisory; the application is not changed.
    Profile {
        #[command(flatten)]
        common: Common,
    },
    /// Judge state invariants by induction, or --transition relations on actual before/after decisions. Exit 0 when every invariant is inductive or relation is checked, 1 when refuted, 2 when incomplete, undefined or vacuous, 65 for a formula with no contract form. Advisory, finite-domain results only.
    Lemma {
        /// A project.zeno state formula, or with --transition a before/after relation. Repeat for more lemmas.
        #[arg(long = "lemma", required = true)]
        lemmas: Vec<String>,
        /// Check a before/after relation on every admitted committed decision, instead of state induction.
        /// Relations may read pre., command., context. and actual post. values.
        /// Exit 0 means checked on the finite domain, 1 refuted, 2 undefined or vacuous.
        #[arg(long)]
        transition: bool,
        #[command(flatten)]
        common: Common,
    },
    /// Replay scenarios, runs of decision examples under `# scenario NAME` headers, through the contract and, when every scenario passes on it, through every mutant of the extended catalog and the reject-everything core. Exit 1 when a scenario fails on the contract, 65 for a malformed scenario file. Advisory.
    Scenario {
        /// The scenario file; by default tests/decision-scenarios.txt in the application.
        #[arg(long)]
        file: Option<PathBuf>,
        #[command(flatten)]
        common: Common,
    },
    /// Ask an exact question about a contract's decisions over its declared domain. Advisory.
    Probe {
        #[command(subcommand)]
        question: ProbeCommand,
    },
}

#[derive(Subcommand)]
pub(super) enum ProbeCommand {
    /// The case that decides one input, the cases tried before it, and what the Authority does.
    Explain {
        /// One number per input position, in program order, separated by spaces.
        #[arg(long)]
        input: String,
        #[command(flatten)]
        common: Common,
    },
    /// A shortest trace from genesis to a state satisfying a state formula, or that no reachable state does.
    Reachable {
        /// A state formula, written as a state law over post. state fields.
        #[arg(long)]
        state: String,
        #[command(flatten)]
        common: Common,
    },
    /// On how many inputs a condition holds on the contract's decision.
    Count {
        /// A project.zeno relation over pre., command., context. and post. fields.
        #[arg(long = "where")]
        condition: String,
        #[command(flatten)]
        common: Common,
    },
    /// The first input where one condition holds on the contract's decision and another does not.
    Implies {
        /// The condition that must imply the other.
        #[arg(long = "if")]
        premise: String,
        /// The implied condition.
        #[arg(long = "then")]
        conclusion: String,
        #[command(flatten)]
        common: Common,
    },
    /// The distinct outcomes, with counts, over the inputs where a condition holds, or over every input.
    Decisions {
        /// Only inputs where this condition holds.
        #[arg(long = "where")]
        condition: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Two inputs that differ only in one position and are decided differently, or that none are.
    Depends {
        /// The input position, as the review names it: pre.100.110, command.101 or context.102.130.
        #[arg(long)]
        position: String,
        /// Only inputs where this condition holds.
        #[arg(long = "where")]
        condition: Option<String>,
        #[command(flatten)]
        common: Common,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Collections { common } => judge(
            &common,
            "zeno-fcis/collections-check/1",
            None,
            true,
            |sources| {
                use zeno_fcis_contract::collections::{self, Limits, Verdict};
                let result = collections::check(
                    sources.project,
                    sources.rules,
                    Limits {
                        max_tuples: common.max_tuples,
                    },
                )?;
                let exit = match result.verdict() {
                    Verdict::Checked => OK,
                    Verdict::Refuted => INVALID,
                    Verdict::Unknown => BLOCKED,
                };
                Ok(Judged::Done(
                    result.packet().clone(),
                    exit,
                    result.verdict().name(),
                ))
            },
        ),
        Command::Runs { file, common } => {
            let path = file.unwrap_or_else(|| common.dir.join(run_properties::FILE));
            judge(
                &common,
                "zeno-fcis/contract-run-properties/1",
                Some(&path),
                true,
                |sources| {
                    Ok(
                        match run_properties::judge(
                            sources.project,
                            sources.rules,
                            sources.extra.unwrap_or_default(),
                            common.max_tuples,
                        )? {
                            lemma::Lemmas::Inconclusive(stop) => Judged::Inconclusive(stop),
                            lemma::Lemmas::NoStartingState(value) => {
                                Judged::Done(value, INVALID, "no-admissible-starting-state")
                            }
                            lemma::Lemmas::Judged(properties) => {
                                let (exit, status) =
                                    if properties.iter().any(|p| p["verdict"] == "false") {
                                        (INVALID, "refuted")
                                    } else if properties.iter().any(|p| p["verdict"] != "inductive")
                                    {
                                        (BLOCKED, "inconclusive")
                                    } else {
                                        (OK, "inductive")
                                    };
                                Judged::Done(json!({"properties":properties}), exit, status)
                            }
                        },
                    )
                },
            )
        }
        Command::Distribution { max_work, common } => judge(
            &common,
            "zeno-fcis/distribution-laws/1",
            None,
            true,
            |sources| {
                use zeno_fcis_contract::distribution::{self, Limits, Verdict};
                let result = distribution::check(
                    sources.project,
                    sources.rules,
                    Limits {
                        max_tuples: common.max_tuples,
                        max_work,
                    },
                )?;
                let exit = match result.verdict() {
                    Verdict::Checked => OK,
                    Verdict::Refuted => INVALID,
                    Verdict::Unknown => BLOCKED,
                };
                Ok(Judged::Done(
                    result.packet().clone(),
                    exit,
                    result.verdict().name(),
                ))
            },
        ),
        Command::Profile { common } => judge(
            &common,
            "zeno-fcis/contract-profile/2",
            None,
            true,
            |sources| {
                Ok(
                    match profile::profile(sources.project, sources.rules, common.max_tuples)? {
                        profile::Profile::Measured(value) => Judged::Done(value, OK, "measured"),
                        profile::Profile::NoStartingState(value) => {
                            Judged::Done(value, INVALID, "no-admissible-starting-state")
                        }
                        profile::Profile::Inconclusive(inconclusive) => {
                            Judged::Inconclusive(inconclusive)
                        }
                    },
                )
            },
        ),
        Command::Lemma {
            lemmas,
            transition,
            common,
        } => judge(
            &common,
            if transition {
                "zeno-fcis/contract-transition-lemma/2"
            } else {
                "zeno-fcis/contract-lemma/2"
            },
            None,
            true,
            |sources| {
                Ok(
                    match if transition {
                        lemma::judge_transitions(
                            sources.project,
                            sources.rules,
                            &lemmas,
                            common.max_tuples,
                        )?
                    } else {
                        lemma::judge(sources.project, sources.rules, &lemmas, common.max_tuples)?
                    } {
                        lemma::Lemmas::Inconclusive(inconclusive) => {
                            Judged::Inconclusive(inconclusive)
                        }
                        lemma::Lemmas::NoStartingState(value) => {
                            Judged::Done(value, INVALID, "no-admissible-starting-state")
                        }
                        lemma::Lemmas::Judged(verdicts) => {
                            let any = |name: &str| {
                                verdicts.iter().any(|verdict| verdict["verdict"] == name)
                            };
                            let (exit, status) = if any(lemma::Verdict::False.name()) {
                                (INVALID, "refuted")
                            } else if transition && (any("undefined") || any("vacuous")) {
                                (BLOCKED, "inconclusive")
                            } else if transition {
                                (OK, "checked")
                            } else if any(lemma::Verdict::NotInductive.name()) {
                                (BLOCKED, "not-inductive")
                            } else {
                                (OK, "inductive")
                            };
                            Judged::Done(json!({"lemmas": verdicts}), exit, status)
                        }
                    },
                )
            },
        ),
        Command::Scenario { file, common } => {
            let path = file.unwrap_or_else(|| common.dir.join(scenario::FILE));
            let label = path.display().to_string();
            judge(
                &common,
                "zeno-fcis/contract-scenario/2",
                Some(&path),
                false,
                |sources| {
                    let text = sources.extra.unwrap_or_default();
                    let replayed = scenario::replay(sources.project, sources.rules, text, &label)?;
                    Ok(if replayed.failed == 0 {
                        Judged::Done(replayed.packet, OK, "passed")
                    } else {
                        Judged::Done(replayed.packet, INVALID, "failed")
                    })
                },
            )
        }
        Command::Probe { question } => {
            let (common, question) = match parse_question(question) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    return USAGE;
                }
            };
            let enumerates = !matches!(question, probe::Question::Explain { .. });
            judge(
                &common,
                "zeno-fcis/contract-probe/2",
                None,
                enumerates,
                |sources| {
                    Ok(
                        match probe::probe(
                            sources.project,
                            sources.rules,
                            &question,
                            common.max_tuples,
                        )? {
                            probe::Answer::Answered(value) if value["answer"] == "vacuous" => {
                                Judged::Done(
                                    json!({"question": question.name(), "result": value}),
                                    BLOCKED,
                                    "vacuous",
                                )
                            }
                            probe::Answer::Answered(value) => Judged::Done(
                                json!({"question": question.name(), "result": value}),
                                OK,
                                "answered",
                            ),
                            probe::Answer::NoStartingState(value) => {
                                Judged::Done(value, INVALID, "no-admissible-starting-state")
                            }
                            probe::Answer::Inconclusive(inconclusive) => {
                                Judged::Inconclusive(inconclusive)
                            }
                        },
                    )
                },
            )
        }
    }
}

fn parse_question(question: ProbeCommand) -> Result<(Common, probe::Question), String> {
    Ok(match question {
        ProbeCommand::Explain { input, common } => {
            let input = input
                .split_whitespace()
                .map(|word| {
                    word.parse::<i128>()
                        .map_err(|_| format!("--input: `{word}` is not an integer"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            (common, probe::Question::Explain { input })
        }
        ProbeCommand::Reachable { state, common } => (common, probe::Question::Reachable { state }),
        ProbeCommand::Count { condition, common } => (common, probe::Question::Count { condition }),
        ProbeCommand::Implies {
            premise,
            conclusion,
            common,
        } => (
            common,
            probe::Question::Implies {
                premise,
                conclusion,
            },
        ),
        ProbeCommand::Decisions { condition, common } => {
            (common, probe::Question::Decisions { condition })
        }
        ProbeCommand::Depends {
            position,
            condition,
            common,
        } => (
            common,
            probe::Question::Depends {
                position,
                condition,
            },
        ),
    })
}

/// The files a judge reads.
struct Sources<'a> {
    project: &'a str,
    rules: &'a str,
    extra: Option<&'a str>,
}

/// A judge's result: its section of the packet, exit code and status, or
/// inconclusive.
enum Judged {
    Done(Value, u8, &'static str),
    Inconclusive(Inconclusive),
}

/// Why a judge did not run.
enum Failure {
    Read(String),
    Invalid { place: String, reason: String },
}

fn judge(
    common: &Common,
    schema: &str,
    extra: Option<&Path>,
    enumerates: bool,
    work: impl FnOnce(&Sources<'_>) -> Result<Judged, zeno_fcis_contract::ContractError>,
) -> u8 {
    if common.max_tuples == 0 {
        eprintln!("--max-tuples must be at least 1");
        return USAGE;
    }
    let dir = &common.dir;
    let result = (|| {
        let project = read_text(&dir.join(PROJECT), PROJECT)?;
        let rules = read_text(&dir.join(RULES), RULES)?;
        let extra_text = extra
            .map(|path| read_text(path, &path.display().to_string()))
            .transpose()?;
        let sources = Sources {
            project: &project,
            rules: &rules,
            extra: extra_text.as_deref(),
        };
        let invalid = |error: zeno_fcis_contract::ContractError| Failure::Invalid {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        };
        let application = review::application(&project, &rules).map_err(invalid)?;
        let judged = work(&sources).map_err(invalid)?;
        let sha256 = |text: &str| hex(&digest(text.as_bytes()));
        Ok((
            application,
            judged,
            json!({
                "project_sha256": sha256(&project),
                "rules_sha256": sha256(&rules),
                "extra_sha256": extra_text.as_deref().map(sha256),
            }),
        ))
    })();
    let (application, judged, sources) = match result {
        Ok(done) => done,
        Err(failure) => return report_failure(dir, failure, common.format),
    };
    let (body, exit, status) = match judged {
        Judged::Done(body, exit, status) => (body, exit, status),
        Judged::Inconclusive(inconclusive) => (inconclusive.json(), BLOCKED, "inconclusive"),
    };
    let mut packet = json!({
        "schema": schema,
        "integer_encoding": crate::report_numbers::ENCODING,
        "authority": "none",
        "advisory": if schema == "zeno-fcis/distribution-laws/1" {
            "Advisory only; no runtime or publication authority. Exactly checked only on complete finite-domain results, given uniform independent random bits. UNKNOWN claims nothing."
        } else { ADVISORY },
        "application": application,
        "tool": {"crate": "zeno-fcis-cli", "version": env!("CARGO_PKG_VERSION")},
        "sources": sources,
        "status": status,
    });
    if enumerates {
        packet["max_tuples"] = json!(common.max_tuples);
    }
    if let (Value::Object(packet), Value::Object(body)) = (&mut packet, body) {
        for (key, value) in body {
            packet.entry(key).or_insert(value);
        }
    }
    crate::telemetry::observe(&packet);
    let text = crate::transform::canonical_json(&packet);
    let premise = packet.get("premise").or_else(|| packet.get("reason"));
    let line = match premise.and_then(Value::as_str) {
        Some(premise) => format!("{status} {application}: {schema}; {premise}"),
        None => format!("{status} {application}: {schema}"),
    };
    match &common.out {
        Some(path) => {
            let written = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| atomic_replace(path, text.as_bytes()));
            if let Err(error) = written {
                let message = format!("write {}: {error}", path.display());
                return report_failure(dir, Failure::Read(message), common.format);
            }
            match common.format {
                OutputFormat::Json => {
                    let mut summary = json!({
                        "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
                        "authority": "none", "packet": path.display().to_string(),
                        "packet_schema": schema,
                    });
                    if let Some(premise) = premise {
                        summary["premise"] = premise.clone();
                    }
                    print_json(&summary);
                }
                OutputFormat::Human => {
                    println!("{line}");
                    println!("packet: {}", path.display());
                }
            }
        }
        None => {
            print!("{text}");
            eprintln!("{line}");
        }
    }
    exit
}

fn report_failure(dir: &Path, failure: Failure, format: OutputFormat) -> u8 {
    let (code, place, message, exit) = match failure {
        Failure::Read(message) => ("judge-read-failed", None, message, FAILURE),
        Failure::Invalid { place, reason } => {
            ("judge-input-invalid", Some(place), reason, MALFORMED)
        }
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

fn read_text(path: &Path, name: &str) -> Result<String, Failure> {
    let bytes = read_input(path).map_err(|error| Failure::Read(format!("read {name}: {error}")))?;
    String::from_utf8(bytes).map_err(|error| Failure::Read(format!("read {name}: {error}")))
}
