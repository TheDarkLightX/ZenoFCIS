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
            | Operation::AssessmentQueue
            | Operation::AssessmentView
            | Operation::AssessmentTask { .. }
    )
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
        Ok(v) => (if v["status"] == "inconclusive" { 2 } else { 0 }, v),
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
                        c.state = State::Proposed;
                        c.assessment = None;
                        c.statement_sha256 = S::statement(&c)?;
                        if let Some(old) = next
                            .candidates
                            .iter_mut()
                            .find(|x| x.statement_sha256 == c.statement_sha256)
                        {
                            old.families.extend(c.families);
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
                    let sha = S::commit(&path, Some(&binding), &next)?;
                    let record = &next.assessments[&key];
                    Ok(
                        json!({"status":if record.decision==Decision::Inconclusive{"inconclusive"}else{"assessed"},"authority":"none","assessment_sha256":key,"layer_sha256":sha,"assessment":record,"aggregate":aggregate(record)}),
                    )
                }
                Operation::AssessmentQueue => Ok(
                    json!({"status":"assessment-queue","authority":"none","queue":queue(&layer,None),"scores_have_promotion_authority":false}),
                ),
                Operation::AssessmentView => Ok(
                    json!({"status":"assessments","authority":"none","assessments":layer.assessments,"history":layer.history}),
                ),
                _ => Err("kb-assessment-routing"),
            }
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
    json!({"scope":"recorded-frozen-suite-only","off":{"solved":off.0,"work":off.1},"on":{"solved":on.0,"work":on.1},"assessment_work_reserved":a.assessment_work,"curation_work":a.curation_work,"on_total_with_assessment_and_curation":on.1.saturating_add(a.assessment_work).saturating_add(a.curation_work),"wall_time_used":false})
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
    json!(rows.into_iter().map(|(families,applicable,work,c)|json!({"id":c.id,"statement_sha256":c.statement_sha256,"state":c.state,"strength":if c.assessment.is_some(){"measured"}else if families>=2{"observed"}else{"proposed"},"independent_families":families,"applicable_tasks":applicable,"observed_work":work,"conflicts":layer.candidates.iter().filter(|other|other.statement_sha256!=c.statement_sha256 && other.subject==c.subject && other.scope==c.scope && other.action!=c.action).map(|c|&c.id).collect::<Vec<_>>()})).collect::<Vec<_>>())
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
    let scope = task_scope(facts);
    layers
        .iter()
        .flat_map(|l| {
            l.candidates
                .iter()
                .filter(|c| c.state == State::Promoted && applies(&c.scope, &scope, &l.isa))
        })
        .collect()
}
