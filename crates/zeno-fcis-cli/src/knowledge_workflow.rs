//! Native knowledge commands; all writes remain explicit and private.
use crate::knowledge_assessment::*;
use crate::knowledge_command::Operation;
use crate::knowledge_store::{self as S, Layer};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) fn handles(op: &Operation) -> bool {
    matches!(
        op,
        Operation::Init { .. }
            | Operation::Migrate { .. }
            | Operation::Curate { .. }
            | Operation::Assess { .. }
            | Operation::AssessmentQueue { .. }
            | Operation::AssessmentView
            | Operation::ReplayAssessment { .. }
            | Operation::AssessmentTask { .. }
            | Operation::Grow
            | Operation::IngestEpisodes { .. }
            | Operation::Refresh
            | Operation::Telemetry
            | Operation::Leaderboard { .. }
            | Operation::Facts { .. }
            | Operation::Certify { .. }
            | Operation::Reuse { .. }
    ) || matches!(op,Operation::Explain{query,..} if !query.contains('('))
}
fn layer_path(given: &[PathBuf]) -> Result<PathBuf, &'static str> {
    let paths = crate::knowledge_command::paths(given).map_err(|_| "kb-paths")?;
    if paths.len() != 1 {
        return Err("kb-one-assessment-layer-required");
    }
    Ok(paths[0].clone())
}
pub(crate) fn invoke(given: &[PathBuf], op: Operation) -> (u8, Value) {
    match execute(given, op) {
        Ok(v) => (
            if v["status"] == "refused" {
                1
            } else if v["status"] == "inconclusive" {
                2
            } else {
                0
            },
            v,
        ),
        Err(e) => (1, S::error(e)),
    }
}
fn execute(given: &[PathBuf], op: Operation) -> Result<Value, &'static str> {
    match op {
        Operation::Init { out } => {
            let sha = S::commit(&out, None, &Layer::default())?;
            Ok(json!({"status":"created","authority":"none","path":out,"sha256":sha}))
        }
        Operation::Migrate { source, out } => {
            let data = S::read(&source, 65_536)?;
            let v: Value = serde_json::from_slice(&data).map_err(|_| "kb-layer-schema")?;
            if v["schema"] != "zeno-fcis/kb-layer/1" {
                return Err("kb-layer-schema");
            }
            let input: crate::knowledge_evidence::Input =
                serde_json::from_value(v["knowledge"].clone()).map_err(|_| "kb-layer-schema")?;
            let legacy = crate::knowledge_evidence::bound_input(
                source.parent().ok_or("kb-parent")?,
                &input,
                65_536,
            )
            .map_err(|_| "kb-migration-input")?;
            let layer = Layer {
                legacy: Some(String::from_utf8(legacy).map_err(|_| "kb-utf8")?),
                history: vec![S::Change {
                    kind: "migrated-pending".into(),
                    subjects: BTreeSet::new(),
                    evidence: crate::transform::sha256_hex(&data),
                }],
                ..Layer::default()
            };
            let sha = S::commit(&out, None, &layer)?;
            Ok(json!({"status":"migrated-pending","authority":"none","path":out,"sha256":sha}))
        }
        Operation::AssessmentTask { request, out } => {
            crate::knowledge_replay::worker(&request, &out)?;
            Ok(json!({"status":"recorded","authority":"none"}))
        }
        other => {
            let path = layer_path(given)?;
            let (layer, binding) = S::live(&path)?;
            match other {
                Operation::ReplayAssessment { assessment, suite } => {
                    let suite_data: crate::knowledge_replay::Suite =
                        serde_json::from_slice(&S::read(&suite, 1_048_576)?)
                            .map_err(|_| "kb-suite-schema")?;
                    crate::knowledge_replay::replay_record(
                        &layer,
                        &assessment,
                        &suite_data,
                        suite.parent().ok_or("kb-parent")?,
                        path.parent().ok_or("kb-parent")?,
                    )
                }
                Operation::Curate { source } => {
                    let bytes = S::read(&source, 2 * 1024 * 1024)?;
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Batch {
                        schema: String,
                        candidates: Vec<Candidate>,
                        isa: BTreeSet<(String, String)>,
                    }
                    let batch: Batch =
                        serde_json::from_slice(&bytes).map_err(|_| "kb-curation-source")?;
                    if batch.schema != "zeno-fcis/kb-candidates/1" || batch.candidates.len() > 128 {
                        return Err("kb-curation-source");
                    }
                    let mut next = layer.clone();
                    next.isa.extend(batch.isa);
                    for mut c in batch.candidates {
                        c.action.check()?;
                        c.scope = c
                            .scope
                            .into_iter()
                            .map(|(key, values)| {
                                (
                                    key,
                                    values.into_iter().map(|v| v.trim().to_owned()).collect(),
                                )
                            })
                            .collect();
                        if !c.origin.is_empty() {
                            c.evidence_refs.insert(c.origin.clone());
                        }
                        c.state = State::Proposed;
                        c.assessment = None;
                        c.statement_sha256 = S::statement(&c)?;
                        if let Some(old) = next
                            .candidates
                            .iter_mut()
                            .find(|x| x.statement_sha256 == c.statement_sha256)
                        {
                            old.families.extend(c.families);
                            old.evidence_refs.extend(c.evidence_refs);
                            old.episodes.extend(c.episodes);
                            old.proposing_tasks.extend(c.proposing_tasks);
                            old.observed_work = old.observed_work.max(c.observed_work);
                        } else {
                            next.candidates.push(c);
                        }
                        next.curation_work = next
                            .curation_work
                            .checked_add(1)
                            .ok_or("kb-work-overflow")?;
                    }
                    next.generation += 1;
                    next.history.push(S::Change {
                        kind: "curated-proposals".into(),
                        subjects: BTreeSet::new(),
                        evidence: crate::transform::sha256_hex(&bytes),
                    });
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    Ok(
                        json!({"status":"curated","authority":"none","path":path,"sha256":sha,"queue":queue(&next,None)}),
                    )
                }
                Operation::Assess {
                    suite,
                    candidate,
                    total_budget,
                    minimum_effect_bps,
                    annotations,
                } => {
                    let bytes = S::read(&suite, 1_048_576)?;
                    let suite_data: crate::knowledge_replay::Suite =
                        serde_json::from_slice(&bytes).map_err(|_| "kb-suite-schema")?;
                    let selected = if let Some(id) = candidate {
                        BTreeSet::from([layer
                            .candidates
                            .iter()
                            .find(|c| c.id == id || c.statement_sha256 == id)
                            .ok_or("kb-candidate-missing")?
                            .statement_sha256
                            .clone()])
                    } else {
                        layer
                            .candidates
                            .iter()
                            .filter(|c| c.state == State::Promoted)
                            .map(|c| c.statement_sha256.clone())
                            .collect()
                    };
                    let notes = if let Some(file) = annotations {
                        serde_json::from_slice::<Vec<Annotation>>(&S::read(&file, 65_536)?)
                            .map_err(|_| "kb-review-annotation")?
                    } else {
                        vec![]
                    };
                    let (next, key) = crate::knowledge_replay::assess(
                        &layer,
                        &suite_data,
                        suite.parent().ok_or("kb-parent")?,
                        &selected,
                        total_budget,
                        Policy {
                            minimum_effect_bps,
                            ..Policy::default()
                        },
                        notes,
                        path.parent().ok_or("kb-parent")?,
                    )?;
                    publish_assessment(&path, &binding, &next, &key)
                }
                Operation::Grow => {
                    let mut next = layer.clone();
                    let mut proposals = crate::knowledge_growth::grow(&layer)?;
                    proposals.extend(crate::knowledge_growth::mine(&layer.episodes)?);
                    let mut inserted = BTreeSet::new();
                    for c in proposals {
                        if !next
                            .candidates
                            .iter()
                            .any(|x| x.statement_sha256 == c.statement_sha256)
                        {
                            inserted.insert(c.statement_sha256.clone());
                            next.candidates.push(c);
                        }
                    }
                    next.curation_work = next.curation_work.saturating_add(inserted.len() as u64);
                    next.generation += 1;
                    next.history.push(S::Change {
                        kind: "growth-proposals".into(),
                        subjects: inserted,
                        evidence: S::hash(&layer)?,
                    });
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    Ok(
                        json!({"status":"proposed","authority":"none","sha256":sha,"queue":queue(&next,None)}),
                    )
                }
                Operation::IngestEpisodes { source } => {
                    let bytes = S::read(&source, 2 * 1024 * 1024)?;
                    let value: Value =
                        serde_json::from_slice(&bytes).map_err(|_| "kb-episode-source")?;
                    let episodes: Vec<crate::knowledge_growth::Episode> = if value.is_array() {
                        serde_json::from_value(value).map_err(|_| "kb-episode-source")?
                    } else {
                        vec![serde_json::from_value(value).map_err(|_| "kb-episode-source")?]
                    };
                    if episodes.len() > 512 {
                        return Err("kb-episode-budget");
                    }
                    let mut next = layer.clone();
                    for e in episodes {
                        e.evidence()?;
                        let observed_work = e.work.values().copied().sum::<u64>();
                        let fresh = next.episodes.insert(S::hash(&e)?, e).is_none();
                        next.curation_work = next
                            .curation_work
                            .checked_add(1 + if fresh { observed_work } else { 0 })
                            .ok_or("kb-work-overflow")?;
                    }
                    next.generation += 1;
                    next.history.push(S::Change {
                        kind: "observational-episodes".into(),
                        subjects: BTreeSet::new(),
                        evidence: crate::transform::sha256_hex(&bytes),
                    });
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    Ok(
                        json!({"status":"observed","authority":"none","sha256":sha,"telemetry":crate::knowledge_growth::telemetry(&next)}),
                    )
                }
                Operation::Refresh => {
                    let mut next = layer.clone();
                    next.generation += 1;
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    Ok(
                        json!({"status":"pending-refreshed","authority":"none","sha256":sha,"history":next.history}),
                    )
                }
                Operation::Leaderboard { population } => {
                    let (engine, checker, _) = S::identity()?;
                    crate::knowledge_metrics::profiles(&layer, &checker, &engine, &population)
                }
                Operation::Telemetry => Ok(
                    json!({"status":"observational","authority":"none","telemetry":crate::knowledge_growth::telemetry(&layer)}),
                ),
                Operation::Certify {
                    original,
                    candidate,
                    scope,
                    refutes,
                } => {
                    let original = S::read(&original, 1_048_576)?;
                    let candidate = S::read(&candidate, 1_048_576)?;
                    let scope =
                        serde_json::from_str::<Scope>(&scope).map_err(|_| "kb-fact-scope")?;
                    let output =
                        crate::loop_command::doors::assessment_certificate(&original, &candidate)?;
                    let (next, key) = crate::knowledge_growth::certify(
                        &layer,
                        &output,
                        scope,
                        refutes.as_deref(),
                    )?;
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    Ok(
                        json!({"status":"artifact-replayed","authority":"none","fact_sha256":key,"layer_sha256":sha}),
                    )
                }
                Operation::Facts { subject } => {
                    if !digest(&subject) {
                        return Err("kb-exact-subject-required");
                    }
                    let (_, checker, _) = S::identity()?;
                    let facts = layer
                        .facts
                        .iter()
                        .filter(|(_, f)| f.subject == subject && f.checker_sha256 == checker)
                        .map(|(k, f)| json!({"sha256":k,"fact":f,"requires_receipt_replay":true}))
                        .collect::<Vec<_>>();
                    Ok(
                        json!({"status":"certified-facts","authority":"none","facts":facts,"strategy_advice":false}),
                    )
                }
                Operation::Reuse { fact, subject } => {
                    let f = layer.facts.get(&fact).ok_or("kb-fact-missing")?;
                    let data = S::read(&subject, 1_048_576)?;
                    let (_, checker, _) = S::identity()?;
                    crate::knowledge_growth::replay_for_reuse(
                        f,
                        layer
                            .outputs
                            .get(&f.output_sha256)
                            .ok_or("kb-fact-artifact")?,
                        &data,
                        &checker,
                    )?;
                    Ok(
                        json!({"status":"receipt-replayed","authority":"none","fact_sha256":fact,"subject_sha256":f.subject,"receipt_sha256":f.run.receipt_sha256}),
                    )
                }
                Operation::Explain { query, .. } => {
                    let c = layer
                        .candidates
                        .iter()
                        .find(|c| c.id == query || c.statement_sha256 == query)
                        .ok_or("kb-candidate-missing")?;
                    let mut parents = BTreeSet::new();
                    let mut todo = c.parents.iter().cloned().collect::<Vec<_>>();
                    while let Some(p) = todo.pop() {
                        if parents.insert(p.clone()) {
                            if let Some(parent) =
                                layer.candidates.iter().find(|c| c.statement_sha256 == p)
                            {
                                todo.extend(parent.parents.iter().cloned());
                            }
                        }
                    }
                    let episodes = c
                        .episodes
                        .iter()
                        .filter_map(|id| {
                            layer
                                .episodes
                                .get(id)
                                .map(|e| json!({"sha256":id,"episode":e}))
                        })
                        .collect::<Vec<_>>();
                    Ok(
                        json!({"status":"explained","authority":"none","candidate":c,"assessment":c.assessment.as_ref().and_then(|a|layer.assessments.get(a)),"episodes":episodes,"justification_chain":parents.into_iter().filter_map(|p|layer.candidates.iter().find(|c|c.statement_sha256==p)).collect::<Vec<_>>(),"derived_memberships":layer.isa.iter().filter(|(_,p)|c.scope.values().any(|v|v.contains(p))).map(|(child,parent)|json!({"conclusion":[child,parent],"witness":"declared-isa-edge"})).collect::<Vec<_>>()}),
                    )
                }
                Operation::AssessmentQueue { suite } => {
                    let given = if let Some(file) = suite {
                        let s: crate::knowledge_replay::Suite =
                            serde_json::from_slice(&S::read(&file, 1_048_576)?)
                                .map_err(|_| "kb-suite-schema")?;
                        s.check()?;
                        Some(s)
                    } else {
                        None
                    };
                    let stored = layer
                        .assessments
                        .values()
                        .filter_map(|a| layer.outputs.get(&a.suite_sha256))
                        .find_map(|v| {
                            serde_json::from_value::<crate::knowledge_replay::Suite>(v.clone()).ok()
                        });
                    Ok(
                        json!({"status":"assessment-queue","authority":"none","queue":queue(&layer,given.as_ref().or(stored.as_ref())),"scores_have_promotion_authority":false}),
                    )
                }
                Operation::AssessmentView => Ok(
                    json!({"status":"assessments","authority":"none","assessments":layer.assessments,"history":layer.history}),
                ),
                _ => Err("kb-assessment-routing"),
            }
        }
    }
}
/// Publication is shell work; a refused publication retains the measured result.
pub(crate) fn publish_assessment(
    path: &std::path::Path,
    binding: &str,
    next: &Layer,
    key: &str,
) -> Result<Value, &'static str> {
    let record = next.assessments.get(key).ok_or("kb-assessment-missing")?;
    match S::commit(path, Some(binding), next) {
        Ok(sha) => Ok(
            json!({"status":if record.decision==Decision::Inconclusive{"inconclusive"}else{"assessed"},"authority":"none","publication":"confirmed","assessment_sha256":key,"layer_sha256":sha,"assessment":record,"aggregate":aggregate(record)}),
        ),
        Err(reason) => {
            let recovery = match S::retain_unpublished(path, next) {
                Ok(file) => {
                    json!({"path":path.parent().ok_or("kb-parent")?.join(&file.path),"sha256":file.sha256,"schema":S::SCHEMA,"authority":"none","is_layer_descriptor":false})
                }
                Err(error) => json!({"reason":error,"authority":"none"}),
            };
            let mut value = S::error(reason);
            value["assessment_sha256"] = json!(key);
            value["assessment"] = json!(record);
            value["aggregate"] = aggregate(record);
            value["publication"] = json!("unconfirmed");
            value["recovery"] = recovery;
            value["next_step"] = json!(
                "Inspect the retained assessment and current layer revision before retrying. The recovery snapshot is not a knowledge layer."
            );
            Ok(value)
        }
    }
}

pub(crate) fn aggregate(a: &Assessment) -> Value {
    let sum = |on: bool| {
        a.cases
            .iter()
            .map(|c| if on { &c.on } else { &c.off })
            .fold((0u64, 0u64), |(solved, work), r| {
                (
                    solved + u64::from(r.verdict.solved()),
                    work.saturating_add(r.work.values().copied().sum::<u64>()),
                )
            })
    };
    let off = sum(false);
    let on = sum(true);
    json!({"scope":"recorded-frozen-suite-only","off":{"solved":off.0,"work":off.1},"on":{"solved":on.0,"work":on.1},"assessment_work":a.assessment_work,"assessment_work_reserved":a.reserved_work,"prior_assessment_work":a.prior_assessment_work,"curation_work":a.curation_work,"on_total_with_assessment_and_curation":on.1.saturating_add(a.assessment_work).saturating_add(a.prior_assessment_work).saturating_add(a.curation_work),"wall_time_used":false})
}
pub(crate) fn queue(layer: &Layer, suite: Option<&crate::knowledge_replay::Suite>) -> Value {
    let mut rows = layer
        .candidates
        .iter()
        .filter(|c| matches!(c.state, State::Proposed | State::Pending))
        .map(|c| {
            let applicable = suite
                .map(|s| {
                    s.tasks
                        .iter()
                        .filter(|t| applies(&c.scope, &t.scope, &layer.isa))
                        .count()
                })
                .unwrap_or(0);
            (c.families.len(), applicable, c.observed_work, c)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(a.3.statement_sha256.cmp(&b.3.statement_sha256))
    });
    json!(rows.into_iter().map(|(families,applicable,work,c)|json!({"id":c.id,"statement_sha256":c.statement_sha256,"state":c.state,"strength":if c.assessment.as_ref().and_then(|h|layer.assessments.get(h)).is_some_and(|a|a.cases.iter().any(|case|case.applicable)){"measured"}else if families>=2{"observed"}else{"proposed"},"independent_families":families,"applicable_tasks":applicable,"observed_work":work,"conflicts":layer.candidates.iter().filter(|other|other.statement_sha256!=c.statement_sha256 && S::conflicts(c,other,&layer.isa)).map(|c|&c.id).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
pub(crate) fn task_scope(facts: &[zeno_fcis_loop_doors::strategy_knowledge::Fact]) -> Scope {
    use zeno_fcis_loop_doors::strategy_knowledge::Value as V;
    let mut scope = BTreeMap::new();
    for f in facts {
        if f.predicate == "task" {
            if let [V::Text(k), V::Text(v)] = f.arguments.as_slice() {
                if DIMENSIONS.contains(&k.as_str()) {
                    scope
                        .entry(k.clone())
                        .or_insert_with(BTreeSet::new)
                        .insert(v.clone());
                }
            }
        }
    }
    scope
}
pub(crate) fn layer_conflict(layers: &[Layer]) -> bool {
    let isa: BTreeSet<(String, String)> =
        layers.iter().flat_map(|l| l.isa.iter().cloned()).collect();
    if isa.iter().any(|(c, p)| c == p || member(p, c, &isa)) {
        return true;
    }
    let candidates = layers
        .iter()
        .flat_map(|l| l.candidates.iter())
        .collect::<Vec<_>>();
    candidates.iter().enumerate().any(|(i, a)| {
        candidates.iter().skip(i + 1).any(|b| {
            a.id == b.id && *a != *b
                || a.state == State::Promoted
                    && b.state == State::Promoted
                    && S::conflicts(a, b, &isa)
        })
    })
}
pub(crate) fn advice<'a>(
    layers: &'a [Layer],
    facts: &[zeno_fcis_loop_doors::strategy_knowledge::Fact],
) -> Vec<&'a Candidate> {
    use zeno_fcis_loop_doors::strategy_knowledge::Value as V;
    if !facts.iter().any(|f| {
        f.predicate == "task"
            && f.arguments == vec![V::Text("subject".into()), V::Text("pipeline".into())]
    }) {
        return vec![];
    }
    if layer_conflict(layers) {
        return vec![];
    }
    let scope = task_scope(facts);
    let mut seen = BTreeSet::new();
    let isa: BTreeSet<(String, String)> =
        layers.iter().flat_map(|l| l.isa.iter().cloned()).collect();
    layers
        .iter()
        .flat_map(|l| {
            l.candidates
                .iter()
                .filter(|c| c.state == State::Promoted && applies(&c.scope, &scope, &isa))
        })
        .filter(|candidate| seen.insert(candidate.statement_sha256.clone()))
        .collect()
}
