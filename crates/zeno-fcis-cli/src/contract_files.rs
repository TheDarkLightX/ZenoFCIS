//! Application contracts on disk.
//!
//! `zeno-fcis generate contract` reads an application's `project.zeno`,
//! `v2/policy.json` and the retained files of every adoption it lists, and
//! writes or checks `v2/schema.zcve`, `src/v2_contract.rs`, `v2/policy.zcve`
//! and each superseded version's `src/v2_contract_v{k}.rs` and
//! `v2/policy_v{k}.zcve`. `zeno-fcis new --contract` builds a new application
//! around them, and `zeno-fcis contract adopt` records one more adoption.
//! Generation is the pure `crate::contract::generate_contract`; this module
//! only reads and writes files.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

use serde_json::{Value, json};

use crate::contract::{
    AdoptionSources, ContractError, ContractSources, GeneratedContract, adoption_directory,
    generate_contract,
};
use crate::{
    FAILURE, INVALID, JSON_SCHEMA, OK, OutputFormat, artifact_is_current, atomic_create,
    atomic_replace, print_json,
};

/// Largest input file read; real declarations and rules are far smaller.
const INPUT_LIMIT: u64 = 16 * 1024 * 1024;

pub(crate) const PROJECT: &str = "project.zeno";
pub(crate) const RULES: &str = "v2/policy.json";
pub(crate) const SCHEMA_ORIGIN: &str = "v2/schema-origin.json";
const EXAMPLES: &str = "tests/decision-examples.txt";
const SCHEMA: &str = "v2/schema.zcve";
const SOURCE: &str = "src/v2_contract.rs";
const POLICY: &str = "v2/policy.zcve";
/// An adoption's retained candidate and receipt, inside `adoption_directory`.
pub(crate) const ADOPTED_PROGRAM: &str = "program.zcve";
pub(crate) const ADOPTED_RECEIPT: &str = "receipt.json";

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
pub(crate) enum Failure {
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

pub(crate) fn invalid(place: &str, reason: impl std::fmt::Display) -> Failure {
    Failure::Invalid {
        place: place.to_owned(),
        reason: reason.to_string(),
    }
}

/// The retained files of one adoption, read from disk.
pub(crate) struct AdoptionFiles {
    pub(crate) candidate: Vec<u8>,
    pub(crate) receipt: Vec<u8>,
}

/// The inputs a contract is generated from, as read from `dir`.
pub(crate) struct Inputs {
    pub(crate) project: String,
    pub(crate) rules: String,
    pub(crate) origin: Option<String>,
    pub(crate) adoptions: Vec<AdoptionFiles>,
}

impl Inputs {
    pub(crate) fn read(dir: &Path) -> Result<Self, Failure> {
        let project = read_text(dir, PROJECT)?;
        let rules = read_text(dir, RULES)?;
        let origin = match read_input(&dir.join(SCHEMA_ORIGIN)) {
            Ok(bytes) => Some(text(SCHEMA_ORIGIN, bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(format!("read {SCHEMA_ORIGIN}: {error}"))),
        };
        // The generator validates the rules; this only counts the adoptions
        // whose retained files it must be given.
        let count = serde_json::from_str::<Value>(&rules)
            .ok()
            .and_then(|rules| Some(rules.get("adoptions")?.as_array()?.len()))
            .unwrap_or(0);
        let adoptions = (1..=count)
            .map(|ordinal| {
                let directory = adoption_directory(ordinal);
                Ok(AdoptionFiles {
                    candidate: read_file(dir, &format!("{directory}/{ADOPTED_PROGRAM}"))?,
                    receipt: read_file(dir, &format!("{directory}/{ADOPTED_RECEIPT}"))?,
                })
            })
            .collect::<Result<_, Failure>>()?;
        Ok(Self {
            project,
            rules,
            origin,
            adoptions,
        })
    }

    pub(crate) fn sources<'a>(
        &'a self,
        rules: &'a str,
        adoptions: &'a [AdoptionSources<'a>],
    ) -> ContractSources<'a> {
        ContractSources {
            project: &self.project,
            rules,
            schema_origin: self.origin.as_deref(),
            adoptions,
            replayed: &[],
        }
    }

    pub(crate) fn adoption_sources(&self) -> Vec<AdoptionSources<'_>> {
        self.adoptions
            .iter()
            .map(|files| AdoptionSources {
                candidate: &files.candidate,
                receipt: &files.receipt,
            })
            .collect()
    }

    pub(crate) fn generate(&self) -> Result<GeneratedContract, Failure> {
        let adoptions = self.adoption_sources();
        Ok(generate_contract(self.sources(&self.rules, &adoptions))?)
    }
}

/// The generated files in report order: the three every contract has, then
/// each superseded version's source and policy.
pub(crate) fn outputs(generated: &GeneratedContract) -> Vec<(String, &[u8])> {
    let mut files = vec![
        (SCHEMA.to_owned(), generated.schema()),
        (SOURCE.to_owned(), generated.source().as_bytes()),
        (POLICY.to_owned(), generated.policy()),
    ];
    for (index, previous) in generated.previous().iter().enumerate() {
        let version = index + 1;
        files.push((
            format!("src/v2_contract_v{version}.rs"),
            previous.source().as_bytes(),
        ));
        files.push((format!("v2/policy_v{version}.zcve"), previous.policy()));
    }
    files
}

/// `zeno-fcis generate contract`.
pub(crate) fn run(dir: &Path, check_only: bool, format: OutputFormat) -> u8 {
    let generated = match Inputs::read(dir).and_then(|inputs| inputs.generate()) {
        Ok(generated) => generated,
        Err(failure) => return report_failure(dir, failure, format),
    };
    let files = outputs(&generated);
    let mut drift = Vec::new();
    for (name, bytes) in &files {
        let path = dir.join(name);
        let result = if check_only {
            artifact_is_current(&path, bytes).map(|current| {
                if !current {
                    drift.push(name.as_str());
                }
            })
        } else {
            write_output(&path, bytes)
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

pub(crate) fn write_output(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    path.parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| atomic_replace(path, bytes))
}

/// `zeno-fcis new <dir> --contract <contract>`: an application built from
/// `project.zeno`, `v2/policy.json`, the adoptions it lists and, when
/// present, `tests/decision-examples.txt` in `contract`. `dir` exists and is
/// empty.
pub(crate) fn scaffold(dir: &Path, contract: &Path) -> u8 {
    let result = (|| {
        let inputs = Inputs::read(contract)?;
        let examples = match read_input(&contract.join(EXAMPLES)) {
            Ok(bytes) => text(EXAMPLES, bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => NO_EXAMPLES.to_owned(),
            Err(error) => return Err(Failure::Read(format!("read {EXAMPLES}: {error}"))),
        };
        let generated = inputs.generate()?;
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
        let mut files: Vec<(String, Vec<u8>)> = vec![
            ("Cargo.toml".to_owned(), manifest.into_bytes()),
            ("README.md".to_owned(), readme.into_bytes()),
            (PROJECT.to_owned(), inputs.project.clone().into_bytes()),
            (RULES.to_owned(), inputs.rules.clone().into_bytes()),
            (EXAMPLES.to_owned(), examples.into_bytes()),
        ];
        for (index, adoption) in inputs.adoptions.iter().enumerate() {
            let directory = adoption_directory(index + 1);
            files.push((
                format!("{directory}/{ADOPTED_PROGRAM}"),
                adoption.candidate.clone(),
            ));
            files.push((
                format!("{directory}/{ADOPTED_RECEIPT}"),
                adoption.receipt.clone(),
            ));
        }
        files.extend(
            outputs(&generated)
                .into_iter()
                .map(|(name, bytes)| (name, bytes.to_vec())),
        );
        files.extend(
            APPLICATION
                .iter()
                .map(|(name, source)| ((*name).to_owned(), source.as_bytes().to_vec())),
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

/// The summary a report carries.
pub(crate) fn summary_json(generated: &GeneratedContract) -> Value {
    let summary = generated.summary();
    json!({
        "application": summary.application,
        "version": summary.version,
        "program_nodes": summary.program_nodes, "outputs": summary.outputs,
        "law_nodes": summary.law_nodes, "law_ids": summary.law_ids,
        "schema_types": summary.schema_types, "policy_bytes": generated.policy().len(),
        "budgets": {"read": summary.read_budget, "step": summary.step_budget, "byte": summary.byte_budget},
        "adoptions": summary.adoptions.iter().map(|adoption| json!({
            "candidate_sha256": adoption.candidate_sha256,
            "receipt_sha256": adoption.receipt_sha256,
            "usage": adoption.usage.name(),
            "usage_preserved": adoption.usage_preserved,
            "program_nodes": {"before": adoption.program_nodes[0], "after": adoption.program_nodes[1]},
            "premises": {
                "decision_conformance_law": true, "receipt_equivalent": true,
                "step_limits_never_bind": adoption.steps.never_binds(),
                "steps": {
                    "program": {"before": adoption.steps.program[0], "after": adoption.steps.program[1]},
                    "laws": adoption.steps.laws,
                    "limit": {"before": adoption.steps.limits[0], "after": adoption.steps.limits[1]}
                }
            }
        })).collect::<Vec<_>>()
    })
}

fn report(
    dir: &Path,
    check_only: bool,
    drift: &[&str],
    files: &[(String, &[u8])],
    generated: &GeneratedContract,
    format: OutputFormat,
) {
    let status = match (check_only, drift.is_empty()) {
        (false, _) => "generated",
        (true, true) => "current",
        (true, false) => "drift",
    };
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    match format {
        OutputFormat::Json => {
            print_json(&json!({
                "schema": JSON_SCHEMA, "status": status, "path": dir.display().to_string(),
                "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
                "artifacts": names, "drift": drift,
                "summary": summary_json(generated)
            }));
        }
        OutputFormat::Human => match status {
            "generated" => println!("generated {} in {}", names.join(", "), dir.display()),
            "current" => println!("contract is current in {}", dir.display()),
            _ => eprintln!("contract drift in {}: {}", dir.display(), drift.join(", ")),
        },
    }
}

pub(crate) fn report_failure(dir: &Path, failure: Failure, format: OutputFormat) -> u8 {
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
    text(name, read_file(dir, name)?)
}

pub(crate) fn read_file(dir: &Path, name: &str) -> Result<Vec<u8>, Failure> {
    read_input(&dir.join(name)).map_err(|error| Failure::Read(format!("read {name}: {error}")))
}

fn text(name: &str, bytes: Vec<u8>) -> Result<String, Failure> {
    String::from_utf8(bytes).map_err(|error| Failure::Read(format!("read {name}: {error}")))
}

/// Reads a regular file of at most `INPUT_LIMIT` bytes.
pub(crate) fn read_input(path: &Path) -> std::io::Result<Vec<u8>> {
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
