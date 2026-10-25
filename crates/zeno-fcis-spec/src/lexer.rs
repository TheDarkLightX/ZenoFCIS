//! Exact owned adapter for tokens already certified against the source.
//!
//! The semantic expansion pass retains its 2.6 token representation. Only this
//! exhaustive conversion can construct that representation from the validated
//! borrowed frontier; trivia and lexical failures are refused.

use crate::SourceSpan;
use crate::lexical_core::LexemeKind;
use alloc::boxed::Box;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TokenKind {
    Ident(Box<str>),
    // A magnitude, so the signed i128 minimum can be read after a minus sign.
    // Identifiers, counts and resource limits narrow at their existing readers.
    Number(u128),
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

pub(crate) fn from_surface(token: &crate::surface_lexer::Token<'_>) -> Option<Token> {
    let kind = match token.kind {
        LexemeKind::Ident => TokenKind::Ident(Box::from(token.spelling)),
        LexemeKind::Number(value) => TokenKind::Number(value),
        LexemeKind::Hex => TokenKind::Hex(Box::from(token.spelling)),
        LexemeKind::End => TokenKind::Eof,
        LexemeKind::LBrace => TokenKind::LBrace,
        LexemeKind::RBrace => TokenKind::RBrace,
        LexemeKind::LParen => TokenKind::LParen,
        LexemeKind::RParen => TokenKind::RParen,
        LexemeKind::LBracket => TokenKind::LBracket,
        LexemeKind::RBracket => TokenKind::RBracket,
        LexemeKind::Semicolon => TokenKind::Semicolon,
        LexemeKind::Comma => TokenKind::Comma,
        LexemeKind::Dot => TokenKind::Dot,
        LexemeKind::Range => TokenKind::Range,
        LexemeKind::Colon => TokenKind::Colon,
        LexemeKind::Equal => TokenKind::Equal,
        LexemeKind::EqEq => TokenKind::EqEq,
        LexemeKind::NotEq => TokenKind::NotEq,
        LexemeKind::Less => TokenKind::Less,
        LexemeKind::LessEq => TokenKind::LessEq,
        LexemeKind::Greater => TokenKind::Greater,
        LexemeKind::GreaterEq => TokenKind::GreaterEq,
        LexemeKind::Plus => TokenKind::Plus,
        LexemeKind::Minus => TokenKind::Minus,
        LexemeKind::Star => TokenKind::Star,
        LexemeKind::Slash => TokenKind::Slash,
        LexemeKind::AndAnd => TokenKind::AndAnd,
        LexemeKind::OrOr => TokenKind::OrOr,
        LexemeKind::Bang => TokenKind::Bang,
        LexemeKind::Arrow => TokenKind::Arrow,
        LexemeKind::Space
        | LexemeKind::Newline
        | LexemeKind::Comment
        | LexemeKind::InvalidNumber
        | LexemeKind::InvalidHex
        | LexemeKind::InvalidByte => return None,
    };
    Some(Token {
        kind,
        span: token.span,
    })
}

pub(crate) fn lexeme_kind(kind: &TokenKind) -> LexemeKind {
    match kind {
        TokenKind::Ident(_) => LexemeKind::Ident,
        TokenKind::Number(value) => LexemeKind::Number(*value),
        TokenKind::Hex(_) => LexemeKind::Hex,
        TokenKind::Eof => LexemeKind::End,
        TokenKind::LBrace => LexemeKind::LBrace,
        TokenKind::RBrace => LexemeKind::RBrace,
        TokenKind::LParen => LexemeKind::LParen,
        TokenKind::RParen => LexemeKind::RParen,
        TokenKind::LBracket => LexemeKind::LBracket,
        TokenKind::RBracket => LexemeKind::RBracket,
        TokenKind::Semicolon => LexemeKind::Semicolon,
        TokenKind::Comma => LexemeKind::Comma,
        TokenKind::Dot => LexemeKind::Dot,
        TokenKind::Range => LexemeKind::Range,
        TokenKind::Colon => LexemeKind::Colon,
        TokenKind::Equal => LexemeKind::Equal,
        TokenKind::EqEq => LexemeKind::EqEq,
        TokenKind::NotEq => LexemeKind::NotEq,
        TokenKind::Less => LexemeKind::Less,
        TokenKind::LessEq => LexemeKind::LessEq,
        TokenKind::Greater => LexemeKind::Greater,
        TokenKind::GreaterEq => LexemeKind::GreaterEq,
        TokenKind::Plus => LexemeKind::Plus,
        TokenKind::Minus => LexemeKind::Minus,
        TokenKind::Star => LexemeKind::Star,
        TokenKind::Slash => LexemeKind::Slash,
        TokenKind::AndAnd => LexemeKind::AndAnd,
        TokenKind::OrOr => LexemeKind::OrOr,
        TokenKind::Bang => LexemeKind::Bang,
        TokenKind::Arrow => LexemeKind::Arrow,
    }
}
