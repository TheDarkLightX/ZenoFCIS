//! Bounded hand-written lexer.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::{AstPath, Diagnostic, DiagnosticCode, DiagnosticStage, SourceLimits, SourceSpan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TokenKind {
    Ident(Box<str>),
    Number(u64),
    Hex(Box<str>),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Range,
    Colon,
    Equal,
    EqEq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Plus,
    Minus,
    Star,
    Slash,
    AndAnd,
    OrOr,
    Bang,
    Arrow,
    Eof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: SourceSpan,
}

pub(crate) fn lex(source: &str, limits: SourceLimits) -> (Vec<Token>, Vec<Diagnostic>) {
    if source.len() > limits.max_bytes() {
        return (
            vec![Token {
                kind: TokenKind::Eof,
                span: SourceSpan::new(0, 0, 1, 1),
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
        if tokens.len() >= limits.max_tokens() {
            diagnostics.push(diagnostic(
                DiagnosticCode::TokenLimit,
                span(index, index, line, column),
                "token count within configured bound",
                "token limit exceeded",
                "reduce declarations or raise a bounded SourceLimits value",
            ));
            break;
        }
        let byte = bytes[index];
        if matches!(byte, b' ' | b'\t' | b'\r') {
            index += 1;
            column = column.saturating_add(1);
            continue;
        }
        if byte == b'\n' {
            index += 1;
            line = line.saturating_add(1);
            column = 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            column = column.saturating_add(2);
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
                column = column.saturating_add(1);
            }
            continue;
        }
        let start = index;
        let start_column = column;
        if byte.is_ascii_alphabetic() || byte == b'_' {
            index += 1;
            column = column.saturating_add(1);
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'-'))
            {
                index += 1;
                column = column.saturating_add(1);
            }
            let value = &source[start..index];
            tokens.push(Token {
                kind: TokenKind::Ident(Box::from(value)),
                span: span(start, index, line, start_column),
            });
            continue;
        }
        if byte.is_ascii_digit() {
            index += 1;
            column = column.saturating_add(1);
            if byte == b'0' && bytes.get(index) == Some(&b'x') {
                index += 1;
                column = column.saturating_add(1);
                let hex_start = index;
                while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                    index += 1;
                    column = column.saturating_add(1);
                }
                let value = &source[hex_start..index];
                if value.len() != 64 || value.bytes().any(|value| value.is_ascii_uppercase()) {
                    diagnostics.push(diagnostic(
                        DiagnosticCode::InvalidNumber,
                        span(start, index, line, start_column),
                        "0x followed by 64 lowercase hex digits",
                        value,
                        "use the canonical lowercase 32-byte commitment form",
                    ));
                } else {
                    tokens.push(Token {
                        kind: TokenKind::Hex(Box::from(value)),
                        span: span(start, index, line, start_column),
                    });
                }
                continue;
            }
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
                column = column.saturating_add(1);
            }
            let value = &source[start..index];
            match value.parse::<u64>() {
                Ok(number) => tokens.push(Token {
                    kind: TokenKind::Number(number),
                    span: span(start, index, line, start_column),
                }),
                Err(_) => diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidNumber,
                    span(start, index, line, start_column),
                    "unsigned 64-bit integer",
                    value,
                    "use a decimal value within the u64 range",
                )),
            }
            continue;
        }
        let (kind, width) = match (byte, bytes.get(index + 1).copied()) {
            (b'.', Some(b'.')) => (Some(TokenKind::Range), 2),
            (b'=', Some(b'=')) => (Some(TokenKind::EqEq), 2),
            (b'!', Some(b'=')) => (Some(TokenKind::NotEq), 2),
            (b'<', Some(b'=')) => (Some(TokenKind::LessEq), 2),
            (b'>', Some(b'=')) => (Some(TokenKind::GreaterEq), 2),
            (b'&', Some(b'&')) => (Some(TokenKind::AndAnd), 2),
            (b'|', Some(b'|')) => (Some(TokenKind::OrOr), 2),
            (b'-', Some(b'>')) => (Some(TokenKind::Arrow), 2),
            (b'{', _) => (Some(TokenKind::LBrace), 1),
            (b'}', _) => (Some(TokenKind::RBrace), 1),
            (b'(', _) => (Some(TokenKind::LParen), 1),
            (b')', _) => (Some(TokenKind::RParen), 1),
            (b'[', _) => (Some(TokenKind::LBracket), 1),
            (b']', _) => (Some(TokenKind::RBracket), 1),
            (b';', _) => (Some(TokenKind::Semicolon), 1),
            (b',', _) => (Some(TokenKind::Comma), 1),
            (b'.', _) => (Some(TokenKind::Dot), 1),
            (b':', _) => (Some(TokenKind::Colon), 1),
            (b'=', _) => (Some(TokenKind::Equal), 1),
            (b'<', _) => (Some(TokenKind::Less), 1),
            (b'>', _) => (Some(TokenKind::Greater), 1),
            (b'+', _) => (Some(TokenKind::Plus), 1),
            (b'-', _) => (Some(TokenKind::Minus), 1),
            (b'*', _) => (Some(TokenKind::Star), 1),
            (b'/', _) => (Some(TokenKind::Slash), 1),
            (b'!', _) => (Some(TokenKind::Bang), 1),
            _ => (
                None,
                source[index..].chars().next().map_or(1, char::len_utf8),
            ),
        };
        index = index.saturating_add(width);
        column = column.saturating_add(u32_len(width));
        if let Some(kind) = kind {
            tokens.push(Token {
                kind,
                span: span(start, index, line, start_column),
            });
        } else {
            diagnostics.push(diagnostic(
                DiagnosticCode::UnexpectedCharacter,
                span(start, index, line, start_column),
                "ASCII .zeno token",
                &source[start..index],
                "remove shell syntax, interpolation, quotes, or non-grammar characters",
            ));
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: span(index, index, line, column),
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
