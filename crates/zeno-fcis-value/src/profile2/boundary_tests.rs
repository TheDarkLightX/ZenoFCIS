//! Source-complete native tests prepared for a later authorized compiler window.
//! No result is claimed by retaining this file.
use super::*;
use alloc::vec::Vec;

// The corpus carries exactly one 33-byte atom. Refuse malformed hex instead
// of letting chunks_exact discard an unmatched trailing nibble.
fn atom_vector_hex(text: &str) -> Result<Vec<u8>, &'static str> {
    if text.len() != 66 {
        return Err("atom vector hex length");
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = core::str::from_utf8(pair).map_err(|_| "atom vector hex syntax")?;
            u8::from_str_radix(pair, 16).map_err(|_| "atom vector hex syntax")
        })
        .collect()
}

#[test]
fn independent_atom_and_decimal_vectors() {
    for row in include_str!("atom-vectors.tsv").lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let value = match fields[0] {
            "u256" => {
                let value = parse_unsigned_decimal(fields[1]).unwrap();
                assert_eq!(unsigned_decimal(value).unwrap(), fields[1]);
                Integer256::Unsigned(value)
            }
            "i256" => {
                let value = parse_signed_decimal(fields[1]).unwrap();
                assert_eq!(signed_decimal(value).unwrap(), fields[1]);
                Integer256::Signed(value)
            }
            _ => panic!("invalid committed control kind"),
        };
        let expected = atom_vector_hex(fields[2]).unwrap();
        let actual = encode_atom(ValueProfile::Zcve2, value).unwrap();
        assert_eq!(&actual[..], &expected[..]);
        assert_eq!(decode_atom(ValueProfile::Zcve2, &actual).unwrap(), value);
        assert_eq!(
            encode_atom(ValueProfile::Zcve1, value),
            Err(BoundaryError::UnsupportedProfile)
        );
        assert_eq!(
            decode_atom(ValueProfile::Zcve1, &actual),
            Err(BoundaryError::UnsupportedProfile)
        );
    }
}

#[test]
fn malformed_atom_and_canonical_decimal_refuse() {
    assert_eq!(
        ValueProfile::parse("ZCVE/3"),
        Err(BoundaryError::UnsupportedProfile)
    );
    for raw in [
        "", "-0", "00", "01", "+1", " 1", "1 ", "1.0", "1e2", "0x1", "１",
    ] {
        assert_eq!(parse_signed_decimal(raw), Err(BoundaryError::DecimalSyntax));
    }
    assert_eq!(
        parse_unsigned_decimal(
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        ),
        Err(BoundaryError::OutOfRange)
    );
    assert_eq!(
        parse_signed_decimal(
            "57896044618658097711785492504343953926634992332820282019728792003956564819968"
        ),
        Err(BoundaryError::OutOfRange)
    );
    assert_eq!(
        parse_signed_decimal(
            "-57896044618658097711785492504343953926634992332820282019728792003956564819969"
        ),
        Err(BoundaryError::OutOfRange)
    );
    for length in 0..33 {
        assert_eq!(
            decode_atom(ValueProfile::Zcve2, &alloc::vec![0; length]),
            Err(BoundaryError::Length {
                expected: 33,
                actual: length
            })
        );
    }
    assert_eq!(
        decode_atom(ValueProfile::Zcve2, &[0; 34]),
        Err(BoundaryError::Length {
            expected: 33,
            actual: 34
        })
    );
    assert_eq!(
        decode_atom(ValueProfile::Zcve2, &[0; 33]),
        Err(BoundaryError::UnknownTag(0))
    );
}

#[test]
fn signed_delta_is_not_a_full_unsigned_transfer() {
    assert_eq!(
        I256::difference_unsigned(U256::MAX, U256::ZERO),
        Err(Failure::Overflow)
    );
    let value = Integer256::Unsigned(U256::MAX);
    let encoded = encode_atom(ValueProfile::Zcve2, value).unwrap();
    assert_eq!(decode_atom(ValueProfile::Zcve2, &encoded).unwrap(), value);
    assert_eq!(I256::MIN.checked_neg(), Err(Failure::Overflow));
    assert_eq!(U256::MAX.try_u128(), Err(Failure::Overflow));
}

#[test]
fn atom_vector_reader_refuses_malformed_hex_without_discarding_input() {
    let valid = "00".repeat(33);
    assert_eq!(atom_vector_hex(&valid).unwrap(), alloc::vec![0; 33]);
    for malformed in [
        alloc::string::String::new(),
        "0".into(),
        valid.clone() + "0",
        valid.clone() + "00",
        "00".repeat(32),
        "00".repeat(32) + "0G",
        "00".repeat(32) + " 0",
        "00".repeat(32) + "é",
    ] {
        assert!(atom_vector_hex(&malformed).is_err(), "{malformed:?}");
    }
}
