//! Checked complete-source acceptance and bounded strict diagnostics.

use crate::lexer::{TokenKind, lex};
use crate::syntax::parse_tokens;
use crate::syntax_lowering::lower;
use crate::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, ParsedProject,
    SourceLimits, SourceSpan,
};
use alloc::{format, string::String, vec};

/// Parses the complete source through a checked lossless syntax derivation.
///
/// Successful acceptance uses the functional lexer, ordered grammar and typed
/// CST lowering. Diagnostics come from the strict path; no recovering parser
/// constructs an unchecked AST from a rejected source or valid prefix.
pub fn parse_project(source: &str, limits: SourceLimits) -> Result<ParsedProject, DiagnosticSet> {
    let (tokens, lexical_errors) = lex(source, limits);
    if !lexical_errors.is_empty() {
        return Err(DiagnosticSet::from_vec(
            lexical_errors,
            limits.max_diagnostics(),
        ));
    }
    let refusal = match parse_tokens(&tokens) {
        Ok(tree) => match lower(&tree, &tokens, limits.max_diagnostics()) {
            Ok(parsed) => return Ok(parsed),
            Err(error) => {
                let position = tree.nodes.get(error.node).map_or(0, |n| n.start);
                (
                    position,
                    if error.resource_limit {
                        DiagnosticCode::ResourceLimit
                    } else {
                        error.code
                    },
                    error.message,
                )
            }
        },
        Err(error) => (
            error.token,
            if error.resource_limit {
                DiagnosticCode::ResourceLimit
            } else {
                error.code
            },
            error.message,
        ),
    };
    let found = tokens.get(refusal.0).or_else(|| tokens.last());
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
        refusal.1,
        DiagnosticStage::Parse,
        AstPath::new("source.syntax"),
        span,
        refusal.2,
        actual,
        "correct the indicated syntax or reduce its bounded complexity",
    )];
    // Additional diagnostics inspect statement boundaries only. They never
    // recover a semantic value or construct an AST from a rejected prefix.
    // Retain one overflow diagnostic so DiagnosticSet can report truncation.
    if refusal.1 != DiagnosticCode::ResourceLimit {
        let mut depth = 0usize;
        let mut boundary = true;
        for (index, token) in tokens.iter().enumerate() {
            if depth == 0
                && boundary
                && !matches!(token.kind, TokenKind::End | TokenKind::Semicolon)
            {
                if index != refusal.0
                    && !crate::syntax_grammar::is_declaration_start(token)
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
    Err(DiagnosticSet::from_vec(
        diagnostics,
        limits.max_diagnostics(),
    ))
}
