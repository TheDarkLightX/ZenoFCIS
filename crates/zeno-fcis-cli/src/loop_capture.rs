//! Black-box capture shell, LLM-generated. Doors propose; transform decides.
//! Reference source never enters a proposer packet or an ordinary loop view.

use crate::neural_loop::program_json::{domains_json, encode, program_from_json, program_to_json};
use crate::transform::{self, sha256_hex};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};
use zeno_fcis_loop_doors::{
    capture as proposal,
    expr::{self, Allow, Vocabulary},
    lower,
};
use zeno_fcis_synthesis::finite::{MAX_NODES, Program};

#[path = "capture_runtime.rs"]
mod runtime;

#[path = "loop_capture_whole.rs"]
mod whole;

const SCHEMA: &str = "zeno-fcis/black-box-capture/2";
const SESSION_SCHEMA: &str = "zeno-fcis/capture-session/1";
const FILE_LIMIT: usize = 65536;

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Owner-side whole-domain arithmetic capture by a complete bounded cover.
    /// Every piece needs the existing exact checker and receipt replay.
    Whole {
        #[command(flatten)]
        options: whole::Options,
    },
    /// Owner-side: freeze a complete contained executable reference into a new private session.
    Open {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        reference: PathBuf,
        #[arg(long, default_value = "python-function-v1")]
        reference_format: String,
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value_t = 4096)]
        max_tuples: u64,
        #[arg(long, default_value_t = 32)]
        rounds: u64,
        #[arg(long, default_value_t = 4096)]
        max_examples: u64,
        #[arg(long, default_value_t = 2_000_000)]
        node_visits: u64,
    },
    /// Public allowlisted ABI/round/examples; no reference, hidden labels or private evidence.
    Packet {
        #[arg(long)]
        session: PathBuf,
    },
    /// Pre-check every exposed example, then transform check plus exact receipt replay.
    Candidate {
        #[arg(long)]
        session: PathBuf,
        #[arg(
            long,
            conflicts_with = "expressions",
            required_unless_present = "expressions"
        )]
        candidate_json: Option<PathBuf>,
        /// Complete expressions over x0..xN, with no keep, holes or parameters.
        #[arg(long)]
        expressions: Option<PathBuf>,
    },
    /// Owner-only evidence. A previously captured program is checked and replayed afresh.
    Status {
        #[arg(long)]
        session: PathBuf,
        /// Include private table and events only if the report fits the output cap.
        #[arg(long)]
        full: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Budget {
    max_tuples: u64,
    rounds: u64,
    max_examples: u64,
    node_visits: u64,
}
impl Budget {
    fn validate(&self) -> Result<(), String> {
        if self.max_tuples > 4096
            || self.rounds > 64
            || self.max_examples > 4096
            || self.node_visits > 10_000_000
        {
            return Err("capture-budget-shape".into());
        }
        Ok(())
    }
    fn lifetime(&self) -> u64 {
        self.node_visits
            .saturating_mul(self.rounds.saturating_mul(2).saturating_add(1))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Session {
    schema: String,
    spec: Value,
    source_sha256: String,
    table_sha256: String,
    program_sha256: String,
    checker: Value,
    rows: Vec<Vec<i128>>,
    reference_observations: u64,
    budget: Budget,
    attempts: u64,
    invocations: u64,
    reserved_visits: u64,
    examples: Vec<usize>,
    status: String,
    events: Vec<Value>,
    candidate: Option<Value>,
    receipt: Option<Value>,
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(dir: &Path) -> Result<Self, String> {
        let path = dir.join("capture.lock");
        crate::atomic_create(&path, b"exclusive capture invocation\n")
            .map_err(|_| "capture-session-busy")?;
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn report(status: &str, mut detail: Value) -> (u8, Value) {
    super::search_limits::annotate("capture", status, &mut detail);
    let code = match status {
        "opened" | "packet" | "ready" | "captured-exact" => crate::OK,
        "inconclusive" => crate::BLOCKED,
        _ => crate::INVALID,
    };
    (
        code,
        json!({"schema":SCHEMA,"integer_encoding":crate::report_numbers::ENCODING,"door":"capture","authority":"none","status":status,"detail":detail}),
    )
}
fn refused(reason: &str) -> (u8, Value) {
    report("refused", json!({"reason":reason,"attempt_spent":false}))
}
fn inconclusive(reason: &str) -> (u8, Value) {
    report(
        "inconclusive",
        json!({"reason":reason,"attempt_spent":false}),
    )
}

/// JSON with no duplicate keys, nonintegral numbers, trailing bytes or deep recursion.
pub(super) fn strict_json(bytes: &[u8]) -> Result<Value, String> {
    struct Strict(Value);
    impl<'de> Deserialize<'de> for Strict {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Strict;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("integral JSON without duplicate fields")
                }
                fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Strict, E> {
                    Ok(Strict(json!(v)))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Strict, E> {
                    Ok(Strict(json!(v)))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Strict, E> {
                    Ok(Strict(json!(v)))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Strict, E> {
                    Ok(Strict(json!(v)))
                }
                fn visit_none<E: serde::de::Error>(self) -> Result<Strict, E> {
                    Ok(Strict(Value::Null))
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Strict, E> {
                    Ok(Strict(Value::Null))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut a: A,
                ) -> Result<Strict, A::Error> {
                    let mut out = Vec::new();
                    while let Some(Strict(v)) = a.next_element()? {
                        out.push(v);
                    }
                    Ok(Strict(Value::Array(out)))
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut a: A,
                ) -> Result<Strict, A::Error> {
                    let mut out = serde_json::Map::new();
                    while let Some((key, Strict(v))) = a.next_entry::<String, Strict>()? {
                        if out.insert(key, v).is_some() {
                            return Err(serde::de::Error::custom("duplicate field"));
                        }
                    }
                    Ok(Strict(Value::Object(out)))
                }
            }
            d.deserialize_any(Visitor)
        }
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let Strict(v) = Strict::deserialize(&mut decoder).map_err(|_| "capture-json")?;
    decoder.end().map_err(|_| "capture-json")?;
    Ok(v)
}

fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "capture-file-unreadable")?;
    if metadata.file_type().is_symlink() {
        return Err("capture-file-symlink".into());
    }
    if !metadata.is_file() {
        return Err("capture-file-not-regular".into());
    }
    let file = fs::File::open(path).map_err(|_| "capture-file-unreadable")?;
    if !file
        .metadata()
        .map_err(|_| "capture-file-unreadable")?
        .is_file()
    {
        return Err("capture-file-not-regular".into());
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(limit + 1).map_err(|_| "capture-file-bound")?)
        .read_to_end(&mut bytes)
        .map_err(|_| "capture-file-unreadable")?;
    if bytes.len() > limit {
        return Err("capture-file-bound".into());
    }
    Ok(bytes)
}
fn save(dir: &Path, state: &Session) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|_| "capture-session-json")?;
    if bytes.len() > runtime::PROTOCOL_LIMIT {
        return Err("capture-session-bound".into());
    }
    crate::atomic_replace(&dir.join("capture.json"), &bytes)
        .map_err(|_| "capture-session-write".to_owned())
}
fn spec_program(spec: &Value) -> Result<Program, String> {
    let fields = spec.as_object().ok_or("capture-spec-shape")?;
    if fields.len() != 2 || !fields.contains_key("inputs") || !fields.contains_key("outputs") {
        return Err("capture-spec-fields".into());
    }
    let outputs = spec["outputs"].as_array().ok_or("capture-spec-shape")?;
    let nodes = outputs
        .iter()
        .map(|d| {
            if d["kind"] == "Bool" {
                json!(["Bool", false])
            } else {
                json!(["Int", d["min"]])
            }
        })
        .collect::<Vec<_>>();
    program_from_json(
        &json!({"inputs":spec["inputs"],"outputs":outputs,"nodes":nodes,
        "roots":(0..nodes.len()).collect::<Vec<_>>()}),
    )
    .map_err(|_| "capture-spec-abi".into())
}

fn load(dir: &Path) -> Result<(Session, proposal::ReferenceTable, Vec<u8>), String> {
    if fs::symlink_metadata(dir)
        .map_err(|_| "capture-session-unreadable")?
        .file_type()
        .is_symlink()
    {
        return Err("capture-session-symlink".into());
    }
    let bytes = read(&dir.join("capture.json"), runtime::PROTOCOL_LIMIT)?;
    let state: Session =
        serde_json::from_value(strict_json(&bytes)?).map_err(|_| "capture-session-shape")?;
    state.budget.validate()?;
    let captured = state.status == "captured-exact";
    if captured != state.candidate.is_some()
        || captured != state.receipt.is_some()
        || !matches!(
            state.status.as_str(),
            "ready"
                | "captured-exact"
                | "refuted"
                | "precheck-refuted"
                | "candidate-refused"
                | "inconclusive"
                | "refused"
        )
    {
        return Err("capture-session-status-binding".into());
    }
    if state.schema != SESSION_SCHEMA
        || state.checker != crate::neural_loop::request::CheckerIdentity::current().json()
        || state.attempts > state.budget.rounds
        || state.invocations > state.budget.rounds.saturating_mul(2).saturating_add(1)
        || state.reserved_visits > state.budget.lifetime()
        || state.events.len() > 129
        || state.examples.len()
            > usize::try_from(state.budget.max_examples).map_err(|_| "capture-budget-shape")?
    {
        return Err("capture-session-binding".into());
    }
    let skeleton = spec_program(&state.spec)?;
    let table = proposal::ReferenceTable::new(
        skeleton.inputs().to_vec(),
        skeleton.outputs().to_vec(),
        state.rows.clone(),
    )
    .map_err(|_| "capture-reference-table")?;
    if table.points().len() as u64 > state.budget.max_tuples
        || state.examples.iter().any(|i| *i >= state.rows.len())
        || state
            .examples
            .iter()
            .enumerate()
            .any(|(i, v)| state.examples[..i].contains(v))
    {
        return Err("capture-session-binding".into());
    }
    let program = table.program().map_err(|_| "capture-reference-encoding")?;
    let bytes = encode(&program).map_err(|_| "capture-reference-encoding")?;
    if sha256_hex(&serde_json::to_vec(&state.rows).map_err(|_| "capture-session-json")?)
        != state.table_sha256
        || sha256_hex(&bytes) != state.program_sha256
        || state.reference_observations != 6 * state.rows.len() as u64
    {
        return Err("capture-session-binding".into());
    }
    Ok((state, table, bytes))
}

// The persisted Session keeps its existing raw-integer format and hash.
// Its public report is a fresh owned tree using the versioned scalar encoding.
fn session_report(state: &Session) -> Value {
    json!({
        "schema": state.schema, "spec": state.spec,
        "source_sha256": state.source_sha256, "table_sha256": state.table_sha256,
        "program_sha256": state.program_sha256, "checker": state.checker,
        "rows": state.rows.iter().map(|row| crate::report_numbers::integers(row)).collect::<Vec<_>>(),
        "reference_observations": state.reference_observations, "budget": state.budget,
        "attempts": state.attempts, "invocations": state.invocations,
        "reserved_visits": state.reserved_visits, "examples": state.examples,
        "status": state.status, "events": state.events,
        "candidate": state.candidate, "receipt": state.receipt,
    })
}

fn packet(state: &Session, table: &proposal::ReferenceTable) -> Value {
    let mut examples = state.examples.clone();
    examples.sort_unstable();
    json!({"profile":"zal/fcis-loop/1","abi":{"inputs":domains_json(table.inputs()),
        "outputs":domains_json(table.outputs()),"tuples":table.points().len(),"order":transform::ENUMERATION},
        "round":state.attempts,"examples":examples.iter().map(|i|json!({"input":crate::report_numbers::integers(&table.points()[*i]),"output":crate::report_numbers::integers(&table.rows()[*i])})).collect::<Vec<_>>()})
}

fn reference_rows(
    reply: Value,
    skeleton: &Program,
    count: usize,
) -> Result<Vec<Vec<i128>>, String> {
    let object = reply.as_object().ok_or("capture-reference-output-shape")?;
    if object.len() != 1 || !object.contains_key("rows") {
        return Err("capture-reference-output-shape".into());
    }
    let rows = reply["rows"]
        .as_array()
        .ok_or("capture-reference-output-shape")?;
    if rows.len() != count {
        return Err("capture-reference-row-count".into());
    }
    rows.iter()
        .map(|row| {
            let row = row.as_array().ok_or("capture-reference-output-shape")?;
            if row.len() != skeleton.outputs().len() {
                return Err("capture-reference-output-shape".into());
            }
            row.iter()
                .zip(skeleton.outputs())
                .map(|(v, d)| {
                    v.as_i64()
                        .map(i128::from)
                        .or_else(|| v.as_u64().map(i128::from))
                        .filter(|v| d.contains(*v))
                        .ok_or_else(|| "capture-reference-output-domain".to_owned())
                })
                .collect()
        })
        .collect()
}

#[derive(Default, Serialize)]
struct ReferenceWork {
    schedules_reserved: u64,
    observations_scheduled: u64,
    valid_observations_received: u64,
}

fn observed_table(
    source: &str,
    skeleton: &Program,
    points: &[Vec<i128>],
    stages: &mut super::Stages,
    work: &mut ReferenceWork,
) -> Result<Vec<Vec<i128>>, String> {
    observed_table_with(skeleton, points, work, |schedule| {
        stages.run(true, || runtime::invoke(source, schedule))?
    })
}

// The ordinary session uses one contained stage per schedule. Whole capture
// groups bounded schedules inside a stage while retaining these same repeat,
// reverse-order, strict-label and work-accounting checks.
fn observed_table_with(
    skeleton: &Program,
    points: &[Vec<i128>],
    work: &mut ReferenceWork,
    mut invoke: impl FnMut(&[Vec<i128>]) -> Result<Value, String>,
) -> Result<Vec<Vec<i128>>, String> {
    let mut baseline = None;
    for reversed in [false, true, false] {
        let mut order = (0..points.len()).collect::<Vec<_>>();
        if reversed {
            order.reverse();
        }
        let schedule = order
            .iter()
            .chain(&order)
            .map(|i| points[*i].clone())
            .collect::<Vec<_>>();
        // Reserve before execution. Failed workers have unknown internal work;
        // only complete, valid replies count as received observations.
        work.schedules_reserved += 1;
        work.observations_scheduled += schedule.len() as u64;
        let reply = invoke(&schedule)?;
        let rows = reference_rows(reply, skeleton, schedule.len())?;
        work.valid_observations_received += rows.len() as u64;
        let table = reconcile_reference(baseline.as_deref(), &order, &rows, |a, b| a == b)?;
        baseline = Some(table);
    }
    baseline.ok_or_else(|| "capture-reference-empty".into())
}

fn reconcile_reference(
    baseline: Option<&[Vec<i128>]>,
    order: &[usize],
    rows: &[Vec<i128>],
    mut same: impl FnMut(&[Vec<i128>], &[Vec<i128>]) -> bool,
) -> Result<Vec<Vec<i128>>, String> {
    let n = order.len();
    if rows.len() != 2 * n || order.iter().any(|i| *i >= n) {
        return Err("capture-reference-row-count".into());
    }
    if !same(&rows[..n], &rows[n..]) {
        return Err("capture-reference-nondeterministic".into());
    }
    let mut table = vec![Vec::new(); n];
    for (j, i) in order.iter().enumerate() {
        table[*i] = rows[j].clone();
    }
    if baseline.is_some_and(|old| !same(old, &table)) {
        return Err("capture-reference-nondeterministic".into());
    }
    Ok(table)
}

fn open(
    spec: &Path,
    reference: &Path,
    format: &str,
    dir: &Path,
    budget: Budget,
    stages: &mut super::Stages,
) -> (u8, Value) {
    let mut reference_work = ReferenceWork::default();
    let work: Result<Value, String> = (|| {
        budget.validate()?;
        if format != "python-function-v1" {
            return Err("capture-reference-format".into());
        }
        if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            return Err("capture-containment-unavailable:platform".into());
        }
        if dir.symlink_metadata().is_ok() {
            return Err("capture-session-exists".into());
        }
        let spec = strict_json(&read(spec, FILE_LIMIT)?)?;
        let skeleton = spec_program(&spec)?;
        let points = proposal::points(skeleton.inputs(), budget.max_tuples).map_err(|e| e.0)?;
        let source = String::from_utf8(read(reference, FILE_LIMIT)?)
            .map_err(|_| "capture-source-encoding")?;
        let rows = observed_table(&source, &skeleton, &points, stages, &mut reference_work)?;
        let table = proposal::ReferenceTable::new(
            skeleton.inputs().to_vec(),
            skeleton.outputs().to_vec(),
            rows.clone(),
        )
        .map_err(|e| e.0)?;
        let original = stages.run(true, || table.program())?.map_err(|e| e.0)?;
        let bytes = encode(&original).map_err(|_| "reference-encoding-budget")?;
        // Independently exercise the library evaluator on every encoded row.
        // This is an encoder consistency check; candidates still need transform.
        if !stages.run(false, || {
            points
                .iter()
                .zip(&rows)
                .all(|(p, r)| original.evaluate(p).ok().as_ref() == Some(r))
        })? {
            return Err("capture-table-encoding-defect".into());
        }
        let state = Session {
            schema: SESSION_SCHEMA.into(),
            spec,
            source_sha256: sha256_hex(source.as_bytes()),
            table_sha256: sha256_hex(&serde_json::to_vec(&rows).map_err(|_| "capture-table-json")?),
            program_sha256: sha256_hex(&bytes),
            checker: crate::neural_loop::request::CheckerIdentity::current().json(),
            reference_observations: 6 * points.len() as u64,
            rows,
            budget,
            attempts: 0,
            invocations: 0,
            reserved_visits: 0,
            examples: Vec::new(),
            status: "ready".into(),
            events: Vec::new(),
            candidate: None,
            receipt: None,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(dir)
                .map_err(|_| "capture-session-create")?;
        }
        #[cfg(not(unix))]
        fs::create_dir(dir).map_err(|_| "capture-session-create")?;
        if let Err(e) = save(dir, &state) {
            let _ = fs::remove_dir(dir);
            return Err(e);
        }
        Ok(packet(&state, &table))
    })();
    let (code, mut result) = match work {
        Ok(packet) => report(
            "opened",
            json!({"packet":packet,"assumption":"reference is a pure deterministic function of all declared inputs",
        "exact_subject":"complete frozen observed table"}),
        ),
        Err(e) if budget_stop(&e) => inconclusive(&e),
        Err(e) => refused(&e),
    };
    result["detail"]["reference_work"] = json!({"reservation":reference_work,
        "worker_internal_work":"not-measured","partial_failed_schedule":"unknown"});
    (code, result)
}

fn budget_stop(reason: &str) -> bool {
    matches!(
        reason,
        "reference-domain-budget"
            | "reference-encoding-budget"
            | "stage-budget"
            | "capture-whole-domain-budget"
            | "capture-whole-work-budget"
    ) || reason == "invocation-deadline"
        || reason == "stage-deadline"
        || reason == "stage-deadline-or-control-failure"
}

/// One authority path: the existing checker, immediately followed by receipt replay.
fn confirm(
    stages: &mut super::Stages,
    original: &[u8],
    candidate: &[u8],
) -> Result<(Value, Option<Value>), String> {
    stages.run(false, || compare_exact(original, candidate))
}

fn compare_exact(original: &[u8], candidate: &[u8]) -> (Value, Option<Value>) {
    let limits = transform::Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: proposal::MAX_TUPLES,
    };
    checker_reply(transform::check(original, candidate, limits), |receipt| {
        transform::replay(receipt, original, candidate, proposal::MAX_TUPLES)
    })
}

// The production verdict/receipt mapping has a testable checker-result and
// replay seam. Production always supplies the existing exact checker/replayer.
fn checker_reply(
    checked: Result<transform::Equivalence, transform::Rejection>,
    replay: impl FnOnce(&[u8]) -> transform::Replay,
) -> (Value, Option<Value>) {
    match checked {
        Ok(equal) => {
            let receipt = equal.receipt();
            let replay = replay(&receipt);
            (
                json!({"verdict":"equivalent","replay":if replay==transform::Replay::Matched{"matched"}else{"differs"},
                "checker":transform::CHECKER,"receipt_sha256":sha256_hex(&receipt)}),
                strict_json(&receipt).ok(),
            )
        }
        Err(transform::Rejection::Counterexample(c)) => (
            json!({"verdict":"counterexample","ordinal":c.ordinal}),
            None,
        ),
        Err(transform::Rejection::Inconclusive(_)) => (json!({"verdict":"inconclusive"}), None),
        Err(transform::Rejection::Refused(_)) => (json!({"verdict":"refused"}), None),
    }
}

fn proposed_program(
    bytes: &[u8],
    expressions: bool,
    table: &proposal::ReferenceTable,
) -> Result<Program, String> {
    let skeleton = spec_program(
        &json!({"inputs":domains_json(table.inputs()),"outputs":domains_json(table.outputs())}),
    )?;
    proposed_program_for_abi(bytes, expressions, &skeleton)
}

fn proposed_program_for_abi(
    bytes: &[u8],
    expressions: bool,
    skeleton: &Program,
) -> Result<Program, String> {
    let value = strict_json(bytes)?;
    if !expressions {
        return program_from_json(&value).map_err(|_| "capture-candidate-format".into());
    }
    if value.as_object().is_none_or(|m| m.len() != 2)
        || value["schema"] != "zeno-fcis/capture-expressions/1"
    {
        return Err("capture-expressions-shape".into());
    }
    let texts = value["outputs"]
        .as_array()
        .ok_or("capture-expressions-shape")?;
    let nodes = texts
        .iter()
        .map(|v| {
            expr::parse(
                v.as_str().ok_or("capture-expressions-shape")?,
                0,
                Allow::PLAIN,
            )
            .map_err(|_| "capture-expression-refused".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if nodes.iter().any(|node| {
        expr::names_of(node)
            .iter()
            .any(|name| name.starts_with("out"))
    }) {
        return Err("capture-expression-output-reference".into());
    }
    let names = (0..skeleton.inputs().len())
        .map(|i| format!("x{i}"))
        .collect::<Vec<_>>();
    let vocab = Vocabulary::new(
        names,
        skeleton.inputs().to_vec(),
        skeleton.outputs().to_vec(),
        &Default::default(),
    )
    .map_err(|_| "capture-vocabulary")?;
    lower::lower_outputs(&nodes, &vocab, skeleton).map_err(|_| "capture-lowering".into())
}

#[derive(Clone, Copy)]
enum Attempt {
    None,
    Spend,
    // The refutation was durably recorded before advisory work started.
    Recorded,
}

fn finish_event(
    dir: &Path,
    state: &mut Session,
    status: &str,
    mut detail: Value,
    attempt: Attempt,
) -> (u8, Value) {
    if matches!(attempt, Attempt::Spend) {
        state.attempts += 1;
    }
    if let Some(packet) = detail.get_mut("packet") {
        packet["round"] = json!(state.attempts);
    }
    detail["attempt_spent"] = json!(!matches!(attempt, Attempt::None));
    detail["attempts"] = json!(state.attempts);
    // Budget stops do not erase earlier work reservations or example history.
    state.status = status.into();
    let previous = state
        .events
        .last()
        .and_then(|event| event["example_count"].as_u64())
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n <= state.examples.len())
        .unwrap_or(0);
    if matches!(attempt, Attempt::Recorded) {
        let _ = state.events.pop(); // replace this invocation's durable decision event
    }
    let mut private = detail.clone();
    if let Some(fields) = private.as_object_mut() {
        fields.remove("packet");
        fields.remove("example");
    }
    if let Some(coverage) = private.get_mut("coverage") {
        let outcomes = coverage
            .as_object_mut()
            .and_then(|fields| fields.remove("outcomes"));
        if let Some(outcomes) = outcomes.and_then(|v| v.as_array().cloned()) {
            coverage["outcomes_count"] = json!(outcomes.len());
            coverage["reachable_count"] =
                json!(outcomes.iter().filter(|r| r["reachable"] == true).count());
            coverage["covered_count"] =
                json!(outcomes.iter().filter(|r| r["covered"] == true).count());
        }
    }
    state.events.push(
        json!({"status":status,"detail":private,"example_count":state.examples.len(),
        "new_example_ordinals":state.examples[previous..]}),
    );
    if let Err(e) = save(dir, state) {
        return refused(&e);
    }
    report(status, public_detail(detail))
}

// No reference-dependent receipt/hash or full checker witness enters a public
// candidate reply. The private session retains the complete evidence.
fn public_detail(mut detail: Value) -> Value {
    if let Some(fields) = detail.as_object_mut() {
        fields.remove("exact_reserved_visits");
        fields.remove("precheck_reserved_visits");
    }
    if let Some(check) = detail.get_mut("check").and_then(Value::as_object_mut) {
        check.remove("receipt_sha256");
        check.remove("ordinal");
    }
    detail
}

fn candidate(
    dir: &Path,
    path: &Path,
    expressions: bool,
    stages: &mut super::Stages,
) -> (u8, Value) {
    let _lock = match Lock::acquire(dir) {
        Ok(l) => l,
        Err(e) => return refused(&e),
    };
    let (mut state, table, original) = match load(dir) {
        Ok(v) => v,
        Err(e) => return refused(&e),
    };
    if state.candidate.is_some() {
        return refused("capture-session-already-captured");
    }
    if state.attempts >= state.budget.rounds {
        return inconclusive("round-budget");
    }
    if state.invocations >= state.budget.rounds.saturating_mul(2).saturating_add(1) {
        return inconclusive("invocation-budget");
    }
    // Prepay one whole invocation before reading a proposal or running search.
    // A killed invocation cannot lose its reservation or pretend it used zero.
    if state
        .reserved_visits
        .saturating_add(state.budget.node_visits)
        > state.budget.lifetime()
    {
        return inconclusive("lifetime-work-budget");
    }
    state.invocations += 1;
    state.reserved_visits += state.budget.node_visits;
    if let Err(e) = save(dir, &state) {
        return refused(&e);
    }
    let bytes = match read(path, FILE_LIMIT) {
        Ok(b) => b,
        Err(e) => {
            return finish_event(
                dir,
                &mut state,
                "refused",
                json!({"reason":e}),
                Attempt::None,
            );
        }
    };
    let p = match proposed_program(&bytes, expressions, &table) {
        Ok(p) => p,
        Err(e) => {
            return finish_event(
                dir,
                &mut state,
                "candidate-refused",
                json!({"reason":e}),
                Attempt::Spend,
            );
        }
    };
    if p.inputs() != table.inputs() || p.outputs() != table.outputs() {
        return finish_event(
            dir,
            &mut state,
            "candidate-refused",
            json!({"reason":"capture-candidate-abi"}),
            Attempt::Spend,
        );
    }
    let size = table.points().len() as u64;
    let nodes = p.nodes().len() as u64;
    let pre = state.examples.len() as u64 * nodes;
    if pre > state.budget.node_visits {
        return finish_event(
            dir,
            &mut state,
            "inconclusive",
            json!({"reason":"precheck-work-budget"}),
            Attempt::None,
        );
    }
    let seen = match stages.run(true, || proposal::precheck(&table, &p, &state.examples)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            return finish_event(
                dir,
                &mut state,
                "refused",
                json!({"reason":e.0}),
                Attempt::None,
            );
        }
        Err(e) => {
            return finish_event(
                dir,
                &mut state,
                "inconclusive",
                json!({"reason":e}),
                Attempt::None,
            );
        }
    };
    if let Some(i) = seen {
        return finish_event(
            dir,
            &mut state,
            "precheck-refuted",
            json!({"precheck":"precheck-refuted","decided_by":"proposal-example-filter",
        "example":{"input":crate::report_numbers::integers(&table.points()[i]),"output":crate::report_numbers::integers(&table.rows()[i])},"precheck_reserved_visits":pre,"exact":"not-run"}),
            Attempt::Spend,
        );
    }
    // Reserve full check AND replay, plus the later advisory failure scan.
    // A public budget outcome must not reveal the hidden table's node count.
    // Reserve the fixed admitted target capacity, independent of its encoding.
    let exact = 2 * size * (MAX_NODES as u64 + nodes);
    let scan = size * nodes;
    if pre.saturating_add(exact).saturating_add(scan) > state.budget.node_visits {
        return finish_event(
            dir,
            &mut state,
            "inconclusive",
            json!({"reason":"exact-work-budget","precheck":"examples-equivalent","precheck_reserved_visits":pre}),
            Attempt::None,
        );
    }
    let encoded = match encode(&p) {
        Ok(b) => b,
        Err(_) => {
            return finish_event(
                dir,
                &mut state,
                "candidate-refused",
                json!({"reason":"capture-candidate-encoding"}),
                Attempt::Spend,
            );
        }
    };
    let (check, receipt) = match confirm(stages, &original, &encoded) {
        Ok(v) => v,
        Err(e) => {
            return finish_event(
                dir,
                &mut state,
                "inconclusive",
                json!({"reason":e,"precheck":"examples-equivalent"}),
                Attempt::None,
            );
        }
    };
    let mut detail = json!({"precheck":"examples-equivalent","decided_by":"transform-check-and-replay","check":check,
        "precheck_reserved_visits":pre,"exact_reserved_visits":exact,"candidate_sha256":sha256_hex(&encoded)});
    if check["verdict"] == "equivalent" {
        if check["replay"] != "matched" || receipt.is_none() {
            return finish_event(
                dir,
                &mut state,
                "inconclusive",
                json!({"reason":"capture-replay-not-matched"}),
                Attempt::None,
            );
        }
        state.candidate = Some(program_to_json(&p));
        state.receipt = receipt;
        detail["checked"] = json!(size);
        return finish_event(dir, &mut state, "captured-exact", detail, Attempt::Spend);
    }
    if check["verdict"] != "counterexample" {
        return finish_event(
            dir,
            &mut state,
            if check["verdict"] == "inconclusive" {
                "inconclusive"
            } else {
                "refused"
            },
            detail,
            Attempt::None,
        );
    }
    // Charge and persist the decided refutation BEFORE any optional search.
    // A stage kill cannot provide free feedback on retry.
    let decided = finish_event(dir, &mut state, "refuted", detail.clone(), Attempt::Spend);
    if decided.1["status"] != "refuted" {
        return decided;
    }
    let bad = match stages.run(true, || proposal::differences(&table, &p)) {
        Ok(Ok(b)) => b,
        _ => {
            return finish_event(
                dir,
                &mut state,
                "refuted",
                json!({"check":{"verdict":"counterexample"},"feedback":"counterexample-selection-budget"}),
                Attempt::Recorded,
            );
        }
    };
    let ordinal = match proposal::choose_counterexample(
        &table,
        &bad,
        state.examples.last().copied(),
    ) {
        Some(i) => i,
        None => {
            return finish_event(
                dir,
                &mut state,
                "refuted",
                json!({"check":{"verdict":"counterexample"},"feedback":"capture-feedback-defect"}),
                Attempt::Recorded,
            );
        }
    };
    // Always add the actual failing example before coverage can consume slots.
    if !state.examples.contains(&ordinal) {
        if state.examples.len() as u64 >= state.budget.max_examples {
            let captured_packet = packet(&state, &table);
            return finish_event(
                dir,
                &mut state,
                "refuted",
                json!({"feedback":"example-budget","check":{"verdict":"counterexample"},
                    "precheck":"examples-equivalent","packet":captured_packet}),
                Attempt::Recorded,
            );
        }
        state.examples.push(ordinal);
    }
    detail["example"] = json!({"input":crate::report_numbers::integers(&table.points()[ordinal]),"output":crate::report_numbers::integers(&table.rows()[ordinal])});
    // The full checker witness is kept private; its ordinal must not expose an
    // additional hidden input through the public counterexample-selection path.
    detail["check"] = json!({"verdict":"counterexample"});
    let coverage_visits = proposal::coverage_visits(&p, table.points().len());
    if pre + exact + scan + coverage_visits <= state.budget.node_visits {
        match stages.run(true, || proposal::coverage(&table, &p, &state.examples)) {
            Ok(Ok(rows)) => {
                let mut outcomes = Vec::new();
                for row in rows {
                    if let Some(i) = row.example
                        && !row.covered
                        && !state.examples.contains(&i)
                        && (state.examples.len() as u64) < state.budget.max_examples
                    {
                        state.examples.push(i);
                    }
                    outcomes.push(json!({"node":row.node,"outcome":row.outcome,"reachable":row.example.is_some(),
                        "covered":row.covered || row.example.is_some_and(|i|state.examples.contains(&i))}));
                }
                detail["coverage"] = json!({"status":if outcomes.iter().all(|r|r["reachable"]==false || r["covered"]==true){"complete"}else{"inconclusive"},
                    "decided_by":"proposal-condition-coverage","outcomes":outcomes});
            }
            _ => {
                detail["coverage"] =
                    json!({"status":"inconclusive","reason":"coverage-stage-budget"})
            }
        }
    } else {
        detail["coverage"] = json!({"status":"inconclusive","reason":"coverage-work-budget"});
    }
    // Include ONLY newly judge-selected examples, never a full target table.
    detail["packet"] = packet(&state, &table);
    finish_event(dir, &mut state, "refuted", detail, Attempt::Recorded)
}

fn status_loaded(
    state: &Session,
    table: &proposal::ReferenceTable,
    original: &[u8],
    stages: &mut super::Stages,
    full: bool,
) -> (u8, Value) {
    if let Some(fixture) = &state.candidate {
        let candidate = match program_from_json(fixture).and_then(|p| encode(&p)) {
            Ok(b) => b,
            Err(_) => return refused("capture-stored-candidate"),
        };
        let (check, receipt) = match confirm(stages, original, &candidate) {
            Ok(v) => v,
            Err(e) => return inconclusive(&e),
        };
        if check["verdict"] == "inconclusive" {
            return inconclusive("capture-stored-check-budget");
        }
        if check["verdict"] != "equivalent"
            || check["replay"] != "matched"
            || receipt != state.receipt
        {
            return refused("capture-stored-receipt");
        }
    }
    let detail = if full {
        json!({"session":session_report(&state),
            "table_program":program_to_json(&match table.program(){Ok(p)=>p,Err(_)=>return refused("capture-reference-encoding")})})
    } else {
        json!({"source_sha256":state.source_sha256,"table_sha256":state.table_sha256,
            "program_sha256":state.program_sha256,"checker":state.checker,"budget":state.budget,
            "attempts":state.attempts,"invocations":state.invocations,"reserved_visits":state.reserved_visits,
            "reference_observations":state.reference_observations,"tuples":table.points().len(),
            "examples":state.examples.len(),"events":state.events.len()})
    };
    let (code, mut result) = report(
        if state.candidate.is_some() {
            "captured-exact"
        } else {
            &state.status
        },
        detail,
    );
    result["detail"]["assumption"] =
        json!("reference is a pure deterministic function of all declared inputs");
    result["detail"]["exact_subject"] = json!("complete frozen observed table");
    if serde_json::to_vec_pretty(&result).map_or(true, |bytes| bytes.len() + 1 > 1024 * 1024) {
        return refused("capture-full-status-output-bound");
    }
    (code, result)
}

pub(super) fn run(command: Command) -> (u8, Value) {
    let mut stages = super::Stages::new(Instant::now(), super::STATELESS_DEADLINE, 0);
    match command {
        Command::Whole { options } => whole::run(options, &mut stages),
        Command::Open {
            spec,
            reference,
            reference_format,
            session,
            max_tuples,
            rounds,
            max_examples,
            node_visits,
        } => open(
            &spec,
            &reference,
            &reference_format,
            &session,
            Budget {
                max_tuples,
                rounds,
                max_examples,
                node_visits,
            },
            &mut stages,
        ),
        Command::Candidate {
            session,
            candidate_json,
            expressions,
        } => match candidate_json.or_else(|| expressions.clone()) {
            Some(p) => candidate(&session, &p, expressions.is_some(), &mut stages),
            None => refused("capture-candidate-required"),
        },
        Command::Packet { session } => match load(&session) {
            Ok((s, t, _)) => report("packet", json!({"packet":packet(&s,&t)})),
            Err(e) => refused(&e),
        },
        Command::Status { session, full } => match load(&session) {
            Ok((s, t, b)) => status_loaded(&s, &t, &b, &mut stages, full),
            Err(e) => refused(&e),
        },
    }
}

#[cfg(test)]
#[path = "loop_capture_tests.rs"]
mod tests;
