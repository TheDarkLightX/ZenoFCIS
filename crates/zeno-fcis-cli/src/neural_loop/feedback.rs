//! Replayable difference witnesses and typed feedback (NSF-004, NSF-005,
//! NSR-008).
//!
//! A witness binds the request, both artifacts, the ordinal, the tuple and
//! both full typed observations. Before the shell labels one factual, or
//! reuses a stored one after resume, [`replay_witness`] recomputes the tuple
//! from the ordinal through the versioned enumeration and re-executes both
//! programs from their actual bytes. Incomplete checking never yields a
//! counterexample; a model's explanation is never part of feedback.

use super::incumbent::{Cost, Selection};
use super::program_json::typed_values;
use super::request::Request;
use crate::transform::{Counterexample, FULL_BUDGET, Observation, failure_tag, sha256_hex};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    Domain, Program, V2ExecutionFailure, V2Resource, v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

pub(crate) const WITNESS_SCHEMA: &str = "zeno-fcis/transform-witness/1";

/// The first differing tuple of one check, bound to its request and artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Witness {
    pub(crate) request_id: String,
    pub(crate) original_sha256: String,
    pub(crate) candidate_sha256: String,
    pub(crate) ordinal: u64,
    pub(crate) input: Vec<i64>,
    pub(crate) original: Observation,
    pub(crate) candidate: Observation,
}

/// Why a witness is not a reproduced fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WitnessFault {
    /// Names another request.
    Request,
    /// Names another original.
    Original,
    /// Names other candidate bytes than the ones supplied.
    Candidate,
    /// The supplied candidate bytes no longer admit.
    CandidateNotAdmitted,
    /// The ordinal is outside the domain.
    Ordinal,
    /// The recorded tuple is not the ordinal's tuple in the enumeration.
    Tuple,
    /// Re-execution gives other observations than recorded.
    Observations,
    /// Both programs agree on the tuple: no difference to witness.
    NoDifference,
}

impl Witness {
    /// Binds a fresh counterexample from F3 to this request and candidate.
    pub(crate) fn bind(request: &Request, candidate: &[u8], found: Counterexample) -> Witness {
        Witness {
            request_id: request.id().to_owned(),
            original_sha256: request.original_sha256().to_owned(),
            candidate_sha256: sha256_hex(candidate),
            ordinal: found.ordinal,
            input: found.input,
            original: found.original,
            candidate: found.candidate,
        }
    }

    pub(crate) fn json(&self, inputs: &[Domain], outputs: &[Domain]) -> Value {
        json!({
            "schema": WITNESS_SCHEMA,
            "request_id": self.request_id,
            "original": {"sha256": self.original_sha256},
            "candidate": {"sha256": self.candidate_sha256},
            "ordinal": self.ordinal,
            "input": typed_values(inputs, &self.input),
            "observations": {
                "original": observation_json(outputs, &self.original),
                "candidate": observation_json(outputs, &self.candidate),
            },
        })
    }

    /// Strict reader for a stored witness; the domains decode the typed values.
    pub(crate) fn from_json(
        value: &Value,
        inputs: &[Domain],
        outputs: &[Domain],
    ) -> Option<Witness> {
        if !super::only_fields(
            value,
            &[
                "schema",
                "request_id",
                "original",
                "candidate",
                "ordinal",
                "input",
                "observations",
            ],
        ) || value.get("schema")?.as_str()? != WITNESS_SCHEMA
        {
            return None;
        }
        let digest = |side: &str| {
            let side = value.get(side)?;
            if !super::only_fields(side, &["sha256"]) {
                return None;
            }
            Some(side.get("sha256")?.as_str()?.to_owned())
        };
        let observations = value.get("observations")?;
        if !super::only_fields(observations, &["original", "candidate"]) {
            return None;
        }
        Some(Witness {
            request_id: value.get("request_id")?.as_str()?.to_owned(),
            original_sha256: digest("original")?,
            candidate_sha256: digest("candidate")?,
            ordinal: super::json_u64(value, "ordinal")?,
            input: values_from_json(inputs, value.get("input")?)?,
            original: observation_from_json(outputs, observations.get("original")?)?,
            candidate: observation_from_json(outputs, observations.get("candidate")?)?,
        })
    }
}

fn observation_json(outputs: &[Domain], observation: &Observation) -> Value {
    match &observation.result {
        Ok(values) => json!({"ok": typed_values(outputs, values), "steps": observation.steps}),
        Err(failure) => json!({"error": failure_tag(*failure), "steps": observation.steps}),
    }
}

fn observation_from_json(outputs: &[Domain], value: &Value) -> Option<Observation> {
    let steps = super::json_u64(value, "steps")?;
    if super::only_fields(value, &["ok", "steps"]) {
        return Some(Observation {
            result: Ok(values_from_json(outputs, value.get("ok")?)?),
            steps,
        });
    }
    if !super::only_fields(value, &["error", "steps"]) {
        return None;
    }
    let failure = match value.get("error")?.as_str()? {
        "InputDomain" => V2ExecutionFailure::InputDomain,
        "Reference" => V2ExecutionFailure::Reference,
        "Arithmetic" => V2ExecutionFailure::Arithmetic,
        "OutputDomain" => V2ExecutionFailure::OutputDomain,
        // A full-budget run never refuses on budget, so no witness records one.
        _ => return None,
    };
    Some(Observation {
        result: Err(failure),
        steps,
    })
}

/// Typed fixture values back to wire scalars, checked against their domains.
pub(crate) fn values_from_json(domains: &[Domain], value: &Value) -> Option<Vec<i64>> {
    let items = value.as_array()?;
    if items.len() != domains.len() {
        return None;
    }
    items
        .iter()
        .zip(domains)
        .map(|(item, domain)| {
            let value = match (domain, item) {
                (Domain::Bool, Value::Bool(flag)) => i64::from(*flag),
                (Domain::Bool, _) => return None,
                (_, Value::String(text)) => text.parse().ok()?,
                _ => return None,
            };
            domain.contains(value).then_some(value)
        })
        .collect()
}

/// The ordinal's tuple under `ordered-product-last-input-fastest-v1`,
/// computed by mixed-radix decoding, independently of the checker's odometer.
pub(crate) fn tuple_of_ordinal(domains: &[Domain], ordinal: u64) -> Option<Vec<i64>> {
    let mut remaining = u128::from(ordinal);
    let mut tuple = vec![0_i64; domains.len()];
    for (slot, domain) in tuple.iter_mut().zip(domains).rev() {
        let (min, max) = domain.bounds();
        let width = u128::from(max.abs_diff(min)) + 1;
        let offset = i128::try_from(remaining % width).ok()?;
        remaining /= width;
        *slot = i64::try_from(i128::from(min) + offset).ok()?;
    }
    (remaining == 0).then_some(tuple)
}

/// Observes a program on one tuple at the full budget, as the checker does.
pub(crate) fn observe(program: &Program, input: &[i64]) -> Observation {
    let meter = v2_zero_limits().with_limit(V2Resource::Step, FULL_BUDGET);
    let (result, usage) = program.execute_v2(input, meter).into_parts();
    Observation {
        result,
        steps: usage.used(V2Resource::Step),
    }
}

/// NSF-004: recomputes a witness from the current immutable artifacts. Only a
/// witness that passes is factual feedback.
pub(crate) fn replay_witness(
    request: &Request,
    candidate: &[u8],
    witness: &Witness,
) -> Result<(), WitnessFault> {
    if witness.request_id != request.id() {
        return Err(WitnessFault::Request);
    }
    if witness.original_sha256 != request.original_sha256() {
        return Err(WitnessFault::Original);
    }
    if witness.candidate_sha256 != sha256_hex(candidate) {
        return Err(WitnessFault::Candidate);
    }
    let candidate = import_program(candidate).map_err(|_| WitnessFault::CandidateNotAdmitted)?;
    if witness.ordinal >= request.domain_size() {
        return Err(WitnessFault::Ordinal);
    }
    let tuple = tuple_of_ordinal(request.program().inputs(), witness.ordinal)
        .ok_or(WitnessFault::Ordinal)?;
    if tuple != witness.input {
        return Err(WitnessFault::Tuple);
    }
    let original = observe(request.program(), &tuple);
    let replacement = observe(&candidate, &tuple);
    if original != witness.original || replacement != witness.candidate {
        return Err(WitnessFault::Observations);
    }
    if original.result == replacement.result {
        return Err(WitnessFault::NoDifference);
    }
    Ok(())
}

/// Which earlier artifact a duplicate proposal repeats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DuplicateOf {
    Original,
    Attempt(u8),
}

/// Checker-derived feedback for one attempt (NSF-005). Everything here comes
/// from admission, the checker or the selection rule; nothing from the model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Feedback {
    /// A replayed semantic difference.
    Difference { attempt: u8, witness: Witness },
    /// Admission refused the candidate for this reason.
    Refused { attempt: u8, reason: Value },
    /// The check stopped without deciding (resource stop, worker failure).
    Incomplete { attempt: u8, stop: Value },
    /// Equivalent, with the actual cost and the selection decision.
    Equivalent {
        attempt: u8,
        cost: Cost,
        selection: Selection,
    },
    /// Byte-identical to an earlier artifact.
    Duplicate { attempt: u8, of: DuplicateOf },
    /// A mode or provider that is unavailable.
    Unavailable { attempt: u8, reason: String },
    /// The proposer produced nothing usable.
    Failed { attempt: u8, reason: String },
}

impl Feedback {
    pub(crate) fn json(&self, inputs: &[Domain], outputs: &[Domain]) -> Value {
        match self {
            Feedback::Difference { attempt, witness } => json!({
                "attempt": attempt, "kind": "verified-difference",
                "witness": witness.json(inputs, outputs)
            }),
            Feedback::Refused { attempt, reason } => {
                json!({"attempt": attempt, "kind": "admission-refused", "refusal": reason})
            }
            Feedback::Incomplete { attempt, stop } => {
                json!({"attempt": attempt, "kind": "incomplete-check", "stop": stop})
            }
            Feedback::Equivalent {
                attempt,
                cost,
                selection,
            } => json!({
                "attempt": attempt, "kind": "equivalent", "cost": cost.json(),
                "selection": selection.json()
            }),
            Feedback::Duplicate { attempt, of } => json!({
                "attempt": attempt, "kind": "duplicate",
                "of": match of {
                    DuplicateOf::Original => json!("original"),
                    DuplicateOf::Attempt(earlier) => json!({"attempt": earlier}),
                }
            }),
            Feedback::Unavailable { attempt, reason } => {
                json!({"attempt": attempt, "kind": "unavailable", "reason": reason})
            }
            Feedback::Failed { attempt, reason } => {
                json!({"attempt": attempt, "kind": "proposal-failed", "reason": reason})
            }
        }
    }
}

impl WitnessFault {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            WitnessFault::Request => "witness-request",
            WitnessFault::Original => "witness-original",
            WitnessFault::Candidate => "witness-candidate",
            WitnessFault::CandidateNotAdmitted => "witness-candidate-not-admitted",
            WitnessFault::Ordinal => "witness-ordinal",
            WitnessFault::Tuple => "witness-tuple",
            WitnessFault::Observations => "witness-observations",
            WitnessFault::NoDifference => "witness-no-difference",
        }
    }
}
