//! Bounded curation, episode evidence and justification-preserving proposal growth.
use crate::knowledge_assessment::*;
use crate::knowledge_store::{self as S, Layer};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdviceUse {
    pub lesson: String,
    pub outcome: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Episode {
    pub schema: String,
    pub task: String,
    pub subject: Option<String>,
    pub scope: Scope,
    pub door: String,
    pub steps: Vec<String>,
    pub advice: Vec<AdviceUse>,
    pub counterexamples: Vec<Value>,
    pub stopping_bound: Option<String>,
    pub verdict: Option<Verdict>,
    pub receipt_sha256: Option<String>,
    pub work: BTreeMap<String, u64>,
    pub checker_sha256: String,
    pub engine_sha256: String,
    pub family: EvidenceFamily,
    pub model_calls: Option<u64>,
    pub tokens: Option<u64>,
    pub money_reserved: Option<u64>,
    pub owner_questions: Option<u64>,
}
impl Episode {
    pub(crate) fn evidence(&self) -> Result<(), &'static str> {
        if self.schema != "zeno-fcis/kb-episode/1"
            || !digest(&self.task)
            || self.subject.as_ref().is_none_or(|s| !digest(s))
            || !valid_scope(&self.scope)
            || self.verdict.is_none()
            || self.receipt_sha256.as_ref().is_none_or(|s| !digest(s))
            || self.work.is_empty()
            || self.work.len() > 16
            || self
                .work
                .values()
                .try_fold(0u64, |a, b| a.checked_add(*b))
                .is_none_or(|w| w > 1_000_000_000)
            || self.steps.len() > 64
            || self.counterexamples.len() > 64
            || self.advice.len() > 128
            || self.advice.iter().any(|a| {
                !matches!(
                    a.outcome.as_str(),
                    "consulted" | "applied" | "overridden" | "blocked"
                )
            })
        {
            return Err("kb-episode-evidence");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FactStatus {
    Proved,
    Refuted,
    Certified,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CertifiedFact {
    pub subject: String,
    pub checker_sha256: String,
    pub engine_sha256: String,
    pub tool_version: String,
    pub status: FactStatus,
    pub property: String,
    pub premises: Scope,
    pub door: String,
    pub run: Run,
    pub output_sha256: String,
    pub episode: String,
    pub source: String,
}
pub(crate) fn replay_proof(output: &Value, run: &Run) -> Result<(), &'static str> {
    let original = crate::neural_loop::program_json::program_from_json(
        &output["detail"]["original"]["program"],
    )
    .map_err(|_| "kb-proof-original")?;
    let candidate =
        crate::neural_loop::program_json::program_from_json(&output["detail"]["best"]["program"])
            .map_err(|_| "kb-proof-candidate")?;
    let original =
        crate::neural_loop::program_json::encode(&original).map_err(|_| "kb-proof-encoding")?;
    let candidate =
        crate::neural_loop::program_json::encode(&candidate).map_err(|_| "kb-proof-encoding")?;
    if crate::transform::sha256_hex(&original) != run.inputs {
        return Err("kb-proof-subject");
    }
    let tuples = output["detail"]["checker_limits"]["input_tuples"]
        .as_u64()
        .ok_or("kb-proof-limits")?;
    let steps = output["detail"]["checker_limits"]["steps"]
        .as_u64()
        .ok_or("kb-proof-limits")?;
    if !matches!(tuples, 4096 | crate::transform::DEFAULT_MAX_INPUT_TUPLES)
        || steps != crate::transform::DEFAULT_STEP_LIMIT
    {
        return Err("kb-proof-limits");
    }
    let limits = crate::transform::Limits {
        steps,
        input_tuples: tuples,
    };
    let checked =
        crate::transform::check(&original, &candidate, limits).map_err(|_| "kb-proof-replay")?;
    let receipt = checked.receipt();
    if crate::transform::sha256_hex(&receipt) != run.receipt_sha256
        || crate::transform::replay(&receipt, &original, &candidate, tuples)
            != crate::transform::Replay::Matched
    {
        return Err("kb-proof-replay");
    }
    Ok(())
}

pub(crate) fn replay_for_reuse(
    fact: &CertifiedFact,
    output: &Value,
    subject: &[u8],
    checker: &str,
) -> Result<(), &'static str> {
    if fact.subject != crate::transform::sha256_hex(subject) || fact.checker_sha256 != checker {
        return Err("kb-fact-subject-or-checker");
    }
    match fact.status {
        FactStatus::Proved => replay_proof(output, &fact.run),
        FactStatus::Refuted => replay_refutation(output, &fact.run),
        FactStatus::Certified => Err("kb-run-replay-required"),
    }
}
pub(crate) fn replay_refutation(output: &Value, run: &Run) -> Result<(), &'static str> {
    let original = crate::neural_loop::program_json::program_from_json(
        &output["detail"]["original"]["program"],
    )
    .map_err(|_| "kb-proof-original")?;
    let original =
        crate::neural_loop::program_json::encode(&original).map_err(|_| "kb-proof-encoding")?;
    if crate::transform::sha256_hex(&original) != run.inputs {
        return Err("kb-proof-subject");
    }
    let case = output["detail"]["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["check"]["verdict"] == "counterexample")
        .ok_or("kb-refutation-artifact")?;
    let candidate =
        crate::neural_loop::program_json::program_from_json(&case["candidate"]["program"])
            .map_err(|_| "kb-proof-candidate")?;
    let candidate =
        crate::neural_loop::program_json::encode(&candidate).map_err(|_| "kb-proof-encoding")?;
    let replay = crate::loop_command::doors::assessment_certificate(&original, &candidate)?;
    let checked = &replay["detail"]["candidates"][0]["check"];
    if checked["verdict"] != "counterexample"
        || checked["ordinal"] != case["check"]["ordinal"]
        || checked["input"] != case["check"]["input"]
        || checked["original"] != case["check"]["original"]
        || checked["candidate"] != case["check"]["candidate"]
    {
        return Err("kb-refutation-replay");
    }
    Ok(())
}
pub(crate) fn certify(
    layer: &Layer,
    output: &Value,
    scope: Scope,
    claim: Option<&str>,
) -> Result<(Layer, String), &'static str> {
    if !valid_scope(&scope) {
        return Err("kb-fact-scope");
    }
    let (engine, checker, version) = S::identity()?;
    let subject = output["detail"]["original"]["sha256"]
        .as_str()
        .filter(|s| digest(s))
        .ok_or("kb-proof-subject")?
        .to_owned();
    let candidate = output["detail"]["candidates"][0]["candidate"]["sha256"]
        .as_str()
        .filter(|s| digest(s))
        .ok_or("kb-proof-candidate")?;
    let property = format!("equivalent:{subject}:{candidate}");
    let task = S::hash(&(&property, &scope))?;
    let run =
        crate::knowledge_replay::measure(output, task.clone(), 0, 10_000_000, subject.clone())?;
    let status = match run.verdict {
        Verdict::Accepted => {
            replay_proof(output, &run)?;
            FactStatus::Proved
        }
        Verdict::Counterexample => {
            replay_refutation(output, &run)?;
            FactStatus::Refuted
        }
        _ => return Err("kb-artifact-inconclusive"),
    };
    let episode = Episode {
        schema: "zeno-fcis/kb-episode/1".into(),
        task,
        subject: Some(subject.clone()),
        scope: scope.clone(),
        door: "artifact-replay".into(),
        steps: vec!["check-program".into(), "replay-receipt".into()],
        advice: vec![],
        counterexamples: if status == FactStatus::Refuted {
            vec![output["detail"]["candidates"][0]["check"].clone()]
        } else {
            vec![]
        },
        stopping_bound: None,
        verdict: Some(run.verdict),
        receipt_sha256: Some(run.receipt_sha256.clone()),
        work: run.work.clone(),
        checker_sha256: checker.clone(),
        engine_sha256: engine.clone(),
        family: EvidenceFamily {
            subject: subject.clone(),
            session: run.output_sha256.clone(),
            actor: "artifact-replay".into(),
            tool: version.clone(),
            environment: "finite-checker".into(),
        },
        model_calls: None,
        tokens: None,
        money_reserved: None,
        owner_questions: None,
    };
    episode.evidence()?;
    let episode_key = S::hash(&episode)?;
    let fact = CertifiedFact {
        subject,
        checker_sha256: checker,
        engine_sha256: engine,
        tool_version: version,
        status: status.clone(),
        property: property.clone(),
        premises: scope,
        door: "artifact-replay".into(),
        run: run.clone(),
        output_sha256: run.output_sha256.clone(),
        episode: episode_key.clone(),
        source: "observational".into(),
    };
    let fact_key = S::hash(&fact)?;
    let mut next = layer.clone();
    next.outputs.insert(run.output_sha256, output.clone());
    next.episodes.insert(episode_key, episode);
    next.facts.insert(fact_key.clone(), fact);
    if let Some(id) = claim {
        let c = next
            .candidates
            .iter_mut()
            .find(|c| c.id == id || c.statement_sha256 == id)
            .ok_or("kb-candidate-missing")?;
        if c.claim != property || status != FactStatus::Refuted {
            return Err("kb-refuted-claim-binding");
        }
        c.state = State::Refuted;
        let roots = BTreeSet::from([c.statement_sha256.clone()]);
        next.candidates = retract(&next.candidates, &roots);
        next.history.push(S::Change {
            kind: "counterexample-refutation".into(),
            subjects: roots,
            evidence: fact_key.clone(),
        });
    }
    next.generation += 1;
    next.history.push(S::Change {
        kind: "artifact-certified".into(),
        subjects: BTreeSet::new(),
        evidence: fact_key.clone(),
    });
    S::check(&next)?;
    Ok((next, fact_key))
}
pub(crate) fn add_cases(layer: &Layer, assessment: &str) -> Result<Layer, &'static str> {
    let mut next = layer.clone();
    let a = layer
        .assessments
        .get(assessment)
        .ok_or("kb-assessment-missing")?;
    for c in &a.cases {
        for (on, run) in [(false, &c.off), (true, &c.on)] {
            let output = layer
                .outputs
                .get(&run.output_sha256)
                .ok_or("kb-assessment-output")?;
            let advice = a
                .candidates
                .iter()
                .map(|id| AdviceUse {
                    lesson: id.clone(),
                    outcome: if on && c.applicable {
                        "applied"
                    } else {
                        "consulted"
                    }
                    .into(),
                })
                .collect();
            let episode = Episode {
                schema: "zeno-fcis/kb-episode/1".into(),
                task: run.task.clone(),
                subject: Some(run.inputs.clone()),
                scope: c.scope.clone(),
                door: if on {
                    format!("pipeline-on-{}", S::hash(&a.candidates)?)
                } else {
                    "pipeline-off".into()
                },
                steps: output["detail"]["execution_trace"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                advice,
                counterexamples: output["detail"]["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|x| x["check"]["verdict"] == "counterexample")
                    .map(|x| x["check"].clone())
                    .collect(),
                stopping_bound: run.stopping_bound.clone(),
                verdict: Some(run.verdict),
                receipt_sha256: Some(run.receipt_sha256.clone()),
                work: run.work.clone(),
                checker_sha256: a.checker_sha256.clone(),
                engine_sha256: a.engine_sha256.clone(),
                family: EvidenceFamily {
                    subject: run.inputs.clone(),
                    session: a.revision_sha256.clone(),
                    actor: "assessment-process".into(),
                    tool: a.tool_version.clone(),
                    environment: "bounded-replay".into(),
                },
                model_calls: Some(0),
                tokens: Some(0),
                money_reserved: Some(0),
                owner_questions: Some(0),
            };
            episode.evidence()?;
            let episode_key = S::hash(&episode)?;
            next.episodes.insert(episode_key.clone(), episode.clone());
            if on && c.applicable {
                for candidate in &mut next.candidates {
                    if a.candidates.contains(&candidate.statement_sha256) {
                        candidate.episodes.insert(episode_key.clone());
                    }
                }
            }
            let status = if run.verdict == Verdict::Accepted {
                replay_proof(output, run)?;
                FactStatus::Proved
            } else if run.verdict == Verdict::Counterexample {
                FactStatus::Refuted
            } else {
                FactStatus::Certified
            };
            let fact = CertifiedFact {
                subject: run.inputs.clone(),
                checker_sha256: a.checker_sha256.clone(),
                engine_sha256: a.engine_sha256.clone(),
                tool_version: a.tool_version.clone(),
                status,
                property: if run.verdict.solved() {
                    "finite-program-equivalence"
                } else {
                    "exact-bounded-run"
                }
                .into(),
                premises: c.scope.clone(),
                door: episode.door.clone(),
                run: run.clone(),
                output_sha256: run.output_sha256.clone(),
                episode: episode_key,
                source: "held-out".into(),
            };
            next.facts.insert(S::hash(&fact)?, fact);
        }
    }
    Ok(next)
}
pub(crate) fn validate(layer: &Layer) -> Result<(), &'static str> {
    if layer.episodes.len() > 4096 || layer.facts.len() > 4096 {
        return Err("kb-case-base-budget");
    }
    for (id, e) in &layer.episodes {
        if S::hash(e)? != *id {
            return Err("kb-episode-binding");
        }
    }
    for (id, f) in &layer.facts {
        if S::hash(f)? != *id
            || f.subject != f.run.inputs
            || f.output_sha256 != f.run.output_sha256
            || !digest(&f.checker_sha256)
            || !digest(&f.engine_sha256)
        {
            return Err("kb-fact-binding");
        }
        let e = layer.episodes.get(&f.episode).ok_or("kb-episode-missing")?;
        e.evidence()?;
        if e.task != f.run.task
            || e.subject.as_ref() != Some(&f.subject)
            || e.verdict != Some(f.run.verdict)
            || e.work != f.run.work
            || e.receipt_sha256.as_ref() != Some(&f.run.receipt_sha256)
        {
            return Err("kb-fact-episode");
        }
        if e.scope != f.premises
            || e.door != f.door
            || e.checker_sha256 != f.checker_sha256
            || e.engine_sha256 != f.engine_sha256
        {
            return Err("kb-fact-episode");
        }
        if !matches!(f.source.as_str(), "held-out" | "observational") {
            return Err("kb-fact-source");
        }
        if f.source == "held-out"
            && !layer.assessments.values().any(|a| {
                a.engine_sha256 == f.engine_sha256
                    && a.checker_sha256 == f.checker_sha256
                    && a.cases
                        .iter()
                        .any(|c| c.scope == f.premises && (c.off == f.run || c.on == f.run))
            })
        {
            return Err("kb-fact-held-out-source");
        }
        let output = layer
            .outputs
            .get(&f.output_sha256)
            .ok_or("kb-fact-artifact")?;
        if crate::knowledge_replay::measure(
            output,
            f.run.task.clone(),
            f.run.seed,
            f.run.budget,
            f.run.inputs.clone(),
        )? != f.run
        {
            return Err("kb-fact-artifact-measurements");
        }
        if f.status == FactStatus::Proved {
            replay_proof(output, &f.run)?;
        }
        if f.status == FactStatus::Refuted {
            if f.run.verdict != Verdict::Counterexample {
                return Err("kb-refutation-artifact");
            }
            replay_refutation(output, &f.run)?;
        }
    }
    Ok(())
}
pub(crate) fn telemetry(layer: &Layer) -> Value {
    let mut rows = BTreeMap::<(String, String), Value>::new();
    for e in layer.episodes.values() {
        for advice in &e.advice {
            let key = (advice.lesson.clone(), S::hash(&e.scope).unwrap_or_default());
            let row=rows.entry(key).or_insert_with(||json!({"applied":0,"overridden":0,"blocked":0,"consulted":0,"checker_outcomes":{},"work":0,"reassessment_required":false}));
            let n = row[&advice.outcome].as_u64().unwrap_or(0);
            row[&advice.outcome] = json!(n + 1);
            let work = e.work.values().copied().sum::<u64>();
            row["work"] = json!(row["work"].as_u64().unwrap_or(0).saturating_add(work));
            let verdict = format!("{:?}", e.verdict);
            let n = row["checker_outcomes"][&verdict].as_u64().unwrap_or(0);
            row["checker_outcomes"][&verdict] = json!(n + 1);
            if advice.outcome == "applied" {
                if let Some(c) = layer
                    .candidates
                    .iter()
                    .find(|c| c.id == advice.lesson || c.statement_sha256 == advice.lesson)
                {
                    if let Some(a) = c.assessment.as_ref().and_then(|a| layer.assessments.get(a)) {
                        for case in &a.cases {
                            if case.scope == e.scope
                                && (e.verdict.is_some_and(|v| !v.solved())
                                    && case.off.verdict.solved()
                                    || e.work.iter().any(|(k, v)| {
                                        case.off.work.get(k).is_some_and(|base| v > base)
                                    }))
                            {
                                row["reassessment_required"] = json!(true);
                            }
                        }
                    }
                }
            }
        }
    }
    json!({"kind":"observational-only","promotes":false,"rows":rows.into_iter().map(|((lesson,scope),v)|json!({"lesson":lesson,"scope_sha256":scope,"outcomes":v})).collect::<Vec<_>>()})
}
fn proposal(mut c: Candidate, origin: &str) -> Result<Candidate, &'static str> {
    c.state = State::Proposed;
    c.assessment = None;
    c.origin = origin.into();
    c.statement_sha256 = S::statement(&c)?;
    c.id = format!("grown-{}", &c.statement_sha256[..24]);
    Ok(c)
}
/// Only declared isa parents and declared dimensions are available to induction.
pub(crate) fn grow(layer: &Layer) -> Result<Vec<Candidate>, &'static str> {
    let mut out = Vec::new();
    let proven = layer
        .candidates
        .iter()
        .filter(|c| c.state == State::Promoted)
        .collect::<Vec<_>>();
    for (i, a) in proven.iter().enumerate() {
        for b in proven.iter().skip(i + 1) {
            if a.action != b.action || a.effect != b.effect {
                continue;
            }
            for dimension in ["family", "tool"] {
                let (Some(av), Some(bv)) = (
                    a.scope.get(dimension).and_then(|v| v.first()),
                    b.scope.get(dimension).and_then(|v| v.first()),
                ) else {
                    continue;
                };
                if av == bv {
                    continue;
                }
                for (_, parent) in layer.isa.iter().filter(|(child, _)| child == av) {
                    if !member(bv, parent, &layer.isa) {
                        continue;
                    }
                    let mut left = a.scope.clone();
                    let mut right = b.scope.clone();
                    left.remove(dimension);
                    right.remove(dimension);
                    if left != right {
                        continue;
                    }
                    let mut c = (*a).clone();
                    c.scope
                        .insert(dimension.into(), BTreeSet::from([parent.clone()]));
                    c.parents =
                        BTreeSet::from([a.statement_sha256.clone(), b.statement_sha256.clone()]);
                    for p in [*a, *b] {
                        c.proposing_tasks.extend(p.proposing_tasks.iter().cloned());
                        c.families.extend(p.families.iter().cloned());
                        c.episodes.extend(p.episodes.iter().cloned());
                        if let Some(r) =
                            p.assessment.as_ref().and_then(|h| layer.assessments.get(h))
                        {
                            for case in &r.cases {
                                c.proposing_tasks.insert(case.task.clone());
                                c.families.insert(EvidenceFamily {
                                    subject: case.off.inputs.clone(),
                                    session: r.revision_sha256.clone(),
                                    actor: "assessed-case".into(),
                                    tool: r.tool_version.clone(),
                                    environment: "held-out-parent".into(),
                                });
                            }
                        }
                    }
                    out.push(proposal(c, "isa-generalization")?);
                }
            }
        }
    }
    for c in &layer.candidates {
        let Some(a) = c.assessment.as_ref().and_then(|h| layer.assessments.get(h)) else {
            continue;
        };
        let helps = a
            .cases
            .iter()
            .filter(|case| {
                case.applicable
                    && decide(&[(*case).clone()], &BTreeSet::new(), &a.policy, &[])
                        .is_ok_and(|x| x.0 == Decision::Promote)
            })
            .collect::<Vec<_>>();
        let hurts = a
            .cases
            .iter()
            .filter(|case| {
                case.applicable
                    && decide(&[(*case).clone()], &BTreeSet::new(), &a.policy, &[])
                        .is_ok_and(|x| x.0 == Decision::Reject)
            })
            .collect::<Vec<_>>();
        if helps.is_empty() || hurts.is_empty() {
            continue;
        }
        for dimension in DIMENSIONS {
            for positive in &helps {
                let Some(values) = positive.scope.get(dimension) else {
                    continue;
                };
                if hurts
                    .iter()
                    .any(|negative| negative.scope.get(dimension) == Some(values))
                {
                    continue;
                }
                let mut narrow = c.clone();
                narrow.scope.insert(dimension.into(), values.clone());
                narrow.parents = BTreeSet::from([c.statement_sha256.clone()]);
                for case in &a.cases {
                    narrow.proposing_tasks.insert(case.task.clone());
                    narrow.families.insert(EvidenceFamily {
                        subject: case.off.inputs.clone(),
                        session: a.revision_sha256.clone(),
                        actor: "assessed-case".into(),
                        tool: a.tool_version.clone(),
                        environment: "specialization-proposal".into(),
                    });
                }
                out.push(proposal(narrow, "episode-specialization")?);
            }
        }
    }
    out.sort_by(|a, b| a.statement_sha256.cmp(&b.statement_sha256));
    out.dedup_by(|a, b| a.statement_sha256 == b.statement_sha256);
    Ok(out)
}
pub(crate) fn mine(episodes: &BTreeMap<String, Episode>) -> Result<Vec<Candidate>, &'static str> {
    let mut groups = BTreeMap::<(String, String, String), Vec<(&String, &Episode)>>::new();
    for (id, e) in episodes {
        if e.evidence().is_err() {
            continue;
        }
        if let Some(reason) = &e.stopping_bound {
            let family = e
                .scope
                .get("family")
                .and_then(|v| v.first())
                .cloned()
                .unwrap_or_default();
            groups
                .entry((e.door.clone(), family, reason.clone()))
                .or_default()
                .push((id, e));
        }
    }
    let mut out = Vec::new();
    for ((door, _family, reason), group) in groups {
        if group
            .iter()
            .filter_map(|(_, e)| e.subject.as_ref())
            .collect::<BTreeSet<_>>()
            .len()
            < 2
        {
            continue;
        }
        let mut scope = group[0].1.scope.clone();
        scope.retain(|k, v| group.iter().all(|(_, e)| e.scope.get(k) == Some(v)));
        let c = Candidate {
            id: String::new(),
            statement_sha256: String::new(),
            subject: "pipeline".into(),
            claim: format!("failure:{door}:{reason}"),
            scope,
            effect: if reason.contains("budget") {
                "limit"
            } else {
                "defect"
            }
            .into(),
            action: Action {
                skip: BTreeSet::new(),
                order: vec![],
            },
            proposing_tasks: group.iter().map(|(_, e)| e.task.clone()).collect(),
            families: group.iter().map(|(_, e)| e.family.clone()).collect(),
            episodes: group.iter().map(|(id, _)| (*id).clone()).collect(),
            parents: BTreeSet::new(),
            dependencies: BTreeMap::new(),
            state: State::Proposed,
            assessment: None,
            observed_work: group
                .iter()
                .map(|(_, e)| e.work.values().copied().sum::<u64>())
                .sum(),
            origin: format!("failure-signature:{door}:{reason}"),
        };
        out.push(proposal(c, "failure-mining")?);
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proved_artifact_requires_replay_and_exact_limits() {
        let bytes =
            include_bytes!("../tests/fixtures/i128-door-programs/bounded-counter-c3.original.zcve");
        let output = crate::loop_command::doors::assessment_certificate(bytes, bytes).unwrap();
        let run = crate::knowledge_replay::measure(
            &output,
            "a".repeat(64),
            0,
            100000,
            crate::transform::sha256_hex(bytes),
        )
        .unwrap();
        assert!(replay_proof(&output, &run).is_ok());
        let mut changed = output.clone();
        changed["detail"]["checker_limits"]["input_tuples"] =
            json!(crate::transform::DEFAULT_MAX_INPUT_TUPLES);
        assert!(replay_proof(&changed, &run).is_err());
        let mut absent = output;
        absent["detail"]["best"]["program"] = Value::Null;
        assert!(replay_proof(&absent, &run).is_err());
    }
    #[test]
    fn reused_result_must_replay_receipt() {
        let bytes =
            include_bytes!("../tests/fixtures/i128-door-programs/bounded-counter-c3.original.zcve");
        let mut output = crate::loop_command::doors::assessment_certificate(bytes, bytes).unwrap();
        let run = crate::knowledge_replay::measure(
            &output,
            "a".repeat(64),
            0,
            100000,
            crate::transform::sha256_hex(bytes),
        )
        .unwrap();
        let fact = CertifiedFact {
            subject: run.inputs.clone(),
            checker_sha256: "checker".into(),
            engine_sha256: "engine".into(),
            tool_version: "v".into(),
            status: FactStatus::Proved,
            property: "finite-program-equivalence".into(),
            premises: Scope::new(),
            door: "artifact-replay".into(),
            output_sha256: run.output_sha256.clone(),
            episode: "episode".into(),
            source: "observational".into(),
            run,
        };
        assert!(replay_for_reuse(&fact, &output, bytes, "checker").is_ok());
        output["detail"]["best"]["program"] = Value::Null;
        assert!(replay_for_reuse(&fact, &output, bytes, "checker").is_err());
    }
    #[test]
    fn tier_one_proof_is_bound_to_its_subject() {
        let bytes =
            include_bytes!("../tests/fixtures/i128-door-programs/bounded-counter-c3.original.zcve");
        let output = crate::loop_command::doors::assessment_certificate(bytes, bytes).unwrap();
        let mut run = crate::knowledge_replay::measure(
            &output,
            "a".repeat(64),
            0,
            100000,
            crate::transform::sha256_hex(bytes),
        )
        .unwrap();
        run.inputs = "e".repeat(64);
        assert_eq!(replay_proof(&output, &run), Err("kb-proof-subject"));
    }
    #[test]
    fn episode_requires_verdict_and_receipt() {
        let e = Episode {
            schema: "zeno-fcis/kb-episode/1".into(),
            task: "a".repeat(64),
            subject: Some("b".repeat(64)),
            scope: Scope::new(),
            door: "pipeline".into(),
            steps: vec![],
            advice: vec![],
            counterexamples: vec![],
            stopping_bound: None,
            verdict: None,
            receipt_sha256: None,
            work: BTreeMap::from([("check_requests".into(), 1)]),
            checker_sha256: "c".repeat(64),
            engine_sha256: "d".repeat(64),
            family: EvidenceFamily {
                subject: "b".repeat(64),
                session: "s".into(),
                actor: "a".into(),
                tool: "v".into(),
                environment: "e".into(),
            },
            model_calls: None,
            tokens: None,
            money_reserved: None,
            owner_questions: None,
        };
        assert_eq!(e.evidence(), Err("kb-episode-evidence"));
        let mut complete = e.clone();
        complete.verdict = Some(Verdict::Accepted);
        complete.receipt_sha256 = Some("e".repeat(64));
        assert!(complete.evidence().is_ok());
        let mut no_verdict = complete.clone();
        no_verdict.verdict = None;
        assert!(no_verdict.evidence().is_err());
        complete.receipt_sha256 = None;
        assert!(complete.evidence().is_err());
    }
    #[test]
    fn repeated_family_counts_once() {
        let f = EvidenceFamily {
            subject: "s".into(),
            session: "x".into(),
            actor: "a".into(),
            tool: "v".into(),
            environment: "e".into(),
        };
        assert_eq!(BTreeSet::from([f.clone(), f]).len(), 1);
    }
}
