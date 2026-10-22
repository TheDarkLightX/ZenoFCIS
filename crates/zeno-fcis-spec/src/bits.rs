//! Finite Boolean algebras lowered atom by atom to the Boolean round compiler.
use super::*;

fn mask(width: u8) -> u128 {
    u128::MAX >> (128 - width)
}

fn pack(atoms: Vec<RelExpr>) -> ValueExpr {
    if let Some(values) = atoms
        .iter()
        .map(tables::static_truth)
        .collect::<Option<Vec<_>>>()
    {
        return ValueExpr::U128(
            values
                .iter()
                .enumerate()
                .fold(0, |n, (k, b)| n | (u128::from(*b) << k)),
        );
    }
    fn total(mut terms: Vec<ValueExpr>) -> ValueExpr {
        match terms.len() {
            0 => ValueExpr::U128(0),
            1 => terms.pop().unwrap_or(ValueExpr::U128(0)),
            n => {
                let right = terms.split_off(n / 2);
                ValueExpr::Add(Box::new(total(terms)), Box::new(total(right)))
            }
        }
    }
    total(
        atoms
            .into_iter()
            .enumerate()
            .map(|(k, f)| {
                ValueExpr::Select(
                    Box::new(f),
                    Box::new(ValueExpr::U128(1u128 << k)),
                    Box::new(ValueExpr::U128(0)),
                )
            })
            .collect(),
    )
}

impl Parser {
    pub(super) fn bits_width(&mut self) -> u8 {
        let at = self.current().span;
        match self.take_u32("bits.width") {
            Some(w @ 1..=128) => w as u8,
            _ => {
                self.recursive_error(at, "bits width must lie in 1..=128");
                1
            }
        }
    }
    pub(super) fn parse_bits_type(&mut self, id: StableId) -> Option<TypeDecl> {
        let name = self.take_identifier("bits.name")?;
        self.expect_keyword("width", "bits.type");
        let width = self.bits_width();
        self.expect_symbol(TokenKind::Semicolon, "bits.type");
        self.rewrite_declaration(format!(
            "type {} u128 {} in 0..={};",
            id.get(),
            name.as_str(),
            mask(width)
        ));
        TypeDecl::new(id, TypeKind::U128, name).with_unsigned_range(0, mask(width))
    }
    pub(super) fn parse_derived_value(&mut self, width: Option<u8>) -> Vec<RelExpr> {
        match width {
            Some(w) => self.parse_bits(w),
            None => vec![self.parse_rel()],
        }
    }
    pub(super) fn is_bits_entry(&self) -> bool {
        let TokenKind::Ident(name) = &self.current().kind else {
            return false;
        };
        self.derived
            .get(name.as_ref())
            .is_some_and(|d| d.bits.is_some())
            || self
                .defining
                .as_ref()
                .is_some_and(|(n, _, _)| n == name.as_ref())
                && self.defining_bits.is_some()
    }
    /// Outside a definition, algebra helpers carry an explicit literal width.
    pub(super) fn bits_value(&mut self) -> Option<ValueExpr> {
        if self.is_bits_entry() {
            let width = if self.defining.as_ref().is_some_and(
                |(n, _, _)| matches!(&self.current().kind,TokenKind::Ident(t) if t.as_ref()==n),
            ) {
                self.defining_bits.unwrap_or(1)
            } else if let TokenKind::Ident(n) = &self.current().kind {
                self.derived[n.as_ref()].bits.unwrap_or(1)
            } else {
                1
            };
            let atoms = self.bits_entry(width);
            let value = pack(atoms);
            self.table_values += 1;
            return Some(value);
        }
        if !["join", "meet", "complement"]
            .iter()
            .any(|n| self.peek_keyword(n))
        {
            return None;
        }
        let name = self
            .take_identifier("bits.operation")
            .unwrap_or_else(fallback_identifier);
        self.expect_symbol(TokenKind::LParen, "bits.operation");
        let width = self.bits_width();
        self.expect_symbol(TokenKind::Comma, "bits.operation");
        let a = self.parse_bits(width);
        let atoms = if name.as_str() == "complement" {
            a.into_iter().map(|a| RelExpr::Not(Box::new(a))).collect()
        } else {
            self.expect_symbol(TokenKind::Comma, "bits.operation");
            let b = self.parse_bits(width);
            a.into_iter()
                .zip(b)
                .map(|(a, b)| {
                    if name.as_str() == "join" {
                        RelExpr::Or(Box::new(a), Box::new(b))
                    } else {
                        RelExpr::And(Box::new(a), Box::new(b))
                    }
                })
                .collect()
        };
        self.expect_symbol(TokenKind::RParen, "bits.operation");
        let value = pack(atoms);
        self.table_values += 1;
        Some(value)
    }
    pub(super) fn parse_subset(&mut self) -> RelExpr {
        let at = self.current().span;
        self.take_keyword("subset");
        self.expect_symbol(TokenKind::LParen, "bits.subset");
        let width = self.bits_width();
        self.expect_symbol(TokenKind::Comma, "bits.subset");
        let (a, b) =
            self.with_self_refusal("a comparison cannot read the table being defined", |p| {
                let a = p.parse_bits(width);
                p.expect_symbol(TokenKind::Comma, "bits.subset");
                let b = p.parse_bits(width);
                (a, b)
            });
        self.expect_symbol(TokenKind::RParen, "bits.subset");
        let formula = tables::chain(
            a.into_iter()
                .zip(b)
                .map(|(a, b)| RelExpr::Or(Box::new(RelExpr::Not(Box::new(a))), Box::new(b)))
                .collect(),
            RelExpr::And,
            true,
        );
        let formula = recursive_tables::shared(formula).unwrap_or_else(|| {
            self.recursive_error(at, "bit comparison exceeds the formula limit");
            RelExpr::Bool(false)
        });
        if self.table_depth == 0 {
            self.rewrite_from(at, format!("({})", tables::rel_text(&formula)));
        }
        formula
    }
    fn parse_bits(&mut self, width: u8) -> Vec<RelExpr> {
        let zero = || vec![RelExpr::Bool(false); width as usize];
        self.nested("bits.value", zero(), |p| p.parse_bits_atom(width))
    }
    fn parse_bits_atom(&mut self, width: u8) -> Vec<RelExpr> {
        let at = self.current().span;
        if self.is_bits_entry() {
            return self.bits_entry(width);
        }
        if self.peek_keyword("join") || self.peek_keyword("meet") {
            let join = self.take_keyword("join");
            if !join {
                self.take_keyword("meet");
            }
            if self.table_form_ahead(0) {
                let Some((var, size)) = self.table_header(at) else {
                    return vec![RelExpr::Bool(false); width as usize];
                };
                self.table_depth += 1;
                let terms = self.per_entry(&var, size, |p| {
                    let guard = p.take_keyword("where").then(|| {
                        p.with_self_refusal(
                            "a quantifier guard cannot read the table being defined",
                            |q| q.parse_rel(),
                        )
                    });
                    p.expect_symbol(TokenKind::LBrace, "bits.fold");
                    let value = p.parse_bits(width);
                    p.expect_symbol(TokenKind::RBrace, "bits.fold");
                    value
                        .into_iter()
                        .map(|v| match &guard {
                            None => v,
                            Some(g) if join => RelExpr::And(Box::new(g.clone()), Box::new(v)),
                            Some(g) => RelExpr::Or(
                                Box::new(RelExpr::Not(Box::new(g.clone()))),
                                Box::new(v),
                            ),
                        })
                        .collect::<Vec<_>>()
                });
                self.table_depth -= 1;
                let nodes = terms.iter().flatten().map(|f| formula_shape_rel(f).0).sum();
                self.charge(at, nodes);
                return (0..width as usize)
                    .map(|k| {
                        tables::chain(
                            terms.iter().map(|t| t[k].clone()).collect(),
                            if join { RelExpr::Or } else { RelExpr::And },
                            !join,
                        )
                    })
                    .collect();
            }
            self.expect_symbol(TokenKind::LParen, "bits.operation");
            let a = self.parse_bits(width);
            self.expect_symbol(TokenKind::Comma, "bits.operation");
            let b = self.parse_bits(width);
            self.expect_symbol(TokenKind::RParen, "bits.operation");
            return a
                .into_iter()
                .zip(b)
                .map(|(a, b)| {
                    if join {
                        RelExpr::Or(Box::new(a), Box::new(b))
                    } else {
                        RelExpr::And(Box::new(a), Box::new(b))
                    }
                })
                .collect();
        }
        if self.take_keyword("complement") {
            self.expect_symbol(TokenKind::LParen, "bits.complement");
            let value = self
                .with_self_refusal("a complement cannot read the table being defined", |p| {
                    p.parse_bits(width)
                });
            self.expect_symbol(TokenKind::RParen, "bits.complement");
            return value
                .into_iter()
                .map(|v| RelExpr::Not(Box::new(v)))
                .collect();
        }
        let value = self.with_self_refusal(
            "a lower bit value cannot read the table being defined",
            |p| p.parse_value(),
        );
        if let Some(n) = match &value {
            ValueExpr::U128(n) => Some(*n),
            ValueExpr::Int(n) => u128::try_from(*n).ok(),
            _ => None,
        } {
            if n <= mask(width) {
                return (0..width)
                    .map(|k| RelExpr::Bool(n & (1u128 << k) != 0))
                    .collect();
            }
        } else if self.word_bound(&value).is_some_and(|n| n <= mask(width)) {
            return (0..width)
                .map(|k| {
                    let q = if k == 0 {
                        value.clone()
                    } else {
                        ValueExpr::Div(
                            DivisionMode::Floor,
                            Box::new(value.clone()),
                            Box::new(ValueExpr::U128(1u128 << k)),
                        )
                    };
                    let bit = if k == 127 {
                        q
                    } else {
                        let upper = ValueExpr::Div(
                            DivisionMode::Floor,
                            Box::new(value.clone()),
                            Box::new(ValueExpr::U128(1u128 << (k + 1))),
                        );
                        ValueExpr::Sub(
                            Box::new(q),
                            Box::new(ValueExpr::Mul(
                                Box::new(ValueExpr::U128(2)),
                                Box::new(upper),
                            )),
                        )
                    };
                    RelExpr::Compare(CompareOp::Eq, bit, ValueExpr::U128(1))
                })
                .collect();
        }
        self.recursive_error(
            at,
            "a lower bit value must be an in-range literal or a bounded unsigned projection",
        );
        vec![RelExpr::Bool(false); width as usize]
    }
    fn word_bound(&self, value: &ValueExpr) -> Option<u128> {
        let ValueExpr::Projection(p) = value else {
            return None;
        };
        if !matches!(
            p.root(),
            ProjectionRoot::Pre
                | ProjectionRoot::Post
                | ProjectionRoot::Command
                | ProjectionRoot::Context
        ) {
            return None;
        }
        let [owner, field] = p.segments() else {
            return None;
        };
        let field = self
            .declarations
            .iter()
            .find_map(|d| match &d.declaration {
                Declaration::Field(f) if f.id() == *field && f.owner() == *owner => Some(f),
                _ => None,
            })?;
        self.declarations.iter().find_map(|d| match &d.declaration {
            Declaration::Type(t) if t.id() == field.field_type() => {
                t.unsigned_range().map(|(_, n)| n)
            }
            _ => None,
        })
    }
    fn bits_entry(&mut self, width: u8) -> Vec<RelExpr> {
        let at = self.current().span;
        let name = self
            .take_identifier("bits.entry")
            .unwrap_or_else(fallback_identifier);
        let own_dimensions = self
            .defining
            .as_ref()
            .filter(|(n, _, _)| n == name.as_str())
            .map(|(_, s, r)| (*s, *r, self.defining_bits));
        let own = own_dimensions.is_some();
        let dimensions = own_dimensions.or_else(|| {
            self.derived
                .get(name.as_str())
                .map(|d| (d.size, d.rank, d.bits))
        });
        let Some((size, rank, actual)) = dimensions else {
            self.recursive_error(at, "unknown or later-declared bit table");
            return vec![RelExpr::Bool(false); width as usize];
        };
        let mut indices = Vec::new();
        while self.take_symbol(&TokenKind::LBracket) {
            let n = match &self.current().kind {
                TokenKind::Number(n) => u32::try_from(*n).ok(),
                TokenKind::Ident(n) => self.index_value(n).and_then(|n| u32::try_from(n).ok()),
                _ => None,
            };
            self.advance();
            self.expect_symbol(TokenKind::RBracket, "bits.index");
            indices.push(n);
            if indices.len() > 2 {
                break;
            }
        }
        if actual != Some(width)
            || indices.len() != rank
            || indices.iter().any(|n| n.is_none_or(|n| n >= size))
        {
            self.recursive_error(
                at,
                "bit entry width and bound/literal indices must match its declaration",
            );
            return vec![RelExpr::Bool(false); width as usize];
        }
        let entry = indices.iter().fold(0, |n, i| n * size + i.unwrap_or(0));
        if own {
            if let Some(reason) = self.self_refusal {
                self.recursive_error(at, reason);
            }
            return (0..width)
                .map(|k| RelExpr::Predicate {
                    name: Identifier::try_new(recursive_tables::SELF)
                        .unwrap_or_else(fallback_identifier),
                    arguments: vec![ValueExpr::Int(
                        i128::from(entry) * i128::from(width) + i128::from(k),
                    )]
                    .into_boxed_slice(),
                })
                .collect();
        }
        let d = &self.derived[name.as_str()];
        if let Some(constants) = &d.constants {
            return (0..width)
                .map(|k| RelExpr::Bool(constants[entry as usize * width as usize + k as usize]))
                .collect();
        }
        (0..width)
            .map(|k| {
                d.circuit
                    .at_output(d.outputs[entry as usize * width as usize + k as usize])
                    .map(|c| RelExpr::Circuit(Box::new(c)))
                    .unwrap_or(RelExpr::Bool(false))
            })
            .collect()
    }
}
