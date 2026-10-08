//! Pure checker for `zeno-fcis transform`: exhaustive comparison of two
//! canonical finite scalar programs, with a canonical equivalence receipt.
//!
//! This CLI adapter owns admission and receipts. Its finite comparison core is
//! the same source used by SQLite upgrades; no evaluator identity source changes.
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
    Domain, Error, PROFILE, Program, V2_EXECUTION_PROFILE, V2ExecutionFailure, V2ScalarProgram,
    v2_authority::EVALUATOR,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

/// Version of the receipt format.
pub(crate) const RECEIPT_SCHEMA: &str = "zeno-fcis/transform-receipt/1";
/// Lexicographic product order: the last input advances fastest, Bool takes
/// 0 then 1, and integers increase.
pub(crate) const ENUMERATION: &str = "ordered-product-last-input-fastest-v1";
/// Each instruction attempt costs one Step and an admitted program has at most
/// `MAX_NODES` nodes, so no admitted program is refused at this budget.
pub(crate) use crate::finite_checker::FULL_BUDGET;
/// The default declared Step limit; it never binds.
pub(crate) const DEFAULT_STEP_LIMIT: u64 = FULL_BUDGET;
/// Default ceiling on the number of input tuples one check may enumerate.
pub(crate) const DEFAULT_MAX_INPUT_TUPLES: u64 = 100_000_000;
/// The checker's semantics version, which every receipt binds with the
/// library's evaluator identity. It names what a verdict means: admission
/// and ABI checks, the enumeration, full-budget evaluation, Step-limit and
/// usage accounting, and the receipt's fields. A change that could alter any
/// receipt or rejection needs a new version and new known answers under
/// `tests/fixtures/transform-check/`, which the tests compare with this
/// checker's output; refactoring the source, or a new crate version, changes
/// no receipt.
pub(crate) const CHECKER: &str = "zeno-fcis/transform-check/1";

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

pub(crate) use crate::finite_checker::{Observation, Usage, advance, domain_size, first_tuple};

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Equivalence {
    original: Artifact,
    candidate: Artifact,
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    limits: Limits,
    inputs_checked: u64,
    usage: Usage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    let left = scalar(&pair.original);
    let right = scalar(&pair.candidate);
    let complete =
        crate::finite_checker::compare_with_usage(&left, &right, limits.input_tuples, limits.steps)
            .map_err(|error| comparison_rejection(error, &pair.original, &pair.candidate))?;
    let domains = pair.original.inputs();
    Ok(Equivalence {
        original: Artifact::new(original, &pair.original),
        candidate: Artifact::new(candidate, &pair.candidate),
        inputs: domains.to_vec(),
        outputs: pair.original.outputs().to_vec(),
        limits,
        inputs_checked: complete.0,
        usage: complete.1,
    })
}

/// Admission, ABI and domain checks, in refusal order, before any evaluation.
fn admit(original: &[u8], candidate: &[u8], input_tuples: u64) -> Result<Pair, Rejection> {
    let original =
        import_program(original).map_err(|error| not_admitted(Side::Original, &error))?;
    let candidate =
        import_program(candidate).map_err(|error| not_admitted(Side::Candidate, &error))?;
    let size =
        crate::finite_checker::validate_pair(&scalar(&original), &scalar(&candidate), input_tuples)
            .map_err(|error| comparison_rejection(error, &original, &candidate))?;
    Ok(Pair {
        original,
        candidate,
        size,
    })
}

fn scalar(program: &Program) -> V2ScalarProgram<'_> {
    V2ScalarProgram {
        inputs: program.inputs(),
        outputs: program.outputs(),
        nodes: program.nodes(),
        roots: program.roots(),
    }
}

fn comparison_rejection(
    error: crate::finite_checker::Failure,
    original: &Program,
    candidate: &Program,
) -> Rejection {
    use crate::finite_checker::Failure;
    match error {
        Failure::InputAbi => Rejection::Refused(Refusal::InputAbi {
            original: original.inputs().to_vec(),
            candidate: candidate.inputs().to_vec(),
        }),
        Failure::OutputAbi => Rejection::Refused(Refusal::OutputAbi {
            original: original.outputs().to_vec(),
            candidate: candidate.outputs().to_vec(),
        }),
        Failure::EmptyInputDomain { position } => {
            Rejection::Refused(Refusal::EmptyInputDomain { position })
        }
        Failure::DomainTooLarge { size, limit } => {
            Rejection::Inconclusive(Inconclusive::DomainTooLarge { size, limit })
        }
        Failure::Counterexample {
            ordinal,
            input,
            original,
            candidate,
        } => Rejection::Counterexample(Counterexample {
            ordinal,
            input,
            original,
            candidate,
        }),
        Failure::CoverageMismatch { expected, visited } => {
            Rejection::Inconclusive(Inconclusive::CoverageMismatch { expected, visited })
        }
        Failure::BudgetBoundary {
            over_limit,
            minimum_limit,
            usage,
        } => Rejection::Inconclusive(Inconclusive::BudgetBoundary {
            over_limit,
            minimum_limit,
            usage,
        }),
    }
}

fn not_admitted(side: Side, error: &Error) -> Rejection {
    let code = match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    };
    Rejection::Refused(Refusal::NotAdmitted { side, code })
}

impl Equivalence {
    /// The true Step usage of both programs over the whole domain.
    pub(crate) fn usage(&self) -> Usage {
        self.usage
    }

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
                "semantics": CHECKER,
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
    match replayed(receipt, original, candidate, max_input_tuples) {
        Ok(_) => Replay::Matched,
        Err(refused) => refused,
    }
}

/// A receipt that replayed: the exact receipt, original and candidate bytes,
/// the verifier's tuple cap, and the equivalence the check recomputed. Only
/// [`replayed`] constructs one, so a value shows that replay matched and can
/// stand for it again over the same bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Replayed {
    receipt: Vec<u8>,
    original: Vec<u8>,
    candidate: Vec<u8>,
    max_input_tuples: u64,
    equivalence: Equivalence,
}

impl Replayed {
    /// Whether this is the replay of exactly these bytes under this cap.
    pub(crate) fn is_of(
        &self,
        receipt: &[u8],
        original: &[u8],
        candidate: &[u8],
        max_input_tuples: u64,
    ) -> bool {
        self.receipt == receipt
            && self.original == original
            && self.candidate == candidate
            && self.max_input_tuples == max_input_tuples
    }

    /// The replayed receipt's bytes.
    pub(crate) fn receipt(&self) -> &[u8] {
        &self.receipt
    }

    /// The equivalence the replay recomputed.
    pub(crate) fn equivalence(&self) -> &Equivalence {
        &self.equivalence
    }
}

/// [`replay`], keeping the evidence of a match. `Err` is never
/// `Replay::Matched`.
pub(crate) fn replayed(
    receipt: &[u8],
    original: &[u8],
    candidate: &[u8],
    max_input_tuples: u64,
) -> Result<Replayed, Replay> {
    let (recorded, equivalence) = rerun(receipt, original, candidate, max_input_tuples)?;
    if equivalence.receipt() != receipt {
        return Err(Replay::Differs(differing_fields(
            &recorded.value,
            &equivalence.receipt_value(),
        )));
    }
    Ok(Replayed {
        receipt: receipt.to_vec(),
        original: original.to_vec(),
        candidate: candidate.to_vec(),
        max_input_tuples,
        equivalence,
    })
}

/// Rechecks the pair a receipt names, under the receipt's limits, and returns
/// the replay witness of the receipt this checker writes for it. Refused,
/// like a replay, unless every field but `checker` is the one the old
/// receipt records: a refresh only rebinds a receipt to this checker's
/// identity and never changes a verdict, a usage report or what was checked.
/// Nothing of the old receipt is trusted beyond its bindings and limits.
pub(crate) fn refreshed(
    receipt: &[u8],
    original: &[u8],
    candidate: &[u8],
    max_input_tuples: u64,
) -> Result<Replayed, Replay> {
    let (recorded, equivalence) = rerun(receipt, original, candidate, max_input_tuples)?;
    let fields: Vec<String> = differing_fields(&recorded.value, &equivalence.receipt_value())
        .into_iter()
        .filter(|field| field != "checker")
        .collect();
    if !fields.is_empty() {
        return Err(Replay::Differs(fields));
    }
    Ok(Replayed {
        receipt: equivalence.receipt(),
        original: original.to_vec(),
        candidate: candidate.to_vec(),
        max_input_tuples,
        equivalence,
    })
}

/// Reads a receipt's bindings, checks them against both programs and the
/// cap before any evaluation, and reruns the check with its limits.
fn rerun(
    receipt: &[u8],
    original: &[u8],
    candidate: &[u8],
    max_input_tuples: u64,
) -> Result<(Recorded, Equivalence), Replay> {
    let Some(recorded) = read_receipt(receipt) else {
        return Err(Replay::Unreadable);
    };
    let mut fields = Vec::new();
    if sha256_hex(original) != recorded.original_sha256 {
        fields.push(String::from("original"));
    }
    if sha256_hex(candidate) != recorded.candidate_sha256 {
        fields.push(String::from("candidate"));
    }
    if !fields.is_empty() {
        return Err(Replay::Differs(fields));
    }
    // The verifier's cap bounds the work; the receipt cannot raise it.
    match admit(original, candidate, max_input_tuples) {
        Ok(pair) if pair.size == recorded.domain_size => {}
        Ok(_) => return Err(Replay::Differs(vec![String::from("domain")])),
        Err(Rejection::Inconclusive(Inconclusive::DomainTooLarge { size, limit })) => {
            return Err(Replay::OverCap { size, cap: limit });
        }
        Err(rejection) => return Err(Replay::NotEquivalent(rejection)),
    }
    match check(original, candidate, recorded.limits) {
        Ok(equivalence) => Ok((recorded, equivalence)),
        Err(rejection) => Err(Replay::NotEquivalent(rejection)),
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
pub(crate) mod tests;
