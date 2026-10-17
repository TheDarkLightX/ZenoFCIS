//! Explicit bounded knowledge acquisition and advisory planning. LLM-generated.
//! No private default path, report export, program execution or owner answer.
use super::pipeline::SearchLimits;
use super::*;
use std::time::Instant;
use zeno_fcis_loop_doors::strategy_knowledge as K;

fn names(value: &str) -> Result<Vec<String>, (u8, Value)> {
    if value.len() > 4096 {
        return Err(refused("kb-type-list-bound"));
    }
    let names: Vec<_> = value.split(',').map(str::trim).map(str::to_owned).collect();
    if names.len() > 18 || names.iter().any(String::is_empty) {
        return Err(refused("kb-type-list"));
    }
    Ok(names)
}
fn constant(v: &K::Value) -> Value {
    match v {
        K::Value::Text(s) => json!(s),
        K::Value::Integer(n) => json!(n),
    }
}
fn implementation() -> String {
    sha256_hex(
        concat!(
            include_str!("loop_strategy_knowledge.rs"),
            include_str!("../../zeno-fcis-loop-doors/src/strategy_knowledge.rs"),
            include_str!("../../zeno-fcis-loop-doors/src/strategy_knowledge/reasoning.rs"),
            include_str!("knowledge_command.rs"),
            include_str!("knowledge_evidence.rs"),
            include_str!("loop_knowledge.rs"),
            include_str!("../../zeno-fcis-loop-doors/src/strategy_algebra.rs"),
            include_str!("../../zeno-fcis-loop-doors/examples/strategy-example.kb"),
        )
        .as_bytes(),
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn command(
    path: Option<&Path>,
    operation: &str,
    query: Option<&str>,
    have: Option<&str>,
    want: Option<&str>,
    max_steps: usize,
    max_states: usize,
    search: SearchLimits,
) -> (u8, Value) {
    command_layered(
        &path.map(Path::to_path_buf).into_iter().collect::<Vec<_>>(),
        operation,
        query,
        have,
        want,
        "",
        max_steps,
        max_states,
        search,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn command_layered(
    paths: &[PathBuf],
    operation: &str,
    query: Option<&str>,
    have: Option<&str>,
    want: Option<&str>,
    task: &str,
    max_steps: usize,
    max_states: usize,
    search: SearchLimits,
) -> (u8, Value) {
    if !search.valid() || max_steps > 16 || max_states > 4096 {
        return refused("kb-limits-out-of-bounds");
    }
    let request = match operation {
        "advise" if query.is_none() && have.is_none() && want.is_none() => {
            K::Request::Advise(task.to_owned())
        }
        "explain" if have.is_none() && want.is_none() => match query {
            Some(q) => K::Request::Explain {
                query: q.to_owned(),
                task: task.to_owned(),
            },
            None => return refused("kb-query-required"),
        },
        "query" if have.is_none() && want.is_none() => match query {
            Some(q) => K::Request::Query(q.to_owned()),
            None => return refused("kb-query-required"),
        },
        "check" if query.is_none() && have.is_none() && want.is_none() => K::Request::Check,
        "plan" if query.is_none() => {
            let (Some(h), Some(w)) = (have, want) else {
                return refused("kb-have-want-required");
            };
            let have = match names(h) {
                Ok(n) => n,
                Err(r) => return r,
            };
            let want = match names(w) {
                Ok(n) => n,
                Err(r) => return r,
            };
            if task.is_empty() {
                K::Request::Plan { have, want }
            } else {
                K::Request::PlanFor {
                    have,
                    want,
                    task: task.to_owned(),
                }
            }
        }
        _ => return refused("kb-operation-arguments"),
    };
    let started = Instant::now();
    let configured = match crate::knowledge_command::paths(paths) {
        Ok(p) => p,
        Err(e) => return (if e["status"] == "inconclusive" { 2 } else { 1 }, e),
    };
    let limits = K::Limits {
        work: search.max_search_work.min(2_000_000),
        rows: usize::try_from(search.max_cells.min(8192)).unwrap_or(8192),
        bindings: 4096,
        states: max_states,
        steps: max_steps,
    };
    let loaded = match crate::knowledge_command::load_with_limits(&configured, limits) {
        Ok(l) => l,
        Err(e) => {
            let inconclusive = e["status"] == "inconclusive";
            let io = e["reason"] == "kb-read";
            return doors_result(
                if inconclusive {
                    2
                } else if io {
                    3
                } else {
                    1
                },
                "strategy",
                if inconclusive {
                    "inconclusive"
                } else {
                    "refused"
                },
                json!({"schema":K::SCHEMA,"operation":operation,"reason":e["reason"],"work":e["work"],"attempts_spent":0,"authority":"none"}),
            );
        }
    };
    let source = loaded
        .sources
        .last()
        .map(String::as_str)
        .unwrap_or(K::EXAMPLE);
    let binding = json!({"knowledge_sha256":sha256_hex(source.as_bytes()),
        "implementation_sha256":implementation(),"source":if !configured.is_empty() {"external-user-file"} else {"compiled-public-example"}});
    if started.elapsed().as_millis() >= u128::from(search.deadline_ms) {
        return doors_result(
            2,
            "strategy",
            "inconclusive",
            json!({"schema":K::SCHEMA,"operation":operation,"reason":"kb-deadline",
                "binding":binding,"work":0,"attempts_spent":0,"authority":"none"}),
        );
    }
    let result = crate::knowledge_command::run(&loaded, &request, limits);
    let spent = result.as_ref().map_or_else(|s| s.work, |r| r.work);
    if started.elapsed().as_millis() >= u128::from(search.deadline_ms) {
        return doors_result(
            2,
            "strategy",
            "inconclusive",
            json!({"schema":K::SCHEMA,"operation":operation,"reason":"kb-deadline",
                "binding":binding,"work":spent,"attempts_spent":0,"authority":"none"}),
        );
    }
    let r = match result {
        Ok(r) => r,
        Err(s) => {
            return doors_result(
                if s.inconclusive { 2 } else { 1 },
                "strategy",
                if s.inconclusive {
                    "inconclusive"
                } else {
                    "refused"
                },
                json!({"schema":K::SCHEMA,"operation":operation,"reason":s.code,"work":s.work,
                "binding":binding,"attempts_spent":0,"authority":"none"}),
            );
        }
    };
    if matches!(r.outcome, K::Outcome::Advised(_) | K::Outcome::NoPlan(_)) {
        return crate::knowledge_command::report(&loaded, Ok(r));
    }
    let explanations =
        crate::knowledge_command::report(&loaded, Ok(r.clone())).1["explanations"].clone();
    let mut detail = json!({"schema":K::SCHEMA,"operation":operation,"binding":binding,
        "work":r.work,"derived_rows":r.rows,"states":r.states,"attempts_spent":0,"authority":"none",
        "claim":"bounded structural inference only; no program, strategy-fit or owner authority"});
    detail["explanations"] = explanations;
    let (code, status) = match r.outcome {
        K::Outcome::Advised(_) | K::Outcome::NoPlan(_) => return refused("kb-result"),
        K::Outcome::Query(rows) => {
            detail["rows"] = json!(
                rows.iter()
                    .map(|b| b
                        .iter()
                        .map(|(k, v)| (k.clone(), constant(v)))
                        .collect::<serde_json::Map<_, _>>())
                    .collect::<Vec<_>>()
            );
            (0, "proposed-query")
        }
        K::Outcome::Checked(problems) => {
            detail["problems"] = json!(problems);
            if problems.is_empty() {
                (0, "consistent")
            } else {
                (1, "inconsistent")
            }
        }
        K::Outcome::Planned(p) => {
            detail["steps"] = json!(p.steps.iter().map(|s| s.name()).collect::<Vec<_>>());
            detail["eligible_strategies"] = json!(p.eligible);
            detail["selected_strategy"] = json!(p.selected);
            detail["composition"] = super::strategy_profiles::plan_json(&p.composition);
            detail["strategy_plan"] = p
                .strategy_plan
                .as_ref()
                .map(super::strategy_profiles::plan_json)
                .unwrap_or(Value::Null);
            detail["type_check"] = json!("passed");
            (0, "proposed-plan")
        }
    };
    doors_result(code, "strategy", status, detail)
}
