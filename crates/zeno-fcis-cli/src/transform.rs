//! Pure checker for `zeno-fcis transform`: exhaustive comparison of two
//! canonical finite scalar programs, with a canonical equivalence receipt.
//!
//! It lives in the CLI crate because the library's evaluator digest covers the
//! synthesis crate, the root Cargo.toml and Cargo.lock.
//!
//! No I/O, clock or ambient state. Both programs must pass the library
//! importer's complete admission and share the exact ordered input and output
//! ABI. Each tuple of the declared input domain runs through the library's
//! metered evaluator (`finite::execute_v2`) at the full budget of `MAX_NODES`
//! Steps, which no admitted program can exhaust, so unused nodes and unselected
//! arms still trap and each run yields the program's result and its true Step
//! usage. The declared Step limit then only decides whether it would ever bind.
//! Only a complete enumeration constructs an [`Equivalence`]; its receipt grants
//! no application or publication authority.

use serde_json::{Value, json};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    Domain, Error, MAX_NODES, PROFILE, Program, V2_EXECUTION_PROFILE, V2ExecutionFailure, V2Limits,
    V2Resource, execute_v2, v2_authority::EVALUATOR, v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

/// Version of the receipt format.
pub(crate) const RECEIPT_SCHEMA: &str = "zeno-fcis/transform-receipt/1";
/// Lexicographic product order: the last input advances fastest, Bool takes
/// 0 then 1, and integers increase.
pub(crate) const ENUMERATION: &str = "ordered-product-last-input-fastest-v1";
/// Each instruction attempt costs one Step and an admitted program has at most
/// `MAX_NODES` nodes, so no admitted program is refused at this budget.
pub(crate) const FULL_BUDGET: u64 = MAX_NODES as u64;
/// The default declared Step limit; it never binds.
pub(crate) const DEFAULT_STEP_LIMIT: u64 = FULL_BUDGET;
/// Default ceiling on the number of input tuples one check may enumerate.
pub(crate) const DEFAULT_MAX_INPUT_TUPLES: u64 = 100_000_000;
/// The receipt binds these exact bytes as the checker's source identity.
const SOURCE: &[u8] = include_bytes!("transform.rs");
const SOURCE_PATH: &str = "crates/zeno-fcis-cli/src/transform.rs";

/// Limits that fix the meaning of one check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    /// Declared Step limit that every evaluation must fit within.
    pub(crate) steps: u64,
    /// Largest input domain, in tuples, that the check may enumerate.
    pub(crate) input_tuples: u64,
}

/// Which supplied program an observation or refusal concerns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    Original,
    Candidate,
}

/// One program's full-budget result for one input tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Observation {
    /// Complete output tuple, or the evaluator's exact failure.
    pub(crate) result: Result<Vec<i64>, V2ExecutionFailure>,
    /// True Step usage: instruction attempts, including a trapping one.
    pub(crate) steps: u64,
}

/// Step usage over the whole domain, from the full-budget runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Usage {
    /// Largest Step usage on any tuple: original, then candidate.
    pub(crate) max_steps: [u64; 2],
    /// Tuples on which the candidate uses more Steps than the original.
    pub(crate) candidate_uses_more: u64,
    /// Whether the two programs use equal Steps on every tuple.
    pub(crate) usage_preserved: bool,
}

/// Why a check produced no equivalence.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Rejection {
    Counterexample(Counterexample),
    Inconclusive(Inconclusive),
    Refused(Refusal),
}

/// The first input tuple, in enumeration order, whose results differ.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Counterexample {
    pub(crate) ordinal: u64,
    pub(crate) input: Vec<i64>,
    pub(crate) original: Observation,
    pub(crate) candidate: Observation,
}

/// The check stopped without deciding equivalence.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Inconclusive {
    /// The domain has more tuples than the limit; `size` is `None` when the
    /// product exceeds `u128::MAX`.
    DomainTooLarge { size: Option<u128>, limit: u64 },
    /// The programs are functionally equal, but some tuple needs more Steps
    /// than the declared limit. `over_limit` counts those tuples for the
    /// original, then the candidate; `minimum_limit` is the smallest limit
    /// that binds on no tuple.
    BudgetBoundary {
        over_limit: [u64; 2],
        minimum_limit: u64,
        usage: Usage,
    },
    /// Defensive terminal check: the odometer did not stop exactly after the
    /// computed number of tuples. `visited` counts the tuples it produced.
    CoverageMismatch { expected: u64, visited: u64 },
}

/// The pair cannot be compared under this check's contract.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Refusal {
    /// The library importer refused the canonical bytes; `code` is its diagnostic.
    NotAdmitted { side: Side, code: &'static str },
    /// Input count, order or domains differ.
    InputAbi {
        original: Vec<Domain>,
        candidate: Vec<Domain>,
    },
    /// Output count, order or domains differ.
    OutputAbi {
        original: Vec<Domain>,
        candidate: Vec<Domain>,
    },
    /// An input domain has no values. Admission already refuses inverted
    /// intervals; this keeps the product from being silently empty.
    EmptyInputDomain { position: usize },
}

/// A complete equivalence. Only [`check`] constructs one, so a receipt always
/// describes a finished enumeration whose limit never binds.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Equivalence {
    original: Artifact,
    candidate: Artifact,
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    limits: Limits,
    inputs_checked: u64,
    usage: Usage,
}

#[derive(Debug, Eq, PartialEq)]
struct Artifact {
    sha256: String,
    bytes: usize,
    nodes: usize,
}

impl Artifact {
    fn new(bytes: &[u8], program: &Program) -> Self {
        Self {
            sha256: sha256_hex(bytes),
            bytes: bytes.len(),
            nodes: program.nodes().len(),
        }
    }

    fn json(&self) -> Value {
        json!({"sha256": self.sha256, "bytes": self.bytes, "nodes": self.nodes})
    }
}

/// Both admitted programs and the size of their shared input domain.
struct Pair {
    original: Program,
    candidate: Program,
    size: u64,
}

/// Admits both canonical programs, then compares them on every input tuple.
///
/// Refusal order: original admission, candidate admission, input ABI, output
/// ABI, empty domain; then a domain above `limits.input_tuples` is
/// inconclusive. Otherwise the first tuple whose full-budget results differ is
/// the counterexample. With equal results everywhere, a tuple whose true Step
/// usage exceeds `limits.steps` in either program makes the check inconclusive;
/// if there is none, the programs are equivalent.
pub(crate) fn check(
    original: &[u8],
    candidate: &[u8],
    limits: Limits,
) -> Result<Equivalence, Rejection> {
    let pair = admit(original, candidate, limits.input_tuples)?;
    let meter = v2_zero_limits().with_limit(V2Resource::Step, FULL_BUDGET);
    let domains = pair.original.inputs();
    let mut tally = Tally::default();
    let mut input = first_tuple(domains);
    // The odometer drives the enumeration. The separately computed product
    // bounds it, and only an exact match of the two completes it.
    let mut visited = 0_u64;
    loop {
        if visited == pair.size {
            return Err(Rejection::Inconclusive(Inconclusive::CoverageMismatch {
                expected: pair.size,
                visited: visited.saturating_add(1),
            }));
        }
        let left = observe(&pair.original, &input, meter);
        let right = observe(&pair.candidate, &input, meter);
        if left.result != right.result {
            return Err(Rejection::Counterexample(Counterexample {
                ordinal: visited,
                input,
                original: left,
                candidate: right,
            }));
        }
        tally.record(left.steps, right.steps, limits.steps);
        visited += 1;
        if !advance(domains, &mut input) {
            break;
        }
    }
    if visited != pair.size {
        return Err(Rejection::Inconclusive(Inconclusive::CoverageMismatch {
            expected: pair.size,
            visited,
        }));
    }
    let usage = tally.usage();
    if tally.over_limit != [0, 0] {
        return Err(Rejection::Inconclusive(Inconclusive::BudgetBoundary {
            over_limit: tally.over_limit,
            minimum_limit: usage.max_steps[0].max(usage.max_steps[1]),
            usage,
        }));
    }
    Ok(Equivalence {
        original: Artifact::new(original, &pair.original),
        candidate: Artifact::new(candidate, &pair.candidate),
        inputs: domains.to_vec(),
        outputs: pair.original.outputs().to_vec(),
        limits,
        inputs_checked: visited,
        usage,
    })
}

/// Admission, ABI and domain checks, in refusal order, before any evaluation.
fn admit(original: &[u8], candidate: &[u8], input_tuples: u64) -> Result<Pair, Rejection> {
    let original =
        import_program(original).map_err(|error| not_admitted(Side::Original, &error))?;
    let candidate =
        import_program(candidate).map_err(|error| not_admitted(Side::Candidate, &error))?;
    if original.inputs() != candidate.inputs() {
        return Err(Rejection::Refused(Refusal::InputAbi {
            original: original.inputs().to_vec(),
            candidate: candidate.inputs().to_vec(),
        }));
    }
    if original.outputs() != candidate.outputs() {
        return Err(Rejection::Refused(Refusal::OutputAbi {
            original: original.outputs().to_vec(),
            candidate: candidate.outputs().to_vec(),
        }));
    }
    let size = domain_size(original.inputs())
        .map_err(|position| Rejection::Refused(Refusal::EmptyInputDomain { position }))?;
    match size
        .and_then(|size| u64::try_from(size).ok())
        .filter(|size| *size <= input_tuples)
    {
        Some(size) => Ok(Pair {
            original,
            candidate,
            size,
        }),
        None => Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge {
            size,
            limit: input_tuples,
        })),
    }
}

fn not_admitted(side: Side, error: &Error) -> Rejection {
    let code = match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    };
    Rejection::Refused(Refusal::NotAdmitted { side, code })
}

/// Exact number of tuples in the product of `domains`; `None` when it exceeds
/// `u128::MAX`. `Err` names the first empty domain. Zero domains give one
/// empty tuple.
pub(crate) fn domain_size(domains: &[Domain]) -> Result<Option<u128>, usize> {
    let mut size = Some(1_u128);
    for (position, domain) in domains.iter().enumerate() {
        let (min, max) = domain.bounds();
        if min > max {
            return Err(position);
        }
        // `max - min` fits in u64, so the width is at most 2^64.
        let width = u128::from(max.abs_diff(min)) + 1;
        size = size.and_then(|size| size.checked_mul(width));
    }
    Ok(size)
}

/// The first tuple in enumeration order: every input at its minimum.
pub(crate) fn first_tuple(domains: &[Domain]) -> Vec<i64> {
    domains.iter().map(|domain| domain.bounds().0).collect()
}

/// Steps `tuple` to its successor, last input fastest. Returns false after the
/// last tuple, leaving the first one in place. An input is incremented only
/// while it is below its maximum, so no value overflows at an `i64` endpoint.
pub(crate) fn advance(domains: &[Domain], tuple: &mut [i64]) -> bool {
    for (value, domain) in tuple.iter_mut().zip(domains).rev() {
        let (min, max) = domain.bounds();
        if *value < max {
            *value += 1;
            return true;
        }
        *value = min;
    }
    false
}

fn observe(program: &Program, input: &[i64], meter: V2Limits) -> Observation {
    let (result, usage) = execute_v2(
        program.inputs(),
        program.outputs(),
        program.nodes(),
        program.roots(),
        input,
        meter,
    )
    .into_parts();
    Observation {
        result,
        steps: usage.used(V2Resource::Step),
    }
}

/// Step accounting over tuples whose results already matched.
#[derive(Default)]
struct Tally {
    max_steps: [u64; 2],
    over_limit: [u64; 2],
    candidate_uses_more: u64,
    usage_differs: bool,
}

impl Tally {
    fn record(&mut self, original: u64, candidate: u64, limit: u64) {
        for (side, steps) in [original, candidate].into_iter().enumerate() {
            self.max_steps[side] = self.max_steps[side].max(steps);
            if steps > limit {
                self.over_limit[side] += 1;
            }
        }
        if candidate > original {
            self.candidate_uses_more += 1;
        }
        self.usage_differs |= candidate != original;
    }

    fn usage(&self) -> Usage {
        Usage {
            max_steps: self.max_steps,
            candidate_uses_more: self.candidate_uses_more,
            usage_preserved: !self.usage_differs,
        }
    }
}

impl Equivalence {
    /// Canonical receipt bytes: compact JSON with object keys in byte order,
    /// followed by one newline.
    pub(crate) fn receipt(&self) -> Vec<u8> {
        let mut text = String::new();
        write_canonical(&self.receipt_value(), &mut text);
        text.push('\n');
        text.into_bytes()
    }

    /// The receipt's fields. Every value is computed by this check.
    pub(crate) fn receipt_value(&self) -> Value {
        json!({
            "schema": RECEIPT_SCHEMA,
            "verdict": "equivalent",
            "authority": "none",
            "claim": "Functionally equal on the full declared input domain: every tuple gives identical outputs or identical failures. The Step limit never binds: no tuple needs more Steps than the limit in either program.",
            "profile": {"program": PROFILE, "execution": V2_EXECUTION_PROFILE},
            "original": self.original.json(),
            "candidate": self.candidate.json(),
            "domain": {
                "enumeration": ENUMERATION,
                "inputs": domains_json(&self.inputs),
                "size": self.inputs_checked,
            },
            "outputs": domains_json(&self.outputs),
            "limits": {"steps": self.limits.steps, "input_tuples": self.limits.input_tuples},
            "inputs_checked": self.inputs_checked,
            "usage": usage_json(&self.usage),
            "checker": {
                "crate": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION"),
                "source": SOURCE_PATH,
                "source_sha256": sha256_hex(SOURCE),
                "evaluator_identity": hex(&EVALUATOR),
            },
        })
    }
}

/// The result of recomputing a receipt from both programs.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Replay {
    /// The recomputed receipt equals the supplied bytes.
    Matched,
    /// The bytes are not a receipt of this schema with readable bindings.
    Unreadable,
    /// The domain exceeds the verifier's own tuple cap; nothing was evaluated.
    OverCap { size: Option<u128>, cap: u64 },
    /// Rerunning the check with the receipt's limits gives no equivalence.
    NotEquivalent(Rejection),
    /// The supplied programs or the recomputed receipt differ from the record.
    /// Names the differing top-level fields; empty when only the encoding
    /// differs.
    Differs(Vec<String>),
}

/// Checks the receipt's program digests and domain size against the supplied
/// files, and the domain against `max_input_tuples`, before any evaluation.
/// Then reruns the check with the receipt's limits and compares the rebuilt
/// receipt with the supplied bytes exactly.
pub(crate) fn replay(
    receipt: &[u8],
    original: &[u8],
    candidate: &[u8],
    max_input_tuples: u64,
) -> Replay {
    let Some(recorded) = read_receipt(receipt) else {
        return Replay::Unreadable;
    };
    let mut fields = Vec::new();
    if sha256_hex(original) != recorded.original_sha256 {
        fields.push(String::from("original"));
    }
    if sha256_hex(candidate) != recorded.candidate_sha256 {
        fields.push(String::from("candidate"));
    }
    if !fields.is_empty() {
        return Replay::Differs(fields);
    }
    // The verifier's cap bounds the work; the receipt cannot raise it.
    match admit(original, candidate, max_input_tuples) {
        Ok(pair) if pair.size == recorded.domain_size => {}
        Ok(_) => return Replay::Differs(vec![String::from("domain")]),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge { size, limit })) => {
            return Replay::OverCap { size, cap: limit };
        }
        Err(rejection) => return Replay::NotEquivalent(rejection),
    }
    match check(original, candidate, recorded.limits) {
        Ok(equivalence) if equivalence.receipt() == receipt => Replay::Matched,
        Ok(equivalence) => Replay::Differs(differing_fields(
            &recorded.value,
            &equivalence.receipt_value(),
        )),
        Err(rejection) => Replay::NotEquivalent(rejection),
    }
}

/// The receipt fields replay binds before evaluating anything.
struct Recorded {
    value: Value,
    original_sha256: String,
    candidate_sha256: String,
    domain_size: u64,
    limits: Limits,
}

fn read_receipt(bytes: &[u8]) -> Option<Recorded> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value.get("schema")?.as_str()? != RECEIPT_SCHEMA {
        return None;
    }
    let digest = |side: &str| Some(value.get(side)?.get("sha256")?.as_str()?.to_owned());
    let original_sha256 = digest("original")?;
    let candidate_sha256 = digest("candidate")?;
    let domain_size = value.get("domain")?.get("size")?.as_u64()?;
    let limits = value.get("limits")?;
    let limits = Limits {
        steps: limits.get("steps")?.as_u64()?,
        input_tuples: limits.get("input_tuples")?.as_u64()?,
    };
    Some(Recorded {
        value,
        original_sha256,
        candidate_sha256,
        domain_size,
        limits,
    })
}

fn differing_fields(recorded: &Value, computed: &Value) -> Vec<String> {
    let (Value::Object(recorded), Value::Object(computed)) = (recorded, computed) else {
        return vec![String::from("receipt")];
    };
    let mut names: Vec<&String> = recorded.keys().chain(computed.keys()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|name| recorded.get(*name) != computed.get(*name))
        .cloned()
        .collect()
}

/// Scalar values as exact decimal strings, which JSON readers cannot round.
pub(crate) fn values_json(values: &[i64]) -> Value {
    Value::from(
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
}

/// Domains in the synthesis problem vocabulary, with decimal-string bounds.
pub(crate) fn domains_json(domains: &[Domain]) -> Value {
    Value::from(
        domains
            .iter()
            .map(|domain| match *domain {
                Domain::Bool => json!({"kind": "bool"}),
                Domain::Int { min, max } => {
                    json!({"kind": "int", "min": min.to_string(), "max": max.to_string()})
                }
                _ => {
                    let (min, max) = domain.bounds();
                    json!({"kind": "unclassified", "min": min.to_string(), "max": max.to_string()})
                }
            })
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn usage_json(usage: &Usage) -> Value {
    json!({
        "max_steps": {"original": usage.max_steps[0], "candidate": usage.max_steps[1]},
        "candidate_uses_more": usage.candidate_uses_more,
        "usage_preserved": usage.usage_preserved,
    })
}

/// The library's failure names, as used by the benchmark fixtures.
pub(crate) fn failure_tag(failure: V2ExecutionFailure) -> &'static str {
    match failure {
        V2ExecutionFailure::InputDomain => "InputDomain",
        V2ExecutionFailure::Reference => "Reference",
        V2ExecutionFailure::Arithmetic => "Arithmetic",
        V2ExecutionFailure::OutputDomain => "OutputDomain",
        V2ExecutionFailure::Budget(_) => "Budget",
        _ => "Unclassified",
    }
}

/// Compact JSON with object keys sorted by their bytes, followed by one
/// newline: the form of every receipt and packet this crate writes.
pub(crate) fn canonical_json(value: &Value) -> String {
    let mut text = String::new();
    write_canonical(value, &mut text);
    text.push('\n');
    text
}

/// Compact JSON with object keys sorted by their bytes, independent of the
/// map order `serde_json` was built with. The neural loop's request, ledger
/// and witness records use the same canonical form.
pub(crate) fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::from(key.as_str()).to_string());
                out.push(':');
                write_canonical(item, out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(RustCryptoSha256::hash(bytes).as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
