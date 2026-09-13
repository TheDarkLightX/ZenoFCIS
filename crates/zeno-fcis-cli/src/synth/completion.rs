//! Inert, replayable finite completion cases; never publication authority.

use super::{Failure, Result, hex, problem, sha};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use zeno_fcis_codec::DecodeError;
use zeno_fcis_synthesis::finite::completion::{
    CompletionError, CompletionLimits, CompletionProblem, VerifiedCompletion, find_completion,
    verify_completion, verify_completion_bytes,
};
use zeno_fcis_synthesis::finite::{Domain, Error, MAX_INPUTS, MAX_STEPS, PROFILE};

const SCHEMA: &str = "zeno-fcis/completion-problem/1";
const RESULT_SCHEMA: &str = "zeno-fcis/completion-result/1";

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Describe the closed source grammar, commands, limits and finite claim.
    Discover,
    /// Find and independently verify exits; retain an inert success or failure case.
    Find {
        /// Independently chosen completion problem.
        problem: PathBuf,
        /// New directory; existing paths, including empty directories, are rejected.
        #[arg(long)]
        out: PathBuf,
    },
    /// Verify portable canonical plan bytes against an independently chosen problem.
    Verify {
        problem: PathBuf,
        #[arg(long)]
        plan: PathBuf,
    },
    /// Recompute a case from the supplied problem and compare its complete file set.
    Replay {
        problem: PathBuf,
        #[arg(long)]
        case: PathBuf,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema: String,
    profile: String,
    state: Vec<problem::Field>,
    commands: Vec<problem::Field>,
    step: problem::Graph,
    terminal: problem::Graph,
    #[serde(default)]
    limits: Limits,
}

#[derive(Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Limits {
    max_transitions: u64,
    max_steps: u64,
    max_state_bytes: u64,
    max_command_bytes: u64,
    max_policy_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        let limits = CompletionLimits::default();
        Self {
            max_transitions: limits.max_transitions,
            max_steps: limits.max_steps,
            max_state_bytes: limits.max_state_bytes,
            max_command_bytes: limits.max_command_bytes,
            max_policy_bytes: limits.max_policy_bytes,
        }
    }
}
impl Limits {
    fn core(&self) -> CompletionLimits {
        CompletionLimits {
            max_transitions: self.max_transitions,
            max_steps: self.max_steps,
            max_state_bytes: self.max_state_bytes,
            max_command_bytes: self.max_command_bytes,
            max_policy_bytes: self.max_policy_bytes,
        }
    }
}

struct Loaded {
    bytes: Vec<u8>,
    names: Value,
    model: std::result::Result<CompletionProblem, CompletionError>,
}
struct PreparedCase {
    files: BTreeMap<&'static str, Vec<u8>>,
    report: Value,
    exit: u8,
}

fn fail(exit: u8, status: &str, detail: Value) -> Failure {
    Failure {
        exit,
        report: json!({"schema": RESULT_SCHEMA, "status": status,
        "authority": "none", "detail": detail}),
    }
}
fn io(error: std::io::Error) -> Failure {
    // Portable failure records contain no filesystem paths or host error text.
    fail(
        crate::FAILURE,
        "io-error",
        json!({"kind": format!("{:?}", error.kind())}),
    )
}
fn invalid(message: String) -> Failure {
    fail(
        crate::INVALID,
        "invalid-problem",
        json!({"message": message}),
    )
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| fail(crate::FAILURE, "encoding-error", json!({})))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn run(command: Command) -> u8 {
    let result = match command {
        Command::Discover => Ok((
            crate::OK,
            json!({
                "schema": "zeno-fcis/completion-discovery/1", "authority": "none",
                "problem_schema": SCHEMA, "profile": PROFILE,
                "plan_schema": "zeno-fcis/completion-plan/1",
                "commands": ["find PROBLEM --out NEW_DIRECTORY", "verify PROBLEM --plan PLAN", "replay PROBLEM --case DIRECTORY"],
                "source_fields": ["schema", "profile", "state", "commands", "step", "terminal", "limits"],
                "field_example": {"name": "count", "type": {"kind": "int", "min": 0, "max": 3}},
                "boolean_field_type": {"kind": "bool"},
                "instructions": problem::OPERATIONS.iter().map(|(name, arity)| json!({"opcode": name, "wire_items": arity})).collect::<Vec<_>>(),
                "step": {"inputs": "state followed by commands", "outputs": "accepted Bool followed by successor state"},
                "terminal": {"inputs": "state", "outputs": "terminal Bool"},
                "default_limits": Limits::default(),
                "hard_limits": {"source_bytes": problem::MAX_BYTES, "transitions": MAX_INPUTS, "steps": MAX_STEPS},
                "plan_input_limit": "derived from the independently supplied problem",
                "claim": "every declared state has a selected accepted finite path to terminal under the fixed model",
                "nonclaims": ["unbounded temporal proof", "scheduler fairness", "context provenance", "application authorization", "external effects or publication"],
                "replay": "matched replay preserves the recomputed claim exit code"
            }),
        )),
        Command::Find { problem, out } => find(&problem, &out),
        Command::Verify { problem, plan } => verify(&problem, &plan),
        Command::Replay { problem, case } => replay(&problem, &case),
    };
    let (exit, report) = match result {
        Ok(result) => result,
        Err(failure) => (failure.exit, failure.report),
    };
    crate::print_json(&report);
    exit
}

fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() {
        return Err(fail(crate::INVALID, "not-regular-file", json!({})));
    }
    if metadata.len() > limit {
        return Err(fail(
            crate::BLOCKED,
            "byte-limit",
            json!({"required": metadata.len(), "declared": limit}),
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err(fail(crate::INVALID, "not-regular-file", json!({})));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(io)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(fail(
            crate::BLOCKED,
            "byte-limit",
            json!({"required": bytes.len(), "declared": limit}),
        ));
    }
    Ok(bytes)
}

fn load(path: &Path) -> Result<Loaded> {
    let source: Source = serde_json::from_slice(&read_file(path, problem::MAX_BYTES)?)
        .map_err(|error| invalid(format!("problem-json: {error}")))?;
    if source.schema != SCHEMA || source.profile != PROFILE {
        return Err(invalid("unsupported-profile-or-schema".into()));
    }
    let state = problem::fields(&source.state).map_err(invalid)?;
    let commands = problem::fields(&source.commands).map_err(invalid)?;
    let step = source
        .step
        .program(
            [state.clone(), commands].concat(),
            [vec![Domain::Bool], state.clone()].concat(),
        )
        .map_err(invalid)?;
    let terminal = source
        .terminal
        .program(state, vec![Domain::Bool])
        .map_err(invalid)?;
    let names = json!({
        "state": source.state.iter().map(|field| &field.name).collect::<Vec<_>>(),
        "commands": source.commands.iter().map(|field| &field.name).collect::<Vec<_>>()
    });
    Ok(Loaded {
        bytes: encode(&source)?,
        names,
        model: CompletionProblem::try_new(step, terminal, source.limits.core()),
    })
}

fn verified_report(model: &CompletionProblem, checked: &VerifiedCompletion, bytes: &[u8]) -> Value {
    json!({"schema": RESULT_SCHEMA, "status": "verified", "authority": "none",
        "profile": PROFILE, "problem": hex(model.problem_hash()),
        "states_checked": model.state_count(), "commands_per_state": model.command_count(),
        "maximum_exit_steps": checked.plan().steps.iter().map(|row| row.remaining).max().unwrap_or(0),
        "plan_sha256": sha(bytes), "assurance": "complete-finite"})
}

fn prepare(loaded: Loaded) -> Result<PreparedCase> {
    let binding = loaded
        .model
        .as_ref()
        .ok()
        .map(|model| hex(model.problem_hash()));
    let outcome = loaded.model.and_then(|model| {
        let proposed = find_completion(&model)?;
        let checked = verify_completion(&model, &proposed)?;
        let bytes = checked.canonical_bytes()?;
        Ok((verified_report(&model, &checked, &bytes), bytes))
    });
    let mut files = BTreeMap::new();
    let (exit, mut report) = match outcome {
        Ok((report, bytes)) => {
            files.insert("plan.zcve", bytes);
            (crate::OK, report)
        }
        Err(error) => {
            let failure = semantic(error);
            (failure.exit, failure.report)
        }
    };
    report["problem"] = json!(binding);
    report["source_sha256"] = json!(sha(&loaded.bytes));
    report["fields"] = loaded.names;
    files.insert("problem.json", loaded.bytes);
    files.insert("result.json", encode(&report)?);
    Ok(PreparedCase {
        files,
        report,
        exit,
    })
}

fn find(path: &Path, out: &Path) -> Result<(u8, Value)> {
    let prepared = prepare(load(path)?)?;
    fs::create_dir(out).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            fail(crate::INVALID, "output-exists", json!({}))
        } else {
            io(error)
        }
    })?;
    let result = prepared
        .files
        .iter()
        .try_for_each(|(name, bytes)| crate::atomic_create(&out.join(name), bytes));
    if let Err(error) = result {
        // The exclusive create_dir above established ownership of this output.
        fs::remove_dir_all(out).map_err(io)?;
        return Err(io(error));
    }
    Ok((prepared.exit, prepared.report))
}

fn verify(path: &Path, plan: &Path) -> Result<(u8, Value)> {
    let loaded = load(path)?;
    let model = loaded.model.map_err(semantic)?;
    let bytes = read_file(plan, model.plan_byte_limit())?;
    let checked = verify_completion_bytes(&model, &bytes).map_err(semantic)?;
    let mut report = verified_report(&model, &checked, &bytes);
    report["source_sha256"] = json!(sha(&loaded.bytes));
    report["fields"] = loaded.names;
    Ok((crate::OK, report))
}

fn replay(path: &Path, case: &Path) -> Result<(u8, Value)> {
    let mut prepared = prepare(load(path)?)?;
    if !fs::symlink_metadata(case).map_err(io)?.is_dir() {
        return Err(fail(crate::INVALID, "not-case-directory", json!({})));
    }
    let mut count = 0;
    for entry in fs::read_dir(case).map_err(io)? {
        let entry = entry.map_err(io)?;
        count += 1;
        let name = entry.file_name();
        if count > prepared.files.len()
            || !entry.file_type().map_err(io)?.is_file()
            || !name
                .to_str()
                .is_some_and(|name| prepared.files.contains_key(name))
        {
            return Err(fail(
                crate::INVALID,
                "artifact-drift",
                json!({"reason": "file-set"}),
            ));
        }
    }
    if count != prepared.files.len() {
        return Err(fail(
            crate::INVALID,
            "artifact-drift",
            json!({"reason": "file-set"}),
        ));
    }
    for (name, expected) in &prepared.files {
        let limit = u64::try_from(expected.len())
            .map_err(|_| fail(crate::FAILURE, "encoding-error", json!({})))?;
        let actual = read_file(&case.join(name), limit).map_err(|failure| {
            if failure.exit == crate::BLOCKED {
                fail(crate::INVALID, "artifact-drift", json!({"artifact": name}))
            } else {
                failure
            }
        })?;
        if actual != *expected {
            return Err(fail(
                crate::INVALID,
                "artifact-drift",
                json!({"artifact": name}),
            ));
        }
    }
    prepared.report["replay"] = json!("matched");
    Ok((prepared.exit, prepared.report))
}

fn semantic(error: CompletionError) -> Failure {
    match error {
        CompletionError::Resource(Error::Budget {
            resource,
            required,
            declared,
        }) => fail(
            crate::BLOCKED,
            "resource-limit",
            json!({"resource": resource, "required": required, "declared": declared}),
        ),
        CompletionError::Resource(Error::Limit(resource)) => fail(
            crate::BLOCKED,
            "resource-limit",
            json!({"resource": resource}),
        ),
        CompletionError::NoExit { state } => {
            fail(crate::INVALID, "no-exit", json!({"state": state}))
        }
        CompletionError::Evaluation { input, source } => fail(
            crate::INVALID,
            "evaluation-trap",
            json!({"input": input, "error": source.to_string()}),
        ),
        CompletionError::RejectedStateChange { input } => fail(
            crate::INVALID,
            "rejected-state-change",
            json!({"input": input}),
        ),
        CompletionError::InvalidPlan { reason, state } => fail(
            crate::INVALID,
            "invalid-plan",
            json!({"reason": reason, "state": state}),
        ),
        CompletionError::Decoding(error) => decoded(error),
        CompletionError::Encoding(error) => fail(
            crate::FAILURE,
            "encoding-error",
            json!({"message": error.to_string()}),
        ),
        CompletionError::Invalid(reason) => invalid(reason.into()),
        CompletionError::Resource(error) => invalid(error.to_string()),
    }
}

fn decoded(error: DecodeError) -> Failure {
    let capacity = match &error {
        DecodeError::InputLimit { limit, actual } => Some(("bytes", *actual, *limit)),
        DecodeError::BlobLimit { limit, attempted } => Some(("blob-bytes", *attempted, *limit)),
        DecodeError::NodeLimit { limit, attempted } => Some(("nodes", *attempted, *limit)),
        DecodeError::PayloadLimit { limit, attempted } => {
            Some(("payload-bytes", *attempted, *limit))
        }
        DecodeError::DepthLimit { limit, attempted } => {
            Some(("depth", u64::from(*attempted), u64::from(*limit)))
        }
        DecodeError::CollectionLimit { limit, attempted } => {
            Some(("collection-items", u64::from(*attempted), u64::from(*limit)))
        }
        _ => None,
    };
    if let Some((resource, required, declared)) = capacity {
        fail(
            crate::BLOCKED,
            "resource-limit",
            json!({"resource": resource, "required": required, "declared": declared}),
        )
    } else {
        fail(
            crate::INVALID,
            "invalid-plan-bytes",
            json!({"message": error.to_string()}),
        )
    }
}
