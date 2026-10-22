//! Bounded, separate-process held-out replay. Time is a refusal bound only.
use crate::knowledge_assessment::*;
use crate::knowledge_store::{self as S, FileRef, Layer};
use crate::transform::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskBudget {
    pub work: u64,
    pub tuples: u64,
    pub gate_pairs: u64,
    pub checks: u32,
    pub search_work: u64,
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Task {
    pub name: String,
    pub subject: FileRef,
    pub scope: Scope,
    pub seed: u64,
    pub budget: TaskBudget,
    pub proposal: Value,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Suite {
    pub schema: String,
    pub proposing_subjects: BTreeSet<String>,
    pub tasks: Vec<Task>,
}
impl Task {
    pub(crate) fn digest(&self) -> Result<String, &'static str> {
        S::hash(&(
            &self.name,
            &self.subject.sha256,
            &self.scope,
            self.seed,
            &self.budget,
            &self.proposal,
        ))
    }
}
impl Suite {
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        if self.schema != "zeno-fcis/kb-suite/1"
            || self.tasks.is_empty()
            || self.tasks.len() > 64
            || self.proposing_subjects.is_empty()
        {
            return Err("kb-suite-schema");
        }
        let mut names = BTreeSet::new();
        let mut hashes = BTreeSet::new();
        for t in &self.tasks {
            if t.name.len() > 96
                || !names.insert(&t.name)
                || !hashes.insert(&t.sha256)
                || t.digest()? != t.sha256
                || !digest(&t.subject.sha256)
                || !valid_scope(&t.scope)
                || self.proposing_subjects.contains(&t.subject.sha256)
                || t.budget.work == 0
                || t.budget.work > 10_000_000
                || t.budget.tuples > 4096
                || t.budget.gate_pairs > 1_000_000
                || t.budget.checks > 24
                || t.budget.search_work > 1_000_000
                || !(10..=22_000).contains(&t.budget.timeout_ms)
            {
                return Err("kb-suite-task");
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    task: Task,
    original: Vec<u8>,
    actions: Vec<Action>,
}
pub(crate) fn normalise(value: &Value) -> Value {
    match value {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| {
                    !k.ends_with("_micros")
                        && !k.ends_with("_ns")
                        && !matches!(k.as_str(), "elapsed" | "duration" | "last_validation")
                })
                .map(|(k, v)| (k.clone(), normalise(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(normalise).collect()),
        _ => value.clone(),
    }
}
pub(crate) fn measure(
    output: &Value,
    task: String,
    seed: u64,
    budget: u64,
    inputs: String,
) -> Result<Run, &'static str> {
    let d = output.get("detail").ok_or("kb-task-output")?;
    let accepted = d["status"] == "qualified"
        && d["best_check"]["verdict"] == "equivalent"
        && d["best_check"]["replay"] == "matched";
    let mut work = BTreeMap::new();
    for key in [
        "check_requests",
        "gate_pairs_reserved",
        "search_stage_requests",
        "control_frames",
        "capture_work_reserved",
        "capture_cells_reserved",
    ] {
        work.insert(key.into(), d["work"][key].as_u64().ok_or("kb-task-work")?);
    }
    let synthesis = d["trace"]
        .as_array()
        .into_iter()
        .flatten()
        .try_fold(0u64, |total, t| {
            total.checked_add(t["synthesis_work_used"].as_u64().unwrap_or(0))
        })
        .ok_or("kb-work-overflow")?;
    work.insert("synthesis_work_used".into(), synthesis);
    let work_sum = work
        .values()
        .try_fold(0u64, |a, b| a.checked_add(*b))
        .ok_or("kb-work-overflow")?;
    let reason = d["reason"].as_str().map(str::to_owned);
    let stopped = work_sum > budget
        || d["status"] == "inconclusive"
        || reason
            .as_ref()
            .is_some_and(|s| s.contains("budget") || s.contains("deadline"));
    let verdict = if stopped {
        Verdict::Inconclusive
    } else if accepted {
        Verdict::Accepted
    } else if d["status"] == "refuted" {
        Verdict::Counterexample
    } else if d["status"] == "refused" {
        Verdict::Refused
    } else {
        Verdict::NotFound
    };
    let cost = if verdict.solved() {
        Some([
            d["best"]["nodes"].as_u64().ok_or("kb-task-cost")?,
            d["best"]["bytes"].as_u64().ok_or("kb-task-cost")?,
        ])
    } else {
        None
    };
    let receipt = if accepted {
        d["best_check"]["receipt_sha256"]
            .as_str()
            .filter(|s| digest(s))
            .ok_or("kb-task-receipt")?
            .to_owned()
    } else {
        S::hash(
            &json!({"schema":"zeno-fcis/kb-run-observation/1","task":task,"verdict":verdict,"work":work,"output_sha256":S::hash(output)?}),
        )?
    };
    Ok(Run {
        task,
        seed,
        budget,
        inputs,
        verdict,
        work,
        result_cost: cost,
        budget_stop: stopped,
        stopping_bound: if work_sum > budget {
            Some("assessment-work-budget".into())
        } else {
            reason
        },
        receipt_sha256: receipt,
        output_sha256: S::hash(output)?,
    })
}
pub(crate) fn worker(request: &Path, out: &Path) -> Result<(), &'static str> {
    let bytes = S::read(request, 2 * 1024 * 1024)?;
    let req: Request = serde_json::from_slice(&bytes).map_err(|_| "kb-task-request")?;
    if req.task.digest()? != req.task.sha256
        || sha256_hex(&req.original) != req.task.subject.sha256
        || req.actions.len() > 128
    {
        return Err("kb-task-binding");
    }
    for a in &req.actions {
        a.check()?;
    }
    let output = crate::loop_command::doors::assessment_run(
        &req.original,
        &req.task.proposal,
        &req.actions,
        &req.task.budget,
    )?;
    let output = normalise(&output);
    let bytes = serde_json::to_vec(&output).map_err(|_| "kb-encoding")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("kb-task-output-budget");
    }
    private_write(out, &bytes)
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path).map_err(|_| "kb-task-create")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "kb-task-write")
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn subprocess(
    task: &Task,
    original: &[u8],
    actions: &[Action],
    root: &Path,
    ordinal: usize,
) -> Result<Value, &'static str> {
    let request = root.join(format!("request-{ordinal}.json"));
    let out = root.join(format!("result-{ordinal}.json"));
    private_write(
        &request,
        &serde_json::to_vec(&Request {
            task: task.clone(),
            original: original.to_vec(),
            actions: actions.to_vec(),
        })
        .map_err(|_| "kb-encoding")?,
    )?;
    let executable = std::env::current_exe().map_err(|_| "kb-task-executable")?;
    let mut command = Command::new("nice");
    command
        .args(["-n", "19", "ionice", "-c3"])
        .arg(executable)
        .args(["kb", "assessment-task", "--request"])
        .arg(request)
        .arg("--out")
        .arg(&out);
    command
        .env_remove("ZENO_FCIS_KB")
        .env_remove("ZENO_FCIS_CHECKER_CONTROL")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| "kb-task-spawn")?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err("kb-task-refused");
                }
                break;
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("kb-task-wait");
            }
        }
        if start.elapsed() >= Duration::from_millis(task.budget.timeout_ms.saturating_add(1000)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("kb-task-timeout");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let bytes = S::read(&out, 2 * 1024 * 1024)?;
    serde_json::from_slice(&bytes).map_err(|_| "kb-task-output")
}
pub(crate) fn assess(
    layer: &Layer,
    suite: &Suite,
    suite_root: &Path,
    selected: &BTreeSet<String>,
    total_budget: u64,
    policy: Policy,
    annotations: Vec<Annotation>,
    private_parent: &Path,
) -> Result<(Layer, String), &'static str> {
    suite.check()?;
    S::check(layer)?;
    let candidates = layer
        .candidates
        .iter()
        .filter(|c| selected.contains(&c.statement_sha256))
        .collect::<Vec<_>>();
    if candidates.len() != selected.len() || candidates.iter().any(|c| c.state == State::Refuted) {
        return Err("kb-assessment-selection");
    }
    for c in &candidates {
        for (path, expected) in &c.dependencies {
            if sha256_hex(&S::read(Path::new(path), 64 * 1024 * 1024)?) != *expected {
                return Err("kb-dependency-changed");
            }
        }
    }
    let proposing = candidates
        .iter()
        .flat_map(|c| c.proposing_tasks.iter().cloned())
        .collect::<BTreeSet<_>>();
    if suite.tasks.iter().any(|t| {
        proposing.contains(&t.sha256)
            || candidates
                .iter()
                .any(|c| c.families.iter().any(|f| f.subject == t.subject.sha256))
    }) {
        return Err("kb-held-out-split");
    }
    S::private_root(private_parent)?;
    let root = private_parent.join(format!("assessment-{}", std::process::id()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&root).map_err(|_| "kb-assessment-scratch")?;
    let _scratch = Scratch(root.clone());
    let (engine, checker, version) = S::identity()?;
    let mut next = layer.clone();
    let mut cases = Vec::new();
    let mut reserved = 0u64;
    let mut assessment_work = 0u64;
    let mut stop = None;
    let suite_sha = S::hash(suite)?;
    next.outputs.insert(
        suite_sha.clone(),
        serde_json::to_value(suite).map_err(|_| "kb-encoding")?,
    );
    for (i, t) in suite.tasks.iter().enumerate() {
        let needed = t.budget.work.checked_mul(4).ok_or("kb-work-overflow")?;
        if needed > total_budget.saturating_sub(reserved) {
            stop = Some("total-work-budget".into());
            break;
        }
        reserved += needed;
        let original = S::bound(suite_root, &t.subject, 1_048_576)?;
        let matched = candidates
            .iter()
            .filter(|c| applies(&c.scope, &t.scope, &layer.isa))
            .collect::<Vec<_>>();
        let actions = matched.iter().map(|c| c.action.clone()).collect::<Vec<_>>();
        let mut results = Vec::new();
        for (n, action) in [vec![], actions.clone(), vec![], actions]
            .into_iter()
            .enumerate()
        {
            match subprocess(t, &original, &action, &root, i * 4 + n) {
                Ok(v) => results.push(v),
                Err(e) if e == "kb-task-timeout" => {
                    stop = Some(e.into());
                    break;
                }
                Err(e) => {
                    stop = Some(e.into());
                    assessment_work = assessment_work.saturating_add(t.budget.work);
                    break;
                }
            }
        }
        if results.len() != 4 {
            break;
        }
        if results[0] != results[2] || results[1] != results[3] {
            return Err("kb-nondeterministic-replay");
        }
        let inputs = sha256_hex(&original);
        let off = measure(
            &results[0],
            t.sha256.clone(),
            t.seed,
            t.budget.work,
            inputs.clone(),
        )?;
        let on = measure(&results[1], t.sha256.clone(), t.seed, t.budget.work, inputs)?;
        for r in &results {
            let measured = measure(
                r,
                t.sha256.clone(),
                t.seed,
                t.budget.work,
                t.subject.sha256.clone(),
            )?;
            assessment_work = assessment_work
                .checked_add(measured.work.values().copied().sum::<u64>())
                .and_then(|n| n.checked_add(8))
                .ok_or("kb-work-overflow")?;
            next.outputs.insert(S::hash(r)?, r.clone());
        }
        cases.push(Case {
            task: t.sha256.clone(),
            scope: t.scope.clone(),
            applicable: !matched.is_empty(),
            off,
            on,
        });
    }
    let (mut decision, mut reason) = decide(&cases, &proposing, &policy, &annotations)?;
    if stop.is_some() && decision != Decision::Reject {
        decision = Decision::Inconclusive;
        reason = "assessment-resource-stop";
    }
    let a = Assessment {
        schema: "zeno-fcis/kb-assessment/1".into(),
        suite_sha256: suite_sha,
        revision_sha256: S::hash(layer)?,
        candidates: selected.clone(),
        proposing_tasks: proposing,
        proposing_subjects: candidates
            .iter()
            .flat_map(|c| c.families.iter().map(|f| f.subject.clone()))
            .collect(),
        total_budget,
        justifications: candidates
            .iter()
            .map(|c| {
                Ok((
                    c.statement_sha256.clone(),
                    S::hash(&(&c.parents, &c.dependencies))?,
                ))
            })
            .collect::<Result<_, &'static str>>()?,
        engine_sha256: engine,
        checker_sha256: checker,
        tool_version: version,
        policy,
        cases,
        annotations,
        decision,
        reason: reason.into(),
        resource_stop: stop,
        assessment_work,
        reserved_work: reserved,
        prior_assessment_work: layer.assessments.values().map(|a| a.assessment_work).sum(),
        curation_work: layer.curation_work,
    };
    let key = S::hash(&a)?;
    next.assessments.insert(key.clone(), a.clone());
    if selected.len() == 1 {
        for c in &mut next.candidates {
            if selected.contains(&c.statement_sha256) {
                c.state = match decision {
                    Decision::Promote => State::Promoted,
                    Decision::Reject => State::Rejected,
                    Decision::Inconclusive => State::Proposed,
                };
                c.assessment = Some(key.clone());
            }
        }
        if decision != Decision::Promote {
            next.candidates = retract(&next.candidates, selected);
            for c in &mut next.candidates {
                if selected.contains(&c.statement_sha256) {
                    c.state = if decision == Decision::Reject {
                        State::Rejected
                    } else {
                        State::Proposed
                    };
                }
            }
        }
    }
    next.generation = next
        .generation
        .checked_add(1)
        .ok_or("kb-generation-overflow")?;
    next.history.push(S::Change {
        kind: format!("assessment-{:?}", decision).to_lowercase(),
        subjects: selected.clone(),
        evidence: key.clone(),
    });
    next = crate::knowledge_growth::add_cases(&next, &key)?;
    S::check(&next)?;
    Ok((next, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renamed_proposing_subject_is_not_held_out() {
        let mut suite: Suite =
            serde_json::from_str(include_str!("../tests/data/knowledge-suite/suite.json")).unwrap();
        assert!(suite.check().is_ok());
        suite
            .proposing_subjects
            .insert(suite.tasks[0].subject.sha256.clone());
        suite.tasks[0].name = "renamed".into();
        suite.tasks[0].sha256 = suite.tasks[0].digest().unwrap();
        assert_eq!(suite.check(), Err("kb-suite-task"));
    }
}
