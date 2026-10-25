use crate::lexical_core::LexemeKind as K;
use crate::source_span::SourceSpan;
use crate::source_token::{Token, advance_coordinate, tokens_match_source};

fn token(kind: K, start: u32, end: u32, line: u32, column: u32, spelling: &str) -> Token<'_> {
    Token {
        kind,
        span: SourceSpan::new(start, end, line, column),
        spelling,
    }
}

#[test]
fn public_span_keeps_const_construction_and_accessors() {
    const SPAN: SourceSpan = SourceSpan::new(3, 7, 2, 4);
    const FIELDS: (u32, u32, u32, u32) = (SPAN.start(), SPAN.end(), SPAN.line(), SPAN.column());
    assert_eq!(FIELDS, (3, 7, 2, 4));
    assert_eq!(SPAN, SPAN.clone());
}

#[test]
fn empty_and_all_trivia_sources_accept_zero_budget() {
    for source in ["", " \t\r", "// note", "// 雪\n \t// next\n"] {
        assert!(tokens_match_source(source, &[], 0), "{source:?}");
    }
}

#[test]
fn exact_numeric_payload_and_multichar_symbols_accept() {
    let source = "name 340282366920938463463374607431768211455 != 0";
    let tokens = [
        token(K::Ident, 0, 4, 1, 1, "name"),
        token(K::Number(u128::MAX), 5, 44, 1, 6, ""),
        token(K::NotEq, 45, 47, 1, 46, ""),
        token(K::Number(0), 48, 49, 1, 49, ""),
    ];
    assert!(tokens_match_source(source, &tokens, 4));
}

#[test]
fn comments_utf8_trivia_and_byte_columns_accept() {
    let source = "// 雪\nx\r\n\t+ 7";
    let tokens = [
        token(K::Ident, 7, 8, 2, 1, "x"),
        token(K::Plus, 11, 12, 3, 2, ""),
        token(K::Number(7), 13, 14, 3, 4, ""),
    ];
    assert!(tokens_match_source(source, &tokens, 3));
}

#[test]
fn canonical_hex_borrows_only_the_payload() {
    let source = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let tokens = [token(K::Hex, 0, 66, 1, 1, &source[2..])];
    assert!(tokens_match_source(source, &tokens, 1));
    let prefixed = [token(K::Hex, 0, 66, 1, 1, source)];
    assert!(!tokens_match_source(source, &prefixed, 1));
}

#[test]
fn wrong_kind_and_numeric_payload_refuse() {
    assert!(!tokens_match_source(
        "+",
        &[token(K::Minus, 0, 1, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        "7",
        &[token(K::Number(8), 0, 1, 1, 1, "")],
        1
    ));
}

#[test]
fn wrong_start_end_and_unrepresentable_spans_refuse() {
    for span in [
        SourceSpan::new(1, 1, 1, 1),
        SourceSpan::new(0, 0, 1, 1),
        SourceSpan::new(0, 2, 1, 1),
        SourceSpan::new(u32::MAX, u32::MAX, 1, 1),
    ] {
        let tokens = [Token {
            kind: K::Plus,
            span,
            spelling: "",
        }];
        assert!(!tokens_match_source("+", &tokens, 1), "{span:?}");
    }
}

#[test]
fn wrong_line_or_byte_column_refuses() {
    assert!(!tokens_match_source(
        "\n+",
        &[token(K::Plus, 1, 2, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        " +",
        &[token(K::Plus, 1, 2, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        "+",
        &[token(K::Plus, 0, 1, 0, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        "+",
        &[token(K::Plus, 0, 1, 1, 0, "")],
        1
    ));
}

#[test]
fn identifier_spelling_must_equal_source_bytes() {
    for spelling in ["", "other", "Name", "name ", "nam", "雪"] {
        assert!(!tokens_match_source(
            "name",
            &[token(K::Ident, 0, 4, 1, 1, spelling)],
            1
        ));
    }
}

#[test]
fn hex_payload_mutation_refuses() {
    let source = "0x0000000000000000000000000000000000000000000000000000000000000000";
    for spelling in [
        "",
        "0",
        "1000000000000000000000000000000000000000000000000000000000000000",
    ] {
        assert!(!tokens_match_source(
            source,
            &[token(K::Hex, 0, 66, 1, 1, spelling)],
            1
        ));
    }
}

#[test]
fn inert_payloads_must_be_empty() {
    assert!(!tokens_match_source(
        "7",
        &[token(K::Number(7), 0, 1, 1, 1, "7")],
        1
    ));
    assert!(!tokens_match_source(
        "+",
        &[token(K::Plus, 0, 1, 1, 1, "+")],
        1
    ));
}

#[test]
fn token_budget_is_exact_and_excludes_trivia() {
    let tokens = [token(K::Plus, 2, 3, 2, 1, "")];
    assert!(tokens_match_source(" \n+ //tail", &tokens, 1));
    assert!(!tokens_match_source(" \n+ //tail", &tokens, 0));
}

#[test]
fn missing_surplus_and_reordered_tokens_refuse() {
    let plus = token(K::Plus, 0, 1, 1, 1, "");
    let minus = token(K::Minus, 2, 3, 1, 3, "");
    assert!(!tokens_match_source("+ -", &[plus.clone()], 2));
    assert!(!tokens_match_source("+", &[plus.clone(), minus.clone()], 2));
    assert!(!tokens_match_source("+ -", &[minus, plus], 2));
}

#[test]
fn eof_and_trivia_are_not_candidate_tokens() {
    assert!(!tokens_match_source(
        "",
        &[token(K::End, 0, 0, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        " ",
        &[token(K::Space, 0, 1, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        "\n",
        &[token(K::Newline, 0, 1, 1, 1, "")],
        1
    ));
    assert!(!tokens_match_source(
        "//",
        &[token(K::Comment, 0, 2, 1, 1, "")],
        1
    ));
}

#[test]
fn invalid_source_cannot_be_accepted_by_empty_or_forged_tokens() {
    for source in ["@", "雪", "0x", "340282366920938463463374607431768211456"] {
        assert!(!tokens_match_source(source, &[], 1));
        assert!(!tokens_match_source(
            source,
            &[token(K::Ident, 0, source.len() as u32, 1, 1, source)],
            1
        ));
    }
}

#[test]
fn exact_full_stream_accepts_leading_and_trailing_trivia() {
    let source = " \n// note\nfoo == 42; // trailing \n";
    let tokens = [
        token(K::Ident, 10, 13, 3, 1, "foo"),
        token(K::EqEq, 14, 16, 3, 5, ""),
        token(K::Number(42), 17, 19, 3, 8, ""),
        token(K::Semicolon, 19, 20, 3, 10, ""),
    ];
    assert!(tokens_match_source(source, &tokens, 4));
    assert!(tokens_match_source(source, &tokens, usize::MAX));
    assert!(!tokens_match_source(source, &tokens, 3));
    assert!(!tokens_match_source(source, &tokens[1..], 4));
}

#[test]
fn invalid_source_between_valid_tokens_cannot_be_skipped() {
    let source = "foo $ 42;";
    let tokens = [
        token(K::Ident, 0, 3, 1, 1, "foo"),
        token(K::Number(42), 6, 8, 1, 7, ""),
        token(K::Semicolon, 8, 9, 1, 9, ""),
    ];
    assert!(!tokens_match_source(source, &tokens, 3));
    assert!(!tokens_match_source("\0", &[], 0));
    let source = "340282366920938463463374607431768211456";
    let forged = [token(K::Number(u128::MAX), 0, 39, 1, 1, "")];
    assert!(!tokens_match_source(source, &forged, 1));
}

#[test]
fn coordinates_saturate_without_narrowing_width() {
    assert_eq!(advance_coordinate(1, 0), 1);
    assert_eq!(advance_coordinate(1, 2), 3);
    assert_eq!(advance_coordinate(u32::MAX, 0), u32::MAX);
    assert_eq!(advance_coordinate(u32::MAX - 1, 1), u32::MAX);
    assert_eq!(advance_coordinate(u32::MAX - 1, 2), u32::MAX);
    assert_eq!(advance_coordinate(1, usize::MAX), u32::MAX);
}
