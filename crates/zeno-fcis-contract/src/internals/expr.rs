//! Decision-rule expressions and the scoped `.zeno` law formulas, as one tree.
//!
//! Rules use a small infix language: `->`, `||`, `&&`, comparisons, `+`, `-`,
//! `*`, prefix `!` and `-`, `choose(c, a, b)`, `div_floor(a, b)`,
//! `div_ceil(a, b)`, fused `mul_div_floor/ceil(a,b,d)`, `products_ge` with four
//! or six arguments, integers, `true`, `false` and dotted names. Binary
//! operators associate left except `->`. Law formulas come from the
//! `zeno-fcis-spec` parser and convert to the same tree.

#[path = "table_expr.rs"]
mod table_expr;
pub(super) use table_expr::TableDeclarations;

use std::collections::BTreeMap;

use zeno_fcis_spec::{CompareOp, DivisionMode, ProjectionRoot, RelExpr, ValueExpr};

/// Binary operators, from loosest to tightest binding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Binary {
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
        OPERATORS
            .iter()
            .find(|(written, _)| *written == token)
            .map(|(_, operator)| *operator)
    }
}

/// Every binary operator as written, from loosest to tightest binding;
/// `docs/CONTRACT_RULES.md` documents each.
pub const OPERATORS: [(&str, Binary); 12] = [
    ("->", Binary::Implies),
    ("||", Binary::Or),
    ("&&", Binary::And),
    ("==", Binary::Eq),
    ("!=", Binary::Ne),
    ("<", Binary::Lt),
    ("<=", Binary::Le),
    (">", Binary::Gt),
    (">=", Binary::Ge),
    ("+", Binary::Add),
    ("-", Binary::Sub),
    ("*", Binary::Mul),
];

/// Every function with its number of arguments.
pub const FUNCTIONS: [(&str, usize); 16] = [
    ("choose", 3),
    ("u128", 1),
    ("to_i128", 1),
    ("to_u128", 1),
    ("div_floor", 2),
    ("div_ceil", 2),
    ("mul_div_floor", 3),
    ("signed_mul_div_floor", 3),
    ("mul_div_ceil", 3),
    ("products_ge", 4),
    ("products_ge", 6),
    ("mul_div_floor", 4),
    ("mul_div_ceil", 4),
    ("signed_mul_div_floor", 4),
    ("repeat_mul_div_floor", 4),
    ("repeat_signed_mul_div_floor", 4),
];

/// Rounding of `div_floor` and `div_ceil`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Rounding {
    Floor,
    Ceil,
}

/// An expression tree. Structural equality decides which program outputs
/// are shared.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Ast {
    Bool(bool),
    Int(i128),
    U128(u128),
    ToI128(Box<Ast>),
    ToU128(Box<Ast>),
    /// Candidate reason, available only to rejection/failure laws.
    DecisionReason,
    Name(String),
    Not(Box<Ast>),
    Neg(Box<Ast>),
    Binary(Binary, Box<Ast>, Box<Ast>),
    Choose(Box<Ast>, Box<Ast>, Box<Ast>),
    Div(Rounding, Box<Ast>, Box<Ast>),
    /// Explicit fused product/division, distinct from Div(Mul(..), ..).
    MulDiv(Rounding, Box<Ast>, Box<Ast>, Box<Ast>),
    SignedMulDivFloor(Box<Ast>, Box<Ast>, Box<Ast>),
    ScaledMulDiv(Rounding, bool, Box<Ast>, Box<Ast>, Box<Ast>, u8),
    RepeatMulDiv(bool, Box<Ast>, Box<Ast>, Box<Ast>, u8),
    /// Four arguments compare two-factor products; six compare three factors.
    ProductsGe(Box<[Ast]>),
}

const MAX_DEPTH: usize = 64;

/// Parses one rule expression.
pub fn parse(text: &str) -> Result<Ast, String> {
    parse_in(text, None)
}

/// Parses one expression against project table declarations and returns the
/// owned scalar expansion. This is preparation, not decision authority.
pub fn parse_for_project(project: &str, text: &str) -> Result<Ast, String> {
    if let Ok(ast) = parse(text) {
        return Ok(ast);
    }
    parse_tables(text, &TableDeclarations::read(project)?)
}

pub(super) fn parse_tables(text: &str, tables: &TableDeclarations) -> Result<Ast, String> {
    parse_in(text, Some(tables))
}

fn parse_in(text: &str, tables: Option<&TableDeclarations>) -> Result<Ast, String> {
    let tokens = tokenize(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        index: 0,
        tables,
        bound: Vec::new(),
        expanded: 0,
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
        } else if b"!<>()+*,-[]{}".contains(&bytes[0]) {
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
    tables: Option<&'t TableDeclarations>,
    bound: Vec<(&'t str, i128)>,
    expanded: usize,
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
        self.expansion_work(1)?;
        if depth > MAX_DEPTH {
            return Err("expression nests too deeply".to_owned());
        }
        let token = self.next()?;
        let mut left = match token {
            "forall" | "exists" | "sum"
                if self.tables.is_some() && self.tokens.get(self.index) != Some(&"(") =>
            {
                self.table_form(token, depth + 1)?
            }
            "(" => {
                let inner = self.expression(0, depth + 1)?;
                self.expect(")")?;
                inner
            }
            "!" => Ast::Not(Box::new(self.expression(8, depth + 1)?)),
            "-" => {
                // The minimum has no positive i128 magnitude. Preserve the
                // existing Neg tree for every other expression and literal.
                if self
                    .tokens
                    .get(self.index)
                    .is_some_and(|digits| digits.parse::<u128>() == Ok(1u128 << 127))
                {
                    self.index += 1;
                    Ast::Int(i128::MIN)
                } else {
                    Ast::Neg(Box::new(self.expression(8, depth + 1)?))
                }
            }
            "true" => Ast::Bool(true),
            "false" => Ast::Bool(false),
            "u128" => {
                self.expect("(")?;
                let digits = self.next()?;
                if digits.len() > 1 && digits.starts_with('0') {
                    return Err("noncanonical U128 literal".into());
                }
                let n = digits
                    .parse::<u128>()
                    .map_err(|_| "U128 literal out of range")?;
                self.expect(")")?;
                Ast::U128(n)
            }
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
                self.table_name(token)?
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
    if count == 1 && matches!(name, "to_i128" | "to_u128") {
        let v = Box::new(arguments.into_iter().next().ok_or("missing cast operand")?);
        return Ok(if name == "to_i128" {
            Ast::ToI128(v)
        } else {
            Ast::ToU128(v)
        });
    }
    if count == 4
        && matches!(
            name,
            "mul_div_floor"
                | "mul_div_ceil"
                | "signed_mul_div_floor"
                | "repeat_mul_div_floor"
                | "repeat_signed_mul_div_floor"
        )
    {
        let mut args = arguments.into_iter();
        let a = Box::new(args.next().ok_or("missing a")?);
        let b = Box::new(args.next().ok_or("missing b")?);
        let d = Box::new(args.next().ok_or("missing d")?);
        let last = args.next().ok_or("missing literal")?;
        let signed = name.contains("signed");
        return if name.starts_with("repeat_") {
            match last {
                Ast::Int(p) if (0..=16).contains(&p) => {
                    Ok(Ast::RepeatMulDiv(signed, a, b, d, p as u8))
                }
                _ => Err("repeat count must be a literal in 0..16".to_owned()),
            }
        } else {
            let scale = match last {
                Ast::Name(s) => s
                    .strip_prefix('e')
                    .and_then(|s| s.parse::<u8>().ok())
                    .filter(|p| *p <= 38),
                _ => None,
            };
            Ok(Ast::ScaledMulDiv(
                if name == "mul_div_ceil" {
                    Rounding::Ceil
                } else {
                    Rounding::Floor
                },
                signed,
                a,
                b,
                d,
                scale.ok_or("target scale must be e0..e38")?,
            ))
        };
    }
    if name == "products_ge" && (count == 4 || count == 6) {
        return Ok(Ast::ProductsGe(arguments.into_boxed_slice()));
    }
    let mut arguments = arguments.into_iter().map(Box::new);
    let mut next = || arguments.next();
    let call = match (name, count, next(), next(), next()) {
        ("signed_mul_div_floor", 3, Some(a), Some(b), Some(d)) => {
            Some(Ast::SignedMulDivFloor(a, b, d))
        }
        ("mul_div_floor", 3, Some(a), Some(b), Some(d)) => {
            Some(Ast::MulDiv(Rounding::Floor, a, b, d))
        }
        ("mul_div_ceil", 3, Some(a), Some(b), Some(d)) => {
            Some(Ast::MulDiv(Rounding::Ceil, a, b, d))
        }
        ("choose", 3, Some(condition), Some(then), Some(otherwise)) => {
            Some(Ast::Choose(condition, then, otherwise))
        }
        ("div_floor", 2, Some(left), Some(right), None) => {
            Some(Ast::Div(Rounding::Floor, left, right))
        }
        ("div_ceil", 2, Some(left), Some(right), None) => {
            Some(Ast::Div(Rounding::Ceil, left, right))
        }
        _ => None,
    };
    call.filter(|_| FUNCTIONS.contains(&(name, count)))
        .ok_or_else(|| {
            let known: Vec<String> = FUNCTIONS
                .iter()
                .map(|(function, arity)| format!("{function}/{arity}"))
                .collect();
            format!(
                "`{name}` with {count} arguments is not {}",
                known.join(", ")
            )
        })
}

/// The expression as rule text, with only the parentheses its operators
/// need: `parse` reads it back as the same tree, except that a negative
/// integer constant reads back as a negated one. DecisionReason is a law-only
/// value, read back through the `.zeno` law parser rather than rule syntax.
pub fn render(ast: &Ast) -> String {
    let mut text = String::new();
    write(ast, 0, &mut text);
    text
}

/// Writes `ast` where only operators binding at least as tightly as `level`
/// may stand without parentheses.
fn write(ast: &Ast, level: u8, out: &mut String) {
    let call = |name: &str, arguments: &[&Ast], out: &mut String| {
        out.push_str(name);
        out.push('(');
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            write(argument, 0, out);
        }
        out.push(')');
    };
    match ast {
        Ast::U128(v) => out.push_str(&format!("u128({v})")),
        Ast::ToI128(v) => call("to_i128", &[v], out),
        Ast::ToU128(v) => call("to_u128", &[v], out),
        Ast::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Ast::Int(value) => out.push_str(&value.to_string()),
        Ast::DecisionReason => out.push_str("decision_reason"),
        Ast::Name(name) => out.push_str(name),
        Ast::Not(inner) => {
            out.push('!');
            write(inner, 8, out);
        }
        Ast::Neg(inner) => {
            out.push('-');
            write(inner, 8, out);
        }
        Ast::Binary(operator, left, right) => {
            let precedence = operator.precedence();
            let enclosed = precedence < level;
            if enclosed {
                out.push('(');
            }
            // `->` is right associative; the others associate left.
            let (left_level, right_level) = if *operator == Binary::Implies {
                (precedence + 1, precedence)
            } else {
                (precedence, precedence + 1)
            };
            write(left, left_level, out);
            let token = OPERATORS
                .iter()
                .find(|(_, candidate)| candidate == operator)
                .map_or("?", |(token, _)| token);
            out.push(' ');
            out.push_str(token);
            out.push(' ');
            write(right, right_level, out);
            if enclosed {
                out.push(')');
            }
        }
        Ast::Choose(condition, then, otherwise) => {
            call("choose", &[condition, then, otherwise], out);
        }
        Ast::Div(Rounding::Floor, left, right) => call("div_floor", &[left, right], out),
        Ast::Div(Rounding::Ceil, left, right) => call("div_ceil", &[left, right], out),
        Ast::MulDiv(rounding, a, b, d) => call(
            match rounding {
                Rounding::Floor => "mul_div_floor",
                Rounding::Ceil => "mul_div_ceil",
            },
            &[a, b, d],
            out,
        ),
        Ast::SignedMulDivFloor(a, b, d) => call("signed_mul_div_floor", &[a, b, d], out),
        Ast::ScaledMulDiv(r, signed, a, b, d, scale) => {
            let name = if *signed {
                "signed_mul_div_floor"
            } else if *r == Rounding::Floor {
                "mul_div_floor"
            } else {
                "mul_div_ceil"
            };
            out.push_str(name);
            out.push('(');
            for operand in [a, b, d] {
                write(operand, 0, out);
                out.push_str(", ");
            }
            out.push_str(&format!("e{scale})"));
        }
        Ast::RepeatMulDiv(signed, a, b, d, count) => {
            out.push_str(if *signed {
                "repeat_signed_mul_div_floor("
            } else {
                "repeat_mul_div_floor("
            });
            for operand in [a, b, d] {
                write(operand, 0, out);
                out.push_str(", ");
            }
            out.push_str(&format!("{count})"));
        }
        Ast::ProductsGe(arguments) => {
            call("products_ge", &arguments.iter().collect::<Vec<_>>(), out)
        }
    }
}

/// Replaces rule variables by their definitions; a variable may not refer to
/// itself, directly or through others.
pub fn expand(ast: &Ast, variables: &BTreeMap<String, Ast>) -> Result<Ast, String> {
    expand_with_limits(
        ast,
        variables,
        uses_new_arithmetic(ast) || variables.values().any(uses_new_arithmetic),
    )
}

/// The authoring boundary opts into the new limits. Historical expansion
/// retains its original admission and cycle check, without a new work cap.
pub fn expand_with_limits(
    ast: &Ast,
    variables: &BTreeMap<String, Ast>,
    bounded: bool,
) -> Result<Ast, String> {
    expand_with(
        ast,
        variables,
        &mut Vec::new(),
        &mut bounded.then_some(65_536),
        0,
    )
}

/// Recognizes unsigned syntax without inspecting values through signed integers.
pub fn uses_unsigned(ast: &Ast) -> bool {
    match ast {
        Ast::U128(_) | Ast::ToI128(_) | Ast::ToU128(_) => true,
        Ast::Bool(_) | Ast::Int(_) | Ast::Name(_) | Ast::DecisionReason => false,
        Ast::Not(v) | Ast::Neg(v) => uses_unsigned(v),
        Ast::Binary(_, a, b) | Ast::Div(_, a, b) => uses_unsigned(a) || uses_unsigned(b),
        Ast::Choose(a, b, c)
        | Ast::MulDiv(_, a, b, c)
        | Ast::SignedMulDivFloor(a, b, c)
        | Ast::ScaledMulDiv(_, _, a, b, c, _)
        | Ast::RepeatMulDiv(_, a, b, c, _) => {
            uses_unsigned(a) || uses_unsigned(b) || uses_unsigned(c)
        }
        Ast::ProductsGe(args) => args.iter().any(uses_unsigned),
    }
}
/// Recognizes the new syntax before expansion, including in unused bindings.
pub fn uses_new_arithmetic(ast: &Ast) -> bool {
    match ast {
        Ast::ScaledMulDiv(..) | Ast::RepeatMulDiv(..) => true,
        Ast::U128(_) | Ast::ToI128(_) | Ast::ToU128(_) => true,
        Ast::Bool(_) | Ast::Int(_) | Ast::Name(_) | Ast::DecisionReason => false,
        Ast::Neg(a) | Ast::Not(a) => uses_new_arithmetic(a),
        Ast::Binary(_, a, b) | Ast::Div(_, a, b) => {
            uses_new_arithmetic(a) || uses_new_arithmetic(b)
        }
        Ast::Choose(a, b, c) | Ast::MulDiv(_, a, b, c) | Ast::SignedMulDivFloor(a, b, c) => {
            uses_new_arithmetic(a) || uses_new_arithmetic(b) || uses_new_arithmetic(c)
        }
        Ast::ProductsGe(arguments) => arguments.iter().any(uses_new_arithmetic),
    }
}

fn expand_with<'v>(
    ast: &Ast,
    variables: &'v BTreeMap<String, Ast>,
    stack: &mut Vec<&'v str>,
    fuel: &mut Option<usize>,
    depth: usize,
) -> Result<Ast, String> {
    if let Some(remaining) = fuel {
        *remaining = remaining
            .checked_sub(1)
            .ok_or("expanded expression exceeds admission work cap")?;
    }
    if fuel.is_some() && depth > 256 {
        return Err("expanded expression nests too deeply".to_owned());
    }
    let recurse = |ast: &Ast, stack: &mut Vec<&'v str>, fuel: &mut Option<usize>| {
        expand_with(ast, variables, stack, fuel, depth + 1).map(Box::new)
    };
    Ok(match ast {
        Ast::Name(name) => match variables.get_key_value(name) {
            Some((key, definition)) => {
                if stack.contains(&key.as_str()) {
                    return Err(format!("variable `{name}` refers to itself"));
                }
                stack.push(key);
                let expanded = expand_with(definition, variables, stack, fuel, depth + 1)?;
                stack.pop();
                expanded
            }
            None => ast.clone(),
        },
        Ast::Bool(_) | Ast::Int(_) | Ast::U128(_) | Ast::DecisionReason => ast.clone(),
        Ast::ToI128(v) => Ast::ToI128(recurse(v, stack, fuel)?),
        Ast::ToU128(v) => Ast::ToU128(recurse(v, stack, fuel)?),
        Ast::Not(inner) => Ast::Not(recurse(inner, stack, fuel)?),
        Ast::Neg(inner) => Ast::Neg(recurse(inner, stack, fuel)?),
        Ast::Binary(operator, left, right) => Ast::Binary(
            *operator,
            recurse(left, stack, fuel)?,
            recurse(right, stack, fuel)?,
        ),
        Ast::Choose(condition, then, otherwise) => Ast::Choose(
            recurse(condition, stack, fuel)?,
            recurse(then, stack, fuel)?,
            recurse(otherwise, stack, fuel)?,
        ),
        Ast::SignedMulDivFloor(a, b, d) => Ast::SignedMulDivFloor(
            recurse(a, stack, fuel)?,
            recurse(b, stack, fuel)?,
            recurse(d, stack, fuel)?,
        ),
        Ast::ScaledMulDiv(r, signed, a, b, d, scale) => Ast::ScaledMulDiv(
            *r,
            *signed,
            recurse(a, stack, fuel)?,
            recurse(b, stack, fuel)?,
            recurse(d, stack, fuel)?,
            *scale,
        ),
        Ast::RepeatMulDiv(signed, a, b, d, count) => Ast::RepeatMulDiv(
            *signed,
            recurse(a, stack, fuel)?,
            recurse(b, stack, fuel)?,
            recurse(d, stack, fuel)?,
            *count,
        ),
        Ast::MulDiv(rounding, a, b, d) => Ast::MulDiv(
            *rounding,
            recurse(a, stack, fuel)?,
            recurse(b, stack, fuel)?,
            recurse(d, stack, fuel)?,
        ),
        Ast::ProductsGe(arguments) => Ast::ProductsGe(
            arguments
                .iter()
                .map(|a| expand_with(a, variables, stack, fuel, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Ast::Div(rounding, left, right) => Ast::Div(
            *rounding,
            recurse(left, stack, fuel)?,
            recurse(right, stack, fuel)?,
        ),
    })
}

/// Converts a `.zeno` law formula. Projections become dotted names such as
/// `post.100.110`; quantifiers, sums, named predicates, free variables and
/// exact division have no contract form.
pub fn law(formula: &RelExpr) -> Result<Ast, String> {
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
        RelExpr::Predicate { name, arguments } if name.as_str() == "products_ge" => call(
            "products_ge",
            arguments.iter().map(value).collect::<Result<_, _>>()?,
        ),
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
        ValueExpr::Select(c, a, b) => Ok(Ast::Choose(
            Box::new(law(c)?),
            Box::new(value(a)?),
            Box::new(value(b)?),
        )),
        ValueExpr::U128(v) => Ok(Ast::U128(*v)),
        ValueExpr::ToI128(v) => Ok(Ast::ToI128(Box::new(value(v)?))),
        ValueExpr::ToU128(v) => Ok(Ast::ToU128(Box::new(value(v)?))),
        ValueExpr::Int(number) => Ok(Ast::Int(*number)),
        ValueExpr::Projection(path) => {
            let root = match path.root() {
                ProjectionRoot::Pre => "pre",
                ProjectionRoot::Post => "post",
                ProjectionRoot::Command => "command",
                ProjectionRoot::Context => "context",
                ProjectionRoot::Outbox => "outbox",
                ProjectionRoot::Effects | ProjectionRoot::Events => {
                    return Err("effect and event projections".to_owned());
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
        ValueExpr::ScaledMulDiv(mode, signed, a, b, d, scale) => Ok(Ast::ScaledMulDiv(
            match mode {
                DivisionMode::Floor => Rounding::Floor,
                DivisionMode::Ceil => Rounding::Ceil,
                DivisionMode::Exact => return Err("exact scaled division".to_owned()),
            },
            *signed,
            Box::new(value(a)?),
            Box::new(value(b)?),
            Box::new(value(d)?),
            *scale,
        )),
        ValueExpr::RepeatMulDiv(signed, a, b, d, count) => Ok(Ast::RepeatMulDiv(
            *signed,
            Box::new(value(a)?),
            Box::new(value(b)?),
            Box::new(value(d)?),
            *count,
        )),
        ValueExpr::SignedMulDivFloor(a, b, d) => Ok(Ast::SignedMulDivFloor(
            Box::new(value(a)?),
            Box::new(value(b)?),
            Box::new(value(d)?),
        )),
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
        ValueExpr::MulDiv(mode, a, b, d) => {
            let rounding = match mode {
                DivisionMode::Floor => Rounding::Floor,
                DivisionMode::Ceil => Rounding::Ceil,
                DivisionMode::Exact => return Err("fused exact division".to_owned()),
            };
            // Compile with the existing full-product graph operation. Do not
            // turn this into Ast::Div(Ast::Mul(..), ..).
            Ok(Ast::MulDiv(
                rounding,
                Box::new(value(a)?),
                Box::new(value(b)?),
                Box::new(value(d)?),
            ))
        }
        ValueExpr::Var(name) if name.as_str() == "decision_reason" => Ok(Ast::DecisionReason),
        ValueExpr::Var(name) => Err(format!("free variable `{}`", name.as_str())),
        ValueExpr::Sum { .. } => Err("sum".to_owned()),
    }
}

#[cfg(test)]
mod signed_literal_tests {
    use super::*;

    #[test]
    fn minimum_literal_parses_directly_and_keeps_other_negation_trees() {
        assert_eq!(parse(&i128::MIN.to_string()), Ok(Ast::Int(i128::MIN)));
        assert_eq!(parse(&i128::MAX.to_string()), Ok(Ast::Int(i128::MAX)));
        assert_eq!(parse("-1"), Ok(Ast::Neg(Box::new(Ast::Int(1)))));
        for text in [
            "170141183460469231731687303715884105728",
            "-170141183460469231731687303715884105729",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        let tree = Ast::Int(i128::MIN);
        assert_eq!(parse(&render(&tree)), Ok(tree));
    }
}
