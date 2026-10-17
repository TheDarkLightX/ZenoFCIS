//! Approved local evidence adapter. Recipes are data, never shell commands.
//! LLM-generated.
use crate::transform::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use zeno_fcis_loop_doors::strategy_knowledge as K;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Assertion {
    pointer: String,
    equals: Value,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    arguments: Vec<String>,
    inputs: Vec<Input>,
    expected_exit: u8,
    assertion: Assertion,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StatementFact {
    predicate: String,
    arguments: Vec<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    schema: String,
    runner: String,
    lesson: String,
    family: String,
    mode: String,
    checker_sha256: String,
    statement: Vec<StatementFact>,
    dependencies: Vec<Input>,
    positive: Case,
    negative: Case,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CaseResult {
    exit: u8,
    output: Value,
    output_sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    recipe: Input,
    dependency_sha256: String,
    last_validation: u64,
    positive: CaseResult,
    negative: CaseResult,
}
fn error(reason: &'static str) -> Value {
    json!({"schema":"zeno-fcis/kb-evidence/1","authority":"none","status":"refused","reason":reason})
}
pub(crate) fn read(path: &Path, limit: u64) -> Result<Vec<u8>, Value> {
    let target = std::fs::canonicalize(path).map_err(|_| error("kb-evidence-input-missing"))?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| error("kb-evidence-input-missing"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return Err(error("kb-evidence-input-bound"));
    }
    let data = crate::transform_command::read_bounded(&target, limit)
        .map_err(|_| error("kb-evidence-input-read"))?;
    if data.len() as u64 > limit {
        return Err(error("kb-evidence-input-bound"));
    }
    Ok(data)
}
fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
pub(crate) fn bound_input(root: &Path, input: &Input, limit: u64) -> Result<Vec<u8>, Value> {
    let data = read(&resolve(root, &input.path), limit)?;
    if sha256_hex(&data) != input.sha256 {
        return Err(error("kb-evidence-stale"));
    }
    Ok(data)
}
fn validate_recipe(r: &Recipe) -> Result<(), Value> {
    if r.statement.is_empty()
        || r.statement.len() > 128
        || !matches!(
            r.mode.as_str(),
            "observation" | "exact-precondition" | "reproduced-blocker"
        )
        || r.schema != "zeno-fcis/kb-recipe/1"
        || r.runner != "zeno-fcis-readonly-v1"
        || r.dependencies.is_empty()
        || r.dependencies.len() > 16
    {
        return Err(error("kb-evidence-recipe"));
    }
    for c in [&r.positive, &r.negative] {
        let args = &c.arguments;
        if args.len() < 2
            || args.len() > 32
            || args.iter().any(|s| s.len() > 4096)
            || c.inputs.is_empty()
            || c.inputs.len() > 16
            || c.assertion.pointer.is_empty()
            || !c.assertion.pointer.starts_with('/')
        {
            return Err(error("kb-evidence-case"));
        }
        let allowed = match (args[0].as_str(), args[1].as_str()) {
            ("transform", "check") => {
                args.len() == 6
                    && args[2] == "--original"
                    && !args[3].starts_with('-')
                    && args[4] == "--candidate"
                    && !args[5].starts_with('-')
            }
            ("contract", "review") => {
                (args.len() == 3 || args.len() == 4 && args[3] == "--gaps")
                    && !args[2].starts_with('-')
            }
            _ => false,
        };
        if !allowed {
            return Err(error("kb-evidence-runner-refused"));
        }
        if matches!(
            c.assertion.pointer.as_str(),
            "/authority" | "/schema" | "/version"
        ) {
            return Err(error("kb-evidence-semantic-assertion-required"));
        }
    }
    if r.positive.expected_exit == r.negative.expected_exit
        && r.positive.assertion.pointer == r.negative.assertion.pointer
        && r.positive.assertion.equals == r.negative.assertion.equals
    {
        return Err(error("kb-evidence-negative-required"));
    }
    if r.positive.arguments == r.negative.arguments
        && r.positive.inputs.iter().map(|i| &i.sha256).eq(r
            .negative
            .inputs
            .iter()
            .map(|i| &i.sha256))
    {
        return Err(error("kb-evidence-negative-required"));
    }
    Ok(())
}
pub(crate) fn checker_sha256() -> Result<String, Value> {
    let executable = std::env::current_exe().map_err(|_| error("kb-evidence-checker"))?;
    Ok(sha256_hex(&read(&executable, 134_217_728)?))
}
fn bind_checked_subjects(root: &Path, c: &Case) -> Result<(), Value> {
    let mut required = Vec::new();
    if c.arguments[0] == "transform" {
        required.extend([
            resolve(root, Path::new(&c.arguments[3])),
            resolve(root, Path::new(&c.arguments[5])),
        ]);
    } else {
        let dir = resolve(root, Path::new(&c.arguments[2]));
        required.extend([dir.join("project.zeno"), dir.join("v2/policy.json")]);
        let examples = dir.join("tests/decision-examples.txt");
        if examples.exists() {
            required.push(examples);
        }
    }
    for path in required {
        let target = std::fs::canonicalize(path).map_err(|_| error("kb-evidence-input-missing"))?;
        let mut bound = false;
        for input in &c.inputs {
            if std::fs::canonicalize(resolve(root, &input.path)).is_ok_and(|p| p == target) {
                bound = true;
                break;
            }
        }
        if !bound {
            return Err(error("kb-evidence-subject-unbound"));
        }
    }
    Ok(())
}
fn dependencies(root: &Path, r: &Recipe, checker: Option<&str>) -> Result<String, Value> {
    bind_checked_subjects(root, &r.positive)?;
    bind_checked_subjects(root, &r.negative)?;
    let mut digests = Vec::new();
    for input in r
        .dependencies
        .iter()
        .chain(r.positive.inputs.iter())
        .chain(r.negative.inputs.iter())
    {
        bound_input(root, input, 1_048_576)?;
        digests.push((
            input.path.to_string_lossy().into_owned(),
            input.sha256.clone(),
        ));
    }
    let actual = match checker {
        Some(s) => s.to_owned(),
        None => checker_sha256()?,
    };
    if actual != r.checker_sha256 {
        return Err(error("kb-evidence-checker-stale"));
    }
    digests.push(("checker".into(), actual));
    digests.sort();
    digests.dedup();
    Ok(sha256_hex(
        &serde_json::to_vec(&digests).map_err(|_| error("kb-evidence-encoding"))?,
    ))
}
fn checked(c: &Case, r: &CaseResult) -> bool {
    r.exit == c.expected_exit
        && r.output.pointer(&c.assertion.pointer) == Some(&c.assertion.equals)
        && serde_json::to_vec(&r.output).is_ok_and(|b| sha256_hex(&b) == r.output_sha256)
}
/// Read retained replay results, recheck every dependency, and bind the exact
/// positive and boundary assertions. This does not authenticate a curator.
pub(crate) fn load(
    root: &Path,
    input: &Input,
    checker: Option<&str>,
) -> Result<K::Validation, Value> {
    let bytes = bound_input(root, input, 65_536)?;
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|_| error("kb-evidence-receipt"))?;
    if receipt.schema != "zeno-fcis/kb-validation/1" {
        return Err(error("kb-evidence-schema"));
    }
    let receipt_root = resolve(root, &input.path)
        .parent()
        .ok_or_else(|| error("kb-evidence-parent"))?
        .to_path_buf();
    let recipe_bytes = bound_input(&receipt_root, &receipt.recipe, 65_536)?;
    let r: Recipe =
        serde_json::from_slice(&recipe_bytes).map_err(|_| error("kb-evidence-recipe"))?;
    validate_recipe(&r)?;
    let recipe_root = resolve(&receipt_root, &receipt.recipe.path)
        .parent()
        .ok_or_else(|| error("kb-evidence-parent"))?
        .to_path_buf();
    let dependency = dependencies(&recipe_root, &r, checker)?;
    if dependency != receipt.dependency_sha256
        || !checked(&r.positive, &receipt.positive)
        || !checked(&r.negative, &receipt.negative)
    {
        return Err(error("kb-evidence-replay-mismatch"));
    }
    let mut statement = Vec::new();
    for f in r.statement {
        if f.arguments.len() > 8 || f.predicate.len() > 256 {
            return Err(error("kb-evidence-statement"));
        }
        let arguments = f
            .arguments
            .into_iter()
            .map(|v| match v {
                Value::String(s) => Ok(K::Value::Text(s)),
                Value::Number(n) => n
                    .as_i64()
                    .map(i128::from)
                    .map(K::Value::Integer)
                    .ok_or_else(|| error("kb-evidence-statement")),
                _ => Err(error("kb-evidence-statement")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        statement.push(K::Fact {
            predicate: f.predicate,
            arguments,
        });
    }
    Ok(K::Validation {
        statement,
        lesson: r.lesson,
        recipe_sha256: receipt.recipe.sha256,
        dependency_sha256: dependency,
        validation_sha256: sha256_hex(&bytes),
        mode: r.mode,
        family: r.family,
        last_validation: receipt.last_validation,
        status: K::EvidenceStatus::Validated,
    })
}
fn run_case(root: &Path, c: &Case) -> Result<CaseResult, Value> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    for input in &c.inputs {
        bound_input(root, input, 1_048_576)?;
    }
    let mut process =
        Command::new(std::env::current_exe().map_err(|_| error("kb-evidence-checker"))?)
            .args(&c.arguments)
            .current_dir(root)
            .env_remove("ZENO_FCIS_KB")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| error("kb-evidence-runner-start"))?;
    let stdout = process
        .stdout
        .take()
        .ok_or_else(|| error("kb-evidence-output"))?;
    let stderr = process
        .stderr
        .take()
        .ok_or_else(|| error("kb-evidence-output"))?;
    let output = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.take(65_537).read_to_end(&mut b).map(|_| b)
    });
    let errors = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.take(65_537).read_to_end(&mut b).map(|_| b)
    });
    let started = std::time::Instant::now();
    let status = loop {
        match process.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if started.elapsed().as_millis() < 3000 => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            _ => {
                let _ = process.kill();
                let _ = process.wait();
                break None;
            }
        }
    };
    let bytes = output
        .join()
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| error("kb-evidence-output"))?;
    let diagnostics = errors
        .join()
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| error("kb-evidence-output"))?;
    let status = status.ok_or_else(|| error("kb-evidence-timeout"))?;
    if bytes.len() > 65_536 || diagnostics.len() > 65_536 {
        return Err(error("kb-evidence-output-bound"));
    }
    let output: Value =
        serde_json::from_slice(&bytes).map_err(|_| error("kb-evidence-output-json"))?;
    let output_sha256 =
        sha256_hex(&serde_json::to_vec(&output).map_err(|_| error("kb-evidence-encoding"))?);
    let exit = status
        .code()
        .and_then(|c| u8::try_from(c).ok())
        .ok_or_else(|| error("kb-evidence-exit"))?;
    let result = CaseResult {
        exit,
        output,
        output_sha256,
    };
    if !checked(c, &result) {
        return Err(error("kb-evidence-assertion"));
    }
    Ok(result)
}
/// Explicit CLI replay only; it never changes lesson promotion state.
pub(crate) fn replay(path: &Path, out: &Path) -> (u8, Value) {
    let result = (|| {
        // Retained checker outputs can contain private subject data.
        let parent = std::fs::canonicalize(
            out.parent()
                .ok_or_else(|| error("kb-evidence-output-parent"))?,
        )
        .map_err(|_| error("kb-evidence-output-parent"))?;
        if parent
            .ancestors()
            .any(|p| p.join("crates/zeno-fcis-cli/Cargo.toml").is_file())
        {
            return Err(error("kb-evidence-output-checkout"));
        }
        if out.exists() {
            return Err(error("kb-evidence-output-exists"));
        }
        let recipe = std::fs::canonicalize(path).map_err(|_| error("kb-evidence-input-missing"))?;
        let bytes = read(&recipe, 65_536)?;
        let r: Recipe = serde_json::from_slice(&bytes).map_err(|_| error("kb-evidence-recipe"))?;
        validate_recipe(&r)?;
        let root = recipe.parent().ok_or_else(|| error("kb-evidence-parent"))?;
        let dependency_sha256 = dependencies(root, &r, None)?;
        let positive = run_case(root, &r.positive)?;
        let negative = run_case(root, &r.negative)?;
        // A dependency changing during replay invalidates the entire result.
        if dependencies(root, &r, None)? != dependency_sha256 {
            return Err(error("kb-evidence-stale"));
        }
        let receipt = Receipt {
            schema: "zeno-fcis/kb-validation/1".into(),
            recipe: Input {
                path: recipe,
                sha256: sha256_hex(&bytes),
            },
            dependency_sha256,
            last_validation: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| error("kb-evidence-clock"))?
                .as_secs(),
            positive,
            negative,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|_| error("kb-evidence-encoding"))?;
        if bytes.len() > 65_536 {
            return Err(error("kb-evidence-output-bound"));
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(out)
            .map_err(|_| error("kb-evidence-output-exists"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| error("kb-evidence-output-write"))?;
        Ok(
            json!({"schema":"zeno-fcis/kb-evidence/1","authority":"none","status":"validated","sha256":sha256_hex(&bytes),"dependency_sha256":receipt.dependency_sha256,"last_validation":receipt.last_validation}),
        )
    })();
    match result {
        Ok(v) => (0, v),
        Err(v) => (1, v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Recipe {
        let input = Input {
            path: "original.zcve".into(),
            sha256: "a".repeat(64),
        };
        let case = |candidate: &str, expected: &str| Case {
            arguments: vec![
                "transform".into(),
                "check".into(),
                "--original".into(),
                "original.zcve".into(),
                "--candidate".into(),
                candidate.into(),
            ],
            inputs: vec![input.clone()],
            expected_exit: 0,
            assertion: Assertion {
                pointer: "/verdict".into(),
                equals: json!(expected),
            },
        };
        Recipe {
            schema: "zeno-fcis/kb-recipe/1".into(),
            runner: "zeno-fcis-readonly-v1".into(),
            lesson: "fixture".into(),
            family: "fixture".into(),
            mode: "observation".into(),
            checker_sha256: "a".repeat(64),
            statement: vec![StatementFact {
                predicate: "lesson".into(),
                arguments: vec![json!("fixture"), json!("sample")],
            }],
            dependencies: vec![input.clone()],
            positive: case("same.zcve", "equivalent"),
            negative: case("changed.zcve", "different"),
        }
    }
    #[test]
    fn kb_usage_evidence_requires_semantic_assertions_and_closed_argv() {
        let mut r = recipe();
        assert!(validate_recipe(&r).is_ok());
        r.positive.arguments.push("--receipt".into());
        assert_eq!(
            validate_recipe(&r).err().map(|v| v["reason"].clone()),
            Some(json!("kb-evidence-runner-refused"))
        );
        let mut r = recipe();
        r.positive.assertion.pointer = "/authority".into();
        assert!(validate_recipe(&r).is_err());
        let mut r = recipe();
        r.negative = r.positive.clone();
        assert!(validate_recipe(&r).is_err());
    }
    #[test]
    fn kb_usage_evidence_checks_output_hash_assertion_and_exit_together() {
        let r = recipe();
        let output = json!({"verdict":"equivalent"});
        let hash = sha256_hex(&serde_json::to_vec(&output).unwrap_or_default());
        let mut result = CaseResult {
            exit: 0,
            output,
            output_sha256: hash,
        };
        assert!(checked(&r.positive, &result));
        result.exit = 2;
        assert!(!checked(&r.positive, &result));
        result.exit = 0;
        result.output["verdict"] = json!("different");
        assert!(!checked(&r.positive, &result));
    }
}
