//! Language-neutral synthesis authoring; no commit or publication authority.
mod problem;
mod runner;

use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::emit::{PythonEmitter, RustEmitter, TargetEmitter};
use zeno_fcis_synthesis::finite::{
    Budget, Case, Contract, Error, Outcome, PROFILE, Program, Witness, synthesize,
};

#[derive(Subcommand)]
pub(super) enum Command {
    /// List semantic profiles, target adapters, bounds, and evidence stages.
    Discover,
    /// Synthesize a complete finite relation and emit a pure target module.
    Run {
        #[command(flatten)]
        selection: Selection,
        /// Recompute and compare the complete artifact set without writing it.
        #[arg(long)]
        check: bool,
    },
    /// Recompute artifact bindings and replay every input using the target tool.
    Verify {
        #[command(flatten)]
        selection: Selection,
        /// Explicit compiler/interpreter path; Rust must be the qualified pin.
        #[arg(long)]
        tool: Option<PathBuf>,
        /// Create a separate conformance receipt without overwriting a file.
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
}
#[derive(Args)]
pub(super) struct Selection {
    #[arg(default_value = "synthesis.json")]
    problem: PathBuf,
    #[arg(long, default_value = "rust")]
    target: String,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 100_000)]
    max_assignments: u64,
    #[arg(long, default_value_t = 100_000_000)]
    max_steps: u64,
}

struct Target {
    emitter: &'static dyn TargetEmitter,
    runner: &'static dyn runner::TargetRunner,
}
fn targets() -> [Target; 2] {
    [
        Target {
            emitter: &RustEmitter,
            runner: &runner::RustRunner,
        },
        Target {
            emitter: &PythonEmitter,
            runner: &runner::PythonRunner,
        },
    ]
}
fn target(name: &str) -> Option<Target> {
    targets()
        .into_iter()
        .find(|target| target.emitter.target().language == name)
}
struct Prepared {
    files: BTreeMap<String, Vec<u8>>,
    manifest: Value,
    source: String,
    program: Program,
    contract: Contract,
    cases: Vec<Case>,
    target: Target,
}
struct Failure {
    exit: u8,
    report: Value,
}
type Result<T> = std::result::Result<T, Failure>;
fn fail(exit: u8, code: &str, detail: Value) -> Failure {
    Failure {
        exit,
        report: json!({"schema":"zeno-fcis/synthesis-result/1","status":code,"authority":"none","detail":detail}),
    }
}
fn io(error: std::io::Error) -> Failure {
    fail(
        crate::FAILURE,
        "io-error",
        json!({"message":error.to_string()}),
    )
}
fn semantic(error: Error) -> Failure {
    match error {
        Error::Budget {
            resource,
            required,
            declared,
        } => fail(
            crate::BLOCKED,
            "incomplete",
            json!({"resource":resource,"required":required,"declared":declared}),
        ),
        Error::Limit(resource) => fail(
            crate::BLOCKED,
            "limit-exceeded",
            json!({"resource":resource}),
        ),
        Error::ContractTrap { input, output } => fail(
            crate::INVALID,
            "contract-trap",
            json!({"input":input,"output":output}),
        ),
        Error::Search(
            zeno_fcis_synthesis::SynthesisError::CheckerIndeterminate
            | zeno_fcis_synthesis::SynthesisError::MissingAcceptanceEvidence,
        ) => fail(
            crate::BLOCKED,
            "indeterminate",
            json!({"message":error.to_string()}),
        ),
        _ => fail(
            crate::INVALID,
            "invalid-problem",
            json!({"message":error.to_string()}),
        ),
    }
}
pub(super) fn run(command: Command) -> u8 {
    let result = match command {
        Command::Discover => Ok(
            json!({"schema":"zeno-fcis/synthesis-discovery/1","authority":"none","profiles":[PROFILE],"problem_schema":problem::SCHEMA,"targets":targets().iter().map(|target|json!({"language":target.emitter.target().language,"revision":target.emitter.target().revision,"compiler":target.runner.tool_requirement(),"pure_module":true})).collect::<Vec<_>>(),"instructions":problem::OPERATIONS.iter().map(|(name,arity)|json!({"opcode":name,"wire_items":arity})).collect::<Vec<_>>(),"contract_environment":"input fields followed by output fields","example_command":"zeno-fcis new counter --template durable-counter","limits":{"source_bytes":problem::MAX_BYTES,"fields_per_side":16,"nodes_per_graph":256,"input_tuples":65536,"output_tuples":4096,"graph_steps":100000000},"runtime_platform":"linux", "stages":["realizability","canonical-search","emission","target-conformance"],"semantics":{"evaluation":"eager","arithmetic":"checked-i64","boolean_wire":[0,1],"temporal":"unsupported","effects":"output-data-only"}}),
        ),
        Command::Run { selection, check } => author(&selection, check),
        Command::Verify {
            selection,
            tool,
            receipt,
        } => verify(&selection, tool.as_deref(), receipt.as_deref()),
    };
    match result {
        Ok(report) => {
            crate::print_json(&report);
            crate::OK
        }
        Err(failure) => {
            crate::print_json(&failure.report);
            failure.exit
        }
    }
}
fn prepare(selection: &Selection) -> Result<Prepared> {
    let target = target(&selection.target).ok_or_else(|| {
        fail(
            crate::BLOCKED,
            "unsupported-target",
            json!({"target":selection.target,"next":"synth discover"}),
        )
    })?;
    let problem = problem::read(&selection.problem).map_err(|failure| match failure {
        problem::LoadError::Io(error) => io(error),
        problem::LoadError::Invalid(message) => fail(
            crate::INVALID,
            "invalid-problem",
            json!({"message":message}),
        ),
    })?;
    let outcome = synthesize(
        &problem.contract,
        &problem.sketch,
        Budget {
            max_assignments: selection.max_assignments,
            max_steps: selection.max_steps,
        },
    )
    .map_err(semantic)?;
    let (program, certificate, cases, witness) = match outcome {
        Outcome::Selected {
            program,
            certificate,
            cases,
            first_counterexample,
        } => (program, certificate, cases, first_counterexample),
        Outcome::Unrealizable { input } => {
            return Err(fail(crate::INVALID, "unrealizable", json!({"input":input})));
        }
        Outcome::NoSolution {
            certificate,
            first_counterexample,
        } => {
            return Err(fail(
                crate::INVALID,
                "no-solution",
                json!({"assignments_evaluated":certificate.evaluated(),"certificate":hex(certificate.commitment().map_err(|e|semantic(Error::Search(e)))?),"first_counterexample":witness_json(first_counterexample)}),
            ));
        }
    };
    let source = target.emitter.emit(&program).map_err(semantic)?;
    let mut files = BTreeMap::new();
    files.insert("problem.json".into(), problem.bytes);
    files.insert(
        "program.zcve".into(),
        program.value().canonical_bytes().map_err(|e| {
            fail(
                crate::FAILURE,
                "encoding-error",
                json!({"message":e.to_string()}),
            )
        })?,
    );
    files.insert(
        format!("transition.{}", target.emitter.target().extension),
        source.as_bytes().to_vec(),
    );
    files.insert("vectors.json".into(),json_bytes(&json!({"schema":"zeno-fcis/synthesis-vectors/1","cases":cases.iter().map(|c|json!({"input":c.input,"output":c.output})).collect::<Vec<_>>()}))?);
    let manifest = json!({"schema":"zeno-fcis/synthesis-artifact/1","status":"selected","authority":"none","profile":PROFILE,"target":{"language":target.emitter.target().language,"revision":target.emitter.target().revision},"emitter_sha256":hex(target.emitter.identity()),"abi":problem.names,"contract":hex(problem.contract.commitment().map_err(semantic)?),"certificate":hex(certificate.commitment().map_err(|e|semantic(Error::Search(e)))?),"assignments_evaluated":certificate.evaluated(),"input_coverage":cases.len(),"trace":hex(certificate.trace_hash()),"first_counterexample":witness_json(witness),"runtime_conformance":"not-run","files":files.iter().map(|(name,bytes)|json!({"path":name,"bytes":bytes.len(),"sha256":sha(bytes)})).collect::<Vec<_>>()});
    files.insert("manifest.json".into(), json_bytes(&manifest)?);
    Ok(Prepared {
        files,
        manifest,
        source,
        program,
        contract: problem.contract,
        cases,
        target,
    })
}
fn author(selection: &Selection, check: bool) -> Result<Value> {
    let prepared = prepare(selection)?;
    if check {
        check_files(&selection.out, &prepared.files)?;
    } else {
        if selection.out.exists() {
            if fs::read_dir(&selection.out).map_err(io)?.next().is_some() {
                return Err(fail(
                    crate::INVALID,
                    "nonempty-output",
                    json!({"path":selection.out}),
                ));
            }
        } else {
            fs::create_dir(&selection.out).map_err(io)?;
        }
        for (name, bytes) in &prepared.files {
            crate::atomic_create(&selection.out.join(name), bytes).map_err(io)?;
        }
    }
    Ok(
        json!({"schema":"zeno-fcis/synthesis-result/1","status":if check {"current"}else{"selected"},"authority":"none","semantic":"complete-finite","runtime_conformance":"not-run","manifest":prepared.manifest}),
    )
}
fn verify(selection: &Selection, tool: Option<&Path>, receipt: Option<&Path>) -> Result<Value> {
    let prepared = prepare(selection)?;
    check_files(&selection.out, &prepared.files)?;
    if let Some(receipt) = receipt {
        let parent = fs::canonicalize(
            receipt
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )
        .map_err(io)?;
        if parent.starts_with(fs::canonicalize(&selection.out).map_err(io)?) || receipt.exists() {
            return Err(fail(
                crate::INVALID,
                "invalid-receipt-path",
                json!({"message":"choose a new receipt file outside the immutable artifact directory"}),
            ));
        }
    }
    let observation = prepared
        .target
        .runner
        .run(&prepared.source, &prepared.cases, tool)
        .map_err(|error| {
            fail(
                crate::BLOCKED,
                "conformance-unknown",
                json!({"code":error.code,"message":error.message}),
            )
        })?;
    runner::validate(&prepared.contract, &prepared.cases, &observation.stdout).map_err(
        |error| {
            fail(
                crate::INVALID,
                "conformance-failed",
                json!({"code":error.code,"message":error.message}),
            )
        },
    )?;
    // The record binds actual captured outputs and the exact regenerated source.
    let report = json!({"schema":"zeno-fcis/synthesis-conformance/1","status":"passed","authority":"none","profile":PROFILE,"target":prepared.target.emitter.target().language,"manifest_sha256":sha(&prepared.files["manifest.json"]),"certificate":prepared.manifest["certificate"],"contract":prepared.manifest["contract"],"program_sha256":sha(&prepared.files["program.zcve"]),"source_sha256":sha(prepared.source.as_bytes()),"emitter_sha256":hex(prepared.target.emitter.identity()),"fixture_sha256":observation.fixture_sha256,"execution_artifact_sha256":observation.execution_artifact_sha256,"tool":observation.tool,"inputs_checked":prepared.cases.len(),"outputs_per_input":prepared.program.outputs().len(),"stdout_sha256":sha(&observation.stdout),"claim":"exact emitted function agrees with the interpreter and reviewed relation on every admitted input"});
    if let Some(path) = receipt {
        crate::atomic_create(path, &json_bytes(&report)?).map_err(io)?;
    }
    Ok(report)
}
fn check_files(out: &Path, expected: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let mut actual = Vec::new();
    for entry in fs::read_dir(out).map_err(io)? {
        let entry = entry.map_err(io)?;
        if !entry.file_type().map_err(io)?.is_file() {
            return Err(fail(
                crate::INVALID,
                "artifact-drift",
                json!({"path":entry.path()}),
            ));
        }
        actual.push(entry.file_name().into_string().map_err(|_| {
            fail(
                crate::INVALID,
                "artifact-drift",
                json!({"message":"non-UTF8 filename"}),
            )
        })?);
    }
    actual.sort();
    if actual != expected.keys().cloned().collect::<Vec<_>>() {
        return Err(fail(
            crate::INVALID,
            "artifact-drift",
            json!({"message":"artifact file set differs"}),
        ));
    }
    for (name, bytes) in expected {
        // Bound reads before comparing. Execute only the regenerated memory copy.
        let path = out.join(name);
        let mut actual = Vec::new();
        fs::File::open(&path)
            .map_err(io)?
            .take(bytes.len() as u64 + 1)
            .read_to_end(&mut actual)
            .map_err(io)?;
        if actual != *bytes {
            return Err(fail(crate::INVALID, "artifact-drift", json!({"path":name})));
        }
    }
    Ok(())
}
fn witness_json(witness: Option<Witness>) -> Value {
    witness.map_or(Value::Null, |w| json!({"input":w.input,"output":w.output}))
}
fn json_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        fail(
            crate::FAILURE,
            "encoding-error",
            json!({"message":e.to_string()}),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(super) fn hex(hash: Hash32) -> String {
    hash.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn sha(bytes: &[u8]) -> String {
    hex(RustCryptoSha256::hash(bytes))
}

#[cfg(test)]
mod tests;
