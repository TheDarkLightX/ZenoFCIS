use alloc::{vec, vec::Vec};

use crate::lexical_core::{LexemeKind as Kind, is_trivia, next_lexeme};
use crate::semantic_readers::{read_projection_tail, signed_magnitude};

fn tail(ids: &[u32]) -> Vec<Kind> {
    ids.iter()
        .flat_map(|id| [Kind::Dot, Kind::Number(u128::from(*id))])
        .collect()
}

#[test]
fn signed_zero_and_small_values_preserve_the_sign() {
    for magnitude in [0, 1, 7, 10, 1_000_000] {
        assert_eq!(signed_magnitude(false, magnitude), Some(magnitude as i128));
        assert_eq!(
            signed_magnitude(true, magnitude),
            Some(-(magnitude as i128))
        );
    }
}

#[test]
fn signed_positive_endpoint_is_inclusive() {
    assert_eq!(signed_magnitude(false, i128::MAX as u128), Some(i128::MAX));
    assert_eq!(signed_magnitude(true, i128::MAX as u128), Some(-i128::MAX));
    assert_eq!(signed_magnitude(false, i128::MAX as u128 + 1), None);
}

#[test]
fn signed_negative_endpoint_has_its_exact_value() {
    assert_eq!(signed_magnitude(true, 1u128 << 127), Some(i128::MIN));
    assert_eq!(signed_magnitude(true, (1u128 << 127) + 1), None);
}

#[test]
fn signed_wide_magnitudes_do_not_wrap() {
    for magnitude in [(1u128 << 127) + 1, u128::MAX - 1, u128::MAX] {
        assert_eq!(signed_magnitude(false, magnitude), None);
        assert_eq!(signed_magnitude(true, magnitude), None);
    }
}

#[test]
fn only_empty_input_returns_the_empty_variable_tail() {
    for max_segments in [0, 1, 64, usize::MAX] {
        assert_eq!(read_projection_tail(&[], max_segments), Some(vec![]));
    }
    assert_eq!(read_projection_tail(&tail(&[1]), 0), None);
    assert_eq!(read_projection_tail(&tail(&[1]), 1), Some(vec![1]));
}

#[test]
fn projection_order_and_full_u32_payloads_are_preserved() {
    let ids = [7, 2, u32::MAX, 1, 7];
    assert_eq!(read_projection_tail(&tail(&ids), 64), Some(ids.to_vec()));
}

#[test]
fn projection_refuses_unpaired_prefix_and_suffix_tokens() {
    for kinds in [
        vec![Kind::Dot],
        vec![Kind::Number(1)],
        vec![Kind::Dot, Kind::Number(1), Kind::Dot],
        vec![Kind::Dot, Kind::Number(1), Kind::Ident],
        vec![Kind::Number(1), Kind::Dot, Kind::Number(2)],
    ] {
        assert_eq!(read_projection_tail(&kinds, 64), None);
    }
}

#[test]
fn projection_refuses_non_dot_separators_and_non_number_ids() {
    for invalid in [
        Kind::Range,
        Kind::Comma,
        Kind::End,
        Kind::Space,
        Kind::Minus,
        Kind::InvalidNumber,
        Kind::Hex,
        Kind::Ident,
    ] {
        assert_eq!(read_projection_tail(&[invalid, Kind::Number(1)], 64), None);
        assert_eq!(read_projection_tail(&[Kind::Dot, invalid], 64), None);
    }
}

#[test]
fn projection_refuses_zero_and_wide_ids_without_filtering() {
    for magnitude in [0, u32::MAX as u128 + 1, u64::MAX as u128, u128::MAX] {
        let kinds = [
            Kind::Dot,
            Kind::Number(1),
            Kind::Dot,
            Kind::Number(magnitude),
        ];
        assert_eq!(read_projection_tail(&kinds, 64), None);
    }
}

#[test]
fn projection_budget_is_exact_and_never_truncates() {
    let sixty_four: Vec<_> = (1..=64).collect();
    let sixty_five: Vec<_> = (1..=65).collect();
    assert_eq!(
        read_projection_tail(&tail(&sixty_four), 64),
        Some(sixty_four)
    );
    assert_eq!(read_projection_tail(&tail(&sixty_five), 64), None);
    assert_eq!(
        read_projection_tail(&tail(&sixty_five), 65),
        Some(sixty_five)
    );
}

#[test]
fn actual_lexical_payloads_feed_the_projection_reader() {
    let source = b".7 // a retained separator follows\n .4294967295 .2";
    let mut kinds = Vec::new();
    let mut cursor = 0;
    while cursor < source.len() {
        let lexeme = next_lexeme(source, cursor);
        if !is_trivia(lexeme.kind) {
            kinds.push(lexeme.kind);
        }
        cursor = lexeme.end;
    }
    assert_eq!(read_projection_tail(&kinds, 64), Some(vec![7, u32::MAX, 2]));
    kinds.push(Kind::Dot);
    assert_eq!(read_projection_tail(&kinds, 64), None);
}

#[test]
fn signed_range_endpoints_match_independent_checked_conversion() {
    let magnitudes = [
        0,
        1,
        i64::MAX as u128,
        i128::MAX as u128 - 1,
        i128::MAX as u128,
        i128::MAX as u128 + 1,
        i128::MAX as u128 + 2,
        u128::MAX,
    ];
    for negative in [false, true] {
        for magnitude in magnitudes {
            let expected = if negative && magnitude == 1u128 << 127 {
                Some(i128::MIN)
            } else {
                i128::try_from(magnitude).ok().and_then(|value| {
                    if negative {
                        value.checked_neg()
                    } else {
                        Some(value)
                    }
                })
            };
            assert_eq!(signed_magnitude(negative, magnitude), expected);
        }
    }
}
