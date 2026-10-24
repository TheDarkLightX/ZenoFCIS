//! Import-only fingerprints of the preceding collection application implementation.
//! They never bind a current store or grant decision/publication authority.
//! Inputs are the emitted library template and application/storage sources in
//! the preceding profile; all imported submissions still undergo full replay.
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_synthesis::finite::v2_authority::Authority;

pub(super) fn identity(authority: &Authority<'_>) -> Hash32 {
    RustCryptoSha256::hash_parts(&[
        &[
            205, 212, 142, 183, 42, 28, 113, 203, 20, 184, 112, 178, 249, 53, 70, 23, 231, 206, 92,
            54, 125, 102, 166, 82, 18, 212, 168, 249, 53, 21, 57, 181,
        ],
        &zeno_fcis_contract::collections::runtime::source_digest(),
        &[
            120, 214, 174, 93, 192, 25, 144, 244, 172, 195, 82, 13, 203, 237, 64, 24, 132, 183,
            111, 155, 114, 218, 127, 130, 78, 180, 109, 10, 112, 85, 141, 209,
        ],
        b"ZFCIS-COLLECTION-APP/2\0",
        authority.identity(),
        super::PROJECT.as_bytes(),
        super::POLICY.as_bytes(),
    ])
}
