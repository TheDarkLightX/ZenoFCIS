//! The loop expression language.
//!
//! One grammar serves every door. It is the contract policy-expression
//! grammar (the same operators and precedence, `c ? a : b` at the lowest
//! level, `!` and unary `-` binding to an atom) extended with:
//!
//! - `$C`: a family parameter, bound per instance (family door);
//! - `?c`: a constant hole of the position's kind;
//! - `?c[lo..hi]`: a constant hole with an explicit inclusive range;
//! - `?e`: an expression hole, filled by synthesis;
//! - `?{a | b}`: a choice hole, exactly one of the listed hole-free
//!   expressions;
//! - `keep`: a whole output only, the original program's output.
//!
//! Precedence, loosest first: `?:`, `->` (right), `||`, `&&`, `==` `!=`,
//! `<` `<=` `>` `>=`, `+` `-`, `*`. Names are input fields, outputs
//! (`out0`, ...) and declared variant names; `true` and `false` are 1 and 0.
//!
//! Text is bounded and fails closed: ASCII only, at most [`MAX_TEXT`] bytes
//! per expression, [`MAX_NODES`] nodes and [`MAX_HOLES`] holes. Unknown names
//! are refused when an expression is checked against a [`Vocabulary`].
//! [`print_symbolic`] renders canonical text with
//! `parse(print_symbolic(t)) == t`.
//!
//! Evaluation ([`evaluate`]) follows the finite program semantics the
//! expressions lower to: Booleans are 0 and 1, `!x` is `x == 0`, a select
//! tests `== 1`, and `x || y` is `x == 1 ? x : y`. It is the doors' own
//! untrusted evaluator; programs are judged by the transform checker.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Domain;

use crate::{Refusal, Result};

/// Longest expression text, in bytes.
pub const MAX_TEXT: usize = 8192;
/// Most nodes in one parsed expression.
pub const MAX_NODES: usize = 512;
/// Most nested parser descents (parentheses, `!`, unary minus,
/// conditionals, right-associative chains), counted before descending.
pub const MAX_DESCENT: usize = 192;
/// Deepest tree a parse may return; checked without recursion, before any
/// recursive pass reads it.
pub const MAX_DEPTH: usize = 128;
/// Most holes in one expression, or in all outputs of one sketch.
pub const MAX_HOLES: usize = 16;
/// Most alternatives of a choice hole.
pub const MAX_CHOICES: usize = 16;
/// Widest explicit constant-hole range (`hi - lo`).
pub const MAX_RANGE: i64 = 1 << 16;

/// Words that cannot be names: the controlled-English surface uses them.
pub const RESERVED: [&str; 34] = [
    "true",
    "false",
    "keep",
    "not",
    "and",
    "or",
    "is",
    "if",
    "then",
    "otherwise",
    "implies",
    "plus",
    "minus",
    "times",
    "less",
    "greater",
    "than",
    "at",
    "most",
    "least",
    "some",
    "constant",
    "expression",
    "one",
    "of",
    "from",
    "to",
    "parameter",
    "before",
    "after",
    "the",
    "original",
    "output",
    "where",
];

/// A binary operator.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Op {
    /// `->`
    Implies,
    /// `||`
    Or,
    /// `&&`
    And,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*` (parsed, never lowered)
    Mul,
}

impl Op {
    /// The operator's text.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Implies => "->",
            Self::Or => "||",
            Self::And => "&&",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
        }
    }

    fn from_symbol(text: &str) -> Option<Self> {
        Some(match text {
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

    /// Binding strength (higher binds tighter) and right associativity.
    #[must_use]
    pub const fn precedence(self) -> (u8, bool) {
        match self {
            Self::Implies => (1, true),
            Self::Or => (2, false),
            Self::And => (3, false),
            Self::Eq | Self::Ne => (4, false),
            Self::Lt | Self::Le | Self::Gt | Self::Ge => (5, false),
            Self::Add | Self::Sub => (6, false),
            Self::Mul => (7, false),
        }
    }

    /// `==`, `!=`, `<`, `<=`, `>`, `>=`.
    #[must_use]
    pub const fn is_compare(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge
        )
    }

    /// `&&`, `||`, `->`.
    #[must_use]
    pub const fn is_boolean(self) -> bool {
        matches!(self, Self::And | Self::Or | Self::Implies)
    }

    /// `+`, `-`, `*`.
    #[must_use]
    pub const fn is_arith(self) -> bool {
        matches!(self, Self::Add | Self::Sub | Self::Mul)
    }

    /// Every comparison, in a fixed order.
    pub const COMPARE: [Self; 6] = [Self::Eq, Self::Ne, Self::Lt, Self::Le, Self::Gt, Self::Ge];
}

/// What a hole may be filled with.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Hole {
    /// A constant, optionally within an explicit inclusive range.
    Const(Option<(i64, i64)>),
    /// Any expression (filled by synthesis).
    Expr,
    /// Exactly one of these hole-free alternatives.
    Choice(Vec<Expr>),
}

/// An expression. Holes carry their number in reading order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Expr {
    /// An integer literal (Booleans are 0 and 1).
    Int(i64),
    /// A name: an input field, an output or a variant.
    Var(String),
    /// A family parameter `$NAME`.
    Param(String),
    /// A hole.
    Hole(usize, Hole),
    /// The original program's output (a whole output only).
    Keep,
    /// `!a`
    Not(Box<Expr>),
    /// `a OP b`
    Bin(Op, Box<Expr>, Box<Expr>),
    /// `c ? a : b`
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// `a OP b`.
    #[must_use]
    pub fn bin(op: Op, a: Self, b: Self) -> Self {
        Self::Bin(op, Box::new(a), Box::new(b))
    }

    /// `c ? a : b`.
    #[must_use]
    pub fn select(c: Self, a: Self, b: Self) -> Self {
        Self::If(Box::new(c), Box::new(a), Box::new(b))
    }

    /// `!a`.
    #[must_use]
    pub fn negation(a: Self) -> Self {
        Self::Not(Box::new(a))
    }

    /// A name.
    #[must_use]
    pub fn var(name: &str) -> Self {
        Self::Var(name.to_string())
    }
}

/// The kind of a value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    /// Integers.
    Int,
    /// Booleans (0 and 1).
    Bool,
}

impl Kind {
    /// The kind of a declared domain; `None` for a domain the doors do not
    /// know.
    #[must_use]
    pub const fn of(domain: Domain) -> Option<Self> {
        match domain {
            Domain::Bool => Some(Self::Bool),
            Domain::Int { .. } => Some(Self::Int),
            _ => None,
        }
    }
}

// --- tokens ------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Num(String),
    Param(String),
    Hole(&'static str),
    Name(String),
    Op(&'static str),
    End,
}

const TWO: [&str; 8] = ["->", "&&", "||", "==", "!=", "<=", ">=", ".."];
const ONE: [&str; 13] = [
    "(", ")", "!", "<", ">", "+", "-", "*", "?", ":", "]", "|", "}",
];

fn word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    if text.len() > MAX_TEXT || !text.is_ascii() {
        return Err(Refusal::new("expression-text"));
    }
    let bytes = text.trim().as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    let word = |from: usize| {
        let mut end = from;
        while end < bytes.len() && word_byte(bytes[end]) {
            end += 1;
        }
        end
    };
    let text_of = |from: usize, to: usize| String::from_utf8_lossy(&bytes[from..to]).into_owned();
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if byte.is_ascii_digit() {
            let mut end = at;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            tokens.push(Token::Num(text_of(at, end)));
            at = end;
            continue;
        }
        if byte == b'$' && at + 1 < bytes.len() && word_start(bytes[at + 1]) {
            let end = word(at + 1);
            tokens.push(Token::Param(text_of(at + 1, end)));
            at = end;
            continue;
        }
        if byte == b'?' {
            let next = bytes.get(at + 1).copied();
            let after = bytes.get(at + 2).copied();
            let free = after.is_none_or(|b| !word_byte(b));
            let hole = match (next, after) {
                (Some(b'c'), Some(b'[')) => Some(("?c[", 3)),
                (Some(b'c'), _) if free => Some(("?c", 2)),
                (Some(b'e'), _) if free => Some(("?e", 2)),
                (Some(b'{'), _) => Some(("?{", 2)),
                _ => None,
            };
            if let Some((hole, width)) = hole {
                tokens.push(Token::Hole(hole));
                at += width;
                continue;
            }
        }
        if word_start(byte) {
            let mut end = word(at);
            if end + 1 < bytes.len() && bytes[end] == b'.' && word_start(bytes[end + 1]) {
                end = word(end + 1);
            }
            tokens.push(Token::Name(text_of(at, end)));
            at = end;
            continue;
        }
        if let Some(two) = TWO.iter().find(|op| bytes[at..].starts_with(op.as_bytes())) {
            tokens.push(Token::Op(two));
            at += 2;
            continue;
        }
        if let Some(one) = ONE.iter().find(|op| op.as_bytes()[0] == byte) {
            tokens.push(Token::Op(one));
            at += 1;
            continue;
        }
        let end = (at + 12).min(bytes.len());
        return Err(Refusal::new(format!(
            "expression-token:{}",
            text_of(at, end)
        )));
    }
    Ok(tokens)
}

/// What a parse admits besides plain expressions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Allow {
    /// Holes (`?c`, `?c[..]`, `?e`, `?{..}`).
    pub holes: bool,
    /// Family parameters (`$P`).
    pub params: bool,
    /// `keep` as a whole output.
    pub keep: bool,
}

impl Allow {
    /// Everything: holes, parameters and `keep`.
    pub const ALL: Self = Self {
        holes: true,
        params: true,
        keep: true,
    };
    /// Plain expressions only.
    pub const PLAIN: Self = Self {
        holes: false,
        params: false,
        keep: false,
    };
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_hole: usize,
    allow: Allow,
    /// Parser calls in progress.
    descent: usize,
}

impl Parser {
    /// Runs `step` one level deeper; past [`MAX_DESCENT`] the parse is
    /// refused before it descends, so deep input never exhausts the stack.
    fn nested<T>(&mut self, step: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.descent >= MAX_DESCENT {
            return Err(Refusal::new("expression-too-deeply-nested"));
        }
        self.descent += 1;
        let out = step(self);
        self.descent -= 1;
        out
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::End)
    }

    fn take(&mut self) -> Token {
        let token = self.peek().clone();
        self.pos += 1;
        token
    }

    fn accept(&mut self, value: &str) -> bool {
        let hit = match self.peek() {
            Token::Op(op) => *op == value,
            Token::Name(name) => name == value,
            _ => false,
        };
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn expect(&mut self, value: &str) -> Result<()> {
        if self.accept(value) {
            Ok(())
        } else {
            Err(Refusal::new(format!("expression-expected:{value}")))
        }
    }

    fn number(text: &str, negative: bool) -> Result<i64> {
        let magnitude: i128 = text
            .parse()
            .map_err(|_| Refusal::new("expression-integer-range"))?;
        let value = if negative { -magnitude } else { magnitude };
        i64::try_from(value).map_err(|_| Refusal::new("expression-integer-range"))
    }

    fn primary(&mut self) -> Result<Expr> {
        self.nested(Self::primary_body)
    }

    fn primary_body(&mut self) -> Result<Expr> {
        match self.take() {
            Token::Num(text) => Ok(Expr::Int(Self::number(&text, false)?)),
            Token::Param(name) => {
                if !self.allow.params {
                    return Err(Refusal::new("expression-parameter-not-allowed"));
                }
                Ok(Expr::Param(name))
            }
            Token::Hole(token) => {
                if !self.allow.holes {
                    return Err(Refusal::new("expression-hole-not-allowed"));
                }
                self.hole(token)
            }
            Token::Name(name) => {
                if name == "true" {
                    return Ok(Expr::Int(1));
                }
                if name == "false" {
                    return Ok(Expr::Int(0));
                }
                if name == "keep" {
                    if !self.allow.keep {
                        return Err(Refusal::new("expression-keep-only-as-a-whole-output"));
                    }
                    return Ok(Expr::Keep);
                }
                if RESERVED.contains(&name.as_str()) {
                    return Err(Refusal::new(format!("expression-reserved-word:{name}")));
                }
                if *self.peek() == Token::Op("(") {
                    return Err(Refusal::new(format!("expression-call-unsupported:{name}")));
                }
                Ok(Expr::Var(name))
            }
            Token::Op("(") => {
                let node = self.conditional()?;
                self.expect(")")?;
                Ok(node)
            }
            Token::Op("!") => Ok(Expr::negation(self.primary()?)),
            Token::Op("-") => {
                if let Token::Num(text) = self.peek().clone() {
                    self.pos += 1;
                    return Ok(Expr::Int(Self::number(&text, true)?));
                }
                Ok(Expr::bin(Op::Sub, Expr::Int(0), self.primary()?))
            }
            Token::Op(other) => Err(Refusal::new(format!("expression-primary:{other}"))),
            Token::End => Err(Refusal::new("expression-primary:end")),
        }
    }

    fn integer(&mut self) -> Result<i64> {
        let negative = self.accept("-");
        match self.take() {
            Token::Num(text) => Self::number(&text, negative),
            _ => Err(Refusal::new("hole-range-integer")),
        }
    }

    fn hole(&mut self, token: &str) -> Result<Expr> {
        let k = self.next_hole;
        self.next_hole += 1;
        if k >= MAX_HOLES {
            return Err(Refusal::new("too-many-holes"));
        }
        match token {
            "?c" => Ok(Expr::Hole(k, Hole::Const(None))),
            "?c[" => {
                let lo = self.integer()?;
                self.expect("..")?;
                let hi = self.integer()?;
                self.expect("]")?;
                if hi < lo || hi.checked_sub(lo).is_none_or(|width| width > MAX_RANGE) {
                    return Err(Refusal::new("hole-range"));
                }
                Ok(Expr::Hole(k, Hole::Const(Some((lo, hi)))))
            }
            "?e" => Ok(Expr::Hole(k, Hole::Expr)),
            _ => {
                let mut alternatives = vec![self.conditional()?];
                while self.accept("|") {
                    alternatives.push(self.conditional()?);
                }
                self.expect("}")?;
                if alternatives.len() < 2 || alternatives.len() > MAX_CHOICES {
                    return Err(Refusal::new("choice-hole-arity"));
                }
                if alternatives.iter().any(has_holes) {
                    return Err(Refusal::new("choice-alternatives-must-be-hole-free"));
                }
                Ok(Expr::Hole(k, Hole::Choice(alternatives)))
            }
        }
    }

    fn expression(&mut self, min_prec: u8) -> Result<Expr> {
        self.nested(|parser| parser.expression_body(min_prec))
    }

    fn expression_body(&mut self, min_prec: u8) -> Result<Expr> {
        let mut left = self.primary()?;
        loop {
            let op = match self.peek() {
                Token::Op(text) => Op::from_symbol(text),
                _ => None,
            };
            let Some(op) = op else {
                return Ok(left);
            };
            let (prec, right) = op.precedence();
            if prec < min_prec {
                return Ok(left);
            }
            self.pos += 1;
            let rhs = self.expression(if right { prec } else { prec + 1 })?;
            left = Expr::bin(op, left, rhs);
        }
    }

    fn conditional(&mut self) -> Result<Expr> {
        self.nested(Self::conditional_body)
    }

    fn conditional_body(&mut self) -> Result<Expr> {
        let test = self.expression(0)?;
        if self.accept("?") {
            let yes = self.conditional()?;
            self.expect(":")?;
            let no = self.conditional()?;
            return Ok(Expr::select(test, yes, no));
        }
        Ok(test)
    }
}

/// Parses one expression; holes are numbered from `first_hole` in reading
/// order.
pub fn parse(text: &str, first_hole: usize, allow: Allow) -> Result<Expr> {
    let mut parser = Parser {
        tokens: tokenize(text)?,
        pos: 0,
        next_hole: first_hole,
        allow,
        descent: 0,
    };
    let node = parser.conditional()?;
    if parser.pos != parser.tokens.len() {
        return Err(Refusal::new("expression-trailing"));
    }
    if depth(&node) > MAX_DEPTH {
        return Err(Refusal::new("expression-too-deep"));
    }
    if size(&node) > MAX_NODES {
        return Err(Refusal::new("expression-too-large"));
    }
    if node != Expr::Keep && contains_keep(&node) {
        return Err(Refusal::new("expression-keep-only-as-a-whole-output"));
    }
    Ok(node)
}

// --- structure ---------------------------------------------------------------

/// The tree's depth, computed without recursion; it stops counting past
/// [`MAX_DEPTH`].
#[must_use]
pub fn depth(node: &Expr) -> usize {
    let mut best = 0;
    let mut stack = vec![(node, 1usize)];
    while let Some((n, d)) = stack.pop() {
        best = best.max(d);
        if d > MAX_DEPTH {
            return d;
        }
        match n {
            Expr::Hole(_, Hole::Choice(alternatives)) => {
                stack.extend(alternatives.iter().map(|a| (a, d + 1)));
            }
            _ => stack.extend(children(n).into_iter().map(|c| (c, d + 1))),
        }
    }
    best
}

/// Nodes in the expression (a choice hole counts its alternatives).
#[must_use]
pub fn size(node: &Expr) -> usize {
    match node {
        Expr::Not(a) => 1 + size(a),
        Expr::Bin(_, a, b) => 1 + size(a) + size(b),
        Expr::If(c, a, b) => 1 + size(c) + size(a) + size(b),
        Expr::Hole(_, Hole::Choice(alternatives)) => {
            1 + alternatives.iter().map(size).sum::<usize>()
        }
        _ => 1,
    }
}

fn contains_keep(node: &Expr) -> bool {
    match node {
        Expr::Keep => true,
        Expr::Not(a) => contains_keep(a),
        Expr::Bin(_, a, b) => contains_keep(a) || contains_keep(b),
        Expr::If(c, a, b) => contains_keep(c) || contains_keep(a) || contains_keep(b),
        Expr::Hole(_, Hole::Choice(alternatives)) => alternatives.iter().any(contains_keep),
        _ => false,
    }
}

/// The children of a node (a choice hole is a leaf here).
#[must_use]
pub fn children(node: &Expr) -> Vec<&Expr> {
    match node {
        Expr::Not(a) => vec![a],
        Expr::Bin(_, a, b) => vec![a, b],
        Expr::If(c, a, b) => vec![c, a, b],
        _ => Vec::new(),
    }
}

/// Hole nodes in reading order (choice alternatives are hole-free).
#[must_use]
pub fn holes_of(node: &Expr) -> Vec<&Expr> {
    fn visit<'a>(node: &'a Expr, out: &mut Vec<&'a Expr>) {
        if let Expr::Hole(..) = node {
            out.push(node);
            return;
        }
        for child in children(node) {
            visit(child, out);
        }
    }
    let mut out = Vec::new();
    visit(node, &mut out);
    out
}

/// Whether the expression has a hole.
#[must_use]
pub fn has_holes(node: &Expr) -> bool {
    !holes_of(node).is_empty()
}

/// Every name the expression reads.
#[must_use]
pub fn names_of(node: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    visit_all(node, &mut |n| {
        if let Expr::Var(name) = n {
            out.insert(name.clone());
        }
    });
    out
}

/// Every family parameter the expression reads.
#[must_use]
pub fn params_of(node: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    visit_all(node, &mut |n| {
        if let Expr::Param(name) = n {
            out.insert(name.clone());
        }
    });
    out
}

/// Integer literals of the expression.
#[must_use]
pub fn constants(node: &Expr) -> BTreeSet<i64> {
    let mut out = BTreeSet::new();
    visit_all(node, &mut |n| {
        if let Expr::Int(value) = n {
            out.insert(*value);
        }
    });
    out
}

fn visit_all(node: &Expr, f: &mut impl FnMut(&Expr)) {
    f(node);
    match node {
        Expr::Not(a) => visit_all(a, f),
        Expr::Bin(_, a, b) => {
            visit_all(a, f);
            visit_all(b, f);
        }
        Expr::If(c, a, b) => {
            visit_all(c, f);
            visit_all(a, f);
            visit_all(b, f);
        }
        Expr::Hole(_, Hole::Choice(alternatives)) => {
            for a in alternatives {
                visit_all(a, f);
            }
        }
        _ => {}
    }
}

/// Bottom-up rewrite: `f` returns a replacement or `None` to keep a node.
/// Choice alternatives are rewritten too.
pub fn substitute(node: &Expr, f: &mut impl FnMut(&Expr) -> Result<Option<Expr>>) -> Result<Expr> {
    let rebuilt = match node {
        Expr::Not(a) => Expr::negation(substitute(a, f)?),
        Expr::Bin(op, a, b) => Expr::bin(*op, substitute(a, f)?, substitute(b, f)?),
        Expr::If(c, a, b) => Expr::select(substitute(c, f)?, substitute(a, f)?, substitute(b, f)?),
        Expr::Hole(k, Hole::Choice(alternatives)) => Expr::Hole(
            *k,
            Hole::Choice(
                alternatives
                    .iter()
                    .map(|a| substitute(a, f))
                    .collect::<Result<Vec<_>>>()?,
            ),
        ),
        other => other.clone(),
    };
    Ok(f(&rebuilt)?.unwrap_or(rebuilt))
}

/// Replaces every parameter by its value; an unbound one is refused.
pub fn bind_params(node: &Expr, values: &BTreeMap<String, i64>) -> Result<Expr> {
    substitute(node, &mut |n| match n {
        Expr::Param(name) => values
            .get(name)
            .map(|value| Some(Expr::Int(*value)))
            .ok_or_else(|| Refusal::new(format!("unbound-parameter:{name}"))),
        _ => Ok(None),
    })
}

/// Folds `+` and `-` of two literals (`8 + 1` is `9`) so bound parameters
/// cost no node. A fold that would overflow is left alone.
pub fn fold(node: &Expr) -> Result<Expr> {
    substitute(node, &mut |n| {
        Ok(match n {
            Expr::Bin(op @ (Op::Add | Op::Sub), a, b) => match (a.as_ref(), b.as_ref()) {
                (Expr::Int(x), Expr::Int(y)) => {
                    let folded = if *op == Op::Add {
                        x.checked_add(*y)
                    } else {
                        x.checked_sub(*y)
                    };
                    folded.map(Expr::Int)
                }
                _ => None,
            },
            _ => None,
        })
    })
}

/// A value filling one hole.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Fill {
    /// A constant (constant holes).
    Const(i64),
    /// An alternative's index (choice holes).
    Choice(usize),
    /// An expression (any hole).
    Ast(Expr),
}

/// Replaces the listed holes by their fills.
pub fn fill(node: &Expr, assignment: &BTreeMap<usize, Fill>) -> Result<Expr> {
    substitute(node, &mut |n| {
        let Expr::Hole(k, hole) = n else {
            return Ok(None);
        };
        let Some(value) = assignment.get(k) else {
            return Ok(None);
        };
        Ok(Some(match (value, hole) {
            (Fill::Ast(ast), _) => ast.clone(),
            (Fill::Choice(index), Hole::Choice(alternatives)) => alternatives
                .get(*index)
                .cloned()
                .ok_or_else(|| Refusal::new("choice-index"))?,
            (Fill::Choice(index), _) => {
                Expr::Int(i64::try_from(*index).map_err(|_| Refusal::new("choice-index"))?)
            }
            (Fill::Const(value), _) => Expr::Int(*value),
        }))
    })
}

/// Numbers the holes of several expressions consecutively (one sketch,
/// several outputs).
pub fn renumber(nodes: &[Expr]) -> Result<Vec<Expr>> {
    fn walk(node: &Expr, next: &mut usize) -> Result<Expr> {
        Ok(match node {
            Expr::Hole(_, hole) => {
                let k = *next;
                *next += 1;
                if k >= MAX_HOLES {
                    return Err(Refusal::new("too-many-holes"));
                }
                Expr::Hole(k, hole.clone())
            }
            Expr::Not(a) => Expr::negation(walk(a, next)?),
            Expr::Bin(op, a, b) => Expr::bin(*op, walk(a, next)?, walk(b, next)?),
            Expr::If(c, a, b) => Expr::select(walk(c, next)?, walk(a, next)?, walk(b, next)?),
            other => other.clone(),
        })
    }
    let mut next = 0;
    nodes.iter().map(|node| walk(node, &mut next)).collect()
}

// --- vocabulary --------------------------------------------------------------

/// What names mean for one subject: input fields in program order with their
/// domains, outputs `out0`.., and declared variant names per field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vocabulary {
    names: Vec<String>,
    index: BTreeMap<String, usize>,
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    input_kinds: Vec<Kind>,
    output_kinds: Vec<Kind>,
    variants: BTreeMap<String, i64>,
    field_variants: BTreeMap<String, BTreeMap<i64, String>>,
}

fn is_identifier(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && word_start(bytes[0])
        && bytes.iter().all(|b| word_byte(*b))
}

fn output_name(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("out")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    digits.parse().ok()
}

impl Vocabulary {
    /// A vocabulary for a program with these input and output domains.
    ///
    /// `names` gives each input's field name in program order; `variants`
    /// maps a field to its declared variant names and values. Names must be
    /// distinct identifiers, not reserved words and not `outN`; a variant
    /// name must not be a field or output name, must mean one value
    /// wherever it is declared, and must lie in its field's domain.
    pub fn new(
        names: Vec<String>,
        inputs: Vec<Domain>,
        outputs: Vec<Domain>,
        variants: &BTreeMap<String, BTreeMap<String, i64>>,
    ) -> Result<Self> {
        if names.len() != inputs.len() {
            return Err(Refusal::new("vocabulary-input-count"));
        }
        let kinds = |domains: &[Domain]| {
            domains
                .iter()
                .map(|domain| Kind::of(*domain).ok_or_else(|| Refusal::new("unsupported-domain")))
                .collect::<Result<Vec<_>>>()
        };
        let input_kinds = kinds(&inputs)?;
        let output_kinds = kinds(&outputs)?;
        let mut index = BTreeMap::new();
        for (k, name) in names.iter().enumerate() {
            if !is_identifier(name) || RESERVED.contains(&name.as_str()) {
                return Err(Refusal::new(format!("vocabulary-name:{name}")));
            }
            if output_name(name).is_some() {
                return Err(Refusal::new(format!("vocabulary-name:{name}")));
            }
            if index.insert(name.clone(), k).is_some() {
                return Err(Refusal::new(format!("vocabulary-duplicate-name:{name}")));
            }
        }
        let mut flat = BTreeMap::new();
        let mut field_variants = BTreeMap::new();
        for (field, table) in variants {
            let Some(k) = index.get(field) else {
                return Err(Refusal::new(format!("vocabulary-variant-field:{field}")));
            };
            let mut by_value = BTreeMap::new();
            for (name, value) in table {
                if !is_identifier(name)
                    || RESERVED.contains(&name.as_str())
                    || index.contains_key(name)
                    || output_name(name).is_some()
                {
                    return Err(Refusal::new(format!("variant-name-clash:{name}")));
                }
                if !inputs[*k].contains(*value) || matches!(inputs[*k], Domain::Bool) {
                    return Err(Refusal::new(format!("variant-value:{name}")));
                }
                if flat
                    .insert(name.clone(), *value)
                    .is_some_and(|old| old != *value)
                {
                    return Err(Refusal::new(format!("ambiguous-variant-name:{name}")));
                }
                if by_value.insert(*value, name.clone()).is_some() {
                    return Err(Refusal::new(format!("variant-value-twice:{field}")));
                }
            }
            field_variants.insert(field.clone(), by_value);
        }
        Ok(Self {
            names,
            index,
            inputs,
            outputs,
            input_kinds,
            output_kinds,
            variants: flat,
            field_variants,
        })
    }

    /// Input field names in program order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Input domains in program order.
    #[must_use]
    pub fn inputs(&self) -> &[Domain] {
        &self.inputs
    }

    /// Output domains in program order.
    #[must_use]
    pub fn outputs(&self) -> &[Domain] {
        &self.outputs
    }

    /// Input kinds in program order.
    #[must_use]
    pub fn input_kinds(&self) -> &[Kind] {
        &self.input_kinds
    }

    /// Output kinds in program order.
    #[must_use]
    pub fn output_kinds(&self) -> &[Kind] {
        &self.output_kinds
    }

    /// The position of an input field.
    #[must_use]
    pub fn position(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// Every declared variant name and its value.
    #[must_use]
    pub fn variants(&self) -> &BTreeMap<String, i64> {
        &self.variants
    }

    /// The kind of a name; an unknown name is refused.
    pub fn kind(&self, name: &str) -> Result<Kind> {
        if let Some(k) = self.index.get(name) {
            return Ok(self.input_kinds[*k]);
        }
        if let Some(j) = output_name(name)
            && let Some(kind) = self.output_kinds.get(j)
        {
            return Ok(*kind);
        }
        if self.variants.contains_key(name) {
            return Ok(Kind::Int);
        }
        Err(Refusal::new(format!("unknown-name:{name}")))
    }

    /// The declared name of `value` for field `field`, if any.
    #[must_use]
    pub fn variant_name(&self, field: &str, value: i64) -> Option<&str> {
        self.field_variants
            .get(field)
            .and_then(|table| table.get(&value))
            .map(String::as_str)
    }

    /// Replaces variant names by their values (a field of the same name
    /// wins, which [`Vocabulary::new`] already rules out).
    pub fn resolve_variants(&self, node: &Expr) -> Result<Expr> {
        substitute(node, &mut |n| {
            Ok(match n {
                Expr::Var(name) if !self.index.contains_key(name) => {
                    self.variants.get(name).map(|value| Expr::Int(*value))
                }
                _ => None,
            })
        })
    }

    /// Refuses a name that is neither an input field nor a variant.
    pub fn check_names(&self, node: &Expr) -> Result<()> {
        for name in names_of(node) {
            if !self.index.contains_key(&name) && !self.variants.contains_key(&name) {
                return Err(Refusal::new(format!("unknown-name:{name}")));
            }
        }
        Ok(())
    }
}

/// The node's kind when fixed without context (`None`: a literal or hole,
/// whose kind comes from its position).
pub fn kind_of(node: &Expr, vocab: &Vocabulary) -> Result<Option<Kind>> {
    Ok(match node {
        Expr::Var(name) => Some(vocab.kind(name)?),
        Expr::Int(_) | Expr::Keep | Expr::Hole(_, Hole::Const(_) | Hole::Expr) => None,
        Expr::Hole(_, Hole::Choice(alternatives)) => {
            for a in alternatives {
                if let Some(kind) = kind_of(a, vocab)? {
                    return Ok(Some(kind));
                }
            }
            None
        }
        Expr::Param(_) => Some(Kind::Int),
        Expr::Not(_) => Some(Kind::Bool),
        Expr::If(_, a, b) => match kind_of(a, vocab)? {
            Some(kind) => Some(kind),
            None => kind_of(b, vocab)?,
        },
        Expr::Bin(op, ..) => Some(if op.is_arith() { Kind::Int } else { Kind::Bool }),
    })
}

/// The side kind of a comparison: either operand's fixed kind, else `Int`.
pub fn compare_side(a: &Expr, b: &Expr, vocab: &Vocabulary) -> Result<Kind> {
    Ok(match kind_of(a, vocab)? {
        Some(kind) => kind,
        None => kind_of(b, vocab)?.unwrap_or(Kind::Int),
    })
}

/// The kind each hole must take, given the kind wanted at the root (as
/// lowering assigns kinds).
pub fn hole_kinds(
    node: &Expr,
    want: Kind,
    vocab: &Vocabulary,
    out: &mut BTreeMap<usize, Kind>,
) -> Result<()> {
    match node {
        Expr::Hole(k, hole) => {
            out.insert(*k, want);
            if let Hole::Choice(alternatives) = hole {
                for a in alternatives {
                    hole_kinds(a, want, vocab, out)?;
                }
            }
            Ok(())
        }
        Expr::Int(_) | Expr::Var(_) | Expr::Param(_) | Expr::Keep => Ok(()),
        Expr::Not(a) => hole_kinds(a, Kind::Bool, vocab, out),
        Expr::If(c, a, b) => {
            hole_kinds(c, Kind::Bool, vocab, out)?;
            hole_kinds(a, want, vocab, out)?;
            hole_kinds(b, want, vocab, out)
        }
        Expr::Bin(op, a, b) => {
            let side = if op.is_arith() {
                Kind::Int
            } else if op.is_boolean() {
                Kind::Bool
            } else {
                compare_side(a, b, vocab)?
            };
            hole_kinds(a, side, vocab, out)?;
            hole_kinds(b, side, vocab, out)
        }
    }
}

// --- printing ----------------------------------------------------------------

/// Canonical symbolic text, independent of any vocabulary:
/// `parse(print_symbolic(t)) == t`.
#[must_use]
pub fn print_symbolic(node: &Expr) -> String {
    Printer { vocab: None }.show(node, 0, None)
}

/// Symbolic text for people: variant names where a variant field is
/// compared with a constant, and `true`/`false` for literals in Boolean
/// positions. It parses back to the same expression after variant names are
/// resolved.
#[must_use]
pub fn show(node: &Expr, vocab: &Vocabulary, want: Option<Kind>) -> String {
    Printer { vocab: Some(vocab) }.show(node, 0, want)
}

struct Printer<'a> {
    vocab: Option<&'a Vocabulary>,
}

fn prec_of(node: &Expr) -> u8 {
    match node {
        Expr::If(..) => 0,
        Expr::Bin(op, ..) => op.precedence().0,
        _ => 9,
    }
}

impl Printer<'_> {
    fn kind(&self, node: &Expr) -> Option<Kind> {
        self.vocab
            .and_then(|vocab| kind_of(node, vocab).ok().flatten())
    }

    fn show(&self, node: &Expr, parent_prec: u8, want: Option<Kind>) -> String {
        match node {
            Expr::Int(value) => {
                if want == Some(Kind::Bool) && (*value == 0 || *value == 1) {
                    String::from(if *value == 1 { "true" } else { "false" })
                } else {
                    value.to_string()
                }
            }
            Expr::Var(name) => name.clone(),
            Expr::Param(name) => format!("${name}"),
            Expr::Keep => String::from("keep"),
            Expr::Hole(_, hole) => match hole {
                Hole::Const(None) => String::from("?c"),
                Hole::Const(Some((lo, hi))) => format!("?c[{lo}..{hi}]"),
                Hole::Expr => String::from("?e"),
                Hole::Choice(alternatives) => {
                    let parts: Vec<String> =
                        alternatives.iter().map(|a| self.show(a, 0, want)).collect();
                    format!("?{{{}}}", parts.join(" | "))
                }
            },
            Expr::Not(inner) => {
                let text = if matches!(inner.as_ref(), Expr::Bin(..) | Expr::If(..)) {
                    format!("({})", self.show(inner, 0, Some(Kind::Bool)))
                } else {
                    self.show(inner, 9, Some(Kind::Bool))
                };
                format!("!{text}")
            }
            Expr::If(c, a, b) => {
                let branch = want.or_else(|| self.kind(a)).or_else(|| self.kind(b));
                let mut test = self.show(c, 0, Some(Kind::Bool));
                if matches!(c.as_ref(), Expr::If(..)) {
                    test = format!("({test})");
                }
                let mut yes = self.show(a, 0, branch);
                if matches!(a.as_ref(), Expr::If(..)) {
                    yes = format!("({yes})");
                }
                let no = self.show(b, 0, branch);
                let text = format!("{test} ? {yes} : {no}");
                if parent_prec > 0 {
                    format!("({text})")
                } else {
                    text
                }
            }
            Expr::Bin(op, a, b) => {
                let (prec, right) = op.precedence();
                let side = if op.is_boolean() {
                    Some(Kind::Bool)
                } else if op.is_arith() {
                    Some(Kind::Int)
                } else {
                    self.kind(a).or_else(|| self.kind(b))
                };
                let left = self.operand(a, prec, right, side);
                let mut rhs = self.operand(b, prec, !right, side);
                if matches!(op, Op::Eq | Op::Ne)
                    && let (Expr::Var(field), Expr::Int(value), Some(vocab)) =
                        (a.as_ref(), b.as_ref(), self.vocab)
                    && let Some(name) = vocab.variant_name(field, *value)
                {
                    rhs = name.to_string();
                }
                let text = format!("{left} {} {rhs}", op.symbol());
                if prec < parent_prec {
                    format!("({text})")
                } else {
                    text
                }
            }
        }
    }

    /// Parenthesises a child of lower precedence, or of equal precedence on
    /// the non-associative side; a comparison under an equality and an `&&`
    /// under an `||` are parenthesised for readability only.
    fn operand(&self, node: &Expr, prec: u8, strict: bool, want: Option<Kind>) -> String {
        if matches!(node, Expr::If(..)) {
            return format!("({})", self.show(node, 0, want));
        }
        let child = prec_of(node);
        let text = self.show(node, 0, want);
        let bin = |test: fn(Op) -> bool| matches!(node, Expr::Bin(op, ..) if test(*op));
        if child < prec || (strict && child == prec && bin(|_| true)) {
            return format!("({text})");
        }
        if bin(Op::is_compare) && prec == Op::Eq.precedence().0 {
            return format!("({text})");
        }
        if bin(|op| op == Op::And) && prec == Op::Or.precedence().0 {
            return format!("({text})");
        }
        text
    }
}

// --- evaluation --------------------------------------------------------------

/// The value a hole takes during evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoleValue {
    /// A constant hole's value.
    Const(i64),
    /// A choice hole's alternative.
    Choice(usize),
}

/// Input columns (one value per considered tuple) for evaluation, and the
/// original output used by `keep`.
#[derive(Clone, Copy, Debug)]
pub struct Env<'a> {
    /// Names and kinds.
    pub vocab: &'a Vocabulary,
    /// One column per input, in program order.
    pub columns: &'a [Vec<i64>],
    /// The value `keep` stands for, when available.
    pub keep: Option<&'a [i64]>,
    /// Tuples considered.
    pub len: usize,
}

fn bit(value: bool) -> i64 {
    i64::from(value)
}

/// Refusal code for `+` or `-` leaving the signed 64-bit range.
pub const OVERFLOW: &str = "expression-arithmetic-overflow";

/// `Ok(None)` for a refusal with [`OVERFLOW`]; other refusals pass. Searches
/// use it to skip candidates that would trap.
pub fn skip_overflow<T>(result: Result<T>) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(refusal) if refusal.0 == OVERFLOW => Ok(None),
        Err(refusal) => Err(refusal),
    }
}

/// The value of `node` on every considered tuple. Evaluation is eager, like
/// a lowered program's: every subexpression is computed on every tuple, both
/// branches of `c ? a : b` included, and `+` or `-` leaving the signed
/// 64-bit range on any tuple refuses the whole evaluation with [`OVERFLOW`]
/// (where the lowered program would trap). A caller that searches skips
/// such a candidate: it cannot be a completion.
pub fn evaluate(
    node: &Expr,
    env: &Env<'_>,
    holes: &BTreeMap<usize, HoleValue>,
) -> Result<Vec<i64>> {
    let n = env.len;
    Ok(match node {
        Expr::Int(value) => vec![*value; n],
        Expr::Var(name) => {
            let k = env
                .vocab
                .position(name)
                .ok_or_else(|| Refusal::new(format!("unknown-name:{name}")))?;
            env.columns
                .get(k)
                .cloned()
                .ok_or_else(|| Refusal::new("column-missing"))?
        }
        Expr::Param(name) => return Err(Refusal::new(format!("unbound-parameter:{name}"))),
        Expr::Keep => env
            .keep
            .map(<[i64]>::to_vec)
            .ok_or_else(|| Refusal::new("keep-unavailable"))?,
        Expr::Hole(k, hole) => {
            let value = holes.get(k).ok_or_else(|| Refusal::new("unfilled-hole"))?;
            match (hole, value) {
                // Only the chosen alternative is part of the program.
                (Hole::Choice(alternatives), HoleValue::Choice(index)) => evaluate(
                    alternatives
                        .get(*index)
                        .ok_or_else(|| Refusal::new("choice-index"))?,
                    env,
                    holes,
                )?,
                (_, HoleValue::Const(value)) => vec![*value; n],
                (_, HoleValue::Choice(_)) => return Err(Refusal::new("choice-for-constant-hole")),
            }
        }
        Expr::Not(a) => evaluate(a, env, holes)?
            .into_iter()
            .map(|x| bit(x == 0))
            .collect(),
        Expr::If(c, a, b) => {
            let c = evaluate(c, env, holes)?;
            let a = evaluate(a, env, holes)?;
            let b = evaluate(b, env, holes)?;
            c.iter()
                .zip(a.iter().zip(&b))
                .map(|(c, (a, b))| if *c == 1 { *a } else { *b })
                .collect()
        }
        Expr::Bin(op, a, b) => {
            let a = evaluate(a, env, holes)?;
            let b = evaluate(b, env, holes)?;
            let f: fn(i64, i64) -> Option<i64> = match op {
                Op::Add => i64::checked_add,
                Op::Sub => i64::checked_sub,
                Op::Mul => return Err(Refusal::new("proposal-operator-unsupported:*")),
                Op::And => |x, y| Some(bit(x == 1 && y == 1)),
                Op::Or => |x, y| Some(if x == 1 { x } else { y }),
                Op::Implies => |x, y| Some(if x == 1 { y } else { 1 }),
                Op::Eq => |x, y| Some(bit(x == y)),
                Op::Ne => |x, y| Some(bit(x != y)),
                Op::Lt => |x, y| Some(bit(x < y)),
                Op::Le => |x, y| Some(bit(x <= y)),
                Op::Gt => |x, y| Some(bit(x > y)),
                Op::Ge => |x, y| Some(bit(x >= y)),
            };
            a.iter()
                .zip(&b)
                .map(|(x, y)| f(*x, *y).ok_or_else(|| Refusal::new(OVERFLOW)))
                .collect::<Result<Vec<i64>>>()?
        }
    })
}
