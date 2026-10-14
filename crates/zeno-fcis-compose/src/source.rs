//! The source-bound identity of this crate.
//!
//! [`source_digest`] is the SHA-256 of every file this library is compiled
//! from: each file's path relative to the crate, its length and its exact
//! bytes, framed so that no two file lists frame the same bytes. A checker
//! that lets this crate judge a composition's structure records the digest,
//! so a change to these rules changes that checker's identity. The test
//! `tests/source_identity.rs` walks the crate directory and fails when a file
//! is neither listed here nor reviewed out as a test or package metadata.

use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};

const TAG: &[u8] = b"ZENO-FCIS-COMPOSE-SOURCES\0";
const VERSION: u32 = 1;

/// Every file this library is compiled from, relative to the crate, with its
/// exact bytes, in path order.
pub const SOURCE_FILES: &[(&str, &[u8])] = &[
    ("Cargo.toml", include_bytes!("../Cargo.toml")),
    ("src/lib.rs", include_bytes!("lib.rs")),
    ("src/source.rs", include_bytes!("source.rs")),
];

/// The SHA-256 of [`SOURCE_FILES`], framed with each path and length.
#[must_use]
pub fn source_digest() -> [u8; 32] {
    let mut framed = TAG.to_vec();
    framed.extend_from_slice(&VERSION.to_be_bytes());
    frame(&mut framed, SOURCE_FILES.len());
    for (path, bytes) in SOURCE_FILES {
        frame(&mut framed, path.len());
        framed.extend_from_slice(path.as_bytes());
        frame(&mut framed, bytes.len());
        framed.extend_from_slice(bytes);
    }
    *RustCryptoSha256::hash(&framed).as_bytes()
}

fn frame(out: &mut Vec<u8>, length: usize) {
    out.extend_from_slice(&(length as u64).to_be_bytes());
}
