//! Fixed-size tables, expanded into ordinary declarations while parsing.
//!
//! `table ID OWNER name TYPE size N;` declares the fields `ID + k`, named
//! `name_k`, for each entry `k` from 0 to N-1. `type ID index Name of T;` is
//! the integer type `0..=N-1` of table `T`'s entries. In formulas,
//! `ROOT.OWNER.T[k]` names entry k, where k is an integer literal or a table
//! index variable, and `forall`, `exists`, `count` and `sum` range over a
//! table's entries. Every form expands to the declarations and formula nodes
//! language version 1 already has, so the encoded project, its identity and
//! every check of it are those of the expansion. The rule:
//!
//! - `forall i in table T [where C] { F }` is `(C_0 -> F_0) && ... && (C_n -> F_n)`,
//!   or `F_0 && ... && F_n` without a condition, chained left to right;
//! - `exists i in table T [where C] { F }` is `(C_0 && F_0) || ... || (C_n && F_n)`;
//! - `sum i in table T [where C] { V }` is `V_k + ...` over the entries whose
//!   condition holds; input-reading conditions become eager Select values;
//! - `count i in table T { C } OP K` compares the number of entries satisfying
//!   C with the integer K. `count <= K` is the conjunction, over every set of
//!   K+1 entries in order, of `!(C_a && C_b && ...)`; `count >= K` is the
//!   disjunction, over every set of K entries, of their conjunction. `<`, `>`,
//!   `==` and `!=` are composed from those. Clause lists are combined in a
//!   balanced tree, left half first.
//!
//! Counts in value positions expand to sums of eager Select(C_k, 1, 0).
//! Here X_k is X with the index variable replaced by the integer k.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::ast::{CompareOp, DivisionMode, ProjectionRoot, RelExpr, ValueExpr};
use crate::lexer::{Token, TokenKind};

/// Largest number of entries one table may declare.
pub const MAX_TABLE_ENTRIES: u32 = 64;

/// One table declaration, as written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableInfo {
    /// The table's stable ID, which is also the ID of entry 0.
    pub id: u32,
    /// The record type that owns the entry fields.
    pub owner: u32,
    /// The table name; entry k is the field `name_k`.
    pub name: String,
    /// The type of every entry field.
    pub field_type: u32,
    /// The number of entries.
    pub size: u32,
}

/// One `count` form as written, with the clauses it expands to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountInfo {
    /// One-based source line of the `count` keyword.
    pub line: u32,
    /// One-based source column of the `count` keyword.
    pub column: u32,
    /// Clauses in one expansion, C(N, K+1) for constant-bound `count <= K`,
    /// or N Select terms for a linear value count.
    pub clauses: u128,
    /// True when clauses counts Select terms in a linear value count.
    pub linear: bool,
    /// Times the form is expanded: once, or once per entry of each
    /// enclosing table form.
    pub expansions: u64,
}

/// The exact expansion of a project's table forms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableExpansion {
    /// The source with every table form replaced by the declarations and
    /// formulas it expands to. Parsing it gives the same canonical project.
    pub text: String,
    /// Every table, in stable-ID order.
    pub tables: Vec<TableInfo>,
    /// Every `count` whose conditions read an input, in source order.
    pub counts: Vec<CountInfo>,
}

/// One replaced byte range of the source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Rewrite {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) text: String,
}

/// Applies non-overlapping rewrites to the source, in byte order.
pub(crate) fn apply(source: &str, mut rewrites: Vec<Rewrite>) -> Option<String> {
    rewrites.sort_by_key(|rewrite| rewrite.start);
    let mut out = String::new();
    let mut at = 0usize;
    for rewrite in rewrites {
        let (start, end) = (rewrite.start as usize, rewrite.end as usize);
        if start < at || end < start {
            return None;
        }
        out.push_str(source.get(at..start)?);
        out.push_str(&rewrite.text);
        at = end;
    }
    out.push_str(source.get(at..)?);
    Some(out)
}

/// Table sizes by ID, read from every well-formed table declaration before
/// parsing, so formulas may name tables declared later in the file.
pub(crate) fn prescan(tokens: &[Token]) -> BTreeMap<u32, u32> {
    let mut tables = BTreeMap::new();
    for window in tokens.windows(8) {
        if let [
            Token {
                kind: TokenKind::Ident(keyword),
                ..
            },
            Token {
                kind: TokenKind::Number(id),
                ..
            },
            Token {
                kind: TokenKind::Number(_),
                ..
            },
            Token {
                kind: TokenKind::Ident(_),
                ..
            },
            Token {
                kind: TokenKind::Number(_),
                ..
            },
            Token {
                kind: TokenKind::Ident(size_keyword),
                ..
            },
            Token {
                kind: TokenKind::Number(size),
                ..
            },
            Token {
                kind: TokenKind::Semicolon,
                ..
            },
        ] = window
            && keyword.as_ref() == "table"
            && size_keyword.as_ref() == "size"
            && let (Ok(id), Ok(size)) = (u32::try_from(*id), u32::try_from(*size))
        {
            tables.entry(id).or_insert(size);
        }
    }
    tables
}

/// The entries' formulas joined left to right; `empty` for no entries.
pub(crate) fn chain(
    entries: Vec<RelExpr>,
    join: fn(Box<RelExpr>, Box<RelExpr>) -> RelExpr,
    empty: bool,
) -> RelExpr {
    entries
        .into_iter()
        .reduce(|left, right| join(Box::new(left), Box::new(right)))
        .unwrap_or(RelExpr::Bool(empty))
}

/// The entries' formulas joined in a balanced tree, left half first.
fn balanced(
    mut entries: Vec<RelExpr>,
    join: fn(Box<RelExpr>, Box<RelExpr>) -> RelExpr,
    empty: bool,
) -> RelExpr {
    match entries.len() {
        0 => RelExpr::Bool(empty),
        1 => entries.pop().unwrap_or(RelExpr::Bool(empty)),
        n => {
            let right = entries.split_off(n / 2);
            join(
                Box::new(balanced(entries, join, empty)),
                Box::new(balanced(right, join, empty)),
            )
        }
    }
}

/// The entries' values added left to right; 0 for no entries.
pub(crate) fn total(entries: Vec<ValueExpr>) -> ValueExpr {
    entries
        .into_iter()
        .reduce(|left, right| ValueExpr::Add(Box::new(left), Box::new(right)))
        .unwrap_or(ValueExpr::Int(0))
}

/// The number of `k`-element subsets of `n` entries, saturating.
pub(crate) fn subsets(n: usize, k: usize) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut value = 1u128;
    for i in 0..k {
        value = value.saturating_mul((n - i) as u128) / (i as u128 + 1);
    }
    value
}

/// The clauses and condition copies `count OP k` expands to over `n`
/// entries: C(n, k+1) clauses of k+1 copies for `count <= k`.
pub(crate) fn count_size(n: usize, op: CompareOp, k: i128) -> (u128, u128) {
    let at_least = |m: i128| match usize::try_from(m) {
        Ok(m) if m >= 1 && m <= n => (subsets(n, m), subsets(n, m).saturating_mul(m as u128)),
        _ => (0, 0),
    };
    let at_most = |m: i128| at_least(m.saturating_add(1));
    match op {
        CompareOp::LessEq => at_most(k),
        CompareOp::Less => at_most(k.saturating_sub(1)),
        CompareOp::GreaterEq => at_least(k),
        CompareOp::Greater => at_least(k.saturating_add(1)),
        CompareOp::Eq | CompareOp::NotEq => {
            let ((a, b), (c, d)) = (at_least(k), at_most(k));
            (a.saturating_add(c), b.saturating_add(d))
        }
    }
}

/// `count OP k`, where entry `i` is counted when `conditions[i]` holds.
pub(crate) fn count(conditions: &[RelExpr], op: CompareOp, k: i128) -> RelExpr {
    match op {
        CompareOp::LessEq => at_most(conditions, k),
        CompareOp::Less => at_most(conditions, k.saturating_sub(1)),
        CompareOp::GreaterEq => at_least(conditions, k),
        CompareOp::Greater => at_least(conditions, k.saturating_add(1)),
        CompareOp::Eq => RelExpr::And(
            Box::new(at_least(conditions, k)),
            Box::new(at_most(conditions, k)),
        ),
        CompareOp::NotEq => RelExpr::Not(Box::new(RelExpr::And(
            Box::new(at_least(conditions, k)),
            Box::new(at_most(conditions, k)),
        ))),
    }
}

/// At least `m` conditions hold: some `m` entries all satisfy theirs.
fn at_least(conditions: &[RelExpr], m: i128) -> RelExpr {
    let Ok(m) = usize::try_from(m) else {
        return RelExpr::Bool(true);
    };
    if m == 0 {
        return RelExpr::Bool(true);
    }
    if m > conditions.len() {
        return RelExpr::Bool(false);
    }
    let clauses = choose(conditions.len(), m)
        .into_iter()
        .map(|set| {
            chain(
                set.iter().map(|i| conditions[*i].clone()).collect(),
                RelExpr::And,
                true,
            )
        })
        .collect();
    balanced(clauses, RelExpr::Or, false)
}

/// At most `m` conditions hold: no `m + 1` entries all satisfy theirs.
fn at_most(conditions: &[RelExpr], m: i128) -> RelExpr {
    if m < 0 {
        return RelExpr::Bool(false);
    }
    let Ok(limit) = usize::try_from(m) else {
        return RelExpr::Bool(true);
    };
    if limit >= conditions.len() {
        return RelExpr::Bool(true);
    }
    let clauses = choose(conditions.len(), limit + 1)
        .into_iter()
        .map(|set| {
            RelExpr::Not(Box::new(chain(
                set.iter().map(|i| conditions[*i].clone()).collect(),
                RelExpr::And,
                true,
            )))
        })
        .collect();
    balanced(clauses, RelExpr::And, true)
}

/// Every `k`-element subset of `0..n`, each ascending, in lexicographic order.
fn choose(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut set: Vec<usize> = (0..k).collect();
    if k > n {
        return out;
    }
    loop {
        out.push(set.clone());
        let Some(i) = (0..k).rev().find(|&i| set[i] != i + n - k) else {
            return out;
        };
        set[i] += 1;
        for j in i + 1..k {
            set[j] = set[j - 1] + 1;
        }
    }
}

/// A checked constant truth. Both Boolean operands must be defined before a
/// formula can be folded, just as in the eager runtime graph.
pub(crate) fn static_truth(value: &RelExpr) -> Option<bool> {
    Some(match value {
        RelExpr::Circuit(c) => c.evaluate(static_truth)?,
        RelExpr::Bool(value) => *value,
        RelExpr::Not(inner) => !static_truth(inner)?,
        RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
            // Establish both operands' definedness before applying Boolean logic.
            // Short-circuiting the recursive calls would turn a trap into a truth.
            let (left, right) = (static_truth(a)?, static_truth(b)?);
            match value {
                RelExpr::And(..) => left && right,
                RelExpr::Or(..) => left || right,
                _ => !left || right,
            }
        }
        RelExpr::Compare(op, a, b) => {
            let (a, b) = (static_value(a)?, static_value(b)?);
            match op {
                CompareOp::Eq => a == b,
                CompareOp::NotEq => a != b,
                CompareOp::Less => a < b,
                CompareOp::LessEq => a <= b,
                CompareOp::Greater => a > b,
                CompareOp::GreaterEq => a >= b,
            }
        }
        RelExpr::Predicate { .. } | RelExpr::ForAll { .. } | RelExpr::Exists { .. } => return None,
    })
}

/// A checked literal calculation; `Some` also establishes totality.
pub(crate) fn static_value(value: &ValueExpr) -> Option<i128> {
    match value {
        ValueExpr::Int(value) => Some(*value),
        ValueExpr::Add(a, b) => static_value(a)?.checked_add(static_value(b)?),
        ValueExpr::Sub(a, b) => static_value(a)?.checked_sub(static_value(b)?),
        ValueExpr::Mul(a, b) => static_value(a)?.checked_mul(static_value(b)?),
        _ => None,
    }
}

/// `.zeno` text that parses back to exactly this formula.
pub(crate) fn rel_text(value: &RelExpr) -> String {
    let mut out = String::new();
    write_rel(value, 1, &mut out);
    out
}

fn rel_level(value: &RelExpr) -> u8 {
    match value {
        RelExpr::Implies(..) => 1,
        RelExpr::Or(..) => 2,
        RelExpr::And(..) => 3,
        _ => 4,
    }
}

fn write_rel(value: &RelExpr, least: u8, out: &mut String) {
    let enclosed = rel_level(value) < least;
    if enclosed {
        out.push('(');
    }
    match value {
        RelExpr::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        RelExpr::Circuit(c) => {
            use crate::BoolNode;
            out.push_str("circuit { ");
            for node in c.nodes() {
                match node {
                    BoolNode::Formula(f) => {
                        out.push_str("formula { ");
                        write_rel(f, 1, out);
                        out.push_str(" }");
                    }
                    BoolNode::Literal(v) => out.push_str(if *v { "true" } else { "false" }),
                    BoolNode::Not(a) => {
                        let _ = write!(out, "not {a}");
                    }
                    BoolNode::And(a, b) => {
                        let _ = write!(out, "and {a} {b}");
                    }
                    BoolNode::Or(a, b) => {
                        let _ = write!(out, "or {a} {b}");
                    }
                    BoolNode::Select(c, a, b) => {
                        let _ = write!(out, "select {c} {a} {b}");
                    }
                }
                out.push_str("; ");
            }
            let _ = write!(out, "}} output {}", c.output());
        }
        RelExpr::Not(inner) => {
            out.push_str("!(");
            write_rel(inner, 1, out);
            out.push(')');
        }
        RelExpr::And(a, b) => binary_rel(a, " && ", b, 3, 4, out),
        RelExpr::Or(a, b) => binary_rel(a, " || ", b, 2, 3, out),
        RelExpr::Implies(a, b) => binary_rel(a, " -> ", b, 2, 1, out),
        RelExpr::Compare(op, a, b) => {
            write_value(a, 1, out);
            out.push_str(match op {
                CompareOp::Eq => " == ",
                CompareOp::NotEq => " != ",
                CompareOp::Less => " < ",
                CompareOp::LessEq => " <= ",
                CompareOp::Greater => " > ",
                CompareOp::GreaterEq => " >= ",
            });
            write_value(b, 1, out);
        }
        RelExpr::Predicate { name, arguments } => {
            out.push_str(name.as_str());
            arguments_text(arguments.iter(), out);
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        }
        | RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => {
            let keyword = if matches!(value, RelExpr::ForAll { .. }) {
                "forall"
            } else {
                "exists"
            };
            let _ = write!(out, "{keyword} {} in {start}..{end} {{ ", variable.as_str());
            write_rel(body, 1, out);
            out.push_str(" }");
        }
    }
    if enclosed {
        out.push(')');
    }
}

fn binary_rel(a: &RelExpr, operator: &str, b: &RelExpr, left: u8, right: u8, out: &mut String) {
    write_rel(a, left, out);
    out.push_str(operator);
    write_rel(b, right, out);
}

/// `0 - x` is how the parser reads a negation `-x`.
fn negated(value: &ValueExpr) -> Option<&ValueExpr> {
    match value {
        ValueExpr::Sub(zero, inner) if **zero == ValueExpr::Int(0) => Some(inner),
        _ => None,
    }
}

fn value_level(value: &ValueExpr) -> u8 {
    if negated(value).is_some() {
        return 3;
    }
    match value {
        ValueExpr::Add(..) | ValueExpr::Sub(..) => 1,
        ValueExpr::Mul(..) => 2,
        _ => 3,
    }
}

fn write_value(value: &ValueExpr, least: u8, out: &mut String) {
    let enclosed = value_level(value) < least;
    if enclosed {
        out.push('(');
    }
    if let Some(inner) = negated(value) {
        // A negation never starts with `(`, which a comparison cannot.
        out.push('-');
        write_value(inner, 3, out);
        if enclosed {
            out.push(')');
        }
        return;
    }
    match value {
        ValueExpr::Int(value) => {
            let _ = write!(out, "{value}");
        }
        ValueExpr::U128(value) => {
            let _ = write!(out, "u128({value})");
        }
        ValueExpr::Select(c, a, b) => {
            out.push_str("choose(");
            write_rel(c, 0, out);
            out.push_str(", ");
            write_value(a, 0, out);
            out.push_str(", ");
            write_value(b, 0, out);
            out.push(')');
        }
        ValueExpr::ToI128(inner) => call("to_i128", &[inner.as_ref()], out),
        ValueExpr::ToU128(inner) => call("to_u128", &[inner.as_ref()], out),
        ValueExpr::Var(name) => out.push_str(name.as_str()),
        ValueExpr::Projection(path) => {
            out.push_str(match path.root() {
                ProjectionRoot::Pre => "pre",
                ProjectionRoot::Post => "post",
                ProjectionRoot::Command => "command",
                ProjectionRoot::Context => "context",
                ProjectionRoot::Effects => "effects",
                ProjectionRoot::Outbox => "outbox",
                ProjectionRoot::Events => "events",
            });
            for segment in path.segments() {
                let _ = write!(out, ".{}", segment.get());
            }
        }
        ValueExpr::Add(a, b) => binary_value(a, " + ", b, 1, 2, out),
        ValueExpr::Sub(a, b) => binary_value(a, " - ", b, 1, 2, out),
        ValueExpr::Mul(a, b) => binary_value(a, " * ", b, 2, 3, out),
        ValueExpr::Div(mode, a, b) => call(
            match mode {
                DivisionMode::Exact => "div_exact",
                DivisionMode::Floor => "div_floor",
                DivisionMode::Ceil => "div_ceil",
            },
            &[a.as_ref(), b.as_ref()],
            out,
        ),
        ValueExpr::MulDiv(mode, a, b, d) => call(
            if *mode == DivisionMode::Ceil {
                "mul_div_ceil"
            } else {
                "mul_div_floor"
            },
            &[a.as_ref(), b.as_ref(), d.as_ref()],
            out,
        ),
        ValueExpr::SignedMulDivFloor(a, b, d) => call(
            "signed_mul_div_floor",
            &[a.as_ref(), b.as_ref(), d.as_ref()],
            out,
        ),
        ValueExpr::ScaledMulDiv(mode, signed, a, b, d, scale) => {
            let name = if *signed {
                "signed_mul_div_floor"
            } else if *mode == DivisionMode::Ceil {
                "mul_div_ceil"
            } else {
                "mul_div_floor"
            };
            out.push_str(name);
            out.push('(');
            for value in [a, b, d] {
                write_value(value, 1, out);
                out.push_str(", ");
            }
            let _ = write!(out, "e{scale})");
        }
        ValueExpr::RepeatMulDiv(signed, a, b, d, count) => {
            out.push_str(if *signed {
                "repeat_signed_mul_div_floor("
            } else {
                "repeat_mul_div_floor("
            });
            for value in [a, b, d] {
                write_value(value, 1, out);
                out.push_str(", ");
            }
            let _ = write!(out, "{count})");
        }
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            let _ = write!(out, "sum {} in {start}..{end} {{ ", variable.as_str());
            write_value(body, 1, out);
            out.push_str(" }");
        }
    }
    if enclosed {
        out.push(')');
    }
}

fn binary_value(
    a: &ValueExpr,
    operator: &str,
    b: &ValueExpr,
    left: u8,
    right: u8,
    out: &mut String,
) {
    write_value(a, left, out);
    out.push_str(operator);
    write_value(b, right, out);
}

fn call(name: &str, arguments: &[&ValueExpr], out: &mut String) {
    out.push_str(name);
    arguments_text(arguments.iter().copied(), out);
}

fn arguments_text<'a>(arguments: impl Iterator<Item = &'a ValueExpr>, out: &mut String) {
    out.push('(');
    for (index, value) in arguments.enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write_value(value, 1, out);
    }
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsets_are_complete_ordered_and_counted() {
        assert_eq!(
            choose(4, 2),
            [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]]
        );
        for n in 0..8 {
            for k in 0..=n {
                assert_eq!(choose(n, k).len() as u128, subsets(n, k));
            }
        }
        assert_eq!(subsets(64, 3), 41_664);
        assert_eq!(subsets(64, 32), 1_832_624_140_942_590_534);
    }
}
