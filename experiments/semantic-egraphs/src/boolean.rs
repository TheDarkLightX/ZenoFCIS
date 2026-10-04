use egg::{Id, Language, RecExpr, Symbol, define_language};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

define_language! {
    pub enum Boolean {
        "true" = True,
        "false" = False,
        "not" = Not(Id),
        "and" = And([Id; 2]),
        "or" = Or([Id; 2]),
        "ite" = Ite([Id; 3]),
        Input(Symbol),
    }
}

pub fn input_index(symbol: Symbol, nvars: usize) -> Result<usize, String> {
    let text = symbol.as_str();
    let index = text
        .strip_prefix('v')
        .ok_or_else(|| format!("Non-Boolean leaf {text}"))?
        .parse::<usize>()
        .map_err(|_| format!("Invalid Boolean input {text}"))?;
    if index >= nvars || text != format!("v{index}") {
        return Err(format!(
            "Input {text} outside the declared {nvars}-Boolean interface"
        ));
    }
    Ok(index)
}

pub fn parse_outputs(outputs: &[String], nvars: usize) -> Result<Vec<RecExpr<Boolean>>, String> {
    if nvars > 6 || outputs.is_empty() || outputs.len() > 16 {
        return Err("Boolean region requires 0..6 inputs and 1..16 outputs".into());
    }
    outputs
        .iter()
        .map(|text| {
            if text.len() > 128_000 {
                return Err("Expression text resource limit".into());
            }
            let expr = text
                .parse::<RecExpr<Boolean>>()
                .map_err(|e| format!("Boolean parse: {e}"))?;
            if expr.is_empty() || expr.len() > 4096 {
                return Err("Parsed expression resource limit".into());
            }
            for node in expr.as_ref() {
                if let Boolean::Input(symbol) = node {
                    input_index(*symbol, nvars)?;
                }
            }
            Ok(expr)
        })
        .collect()
}

pub fn direct(expr: &RecExpr<Boolean>, input: &[i64]) -> Result<bool, String> {
    if input.iter().any(|&v| v != 0 && v != 1) {
        return Err("input-domain".into());
    }
    let mut values: Vec<bool> = Vec::with_capacity(expr.len());
    for node in expr.as_ref() {
        let at = |id: Id| values[usize::from(id)];
        values.push(match node {
            Boolean::True => true,
            Boolean::False => false,
            Boolean::Input(symbol) => input[input_index(*symbol, input.len())?] == 1,
            Boolean::Not(a) => !at(*a),
            Boolean::And([a, b]) => at(*a) && at(*b),
            Boolean::Or([a, b]) => at(*a) || at(*b),
            Boolean::Ite([c, a, b]) => {
                if at(*c) {
                    at(*a)
                } else {
                    at(*b)
                }
            }
        });
    }
    values
        .last()
        .copied()
        .ok_or_else(|| "Empty direct Boolean expression".into())
}

#[derive(Default)]
struct Builder {
    nodes: Vec<Op>,
    interned: BTreeMap<String, u16>,
}
impl Builder {
    fn intern(&mut self, op: Op) -> Result<u16, String> {
        // FCIS Op has no Hash implementation; its exhaustive Debug encoding is
        // used only as a local structural interning key, never an identity claim.
        let key = format!("{op:?}");
        if let Some(&id) = self.interned.get(&key) {
            return Ok(id);
        }
        if self.nodes.len() >= 256 {
            return Err("Emitted FCIS Program exceeds256-node ceiling".into());
        }
        let id = self.nodes.len() as u16;
        self.nodes.push(op);
        self.interned.insert(key, id);
        Ok(id)
    }
    fn expression(&mut self, expr: &RecExpr<Boolean>, nvars: usize) -> Result<u16, String> {
        let mut ids = Vec::with_capacity(expr.len());
        for node in expr.as_ref() {
            let at = |id: Id| ids[usize::from(id)];
            let op = match node {
                Boolean::True => Op::Bool(true),
                Boolean::False => Op::Bool(false),
                Boolean::Input(symbol) => Op::Input(input_index(*symbol, nvars)? as u16),
                Boolean::Not(a) => Op::Not(at(*a)),
                Boolean::And([a, b]) => Op::And(at(*a), at(*b)),
                Boolean::Ite([c, a, b]) => Op::Select(at(*c), at(*a), at(*b)),
                Boolean::Or([a, b]) => {
                    let na = self.intern(Op::Not(at(*a)))?;
                    let nb = self.intern(Op::Not(at(*b)))?;
                    let conjunction = self.intern(Op::And(na, nb))?;
                    Op::Not(conjunction)
                }
            };
            ids.push(self.intern(op)?);
        }
        ids.last()
            .copied()
            .ok_or_else(|| "Cannot lower empty output".into())
    }
}

pub fn lower(expressions: &[RecExpr<Boolean>], nvars: usize) -> Result<Program, String> {
    let mut builder = Builder::default();
    let roots = expressions
        .iter()
        .map(|e| builder.expression(e, nvars))
        .collect::<Result<Vec<_>, _>>()?;
    Program::try_new(
        vec![Domain::Bool; nvars],
        vec![Domain::Bool; roots.len()],
        builder.nodes,
        roots,
    )
    .map_err(|e| format!("FCIS construction: {e:?}"))
}

pub fn admit_total_boolean(program: &Program) -> Result<(), String> {
    if program
        .inputs()
        .iter()
        .chain(program.outputs())
        .any(|d| *d != Domain::Bool)
    {
        return Err("Optimizer accepts only declared Bool inputs and outputs".into());
    }
    if program.nodes().iter().any(|op| {
        !matches!(
            op,
            Op::Input(_) | Op::Bool(_) | Op::And(_, _) | Op::Not(_) | Op::Select(_, _, _)
        )
    }) {
        return Err("Optimizer rejects arithmetic/comparison operations, including dead nodes and Select arms".into());
    }
    Ok(())
}

pub fn program_json(program: &Program) -> Value {
    let domain = |d: &Domain| match d {
        Domain::Bool => json!({"kind":"bool"}),
        Domain::Int { min, max } => json!({"kind":"int","min":min,"max":max}),
    };
    let nodes: Vec<_> = program
        .nodes()
        .iter()
        .map(|op| match *op {
            Op::Input(index) => json!({"op":"input","index":index,"args":[]}),
            Op::Bool(value) => json!({"op":"bool","value":value,"args":[]}),
            Op::Int(value) => json!({"op":"int","value":value,"args":[]}),
            Op::Not(a) => json!({"op":"not","args":[a]}),
            Op::And(a, b) => json!({"op":"and","args":[a,b]}),
            Op::Select(c, a, b) => json!({"op":"select","args":[c,a,b]}),
            Op::Add(a, b) => json!({"op":"add","args":[a,b]}),
            Op::Sub(a, b) => json!({"op":"sub","args":[a,b]}),
            Op::Eq(a, b) => json!({"op":"eq","args":[a,b]}),
            Op::Lt(a, b) => json!({"op":"lt","args":[a,b]}),
        })
        .collect();
    json!({"inputs":program.inputs().iter().map(domain).collect::<Vec<_>>(),
           "outputs":program.outputs().iter().map(domain).collect::<Vec<_>>(),
           "nodes":nodes,"roots":program.roots(),"profile":"zeno-fcis/finite-i64/1"})
}

pub fn rooted_outputs(expr: &RecExpr<Boolean>, roots: &[Id]) -> Vec<String> {
    // Public RecExpr formatting prints only its last node; construct a compact
    // postorder reachable slice for each root while preserving repeated edges.
    roots
        .iter()
        .map(|&root| {
            let mut ids = BTreeMap::new();
            let mut reached = std::collections::BTreeSet::new();
            let mut todo = vec![root];
            while let Some(id) = todo.pop() {
                if reached.insert(id) {
                    todo.extend_from_slice(expr[id].children());
                }
            }
            let mut output = RecExpr::default();
            for id in reached {
                let new = output.add(expr[id].clone().map_children(|child| ids[&child]));
                ids.insert(id, new);
            }
            output.to_string()
        })
        .collect()
}
