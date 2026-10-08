//! `zeno-fcis contract evolve`: replaces an application's contract and
//! regenerates its lineage. The pure functions in `crate::contract` classify
//! the change and compute the owner's review, the plain-language account of
//! `contract diff`; this module only reads and writes files, and writes
//! nothing unless the whole new lineage generated, which runs every check
//! again.
//!
//! Three kinds of change take this path:
//! - a rule change without `--migration`, a behaviour change: the schemas
//!   and channels are unchanged. A store follows with the application's
//!   `--upgrade`, which the SQLite shell admits only when every state law of
//!   the new contract and every inductive claim it declares hold on the
//!   store's state. Genesis exactness, law 990, applies only to new stores
//!   and is not checked there.
//! - a rename, without `--migration`: only names differ, and a store follows
//!   at any state, its state framed again under the new names.
//! - a layout change, or a rule change, with `--migration m.json`: a data
//!   migration, admitted only by forward simulation over the old version's
//!   whole declared input domain, here and in every later generation and
//!   store upgrade. `--shortcut` adds a migration directly from an earlier
//!   version, checked against the composed route.
//!
//! Any other kind is refused with its name.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::json;

use crate::contract::{
    ContractError, ContractSources, EVOLUTION_MIGRATION, EVOLUTION_REVIEW, Evolution,
    EvolutionKind, EvolutionPath, EvolutionSources, GeneratedContract, Shortcut,
    evolution_directory, evolve_change, generate_contract, listed_evolutions, shortcut_file,
    with_evolution, without_evolutions,
};
use crate::contract_files::{
    EXAMPLES, Failure, Inputs, PROJECT, RULES, invalid, outputs, read_input, report_failure,
    summary_json, write_output,
};
use crate::transform::sha256_hex;
use crate::{FAILURE, JSON_SCHEMA, OK, OutputFormat, atomic_create, atomic_replace, print_json};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Replace the application's contract: classify the change as `contract diff` does; take a rule change as a reviewed behaviour change, a rename as an exact rename, and a layout or rule change with --migration as a data migration admitted by forward simulation; refuse any other kind; keep the replaced contract and the plain-language diff under v2/evolutions/N, and regenerate the lineage. A store follows with the application's --upgrade.
    Evolve {
        /// Application directory holding project.zeno and v2/policy.json.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The new contract: a directory holding project.zeno, v2/policy.json with no adoptions or evolutions, and optionally tests/decision-examples.txt, which replaces the application's examples.
        #[arg(long)]
        to: PathBuf,
        /// A data migration, zeno-fcis/migration/1, from the application's state to the new contract's: every new state field carried from an old one, given a default, or mapped from an old one's values. Admitted only by forward simulation over the whole declared input domain, at most 2^20 tuples.
        #[arg(long)]
        migration: Option<PathBuf>,
        /// A shortcut: a migration file with from_version k, from version k directly to the new contract. Every step since version k must be a migration or rename, and the shortcut must agree with their composed route. Repeatable; needs --migration.
        #[arg(long)]
        shortcut: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

pub(super) fn run(command: Command) -> u8 {
    match command {
        Command::Evolve {
            dir,
            to,
            migration,
            shortcut,
            format,
        } => evolve(&dir, &to, migration.as_deref(), &shortcut, format),
    }
}

/// Everything an evolution writes, computed before any write.
struct EvolutionPlan {
    ordinal: usize,
    directory: String,
    /// The replaced contract and the review, by path in the application.
    retained: Vec<(String, Vec<u8>)>,
    /// The new `project.zeno`, when it differs.
    project: Option<String>,
    /// The new `v2/policy.json`.
    rules: String,
    /// The new contract's examples, when it has them.
    examples: Option<Vec<u8>>,
    /// The application's adoption directory, which moves into the evolution's.
    moved_adoptions: bool,
    review: String,
    /// The classifier's kind of the change.
    classified: &'static str,
    evolution: Evolution,
    from_version: u32,
    generated: GeneratedContract,
}

fn evolve(
    dir: &Path,
    to: &Path,
    migration: Option<&Path>,
    shortcuts: &[PathBuf],
    format: OutputFormat,
) -> u8 {
    let plan = match plan(dir, to, migration, shortcuts) {
        Ok(plan) => plan,
        Err(failure) => return report_failure(dir, failure, format),
    };
    match write(dir, &plan) {
        Ok(artifacts) => report(dir, &plan, &artifacts, format),
        Err((name, error)) => {
            let message = format!("write {name}: {error}");
            match format {
                OutputFormat::Human => eprintln!("{message}"),
                OutputFormat::Json => print_json(&json!({
                    "schema": JSON_SCHEMA, "status": "error", "path": dir.display().to_string(),
                    "authority": "none",
                    "error": {"code": "evolution-write-failed", "place": name, "message": message}
                })),
            }
            FAILURE
        }
    }
}

/// A refusal about the new contract names `--to`.
fn new_side(error: ContractError) -> Failure {
    invalid(
        &format!("--to {}", error.place()),
        error.reason().to_owned(),
    )
}

/// `error` from generating the planned lineage, with a place inside the
/// planned evolution's migration or one of its shortcuts named as the file
/// given on the command line: on a refusal nothing is kept at that path.
fn given_place(
    error: ContractError,
    directory: &str,
    migration: Option<&Path>,
    shortcuts: &[(u32, &Path)],
) -> Failure {
    let place = error.place();
    let files = migration
        .map(|path| (EVOLUTION_MIGRATION.to_owned(), "--migration", path))
        .into_iter()
        .chain(
            shortcuts
                .iter()
                .map(|(from, path)| (shortcut_file(*from), "--shortcut", *path)),
        );
    for (kept, flag, path) in files {
        if let Some(rest) = place.strip_prefix(&format!("{directory}/{kept}")) {
            return invalid(
                &format!("{flag} {}{rest}", path.display()),
                error.reason().to_owned(),
            );
        }
    }
    error.into()
}

/// The bytes of a file given on the command line.
fn given(flag: &str, path: &Path) -> Result<Vec<u8>, Failure> {
    read_input(path)
        .map_err(|error| Failure::Read(format!("read {flag} {}: {error}", path.display())))
}

/// The version a shortcut file starts at, as its `from_version` says; the
/// generator validates the file.
fn from_version(bytes: &[u8]) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    u32::try_from(value.get("from_version")?.as_u64()?).ok()
}

/// Reads both contracts, classifies the change, computes the review and
/// generates the whole new lineage, which recomputes and checks the review,
/// and simulates a migration, once more.
fn plan(
    dir: &Path,
    to: &Path,
    migration: Option<&Path>,
    shortcut_paths: &[PathBuf],
) -> Result<EvolutionPlan, Failure> {
    let migration_path = migration;
    let migration = migration
        .map(|path| given("--migration", path))
        .transpose()?;
    if migration.is_none() && !shortcut_paths.is_empty() {
        return Err(invalid(
            "--shortcut",
            "a shortcut needs --migration: it is checked against the route that ends with the new migration",
        ));
    }
    let mut shortcut_files: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut shortcut_given: Vec<(u32, &Path)> = Vec::new();
    for path in shortcut_paths {
        let bytes = given("--shortcut", path)?;
        let from = from_version(&bytes).ok_or_else(|| {
            invalid(
                &format!("--shortcut {}", path.display()),
                "must be a migration file with a from_version, the earlier version it starts at",
            )
        })?;
        if shortcut_files.iter().any(|(earlier, _)| *earlier == from) {
            return Err(invalid(
                &format!("--shortcut {}", path.display()),
                format!("is a second shortcut from version {from}"),
            ));
        }
        shortcut_files.push((from, bytes));
        shortcut_given.push((from, path));
    }
    shortcut_files.sort_by_key(|(from, _)| *from);
    let app = Inputs::read(dir)?;
    let new = Inputs::read(to)?;
    let app_adoptions = app.adoption_sources();
    let app_evolutions = app.evolution_sources();
    let app_sources = app.sources(&app.rules, &app_adoptions, &app_evolutions);
    let app_generated = generate_contract(app_sources)?;
    let (new_adoptions, new_evolutions) = (new.adoption_sources(), new.evolution_sources());
    let new_sources = new.sources(&new.rules, &new_adoptions, &new_evolutions);
    // The new contract alone first, so that its own refusal names it.
    generate_contract(new_sources).map_err(new_side)?;
    // The classifier runs first: a change no path takes is refused with its
    // name.
    let (review, path, classified) = evolve_change(
        app_sources,
        &app_generated,
        new_sources,
        migration.is_some(),
    )?;
    if !new.adoptions.is_empty() || !new.evolutions.is_empty() {
        return Err(invalid(
            "--to",
            "the new contract lists adoptions or evolutions of its own; evolve to the contract \
             without them, then adopt in the application",
        ));
    }
    let ordinal = app.evolutions.len() + 1;
    let directory = evolution_directory(ordinal);
    // Unlike `exists`, `symlink_metadata` also sees a dangling link.
    if dir.join(&directory).symlink_metadata().is_ok() {
        return Err(invalid(
            &directory,
            "already exists, but the rules list fewer evolutions: an interrupted evolution left \
             it; remove it and run `contract evolve` again",
        ));
    }
    let archived_rules = without_evolutions(&app.rules)?;
    let kind = match (path, &migration) {
        (EvolutionPath::Migration, Some(bytes)) => EvolutionKind::Migration {
            migration_sha256: sha256_hex(bytes),
            shortcuts: shortcut_files
                .iter()
                .map(|(from, bytes)| Shortcut {
                    from_version: *from,
                    sha256: sha256_hex(bytes),
                })
                .collect(),
        },
        (EvolutionPath::Rename, _) => EvolutionKind::Rename,
        _ => EvolutionKind::BehaviourChange,
    };
    let evolution = Evolution {
        superseded_policy_sha256: sha256_hex(app_generated.policy()),
        review_sha256: sha256_hex(review.as_bytes()),
        kind,
    };
    let rules = with_evolution(&new.rules, &listed_evolutions(&app.rules)?, &evolution)
        .map_err(new_side)?;
    let mut planned_evolutions = app_evolutions.clone();
    planned_evolutions.push(EvolutionSources {
        project: &app.project,
        rules: &archived_rules,
        adoptions: app_adoptions.clone(),
        review: review.as_bytes(),
        migration: migration.as_deref(),
        shortcuts: shortcut_files
            .iter()
            .map(|(_, bytes)| bytes.as_slice())
            .collect(),
    });
    let generated = generate_contract(ContractSources {
        project: &new.project,
        rules: &rules,
        schema_origin: app.origin.as_deref(),
        adoptions: &[],
        replayed: &[],
        evolutions: &planned_evolutions,
    })
    .map_err(|error| given_place(error, &directory, migration_path, &shortcut_given))?;
    let base = format!("{directory}/");
    let mut retained = vec![
        (format!("{base}{PROJECT}"), app.project.clone().into_bytes()),
        (
            format!("{base}{RULES}"),
            archived_rules.clone().into_bytes(),
        ),
    ];
    let moved: Vec<(String, Vec<u8>)> = app
        .retained()
        .into_iter()
        .filter(|(name, _)| name.starts_with("v2/adoptions/"))
        .collect();
    let moved_adoptions = !moved.is_empty();
    retained.extend(
        moved
            .into_iter()
            .map(|(name, bytes)| (format!("{base}{name}"), bytes)),
    );
    retained.push((
        format!("{base}{EVOLUTION_REVIEW}"),
        review.clone().into_bytes(),
    ));
    if let Some(bytes) = &migration {
        retained.push((format!("{base}{EVOLUTION_MIGRATION}"), bytes.clone()));
    }
    for (from, bytes) in &shortcut_files {
        retained.push((format!("{base}{}", shortcut_file(*from)), bytes.clone()));
    }
    let examples = match read_input(&to.join(EXAMPLES)) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(Failure::Read(format!("read --to {EXAMPLES}: {error}")));
        }
    };
    Ok(EvolutionPlan {
        ordinal,
        directory,
        retained,
        project: (new.project != app.project).then(|| new.project.clone()),
        rules,
        examples,
        moved_adoptions,
        review,
        classified,
        evolution,
        from_version: app_generated.summary().version,
        generated,
    })
}

/// Writes the replaced contract and the review first, then the generated
/// lineage, the examples, `project.zeno` and the rules, and last removes the
/// application's adoption directory, which the evolution now keeps. An
/// interruption before the rules leaves the application at its contract,
/// with an evolution directory `contract evolve` refuses until it is
/// removed; one between `project.zeno` and the rules leaves the two
/// inconsistent, and generation refuses them until they are restored.
fn write(dir: &Path, plan: &EvolutionPlan) -> Result<Vec<String>, (String, std::io::Error)> {
    let mut artifacts = Vec::new();
    for (name, bytes) in &plan.retained {
        let path = dir.join(name);
        path.parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| atomic_create(&path, bytes))
            .map_err(|error| (name.clone(), error))?;
        artifacts.push(name.clone());
    }
    for (name, bytes) in outputs(&plan.generated) {
        write_output(&dir.join(&name), bytes).map_err(|error| (name.clone(), error))?;
        artifacts.push(name);
    }
    if let Some(examples) = &plan.examples {
        write_output(&dir.join(EXAMPLES), examples)
            .map_err(|error| (EXAMPLES.to_owned(), error))?;
        artifacts.push(EXAMPLES.to_owned());
    }
    if let Some(project) = &plan.project {
        atomic_replace(&dir.join(PROJECT), project.as_bytes())
            .map_err(|error| (PROJECT.to_owned(), error))?;
        artifacts.push(PROJECT.to_owned());
    }
    atomic_replace(&dir.join(RULES), plan.rules.as_bytes())
        .map_err(|error| (RULES.to_owned(), error))?;
    artifacts.push(RULES.to_owned());
    if plan.moved_adoptions {
        let adoptions = "v2/adoptions";
        fs::remove_dir_all(dir.join(adoptions)).map_err(|error| (adoptions.to_owned(), error))?;
    }
    Ok(artifacts)
}

fn report(dir: &Path, plan: &EvolutionPlan, artifacts: &[String], format: OutputFormat) -> u8 {
    let summary = plan.generated.summary();
    let last = summary.evolutions.last();
    let claims = last
        .map(|evolution| evolution.claims.clone())
        .unwrap_or_default();
    let lines: Vec<&str> = plan.review.lines().collect();
    let path = plan.evolution.kind.name();
    let migration = last.and_then(|evolution| evolution.migration.as_ref());
    match format {
        OutputFormat::Json => {
            let mut evolution = json!({
                "ordinal": plan.ordinal, "directory": plan.directory,
                "from_version": plan.from_version, "version": summary.version,
                "path": path,
                "superseded_policy_sha256": plan.evolution.superseded_policy_sha256,
                "review_sha256": plan.evolution.review_sha256,
                "claims": claims,
                "review_path": format!("{}/{EVOLUTION_REVIEW}", plan.directory),
            });
            if let (serde_json::Value::Object(fields), Some(migration)) =
                (&mut evolution, migration)
            {
                fields.insert(
                    "migration".to_owned(),
                    json!({
                        "sha256": migration.sha256,
                        "path": format!("{}/{EVOLUTION_MIGRATION}", plan.directory),
                        "states": migration.states[0],
                        "states_satisfying_state_laws": migration.states[1],
                        "genesis_states": migration.states[2],
                        "tuples_compared": migration.tuples,
                        "observations": ["genesis", "new-state-laws", "decision-class-and-reason", "deliveries", "successor-state"],
                        "shortcuts": migration.shortcuts.iter().map(|(from, tuples)| json!({
                            "from_version": from, "tuples_compared": tuples
                        })).collect::<Vec<_>>(),
                    }),
                );
            }
            print_json(&json!({
                "schema": JSON_SCHEMA, "status": "evolved", "path": dir.display().to_string(),
                "authority": "none", "evidence": "generated-contract", "catalog_binding": "checked",
                "kind": plan.classified,
                "evolution": evolution,
                "review": lines,
                "artifacts": artifacts,
                "summary": summary_json(&plan.generated)
            }));
        }
        OutputFormat::Human => {
            for line in &lines {
                println!("{line}");
            }
            if let Some(migration) = migration {
                println!(
                    "the migration was admitted by forward simulation: {} input tuples compared over the {} of {} declared states on which the old contract's state laws hold, {} of them genesis states; genesis, decision class and reason, deliveries and successor state all agreed",
                    migration.tuples, migration.states[1], migration.states[0], migration.states[2]
                );
            }
            println!(
                "evolved {} from contract version {} to version {} in {} as a {path}; the review is {}/{EVOLUTION_REVIEW} (SHA-256 {}); wrote {}",
                summary.application,
                plan.from_version,
                summary.version,
                dir.display(),
                plan.directory,
                plan.evolution.review_sha256,
                artifacts.join(", ")
            );
        }
    }
    OK
}
