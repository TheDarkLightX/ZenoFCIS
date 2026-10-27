//! Concrete rule discovery and target qualification. LLM-generated.
//! A bank never supplies authority. Every rule is checked/replayed on the
//! target input domains, and every proposed whole program against the original.

use super::pipeline::{self, Budget, SearchLimits};
use super::*;
use zeno_fcis_program_search::concrete::{self, Rule};

const SCHEMA: &str = "zeno-fcis/loop-rule-bank/1";

/// Derived reporting only: retain the first counterexample/refusal per rule.
/// A term rule can qualify and later have a refuted whole-program application.
fn rules_summary(considered: usize, qualified: usize, checks: &[Value]) -> Value {
    use std::collections::{BTreeMap, BTreeSet};
    let mut examined = BTreeSet::new();
    let mut refutations = BTreeMap::new();
    let mut refusals = BTreeMap::new();
    let mut incomplete = BTreeSet::new();
    for row in checks {
        let Some(index) = row["rule"].as_u64() else {
            continue;
        };
        if row["stage"] == "target-rule" {
            examined.insert(index);
        }
        let check = row.get("check").unwrap_or(row);
        match check["verdict"].as_str() {
            Some("counterexample") => {
                refutations.entry(index).or_insert_with(|| {
                    json!({
                        "rule":index,"stage":row["stage"],"verdict":"counterexample",
                        "ordinal":check["ordinal"],"input":check["input"],
                        "original":check["original"],"candidate":check["candidate"]
                    })
                });
            }
            Some("refused") => {
                refusals.entry(index).or_insert_with(|| {
                    json!({
                        "rule":index,"stage":row["stage"],"code":check["reason"]
                    })
                });
            }
            Some("inconclusive") => {
                incomplete.insert(index);
            }
            Some("equivalent") if !accepted(check) => {
                incomplete.insert(index);
            }
            _ => {}
        }
    }
    json!({"considered":considered,"qualified":qualified,
        "refuted":refutations.len(),"refused":refusals.len(),
        "inconclusive":incomplete.len(),"unexamined":considered.saturating_sub(examined.len()),
        "refutations":refutations.into_values().collect::<Vec<_>>(),
        "refusals":refusals.into_values().collect::<Vec<_>>()})
}

fn program(value: &Value) -> Result<Program, String> {
    crate::neural_loop::program_json::program_from_json(value)
        .map_err(|_| "rule-program-not-admitted".into())
}

fn measured_cost(program: &Program) -> Option<(usize, usize)> {
    concrete::cost(program).ok().map(|cost| (cost.0, cost.1))
}

fn expression_term(
    value: &Value,
    vocabulary_value: &Value,
    parameters: &Value,
    original: &Program,
) -> Result<Program, String> {
    use zeno_fcis_synthesis::finite::{Domain, Op};
    let object = value.as_object().ok_or("term-shape")?;
    only_keys(object, &["kind", "expression"], "term")?;
    let (output, constant) = match value["kind"].as_str() {
        Some("Bool") => (Domain::Bool, Op::Bool(false)),
        Some("Int") => (
            Domain::Int {
                min: i128::MIN,
                max: i128::MAX,
            },
            Op::Int(0),
        ),
        _ => return Err("term-kind".into()),
    };
    let base = Program::try_new(
        original.inputs().to_vec(),
        vec![output],
        vec![constant],
        vec![0],
    )
    .map_err(|_| "term-base-admission")?;
    let vocab = vocabulary(&vocabulary_spec(vocabulary_value)?, &base)?;
    let text = value["expression"].as_str().ok_or("term-expression")?;
    let node = parse(
        text,
        0,
        Allow {
            holes: false,
            params: true,
            keep: false,
        },
    )
    .map_err(|r| r.0)?;
    let parameters = if parameters.is_null() {
        BTreeMap::new()
    } else {
        integer_map(parameters, "parameter")?
    };
    let node = zeno_fcis_loop_doors::expr::bind_params(&node, &parameters).map_err(|r| r.0)?;
    lower_outputs(&[node], &vocab, &base).map_err(|r| r.0)
}

fn stop_reason(stop: zeno_fcis_program_search::Stop) -> (u8, Value) {
    use zeno_fcis_program_search::Stop;
    match stop {
        Stop::NotAdmitted { code } | Stop::Inconsistent { code } => refused(code),
        _ => doors_result(
            2,
            "rules",
            "inconclusive",
            json!({"reason": format!("{stop:?}"), "attempts_spent": 0}),
        ),
    }
}

fn rule_json(rule: &Rule) -> Value {
    let lhs = program_json(&rule.lhs);
    let rhs = program_json(&rule.rhs);
    let id = sha256_hex(
        crate::transform::canonical_json(&json!([lhs["sha256"], rhs["sha256"]])).as_bytes(),
    );
    json!({"id": id, "lhs": lhs["program"], "rhs": rhs["program"],
        "before": {"nodes": lhs["nodes"], "bytes": lhs["bytes"]},
        "after": {"nodes": rhs["nodes"], "bytes": rhs["bytes"]}})
}

fn read_bank(value: &Value, limit: usize) -> Result<Vec<Rule>, String> {
    let object = value.as_object().ok_or("rule-bank-shape")?;
    only_keys(
        object,
        &[
            "schema",
            "authority",
            "scope",
            "rules",
            "terms",
            "classes",
            "rejected",
            "work_used",
            "claim",
        ],
        "rule-bank",
    )?;
    if value["schema"] != SCHEMA {
        return Err("rule-bank-schema".into());
    }
    let rules = value["rules"].as_array().ok_or("rule-bank-rules")?;
    if rules.len() > limit {
        return Err("rule-bank-over-budget".into());
    }
    rules
        .iter()
        .map(|entry| {
            let object = entry.as_object().ok_or("rule-shape")?;
            only_keys(object, &["id", "lhs", "rhs", "before", "after"], "rule")?;
            Ok(Rule {
                lhs: program(&entry["lhs"])?,
                rhs: program(&entry["rhs"])?,
            })
        })
        .collect()
}

pub(super) fn command(
    original: &Path,
    terms: Option<&Path>,
    bank: Option<&Path>,
    out: Option<&Path>,
    limits: SearchLimits,
) -> (u8, Value) {
    if !limits.valid() {
        return refused("search-limits-out-of-bounds");
    }
    if out.is_some_and(Path::exists) {
        return refused("output-must-be-new-directory");
    }
    let bytes = match pipeline::read_program(original) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let original = match import_program(&bytes) {
        Ok(p) => p,
        Err(_) => return refused("original-not-admitted"),
    };
    if tuples(original.inputs()) > u128::from(limits.max_tuples) {
        return doors_result(
            2,
            "rules",
            "inconclusive",
            json!({"reason":"domain-budget", "attempts_spent":0}),
        );
    }
    let mut budget = Budget::new(limits);
    let (rules, bank_json) = if let Some(bank) = bank {
        let value = match pipeline::read_json(bank) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let rules = match read_bank(&value, limits.max_terms) {
            Ok(r) => r,
            Err(r) => {
                if r == "rule-bank-over-budget" {
                    return doors_result(
                        2,
                        "rules",
                        "inconclusive",
                        json!({"reason": r, "attempts_spent":0}),
                    );
                }
                return refused(&r);
            }
        };
        (rules, value)
    } else {
        let mut proposed = Vec::new();
        if let Some(terms) = terms {
            let value = match pipeline::read_json(terms) {
                Ok(v) => v,
                Err(r) => return r,
            };
            let Some(object) = value.as_object() else {
                return refused("term-corpus-shape");
            };
            if let Err(r) = only_keys(
                object,
                &["schema", "vocabulary", "parameters", "terms"],
                "term-corpus",
            ) {
                return refused(&r);
            }
            if value["schema"] != "zeno-fcis/loop-terms/1" {
                return refused("term-corpus-schema");
            }
            let Some(list) = value["terms"].as_array() else {
                return refused("term-corpus-terms");
            };
            if list.len() > limits.max_terms {
                return doors_result(
                    2,
                    "rules",
                    "inconclusive",
                    json!({"reason":"term-budget", "attempts_spent":0}),
                );
            }
            for term in list {
                let proposed_term = if term.get("expression").is_some() {
                    expression_term(term, &value["vocabulary"], &value["parameters"], &original)
                } else {
                    program(term)
                };
                match proposed_term {
                    Ok(p) => proposed.push(p),
                    Err(r) => return refused(&r),
                }
            }
        }
        let settings = concrete::Budget {
            max_tuples: limits.max_tuples,
            max_terms: limits.max_terms,
            max_cells: limits.max_cells,
            max_work: limits.max_search_work,
        };
        let discovered = match budget.search(|| concrete::discover(&original, &proposed, settings))
        {
            Ok(Ok(d)) => d,
            Ok(Err(r)) => {
                let (exit, mut report) = stop_reason(r.reason);
                report["detail"]["proposer_work_charged"] = json!(r.work_used);
                report["detail"]["work"] = budget.work();
                return (exit, report);
            }
            Err(r) => {
                return doors_result(
                    2,
                    "rules",
                    "inconclusive",
                    json!({"reason":r,"work":budget.work(),"attempts_spent":0}),
                );
            }
        };
        let value = json!({"schema": SCHEMA, "authority": "none",
            "scope": {"original_sha256": sha256_hex(&bytes), "implementation_sha256": pipeline::implementation(),
                "inputs": crate::transform::domains_json(original.inputs()), "domain_tuples": tuples(original.inputs()),
                "semantics": "eager-i128-full-domain", "decided_by": "proposer"},
            "terms": discovered.terms.iter().map(program_to_json).collect::<Vec<_>>(),
            "classes": discovered.classes, "rejected": discovered.rejected,
            "rules": discovered.rules.iter().map(rule_json).collect::<Vec<_>>(), "work_used": discovered.work_used,
            "claim": "concrete-corpus-proposal-only"});
        (discovered.rules, value)
    };
    let mut checks = Vec::new();
    let mut qualified = Vec::new();
    let mut status = "qualified";
    let mut reason = Value::Null;
    let baseline = budget.gate(&bytes, &original);
    if !accepted(&baseline) {
        status = "inconclusive";
        reason = json!("baseline-check-or-replay");
    }
    let mut best_check = baseline.clone();
    let mut current = original.clone();
    if status == "qualified" {
        for (index, rule) in rules.iter().enumerate() {
            let rebound = concrete::rebind(&rule.lhs, original.inputs()).and_then(|lhs| {
                let rhs = concrete::rebind(&rule.rhs, original.inputs())?;
                concrete::validate_term(&lhs, original.inputs())?;
                concrete::validate_term(&rhs, original.inputs())?;
                Ok(Rule { lhs, rhs })
            });
            let rule = match rebound {
                Ok(r) => r,
                Err(r) => {
                    checks.push(json!({"rule":index,"stage":"target-rule","verdict":"refused","reason":format!("{r:?}")}));
                    continue;
                }
            };
            if !pipeline::decreasing_cost(measured_cost(&rule.rhs), measured_cost(&rule.lhs)) {
                checks.push(json!({"rule":index,"stage":"target-rule","verdict":"refused","reason":"nondecreasing-rule"}));
                continue;
            }
            let lhs = match bytes_of(&rule.lhs) {
                Some(b) => b,
                None => return refused("rule-encoding"),
            };
            let check = budget.gate(&lhs, &rule.rhs);
            checks.push(json!({"rule": index, "stage":"target-rule", "check":check,
                "lhs":program_json(&rule.lhs), "rhs":program_json(&rule.rhs)}));
            if check["verdict"] == "inconclusive"
                || (check["verdict"] == "equivalent" && !accepted(&check))
            {
                status = "inconclusive";
                reason = json!("target-rule-check-or-replay-budget");
                break;
            }
            if accepted(&check) {
                qualified.push((index, rule));
            }
        }
    }
    let mut applied = Vec::new();
    if status == "qualified" {
        loop {
            let mut changed = false;
            for (index, rule) in &qualified {
                let candidate = match budget.search(|| concrete::replace(&current, rule)) {
                    Ok(Ok(p)) => p,
                    Ok(Err(r)) => {
                        checks.push(json!({"rule":index,"stage":"whole-program","verdict":"refused","reason":format!("{r:?}")}));
                        continue;
                    }
                    Err(r) => {
                        status = "inconclusive";
                        reason = json!(r);
                        break;
                    }
                };
                if !pipeline::decreasing_cost(measured_cost(&candidate), measured_cost(&current)) {
                    continue;
                }
                let check = budget.gate(&bytes, &candidate);
                checks.push(json!({"rule":index,"stage":"whole-program","check":check,"candidate":program_json(&candidate)}));
                if check["verdict"] == "inconclusive"
                    || (check["verdict"] == "equivalent" && !accepted(&check))
                {
                    status = "inconclusive";
                    reason = json!("whole-program-check-or-replay-budget");
                    break;
                }
                if accepted(&check) {
                    applied.push(json!({"rule":index,"before":program_json(&current),"after":program_json(&candidate),"check":check}));
                    current = candidate;
                    best_check = check;
                    changed = true;
                    break;
                }
            }
            if !changed || status == "inconclusive" {
                break;
            }
        }
    }
    if budget.stages.started.elapsed() >= budget.stages.deadline {
        status = "inconclusive";
        reason = json!("invocation-deadline");
    }
    let improved = pipeline::decreasing_cost(measured_cost(&current), measured_cost(&original));
    let summary = rules_summary(rules.len(), qualified.len(), &checks);
    let report = json!({"schema":"zeno-fcis/loop-rules-result/1","status":status,"authority":"none",
        "original": program_json(&original),
        "binding":{"original_sha256":sha256_hex(&bytes),"implementation_sha256":pipeline::implementation(),
            "scope":"full-declared-cartesian-domain"},"bank":bank_json,"baseline_check":baseline,
        "checks":checks,"rules_requalified":qualified.len(),"applied":applied,"rules_summary":summary,
        "best":program_json(&current),"best_check":best_check,"work":budget.work(),"limits":limits.data(),
        "reason":reason,"attempts_spent":0,"improved":improved,
        "result_kind":if improved { "improved-program" } else { "unchanged-original" },
        "claim":if improved { "target-domain-concrete-rules-and-whole-program-equivalence-only" }
            else { "original-equivalence-only; no improving rewrite checked" }});
    for row in summary["refutations"].as_array().into_iter().flatten() {
        eprintln!(
            "rules: rule {} refuted at {} input={}",
            row["rule"],
            row["stage"].as_str().unwrap_or("unknown"),
            row["input"]
        );
    }
    let (exit, mut response) =
        pipeline::finish("rules", report, status, out, Some(&bytes), Some(&current));
    response["rules_summary"] = summary;
    (exit, response)
}
