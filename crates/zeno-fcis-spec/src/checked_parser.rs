//! Exact lexical and complete syntax acceptance before semantic expansion.
//!
//! The 2.6 semantic expansion remains a distinct, unproved mapping boundary.
//! Deep ordinary core expressions use its independently tested iterative fold;
//! no rejected extended certificate can enter either semantic path.

use crate::surface_lexer::{self, Token, TokenKind};
use crate::syntax::{self, Grammar, Root, SyntaxError};
use crate::syntax_resources;
use crate::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, ParsedProject, RelExpr,
    SourceLimits, SourceSpan,
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

pub(crate) enum CoreValue {
    Project(ParsedProject),
    Rel(RelExpr),
}

pub(crate) struct CheckedInput {
    pub(crate) tokens: Vec<crate::lexer::Token>,
    pub(crate) core: Option<CoreValue>,
}

/// Extension-sensitive contextual words disable the old core action mapping.
/// Core grammar recognition alone cannot establish the 2.6 interpretation of
/// names such as `u128` or `circuit`, even when a core production accepts them.
fn core_eligible(tree: &syntax::SyntaxTree, tokens: &[Token<'_>]) -> bool {
    !tree.nodes.iter().any(|node| {
        !node.symbol.terminal
            && (matches!(
                tree.grammar.rule_name(node.symbol.tag),
                Some(
                    "Table"
                        | "ConstantTable"
                        | "DerivedTable"
                        | "Map"
                        | "Entry"
                        | "Order"
                        | "Summary"
                        | "Tally"
                        | "Random"
                        | "Distribution"
                        | "Reward"
                )
            ) || (matches!(
                tree.grammar.rule_name(node.symbol.tag),
                Some("ExprNamed" | "ExprCall")
            ) && tokens.get(node.start).is_some_and(|token| {
                token.kind == TokenKind::Ident
                    && matches!(
                        token.spelling,
                        "choose"
                            | "join"
                            | "meet"
                            | "complement"
                            | "u128"
                            | "to_i128"
                            | "to_u128"
                            | "circuit"
                            | "subset"
                    )
            })))
    })
}

/// Removes only an outer pair whose complete contents are one paired group.
/// Only certified ExprAtom/TemporalAtom grouping nodes are eligible; call
/// delimiters are never projected out. At least one grouping pair remains, so
/// precedence and the semantic parser's scalar/relational decision are kept.
/// The original complete source is certified before this bounded projection;
/// the resulting semantic stream is certified again before it is consumed.
fn compact_groups<'a>(tree: &syntax::SyntaxTree, tokens: &[Token<'a>]) -> Option<Vec<Token<'a>>> {
    let mut closes = vec![usize::MAX; tokens.len()];
    let is_group = |node: &crate::syntax_certificate::Node| {
        !node.symbol.terminal
            && matches!(
                tree.grammar.rule_name(node.symbol.tag),
                Some("ExprAtom" | "TemporalAtom")
            )
            && tokens
                .get(node.start)
                .is_some_and(|token| token.kind == TokenKind::LParen)
    };
    for node in tree.nodes.iter().filter(|node| is_group(node)) {
        closes[node.start] = node.end - 1;
    }
    let mut omit = vec![false; tokens.len()];
    let mut removed = false;
    for node in tree.nodes.iter().filter(|node| is_group(node)) {
        if closes.get(node.start + 1).copied() == node.end.checked_sub(2) {
            omit[node.start] = true;
            omit[node.end - 1] = true;
            removed = true;
        }
    }
    if !removed {
        return None;
    }
    Some(
        tokens
            .iter()
            .enumerate()
            .filter(|(index, _)| !omit[*index])
            .map(|(_, token)| token.clone())
            .collect(),
    )
}

pub(crate) fn checked_tokens(
    source: &str,
    limits: SourceLimits,
    root: Root,
    core_environment: bool,
    ambient_entries: Option<&BTreeMap<u32, u32>>,
    ambient_derived: &BTreeMap<String, syntax_resources::DerivedHeight>,
    ambient_tables: Option<&BTreeMap<u32, u32>>,
) -> Result<CheckedInput, DiagnosticSet> {
    let (mut tokens, lexical_errors) = surface_lexer::lex(source, limits);
    if !lexical_errors.is_empty() {
        return Err(DiagnosticSet::from_vec(
            lexical_errors,
            limits.max_diagnostics(),
        ));
    }
    let prepare = |tokens: &[Token<'_>]| -> Result<_, SyntaxError> {
        let tree = syntax::parse_tokens(tokens, Grammar::Extended, root)?;
        // Only these authored roots can introduce recursive expressions. The
        // grammar certificate still checks every token and declaration before
        // an expression-free project avoids numeric-expression scratch.
        if !tree.nodes.iter().any(|node| {
            !node.symbol.terminal
                && matches!(
                    tree.grammar.rule_name(node.symbol.tag),
                    Some("Rel" | "Value" | "Temporal" | "TableRead")
                )
        }) {
            return Ok((tree, syntax_resources::ResourceUse { recursion: 0 }));
        }
        let mut counts = BTreeMap::new();
        if let Some(entries) = ambient_entries {
            for &map in entries.values() {
                *counts.entry(map).or_insert(0usize) += 1;
            }
        } else {
            // Mirror first-entry-ID ownership from the semantic prescan, using
            // only complete Entry declarations selected by this certificate.
            let mut entries = BTreeMap::new();
            for node in &tree.nodes {
                if node.symbol.terminal || tree.grammar.rule_name(node.symbol.tag) != Some("Entry")
                {
                    continue;
                }
                if let (Some(id), Some(map)) =
                    (tokens.get(node.start + 1), tokens.get(node.start + 4))
                    && let (TokenKind::Number(id), TokenKind::Number(map)) = (id.kind, map.kind)
                    && let (Ok(id), Ok(map)) = (u32::try_from(id), u32::try_from(map))
                {
                    entries.entry(id).or_insert(map);
                }
            }
            for &map in entries.values() {
                *counts.entry(map).or_insert(0usize) += 1;
            }
        }
        let mut table_sizes = ambient_tables.cloned().unwrap_or_default();
        if ambient_tables.is_none() {
            // Match the semantic prescan's first stored table and last constant
            // declaration ownership, while considering every certified source
            // declaration before processing expression bodies.
            for node in &tree.nodes {
                if node.symbol.terminal {
                    continue;
                }
                let constant = tree.grammar.rule_name(node.symbol.tag) == Some("ConstantTable");
                if !constant && tree.grammar.rule_name(node.symbol.tag) != Some("Table") {
                    continue;
                }
                let id_at = node.start + if constant { 2 } else { 1 };
                if let (Some(id), Some(size)) = (tokens.get(id_at), tokens.get(node.start + 6))
                    && let (TokenKind::Number(id), TokenKind::Number(size)) = (id.kind, size.kind)
                    && let (Ok(id), Ok(size)) = (u32::try_from(id), u32::try_from(size))
                {
                    if constant {
                        table_sizes.insert(id, size);
                    } else {
                        table_sizes.entry(id).or_insert(size);
                    }
                }
            }
        }
        let resources =
            syntax_resources::check(&tree, tokens, &counts, ambient_derived, &table_sizes)?;
        Ok((tree, resources))
    };
    let (tree, mut resources) =
        prepare(&tokens).map_err(|error| refusal(&tokens, limits, root, error))?;
    let mut core = None;
    if resources.recursion > syntax_resources::MAX_SEMANTIC_RECURSION {
        let unsafe_depth = SyntaxError {
            token: 0,
            resource_limit: true,
            code: DiagnosticCode::ResourceLimit,
            message: "extended expression nesting exceeds the semantic expansion limit",
        };
        if !core_eligible(&tree, &tokens) {
            return Err(refusal(&tokens, limits, root, unsafe_depth));
        }
        if let Some(compact) = compact_groups(&tree, &tokens) {
            let (_, compact_resources) =
                prepare(&compact).map_err(|error| refusal(&compact, limits, root, error))?;
            tokens = compact;
            resources = compact_resources;
        }
        if resources.recursion > syntax_resources::MAX_SEMANTIC_RECURSION {
            if !core_environment {
                return Err(refusal(&tokens, limits, root, unsafe_depth));
            }
            let core_tree = syntax::parse_tokens(&tokens, Grammar::Core, root)
                .map_err(|_| refusal(&tokens, limits, root, unsafe_depth))?;
            let result = match root {
                Root::Project => {
                    crate::syntax_lowering::lower(&core_tree, &tokens, limits.max_diagnostics())
                        .map(CoreValue::Project)
                }
                Root::Rel => {
                    crate::syntax_lowering::lower_rel(&core_tree, &tokens).map(CoreValue::Rel)
                }
                Root::Value | Root::TableRead => {
                    return Err(refusal(&tokens, limits, root, unsafe_depth));
                }
            };
            core = Some(result.map_err(|error| {
                refusal(
                    &tokens,
                    limits,
                    root,
                    SyntaxError {
                        token: core_tree.nodes.get(error.node).map_or(0, |node| node.start),
                        resource_limit: error.resource_limit,
                        code: error.code,
                        message: error.message,
                    },
                )
            })?);
        }
    }
    let owned = tokens
        .iter()
        .map(crate::lexer::from_surface)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            refusal(
                &tokens,
                limits,
                root,
                SyntaxError {
                    token: 0,
                    resource_limit: false,
                    code: DiagnosticCode::InvalidDeclaration,
                    message: "validated lexical frontier contains a non-token",
                },
            )
        })?;
    Ok(CheckedInput {
        tokens: owned,
        core,
    })
}

fn refusal(
    tokens: &[Token<'_>],
    limits: SourceLimits,
    root: Root,
    error: SyntaxError,
) -> DiagnosticSet {
    let found = tokens.get(error.token).or_else(|| tokens.last());
    let span = found.map_or(SourceSpan::new(0, 0, 1, 1), |token| token.span);
    let actual = found.map_or_else(
        || String::from("end of source"),
        |token| {
            if token.spelling.is_empty() {
                format!("{:?}", token.kind)
            } else {
                format!("{:?}: {}", token.kind, token.spelling)
            }
        },
    );
    let mut diagnostics = vec![Diagnostic::new(
        error.code,
        DiagnosticStage::Parse,
        AstPath::new("source.syntax"),
        span,
        error.message,
        actual,
        "correct the indicated syntax or reduce its bounded complexity",
    )];
    if root == Root::Project && !error.resource_limit {
        let mut depth = 0usize;
        let mut boundary = true;
        for (index, token) in tokens.iter().enumerate() {
            if depth == 0
                && boundary
                && !matches!(token.kind, TokenKind::End | TokenKind::Semicolon)
            {
                if index != error.token
                    && !crate::syntax_grammar_extended::is_declaration_start(token)
                    && !matches!(token.spelling, "zeno" | "project")
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::InvalidDeclaration,
                        DiagnosticStage::Parse,
                        AstPath::new("source.declarations"),
                        token.span,
                        "top-level declaration keyword",
                        format!("{:?}: {}", token.kind, token.spelling),
                        "use a declaration from the version-1 grammar",
                    ));
                    if diagnostics.len() > limits.max_diagnostics() {
                        break;
                    }
                }
                boundary = false;
            }
            match token.kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && token.kind == TokenKind::RBrace {
                        boundary = true;
                    }
                }
                TokenKind::Semicolon if depth == 0 => boundary = true,
                _ => {}
            }
        }
    }
    DiagnosticSet::from_vec(diagnostics, limits.max_diagnostics())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_semantic_recursion_edges_use_the_normal_test_stack() {
        for depth in [31usize, 32] {
            let mut division = String::from("1");
            let mut selected = String::from("1");
            for _ in 0..depth {
                division = format!("div_exact(1,{division})");
                selected = format!("choose(true,1,{selected})");
            }
            for value in [
                division,
                selected,
                format!("{}1{}", "(".repeat(depth), ")".repeat(depth)),
            ] {
                let source = format!("zeno 1; project 1 p; law 2 l = {value} == 1;");
                assert!(
                    crate::parse_project(&source, SourceLimits::default()).is_ok(),
                    "depth {depth}"
                );
            }
        }
        for depth in [33usize, 80, 255] {
            for expression in [
                format!("{}true", "not ".repeat(depth)),
                format!("{}1{} == 1", "(".repeat(depth), ")".repeat(depth)),
            ] {
                let source = format!("zeno 1; project 1 p; law 2 l = {expression};");
                assert!(
                    crate::parse_project(&source, SourceLimits::default()).is_ok(),
                    "depth {depth}"
                );
            }
        }
    }

    #[test]
    fn nonredundant_scalar_groups_reach_the_source_bound_without_recursion() {
        for depth in [40usize, 80, 256] {
            let mut value = String::from("1");
            for level in 0..depth {
                value = if level % 2 == 0 {
                    format!("({value} * 3)")
                } else {
                    format!("({value} + 2)")
                };
            }
            let source = format!("zeno 1; project 1 p; law 2 l = {value} == 1;");
            assert!(
                crate::parse_project(&source, SourceLimits::default()).is_ok(),
                "depth {depth}"
            );
        }
    }

    #[test]
    fn grouping_projection_never_changes_call_literal_requirements() {
        for source in [
            format!(
                "zeno 1; project 1 p; law 2 l = u128({}1{}) == u128(1);",
                "(".repeat(80),
                ")".repeat(80)
            ),
            format!(
                "zeno 1; project 1 p; law 2 l = -{}170141183460469231731687303715884105728{} == 0;",
                "(".repeat(80),
                ")".repeat(80)
            ),
        ] {
            assert!(crate::parse_project(&source, SourceLimits::default()).is_err());
        }
    }
}
