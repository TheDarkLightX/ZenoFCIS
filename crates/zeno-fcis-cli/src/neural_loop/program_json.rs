//! The benchmark fixture vocabulary for finite programs as JSON
//! (`docs/benchmarks/DESIGN.md` section 6): domains are `{"kind": "Bool"}` or
//! `{"kind": "Int", "min": "<decimal>", "max": "<decimal>"}`, nodes are
//! `["Opcode", args...]` with Bool literals as JSON Booleans, integer literals
//! as decimal strings and references as JSON integers.
//!
//! This is an interchange form for proposers and the evaluation harness. It is
//! not the artifact codec: a program given this way is admitted by the library
//! and re-encoded canonically before it enters the checker, so its JSON length
//! and key order never define cost.

use serde_json::{Value, json};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

/// Why a JSON program could not become an admitted canonical program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProgramJsonError {
    /// The value is not an object with exactly inputs, outputs, nodes, roots.
    Shape,
    /// A domain object is malformed.
    Domain { position: usize },
    /// A decimal string is not a canonical i64.
    Decimal { text: String },
    /// A node is malformed or names an unknown opcode or arity.
    Node { index: usize },
    /// A root is not a node index.
    Root { position: usize },
    /// The library importer refused the program; `code` is its diagnostic.
    NotAdmitted { code: String },
    /// Canonical encoding failed after admission.
    Encoding,
}

/// Reads a canonical signed decimal: no plus sign, leading zeros or `-0`.
fn decimal(value: &Value) -> Result<i64, ProgramJsonError> {
    let text = value.as_str().ok_or_else(|| ProgramJsonError::Decimal {
        text: value.to_string(),
    })?;
    let error = || ProgramJsonError::Decimal {
        text: text.to_owned(),
    };
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || text == "-0"
    {
        return Err(error());
    }
    text.parse().map_err(|_| error())
}

fn domain(value: &Value, position: usize) -> Result<Domain, ProgramJsonError> {
    let error = || ProgramJsonError::Domain { position };
    let fields = value.as_object().ok_or_else(error)?;
    match (fields.get("kind").and_then(Value::as_str), fields.len()) {
        (Some("Bool"), 1) => Ok(Domain::Bool),
        (Some("Int"), 3) => Ok(Domain::Int {
            min: decimal(fields.get("min").ok_or_else(error)?)?,
            max: decimal(fields.get("max").ok_or_else(error)?)?,
        }),
        _ => Err(error()),
    }
}

fn reference(value: &Value) -> Option<u16> {
    value.as_u64().and_then(|id| u16::try_from(id).ok())
}

fn node(value: &Value, index: usize) -> Result<Op, ProgramJsonError> {
    let error = || ProgramJsonError::Node { index };
    let items = value.as_array().ok_or_else(error)?;
    let id = |position: usize| reference(items.get(position)?);
    let op = match (items.first().and_then(Value::as_str), items.len()) {
        (Some("Input"), 2) => Op::Input(id(1).ok_or_else(error)?),
        (Some("Int"), 2) => Op::Int(decimal(&items[1])?),
        (Some("Bool"), 2) => Op::Bool(items[1].as_bool().ok_or_else(error)?),
        (Some("Add"), 3) => Op::Add(id(1).ok_or_else(error)?, id(2).ok_or_else(error)?),
        (Some("Sub"), 3) => Op::Sub(id(1).ok_or_else(error)?, id(2).ok_or_else(error)?),
        (Some("Eq"), 3) => Op::Eq(id(1).ok_or_else(error)?, id(2).ok_or_else(error)?),
        (Some("Lt"), 3) => Op::Lt(id(1).ok_or_else(error)?, id(2).ok_or_else(error)?),
        (Some("And"), 3) => Op::And(id(1).ok_or_else(error)?, id(2).ok_or_else(error)?),
        (Some("Not"), 2) => Op::Not(id(1).ok_or_else(error)?),
        (Some("Select"), 4) => Op::Select(
            id(1).ok_or_else(error)?,
            id(2).ok_or_else(error)?,
            id(3).ok_or_else(error)?,
        ),
        _ => return Err(error()),
    };
    Ok(op)
}

/// Builds a program through the library's own admission.
pub(crate) fn program_from_json(value: &Value) -> Result<Program, ProgramJsonError> {
    if !super::only_fields(value, &["inputs", "outputs", "nodes", "roots"]) {
        return Err(ProgramJsonError::Shape);
    }
    let list = |field: &str| value.get(field)?.as_array();
    let (Some(inputs), Some(outputs), Some(nodes), Some(roots)) = (
        list("inputs"),
        list("outputs"),
        list("nodes"),
        list("roots"),
    ) else {
        return Err(ProgramJsonError::Shape);
    };
    let inputs = inputs
        .iter()
        .enumerate()
        .map(|(position, value)| domain(value, position))
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = outputs
        .iter()
        .enumerate()
        .map(|(position, value)| domain(value, position))
        .collect::<Result<Vec<_>, _>>()?;
    let nodes = nodes
        .iter()
        .enumerate()
        .map(|(index, value)| node(value, index))
        .collect::<Result<Vec<_>, _>>()?;
    let roots = roots
        .iter()
        .enumerate()
        .map(|(position, value)| reference(value).ok_or(ProgramJsonError::Root { position }))
        .collect::<Result<Vec<_>, _>>()?;
    Program::try_new(inputs, outputs, nodes, roots).map_err(|error| ProgramJsonError::NotAdmitted {
        code: format!("{error:?}"),
    })
}

/// Canonical artifact bytes of an admitted program.
pub(crate) fn encode(program: &Program) -> Result<Vec<u8>, ProgramJsonError> {
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .map_err(|_| ProgramJsonError::Encoding)
}

fn domain_json(domain: &Domain) -> Value {
    match *domain {
        Domain::Bool => json!({"kind": "Bool"}),
        Domain::Int { min, max } => {
            json!({"kind": "Int", "min": min.to_string(), "max": max.to_string()})
        }
        _ => {
            let (min, max) = domain.bounds();
            json!({"kind": "Int", "min": min.to_string(), "max": max.to_string()})
        }
    }
}

/// Domains in the fixture vocabulary.
pub(crate) fn domains_json(domains: &[Domain]) -> Value {
    Value::from(domains.iter().map(domain_json).collect::<Vec<_>>())
}

fn op_json(op: &Op) -> Value {
    match *op {
        Op::Input(a) => json!(["Input", a]),
        Op::Int(a) => json!(["Int", a.to_string()]),
        Op::Bool(a) => json!(["Bool", a]),
        Op::Add(a, b) => json!(["Add", a, b]),
        Op::Sub(a, b) => json!(["Sub", a, b]),
        Op::Eq(a, b) => json!(["Eq", a, b]),
        Op::Lt(a, b) => json!(["Lt", a, b]),
        Op::And(a, b) => json!(["And", a, b]),
        Op::Not(a) => json!(["Not", a]),
        Op::Select(c, a, b) => json!(["Select", c, a, b]),
        _ => json!(["Unclassified"]),
    }
}

/// An admitted program in the fixture vocabulary.
pub(crate) fn program_to_json(program: &Program) -> Value {
    json!({
        "inputs": domains_json(program.inputs()),
        "outputs": domains_json(program.outputs()),
        "nodes": program.nodes().iter().map(op_json).collect::<Vec<_>>(),
        "roots": program.roots(),
    })
}

/// Typed values in the fixture vocabulary: Bool positions as JSON Booleans,
/// integers as decimal strings.
pub(crate) fn typed_values(domains: &[Domain], values: &[i64]) -> Value {
    Value::Array(
        domains
            .iter()
            .zip(values)
            .map(|(domain, value)| match domain {
                Domain::Bool => Value::Bool(*value == 1),
                _ => Value::String(value.to_string()),
            })
            .collect(),
    )
}

impl ProgramJsonError {
    pub(crate) fn json(&self) -> Value {
        match self {
            ProgramJsonError::Shape => json!({"reason": "program-json-shape"}),
            ProgramJsonError::Domain { position } => {
                json!({"reason": "program-json-domain", "position": position})
            }
            ProgramJsonError::Decimal { text } => {
                json!({"reason": "program-json-decimal", "text": text})
            }
            ProgramJsonError::Node { index } => {
                json!({"reason": "program-json-node", "index": index})
            }
            ProgramJsonError::Root { position } => {
                json!({"reason": "program-json-root", "position": position})
            }
            ProgramJsonError::NotAdmitted { code } => {
                json!({"reason": "program-not-admitted", "code": code})
            }
            ProgramJsonError::Encoding => json!({"reason": "program-encoding"}),
        }
    }
}
