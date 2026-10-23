//! Native samples for each additional standard-library contract.
#![forbid(unsafe_code)]

#[test]
fn vec_into_boxed_slice_preserves_elements() {
    for size in [0, 1, 2, 31, 256, 4096] {
        let input: Vec<u32> = (0..size).map(|i| i ^ 0x9e37_79b9).collect();
        let expected = input.clone();
        let boxed = input.into_boxed_slice();
        assert_eq!(&*boxed, expected.as_slice());
    }
}

#[test]
fn string_into_boxed_str_preserves_characters_and_bytes() {
    for input in ["", "\0", "ASCII", "naïve", "💖", "a\r\n\u{7f}"] {
        let boxed = input.to_owned().into_boxed_str();
        assert_eq!(&*boxed, input);
        assert_eq!(boxed.as_bytes(), input.as_bytes());
    }
}

#[test]
fn slice_to_vec_clones_in_order() {
    for size in [0, 1, 2, 255, 256, 4096] {
        let input: Vec<u8> = (0..size).map(|i| (i * 73 + 17) as u8).collect();
        assert_eq!(input.as_slice().to_vec(), input);
    }
    let words = [String::from("first"), String::new(), String::from("last")];
    assert_eq!(words.as_slice().to_vec(), words);
}

#[test]
fn byte_ascii_is_exactly_the_seven_bit_range() {
    assert!([].is_ascii());
    for byte in 0_u8..=255 {
        assert_eq!([byte].is_ascii(), byte < 128);
        assert_eq!([0, byte, 127].is_ascii(), byte < 128);
    }
    let all_ascii: Vec<u8> = (0..128).collect();
    assert!(all_ascii.is_ascii());
}

#[test]
fn utf8_conversion_accepts_valid_sequences_and_preserves_bytes() {
    for input in ["", "\0\u{7f}", "ASCII", "é", "€", "💖", "\u{10ffff}"] {
        let decoded = std::str::from_utf8(input.as_bytes()).expect("valid literal");
        assert_eq!(decoded, input);
        assert_eq!(decoded.as_bytes(), input.as_bytes());
    }
    for bytes in [
        &[0x80][..],
        &[0xff],
        &[0xc0, 0x80],
        &[0xed, 0xa0, 0x80],
        &[0xf4, 0x90, 0x80, 0x80],
        &[0xe2, 0x82],
    ] {
        assert!(std::str::from_utf8(bytes).is_err());
    }
}
