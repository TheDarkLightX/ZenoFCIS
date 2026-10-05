//! Private historical shallow Lean CLI path; no production interface.
//! The argv launcher calls the original prove and retention bodies in a child
//! unit-test process. Success qualifies only this old formula reference.
use crate::spec_authoring::{
    ClaimDecl, ClaimMode, Diagnostic, DiagnosticSet, ProjectLimits, ProjectSpec, SourceLimits,
    StableId, elaborate_project, parse_project,
};
use crate::{
    LEAN_LINUX_X86_64_TREE_SHA256, ToolBackend, ToolFailure, ToolRunStatus, execute_tool,
    export_inductive_smt, export_lean, export_smt, inspect_lean_toolchain, load_tools_manifest,
    retain_run,
};
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::CommitmentHasher as _;
use zeno_fcis_crypto::RustCryptoSha256;
const JSON_SCHEMA: &str = "zeno-fcis/cli/1";
const OK: u8 = 0;
const INVALID: u8 = 1;
const BLOCKED: u8 = 2;
const FAILURE: u8 = 3;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy)]
enum BackendChoice {
    Cvc5,
    Z3,
    Lean,
    All,
}
#[derive(Clone, Copy)]
enum OutputFormat {
    Human,
    Json,
}
enum ProjectLoad {
    Invalid(DiagnosticSet),
    System(String),
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
                ClaimMode::Relational | ClaimMode::Finite { .. } | ClaimMode::Inductive => {
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
            let obligation = match (tool_backend, claim.mode()) {
                (ToolBackend::Cvc5 | ToolBackend::Z3, ClaimMode::Inductive) => {
                    export_inductive_smt(claim, &spec, tool_backend)
                }
                (ToolBackend::Cvc5 | ToolBackend::Z3, _) => export_smt(claim, tool_backend),
                (ToolBackend::Lean, _) => export_lean(claim),
            };
            let obligation = match obligation {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("claim {} export blocked: {error:?}", claim.id().get());
                    exit = exit.max(BLOCKED);
                    continue;
                }
            };
            let scope = obligation.scope();
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
            let scope_note = scope.meaning(run.status());
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
                ToolRunStatus::Undefined(reason) => {
                    println!(
                        "{} claim {}: replayed counterexample retained; the claim has no value there ({})",
                        backend_name(tool_backend),
                        claim.id().get(),
                        reason.name()
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
            let assumptions = claim.assumptions();
            if !assumptions.is_empty() {
                let list = |group: &[StableId]| {
                    group
                        .iter()
                        .map(|law| law.get().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                println!(
                    "{} claim {} assumes laws [{}] on every commit, [{}] on accepts, [{}] on committed failures",
                    backend_name(tool_backend),
                    claim.id().get(),
                    list(assumptions.every_commit()),
                    list(assumptions.accepts()),
                    list(assumptions.committed_failures())
                );
            }
            if let Some(note) = scope_note {
                println!(
                    "{} claim {} scope: {note}",
                    backend_name(tool_backend),
                    claim.id().get()
                );
            }
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

fn project_or_report(path: &Path, format: OutputFormat) -> Result<ProjectSpec, u8> {
    match load_project(path) {
        Ok(spec) => Ok(spec),
        Err(ProjectLoad::Invalid(set)) => {
            print_diagnostics(path, &set, format);
            Err(INVALID)
        }
        Err(ProjectLoad::System(message)) => {
            print_command_error(path, "project-read-failed", &message, format);
            Err(FAILURE)
        }
    }
}

fn load_project(path: &Path) -> Result<ProjectSpec, ProjectLoad> {
    let limits = SourceLimits::default();
    let source = fs::File::open(path)
        .and_then(|file| read_project_source(file, limits))
        .map_err(|error| ProjectLoad::System(format!("read {}: {error}", path.display())))?;
    let parsed = parse_project(&source, limits).map_err(ProjectLoad::Invalid)?;
    elaborate_project(parsed, ProjectLimits::default()).map_err(ProjectLoad::Invalid)
}

fn read_project_source(reader: impl Read, limits: SourceLimits) -> std::io::Result<String> {
    let read_limit =
        u64::try_from(limits.max_bytes().saturating_add(1)).map_err(std::io::Error::other)?;
    let mut bytes = Vec::new();
    reader.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > limits.max_bytes() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "source byte limit exceeded (maximum {} bytes)",
                limits.max_bytes()
            ),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
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
    json!({ "authority": "none", "runtime_admission": "not-run", "diagnostics": entries, "path": path.display().to_string(), "schema": JSON_SCHEMA, "status": "invalid", "truncated": truncated })
}

fn print_command_error(path: &Path, code: &str, message: &str, format: OutputFormat) {
    match format {
        OutputFormat::Human => eprintln!("{message}"),
        OutputFormat::Json => print_json(&json!({
            "authority": "none", "runtime_admission": "not-run",
            "error": {"code": code, "message": message},
            "path": path.display().to_string(),
            "schema": JSON_SCHEMA,
            "status": "error"
        })),
    }
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
        | ToolFailure::Timeout
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
        ToolRunStatus::Refuted | ToolRunStatus::Undefined(_) if counterexample => OK,
        ToolRunStatus::Refuted | ToolRunStatus::Undefined(_) => INVALID,
        ToolRunStatus::Failed(_) => FAILURE,
    }
}

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-cli-{label}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create temp root: {error}"));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("CLI crate is outside the repository"))
        .to_path_buf()
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .unwrap_or_else(|error| panic!("run CLI: {error}"))
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read test file: {error}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes).to_string()
}

fn evidence_directory(project: &Path) -> PathBuf {
    let root = project
        .parent()
        .unwrap_or_else(|| panic!("project has no parent"))
        .join(".zeno-fcis/evidence");
    let directories = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("read evidence root: {error}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("read evidence entry: {error}"))
                .path()
        })
        .collect::<Vec<_>>();
    assert_eq!(directories.len(), 1);
    directories[0].clone()
}

fn assert_retained(directory: &Path, names: &[&str]) {
    for name in names {
        assert!(directory.join(name).is_file(), "missing retained {name}");
    }
}

fn cli() -> &'static str {
    static PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        use std::os::unix::fs::PermissionsExt as _;
        let path = std::env::temp_dir().join(format!("zeno-fcis-private-lean-cli-{}", std::process::id()));
        let exe = std::env::current_exe().unwrap_or_else(|error| panic!("private executable: {error}"));
        let executable_json = serde_json::to_string(&exe.to_string_lossy()).unwrap_or_else(|error| panic!("private path: {error}"));
        let script = format!("#!/usr/bin/python3\nimport json,os,subprocess,sys\ne=dict(os.environ)\ne['ZENO_FCIS_PRIVATE_CLI_ARGS']=json.dumps(sys.argv[1:])\np=subprocess.run([{executable_json},'--ignored','--exact','tests::process_helper_original_lean_cli','--nocapture'],env=e)\nsys.exit(p.returncode)\n");
        fs::write(&path, script).unwrap_or_else(|error| panic!("private launcher: {error}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap_or_else(|error| panic!("private launcher mode: {error}"));
        path
    }).to_str().unwrap_or_else(|| panic!("private launcher path"))
}

pub(super) fn dispatch(argv: &[String]) -> u8 {
    let [
        command,
        project,
        claim_key,
        claim,
        backend_key,
        backend,
        tools_key,
        tools,
    ] = argv
    else {
        return BLOCKED;
    };
    if command != "prove"
        || claim_key != "--claim"
        || backend_key != "--backend"
        || backend != "lean"
        || tools_key != "--tools"
    {
        return BLOCKED;
    }
    prove(
        Path::new(project),
        claim,
        BackendChoice::Lean,
        Path::new(tools),
        false,
    )
}

#[test]
#[cfg(unix)]
#[ignore = "requires the workflow-pinned Lean 4.30.0 Linux x86-64 distribution"]
fn pinned_lean_cli_prove_is_process_level() {
    let lean = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| panic!("missing pinned Lean")),
    );
    let lean_root = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN_ROOT")
            .unwrap_or_else(|| panic!("missing pinned Lean root")),
    );
    let inventory = inspect_lean_toolchain(&lean_root)
        .unwrap_or_else(|error| panic!("inventory pinned Lean: {error:?}"));
    assert_eq!(
        inventory.tree_sha256().to_string(),
        LEAN_LINUX_X86_64_TREE_SHA256
    );
    let executable = fs::read(&lean).unwrap_or_else(|error| panic!("read pinned Lean: {error}"));
    let root = TempRoot::new("pinned-lean-process");
    let tools = root.path().join("tools.json");
    fs::write(
        &tools,
        serde_json::to_vec(&json!({
            "format": "zeno-fcis/tools/2",
            "tools": [{
                "backend": "lean",
                "path": lean,
                "version": "4.30.0",
                "sha256": sha256_hex(&executable),
                "runtime": {
                    "root": lean_root,
                    "tree_sha256": LEAN_LINUX_X86_64_TREE_SHA256
                },
                "timeout_ms": 30_000,
                "max_output_bytes": 1_048_576,
                "allowed_axioms": ["Quot.sound", "propext"]
            }]
        }))
        .unwrap_or_else(|_| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("write Lean manifest: {error}"));
    let project = root.path().join("project.zeno");
    fs::copy(
        repository_root().join("examples/mini-determinator/project.zeno"),
        &project,
    )
    .unwrap_or_else(|error| panic!("copy Mini Determinator project: {error}"));
    let output = run(Command::new(cli())
        .arg("prove")
        .arg(&project)
        .args(["--claim", "501", "--backend", "lean", "--tools"])
        .arg(&tools));
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("kernel checked"));
    let evidence = evidence_directory(&project);
    assert_retained(
        &evidence,
        &[
            "formal-run-record.bin",
            "record.json",
            "source",
            "toolchain.json",
            "transcript-01-kernel-input",
            "transcript-01-kernel-stdout",
        ],
    );
}

#[test]
fn private_original_cli_process_preserves_missing_tool_refusal() {
    let root = TempRoot::new("private-missing-lean");
    let tools = root.path().join("tools.json");
    fs::write(&tools, br#"{"format":"zeno-fcis/tools/2","tools":[]}"#)
        .unwrap_or_else(|error| panic!("tools: {error}"));
    let output = run(Command::new(cli())
        .arg("prove")
        .arg(repository_root().join("examples/mini-determinator/project.zeno"))
        .args(["--claim", "501", "--backend", "lean", "--tools"])
        .arg(&tools));
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("lean is absent from tools manifest"));
}
