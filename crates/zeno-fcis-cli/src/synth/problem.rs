use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, fs::File, io::Read, path::Path};
use zeno_fcis_synthesis::finite::{Contract, Domain, Error, Op, PROFILE, Program, Sketch, Slot};

pub(super) const SCHEMA: &str = "zeno-fcis/synthesis-problem/1";
pub(super) const MAX_BYTES: u64 = 262_144;

/// One closed parser/discovery table; lengths include the opcode itself.
pub(super) const OPERATIONS: &[(&str, usize)] = &[
    ("input", 2),
    ("int", 2),
    ("bool", 2),
    ("not", 2),
    ("add", 3),
    ("sub", 3),
    ("eq", 3),
    ("lt", 3),
    ("and", 3),
    ("select", 4),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema: String,
    profile: String,
    inputs: Vec<Field>,
    outputs: Vec<Field>,
    contract: Graph,
    sketch: SketchGraph,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    name: String,
    #[serde(rename = "type")]
    kind: Type,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Type {
    Bool,
    Int { min: i64, max: i64 },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    nodes: Vec<Vec<Value>>,
    roots: Vec<u16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchGraph {
    nodes: Vec<Node>,
    roots: Vec<u16>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Node {
    Fixed(Vec<Value>),
    Hole(Hole),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hole {
    hole: u32,
    alternatives: Vec<Vec<Value>>,
}

pub(super) struct Problem {
    pub bytes: Vec<u8>,
    pub contract: Contract,
    pub sketch: Sketch,
    pub names: Value,
}

pub(super) enum LoadError {
    Io(std::io::Error),
    Invalid(String),
}

pub(super) fn read(path: &Path) -> Result<Problem, LoadError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(LoadError::Io)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(LoadError::Io)?;
    parse(bytes).map_err(LoadError::Invalid)
}
pub(super) fn parse(bytes: Vec<u8>) -> Result<Problem, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("problem-too-large".into());
    }
    let source: Source =
        serde_json::from_slice(&bytes).map_err(|e| format!("problem-json: {e}"))?;
    if source.schema != SCHEMA || source.profile != PROFILE {
        return Err("unsupported-profile-or-schema".into());
    }
    let inputs = fields(&source.inputs)?;
    let outputs = fields(&source.outputs)?;
    let nodes = source
        .contract
        .nodes
        .iter()
        .map(|op| parse_op(op))
        .collect::<Result<Vec<_>, _>>()?;
    let relation = Program::try_new(
        [inputs.clone(), outputs.clone()].concat(),
        vec![Domain::Bool],
        nodes,
        source.contract.roots,
    )
    .map_err(error)?;
    let contract = Contract::try_new(inputs.clone(), outputs.clone(), relation).map_err(error)?;
    let nodes = source
        .sketch
        .nodes
        .into_iter()
        .map(|node| match node {
            Node::Fixed(op) => Ok(Slot::Fixed(parse_op(&op)?)),
            Node::Hole(hole) => Ok(Slot::Choice {
                id: hole.hole,
                alternatives: hole
                    .alternatives
                    .iter()
                    .map(|op| parse_op(op))
                    .collect::<Result<Vec<_>, String>>()?,
            }),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let sketch = Sketch::try_new(inputs, outputs, nodes, source.sketch.roots).map_err(error)?;
    let names = serde_json::json!({"inputs":source.inputs.iter().map(|f|&f.name).collect::<Vec<_>>(),"outputs":source.outputs.iter().map(|f|&f.name).collect::<Vec<_>>()});
    Ok(Problem {
        bytes,
        contract,
        sketch,
        names,
    })
}
fn error(error: Error) -> String {
    error.to_string()
}
fn fields(fields: &[Field]) -> Result<Vec<Domain>, String> {
    if fields.len() > 16 {
        return Err("too-many-fields".into());
    }
    let mut names = BTreeSet::new();
    fields
        .iter()
        .map(|f| {
            if f.name.is_empty()
                || f.name.len() > 64
                || !f
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
                || !names.insert(&f.name)
            {
                return Err("invalid-or-duplicate-field-name".into());
            }
            Ok(match f.kind {
                Type::Bool => Domain::Bool,
                Type::Int { min, max } => Domain::Int { min, max },
            })
        })
        .collect()
}
fn parse_op(op: &[Value]) -> Result<Op, String> {
    let malformed = || "invalid-instruction".to_owned();
    let tag = op.first().and_then(Value::as_str).ok_or_else(malformed)?;
    let node = |i: usize| {
        op.get(i)
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(malformed)
    };
    let arity = OPERATIONS
        .iter()
        .find(|(name, _)| *name == tag)
        .map(|(_, arity)| *arity)
        .ok_or_else(|| "unsupported-instruction".to_owned())?;
    if op.len() != arity {
        return Err(malformed());
    }
    Ok(match tag {
        "input" => Op::Input(node(1)?),
        "int" => Op::Int(op[1].as_i64().ok_or_else(malformed)?),
        "bool" => Op::Bool(op[1].as_bool().ok_or_else(malformed)?),
        "add" => Op::Add(node(1)?, node(2)?),
        "sub" => Op::Sub(node(1)?, node(2)?),
        "eq" => Op::Eq(node(1)?, node(2)?),
        "lt" => Op::Lt(node(1)?, node(2)?),
        "and" => Op::And(node(1)?, node(2)?),
        "not" => Op::Not(node(1)?),
        "select" => Op::Select(node(1)?, node(2)?, node(3)?),
        _ => return Err(malformed()),
    })
}
