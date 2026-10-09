//! Mutable authoring workspace. Supplied labels never grant publication authority.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use clap::Subcommand;
use serde_json::{Value, json};

use crate::contract::draft::{Draft, SCHEMA};
use crate::{INVALID, OK, atomic_create, atomic_replace, contract_files, print_json};

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Draft rules from supplied intent and explicitly labeled examples; no hosted model.
    Draft {
        #[command(subcommand)]
        command: Operation,
    },
}

#[derive(Subcommand)]
pub(crate) enum Operation {
    /// Create a tooling session with fixed declarations and work limits.
    Start {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        intent: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        examples: Option<PathBuf>,
        /// Supplied provenance, not authenticated owner identity.
        #[arg(long)]
        provenance: String,
        #[arg(long, default_value_t = 4)]
        rounds: u8,
        #[arg(long, default_value_t = 4096)]
        max_tuples: u64,
    },
    /// Spend a round on complete rules; invalid proposals remain in the transcript.
    Propose {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        rules: PathBuf,
        #[arg(long)]
        provenance: String,
    },
    /// Show F2's advisory packet and distinguishing inputs needing labels.
    Questions {
        #[arg(long)]
        session: PathBuf,
    },
    /// Append supplied decision-example labels for current distinguishing inputs.
    Label {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        examples: PathBuf,
        #[arg(long)]
        provenance: String,
    },
    /// Rebind and compare every supplied observation; does not write a contract.
    Check {
        #[arg(long)]
        session: PathBuf,
    },
    /// Write a new draft contract only after every required label agrees.
    Finalize {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        out: PathBuf,
    },
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(session: &Path) -> Result<Self, String> {
        Self::at(session.join("draft.lock"))
    }
    fn at(path: PathBuf) -> Result<Self, String> {
        atomic_create(&path, b"exclusive draft operation\n")
            .map_err(|e| format!("draft lock: {e}"))?;
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn read(path: &Path) -> Result<String, String> {
    let bytes = contract_files::read_input(path).map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
fn load(session: &Path) -> Result<Draft, String> {
    let draft: Draft =
        serde_json::from_str(&read(&session.join("draft.json"))?).map_err(|e| e.to_string())?;
    draft.validate()?;
    Ok(draft)
}
fn save(session: &Path, draft: &Draft) -> Result<(), String> {
    let bytes = draft.bytes()?;
    // Match the existing bounded reader so every saved session can be reopened.
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("draft transcript exceeds 16 MiB".to_owned());
    }
    atomic_replace(&session.join("draft.json"), &bytes).map_err(|e| e.to_string())
}
fn report(draft: &Draft, status: &str, detail: Value) -> Result<Value, String> {
    Ok(
        json!({"schema": SCHEMA, "status": status, "revision": draft.revision()?,
        "attempts": draft.proposals.len(), "rounds": draft.rounds, "max_tuples": draft.max_tuples,
        "authority": "none", "hosted_model": "off", "detail": detail}),
    )
}

pub(crate) fn run(command: Command) -> u8 {
    let Command::Draft { command } = command;
    match execute(command) {
        Ok((value, code)) => {
            print_json(&value);
            code
        }
        Err(error) => {
            print_json(
                &json!({"schema": SCHEMA, "status": "refused", "reason": error,
                "authority": "none", "hosted_model": "off"}),
            );
            INVALID
        }
    }
}

fn execute(command: Operation) -> Result<(Value, u8), String> {
    if let Operation::Start {
        session,
        intent,
        project,
        examples,
        provenance,
        rounds,
        max_tuples,
    } = command
    {
        let draft = Draft::start(
            read(&intent)?,
            read(&project)?,
            examples
                .as_deref()
                .map(read)
                .transpose()?
                .unwrap_or_default(),
            provenance,
            rounds,
            max_tuples,
        )?;
        fs::create_dir(&session).map_err(|e| format!("new session: {e}"))?;
        let _lock = Lock::acquire(&session)?;
        save(&session, &draft)?;
        return Ok((
            report(
                &draft,
                "started",
                json!({"intent": draft.intent, "note": "Supply complete rules. Initial labels are supplied assumptions."}),
            )?,
            OK,
        ));
    }
    let session = match &command {
        Operation::Propose { session, .. }
        | Operation::Questions { session }
        | Operation::Label { session, .. }
        | Operation::Check { session }
        | Operation::Finalize { session, .. } => session,
        Operation::Start { .. } => unreachable!(),
    };
    let _lock = Lock::acquire(session)?;
    let mut draft = load(session)?;
    match &command {
        Operation::Propose {
            revision,
            rules,
            provenance,
            ..
        } => {
            draft.expect_revision(revision)?;
            draft.reserve(read(rules)?, provenance.clone())?;
            save(session, &draft)?; // Consume the round before F1/F2, including failures/crashes.
            let (assessment, code) = match draft.assess() {
                Ok(assessment) => (assessment.report, OK),
                Err(reason) => (
                    json!({"status": "proposal-refused", "reason": reason}),
                    INVALID,
                ),
            };
            let mut retained = assessment.clone();
            if let Some(object) = retained.as_object_mut() {
                object.remove("review");
            }
            draft.record_assessment(retained)?;
            save(session, &draft)?;
            Ok((report(&draft, "proposal-assessed", assessment)?, code))
        }
        Operation::Label {
            revision,
            examples,
            provenance,
            ..
        } => {
            draft.add_labels(revision, read(examples)?, provenance.clone())?;
            save(session, &draft)?;
            let assessment = draft.assess()?;
            Ok((report(&draft, "labels-recorded", assessment.report)?, OK))
        }
        Operation::Questions { .. } | Operation::Check { .. } => {
            draft.completed()?;
            let assessment = draft.assess()?;
            let code = if matches!(command, Operation::Check { .. }) && !assessment.ready {
                INVALID
            } else {
                OK
            };
            Ok((report(&draft, "assessed", assessment.report)?, code))
        }
        Operation::Finalize { revision, out, .. } => {
            draft.expect_revision(revision)?;
            draft.completed()?;
            let assessment = draft.assess()?;
            if !assessment.ready {
                return Ok((
                    report(&draft, "finalization-refused", assessment.report)?,
                    INVALID,
                ));
            }
            let mut files: Vec<(String, Vec<u8>)> = contract_files::outputs(&assessment.generated)
                .into_iter()
                .map(|(name, bytes)| (name, bytes.to_vec()))
                .collect();
            files.extend([
                (
                    contract_files::PROJECT.to_owned(),
                    draft.project.as_bytes().to_vec(),
                ),
                (
                    contract_files::RULES.to_owned(),
                    draft.current()?.rules.as_bytes().to_vec(),
                ),
                (
                    contract_files::EXAMPLES.to_owned(),
                    assessment.examples.into_bytes(),
                ),
                (
                    "draft-intent.txt".to_owned(),
                    draft.intent.as_bytes().to_vec(),
                ),
                ("draft-transcript.json".to_owned(), draft.bytes()?),
            ]);
            let result = report(&draft, "draft-finalized", assessment.report)?;
            files.push((
                "draft-finalized.json".to_owned(),
                crate::transform::canonical_json(&result).into_bytes(),
            ));
            publish_draft(out, files)?;
            Ok((result, OK))
        }
        Operation::Start { .. } => unreachable!(),
    }
}

/// Atomic directory visibility under the ordinary tooling concurrency contract:
/// all draft publishers use this sibling lock; other processes must not mutate
/// the output name or parent while publication runs. std::fs has no portable
/// rename-no-replace operation. Existing paths (including dangling links) refuse.
fn publish_draft(out: &Path, files: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let name = out
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("output must name a new directory")?;
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let out = parent.join(name);
    let _lock = Lock::at(parent.join(format!(".{name}.draft-publish.lock")))?;
    fn absent(path: &Path) -> Result<(), String> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
            Ok(_) => Err("draft output already exists".to_owned()),
        }
    }
    absent(&out)?;
    let staging = parent.join(format!(
        ".{name}.draft-stage-{}-{}",
        std::process::id(),
        crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        for (name, bytes) in files {
            let path = staging.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            atomic_create(&path, &bytes).map_err(|e| e.to_string())?;
        }
        absent(&out)?;
        fs::rename(&staging, &out).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    } // Exclusively created above.
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_partial_write_never_exposes_target_and_existing_output_is_preserved() {
        let root = std::env::temp_dir().join(format!(
            "draft-publish-test-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        let out = root.join("out");
        // A real write failure after one file succeeds: a file cannot be a directory.
        assert!(
            publish_draft(
                &out,
                vec![
                    ("v2".into(), b"first".to_vec()),
                    ("v2/policy.json".into(), b"second".to_vec())
                ]
            )
            .is_err()
        );
        assert!(!out.exists());
        assert_eq!(
            fs::read_dir(&root)
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
                .count(),
            0
        );
        fs::create_dir(&out).unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        fs::write(out.join("owned"), b"preserve")
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
        assert!(publish_draft(&out, vec![]).is_err());
        assert_eq!(
            fs::read(out.join("owned"))
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
            b"preserve"
        );
        fs::remove_dir_all(root).unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    }
}
