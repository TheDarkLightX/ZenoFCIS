//! Deterministic, invocation-local proposals under the frozen finite grammar.

use super::{
    common::*,
    layout::{self, Features, Layout},
    reference,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_loop_doors::{
    expr::{self, Expr, Fill, Hole, Vocabulary},
    sketch::{self, Settings, Status},
};
use zeno_fcis_synthesis::finite::Domain;

pub(super) type Coordinate = (usize, usize);
pub(super) const PROPOSAL_WORK: u64 = 50_000_000;
pub(super) const LAYOUT_WORK: u64 = 1_600_000_000;
pub(super) struct Proposal {
    pub(super) document: Value,
    pub(super) examples: Value,
    pub(super) expressions: Option<Vec<Expr>>,
    pub(super) work: u64,
    pub(super) stop: Option<String>,
}
pub(super) fn example_rows(
    relation: &reference::Relation,
    layout: Layout,
    coordinates: &BTreeSet<Coordinate>,
) -> Result<Value> {
    if coordinates.is_empty() || coordinates.len() > 48 {
        return fail(65, "example-count");
    }
    let alpha = layout.alpha(relation)?;
    let mut rows = Vec::new();
    for (s, q) in coordinates {
        if *s >= alpha.len() || *q >= 16 {
            return fail(65, "example-coordinate");
        }
        let a = alpha[*s].ok_or_else(|| gate("integration-mismatch:invalid-feedback-state"))?;
        rows.push(
            json!({"raw_state":s,"input":q,"abstract_state":a,"requirement":relation.rows[a*16+q]}),
        );
    }
    Ok(
        json!({"schema":"zeno-fcis/priority-construction-examples/1","revision":REVISION,"layout":layout.name(),"rows":rows}),
    )
}

pub(super) fn propose(
    relation: &reference::Relation,
    layout: Layout,
    coordinates: &BTreeSet<Coordinate>,
) -> Result<Proposal> {
    let examples = example_rows(relation, layout, coordinates)?;
    let features = Features::new(layout);
    let sketches = features.sketches()?;
    let grammar = canonical(&features.grammar(&layout.alpha(relation)?)?)?;
    let mut work = 0u64;
    let mut records = Vec::new();
    let mut completed = Vec::new();
    let mut stop = None;
    let rows = examples["rows"]
        .as_array()
        .ok_or_else(|| malformed("examples-rows"))?;
    for (j, sketch) in sketches.iter().enumerate() {
        let selected: Vec<_> = rows
            .iter()
            .filter(|row| j == 0 || row["requirement"]["class"] == "Accept")
            .collect();
        if selected.is_empty() {
            return fail(1, "integration-mismatch:empty-state-target-set");
        }
        let mut inputs = Vec::new();
        let mut targets = Vec::new();
        for row in selected {
            let s = number(&row["raw_state"], 0, layout.raw_count() as i128 - 1)? as usize;
            let q = number(&row["input"], 0, 15)?;
            inputs.push(features.values(&layout.raw(s), q)?);
            let requirement = &row["requirement"];
            targets.push(if j == 0 {
                if requirement["class"] == "Accept" {
                    5
                } else {
                    number(&requirement["reason"], 201, 205)? - 201
                }
            } else {
                let successor = number(&requirement["successor"], 0, 44)? as usize;
                layout.encode(&relation.states[successor])?[j - 1]
            });
        }
        let max = if j == 0 { 5 } else { layout.bounds()[j - 1] };
        let vocab = Vocabulary::new(
            features.names.clone(),
            features.domains.clone(),
            vec![Domain::Int { min: 0, max }],
            &BTreeMap::new(),
        )
        .map_err(|e| gate(e.to_string()))?;
        let settings = Settings {
            max_work: PROPOSAL_WORK
                .checked_sub(work)
                .ok_or_else(|| gate("proposal-work-overflow"))?,
            ..Settings::default()
        };
        let problem = sketch::TargetProblem::new(vocab, inputs, targets, sketch.clone(), settings)
            .map_err(|e| gate(e.to_string()))?;
        let result = sketch::propose_targets(&problem);
        work = work
            .checked_add(result.work)
            .ok_or_else(|| gate("proposal-work-overflow"))?;
        if work > PROPOSAL_WORK {
            return fail(1, "proposal-work-over-cap");
        }
        let mut fills = BTreeMap::new();
        for (k, value) in &result.holes {
            let Fill::Choice(index) = value else {
                return fail(1, "out-of-grammar-fill");
            };
            fills.insert(format!("out{j}/hole{k}"), *index);
        }
        let search=result.search.as_ref().map(|s|json!({"tried":s.tried,"visits":s.visits,"class_values":s.class_values,"assignments":s.assignments.to_string(),"solved_alone":s.solved_alone}));
        records.push(json!({"output":j,"status":result.status.name(),"stop_reason":result.reason,"fills":fills,
            "expression":result.expression.as_ref().map(expr::print_symbolic),"evaluated":result.evaluated,"work":result.work,"search":search}));
        if result.status != Status::Completed {
            stop = Some(format!("target-output-{j}:{}", result.status.name()));
            break;
        }
        completed.push(
            result
                .expression
                .ok_or_else(|| gate("completed-target-without-expression"))?,
        );
    }
    let document = json!({"schema":"zeno-fcis/priority-construction-proposal/1","revision":REVISION,"layout":layout.name(),
        "proposer":"sketch::propose_targets/meanings","grammar_sha256":sha(&grammar),"examples_sha256":sha(&canonical(&examples)?),
        "outputs":records,"proposal_work":work,"status":if stop.is_none(){"proposed"}else{"incomplete"},"stop_reason":stop});
    if canonical(&document)?.len() > DOCUMENT_LIMIT {
        return fail(1, "proposal-size-limit");
    }
    Ok(Proposal {
        document,
        examples,
        expressions: stop.is_none().then_some(completed),
        work,
        stop,
    })
}

/// Rebuild from finite assignments only; never accept the recorded expression
/// as source, never search, and never adopt bytes from a caller's run.
pub(super) fn replay_proposal(
    relation: &reference::Relation,
    layout: Layout,
    document: &Value,
    examples: &Value,
) -> Result<Option<Vec<Expr>>> {
    closed(
        document,
        &[
            "schema",
            "revision",
            "layout",
            "proposer",
            "grammar_sha256",
            "examples_sha256",
            "outputs",
            "proposal_work",
            "status",
            "stop_reason",
        ],
    )?;
    if document["schema"] != "zeno-fcis/priority-construction-proposal/1"
        || document["revision"] != REVISION
        || document["layout"] != layout.name()
        || document["proposer"] != "sketch::propose_targets/meanings"
    {
        return fail(65, "proposal-schema-or-seed");
    }
    closed(examples, &["schema", "revision", "layout", "rows"])?;
    let mut coordinates = BTreeSet::new();
    let rows = examples["rows"]
        .as_array()
        .ok_or_else(|| malformed("example-rows"))?;
    for row in rows {
        closed(
            row,
            &["raw_state", "input", "abstract_state", "requirement"],
        )?;
        let coord = (
            number(&row["raw_state"], 0, layout.raw_count() as i128 - 1)? as usize,
            number(&row["input"], 0, 15)? as usize,
        );
        if !coordinates.insert(coord) {
            return fail(65, "duplicate-example-coordinate");
        }
    }
    if *examples != example_rows(relation, layout, &coordinates)? {
        return fail(65, "example-order-or-requirement");
    }
    let features = Features::new(layout);
    let sketches = features.sketches()?;
    if document["grammar_sha256"].as_str()
        != Some(sha(&canonical(&features.grammar(&layout.alpha(relation)?)?)?).as_str())
        || document["examples_sha256"].as_str() != Some(sha(&canonical(examples)?).as_str())
    {
        return fail(65, "proposal-grammar-or-examples-pin");
    }
    let records = document["outputs"]
        .as_array()
        .ok_or_else(|| malformed("proposal-outputs"))?;
    if records.is_empty() || records.len() > sketches.len() {
        return fail(65, "proposal-output-count");
    }
    let complete = document["status"] == "proposed";
    if !complete && document["status"] != "incomplete" {
        return fail(65, "proposal-status");
    }
    let mut work = 0u64;
    let mut outputs = Vec::new();
    for (j, record) in records.iter().enumerate() {
        closed(
            record,
            &[
                "output",
                "status",
                "stop_reason",
                "fills",
                "expression",
                "evaluated",
                "work",
                "search",
            ],
        )?;
        number(&record["output"], j as i128, j as i128)?;
        let used = number(&record["work"], 0, PROPOSAL_WORK as i128)? as u64;
        work += used;
        number(&record["evaluated"], 0, 20000)?;
        let holes = expr::holes_of(&sketches[j]);
        let fills = record["fills"]
            .as_object()
            .ok_or_else(|| malformed("proposal-fills"))?;
        let mut assignment = BTreeMap::new();
        for (name, value) in fills {
            let prefix = format!("out{j}/hole");
            let id = name
                .strip_prefix(&prefix)
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| malformed("proposal-hole-namespace"))?;
            let Some(Expr::Hole(_, Hole::Choice(alternatives))) = holes.get(id).copied() else {
                return fail(65, "out-of-grammar-hole");
            };
            if name != &format!("{prefix}{id}") {
                return fail(65, "noncanonical-hole-name");
            }
            let index = number(value, 0, alternatives.len() as i128 - 1)? as usize;
            assignment.insert(id, Fill::Choice(index));
        }
        if record["status"] == "completed" {
            if fills.len() != holes.len() {
                return fail(65, "missing-hole-assignment");
            }
            let filled =
                expr::fill(&sketches[j], &assignment).map_err(|e| malformed(e.to_string()))?;
            if record["expression"].as_str() != Some(expr::print_symbolic(&filled).as_str()) {
                return fail(65, "out-of-grammar-expression-or-seed");
            }
            outputs.push(filled);
        } else if complete || j + 1 != records.len() {
            return fail(65, "proposal-partial-output-order");
        }
    }
    if work > PROPOSAL_WORK || document["proposal_work"].as_u64() != Some(work) {
        return fail(65, "proposal-cumulative-work");
    }
    if complete && (records.len() != sketches.len() || !document["stop_reason"].is_null()) {
        return fail(65, "proposal-completion-shape");
    }
    Ok(complete.then_some(outputs))
}

pub(super) fn sources(layout: Layout, outputs: &[Expr]) -> Result<(String, Vec<u8>)> {
    let features = Features::new(layout);
    let sketches = features.sketches()?;
    if outputs.len() != sketches.len() {
        return fail(65, "materialization-output-count");
    }
    let mut cursor = &outputs[0];
    let mut cases = Vec::new();
    for j in 0..5 {
        let Expr::If(guard, index, tail) = cursor else {
            return fail(65, "selector-not-frozen-shape");
        };
        if **index != Expr::Int(j as i128) {
            return fail(65, "selector-case-order");
        }
        cases.push(json!({"when":layout::policy_guard(guard)?,"class":"Reject","reason":201+j,"post":{},"outbox":[]}));
        cursor = tail;
    }
    if *cursor != Expr::Int(5) {
        return fail(65, "selector-final-accept");
    }
    let post = outputs[1..]
        .iter()
        .enumerate()
        .map(|(j, node)| Ok((format!("{}", 110 + j), json!(layout::policy_text(node)?))))
        .collect::<Result<serde_json::Map<String, Value>>>()?;
    let payload = features
        .event()?
        .iter()
        .enumerate()
        .map(|(j, node)| Ok((format!("{}", 140 + j), json!(layout::policy_text(node)?))))
        .collect::<Result<serde_json::Map<String, Value>>>()?;
    cases.push(json!({"when":"true","class":"Accept","reason":Value::Null,"post":post,"outbox":[{"ordinal":0,"channel":300,"destination":"priority-events","payload":payload,"idempotency_ordinal":0}]}));
    let genesis = (0..layout.cells() + 1)
        .map(|j| (format!("{}", 110 + j), json!(0)))
        .collect::<serde_json::Map<_, _>>();
    let pre = features.validity("pre")?;
    let post_valid = features.validity("post")?;
    let project = reference::project(layout.name(), &layout.bounds(), Some((&pre, &post_valid)));
    let policy = canonical(
        &json!({"schema":"zeno-fcis/template-declarative-policy/2","template":format!("priority-{}",layout.name()),
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"103":["Text",1,32]},"variables":features.variables()?,"cases":cases,"genesis":genesis,
        "law_kinds":{"499":"DecisionConformance","500":"StateInvariant"}}),
    )?;
    if project.len() > SOURCE_LIMIT || policy.len() > SOURCE_LIMIT {
        return fail(1, "candidate-source-size-limit");
    }
    Ok((project, policy))
}

pub(super) fn abstraction(
    abstract_project: &str,
    abstract_policy: &str,
    relation: &reference::Relation,
    layout: Layout,
    project: &[u8],
    policy: &[u8],
) -> Result<Vec<u8>> {
    canonical(
        &json!({"schema":"zeno-fcis/contract-refinement/1","abstract_project_sha256":sha(abstract_project.as_bytes()),"abstract_policy_sha256":sha(abstract_policy.as_bytes()),
        "concrete_project_sha256":sha(project),"concrete_policy_sha256":sha(policy),"abstraction":layout.alpha(relation)?,"invalid_state_law":499}),
    )
}
