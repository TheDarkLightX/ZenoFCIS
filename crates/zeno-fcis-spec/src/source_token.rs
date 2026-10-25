//! Actual borrowed source tokens and their complete source correspondence check.
//!
//! The checked view includes token kinds and numeric payloads, start/end bytes,
//! line/byte-column coordinates, and exact identifier/hex spelling. Validation
//! is allocation-free and keeps only invocation-local scalar cursors.

use crate::lexical_core::{
    Lexeme, LexemeKind, check_spelling, is_token, is_trivia, next_lexeme, same_kind,
};
use crate::source_span::SourceSpan;

#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, string::StringSliceAdditionalSpecFns};

#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub(crate) struct Token<'a> {
    pub(crate) kind: LexemeKind,
    pub(crate) span: SourceSpan,
    pub(crate) spelling: &'a str,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub(crate) open spec fn saturated(value: u32, amount: nat) -> u32 {
        if value as nat + amount >= u32::MAX { u32::MAX }
        else { (value as nat + amount) as u32 }
    }

    pub(crate) open spec fn next_line(kind: LexemeKind, line: u32) -> u32 {
        if kind == LexemeKind::Newline { saturated(line, 1) } else { line }
    }

    pub(crate) open spec fn next_column(kind: LexemeKind, column: u32, width: nat) -> u32 {
        if kind == LexemeKind::Newline { 1 } else { saturated(column, width) }
    }

    pub(crate) open spec fn token_fields_match(
        bytes: Seq<u8>, token: Token<'_>, start: int, end: int,
        kind: LexemeKind, line: u32, column: u32,
    ) -> bool {
        0 <= start <= end <= bytes.len()
        && token.kind == kind
        && token.span.fields().0 as int == start
        && token.span.fields().1 as int == end
        && token.span.fields().2 == line
        && token.span.fields().3 == column
        && if kind == LexemeKind::Ident {
            token.spelling.spec_bytes() == bytes.subrange(start, end)
        } else if kind == LexemeKind::Hex {
            start + 2 <= end
            && token.spelling.spec_bytes() == bytes.subrange(start + 2, end)
        } else {
            token.spelling.spec_bytes().len() == 0
        }
    }

    pub(crate) open spec fn source_step(
        bytes: Seq<u8>, start: int, end: int, kind: LexemeKind,
        before_line: u32, before_column: u32, line: u32, column: u32,
    ) -> bool {
        0 <= start < end <= bytes.len()
        && crate::lexical_core::lexeme_matches(bytes, start, end, kind)
        && line == next_line(kind, before_line)
        && column == next_column(kind, before_column, (end - start) as nat)
    }

    /// Complete inductive source frontier over the real Token representation.
    /// Each nontrivia lexeme contributes exactly one complete token, and each
    /// step preserves the same source cursor and display-coordinate state.
    pub(crate) open spec fn source_prefix(
        bytes: Seq<u8>, end: int, tokens: Seq<Token<'_>>, line: u32, column: u32,
    ) -> bool
        decreases end,
    {
        if end == 0 { tokens.len() == 0 && line == 1 && column == 1 }
        else if end < 0 || end > bytes.len() { false }
        else {
            exists|previous: int, kind: LexemeKind, before_line: u32, before_column: u32|
                #[trigger] source_step(bytes, previous, end, kind, before_line, before_column, line, column)
                && ((crate::lexical_core::trivia_kind(kind)
                        && source_prefix(bytes, previous, tokens, before_line, before_column))
                    || (crate::lexical_core::token_kind(kind) && tokens.len() > 0
                        && token_fields_match(bytes, tokens.last(), previous, end,
                                              kind, before_line, before_column)
                        && source_prefix(bytes, previous, tokens.drop_last(), before_line, before_column)))
        }
    }

    pub(crate) proof fn extend_source_trivia(
        bytes: Seq<u8>, start: int, end: int, kind: LexemeKind,
        tokens: Seq<Token<'_>>, line: u32, column: u32,
    )
        requires 0 <= start < end <= bytes.len(),
                 source_prefix(bytes, start, tokens, line, column),
                 crate::lexical_core::lexeme_matches(bytes, start, end, kind),
                 crate::lexical_core::trivia_kind(kind),
        ensures source_prefix(bytes, end, tokens, next_line(kind, line),
                               next_column(kind, column, (end - start) as nat)),
    {
        assert(source_step(bytes, start, end, kind, line, column,
                           next_line(kind, line), next_column(kind, column, (end - start) as nat)));
    }

    pub(crate) proof fn extend_source_token(
        bytes: Seq<u8>, start: int, end: int, kind: LexemeKind,
        tokens: Seq<Token<'_>>, token: Token<'_>, line: u32, column: u32,
    )
        requires 0 <= start < end <= bytes.len(),
                 source_prefix(bytes, start, tokens, line, column),
                 crate::lexical_core::lexeme_matches(bytes, start, end, kind),
                 crate::lexical_core::token_kind(kind),
                 token_fields_match(bytes, token, start, end, kind, line, column),
        ensures source_prefix(bytes, end, tokens.push(token), next_line(kind, line),
                               next_column(kind, column, (end - start) as nat)),
    {
        assert(tokens.push(token).drop_last() =~= tokens);
        assert(tokens.push(token).last() == token);
        assert(source_step(bytes, start, end, kind, line, column,
                           next_line(kind, line), next_column(kind, column, (end - start) as nat)));
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == saturated(value, amount as nat),
))]
pub(crate) fn advance_coordinate(value: u32, amount: usize) -> u32 {
    if amount as u128 >= (u32::MAX - value) as u128 {
        u32::MAX
    } else {
        value + amount as u32
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == token_fields_match(bytes@, *token, found.start as int,
                                         found.end as int, found.kind, line, column),
))]
fn check_token_fields(
    bytes: &[u8],
    token: &Token<'_>,
    found: Lexeme,
    line: u32,
    column: u32,
) -> bool {
    if found.start > found.end
        || found.end > bytes.len()
        || !same_kind(found.kind, token.kind)
        || token.span.start() as u128 != found.start as u128
        || token.span.end() as u128 != found.end as u128
        || token.span.line() != line
        || token.span.column() != column
    {
        return false;
    }
    let spelling = token.spelling.as_bytes();
    match found.kind {
        LexemeKind::Ident => check_spelling(bytes, found.start, found.end, spelling),
        LexemeKind::Hex => {
            if found.end - found.start < 2 {
                return false;
            }
            check_spelling(bytes, found.start + 2, found.end, spelling)
        }
        _ => spelling.is_empty(),
    }
}

/// Validates the complete actual borrowed token stream against immutable source.
/// EOF is lookahead and must be excluded; trivia consumes no token budget.
/// The scanner and this checker share a proved lexical relation, while the
/// candidate tokens' real fields are independently compared against the source.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result ==> tokens.len() <= max_tokens
        && exists|line: u32, column: u32|
            source_prefix(source.spec_bytes(), source.spec_bytes().len() as int,
                          tokens@, line, column),
))]
pub(crate) fn tokens_match_source(source: &str, tokens: &[Token<'_>], max_tokens: usize) -> bool {
    if tokens.len() > max_tokens {
        return false;
    }
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    let mut used = 0usize;
    let mut line = 1u32;
    let mut column = 1u32;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant cursor <= bytes.len(), used <= tokens.len(), tokens.len() <= max_tokens,
            bytes@ == source.spec_bytes(),
            source_prefix(bytes@, cursor as int, tokens@.take(used as int), line, column),
        decreases bytes.len() - cursor,
    ))]
    while cursor < bytes.len() {
        let found = next_lexeme(bytes, cursor);
        let newline = matches!(found.kind, LexemeKind::Newline);
        let following_line = if newline {
            advance_coordinate(line, 1)
        } else {
            line
        };
        let following_column = if newline {
            1
        } else {
            advance_coordinate(column, found.end - cursor)
        };
        if is_trivia(found.kind) {
            #[cfg(verus_keep_ghost)]
            proof! {
                extend_source_trivia(bytes@, cursor as int, found.end as int, found.kind,
                                     tokens@.take(used as int), line, column);
            }
        } else {
            if !is_token(found.kind) || used >= tokens.len() {
                return false;
            }
            if !check_token_fields(bytes, &tokens[used], found, line, column) {
                return false;
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(tokens@.take(used as int).push(tokens@[used as int]) =~= tokens@.take(used as int + 1));
                extend_source_token(bytes@, cursor as int, found.end as int, found.kind,
                                    tokens@.take(used as int), tokens@[used as int], line, column);
            }
            used += 1;
        }
        cursor = found.end;
        line = following_line;
        column = following_column;
    }
    if used != tokens.len() {
        return false;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(tokens@.take(used as int) =~= tokens@);
        assert(exists|final_line: u32, final_column: u32|
            source_prefix(source.spec_bytes(), source.spec_bytes().len() as int,
                          tokens@, final_line, final_column));
    }
    true
}
