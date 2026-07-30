//! Recovering parser for language version 1.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::ast::*;
use crate::diagnostic::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, text,
};
use crate::lexer::{Token, TokenKind, lex};
use crate::{MAX_FORMULA_DEPTH, SourceLimits, ZENO_DSL_VERSION};

/// Parses one bounded `.zeno` source file and accumulates deterministic diagnostics.
pub fn parse_project(source: &str, limits: SourceLimits) -> Result<ParsedProject, DiagnosticSet> {
    let (tokens, diagnostics) = lex(source, limits);
    let mut parser = Parser {
        tokens,
        index: 0,
        diagnostics,
        declarations: Vec::new(),
        merge_order: Vec::new(),
        nesting: 0,
    };
    let parsed = parser.parse(limits.max_diagnostics());
    if parser.diagnostics.is_empty() {
        Ok(parsed)
    } else {
        Err(DiagnosticSet::from_vec(
            parser.diagnostics,
            limits.max_diagnostics(),
        ))
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<Diagnostic>,
    declarations: Vec<SpannedDeclaration>,
    merge_order: Vec<StableId>,
    nesting: usize,
}

impl Parser {
    fn parse(&mut self, diagnostic_limit: usize) -> ParsedProject {
        let version = if self.take_keyword("zeno") {
            self.take_u16("header.version").unwrap_or(0)
        } else {
            self.error(
                DiagnosticCode::MissingHeader,
                "header",
                "zeno <version>;",
                "missing zeno header",
                "start the file with `zeno 1;`",
            );
            0
        };
        self.expect_symbol(TokenKind::Semicolon, "header.version");
        if version != ZENO_DSL_VERSION {
            self.error(
                DiagnosticCode::UnsupportedVersion,
                "header.version",
                text(ZENO_DSL_VERSION),
                text(version),
                "select the supported language version",
            );
        }
        let (project_id, name) = if self.take_keyword("project") {
            let id = self.take_id("project.id").unwrap_or_else(one_id);
            let name = self
                .take_identifier("project.name")
                .unwrap_or_else(fallback_identifier);
            self.expect_symbol(TokenKind::Semicolon, "project");
            (id, name)
        } else {
            self.error(
                DiagnosticCode::MissingHeader,
                "project",
                "project <id> <name>;",
                "missing project declaration",
                "add one explicit project declaration after the version header",
            );
            (one_id(), fallback_identifier())
        };
        while !matches!(self.current().kind, TokenKind::Eof) {
            let start = self.current().span;
            let result = if self.take_keyword("namespace") {
                self.parse_namespace().map(Declaration::Namespace)
            } else if self.take_keyword("type") {
                self.parse_type().map(Declaration::Type)
            } else if self.take_keyword("field") {
                self.parse_field().map(Declaration::Field)
            } else if self.take_keyword("variant") {
                self.parse_variant().map(Declaration::Variant)
            } else if self.take_keyword("reason") {
                self.parse_reason().map(Declaration::Reason)
            } else if self.take_keyword("effect") {
                self.parse_effect().map(Declaration::Effect)
            } else if self.take_keyword("channel") {
                self.parse_channel().map(Declaration::Channel)
            } else if self.take_keyword("component") {
                self.parse_component().map(Declaration::Component)
            } else if self.take_keyword("wire") {
                self.parse_wiring().map(Declaration::Wiring)
            } else if self.take_keyword("merge") {
                self.parse_merge();
                None
            } else if self.take_keyword("law") {
                self.parse_law().map(Declaration::Law)
            } else if self.take_keyword("claim") {
                self.parse_claim().map(Declaration::Claim)
            } else {
                self.error(DiagnosticCode::UnexpectedToken, "project.declarations", "top-level declaration", self.describe_current(), "use a version-1 declaration keyword and terminate declarations with semicolons");
                self.recover_top();
                None
            };
            if let Some(declaration) = result {
                self.declarations.push(SpannedDeclaration {
                    declaration,
                    span: start,
                });
            }
        }
        ParsedProject {
            version,
            project_id,
            name,
            declarations: core::mem::take(&mut self.declarations),
            merge_order: core::mem::take(&mut self.merge_order),
            diagnostic_limit,
        }
    }

    fn parse_namespace(&mut self) -> Option<NamespaceDecl> {
        let id = self.take_id("namespace.id")?;
        let name = self.take_identifier("namespace.name")?;
        self.expect_symbol(TokenKind::Semicolon, "namespace");
        Some(NamespaceDecl::new(id, name))
    }
    fn parse_type(&mut self) -> Option<TypeDecl> {
        let id = self.take_id("type.id")?;
        let kind_name = self.take_identifier("type.kind")?;
        let kind = match kind_name.as_str() {
            "state" => TypeKind::State,
            "command" => TypeKind::Command,
            "context" => TypeKind::Context,
            "effect" => TypeKind::Effect,
            "destination" => TypeKind::Destination,
            "payload" => TypeKind::Payload,
            "data" => TypeKind::Data,
            "bool" => TypeKind::Bool,
            "int" => TypeKind::Int,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "type.kind",
                    "state|command|context|effect|destination|payload|data|bool|int",
                    kind_name.as_str(),
                    "select one closed type kind",
                );
                TypeKind::Data
            }
        };
        let name = self.take_identifier("type.name")?;
        self.expect_symbol(TokenKind::Semicolon, "type");
        Some(TypeDecl::new(id, kind, name))
    }
    fn parse_field(&mut self) -> Option<FieldDecl> {
        let id = self.take_id("field.id")?;
        let owner = self.take_id("field.owner")?;
        let name = self.take_identifier("field.name")?;
        let field_type = self.take_id("field.type")?;
        self.expect_symbol(TokenKind::Semicolon, "field");
        Some(FieldDecl::new(id, owner, name, field_type))
    }
    fn parse_variant(&mut self) -> Option<VariantDecl> {
        let id = self.take_id("variant.id")?;
        let owner = self.take_id("variant.owner")?;
        let name = self.take_identifier("variant.name")?;
        let payload_type = if self.take_keyword("none") {
            None
        } else {
            self.take_id("variant.payload")
        };
        self.expect_symbol(TokenKind::Semicolon, "variant");
        Some(VariantDecl::new(id, owner, name, payload_type))
    }
    fn parse_reason(&mut self) -> Option<ReasonDecl> {
        let id = self.take_id("reason.id")?;
        let name = self.take_identifier("reason.name")?;
        self.expect_keyword("precedence", "reason.precedence");
        let rank = self.take_u32("reason.precedence")?;
        self.expect_symbol(TokenKind::Semicolon, "reason");
        Some(ReasonDecl::new(id, name, rank))
    }
    fn parse_effect(&mut self) -> Option<EffectDecl> {
        let id = self.take_id("effect.id")?;
        let name = self.take_identifier("effect.name")?;
        self.expect_keyword("destination", "effect.destination");
        let destination = self.take_id("effect.destination")?;
        self.expect_keyword("payload", "effect.payload");
        let payload = self.take_id("effect.payload")?;
        self.expect_symbol(TokenKind::Semicolon, "effect");
        Some(EffectDecl::new(id, name, destination, payload))
    }
    fn parse_channel(&mut self) -> Option<ChannelDecl> {
        let id = self.take_id("channel.id")?;
        let name = self.take_identifier("channel.name")?;
        self.expect_keyword("destination", "channel.destination");
        let destination = self.take_id("channel.destination")?;
        self.expect_keyword("payload", "channel.payload");
        let payload = self.take_id("channel.payload")?;
        self.expect_symbol(TokenKind::Semicolon, "channel");
        Some(ChannelDecl::new(id, name, destination, payload))
    }

    fn parse_component(&mut self) -> Option<ComponentDecl> {
        let id = self.take_id("component.id")?;
        let name = self.take_identifier("component.name")?;
        self.expect_symbol(TokenKind::LBrace, "component");
        let (mut owned, mut ports, mut footprints, mut budgets, mut assumptions, mut guarantees) = (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        while !matches!(self.current().kind, TokenKind::RBrace | TokenKind::Eof) {
            if self.take_keyword("owns") {
                if let Some(value) = self.take_id("component.owns") {
                    owned.push(value);
                }
                self.expect_symbol(TokenKind::Semicolon, "component.owns");
            } else if self.take_keyword("port") {
                let port_id = self.take_id("component.port.id")?;
                let direction = if self.take_keyword("input") {
                    PortDirection::Input
                } else if self.take_keyword("output") {
                    PortDirection::Output
                } else {
                    self.error(
                        DiagnosticCode::ExpectedToken,
                        "component.port.direction",
                        "input|output",
                        self.describe_current(),
                        "declare an explicit direction",
                    );
                    PortDirection::Input
                };
                let port_name = self.take_identifier("component.port.name")?;
                let payload = self.take_id("component.port.type")?;
                self.expect_symbol(TokenKind::Semicolon, "component.port");
                ports.push(PortDecl::new(port_id, direction, port_name, payload));
            } else if let Some(kind) = self.take_footprint_kind() {
                if let Some(path) = self.parse_path() {
                    footprints.push(FootprintDecl::new(kind, path));
                }
                self.expect_symbol(TokenKind::Semicolon, "component.footprint");
            } else if self.take_keyword("budget") {
                let resource_name = self.take_identifier("component.budget.resource")?;
                let resource = match resource_name.as_str() {
                    "steps" => BudgetResource::Steps,
                    "nodes" => BudgetResource::Nodes,
                    "bytes" => BudgetResource::Bytes,
                    "predicate_calls" => BudgetResource::PredicateCalls,
                    _ => {
                        self.error(
                            DiagnosticCode::InvalidDeclaration,
                            "component.budget.resource",
                            "steps|nodes|bytes|predicate_calls",
                            resource_name.as_str(),
                            "select one closed resource kind",
                        );
                        BudgetResource::Steps
                    }
                };
                let limit = self.take_number("component.budget.limit")?;
                if let Some(budget) = BudgetDecl::try_new(resource, limit) {
                    budgets.push(budget)
                } else {
                    self.error(
                        DiagnosticCode::InvalidDeclaration,
                        "component.budget.limit",
                        "nonzero u64",
                        text(limit),
                        "use a nonzero resource limit",
                    )
                }
                self.expect_symbol(TokenKind::Semicolon, "component.budget");
            } else if self.take_keyword("assume") {
                if let Some(value) = self.take_id("component.assume") {
                    assumptions.push(value)
                }
                self.expect_symbol(TokenKind::Semicolon, "component.assume");
            } else if self.take_keyword("guarantee") {
                if let Some(value) = self.take_id("component.guarantee") {
                    guarantees.push(value)
                }
                self.expect_symbol(TokenKind::Semicolon, "component.guarantee");
            } else {
                self.error(
                    DiagnosticCode::UnexpectedToken,
                    "component",
                    "component declaration",
                    self.describe_current(),
                    "use owns, port, a footprint, budget, assume, or guarantee",
                );
                self.recover_member();
            }
        }
        self.expect_symbol(TokenKind::RBrace, "component");
        Some(ComponentDecl::new(
            id,
            name,
            owned,
            ports,
            footprints,
            budgets,
            assumptions,
            guarantees,
        ))
    }

    fn take_footprint_kind(&mut self) -> Option<FootprintKind> {
        if self.take_keyword("reads") {
            Some(FootprintKind::Read)
        } else if self.take_keyword("writes") {
            Some(FootprintKind::Write)
        } else if self.take_keyword("contexts") {
            Some(FootprintKind::Context)
        } else if self.take_keyword("effects") {
            Some(FootprintKind::Effect)
        } else if self.take_keyword("outbox") {
            Some(FootprintKind::Outbox)
        } else {
            None
        }
    }
    fn parse_path(&mut self) -> Option<ProjectionPath> {
        let root_name = self.take_identifier("path.root")?;
        let root = match root_name.as_str() {
            "pre" => ProjectionRoot::Pre,
            "post" => ProjectionRoot::Post,
            "command" => ProjectionRoot::Command,
            "context" => ProjectionRoot::Context,
            "effects" => ProjectionRoot::Effects,
            "outbox" => ProjectionRoot::Outbox,
            "events" => ProjectionRoot::Events,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "path.root",
                    "pre|post|command|context|effects|outbox|events",
                    root_name.as_str(),
                    "select one typed projection root",
                );
                ProjectionRoot::Pre
            }
        };
        let mut segments = Vec::new();
        while self.take_symbol(&TokenKind::Dot) {
            if let Some(id) = self.take_id("path.segment") {
                segments.push(id)
            } else {
                break;
            }
        }
        match ProjectionPath::try_new(root, segments) {
            Some(path) => Some(path),
            None => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "path",
                    "nonempty path with at most 64 stable IDs",
                    root_name.as_str(),
                    "append one or more `.ID` segments",
                );
                None
            }
        }
    }
    fn parse_wiring(&mut self) -> Option<WiringDecl> {
        let sc = self.take_id("wire.source.component")?;
        self.expect_symbol(TokenKind::Dot, "wire.source");
        let sp = self.take_id("wire.source.port")?;
        self.expect_symbol(TokenKind::Arrow, "wire");
        let dc = self.take_id("wire.destination.component")?;
        self.expect_symbol(TokenKind::Dot, "wire.destination");
        let dp = self.take_id("wire.destination.port")?;
        self.expect_symbol(TokenKind::Semicolon, "wire");
        Some(WiringDecl::new(sc, sp, dc, dp))
    }
    fn parse_merge(&mut self) {
        self.expect_symbol(TokenKind::LBracket, "merge");
        let mut values = Vec::new();
        while !matches!(self.current().kind, TokenKind::RBracket | TokenKind::Eof) {
            if let Some(id) = self.take_id("merge.component") {
                values.push(id)
            }
            if !self.take_symbol(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_symbol(TokenKind::RBracket, "merge");
        self.expect_symbol(TokenKind::Semicolon, "merge");
        self.merge_order = values;
    }
    fn parse_law(&mut self) -> Option<LawDecl> {
        let id = self.take_id("law.id")?;
        let name = self.take_identifier("law.name")?;
        self.expect_symbol(TokenKind::Equal, "law");
        let formula = self.parse_rel();
        self.expect_symbol(TokenKind::Semicolon, "law");
        Some(LawDecl::new(id, name, formula))
    }
    fn parse_claim(&mut self) -> Option<ClaimDecl> {
        let id = self.take_id("claim.id")?;
        let name = self.take_identifier("claim.name")?;
        let backend_name = self.take_identifier("claim.backend")?;
        let backends = match backend_name.as_str() {
            "cvc5" => vec![BackendId::Cvc5],
            "z3" => vec![BackendId::Z3],
            "lean" => vec![BackendId::Lean],
            "all" => vec![BackendId::Cvc5, BackendId::Z3, BackendId::Lean],
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "claim.backend",
                    "cvc5|z3|lean|all",
                    backend_name.as_str(),
                    "select a closed backend ID",
                );
                Vec::new()
            }
        };
        let mode_name = self.take_identifier("claim.mode")?;
        let mode = match mode_name.as_str() {
            "relational" => ClaimMode::Relational,
            "finite" => ClaimMode::Finite {
                horizon: self.take_u32("claim.horizon").unwrap_or(0),
            },
            "unbounded" => ClaimMode::UnboundedProof,
            _ => {
                self.error(
                    DiagnosticCode::InvalidDeclaration,
                    "claim.mode",
                    "relational|finite N|unbounded",
                    mode_name.as_str(),
                    "select an explicit claim mode",
                );
                ClaimMode::Relational
            }
        };
        self.expect_symbol(TokenKind::Equal, "claim");
        let formula = match mode {
            ClaimMode::Relational => ClaimFormula::Relational(self.parse_rel()),
            ClaimMode::Finite { .. } | ClaimMode::UnboundedProof => {
                ClaimFormula::Temporal(self.parse_temporal())
            }
        };
        self.expect_symbol(TokenKind::Semicolon, "claim");
        Some(ClaimDecl::new(id, name, backends, mode, formula))
    }

    fn parse_rel(&mut self) -> RelExpr {
        self.parse_implies()
    }
    fn parse_implies(&mut self) -> RelExpr {
        let left = self.parse_or();
        if self.take_symbol(&TokenKind::Arrow) {
            let right = self.nested("formula.implication", RelExpr::Bool(false), |parser| {
                parser.parse_implies()
            });
            RelExpr::Implies(Box::new(left), Box::new(right))
        } else {
            left
        }
    }
    fn parse_or(&mut self) -> RelExpr {
        let mut value = self.parse_and();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::OrOr) {
            if !self.chain_available("formula.or", operators) {
                break;
            }
            self.take_symbol(&TokenKind::OrOr);
            operators += 1;
            value = RelExpr::Or(Box::new(value), Box::new(self.parse_and()));
        }
        value
    }
    fn parse_and(&mut self) -> RelExpr {
        let mut value = self.parse_not();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::AndAnd) {
            if !self.chain_available("formula.and", operators) {
                break;
            }
            self.take_symbol(&TokenKind::AndAnd);
            operators += 1;
            value = RelExpr::And(Box::new(value), Box::new(self.parse_not()));
        }
        value
    }
    fn parse_not(&mut self) -> RelExpr {
        if self.take_symbol(&TokenKind::Bang) || self.take_keyword("not") {
            let value = self.nested("formula.not", RelExpr::Bool(false), |parser| {
                parser.parse_not()
            });
            RelExpr::Not(Box::new(value))
        } else {
            self.parse_rel_atom()
        }
    }
    fn parse_rel_atom(&mut self) -> RelExpr {
        if self.take_keyword("true") {
            return RelExpr::Bool(true);
        }
        if self.take_keyword("false") {
            return RelExpr::Bool(false);
        }
        if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("formula.group", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.expect_symbol(TokenKind::RParen, "formula");
            return value;
        }
        if self.peek_keyword("forall") || self.peek_keyword("exists") {
            let universal = self.take_keyword("forall");
            if !universal {
                self.take_keyword("exists");
            }
            let variable = self
                .take_identifier("quantifier.variable")
                .unwrap_or_else(fallback_identifier);
            self.expect_keyword("in", "quantifier");
            let start = self.take_i128("quantifier.start").unwrap_or(0);
            self.expect_symbol(TokenKind::Range, "quantifier");
            let end = self.take_i128("quantifier.end").unwrap_or(0);
            self.expect_symbol(TokenKind::LBrace, "quantifier");
            let body = self.nested("formula.quantifier", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.expect_symbol(TokenKind::RBrace, "quantifier");
            return if universal {
                RelExpr::ForAll {
                    variable,
                    start,
                    end,
                    body: Box::new(body),
                }
            } else {
                RelExpr::Exists {
                    variable,
                    start,
                    end,
                    body: Box::new(body),
                }
            };
        }
        let named_predicate = matches!(
            (&self.current().kind, self.tokens.get(self.index + 1)),
            (TokenKind::Ident(name), Some(next))
                if matches!(next.kind, TokenKind::LParen)
                    && !matches!(name.as_ref(), "div_exact" | "div_floor" | "div_ceil")
        );
        if named_predicate {
            let name = self
                .take_identifier("predicate.name")
                .unwrap_or_else(fallback_identifier);
            self.expect_symbol(TokenKind::LParen, "predicate");
            let mut arguments = Vec::new();
            while !matches!(self.current().kind, TokenKind::RParen | TokenKind::Eof) {
                arguments.push(self.parse_value());
                if !self.take_symbol(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_symbol(TokenKind::RParen, "predicate");
            return RelExpr::Predicate {
                name,
                arguments: arguments.into_boxed_slice(),
            };
        }
        let left = self.parse_value();
        let op = if self.take_symbol(&TokenKind::EqEq) {
            Some(CompareOp::Eq)
        } else if self.take_symbol(&TokenKind::NotEq) {
            Some(CompareOp::NotEq)
        } else if self.take_symbol(&TokenKind::LessEq) {
            Some(CompareOp::LessEq)
        } else if self.take_symbol(&TokenKind::Less) {
            Some(CompareOp::Less)
        } else if self.take_symbol(&TokenKind::GreaterEq) {
            Some(CompareOp::GreaterEq)
        } else if self.take_symbol(&TokenKind::Greater) {
            Some(CompareOp::Greater)
        } else {
            None
        };
        if let Some(op) = op {
            RelExpr::Compare(op, left, self.parse_value())
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "formula.comparison",
                "comparison operator",
                self.describe_current(),
                "compare scalar expressions or use a named predicate",
            );
            RelExpr::Bool(false)
        }
    }
    fn parse_value(&mut self) -> ValueExpr {
        let mut value = self.parse_term();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::Plus | TokenKind::Minus) {
            if !self.chain_available("value.additive", operators) {
                break;
            }
            let add = self.take_symbol(&TokenKind::Plus);
            if !add {
                self.take_symbol(&TokenKind::Minus);
            }
            operators += 1;
            let right = self.parse_term();
            value = if add {
                ValueExpr::Add(Box::new(value), Box::new(right))
            } else {
                ValueExpr::Sub(Box::new(value), Box::new(right))
            };
        }
        value
    }
    fn parse_term(&mut self) -> ValueExpr {
        let mut value = self.parse_value_atom();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::Star) {
            if !self.chain_available("value.multiplicative", operators) {
                break;
            }
            self.take_symbol(&TokenKind::Star);
            operators += 1;
            value = ValueExpr::Mul(Box::new(value), Box::new(self.parse_value_atom()));
        }
        value
    }
    fn parse_value_atom(&mut self) -> ValueExpr {
        if self.take_symbol(&TokenKind::Minus) {
            let value = self.nested("value.negation", ValueExpr::Int(0), |parser| {
                parser.parse_value_atom()
            });
            return ValueExpr::Sub(Box::new(ValueExpr::Int(0)), Box::new(value));
        }
        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            return ValueExpr::Int(i128::from(value));
        }
        if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("value.group", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::RParen, "value");
            return value;
        }
        let name = self
            .take_identifier("value")
            .unwrap_or_else(fallback_identifier);
        if matches!(name.as_str(), "div_exact" | "div_floor" | "div_ceil")
            && self.take_symbol(&TokenKind::LParen)
        {
            let left = self.nested("value.division.left", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::Comma, "division");
            let right = self.nested("value.division.right", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::RParen, "division");
            let mode = match name.as_str() {
                "div_floor" => DivisionMode::Floor,
                "div_ceil" => DivisionMode::Ceil,
                _ => DivisionMode::Exact,
            };
            return ValueExpr::Div(mode, Box::new(left), Box::new(right));
        }
        if name.as_str() == "sum" {
            let variable = self
                .take_identifier("sum.variable")
                .unwrap_or_else(fallback_identifier);
            self.expect_keyword("in", "sum");
            let start = self.take_i128("sum.start").unwrap_or(0);
            self.expect_symbol(TokenKind::Range, "sum");
            let end = self.take_i128("sum.end").unwrap_or(0);
            self.expect_symbol(TokenKind::LBrace, "sum");
            let body = self.nested("value.sum", ValueExpr::Int(0), |parser| {
                parser.parse_value()
            });
            self.expect_symbol(TokenKind::RBrace, "sum");
            return ValueExpr::Sum {
                variable,
                start,
                end,
                body: Box::new(body),
            };
        }
        if let Some(root) = projection_root(name.as_str()) {
            let mut segments = Vec::new();
            while self.take_symbol(&TokenKind::Dot) {
                if let Some(id) = self.take_id("projection.segment") {
                    segments.push(id)
                }
            }
            if let Some(path) = ProjectionPath::try_new(root, segments) {
                return ValueExpr::Projection(path);
            }
        }
        ValueExpr::Var(name)
    }
    fn parse_temporal(&mut self) -> TemporalFormula {
        self.parse_temporal_until()
    }
    fn parse_temporal_until(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_or();
        let mut operators = 0usize;
        while self.peek_keyword("until") {
            if !self.chain_available("temporal.until", operators) {
                break;
            }
            self.take_keyword("until");
            operators += 1;
            value = TemporalFormula::Until(Box::new(value), Box::new(self.parse_temporal_or()));
        }
        value
    }
    fn parse_temporal_or(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_and();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::OrOr) {
            if !self.chain_available("temporal.or", operators) {
                break;
            }
            self.take_symbol(&TokenKind::OrOr);
            operators += 1;
            value = TemporalFormula::Or(Box::new(value), Box::new(self.parse_temporal_and()));
        }
        value
    }
    fn parse_temporal_and(&mut self) -> TemporalFormula {
        let mut value = self.parse_temporal_atom();
        let mut operators = 0usize;
        while matches!(self.current().kind, TokenKind::AndAnd) {
            if !self.chain_available("temporal.and", operators) {
                break;
            }
            self.take_symbol(&TokenKind::AndAnd);
            operators += 1;
            value = TemporalFormula::And(Box::new(value), Box::new(self.parse_temporal_atom()));
        }
        value
    }
    fn parse_temporal_atom(&mut self) -> TemporalFormula {
        if self.take_keyword("next") {
            let value = self.nested("temporal.next", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Next(Box::new(value))
        } else if self.take_keyword("always") {
            let value = self.nested("temporal.always", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Always(Box::new(value))
        } else if self.take_keyword("eventually") {
            let value = self.nested("temporal.eventually", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Eventually(Box::new(value))
        } else if self.take_keyword("not") || self.take_symbol(&TokenKind::Bang) {
            let value = self.nested("temporal.not", temporal_false(), |parser| {
                parser.parse_temporal_atom()
            });
            TemporalFormula::Not(Box::new(value))
        } else if self.take_keyword("atom") {
            self.expect_symbol(TokenKind::LParen, "temporal.atom");
            let rel = self.nested("temporal.atom", RelExpr::Bool(false), |parser| {
                parser.parse_rel()
            });
            self.expect_symbol(TokenKind::RParen, "temporal.atom");
            TemporalFormula::Atom(rel)
        } else if self.take_symbol(&TokenKind::LParen) {
            let value = self.nested("temporal.group", temporal_false(), |parser| {
                parser.parse_temporal()
            });
            self.expect_symbol(TokenKind::RParen, "temporal");
            value
        } else if self.take_keyword("true") {
            TemporalFormula::Atom(RelExpr::Bool(true))
        } else if self.take_keyword("false") {
            TemporalFormula::Atom(RelExpr::Bool(false))
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                "temporal",
                "temporal operator or atom(<relational>)",
                self.describe_current(),
                "wrap relational formulas with `atom(...)`",
            );
            TemporalFormula::Atom(RelExpr::Bool(false))
        }
    }

    fn nested<T>(&mut self, path: &str, fallback: T, parse: impl FnOnce(&mut Self) -> T) -> T {
        if self.nesting >= MAX_FORMULA_DEPTH {
            self.error(
                DiagnosticCode::ResourceLimit,
                path,
                text(MAX_FORMULA_DEPTH),
                "expression nesting exceeds the parser limit",
                "reduce nested operators or split the claim into named laws",
            );
            return fallback;
        }
        self.nesting += 1;
        let value = parse(self);
        self.nesting -= 1;
        value
    }
    fn chain_available(&mut self, path: &str, operators: usize) -> bool {
        if operators.saturating_add(1) < MAX_FORMULA_DEPTH {
            return true;
        }
        self.error(
            DiagnosticCode::ResourceLimit,
            path,
            text(MAX_FORMULA_DEPTH),
            "operator chain exceeds the parser limit",
            "split the expression into named laws or predicates",
        );
        false
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len().saturating_sub(1))]
    }
    fn advance(&mut self) {
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
    }
    fn peek_keyword(&self, word: &str) -> bool {
        matches!(&self.current().kind,TokenKind::Ident(value) if value.as_ref()==word)
    }
    fn take_keyword(&mut self, word: &str) -> bool {
        if self.peek_keyword(word) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect_keyword(&mut self, word: &str, path: &str) {
        if !self.take_keyword(word) {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                word,
                self.describe_current(),
                format!("insert `{word}`"),
            );
        }
    }
    fn take_symbol(&mut self, kind: &TokenKind) -> bool {
        if same_symbol(&self.current().kind, kind) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect_symbol(&mut self, kind: TokenKind, path: &str) {
        if !self.take_symbol(&kind) {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                describe_kind(&kind),
                self.describe_current(),
                "insert the required punctuation",
            );
        }
    }
    fn take_number(&mut self, path: &str) -> Option<u64> {
        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            Some(value)
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                "unsigned integer",
                self.describe_current(),
                "use an explicit decimal integer",
            );
            None
        }
    }
    fn take_u32(&mut self, path: &str) -> Option<u32> {
        let value = self.take_number(path)?;
        match u32::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "u32",
                    text(value),
                    "use a value no greater than 4294967295",
                );
                None
            }
        }
    }
    fn take_u16(&mut self, path: &str) -> Option<u16> {
        let value = self.take_number(path)?;
        match u16::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "u16",
                    text(value),
                    "use a value no greater than 65535",
                );
                None
            }
        }
    }
    fn take_i128(&mut self, path: &str) -> Option<i128> {
        let negative = self.take_symbol(&TokenKind::Minus);
        let value = i128::from(self.take_number(path)?);
        Some(if negative { -value } else { value })
    }
    fn take_id(&mut self, path: &str) -> Option<StableId> {
        let value = self.take_u32(path)?;
        match StableId::new(value) {
            Some(value) => Some(value),
            None => {
                self.error(
                    DiagnosticCode::InvalidNumber,
                    path,
                    "nonzero stable ID",
                    "0",
                    "allocate an explicit nonzero stable ID",
                );
                None
            }
        }
    }
    fn take_identifier(&mut self, path: &str) -> Option<Identifier> {
        if let TokenKind::Ident(value) = self.current().kind.clone() {
            let span = self.current().span;
            self.advance();
            match Identifier::try_new(value) {
                Some(value) => Some(value),
                None => {
                    self.diagnostics.push(Diagnostic::new(DiagnosticCode::InvalidDeclaration,DiagnosticStage::Parse,AstPath::new(path),span,"ASCII identifier up to 128 bytes","invalid identifier","use letters, digits, underscore, and hyphen with a letter or underscore first"));
                    None
                }
            }
        } else {
            self.error(
                DiagnosticCode::ExpectedToken,
                path,
                "ASCII identifier",
                self.describe_current(),
                "add an explicit stable name",
            );
            None
        }
    }
    fn describe_current(&self) -> Box<str> {
        match &self.current().kind {
            TokenKind::Ident(value) => value.clone(),
            TokenKind::Number(value) => text(value),
            TokenKind::Hex(value) => value.clone(),
            kind => Box::from(describe_kind(kind)),
        }
    }
    fn error(
        &mut self,
        code: DiagnosticCode,
        path: &str,
        expected: impl Into<Box<str>>,
        actual: impl Into<Box<str>>,
        remediation: impl Into<Box<str>>,
    ) {
        self.diagnostics.push(Diagnostic::new(
            code,
            DiagnosticStage::Parse,
            AstPath::new(path),
            self.current().span,
            expected,
            actual,
            remediation,
        ));
    }
    fn recover_top(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) {
            self.advance();
        }
        if matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace
        ) {
            self.advance();
        }
    }
    fn recover_member(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        ) {
            self.advance();
        }
        if self.take_symbol(&TokenKind::Semicolon) {}
    }
}

fn temporal_false() -> TemporalFormula {
    TemporalFormula::Atom(RelExpr::Bool(false))
}

fn projection_root(value: &str) -> Option<ProjectionRoot> {
    match value {
        "pre" => Some(ProjectionRoot::Pre),
        "post" => Some(ProjectionRoot::Post),
        "command" => Some(ProjectionRoot::Command),
        "context" => Some(ProjectionRoot::Context),
        "effects" => Some(ProjectionRoot::Effects),
        "outbox" => Some(ProjectionRoot::Outbox),
        "events" => Some(ProjectionRoot::Events),
        _ => None,
    }
}
fn one_id() -> StableId {
    StableId::new(1).unwrap_or_else(|| unreachable!())
}
fn fallback_identifier() -> Identifier {
    Identifier::try_new("invalid").unwrap_or_else(|| unreachable!())
}
fn same_symbol(left: &TokenKind, right: &TokenKind) -> bool {
    matches!(
        (left, right),
        (TokenKind::LBrace, TokenKind::LBrace)
            | (TokenKind::RBrace, TokenKind::RBrace)
            | (TokenKind::LParen, TokenKind::LParen)
            | (TokenKind::RParen, TokenKind::RParen)
            | (TokenKind::LBracket, TokenKind::LBracket)
            | (TokenKind::RBracket, TokenKind::RBracket)
            | (TokenKind::Semicolon, TokenKind::Semicolon)
            | (TokenKind::Comma, TokenKind::Comma)
            | (TokenKind::Dot, TokenKind::Dot)
            | (TokenKind::Range, TokenKind::Range)
            | (TokenKind::Colon, TokenKind::Colon)
            | (TokenKind::Equal, TokenKind::Equal)
            | (TokenKind::EqEq, TokenKind::EqEq)
            | (TokenKind::NotEq, TokenKind::NotEq)
            | (TokenKind::Less, TokenKind::Less)
            | (TokenKind::LessEq, TokenKind::LessEq)
            | (TokenKind::Greater, TokenKind::Greater)
            | (TokenKind::GreaterEq, TokenKind::GreaterEq)
            | (TokenKind::Plus, TokenKind::Plus)
            | (TokenKind::Minus, TokenKind::Minus)
            | (TokenKind::Star, TokenKind::Star)
            | (TokenKind::Slash, TokenKind::Slash)
            | (TokenKind::AndAnd, TokenKind::AndAnd)
            | (TokenKind::OrOr, TokenKind::OrOr)
            | (TokenKind::Bang, TokenKind::Bang)
            | (TokenKind::Arrow, TokenKind::Arrow)
            | (TokenKind::Eof, TokenKind::Eof)
    )
}
fn describe_kind(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Ident(_) => "identifier",
        TokenKind::Number(_) => "number",
        TokenKind::Hex(_) => "commitment",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::Semicolon => ";",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::Range => "..",
        TokenKind::Colon => ":",
        TokenKind::Equal => "=",
        TokenKind::EqEq => "==",
        TokenKind::NotEq => "!=",
        TokenKind::Less => "<",
        TokenKind::LessEq => "<=",
        TokenKind::Greater => ">",
        TokenKind::GreaterEq => ">=",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::AndAnd => "&&",
        TokenKind::OrOr => "||",
        TokenKind::Bang => "!",
        TokenKind::Arrow => "->",
        TokenKind::Eof => "end of file",
    }
}
