use crate::boolean::{Boolean, input_index, rooted_outputs};
use egg::{
    Analysis, CostFunction, DidMerge, EGraph, Extractor, Id, Language, LpCostFunction, LpExtractor,
    RecExpr, Rewrite, Runner, SimpleScheduler, rewrite,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct Truth {
    nvars: usize,
    mask: u64,
}
impl Truth {
    pub fn new(nvars: usize) -> Self {
        let rows = 1usize << nvars;
        Self {
            nvars,
            mask: if rows == 64 {
                u64::MAX
            } else {
                (1u64 << rows) - 1
            },
        }
    }
}
impl Analysis<Boolean> for Truth {
    type Data = u64;
    fn make(egraph: &mut EGraph<Boolean, Self>, node: &Boolean, _: Id) -> u64 {
        let at = |id: Id| egraph[id].data;
        let mask = egraph.analysis.mask;
        match node {
            Boolean::True => mask,
            Boolean::False => 0,
            Boolean::Input(symbol) => {
                let index =
                    input_index(*symbol, egraph.analysis.nvars).expect("validated Boolean leaf");
                (0..1usize << egraph.analysis.nvars)
                    .filter(|row| row & (1 << index) != 0)
                    .fold(0, |value, row| value | (1u64 << row))
            }
            Boolean::Not(a) => !at(*a) & mask,
            Boolean::And([a, b]) => at(*a) & at(*b),
            Boolean::Or([a, b]) => at(*a) | at(*b),
            Boolean::Ite([c, a, b]) => (at(*c) & at(*a)) | (!at(*c) & at(*b) & mask),
        }
    }
    fn merge(&mut self, to: &mut u64, from: u64) -> DidMerge {
        assert_eq!(
            *to, from,
            "A proposed rewrite/semantic union changed exact Boolean truth signature"
        );
        DidMerge(false, false)
    }
}

fn merge_signatures(egraph: &mut EGraph<Boolean, Truth>) -> usize {
    let mut classes: Vec<_> = egraph
        .classes()
        .map(|class| (class.id, class.data))
        .collect();
    classes.sort_by_key(|&(id, _)| id);
    let mut representatives = BTreeMap::new();
    let mut merged = 0;
    for (id, signature) in classes {
        if let Some(&representative) = representatives.get(&signature) {
            merged += usize::from(egraph.union(representative, id));
        } else {
            representatives.insert(signature, id);
        }
    }
    egraph.rebuild();
    merged
}

// Binary instances of the frozen ordinary baseline identities. Commutation
// and association expose flattened absorption/factors; the exploration budget
// can stop before every generalized baseline proposal is exposed.
fn rules() -> Vec<Rewrite<Boolean, Truth>> {
    vec![
        rewrite!("not-true"; "(not true)"=>"false"),
        rewrite!("not-false"; "(not false)"=>"true"),
        rewrite!("double-not"; "(not (not ?a))"=>"?a"),
        rewrite!("and-true"; "(and ?a true)"=>"?a"),
        rewrite!("and-false"; "(and ?a false)"=>"false"),
        rewrite!("or-false"; "(or ?a false)"=>"?a"),
        rewrite!("or-true"; "(or ?a true)"=>"true"),
        rewrite!("and-self"; "(and ?a ?a)"=>"?a"),
        rewrite!("or-self"; "(or ?a ?a)"=>"?a"),
        rewrite!("and-complement"; "(and ?a (not ?a))"=>"false"),
        rewrite!("or-complement"; "(or ?a (not ?a))"=>"true"),
        rewrite!("and-commute"; "(and ?a ?b)"=>"(and ?b ?a)"),
        rewrite!("or-commute"; "(or ?a ?b)"=>"(or ?b ?a)"),
        rewrite!("and-associate"; "(and (and ?a ?b) ?c)"=>"(and ?a (and ?b ?c))"),
        rewrite!("or-associate"; "(or (or ?a ?b) ?c)"=>"(or ?a (or ?b ?c))"),
        rewrite!("or-absorb"; "(or ?a (and ?a ?b))"=>"?a"),
        rewrite!("and-absorb"; "(and ?a (or ?a ?b))"=>"?a"),
        rewrite!("de-morgan-and"; "(not (and ?a ?b))"=>"(or (not ?a) (not ?b))"),
        rewrite!("de-morgan-or"; "(not (or ?a ?b))"=>"(and (not ?a) (not ?b))"),
        rewrite!("factor-and"; "(or (and ?a ?b) (and ?a ?c))"=>"(and ?a (or ?b ?c))"),
        rewrite!("factor-or"; "(and (or ?a ?b) (or ?a ?c))"=>"(or ?a (and ?b ?c))"),
        rewrite!("ite-true"; "(ite true ?a ?b)"=>"?a"),
        rewrite!("ite-false"; "(ite false ?a ?b)"=>"?b"),
        rewrite!("ite-equal"; "(ite ?c ?a ?a)"=>"?a"),
        rewrite!("ite-id"; "(ite ?c true false)"=>"?c"),
        rewrite!("ite-not"; "(ite ?c false true)"=>"(not ?c)"),
        rewrite!("ite-negated-condition"; "(ite (not ?c) ?a ?b)"=>"(ite ?c ?b ?a)"),
        rewrite!("ite-true-arm"; "(ite ?c true ?b)"=>"(or ?c ?b)"),
        rewrite!("ite-false-arm"; "(ite ?c false ?b)"=>"(and (not ?c) ?b)"),
        rewrite!("ite-else-true"; "(ite ?c ?a true)"=>"(or (not ?c) ?a)"),
        rewrite!("ite-else-false"; "(ite ?c ?a false)"=>"(and ?c ?a)"),
        rewrite!("ite-expand"; "(ite ?c ?a ?b)"=>"(or (and ?c ?a) (and (not ?c) ?b))"),
        rewrite!("not-ite"; "(not (ite ?c ?a ?b))"=>"(ite ?c (not ?a) (not ?b))"),
    ]
}

fn node_cost(node: &Boolean) -> f64 {
    if matches!(node, Boolean::Or(_)) {
        4.0
    } else {
        1.0
    }
}
struct DagCost;
impl LpCostFunction<Boolean, Truth> for DagCost {
    fn node_cost(&mut self, _: &EGraph<Boolean, Truth>, _: Id, node: &Boolean) -> f64 {
        node_cost(node)
    }
}
struct TreeCost;
impl CostFunction<Boolean> for TreeCost {
    type Cost = f64;
    fn cost<C>(&mut self, node: &Boolean, mut child: C) -> f64
    where
        C: FnMut(Id) -> f64,
    {
        node.children()
            .iter()
            .fold(node_cost(node), |cost, &id| cost + child(id))
    }
}

struct StatusLogger {
    records: Mutex<Vec<String>>,
}
static STATUS_LOGGER: StatusLogger = StatusLogger {
    records: Mutex::new(Vec::new()),
};
impl log::Log for StatusLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "egg::lp_extract"
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            STATUS_LOGGER
                .records
                .lock()
                .unwrap()
                .push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}
pub fn initialize_logger() -> Result<(), String> {
    log::set_logger(&STATUS_LOGGER)
        .map_err(|e| format!("Cannot establish scoped LP status capture: {e}"))?;
    log::set_max_level(log::LevelFilter::Info);
    Ok(())
}
fn classify_status(records: &[String]) -> &'static str {
    if records
        .iter()
        .any(|s| s == "Solver timed out, solution may not be optimal.")
    {
        return "time_limit";
    }
    if records
        .iter()
        .any(|s| s == "Solver reached gap limit, solution may not be optimal.")
    {
        return "gap_limit";
    }
    if records
        .iter()
        .filter(|s| s.as_str() == "Solution is optimal")
        .count()
        == 1
    {
        "optimal"
    } else {
        "unknown"
    }
}
fn stable_cbc(
    problem: good_lp::variable::UnsolvedProblem,
) -> good_lp::solvers::coin_cbc::CoinCbcProblem {
    let mut model = good_lp::coin_cbc(problem);
    model.set_parameter("randomSeed", "1");
    model.set_parameter("randomCbcSeed", "1");
    model.set_parameter("threads", "1");
    model.set_parameter("logLevel", "0");
    model
}

pub struct Optimized {
    pub outputs: Vec<String>,
    pub metadata: Value,
    pub failed: Option<String>,
}
pub fn optimize(
    expressions: &[RecExpr<Boolean>],
    nvars: usize,
    semantic: bool,
    tree: bool,
    iterations: usize,
    node_limit: usize,
    lp_seconds: f64,
) -> Result<Optimized, String> {
    let start = Instant::now();
    let mut egraph = EGraph::new(Truth::new(nvars));
    egraph.add(Boolean::False);
    egraph.add(Boolean::True);
    for i in 0..nvars {
        egraph.add(Boolean::Input(format!("v{i}").into()));
    }
    let roots: Vec<_> = expressions
        .iter()
        .map(|expr| egraph.add_expr(expr))
        .collect();
    egraph.rebuild();
    if egraph.total_size() > node_limit {
        return Err("Initial egraph exceeds registered node budget".into());
    }
    let mut runner: Runner<Boolean, Truth> = Runner::new(Truth::new(nvars))
        .with_egraph(egraph)
        .with_iter_limit(iterations)
        .with_node_limit(node_limit)
        .with_time_limit(Duration::from_secs(600))
        .with_scheduler(SimpleScheduler);
    if semantic {
        runner = runner.with_hook(|runner| {
            merge_signatures(&mut runner.egraph);
            Ok(())
        });
    }
    let mut runner = runner.run(&rules());
    let final_semantic_merges = if semantic {
        merge_signatures(&mut runner.egraph)
    } else {
        0
    };
    let egraph = &runner.egraph;
    let saturation_ms = start.elapsed().as_secs_f64() * 1000.0;
    let extraction = Instant::now();
    let (outputs, status, records, mut failed) = if tree {
        let extractor = Extractor::new(egraph, TreeCost);
        let outputs = roots
            .iter()
            .map(|&root| extractor.find_best(root).1.to_string())
            .collect();
        (outputs, "not_applicable", Vec::new(), None)
    } else {
        STATUS_LOGGER.records.lock().unwrap().clear();
        let solved = catch_unwind(AssertUnwindSafe(|| {
            LpExtractor::new(egraph, DagCost)
                .solve_multiple_with_timeout(&roots, stable_cbc, lp_seconds)
        }));
        let records = std::mem::take(&mut *STATUS_LOGGER.records.lock().unwrap());
        let status = classify_status(&records);
        match solved {
            Ok((expr, selected)) => {
                let outputs = rooted_outputs(&expr, &selected);
                let failed = if status == "optimal" {
                    None
                } else {
                    Some(format!(
                        "LP status {status} is extraction failure under registered protocol"
                    ))
                };
                (outputs, status, records, failed)
            }
            Err(payload) => {
                let reason = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "Non-string solver panic".into());
                return Err(format!(
                    "LP extraction panic: {reason}; status={status}; solver_log={records:?}"
                ));
            }
        }
    };
    if matches!(runner.stop_reason, Some(egg::StopReason::TimeLimit(_))) {
        failed = Some("Equality saturation emergency watchdog timed out".into());
    }
    Ok(Optimized {
        outputs,
        failed,
        metadata: json!({"iterations":runner.iterations.len(),"egraph_nodes":egraph.total_size(),
        "eclasses":egraph.number_of_classes(),"stop_reason":format!("{:?}",runner.stop_reason),"final_semantic_merges":final_semantic_merges,
        "lp_timeout_budget_seconds":if tree {None}else{Some(lp_seconds)},"solver_status":status,"solver_log":records,
        "saturation_ms":saturation_ms,"extraction_ms":extraction.elapsed().as_secs_f64()*1000.0,
        "surrogate_weights":{"and":1,"not":1,"ite":1,"input":1,"bool":1,"or":4},
        "tie_controls":{"egg_deterministic":true,"semantic_group_order":"sorted canonical class ID and BTreeMap signatures","cbc_randomSeed":1,"cbc_randomCbcSeed":1,"cbc_threads":1},
        "scc_scope":"All possible edges, including unselected alternatives; patched LP extractor applies continuous ranks only inside non-singleton SCCs and disables direct self-loops.",
        "optimality_scope":"Positive backend optimal status is for its surrogate class-consistent DAG objective, subject to floating-point tolerances; no minimum final emitted FCIS cost claim.",
        "budget_scope":"Iteration/node limits are checked between rewrite batches and can overshoot within a batch; 600s emergency saturation watchdog, LP timeout is solver budget only."}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positive_status_required_and_limits_never_accepted() {
        assert_eq!(classify_status(&[]), "unknown");
        assert_eq!(classify_status(&["Solution is optimal".into()]), "optimal");
        assert_eq!(
            classify_status(&["Solver timed out, solution may not be optimal.".into()]),
            "time_limit"
        );
        assert_eq!(
            classify_status(&["Solver reached gap limit, solution may not be optimal.".into()]),
            "gap_limit"
        );
    }
}
