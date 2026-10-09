//! The owner's strengthening invariant: extra state formulas that
//! `contract check-symbolic` assumes on every pre-state, alongside the
//! declared state laws, and checks on every committing case's successor and
//! on the genesis state.
//!
//! The file is JSON:
//!
//! ```json
//! {
//!   "schema": "zeno-fcis/strengthening/1",
//!   "invariants": [
//!     {"name": "created_pays_nothing",
//!      "formula": "post.100.110 == 150 -> post.100.113 == 0 && post.100.114 == 0"}
//!   ]
//! }
//! ```
//!
//! Each formula is written in the rules' expression language and reads the
//! state as a state law does, through `post.100.FIELD`. It compiles with the
//! law compiler into a law program, which the library's law evaluator runs
//! on genesis and on each replayed state. A strengthening is not enforced at
//! run time: it is an argument, sound only because the check shows that
//! genesis satisfies it and that every committing case preserves it.

use serde_json::Value;

use super::super::ContractError;
use super::super::declarations::Declarations;
use super::super::expr;
use super::super::graph::{Graph, LawGraph, LawOp, Observation};

/// The file format this check reads.
pub(crate) const SCHEMA: &str = "zeno-fcis/strengthening/1";
/// The most clauses one file may hold.
const MAX_CLAUSES: usize = 64;
/// The longest formula, in bytes.
const MAX_FORMULA_BYTES: usize = 4096;

/// One compiled clause.
#[derive(Clone, Debug)]
pub(super) struct Clause {
    pub(super) name: String,
    pub(super) formula: String,
    pub(super) nodes: Vec<LawOp>,
    pub(super) root: usize,
}

/// Reads and compiles every clause.
///
/// # Errors
/// A malformed file, a name that is not a lowercase identifier or repeats,
/// a formula that does not parse or compile, or one that reads anything
/// but the state.
pub(super) fn read(text: &str, declarations: &Declarations) -> Result<Vec<Clause>, ContractError> {
    let invalid =
        |place: &str, reason: String| ContractError::new(format!("strengthening {place}"), reason);
    let file: Value =
        serde_json::from_str(text).map_err(|error| invalid("file", error.to_string()))?;
    let Some(object) = file.as_object() else {
        return Err(invalid("file", "must be a JSON object".to_owned()));
    };
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "schema" | "invariants"))
    {
        return Err(invalid(key, "is not a key of this format".to_owned()));
    }
    match object.get("schema").and_then(Value::as_str) {
        Some(SCHEMA) => {}
        found => {
            return Err(invalid(
                "schema",
                format!("expected `{SCHEMA}`, found {found:?}"),
            ));
        }
    }
    let Some(entries) = object.get("invariants").and_then(Value::as_array) else {
        return Err(invalid("invariants", "must be an array".to_owned()));
    };
    if entries.len() > MAX_CLAUSES {
        return Err(invalid(
            "invariants",
            format!("holds {} clauses; at most {MAX_CLAUSES}", entries.len()),
        ));
    }
    let mut clauses: Vec<Clause> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let place = format!("invariants[{index}]");
        let Some(entry) = entry.as_object() else {
            return Err(invalid(&place, "must be an object".to_owned()));
        };
        if let Some(key) = entry
            .keys()
            .find(|key| !matches!(key.as_str(), "name" | "formula"))
        {
            return Err(invalid(
                &format!("{place}.{key}"),
                "is not a key of a clause".to_owned(),
            ));
        }
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(&format!("{place}.{key}"), "must be a string".to_owned()))
        };
        let name = text("name")?;
        let identifier = name.len() <= 64
            && name.starts_with(|first: char| first.is_ascii_lowercase())
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if !identifier {
            return Err(invalid(
                &format!("{place}.name"),
                format!("`{name}` is not a lowercase identifier of at most 64 bytes"),
            ));
        }
        if clauses.iter().any(|clause| clause.name == name) {
            return Err(invalid(
                &format!("{place}.name"),
                format!("`{name}` repeats"),
            ));
        }
        let formula = text("formula")?;
        if formula.len() > MAX_FORMULA_BYTES {
            return Err(invalid(
                &format!("{place}.formula"),
                format!("is longer than {MAX_FORMULA_BYTES} bytes"),
            ));
        }
        let failed = |reason: String| invalid(&format!("{place}.formula"), reason);
        let ast = expr::parse(formula).map_err(failed)?;
        let mut graph = LawGraph::new(declarations);
        let value = graph.compile(&ast).map_err(failed)?;
        let root = graph.boolean(value).map_err(failed)?;
        if let Some(read) = graph.table.nodes.iter().find_map(|node| match node {
            LawOp::Observe(Observation::Post(_)) => None,
            LawOp::Observe(observation) | LawOp::ObserveWhen(_, observation, _) => {
                Some(*observation)
            }
            _ => None,
        }) {
            return Err(failed(format!(
                "reads {read:?}; a clause reads only the state, through post.100.FIELD"
            )));
        }
        clauses.push(Clause {
            name: name.to_owned(),
            formula: formula.to_owned(),
            nodes: graph.table.nodes,
            root: root.index,
        });
    }
    Ok(clauses)
}
