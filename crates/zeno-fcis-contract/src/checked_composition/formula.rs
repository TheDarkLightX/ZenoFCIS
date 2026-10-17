//! Composed-domain formulas over named fields: participation, bindings,
//! assumptions, invariants and couplings.
//!
//! A formula is written in the contract rules' expression language. A
//! predicate (participation, assumption, invariant, coupling) is compiled by
//! the contract's own law compiler, with each name it reads bound to one
//! field of a record, and the library's law evaluator decides it, as `/1`
//! decides its invariants. A binding is a rule expression: the contract's
//! decision-program compiler compiles it once into a library program of one
//! output, with checked 64-bit arithmetic and no multiplication or division,
//! and the library's program evaluator computes its value. Nothing here
//! interprets a formula.
//!
//! A formula is tabulated once over the complete product of the domains of
//! the names it reads, one evaluation per cell: a predicate's table holds
//! true, false or no value; a binding's table holds its value when the value
//! lies in the bound field's domain, and otherwise says that it lies outside
//! or has no value (an arithmetic overflow). The checks and the joint step
//! look values up. A predicate with no value counts as false wherever it is
//! read, as in the contract rules, where a number is true exactly when it is
//! 1. Every evaluation is counted in the caller's counter.

use std::cell::Cell as Counter;
use std::collections::{BTreeMap, BTreeSet};

use zeno_fcis_synthesis::finite::{
    Domain as ProgramDomain, Op as ProgramOp, Program, v2_composition as c,
};

use super::domain::FieldDomain;
use super::leaf::for_each_tuple;
use super::predicate::{Predicate, Truth};
use crate::internals::declarations::{
    Declarations, Form as TypeForm, Input, Leaf, Notes, Source, Type,
};
use crate::internals::expr::{self, Ast};
use crate::internals::graph::{
    Atom, Graph, Kind, LawGraph, Observation, Ref, ScalarGraph, ScalarOp,
};

/// The longest formula text the profile reads.
pub(crate) const MAX_FORMULA_BYTES: usize = 4_096;
/// The most expression nodes one formula may have.
pub(crate) const MAX_FORMULA_NODES: usize = 1_024;
/// The deepest one formula's expression tree may nest, counting a flat chain
/// of binary operators as one level per operator.
pub(crate) const MAX_FORMULA_DEPTH: usize = 128;

/// Why a formula's text is not admitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Intake {
    /// The expression parser refuses it.
    Syntax(String),
    /// It exceeds a size, node or depth limit.
    TooLarge(String),
}

/// One formula as written, with the distinct names it reads in byte order.
#[derive(Clone, Debug)]
pub(crate) struct Formula {
    pub(crate) text: String,
    ast: Ast,
    pub(crate) names: Vec<String>,
}

impl Formula {
    /// Parses a formula and collects the names it reads.
    ///
    /// # Errors
    /// The expression parser's refusal.
    pub(crate) fn parse(text: &str) -> Result<Self, Intake> {
        // A long flat chain such as `a && b && ...` parses into a deep left
        // spine without deepening the parser's recursion: the text is bounded
        // before a tree exists, and the tree's size and depth after.
        if text.len() > MAX_FORMULA_BYTES {
            return Err(Intake::TooLarge(format!(
                "is {} bytes long; a formula has at most {MAX_FORMULA_BYTES}",
                text.len()
            )));
        }
        let ast = expr::parse(text).map_err(Intake::Syntax)?;
        let (nodes, depth) = shape(&ast);
        if nodes > MAX_FORMULA_NODES || depth > MAX_FORMULA_DEPTH {
            return Err(Intake::TooLarge(format!(
                "has {nodes} nodes nested {depth} deep; a formula has at most {MAX_FORMULA_NODES} nodes nested {MAX_FORMULA_DEPTH} deep"
            )));
        }
        let mut names = BTreeSet::new();
        collect(&ast, &mut names);
        Ok(Self {
            text: text.to_owned(),
            ast,
            names: names.into_iter().collect(),
        })
    }

    /// The law graph of the formula, with name `i` bound to field `i + 1` of
    /// the record it is decided on. `declarations` only satisfies the
    /// compiler: every name is bound, so none is resolved against them.
    fn graph<'d>(&self, declarations: &'d Declarations) -> Result<(LawGraph<'d>, Ref), String> {
        let mut graph = LawGraph::new(declarations);
        for (index, name) in self.names.iter().enumerate() {
            let field = field_id(index)?;
            let value = graph.observe(Observation::Post(field), Kind::Int, None, Atom::I128(0))?;
            graph.bind(name.clone(), value);
        }
        let value = graph.compile(&self.ast)?;
        Ok((graph, value))
    }

    /// Checks that the formula compiles as a predicate.
    ///
    /// # Errors
    /// The law compiler's refusal.
    pub(crate) fn check_predicate(&self, declarations: &Declarations) -> Result<(), String> {
        let (mut graph, value) = self.graph(declarations)?;
        graph.boolean(value).map(|_| ())
    }

    /// The binding's decision-program nodes and root, every name read as an
    /// integer input in name order.
    fn scalar(&self) -> Result<(Vec<ScalarOp>, usize), String> {
        // Every name is an integer input: a request field or state field of
        // any type, a boolean's 0 or 1 included, is read as the number it
        // is. The compiler asks only each input type's kind, so the inputs
        // name the one integer type of a synthetic declaration, whatever the
        // component declares.
        let integer = integer_declarations();
        let inputs: Vec<Input> = self
            .names
            .iter()
            .map(|name| Input {
                name: name.clone(),
                source: Source::Command,
                field: None,
                type_id: SYNTHETIC_INTEGER,
            })
            .collect();
        let mut graph = ScalarGraph::new(&integer, &inputs);
        let value = graph.compile(&self.ast)?;
        if graph.table.nodes.iter().any(|node| {
            matches!(
                node,
                ScalarOp::Mul(..)
                    | ScalarOp::Div(..)
                    | ScalarOp::MulDiv(..)
                    | ScalarOp::SignedMulDivFloor(..)
                    | ScalarOp::ProductsGe(..)
            )
        }) {
            return Err("composed-domain bindings do not support multiplication, division or exact product comparisons".to_owned());
        }
        let root = graph.numeric(value)?.index;
        Ok((graph.table.nodes, root))
    }

    /// Checks that the formula compiles as a binding: a rule expression with
    /// an integer value.
    ///
    /// # Errors
    /// The decision-program compiler's refusal.
    pub(crate) fn check_value(&self) -> Result<(), String> {
        self.scalar().map(|_| ())
    }

    /// The predicate's table over `axes`, one axis for each name in order.
    ///
    /// # Errors
    /// The law compiler's refusal.
    pub(crate) fn truth_table(
        &self,
        declarations: &Declarations,
        axes: Vec<Axis>,
        evaluations: &Counter<u64>,
    ) -> Result<TruthTable, String> {
        let (mut graph, value) = self.graph(declarations)?;
        let root = graph.boolean(value)?.index;
        let predicate = Predicate::from_state_graph(graph.table.nodes, root, self.fields());
        let mut cells = Vec::with_capacity(product(&axes));
        let values: Vec<Vec<i128>> = axes.iter().map(|axis| axis.values.clone()).collect();
        for_each_tuple::<()>(&values, |tuple| {
            evaluations.set(evaluations.get() + 1);
            cells.push(match predicate.on_state(&record(tuple)) {
                Truth::Holds => Cell::True,
                Truth::Fails => Cell::False,
                Truth::OutOfScope | Truth::Undefined(_) => Cell::Undefined,
            });
            Ok(())
        })
        .ok();
        Ok(TruthTable {
            index: Index::new(axes),
            cells,
        })
    }

    /// The binding's table over `axes`, against the bound field's domain:
    /// one program evaluation per cell.
    ///
    /// # Errors
    /// The compiler's or the library's refusal of the program.
    pub(crate) fn value_table(
        &self,
        axes: Vec<Axis>,
        target: &FieldDomain,
        evaluations: &Counter<u64>,
    ) -> Result<ValueTable, String> {
        let (nodes, root) = self.scalar()?;
        let index = |at: usize| {
            u16::try_from(at).map_err(|_| "a binding has too many program nodes".to_owned())
        };
        let ops = nodes
            .iter()
            .map(|node| {
                Ok(match *node {
                    ScalarOp::Input(at) => ProgramOp::Input(index(at)?),
                    ScalarOp::Int(value) => ProgramOp::Int(value),
                    ScalarOp::Bool(value) => ProgramOp::Bool(value),
                    ScalarOp::Add(a, b) => ProgramOp::Add(index(a)?, index(b)?),
                    ScalarOp::Sub(a, b) => ProgramOp::Sub(index(a)?, index(b)?),
                    ScalarOp::Mul(..)
                    | ScalarOp::Div(..)
                    | ScalarOp::MulDiv(..)
                    | ScalarOp::SignedMulDivFloor(..)
                    | ScalarOp::ProductsGe(..) => {
                        return Err(
                            "composed-domain bindings do not support arithmetic extension opcodes"
                                .to_owned(),
                        );
                    }
                    ScalarOp::Eq(a, b) => ProgramOp::Eq(index(a)?, index(b)?),
                    ScalarOp::Lt(a, b) => ProgramOp::Lt(index(a)?, index(b)?),
                    ScalarOp::And(a, b) => ProgramOp::And(index(a)?, index(b)?),
                    ScalarOp::Not(a) => ProgramOp::Not(index(a)?),
                    ScalarOp::Select(c, a, b) => ProgramOp::Select(index(c)?, index(a)?, index(b)?),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let inputs = axes
            .iter()
            .map(|axis| {
                let min = axis.values.iter().copied().min().unwrap_or(0);
                let max = axis.values.iter().copied().max().unwrap_or(0);
                ProgramDomain::Int { min, max }
            })
            .collect();
        let program = Program::try_new(
            inputs,
            vec![ProgramDomain::Int {
                min: i128::MIN,
                max: i128::MAX,
            }],
            ops,
            vec![index(root)?],
        )
        .map_err(|error| format!("the library refuses the binding's program: {error}"))?;
        let mut cells = Vec::with_capacity(product(&axes));
        let values: Vec<Vec<i128>> = axes.iter().map(|axis| axis.values.clone()).collect();
        for_each_tuple::<()>(&values, |tuple| {
            evaluations.set(evaluations.get() + 1);
            cells.push(
                match program
                    .evaluate(tuple)
                    .ok()
                    .and_then(|out| out.first().copied())
                {
                    None => Delivered::Undefined,
                    Some(value) if target.contains(value) => Delivered::Value(value),
                    Some(value) => Delivered::Outside(match target.interval() {
                        Some((min, _)) if value < min => Side::Below,
                        Some(_) => Side::Above,
                        None => Side::Unordered,
                    }),
                },
            );
            Ok(())
        })
        .ok();
        Ok(ValueTable {
            index: Index::new(axes),
            cells,
        })
    }

    fn fields(&self) -> BTreeSet<u16> {
        (0..self.names.len())
            .filter_map(|index| field_id(index).ok())
            .collect()
    }
}

/// The type ID of the synthetic declaration's one integer type.
const SYNTHETIC_INTEGER: u32 = 1;

/// A declaration of one integer type spanning 64 bits, which binding inputs
/// name; nothing else of a contract is declared.
fn integer_declarations() -> Declarations {
    let integer = Type {
        name: "Integer".to_owned(),
        form: TypeForm::Leaf(Leaf::I128 {
            min: i128::from(i128::MIN),
            max: i128::from(i128::MAX),
        }),
    };
    Declarations {
        profile: String::new(),
        types: BTreeMap::from([(SYNTHETIC_INTEGER, integer)]),
        channels: Vec::new(),
        reasons: Vec::new(),
        laws: Vec::new(),
        notes: Notes::default(),
    }
}

fn field_id(index: usize) -> Result<u16, String> {
    u16::try_from(index + 1).map_err(|_| "a formula reads too many names".to_owned())
}

/// The record a formula is decided on: name `i`'s value as field `i + 1`.
fn record(tuple: &[i128]) -> Vec<c::Field<'static>> {
    tuple
        .iter()
        .enumerate()
        .map(|(index, value)| c::Field {
            id: u16::try_from(index + 1).unwrap_or(u16::MAX),
            value: c::Atom::I128(i128::from(*value)),
        })
        .collect()
}

/// The children of one expression node.
fn children(ast: &Ast) -> Vec<&Ast> {
    match ast {
        Ast::Name(_) | Ast::Bool(_) | Ast::Int(_) | Ast::DecisionReason => Vec::new(),
        Ast::Not(inner) | Ast::Neg(inner) => vec![inner],
        Ast::Binary(_, left, right) | Ast::Div(_, left, right) => vec![left, right],
        Ast::MulDiv(_, a, b, d) | Ast::SignedMulDivFloor(a, b, d) => vec![a, b, d],
        Ast::ProductsGe(arguments) => arguments.iter().collect(),
        Ast::Choose(condition, then, otherwise) => vec![condition, then, otherwise],
    }
}

/// The number of nodes and the depth of a tree, walked without recursion.
fn shape(ast: &Ast) -> (usize, usize) {
    let mut nodes = 0;
    let mut deepest = 0;
    let mut stack = vec![(ast, 1usize)];
    while let Some((node, depth)) = stack.pop() {
        nodes += 1;
        deepest = deepest.max(depth);
        stack.extend(children(node).into_iter().map(|child| (child, depth + 1)));
    }
    (nodes, deepest)
}

/// Every name a tree reads, walked without recursion.
fn collect(ast: &Ast, names: &mut BTreeSet<String>) {
    let mut stack = vec![ast];
    while let Some(node) = stack.pop() {
        if let Ast::Name(name) = node {
            names.insert(name.clone());
        }
        stack.extend(children(node));
    }
}

/// The values of one name, in enumeration order.
#[derive(Clone, Debug)]
pub(crate) struct Axis {
    pub(crate) values: Vec<i128>,
}

/// Mixed-radix positions of tuples over axes, the last axis fastest, as
/// [`for_each_tuple`] enumerates them.
#[derive(Clone, Debug)]
pub(crate) struct Index {
    digits: Vec<BTreeMap<i128, usize>>,
    strides: Vec<usize>,
}

impl Index {
    pub(crate) fn new(axes: Vec<Axis>) -> Self {
        let mut strides = vec![1usize; axes.len()];
        for position in (0..axes.len().saturating_sub(1)).rev() {
            strides[position] =
                strides[position + 1].saturating_mul(axes[position + 1].values.len());
        }
        Self {
            digits: axes
                .into_iter()
                .map(|axis| {
                    axis.values
                        .into_iter()
                        .enumerate()
                        .map(|(digit, value)| (value, digit))
                        .collect()
                })
                .collect(),
            strides,
        }
    }

    /// The position of a tuple; `None` when a value is not on its axis.
    pub(crate) fn of(&self, tuple: &[i128]) -> Option<usize> {
        let mut at = 0usize;
        for ((digits, stride), value) in self.digits.iter().zip(&self.strides).zip(tuple) {
            at += digits.get(value)? * stride;
        }
        Some(at)
    }
}

/// The number of tuples over `axes`, saturating.
pub(crate) fn product(axes: &[Axis]) -> usize {
    axes.iter()
        .fold(1usize, |size, axis| size.saturating_mul(axis.values.len()))
}

/// A predicate's value on one tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cell {
    True,
    False,
    Undefined,
}

/// Where a binding's value lies against the bound field's domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    Below,
    Above,
    /// Outside a domain of booleans or variants.
    Unordered,
}

/// A binding's value on one tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Delivered {
    Value(i128),
    Outside(Side),
    Undefined,
}

/// A predicate's complete table.
#[derive(Clone, Debug)]
pub(crate) struct TruthTable {
    index: Index,
    cells: Vec<Cell>,
}

impl TruthTable {
    /// The value on a tuple of the axes' values; a tuple off the axes has
    /// none.
    pub(crate) fn cell(&self, tuple: &[i128]) -> Cell {
        self.index
            .of(tuple)
            .and_then(|at| self.cells.get(at).copied())
            .unwrap_or(Cell::Undefined)
    }

    /// Whether the predicate holds: no value counts as false.
    pub(crate) fn holds(&self, tuple: &[i128]) -> bool {
        self.cell(tuple) == Cell::True
    }
}

/// A binding's complete table.
#[derive(Clone, Debug)]
pub(crate) struct ValueTable {
    index: Index,
    cells: Vec<Delivered>,
}

impl ValueTable {
    pub(crate) fn value(&self, tuple: &[i128]) -> Delivered {
        self.index
            .of(tuple)
            .and_then(|at| self.cells.get(at).copied())
            .unwrap_or(Delivered::Undefined)
    }
}
