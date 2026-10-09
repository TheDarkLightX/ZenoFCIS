//! `zeno-fcis contract check-symbolic`, and the solver shell that it and
//! `transform check --symbolic` share. This module reads files, admits the
//! pinned CVC5 and Z3 once per command through the formal-tools adapter,
//! runs every query on both, measures wall-clock times, and writes reports.
//! The checks themselves are the pure `crate::symbolic` and
//! `crate::contract::symbolic`; no result grants authority.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Subcommand;
use serde_json::{Value, json};
use zeno_fcis_formal_tools::{
    ScriptAnswer, ScriptRun, SmtSession, ToolBackend, ToolFailure, load_tools_manifest,
};

use crate::contract::symbolic::{self, Checked, DEFAULT_MAX_TUPLES, Status, SymbolicSources};
use crate::contract_files::read_input;
use crate::symbolic::verdict::{Answer, Answers, Query, Solver};
use crate::{BLOCKED, FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, atomic_create, print_json};

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";

#[derive(Subcommand)]
pub(super) enum Command {
    /// Check with one CVC5 query per committing case and state law that every case keeps every state law, and the strengthening, from a lawful pre-state; a counterexample is replayed through the library. Domains that fit --max-tuples are also enumerated, and enumeration decides.
    CheckSymbolic {
        /// Application or contract directory holding project.zeno and v2/policy.json.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Strengthening invariant file (JSON, schema zeno-fcis/strengthening/1): state formulas assumed on every pre-state and checked on every successor and on genesis.
        #[arg(long)]
        strengthening: Option<PathBuf>,
        /// Tools manifest (zeno-fcis/tools/2) naming the pinned CVC5 and, to corroborate it, Z3. Without it no solver runs.
        #[arg(long)]
        tools: Option<PathBuf>,
        /// Largest domain the exhaustive route enumerates through the library Authority.
        #[arg(long, default_value_t = DEFAULT_MAX_TUPLES)]
        max_tuples: u64,
        /// Write the report to this file. Without it the report goes to stdout and the summary to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write every query's SMT-LIB text and each solver's output to this new directory.
        #[arg(long)]
        queries: Option<PathBuf>,
        /// Summary format, used with --out.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::CheckSymbolic {
            dir,
            strengthening,
            tools,
            max_tuples,
            out,
            queries,
            format,
        } => check_directory(&Request {
            dir: &dir,
            strengthening: strengthening.as_deref(),
            tools: tools.as_deref(),
            max_tuples,
            out: out.as_deref(),
            queries: queries.as_deref(),
            format,
        }),
    }
}

struct Request<'a> {
    dir: &'a Path,
    strengthening: Option<&'a Path>,
    tools: Option<&'a Path>,
    max_tuples: u64,
    out: Option<&'a Path>,
    queries: Option<&'a Path>,
    format: OutputFormat,
}

fn check_directory(request: &Request<'_>) -> u8 {
    let fail = |code: &str, place: Option<String>, message: String, exit: u8| {
        match request.format {
            OutputFormat::Human => match &place {
                Some(place) => eprintln!("{place}: {message}"),
                None => eprintln!("{message}"),
            },
            OutputFormat::Json => print_json(&json!({
                "schema": JSON_SCHEMA, "status": "error",
                "path": request.dir.display().to_string(), "authority": "none",
                "error": {"code": code, "place": place, "message": message}
            })),
        }
        exit
    };
    let read = |path: &Path| {
        read_input(path)
            .map_err(|error| format!("read {}: {error}", path.display()))
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|error| format!("read {}: {error}", path.display()))
            })
    };
    let texts = (|| {
        Ok::<_, String>((
            read(&request.dir.join(PROJECT))?,
            read(&request.dir.join(RULES))?,
            request.strengthening.map(read).transpose()?,
        ))
    })();
    let (project, rules, strengthening) = match texts {
        Ok(texts) => texts,
        Err(message) => return fail("symbolic-read-failed", None, message, FAILURE),
    };
    let solvers = match Solvers::open(request.tools, request.queries) {
        Ok(solvers) => solvers,
        Err(message) => return fail("solvers-blocked", None, message, BLOCKED),
    };
    let checked = symbolic::check(
        SymbolicSources {
            project: &project,
            rules: &rules,
            strengthening: strengthening.as_deref(),
        },
        request.max_tuples,
        &mut |query| solvers.solve(query),
    );
    let Checked {
        status,
        mut report,
        lines,
    } = match checked {
        Ok(checked) => checked,
        Err(error) => {
            return fail(
                "contract-invalid",
                Some(error.place().to_owned()),
                error.reason().to_owned(),
                INVALID,
            );
        }
    };
    report["solvers"] = solvers.identities();
    if let Some(message) = solvers.retention_failure() {
        return fail("queries-write-failed", None, message, FAILURE);
    }
    let packet = crate::transform::canonical_json(&report);
    match request.out {
        Some(path) => {
            if let Err(error) = crate::atomic_replace(path, packet.as_bytes()) {
                return fail(
                    "report-write-failed",
                    None,
                    format!("write {}: {error}", path.display()),
                    FAILURE,
                );
            }
            match request.format {
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": status.name(),
                    "path": request.dir.display().to_string(), "authority": "none",
                    "evidence": status.evidence(), "report": path.display().to_string(),
                    "report_schema": symbolic::REPORT_SCHEMA, "summary": report["summary"],
                })),
                OutputFormat::Human => {
                    for line in &lines {
                        println!("{line}");
                    }
                    println!("report: {}", path.display());
                }
            }
        }
        None => {
            print!("{packet}");
            for line in &lines {
                eprintln!("{line}");
            }
        }
    }
    match status {
        Status::Proved => OK,
        Status::Refuted => INVALID,
        Status::Attested | Status::Inconclusive => BLOCKED,
    }
}

/// The admitted solvers of one command and where its queries are kept.
pub(crate) struct Solvers {
    cvc5: Option<SmtSession>,
    z3: Option<SmtSession>,
    queries: Option<PathBuf>,
    retention: RefCell<Option<String>>,
}

impl Solvers {
    /// Admits every CVC5 and Z3 entry of the manifest, once. Without a
    /// manifest no solver is configured.
    ///
    /// # Errors
    /// A manifest that does not load, a configured solver that fails
    /// admission, or a query directory that exists or cannot be created.
    pub(crate) fn open(tools: Option<&Path>, queries: Option<&Path>) -> Result<Self, String> {
        if let Some(directory) = queries
            && directory.symlink_metadata().is_ok()
        {
            return Err(format!(
                "{} exists; queries are written only to a new directory",
                directory.display()
            ));
        }
        let mut solvers = Self {
            cvc5: None,
            z3: None,
            queries: queries.map(Path::to_path_buf),
            retention: RefCell::new(None),
        };
        if let Some(tools) = tools {
            let manifest = load_tools_manifest(tools)
                .map_err(|error| format!("tools manifest blocked: {error:?}"))?;
            for backend in [ToolBackend::Cvc5, ToolBackend::Z3] {
                if let Some(config) = manifest.tool(backend) {
                    let session = SmtSession::open(config).map_err(|error| {
                        format!("{} admission failed: {error:?}", backend_name(backend))
                    })?;
                    match backend {
                        ToolBackend::Cvc5 => solvers.cvc5 = Some(session),
                        _ => solvers.z3 = Some(session),
                    }
                }
            }
        }
        // Created only once every configured solver is admitted, so a
        // refused manifest leaves no directory behind.
        if let Some(directory) = queries {
            fs::create_dir_all(directory)
                .map_err(|error| format!("create {}: {error}", directory.display()))?;
        }
        Ok(solvers)
    }

    /// Whether CVC5, which every `unsat` result needs, is configured.
    pub(crate) fn has_cvc5(&self) -> bool {
        self.cvc5.is_some()
    }

    /// Runs one query on both solvers at once, and keeps it when asked.
    pub(crate) fn solve(&self, query: &Query<'_>) -> Answers {
        let timed = |session: Option<&SmtSession>| {
            session.map(|session| {
                let started = Instant::now();
                let run = session.run(query.source.as_bytes());
                let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                (run, millis)
            })
        };
        let (cvc5_session, z3_session) = (self.cvc5.as_ref(), self.z3.as_ref());
        let (cvc5, z3) = std::thread::scope(|scope| {
            let z3 = scope.spawn(move || timed(z3_session));
            let cvc5 = timed(cvc5_session);
            let z3 = z3.join().unwrap_or_else(|_| {
                Some((Err(ToolFailure::Io("the Z3 thread panicked".to_owned())), 0))
            });
            (cvc5, z3)
        });
        self.retain(query, [cvc5.as_ref(), z3.as_ref()]);
        let answer = |run: &Option<(Result<ScriptRun, ToolFailure>, u64)>| match run {
            None => (Answer::NotConfigured, None),
            Some((Ok(run), millis)) => (classified(run.answer()), Some(*millis)),
            Some((Err(failure), millis)) => (Answer::Failed(format!("{failure:?}")), Some(*millis)),
        };
        let (cvc5, cvc5_millis) = answer(&cvc5);
        let (z3, z3_millis) = answer(&z3);
        Answers {
            cvc5,
            z3,
            millis: [cvc5_millis, z3_millis],
        }
    }

    fn retain(&self, query: &Query<'_>, runs: [Option<&(Result<ScriptRun, ToolFailure>, u64)>; 2]) {
        let Some(directory) = &self.queries else {
            return;
        };
        let mut files = vec![(
            format!("{}.smt2", query.label),
            query.source.as_bytes().to_vec(),
        )];
        for (solver, run) in Solver::BOTH.into_iter().zip(runs) {
            if let Some((Ok(run), _)) = run {
                let mut output = run.stdout().to_vec();
                output.extend_from_slice(run.stderr());
                files.push((format!("{}.{}.out", query.label, solver.name()), output));
            }
        }
        for (name, bytes) in files {
            if let Err(error) = atomic_create(&directory.join(&name), &bytes) {
                self.retention.borrow_mut().get_or_insert_with(|| {
                    format!("write {}: {error}", directory.join(name).display())
                });
            }
        }
    }

    /// The first query file that could not be written.
    pub(crate) fn retention_failure(&self) -> Option<String> {
        self.retention.borrow().clone()
    }

    /// Each solver's checked identity.
    pub(crate) fn identities(&self) -> Value {
        let identity = |session: &Option<SmtSession>| {
            session.as_ref().map_or(Value::Null, |session| {
                let identity = session.identity();
                json!({"version": identity.version(), "sha256": identity.binary_hash().to_string()})
            })
        };
        json!({"cvc5": identity(&self.cvc5), "z3": identity(&self.z3)})
    }
}

/// A formal-tools answer as the checks judge it.
fn classified(answer: &ScriptAnswer) -> Answer {
    match answer {
        ScriptAnswer::Unsat { proof_output } => Answer::Unsat {
            proof_output: *proof_output,
        },
        ScriptAnswer::Sat { values } => Answer::Sat(values.clone()),
        ScriptAnswer::Unknown => Answer::Unknown,
        ScriptAnswer::NoAnswer(failure) => Answer::Failed(format!("{failure:?}")),
        other => Answer::Failed(format!("{other:?}")),
    }
}

fn backend_name(backend: ToolBackend) -> &'static str {
    match backend {
        ToolBackend::Cvc5 => "cvc5",
        ToolBackend::Z3 => "z3",
        ToolBackend::Lean => "lean",
    }
}
