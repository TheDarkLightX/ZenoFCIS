//! Descriptive deterministic profiles. No metric or point can create an assessment.
use crate::knowledge_assessment::*;
use crate::knowledge_growth::{CertifiedFact, FactStatus};
use crate::knowledge_store::Layer;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn fraction(n: u128, d: u128) -> Value {
    if d == 0 {
        Value::Null
    } else {
        json!({"numerator":n,"denominator":d})
    }
}
fn work(r: &Run) -> u64 {
    r.work.values().copied().sum()
}
fn par2(r: &Run) -> u64 {
    if r.verdict.solved() {
        work(r)
    } else {
        r.budget * 2
    }
}
fn percentile(v: &mut [u64], n: usize, d: usize) -> Option<u64> {
    if v.is_empty() {
        None
    } else {
        v.sort_unstable();
        Some(v[(v.len() * n).div_ceil(d).saturating_sub(1)])
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn add_ratio(a: (u128, u128), b: (u128, u128)) -> Result<(u128, u128), &'static str> {
    let g = gcd(a.1, b.1);
    let d = a.1.checked_mul(b.1 / g).ok_or("kb-metric-overflow")?;
    let n =
        a.0.checked_mul(b.1 / g)
            .and_then(|x| b.0.checked_mul(a.1 / g).and_then(|y| x.checked_add(y)))
            .ok_or("kb-metric-overflow")?;
    let g = gcd(n, d);
    Ok((n / g, d / g))
}
/// Coverage-game Shapley: every solved task contributes exactly 1/k to its k solvers.
fn shapley(
    matrix: &BTreeMap<String, BTreeMap<String, &CertifiedFact>>,
    door: &str,
) -> Result<(u128, u128), &'static str> {
    let mut total = (0, 1);
    for runs in matrix.values() {
        let solvers = runs.values().filter(|f| f.run.verdict.solved()).count();
        if runs.get(door).is_some_and(|f| f.run.verdict.solved()) {
            total = add_ratio(total, (1, solvers as u128))?;
        }
    }
    Ok(total)
}
pub(crate) fn profiles(
    layer: &Layer,
    checker: &str,
    engine: &str,
    population: &str,
) -> Result<Value, &'static str> {
    let mut classes = BTreeMap::<String, Vec<&CertifiedFact>>::new();
    for f in layer.facts.values().filter(|f| {
        f.checker_sha256 == checker && f.engine_sha256 == engine && f.source == population
    }) {
        let class = f
            .premises
            .get("family")
            .and_then(|v| v.first())
            .cloned()
            .unwrap_or_else(|| "unknown".into());
        classes.entry(class).or_default().push(f);
    }
    let mut leaderboards = Vec::new();
    let mut breadth = BTreeMap::<String, usize>::new();
    for (class, facts) in classes {
        let mut matrix = BTreeMap::<String, BTreeMap<String, &CertifiedFact>>::new();
        for f in facts {
            let row = matrix.entry(f.run.task.clone()).or_default();
            if let Some(old) = row.insert(f.door.clone(), f) {
                if old.run != f.run {
                    return Err("kb-profile-conflicting-case");
                }
            }
        }
        let doors = matrix
            .values()
            .flat_map(|r| r.keys().cloned())
            .collect::<BTreeSet<_>>();
        if doors.len() > 32 || matrix.len() > 64 {
            return Err("kb-profile-budget");
        }
        // Comparability requires a complete matrix; refusal is out of scope, never an unsolved run.
        let common = matrix
            .values()
            .filter(|row| {
                row.len() == doors.len() && row.values().all(|f| f.run.verdict != Verdict::Refused)
            })
            .collect::<Vec<_>>();
        let vbs = common
            .iter()
            .map(|row| row.values().map(|f| par2(&f.run)).min().unwrap_or(0))
            .sum::<u64>();
        let sbs = doors
            .iter()
            .map(|door| common.iter().map(|row| par2(&row[door].run)).sum::<u64>())
            .min()
            .unwrap_or(0);
        let mut profiles = Vec::new();
        for door in &doors {
            let runs = matrix
                .values()
                .filter_map(|row| row.get(door).copied())
                .collect::<Vec<_>>();
            let eligible = runs
                .iter()
                .copied()
                .filter(|f| f.run.verdict != Verdict::Refused)
                .collect::<Vec<_>>();
            let solved = eligible
                .iter()
                .copied()
                .filter(|f| f.run.verdict.solved())
                .collect::<Vec<_>>();
            let decisive = eligible.iter().filter(|f| f.run.verdict.decisive()).count();
            let counterexamples = eligible
                .iter()
                .filter(|f| f.run.verdict == Verdict::Counterexample)
                .count();
            let mut first_counterexample = eligible
                .iter()
                .filter(|f| f.door == "artifact-replay" && f.run.verdict == Verdict::Counterexample)
                .map(|f| work(&f.run))
                .collect::<Vec<_>>();
            let first_counterexample_median = percentile(&mut first_counterexample, 1, 2);
            let unique = matrix
                .values()
                .filter(|row| {
                    row.get(door).is_some_and(|f| f.run.verdict.solved())
                        && row.values().filter(|f| f.run.verdict.solved()).count() == 1
                })
                .count();
            if unique > 0 {
                *breadth.entry(door.clone()).or_default() += 1;
            }
            let mut solve_work = solved.iter().map(|f| work(&f.run)).collect::<Vec<_>>();
            let median = percentile(&mut solve_work, 1, 2);
            let p90 = percentile(&mut solve_work, 9, 10);
            let par = eligible.iter().map(|f| par2(&f.run)).sum::<u64>();
            let total = eligible.iter().map(|f| work(&f.run)).sum::<u64>();
            let wasted = eligible
                .iter()
                .filter(|f| matches!(f.run.verdict, Verdict::Inconclusive | Verdict::NotFound))
                .map(|f| work(&f.run))
                .sum::<u64>();
            let max_budget = eligible.iter().map(|f| f.run.budget).max().unwrap_or(0);
            let budgets = BTreeSet::from([0, max_budget / 4, max_budget / 2, max_budget]);
            let curve=budgets.iter().map(|b|json!({"budget":b,"solved":solved.iter().filter(|f|work(&f.run)<=*b).count(),"applicable":eligible.len()})).collect::<Vec<_>>();
            let auc = solved
                .iter()
                .map(|f| u128::from(max_budget.saturating_sub(work(&f.run))))
                .sum::<u128>();
            let ert = fraction(u128::from(total), solved.len() as u128);
            let comparable = common.iter().map(|row| par2(&row[door].run)).sum::<u64>();
            let gap = if sbs == vbs {
                Value::Null
            } else {
                json!({"numerator":i128::from(sbs)-i128::from(comparable),"denominator":sbs-vbs})
            };
            let performance = [1u64, 2, 4, 10]
                .into_iter()
                .map(|tau| {
                    let count = matrix
                        .values()
                        .filter(|row| {
                            row.get(door).is_some_and(|f| {
                                f.run.verdict.solved()
                                    && row
                                        .values()
                                        .filter(|f| f.run.verdict.solved())
                                        .map(|f| work(&f.run))
                                        .min()
                                        .is_some_and(|best| {
                                            work(&f.run) <= best.saturating_mul(tau)
                                        })
                            })
                        })
                        .count();
                    json!({"tau":tau,"within_factor":count,"tasks":matrix.len()})
                })
                .collect::<Vec<_>>();
            let mut bounds = BTreeMap::<String, u64>::new();
            for f in &eligible {
                if let Some(bound) = &f.run.stopping_bound {
                    *bounds.entry(bound.clone()).or_default() += 1;
                }
            }
            let quality=solved.iter().map(|f|{
                let cost=f.run.result_cost.unwrap_or([0,0]);let best=matrix[&f.run.task].values().filter_map(|f|f.run.result_cost).map(|v|v[0]).min().unwrap_or(cost[0]);
                json!({"task":f.run.task,"nodes":cost[0],"best_known_nodes":best,"optimality_gap":fraction(u128::from(cost[0].saturating_sub(best)),u128::from(best))})
            }).collect::<Vec<_>>();
            let improved = solved
                .iter()
                .filter(|f| {
                    layer.outputs.get(&f.output_sha256).is_some_and(|v| {
                        f.run.result_cost.is_some_and(|c| {
                            v["detail"]["original"]["nodes"]
                                .as_u64()
                                .is_some_and(|n| c[0] < n)
                        })
                    })
                })
                .count();
            let replayed = solved
                .iter()
                .filter(|f| f.status == FactStatus::Proved)
                .count();
            let accounts = |field: fn(&crate::knowledge_growth::Episode) -> Option<u64>| {
                let values = eligible
                    .iter()
                    .filter_map(|f| layer.episodes.get(&f.episode).and_then(field))
                    .collect::<Vec<_>>();
                if values.len() != eligible.len() {
                    Value::Null
                } else {
                    fraction(
                        values.into_iter().map(u128::from).sum(),
                        solved.len() as u128,
                    )
                }
            };
            let shapley = shapley(&matrix, door)?;
            let effectiveness = if eligible.is_empty() {
                0
            } else {
                (solved.len() * 10000 / eligible.len()) as u64
            };
            let penalty = eligible.iter().map(|f| f.run.budget * 2).sum::<u64>();
            let efficiency = if penalty == 0 {
                0
            } else {
                penalty.saturating_sub(par).saturating_mul(10000) / penalty
            };
            let marginal = if matrix.is_empty() {
                0
            } else {
                (unique * 10000 / matrix.len()) as u64
            };
            let replay = if solved.is_empty() {
                0
            } else {
                (replayed * 10000 / solved.len()) as u64
            };
            let points =
                (effectiveness * 40 + efficiency * 25 + marginal * 20 + replay * 15) / 10000;
            let mut profile = json!({"door":door,"problem_class":class,"population":population,"tasks":matrix.len(),"recorded_runs":runs.len(),"applicable":eligible.len(),"applicability":fraction(eligible.len() as u128,matrix.len() as u128),"refusal_rate":fraction((runs.len()-eligible.len()) as u128,runs.len() as u128),"solved":solved.len(),"solve_rate":fraction(solved.len() as u128,eligible.len() as u128),"decisive_verdict_rate":fraction(decisive as u128,eligible.len() as u128),"counterexample_yield":counterexamples,"work_to_first_counterexample":{"median_nearest_rank":first_counterexample_median,"recorded":first_counterexample.len(),"missing":counterexamples.saturating_sub(first_counterexample.len())},"unique_solves":unique,"marginal_contribution":unique,"shapley_coverage":fraction(shapley.0,shapley.1)});
            for group in [
                json!({"work_to_solution":{"median_nearest_rank":median,"p90_nearest_rank":p90},"par2_work":par,"par2_denominator":eligible.len(),"anytime_profile":curve,"anytime_auc":fraction(auc,u128::from(max_budget)*eligible.len() as u128),"ert_work":ert,"wasted_work":wasted,"stopping_bounds":bounds}),
                json!({"comparable_par2_work":comparable,"closed_gap":gap,"performance_profile":performance,"quality":quality,"checked_improvement_rate":fraction(improved as u128,solved.len() as u128),"receipt_replay_rate":fraction(replayed as u128,solved.len() as u128)}),
                json!({"model_calls_per_solve":accounts(|e|e.model_calls),"tokens_per_solve":accounts(|e|e.tokens),"money_reserved_per_solve":accounts(|e|e.money_reserved),"owner_questions_per_solve":accounts(|e|e.owner_questions)}),
                json!({"points":{"value":points,"grade":if points>=85{"A"}else if points>=70{"B"}else if points>=50{"C"}else{"D"},"weights_percent":{"solve_rate":40,"par2_efficiency":25,"marginal_contribution":20,"receipt_replay":15},"components_bps":{"solve_rate":effectiveness,"par2_efficiency":efficiency,"marginal_contribution":marginal,"receipt_replay":replay},"promotion_authority":false}}),
            ] {
                if let (Some(target), Some(fields)) = (profile.as_object_mut(), group.as_object()) {
                    target.extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
            }
            profiles.push(profile);
        }
        profiles.sort_by(|a, b| {
            b["solved"]
                .as_u64()
                .cmp(&a["solved"].as_u64())
                .then(a["par2_work"].as_u64().cmp(&b["par2_work"].as_u64()))
                .then_with(|| {
                    let ratio = |v: &Value| {
                        (
                            v["closed_gap"]["numerator"].as_i64().unwrap_or(0) as i128,
                            v["closed_gap"]["denominator"].as_u64().unwrap_or(1) as i128,
                        )
                    };
                    let (an, ad) = ratio(a);
                    let (bn, bd) = ratio(b);
                    (bn * ad).cmp(&(an * bd))
                })
                .then(a["door"].as_str().cmp(&b["door"].as_str()))
        });
        leaderboards.push(json!({"class":class,"comparable_tasks":common.len(),"vbs_par2_work":vbs,"sbs_par2_work":sbs,"profiles":profiles}));
    }
    let impacts=layer.candidates.iter().filter_map(|c|c.assessment.as_ref().and_then(|k|layer.assessments.get(k)).map(|a|json!({"lesson":c.id,"state":c.state,"assessment_sha256":c.assessment,"assessed_delta":crate::knowledge_workflow::aggregate(a)}))).collect::<Vec<_>>();
    Ok(
        json!({"schema":"zeno-fcis/kb-loop-values/1","authority":"none","population":population,"work_unit":"sum-of-declared-loop-counters-no-cpu-claim","ranking":"solved-descending; PAR-2-work-ascending; closed-gap-descending; lexical","points_never_promote_veto_or_exclude":true,"leaderboards":leaderboards,"breadth":breadth,"knowledge_impact":impacts,"cross_version_regressions":version_regressions(layer)}),
    )
}
fn version_regressions(layer: &Layer) -> Value {
    let mut groups = BTreeMap::<(&str, &str), Vec<(usize, &CertifiedFact)>>::new();
    for (key, f) in &layer.facts {
        let order = layer.history.iter().position(|h| {
            h.evidence == *key
                || layer.assessments.get(&h.evidence).is_some_and(|a| {
                    a.tool_version == f.tool_version
                        && a.cases.iter().any(|c| c.off == f.run || c.on == f.run)
                })
        });
        if let Some(order) = order {
            groups
                .entry((&f.subject, &f.door))
                .or_default()
                .push((order, f));
        }
    }
    json!(groups.into_iter().flat_map(|((subject,door),facts)|{let mut rows=Vec::new();for (ai,a) in &facts{for (bi,b) in &facts{if ai<bi && a.tool_version!=b.tool_version && (a.run.verdict.solved()&&!b.run.verdict.solved()||b.run.work.iter().any(|(k,v)|a.run.work.get(k).is_some_and(|base|v>base))){rows.push(json!({"subject":subject,"door":door,"from":a.tool_version,"to":b.tool_version,"kind":"observational-version-pair"}));}}}rows}).collect::<Vec<_>>())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fact(task: &str, door: &str, verdict: Verdict, source: &str) -> CertifiedFact {
        let run = Run {
            task: task.repeat(64),
            seed: 0,
            budget: 100,
            inputs: task.repeat(64),
            verdict,
            work: BTreeMap::from([("check_requests".into(), 7)]),
            result_cost: if verdict.solved() { Some([1, 1]) } else { None },
            budget_stop: false,
            stopping_bound: None,
            receipt_sha256: "c".repeat(64),
            output_sha256: "d".repeat(64),
        };
        CertifiedFact {
            subject: run.inputs.clone(),
            checker_sha256: "checker".into(),
            engine_sha256: "engine".into(),
            tool_version: "v".into(),
            status: if verdict.solved() {
                FactStatus::Proved
            } else {
                FactStatus::Certified
            },
            property: "fixture".into(),
            premises: Scope::from([("family".into(), BTreeSet::from(["counter".into()]))]),
            door: door.into(),
            output_sha256: run.output_sha256.clone(),
            episode: "episode".into(),
            source: source.into(),
            run,
        }
    }
    #[test]
    fn refused_runs_are_out_of_scope() {
        let layer = Layer {
            facts: BTreeMap::from([
                ("a".into(), fact("a", "door", Verdict::Accepted, "held-out")),
                ("b".into(), fact("b", "door", Verdict::Refused, "held-out")),
            ]),
            ..Layer::default()
        };
        let out = profiles(&layer, "checker", "engine", "held-out").unwrap();
        let p = &out["leaderboards"][0]["profiles"][0];
        assert_eq!(p["applicable"], 1);
        assert_eq!(p["recorded_runs"], 2);
        assert_eq!(p["par2_work"], 7);
    }
    #[test]
    fn marginal_contribution_uses_held_out_cases() {
        let layer = Layer {
            facts: BTreeMap::from([
                (
                    "a".into(),
                    fact("a", "proposing-door", Verdict::Accepted, "proposing"),
                ),
                (
                    "b".into(),
                    fact("b", "held-out-door", Verdict::Accepted, "held-out"),
                ),
            ]),
            ..Layer::default()
        };
        let out = profiles(&layer, "checker", "engine", "held-out").unwrap();
        let rows = out["leaderboards"][0]["profiles"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["door"], "held-out-door");
        assert_eq!(rows[0]["marginal_contribution"], 1);
    }
    #[test]
    fn shapley_coverage_sums_to_portfolio_solves() {
        let a = fact("a", "a", Verdict::Accepted, "held-out");
        let b = fact("a", "b", Verdict::Accepted, "held-out");
        let c = fact("b", "a", Verdict::Accepted, "held-out");
        let d = fact("b", "b", Verdict::NotFound, "held-out");
        let matrix = BTreeMap::from([
            (
                "a".into(),
                BTreeMap::from([("a".into(), &a), ("b".into(), &b)]),
            ),
            (
                "b".into(),
                BTreeMap::from([("a".into(), &c), ("b".into(), &d)]),
            ),
        ]);
        assert_eq!(shapley(&matrix, "a"), Ok((3, 2)));
        assert_eq!(shapley(&matrix, "b"), Ok((1, 2)));
    }
    #[test]
    fn par2_uses_deterministic_work() {
        let run = Run {
            task: "a".repeat(64),
            seed: 0,
            budget: 100,
            inputs: "b".repeat(64),
            verdict: Verdict::Accepted,
            work: BTreeMap::from([("check_requests".into(), 7)]),
            result_cost: Some([1, 1]),
            budget_stop: false,
            stopping_bound: None,
            receipt_sha256: "c".repeat(64),
            output_sha256: "d".repeat(64),
        };
        assert_eq!(par2(&run), 7);
        let mut unsolved = run;
        unsolved.verdict = Verdict::Inconclusive;
        assert_eq!(par2(&unsolved), 200);
    }
    #[test]
    fn exact_fraction_addition() {
        assert_eq!(add_ratio((1, 2), (1, 3)), Ok((5, 6)));
        assert_eq!(fraction(1, 0), Value::Null);
    }
}
