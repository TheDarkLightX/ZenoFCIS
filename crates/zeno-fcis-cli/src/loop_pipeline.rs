//! Guarded program-stage composition. LLM-generated proposer orchestration.
//!
//! Inputs are owned snapshots. Only the existing transform checker with matched
//! receipt replay qualifies a candidate. Stage guards and costs are advisory;
//! no owner answer, contract adoption or session attempt is produced here.

use super::*;
use clap::Args;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use zeno_fcis_loop_doors::strategy_algebra as algebra;
use zeno_fcis_program_search::concrete;

pub(super) const PIPELINE_SCHEMA: &str = "zeno-fcis/loop-pipeline/1";
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ProgramStage {
    Complete,
    Repair,
    Canonical,
    Resubstitute,
}
impl ProgramStage {
    fn primitive(self) -> algebra::Step {
        match self {
            Self::Complete => algebra::Step::Complete,
            Self::Repair => algebra::Step::Repair,
            Self::Canonical => algebra::Step::Canonical,
            Self::Resubstitute => algebra::Step::Resubstitute,
        }
    }
}
pub(super) const OPERATORS: [ProgramStage; 4] = [
    ProgramStage::Complete,
    ProgramStage::Repair,
    ProgramStage::Canonical,
    ProgramStage::Resubstitute,
];

/// Shared check/time limits and per-stage proposer bounds for an invocation.
#[derive(Args, Clone, Copy, Debug)]
pub(crate) struct SearchLimits {
    /// Input tuples per full-domain check, at most 131,072.
    #[arg(long, default_value_t = 131_072)]
    pub(super) max_tuples: u64,
    /// Original/candidate evaluation pairs reserved, including receipt replay.
    #[arg(long, default_value_t = 1_000_000)]
    pub(super) max_gate_pairs: u64,
    /// Proposer work allowance per search stage, at most 100,000,000; not total CPU work.
    #[arg(long, default_value_t = 50_000_000)]
    pub(super) max_search_work: u64,
    /// Proposer table-cell bound, at most 16,777,216.
    #[arg(long, default_value_t = 4_194_304)]
    pub(super) max_cells: u64,
    /// Hole assignments per completion search, at most 20,000.
    #[arg(long, default_value_t = 2_000)]
    pub(super) max_assignments: usize,
    /// Terms or rule-bank entries admitted, at most 512.
    #[arg(long, default_value_t = 128)]
    pub(super) max_terms: usize,
    /// Shared check-and-replay requests per invocation, at most 30.
    #[arg(long, default_value_t = 24)]
    pub(super) max_checks: u32,
    /// Invocation wall allowance in milliseconds, at most 22,000.
    #[arg(long, default_value_t = 22_000)]
    pub(super) deadline_ms: u64,
}

impl SearchLimits {
    pub(super) fn valid(self) -> bool {
        self.max_tuples <= DOOR_MAX_TUPLES
            && self.max_gate_pairs <= 4_000_000
            && self.max_search_work <= 100_000_000
            && self.max_cells <= 1 << 24
            && self.max_assignments <= 20_000
            && self.max_terms <= 512
            && self.max_checks <= 30
            && self.deadline_ms <= 22_000
    }

    pub(super) fn data(self) -> Value {
        json!({"max_tuples": self.max_tuples, "max_gate_pairs": self.max_gate_pairs,
            "max_search_work": self.max_search_work, "max_cells": self.max_cells,
            "max_assignments": self.max_assignments, "max_terms": self.max_terms,
            "max_checks": self.max_checks, "deadline_ms": self.deadline_ms})
    }
}

/// Exclusive shell accounting. It cannot supply a semantic verdict.
pub(super) struct Budget {
    pub(super) stages: Stages,
    pub(super) limits: SearchLimits,
    pub(super) checks: u32,
    pub(super) gate_pairs: u64,
    pub(super) searches: u32,
    pub(super) knowledge: Option<crate::loop_knowledge::LoopPolicy>,
    // Owned invocation accounting; a reservation is retained on refusal.
    capture_work: u64,
    capture_cells: u64,
}

impl Budget {
    pub(super) fn new(limits: SearchLimits) -> Self {
        Self {
            stages: Stages::new(Instant::now(), Duration::from_millis(limits.deadline_ms), 0),
            limits,
            checks: 0,
            gate_pairs: 0,
            searches: 0,
            knowledge: None,
            capture_work: 0,
            capture_cells: 0,
        }
    }

    pub(super) fn work(&self) -> Value {
        let mut work = json!({"check_requests":self.checks,"gate_pairs_reserved":self.gate_pairs,"search_stage_requests":self.searches,"control_frames":self.stages.frames,"gate_work_kind":"full-domain-pair-allowance-including-replay","synthesis_work_is_total_cpu":false});
        work["capture_work_reserved"] = json!(self.capture_work);
        work["capture_cells_reserved"] = json!(self.capture_cells);
        if let Some(k) = &self.knowledge {
            work["knowledge_consultation_work"] = json!(k.consultation_work);
            work["knowledge_snapshot_sha256"] = json!(k.snapshot);
        }
        work
    }

    pub(super) fn search<T>(&mut self, f: impl FnOnce() -> T) -> Result<T, String> {
        self.searches += 1;
        self.stages.run(true, f)
    }

    pub(super) fn gate(&mut self, original: &[u8], candidate: &Program) -> Value {
        let bytes = match bytes_of(candidate) {
            Some(bytes) => bytes,
            None => return json!({"verdict": "refused", "reason": "candidate-encoding"}),
        };
        let size = import_program(original).ok().map(|p| tuples(p.inputs()));
        let Some(size) = size.filter(|n| *n <= u128::from(self.limits.max_tuples)) else {
            return json!({"verdict": "inconclusive", "reason": "domain-budget"});
        };
        let allowance = u64::try_from(size.saturating_mul(2)).unwrap_or(u64::MAX);
        if self.checks >= self.limits.max_checks
            || allowance > self.limits.max_gate_pairs.saturating_sub(self.gate_pairs)
        {
            return json!({"verdict": "inconclusive", "reason": "gate-budget"});
        }
        self.checks += 1;
        self.gate_pairs += allowance;
        let mut report = checked(&mut self.stages, original, &bytes, None);
        report["gate_pairs_reserved"] = json!(allowance);
        report["candidate_sha256"] = json!(sha256_hex(&bytes));
        report["original_sha256"] = json!(sha256_hex(original));
        report["domain_tuples"] = json!(u64::try_from(size).unwrap_or(u64::MAX));
        report
    }

    /// Complete reference acquisition -> capture checker/replay -> native
    /// original checker/replay. Neither a table nor a sample fit qualifies.
    pub(super) fn capture_and_gate(
        &mut self,
        original: &[u8],
        candidate: &Program,
        proposal_work: u64,
    ) -> Value {
        use zeno_fcis_loop_doors::capture::{MAX_TUPLES, ReferenceTable};
        let reference = match import_program(original) {
            Ok(reference) => reference,
            Err(_) => return json!({"verdict":"refused","reason":"capture-reference-admission"}),
        };
        if reference.inputs() != candidate.inputs() || reference.outputs() != candidate.outputs() {
            return json!({"verdict":"refused","reason":"capture-candidate-abi"});
        }
        let count = tuples(reference.inputs());
        if count > u128::from(self.limits.max_tuples.min(MAX_TUPLES)) {
            return json!({"verdict":"inconclusive","reason":"capture-domain-budget",
                "limit":self.limits.max_tuples.min(MAX_TUPLES),"needed":count.to_string()});
        }
        let cells = count
            .saturating_mul((3 * reference.inputs().len() + 2 * reference.outputs().len()) as u128);
        // Conservative reservation for evaluations and bounded table encoding;
        // it is not a claim about total CPU instructions or allocator usage.
        let work = count
            .saturating_mul(
                (reference.nodes().len() + reference.inputs().len() + reference.outputs().len())
                    as u128,
            )
            .saturating_add(
                count
                    .saturating_mul(count)
                    .saturating_mul(reference.outputs().len() as u128),
            );
        if cells > u128::from(self.limits.max_cells.saturating_sub(self.capture_cells)) {
            return json!({"verdict":"inconclusive","reason":"capture-cell-budget",
                "limit":self.limits.max_cells,"needed":cells.to_string()});
        }
        if work
            > u128::from(
                self.limits
                    .max_search_work
                    .saturating_sub(proposal_work)
                    .saturating_sub(self.capture_work),
            )
        {
            return json!({"verdict":"inconclusive","reason":"capture-work-budget",
                "limit":self.limits.max_search_work,"needed":work.to_string()});
        }
        self.capture_cells += u64::try_from(cells).unwrap_or(u64::MAX);
        self.capture_work += u64::try_from(work).unwrap_or(u64::MAX);
        let max_tuples = self.limits.max_tuples;
        let table = match self.search(|| ReferenceTable::from_program(&reference, max_tuples)) {
            Err(reason) => return json!({"verdict":"inconclusive","reason":reason}),
            Ok(Err(reason)) => {
                return json!({"verdict":"refused","reason":reason.0,
                "capture_profile":"complete-successful-reference-table"});
            }
            Ok(Ok(table)) => table,
        };
        let encoded = match self.search(|| table.program()) {
            Err(reason) => return json!({"verdict":"inconclusive","reason":reason}),
            Ok(Err(reason)) => return json!({"verdict":"inconclusive","reason":reason.0}),
            Ok(Ok(encoded)) => encoded,
        };
        let Some(captured_bytes) = bytes_of(&encoded) else {
            return json!({"verdict":"refused","reason":"capture-reference-encoding"});
        };
        let capture_check = self.gate(&captured_bytes, candidate);
        let capture = json!({"status":if accepted(&capture_check){"captured"}else{"not-captured"},
            "complete":true,"rows":table.rows().len(),"reference_sha256":sha256_hex(original),
            "table_program_sha256":sha256_hex(&captured_bytes),
            "reference_program":program_json(&encoded),"check":capture_check});
        if !accepted(&capture_check) {
            let mut check = capture_check;
            check["capture"] = capture;
            return check;
        }
        let mut check = self.gate(original, candidate);
        check["capture"] = capture;
        check
    }
}

/// Compiled source binding. Embedded bytes describe this build, not files a
/// proposer can rewrite during a search. This is not a binary attestation.
pub(crate) fn implementation() -> String {
    let sources = [
        include_str!("loop_pipeline.rs"),
        include_str!("loop_knowledge.rs"),
        include_str!("knowledge_command.rs"),
        include_str!("knowledge_evidence.rs"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_knowledge.rs.txt"),
        include_str!(
            "../source-snapshots/zeno-fcis-loop-doors/src/strategy_knowledge/reasoning.rs.txt"
        ),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/examples/strategy-example.kb.txt"),
        include_str!("loop_strategy.rs"),
        include_str!("loop_rules.rs"),
        include_str!("loop_strategy_profiles.rs"),
        // BEGIN L1/L2 compiled source binding.
        include_str!("loop_learn.rs"),
        include_str!("loop_capture.rs"),
        include_str!("loop_capture_whole.rs"),
        include_str!("../../zeno-fcis-spec/src/bool_circuit.rs"),
        include_str!("../../zeno-fcis-spec/src/bits.rs"),
        include_str!("../../zeno-fcis-spec/src/constant_tables.rs"),
        include_str!("../../zeno-fcis-spec/src/recursive_tables.rs"),
        include_str!("../../zeno-fcis-contract/src/internals/genesis.rs"),
        include_str!("../../zeno-fcis-contract/src/internals/graph.rs"),
        include_str!("../../zeno-fcis-contract/src/internals/table_expr.rs"),
        include_str!("../../zeno-fcis-contract/src/internals/table_rules.rs"),
        include_str!("../source-snapshots/zeno-fcis-spec/src/ast.rs.txt"),
        include_str!("../../zeno-fcis-spec/src/collections.rs"),
        include_str!("../source-snapshots/zeno-fcis-spec/src/parser.rs.txt"),
        include_str!("../../zeno-fcis-spec/src/tables.rs"),
        include_str!("../source-snapshots/zeno-fcis-contract/src/internals/expr.rs.txt"),
        include_str!("loop_version_space.rs"),
        include_str!("loop_search_limits.rs"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/learn.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/capture.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/version_space.rs.txt"),
        // END L1/L2 compiled source binding.
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_algebra.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_questions.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/strategy_split.rs.txt"),
        include_str!("loop_doors_command.rs"),
        include_str!("transform.rs"),
        include_str!(
            "../source-snapshots/zeno-fcis-shell-sqlite/src/v2/equivalence/checker.rs.txt"
        ),
        include_str!(
            "../source-snapshots/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_api.rs.txt"
        ),
        include_str!(
            "../source-snapshots/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_equality.rs.txt"
        ),
        include_str!(
            "../source-snapshots/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_ranges.rs.txt"
        ),
        include_str!(
            "../source-snapshots/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_spec.rs.txt"
        ),
        include_str!("neural_loop/program_json.rs"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/concrete.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/lib.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/arithmetic.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/pool.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/ir.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/space.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/resub.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/topdown.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/target.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/fold.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/diagram.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-program-search/src/util.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/expr.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/lower.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/subject.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/sketch.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/repair.rs.txt"),
        include_str!("../source-snapshots/zeno-fcis-loop-doors/src/inverse.rs.txt"),
    ];
    let mut bytes = Vec::new();
    for source in sources {
        bytes.extend_from_slice(
            &u64::try_from(source.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        bytes.extend_from_slice(source.as_bytes());
    }
    bytes.extend_from_slice(&zeno_fcis_synthesis::finite::v2_authority::EVALUATOR);
    sha256_hex(&bytes)
}

// Strict JSON also refuses duplicate keys in nested documents. Value parsing
// alone would silently use the last duplicate. Deserializer limits nesting.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Read;
        impl<'de> Visitor<'de> for Read {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate fields")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, _v: f64) -> Result<Unique, E> {
                Err(E::custom("fractional-json-number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
                let mut out = Vec::new();
                while let Some(Unique(value)) = seq.next_element()? {
                    out.push(value);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut out = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate-json-field"));
                    }
                    let Unique(value) = map.next_value()?;
                    out.insert(key, value);
                }
                Ok(Unique(Value::Object(out)))
            }
        }
        d.deserialize_any(Read)
    }
}

pub(super) fn read_json(path: &Path) -> Result<Value, (u8, Value)> {
    let bytes = read_bytes(path)?;
    if !bytes.is_ascii() {
        return Err(refused("document-not-ascii"));
    }
    serde_json::from_slice::<Unique>(&bytes)
        .map(|v| v.0)
        .map_err(|_| refused("document-json-or-duplicate"))
}

pub(super) fn read_program(path: &Path) -> Result<Vec<u8>, (u8, Value)> {
    let bytes = crate::transform_command::read_bounded(path, PROGRAM_LIMIT)
        .map_err(|error| super::super::io_failure(&error))?;
    if crate::transform_command::over(&bytes, PROGRAM_LIMIT) {
        return Err(refused("program-too-large"));
    }
    import_program(&bytes).map_err(|_| refused("original-not-admitted"))?;
    Ok(bytes)
}

pub(super) fn stages(value: &Value) -> Result<Vec<ProgramStage>, String> {
    let array = value.as_array().ok_or("stages-shape")?;
    if array.len() > 5 {
        return Err("stages-count".into());
    }
    array
        .iter()
        .map(|v| match v.as_str() {
            Some("complete") => Ok(ProgramStage::Complete),
            Some("repair") => Ok(ProgramStage::Repair),
            Some("canonical") => Ok(ProgramStage::Canonical),
            Some("resubstitute") => Ok(ProgramStage::Resubstitute),
            _ => Err("stage-outside-grammar".into()),
        })
        .collect()
}

pub(super) struct Task {
    pub(super) original: Vec<u8>,
    pub(super) program: Program,
    pub(super) proposal: Value,
    pub(super) binding: Value,
    expressions: Vec<Expr>,
    vocabulary: Option<VocabularySpec>,
    parameters: BTreeMap<String, i128>,
    candidate: Option<Program>,
}

pub(super) fn task(original: Vec<u8>, proposal: Value) -> Result<Task, String> {
    let object = proposal.as_object().ok_or("pipeline-shape")?;
    only_keys(
        object,
        &[
            "schema",
            "original_sha256",
            "implementation_sha256",
            "expressions",
            "program",
            "stages",
        ],
        "pipeline",
    )?;
    if proposal["schema"] != PIPELINE_SCHEMA {
        return Err("pipeline-schema".into());
    }
    let original_sha256 = sha256_hex(&original);
    if proposal["original_sha256"] != original_sha256 {
        return Err("subject-binding-mismatch".into());
    }
    if proposal["implementation_sha256"] != implementation() {
        return Err("implementation-binding-mismatch".into());
    }
    let program = import_program(&original).map_err(|_| "original-not-admitted")?;
    let has_expressions = object.contains_key("expressions");
    if has_expressions == object.contains_key("program") {
        return Err("exactly-one-proposal-representation".into());
    }
    let (expressions, vocabulary, parameters, candidate) = if has_expressions {
        let doc_bytes =
            serde_json::to_vec(&proposal["expressions"]).map_err(|_| "document-encoding")?;
        let doc = document(&doc_bytes)?;
        let spec = doc.vocabulary.ok_or("pipeline-vocabulary-required")?;
        let vocab = vocabulary(&spec, &program)?;
        let expressions = expressions_of(&doc.outputs, Allow::ALL)?;
        let expressions = expressions
            .iter()
            .map(|n| {
                let node =
                    zeno_fcis_loop_doors::expr::bind_params(n, &doc.parameters).map_err(|e| e.0)?;
                vocab.resolve_variants(&node).map_err(|e| e.0)
            })
            .collect::<Result<Vec<_>, String>>()?;
        if expressions.len() != program.outputs().len() {
            return Err("proposal-output-count".into());
        }
        let candidate = if expressions.iter().any(has_holes) {
            None
        } else {
            Some(lower_outputs(&expressions, &vocab, &program).map_err(|e| e.0)?)
        };
        (expressions, Some(spec), doc.parameters, candidate)
    } else {
        let candidate = crate::neural_loop::program_json::program_from_json(&proposal["program"])
            .map_err(|_| "candidate-not-admitted")?;
        (Vec::new(), None, BTreeMap::new(), Some(candidate))
    };
    let binding = json!({"original_sha256": original_sha256, "implementation_sha256": implementation(),
        "scope": "full-declared-cartesian-domain", "inputs": crate::transform::domains_json(program.inputs()),
        "outputs": crate::transform::domains_json(program.outputs()),
        "proposal_sha256": sha256_hex(crate::transform::canonical_json(&proposal).as_bytes())});
    Ok(Task {
        original,
        program,
        proposal,
        binding,
        expressions,
        vocabulary,
        parameters,
        candidate,
    })
}

pub(super) struct Run {
    pub(super) status: &'static str,
    pub(super) report: Value,
    pub(super) best: Option<Program>,
}

fn classify(check: &Value) -> &'static str {
    if accepted(check) {
        "qualified"
    } else if check["verdict"] == "counterexample" {
        "refuted"
    } else if check["verdict"] == "inconclusive" || check["verdict"] == "equivalent" {
        "inconclusive"
    } else {
        "refused"
    }
}

/// Missing measurements cannot establish a decrease.
pub(super) fn decreasing_cost<T: Ord>(candidate: Option<T>, current: Option<T>) -> bool {
    matches!((candidate, current), (Some(candidate), Some(current)) if candidate < current)
}

#[derive(Debug)]
enum StageFailure {
    Refused(String),
    Inconclusive(String),
}
impl From<String> for StageFailure {
    fn from(reason: String) -> Self {
        Self::Refused(reason)
    }
}
impl From<&str> for StageFailure {
    fn from(reason: &str) -> Self {
        Self::Refused(reason.into())
    }
}
impl StageFailure {
    fn status(&self) -> &'static str {
        match self {
            Self::Refused(_) => "refused",
            Self::Inconclusive(_) => "inconclusive",
        }
    }
    fn reason(self) -> String {
        match self {
            Self::Refused(reason) | Self::Inconclusive(reason) => reason,
        }
    }
}

fn search_settings(limits: SearchLimits) -> zeno_fcis_program_search::Settings {
    let mut settings = zeno_fcis_program_search::Settings::default();
    settings.budget.max_tuples = limits.max_tuples;
    settings.budget.max_cells = limits.max_cells;
    settings.budget.work_units = limits.max_search_work;
    settings.budget.rounds = 2;
    settings.budget.resub_rounds = 2;
    settings.proposers.top_down = false;
    settings.proposers.fold = false;
    settings.proposers.diagram = false;
    settings.shape.max_new = 1;
    settings
}

fn subject(task: &Task, budget: &mut Budget) -> Result<Subject, StageFailure> {
    let spec = task
        .vocabulary
        .as_ref()
        .ok_or("stage-needs-expression-vocabulary")?;
    let vocab = vocabulary(spec, &task.program)?;
    let size = tuples(task.program.inputs());
    let cells =
        size.saturating_mul((task.program.inputs().len() + task.program.outputs().len()) as u128);
    if cells > u128::from(budget.limits.max_cells)
        || size.saturating_mul(task.program.nodes().len() as u128)
            > u128::from(budget.limits.max_search_work)
    {
        return Err(StageFailure::Inconclusive("subject-table-budget".into()));
    }
    let max_tuples = budget.limits.max_tuples;
    budget
        .search(|| {
            Subject::new(
                task.program.clone(),
                vocab,
                task.parameters.clone(),
                spec.cases,
                max_tuples,
            )
        })
        .map_err(StageFailure::Inconclusive)?
        .map_err(|r| StageFailure::Refused(r.0))
}

/// One immutable task through a fixed guarded sequence. Every emitted program
/// is checked independently against the same original, never the last result.
pub(super) fn run(task: &Task, policy: &[ProgramStage], budget: &mut Budget) -> Run {
    let mut effective = policy.to_vec();
    if let Some(k) = &budget.knowledge {
        effective.retain(|s| k.allows(s.primitive().name()));
        effective.sort_by_key(|s| k.rank(s.primitive().name()));
    }
    let policy = effective.as_slice();
    let mut plan = algebra::pipeline(&policy.iter().map(|s| s.primitive()).collect::<Vec<_>>());
    let mut plan_value = super::strategy_profiles::plan_json(&plan);
    let plan_status = algebra::check(&plan, 100_000);
    match plan_status {
        Ok(report) if report.problems.is_empty() => {}
        result => {
            let (status, reason) = match result {
                Err(s) if s.inconclusive => ("inconclusive", s.code),
                _ => ("refused", "pipeline-plan-type-error"),
            };
            return Run {
                status,
                best: None,
                report: json!({"status":status,"reason":reason,"original":program_json(&task.program),"candidates":[],"binding":task.binding,"plan":plan_value,"attempts_spent":0}),
            };
        }
    }
    let mut executed = Vec::new();
    let pairs_before = budget.gate_pairs;
    let checks_before = budget.checks;
    let mut trace = Vec::new();
    let mut candidates = Vec::new();
    let mut best: Option<Program> = None;
    let mut best_check = Value::Null;
    let mut expressions = task.expressions.clone();
    let mut holes = expressions.iter().any(has_holes);
    let mut status = "inconclusive";
    let mut reason = Value::Null;
    let size = tuples(task.program.inputs());
    if size > u128::from(budget.limits.max_tuples)
        || size.saturating_mul(task.program.inputs().len() as u128)
            > u128::from(budget.limits.max_cells)
        || size.saturating_mul(task.program.nodes().len() as u128)
            > u128::from(budget.limits.max_search_work)
    {
        reason = json!("domain-or-search-budget");
    } else {
        // A finite equivalence receipt may cover failures, but the discovery
        // target additionally needs at least one successful observation.
        let max_tuples = budget.limits.max_tuples;
        let nonvacuous = budget.search(|| {
            let columns = zeno_fcis_loop_doors::subject::columns(task.program.inputs(), max_tuples);
            columns.map(|columns| {
                (0..usize::try_from(size).unwrap_or(0)).any(|n| {
                    task.program
                        .evaluate(&columns.iter().map(|c| c[n]).collect::<Vec<_>>())
                        .is_ok()
                })
            })
        });
        match nonvacuous {
            Ok(Ok(true)) => {
                if let Some(candidate) = &task.candidate {
                    executed.push(algebra::Step::CheckProgram);
                    let check = budget.gate(&task.original, candidate);
                    status = classify(&check);
                    if status == "inconclusive" {
                        reason = check
                            .get("reason")
                            .cloned()
                            .unwrap_or_else(|| json!("proposal-check-or-replay-inconclusive"));
                    }
                    candidates.push(json!({"by": "proposal", "candidate": program_json(candidate), "check": check}));
                    if status == "qualified" {
                        best = Some(candidate.clone());
                        best_check = check;
                    }
                }
                let mut pending = policy.to_vec();
                let mut scheduled = Vec::new();
                while !pending.is_empty() {
                    if reason != Value::Null
                        || (task.candidate.is_some() && status == "inconclusive")
                    {
                        break;
                    }
                    // Configured routing chooses again from the artifacts actually
                    // present. An unavailable stage stays pending rather than being
                    // discarded before a later completion can make it meaningful.
                    let enabled = |stage: &ProgramStage| match stage {
                        ProgramStage::Complete => holes,
                        ProgramStage::Repair => {
                            !holes && !expressions.is_empty() && status == "refuted"
                        }
                        ProgramStage::Canonical | ProgramStage::Resubstitute => best.is_some(),
                    };
                    let position = if budget.knowledge.is_some() {
                        pending.iter().position(&enabled)
                    } else {
                        Some(0)
                    };
                    let Some(position) = position else {
                        for stage in &pending {
                            trace.push(json!({"stage":stage,"status":"guard-skipped"}));
                        }
                        break;
                    };
                    let stage = pending.remove(position);
                    if !enabled(&stage) {
                        trace.push(json!({"stage":stage,"status":"guard-skipped"}));
                        continue;
                    }
                    if budget.knowledge.is_some() {
                        scheduled.push(stage);
                        let next = algebra::pipeline(
                            &scheduled
                                .iter()
                                .chain(pending.iter())
                                .map(|s| s.primitive())
                                .collect::<Vec<_>>(),
                        );
                        match algebra::check(&next, 100_000) {
                            Ok(r) if r.problems.is_empty() => {}
                            _ => {
                                status = "inconclusive";
                                reason = json!("replan-type-check-stopped");
                                break;
                            }
                        }
                        plan_value = super::strategy_profiles::plan_json(&next);
                        plan = next;
                        trace.push(
                            json!({"stage":stage,"status":"replanned-from-observed-artifacts"}),
                        );
                    }
                    executed.push(stage.primitive());
                    let mut repair_search_stopped = false;
                    let result: Result<Option<(Program, Vec<Expr>)>, StageFailure> = match stage {
                        ProgramStage::Complete => {
                            if budget.limits.max_assignments == 0 {
                                Err(StageFailure::Inconclusive("assignment-budget".into()))
                            } else {
                                subject(task, budget).and_then(|subject| {
                                    let settings = sketch::Settings {
                                        max_assignments: budget.limits.max_assignments,
                                        max_work: budget.limits.max_search_work,
                                        ..sketch::Settings::default()
                                    };
                                    let result = budget.search(|| sketch::solve(&subject, &expressions, &settings)).map_err(StageFailure::Inconclusive)?;
                                    trace.push(json!({"stage": stage, "proposal_status": result.status.name(),
                                        "assignments": result.evaluated, "decided_by": "proposer"}));
                                    if result.status == SketchStatus::Inconclusive {
                                        return Err(StageFailure::Inconclusive("completion-budget".into()));
                                    }
                                    if result.status == SketchStatus::Refused {
                                        return Err(StageFailure::Refused(result.reason.unwrap_or_else(|| "completion-proposer-refused".into())));
                                    }
                                    Ok(result.program.map(|p| (p, result.expressions)))
                                })
                            }
                        }
                        ProgramStage::Repair => subject(task, budget).and_then(|subject| {
                            let vocab = task.vocabulary.as_ref().ok_or("repair-vocabulary")?;
                            let program = lower_outputs(
                                &expressions,
                                &vocabulary(vocab, &task.program)?,
                                &task.program,
                            )
                            .map_err(|e| e.0)?;
                            let mut settings = repair::Settings::default();
                            settings.synthesis.total_work = settings.synthesis.total_work.min(budget.limits.max_search_work);
                            settings.synthesis.budget.work_units = settings.synthesis.budget.work_units.min(budget.limits.max_search_work);
                            settings.synthesis.budget.max_cells = settings.synthesis.budget.max_cells.min(budget.limits.max_cells);
                            let recorded = budget
                                .search(|| {
                                    repair::explain_recorded(
                                        &subject,
                                        &expressions,
                                        &program,
                                        true,
                                        &settings,
                                    )
                                }).map_err(StageFailure::Inconclusive)?;
                            trace.push(json!({"stage": stage, "synthesis_work_used": recorded.synthesis_work_used,
                                "synthesis_stopped": recorded.synthesis_stopped, "decided_by": "proposer"}));
                            repair_search_stopped = recorded.synthesis_stopped;
                            let explained = recorded.outcome.map_err(|r| StageFailure::Refused(r.0))?;
                            let proposed = match explained.repair {
                                Some(repair) => {
                                    if let repair::Status::Refused(reason) = repair.status {
                                        return Err(StageFailure::Refused(reason.0));
                                    }
                                    repair.program.map(|p| (p, repair.expressions))
                                }
                                None => None,
                            };
                            if proposed.is_none() && repair_search_stopped {
                                return Err(StageFailure::Inconclusive("repair-synthesis-inconclusive".into()));
                            }
                            Ok(proposed)
                        }),
                        ProgramStage::Canonical => budget
                            .search(|| {
                                concrete::canonical_candidate(
                                    best.as_ref().unwrap_or(&task.program),
                                )
                            }).map_err(StageFailure::Inconclusive)
                            .and_then(|r| {
                                r.map(|p| Some((p, Vec::new())))
                                    .map_err(|e| StageFailure::Refused(format!("{e:?}")))
                            }),
                        ProgramStage::Resubstitute => {
                            let bytes = bytes_of(best.as_ref().unwrap_or(&task.program))
                                .unwrap_or_default();
                            let settings = search_settings(budget.limits);
                            budget
                                .search(|| zeno_fcis_program_search::search(&bytes, &[], &settings))
                                .map_err(StageFailure::Inconclusive)
                                .and_then(|r| match r {
                                    zeno_fcis_program_search::Outcome::Improved(c) => {
                                        import_program(&c.bytes)
                                            .map(|p| Some((p, Vec::new())))
                                            .map_err(|_| "search-program-admission".into())
                                    }
                                    zeno_fcis_program_search::Outcome::Unimproved(_) => {
                                        trace.push(json!({"stage":stage,"status":"unimproved",
                                            "reason":"portfolio-search-shape-exhausted",
                                            "search_bounds":{"rounds":settings.budget.rounds,
                                                "depth":settings.shape.depth,"width":settings.shape.width,
                                                "solve_limit":settings.shape.solve_limit,"max_new":settings.shape.max_new}}));
                                        Ok(best.clone().map(|p| (p, Vec::new())))
                                    }
                                    zeno_fcis_program_search::Outcome::Stopped(s) => {
                                        Err(StageFailure::Inconclusive(format!("search-stop:{s:?}")))
                                    }
                                })
                        }
                    };
                    let candidate = match result {
                        Err(why) => {
                            status = why.status();
                            reason = json!(why.reason());
                            break;
                        }
                        Ok(None) => {
                            trace.push(json!({"stage": stage, "status": "no-candidate-proposed",
                                "limits":budget.limits.data(),"search_bounds":{"synth_depth":sketch::Settings::default().synth_depth}}));
                            continue;
                        }
                        Ok(Some((candidate, nodes))) => {
                            if !nodes.is_empty() {
                                expressions = nodes;
                                holes = false;
                            }
                            candidate
                        }
                    };
                    executed.push(algebra::Step::CheckProgram);
                    let check = budget.gate(&task.original, &candidate);
                    status = classify(&check);
                    candidates.push(
                        json!({"by": stage, "candidate": program_json(&candidate), "check": check}),
                    );
                    if status == "inconclusive" {
                        reason = json!("candidate-check-or-replay-inconclusive");
                        break;
                    }
                    if repair_search_stopped && !accepted(&check) {
                        status = "inconclusive";
                        reason = json!("repair-synthesis-inconclusive");
                        break;
                    }
                    let candidate_cost = concrete::cost(&candidate).ok();
                    if status == "qualified"
                        && candidate_cost.is_some()
                        && best.as_ref().is_none_or(|p| {
                            decreasing_cost(candidate_cost.clone(), concrete::cost(p).ok())
                        })
                    {
                        best = Some(candidate);
                        best_check = check;
                    }
                }
                if reason == Value::Null && best.is_some() {
                    status = "qualified";
                }
                if best.is_none() && status == "inconclusive" && reason == Value::Null {
                    reason = json!("bounded-search-no-qualified-candidate");
                }
            }
            Ok(Ok(false)) => {
                reason = json!("all-original-observations-fail");
            }
            Ok(Err(r)) => {
                reason = json!(r.0);
            }
            Err(r) => {
                reason = json!(r);
            }
        }
    }
    let trace_conforms = algebra::conforms(&plan, &executed, 100_000);
    match &trace_conforms {
        Ok(true) => {}
        Ok(false) => {
            status = "refused";
            reason = json!("pipeline-trace-does-not-conform");
        }
        Err(s) => {
            status = if s.inconclusive {
                "inconclusive"
            } else {
                "refused"
            };
            reason = json!(s.code);
        }
    }
    if budget.stages.started.elapsed() >= budget.stages.deadline {
        status = "inconclusive";
        reason = json!("invocation-deadline");
    }
    let report = json!({"schema": PIPELINE_SCHEMA, "authority": "none", "status": status,
        "binding": task.binding, "original": program_json(&task.program), "proposal": task.proposal,
        "stages": policy, "trace": trace, "plan":plan_value,
        "execution_trace":executed.iter().map(|s|s.name()).collect::<Vec<_>>(),
        "plan_check":{"type_checked":true,"trace_conforms":matches!(trace_conforms,Ok(true)),"scope":"structural macro-step prefix"},
        "candidates": candidates,
        "best": best.as_ref().map(program_json), "best_check": best_check,
        "work": budget.work(), "work_delta": {"gate_pairs_reserved": budget.gate_pairs.saturating_sub(pairs_before),
            "check_requests": budget.checks.saturating_sub(checks_before)}, "limits": budget.limits.data(), "reason": reason,
        "attempts_spent": 0, "claim": "finite-program-equivalence-only"});
    Run {
        status,
        report,
        best,
    }
}

pub(super) fn finish(
    door: &str,
    mut report: Value,
    status: &str,
    out: Option<&Path>,
    original: Option<&[u8]>,
    best: Option<&Program>,
) -> (u8, Value) {
    super::search_limits::annotate(door, status, &mut report);
    if let Some(out) = out {
        if let Err(error) = std::fs::create_dir(out) {
            return super::super::io_failure(&error);
        }
        let write = || -> std::io::Result<()> {
            std::fs::write(out.join("result.json"), serde_json::to_vec_pretty(&report)?)?;
            if report["bank"].is_object() {
                std::fs::write(
                    out.join("bank.json"),
                    serde_json::to_vec_pretty(&report["bank"])?,
                )?;
            }
            if report["selection"].is_object() {
                std::fs::write(
                    out.join("selection.json"),
                    serde_json::to_vec_pretty(&report["selection"])?,
                )?;
            }
            if status == "qualified"
                && let (Some(original), Some(best)) = (original, best)
            {
                let bytes =
                    bytes_of(best).ok_or_else(|| std::io::Error::other("candidate-encoding"))?;
                std::fs::write(out.join("original.zcve"), original)?;
                std::fs::write(out.join("candidate.zcve"), bytes)?;
                // BEGIN L2: retain the exact table program for independent replay.
                if let Some(value) =
                    report["best_check"]["capture"]["reference_program"].get("program")
                {
                    let captured = crate::neural_loop::program_json::program_from_json(value)
                        .map_err(|_| std::io::Error::other("capture-reference-program"))?;
                    let bytes = bytes_of(&captured)
                        .ok_or_else(|| std::io::Error::other("capture-reference-encoding"))?;
                    std::fs::write(out.join("capture-reference.zcve"), bytes)?;
                }
                // END L2.
            }
            Ok(())
        };
        if let Err(error) = write() {
            return super::super::io_failure(&error);
        }
    }
    let exit = match status {
        "qualified" | "packet" | "evaluated" => 0,
        "inconclusive" => 2,
        _ => 1,
    };
    doors_result(exit, door, status, report)
}

pub(super) fn command(
    original: &Path,
    proposal: Option<&Path>,
    packet: bool,
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
    let original = match read_program(original) {
        Ok(b) => b,
        Err(r) => return r,
    };
    if packet {
        return finish(
            "pipeline",
            json!({"original_sha256": sha256_hex(&original),
        "implementation_sha256": implementation(), "scope": "full-declared-cartesian-domain",
        "grammar": OPERATORS, "limits": limits.data(), "authority": "none"}),
            "packet",
            None,
            None,
            None,
        );
    }
    let proposal = match proposal {
        Some(p) => match read_json(p) {
            Ok(v) => v,
            Err(r) => return r,
        },
        None => return refused("proposal-required"),
    };
    let policy =
        match stages(
            proposal
                .get("stages")
                .unwrap_or(&json!(["complete", "repair", "canonical"])),
        ) {
            Ok(p) => p,
            Err(r) => return refused(&r),
        };
    let task = match task(original, proposal) {
        Ok(t) => t,
        Err(r) => return refused(&r),
    };
    let mut budget = Budget::new(limits);
    budget.knowledge = knowledge.cloned();
    let run = run(&task, &policy, &mut budget);
    finish(
        "pipeline",
        run.report,
        run.status,
        out,
        Some(&task.original),
        run.best.as_ref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_replay_is_required_even_for_an_equivalent_report() {
        assert_eq!(
            classify(&json!({"verdict":"equivalent","replay":"differs"})),
            "inconclusive"
        );
        assert_eq!(
            classify(&json!({"verdict":"equivalent","replay":"matched"})),
            "qualified"
        );
    }

    #[test]
    fn pipeline_missing_cost_cannot_establish_a_decrease() {
        assert!(!decreasing_cost::<(usize, usize)>(None, Some((3, 40))));
        assert!(!decreasing_cost(Some((1usize, 20usize)), None));
        assert!(!decreasing_cost::<(usize, usize)>(None, None));
        assert!(!decreasing_cost(Some((3usize, 40usize)), Some((3, 40))));
        assert!(decreasing_cost(Some((1usize, 20usize)), Some((3, 40))));
    }

    #[test]
    fn pipeline_typed_refusal_is_distinct_from_a_budget_stop() {
        assert_eq!(
            StageFailure::Refused("bad-shape".into()).status(),
            "refused"
        );
        assert_eq!(
            StageFailure::Inconclusive("work-budget".into()).status(),
            "inconclusive"
        );
    }

    #[test]
    fn pipeline_grammar_and_duplicate_json_fields_refuse() {
        assert!(stages(&json!(["skip-check"])).is_err());
        assert!(stages(&json!(vec!["repair"; 6])).is_err());
        assert!(serde_json::from_slice::<Unique>(br#"{"stages":[],"stages":["repair"]}"#).is_err());
        assert!(serde_json::from_slice::<Unique>(br#"{"inner":{"x":1,"x":2}}"#).is_err());
    }

    #[test]
    fn capture_refusal_keeps_reserved_work_and_cannot_supply_a_receipt() {
        let reference = Program::try_new(
            vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 2 }],
            vec![zeno_fcis_synthesis::finite::Domain::Int { min: 0, max: 2 }],
            vec![zeno_fcis_synthesis::finite::Op::Input(0)],
            vec![0],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let bytes = bytes_of(&reference).unwrap_or_else(|| panic!("encoding"));
        let mut budget = Budget::new(SearchLimits {
            max_tuples: 4096,
            max_gate_pairs: 1_000_000,
            max_search_work: 2_000_000,
            max_cells: 1_000_000,
            max_assignments: 2000,
            max_terms: 128,
            max_checks: 24,
            deadline_ms: 0,
        });
        let stopped = budget.capture_and_gate(&bytes, &reference, 0);
        assert_eq!(stopped["verdict"], "inconclusive");
        assert!(stopped.get("receipt_sha256").is_none());
        assert!(budget.capture_work > 0);
        assert!(budget.capture_cells > 0);
        let spent = budget.capture_work;
        let _ = budget.capture_and_gate(&bytes, &reference, 0);
        assert!(budget.capture_work > spent);
        assert_eq!(budget.checks, 0);
    }
}
