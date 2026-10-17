//! Automatic loop consultation and private, unpromoted proposal files.
//! No acquired knowledge content is copied to logs, session artifacts or receipts.
use crate::knowledge_command as C;
use crate::transform::sha256_hex;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use zeno_fcis_loop_doors::strategy_knowledge as K;

/// Owned proposal-side routing data; no private fact values escape this module.
#[derive(Clone, Debug, Default)]
pub(crate) struct LoopPolicy {
    excluded: Vec<String>,
    ranks: std::collections::BTreeMap<String, (u64, u64)>,
    pub(crate) snapshot: String,
    pub(crate) consultation_work: u64,
    inbox: Option<PathBuf>,
    invocation: String,
}
impl LoopPolicy {
    pub(crate) fn allows(&self, step: &str) -> bool {
        !self.excluded.iter().any(|s| s == step)
    }
    pub(crate) fn rank(&self, step: &str) -> (u64, u64) {
        self.ranks.get(step).copied().unwrap_or((0, 0))
    }
}
impl LoopPolicy {
    pub(crate) fn record_outcome(&self, exit: u8, report: &Value) -> Result<(), Value> {
        let Some(inbox) = &self.inbox else {
            return Ok(());
        };
        ensure_inbox(inbox)?;
        let select = |pointer: &str, allowed: &[&str]| {
            report
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|s| allowed.contains(s))
                .map(str::to_owned)
        };
        let outcome = report
            .pointer("/detail/outcome/outcome")
            .and_then(Value::as_str)
            .or_else(|| {
                report
                    .pointer("/detail/report/attempts")
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.last())
                    .and_then(|row| row.get("outcome"))
                    .and_then(Value::as_str)
            });
        let loop_outcome = outcome.and_then(|s| {
            if s == "checked-improvement" {
                Some("checked-improvement")
            } else if s.starts_with("equivalent-without-improvement:") {
                Some("equivalent-without-improvement")
            } else if matches!(
                s,
                "different"
                    | "inconclusive"
                    | "refused"
                    | "work-exhausted"
                    | "checks-exhausted"
                    | "late"
                    | "not-pending"
            ) {
                Some(s)
            } else {
                None
            }
        });
        let verdict = ["/detail/best_check/verdict", "/detail/check/verdict"]
            .into_iter()
            .find_map(|p| {
                select(
                    p,
                    &[
                        "equivalent",
                        "different",
                        "counterexample",
                        "inconclusive",
                        "refused",
                    ],
                )
            })
            .or_else(|| match loop_outcome {
                Some("checked-improvement" | "equivalent-without-improvement") => {
                    Some("equivalent".into())
                }
                Some("different" | "inconclusive" | "refused") => loop_outcome.map(str::to_owned),
                _ => None,
            });
        // A loop outcome alone never invents a replay verdict.
        let replay = ["/detail/best_check/replay", "/detail/check/replay"]
            .into_iter()
            .find_map(|p| select(p, &["matched", "differs", "inconclusive"]));
        let mut cost = serde_json::Map::new();
        for key in [
            "check_requests",
            "gate_pairs_reserved",
            "search_stage_requests",
            "control_frames",
        ] {
            if let Some(n) = report
                .pointer(&format!("/detail/work/{key}"))
                .and_then(Value::as_u64)
            {
                cost.insert(key.into(), json!(n));
            }
        }
        for key in [
            "attempts",
            "checks",
            "replays",
            "model_calls",
            "tokens_reserved",
            "money_reserved_micros",
            "work_reserved",
            "settled",
        ] {
            let number = [
                format!("/detail/accounting/{key}"),
                format!("/detail/report/accounting/{key}"),
            ]
            .into_iter()
            .find_map(|p| report.pointer(&p).and_then(Value::as_u64));
            if let Some(n) = number {
                cost.insert(format!("session_{key}"), json!(n));
            }
        }
        let event = json!({"schema":"zeno-fcis/loop-checker-observation/1","authority":"none","snapshot_sha256":self.snapshot,"exit":exit,"loop_outcome":loop_outcome,"checker_verdict":verdict,"replay":replay,"cost":cost,"cost_scope":"reported-invocation-counters-and-cumulative-session-accounting","consultation_work":self.consultation_work,"report_sha256":sha256_hex(&serde_json::to_vec(report).map_err(|_|failure("kb-observation-encoding"))?)});
        write_new(
            &inbox.join(format!("outcome-{}.json", self.invocation)),
            &serde_json::to_vec(&event).map_err(|_| failure("kb-observation-encoding"))?,
        )
    }
}
fn ensure_inbox(inbox: &std::path::Path) -> Result<(), Value> {
    if !inbox.exists() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(inbox)
            .map_err(|_| failure("kb-inbox-create"))?;
    }
    let metadata = std::fs::symlink_metadata(inbox).map_err(|_| failure("kb-inbox-read"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure("kb-inbox-must-be-directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(failure("kb-inbox-must-be-private"));
        }
    }
    Ok(())
}

pub(crate) struct Consultation {
    loaded: C::Loaded,
    subjects: Vec<String>,
    command: String,
    pub(crate) blocked: bool,
    counts: (usize, usize),
    consulted_work: u64,
    consultation_micros: u128,
    started: std::time::Instant,
    pub(crate) policy: LoopPolicy,
    child: bool,
    task: String,
    task_facts: Vec<K::Fact>,
    new_session_marker: Option<PathBuf>,
}
fn failure(reason: &'static str) -> Value {
    json!({"schema":"zeno-fcis/loop-knowledge/1","authority":"none","status":"inconclusive","reason":reason})
}
pub(crate) fn prepare(
    command: &crate::Command,
    given: &[PathBuf],
    task: &str,
    expected_snapshot: Option<&str>,
    invocation: Option<&str>,
    disabled: bool,
) -> Result<Option<Consultation>, Value> {
    // Both processes acquire owned bytes. The child must reproduce the parent
    // snapshot before it can run; only the parent proposes or persists config.
    if disabled {
        return Ok(None);
    }
    let started = std::time::Instant::now();
    let child = std::env::var_os("ZENO_FCIS_CHECKER_CONTROL").is_some();
    let Some(subjects) = crate::loop_command::knowledge_subjects(command) else {
        return Ok(None);
    };
    let session = crate::loop_command::knowledge_session(command);
    let marker = session.map(|(path, _)| session_marker(path)).transpose()?;
    let frozen = if !child && session.is_some_and(|(_, open)| !open) {
        marker
            .as_ref()
            .filter(|p| p.exists())
            .map(|p| read_session_config(p))
            .transpose()?
    } else {
        None
    };
    let paths = if given.is_empty() {
        if let Some(config) = &frozen {
            config.paths.clone()
        } else {
            C::paths(given)?
        }
    } else {
        C::paths(given)?
    };
    let task = if task.is_empty() {
        frozen.as_ref().map(|c| c.task.as_str()).unwrap_or("")
    } else {
        task
    };
    if frozen.as_ref().is_some_and(|c| c.task != task) {
        return Err(failure("kb-session-task-changed"));
    }

    if paths.is_empty() {
        return if expected_snapshot.is_some() {
            Err(failure("kb-snapshot-changed"))
        } else {
            Ok(None)
        };
    }
    let loaded = C::load(&paths)?;
    let snapshot=sha256_hex(&serde_json::to_vec(&json!({"layers":loaded.bindings,"validations":loaded.validations.iter().map(|v|(&v.lesson,&v.recipe_sha256,&v.dependency_sha256,&v.validation_sha256,format!("{:?}",v.status))).collect::<Vec<_>>()})).map_err(|_|failure("kb-snapshot"))?);
    if expected_snapshot.is_some_and(|s| s != snapshot)
        || frozen
            .as_ref()
            .is_some_and(|c| c.snapshot_sha256 != snapshot)
    {
        return Err(failure("kb-snapshot-changed"));
    }

    let mut facts = task.to_owned();
    for subject in &subjects {
        facts.push_str(&format!(
            " task(subject,{}).",
            serde_json::to_string(subject).map_err(|_| failure("kb-task"))?
        ));
    }
    let report = C::run(&loaded, &K::Request::Advise(facts), K::Limits::default())
        .map_err(|_| failure("kb-consultation-stopped"))?;
    if started.elapsed().as_secs() >= 5 {
        return Err(failure("kb-deadline"));
    }
    let K::Outcome::Advised(advice) = report.outcome else {
        return Err(failure("kb-consultation-result"));
    };
    let invocation = match invocation {
        Some(s)
            if !s.is_empty()
                && s.len() <= 96
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
        {
            s.to_owned()
        }
        Some(_) => return Err(failure("kb-invocation-id")),
        None => format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| failure("kb-inbox-clock"))?
                .as_nanos()
        ),
    };
    let mut policy = LoopPolicy {
        snapshot,
        consultation_work: report.work,
        inbox: loaded
            .paths
            .last()
            .and_then(|p| p.parent())
            .map(|p| p.join("inbox")),
        invocation,
        ..LoopPolicy::default()
    };
    policy.excluded = advice
        .exclusions
        .iter()
        .filter(|e| e.enforced)
        .map(|e| e.step.clone())
        .collect();
    for s in zeno_fcis_loop_doors::strategy_algebra::Step::ALL {
        let mut penalty = 0u64;
        let mut work = 0u64;
        for f in &advice.facts {
            let positive_preference=advice.facts.iter().any(|e|e.predicate=="effect"&&e.arguments.first()==f.arguments.first()&&matches!(e.arguments.get(1),Some(K::Value::Text(s)) if s=="helps"||s=="neutral"));
            if f.predicate == "prefer"
                && positive_preference
                && f.arguments.get(2) == Some(&K::Value::Text(s.name().into()))
            {
                penalty = penalty.saturating_add(1);
            }
            let applies_to_step = advice.explanations.iter().any(|e| {
                e.conclusion.predicate == "usage_applies"
                    && e.conclusion.arguments.first() == f.arguments.first()
                    && e.conclusion.arguments.get(1) == Some(&K::Value::Text(s.name().into()))
            });
            if f.predicate == "cost"
                && applies_to_step
                && f.arguments.get(1) == Some(&K::Value::Text("work".into()))
                && let Some(K::Value::Integer(n)) = f.arguments.get(2)
            {
                work = work.saturating_add(*n as u64);
            }
        }
        policy.ranks.insert(s.name().into(), (penalty, work));
    }
    // Pipeline/strategy primitives can be filtered independently. A blocker on
    // the requested whole operation still prevents starting that operation.
    let filterable = subjects
        .first()
        .is_some_and(|s| s == "pipeline" || s == "strategy");
    let blocked = subjects
        .iter()
        .enumerate()
        .any(|(i, s)| (!filterable || i == 0) && !policy.allows(s));
    // Commands are evidence in the private inbox only, never an executable recipe.
    let command = std::env::args_os()
        .map(|a| format!("'{0}'", a.to_string_lossy().replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Some(Consultation {
        loaded,
        subjects,
        command,
        blocked,
        counts: (advice.lessons.len(), advice.exclusions.len()),
        consulted_work: report.work,
        consultation_micros: started.elapsed().as_micros(),
        started,
        policy,
        child,
        task: task.to_owned(),
        task_facts: advice.task_facts,
        new_session_marker: if session.is_some_and(|(_, open)| open) {
            marker
        } else {
            None
        },
    }))
}
impl Consultation {
    pub(crate) fn invocation(&self) -> &str {
        &self.policy.invocation
    }
    pub(crate) fn task(&self) -> &str {
        &self.task
    }
    pub(crate) fn paths(&self) -> &[PathBuf] {
        &self.loaded.paths
    }
    pub(crate) fn show(&self) {
        if self.child {
            return;
        }
        // Fixed field names/counts plus the explicitly permitted paths/hashes.
        // Never format Advice, a lesson ID, scope, evidence or a constraint here.
        eprintln!(
            "{}",
            json!({"knowledge":{"layers":self.loaded.bindings,"advice_applied":true,"step_skipped":self.blocked}})
        );
    }
    pub(crate) fn finish(&self, exit: u8) -> Result<(), Value> {
        if self.child {
            return Ok(());
        }
        let Some(path) = self.loaded.paths.last() else {
            return Ok(());
        };
        let root = path.parent().ok_or_else(|| failure("kb-inbox-parent"))?;
        let inbox = root.join("inbox");
        ensure_inbox(&inbox)?;
        let observation_path = inbox.join(format!("outcome-{}.json", self.policy.invocation));
        let observation = if observation_path.is_file() {
            crate::knowledge_evidence::read(&observation_path, 65_536)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .filter(|v| {
                    v["snapshot_sha256"] == self.policy.snapshot
                        && v["schema"] == "zeno-fcis/loop-checker-observation/1"
                        && v["exit"] == exit
                })
        } else {
            None
        };
        let evidence = json!({"schema":"zeno-fcis/loop-invocation-observation/1","authority":"none","command":self.command,"exit":exit,"step_skipped":self.blocked,"advice_outcome":if self.blocked{"blocked"}else if self.counts.0>0{"applied"}else{"consulted"},"checker_outcome":if self.blocked{"not-run"}else{"see-invocation-exit"},"consultation_work":self.consulted_work,"consultation_micros":self.consultation_micros,"invocation_micros_before_proposal_io":self.started.elapsed().as_micros(),"layers":self.loaded.bindings,"checker_observation":observation});
        let bytes = serde_json::to_vec(&evidence).map_err(|_| failure("kb-inbox-evidence"))?;
        let digest = sha256_hex(&bytes);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| failure("kb-inbox-clock"))?
            .as_nanos();
        let id = format!("proposal-{digest}-{nonce}");
        let subject = self
            .subjects
            .first()
            .ok_or_else(|| failure("kb-inbox-subject"))?;
        let quote = |s: &str| serde_json::to_string(s).map_err(|_| failure("kb-inbox-encoding"));
        // The evidence is a retained invocation observation, not a checker
        // receipt or a claim that the proposed lesson has been established.
        let evidence_name = format!("{id}.json");
        let evidence_command = format!("cat {}", evidence_name);
        let mut scopes = String::new();
        for f in &self.task_facts {
            if let [K::Value::Text(key), K::Value::Text(value)] = f.arguments.as_slice()
                && matches!(
                    key.as_str(),
                    "tool" | "version" | "family" | "environment" | "capability" | "prerequisite"
                )
                && value != "unknown"
            {
                scopes.push_str(&format!("scope({id},{},{}).\n", quote(key)?, quote(value)?));
            }
        }
        let lesson = format!(
            "% Unpromoted invocation observation; review and narrow unknown scopes before adoption.\nlesson({id},{}).\n{scopes}strength({id},observed).\neffect({id},neutral).\nevidence({id},{},{}).\nrecord({id},proposed,usage-v1,{},observation,loop-invocation).\n",
            quote(subject)?,
            quote(&evidence_command)?,
            quote(&digest)?,
            quote(&digest)?
        );
        write_new(&inbox.join(&evidence_name), &bytes)?;
        write_new(&inbox.join(format!("{id}.kb")), lesson.as_bytes())?;
        if exit == 0
            && let Some(marker) = &self.new_session_marker
        {
            let config = SessionConfig {
                schema: "zeno-fcis/loop-knowledge-config/1".into(),
                paths: self.loaded.paths.clone(),
                task: self.task.clone(),
                snapshot_sha256: self.policy.snapshot.clone(),
            };
            let config_bytes =
                serde_json::to_vec(&config).map_err(|_| failure("kb-session-config-encoding"))?;
            let context = inbox.join(format!("session-{}.json", self.policy.invocation));
            write_new(&context, &config_bytes)?;
            // A public/session sidecar carries only an explicit path and hash.
            let binding = json!({"schema":"zeno-fcis/loop-knowledge-binding/1","path":context,"sha256":sha256_hex(&config_bytes)});
            write_new(
                marker,
                &serde_json::to_vec(&binding).map_err(|_| failure("kb-session-config-encoding"))?,
            )?;
        }

        Ok(())
    }
}
fn write_new(path: &std::path::Path, bytes: &[u8]) -> Result<(), Value> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| failure("kb-inbox-write"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| failure("kb-inbox-write"))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionConfig {
    schema: String,
    paths: Vec<PathBuf>,
    task: String,
    snapshot_sha256: String,
}
fn session_marker(session: &std::path::Path) -> Result<PathBuf, Value> {
    let mut name = session
        .file_name()
        .ok_or_else(|| failure("kb-session-path"))?
        .to_os_string();
    name.push(".knowledge.json");
    Ok(session.with_file_name(name))
}
fn read_session_config(marker: &std::path::Path) -> Result<SessionConfig, Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Binding {
        schema: String,
        path: PathBuf,
        sha256: String,
    }
    let b: Binding = serde_json::from_slice(&crate::knowledge_evidence::read(marker, 65_536)?)
        .map_err(|_| failure("kb-session-config"))?;
    if b.schema != "zeno-fcis/loop-knowledge-binding/1" {
        return Err(failure("kb-session-config"));
    }
    let bytes = crate::knowledge_evidence::bound_input(
        marker.parent().unwrap_or(std::path::Path::new(".")),
        &crate::knowledge_evidence::Input {
            path: b.path,
            sha256: b.sha256,
        },
        65_536,
    )?;
    let c: SessionConfig =
        serde_json::from_slice(&bytes).map_err(|_| failure("kb-session-config"))?;
    if c.schema != "zeno-fcis/loop-knowledge-config/1" || c.paths.len() > 7 || c.task.len() > 65_536
    {
        return Err(failure("kb-session-config"));
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn kb_usage_session_configuration_reuses_snapshot_and_invalidates_changes() {
        // Preparation/serialization test only. No checked loop execution is
        // simulated as an acceptance receipt by this unit-level I/O exercise.
        let dir = std::env::temp_dir().join(format!(
            "fcis-kb-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
        let kb = dir.join("team.kb");
        std::fs::write(&kb, "custom(synthetic-canary). ").unwrap_or_else(|e| panic!("{e}"));
        let session = dir.join("session");
        let original = dir.join("original.zcve");
        let parsed = crate::Cli::try_parse_from([
            "zeno-fcis",
            "loop",
            "open",
            "--original",
            original.to_str().unwrap_or(""),
            "--session",
            session.to_str().unwrap_or(""),
            "--kb",
            kb.to_str().unwrap_or(""),
            "--kb-task",
            "task(tool,zeno-fcis).task(version,v241).task(environment,host).",
        ])
        .unwrap_or_else(|e| panic!("{e}"));
        let first = prepare(
            &parsed.command,
            &parsed.kb,
            &parsed.kb_task,
            None,
            None,
            false,
        )
        .unwrap_or_else(|e| panic!("prepare: {e}"))
        .unwrap_or_else(|| panic!("missing"));
        assert!(!first.blocked);
        assert!(first.finish(0).is_ok());
        let marker = session_marker(&session).unwrap_or_else(|_| panic!("marker"));
        let binding = std::fs::read_to_string(&marker).unwrap_or_default();
        assert!(!binding.contains("synthetic-canary"));
        assert!(!binding.contains("task("));
        let next = crate::Cli::try_parse_from([
            "zeno-fcis",
            "loop",
            "resume",
            "--session",
            session.to_str().unwrap_or(""),
        ])
        .unwrap_or_else(|e| panic!("{e}"));
        let resumed = prepare(&next.command, &[], "", None, None, false)
            .unwrap_or_else(|_| panic!("resume"))
            .unwrap_or_else(|| panic!("missing"));
        assert_eq!(first.policy.snapshot, resumed.policy.snapshot);
        assert_eq!(first.task, resumed.task);
        std::fs::write(&kb, "custom(changed). ").unwrap_or_else(|e| panic!("{e}"));
        assert!(prepare(&next.command, &[], "", None, None, false).is_err());
        assert!(matches!(
            prepare(&next.command, &[], "", None, None, true),
            Ok(None)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn kb_usage_feedback_records_observed_outcomes_and_never_infers_replay_or_success() {
        let dir = std::env::temp_dir().join(format!(
            "fcis-kb-feedback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
        let mut policy = LoopPolicy {
            inbox: Some(dir.join("inbox")),
            invocation: "fixture".into(),
            snapshot: "a".repeat(64),
            ..LoopPolicy::default()
        };
        let report = json!({"detail":{"outcome":{"outcome":"checked-improvement","private":"synthetic-never-copy"},"accounting":{"attempts":1,"checks":1,"model_calls":0,"work_reserved":9}}});
        assert!(policy.record_outcome(0, &report).is_ok());
        let bytes = std::fs::read(dir.join("inbox/outcome-fixture.json")).unwrap_or_default();
        let event: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        assert_eq!(event["checker_verdict"], "equivalent");
        assert!(event["replay"].is_null());
        assert_eq!(event["cost"]["session_attempts"], 1);
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-never-copy"));
        policy.invocation = "unknown".into();
        assert!(policy.record_outcome(0, &json!({})).is_ok());
        let bytes = std::fs::read(dir.join("inbox/outcome-unknown.json")).unwrap_or_default();
        let event: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        assert!(event["checker_verdict"].is_null());
        assert!(event["replay"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
