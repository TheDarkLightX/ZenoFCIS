//! Closed SHA-256 providers retained from the original crypto adapters.
//!
//! Marker types, algorithm identities and primitive bodies remain unchanged.

use crate::{CommitmentHasher, Hash32};

use sha2::{Digest as _, Sha256 as RustCryptoEngine};

#[cfg(feature = "libcrux")]
use libcrux_sha2::{Digest as _, Sha256 as LibcruxEngine};

/// SHA-256 backed by RustCrypto `sha2`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RustCryptoSha256;

impl crate::hasher_seal::Sealed for RustCryptoSha256 {}

impl CommitmentHasher for RustCryptoSha256 {
    const ALGORITHM_ID: &'static str = "sha2-256/rustcrypto-0.11.0";

    fn hash(bytes: &[u8]) -> Hash32 {
        let digest = RustCryptoEngine::digest(bytes);
        let mut output = [0_u8; 32];
        output.copy_from_slice(&digest);
        Hash32::new(output)
    }

    fn hash_parts(parts: &[&[u8]]) -> Hash32 {
        let mut engine = RustCryptoEngine::new();
        for part in parts {
            engine.update(part);
        }
        let digest = engine.finalize();
        let mut output = [0_u8; 32];
        output.copy_from_slice(&digest);
        Hash32::new(output)
    }
}

/// SHA-256 backed by the libcrux HACL* implementation.
#[cfg(feature = "libcrux")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LibcruxSha256;

#[cfg(feature = "libcrux")]
impl crate::hasher_seal::Sealed for LibcruxSha256 {}

#[cfg(feature = "libcrux")]
impl CommitmentHasher for LibcruxSha256 {
    const ALGORITHM_ID: &'static str = "sha2-256/libcrux-0.0.8-hacl";

    fn hash(bytes: &[u8]) -> Hash32 {
        Self::hash_parts(&[bytes])
    }

    fn hash_parts(parts: &[&[u8]]) -> Hash32 {
        const MAXIMUM_CHUNK: usize = u32::MAX as usize;

        let mut engine = LibcruxEngine::new();
        for part in parts {
            for chunk in part.chunks(MAXIMUM_CHUNK) {
                engine.update(chunk);
            }
        }
        let mut output = [0_u8; 32];
        engine.finish(&mut output);
        Hash32::new(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const EMPTY: Hash32 = Hash32::new([
        0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9,
        0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52,
        0xb8, 0x55,
    ]);
    const ABC: Hash32 = Hash32::new([
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ]);
    const MULTIBLOCK: Hash32 = Hash32::new([
        0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e, 0x60,
        0x39, 0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67, 0xf6, 0xec, 0xed, 0xd4, 0x19, 0xdb,
        0x06, 0xc1,
    ]);

    fn known_answers_and_segmented_boundaries<H: CommitmentHasher>() {
        let vectors = [
            (&b""[..], EMPTY),
            (&b"abc"[..], ABC),
            (
                &b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..],
                MULTIBLOCK,
            ),
        ];
        for (message, expected) in vectors {
            assert_eq!(H::hash(message), expected);
            for split in 0..=message.len() {
                assert_eq!(
                    H::hash_parts(&[&message[..split], b"", &message[split..]]),
                    expected
                );
            }
        }
        assert_eq!(H::hash_parts(&[]), EMPTY);
        for length in (0..=130).chain([255, 256, 257, 1024, 65_536]) {
            let bytes: Vec<u8> = (0_u8..=255).cycle().take(length).collect();
            let expected = H::hash(&bytes);
            for chunk_length in [1, 55, 56, 63, 64, 65, 257] {
                let parts: Vec<&[u8]> = bytes.chunks(chunk_length).collect();
                assert_eq!(H::hash_parts(&parts), expected);
            }
        }
    }

    #[test]
    fn rustcrypto_fixed_vectors_and_streaming_boundaries() {
        assert_eq!(RustCryptoSha256::ALGORITHM_ID, "sha2-256/rustcrypto-0.11.0");
        known_answers_and_segmented_boundaries::<RustCryptoSha256>();
    }

    #[cfg(feature = "libcrux")]
    #[test]
    fn libcrux_fixed_vectors_and_streaming_boundaries() {
        assert_eq!(LibcruxSha256::ALGORITHM_ID, "sha2-256/libcrux-0.0.8-hacl");
        known_answers_and_segmented_boundaries::<LibcruxSha256>();
    }

    #[cfg(feature = "libcrux")]
    #[test]
    fn independent_provider_parity_includes_actual_domain_commitments() {
        use crate::{commitment, domain_preimage, domains};
        for length in (0..=130).chain([255, 256, 257, 1024, 65_536]) {
            let bytes: Vec<u8> = (0_u8..=255).cycle().take(length).collect();
            let expected = RustCryptoSha256::hash(&bytes);
            assert_eq!(LibcruxSha256::hash(&bytes), expected);
            for domain in [domains::TEST, domains::VALUE, domains::V2_CERTIFICATE] {
                let preimage = domain_preimage(domain, &bytes)
                    .unwrap_or_else(|error| panic!("frame: {error}"));
                let expected = RustCryptoSha256::hash(&preimage);
                assert_eq!(LibcruxSha256::hash(&preimage), expected);
                assert_eq!(commitment::<RustCryptoSha256>(domain, &bytes), Ok(expected));
                assert_eq!(commitment::<LibcruxSha256>(domain, &bytes), Ok(expected));
            }
        }
        let mut changed = Vec::from([1, 2, 3]);
        let before = RustCryptoSha256::hash(&changed);
        changed[1] ^= 1;
        assert_ne!(RustCryptoSha256::hash(&changed), before);
        assert_ne!(LibcruxSha256::hash(&changed), before);
    }
}
