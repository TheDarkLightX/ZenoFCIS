//! Positive, single-stratum recursive tables lowered to shared Boolean rounds.

use super::*;
use crate::{BoolCircuit, BoolNode};

pub(super) const SELF: &str = "__recursive_entry";

pub(super) struct DerivedTable {
    id: StableId,
    span: SourceSpan,
    pub(super) size: u32,
    pub(super) rank: usize,
    pub(super) circuit: BoolCircuit<RelExpr>,
    pub(super) outputs: Vec<u32>,
    pub(super) bits: Option<u8>,
    pub(super) constants: Option<Box<[bool]>>,
}

/// Temporary references exist only during parsing and cannot escape in a project.
pub(super) fn reads_self(f: &RelExpr) -> bool {
    match f {
        RelExpr::Circuit(c) => c.formulas().any(reads_self),
        RelExpr::Predicate { name, .. } => name.as_str() == SELF,
        RelExpr::Not(a) => reads_self(a),
        RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
            reads_self(a) || reads_self(b)
        }
        RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => reads_self(body),
        _ => false,
    }
}

fn safe(f: &RelExpr, guard: bool) -> bool {
    match f {
        RelExpr::Predicate { name, .. } if name.as_str() == SELF => !guard,
        RelExpr::Bool(_) => true,
        RelExpr::Compare(_, a, b) => lower_value(a) && lower_value(b),
        RelExpr::And(a, b) | RelExpr::Or(a, b) => safe(a, guard) && safe(b, guard),
        RelExpr::Not(a) => !reads_self(a) && safe(a, true),
        RelExpr::Implies(a, b) => !reads_self(f) && safe(a, true) && safe(b, true),
        RelExpr::Circuit(c) => c.nodes().iter().all(|n| match n {
            BoolNode::Formula(f) => safe(f, true),
            _ => true,
        }),
        _ => false,
    }
}

fn lower_value(v: &ValueExpr) -> bool {
    match v {
        ValueExpr::Int(_) | ValueExpr::U128(_) => true,
        ValueExpr::Projection(p) => matches!(
            p.root(),
            ProjectionRoot::Pre
                | ProjectionRoot::Post
                | ProjectionRoot::Command
                | ProjectionRoot::Context
        ),
        ValueExpr::Select(c, a, b) => safe(c, true) && lower_value(a) && lower_value(b),
        ValueExpr::ToI128(a) | ValueExpr::ToU128(a) => lower_value(a),
        ValueExpr::Add(a, b)
        | ValueExpr::Sub(a, b)
        | ValueExpr::Mul(a, b)
        | ValueExpr::Div(_, a, b) => lower_value(a) && lower_value(b),
        ValueExpr::MulDiv(_, a, b, c)
        | ValueExpr::SignedMulDivFloor(a, b, c)
        | ValueExpr::ScaledMulDiv(_, _, a, b, c, _)
        | ValueExpr::RepeatMulDiv(_, a, b, c, _) => {
            lower_value(a) && lower_value(b) && lower_value(c)
        }
        _ => false,
    }
}

impl Parser {
    /// Refuse active self reads before a surrounding expansion can erase them.
    /// This context belongs to one parse and never enters the checked project.
    pub(super) fn with_self_refusal<T>(
        &mut self,
        reason: &'static str,
        parse: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.self_refusal;
        self.self_refusal = previous.or(Some(reason));
        let value = parse(self);
        self.self_refusal = previous;
        value
    }

    pub(super) fn refuse_derived_collisions(&mut self, project: StableId) {
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
        used.extend(self.constants.values().map(|t| t.id.get()));
        let collisions: Vec<SourceSpan> = self
            .derived
            .values()
            .filter_map(|table| (!used.insert(table.id.get())).then_some(table.span))
            .collect();
        for span in collisions {
            self.recursive_error(
                span,
                "a derived table ID must be unused by every declaration and the project",
            );
        }
    }

    pub(super) fn recursive_error(&mut self, at: SourceSpan, reason: &str) {
        self.error_at(
            at,
            DiagnosticCode::InvalidDeclaration,
            "derived.table",
            "positive recursion over lower data",
            reason,
            "use and/or with positive self entries; guards read only lower data",
        );
    }

    /// `derived table ID name[i][j] in table T rounds B { when G then V; default V; };`
    pub(super) fn parse_derived_table(&mut self) {
        let at = self.declaration_start;
        self.expect_keyword("table", "derived.table");
        let Some(id) = self.take_id("derived.id") else {
            self.recover_top();
            return;
        };
        let Some(name) = self.take_identifier("derived.name") else {
            self.recover_top();
            return;
        };
        let mut variables = Vec::new();
        while self.take_symbol(&TokenKind::LBracket) {
            if let Some(v) = self.take_identifier("derived.index") {
                variables.push(v);
            }
            self.expect_symbol(TokenKind::RBracket, "derived.index");
            if variables.len() > 2 {
                self.recursive_error(at, "a derived table has one or two indices");
                self.recover_top();
                return;
            }
        }
        self.expect_keyword("in", "derived.domain");
        self.expect_keyword("table", "derived.domain");
        let table = self.take_u32("derived.domain");
        let bits = self.take_keyword("bits").then(|| self.bits_width());
        self.expect_keyword("rounds", "derived.bound");
        let rounds = self.take_u32("derived.bound").unwrap_or(0);
        let size = table
            .and_then(|t| self.tables.get(&t).copied())
            .unwrap_or(0);
        let rank = variables.len();
        let entries = if rank == 1 {
            size
        } else {
            size.saturating_mul(size)
        };
        let atoms = entries.saturating_mul(u32::from(bits.unwrap_or(1)));
        if size == 0
            || size > MAX_TABLE_ENTRIES
            || rank == 0
            || rounds != atoms
            || rounds.saturating_mul(atoms) as usize > MAX_FORMULA_NODES
        {
            self.recursive_error(at, "declare one or two indices over a fixed-size table and exactly entries * width rounds (Boolean width is one), within the formula limit");
            self.recover_top();
            return;
        }
        if variables.len() == 2 && variables[0] == variables[1]
            || self.derived.contains_key(name.as_str())
            || self.constants.contains_key(name.as_str())
            || matches!(name.as_str(), "circuit" | "subset")
        {
            self.recursive_error(at, "derived names and bound indices must be distinct");
            self.recover_top();
            return;
        }
        self.expect_symbol(TokenKind::LBrace, "derived.rows");
        let start = self.index;
        let mut rules = Vec::new();
        self.table_depth += 1;
        self.defining = Some((name.as_str().into(), size, rank));
        self.defining_bits = bits;
        let before = self.diagnostics.len();
        for entry in 0..entries {
            self.index = start;
            self.bound.push((
                variables[0].as_str().into(),
                Some(i128::from(if rank == 1 { entry } else { entry / size })),
            ));
            if rank == 2 {
                self.bound
                    .push((variables[1].as_str().into(), Some(i128::from(entry % size))));
            }
            let mut rows = Vec::new();
            while self.take_keyword("when") {
                let guard = self
                    .with_self_refusal("a guard cannot read the table being defined", |p| {
                        p.parse_rel()
                    });
                self.expect_keyword("then", "derived.row");
                let value = self.parse_derived_value(bits);
                self.expect_symbol(TokenKind::Semicolon, "derived.row");
                if !safe(&guard, true) {
                    self.recursive_error(
                        at,
                        "a guard cannot read the table being defined or non-lower data",
                    );
                }
                if !value.iter().all(|v| safe(v, false)) {
                    self.recursive_error(at, "self entries occur only positively under and/or and bounded table quantifiers");
                }
                rows.push((guard, value));
            }
            self.expect_keyword("default", "derived.default");
            let default = self.parse_derived_value(bits);
            self.expect_symbol(TokenKind::Semicolon, "derived.default");
            if !default.iter().all(|v| safe(v, false)) {
                self.recursive_error(
                    at,
                    "the explicit default must also be positive in self entries",
                );
            }
            for (atom, default) in default.into_iter().enumerate() {
                rules.push((
                    rows.iter()
                        .map(|(g, v)| (g.clone(), v[atom].clone()))
                        .collect(),
                    default,
                ));
            }
            for _ in 0..rank {
                self.bound.pop();
            }
            if self.diagnostics.len() != before {
                break;
            }
        }
        self.defining = None;
        self.defining_bits = None;
        self.table_depth -= 1;
        self.expect_symbol(TokenKind::RBrace, "derived.rows");
        self.expect_symbol(TokenKind::Semicolon, "derived.table");
        if self.diagnostics.len() != before {
            return;
        }
        match compile(&rules, rounds) {
            Some((circuit, outputs)) if self.charge(at, circuit.nodes().len()) => {
                let literal: fn(&RelExpr) -> Option<bool> = if self.constants.is_empty() {
                    tables::static_truth
                } else {
                    constant_tables::truth
                };
                let constants = circuit
                    .evaluate_all(literal)
                    .and_then(|all| {
                        outputs
                            .iter()
                            .map(|n| all.get(*n as usize).copied())
                            .collect::<Option<Vec<_>>>()
                    })
                    .map(Vec::into_boxed_slice);
                self.derived.insert(
                    name.as_str().into(),
                    DerivedTable {
                        id,
                        span: at,
                        size,
                        rank,
                        circuit,
                        outputs,
                        bits,
                        constants,
                    },
                );
                self.rewrite_declaration(String::new());
            }
            _ => self.error_at(
                at,
                DiagnosticCode::ResourceLimit,
                "derived.table",
                "bounded shared rounds",
                "recursive expansion exceeds the formula limit",
                "use a smaller domain or rule",
            ),
        }
    }

    /// A direct Boolean entry reference, including temporary positive self reads.
    pub(super) fn derived_atom(&mut self) -> Option<RelExpr> {
        if self.is_bits_entry() || self.constant_name() {
            return None;
        }
        let TokenKind::Ident(name) = self.current().kind.clone() else {
            return None;
        };
        if !matches!(
            self.tokens.get(self.index + 1).map(|t| &t.kind),
            Some(TokenKind::LBracket)
        ) {
            return None;
        }
        let at = self.current().span;
        self.advance();
        let mut indices = Vec::new();
        while self.take_symbol(&TokenKind::LBracket) {
            let index = match self.current().kind.clone() {
                TokenKind::Number(n) => {
                    self.advance();
                    u32::try_from(n).ok()
                }
                TokenKind::Ident(v) => {
                    self.advance();
                    self.index_value(&v).and_then(|n| u32::try_from(n).ok())
                }
                _ => None,
            };
            self.expect_symbol(TokenKind::RBracket, "derived.entry");
            indices.push(index);
        }
        let own = self
            .defining
            .as_ref()
            .is_some_and(|(n, _, _)| n == name.as_ref());
        let dimensions = self
            .defining
            .as_ref()
            .filter(|(n, _, _)| n == name.as_ref())
            .map(|(_, s, r)| (*s, *r))
            .or_else(|| self.derived.get(name.as_ref()).map(|t| (t.size, t.rank)));
        let Some((size, rank)) = dimensions else {
            self.recursive_error(
                at,
                "unknown or later-declared derived table; mutual recursion is refused",
            );
            return Some(RelExpr::Bool(false));
        };
        if indices.len() != rank || indices.iter().any(|n| n.is_none_or(|n| n >= size)) {
            self.recursive_error(
                at,
                "derived indices must be bound table indices or in-range literals",
            );
            return Some(RelExpr::Bool(false));
        }
        let entry = indices.iter().fold(0u32, |n, i| n * size + i.unwrap_or(0));
        if own && let Some(reason) = self.self_refusal {
            self.recursive_error(at, reason);
        }
        let value = if own {
            RelExpr::Predicate {
                name: Identifier::try_new(SELF).unwrap_or_else(fallback_identifier),
                arguments: vec![ValueExpr::Int(i128::from(entry))].into_boxed_slice(),
            }
        } else {
            let t = &self.derived[name.as_ref()];
            if let Some(constants) = &t.constants {
                RelExpr::Bool(constants[entry as usize])
            } else {
                t.circuit
                    .at_output(t.outputs[entry as usize])
                    .map(|c| RelExpr::Circuit(Box::new(c)))
                    .unwrap_or(RelExpr::Bool(false))
            }
        };
        if self.table_depth == 0 {
            self.rewrite_from(at, tables::rel_text(&value));
        }
        Some(value)
    }

    /// The stable scalar graph syntax printed by `check --expand`.
    pub(super) fn parse_circuit(&mut self) -> RelExpr {
        let at = self.current().span;
        self.take_keyword("circuit");
        self.expect_symbol(TokenKind::LBrace, "circuit");
        let mut nodes = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof)
            && nodes.len() <= MAX_FORMULA_NODES
        {
            let node = if self.take_keyword("formula") {
                self.expect_symbol(TokenKind::LBrace, "circuit.formula");
                let f = self.nested("circuit.formula", RelExpr::Bool(false), |p| p.parse_rel());
                self.expect_symbol(TokenKind::RBrace, "circuit.formula");
                BoolNode::Formula(f)
            } else if self.take_keyword("true") {
                BoolNode::Literal(true)
            } else if self.take_keyword("false") {
                BoolNode::Literal(false)
            } else if self.take_keyword("not") {
                BoolNode::Not(self.take_u32("circuit.ref").unwrap_or(u32::MAX))
            } else if self.peek_keyword("and") || self.peek_keyword("or") {
                let and = self.take_keyword("and");
                if !and {
                    self.take_keyword("or");
                }
                let a = self.take_u32("circuit.ref").unwrap_or(u32::MAX);
                let b = self.take_u32("circuit.ref").unwrap_or(u32::MAX);
                if and {
                    BoolNode::And(a, b)
                } else {
                    BoolNode::Or(a, b)
                }
            } else if self.take_keyword("select") {
                BoolNode::Select(
                    self.take_u32("circuit.ref").unwrap_or(u32::MAX),
                    self.take_u32("circuit.ref").unwrap_or(u32::MAX),
                    self.take_u32("circuit.ref").unwrap_or(u32::MAX),
                )
            } else {
                self.recursive_error(at, "unknown Boolean circuit operation");
                self.recover_top();
                return RelExpr::Bool(false);
            };
            nodes.push(node);
            self.expect_symbol(TokenKind::Semicolon, "circuit.node");
        }
        self.expect_symbol(TokenKind::RBrace, "circuit");
        self.expect_keyword("output", "circuit.output");
        let output = self.take_u32("circuit.output").unwrap_or(u32::MAX);
        match BoolCircuit::try_new(nodes, output) {
            Some(c) => RelExpr::Circuit(Box::new(c)),
            None => {
                self.recursive_error(
                    at,
                    "circuit references must be backward and its output must exist",
                );
                RelExpr::Bool(false)
            }
        }
    }
}

#[derive(Default)]
struct CircuitBuilder {
    nodes: Vec<BoolNode<RelExpr>>,
    leaves: BTreeMap<String, u32>,
    operations: BTreeMap<(u8, u32, u32, u32), u32>,
    graphs: Vec<(BoolCircuit<RelExpr>, Vec<u32>)>,
}

impl CircuitBuilder {
    fn put(&mut self, node: BoolNode<RelExpr>) -> Option<u32> {
        let key = match &node {
            BoolNode::Formula(_) => None,
            BoolNode::Literal(v) => Some((0, u32::from(*v), 0, 0)),
            BoolNode::Not(a) => Some((1, *a, 0, 0)),
            BoolNode::And(a, b) => Some((2, *a, *b, 0)),
            BoolNode::Or(a, b) => Some((3, *a, *b, 0)),
            BoolNode::Select(c, a, b) => Some((4, *c, *a, *b)),
        };
        if let Some(n) = key.and_then(|k| self.operations.get(&k)) {
            return Some(*n);
        }
        if self.nodes.len() >= MAX_FORMULA_NODES {
            return None;
        }
        let n = u32::try_from(self.nodes.len()).ok()?;
        self.nodes.push(node);
        if let Some(key) = key {
            self.operations.insert(key, n);
        }
        Some(n)
    }
    fn formula(&mut self, f: &RelExpr, previous: &[u32]) -> Option<u32> {
        Some(match f {
            RelExpr::Bool(v) => self.put(BoolNode::Literal(*v))?,
            RelExpr::Predicate { name, arguments } if name.as_str() == SELF => {
                let [ValueExpr::Int(entry)] = arguments.as_ref() else {
                    return None;
                };
                *previous.get(usize::try_from(*entry).ok()?)?
            }
            RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
                let a = self.formula(a, previous)?;
                let b = self.formula(b, previous)?;
                let node = match f {
                    RelExpr::And(..) => BoolNode::And(a, b),
                    RelExpr::Or(..) => BoolNode::Or(a, b),
                    _ => {
                        let a = self.put(BoolNode::Not(a))?;
                        BoolNode::Or(a, b)
                    }
                };
                self.put(node)?
            }
            RelExpr::Not(a) => {
                let a = self.formula(a, previous)?;
                self.put(BoolNode::Not(a))?
            }
            RelExpr::Circuit(c) => {
                if let Some((_, map)) = self.graphs.iter().find(|(g, _)| g.nodes() == c.nodes()) {
                    return map.get(c.output() as usize).copied();
                }
                let mut map = Vec::new();
                for n in c.nodes() {
                    let n = match n {
                        BoolNode::Formula(f) => {
                            let i = self.formula(f, previous)?;
                            map.push(i);
                            continue;
                        }
                        BoolNode::Literal(v) => BoolNode::Literal(*v),
                        BoolNode::Not(a) => BoolNode::Not(map[*a as usize]),
                        BoolNode::And(a, b) => BoolNode::And(map[*a as usize], map[*b as usize]),
                        BoolNode::Or(a, b) => BoolNode::Or(map[*a as usize], map[*b as usize]),
                        BoolNode::Select(c, a, b) => {
                            BoolNode::Select(map[*c as usize], map[*a as usize], map[*b as usize])
                        }
                    };
                    map.push(self.put(n)?);
                }
                let output = *map.get(c.output() as usize)?;
                self.graphs.push(((**c).clone(), map));
                output
            }
            _ => {
                let key = tables::rel_text(f);
                if let Some(i) = self.leaves.get(&key) {
                    return Some(*i);
                }
                let n = self.put(BoolNode::Formula(f.clone()))?;
                self.leaves.insert(key, n);
                n
            }
        })
    }
}

fn compile(
    rules: &[(Vec<(RelExpr, RelExpr)>, RelExpr)],
    rounds: u32,
) -> Option<(BoolCircuit<RelExpr>, Vec<u32>)> {
    let mut b = CircuitBuilder::default();
    let initial = b.put(BoolNode::Literal(false))?;
    let mut previous = vec![initial; rules.len()];
    for _ in 0..rounds {
        let mut next = Vec::new();
        for (rows, default) in rules {
            let mut value = b.formula(default, &previous)?;
            for (guard, result) in rows.iter().rev() {
                let guard = b.formula(guard, &previous)?;
                let result = b.formula(result, &previous)?;
                value = b.put(BoolNode::Select(guard, result, value))?;
            }
            next.push(value);
        }
        previous = next;
    }
    let c = BoolCircuit::try_new(b.nodes, *previous.first()?)?;
    Some((c, previous))
}

fn has_circuit(f: &RelExpr) -> bool {
    match f {
        RelExpr::Circuit(_) => true,
        RelExpr::Not(a) => has_circuit(a),
        RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
            has_circuit(a) || has_circuit(b)
        }
        _ => false,
    }
}

pub(super) fn share(f: RelExpr) -> Option<RelExpr> {
    if !has_circuit(&f) {
        return Some(f);
    }
    shared(f)
}

pub(super) fn shared(f: RelExpr) -> Option<RelExpr> {
    let mut b = CircuitBuilder::default();
    let output = b.formula(&f, &[])?;
    Some(RelExpr::Circuit(Box::new(BoolCircuit::try_new(
        b.nodes, output,
    )?)))
}
