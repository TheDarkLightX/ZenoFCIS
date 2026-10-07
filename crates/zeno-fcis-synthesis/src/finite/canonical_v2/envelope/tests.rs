use super::{Failure, frame};
use alloc::vec::Vec;

fn encoded(root: u32, schema: [u8; 32], body: &[u8]) -> Vec<u8> {
    let mut bytes = b"ZFCISV1\0".to_vec();
    bytes.extend_from_slice(&root.to_be_bytes());
    bytes.extend_from_slice(&schema);
    bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn oracle<'a>(
    bytes: &'a [u8],
    root: u32,
    schema: &[u8; 32],
    max: u64,
) -> Result<&'a [u8], Failure> {
    if bytes.len() as u64 > max {
        return Err(Failure::Size);
    }
    if bytes.len() < 48 {
        return Err(Failure::Short);
    }
    if bytes.get(..8) != Some(b"ZFCISV1\0".as_slice()) {
        return Err(Failure::Magic);
    }
    let actual_root = u32::from_be_bytes(
        bytes[8..12]
            .try_into()
            .unwrap_or_else(|error| panic!("root: {error:?}")),
    );
    if actual_root != root {
        return Err(Failure::Root);
    }
    if &bytes[12..44] != schema {
        return Err(Failure::Schema);
    }
    let actual_length = u32::from_be_bytes(
        bytes[44..48]
            .try_into()
            .unwrap_or_else(|error| panic!("length: {error:?}")),
    );
    if actual_length as usize != bytes.len() - 48 {
        return Err(Failure::Length);
    }
    Ok(&bytes[48..])
}

#[test]
fn exact_custody_for_empty_and_real_payloads() {
    for root in [0, 1, 101, u32::MAX] {
        for body in [b"".as_slice(), &[0x01], &[0x09, 0, 0, 0, 0], &[0xff; 128]] {
            let schema = core::array::from_fn(|index| (index * 7) as u8);
            let bytes = encoded(root, schema, body);
            let admitted = frame(&bytes, root, &schema, bytes.len() as u64)
                .unwrap_or_else(|error| panic!("frame: {error:?}"));
            assert_eq!(admitted.original(), &bytes);
            assert_eq!(admitted.bytes(), body);
            assert!(core::ptr::eq(admitted.original().as_ptr(), bytes.as_ptr()));
            assert!(core::ptr::eq(
                admitted.bytes().as_ptr(),
                bytes[48..].as_ptr()
            ));
            assert_eq!(
                frame(&bytes, root, &schema, bytes.len() as u64 - 1).err(),
                Some(Failure::Size)
            );
        }
    }
}

#[test]
fn every_header_byte_and_truncation_matches_independent_oracle() {
    let schema = core::array::from_fn(|index| index as u8);
    let root = 0xfedc_0123;
    let original = encoded(root, schema, &[0x09, 0, 0, 0, 0]);
    for size in 0..=original.len() {
        for max in [0, 47, 48, 53, u64::MAX] {
            let result = frame(&original[..size], root, &schema, max);
            assert_eq!(
                result
                    .as_ref()
                    .map(|payload| payload.bytes())
                    .map_err(|error| *error),
                oracle(&original[..size], root, &schema, max)
            );
        }
    }
    for offset in 0..48 {
        for replacement in 0..=u8::MAX {
            let mut changed = original.clone();
            changed[offset] = replacement;
            let result = frame(&changed, root, &schema, u64::MAX);
            assert_eq!(
                result
                    .as_ref()
                    .map(|payload| payload.bytes())
                    .map_err(|error| *error),
                oracle(&changed, root, &schema, u64::MAX)
            );
        }
    }
    let mut trailing = original.clone();
    trailing.push(0);
    assert_eq!(
        frame(&trailing, root, &schema, u64::MAX).err(),
        Some(Failure::Length)
    );
}
