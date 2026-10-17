//! Modes of the existing strategy surface. LLM-generated shell orchestration.
//! The plan/trace checks cover structure. Program qualification is exclusively
//! the existing transform checker plus replay. Other outputs are proposals.
use super::pipeline::{self, Budget, SearchLimits};
use super::*;
use zeno_fcis_loop_doors::strategy_algebra as A;
use zeno_fcis_loop_doors::strategy_questions as Q;
use zeno_fcis_loop_doors::strategy_split as S;

type StrategyRun = (Value, Vec<A::Step>, Option<(Vec<u8>, Program)>);

fn stopped_detail(detail: Value, deadline_expired: bool) -> Value {
    let status = if deadline_expired {
        "inconclusive"
    } else {
        detail["status"].as_str().unwrap_or("refused")
    };
    if !matches!(status, "inconclusive" | "refused") {
        return detail;
    }
    // A stopped invocation may retain accounting, never a partial proposal.
    // Explicitly enumerate retained fields so new proposal fields cannot leak.
    let reason = if deadline_expired {
        "invocation-deadline"
    } else {
        detail["reason"]
            .as_str()
            .or_else(|| detail["best_check"]["reason"].as_str())
            .unwrap_or("strategy-check-incomplete")
    };
    let mut stopped = json!({"status": status,
        "reason": reason,
        "proposer_work": detail["proposer_work"].clone()});
    if detail["search_bounds"].is_object() {
        stopped["search_bounds"] = detail["search_bounds"].clone();
    }
    stopped
}

pub(super) fn plan_json(plan: &A::Plan) -> Value {
    fn node(n: &A::Node) -> Value {
        match n {
            A::Node::Step(s) => json!({"step":s.name()}),
            A::Node::Seq(v) => json!({"seq":v.iter().map(node).collect::<Vec<_>>()}),
            A::Node::Branch { on, cases } => {
                json!({"branch":{"on":format!("{on:?}"),"cases":cases.iter().map(node).collect::<Vec<_>>()}})
            }
            A::Node::Repeat { body, until, bound } => {
                json!({"repeat":{"body":node(body),"until":format!("{until:?}"),"bound":bound}})
            }
        }
    }
    json!({"schema":"zeno-fcis/loop-strategy-plan/1","needs":plan.needs().iter().map(|a|format!("{a:?}")).collect::<Vec<_>>(),"goal":plan.goal().iter().map(|a|format!("{a:?}")).collect::<Vec<_>>(),"node":node(plan.node())})
}
fn p(value: &Value) -> Result<Program, String> {
    crate::neural_loop::program_json::program_from_json(value)
        .map_err(|_| "strategy-program-admission".into())
}
fn limit(l: SearchLimits) -> Q::Limits {
    Q::Limits {
        worlds: l.max_tuples,
        items: l.max_terms,
        cells: l.max_cells,
        work: l.max_search_work,
    }
}
fn stop(s: A::Stop, b: &Budget) -> (u8, Value) {
    doors_result(
        if s.inconclusive { 2 } else { 1 },
        "strategy",
        if s.inconclusive {
            "inconclusive"
        } else {
            "refused"
        },
        json!({"reason":s.code,"proposer_work":s.work,"work":b.work(),"limits":b.limits.data(),"attempts_spent":0}),
    )
}
fn list(v: &Value) -> Result<&Vec<Value>, String> {
    v.as_array().ok_or("strategy-array".into())
}
fn int(v: &Value) -> Result<usize, String> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("strategy-integer".into())
}
fn programs(v: &Value) -> Result<Vec<Program>, String> {
    list(v)?.iter().map(p).collect()
}

pub(super) fn command(
    kind: &str,
    path: &Path,
    original: Option<&Path>,
    out: Option<&Path>,
    limits: SearchLimits,
) -> (u8, Value) {
    if !limits.valid() {
        return refused("search-limits-out-of-bounds");
    }
    if out.is_some_and(Path::exists) {
        return refused("output-must-be-new-directory");
    }
    let input = match pipeline::read_json(path) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(obj) = input.as_object() else {
        return refused("strategy-problem-shape");
    };
    let fields: &[&str] = match kind {
        "split-by-cases" => &["schema", "learn", "config"],
        "diagnose" => &["schema", "circuit", "vector", "observations", "max_faults"],
        "ask-when-it-matters" => &["schema", "background", "readings", "query"],
        _ => return refused("strategy-kind"),
    };
    if let Err(r) = only_keys(obj, fields, "strategy-problem") {
        return refused(&r);
    }
    if kind != "split-by-cases" && original.is_some() {
        return refused("strategy-original-only-for-cases");
    }
    if input["schema"] != "zeno-fcis/loop-strategy-problem/1" {
        return refused("strategy-problem-schema");
    }
    let Some(plan) = A::named(kind, 64) else {
        return refused("strategy-kind");
    };
    let plan_value = plan_json(&plan);
    let mut budget = Budget::new(limits);
    match A::check(&plan, 100_000) {
        Ok(report) if report.problems.is_empty() => {}
        Ok(_) => return refused("strategy-plan-type-error"),
        Err(s) => return stop(s, &budget),
    }
    let case_original = if kind == "split-by-cases" {
        let Some(path) = original else {
            return refused("strategy-split-original-required");
        };
        match pipeline::read_program(path) {
            Ok(bytes) => Some(bytes),
            Err(result) => return result,
        }
    } else {
        None
    };
    let binding = json!({"problem_sha256":sha256_hex(crate::transform::canonical_json(&input).as_bytes()),"implementation_sha256":pipeline::implementation(),"plan_sha256":sha256_hex(crate::transform::canonical_json(&plan_value).as_bytes())});
    let run: Result<StrategyRun, String> = (|| match kind {
        "split-by-cases" => {
            let bytes = case_original.ok_or("strategy-split-original-required")?;
            let original = import_program(&bytes).map_err(|_| "strategy-original-admission")?;
            let learn = &input["learn"];
            let Some(learn_obj) = learn.as_object() else {
                return Err("strategy-learn-interface".into());
            };
            only_keys(
                learn_obj,
                &[
                    "schema",
                    "original_sha256",
                    "status",
                    "integers",
                    "predicates",
                ],
                "strategy-learn-interface",
            )?;
            if learn["schema"] != "zeno-fcis/learn-terms/1"
                || learn["original_sha256"] != sha256_hex(&bytes)
            {
                return Err("strategy-learn-binding".into());
            }
            match learn["status"].as_str() {
                Some("inconclusive") => {
                    return Ok((
                        json!({"status":"inconclusive","reason":"strategy-learn-incomplete","proposer_work":0}),
                        vec![],
                        None,
                    ));
                }
                Some("complete") => {}
                _ => return Err("strategy-learn-status".into()),
            }
            let corpus = S::LearnTermsV1 {
                integers: programs(&learn["integers"])?,
                predicates: programs(&learn["predicates"])?,
                complete: true,
            };
            let mut c = S::Config::default();
            if !input["config"].is_null() {
                let cfg = input["config"].as_object().ok_or("strategy-config")?;
                only_keys(
                    cfg,
                    &[
                        "initial",
                        "depth",
                        "committee",
                        "alternatives",
                        "leaf_alternatives",
                        "spot_checks",
                        "queries",
                    ],
                    "strategy-config",
                )?;
                for (key, value) in cfg {
                    let n = int(value)?;
                    match key.as_str() {
                        "initial" => c.initial = n,
                        "depth" => c.depth = u8::try_from(n).map_err(|_| "strategy-depth")?,
                        "committee" => c.committee = n,
                        "alternatives" => c.alternatives = n,
                        "leaf_alternatives" => c.leaf_alternatives = n,
                        "spot_checks" => c.spot_checks = n,
                        "queries" => c.queries = n,
                        _ => return Err("strategy-config-field".into()),
                    }
                }
            }
            let result = budget
                .search(|| S::split(&original, &corpus, c, limit(limits)))
                .map_err(|r| format!("budget:{r}"))?;
            let mut report = match result {
                Ok(r) => r,
                Err(s) => {
                    return Ok((
                        json!({"status":if s.inconclusive{"inconclusive"}else{"refused"},"reason":s.code,"proposer_work":s.work,
                            "search_bounds":{"depth":c.depth,"queries":c.queries,"terms":corpus.integers.len(),
                                "predicates":corpus.predicates.len(),"committee":c.committee,"alternatives":c.alternatives,
                                "leaf_alternatives":c.leaf_alternatives}}),
                        s.trace,
                        None,
                    ));
                }
            };
            // BEGIN L2: sample fit must cross capture before native qualification.
            report.trace.push(A::Step::Capture);
            let check = budget.capture_and_gate(&bytes, &report.candidate, report.work);
            if accepted(&check["capture"]["check"]) {
                report.trace.push(A::Step::CheckProgram);
            }
            // END L2.
            let status = if accepted(&check) {
                "qualified"
            } else if check["verdict"] == "counterexample" {
                "refuted"
            } else if check["verdict"] == "refused" {
                "refused"
            } else {
                "inconclusive"
            };
            let detail = json!({"status":status,"best":program_json(&report.candidate),"capture":check["capture"],"best_check":check,"original":program_json(&original),"binding_original_sha256":sha256_hex(&bytes),"queried":report.queried,"committee_queries":report.committee_queries,"spot_queries":report.spot_queries,"region_cells":report.region_cells,"proposer_work":report.work,"learn_interface":"zeno-fcis/learn-terms/1","claim":"sample fit and region coverage propose; capture then original-bound check and replay qualify"});
            Ok((detail, report.trace, Some((bytes, report.candidate))))
        }
        "diagnose" => {
            let cv = &input["circuit"];
            let o = cv.as_object().ok_or("strategy-circuit")?;
            only_keys(o, &["inputs", "gates", "outputs"], "strategy-circuit")?;
            let mut gates = Vec::new();
            for g in list(&cv["gates"])? {
                let o = g.as_object().ok_or("strategy-gate")?;
                only_keys(o, &["kind", "wires"], "strategy-gate")?;
                let kind = match g["kind"].as_str() {
                    Some("and") => Q::GateKind::And,
                    Some("or") => Q::GateKind::Or,
                    Some("xor") => Q::GateKind::Xor,
                    Some("nand") => Q::GateKind::Nand,
                    Some("nor") => Q::GateKind::Nor,
                    Some("not") => Q::GateKind::Not,
                    _ => return Err("strategy-gate-kind".into()),
                };
                gates.push(Q::Gate {
                    kind,
                    wires: list(&g["wires"])?
                        .iter()
                        .map(int)
                        .collect::<Result<_, _>>()?,
                });
            }
            let c = Q::Circuit {
                inputs: int(&cv["inputs"])?,
                gates,
                outputs: list(&cv["outputs"])?
                    .iter()
                    .map(int)
                    .collect::<Result<_, _>>()?,
            };
            let vector = list(&input["vector"])?
                .iter()
                .map(|v| {
                    v.as_i64()
                        .map(i128::from)
                        .ok_or("strategy-input-value".into())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut observed = BTreeMap::new();
            for pair in list(&input["observations"])? {
                let pair = list(pair)?;
                if pair.len() != 2 {
                    return Err("strategy-observation".into());
                }
                let wire = int(&pair[0])?;
                let value = pair[1]
                    .as_i64()
                    .map(i128::from)
                    .ok_or("strategy-observation-value")?;
                if observed.insert(wire, value).is_some() {
                    return Err("strategy-duplicate-observation".into());
                }
            }
            let max_faults = if input["max_faults"].is_null() {
                2
            } else {
                int(&input["max_faults"])?
            };
            let result = budget
                .search(|| Q::diagnose(&c, &vector, &observed, max_faults, limit(limits)))
                .map_err(|r| format!("budget:{r}"))?;
            let r = match result {
                Ok(r) => r,
                Err(s) => {
                    return Ok((
                        json!({"status":if s.inconclusive{"inconclusive"}else{"refused"},"reason":s.code,"proposer_work":s.work}),
                        s.trace,
                        None,
                    ));
                }
            };
            Ok((
                json!({"status":if r.probe.is_some(){"question"}else{"proposed-diagnosis"},"diagnoses":r.diagnoses.iter().map(|d|json!({"gates":d.gates,"worlds":d.worlds})).collect::<Vec<_>>(),"question":r.probe.map(|p|json!({"wire":p.wire,"zero":p.zero,"one":p.one})),"proposer_work":r.work,"claim":"inclusion-minimal weak-fault proposals conditioned on supplied measurements; not actual fault identification"}),
                r.trace,
                None,
            ))
        }
        "ask-when-it-matters" => {
            let background = p(&input["background"])?;
            let mut groups = Vec::new();
            for row in list(&input["readings"])? {
                let o = row.as_object().ok_or("strategy-reading")?;
                only_keys(o, &["id", "text", "alternatives"], "strategy-reading")?;
                let id = row["id"]
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 64)
                    .ok_or("strategy-reading-id")?
                    .to_owned();
                if !row["text"].is_null() && !row["text"].as_str().is_some_and(|s| s.len() <= 4096)
                {
                    return Err("strategy-reading-text".into());
                }
                groups.push(Q::Reading {
                    id,
                    alternatives: programs(&row["alternatives"])?,
                });
            }
            let query = match input["query"].as_str() {
                Some("count") => Q::Query::Count,
                Some("must") => Q::Query::Must,
                Some("possible") => Q::Query::Possible,
                _ => return Err("strategy-query".into()),
            };
            let result = budget
                .search(|| Q::readings(&background, &groups, query, limit(limits)))
                .map_err(|r| format!("budget:{r}"))?;
            let r = match result {
                Ok(r) => r,
                Err(s) => {
                    return Ok((
                        json!({"status":if s.inconclusive{"inconclusive"}else{"refused"},"reason":s.code,"proposer_work":s.work}),
                        s.trace,
                        None,
                    ));
                }
            };
            let answer = r
                .answers
                .iter()
                .map(|(_, a)| *a)
                .collect::<std::collections::BTreeSet<_>>();
            let status = if r.question.is_some() {
                "question"
            } else if answer.len() == 1 {
                "proposed-answer"
            } else {
                "proposed-ambiguous"
            };
            let q=r.question.map(|q|json!({"statement":q.statement,"id":groups[q.statement].id,"text":input["readings"][q.statement]["text"],"ordinal":q.ordinal,"world":q.world,"holds":q.holds,"does_not_hold":q.does_not_hold}));
            Ok((
                json!({"status":status,"query":input["query"],"distinct":r.distinct,"answers":r.answers,"answer":if answer.len()==1{answer.first().copied()}else{None},"question":q,"proposer_work":r.work,"claim":"answer stability over listed finite readings only; no owner intent or missing-reading completeness"}),
                r.trace,
                None,
            ))
        }
        _ => Err("strategy-kind".into()),
    })();
    let (detail, trace, candidate) = match run {
        Ok(r) => r,
        Err(r) => {
            return if r.starts_with("budget:") {
                doors_result(
                    2,
                    "strategy",
                    "inconclusive",
                    json!({"reason":r,"work":budget.work(),"attempts_spent":0}),
                )
            } else {
                refused(&r)
            };
        }
    };
    match A::conforms(&plan, &trace, 100_000) {
        Ok(true) => {}
        Ok(false) => return refused("strategy-trace-does-not-conform"),
        Err(s) => return stop(s, &budget),
    }
    let mut detail = stopped_detail(
        detail,
        budget.stages.started.elapsed() >= budget.stages.deadline,
    );
    detail["kind"] = json!(kind);
    detail["plan"] = plan_value;
    detail["plan_check"] = json!({"type_checked":true,"trace_conforms":true,"scope":"structural macro-step prefix; not semantic qualification"});
    detail["trace"] = json!(trace.iter().map(|s| s.name()).collect::<Vec<_>>());
    detail["binding"] = binding;
    detail["authority"] = json!("none");
    detail["attempts_spent"] = json!(0);
    detail["work"] = budget.work();
    detail["limits"] = limits.data();
    let status = detail["status"].as_str().unwrap_or("refused").to_owned();
    let pair = candidate.as_ref();
    let (code, result) = pipeline::finish(
        "strategy",
        detail,
        &status,
        out,
        pair.map(|(b, _)| b.as_slice()),
        pair.map(|(_, p)| p),
    );
    if matches!(
        status.as_str(),
        "question" | "proposed-diagnosis" | "proposed-answer" | "proposed-ambiguous"
    ) && code == 1
        && result["status"] == status
    {
        return (0, result);
    }
    (code, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_late_deadline_has_no_partial_response_or_export_for_any_mode() {
        for (kind, proposal) in [
            (
                "ask-when-it-matters",
                json!({"status":"proposed-answer", "answer":true, "answers":[[0,true]], "distinct":1, "question":{"id":"r"}, "proposer_work":19}),
            ),
            (
                "diagnose",
                json!({"status":"proposed-diagnosis", "diagnoses":[[1]], "probe":{"id":"g"}, "count":1, "proposer_work":23}),
            ),
            (
                "split-by-cases",
                json!({"status":"qualified", "selection":{"leaf":1}, "best":{"nodes":2}, "best_check":{"verdict":"equivalent"}, "proposer_work":29}),
            ),
        ] {
            let completed = stopped_detail(proposal.clone(), false);
            assert_eq!(completed, proposal);
            let stopped = stopped_detail(proposal.clone(), true);
            assert_eq!(stopped["proposer_work"], proposal["proposer_work"]);
            assert_eq!(stopped["reason"], "invocation-deadline");
            let out = std::env::temp_dir().join(format!(
                "zeno-late-strategy-{}-{}",
                std::process::id(),
                crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let (code, response) =
                pipeline::finish("strategy", stopped, "inconclusive", Some(&out), None, None);
            assert_eq!(code, 2, "{kind}");
            let exported: Value = serde_json::from_slice(
                &std::fs::read(out.join("result.json")).unwrap_or_else(|e| panic!("{e}")),
            )
            .unwrap_or_else(|e| panic!("{e}"));
            for field in [
                "answer",
                "answers",
                "question",
                "distinct",
                "diagnoses",
                "probe",
                "count",
                "selection",
                "best",
                "best_check",
            ] {
                assert!(response["detail"].get(field).is_none(), "{kind}: {field}");
                assert!(exported.get(field).is_none(), "{kind}: exported {field}");
            }
            assert!(!out.join("selection.json").exists());
            assert!(!out.join("candidate.zcve").exists());
            std::fs::remove_dir_all(out).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn strategy_core_stop_also_discards_proposal_fields_and_retains_work() {
        for status in ["inconclusive", "refused"] {
            let stopped = stopped_detail(
                json!({"status":status, "reason":"strategy-work-budget", "proposer_work":31, "answer":true, "diagnoses":[[0]], "selection":{"leaf":0}}),
                false,
            );
            assert_eq!(
                stopped,
                json!({"status":status,"reason":"strategy-work-budget","proposer_work":31})
            );
        }
        let stopped = stopped_detail(
            json!({"status":"inconclusive", "best":{"nodes":2},
            "best_check":{"verdict":"inconclusive","reason":"gate-budget"},"proposer_work":37}),
            false,
        );
        assert_eq!(
            stopped,
            json!({"status":"inconclusive","reason":"gate-budget","proposer_work":37})
        );
    }
}
