#![no_main]

//! codec2 (`ZFCISV2`) envelope and bare-value decoding under the default limits.
//!
//! Differential oracle: the harness parses the fixed envelope header on its
//! own (magic, big-endian u32 type id, 32-byte schema hash, big-endian u32
//! payload length, no trailing bytes) and requires the decoder to agree with
//! that parse exactly: it accepts iff the header parses and the payload is a
//! canonical value, and the fields it reports are the header's. Every accepted
//! input must re-encode to the same bytes and decode again to the same value.

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::profile2::{
    Envelope, canonical_value_bytes_with_limits, decode_envelope, decode_value,
};
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Hash32};

const MAGIC: &[u8; 8] = b"ZFCISV2\0";

/// Independent header parse: `Some((type_id, schema_hash, payload))` when the
/// framing is well formed, `None` otherwise. It says nothing about the payload.
fn reference_header(input: &[u8]) -> Option<(u32, Hash32, &[u8])> {
    if input.len() < 8 + 4 + 32 + 4 || &input[..8] != MAGIC {
        return None;
    }
    let type_id = u32::from_be_bytes([input[8], input[9], input[10], input[11]]);
    let mut hash = [0; 32];
    hash.copy_from_slice(&input[12..44]);
    let length = u32::from_be_bytes([input[44], input[45], input[46], input[47]]) as usize;
    let payload = &input[48..];
    if payload.len() != length {
        return None;
    }
    Some((type_id, Hash32::new(hash), payload))
}

fuzz_target!(|input: &[u8]| {
    let limits = DecodeLimits::default();
    let header = reference_header(input);
    match decode_envelope(input, limits) {
        Ok(envelope) => {
            let Some((type_id, hash, payload)) = header else {
                panic!("the decoder accepted an envelope whose framing the reference refuses");
            };
            assert_eq!(
                envelope.type_id(),
                type_id,
                "type id must be the header's big-endian u32"
            );
            assert_eq!(
                envelope.schema_hash(),
                hash,
                "schema hash must be the header's 32 bytes"
            );
            let Ok(payload_value) = decode_value(payload, limits) else {
                panic!("an accepted envelope's payload must decode as a bare value");
            };
            assert_eq!(
                &payload_value,
                envelope.value(),
                "envelope value must be the payload's value"
            );
            let Ok(canonical) = envelope.canonical_bytes() else {
                panic!("a decoded envelope must be canonically encodable");
            };
            assert_eq!(
                canonical, input,
                "the strict decoder must accept canonical bytes only"
            );
            let Ok(again) = decode_envelope(&canonical, limits) else {
                panic!("canonical re-encoding must decode");
            };
            assert_eq!(again, envelope);
            let rebuilt = Envelope::new(type_id, hash, payload_value);
            let Ok(rebuilt_bytes) = rebuilt.canonical_bytes() else {
                panic!("an envelope rebuilt from the header and payload must encode");
            };
            assert_eq!(
                rebuilt_bytes, input,
                "header plus payload must rebuild the same bytes"
            );
        }
        Err(_) => {
            if let Some((_, _, payload)) = header {
                // A refused envelope with well-formed framing must have a payload the
                // bare-value decoder refuses too; otherwise the two decoders disagree.
                assert!(
                    decode_value(payload, limits).is_err(),
                    "well-formed framing with a canonical payload was refused"
                );
            }
        }
    }

    // The bare-value route is the existing codec_roundtrip oracle, kept here for
    // the profile-2 decoder: canonical bytes only, stable under re-encoding.
    if let Ok(value) = decode_value(input, limits) {
        let Ok(canonical) = canonical_value_bytes_with_limits(&value, limits.value) else {
            panic!("a decoded value must be canonically encodable");
        };
        assert_eq!(
            canonical, input,
            "the strict value decoder must accept canonical bytes only"
        );
        assert_eq!(decode_value(&canonical, limits).ok().as_ref(), Some(&value));
    }
});
