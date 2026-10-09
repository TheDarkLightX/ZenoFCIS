//! Expression trees compiled to node graphs: the scalar decision program and
//! the law programs. A node is reused when an identical node of the same kind
//! exists, so node numbering depends only on the order of compilation.

use std::collections::{BTreeMap, BTreeSet};

use super::declarations::{Declarations, Form, Input, Kind as TypeKind};
use super::expr::{Ast, Binary, Rounding};

/// How a node's value is used. Equal nodes of different kinds stay distinct.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Kind {
    Bool,
    Int,
    /// An observation or atom compared as it is, never converted.
    Raw,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Ref {
    pub(super) index: usize,
    pub(super) kind: Kind,
}

/// Operations both program forms share; laws also have `Mul`, `Div` and `ToI128`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Op {
    Not(usize),
    And(usize, usize),
    Eq(usize, usize),
    Lt(usize, usize),
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    Div(Rounding, usize, usize),
    Select(usize, usize, usize),
    ToI128(usize),
}

/// A typed value inside a law program.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Atom {
    Bool(bool),
    I128(i128),
    U128(u128),
    Sum { type_id: u32, variant: u16 },
    Text(Vec<u8>),
}

/// What a law program reads from a candidate transition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Observation {
    PreRoot,
    CommandRoot,
    ContextRoot,
    PostRoot,
    Pre(u16),
    Command(u16),
    Context(u16),
    Post(u16),
    Initial(u16),
    Class,
    HasReason,
    Reason,
    PostLength,
    PatchLength,
    EffectLength,
    OutboxLength,
    OutboxOrdinal(usize),
    OutboxChannel(usize),
    OutboxDestination(usize),
    OutboxPayload(usize, u16),
    OutboxIdempotency(usize),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum LawOp {
    Literal(Atom),
    Observe(Observation),
    ObserveWhen(usize, Observation, Atom),
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    Div(Rounding, usize, usize),
    ToI128(usize),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ScalarOp {
    Input(usize),
    Int(i64),
    Bool(bool),
    Add(usize, usize),
    Sub(usize, usize),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

/// Nodes in creation order with their reuse index.
#[derive(Debug)]
pub(super) struct Table<N> {
    pub(super) nodes: Vec<N>,
    index: BTreeMap<(N, Kind), usize>,
}

impl<N: Clone + Ord> Table<N> {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            index: BTreeMap::new(),
        }
    }

    fn put(&mut self, node: N, kind: Kind) -> Ref {
        let next = self.nodes.len();
        let index = *self.index.entry((node.clone(), kind)).or_insert(next);
        if index == next {
            self.nodes.push(node);
        }
        Ref { index, kind }
    }
}

/// Compilation shared by the scalar program and the law programs.
pub(super) trait Graph {
    type Node: Clone + Ord;

    fn table(&mut self) -> &mut Table<Self::Node>;
    fn bool_node(value: bool) -> Self::Node;
    fn int_node(value: i128) -> Result<Self::Node, String>;
    fn op_node(op: Op) -> Result<Self::Node, String>;
    /// A program input or a law observation.
    fn name(&mut self, name: &str) -> Result<Ref, String>;
    /// The value as an integer.
    fn numeric(&mut self, node: Ref) -> Result<Ref, String>;

    fn op(&mut self, op: Op, kind: Kind) -> Result<Ref, String> {
        let node = Self::op_node(op)?;
        Ok(self.table().put(node, kind))
    }

    fn bool(&mut self, value: bool) -> Ref {
        let node = Self::bool_node(value);
        self.table().put(node, Kind::Bool)
    }

    fn int(&mut self, value: i128) -> Result<Ref, String> {
        let node = Self::int_node(value)?;
        Ok(self.table().put(node, Kind::Int))
    }

    /// The value as a boolean: integers compare equal to one.
    fn boolean(&mut self, node: Ref) -> Result<Ref, String> {
        if node.kind == Kind::Bool {
            return Ok(node);
        }
        let one = self.int(1)?;
        self.op(Op::Eq(node.index, one.index), Kind::Bool)
    }

    /// Compiles an expression whose variables are already expanded.
    fn compile(&mut self, ast: &Ast) -> Result<Ref, String> {
        match ast {
            Ast::Bool(value) => Ok(self.bool(*value)),
            Ast::Int(value) => self.int(*value),
            Ast::Name(name) => self.name(name),
            Ast::Neg(inner) => {
                let zero = self.int(0)?;
                let inner = self.compile(inner)?;
                let inner = self.numeric(inner)?;
                self.op(Op::Sub(zero.index, inner.index), Kind::Int)
            }
            Ast::Not(inner) => {
                let inner = self.compile(inner)?;
                let inner = self.boolean(inner)?;
                self.op(Op::Not(inner.index), Kind::Bool)
            }
            Ast::Choose(condition, then, otherwise) => {
                let condition = self.compile(condition)?;
                let condition = self.boolean(condition)?;
                let mut then = self.compile(then)?;
                let mut otherwise = self.compile(otherwise)?;
                if then.kind != otherwise.kind {
                    then = self.numeric(then)?;
                    otherwise = self.numeric(otherwise)?;
                }
                self.op(
                    Op::Select(condition.index, then.index, otherwise.index),
                    then.kind,
                )
            }
            Ast::Binary(operator, left, right) => {
                let left = self.compile(left)?;
                let right = self.compile(right)?;
                self.binary(*operator, left, right)
            }
            Ast::Div(rounding, left, right) => {
                let left = self.compile(left)?;
                let right = self.compile(right)?;
                let left = self.numeric(left)?;
                let right = self.numeric(right)?;
                self.op(Op::Div(*rounding, left.index, right.index), Kind::Int)
            }
        }
    }

    fn binary(&mut self, operator: Binary, left: Ref, right: Ref) -> Result<Ref, String> {
        let logic =
            |graph: &mut Self| Ok::<_, String>((graph.boolean(left)?, graph.boolean(right)?));
        let values = |graph: &mut Self| {
            Ok::<_, String>((graph.numeric(left)?.index, graph.numeric(right)?.index))
        };
        let not = |graph: &mut Self, op| {
            let inner = graph.op(op, Kind::Bool)?;
            graph.op(Op::Not(inner.index), Kind::Bool)
        };
        match operator {
            Binary::And => {
                let (left, right) = logic(self)?;
                self.op(Op::And(left.index, right.index), Kind::Bool)
            }
            Binary::Or => {
                let (left, right) = logic(self)?;
                self.or(left, right)
            }
            Binary::Implies => {
                // `a -> b` is `!a || b`.
                let (left, right) = logic(self)?;
                let left = self.op(Op::Not(left.index), Kind::Bool)?;
                self.or(left, right)
            }
            Binary::Add => values(self).and_then(|(x, y)| self.op(Op::Add(x, y), Kind::Int)),
            Binary::Sub => values(self).and_then(|(x, y)| self.op(Op::Sub(x, y), Kind::Int)),
            Binary::Mul => values(self).and_then(|(x, y)| self.op(Op::Mul(x, y), Kind::Int)),
            Binary::Eq => values(self).and_then(|(x, y)| self.op(Op::Eq(x, y), Kind::Bool)),
            Binary::Ne => values(self).and_then(|(x, y)| not(self, Op::Eq(x, y))),
            Binary::Lt => values(self).and_then(|(x, y)| self.op(Op::Lt(x, y), Kind::Bool)),
            Binary::Gt => values(self).and_then(|(x, y)| self.op(Op::Lt(y, x), Kind::Bool)),
            Binary::Le => values(self).and_then(|(x, y)| not(self, Op::Lt(y, x))),
            Binary::Ge => values(self).and_then(|(x, y)| not(self, Op::Lt(x, y))),
        }
    }

    /// `a || b` as `!(!a && !b)`.
    fn or(&mut self, left: Ref, right: Ref) -> Result<Ref, String> {
        let left = self.op(Op::Not(left.index), Kind::Bool)?;
        let right = self.op(Op::Not(right.index), Kind::Bool)?;
        let both = self.op(Op::And(left.index, right.index), Kind::Bool)?;
        self.op(Op::Not(both.index), Kind::Bool)
    }

    /// `true && a && b && ...`, each item as a boolean.
    fn all(&mut self, items: &[Ref]) -> Result<Ref, String> {
        let mut result = self.bool(true);
        for item in items {
            let item = self.boolean(*item)?;
            result = self.op(Op::And(result.index, item.index), Kind::Bool)?;
        }
        Ok(result)
    }
}

/// The eagerly evaluated scalar decision program over the input positions.
pub(super) struct ScalarGraph<'d> {
    pub(super) table: Table<ScalarOp>,
    declarations: &'d Declarations,
    inputs: &'d [Input],
}

impl<'d> ScalarGraph<'d> {
    pub(super) fn new(declarations: &'d Declarations, inputs: &'d [Input]) -> Self {
        Self {
            table: Table::new(),
            declarations,
            inputs,
        }
    }
}

impl Graph for ScalarGraph<'_> {
    type Node = ScalarOp;

    fn table(&mut self) -> &mut Table<ScalarOp> {
        &mut self.table
    }

    fn bool_node(value: bool) -> ScalarOp {
        ScalarOp::Bool(value)
    }

    fn int_node(value: i128) -> Result<ScalarOp, String> {
        i64::try_from(value)
            .map(ScalarOp::Int)
            .map_err(|_| format!("decision integer {value} exceeds the 64-bit program range"))
    }

    fn op_node(op: Op) -> Result<ScalarOp, String> {
        Ok(match op {
            Op::Not(a) => ScalarOp::Not(a),
            Op::And(a, b) => ScalarOp::And(a, b),
            Op::Eq(a, b) => ScalarOp::Eq(a, b),
            Op::Lt(a, b) => ScalarOp::Lt(a, b),
            Op::Add(a, b) => ScalarOp::Add(a, b),
            Op::Sub(a, b) => ScalarOp::Sub(a, b),
            Op::Select(c, a, b) => ScalarOp::Select(c, a, b),
            Op::Mul(..) => return Err("decision rules have no multiplication".to_owned()),
            Op::Div(..) => return Err("decision rules have no division".to_owned()),
            Op::ToI128(_) => return Err("decision programs convert by selection".to_owned()),
        })
    }

    fn name(&mut self, name: &str) -> Result<Ref, String> {
        let (index, input) = self
            .inputs
            .iter()
            .enumerate()
            .find(|(_, input)| input.name == name)
            .ok_or_else(|| format!("`{name}` is not a variable or an input"))?;
        let kind = match self
            .declarations
            .kind(input.type_id)
            .map_err(|error| error.to_string())?
        {
            TypeKind::Bool => Kind::Bool,
            _ => Kind::Int,
        };
        Ok(self.table.put(ScalarOp::Input(index), kind))
    }

    fn numeric(&mut self, node: Ref) -> Result<Ref, String> {
        if node.kind == Kind::Int {
            return Ok(node);
        }
        let one = self.int(1)?;
        let zero = self.int(0)?;
        self.op(Op::Select(node.index, one.index, zero.index), Kind::Int)
    }
}

/// One law program over the observations of a candidate transition.
pub(super) struct LawGraph<'d> {
    pub(super) table: Table<LawOp>,
    bindings: BTreeMap<String, Ref>,
    used_bindings: BTreeSet<String>,
    declarations: &'d Declarations,
}

impl<'d> LawGraph<'d> {
    pub(super) fn bind(&mut self, name: String, value: Ref) {
        self.bindings.insert(name, value);
    }

    pub(super) fn require_bindings_used(&self) -> Result<(), String> {
        if let Some(name) = self
            .bindings
            .keys()
            .find(|name| !self.used_bindings.contains(*name))
        {
            return Err(format!("unused delivery observation `{name}`"));
        }
        Ok(())
    }

    pub(super) fn new(declarations: &'d Declarations) -> Self {
        Self {
            table: Table::new(),
            bindings: BTreeMap::new(),
            used_bindings: BTreeSet::new(),
            declarations,
        }
    }

    /// Reads an observation, guarded when `guard` is given; guarded reads
    /// yield `default` where the guard is false.
    pub(super) fn observe(
        &mut self,
        observation: Observation,
        kind: Kind,
        guard: Option<Ref>,
        default: Atom,
    ) -> Result<Ref, String> {
        let node = match guard {
            None => LawOp::Observe(observation),
            Some(guard) => LawOp::ObserveWhen(guard.index, observation, default),
        };
        let raw = self.table.put(node, Kind::Raw);
        match kind {
            Kind::Raw => Ok(raw),
            Kind::Bool => Ok(Ref {
                index: raw.index,
                kind: Kind::Bool,
            }),
            Kind::Int => self.numeric(raw),
        }
    }

    /// An atom compared as it is.
    pub(super) fn literal(&mut self, atom: Atom) -> Ref {
        self.table.put(LawOp::Literal(atom), Kind::Raw)
    }

    /// The observation a law name reads: `post.100.110` reads `Post(110)`,
    /// `command.101` reads `CommandRoot`.
    fn observation(&self, name: &str) -> Result<Observation, String> {
        let parts: Vec<&str> = name.split('.').collect();
        let (root, expected) = match parts.first() {
            Some(&"pre") => (0, 100),
            Some(&"post") => (1, 100),
            Some(&"command") => (2, 101),
            Some(&"context") => (3, 102),
            _ => return Err(format!("`{name}` is not a variable or an observation")),
        };
        if parts.get(1) != Some(&expected.to_string().as_str()) {
            return Err(format!("`{name}` must name root type {expected}"));
        }
        let record = matches!(
            self.declarations
                .get(expected)
                .map_err(|error| error.to_string())?
                .form,
            Form::Record(_)
        );
        match parts.get(2) {
            // The library observes a record root as no value at all.
            None if parts.len() == 2 && record => {
                Err(format!("`{name}` is a record; name one of its fields"))
            }
            None if parts.len() == 2 => Ok(match root {
                0 => Observation::PreRoot,
                1 => Observation::PostRoot,
                2 => Observation::CommandRoot,
                _ => Observation::ContextRoot,
            }),
            Some(field) if parts.len() == 3 => {
                let field = self
                    .declarations
                    .fields(expected)
                    .map_err(|error| error.to_string())?
                    .iter()
                    .find(|declared| declared.id.to_string() == *field)
                    .ok_or_else(|| format!("`{name}` names no field of type {expected}"))?
                    .id;
                Ok(match root {
                    0 => Observation::Pre(field),
                    1 => Observation::Post(field),
                    2 => Observation::Command(field),
                    _ => Observation::Context(field),
                })
            }
            _ => Err(format!("`{name}` is not a root or a root field")),
        }
    }
}

impl Graph for LawGraph<'_> {
    type Node = LawOp;

    fn table(&mut self) -> &mut Table<LawOp> {
        &mut self.table
    }

    fn bool_node(value: bool) -> LawOp {
        LawOp::Literal(Atom::Bool(value))
    }

    fn int_node(value: i128) -> Result<LawOp, String> {
        Ok(LawOp::Literal(Atom::I128(value)))
    }

    fn op_node(op: Op) -> Result<LawOp, String> {
        Ok(match op {
            Op::Not(a) => LawOp::Not(a),
            Op::And(a, b) => LawOp::And(a, b),
            Op::Eq(a, b) => LawOp::Eq(a, b),
            Op::Lt(a, b) => LawOp::Lt(a, b),
            Op::Add(a, b) => LawOp::Add(a, b),
            Op::Sub(a, b) => LawOp::Sub(a, b),
            Op::Mul(a, b) => LawOp::Mul(a, b),
            Op::Div(rounding, a, b) => LawOp::Div(rounding, a, b),
            Op::Select(c, a, b) => LawOp::Select(c, a, b),
            Op::ToI128(a) => LawOp::ToI128(a),
        })
    }

    fn name(&mut self, name: &str) -> Result<Ref, String> {
        if let Some(value) = self.bindings.get(name).copied() {
            self.used_bindings.insert(name.to_owned());
            return Ok(value);
        }
        let observation = self.observation(name)?;
        self.observe(observation, Kind::Int, None, Atom::I128(0))
    }

    fn numeric(&mut self, node: Ref) -> Result<Ref, String> {
        if node.kind == Kind::Int {
            return Ok(node);
        }
        self.op(Op::ToI128(node.index), Kind::Int)
    }
}
