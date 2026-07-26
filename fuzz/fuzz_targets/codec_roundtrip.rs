#![no_main]

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_value};

fuzz_target!(|input: &[u8]| {
    let Ok(value) = decode_value(input, DecodeLimits::default()) else {
        return;
    };
    let Ok(canonical) = value.canonical_bytes() else {
        panic!("a decoded value must be canonically encodable");
    };
    assert_eq!(
        canonical, input,
        "the strict decoder must accept canonical bytes only"
    );
    let Ok(decoded_again) = decode_value(&canonical, DecodeLimits::default()) else {
        panic!("canonical re-encoding must decode");
    };
    assert_eq!(decoded_again, value);
    let Ok(second_encoding) = decoded_again.canonical_bytes() else {
        panic!("a round-tripped value must be canonically encodable");
    };
    assert_eq!(second_encoding, canonical);
});
