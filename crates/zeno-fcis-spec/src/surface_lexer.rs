//! Bounded token/diagnostic shell around the verified functional lexical core.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, string::StringSliceAdditionalSpecFns};

pub(crate) use crate::lexical_core::LexemeKind as TokenKind;
use crate::lexical_core::{LexemeKind, next_lexeme};
pub(crate) use crate::source_token::Token;
use crate::source_token::finish_tokens;
use crate::{AstPath, Diagnostic, DiagnosticCode, DiagnosticStage, SourceLimits, SourceSpan};

#[cfg(verus_keep_ghost)]
verus! {
    use vstd::utf8::*;

    proof fn ascii_boundaries(bytes: Seq<u8>, offset: int)
        requires valid_utf8(bytes), 0 <= offset < bytes.len(), bytes[offset] < 128,
        ensures is_char_boundary(bytes, offset), is_char_boundary(bytes, offset + 1),
        decreases bytes.len(),
    {
        let width = length_of_first_scalar(bytes);
        if offset < width {
            assert(offset == 0);
            reveal_with_fuel(is_char_boundary, 2);
        } else {
            let tail = pop_first_scalar(bytes);
            ascii_boundaries(tail, offset - width);
        }
    }

    proof fn scalar_boundary(bytes: Seq<u8>, start: int)
        requires valid_utf8(bytes), 0 <= start < bytes.len(), is_char_boundary(bytes, start),
        ensures valid_first_scalar(bytes[start..]),
            0 < length_of_first_scalar(bytes[start..]) <= bytes.len() - start,
            is_char_boundary(bytes, start + length_of_first_scalar(bytes[start..])),
        decreases bytes.len(),
    {
        if start == 0 {
            assert(bytes[start..] =~= bytes);
            reveal_with_fuel(is_char_boundary, 2);
        } else {
            let width = length_of_first_scalar(bytes);
            reveal_with_fuel(is_char_boundary, 2);
            assert(start >= width);
            let tail = pop_first_scalar(bytes);
            scalar_boundary(tail, start - width);
            assert(tail[start - width..] =~= bytes[start..]);
        }
    }

    proof fn lexeme_boundaries(bytes: Seq<u8>, start: int, end: int, kind: LexemeKind)
        requires valid_utf8(bytes),
            crate::lexical_core::lexeme_matches(bytes, start, end, kind),
            kind != LexemeKind::InvalidByte,
        ensures is_char_boundary(bytes, end),
            kind == LexemeKind::Hex || kind == LexemeKind::InvalidHex ==>
                is_char_boundary(bytes, start + 2),
    {
        if kind == LexemeKind::Hex || kind == LexemeKind::InvalidHex {
            assert(bytes[start + 1] == b'x');
            ascii_boundaries(bytes, start + 1);
        }
        if end == bytes.len() {
            is_char_boundary_start_end_of_seq(bytes);
        } else if kind == LexemeKind::Comment {
            assert(bytes[end] == b'\n');
            ascii_boundaries(bytes, end);
        } else {
            assert(0 < end);
            assert(bytes[end - 1] < 128);
            ascii_boundaries(bytes, end - 1);
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.1@.len() == 0 ==> {
        &&& result.0@.len() > 0
        &&& result.0@.last().kind == TokenKind::End
        &&& result.0@.last().spelling.spec_bytes().len() == 0
        &&& result.0@.len() - 1 <= limits.fields().1
        &&& exists|line: u32, column: u32|
            crate::source_token::source_prefix(source.spec_bytes(), source.spec_bytes().len() as int,
                result.0@.drop_last(), line, column)
        &&& forall|i: int| 0 <= i < result.0@.len() - 1 ==>
            crate::lexical_core::token_kind(result.0@[i].kind)
    },
))]
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
    #[cfg(verus_keep_ghost)]
    proof! {
        vstd::utf8::encode_utf8_valid_utf8(source@);
        vstd::utf8::is_char_boundary_start_end_of_seq(bytes@);
    }
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let (mut index, mut line, mut column) = (0usize, 1u32, 1u32);
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bytes.len(), bytes@ == source.spec_bytes(),
            vstd::utf8::valid_utf8(bytes@),
            vstd::utf8::is_char_boundary(bytes@, bytes@.len() as int),
            vstd::utf8::is_char_boundary(source.spec_bytes(), index as int),
        ensures diagnostics@.len() == 0 ==> index == bytes.len(),
        decreases bytes.len() - index,
    ))]
    while index < bytes.len() {
        let item = next_lexeme(bytes, index);
        #[cfg(verus_keep_ghost)]
        proof! {
            if item.kind != LexemeKind::InvalidByte {
                lexeme_boundaries(bytes@, index as int, item.end as int, item.kind);
            }
        }
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
        index = if matches!(item.kind, LexemeKind::InvalidByte) {
            let suffix = source.split_at(start).1;
            #[cfg(verus_keep_ghost)]
            proof! {
                scalar_boundary(bytes@, start as int);
                assert(suffix.spec_bytes() == bytes@.subrange(start as int, bytes@.len() as int));
                if suffix@.len() == 0 {
                    reveal(vstd::utf8::encode_utf8);
                }
                assert(suffix@.len() > 0);
                vstd::utf8::encode_utf8_first_scalar(suffix@);
            }
            start
                + match suffix.chars().next() {
                    Some(character) => {
                        #[cfg(verus_keep_ghost)]
                        proof! { assert(character == suffix@[0]); }
                        character.len_utf8()
                    }
                    None => 1,
                }
        } else {
            item.end
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(start < index <= bytes.len());
            assert(vstd::utf8::is_char_boundary(bytes@, index as int));
            if item.kind == LexemeKind::Hex || item.kind == LexemeKind::InvalidHex {
                assert(start + 2 <= index);
                assert(vstd::utf8::is_char_boundary(bytes@, start as int + 2));
            }
        }
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
    let checked = finish_tokens(
        source,
        &mut tokens,
        limits.max_tokens(),
        diagnostics.is_empty(),
        span(index, index, line, column),
    );
    if diagnostics.is_empty() && !checked {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDeclaration,
            span(index, index, line, column),
            "exact complete source tokenization",
            "token stream failed independent validation",
            "report this parser validation failure",
        ));
    }
    (tokens, diagnostics)
}

fn span(start: usize, end: usize, line: u32, column: u32) -> SourceSpan {
    SourceSpan::new(u32_len(start), u32_len(end), line, column)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == crate::source_token::saturated(0, value as nat),
))]
fn u32_len(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
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
    use crate::source_token::tokens_match_source;

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
