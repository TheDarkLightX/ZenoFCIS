//! `zeno-fcis` authoring CLI.
#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_formal_tools::{
    CVC5_VERSION, LEAN_VERSION, ToolBackend, ToolFailure, ToolRunStatus, Z3_VERSION, doctor,
    execute_tool, export_lean, export_smt, inspect_lean_toolchain, load_tools_manifest, retain_run,
    verify_tool,
};
use zeno_fcis_spec::{
    ClaimDecl, ClaimMode, Diagnostic, DiagnosticSet, GraphFormat, ProjectLimits, ProjectSpec,
    SourceLimits, StableId, derive_composition, elaborate_project, generate_project, parse_project,
    render_graph,
};

const JSON_SCHEMA: &str = "zeno-fcis/cli/1";
const OK: u8 = 0;
const INVALID: u8 = 1;
const BLOCKED: u8 = 2;
const FAILURE: u8 = 3;
const USAGE: u8 = 64;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const MINIMAL: &str = r#"zeno 1;
project 1 minimal;
namespace 10 core;
type 100 state State;
type 101 command Command;
type 102 context Context;
type 103 destination Destination;
type 104 payload Payload;
reason 200 invalid precedence 0;
component 300 machine {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 1024;
}
merge [300];
law 400 identity = pre.100 == pre.100;
claim 500 identity cvc5 relational = pre.100 == pre.100;
"#;

const MINI: &str = r#"zeno 1;
project 2 mini_determinator;
namespace 10 mini_os;
type 100 state CoordinatorState;
type 101 state WorkerState;
type 102 command Execute;
type 103 context WorkerResults;
type 104 destination ReturnDestination;
type 105 payload ReturnPayload;
reason 200 merge_conflict precedence 0;
reason 201 missing_footprint precedence 1;
component 300 coordinator {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.103;
  budget steps 4096;
  budget nodes 256;
}
component 301 worker_space {
  owns 101;
  reads pre.101;
  writes post.101;
  budget steps 4096;
  budget nodes 256;
}
merge [300, 301];
law 400 worker_isolation = pre.100 == pre.100;
claim 500 finite_state_reflexivity all finite 4 = always atom(pre.100 == pre.100);
claim 501 unbounded_state_reflexivity lean unbounded = always atom(pre.100 == pre.100);
"#;

#[derive(Parser)]
#[command(
    name = "zeno-fcis",
    version,
    about = "Deterministic authoring, composition, and formal-tool workflows for ZenoFCIS",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a bounded project without overwriting a nonempty directory.
    New {
        dir: PathBuf,
        #[arg(long, value_enum, default_value_t = Template::Minimal)]
        template: Template,
    },
    /// Parse and elaborate a .zeno project with accumulated diagnostics.
    Check {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Generate deterministic Rust and manifest artifacts, or check for drift.
    Generate {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        check: bool,
    },
    /// Render a deterministic composition graph as DOT, Mermaid, or JSON.
    Graph {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long, value_enum)]
        format: GraphOutput,
    },
    /// Explain all diagnostics or one stable diagnostic code.
    Explain {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long)]
        code: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Run an exact, separately configured CVC5, Z3, or Lean backend.
    Prove {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long)]
        claim: String,
        #[arg(long, value_enum)]
        backend: BackendChoice,
        #[arg(long, default_value = "zeno-fcis.tools.json")]
        tools: PathBuf,
    },
    /// Request and replay a bounded SMT counterexample.
    Counterexample {
        #[arg(default_value = "project.zeno")]
        project: PathBuf,
        #[arg(long)]
        claim: String,
        #[arg(long, value_enum)]
        backend: SmtBackend,
        #[arg(long, default_value = "zeno-fcis.tools.json")]
        tools: PathBuf,
    },
    /// Check separately configured formal tools and their exact identities.
    Doctor {
        #[arg(long, default_value = "zeno-fcis.tools.json")]
        tools: PathBuf,
    },
    /// List, inspect, inventory, or verify closed formal backend configurations.
    Backend {
        #[command(subcommand)]
        command: BackendCommand,
    },
}

#[derive(Subcommand)]
enum BackendCommand {
    /// List supported backend families and pinned versions.
    List,
    /// Inspect the separate tools manifest without executing a backend.
    Inspect {
        #[arg(long, default_value = "zeno-fcis.tools.json")]
        tools: PathBuf,
    },
    /// Compute the portable bounded inventory of a Lean distribution.
    InventoryLean {
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Recheck configured executable versions and hashes.
    Verify {
        #[arg(long, default_value = "zeno-fcis.tools.json")]
        tools: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Template {
    Minimal,
    MiniDeterminator,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum GraphOutput {
    Dot,
    Mermaid,
    Json,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackendChoice {
    Cvc5,
    Z3,
    Lean,
    All,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum SmtBackend {
    Cvc5,
    Z3,
}

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => ExitCode::from(run(cli.command)),
        Err(error) => {
            let exit = clap_error_exit(&error);
            let _ = error.print();
            ExitCode::from(exit)
        }
    }
}

fn clap_error_exit(error: &clap::Error) -> u8 {
    if error.exit_code() == 0 { OK } else { USAGE }
}

fn run(command: Command) -> u8 {
    match command {
        Command::New { dir, template } => new_project(&dir, template),
        Command::Check { project, format } => check(&project, format),
        Command::Generate {
            project,
            out,
            check,
        } => generate(&project, &out, check),
        Command::Graph { project, format } => graph(&project, format),
        Command::Explain {
            project,
            code,
            format,
        } => explain(&project, code.as_deref(), format),
        Command::Prove {
            project,
            claim,
            backend,
            tools,
        } => prove(&project, &claim, backend, &tools, false),
        Command::Counterexample {
            project,
            claim,
            backend,
            tools,
        } => prove(
            &project,
            &claim,
            match backend {
                SmtBackend::Cvc5 => BackendChoice::Cvc5,
                SmtBackend::Z3 => BackendChoice::Z3,
            },
            &tools,
            true,
        ),
        Command::Doctor { tools } => run_doctor(&tools),
        Command::Backend { command } => backend(command),
    }
}

fn new_project(dir: &Path, template: Template) -> u8 {
    if dir.exists() {
        match fs::read_dir(dir) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    eprintln!("target is nonempty: {}", dir.display());
                    return INVALID;
                }
            }
            Err(error) => return io_error("inspect target", error),
        }
    } else if let Err(error) = fs::create_dir(dir) {
        return io_error("create target", error);
    }
    let (source, readme) = match template {
        Template::Minimal => (
            MINIMAL,
            "# ZenoFCIS minimal project\n\nRun `zeno-fcis check`.\n",
        ),
        Template::MiniDeterminator => (
            MINI,
            "# Mini Determinator\n\nA pure shared-nothing semantic example. Run `zeno-fcis check`.\n",
        ),
    };
    if let Err(error) = atomic_create(&dir.join("project.zeno"), source.as_bytes()) {
        return io_error("write project", error);
    }
    if let Err(error) = atomic_create(&dir.join("README.md"), readme.as_bytes()) {
        return io_error("write README", error);
    }
    println!("created {}", dir.display());
    OK
}

fn check(path: &Path, format: OutputFormat) -> u8 {
    let spec = match project_or_report(path, format) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let derived = match derive_composition::<RustCryptoSha256>(&spec) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("derive composition failed: {error:?}");
            return FAILURE;
        }
    };
    match format {
        OutputFormat::Human => println!(
            "checked {}: project={} components={} claims={} unresolved_obligations={} semantic_program_hash={}",
            path.display(),
            spec.project_id().get(),
            spec.components().len(),
            spec.claims().len(),
            derived.obligations().len(),
            derived.semantic_program_hash()
        ),
        OutputFormat::Json => print_json(&json!({
            "claims": spec.claims().len(), "components": spec.components().len(),
            "path": path.display().to_string(), "project_id": spec.project_id().get(),
            "schema": JSON_SCHEMA, "semantic_program_hash": derived.semantic_program_hash().to_string(),
            "status": "valid", "unresolved_obligations": derived.obligations().len()
        })),
    }
    OK
}

fn generate(path: &Path, out: &Path, check_only: bool) -> u8 {
    let spec = match project_or_report(path, OutputFormat::Human) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let generated = match generate_project::<RustCryptoSha256>(&spec) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("generation failed: {error:?}");
            return FAILURE;
        }
    };
    let files = [
        ("generated.rs", generated.rust().as_bytes()),
        ("PROJECT_MANIFEST.zfcis", generated.manifest()),
    ];
    if check_only {
        let drift: Vec<&str> = files
            .iter()
            .filter_map(|(name, expected)| match fs::read(out.join(name)) {
                Ok(actual) if actual == *expected => None,
                _ => Some(*name),
            })
            .collect();
        if drift.is_empty() {
            println!("generated artifacts are current");
            OK
        } else {
            eprintln!("generated drift: {}", drift.join(", "));
            INVALID
        }
    } else {
        if let Err(error) = fs::create_dir_all(out) {
            return io_error("create output directory", error);
        }
        for (name, bytes) in files {
            if let Err(error) = atomic_replace(&out.join(name), bytes) {
                return io_error("write generated artifact", error);
            }
        }
        println!("generated {}", out.display());
        OK
    }
}

fn graph(path: &Path, format: GraphOutput) -> u8 {
    let spec = match project_or_report(path, OutputFormat::Human) {
        Ok(value) => value,
        Err(code) => return code,
    };
    print!(
        "{}",
        render_graph(
            &spec,
            match format {
                GraphOutput::Dot => GraphFormat::Dot,
                GraphOutput::Mermaid => GraphFormat::Mermaid,
                GraphOutput::Json => GraphFormat::Json,
            }
        )
    );
    OK
}

fn explain(path: &Path, code: Option<&str>, format: OutputFormat) -> u8 {
    match load_project(path) {
        Ok(spec) => {
            if matches!(format, OutputFormat::Json) {
                print_json(&json!({
                    "authority": "diagnostic-only", "claims": spec.claims().len(), "code": code,
                    "components": spec.components().len(), "schema": JSON_SCHEMA, "status": "valid"
                }));
            } else if let Some(code) = code {
                println!("{code}: no matching diagnostic in {}", path.display());
            } else {
                println!(
                    "{} is valid; derived explanations grant no authority",
                    path.display()
                );
            }
            OK
        }
        Err(ProjectLoad::Invalid(set)) => {
            let filtered: Vec<&Diagnostic> = set
                .diagnostics()
                .iter()
                .filter(|item| code.is_none_or(|wanted| item.code().as_str() == wanted))
                .collect();
            if matches!(format, OutputFormat::Json) {
                print_json(&diagnostic_json(path, &filtered, set.is_truncated()));
            } else if filtered.is_empty() {
                println!("no matching diagnostic");
            } else {
                for item in filtered {
                    println!(
                        "{} {}:{}:{} {}\n  expected: {}\n  actual: {}\n  remediation: {}",
                        item.code().as_str(),
                        item.stage().as_str(),
                        item.span().line(),
                        item.span().column(),
                        item.path().as_str(),
                        item.expected(),
                        item.actual(),
                        item.remediation()
                    );
                }
            }
            INVALID
        }
        Err(ProjectLoad::System(message)) => {
            eprintln!("{message}");
            FAILURE
        }
    }
}

fn prove(
    path: &Path,
    selector: &str,
    choice: BackendChoice,
    tools: &Path,
    counterexample: bool,
) -> u8 {
    let spec = match project_or_report(path, OutputFormat::Human) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let claims = match select_claims(&spec, selector) {
        Ok(values) => values,
        Err(message) => {
            eprintln!("{message}");
            return INVALID;
        }
    };
    let manifest = match load_tools_manifest(tools) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("tools manifest blocked: {error:?}");
            return BLOCKED;
        }
    };
    let requested: &[ToolBackend] = match choice {
        BackendChoice::Cvc5 => &[ToolBackend::Cvc5],
        BackendChoice::Z3 => &[ToolBackend::Z3],
        BackendChoice::Lean => &[ToolBackend::Lean],
        BackendChoice::All => &[ToolBackend::Cvc5, ToolBackend::Z3, ToolBackend::Lean],
    };
    let evidence = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".zeno-fcis/evidence");
    let mut exit = OK;
    let mut ran = 0usize;
    for claim in claims {
        for tool_backend in requested.iter().copied() {
            let compatible = match claim.mode() {
                ClaimMode::UnboundedProof => tool_backend == ToolBackend::Lean,
                ClaimMode::Relational | ClaimMode::Finite { .. } => {
                    tool_backend != ToolBackend::Lean
                }
            } && claim.backends().contains(&tool_backend.spec_backend());
            if !compatible {
                if !matches!(choice, BackendChoice::All) {
                    eprintln!(
                        "claim {} does not select compatible {}",
                        claim.id().get(),
                        backend_name(tool_backend)
                    );
                    exit = exit.max(BLOCKED);
                }
                continue;
            }
            ran += 1;
            let Some(config) = manifest.tool(tool_backend) else {
                eprintln!(
                    "{} is absent from tools manifest",
                    backend_name(tool_backend)
                );
                exit = exit.max(BLOCKED);
                continue;
            };
            let obligation = match tool_backend {
                ToolBackend::Cvc5 | ToolBackend::Z3 => export_smt(claim, tool_backend),
                ToolBackend::Lean => export_lean(claim),
            };
            let obligation = match obligation {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("claim {} export blocked: {error:?}", claim.id().get());
                    exit = exit.max(BLOCKED);
                    continue;
                }
            };
            let run = match execute_tool(config, obligation) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!(
                        "{} claim {} blocked: {error:?}",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    exit = exit.max(failure_exit(&error));
                    continue;
                }
            };
            if let Err(error) = retain_run(&evidence, &run) {
                eprintln!("retain run failed: {error:?}");
                exit = exit.max(FAILURE);
                continue;
            }
            let code = match run.status() {
                ToolRunStatus::ProposedUnsat if counterexample => {
                    println!(
                        "{} claim {}: solver proposed UNSAT for the requested scope; proof output was not independently checked",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::ProposedUnsat => {
                    println!(
                        "{} claim {}: UNSAT proposal retained; proof output was not independently checked",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::KernelChecked if counterexample => {
                    eprintln!(
                        "{} claim {}: kernel-checked theorem does not provide a counterexample",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::KernelChecked => {
                    println!(
                        "{} claim {}: generated theorem kernel checked with the qualified RC3 toolchain identity and exact axiom report; production authority unchanged",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::Refuted => {
                    println!(
                        "{} claim {}: replayed counterexample retained",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::Blocked(error) => {
                    eprintln!(
                        "{} claim {} blocked: {error:?}",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
                ToolRunStatus::Failed(error) => {
                    eprintln!(
                        "{} claim {} failed: {error:?}",
                        backend_name(tool_backend),
                        claim.id().get()
                    );
                    tool_run_exit(run.status(), counterexample)
                }
            };
            exit = exit.max(code);
        }
    }
    if ran == 0 {
        eprintln!("no compatible claim/backend pair was selected");
        BLOCKED
    } else {
        exit
    }
}

fn run_doctor(path: &Path) -> u8 {
    let manifest = match load_tools_manifest(path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("tools manifest blocked: {error:?}");
            return BLOCKED;
        }
    };
    let mut exit = OK;
    for entry in doctor(&manifest) {
        match entry.result() {
            Ok(identity) => println!(
                "{} {} {}",
                backend_name(entry.backend()),
                identity.version(),
                identity.binary_hash()
            ),
            Err(error) => {
                eprintln!("{} blocked: {error:?}", backend_name(entry.backend()));
                exit = exit.max(failure_exit(error));
            }
        }
    }
    exit
}

fn backend(command: BackendCommand) -> u8 {
    match command {
        BackendCommand::List => {
            println!("cvc5 {CVC5_VERSION}\nz3 {Z3_VERSION}\nlean {LEAN_VERSION}");
            OK
        }
        BackendCommand::Inspect { tools } => match load_tools_manifest(&tools) {
            Ok(manifest) => match manifest.canonical_json() {
                Ok(bytes) => {
                    println!("{}", String::from_utf8_lossy(&bytes));
                    OK
                }
                Err(error) => {
                    eprintln!("manifest encoding failed: {error:?}");
                    FAILURE
                }
            },
            Err(error) => {
                eprintln!("tools manifest blocked: {error:?}");
                BLOCKED
            }
        },
        BackendCommand::InventoryLean { root, format } => {
            let inventory = match inspect_lean_toolchain(&root) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Lean toolchain inventory blocked: {error:?}");
                    return failure_exit(&error);
                }
            };
            match format {
                OutputFormat::Human => {
                    println!(
                        "lean tree_sha256 {}\nfiles {}\ntotal_bytes {}",
                        inventory.tree_sha256(),
                        inventory.files().len(),
                        inventory.total_bytes()
                    );
                    OK
                }
                OutputFormat::Json => match inventory.canonical_json() {
                    Ok(bytes) => {
                        println!("{}", String::from_utf8_lossy(&bytes));
                        OK
                    }
                    Err(error) => {
                        eprintln!("Lean toolchain inventory encoding failed: {error:?}");
                        FAILURE
                    }
                },
            }
        }
        BackendCommand::Verify { tools } => {
            let manifest = match load_tools_manifest(&tools) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("tools manifest blocked: {error:?}");
                    return BLOCKED;
                }
            };
            let mut exit = OK;
            for config in manifest.tools() {
                match verify_tool(config) {
                    Ok(identity) => println!(
                        "{} {}",
                        backend_name(config.backend()),
                        identity.binary_hash()
                    ),
                    Err(error) => {
                        eprintln!("{} blocked: {error:?}", backend_name(config.backend()));
                        exit = exit.max(failure_exit(&error));
                    }
                }
            }
            exit
        }
    }
}

enum ProjectLoad {
    Invalid(DiagnosticSet),
    System(String),
}

fn load_project(path: &Path) -> Result<ProjectSpec, ProjectLoad> {
    let source = fs::read_to_string(path)
        .map_err(|error| ProjectLoad::System(format!("read {}: {error}", path.display())))?;
    let parsed = parse_project(&source, SourceLimits::default()).map_err(ProjectLoad::Invalid)?;
    elaborate_project(parsed, ProjectLimits::default()).map_err(ProjectLoad::Invalid)
}

fn project_or_report(path: &Path, format: OutputFormat) -> Result<ProjectSpec, u8> {
    match load_project(path) {
        Ok(spec) => Ok(spec),
        Err(ProjectLoad::Invalid(set)) => {
            print_diagnostics(path, &set, format);
            Err(INVALID)
        }
        Err(ProjectLoad::System(message)) => {
            eprintln!("{message}");
            Err(FAILURE)
        }
    }
}

fn select_claims<'a>(spec: &'a ProjectSpec, selector: &str) -> Result<Vec<&'a ClaimDecl>, String> {
    if selector == "all" {
        return Ok(spec.claims().iter().collect());
    }
    let raw: u32 = selector
        .parse()
        .map_err(|_| "claim must be a nonzero stable ID or `all`".to_string())?;
    let id = StableId::new(raw).ok_or_else(|| "claim ID must be nonzero".to_string())?;
    spec.claims()
        .iter()
        .find(|claim| claim.id() == id)
        .map(|claim| vec![claim])
        .ok_or_else(|| format!("unknown claim ID {raw}"))
}

fn print_diagnostics(path: &Path, set: &DiagnosticSet, format: OutputFormat) {
    match format {
        OutputFormat::Human => eprintln!("{set}"),
        OutputFormat::Json => print_json(&diagnostic_json(
            path,
            &set.diagnostics().iter().collect::<Vec<_>>(),
            set.is_truncated(),
        )),
    }
}

fn diagnostic_json(path: &Path, diagnostics: &[&Diagnostic], truncated: bool) -> Value {
    let entries: Vec<Value> = diagnostics.iter().map(|item| json!({
        "actual": item.actual(), "ast_path": item.path().as_str(), "code": item.code().as_str(),
        "expected": item.expected(), "remediation": item.remediation(), "span": {
            "column": item.span().column(), "end": item.span().end(), "line": item.span().line(), "start": item.span().start()
        }, "stage": item.stage().as_str()
    })).collect();
    json!({ "diagnostics": entries, "path": path.display().to_string(), "schema": JSON_SCHEMA, "status": "invalid", "truncated": truncated })
}

fn print_json(value: &Value) {
    match serde_json::to_string(value) {
        Ok(encoded) => println!("{encoded}"),
        Err(error) => eprintln!("JSON encoding failed: {error}"),
    }
}

fn backend_name(backend: ToolBackend) -> &'static str {
    match backend {
        ToolBackend::Cvc5 => "cvc5",
        ToolBackend::Z3 => "z3",
        ToolBackend::Lean => "lean",
    }
}

fn failure_exit(error: &ToolFailure) -> u8 {
    match error {
        ToolFailure::Io(_)
        | ToolFailure::Crash(_)
        | ToolFailure::OutputLimit
        | ToolFailure::ProcessContainmentFailed => FAILURE,
        _ => BLOCKED,
    }
}

fn tool_run_exit(status: &ToolRunStatus, counterexample: bool) -> u8 {
    match status {
        ToolRunStatus::ProposedUnsat | ToolRunStatus::Blocked(_) => BLOCKED,
        ToolRunStatus::KernelChecked if counterexample => BLOCKED,
        ToolRunStatus::KernelChecked => OK,
        ToolRunStatus::Refuted if counterexample => OK,
        ToolRunStatus::Refuted => INVALID,
        ToolRunStatus::Failed(_) => FAILURE,
    }
}

fn atomic_create(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if path.exists() && fs::read(path)? == bytes {
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    let temp = path.with_file_name(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

fn io_error(context: &str, error: std::io::Error) -> u8 {
    eprintln!("{context}: {error}");
    FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc3_mini_os_check() {
        for source in [MINIMAL, MINI] {
            let Ok(parsed) = parse_project(source, SourceLimits::default()) else {
                panic!("bundled template did not parse");
            };
            assert!(elaborate_project(parsed, ProjectLimits::default()).is_ok());
        }
    }

    #[test]
    fn rc3_project_new() {
        let target = std::env::temp_dir().join(format!(
            "zeno-fcis-new-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(new_project(&target, Template::Minimal), OK);
        assert!(target.join("project.zeno").is_file());
        assert_eq!(new_project(&target, Template::Minimal), INVALID);
        assert!(fs::remove_file(target.join("project.zeno")).is_ok());
        assert!(fs::remove_file(target.join("README.md")).is_ok());
        assert!(fs::remove_dir(target).is_ok());
    }

    #[test]
    fn exit_classes_are_stable() {
        assert_eq!([OK, INVALID, BLOCKED, FAILURE, USAGE], [0, 1, 2, 3, 64]);
        assert_eq!(failure_exit(&ToolFailure::Timeout), BLOCKED);
        assert_eq!(failure_exit(&ToolFailure::HashMismatch), BLOCKED);
        assert_eq!(
            failure_exit(&ToolFailure::ProcessContainmentFailed),
            FAILURE
        );
        assert_eq!(tool_run_exit(&ToolRunStatus::ProposedUnsat, false), BLOCKED);
        assert_eq!(tool_run_exit(&ToolRunStatus::KernelChecked, false), OK);
        assert_eq!(tool_run_exit(&ToolRunStatus::Refuted, false), INVALID);
        assert_eq!(tool_run_exit(&ToolRunStatus::Refuted, true), OK);
    }

    #[test]
    fn help_and_version_succeed_while_invalid_usage_is_64() {
        for arguments in [["zeno-fcis", "--help"], ["zeno-fcis", "--version"]] {
            let Err(error) = Cli::try_parse_from(arguments) else {
                panic!("display request unexpectedly parsed as a command");
            };
            assert_eq!(clap_error_exit(&error), OK);
        }

        let Err(error) = Cli::try_parse_from(["zeno-fcis", "unknown-command"]) else {
            panic!("invalid command unexpectedly parsed");
        };
        assert_eq!(clap_error_exit(&error), USAGE);
    }

    #[test]
    fn hostile_source_is_inert() {
        let source =
            "zeno 1; project 1 inert; claim 2 x cvc5 relational = $(touch /tmp/pwned) == 1;";
        assert!(parse_project(source, SourceLimits::default()).is_err());
    }
}
