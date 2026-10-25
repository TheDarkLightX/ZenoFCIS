use super::lexical_core::{LexemeKind as K, decimal_push, next_lexeme};
use std::format;
use std::string::ToString;

#[test]
fn decimal_edges_and_leading_zeroes_match_the_standard_numeric_oracle() {
    let values = [
        0,
        1,
        9,
        10,
        99,
        100,
        u64::MAX as u128,
        i128::MAX as u128,
        (i128::MAX as u128) + 1,
        u128::MAX - 1,
        u128::MAX,
    ];
    for value in values {
        for padding in [0, 1, 40, 4096] {
            let text = format!("{}{}", "0".repeat(padding), value);
            let parsed = text.parse::<u128>().unwrap();
            let lexeme = next_lexeme(text.as_bytes(), 0);
            assert_eq!(lexeme.kind, K::Number(parsed));
            assert_eq!(lexeme.end, text.len());
        }
    }
}

#[test]
fn overflowing_decimal_is_consumed_maximally_and_refused() {
    for text in [
        "340282366920938463463374607431768211456".to_string(),
        "9".repeat(4096),
        format!("{}1", u128::MAX),
    ] {
        assert!(text.parse::<u128>().is_err());
        let source = format!("{text};");
        let result = next_lexeme(source.as_bytes(), 0);
        assert_eq!(result.kind, K::InvalidNumber);
        assert_eq!(result.end, text.len());
        assert_eq!(
            next_lexeme(source.as_bytes(), result.end).kind,
            K::Semicolon
        );
    }
}

#[test]
fn decimal_accumulation_refuses_exactly_at_the_last_digit_boundary() {
    let prefix = u128::MAX / 10;
    for digit in 0..=9 {
        assert_eq!(
            decimal_push(prefix, digit),
            prefix
                .checked_mul(10)
                .and_then(|p| p.checked_add(digit as u128))
        );
    }
    assert_eq!(decimal_push(prefix + 1, 0), None);
    assert_eq!(decimal_push(0, 0), Some(0));
}

#[test]
fn commitment_spelling_is_exact_and_scan_is_maximal() {
    let valid = format!("0x{}", "0123456789abcdef".repeat(4));
    assert_eq!(next_lexeme(valid.as_bytes(), 0).kind, K::Hex);
    for text in [
        format!("0x{}", "a".repeat(63)),
        format!("0x{}", "a".repeat(65)),
        format!("0x{}A", "0".repeat(63)),
        "0x".to_string(),
    ] {
        let got = next_lexeme(text.as_bytes(), 0);
        assert_eq!(got.kind, K::InvalidHex);
        assert_eq!(got.end, text.len());
    }
    let adjacent = format!("{valid}g");
    let got = next_lexeme(adjacent.as_bytes(), 0);
    assert_eq!(got.kind, K::Hex);
    assert_eq!(got.end, valid.len());
    assert_eq!(next_lexeme(adjacent.as_bytes(), got.end).kind, K::Ident);
}

#[test]
fn hyphens_retain_existing_identifier_precedence() {
    for source in [
        b"x-1;".as_slice(),
        b"release-policy;".as_slice(),
        b"x->1".as_slice(),
    ] {
        let ident = next_lexeme(source, 0);
        assert_eq!(ident.kind, K::Ident);
        let expected_end = if source == b"x->1" {
            2
        } else {
            source.len() - 1
        };
        assert_eq!(ident.end, expected_end);
    }
    assert_eq!(next_lexeme(b"->", 0).kind, K::Arrow);
    assert_eq!(next_lexeme(b"x->1", 2).kind, K::Greater);
}

#[test]
fn operator_prefixes_consume_the_longest_declared_symbol() {
    for (source, kind, length) in [
        (b"===".as_slice(), K::EqEq, 2),
        (b"...", K::Range, 2),
        (b"!==", K::NotEq, 2),
        (b"<==", K::LessEq, 2),
        (b">==", K::GreaterEq, 2),
        (b"&&&", K::AndAnd, 2),
        (b"|||", K::OrOr, 2),
        (b"->>", K::Arrow, 2),
        (b"&", K::InvalidByte, 1),
        (b"|", K::InvalidByte, 1),
    ] {
        let got = next_lexeme(source, 0);
        assert_eq!((got.kind, got.end), (kind, length));
    }
}

#[test]
fn comments_and_trivia_do_not_become_tokens() {
    let source = " \t\r// héllo\n42".as_bytes();
    let space = next_lexeme(source, 0);
    assert_eq!((space.kind, space.end), (K::Space, 3));
    let comment = next_lexeme(source, space.end);
    assert_eq!(comment.kind, K::Comment);
    assert_eq!(source[comment.end], b'\n');
    let newline = next_lexeme(source, comment.end);
    assert_eq!(newline.kind, K::Newline);
    assert_eq!(next_lexeme(source, newline.end).kind, K::Number(42));
    assert_eq!(next_lexeme(b"//", 0).end, 2);
}

#[test]
fn every_two_byte_input_makes_bounded_progress_without_panicking() {
    for a in 0u8..=255 {
        for b in 0u8..=255 {
            let bytes = [a, b];
            let first = next_lexeme(&bytes, 0);
            assert_eq!(first.start, 0);
            assert!(first.end > 0 && first.end <= 2);
            let second = next_lexeme(&bytes, first.end);
            assert_eq!(second.start, first.end);
            assert!(second.end <= 2);
            assert!(second.end > second.start || second.kind == K::End);
        }
    }
}

#[test]
fn arbitrary_cursor_values_are_total_and_eof_is_stable() {
    for source in [b"".as_slice(), b"0".as_slice(), b"name".as_slice()] {
        for cursor in [source.len(), source.len() + 1, usize::MAX] {
            let got = next_lexeme(source, cursor);
            assert_eq!(
                (got.kind, got.start, got.end),
                (K::End, source.len(), source.len())
            );
        }
    }
}

#[test]
fn spelling_check_binds_actual_payload_bytes_and_refuses_bad_bounds() {
    use super::lexical_core::check_spelling;
    let source = b"foo bar 0xabcdef";
    assert!(check_spelling(source, 0, 3, b"foo"));
    assert!(check_spelling(source, 4, 7, b"bar"));
    assert!(check_spelling(source, 10, 16, b"abcdef"));
    assert!(!check_spelling(source, 0, 3, b"bar"));
    assert!(!check_spelling(source, 0, 3, b"fo"));
    assert!(!check_spelling(source, 0, 3, b"food"));
    assert!(!check_spelling(source, 4, 3, b""));
    assert!(!check_spelling(source, 0, usize::MAX, b""));
    assert!(!check_spelling(source, usize::MAX, usize::MAX, b""));
    assert!(check_spelling(source, source.len(), source.len(), b""));
    assert!(check_spelling(b"", 0, 0, b""));
}
