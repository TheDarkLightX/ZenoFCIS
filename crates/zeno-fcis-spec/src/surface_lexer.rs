//! Bounded token/diagnostic shell around the verified functional lexical core.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

pub(crate) use crate::lexical_core::LexemeKind as TokenKind;
use crate::lexical_core::{LexemeKind, next_lexeme};
pub(crate) use crate::source_token::Token;
use crate::source_token::tokens_match_source;
use crate::{AstPath, Diagnostic, DiagnosticCode, DiagnosticStage, SourceLimits, SourceSpan};

pub(crate) fn lex(source: &str, limits: SourceLimits) -> (Vec<Token<'_>>, Vec<Diagnostic>) {
    if source.len() > limits.max_bytes() {
        return (
            vec![Token {
                kind: TokenKind::End,
                span: SourceSpan::new(0, 0, 1, 1),
                spelling: "",
            }],
            vec![diagnostic(
                DiagnosticCode::SourceTooLarge,
                SourceSpan::new(0, u32_len(source.len()), 1, 1),
                "source within configured byte limit",
                "source too large",
                "reduce the file to at most the configured limit",
            )],
        );
    }
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let (mut index, mut line, mut column) = (0usize, 1u32, 1u32);
    while index < bytes.len() {
        let item = next_lexeme(bytes, index);
        let start = index;
        let start_column = column;
        match item.kind {
            LexemeKind::Space | LexemeKind::Comment => {
                index = item.end;
                column = column.saturating_add(u32_len(index - start));
                continue;
            }
            LexemeKind::Newline => {
                index = item.end;
                line = line.saturating_add(1);
                column = 1;
                continue;
            }
            LexemeKind::End => break,
            _ => {}
        }
        // Trivia never consumes a token. EOF is outside the token budget.
        if tokens.len() >= limits.max_tokens() {
            diagnostics.push(diagnostic(
                DiagnosticCode::TokenLimit,
                span(start, start, line, start_column),
                "token count within configured bound",
                "token limit exceeded",
                "reduce declarations or raise a bounded SourceLimits value",
            ));
            break;
        }
        index = if item.kind == LexemeKind::InvalidByte {
            start + source[start..].chars().next().map_or(1, char::len_utf8)
        } else {
            item.end
        };
        column = column.saturating_add(u32_len(index - start));
        let location = span(start, index, line, start_column);
        let spelling = match item.kind {
            LexemeKind::Ident => &source[start..index],
            LexemeKind::Hex => &source[start + 2..index],
            _ => "",
        };
        let kind = match item.kind {
            LexemeKind::InvalidNumber => {
                diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidNumber,
                    location,
                    "unsigned 128-bit integer magnitude",
                    &source[start..index],
                    "use a decimal magnitude within the u128 range",
                ));
                None
            }
            LexemeKind::InvalidHex => {
                diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidNumber,
                    location,
                    "0x followed by 64 lowercase hex digits",
                    &source[start + 2..index],
                    "use the canonical lowercase 32-byte commitment form",
                ));
                None
            }
            LexemeKind::InvalidByte => {
                diagnostics.push(diagnostic(
                    DiagnosticCode::UnexpectedCharacter,
                    location,
                    "ASCII .zeno token",
                    &source[start..index],
                    "remove shell syntax, interpolation, quotes, or non-grammar characters",
                ));
                None
            }
            LexemeKind::End | LexemeKind::Space | LexemeKind::Newline | LexemeKind::Comment => None,
            value => Some(value),
        };
        if let Some(kind) = kind {
            tokens.push(Token {
                kind,
                span: location,
                spelling,
            });
        }
        // An invalid byte does not consume a token, so the token budget cannot
        // bound diagnostic allocation. Keep one overflow diagnostic so the
        // returned DiagnosticSet can accurately report truncated diagnostics.
        if diagnostics.len() > limits.max_diagnostics() {
            break;
        }
    }
    if diagnostics.is_empty() && !tokens_match_source(source, &tokens, limits.max_tokens()) {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDeclaration,
            span(index, index, line, column),
            "exact complete source tokenization",
            "token stream failed independent validation",
            "report this parser validation failure",
        ));
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: span(index, index, line, column),
        spelling: "",
    });
    (tokens, diagnostics)
}

fn span(start: usize, end: usize, line: u32, column: u32) -> SourceSpan {
    SourceSpan::new(u32_len(start), u32_len(end), line, column)
}
fn u32_len(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
fn diagnostic(
    code: DiagnosticCode,
    span: SourceSpan,
    expected: impl Into<Box<str>>,
    actual: impl Into<Box<str>>,
    remediation: impl Into<Box<str>>,
) -> Diagnostic {
    Diagnostic::new(
        code,
        DiagnosticStage::Lex,
        AstPath::new("source"),
        span,
        expected,
        actual,
        remediation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_binds_actual_token_payloads_and_complete_source() {
        let source = "zeno 1; project 2 alpha; // reviewed source\n";
        let (mut tokens, errors) = lex(source, SourceLimits::default());
        assert!(errors.is_empty());
        tokens.pop(); // EOF is lookahead, outside the certified frontier.
        assert!(tokens_match_source(source, &tokens, 7));

        let mut changed = tokens.clone();
        changed[5].spelling = "beta";
        assert!(!tokens_match_source(source, &changed, 7));
        let mut changed = tokens.clone();
        changed[4].kind = TokenKind::Number(3);
        assert!(!tokens_match_source(source, &changed, 7));
        let mut changed = tokens.clone();
        changed[2].kind = TokenKind::Comma;
        assert!(!tokens_match_source(source, &changed, 7));
        let mut changed = tokens.clone();
        changed[0].span = SourceSpan::new(1, 4, 1, 1);
        assert!(!tokens_match_source(source, &changed, 7));
        let mut changed = tokens.clone();
        changed.remove(4);
        assert!(!tokens_match_source(source, &changed, 7));
        let mut changed = tokens.clone();
        changed.push(tokens[6].clone());
        assert!(!tokens_match_source(source, &changed, 8));
        assert!(!tokens_match_source("zeno 1; project 3 alpha;", &tokens, 7));
        assert!(!tokens_match_source(source, &tokens, 6));
    }

    #[test]
    fn commitment_payload_is_bound_to_its_source_bytes() {
        let source = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (mut tokens, errors) = lex(source, SourceLimits::default());
        assert!(errors.is_empty());
        tokens.pop();
        assert!(tokens_match_source(source, &tokens, 1));
        tokens[0].spelling = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        assert!(!tokens_match_source(source, &tokens, 1));
    }

    #[test]
    fn invalid_source_stops_at_the_diagnostic_budget() {
        let source = "?".repeat(10_000);
        let Some(limits) = SourceLimits::try_new(20_000, 100, 2) else {
            panic!("valid test limits");
        };
        let (tokens, errors) = lex(&source, limits);
        assert_eq!(errors.len(), 3); // two retained, one proves truncation
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::End);
        assert_eq!(tokens[0].span.start(), 3);
        let retained = crate::DiagnosticSet::from_vec(errors, limits.max_diagnostics());
        assert_eq!(retained.len(), 2);
        assert!(retained.is_truncated());
    }
}
