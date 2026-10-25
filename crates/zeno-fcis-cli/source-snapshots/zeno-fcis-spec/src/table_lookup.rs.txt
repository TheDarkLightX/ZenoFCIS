//! Checked integer positions and eager lowering; no runtime table or fallback.
use super::*;
use crate::{BoolCircuit, BoolNode};

/// One owned scalar or Boolean table read, after bounded source expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableReadExpression {
    /// Numeric entry, lowered to the ordinary value language.
    Value(ValueExpr),
    /// Boolean entry, lowered to an ordinary formula or checked circuit.
    Formula(RelExpr),
}

/// Owned sizes of stored and constant table index sets, after project checks.
pub fn table_index_sets(
    source: &str,
    limits: SourceLimits,
) -> Result<BTreeMap<u32, u32>, DiagnosticSet> {
    let (parsed, parser) = parse_with_tables(source, limits)?;
    crate::elaborate_project(parsed, crate::ProjectLimits::default())?;
    Ok(parser.tables)
}

/// Parses one indexed read against a project's declarations and closed strata.
/// This owns fresh preparation scratch and grants no evaluation authority.
pub fn parse_table_read(
    source: &str,
    read: &str,
    limits: SourceLimits,
) -> Result<TableReadExpression, DiagnosticSet> {
    let (parsed, mut parser) = parse_with_tables(source, limits)?;
    parser.declarations = parsed.declarations.clone();
    let project = crate::elaborate_project(parsed, crate::ProjectLimits::default())?;
    let checked = checked_tokens(
        read,
        limits,
        Root::TableRead,
        false,
        Some(&parser.scan.entries),
        &parser.derived_heights(),
        Some(&parser.tables),
    )?;
    parser.tokens = checked.tokens;
    parser.index = 0;
    parser.diagnostics.clear();
    parser.rewrites.clear();
    let starts_read = matches!(&parser.current().kind, TokenKind::Ident(name)
        if parser.constants.contains_key(name.as_ref()) || parser.derived.contains_key(name.as_ref()) || projection_root(name).is_some());
    if !starts_read || !parser.tokens.iter().any(|t| t.kind == TokenKind::LBracket) {
        parser.error(
            DiagnosticCode::ExpectedToken,
            "table.read",
            "one indexed table read",
            "another expression",
            "supply a declared table followed by its indices",
        );
    }
    let boolean = parser.constant_bool().or_else(|| parser.derived_atom());
    let numeric = boolean.is_none();
    let formula = boolean.unwrap_or_else(|| {
        RelExpr::Compare(CompareOp::Eq, parser.parse_value_atom(), ValueExpr::Int(0))
    });
    if !matches!(parser.current().kind, TokenKind::Eof) {
        parser.error(
            DiagnosticCode::ExpectedToken,
            "table.read",
            "end of one indexed read",
            parser.describe_current(),
            "remove trailing text or declarations",
        );
    }
    let (nodes, depth) = formula_shape_rel(&formula);
    if nodes > MAX_FORMULA_NODES || depth > MAX_FORMULA_DEPTH {
        parser.error(
            DiagnosticCode::ResourceLimit,
            "table.read",
            "a bounded table read",
            format!("{nodes} nodes and depth {depth}"),
            "use a smaller table read",
        );
    }
    let law = LawDecl::new(project.project_id(), fallback_identifier(), formula);
    for path in crate::law_paths(&law) {
        if crate::resolve_path(&project, path).is_unresolved() {
            parser.error(
                DiagnosticCode::UnknownReference,
                "table.read",
                "a declared projection",
                format!("{path:?}"),
                "use the project's declared fields",
            );
        }
    }
    if let Some(reason) = parser.key_paths.misuse(law.formula()) {
        parser.error(
            DiagnosticCode::InvalidDeclaration,
            "table.read",
            "valid collection use",
            reason,
            "read collections through their declared entries",
        );
    }
    if !parser.diagnostics.is_empty() {
        return Err(DiagnosticSet::from_vec(
            parser.diagnostics,
            limits.max_diagnostics(),
        ));
    }
    Ok(match law.formula {
        RelExpr::Compare(_, value, _) if numeric => TableReadExpression::Value(value),
        formula => TableReadExpression::Formula(formula),
    })
}

#[derive(Clone, Copy)]
struct Bounds {
    lo: i128,
    hi: i128,
    unsigned: bool,
}

impl Bounds {
    fn new(lo: i128, hi: i128, unsigned: bool) -> Option<Self> {
        (lo <= hi && (!unsigned || lo >= 0)).then_some(Self { lo, hi, unsigned })
    }

    fn fits_table(self, size: u32) -> bool {
        if self.unsigned {
            u128::try_from(self.lo)
                .ok()
                .zip(u128::try_from(self.hi).ok())
                .is_some_and(|(min, max)| {
                    crate::TableIndexRange::Unsigned { min, max }.fits_table(size)
                })
        } else {
            crate::TableIndexRange::Signed {
                min: self.lo,
                max: self.hi,
            }
            .fits_table(size)
        }
    }
}

fn rounded(a: i128, b: i128, mode: DivisionMode) -> Option<i128> {
    let q = a.checked_div(b)?;
    let r = a.checked_rem(b)?;
    match mode {
        DivisionMode::Exact => (r == 0).then_some(q),
        DivisionMode::Floor if r != 0 && (a < 0) != (b < 0) => q.checked_sub(1),
        DivisionMode::Ceil if r != 0 && (a < 0) == (b < 0) => q.checked_add(1),
        _ => Some(q),
    }
}

impl Parser {
    fn projection_bounds(&self, p: &ProjectionPath) -> Option<Bounds> {
        if !matches!(
            p.root(),
            ProjectionRoot::Pre
                | ProjectionRoot::Post
                | ProjectionRoot::Command
                | ProjectionRoot::Context
        ) {
            return None;
        }
        let (owner, rest) = p.segments().split_first()?;
        let mut owner = *owner;
        for field in rest {
            owner = self
                .declarations
                .iter()
                .find_map(|d| match &d.declaration {
                    Declaration::Field(f) if f.owner() == owner && f.id() == *field => {
                        Some(f.field_type())
                    }
                    _ => None,
                })?;
        }
        let t = self
            .declarations
            .iter()
            .find_map(|d| match &d.declaration {
                Declaration::Type(t) if t.id() == owner => Some(t),
                _ => None,
            })?;
        match t.kind() {
            TypeKind::Int => t.range().and_then(|r| Bounds::new(r.min(), r.max(), false)),
            TypeKind::U128 => t.unsigned_range().and_then(|(lo, hi)| {
                Bounds::new(i128::try_from(lo).ok()?, i128::try_from(hi).ok()?, true)
            }),
            _ => None,
        }
    }

    fn position_bounds(&self, v: &ValueExpr) -> Option<Bounds> {
        match v {
            ValueExpr::Int(n) => Bounds::new(*n, *n, false),
            ValueExpr::U128(n) => {
                let n = i128::try_from(*n).ok()?;
                Bounds::new(n, n, true)
            }
            ValueExpr::Projection(p) => self.projection_bounds(p),
            ValueExpr::ToI128(inner) | ValueExpr::ToU128(inner) => {
                let r = self.position_bounds(inner)?;
                Bounds::new(r.lo, r.hi, matches!(v, ValueExpr::ToU128(_)))
            }
            ValueExpr::Select(_, a, b) => {
                let (a, b) = (self.position_bounds(a)?, self.position_bounds(b)?);
                (a.unsigned == b.unsigned).then_some(())?;
                Bounds::new(a.lo.min(b.lo), a.hi.max(b.hi), a.unsigned)
            }
            ValueExpr::Add(a, b)
            | ValueExpr::Sub(a, b)
            | ValueExpr::Mul(a, b)
            | ValueExpr::Div(_, a, b) => {
                let (a, b) = (self.position_bounds(a)?, self.position_bounds(b)?);
                (a.unsigned == b.unsigned).then_some(())?;
                match v {
                    ValueExpr::Add(_, _) => {
                        Bounds::new(a.lo.checked_add(b.lo)?, a.hi.checked_add(b.hi)?, a.unsigned)
                    }
                    ValueExpr::Sub(_, _) => {
                        Bounds::new(a.lo.checked_sub(b.hi)?, a.hi.checked_sub(b.lo)?, a.unsigned)
                    }
                    ValueExpr::Mul(_, _) | ValueExpr::Div(_, _, _) => {
                        if let ValueExpr::Div(mode, _, _) = v {
                            if b.lo <= 0 && 0 <= b.hi {
                                return None;
                            }
                            if *mode == DivisionMode::Exact
                                && !(b.lo == b.hi && (b.lo == 1 || b.lo == -1 || a.lo == a.hi))
                            {
                                return None;
                            }
                        }
                        let mut lo = i128::MAX;
                        let mut hi = i128::MIN;
                        for x in [a.lo, a.hi] {
                            for y in [b.lo, b.hi] {
                                let n = if let ValueExpr::Div(mode, _, _) = v {
                                    rounded(x, y, *mode)?
                                } else {
                                    x.checked_mul(y)?
                                };
                                lo = lo.min(n);
                                hi = hi.max(n);
                            }
                        }
                        Bounds::new(lo, hi, a.unsigned)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Parsing an index cannot hide an active self read through simplification.
    pub(super) fn position(&mut self, size: u32, at: SourceSpan) -> Option<ValueExpr> {
        // Keep the old literal/bound path's nesting accounting unchanged.
        let direct = if self
            .tokens
            .get(self.index + 1)
            .is_some_and(|t| t.kind == TokenKind::RBracket)
        {
            match &self.current().kind {
                TokenKind::Number(n) => i128::try_from(*n).ok(),
                TokenKind::Ident(n) => self.index_value(n),
                _ => None,
            }
        } else {
            None
        };
        let v = if let Some(n) = direct {
            self.advance();
            ValueExpr::Int(n)
        } else {
            self.with_self_refusal("a table index cannot read the table being defined", |p| {
                p.nested("table.index", ValueExpr::Int(0), |p| p.parse_value())
            })
        };
        let valid = self
            .position_bounds(&v)
            .is_some_and(|range| range.fits_table(size));
        if !valid {
            self.error_at(
                at,
                DiagnosticCode::InvalidDeclaration,
                "table.entry",
                format!("index range contained in 0..{}", size.saturating_sub(1)),
                "unproved or out-of-range table index",
                "declare bounded integer inputs and use checked range-preserving arithmetic",
            );
            return None;
        }
        Some(v)
    }

    pub(super) fn index_number(&self, index: &ValueExpr, k: u32) -> ValueExpr {
        if self.position_bounds(index).is_some_and(|b| b.unsigned) {
            ValueExpr::U128(u128::from(k))
        } else {
            ValueExpr::Int(i128::from(k))
        }
    }

    pub(super) fn select_values(
        &mut self,
        at: SourceSpan,
        index: &ValueExpr,
        mut entries: Vec<ValueExpr>,
    ) -> ValueExpr {
        if let Some(k) = literal(index) {
            return entries.get(k).cloned().unwrap_or_else(|| {
                self.recursive_error(at, "table index exceeds its checked entries");
                ValueExpr::Int(0)
            });
        }
        // A computed singleton position still evaluates its index, including
        // traps and work. Only the direct literal/bound path above elides it.
        let copies = entries.len().saturating_sub(1).max(1);
        let (nodes, _) = formula_shape_value(index);
        if !self.charge(at, copies.saturating_mul(nodes.saturating_add(3))) {
            return ValueExpr::Int(0);
        }
        let mut value = entries.pop().unwrap_or(ValueExpr::Int(0));
        if entries.is_empty() {
            if !self.charge(at, formula_shape_value(&value).0) {
                return ValueExpr::Int(0);
            }
            return ValueExpr::Select(
                Box::new(RelExpr::Compare(
                    CompareOp::Eq,
                    index.clone(),
                    self.index_number(index, 0),
                )),
                Box::new(value.clone()),
                Box::new(value),
            );
        }
        for (k, entry) in entries.into_iter().enumerate().rev() {
            value = ValueExpr::Select(
                Box::new(RelExpr::Compare(
                    CompareOp::Eq,
                    index.clone(),
                    self.index_number(index, k as u32),
                )),
                Box::new(entry),
                Box::new(value),
            );
        }
        value
    }

    pub(super) fn select_formulas(
        &mut self,
        at: SourceSpan,
        index: &ValueExpr,
        mut entries: Vec<RelExpr>,
    ) -> RelExpr {
        if let Some(k) = literal(index) {
            return entries.get(k).cloned().unwrap_or_else(|| {
                self.recursive_error(at, "table index exceeds its checked entries");
                RelExpr::Bool(false)
            });
        }
        let (n, _) = formula_shape_value(index);
        if !self.charge(at, entries.len().saturating_mul(n.saturating_add(5))) {
            return RelExpr::Bool(false);
        }
        let mut nodes = Vec::new();
        let mut conditions = Vec::new();
        let mut outputs = Vec::new();
        let last = entries.len().saturating_sub(1);
        for (k, entry) in entries.drain(..).enumerate() {
            if k < last || last == 0 {
                conditions.push(nodes.len() as u32);
                nodes.push(BoolNode::Formula(RelExpr::Compare(
                    CompareOp::Eq,
                    index.clone(),
                    self.index_number(index, k as u32),
                )));
            }
            outputs.push(nodes.len() as u32);
            nodes.push(BoolNode::Formula(entry));
        }
        let mut output = outputs.pop().unwrap_or(0);
        if last == 0 {
            nodes.push(BoolNode::Select(0, output, output));
            output = nodes.len() as u32 - 1;
        }
        for (c, a) in conditions.into_iter().zip(outputs).rev() {
            nodes.push(BoolNode::Select(c, a, output));
            output = nodes.len() as u32 - 1;
        }
        BoolCircuit::try_new(nodes, output)
            .map(|c| RelExpr::Circuit(Box::new(c)))
            .unwrap_or(RelExpr::Bool(false))
    }
}

pub(super) fn literal(index: &ValueExpr) -> Option<usize> {
    match index {
        ValueExpr::Int(n) => usize::try_from(*n).ok(),
        ValueExpr::U128(n) => usize::try_from(*n).ok(),
        _ => None,
    }
}
