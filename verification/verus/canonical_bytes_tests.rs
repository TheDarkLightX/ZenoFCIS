//! Independent native byte interpretations using standard-library conversions.
extern crate std;

use super::canonical_v2::{read_big_endian, read_signed_128};
use std::{vec, vec::Vec};

fn unsigned_oracle(bytes: &[u8], offset: usize, width: usize) -> Option<(u128, usize)> {
    if width > 16 {
        return None;
    }
    let end = offset.checked_add(width)?;
    let payload = bytes.get(offset..end)?;
    let mut padded = [0u8; 16];
    padded[16 - width..].copy_from_slice(payload);
    Some((u128::from_be_bytes(padded), end))
}

fn signed_oracle(bytes: &[u8], offset: usize) -> Option<(i128, usize)> {
    let end = offset.checked_add(16)?;
    let payload: [u8; 16] = bytes.get(offset..end)?.try_into().ok()?;
    Some((i128::from_be_bytes(payload), end))
}

#[test]
fn every_two_byte_integer_preserves_byte_order() {
    for value in 0..=u16::MAX {
        let payload = value.to_be_bytes();
        let bytes = [0x51, payload[0], payload[1], 0xa7];
        assert_eq!(
            read_big_endian(&bytes, 1, 2),
            Some((u128::from(u16::from_be_bytes(payload)), 3))
        );
    }
}

#[test]
fn widths_offsets_and_every_truncation_match_std() {
    let patterns: Vec<Vec<u8>> = vec![
        vec![],
        vec![0; 40],
        vec![255; 40],
        (0..40).collect(),
        (0..40).map(|byte| 255 - byte).collect(),
        (0u8..40).map(|byte| byte.wrapping_mul(53)).collect(),
    ];
    for bytes in patterns {
        for length in 0..=bytes.len() {
            let prefix = &bytes[..length];
            for offset in 0..=bytes.len() + 1 {
                for width in 0..=18 {
                    assert_eq!(
                        read_big_endian(prefix, offset, width),
                        unsigned_oracle(prefix, offset, width)
                    );
                }
                assert_eq!(read_signed_128(prefix, offset), signed_oracle(prefix, offset));
            }
        }
    }
}

#[test]
fn every_sign_bit_boundary_and_both_signed_extremes_match_std() {
    let mut patterns = vec![0, 1, u128::MAX, i128::MAX as u128, 1u128 << 127];
    for bit in 0..128 {
        patterns.extend([1u128 << bit, !(1u128 << bit)]);
    }
    for value in patterns {
        let payload = value.to_be_bytes();
        let mut bytes = vec![0x51];
        bytes.extend(payload);
        bytes.push(0xa7);
        assert_eq!(read_signed_128(&bytes, 1), Some((i128::from_be_bytes(payload), 17)));
        assert_eq!(read_big_endian(&bytes, 1, 16), Some((u128::from_be_bytes(payload), 17)));
    }
}

#[test]
fn invalid_offsets_and_widths_never_wrap_or_read_outside_input() {
    for length in [0, 1, 15, 16, 17, 32] {
        let bytes = vec![0xab; length];
        for offset in [0, 1, 16, 17, usize::MAX - 16, usize::MAX - 1, usize::MAX] {
            for width in [0, 1, 15, 16, 17, usize::MAX] {
                assert_eq!(read_big_endian(&bytes, offset, width), unsigned_oracle(&bytes, offset, width));
            }
            assert_eq!(read_signed_128(&bytes, offset), signed_oracle(&bytes, offset));
        }
    }
}
