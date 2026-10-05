//! Application contracts on disk.
//!
//! `zeno-fcis generate contract` reads an application's `project.zeno` and
//! `v2/policy.json`, and writes or checks `v2/schema.zcve`,
//! `src/v2_contract.rs` and `v2/policy.zcve`. `zeno-fcis new --contract`
//! builds a new application around them. Generation is the pure
//! `crate::contract::generate_contract`; this module only reads and writes
//! files.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

use serde_json::json;

use crate::contract::{ContractError, ContractSources, GeneratedContract, generate_contract};
use crate::{
    FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, artifact_is_current, atomic_create,
    atomic_replace, print_json,
};

/// Largest input file read; real declarations and rules are far smaller.
const INPUT_LIMIT: u64 = 16 * 1024 * 1024;

const PROJECT: &str = "project.zeno";
const RULES: &str = "v2/policy.json";
const SCHEMA_ORIGIN: &str = "v2/schema-origin.json";
const EXAMPLES: &str = "tests/decision-examples.txt";
const SCHEMA: &str = "v2/schema.zcve";
const SOURCE: &str = "src/v2_contract.rs";
const POLICY: &str = "v2/policy.zcve";

/// The source every application built from a contract shares.
const APPLICATION: &[(&str, &str)] = &[
    ("src/lib.rs", include_str!("../contract-app/src/lib.rs")),
    (
        "src/session.rs",
        include_str!("../contract-app/src/session.rs"),
    ),
    ("src/main.rs", include_str!("../contract-app/src/main.rs")),
    (
        "tests/decisions.rs",
        include_str!("../contract-app/tests/decisions.rs"),
    ),
];
const MANIFEST: &str = include_str!("../contract-app/Cargo.toml.in");
const README: &str = include_str!("../contract-app/README.md.in");
/// The examples file of an application whose contract supplies none.
const NO_EXAMPLES: &str = include_str!("../contract-app/decision-examples.txt");

/// Why no files were produced.
enum Failure {
    /// An input could not be read: exit `FAILURE`.
    Read(String),
    /// The contract is invalid: exit `INVALID`.
    Invalid { place: String, reason: String },
}

impl From<ContractError> for Failure {
    fn from(error: ContractError) -> Self {
        Self::Invalid {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        }
    }
}

fn invalid(place: &str, reason: impl std::fmt::Display) -> Failure {
    Failure::Invalid {
        place: place.to_owned(),
        reason: reason.to_string(),
    }
}

/// The schema, contract source and policy for `project.zeno` and the rules.
fn generate(
    project: &str,
    rules: &str,
    origin: Option<&str>,
) -> Result<GeneratedContract, Failure> {
    Ok(generate_contract(ContractSources {
        project,
        rules,
        schema_origin: origin,
    })?)
}

/// `zeno-fcis generate contract`.
pub(crate) fn run(dir: &Path, check_only: bool, format: OutputFormat) -> u8 {
    let result = (|| {
        let project = read_text(dir, PROJECT)?;
        let rules = read_text(dir, RULES)?;
        let origin = match read_input(&dir.join(SCHEMA_ORIGIN)) {
            Ok(bytes) => Some(text(SCHEMA_ORIGIN, bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(format!("read {SCHEMA_ORIGIN}: {error}"))),
        };
        generate(&project, &rules, origin.as_deref())
    })();
    let generated = match result {
        Ok(generated) => generated,
        Err(failure) => return report_failure(dir, failure, format),
    };
    let files = [
        (SCHEMA, generated.schema()),
        (SOURCE, generated.source().as_bytes()),
        (POLICY, generated.policy()),
    ];
    let mut drift = Vec::new();
    for (name, bytes) in files {
        let path = dir.join(name);
        let result = if check_only {
            artifact_is_current(&path, bytes).map(|current| {
                if !current {
                    drift.push(name);
                }
            })
        } else {
            path.parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| atomic_replace(&path, bytes))
        };
        if let Err(error) = result {
            let action = if check_only { "read" } else { "write" };
            let message = format!("{action} {name}: {error}");
            return report_failure(dir, Failure::Read(message), format);
        }
    }
    report(dir, check_only, &drift, &files, &generated, format);
    if drift.is_empty() { OK } else { INVALID }
}

/// `zeno-fcis new <dir> --contract <contract>`: an application built from
/// `project.zeno`, `v2/policy.json` and, when present,
/// `tests/decision-examples.txt` in `contract`. `dir` exists and is empty.
pub(crate) fn scaffold(dir: &Path, contract: &Path) -> u8 {
    let result = (|| {
        let project = read_text(contract, PROJECT)?;
        let rules = read_text(contract, RULES)?;
        let examples = match read_input(&contract.join(EXAMPLES)) {
            Ok(bytes) => text(EXAMPLES, bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => NO_EXAMPLES.to_owned(),
            Err(error) => return Err(Failure::Read(format!("read {EXAMPLES}: {error}"))),
        };
        let generated = generate(&project, &rules, None)?;
        let package = generated.summary().application.clone();
        let valid_package = package.len() <= 64
            && package.starts_with(|first: char| first.is_ascii_lowercase())
            && package
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !valid_package {
            return Err(invalid(
                RULES,
                format!(
                    "template `{package}` is not a package name: lowercase letters, digits and `-`"
                ),
            ));
        }
        let manifest = MANIFEST
            .replace("{package}", &package)
            .replace("{version}", env!("CARGO_PKG_VERSION"));
        let readme = README.replace("{package}", &package);
        let mut files: Vec<(&str, Vec<u8>)> = vec![
            ("Cargo.toml", manifest.into_bytes()),
            ("README.md", readme.into_bytes()),
            (PROJECT, project.into_bytes()),
            (RULES, rules.into_bytes()),
            (EXAMPLES, examples.into_bytes()),
            (SCHEMA, generated.schema().to_vec()),
            (SOURCE, generated.source().as_bytes().to_vec()),
            (POLICY, generated.policy().to_vec()),
        ];
        files.extend(
            APPLICATION
                .iter()
                .map(|(name, source)| (*name, source.as_bytes().to_vec())),
        );
        Ok(files)
    })();
    let files = match result {
        Ok(files) => files,
        Err(failure) => return report_failure(dir, failure, OutputFormat::Human),
    };
    for (name, bytes) in &files {
        let path = dir.join(name);
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| atomic_create(&path, bytes));
        if let Err(error) = written {
            eprintln!("write {name}: {error}");
            return FAILURE;
        }
    }
    println!("created {}", dir.display());
    OK
}

fn report(
    dir: &Path,
    check_only: bool,
    drift: &[&str],
    files: &[(&str, &[u8])],
    generated: &GeneratedContract,
    format: OutputFormat,
) {
    let status = match (check_only, drift.is_empty()) {
        (false, _) => "generated",
        (true, true) => "current",
        (true, false) => "drift",
    };
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    match format {
        OutputFormat::Json => {
            let summary = generated.summary();
            print_json(&json!({
                "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
                "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
                "artifacts": names, "drift": drift,
                "summary": {
                    "application": summary.application,
                    "program_nodes": summary.program_nodes, "outputs": summary.outputs,
                    "law_nodes": summary.law_nodes, "law_ids": summary.law_ids,
                    "schema_types": summary.schema_types, "policy_bytes": generated.policy().len(),
                    "budgets": {"read": summary.read_budget, "step": summary.step_budget, "byte": summary.byte_budget}
                }
            }));
        }
        OutputFormat::Human => match status {
            "generated" => println!("generated {} in {}", names.join(", "), dir.display()),
            "current" => println!("contract is current in {}", dir.display()),
            _ => eprintln!("contract drift in {}: {}", dir.display(), drift.join(", ")),
        },
    }
}

fn report_failure(dir: &Path, failure: Failure, format: OutputFormat) -> u8 {
    let (code, place, message, exit) = match failure {
        Failure::Read(message) => ("contract-read-failed", None, message, FAILURE),
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

/// Reads a regular file of at most `INPUT_LIMIT` bytes.
fn read_input(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A FIFO can block during open, before the file-type check below.
        options.custom_flags(nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(INPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > INPUT_LIMIT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("larger than {INPUT_LIMIT} bytes"),
        ));
    }
    Ok(bytes)
}
