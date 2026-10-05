//! Decision-rule expressions and the scoped `.zeno` law formulas, as one tree.
//!
//! Rules use a small infix language: `->`, `||`, `&&`, comparisons, `+`, `-`,
//! `*`, prefix `!` and `-`, `choose(c, a, b)`, `div_floor(a, b)`,
//! `div_ceil(a, b)`, integers, `true`, `false` and dotted names. Binary
//! operators associate left except `->`. Law formulas come from the
//! `zeno-fcis-spec` parser and convert to the same tree.

use std::collections::BTreeMap;

use zeno_fcis_spec::{CompareOp, DivisionMode, ProjectionRoot, RelExpr, ValueExpr};

/// Binary operators, from loosest to tightest binding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Binary {
    Implies,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
}

impl Binary {
    fn precedence(self) -> u8 {
        match self {
            Self::Implies => 1,
            Self::Or => 2,
            Self::And => 3,
            Self::Eq | Self::Ne => 4,
            Self::Lt | Self::Le | Self::Gt | Self::Ge => 5,
            Self::Add | Self::Sub => 6,
            Self::Mul => 7,
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "->" => Self::Implies,
            "||" => Self::Or,
            "&&" => Self::And,
            "==" => Self::Eq,
            "!=" => Self::Ne,
            "<" => Self::Lt,
            "<=" => Self::Le,
            ">" => Self::Gt,
            ">=" => Self::Ge,
            "+" => Self::Add,
            "-" => Self::Sub,
            "*" => Self::Mul,
            _ => return None,
        })
    }
}

/// Rounding of `div_floor` and `div_ceil`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Rounding {
    Floor,
    Ceil,
}

/// An expression tree. Structural equality decides which program outputs
/// are shared.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Ast {
    Bool(bool),
    Int(i128),
    Name(String),
    Not(Box<Ast>),
    Neg(Box<Ast>),
    Binary(Binary, Box<Ast>, Box<Ast>),
    Choose(Box<Ast>, Box<Ast>, Box<Ast>),
    Div(Rounding, Box<Ast>, Box<Ast>),
}

const MAX_DEPTH: usize = 64;

/// Parses one rule expression.
pub(super) fn parse(text: &str) -> Result<Ast, String> {
    let tokens = tokenize(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        index: 0,
    };
    let ast = parser.expression(0, 0)?;
    match tokens.get(parser.index) {
        None => Ok(ast),
        Some(token) => Err(format!("unexpected `{token}` in `{text}`")),
    }
}

fn tokenize(text: &str) -> Result<Vec<&str>, String> {
    let mut tokens = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let bytes = rest.as_bytes();
        let length = if ["->", "&&", "||", "==", "!=", "<=", ">="]
            .iter()
            .any(|operator| rest.starts_with(operator))
        {
            2
        } else if b"!<>()+*,-".contains(&bytes[0]) {
            1
        } else if bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' {
            bytes
                .iter()
                .position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.'))
                .unwrap_or(bytes.len())
        } else if bytes[0].is_ascii_digit() {
            bytes
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .unwrap_or(bytes.len())
        } else {
            return Err(format!("unsupported text `{rest}` in `{text}`"));
        };
        tokens.push(&rest[..length]);
        rest = rest[length..].trim_start();
    }
    Ok(tokens)
}

struct Parser<'t> {
    tokens: &'t [&'t str],
    index: usize,
}

impl<'t> Parser<'t> {
    fn next(&mut self) -> Result<&'t str, String> {
        let token = *self
            .tokens
            .get(self.index)
            .ok_or_else(|| "expression ends early".to_owned())?;
        self.index += 1;
        Ok(token)
    }

    fn expect(&mut self, wanted: &str) -> Result<(), String> {
        let token = self.next()?;
        if token == wanted {
            Ok(())
        } else {
            Err(format!("expected `{wanted}`, found `{token}`"))
        }
    }

    /// Precedence climbing: operators binding at least as tightly as `level`.
    fn expression(&mut self, level: u8, depth: usize) -> Result<Ast, String> {
        if depth > MAX_DEPTH {
            return Err("expression nests too deeply".to_owned());
        }
        let token = self.next()?;
        let mut left = match token {
            "(" => {
                let inner = self.expression(0, depth + 1)?;
                self.expect(")")?;
                inner
            }
            "!" => Ast::Not(Box::new(self.expression(8, depth + 1)?)),
            "-" => Ast::Neg(Box::new(self.expression(8, depth + 1)?)),
            "true" => Ast::Bool(true),
            "false" => Ast::Bool(false),
            _ if token.as_bytes()[0].is_ascii_digit() => Ast::Int(
                token
                    .parse()
                    .map_err(|_| format!("integer `{token}` is out of range"))?,
            ),
            _ if self.tokens.get(self.index) == Some(&"(") => {
                let name = token.to_owned();
                self.index += 1;
                let mut arguments = vec![self.expression(0, depth + 1)?];
                while self.tokens.get(self.index) == Some(&",") {
                    self.index += 1;
                    arguments.push(self.expression(0, depth + 1)?);
                }
                self.expect(")")?;
                call(&name, arguments)?
            }
            _ if token.as_bytes()[0].is_ascii_alphabetic() || token.starts_with('_') => {
                Ast::Name(token.to_owned())
            }
            _ => return Err(format!("unexpected `{token}`")),
        };
        while let Some(operator) = self
            .tokens
            .get(self.index)
            .and_then(|token| Binary::from_token(token))
            .filter(|operator| operator.precedence() >= level)
        {
            self.index += 1;
            // `->` is right associative; the others associate left.
            let next = operator.precedence() + u8::from(operator != Binary::Implies);
            let right = self.expression(next, depth + 1)?;
            left = Ast::Binary(operator, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}

fn call(name: &str, arguments: Vec<Ast>) -> Result<Ast, String> {
    let count = arguments.len();
    let mut arguments = arguments.into_iter().map(Box::new);
    let mut next = || arguments.next();
    match (name, count, next(), next(), next()) {
        ("choose", 3, Some(condition), Some(then), Some(otherwise)) => {
            Ok(Ast::Choose(condition, then, otherwise))
        }
        ("div_floor", 2, Some(left), Some(right), None) => {
            Ok(Ast::Div(Rounding::Floor, left, right))
        }
        ("div_ceil", 2, Some(left), Some(right), None) => Ok(Ast::Div(Rounding::Ceil, left, right)),
        _ => Err(format!(
            "`{name}` with {count} arguments is not choose/3, div_floor/2 or div_ceil/2"
        )),
    }
}

/// Replaces rule variables by their definitions; a variable may not refer to
/// itself, directly or through others.
pub(super) fn expand(ast: &Ast, variables: &BTreeMap<String, Ast>) -> Result<Ast, String> {
    expand_with(ast, variables, &mut Vec::new())
}

fn expand_with<'v>(
    ast: &Ast,
    variables: &'v BTreeMap<String, Ast>,
    stack: &mut Vec<&'v str>,
) -> Result<Ast, String> {
    let recurse =
        |ast: &Ast, stack: &mut Vec<&'v str>| expand_with(ast, variables, stack).map(Box::new);
    Ok(match ast {
        Ast::Name(name) => match variables.get_key_value(name) {
            Some((key, definition)) => {
                if stack.contains(&key.as_str()) {
                    return Err(format!("variable `{name}` refers to itself"));
                }
                stack.push(key);
                let expanded = expand_with(definition, variables, stack)?;
                stack.pop();
                expanded
            }
            None => ast.clone(),
        },
        Ast::Bool(_) | Ast::Int(_) => ast.clone(),
        Ast::Not(inner) => Ast::Not(recurse(inner, stack)?),
        Ast::Neg(inner) => Ast::Neg(recurse(inner, stack)?),
        Ast::Binary(operator, left, right) => {
            Ast::Binary(*operator, recurse(left, stack)?, recurse(right, stack)?)
        }
        Ast::Choose(condition, then, otherwise) => Ast::Choose(
            recurse(condition, stack)?,
            recurse(then, stack)?,
            recurse(otherwise, stack)?,
        ),
        Ast::Div(rounding, left, right) => {
            Ast::Div(*rounding, recurse(left, stack)?, recurse(right, stack)?)
        }
    })
}

/// Converts a `.zeno` law formula. Projections become dotted names such as
/// `post.100.110`; quantifiers, sums, named predicates, free variables and
/// exact division have no contract form.
pub(super) fn law(formula: &RelExpr) -> Result<Ast, String> {
    let binary = |operator, left: &RelExpr, right: &RelExpr| {
        Ok(Ast::Binary(
            operator,
            Box::new(law(left)?),
            Box::new(law(right)?),
        ))
    };
    match formula {
        RelExpr::Bool(value) => Ok(Ast::Bool(*value)),
        RelExpr::Not(inner) => Ok(Ast::Not(Box::new(law(inner)?))),
        RelExpr::And(left, right) => binary(Binary::And, left, right),
        RelExpr::Or(left, right) => binary(Binary::Or, left, right),
        RelExpr::Implies(left, right) => binary(Binary::Implies, left, right),
        RelExpr::Compare(operator, left, right) => {
            let operator = match operator {
                CompareOp::Eq => Binary::Eq,
                CompareOp::NotEq => Binary::Ne,
                CompareOp::Less => Binary::Lt,
                CompareOp::LessEq => Binary::Le,
                CompareOp::Greater => Binary::Gt,
                CompareOp::GreaterEq => Binary::Ge,
            };
            Ok(Ast::Binary(
                operator,
                Box::new(value(left)?),
                Box::new(value(right)?),
            ))
        }
        RelExpr::Predicate { name, .. } => Err(format!("named predicate `{}`", name.as_str())),
        RelExpr::ForAll { .. } | RelExpr::Exists { .. } => Err("quantifier".to_owned()),
    }
}

fn value(expression: &ValueExpr) -> Result<Ast, String> {
    let binary = |operator, left: &ValueExpr, right: &ValueExpr| {
        Ok(Ast::Binary(
            operator,
            Box::new(value(left)?),
            Box::new(value(right)?),
        ))
    };
    match expression {
        ValueExpr::Int(number) => Ok(Ast::Int(*number)),
        ValueExpr::Projection(path) => {
            let root = match path.root() {
                ProjectionRoot::Pre => "pre",
                ProjectionRoot::Post => "post",
                ProjectionRoot::Command => "command",
                ProjectionRoot::Context => "context",
                ProjectionRoot::Effects | ProjectionRoot::Outbox | ProjectionRoot::Events => {
                    return Err("effect, outbox and event projections".to_owned());
                }
            };
            let mut name = root.to_owned();
            for segment in path.segments() {
                name.push('.');
                name.push_str(&segment.get().to_string());
            }
            Ok(Ast::Name(name))
        }
        ValueExpr::Add(left, right) => binary(Binary::Add, left, right),
        ValueExpr::Sub(left, right) => binary(Binary::Sub, left, right),
        ValueExpr::Mul(left, right) => binary(Binary::Mul, left, right),
        ValueExpr::Div(mode, left, right) => {
            let rounding = match mode {
                DivisionMode::Floor => Rounding::Floor,
                DivisionMode::Ceil => Rounding::Ceil,
                DivisionMode::Exact => return Err("div_exact".to_owned()),
            };
            Ok(Ast::Div(
                rounding,
                Box::new(value(left)?),
                Box::new(value(right)?),
            ))
        }
        ValueExpr::Var(name) => Err(format!("free variable `{}`", name.as_str())),
        ValueExpr::Sum { .. } => Err("sum".to_owned()),
    }
}
