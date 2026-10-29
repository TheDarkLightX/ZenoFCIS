//! Pure byte-oriented lexical stepping shared by runtime and Verus.
//!
//! The input and cursor are immutable. Every non-EOF result makes progress.
//! Source spans, UTF-8 diagnostic grouping, allocation and token budgets belong
//! to the caller. ASCII tokens and comments are recognized here exactly.

// Keep the first-order comparisons identical in native code and the proved
// arithmetic model, without introducing iterator/range trait calls.
#![allow(clippy::manual_range_contains)]

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LexemeKind {
    End,
    Space,
    Newline,
    Comment,
    Ident,
    Number(u128),
    Hex,
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
    InvalidNumber,
    InvalidHex,
    InvalidByte,
}

#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Lexeme {
    pub(crate) kind: LexemeKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy)]
pub(crate) enum Class {
    Space,
    IdentContinue,
    Digit,
    Hex,
    LowerHex,
    Comment,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub(crate) open spec fn ident_start(byte: u8) -> bool {
        (b'a' <= byte <= b'z') || (b'A' <= byte <= b'Z') || byte == b'_'
    }
    pub(crate) open spec fn member(byte: u8, class: Class) -> bool {
        match class {
            Class::Space => byte == b' ' || byte == b'\t' || byte == b'\r',
            Class::IdentContinue => ident_start(byte) || (b'0' <= byte <= b'9') || byte == b'-',
            Class::Digit => b'0' <= byte <= b'9',
            Class::Hex => (b'0' <= byte <= b'9') || (b'a' <= byte <= b'f') || (b'A' <= byte <= b'F'),
            Class::LowerHex => (b'0' <= byte <= b'9') || (b'a' <= byte <= b'f'),
            Class::Comment => byte != b'\n',
        }
    }
    pub(crate) open spec fn all_members(bytes: Seq<u8>, start: int, end: int, class: Class) -> bool {
        0 <= start <= end <= bytes.len()
        && forall|i: int| start <= i < end ==> member(bytes[i], class)
    }
    pub(crate) open spec fn maximal(bytes: Seq<u8>, start: int, end: int, class: Class) -> bool {
        all_members(bytes, start, end, class)
        && (end == bytes.len() || !member(bytes[end], class))
    }
    /// Mathematical decimal magnitude, with no machine-width truncation.
    pub(crate) open spec fn decimal_value(bytes: Seq<u8>, start: int, end: int) -> int
        decreases end - start,
    {
        if end <= start || start < 0 || end > bytes.len() { 0 }
        else { decimal_value(bytes, start, end - 1) * 10 + bytes[end - 1] as int - b'0' as int }
    }
    pub(crate) open spec fn second(bytes: Seq<u8>, start: int) -> Option<u8> {
        if 0 <= start && start + 1 < bytes.len() { Some(bytes[start + 1]) } else { None }
    }
    pub(crate) open spec fn hex_prefix(bytes: Seq<u8>, start: int) -> bool {
        0 <= start && start + 1 < bytes.len() && bytes[start] == b'0' && bytes[start + 1] == b'x'
    }
    pub(crate) open spec fn symbol(byte: u8, next: Option<u8>) -> (LexemeKind, int) {
        match (byte, next) {
            (b'.', Some(b'.')) => (LexemeKind::Range, 2),
            (b'=', Some(b'=')) => (LexemeKind::EqEq, 2),
            (b'!', Some(b'=')) => (LexemeKind::NotEq, 2),
            (b'<', Some(b'=')) => (LexemeKind::LessEq, 2),
            (b'>', Some(b'=')) => (LexemeKind::GreaterEq, 2),
            (b'&', Some(b'&')) => (LexemeKind::AndAnd, 2),
            (b'|', Some(b'|')) => (LexemeKind::OrOr, 2),
            (b'-', Some(b'>')) => (LexemeKind::Arrow, 2),
            (b'{', _) => (LexemeKind::LBrace, 1),
            (b'}', _) => (LexemeKind::RBrace, 1),
            (b'(', _) => (LexemeKind::LParen, 1),
            (b')', _) => (LexemeKind::RParen, 1),
            (b'[', _) => (LexemeKind::LBracket, 1),
            (b']', _) => (LexemeKind::RBracket, 1),
            (b';', _) => (LexemeKind::Semicolon, 1),
            (b',', _) => (LexemeKind::Comma, 1),
            (b'.', _) => (LexemeKind::Dot, 1),
            (b':', _) => (LexemeKind::Colon, 1),
            (b'=', _) => (LexemeKind::Equal, 1),
            (b'<', _) => (LexemeKind::Less, 1),
            (b'>', _) => (LexemeKind::Greater, 1),
            (b'+', _) => (LexemeKind::Plus, 1),
            (b'-', _) => (LexemeKind::Minus, 1),
            (b'*', _) => (LexemeKind::Star, 1),
            (b'/', _) => (LexemeKind::Slash, 1),
            (b'!', _) => (LexemeKind::Bang, 1),
            _ => (LexemeKind::InvalidByte, 1),
        }
    }
    /// Ordered lexical rules, including maximal munch and rejected magnitudes.
    pub(crate) open spec fn lexeme_matches(bytes: Seq<u8>, start: int, end: int, kind: LexemeKind) -> bool {
        0 <= start <= end <= bytes.len()
        && if start == bytes.len() { end == start && kind == LexemeKind::End }
        else if member(bytes[start], Class::Space) {
            kind == LexemeKind::Space && start < end && maximal(bytes, start, end, Class::Space)
        } else if bytes[start] == b'\n' {
            kind == LexemeKind::Newline && end == start + 1
        } else if bytes[start] == b'/' && second(bytes, start) == Some(b'/') {
            kind == LexemeKind::Comment && maximal(bytes, start + 2, end, Class::Comment)
        } else if ident_start(bytes[start]) {
            kind == LexemeKind::Ident && start < end && maximal(bytes, start, end, Class::IdentContinue)
        } else if hex_prefix(bytes, start) {
            maximal(bytes, start + 2, end, Class::Hex)
            && if end - start == 66 && all_members(bytes, start + 2, end, Class::LowerHex) {
                kind == LexemeKind::Hex
            } else { kind == LexemeKind::InvalidHex }
        } else if member(bytes[start], Class::Digit) {
            start < end && maximal(bytes, start, end, Class::Digit)
            && match kind {
                LexemeKind::Number(value) => value as int == decimal_value(bytes, start, end),
                LexemeKind::InvalidNumber => decimal_value(bytes, start, end) > u128::MAX,
                _ => false,
            }
        } else {
            kind == symbol(bytes[start], second(bytes, start)).0
            && end == start + symbol(bytes[start], second(bytes, start)).1
        }
    }

    pub(crate) proof fn maximal_unique(bytes: Seq<u8>, start: int, left: int, right: int, class: Class)
        requires maximal(bytes, start, left, class), maximal(bytes, start, right, class),
        ensures left == right,
    {
        if left < right {
            assert(member(bytes[left], class));
            assert(!member(bytes[left], class));
        } else if right < left {
            assert(member(bytes[right], class));
            assert(!member(bytes[right], class));
        }
    }

    /// The lexical relation admits one kind and ending cursor for fixed bytes.
    pub(crate) proof fn lexeme_unique(bytes: Seq<u8>, start: int, left: int, left_kind: LexemeKind,
                                     right: int, right_kind: LexemeKind)
        requires lexeme_matches(bytes, start, left, left_kind),
                 lexeme_matches(bytes, start, right, right_kind),
        ensures left == right, left_kind == right_kind,
    {
        if start == bytes.len() {
        } else if member(bytes[start], Class::Space) {
            maximal_unique(bytes, start, left, right, Class::Space);
        } else if bytes[start] == b'\n' {
        } else if bytes[start] == b'/' && second(bytes, start) == Some(b'/') {
            maximal_unique(bytes, start + 2, left, right, Class::Comment);
        } else if ident_start(bytes[start]) {
            maximal_unique(bytes, start, left, right, Class::IdentContinue);
        } else if hex_prefix(bytes, start) {
            maximal_unique(bytes, start + 2, left, right, Class::Hex);
        } else if member(bytes[start], Class::Digit) {
            maximal_unique(bytes, start, left, right, Class::Digit);
        }
    }

    pub(crate) open spec fn trivia_kind(kind: LexemeKind) -> bool {
        kind == LexemeKind::Space || kind == LexemeKind::Newline || kind == LexemeKind::Comment
    }

    pub(crate) open spec fn token_kind(kind: LexemeKind) -> bool {
        !trivia_kind(kind) && kind != LexemeKind::End && kind != LexemeKind::InvalidNumber
        && kind != LexemeKind::InvalidHex && kind != LexemeKind::InvalidByte
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == ident_start(byte),))]
fn is_ident_start(byte: u8) -> bool {
    (byte >= b'a' && byte <= b'z') || (byte >= b'A' && byte <= b'Z') || byte == b'_'
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == member(byte, class),))]
fn is_member(byte: u8, class: Class) -> bool {
    match class {
        Class::Space => byte == b' ' || byte == b'\t' || byte == b'\r',
        Class::IdentContinue => {
            is_ident_start(byte) || (byte >= b'0' && byte <= b'9') || byte == b'-'
        }
        Class::Digit => byte >= b'0' && byte <= b'9',
        Class::Hex => {
            (byte >= b'0' && byte <= b'9')
                || (byte >= b'a' && byte <= b'f')
                || (byte >= b'A' && byte <= b'F')
        }
        Class::LowerHex => (byte >= b'0' && byte <= b'9') || (byte >= b'a' && byte <= b'f'),
        Class::Comment => byte != b'\n',
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires start <= bytes.len(),
    ensures start <= result <= bytes.len(), maximal(bytes@, start as int, result as int, class),
))]
fn scan(bytes: &[u8], start: usize, class: Class) -> usize {
    let mut end = start;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant start <= end <= bytes.len(), all_members(bytes@, start as int, end as int, class),
        decreases bytes.len() - end,
    ))]
    while end < bytes.len() && is_member(bytes[end], class) {
        end += 1;
    }
    end
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires start <= end <= bytes.len(),
    ensures result == all_members(bytes@, start as int, end as int, Class::LowerHex),
))]
fn all_lower_hex(bytes: &[u8], start: usize, end: usize) -> bool {
    let mut cursor = start;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant start <= cursor <= end <= bytes.len(),
                  all_members(bytes@, start as int, cursor as int, Class::LowerHex),
        decreases end - cursor,
    ))]
    while cursor < end {
        if !is_member(bytes[cursor], Class::LowerHex) {
            return false;
        }
        cursor += 1;
    }
    true
}

/// Exact checked base-ten accumulation; overflow is a refusal, never wraparound.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires digit <= 9,
    ensures match result {
        Some(value) => value as int == prefix as int * 10 + digit as int,
        None => prefix as int * 10 + digit as int > u128::MAX,
    },
))]
pub(crate) fn decimal_push(prefix: u128, digit: u8) -> Option<u128> {
    if prefix > u128::MAX / 10 || (prefix == u128::MAX / 10 && digit as u128 > u128::MAX % 10) {
        None
    } else {
        Some(prefix * 10 + digit as u128)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires start < bytes.len(), member(bytes@[start as int], Class::Digit),
    ensures start < result.0 <= bytes.len(),
            maximal(bytes@, start as int, result.0 as int, Class::Digit),
            match result.1 {
                Some(value) => value as int == decimal_value(bytes@, start as int, result.0 as int),
                None => decimal_value(bytes@, start as int, result.0 as int) > u128::MAX,
            },
))]
fn scan_decimal(bytes: &[u8], start: usize) -> (usize, Option<u128>) {
    let mut end = start;
    let mut value = Some(0u128);
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant start <= end <= bytes.len(), start < bytes.len(), member(bytes@[start as int], Class::Digit),
                  all_members(bytes@, start as int, end as int, Class::Digit),
                  match value {
                      Some(v) => v as int == decimal_value(bytes@, start as int, end as int),
                      None => decimal_value(bytes@, start as int, end as int) > u128::MAX,
                  },
        decreases bytes.len() - end,
    ))]
    while end < bytes.len() && is_member(bytes[end], Class::Digit) {
        let digit = bytes[end] - b'0';
        value = match value {
            Some(prefix) => decimal_push(prefix, digit),
            None => None,
        };
        end += 1;
    }
    (end, value)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.0 == symbol(byte, next).0, result.1 as int == symbol(byte, next).1,
            1 <= result.1 <= 2, result.1 == 2 ==> next.is_some(),
))]
fn read_symbol(byte: u8, next: Option<u8>) -> (LexemeKind, usize) {
    match (byte, next) {
        (b'.', Some(b'.')) => (LexemeKind::Range, 2),
        (b'=', Some(b'=')) => (LexemeKind::EqEq, 2),
        (b'!', Some(b'=')) => (LexemeKind::NotEq, 2),
        (b'<', Some(b'=')) => (LexemeKind::LessEq, 2),
        (b'>', Some(b'=')) => (LexemeKind::GreaterEq, 2),
        (b'&', Some(b'&')) => (LexemeKind::AndAnd, 2),
        (b'|', Some(b'|')) => (LexemeKind::OrOr, 2),
        (b'-', Some(b'>')) => (LexemeKind::Arrow, 2),
        (b'{', _) => (LexemeKind::LBrace, 1),
        (b'}', _) => (LexemeKind::RBrace, 1),
        (b'(', _) => (LexemeKind::LParen, 1),
        (b')', _) => (LexemeKind::RParen, 1),
        (b'[', _) => (LexemeKind::LBracket, 1),
        (b']', _) => (LexemeKind::RBracket, 1),
        (b';', _) => (LexemeKind::Semicolon, 1),
        (b',', _) => (LexemeKind::Comma, 1),
        (b'.', _) => (LexemeKind::Dot, 1),
        (b':', _) => (LexemeKind::Colon, 1),
        (b'=', _) => (LexemeKind::Equal, 1),
        (b'<', _) => (LexemeKind::Less, 1),
        (b'>', _) => (LexemeKind::Greater, 1),
        (b'+', _) => (LexemeKind::Plus, 1),
        (b'-', _) => (LexemeKind::Minus, 1),
        (b'*', _) => (LexemeKind::Star, 1),
        (b'/', _) => (LexemeKind::Slash, 1),
        (b'!', _) => (LexemeKind::Bang, 1),
        _ => (LexemeKind::InvalidByte, 1),
    }
}

/// Return one maximal token or trivia group. An out-of-range cursor is clamped
/// to EOF; all other results satisfy `start < end <= bytes.len()`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.start as int == if start <= bytes.len() { start as int } else { bytes.len() as int },
            result.start <= result.end <= bytes.len(),
            result.start < bytes.len() ==> result.start < result.end,
            lexeme_matches(bytes@, result.start as int, result.end as int, result.kind),
))]
pub(crate) fn next_lexeme(bytes: &[u8], start: usize) -> Lexeme {
    if start >= bytes.len() {
        return Lexeme {
            kind: LexemeKind::End,
            start: bytes.len(),
            end: bytes.len(),
        };
    }
    let byte = bytes[start];
    let next = if start + 1 < bytes.len() {
        Some(bytes[start + 1])
    } else {
        None
    };
    if is_member(byte, Class::Space) {
        let end = scan(bytes, start, Class::Space);
        return Lexeme {
            kind: LexemeKind::Space,
            start,
            end,
        };
    }
    if byte == b'\n' {
        return Lexeme {
            kind: LexemeKind::Newline,
            start,
            end: start + 1,
        };
    }
    if byte == b'/' && next == Some(b'/') {
        let end = scan(bytes, start + 2, Class::Comment);
        return Lexeme {
            kind: LexemeKind::Comment,
            start,
            end,
        };
    }
    if is_ident_start(byte) {
        let end = scan(bytes, start, Class::IdentContinue);
        return Lexeme {
            kind: LexemeKind::Ident,
            start,
            end,
        };
    }
    if byte == b'0' && next == Some(b'x') {
        let end = scan(bytes, start + 2, Class::Hex);
        let valid = end - start == 66 && all_lower_hex(bytes, start + 2, end);
        let kind = if valid {
            LexemeKind::Hex
        } else {
            LexemeKind::InvalidHex
        };
        return Lexeme { kind, start, end };
    }
    if is_member(byte, Class::Digit) {
        let (end, value) = scan_decimal(bytes, start);
        let kind = match value {
            Some(n) => LexemeKind::Number(n),
            None => LexemeKind::InvalidNumber,
        };
        return Lexeme { kind, start, end };
    }
    let (kind, width) = read_symbol(byte, next);
    Lexeme {
        kind,
        start,
        end: start + width,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == trivia_kind(kind),))]
pub(crate) fn is_trivia(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Space | LexemeKind::Newline | LexemeKind::Comment
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == token_kind(kind),))]
pub(crate) fn is_token(kind: LexemeKind) -> bool {
    !matches!(
        kind,
        LexemeKind::End
            | LexemeKind::Space
            | LexemeKind::Newline
            | LexemeKind::Comment
            | LexemeKind::InvalidNumber
            | LexemeKind::InvalidHex
            | LexemeKind::InvalidByte
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left == right),))]
pub(crate) fn same_kind(left: LexemeKind, right: LexemeKind) -> bool {
    match (left, right) {
        (LexemeKind::Number(a), LexemeKind::Number(b)) => a == b,
        (LexemeKind::End, LexemeKind::End)
        | (LexemeKind::Space, LexemeKind::Space)
        | (LexemeKind::Newline, LexemeKind::Newline)
        | (LexemeKind::Comment, LexemeKind::Comment)
        | (LexemeKind::Ident, LexemeKind::Ident)
        | (LexemeKind::Hex, LexemeKind::Hex)
        | (LexemeKind::LBrace, LexemeKind::LBrace)
        | (LexemeKind::RBrace, LexemeKind::RBrace)
        | (LexemeKind::LParen, LexemeKind::LParen)
        | (LexemeKind::RParen, LexemeKind::RParen)
        | (LexemeKind::LBracket, LexemeKind::LBracket)
        | (LexemeKind::RBracket, LexemeKind::RBracket)
        | (LexemeKind::Semicolon, LexemeKind::Semicolon)
        | (LexemeKind::Comma, LexemeKind::Comma)
        | (LexemeKind::Dot, LexemeKind::Dot)
        | (LexemeKind::Range, LexemeKind::Range)
        | (LexemeKind::Colon, LexemeKind::Colon)
        | (LexemeKind::Equal, LexemeKind::Equal)
        | (LexemeKind::EqEq, LexemeKind::EqEq)
        | (LexemeKind::NotEq, LexemeKind::NotEq)
        | (LexemeKind::Less, LexemeKind::Less)
        | (LexemeKind::LessEq, LexemeKind::LessEq)
        | (LexemeKind::Greater, LexemeKind::Greater)
        | (LexemeKind::GreaterEq, LexemeKind::GreaterEq)
        | (LexemeKind::Plus, LexemeKind::Plus)
        | (LexemeKind::Minus, LexemeKind::Minus)
        | (LexemeKind::Star, LexemeKind::Star)
        | (LexemeKind::Slash, LexemeKind::Slash)
        | (LexemeKind::AndAnd, LexemeKind::AndAnd)
        | (LexemeKind::OrOr, LexemeKind::OrOr)
        | (LexemeKind::Bang, LexemeKind::Bang)
        | (LexemeKind::Arrow, LexemeKind::Arrow)
        | (LexemeKind::InvalidNumber, LexemeKind::InvalidNumber)
        | (LexemeKind::InvalidHex, LexemeKind::InvalidHex)
        | (LexemeKind::InvalidByte, LexemeKind::InvalidByte) => true,
        _ => false,
    }
}

/// Check an actual identifier/hex payload against its exact original byte span.
/// Bounds and length are checked before indexing; no allocation is performed.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (start <= end <= bytes.len()
        && spelling@ == bytes@.subrange(start as int, end as int)),
))]
pub(crate) fn check_spelling(bytes: &[u8], start: usize, end: usize, spelling: &[u8]) -> bool {
    if start > end || end > bytes.len() {
        return false;
    }
    if spelling.len() != end - start {
        return false;
    }
    let mut cursor = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant start <= end <= bytes.len(), spelling.len() == end - start,
                  cursor <= spelling.len(),
                  forall|i: int| 0 <= i < cursor ==> bytes@[start as int + i] == spelling@[i],
        decreases spelling.len() - cursor,
    ))]
    while cursor < spelling.len() {
        if bytes[start + cursor] != spelling[cursor] {
            return false;
        }
        cursor += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(spelling@ =~= bytes@.subrange(start as int, end as int)); }
    true
}
