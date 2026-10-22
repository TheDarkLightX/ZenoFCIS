//! The canonical receipt of a checked composition, and its replay.
//!
//! A receipt, `zeno-fcis/checked-composition-receipt/2`, is the result
//! itself under its schema: the manifest's SHA-256 and commitment, each
//! leaf's pins and bound program identity, the checker's source-bound
//! identity with the evaluator digest, every count, and the SHA-256 of each
//! table the check ran. It is written as compact JSON with object keys in
//! byte order and one final newline, so one result has exactly one receipt.
//!
//! [`replay`] reads nothing from the receipt but its bytes: it runs every
//! check again on the sources, writes the receipt of that result, and
//! requires the two to be byte-identical. A receipt with a changed count,
//! digest, identity or pin, or sources that changed since, is refused.
//!
//! A composed-domain result has its own receipt,
//! `zeno-fcis/composed-domain-receipt/2`, written and replayed the same
//! way by [`DomainResult::receipt`] and [`replay_domain`]; it records the
//! verdict, whichever it is.

use serde_json::{Map, Value, json};

use super::{
    CheckedComposition, CompositionSources, DomainResult, DomainSources, Refusal, check,
    check_domain,
};

/// The schema of a checked composition's receipt.
pub const RECEIPT_SCHEMA: &str = "zeno-fcis/checked-composition-receipt/2";
/// The schema of a composed-domain result's receipt.
pub const DOMAIN_RECEIPT_SCHEMA: &str = "zeno-fcis/composed-domain-receipt/2";

/// What is wrong with a receipt's form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReceiptForm {
    /// It is not JSON.
    NotJson,
    /// It is JSON, but not in the canonical form a receipt is written in.
    NotCanonical,
    /// It is not an object holding exactly this schema and a result.
    Schema,
}

impl ReceiptForm {
    /// The form's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotJson => "not-json",
            Self::NotCanonical => "not-canonical",
            Self::Schema => "schema",
        }
    }
}

/// Why a receipt did not replay.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReplayRefusal {
    /// The receipt is not a canonical receipt of this schema.
    Malformed(ReceiptForm),
    /// The check refused the sources.
    Check(Refusal),
    /// The recomputed receipt differs from the supplied one.
    Mismatch {
        /// The JSON path of the first difference, such as
        /// `result.leaves[0].tables.local_sha256`.
        path: String,
        /// The supplied receipt's value there; `null` where it has none.
        receipt: Value,
        /// The recomputed value there; `null` where it has none.
        recomputed: Value,
    },
}

impl ReplayRefusal {
    /// The refusal as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        match self {
            Self::Malformed(form) => json!({"kind": "malformed", "form": form.name()}),
            Self::Check(refusal) => json!({"kind": "check", "refusal": refusal.json()}),
            Self::Mismatch {
                path,
                receipt,
                recomputed,
            } => json!({
                "kind": "mismatch",
                "path": path,
                "receipt": receipt,
                "recomputed": recomputed,
            }),
        }
    }
}

impl std::fmt::Display for ReplayRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(form) => {
                write!(formatter, "the receipt is malformed: {}", form.name())
            }
            Self::Check(refusal) => write!(formatter, "the check refused: {refusal}"),
            Self::Mismatch {
                path,
                receipt,
                recomputed,
            } => write!(
                formatter,
                "the receipt differs at {path}: it records {receipt}, the check gives {recomputed}"
            ),
        }
    }
}

impl std::error::Error for ReplayRefusal {}

impl CheckedComposition {
    /// The canonical receipt of this result.
    #[must_use]
    pub fn receipt(&self) -> String {
        canonical(&json!({"schema": RECEIPT_SCHEMA, "result": self.json()}))
    }
}

impl DomainResult {
    /// The canonical receipt of this result.
    #[must_use]
    pub fn receipt(&self) -> String {
        canonical(&json!({"schema": DOMAIN_RECEIPT_SCHEMA, "result": self.json()}))
    }
}

/// Runs every check on `sources` again and requires `receipt`, byte for
/// byte.
///
/// # Errors
/// A receipt that is not canonical, the check's refusal, or the first
/// difference between the receipt and the recomputed one.
pub fn replay(
    receipt: &str,
    sources: CompositionSources<'_>,
) -> Result<CheckedComposition, ReplayRefusal> {
    replay_against(receipt, RECEIPT_SCHEMA, || {
        let checked = check(sources)?;
        let recomputed = checked.receipt();
        Ok((checked, recomputed))
    })
}

/// Runs a composed-domain check on `sources` again and requires `receipt`,
/// byte for byte.
///
/// # Errors
/// A receipt that is not canonical, the check's refusal at validation, or
/// the first difference between the receipt and the recomputed one.
pub fn replay_domain(
    receipt: &str,
    sources: &DomainSources<'_>,
) -> Result<DomainResult, ReplayRefusal> {
    replay_against(receipt, DOMAIN_RECEIPT_SCHEMA, || {
        let result = check_domain(sources)?;
        let recomputed = result.receipt();
        Ok((result, recomputed))
    })
}

/// Requires `receipt` to be a canonical receipt of `schema`, runs the check
/// and requires its receipt byte for byte.
fn replay_against<T>(
    receipt: &str,
    schema: &str,
    run: impl FnOnce() -> Result<(T, String), Refusal>,
) -> Result<T, ReplayRefusal> {
    let recorded: Value = serde_json::from_str(receipt)
        .map_err(|_| ReplayRefusal::Malformed(ReceiptForm::NotJson))?;
    if canonical(&recorded) != receipt {
        return Err(ReplayRefusal::Malformed(ReceiptForm::NotCanonical));
    }
    let closed = recorded.as_object().is_some_and(|fields| {
        fields.len() == 2
            && fields.get("schema") == Some(&json!(schema))
            && fields.get("result").is_some_and(Value::is_object)
    });
    if !closed {
        return Err(ReplayRefusal::Malformed(ReceiptForm::Schema));
    }
    let (checked, recomputed) = run().map_err(ReplayRefusal::Check)?;
    if recomputed == receipt {
        return Ok(checked);
    }
    let fresh = serde_json::from_str(&recomputed)
        .map_err(|_| ReplayRefusal::Malformed(ReceiptForm::NotJson))?;
    let (path, receipt, recomputed) =
        first_difference("", &recorded, &fresh).unwrap_or_else(|| {
            // Equal values whose canonical texts differ cannot occur; report
            // the whole receipt rather than nothing.
            (String::new(), recorded.clone(), fresh.clone())
        });
    Err(ReplayRefusal::Mismatch {
        path,
        receipt,
        recomputed,
    })
}

/// The first place two values differ, in key and index order.
fn first_difference(path: &str, left: &Value, right: &Value) -> Option<(String, Value, Value)> {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|key| {
                let at = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                field_difference(&at, left, right, key)
            })
        }
        (Value::Array(left), Value::Array(right)) => {
            (0..left.len().max(right.len())).find_map(|index| {
                let at = format!("{path}[{index}]");
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) => first_difference(&at, left, right),
                    (left, right) => Some((
                        at,
                        left.cloned().unwrap_or(Value::Null),
                        right.cloned().unwrap_or(Value::Null),
                    )),
                }
            })
        }
        _ if left == right => None,
        _ => Some((path.to_owned(), left.clone(), right.clone())),
    }
}

fn field_difference(
    at: &str,
    left: &Map<String, Value>,
    right: &Map<String, Value>,
    key: &str,
) -> Option<(String, Value, Value)> {
    match (left.get(key), right.get(key)) {
        (Some(left), Some(right)) => first_difference(at, left, right),
        (left, right) => Some((
            at.to_owned(),
            left.cloned().unwrap_or(Value::Null),
            right.cloned().unwrap_or(Value::Null),
        )),
    }
}

/// Compact JSON with object keys sorted by their bytes, independent of the
/// map order `serde_json` was built with, and one final newline.
pub(crate) fn canonical(value: &Value) -> String {
    let mut text = String::new();
    write_canonical(value, &mut text);
    text.push('\n');
    text
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
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
