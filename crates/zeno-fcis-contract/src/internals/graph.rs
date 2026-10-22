//! Expression trees compiled to node graphs: the scalar decision program and
//! the law programs. A node is reused when an identical node of the same kind
//! exists, so node numbering depends only on the order of compilation.

use std::collections::{BTreeMap, BTreeSet};

use super::declarations::{Declarations, Form, Input, Kind as TypeKind};
use super::expr::{Ast, Binary, Rounding};

/// How a node's value is used. Equal nodes of different kinds stay distinct.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    Bool,
    Int,
    Unsigned,
    /// Original sum input, projected to its exact variant code only by internal numeric use.
    Variant,
    /// An observation or atom compared as it is, never converted.
    Raw,
}

#[derive(Clone, Copy, Debug)]
pub struct Ref {
    pub index: usize,
    pub kind: Kind,
}

/// Operations both program forms share, with explicit law-side numeric projections.
#[derive(Clone, Copy, Debug)]
pub enum Op {
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
    ToU128(usize),
    MulDiv(Rounding, usize, usize, usize),
    SignedMulDivFloor(usize, usize, usize),
    ProductsGe([usize; 3], [usize; 3]),
}

/// A typed value inside a law program.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Atom {
    Bool(bool),
    I128(i128),
    U128(u128),
    Sum { type_id: u32, variant: u16 },
    Text(Vec<u8>),
}

/// What a law program reads from a candidate transition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Observation {
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
pub enum LawOp {
    Literal(Atom),
    Observe(Observation),
    ObserveWhen(usize, Observation, Atom),
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    Div(Rounding, usize, usize),
    ToI128(usize),
    ToU128(usize),
    MulDiv(Rounding, usize, usize, usize),
    SignedMulDivFloor(usize, usize, usize),
    ProductsGe([usize; 3], [usize; 3]),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ScalarOp {
    Numeric(zeno_fcis_synthesis::finite::V2NumericOp),
    Input(usize),
    Int(i128),
    Bool(bool),
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    Div(Rounding, usize, usize),
    MulDiv(Rounding, usize, usize, usize),
    SignedMulDivFloor(usize, usize, usize),
    ProductsGe([usize; 3], [usize; 3]),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

/// Nodes in creation order with their reuse index.
#[derive(Debug)]
pub struct Table<N> {
    pub nodes: Vec<N>,
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
pub trait Graph {
    type Node: Clone + Ord;

    fn table(&mut self) -> &mut Table<Self::Node>;
    fn declarations(&self) -> &Declarations;
    fn strict(&self) -> bool;
    fn bool_node(value: bool) -> Self::Node;
    fn int_node(value: i128) -> Result<Self::Node, String>;
    fn unsigned_node(value: u128) -> Self::Node;
    fn unsigned_op_node(op: Op) -> Result<Self::Node, String> {
        Self::op_node(op)
    }
    fn unsigned_operation(&mut self, op: Op, kind: Kind) -> Result<Ref, String> {
        let node = Self::unsigned_op_node(op)?;
        Ok(self.table().put(node, kind))
    }
    fn uint(&mut self, value: u128) -> Ref {
        let node = Self::unsigned_node(value);
        self.table().put(node, Kind::Unsigned)
    }
    fn op_node(op: Op) -> Result<Self::Node, String>;
    /// A program input or a law observation.
    fn name(&mut self, name: &str) -> Result<Ref, String>;
    fn decision_reason(&mut self) -> Result<Ref, String> {
        Err("decision_reason is a law observation, not a program input".to_owned())
    }
    /// The value as an integer.
    fn numeric(&mut self, node: Ref) -> Result<Ref, String>;

    fn op(&mut self, op: Op, kind: Kind) -> Result<Ref, String> {
        let node = if kind == Kind::Unsigned {
            Self::unsigned_op_node(op)?
        } else {
            Self::op_node(op)?
        };
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
        if self.strict() {
            return Err("U128ConditionKindMismatch".into());
        }
        let one = self.int(1)?;
        self.op(Op::Eq(node.index, one.index), Kind::Bool)
    }

    /// Nonnegative product arguments have an explicit checked widening in laws.
    fn product_operand(&mut self, node: Ref) -> Result<Ref, String> {
        self.numeric(node)
    }
    /// A law's wide unsigned quotient converts back to the signed expression kind.
    fn product_result(&mut self, node: Ref) -> Result<Ref, String> {
        Ok(node)
    }

    /// Compiles an expression whose variables are already expanded.
    fn compile(&mut self, ast: &Ast) -> Result<Ref, String> {
        self.compile_with_limits(ast, false)
    }

    /// Historical expressions follow the original compiler directly. The
    /// new authoring boundary adds admission/lowering, not runtime authority.
    fn compile_with_limits(&mut self, ast: &Ast, bounded: bool) -> Result<Ref, String> {
        if bounded
            || self.strict()
            || super::quantity::typed(self.declarations())
            || super::expr::uses_new_arithmetic(ast)
        {
            let lowered =
                super::quantity::lower_with_strict(self.declarations(), ast, self.strict())?;
            self.compile_raw(&lowered)
        } else {
            self.compile_raw(ast)
        }
    }

    fn compile_raw(&mut self, ast: &Ast) -> Result<Ref, String> {
        match ast {
            Ast::Circuit(c) => {
                use zeno_fcis_spec::BoolNode;
                let mut values: Vec<Ref> = Vec::with_capacity(c.nodes().len());
                for node in c.nodes() {
                    let value = match node {
                        BoolNode::Formula(f) => {
                            let v = self.compile_raw(f)?;
                            self.boolean(v)?
                        }
                        BoolNode::Literal(v) => self.bool(*v),
                        BoolNode::Not(a) => {
                            self.op(Op::Not(values[*a as usize].index), Kind::Bool)?
                        }
                        BoolNode::And(a, b) => self.op(
                            Op::And(values[*a as usize].index, values[*b as usize].index),
                            Kind::Bool,
                        )?,
                        BoolNode::Or(a, b) => {
                            self.binary(Binary::Or, values[*a as usize], values[*b as usize])?
                        }
                        BoolNode::Select(c, a, b) => self.op(
                            Op::Select(
                                values[*c as usize].index,
                                values[*a as usize].index,
                                values[*b as usize].index,
                            ),
                            Kind::Bool,
                        )?,
                    };
                    values.push(value);
                }
                values
                    .get(c.output() as usize)
                    .copied()
                    .ok_or_else(|| "invalid Boolean circuit output".into())
            }
            Ast::U128(value) => Ok(self.uint(*value)),
            Ast::ToI128(v) | Ast::ToU128(v) => {
                let a = self.compile_raw(v)?;
                let signed = matches!(ast, Ast::ToI128(_));
                if a.kind != if signed { Kind::Unsigned } else { Kind::Int } {
                    return Err("U128CastKindMismatch".into());
                }
                self.unsigned_operation(
                    if signed {
                        Op::ToI128(a.index)
                    } else {
                        Op::ToU128(a.index)
                    },
                    if signed { Kind::Int } else { Kind::Unsigned },
                )
            }
            Ast::Bool(value) => Ok(self.bool(*value)),
            Ast::Int(value) => self.int(*value),
            Ast::Name(name) => self.name(name),
            Ast::DecisionReason => self.decision_reason(),
            Ast::Neg(inner) => {
                // The parser admits MIN directly; every other negation remains eager.
                let zero = self.int(0)?;
                let inner = self.compile_raw(inner)?;
                if inner.kind == Kind::Unsigned {
                    return Err("U128NegationUnsupported".into());
                }
                let inner = self.numeric(inner)?;
                self.op(Op::Sub(zero.index, inner.index), Kind::Int)
            }
            Ast::Not(inner) => {
                let inner = self.compile_raw(inner)?;
                let inner = self.boolean(inner)?;
                self.op(Op::Not(inner.index), Kind::Bool)
            }
            Ast::Choose(condition, then, otherwise) => {
                let condition = self.compile_raw(condition)?;
                let condition = self.boolean(condition)?;
                let mut then = self.compile_raw(then)?;
                let mut otherwise = self.compile_raw(otherwise)?;
                if then.kind != otherwise.kind {
                    if self.strict()
                        || then.kind == Kind::Unsigned
                        || otherwise.kind == Kind::Unsigned
                    {
                        return Err("U128SelectKindMismatch".into());
                    }
                    then = self.numeric(then)?;
                    otherwise = self.numeric(otherwise)?;
                }
                self.op(
                    Op::Select(condition.index, then.index, otherwise.index),
                    then.kind,
                )
            }
            Ast::Binary(operator, left, right) => {
                let left = self.compile_raw(left)?;
                let right = self.compile_raw(right)?;
                self.binary(*operator, left, right)
            }
            Ast::ScaledMulDiv(..) => Err("scaled call was not lowered".to_owned()),
            Ast::RepeatMulDiv(signed, x, a, d, count) => {
                let x = self.compile_raw(x)?;
                let mut value = self.numeric(x)?;
                let a = self.compile_raw(a)?;
                let a = self.numeric(a)?;
                let d = self.compile_raw(d)?;
                let d = self.numeric(d)?;
                for _ in 0..*count {
                    value = if *signed {
                        self.op(
                            Op::SignedMulDivFloor(value.index, a.index, d.index),
                            Kind::Int,
                        )?
                    } else {
                        let x = self.product_operand(value)?;
                        let a = self.product_operand(a)?;
                        let d = self.product_operand(d)?;
                        let result = self.op(
                            Op::MulDiv(Rounding::Floor, x.index, a.index, d.index),
                            if value.kind == Kind::Unsigned {
                                Kind::Unsigned
                            } else {
                                Kind::Int
                            },
                        )?;
                        self.product_result(result)?
                    };
                }
                Ok(value)
            }
            Ast::SignedMulDivFloor(a, b, d) => {
                let a = self.compile_raw(a)?;
                let a = self.numeric(a)?;
                let b = self.compile_raw(b)?;
                let b = self.numeric(b)?;
                let d = self.compile_raw(d)?;
                let d = self.numeric(d)?;
                self.op(Op::SignedMulDivFloor(a.index, b.index, d.index), Kind::Int)
            }
            Ast::MulDiv(rounding, a, b, d) => {
                let a = self.compile_raw(a)?;
                let result_kind = if a.kind == Kind::Unsigned {
                    Kind::Unsigned
                } else {
                    Kind::Int
                };
                let a = self.product_operand(a)?;
                let b = self.compile_raw(b)?;
                let b = self.product_operand(b)?;
                let d = self.compile_raw(d)?;
                let d = self.product_operand(d)?;
                let result = self.op(
                    Op::MulDiv(*rounding, a.index, b.index, d.index),
                    result_kind,
                )?;
                self.product_result(result)
            }
            Ast::ProductsGe(arguments) => {
                if arguments.len() != 4 && arguments.len() != 6 {
                    return Err("products_ge needs four or six arguments".to_owned());
                }
                let mut unsigned = false;
                let mut refs = Vec::with_capacity(arguments.len());
                for argument in arguments.iter() {
                    let node = self.compile_raw(argument)?;
                    unsigned |= node.kind == Kind::Unsigned;
                    refs.push(self.product_operand(node)?.index);
                }
                let (a, b) = if refs.len() == 4 {
                    let one = if unsigned { self.uint(1) } else { self.int(1)? };
                    let one = self.product_operand(one)?.index;
                    ([refs[0], refs[1], one], [refs[2], refs[3], one])
                } else {
                    ([refs[0], refs[1], refs[2]], [refs[3], refs[4], refs[5]])
                };
                if unsigned {
                    self.unsigned_operation(Op::ProductsGe(a, b), Kind::Bool)
                } else {
                    self.op(Op::ProductsGe(a, b), Kind::Bool)
                }
            }
            Ast::Div(rounding, left, right) => {
                let left = self.compile_raw(left)?;
                let right = self.compile_raw(right)?;
                let left = self.numeric(left)?;
                let right = self.numeric(right)?;
                if left.kind != right.kind {
                    return Err("U128KindMismatch".into());
                }
                self.op(Op::Div(*rounding, left.index, right.index), left.kind)
            }
        }
    }

    fn binary(&mut self, operator: Binary, left: Ref, right: Ref) -> Result<Ref, String> {
        if left.kind == Kind::Unsigned || right.kind == Kind::Unsigned {
            if left.kind != right.kind {
                return Err("U128KindMismatch".into());
            }
            let (a, b) = (left.index, right.index);
            return match operator {
                Binary::Add => self.op(Op::Add(a, b), Kind::Unsigned),
                Binary::Sub => self.op(Op::Sub(a, b), Kind::Unsigned),
                Binary::Mul => self.op(Op::Mul(a, b), Kind::Unsigned),
                Binary::Eq | Binary::Ne | Binary::Lt | Binary::Le | Binary::Gt | Binary::Ge => {
                    let op = match operator {
                        Binary::Eq | Binary::Ne => Op::Eq(a, b),
                        Binary::Lt | Binary::Ge => Op::Lt(a, b),
                        _ => Op::Lt(b, a),
                    };
                    let out = self.unsigned_operation(op, Kind::Bool)?;
                    if matches!(operator, Binary::Ne | Binary::Le | Binary::Ge) {
                        self.op(Op::Not(out.index), Kind::Bool)
                    } else {
                        Ok(out)
                    }
                }
                _ => Err("U128LogicKindMismatch".into()),
            };
        }
        if self.strict()
            && matches!(operator, Binary::Eq | Binary::Ne)
            && left.kind == Kind::Bool
            && right.kind == Kind::Bool
        {
            let v = self.op(Op::Eq(left.index, right.index), Kind::Bool)?;
            return if operator == Binary::Ne {
                self.op(Op::Not(v.index), Kind::Bool)
            } else {
                Ok(v)
            };
        }
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
pub struct ScalarGraph<'d> {
    pub table: Table<ScalarOp>,
    declarations: &'d Declarations,
    strict: bool,
    inputs: &'d [Input],
}

impl<'d> ScalarGraph<'d> {
    pub fn new(declarations: &'d Declarations, inputs: &'d [Input]) -> Self {
        Self {
            table: Table::new(),
            declarations,
            strict: declarations.unsigned(),
            inputs,
        }
    }
}

impl ScalarGraph<'_> {
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict |= strict;
        self
    }
}

impl Graph for ScalarGraph<'_> {
    type Node = ScalarOp;
    fn strict(&self) -> bool {
        self.strict
    }
    fn unsigned_node(value: u128) -> ScalarOp {
        ScalarOp::Numeric(zeno_fcis_synthesis::finite::V2NumericOp::U128(value))
    }
    fn unsigned_op_node(op: Op) -> Result<ScalarOp, String> {
        use zeno_fcis_synthesis::finite::{V2IntegerWidth as W, V2NumericOp as N};
        let id = |x| u16::try_from(x).map_err(|_| "numeric node reference exceeds u16".to_owned());
        Ok(ScalarOp::Numeric(match op {
            Op::Add(a, b) => N::Add(W::Unsigned128, id(a)?, id(b)?),
            Op::Sub(a, b) => N::Sub(W::Unsigned128, id(a)?, id(b)?),
            Op::Mul(a, b) => N::Mul(W::Unsigned128, id(a)?, id(b)?),
            Op::Div(Rounding::Floor, a, b) => N::DivFloor(W::Unsigned128, id(a)?, id(b)?),
            Op::Div(Rounding::Ceil, a, b) => N::DivCeil(W::Unsigned128, id(a)?, id(b)?),
            Op::Eq(a, b) => N::Eq(id(a)?, id(b)?),
            Op::Lt(a, b) => N::Lt(id(a)?, id(b)?),
            Op::Select(c, a, b) => N::Select(id(c)?, id(a)?, id(b)?),
            Op::MulDiv(Rounding::Floor, a, b, d) => N::MulDivFloor(id(a)?, id(b)?, id(d)?),
            Op::MulDiv(Rounding::Ceil, a, b, d) => N::MulDivCeil(id(a)?, id(b)?, id(d)?),
            Op::ToI128(a) => N::ToI128(id(a)?),
            Op::ToU128(a) => N::ToU128(id(a)?),
            Op::ProductsGe(a, b) => N::ProductsGe(
                [id(a[0])?, id(a[1])?, id(a[2])?],
                [id(b[0])?, id(b[1])?, id(b[2])?],
            ),
            _ => return Err("unsupported unsigned operation".into()),
        }))
    }

    fn declarations(&self) -> &Declarations {
        self.declarations
    }

    fn table(&mut self) -> &mut Table<ScalarOp> {
        &mut self.table
    }

    fn bool_node(value: bool) -> ScalarOp {
        ScalarOp::Bool(value)
    }

    fn int_node(value: i128) -> Result<ScalarOp, String> {
        Ok(ScalarOp::Int(value))
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
            Op::Mul(a, b) => ScalarOp::Mul(a, b),
            Op::Div(r, a, b) => ScalarOp::Div(r, a, b),
            Op::SignedMulDivFloor(a, b, d) => ScalarOp::SignedMulDivFloor(a, b, d),
            Op::MulDiv(r, a, b, d) => ScalarOp::MulDiv(r, a, b, d),
            Op::ProductsGe(a, b) => ScalarOp::ProductsGe(a, b),
            Op::ToU128(_) => {
                return Err("scalar widening is explicit in the arithmetic evaluator".to_owned());
            }
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
            TypeKind::U128 { .. } => Kind::Unsigned,
            TypeKind::Sum(_) if self.strict() => Kind::Variant,
            _ => Kind::Int,
        };
        Ok(self.table.put(ScalarOp::Input(index), kind))
    }

    fn numeric(&mut self, node: Ref) -> Result<Ref, String> {
        if matches!(node.kind, Kind::Int | Kind::Unsigned) {
            return Ok(node);
        }
        if node.kind == Kind::Variant {
            return Ok(Ref {
                index: node.index,
                kind: Kind::Int,
            });
        }
        if self.strict() {
            return Err("U128NumericKindMismatch".into());
        }
        let one = self.int(1)?;
        let zero = self.int(0)?;
        self.op(Op::Select(node.index, one.index, zero.index), Kind::Int)
    }
}

/// One law program over the observations of a candidate transition.
pub struct LawGraph<'d> {
    pub table: Table<LawOp>,
    bindings: BTreeMap<String, Ref>,
    used_bindings: BTreeSet<String>,
    declarations: &'d Declarations,
    strict: bool,
}

impl<'d> LawGraph<'d> {
    pub fn bind(&mut self, name: String, value: Ref) {
        self.bindings.insert(name, value);
    }

    pub fn require_bindings_used(&self) -> Result<(), String> {
        if let Some(name) = self
            .bindings
            .keys()
            .find(|name| !self.used_bindings.contains(*name))
        {
            return Err(format!("unused delivery observation `{name}`"));
        }
        Ok(())
    }

    pub fn new(declarations: &'d Declarations) -> Self {
        Self {
            table: Table::new(),
            bindings: BTreeMap::new(),
            used_bindings: BTreeSet::new(),
            declarations,
            strict: declarations.unsigned(),
        }
    }

    /// Reads an observation, guarded when `guard` is given; guarded reads
    /// yield `default` where the guard is false.
    pub fn observe(
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
            Kind::Variant => Ok(Ref {
                index: raw.index,
                kind: Kind::Variant,
            }),
            Kind::Unsigned => Ok(Ref {
                index: raw.index,
                kind: Kind::Unsigned,
            }),
            Kind::Bool => Ok(Ref {
                index: raw.index,
                kind: Kind::Bool,
            }),
            Kind::Int => self.numeric(raw),
        }
    }

    /// An atom compared as it is.
    pub fn literal(&mut self, atom: Atom) -> Ref {
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

impl LawGraph<'_> {
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict |= strict;
        self
    }
}

impl Graph for LawGraph<'_> {
    type Node = LawOp;
    fn strict(&self) -> bool {
        self.strict
    }
    fn unsigned_node(value: u128) -> LawOp {
        LawOp::Literal(Atom::U128(value))
    }

    fn declarations(&self) -> &Declarations {
        self.declarations
    }

    fn product_operand(&mut self, node: Ref) -> Result<Ref, String> {
        if node.kind == Kind::Unsigned {
            return Ok(node);
        }
        let node = self.numeric(node)?;
        self.op(Op::ToU128(node.index), Kind::Raw)
    }
    fn product_result(&mut self, node: Ref) -> Result<Ref, String> {
        if node.kind == Kind::Unsigned {
            Ok(node)
        } else {
            self.op(Op::ToI128(node.index), Kind::Int)
        }
    }

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
            Op::ToU128(a) => LawOp::ToU128(a),
            Op::SignedMulDivFloor(a, b, d) => LawOp::SignedMulDivFloor(a, b, d),
            Op::MulDiv(r, a, b, d) => LawOp::MulDiv(r, a, b, d),
            Op::ProductsGe(a, b) => LawOp::ProductsGe(a, b),
        })
    }

    fn name(&mut self, name: &str) -> Result<Ref, String> {
        if let Some(value) = self.bindings.get(name).copied() {
            self.used_bindings.insert(name.to_owned());
            return Ok(value);
        }
        let observation = self.observation(name)?;
        let parts: Vec<_> = name.split('.').collect();
        let ty = parts
            .get(1)
            .and_then(|x| x.parse::<u32>().ok())
            .and_then(|root| {
                if parts.len() == 2 {
                    Some(root)
                } else {
                    self.declarations
                        .fields(root)
                        .ok()?
                        .iter()
                        .find(|f| f.id.to_string() == parts[2])
                        .map(|f| f.type_id)
                }
            });
        if ty
            .and_then(|id| self.declarations.kind(id).ok())
            .is_some_and(|k| matches!(k, TypeKind::U128 { .. }))
        {
            let mut value = self.observe(observation, Kind::Raw, None, Atom::U128(0))?;
            value.kind = Kind::Unsigned;
            Ok(value)
        } else if self.strict()
            && ty
                .and_then(|id| self.declarations.kind(id).ok())
                .is_some_and(|k| matches!(k, TypeKind::Sum(_)))
        {
            self.observe(observation, Kind::Variant, None, Atom::I128(0))
        } else if self.strict()
            && ty
                .and_then(|id| self.declarations.kind(id).ok())
                .is_some_and(|k| matches!(k, TypeKind::Bool))
        {
            self.observe(observation, Kind::Bool, None, Atom::Bool(false))
        } else {
            self.observe(observation, Kind::Int, None, Atom::I128(0))
        }
    }

    fn decision_reason(&mut self) -> Result<Ref, String> {
        self.observe(Observation::Reason, Kind::Int, None, Atom::I128(0))
    }

    fn numeric(&mut self, node: Ref) -> Result<Ref, String> {
        if matches!(node.kind, Kind::Int | Kind::Unsigned) {
            return Ok(node);
        }
        if self.strict() && node.kind == Kind::Bool {
            return Err("U128NumericKindMismatch".into());
        }
        self.op(Op::ToI128(node.index), Kind::Int)
    }
}
