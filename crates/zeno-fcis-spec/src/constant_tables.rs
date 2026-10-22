//! Fixed policy data; entries lower to typed literals and add no state fields.
use super::*;

#[derive(Clone)]
pub(super) enum Constant {
    Bool(bool),
    Int(i128),
    Unsigned(u128),
}
pub(super) struct ConstantTable {
    pub(super) id: StableId,
    pub(super) span: SourceSpan,
    pub(super) values: Box<[Constant]>,
}

fn scalar_signed_literal(value: i128) -> ValueExpr {
    // Match parse_value_atom's form for `-n`; MIN has a dedicated literal.
    match value.checked_neg() {
        Some(magnitude) if value < 0 => ValueExpr::Sub(
            Box::new(ValueExpr::Int(0)),
            Box::new(ValueExpr::Int(magnitude)),
        ),
        _ => ValueExpr::Int(value),
    }
}

pub(super) fn prescan(tokens: &[Token]) -> BTreeMap<u32, u32> {
    let mut tables = BTreeMap::new();
    for window in tokens.windows(7) {
        if let [
            Token {
                kind: TokenKind::Ident(c),
                ..
            },
            Token {
                kind: TokenKind::Ident(t),
                ..
            },
            Token {
                kind: TokenKind::Number(id),
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
                kind: TokenKind::Ident(s),
                ..
            },
            Token {
                kind: TokenKind::Number(n),
                ..
            },
        ] = window
            && c.as_ref() == "const"
            && t.as_ref() == "table"
            && s.as_ref() == "size"
            && let (Ok(id), Ok(n)) = (u32::try_from(*id), u32::try_from(*n))
        {
            tables.insert(id, n);
        }
    }
    tables
}
/// Literal-only extension for new constant-table sources. Legacy macro folding
/// keeps its original representation on sources without constant declarations.
pub(super) fn truth(f: &RelExpr) -> Option<bool> {
    if let RelExpr::Compare(op, ValueExpr::U128(a), ValueExpr::U128(b)) = f {
        let order = a.cmp(b);
        return Some(match op {
            CompareOp::Eq => order.is_eq(),
            CompareOp::NotEq => !order.is_eq(),
            CompareOp::Less => order.is_lt(),
            CompareOp::LessEq => !order.is_gt(),
            CompareOp::Greater => order.is_gt(),
            CompareOp::GreaterEq => !order.is_lt(),
        });
    }
    tables::static_truth(f)
}
impl Parser {
    pub(super) fn parse_constant_table(&mut self) {
        let at = self.declaration_start;
        self.expect_keyword("table", "constant.table");
        let Some(id) = self.take_id("constant.id") else {
            self.recover_top();
            return;
        };
        let Some(name) = self.take_identifier("constant.name") else {
            self.recover_top();
            return;
        };
        let ty = self.take_id("constant.type");
        self.expect_keyword("size", "constant.size");
        let size = self.take_u32("constant.size").unwrap_or(0);
        let declared = ty.and_then(|id| {
            self.declarations.iter().find_map(|d| match &d.declaration {
                Declaration::Type(t) if t.id() == id => Some(t.clone()),
                _ => None,
            })
        });
        if !(1..=MAX_TABLE_ENTRIES).contains(&size)
            || declared.as_ref().is_none_or(|t| {
                !matches!(t.kind(), TypeKind::Bool | TypeKind::Int | TypeKind::U128)
            })
            || self.constants.contains_key(name.as_str())
            || self.derived.contains_key(name.as_str())
            || matches!(
                name.as_str(),
                "pre"
                    | "post"
                    | "command"
                    | "context"
                    | "true"
                    | "false"
                    | "subset"
                    | "circuit"
                    | "choose"
                    | "join"
                    | "meet"
                    | "complement"
                    | "sum"
                    | "count"
                    | "forall"
                    | "exists"
            )
        {
            self.recursive_error(
                at,
                "a constant table needs a distinct name, an earlier scalar type and size1..64",
            );
            self.recover_top();
            return;
        }
        self.expect_symbol(TokenKind::LBracket, "constant.values");
        let mut values = Vec::new();
        let before = self.diagnostics.len();
        for k in 0..size {
            if k != 0 {
                self.expect_symbol(TokenKind::Comma, "constant.values");
            }
            let value = if self.take_keyword("true") {
                Some(Constant::Bool(true))
            } else if self.take_keyword("false") {
                Some(Constant::Bool(false))
            } else if self.take_keyword("u128") {
                self.expect_symbol(TokenKind::LParen, "constant.unsigned");
                let n = self.take_unsigned("constant.unsigned");
                self.expect_symbol(TokenKind::RParen, "constant.unsigned");
                n.map(Constant::Unsigned)
            } else {
                self.take_i128("constant.integer").map(Constant::Int)
            };
            let valid = match (&declared, &value) {
                (Some(t), Some(Constant::Bool(_))) => t.kind() == TypeKind::Bool,
                (Some(t), Some(Constant::Int(n))) => {
                    t.kind() == TypeKind::Int
                        && t.range().is_none_or(|r| (r.min()..=r.max()).contains(n))
                }
                (Some(t), Some(Constant::Unsigned(n))) => {
                    t.kind() == TypeKind::U128
                        && t.unsigned_range()
                            .is_some_and(|(lo, hi)| (lo..=hi).contains(n))
                }
                _ => false,
            };
            if !valid {
                self.recursive_error(
                    at,
                    "constant entries must be literals within the declared scalar type",
                );
            }
            if let Some(value) = value {
                values.push(value);
            }
            if self.diagnostics.len() != before {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBracket, "constant.values");
        self.expect_symbol(TokenKind::Semicolon, "constant.table");
        if self.diagnostics.len() == before {
            self.constants.insert(
                name.as_str().into(),
                ConstantTable {
                    id,
                    span: at,
                    values: values.into_boxed_slice(),
                },
            );
            self.rewrite_declaration(String::new());
        }
    }
    pub(super) fn refuse_constant_collisions(&mut self, project: StableId) {
        let mut used: BTreeSet<u32> = self
            .declarations
            .iter()
            .flat_map(|d| {
                declaration_ids(&d.declaration)
                    .into_iter()
                    .map(|(id, _)| id)
            })
            .collect();
        used.insert(project.get());
        let collisions: Vec<_> = self
            .constants
            .values()
            .filter_map(|t| (!used.insert(t.id.get())).then_some(t.span))
            .collect();
        for at in collisions {
            self.recursive_error(
                at,
                "a constant table ID must be unused by every declaration and the project",
            );
        }
    }
    pub(super) fn constant_name(&self) -> bool {
        matches!(&self.current().kind,TokenKind::Ident(n) if self.constants.contains_key(n.as_ref()))
    }
    fn constant_entries(&mut self) -> Option<(ValueExpr, Vec<Constant>)> {
        let TokenKind::Ident(name) = self.current().kind.clone() else {
            return None;
        };
        let size = self.constants.get(name.as_ref())?.values.len() as u32;
        let at = self.current().span;
        self.advance();
        self.expect_symbol(TokenKind::LBracket, "constant.index");
        let index = self.position(size, at);
        self.expect_symbol(TokenKind::RBracket, "constant.index");
        Some((index?, self.constants.get(name.as_ref())?.values.to_vec()))
    }
    pub(super) fn constant_bool(&mut self) -> Option<RelExpr> {
        let TokenKind::Ident(n) = &self.current().kind else {
            return None;
        };
        if !self
            .constants
            .get(n.as_ref())
            .is_some_and(|t| matches!(t.values.first(), Some(Constant::Bool(_))))
        {
            return None;
        }
        let at = self.current().span;
        let value = match self.constant_entries() {
            Some((index, values)) => self.select_formulas(
                at,
                &index,
                values
                    .into_iter()
                    .map(|v| match v {
                        Constant::Bool(b) => RelExpr::Bool(b),
                        _ => RelExpr::Bool(false),
                    })
                    .collect(),
            ),
            None => RelExpr::Bool(false),
        };
        if self.table_depth == 0 {
            self.rewrite_from(at, tables::rel_text(&value));
        }
        Some(value)
    }
    pub(super) fn constant_value(&mut self) -> Option<ValueExpr> {
        if !self.constant_name() {
            return None;
        }
        let at = self.current().span;
        let value = match self.constant_entries() {
            Some((index, values)) if values.iter().all(|v| !matches!(v, Constant::Bool(_))) => {
                let values = values
                    .into_iter()
                    .map(|v| match v {
                        Constant::Int(n) => scalar_signed_literal(n),
                        Constant::Unsigned(n) => ValueExpr::U128(n),
                        Constant::Bool(_) => ValueExpr::Int(0),
                    })
                    .collect();
                self.select_values(at, &index, values)
            }
            Some(_) => {
                self.recursive_error(
                    at,
                    "Boolean constant entries are formulas, not numeric values",
                );
                ValueExpr::Int(0)
            }
            None => ValueExpr::Int(0),
        };
        self.table_values += 1;
        Some(value)
    }
}
