//! Vetted SHA-256 providers for ZenoFCIS commitments.
//!
//! ZenoFCIS owns canonical encoding, domain separation, and exact preimage
//! construction. This crate supplies only cryptographic SHA-256 implementations
//! from independently maintained libraries and verifies them against fixed
//! vectors. It contains no custom cryptographic primitive.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(feature = "std")]
extern crate std;

use core::fmt;

use zeno_fcis_codec::{CommitmentHasher, Domain, Hash32, commitment};

#[cfg(feature = "parity")]
use zeno_fcis_codec::domain_preimage;

#[cfg(feature = "rustcrypto")]
use sha2::{Digest as _, Sha256 as RustCryptoEngine};

#[cfg(feature = "libcrux")]
use libcrux_sha2::{Digest as _, Sha256 as LibcruxEngine};

/// SHA-256 backed by RustCrypto `sha2`.
#[cfg(feature = "rustcrypto")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RustCryptoSha256;

#[cfg(feature = "rustcrypto")]
impl CommitmentHasher for RustCryptoSha256 {
    const ALGORITHM_ID: &'static str = "sha2-256/rustcrypto-0.11.0";

    fn hash(bytes: &[u8]) -> Hash32 {
        let digest = RustCryptoEngine::digest(bytes);
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
impl CommitmentHasher for LibcruxSha256 {
    const ALGORITHM_ID: &'static str = "sha2-256/libcrux-0.0.8-hacl";

    fn hash(bytes: &[u8]) -> Hash32 {
        const MAXIMUM_CHUNK: usize = u32::MAX as usize;

        let mut engine = LibcruxEngine::new();
        for chunk in bytes.chunks(MAXIMUM_CHUNK) {
            engine.update(chunk);
        }
        let mut output = [0_u8; 32];
        engine.finish(&mut output);
        Hash32::new(output)
    }
}

/// Successful fixed-vector verification for one provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownAnswerReport {
    algorithm_id: &'static str,
    vectors_checked: u16,
}

impl KnownAnswerReport {
    /// Returns the provider's stable algorithm identifier.
    #[must_use]
    pub const fn algorithm_id(self) -> &'static str {
        self.algorithm_id
    }

    /// Returns the number of fixed vectors checked.
    #[must_use]
    pub const fn vectors_checked(self) -> u16 {
        self.vectors_checked
    }
}

/// Successful parity verification between two independent providers.
#[cfg(feature = "parity")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderParityReport {
    left_algorithm_id: &'static str,
    right_algorithm_id: &'static str,
    bytes_checked: u64,
}

#[cfg(feature = "parity")]
impl ProviderParityReport {
    /// Returns the ordinary provider identifier.
    #[must_use]
    pub const fn left_algorithm_id(self) -> &'static str {
        self.left_algorithm_id
    }

    /// Returns the independent provider identifier.
    #[must_use]
    pub const fn right_algorithm_id(self) -> &'static str {
        self.right_algorithm_id
    }

    /// Returns the number of input bytes compared.
    #[must_use]
    pub const fn bytes_checked(self) -> u64 {
        self.bytes_checked
    }
}

/// Provider self-check failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderVerificationError {
    /// One fixed SHA-256 vector differed from its published digest.
    KnownAnswerMismatch {
        /// Zero-based vector ordinal.
        vector: u16,
        /// Expected digest.
        expected: Hash32,
        /// Observed digest.
        observed: Hash32,
    },
    /// One domain-separated ZenoFCIS commitment vector differed.
    DomainVectorMismatch {
        /// Expected digest.
        expected: Hash32,
        /// Observed digest.
        observed: Hash32,
    },
    /// Two independently implemented providers disagreed.
    #[cfg(feature = "parity")]
    ProviderMismatch {
        /// Ordinary provider output.
        left: Hash32,
        /// Independent provider output.
        right: Hash32,
    },
    /// The semantic kernel rejected a supposedly fixed test preimage.
    SemanticEncoding,
    /// Input length could not be represented in a report.
    LengthOverflow,
}

impl fmt::Display for ProviderVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KnownAnswerMismatch { vector, .. } => {
                write!(formatter, "SHA-256 known-answer vector {vector} failed")
            }
            Self::DomainVectorMismatch { .. } => {
                formatter.write_str("ZenoFCIS domain-separated SHA-256 vector failed")
            }
            #[cfg(feature = "parity")]
            Self::ProviderMismatch { .. } => {
                formatter.write_str("independent SHA-256 providers disagreed")
            }
            Self::SemanticEncoding => formatter.write_str("fixed semantic preimage was rejected"),
            Self::LengthOverflow => formatter.write_str("verified byte length overflowed u64"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ProviderVerificationError {}

const EMPTY_DIGEST: Hash32 = Hash32::new([
    0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
    0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
]);
const ABC_DIGEST: Hash32 = Hash32::new([
    0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
    0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
]);
const MULTIBLOCK_DIGEST: Hash32 = Hash32::new([
    0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e, 0x60, 0x39,
    0xa3, 0x3c, 0xe4, 0x59, 0x64, 0xff, 0x21, 0x67, 0xf6, 0xec, 0xed, 0xd4, 0x19, 0xdb, 0x06, 0xc1,
]);
const DOMAIN_DIGEST: Hash32 = Hash32::new([
    0x71, 0x1a, 0xf1, 0x6b, 0x1d, 0x6f, 0xbb, 0xbe, 0x7e, 0x6c, 0x01, 0x91, 0x0c, 0xd9, 0x78, 0xdf,
    0x19, 0x92, 0x4f, 0xd6, 0xf4, 0xf7, 0x81, 0x8f, 0x7f, 0x4e, 0x85, 0x81, 0x35, 0x32, 0x72, 0xba,
]);
const MULTIBLOCK_MESSAGE: &[u8] = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";

/// Verifies one provider against published SHA-256 vectors and one ZenoFCIS
/// domain-separation vector.
pub fn verify_known_answers<H: CommitmentHasher>()
-> Result<KnownAnswerReport, ProviderVerificationError> {
    let vectors = [
        (&b""[..], EMPTY_DIGEST),
        (&b"abc"[..], ABC_DIGEST),
        (MULTIBLOCK_MESSAGE, MULTIBLOCK_DIGEST),
    ];
    for (ordinal, (message, expected)) in vectors.into_iter().enumerate() {
        let observed = H::hash(message);
        if observed != expected {
            let vector =
                u16::try_from(ordinal).map_err(|_| ProviderVerificationError::LengthOverflow)?;
            return Err(ProviderVerificationError::KnownAnswerMismatch {
                vector,
                expected,
                observed,
            });
        }
    }

    let domain = Domain::new("zeno-fcis/test", 1)
        .map_err(|_| ProviderVerificationError::SemanticEncoding)?;
    let observed =
        commitment::<H>(domain, b"abc").map_err(|_| ProviderVerificationError::SemanticEncoding)?;
    if observed != DOMAIN_DIGEST {
        return Err(ProviderVerificationError::DomainVectorMismatch {
            expected: DOMAIN_DIGEST,
            observed,
        });
    }

    Ok(KnownAnswerReport {
        algorithm_id: H::ALGORITHM_ID,
        vectors_checked: 4,
    })
}

/// Compares the two configured providers over exact bytes and over the semantic
/// kernel's exact domain-separated preimage.
#[cfg(feature = "parity")]
pub fn verify_provider_parity(
    bytes: &[u8],
) -> Result<ProviderParityReport, ProviderVerificationError> {
    verify_known_answers::<RustCryptoSha256>()?;
    verify_known_answers::<LibcruxSha256>()?;

    compare_hashes(RustCryptoSha256::hash(bytes), LibcruxSha256::hash(bytes))?;

    let domain = Domain::new("zeno-fcis/parity", 1)
        .map_err(|_| ProviderVerificationError::SemanticEncoding)?;
    let preimage =
        domain_preimage(domain, bytes).map_err(|_| ProviderVerificationError::SemanticEncoding)?;
    compare_hashes(
        RustCryptoSha256::hash(&preimage),
        LibcruxSha256::hash(&preimage),
    )?;

    let bytes_checked =
        u64::try_from(bytes.len()).map_err(|_| ProviderVerificationError::LengthOverflow)?;
    Ok(ProviderParityReport {
        left_algorithm_id: RustCryptoSha256::ALGORITHM_ID,
        right_algorithm_id: LibcruxSha256::ALGORITHM_ID,
        bytes_checked,
    })
}

#[cfg(feature = "parity")]
fn compare_hashes(left: Hash32, right: Hash32) -> Result<(), ProviderVerificationError> {
    if left == right {
        Ok(())
    } else {
        Err(ProviderVerificationError::ProviderMismatch { left, right })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "rustcrypto")]
    #[test]
    fn rustcrypto_matches_fixed_vectors() {
        let report = verify_known_answers::<RustCryptoSha256>();
        assert!(report.is_ok());
    }

    #[cfg(feature = "libcrux")]
    #[test]
    fn libcrux_matches_fixed_vectors() {
        let report = verify_known_answers::<LibcruxSha256>();
        assert!(report.is_ok());
    }

    #[cfg(feature = "parity")]
    #[test]
    fn independent_providers_match_on_boundary_inputs() {
        for input in [
            &b""[..],
            &b"a"[..],
            &b"abc"[..],
            &[0_u8; 55][..],
            &[0_u8; 56][..],
            &[0_u8; 63][..],
            &[0_u8; 64][..],
            &[0_u8; 65][..],
            &[0xff_u8; 257][..],
        ] {
            assert!(verify_provider_parity(input).is_ok());
        }
    }

    #[cfg(feature = "rustcrypto")]
    #[test]
    fn semantic_domain_vector_is_stable() {
        let domain = match Domain::new("zeno-fcis/test", 1) {
            Ok(domain) => domain,
            Err(error) => panic!("fixed domain rejected: {error}"),
        };
        let observed = match commitment::<RustCryptoSha256>(domain, b"abc") {
            Ok(observed) => observed,
            Err(error) => panic!("fixed commitment rejected: {error}"),
        };
        assert_eq!(observed, DOMAIN_DIGEST);
    }
}
