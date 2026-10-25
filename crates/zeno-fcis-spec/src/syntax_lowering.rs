//! Independent lowering of a checked, lossless surface derivation.
//!
//! This module reads original token payloads at certified leaf positions. It
//! does not invoke the recovering parser, print an AST, or execute source text.
//! EBNF helper nodes are flattened without changing token or child order. The
//! resulting AST owns its identifiers and uses the existing typed constructors.
//! Grammar-certificate verification and this semantic lowering are separate
//! assurance boundaries: the generic certificate theorem alone does not prove
//! this module's semantic action mapping.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::ast::*;
use crate::lexer::{Token, TokenKind};
use crate::semantic_readers::{read_projection_tail, signed_magnitude};
use crate::syntax::SyntaxTree;
use crate::syntax_grammar::rule_name;
use crate::{DiagnosticCode, MAX_FORMULA_DEPTH, MAX_PARSED_EXPRESSION_HEIGHT, ZENO_DSL_VERSION};

/// A refused semantic action in an otherwise recognized surface derivation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoweringError {
    pub(crate) node: usize,
    pub(crate) message: &'static str,
    pub(crate) resource_limit: bool,
    pub(crate) code: DiagnosticCode,
}

impl LoweringError {
    const fn invalid(node: usize, message: &'static str) -> Self {
        Self {
            node,
            message,
            resource_limit: false,
            code: DiagnosticCode::InvalidDeclaration,
        }
    }

    const fn resource(node: usize, message: &'static str) -> Self {
        Self {
            node,
            message,
            resource_limit: true,
            code: DiagnosticCode::ResourceLimit,
        }
    }

    const fn numeric(node: usize, message: &'static str) -> Self {
        Self {
            code: DiagnosticCode::InvalidNumber,
            ..Self::invalid(node, message)
        }
    }
}

type Result<T> = core::result::Result<T, LoweringError>;

/// Values in the postorder semantic fold. An unsigned 2^127 magnitude remains
/// pending until its immediate syntax parent proves the special -2^127 form.
/// Every other scalar consumer refuses it rather than silently narrowing it.
enum SemanticValue {
    Scalar(ValueExpr),
    Relational(RelExpr),
    Temporal(TemporalFormula),
    MinMagnitude(usize),
}

impl SemanticValue {
    fn scalar(self, node: usize) -> Result<ValueExpr> {
        match self {
            Self::Scalar(value) => Ok(value),
            Self::MinMagnitude(origin) => Err(LoweringError::numeric(
                origin,
                "positive integer exceeds i128",
            )),
            _ => Err(LoweringError::invalid(
                node,
                "expected scalar semantic operand",
            )),
        }
    }

    fn relational(self, node: usize) -> Result<RelExpr> {
        match self {
            Self::Relational(value) => Ok(value),
            _ => Err(LoweringError::invalid(
                node,
                "expected relational semantic operand",
            )),
        }
    }

    fn temporal(self, node: usize) -> Result<TemporalFormula> {
        match self {
            Self::Temporal(value) => Ok(value),
            _ => Err(LoweringError::invalid(
                node,
                "expected temporal semantic operand",
            )),
        }
    }
}

/// Lowers a complete, checked project derivation. The caller must validate the
/// certificate against the same token sequence before invoking this function.
pub(crate) fn lower(
    tree: &SyntaxTree,
    tokens: &[Token<'_>],
    diagnostic_limit: usize,
) -> Result<ParsedProject> {
    let lowerer = Lowerer { tree, tokens };
    lowerer.validate_resources(tree.root)?;
    lowerer.validate_expression_heights()?;
    lowerer.project(tree.root, diagnostic_limit)
}

struct Lowerer<'a> {
    tree: &'a SyntaxTree,
    tokens: &'a [Token<'a>],
}

impl Lowerer<'_> {
    /// Compute the height of the AST actions before allocating any recursive
    /// AST. Independent source-nesting and chain limits do not bound their
    /// composition. This pass uses the certificate's postorder and numeric
    /// scratch only, so refusal cannot drop an already excessive AST.
    fn validate_expression_heights(&self) -> Result<()> {
        let mut heights: Vec<usize> = Vec::with_capacity(self.tree.nodes.len());
        for node in 0..self.tree.nodes.len() {
            let name = self.name(node)?;
            let mut height = 0;
            for &child in self.raw_children(node)? {
                height = height.max(*heights.get(child).ok_or(LoweringError::invalid(
                    node,
                    "non-postorder expression child",
                ))?);
            }
            match name {
                "NonnegativeI128" | "Variable" | "RootProjectionOrVariable" => height = 1,
                "RelAtom" if self.keyword(node, "true")? || self.keyword(node, "false")? => {
                    height = 1
                }
                "Negated" => {
                    let syntax = &self.tree.nodes[node];
                    height = if syntax.end - syntax.start == 2
                        && self.token_at(node, 1)?.kind == TokenKind::Number(1u128 << 127)
                    {
                        1
                    } else {
                        1 + height.max(1)
                    };
                }
                "Quantified" | "NamedPredicate" | "Comparison" | "DivisionCall" | "FusedCall"
                | "Sum" => height += 1,
                "RelImplies" | "RelNot" => {
                    if self.optional(node, &self.parts(node)?, name)?.is_some() {
                        height += 1;
                    }
                }
                "TemporalAtom" => {
                    let parts = self.parts(node)?;
                    if self.optional(node, &parts, "Temporal")?.is_none() {
                        // Prefix or explicit atom adds one AST layer; a bare
                        // Boolean becomes Temporal::Atom(Rel::Bool), height 2.
                        height = if height == 0 { 2 } else { height + 1 };
                    }
                }
                "RelOr" | "RelAnd" | "Value" | "Term" | "TemporalUntil" | "TemporalOr"
                | "TemporalAnd" => {
                    let child_name = match name {
                        "RelOr" => "RelAnd",
                        "RelAnd" => "RelNot",
                        "Value" => "Term",
                        "Term" => "ValueAtom",
                        "TemporalUntil" => "TemporalOr",
                        "TemporalOr" => "TemporalAnd",
                        _ => "TemporalAtom",
                    };
                    let operands = self.named(&self.parts(node)?, child_name)?;
                    let Some((&first, rest)) = operands.split_first() else {
                        return self.fail(node, "empty expression chain");
                    };
                    height = heights[first];
                    for &right in rest {
                        height = 1 + height.max(heights[right]);
                    }
                }
                _ => {}
            }
            if height > MAX_PARSED_EXPRESSION_HEIGHT {
                return Err(LoweringError::resource(
                    node,
                    "constructed expression height exceeds the parser limit",
                ));
            }
            heights.push(height);
        }
        Ok(())
    }

    /// Checks the existing parser's nesting and per-chain limits before any
    /// recursive AST is built. The traversal itself uses an owned work stack.
    /// EBNF recursion is presentation scaffolding, not expression nesting.
    fn validate_resources(&self, root: usize) -> Result<()> {
        let mut pending = vec![(root, 0usize)];
        let mut visited = 0usize;
        while let Some((node, nesting)) = pending.pop() {
            visited += 1;
            if visited > self.tree.nodes.len() {
                return self.fail(node, "cyclic or repeated derivation node");
            }
            let name = self.name(node)?;
            if name.is_empty() {
                continue;
            }
            let parts = self.parts(node)?;
            let chain_operand = match name {
                "RelOr" => Some("RelAnd"),
                "RelAnd" => Some("RelNot"),
                "Value" => Some("Term"),
                "Term" => Some("ValueAtom"),
                "TemporalUntil" => Some("TemporalOr"),
                "TemporalOr" => Some("TemporalAnd"),
                "TemporalAnd" => Some("TemporalAtom"),
                _ => None,
            };
            if let Some(operand) = chain_operand
                && self.named(&parts, operand)?.len() > MAX_FORMULA_DEPTH
            {
                return Err(LoweringError::resource(
                    node,
                    "operator chain exceeds the parser limit",
                ));
            }
            for child in parts {
                let child_name = self.name(child)?;
                let enters = matches!(
                    (name, child_name),
                    ("RelImplies", "RelImplies")
                        | ("RelNot", "RelNot")
                        | ("RelAtom", "Rel")
                        | ("Quantified", "Rel")
                        | ("ValueAtom", "Value")
                        | ("Negated", "ValueAtom")
                        | ("FusedCall", "Value")
                        | ("DivisionCall", "Value")
                        | ("Sum", "Value")
                        | ("TemporalAtom", "TemporalAtom")
                        | ("TemporalAtom", "Temporal")
                        | ("TemporalAtom", "Rel")
                );
                let min_literal = name == "Negated"
                    && self.tree.nodes[node].end - self.tree.nodes[node].start == 2
                    && self.token_at(node, 1)?.kind == TokenKind::Number(1u128 << 127);
                let next = nesting + usize::from(enters && !min_literal);
                if next > MAX_FORMULA_DEPTH {
                    return Err(LoweringError::resource(
                        child,
                        "expression nesting exceeds the parser limit",
                    ));
                }
                pending.push((child, next));
            }
        }
        Ok(())
    }

    fn fail<T>(&self, node: usize, message: &'static str) -> Result<T> {
        Err(LoweringError::invalid(node, message))
    }

    fn name(&self, node: usize) -> Result<&'static str> {
        let n = self
            .tree
            .nodes
            .get(node)
            .ok_or(LoweringError::invalid(node, "missing derivation node"))?;
        if n.symbol.terminal {
            Ok("")
        } else {
            rule_name(n.symbol.tag).ok_or(LoweringError::invalid(node, "unknown grammar rule"))
        }
    }

    fn raw_children(&self, node: usize) -> Result<&[usize]> {
        let n = self
            .tree
            .nodes
            .get(node)
            .ok_or(LoweringError::invalid(node, "missing derivation node"))?;
        let end = n
            .children_start
            .checked_add(n.children_len)
            .ok_or(LoweringError::invalid(node, "invalid child range"))?;
        self.tree
            .children
            .get(n.children_start..end)
            .ok_or(LoweringError::invalid(node, "invalid child range"))
    }

    /// Flattens generated EBNF scaffolding only. Named syntax constructs stay
    /// distinct, so a child's nested identifiers cannot become its parent's.
    fn parts(&self, node: usize) -> Result<Vec<usize>> {
        let mut pending: Vec<usize> = self.raw_children(node)?.iter().rev().copied().collect();
        let mut parts = Vec::new();
        let mut visited = 0usize;
        while let Some(child) = pending.pop() {
            visited += 1;
            if visited > self.tree.nodes.len() {
                return self.fail(node, "cyclic derivation scaffolding");
            }
            if self.name(child)?.starts_with('_') {
                pending.extend(self.raw_children(child)?.iter().rev().copied());
            } else {
                parts.push(child);
            }
        }
        Ok(parts)
    }

    fn named(&self, parts: &[usize], name: &str) -> Result<Vec<usize>> {
        let mut found = Vec::new();
        for &part in parts {
            if self.name(part)? == name {
                found.push(part);
            }
        }
        Ok(found)
    }

    fn one(&self, node: usize, parts: &[usize], name: &str) -> Result<usize> {
        let found = self.named(parts, name)?;
        match found.as_slice() {
            [value] => Ok(*value),
            _ => self.fail(node, "expected one named syntax child"),
        }
    }

    fn optional(&self, node: usize, parts: &[usize], name: &str) -> Result<Option<usize>> {
        let found = self.named(parts, name)?;
        match found.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(*value)),
            _ => self.fail(node, "repeated optional syntax child"),
        }
    }

    fn sole_named(&self, node: usize, parts: &[usize]) -> Result<usize> {
        let mut found = None;
        for &part in parts {
            if !self.name(part)?.is_empty() {
                if found.is_some() {
                    return self.fail(node, "expected one semantic child");
                }
                found = Some(part);
            }
        }
        found.ok_or(LoweringError::invalid(node, "missing semantic child"))
    }

    fn token_at(&self, node: usize, offset: usize) -> Result<&Token<'_>> {
        let n = self
            .tree
            .nodes
            .get(node)
            .ok_or(LoweringError::invalid(node, "missing derivation node"))?;
        let position = n
            .start
            .checked_add(offset)
            .ok_or(LoweringError::invalid(node, "invalid token offset"))?;
        if position >= n.end {
            return self.fail(node, "missing token payload");
        }
        self.tokens
            .get(position)
            .ok_or(LoweringError::invalid(node, "token outside source"))
    }

    fn word(&self, node: usize) -> Result<&str> {
        self.word_at(node, 0)
    }

    fn word_at(&self, node: usize, offset: usize) -> Result<&str> {
        let token = self.token_at(node, offset)?;
        match token.kind {
            TokenKind::Ident => Ok(token.spelling),
            _ => self.fail(node, "expected identifier token"),
        }
    }

    fn identifier(&self, node: usize) -> Result<Identifier> {
        Identifier::try_new(Box::<str>::from(self.word(node)?)).ok_or(LoweringError::invalid(
            node,
            "identifier exceeds its lexical bound",
        ))
    }

    fn keyword(&self, node: usize, word: &str) -> Result<bool> {
        let token = self.token_at(node, 0)?;
        Ok(token.kind == TokenKind::Ident && token.spelling == word)
    }

    fn number(&self, node: usize) -> Result<u128> {
        match self.token_at(node, 0)?.kind {
            TokenKind::Number(value) => Ok(value),
            _ => self.fail(node, "expected numeric token"),
        }
    }

    fn u32(&self, node: usize) -> Result<u32> {
        u32::try_from(self.number(node)?)
            .map_err(|_| LoweringError::numeric(node, "number exceeds u32"))
    }

    fn id(&self, node: usize) -> Result<StableId> {
        StableId::new(self.u32(node)?).ok_or(LoweringError::numeric(
            node,
            "stable identifier must be nonzero",
        ))
    }

    fn signed(&self, node: usize) -> Result<i128> {
        let first = self.token_at(node, 0)?;
        let negative = first.kind == TokenKind::Minus;
        let value = if negative {
            match self.token_at(node, 1)?.kind {
                TokenKind::Number(value) => value,
                _ => return self.fail(node, "expected signed integer magnitude"),
            }
        } else {
            self.number(node)?
        };
        signed_magnitude(negative, value)
            .ok_or(LoweringError::numeric(node, "integer exceeds i128"))
    }

    fn scale(&self, node: usize) -> Result<u8> {
        let text = self.word(node)?;
        if text.len() > 128 {
            return self.fail(node, "scale identifier exceeds 128 bytes");
        }
        text.strip_prefix('e')
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|value| *value <= 38)
            .ok_or(LoweringError::numeric(node, "scale must be e0 through e38"))
    }

    fn ids(&self, node: usize, parts: &[usize], count: usize) -> Result<Vec<StableId>> {
        let nodes = self.named(parts, "ID")?;
        if nodes.len() != count {
            return self.fail(node, "incorrect identifier arity");
        }
        nodes.into_iter().map(|child| self.id(child)).collect()
    }

    fn project(&self, node: usize, diagnostic_limit: usize) -> Result<ParsedProject> {
        if self.name(node)? != "Project" {
            return self.fail(node, "expected complete Project derivation");
        }
        let parts = self.parts(node)?;
        let version_node = self.one(node, &parts, "U16")?;
        let version = u16::try_from(self.number(version_node)?)
            .map_err(|_| LoweringError::numeric(version_node, "language version exceeds u16"))?;
        if version != ZENO_DSL_VERSION {
            return Err(LoweringError {
                code: DiagnosticCode::UnsupportedVersion,
                ..LoweringError::invalid(version_node, "unsupported language version")
            });
        }
        let project_id = self.id(self.one(node, &parts, "ID")?)?;
        let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
        let mut declarations = Vec::new();
        let mut merge_order = Vec::new();
        for item in self.named(&parts, "Declaration")? {
            let inner = self.parts(item)?;
            let child = self.sole_named(item, &inner)?;
            if self.name(child)? == "Merge" {
                let p = self.parts(child)?;
                merge_order = self.id_list(self.one(child, &p, "IdList")?)?;
            } else {
                declarations.push(SpannedDeclaration {
                    declaration: self.declaration(child)?,
                    span: self.token_at(child, 0)?.span,
                });
            }
        }
        Ok(ParsedProject {
            version,
            project_id,
            name,
            declarations,
            merge_order,
            diagnostic_limit,
        })
    }

    fn declaration(&self, node: usize) -> Result<Declaration> {
        let parts = self.parts(node)?;
        match self.name(node)? {
            "Namespace" => {
                let ids = self.ids(node, &parts, 1)?;
                let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
                Ok(Declaration::Namespace(NamespaceDecl::new(ids[0], name)))
            }
            "Type" => Ok(Declaration::Type(self.type_decl(node, &parts)?)),
            "Field" => {
                let ids = self.ids(node, &parts, 3)?;
                let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
                Ok(Declaration::Field(FieldDecl::new(
                    ids[0], ids[1], name, ids[2],
                )))
            }
            "Variant" => {
                let ids = self.ids(node, &parts, 2)?;
                let payload_node = self.one(node, &parts, "VariantPayload")?;
                let payload_parts = self.parts(payload_node)?;
                let payload = self
                    .optional(payload_node, &payload_parts, "ID")?
                    .map(|child| self.id(child))
                    .transpose()?;
                let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
                Ok(Declaration::Variant(VariantDecl::new(
                    ids[0], ids[1], name, payload,
                )))
            }
            "Reason" => {
                let id = self.id(self.one(node, &parts, "ID")?)?;
                let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
                let rank = self.u32(self.one(node, &parts, "U32")?)?;
                Ok(Declaration::Reason(ReasonDecl::new(id, name, rank)))
            }
            "Effect" | "Channel" => {
                let ids = self.ids(node, &parts, 3)?;
                let name = self.identifier(self.one(node, &parts, "Identifier")?)?;
                if self.name(node)? == "Effect" {
                    Ok(Declaration::Effect(EffectDecl::new(
                        ids[0], name, ids[1], ids[2],
                    )))
                } else {
                    Ok(Declaration::Channel(ChannelDecl::new(
                        ids[0], name, ids[1], ids[2],
                    )))
                }
            }
            "Component" => Ok(Declaration::Component(self.component(node, &parts)?)),
            "Wiring" => {
                let ids = self.ids(node, &parts, 4)?;
                Ok(Declaration::Wiring(WiringDecl::new(
                    ids[0], ids[1], ids[2], ids[3],
                )))
            }
            "Law" => Ok(Declaration::Law(self.law(node, &parts)?)),
            "Claim" => Ok(Declaration::Claim(self.claim(node, &parts)?)),
            _ => self.fail(node, "unsupported declaration production"),
        }
    }

    fn type_decl(&self, node: usize, parts: &[usize]) -> Result<TypeDecl> {
        let id = self.id(self.one(node, parts, "ID")?)?;
        let kind_node = self.one(node, parts, "TypeKind")?;
        let kind = match self.word(kind_node)? {
            "state" => TypeKind::State,
            "command" => TypeKind::Command,
            "context" => TypeKind::Context,
            "effect" => TypeKind::Effect,
            "destination" => TypeKind::Destination,
            "payload" => TypeKind::Payload,
            "data" => TypeKind::Data,
            "bool" => TypeKind::Bool,
            "int" => TypeKind::Int,
            _ => return self.fail(kind_node, "unknown type kind"),
        };
        let name = self.identifier(self.one(node, parts, "Identifier")?)?;
        let mut declared = TypeDecl::new(id, kind, name);
        if let Some(range) = self.optional(node, parts, "Range")? {
            let p = self.parts(range)?;
            let bounds = self.named(&p, "SignedI128")?;
            if bounds.len() != 2 {
                return self.fail(range, "range requires two signed bounds");
            }
            let range_value = IntRange::try_new(self.signed(bounds[0])?, self.signed(bounds[1])?)
                .ok_or(LoweringError::invalid(range, "reversed inclusive range"))?;
            declared = declared
                .with_range(range_value)
                .ok_or(LoweringError::invalid(
                    range,
                    "only integer types admit ranges",
                ))?;
        }
        if let Some(quantity) = self.optional(node, parts, "Quantity")? {
            declared =
                declared
                    .with_quantity(self.quantity(quantity)?)
                    .ok_or(LoweringError::invalid(
                        quantity,
                        "only integer types admit quantities",
                    ))?;
        }
        Ok(declared)
    }

    fn quantity(&self, node: usize) -> Result<Quantity> {
        let parts = self.parts(node)?;
        let mut sign = 1i32;
        let mut factors = Vec::new();
        for &part in &parts {
            if self.name(part)? == "UnitAtom" {
                match &self.token_at(part, 0)?.kind {
                    TokenKind::Number(1) => {}
                    TokenKind::Ident => factors.push((self.identifier(part)?, sign)),
                    _ => return self.fail(part, "invalid unit factor"),
                }
            } else if self.name(part)? == "UnitOperator" {
                match self.token_at(part, 0)?.kind {
                    TokenKind::Star => sign = 1,
                    TokenKind::Slash => sign = -1,
                    _ => {}
                }
            }
        }
        let scale = self.scale(self.one(node, &parts, "Scale")?)?;
        Quantity::try_new(factors, scale)
            .ok_or(LoweringError::invalid(node, "quantity exponent overflow"))
    }

    fn id_list(&self, node: usize) -> Result<Vec<StableId>> {
        let p = self.parts(node)?;
        self.named(&p, "ID")?
            .into_iter()
            .map(|child| self.id(child))
            .collect()
    }

    fn root(&self, node: usize) -> Result<ProjectionRoot> {
        match self.word(node)? {
            "pre" => Ok(ProjectionRoot::Pre),
            "post" => Ok(ProjectionRoot::Post),
            "command" => Ok(ProjectionRoot::Command),
            "context" => Ok(ProjectionRoot::Context),
            "effects" => Ok(ProjectionRoot::Effects),
            "outbox" => Ok(ProjectionRoot::Outbox),
            "events" => Ok(ProjectionRoot::Events),
            _ => self.fail(node, "unknown projection root"),
        }
    }

    fn path(&self, node: usize) -> Result<ProjectionPath> {
        let p = self.parts(node)?;
        let root_node = self.one(node, &p, "Root")?;
        let root = self.root(root_node)?;
        let segments = self.projection_segments(node, root_node)?;
        ProjectionPath::try_new(root, segments).ok_or(LoweringError::invalid(
            node,
            "projection requires one through 64 segments",
        ))
    }

    fn projection_segments(&self, node: usize, root: usize) -> Result<Vec<StableId>> {
        // Decode the complete original source span after Root. Filtering the
        // named ID children here would reintroduce a suffix-loss trust boundary.
        let start = self.tree.nodes[root].end;
        let end = self.tree.nodes[node].end;
        let tail = self.tokens.get(start..end).ok_or(LoweringError::invalid(
            node,
            "invalid projection token span",
        ))?;
        let kinds: Vec<TokenKind> = tail.iter().map(|token| token.kind).collect();
        let values = read_projection_tail(&kinds, 64).ok_or(LoweringError::numeric(
            node,
            "projection requires complete nonzero-u32 segments, at most 64",
        ))?;
        values
            .into_iter()
            .map(|value| {
                StableId::new(value).ok_or(LoweringError::numeric(
                    node,
                    "projection segment must be nonzero",
                ))
            })
            .collect()
    }

    fn component(&self, node: usize, parts: &[usize]) -> Result<ComponentDecl> {
        let id = self.id(self.one(node, parts, "ID")?)?;
        let name = self.identifier(self.one(node, parts, "Identifier")?)?;
        let mut owned = Vec::new();
        let mut ports = Vec::new();
        let mut footprints = Vec::new();
        let mut budgets = Vec::new();
        let mut assumptions = Vec::new();
        let mut guarantees = Vec::new();
        for item in self.named(parts, "ComponentItem")? {
            let wrapper = self.parts(item)?;
            let child = self.sole_named(item, &wrapper)?;
            let p = self.parts(child)?;
            match self.name(child)? {
                "Owns" => owned.push(self.id(self.one(child, &p, "ID")?)?),
                "Assume" => assumptions.push(self.id(self.one(child, &p, "ID")?)?),
                "Guarantee" => guarantees.push(self.id(self.one(child, &p, "ID")?)?),
                "Port" => {
                    let ids = self.ids(child, &p, 2)?;
                    let direction = match self.word_at(child, 2)? {
                        "input" => PortDirection::Input,
                        "output" => PortDirection::Output,
                        _ => return self.fail(child, "unknown port direction"),
                    };
                    let name = self.identifier(self.one(child, &p, "Identifier")?)?;
                    ports.push(PortDecl::new(ids[0], direction, name, ids[1]));
                }
                "Footprint" => {
                    let kind = match self.word(child)? {
                        "reads" => FootprintKind::Read,
                        "writes" => FootprintKind::Write,
                        "contexts" => FootprintKind::Context,
                        "effects" => FootprintKind::Effect,
                        "outbox" => FootprintKind::Outbox,
                        _ => return self.fail(child, "unknown footprint kind"),
                    };
                    let path = self.path(self.one(child, &p, "Path")?)?;
                    footprints.push(FootprintDecl::new(kind, path));
                }
                "Budget" => {
                    let resource = match self.word_at(child, 1)? {
                        "steps" => BudgetResource::Steps,
                        "nodes" => BudgetResource::Nodes,
                        "bytes" => BudgetResource::Bytes,
                        "predicate_calls" => BudgetResource::PredicateCalls,
                        _ => return self.fail(child, "unknown budget resource"),
                    };
                    let limit_node = self.one(child, &p, "NonzeroU64")?;
                    let limit = u64::try_from(self.number(limit_node)?)
                        .map_err(|_| LoweringError::numeric(limit_node, "budget exceeds u64"))?;
                    budgets.push(
                        BudgetDecl::try_new(resource, limit)
                            .ok_or(LoweringError::numeric(limit_node, "budget must be nonzero"))?,
                    );
                }
                _ => return self.fail(child, "unsupported component item"),
            }
        }
        Ok(ComponentDecl::new(
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

    fn law(&self, node: usize, parts: &[usize]) -> Result<LawDecl> {
        let id = self.id(self.one(node, parts, "ID")?)?;
        let name = self.identifier(self.one(node, parts, "Identifier")?)?;
        let formula = self.rel(self.one(node, parts, "Rel")?)?;
        let law = LawDecl::new(id, name, formula);
        match self.optional(node, parts, "Applicability")? {
            Some(a) => {
                let scope = match self.word_at(a, 1)? {
                    "any" => LawScope::Always,
                    "accept" => LawScope::Accept,
                    "reject" => LawScope::Reject,
                    "failure" => LawScope::CommittedFailure,
                    "commit" => LawScope::Committing,
                    _ => return self.fail(a, "unknown law scope"),
                };
                let n = &self.tree.nodes[a];
                let genesis = n.end - n.start == 4;
                Ok(law.with_applicability(LawApplicability::new(scope, genesis)))
            }
            None => Ok(law),
        }
    }

    fn claim(&self, node: usize, parts: &[usize]) -> Result<ClaimDecl> {
        let id = self.id(self.one(node, parts, "ID")?)?;
        let name = self.identifier(self.one(node, parts, "Identifier")?)?;
        let backend_node = self.one(node, parts, "Backend")?;
        let backends = match self.word(backend_node)? {
            "cvc5" => vec![BackendId::Cvc5],
            "z3" => vec![BackendId::Z3],
            "lean" => vec![BackendId::Lean],
            "all" => vec![BackendId::Cvc5, BackendId::Z3, BackendId::Lean],
            _ => return self.fail(backend_node, "unknown backend"),
        };
        let tail = self.one(node, parts, "ClaimTail")?;
        let p = self.parts(tail)?;
        match self.word(tail)? {
            "relational" => Ok(ClaimDecl::new(
                id,
                name,
                backends,
                ClaimMode::Relational,
                ClaimFormula::Relational(self.rel(self.one(tail, &p, "Rel")?)?),
            )),
            "finite" => Ok(ClaimDecl::new(
                id,
                name,
                backends,
                ClaimMode::Finite {
                    horizon: self.u32(self.one(tail, &p, "U32")?)?,
                },
                ClaimFormula::Temporal(self.temporal(self.one(tail, &p, "Temporal")?)?),
            )),
            "unbounded" => Ok(ClaimDecl::new(
                id,
                name,
                backends,
                ClaimMode::UnboundedProof,
                ClaimFormula::Temporal(self.temporal(self.one(tail, &p, "Temporal")?)?),
            )),
            "inductive" => {
                let groups = self.one(tail, &p, "InductiveGroups")?;
                let g = self.parts(groups)?;
                let mut every = Vec::new();
                let mut accepts = Vec::new();
                let mut failures = Vec::new();
                let mut kind = "";
                let mut present = false;
                for part in g {
                    if self.name(part)? == "IdList" {
                        let values = self.id_list(part)?;
                        match kind {
                            "assume" => every = values,
                            "accept" => accepts = values,
                            "failure" => failures = values,
                            _ => return self.fail(part, "unlabelled inductive law group"),
                        }
                    } else if self.name(part)?.is_empty() {
                        kind = self.word(part)?;
                        present = true;
                    }
                }
                if !present {
                    return Err(LoweringError {
                        code: DiagnosticCode::ExpectedToken,
                        ..LoweringError::invalid(groups, "inductive claim requires a law group")
                    });
                }
                Ok(ClaimDecl::inductive(
                    id,
                    name,
                    backends,
                    InductiveAssumptions::new(every, accepts, failures),
                    self.rel(self.one(tail, &p, "Rel")?)?,
                ))
            }
            _ => self.fail(tail, "unknown claim mode"),
        }
    }

    fn rel(&self, node: usize) -> Result<RelExpr> {
        self.expression(node)?.relational(node)
    }

    fn temporal(&self, node: usize) -> Result<TemporalFormula> {
        self.expression(node)?.temporal(node)
    }

    fn pop(&self, node: usize, stack: &mut Vec<SemanticValue>) -> Result<SemanticValue> {
        stack
            .pop()
            .ok_or(LoweringError::invalid(node, "missing semantic operand"))
    }

    fn operands(
        &self,
        node: usize,
        stack: &mut Vec<SemanticValue>,
        count: usize,
    ) -> Result<Vec<SemanticValue>> {
        let start = stack
            .len()
            .checked_sub(count)
            .ok_or(LoweringError::invalid(node, "missing semantic operands"))?;
        Ok(stack.split_off(start))
    }

    /// Folds one certified contiguous postorder subtree into an owned AST.
    /// The Rust call stack does not grow with expression nesting. Grammar
    /// identity nodes contribute no value; constructors consume their complete
    /// immediate semantic operands in source order.
    fn expression(&self, root: usize) -> Result<SemanticValue> {
        let first = self
            .tree
            .nodes
            .get(root)
            .ok_or(LoweringError::invalid(root, "missing expression root"))?
            .first_node;
        if first > root {
            return self.fail(root, "invalid expression subtree");
        }
        let mut stack = Vec::new();
        for node in first..=root {
            let name = self.name(node)?;
            // Only semantic action nodes need child inspection. Terminals,
            // EBNF scaffolding, identifiers, and scalar metadata produce none.
            if !matches!(
                name,
                "RelImplies"
                    | "RelOr"
                    | "RelAnd"
                    | "RelNot"
                    | "RelAtom"
                    | "Quantified"
                    | "NamedPredicate"
                    | "Comparison"
                    | "Value"
                    | "Term"
                    | "Negated"
                    | "NonnegativeI128"
                    | "Variable"
                    | "RootProjectionOrVariable"
                    | "DivisionCall"
                    | "FusedCall"
                    | "Sum"
                    | "TemporalUntil"
                    | "TemporalOr"
                    | "TemporalAnd"
                    | "TemporalAtom"
            ) {
                continue;
            }
            let parts = self.parts(node)?;
            let value = match name {
                "RelImplies" => {
                    if self.optional(node, &parts, "RelImplies")?.is_none() {
                        continue;
                    }
                    let right = self.pop(node, &mut stack)?.relational(node)?;
                    let left = self.pop(node, &mut stack)?.relational(node)?;
                    SemanticValue::Relational(RelExpr::Implies(Box::new(left), Box::new(right)))
                }
                "RelOr" | "RelAnd" => {
                    let or = name == "RelOr";
                    let count = self
                        .named(&parts, if or { "RelAnd" } else { "RelNot" })?
                        .len();
                    if count == 1 {
                        continue;
                    }
                    let mut values = self.operands(node, &mut stack, count)?.into_iter();
                    let mut value = values
                        .next()
                        .ok_or(LoweringError::invalid(node, "empty logical chain"))?
                        .relational(node)?;
                    for right in values {
                        let right = right.relational(node)?;
                        value = if or {
                            RelExpr::Or(Box::new(value), Box::new(right))
                        } else {
                            RelExpr::And(Box::new(value), Box::new(right))
                        };
                    }
                    SemanticValue::Relational(value)
                }
                "RelNot" => {
                    if self.optional(node, &parts, "RelNot")?.is_none() {
                        continue;
                    }
                    let value = self.pop(node, &mut stack)?.relational(node)?;
                    SemanticValue::Relational(RelExpr::Not(Box::new(value)))
                }
                "RelAtom" => {
                    if self.keyword(node, "true")? {
                        SemanticValue::Relational(RelExpr::Bool(true))
                    } else if self.keyword(node, "false")? {
                        SemanticValue::Relational(RelExpr::Bool(false))
                    } else {
                        continue;
                    }
                }
                "Quantified" => {
                    let variable = self.identifier(self.one(node, &parts, "Identifier")?)?;
                    let bounds = self.named(&parts, "SignedI128")?;
                    if bounds.len() != 2 {
                        return self.fail(node, "quantifier requires two bounds");
                    }
                    let start = self.signed(bounds[0])?;
                    let end = self.signed(bounds[1])?;
                    let body = Box::new(self.pop(node, &mut stack)?.relational(node)?);
                    SemanticValue::Relational(match self.word(node)? {
                        "forall" => RelExpr::ForAll {
                            variable,
                            start,
                            end,
                            body,
                        },
                        "exists" => RelExpr::Exists {
                            variable,
                            start,
                            end,
                            body,
                        },
                        _ => return self.fail(node, "unknown quantifier"),
                    })
                }
                "NamedPredicate" => {
                    let name = self.identifier(self.one(node, &parts, "PredicateName")?)?;
                    let count = self.named(&parts, "Value")?.len();
                    let arguments = self
                        .operands(node, &mut stack, count)?
                        .into_iter()
                        .map(|value| value.scalar(node))
                        .collect::<Result<Vec<_>>>()?;
                    SemanticValue::Relational(RelExpr::Predicate {
                        name,
                        arguments: arguments.into_boxed_slice(),
                    })
                }
                "Comparison" => {
                    let right = self.pop(node, &mut stack)?.scalar(node)?;
                    let left = self.pop(node, &mut stack)?.scalar(node)?;
                    let op_node = self.one(node, &parts, "CompareOp")?;
                    let op = match self.token_at(op_node, 0)?.kind {
                        TokenKind::EqEq => CompareOp::Eq,
                        TokenKind::NotEq => CompareOp::NotEq,
                        TokenKind::Less => CompareOp::Less,
                        TokenKind::LessEq => CompareOp::LessEq,
                        TokenKind::Greater => CompareOp::Greater,
                        TokenKind::GreaterEq => CompareOp::GreaterEq,
                        _ => return self.fail(op_node, "unknown comparison operator"),
                    };
                    SemanticValue::Relational(RelExpr::Compare(op, left, right))
                }
                "Value" | "Term" => {
                    let additive = name == "Value";
                    let child_rule = if additive { "Term" } else { "ValueAtom" };
                    let count = self.named(&parts, child_rule)?.len();
                    if count == 1 {
                        continue;
                    }
                    let mut values = self.operands(node, &mut stack, count)?.into_iter();
                    let mut value = values
                        .next()
                        .ok_or(LoweringError::invalid(node, "empty arithmetic chain"))?
                        .scalar(node)?;
                    for part in parts {
                        if self.name(part)?.is_empty() || self.name(part)? == "AddOp" {
                            let right = values
                                .next()
                                .ok_or(LoweringError::invalid(part, "missing arithmetic operand"))?
                                .scalar(part)?;
                            value = match self.token_at(part, 0)?.kind {
                                TokenKind::Plus if additive => {
                                    ValueExpr::Add(Box::new(value), Box::new(right))
                                }
                                TokenKind::Minus if additive => {
                                    ValueExpr::Sub(Box::new(value), Box::new(right))
                                }
                                TokenKind::Star if !additive => {
                                    ValueExpr::Mul(Box::new(value), Box::new(right))
                                }
                                _ => return self.fail(part, "unknown arithmetic operator"),
                            };
                        }
                    }
                    if values.next().is_some() {
                        return self.fail(node, "unconsumed arithmetic operand");
                    }
                    SemanticValue::Scalar(value)
                }
                "NonnegativeI128" => {
                    let value = self.number(node)?;
                    match signed_magnitude(false, value) {
                        Some(value) => SemanticValue::Scalar(ValueExpr::Int(value)),
                        None if value == 1u128 << 127 => SemanticValue::MinMagnitude(node),
                        None => {
                            return Err(LoweringError::numeric(
                                node,
                                "positive integer exceeds i128",
                            ));
                        }
                    }
                }
                "Negated" => {
                    let value = self.pop(node, &mut stack)?;
                    let syntax = &self.tree.nodes[node];
                    if syntax.end - syntax.start == 2
                        && self.token_at(node, 1)?.kind == TokenKind::Number(1u128 << 127)
                    {
                        if !matches!(value, SemanticValue::MinMagnitude(_)) {
                            return self.fail(node, "minimum integer magnitude mismatch");
                        }
                        let minimum = signed_magnitude(
                            true,
                            self.number(self.one(node, &parts, "ValueAtom")?)?,
                        )
                        .ok_or(LoweringError::numeric(
                            node,
                            "negative integer exceeds i128",
                        ))?;
                        SemanticValue::Scalar(ValueExpr::Int(minimum))
                    } else {
                        SemanticValue::Scalar(ValueExpr::Sub(
                            Box::new(ValueExpr::Int(0)),
                            Box::new(value.scalar(node)?),
                        ))
                    }
                }
                "Variable" => SemanticValue::Scalar(ValueExpr::Var(self.identifier(node)?)),
                "RootProjectionOrVariable" => {
                    let root_node = self.one(node, &parts, "Root")?;
                    let segments = self.projection_segments(node, root_node)?;
                    let value = if segments.is_empty() {
                        ValueExpr::Var(self.identifier(root_node)?)
                    } else {
                        ProjectionPath::try_new(self.root(root_node)?, segments)
                            .map(ValueExpr::Projection)
                            .ok_or(LoweringError::invalid(
                                node,
                                "projection exceeds 64 segments; suffix cannot be discarded",
                            ))?
                    };
                    SemanticValue::Scalar(value)
                }
                "DivisionCall" => {
                    let right = self.pop(node, &mut stack)?.scalar(node)?;
                    let left = self.pop(node, &mut stack)?.scalar(node)?;
                    let mode = match self.word(node)? {
                        "div_exact" => DivisionMode::Exact,
                        "div_floor" => DivisionMode::Floor,
                        "div_ceil" => DivisionMode::Ceil,
                        _ => return self.fail(node, "unknown division mode"),
                    };
                    SemanticValue::Scalar(ValueExpr::Div(mode, Box::new(left), Box::new(right)))
                }
                "FusedCall" => {
                    let d = self.pop(node, &mut stack)?.scalar(node)?;
                    let b = self.pop(node, &mut stack)?.scalar(node)?;
                    let a = self.pop(node, &mut stack)?.scalar(node)?;
                    SemanticValue::Scalar(self.fused(node, &parts, a, b, d)?)
                }
                "Sum" => {
                    let variable = self.identifier(self.one(node, &parts, "Identifier")?)?;
                    let bounds = self.named(&parts, "SignedI128")?;
                    if bounds.len() != 2 {
                        return self.fail(node, "sum requires two bounds");
                    }
                    let start = self.signed(bounds[0])?;
                    let end = self.signed(bounds[1])?;
                    let body = Box::new(self.pop(node, &mut stack)?.scalar(node)?);
                    SemanticValue::Scalar(ValueExpr::Sum {
                        variable,
                        start,
                        end,
                        body,
                    })
                }
                "TemporalUntil" | "TemporalOr" | "TemporalAnd" => {
                    let child = match name {
                        "TemporalUntil" => "TemporalOr",
                        "TemporalOr" => "TemporalAnd",
                        _ => "TemporalAtom",
                    };
                    let count = self.named(&parts, child)?.len();
                    if count == 1 {
                        continue;
                    }
                    let mut values = self.operands(node, &mut stack, count)?.into_iter();
                    let mut value = values
                        .next()
                        .ok_or(LoweringError::invalid(node, "empty temporal chain"))?
                        .temporal(node)?;
                    for right in values {
                        let right = right.temporal(node)?;
                        value = match name {
                            "TemporalUntil" => {
                                TemporalFormula::Until(Box::new(value), Box::new(right))
                            }
                            "TemporalOr" => TemporalFormula::Or(Box::new(value), Box::new(right)),
                            _ => TemporalFormula::And(Box::new(value), Box::new(right)),
                        };
                    }
                    SemanticValue::Temporal(value)
                }
                "TemporalAtom" => {
                    if self.optional(node, &parts, "TemporalAtom")?.is_some() {
                        let value = Box::new(self.pop(node, &mut stack)?.temporal(node)?);
                        let token = self.token_at(node, 0)?;
                        SemanticValue::Temporal(match token.kind {
                            TokenKind::Bang => TemporalFormula::Not(value),
                            TokenKind::Ident => match token.spelling {
                                "next" => TemporalFormula::Next(value),
                                "always" => TemporalFormula::Always(value),
                                "eventually" => TemporalFormula::Eventually(value),
                                "not" => TemporalFormula::Not(value),
                                _ => return self.fail(node, "unknown temporal prefix"),
                            },
                            _ => return self.fail(node, "unknown temporal prefix"),
                        })
                    } else if self.optional(node, &parts, "Temporal")?.is_some() {
                        continue;
                    } else if self.optional(node, &parts, "Rel")?.is_some() {
                        SemanticValue::Temporal(TemporalFormula::Atom(
                            self.pop(node, &mut stack)?.relational(node)?,
                        ))
                    } else {
                        SemanticValue::Temporal(TemporalFormula::Atom(match self.word(node)? {
                            "true" => RelExpr::Bool(true),
                            "false" => RelExpr::Bool(false),
                            _ => return self.fail(node, "unknown temporal atom"),
                        }))
                    }
                }
                _ => return self.fail(node, "unsupported expression action"),
            };
            stack.push(value);
        }
        let value = self.pop(root, &mut stack)?;
        if !stack.is_empty() {
            return self.fail(root, "unconsumed semantic expression values");
        }
        Ok(value)
    }

    fn fused(
        &self,
        node: usize,
        parts: &[usize],
        a: ValueExpr,
        b: ValueExpr,
        d: ValueExpr,
    ) -> Result<ValueExpr> {
        let a = Box::new(a);
        let b = Box::new(b);
        let d = Box::new(d);
        let (mode, signed, repeated) = match self.word(node)? {
            "mul_div_floor" => (DivisionMode::Floor, false, false),
            "mul_div_ceil" => (DivisionMode::Ceil, false, false),
            "signed_mul_div_floor" => (DivisionMode::Floor, true, false),
            "repeat_mul_div_floor" => (DivisionMode::Floor, false, true),
            "repeat_signed_mul_div_floor" => (DivisionMode::Floor, true, true),
            _ => return self.fail(node, "unknown fused operation"),
        };
        if repeated {
            let count_node = self.one(node, parts, "RepeatCount")?;
            let count = u8::try_from(self.number(count_node)?)
                .ok()
                .filter(|count| *count <= 16)
                .ok_or(LoweringError::numeric(
                    count_node,
                    "repeat count exceeds 16",
                ))?;
            Ok(ValueExpr::RepeatMulDiv(signed, a, b, d, count))
        } else if let Some(scale) = self.optional(node, parts, "Scale")? {
            Ok(ValueExpr::ScaledMulDiv(
                mode,
                signed,
                a,
                b,
                d,
                self.scale(scale)?,
            ))
        } else if signed {
            Ok(ValueExpr::SignedMulDivFloor(a, b, d))
        } else {
            Ok(ValueExpr::MulDiv(mode, a, b, d))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{SourceLimits, parse_project};
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn typed_lowering_matches_recovering_parser_across_every_construct_family() {
        let mut cases: Vec<String> = vec![
            "zeno 1; project 1 p;".into(),
            "zeno 01; project 01 project; namespace 2 namespace; type 3 int amount in -5..=9 unit m/s*kg/1 scale e02; field 4 3 x 3; variant 5 3 no none; variant 6 3 yes 3; reason 7 bad precedence 0; effect 8 e destination 3 payload 3; channel 9 c destination 3 payload 3; component 10 m { owns 3; port 11 input port 3; port 12 output output 3; reads pre.3; writes post.3; contexts context.3; effects effects.3; outbox outbox.3; budget steps 1; budget nodes 2; budget bytes 3; budget predicate_calls 4; assume 20; guarantee 21; } wire 10.12 -> 10.11; merge [10,]; merge []; law 20 l on commit, genesis = true; claim 21 c all relational = false;".into(),
        ];
        for expression in [
            "true",
            "false",
            "not !true && false || true -> false -> true",
            "(true || false) && true",
            "forall x in -3..4 { exists y in 0..1 { x + 1 == y * 2 } }",
            "host_rule()",
            "host_rule(pre, (1 + 2),)",
            "1 + 2 * 3 - 4 - 5 == 0",
            "-170141183460469231731687303715884105728 == -1",
            "-(1 + 2) == 0",
            "div_exact(7,3) == 2",
            "div_floor(7,3) != 1",
            "div_ceil(7,3) <= 3",
            "mul_div_floor(1,2,3) < 5",
            "mul_div_ceil(1,2,3,e2) >= 0",
            "signed_mul_div_floor(1,-2,3) > 0",
            "signed_mul_div_floor(1,-2,3,e0) == 0",
            "repeat_mul_div_floor(1,2,3,16) == 0",
            "repeat_signed_mul_div_floor(1,-2,3,0) == 0",
            "sum x in -3..4 { x * x } == 0",
            "pre.1.2 == post.1.2",
            "pre == pre",
        ] {
            cases.push(format!("zeno 1; project 1 p; law 1 l = {expression};"));
        }
        for mode in [
            "relational = true",
            "finite 1 = next always eventually not !atom(1 == 1) until (true && false) || true",
            "unbounded = atom(true)",
            "inductive assume [1,] accept [] failure [2] = true",
        ] {
            cases.push(format!("zeno 1; project 1 p; claim 1 c all {mode};"));
        }
        for source in cases {
            let old = crate::parser::parse_project_recovering(&source, SourceLimits::default())
                .unwrap_or_else(|error| panic!("reference refused {source}: {error}"));
            let new = parse_project(&source, SourceLimits::default())
                .unwrap_or_else(|error| panic!("checked parser refused {source}: {error}"));
            assert_eq!(new, old, "complete AST including source spans: {source}");
        }
    }
}
