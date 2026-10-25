use crate::comparison_ops::{CompareOp, read_compare_op};
use crate::lexical_core::{LexemeKind as Kind, next_lexeme};

const CASES: [(&[u8], Kind, CompareOp); 6] = [
    (b"==", Kind::EqEq, CompareOp::Eq),
    (b"!=", Kind::NotEq, CompareOp::NotEq),
    (b"<", Kind::Less, CompareOp::Less),
    (b"<=", Kind::LessEq, CompareOp::LessEq),
    (b">", Kind::Greater, CompareOp::Greater),
    (b">=", Kind::GreaterEq, CompareOp::GreaterEq),
];

#[test]
fn all_six_tokens_return_the_actual_ast_enum() {
    for (_, kind, expected) in CASES {
        assert_eq!(read_compare_op(kind), Some(expected));
    }
}

#[test]
fn actual_lexical_spelling_selects_the_expected_operation() {
    for (source, expected_kind, expected_op) in CASES {
        let token = next_lexeme(source, 0);
        assert_eq!(token.kind, expected_kind);
        assert_eq!((token.start, token.end), (0, source.len()));
        assert_eq!(read_compare_op(token.kind), Some(expected_op));
    }
}

#[test]
fn every_noncomparison_kind_is_refused() {
    for kind in [
        Kind::End,
        Kind::Space,
        Kind::Newline,
        Kind::Comment,
        Kind::Ident,
        Kind::Number(0),
        Kind::Number(u128::MAX),
        Kind::Hex,
        Kind::LBrace,
        Kind::RBrace,
        Kind::LParen,
        Kind::RParen,
        Kind::LBracket,
        Kind::RBracket,
        Kind::Semicolon,
        Kind::Comma,
        Kind::Dot,
        Kind::Range,
        Kind::Colon,
        Kind::Equal,
        Kind::Plus,
        Kind::Minus,
        Kind::Star,
        Kind::Slash,
        Kind::AndAnd,
        Kind::OrOr,
        Kind::Bang,
        Kind::Arrow,
        Kind::InvalidNumber,
        Kind::InvalidHex,
        Kind::InvalidByte,
    ] {
        assert_eq!(read_compare_op(kind), None, "{kind:?}");
    }
}

#[test]
fn signed_evaluation_matches_independent_ordering_at_boundaries() {
    let inputs = [i128::MIN, i128::MIN + 1, -1, 0, 1, i128::MAX - 1, i128::MAX];
    for left in inputs {
        for right in inputs {
            let ordering = left.cmp(&right);
            let expected = [
                ordering.is_eq(),
                !ordering.is_eq(),
                ordering.is_lt(),
                !ordering.is_gt(),
                ordering.is_gt(),
                !ordering.is_lt(),
            ];
            for ((_, _, op), expected) in CASES.iter().zip(expected) {
                assert_eq!(op.evaluate_i128(left, right), expected);
            }
        }
    }
}

#[test]
fn unsigned_evaluation_does_not_narrow_high_values() {
    let inputs = [
        0,
        1,
        i128::MAX as u128,
        1u128 << 127,
        u128::MAX - 1,
        u128::MAX,
    ];
    for left in inputs {
        for right in inputs {
            let ordering = left.cmp(&right);
            let expected = [
                ordering.is_eq(),
                !ordering.is_eq(),
                ordering.is_lt(),
                !ordering.is_gt(),
                ordering.is_gt(),
                !ordering.is_lt(),
            ];
            for ((_, _, op), expected) in CASES.iter().zip(expected) {
                assert_eq!(op.evaluate_u128(left, right), expected);
            }
        }
    }
}

#[test]
fn relational_direction_is_preserved_from_source_to_evaluation() {
    for (source, _, op) in CASES {
        let decoded = read_compare_op(next_lexeme(source, 0).kind).unwrap();
        assert_eq!(decoded.evaluate_i128(-7, 11), op.evaluate_i128(-7, 11));
        if matches!(
            op,
            CompareOp::Less | CompareOp::LessEq | CompareOp::Greater | CompareOp::GreaterEq
        ) {
            assert_ne!(decoded.evaluate_i128(-7, 11), decoded.evaluate_i128(11, -7));
        }
    }
}

#[test]
fn strict_and_nonstrict_operations_differ_exactly_at_equality() {
    for value in [i128::MIN, -1, 0, 1, i128::MAX] {
        assert!(!CompareOp::Less.evaluate_i128(value, value));
        assert!(CompareOp::LessEq.evaluate_i128(value, value));
        assert!(!CompareOp::Greater.evaluate_i128(value, value));
        assert!(CompareOp::GreaterEq.evaluate_i128(value, value));
        assert!(CompareOp::Eq.evaluate_i128(value, value));
        assert!(!CompareOp::NotEq.evaluate_i128(value, value));
    }
}
