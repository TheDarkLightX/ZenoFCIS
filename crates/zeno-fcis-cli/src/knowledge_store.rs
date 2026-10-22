//! Private acquisition and atomic publication of owned knowledge snapshots.
use crate::knowledge_assessment::*;
use crate::transform::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const SCHEMA: &str = "zeno-fcis/kb-state/2";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Change {
    pub kind: String,
    pub subjects: BTreeSet<String>,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layer {
    pub schema: String,
    pub generation: u64,
    pub curation_work: u64,
    pub candidates: Vec<Candidate>,
    pub assessments: BTreeMap<String, Assessment>,
    pub outputs: BTreeMap<String, Value>,
    pub isa: BTreeSet<(String, String)>,
    pub history: Vec<Change>,
    pub legacy: Option<String>,
    #[serde(default)]
    pub episodes: BTreeMap<String, crate::knowledge_growth::Episode>,
    #[serde(default)]
    pub facts: BTreeMap<String, crate::knowledge_growth::CertifiedFact>,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            generation: 0,
            curation_work: 0,
            candidates: vec![],
            assessments: BTreeMap::new(),
            outputs: BTreeMap::new(),
            isa: BTreeSet::new(),
            history: vec![],
            legacy: None,
            episodes: BTreeMap::new(),
            facts: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileRef {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    schema: String,
    state: FileRef,
}
pub(crate) fn hash<T: Serialize>(value: &T) -> Result<String, &'static str> {
    serde_json::to_vec(value)
        .map(|v| sha256_hex(&v))
        .map_err(|_| "kb-encoding")
}
pub(crate) fn statement(c: &Candidate) -> Result<String, &'static str> {
    hash(&(&c.subject, &c.claim, &c.scope, &c.effect, &c.action))
}
pub(crate) fn error(reason: &str) -> Value {
    let mut value = json!({"schema":SCHEMA,"status":"refused","authority":"none","reason":reason});
    let next = match reason {
        "kb-candidate-missing" => Some(
            "Run kb assessment-queue with the same explicit layer; curate the candidate before assessing its identifier.",
        ),
        "kb-task-refused" => Some(
            "Inspect the recorded task and containment preflight; replay with the pinned checker and declared budgets. A refused run cannot promote.",
        ),
        "kb-task-program" => Some(
            "Use a supported finite program encoding and a declared proposal; pin the corrected input in a new frozen suite revision.",
        ),
        _ => None,
    };
    if let Some(next) = next {
        value["next_step"] = json!(next);
    }
    value
}
pub(crate) fn engine() -> String {
    let sources = [
        include_str!("knowledge_assessment.rs"),
        include_str!("knowledge_store.rs"),
        include_str!("knowledge_replay.rs"),
        include_str!("knowledge_growth.rs"),
        include_str!("knowledge_metrics.rs"),
        include_str!("knowledge_workflow.rs"),
        include_str!("knowledge_command.rs"),
        include_str!("loop_knowledge.rs"),
    ];
    let mut bytes = Vec::new();
    for source in sources {
        bytes.extend_from_slice(&(source.len() as u64).to_le_bytes());
        bytes.extend_from_slice(source.as_bytes());
    }
    bytes.extend_from_slice(crate::loop_command::doors::pipeline::implementation().as_bytes());
    sha256_hex(&bytes)
}
pub(crate) fn identity() -> Result<(String, String, String), &'static str> {
    Ok((
        engine(),
        crate::knowledge_evidence::checker_sha256().map_err(|_| "kb-checker-identity")?,
        env!("CARGO_PKG_VERSION").into(),
    ))
}
pub(crate) fn check(layer: &Layer) -> Result<(), &'static str> {
    if layer.schema != SCHEMA
        || layer.candidates.len() > 128
        || layer.assessments.len() > 128
        || layer.outputs.len() > 4096
        || layer.history.len() > 1024
        || layer.isa.len() > 128
    {
        return Err("kb-state-budget");
    }
    if layer
        .isa
        .iter()
        .any(|(c, p)| c == p || member(p, c, &layer.isa))
    {
        return Err("kb-isa-cycle");
    }
    let mut ids = BTreeSet::new();
    let mut statements = BTreeSet::new();
    for c in &layer.candidates {
        c.action.check()?;
        if c.id.is_empty()
            || c.id.len() > 96
            || !c
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || c.claim.len() > 4096
            || c.subject != "pipeline"
            || !valid_scope(&c.scope)
            || !matches!(
                c.effect.as_str(),
                "helps" | "hurts" | "neutral" | "defect" | "limit"
            )
            || statement(c)? != c.statement_sha256
            || !ids.insert(&c.id)
            || !statements.insert(&c.statement_sha256)
            || c.evidence_refs.len() > 1024
            || c.evidence_refs.iter().any(|s| s.len() > 4096)
            || c.families.len() > 128
            || c.episodes.len() > 512
            || c.parents.len() > 32
            || c.proposing_tasks.len() > 1024
            || c.proposing_tasks.iter().any(|s| !digest(s))
            || c.dependencies.len() > 32
        {
            return Err("kb-candidate-schema");
        }
        if c.state == State::Promoted {
            let a = c
                .assessment
                .as_ref()
                .and_then(|h| layer.assessments.get(h))
                .ok_or("kb-unassessed-promotion")?;
            if a.justifications.get(&c.statement_sha256)
                != Some(&hash(&(&c.parents, &c.dependencies))?)
            {
                return Err("kb-assessment-justification");
            }
            if a.candidates != BTreeSet::from([c.statement_sha256.clone()])
                || a.decision != Decision::Promote
            {
                return Err("kb-unassessed-promotion");
            }
            let (decision, _) = assessed_decision(a, &a.proposing_tasks)?;
            if decision != Decision::Promote {
                return Err("kb-unassessed-promotion");
            }
        }
        if c.parents.iter().any(|p| {
            p == &c.statement_sha256 || !layer.candidates.iter().any(|x| &x.statement_sha256 == p)
        }) {
            return Err("kb-justification-parent");
        }
    }
    // Reject cyclic justification graphs, including cycles through pending items.
    for c in &layer.candidates {
        let mut seen = BTreeSet::new();
        let mut todo = c.parents.iter().cloned().collect::<Vec<_>>();
        while let Some(p) = todo.pop() {
            if p == c.statement_sha256 {
                return Err("kb-justification-cycle");
            }
            if seen.insert(p.clone()) {
                if let Some(parent) = layer.candidates.iter().find(|x| x.statement_sha256 == p) {
                    todo.extend(parent.parents.iter().cloned());
                }
            }
        }
    }
    for (key, a) in &layer.assessments {
        if hash(a)? != *key
            || a.schema != "zeno-fcis/kb-assessment/1"
            || !digest(&a.suite_sha256)
            || !digest(&a.engine_sha256)
            || !digest(&a.checker_sha256)
            || a.reserved_work > a.total_budget
            || a.partial_runs.len() > 4
            || !a.partial_runs.is_empty() && a.resource_stop.is_none()
            || recorded_work(&a.cases, &a.partial_runs)? != a.assessment_work
        {
            return Err("kb-assessment-binding");
        }
        let proposing = &a.proposing_tasks;
        let suite: crate::knowledge_replay::Suite = serde_json::from_value(
            layer
                .outputs
                .get(&a.suite_sha256)
                .ok_or("kb-assessment-suite")?
                .clone(),
        )
        .map_err(|_| "kb-assessment-suite")?;
        suite.check()?;
        if hash(&suite)? != a.suite_sha256
            || a.resource_stop.is_none() && a.cases.len() != suite.tasks.len()
        {
            return Err("kb-assessment-suite");
        }
        for case in &a.cases {
            let task = suite
                .tasks
                .iter()
                .find(|t| t.sha256 == case.task)
                .ok_or("kb-assessment-task")?;
            let expected = layer.candidates.iter().any(|c| {
                a.candidates.contains(&c.statement_sha256)
                    && applies(&c.scope, &task.scope, &layer.isa)
            });
            if case.scope != task.scope
                || case.applicable != expected
                || case.off.inputs != task.subject.sha256
                || case.off.seed != task.seed
                || case.off.budget != task.budget.work
            {
                return Err("kb-assessment-task");
            }
            if a.proposing_subjects.contains(&task.subject.sha256) {
                return Err("kb-held-out-split");
            }
        }
        for run in &a.partial_runs {
            let task = suite
                .tasks
                .iter()
                .find(|t| t.sha256 == run.task)
                .ok_or("kb-assessment-task")?;
            if run.inputs != task.subject.sha256
                || run.seed != task.seed
                || run.budget != task.budget.work
            {
                return Err("kb-assessment-task");
            }
            let output = layer
                .outputs
                .get(&run.output_sha256)
                .ok_or("kb-assessment-output")?;
            if hash(output)? != run.output_sha256
                || crate::knowledge_replay::measure(
                    output,
                    run.task.clone(),
                    run.seed,
                    run.budget,
                    run.inputs.clone(),
                )? != *run
            {
                return Err("kb-assessment-output");
            }
        }
        let (decision, reason) = assessed_decision(a, proposing)?;
        if decision != a.decision || reason != a.reason {
            return Err("kb-assessment-decision");
        }
        for case in &a.cases {
            for run in [&case.off, &case.on] {
                let output = layer
                    .outputs
                    .get(&run.output_sha256)
                    .ok_or("kb-assessment-output")?;
                if hash(output)? != run.output_sha256
                    || crate::knowledge_replay::measure(
                        output,
                        run.task.clone(),
                        run.seed,
                        run.budget,
                        run.inputs.clone(),
                    )? != *run
                {
                    return Err("kb-assessment-output");
                }
            }
        }
    }
    for (i, a) in layer.candidates.iter().enumerate() {
        for b in layer.candidates.iter().skip(i + 1) {
            if a.state == State::Promoted
                && b.state == State::Promoted
                && conflicts(a, b, &layer.isa)
            {
                return Err("kb-assessed-conflict");
            }
        }
    }
    // An unrelated pending record cannot preserve a promoted dependent.
    for c in layer
        .candidates
        .iter()
        .filter(|c| c.state == State::Promoted)
    {
        if c.parents.iter().any(|p| {
            layer.candidates.iter().any(|x| {
                &x.statement_sha256 == p
                    && (matches!(x.state, State::Pending | State::Refuted)
                        || x.assessment.is_none())
            })
        }) {
            return Err("kb-pending-justification");
        }
    }
    crate::knowledge_growth::validate(layer)?;
    Ok(())
}
fn assessed_decision(
    a: &Assessment,
    proposing: &BTreeSet<String>,
) -> Result<(Decision, &'static str), &'static str> {
    let (decision, reason) = decide(&a.cases, proposing, &a.policy, &a.annotations)?;
    Ok(
        if a.resource_stop.is_some() && decision != Decision::Reject {
            (Decision::Inconclusive, "assessment-resource-stop")
        } else {
            (decision, reason)
        },
    )
}
pub(crate) fn current(
    layer: &Layer,
    engine: &str,
    checker: &str,
    version: &str,
    dependencies: &BTreeMap<String, String>,
) -> Layer {
    let roots = layer
        .candidates
        .iter()
        .filter(|c| {
            c.state != State::Refuted
                && c.assessment.is_some()
                && (c
                    .assessment
                    .as_ref()
                    .and_then(|h| layer.assessments.get(h))
                    .is_none_or(|a| {
                        a.engine_sha256 != engine
                            || a.checker_sha256 != checker
                            || a.tool_version != version
                    })
                    || c.dependencies
                        .iter()
                        .any(|(p, h)| dependencies.get(p) != Some(h)))
        })
        .map(|c| c.statement_sha256.clone())
        .collect();
    let mut next = layer.clone();
    next.candidates = retract(&layer.candidates, &roots);
    for c in &mut next.candidates {
        if c.state == State::Pending {
            for (p, expected) in &mut c.dependencies {
                if let Some(actual) = dependencies.get(p) {
                    if digest(actual) {
                        *expected = actual.clone();
                    }
                }
            }
        }
    }
    if !roots.is_empty() && next.candidates != layer.candidates {
        next.history.push(Change {
            kind: "justification-invalidated".into(),
            subjects: roots.clone(),
            evidence: hash(&(engine, checker, version, dependencies)).unwrap_or_default(),
        });
    }
    let telemetry = crate::knowledge_growth::telemetry(&next);
    let regressed = telemetry["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["outcomes"]["reassessment_required"] == true)
        .filter_map(|r| r["lesson"].as_str())
        .filter_map(|id| {
            next.candidates
                .iter()
                .find(|c| c.id == id || c.statement_sha256 == id)
        })
        .map(|c| c.statement_sha256.clone())
        .collect::<BTreeSet<_>>();
    if !regressed.is_empty() {
        let before = next.candidates.clone();
        next.candidates = retract(&before, &regressed);
        if before != next.candidates {
            next.history.push(Change {
                kind: "telemetry-reassessment-required".into(),
                subjects: regressed,
                evidence: hash(&telemetry).unwrap_or_default(),
            });
        }
    }
    next
}
pub(crate) fn conflicts(a: &Candidate, b: &Candidate, isa: &BTreeSet<(String, String)>) -> bool {
    if a.statement_sha256 == b.statement_sha256 || a.subject != b.subject || a.action == b.action {
        return false;
    }
    for k in ["tool", "version", "family", "environment"] {
        if let (Some(av), Some(bv)) = (
            a.scope.get(k).and_then(|v| v.first()),
            b.scope.get(k).and_then(|v| v.first()),
        ) {
            if av != "any"
                && bv != "any"
                && av != bv
                && !(matches!(k, "tool" | "family")
                    && (member(av, bv, isa)
                        || member(bv, av, isa)
                        || isa
                            .iter()
                            .any(|(child, _)| member(child, av, isa) && member(child, bv, isa))))
            {
                return false;
            }
        }
    }
    true
}
pub(crate) fn read(path: &Path, max: u64) -> Result<Vec<u8>, &'static str> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "kb-read")?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("kb-regular-file-required");
    }
    crate::transform_command::read_bounded(path, max).map_err(|_| "kb-read-budget")
}
pub(crate) fn bound(root: &Path, file: &FileRef, max: u64) -> Result<Vec<u8>, &'static str> {
    if !digest(&file.sha256)
        || file.path.is_absolute()
        || file
            .path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("kb-relative-input");
    }
    let mut p = root.to_path_buf();
    for part in file.path.components() {
        p.push(part);
        if std::fs::symlink_metadata(&p)
            .map_err(|_| "kb-read")?
            .file_type()
            .is_symlink()
        {
            return Err("kb-input-symlink");
        }
    }
    let bytes = read(&p, max)?;
    if sha256_hex(&bytes) != file.sha256 {
        return Err("kb-input-changed");
    }
    Ok(bytes)
}
pub(crate) fn load(path: &Path) -> Result<(Layer, String), &'static str> {
    let data = read(path, 65_536)?;
    let d: Descriptor = serde_json::from_slice(&data).map_err(|_| "kb-layer-schema")?;
    if d.schema != "zeno-fcis/kb-layer/2" {
        return Err("kb-layer-schema");
    }
    let bytes = bound(
        path.parent().ok_or("kb-parent")?,
        &d.state,
        16 * 1024 * 1024,
    )?;
    let layer: Layer = serde_json::from_slice(&bytes).map_err(|_| "kb-state-schema")?;
    check(&layer)?;
    Ok((layer, sha256_hex(&data)))
}
pub(crate) fn live(path: &Path) -> Result<(Layer, String), &'static str> {
    live_with_checker(path, None)
}
pub(crate) fn live_with_checker(
    path: &Path,
    checker: Option<&str>,
) -> Result<(Layer, String), &'static str> {
    let (layer, binding) = load(path)?;
    let (e, c, v) = if let Some(checker) = checker {
        (
            engine(),
            checker.to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        )
    } else {
        identity()?
    };
    let deps = layer
        .candidates
        .iter()
        .flat_map(|x| x.dependencies.keys())
        .map(|p| {
            (
                p.clone(),
                read(Path::new(p), 64 * 1024 * 1024)
                    .map(|b| sha256_hex(&b))
                    .unwrap_or_default(),
            )
        })
        .collect();
    Ok((current(&layer, &e, &c, &v, &deps), binding))
}
pub(crate) fn private_root(root: &Path) -> Result<(), &'static str> {
    let resolved = std::fs::canonicalize(root).map_err(|_| "kb-private-parent")?;
    if crate::knowledge_command::checkout(&resolved) {
        return Err("kb-path-must-be-outside-checkout");
    }
    let meta = std::fs::symlink_metadata(root).map_err(|_| "kb-private-parent")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("kb-private-parent");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("kb-private-parent");
        }
    }
    Ok(())
}
fn create(path: &Path) -> Result<std::fs::File, &'static str> {
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    options.open(path).map_err(|_| "kb-exclusive-create")
}
fn durable(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    let mut f = create(path)?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "kb-sync")
}
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Data,
    Descriptor,
    Swap,
}
pub(crate) fn commit(
    path: &Path,
    expected: Option<&str>,
    layer: &Layer,
) -> Result<String, &'static str> {
    commit_inner(path, expected, layer, Fault::None)
}
fn commit_inner(
    path: &Path,
    expected: Option<&str>,
    layer: &Layer,
    fault: Fault,
) -> Result<String, &'static str> {
    let parent = path.parent().ok_or("kb-parent")?;
    private_root(parent)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("kb-layer-name")?;
    let lock_path = parent.join(format!(".{name}.lock"));
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let lock = opts.open(lock_path).map_err(|_| "kb-lock")?;
    lock.try_lock().map_err(|_| "kb-layer-locked")?;
    let old = if path.exists() {
        Some(read(path, 65_536)?)
    } else {
        None
    };
    if old.as_ref().map(|b| sha256_hex(b)).as_deref() != expected {
        return Err("kb-revision-conflict");
    }
    check(layer)?;
    let state = serde_json::to_vec(layer).map_err(|_| "kb-encoding")?;
    if state.len() > 16 * 1024 * 1024 {
        return Err("kb-state-budget");
    }
    let state_sha = sha256_hex(&state);
    let state_name = format!("state-{state_sha}.json");
    let state_path = parent.join(&state_name);
    if !state_path.exists() {
        durable(&state_path, &state)?;
    } else if read(&state_path, 16 * 1024 * 1024)? != state {
        return Err("kb-state-collision");
    }
    if fault == Fault::Data {
        return Err("kb-simulated-failure");
    }
    let descriptor = serde_json::to_vec(&Descriptor {
        schema: "zeno-fcis/kb-layer/2".into(),
        state: FileRef {
            path: state_name.into(),
            sha256: state_sha,
        },
    })
    .map_err(|_| "kb-encoding")?;
    let staging = parent.join(format!(".{name}-{}.staged", std::process::id()));
    durable(&staging, &descriptor)?;
    let result = (|| {
        let (_, report) = crate::knowledge_command::invoke(
            &[staging.clone()],
            crate::knowledge_command::Operation::Check,
        );
        if report["status"] != "consistent" {
            return Err("kb-pre-swap-check");
        }
        let directory = std::fs::File::open(parent).map_err(|_| "kb-directory")?;
        directory.sync_all().map_err(|_| "kb-directory-sync")?;
        if fault == Fault::Descriptor {
            return Err("kb-simulated-failure");
        }
        std::fs::rename(&staging, path).map_err(|_| "kb-swap")?;
        let finish = if fault == Fault::Swap {
            Err("kb-simulated-failure")
        } else {
            directory.sync_all().map_err(|_| "kb-directory-sync")
        };
        if let Err(reason) = finish {
            if let Some(bytes) = &old {
                durable(&staging, bytes)?;
                std::fs::rename(&staging, path).map_err(|_| "kb-rollback")?;
            } else {
                std::fs::remove_file(path).map_err(|_| "kb-rollback")?;
            }
            directory.sync_all().map_err(|_| "kb-rollback-sync")?;
            return Err(reason);
        }
        let actual = read(path, 65_536)?;
        if actual != descriptor {
            return Err("kb-post-swap-binding");
        }
        Ok(sha256_hex(&actual))
    })();
    if staging.exists() {
        let _ = std::fs::remove_file(&staging);
    }
    drop(lock);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(id: &str) -> Candidate {
        let mut c = Candidate {
            id: id.into(),
            statement_sha256: String::new(),
            subject: "pipeline".into(),
            claim: id.into(),
            scope: Scope::new(),
            effect: "helps".into(),
            action: Action {
                skip: BTreeSet::new(),
                order: vec![],
            },
            proposing_tasks: BTreeSet::new(),
            families: BTreeSet::new(),
            episodes: BTreeSet::new(),
            parents: BTreeSet::new(),
            dependencies: BTreeMap::new(),
            state: State::Proposed,
            assessment: None,
            observed_work: 0,
            evidence_refs: BTreeSet::new(),
            origin: "test".into(),
        };
        c.statement_sha256 = statement(&c).unwrap();
        c
    }
    #[test]
    fn refusal_guidance_keeps_authority_absent() {
        for reason in ["kb-candidate-missing", "kb-task-refused", "kb-task-program"] {
            let value = error(reason);
            assert_eq!(value["authority"], "none");
            assert_eq!(value["reason"], reason);
            assert!(value["next_step"].as_str().is_some_and(|s| !s.is_empty()));
        }
    }

    #[test]
    fn generalized_and_mined_records_cannot_self_promote() {
        for origin in ["isa-generalization", "failure-mining"] {
            let mut c = candidate(origin);
            c.origin = origin.into();
            c.state = State::Promoted;
            let layer = Layer {
                candidates: vec![c],
                ..Layer::default()
            };
            assert_eq!(check(&layer), Err("kb-unassessed-promotion"));
        }
    }
    #[test]
    fn corroboration_and_telemetry_cannot_promote() {
        let mut c = candidate("observed");
        for actor in ["one", "two"] {
            c.families.insert(EvidenceFamily {
                subject: actor.repeat(64),
                session: actor.into(),
                actor: actor.into(),
                tool: "v".into(),
                environment: "e".into(),
            });
        }
        c.observed_work = u64::MAX / 2;
        let layer = Layer {
            candidates: vec![c],
            ..Layer::default()
        };
        let next = current(&layer, "e", "c", "v", &BTreeMap::new());
        assert_eq!(next.candidates[0].state, State::Proposed);
        assert!(crate::knowledge_workflow::advice(&[next], &[]).is_empty());
    }
    #[test]
    fn dependency_change_retracts_only_justified_items() {
        let mut c = candidate("a");
        c.state = State::Promoted;
        c.assessment = Some("old".into());
        c.dependencies.insert("dependency".into(), "a".repeat(64));
        let mut b = candidate("b");
        b.state = State::Promoted;
        b.assessment = Some("old".into());
        b.parents.insert(c.statement_sha256.clone());
        let mut d = candidate("d");
        d.state = State::Promoted;
        d.assessment = Some("old".into());
        let a = Assessment {
            schema: "zeno-fcis/kb-assessment/1".into(),
            suite_sha256: "a".repeat(64),
            revision_sha256: "b".repeat(64),
            candidates: BTreeSet::new(),
            proposing_tasks: BTreeSet::new(),
            proposing_subjects: BTreeSet::new(),
            total_budget: 0,
            justifications: BTreeMap::new(),
            engine_sha256: "e".into(),
            checker_sha256: "c".into(),
            tool_version: "v".into(),
            policy: Policy::default(),
            cases: vec![],
            partial_runs: vec![],
            annotations: vec![],
            decision: Decision::Inconclusive,
            reason: "no-applicable-task".into(),
            resource_stop: None,
            assessment_work: 0,
            reserved_work: 0,
            prior_assessment_work: 0,
            curation_work: 0,
        };
        let layer = Layer {
            candidates: vec![c, b, d],
            assessments: BTreeMap::from([("old".into(), a)]),
            ..Layer::default()
        };
        let next = current(
            &layer,
            "e",
            "c",
            "v",
            &BTreeMap::from([("dependency".into(), "z".repeat(64))]),
        );
        assert_eq!(
            next.candidates.iter().map(|c| c.state).collect::<Vec<_>>(),
            vec![State::Pending, State::Pending, State::Promoted]
        );
        assert!(
            next.history
                .iter()
                .any(|h| h.kind == "justification-invalidated")
        );
    }
    #[test]
    fn changed_lifecycle_in_same_layer_key_is_a_conflict() {
        let a = candidate("same");
        let mut b = a.clone();
        b.state = State::Rejected;
        assert!(crate::knowledge_workflow::layer_conflict(&[
            Layer {
                candidates: vec![a],
                ..Layer::default()
            },
            Layer {
                candidates: vec![b],
                ..Layer::default()
            }
        ]));
    }
    #[test]
    fn unassessed_sources_never_become_advice() {
        use zeno_fcis_loop_doors::strategy_knowledge::{Fact, Value};
        let facts = [Fact {
            predicate: "task".into(),
            arguments: vec![
                Value::Text("subject".into()),
                Value::Text("pipeline".into()),
            ],
        }];
        for origin in ["failure-mining", "isa-generalization", "inbox"] {
            let mut c = candidate("candidate");
            c.origin = origin.into();
            let layers = [Layer {
                candidates: vec![c],
                ..Layer::default()
            }];
            assert!(crate::knowledge_workflow::advice(&layers, &facts).is_empty());
        }
    }
    #[test]
    fn duplicate_corroboration_family_has_one_queue_vote() {
        let mut c = candidate("candidate");
        let f = EvidenceFamily {
            subject: "s".into(),
            session: "session".into(),
            actor: "contributor".into(),
            tool: "v".into(),
            environment: "e".into(),
        };
        c.families.extend([f.clone(), f]);
        let layer = Layer {
            candidates: vec![c],
            ..Layer::default()
        };
        assert_eq!(
            crate::knowledge_workflow::queue(&layer, None)[0]["independent_families"],
            1
        );
    }
    #[test]
    fn refuted_record_is_never_advice() {
        let mut c = candidate("refuted");
        c.state = State::Refuted;
        let layers = [Layer {
            candidates: vec![c],
            ..Layer::default()
        }];
        let task = zeno_fcis_loop_doors::strategy_knowledge::Fact {
            predicate: "task".into(),
            arguments: vec![
                zeno_fcis_loop_doors::strategy_knowledge::Value::Text("subject".into()),
                zeno_fcis_loop_doors::strategy_knowledge::Value::Text("pipeline".into()),
            ],
        };
        assert!(crate::knowledge_workflow::advice(&layers, &[task]).is_empty());
    }
    #[test]
    fn atomic_failure_preserves_complete_layer() {
        let root = std::env::temp_dir().join(format!("kb-atomic-{}", std::process::id()));
        let _ = std::fs::create_dir(&root);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let p = root.join("layer.json");
        let _ = std::fs::remove_file(&p);
        let old = Layer::default();
        let sha = commit(&p, None, &old).unwrap();
        let before = read(&p, 65_536).unwrap();
        for fault in [Fault::Data, Fault::Descriptor, Fault::Swap] {
            let mut next = old.clone();
            next.generation = 1;
            assert!(commit_inner(&p, Some(&sha), &next, fault).is_err());
            assert_eq!(read(&p, 65_536).unwrap(), before);
            assert_eq!(load(&p).unwrap().0, old);
        }
        assert_eq!(
            commit(&p, Some(&"f".repeat(64)), &old),
            Err("kb-revision-conflict")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
