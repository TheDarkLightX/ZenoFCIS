//! Bounded strategy discovery. LLM-generated; selection is advisory.
//! The fixed checker is outside the search grammar and receives immutable
//! program snapshots. Evaluation tasks cannot change the frozen winner.

use super::pipeline::{self, Budget, ProgramStage, SearchLimits, Task};
use super::*;
use std::cmp::Reverse;
use std::collections::BTreeSet;

const SCHEMA: &str = "zeno-fcis/loop-strategy/1";

pub(super) fn enumerate(depth: u8) -> Result<Vec<Vec<ProgramStage>>, String> {
    if !(1..=3).contains(&depth) {
        return Err("strategy-depth-out-of-bounds".into());
    }
    let mut policies = vec![Vec::new()];
    let mut frontier = vec![Vec::<ProgramStage>::new()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for prior in frontier {
            for operator in pipeline::OPERATORS {
                if prior.last().is_some_and(|s| *s == operator) {
                    continue;
                }
                let mut word = prior.clone();
                word.push(operator);
                policies.push(word.clone());
                next.push(word);
            }
        }
        frontier = next;
    }
    Ok(policies)
}

// Runtime restriction: the supervisor's frame limit is invocation-wide.
const RUNTIME_DEPTH: u8 = 1;

fn development_stopped(run: &pipeline::Run) -> bool {
    run.status == "inconclusive"
        && (run.report["reason"] != "bounded-search-no-qualified-candidate"
            || !run.report["candidates"]
                .as_array()
                .is_some_and(Vec::is_empty))
}

fn tasks(value: &Value, base: &Path) -> Result<Vec<Task>, (u8, Value)> {
    let items = value.as_array().ok_or_else(|| refused("strategy-tasks"))?;
    if items.len() != 1 {
        return Err(refused("strategy-requires-one-task-per-split"));
    }
    items
        .iter()
        .map(|entry| {
            let object = entry
                .as_object()
                .ok_or_else(|| refused("strategy-task-shape"))?;
            only_keys(object, &["original", "proposal"], "strategy-task")
                .map_err(|r| refused(&r))?;
            let file = entry["original"]
                .as_str()
                .ok_or_else(|| refused("strategy-original-path"))?;
            let path = relative(base, file).map_err(|r| refused(&r))?;
            let original = pipeline::read_program(&path)?;
            pipeline::task(original, entry["proposal"].clone()).map_err(|r| refused(&r))
        })
        .collect()
}

fn ids(tasks: &[Task]) -> BTreeSet<String> {
    tasks.iter().map(|t| sha256_hex(&t.original)).collect()
}

// The CLI passes immutable arguments directly; no retained workflow handle.
#[allow(clippy::too_many_arguments)]
pub(super) fn command(
    tasks: Option<&Path>,
    kind: Option<&str>,
    problem: Option<&Path>,
    original: Option<&Path>,
    // BEGIN structural knowledge strategy arguments.
    kb: Option<&Path>,
    kb_op: Option<&str>,
    query: Option<&str>,
    have: Option<&str>,
    want: Option<&str>,
    kb_max_steps: usize,
    kb_max_states: usize,
    // END structural knowledge strategy arguments.
    depth: u8,
    out: Option<&Path>,
    limits: SearchLimits,
    knowledge: Option<&crate::loop_knowledge::LoopPolicy>,
) -> (u8, Value) {
    // BEGIN structural knowledge strategy routing.
    if let Some(op) = kb_op {
        if tasks.is_some()
            || kind.is_some()
            || problem.is_some()
            || original.is_some()
            || out.is_some()
            || depth != 1
        {
            return refused("kb-select-one-mode");
        }
        return super::strategy_knowledge::command(
            kb,
            op,
            query,
            have,
            want,
            kb_max_steps,
            kb_max_states,
            limits,
        );
    }
    if kb.is_some()
        || query.is_some()
        || have.is_some()
        || want.is_some()
        || kb_max_steps != 8
        || kb_max_states != 4096
    {
        return refused("kb-options-require-operation");
    }
    // END structural knowledge strategy routing.
    if let (Some(kind), Some(problem)) = (kind, problem) {
        if tasks.is_some() {
            return refused("strategy-select-one-mode");
        }
        if depth != 1 {
            return refused("strategy-mode-depth-uses-problem-config");
        }
        return super::strategy_profiles::command(kind, problem, original, out, limits);
    }
    if kind.is_some() || problem.is_some() || original.is_some() {
        return refused("strategy-mode-arguments");
    }
    let Some(path) = tasks else {
        return refused("strategy-tasks-or-problem-required");
    };
    select(path, depth, out, limits, knowledge)
}

fn select(
    path: &Path,
    depth: u8,
    out: Option<&Path>,
    limits: SearchLimits,
    knowledge: Option<&crate::loop_knowledge::LoopPolicy>,
) -> (u8, Value) {
    if !limits.valid() {
        return refused("search-limits-out-of-bounds");
    }
    if out.is_some_and(Path::exists) {
        return refused("output-must-be-new-directory");
    }
    if depth != RUNTIME_DEPTH {
        return refused("strategy-runtime-depth-must-be-one");
    }
    let policies = match enumerate(depth) {
        Ok(p) => p,
        Err(r) => return refused(&r),
    };
    let input = match pipeline::read_json(path) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(object) = input.as_object() else {
        return refused("strategy-shape");
    };
    if let Err(r) = only_keys(object, &["schema", "development", "evaluation"], "strategy") {
        return refused(&r);
    }
    if input["schema"] != SCHEMA {
        return refused("strategy-schema");
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let development = match tasks(&input["development"], base) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let evaluation = match tasks(&input["evaluation"], base) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let dev_ids = ids(&development);
    let eval_ids = ids(&evaluation);
    if !dev_ids.is_disjoint(&eval_ids)
        || dev_ids.len() != development.len()
        || eval_ids.len() != evaluation.len()
    {
        return refused("strategy-subject-overlap-or-duplicate");
    }
    let mut budget = Budget::new(limits);
    budget.knowledge = knowledge.cloned();
    let mut trials = Vec::new();
    let mut scored = Vec::new();
    for (index, policy) in policies.iter().enumerate() {
        let before = budget.gate_pairs;
        let mut qualified = 0u64;
        let mut nodes = 0u64;
        let mut runs = Vec::new();
        for task in &development {
            let run = pipeline::run(task, policy, &mut budget);
            if development_stopped(&run) {
                runs.push(run.report);
                trials.push(json!({"stages": policy, "runs": runs, "status": "inconclusive"}));
                return pipeline::finish(
                    "strategy",
                    json!({"schema": SCHEMA, "status": "inconclusive",
                    "authority": "none", "trials": trials, "selection": null, "evaluation": [],
                    "policies_generated": policies.len(), "reason": "development-budget-stop",
                    "work": budget.work(), "attempts_spent": 0}),
                    "inconclusive",
                    out,
                    None,
                    None,
                );
            }
            if run.status == "qualified" {
                qualified += 1;
                nodes =
                    nodes.saturating_add(run.best.as_ref().map_or(0, |p| p.nodes().len()) as u64);
            } else {
                nodes = nodes.saturating_add(2 * task.program.nodes().len() as u64);
            }
            runs.push(run.report);
        }
        let pairs = budget.gate_pairs.saturating_sub(before);
        let score = json!({"qualified": qualified, "nodes_with_failure_penalty": nodes,
            "gate_pairs_reserved": pairs, "stage_count": policy.len(), "grammar_index": index});
        scored.push((
            (Reverse(qualified), nodes, pairs, policy.len(), index),
            index,
        ));
        trials.push(json!({"stages": policy, "score": score, "runs": runs}));
    }
    let Some((_, winner)) = scored.into_iter().min_by_key(|(key, _)| *key) else {
        return refused("empty-strategy-grammar");
    };
    // This owned selection is constructed and hashed before evaluation starts.
    // Neither an evaluation result nor a search callback can mutate it.
    let selection = json!({"schema": "zeno-fcis/loop-strategy-selection/1",
        "stages": policies[winner], "score": trials[winner]["score"], "development_subjects": dev_ids,
        "implementation_sha256": pipeline::implementation(), "limits": limits.data(),
        "grammar": pipeline::OPERATORS, "depth": depth,
        "task_manifest_sha256": sha256_hex(crate::transform::canonical_json(&input["development"]).as_bytes())});
    let commitment = sha256_hex(crate::transform::canonical_json(&selection).as_bytes());
    let mut evaluated = Vec::new();
    let mut status = "evaluated";
    for task in &evaluation {
        let run = pipeline::run(task, &policies[winner], &mut budget);
        if run.status == "inconclusive" {
            status = "inconclusive";
        }
        evaluated.push(run.report);
        if status == "inconclusive" {
            break;
        }
    }
    pipeline::finish(
        "strategy",
        json!({"schema": SCHEMA, "authority": "none", "status": status,
        "policies_generated": policies.len(), "trials": trials, "selection": selection,
        "selection_sha256": commitment, "selection_frozen_before_evaluation": true,
        "evaluation_influenced_selection": false, "evaluation_subjects": eval_ids,
        "evaluation": evaluated, "work": budget.work(), "attempts_spent": 0,
        "claim": "bounded-development-selection-only; no optimality or generalization"}),
        status,
        out,
        None,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strategy_development_stop_filter_requires_a_gate_free_empty_search() {
        let mut run = pipeline::Run {
            status: "inconclusive",
            best: None,
            report: json!({"reason":"bounded-search-no-qualified-candidate", "candidates":[]}),
        };
        assert!(!development_stopped(&run));
        run.report["candidates"] =
            json!([{"check":{"verdict":"inconclusive","reason":"gate-budget"}}]);
        assert!(development_stopped(&run));
        run.report["candidates"] = json!([]);
        run.report["reason"] = json!("stage-budget");
        assert!(development_stopped(&run));
    }

    #[test]
    fn strategy_fixed_grammar_counts_and_order() {
        for (depth, count) in [(1, 5), (2, 17), (3, 53)] {
            let words = enumerate(depth).unwrap_or_else(|r| panic!("{r}"));
            assert_eq!(words.len(), count);
            assert!(words.iter().all(|w| w.windows(2).all(|p| p[0] != p[1])));
            assert_eq!(words[0], Vec::<ProgramStage>::new());
        }
        assert!(enumerate(4).is_err());
    }
}
