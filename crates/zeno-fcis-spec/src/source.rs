//! The source-bound identity of this crate.
//!
//! [`source_digest`] is the SHA-256 of every file this library is compiled
//! from: each file's path relative to the crate, its length and its exact
//! bytes, framed so that no two file lists frame the same bytes. A checker
//! that compiles `.zeno` source with this crate records the digest, so a
//! change to the parser changes that checker's identity. The test
//! `tests/source_identity.rs` walks the crate directory and fails when a file
//! is neither listed here nor reviewed out as a test, an example or package
//! metadata.

use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};

const TAG: &[u8] = b"ZENO-FCIS-SPEC-SOURCES\0";
const VERSION: u32 = 1;

/// Every file this library is compiled from, relative to the crate, with its
/// exact bytes, in path order.
pub const SOURCE_FILES: &[(&str, &[u8])] = &[
    ("Cargo.toml", include_bytes!("../Cargo.toml")),
    (
        "grammar/zeno-v1.json",
        include_bytes!("../grammar/zeno-v1.json"),
    ),
    ("src/ast.rs", include_bytes!("ast.rs")),
    ("src/builder.rs", include_bytes!("builder.rs")),
    ("src/checked_parser.rs", include_bytes!("checked_parser.rs")),
    ("src/diagnostic.rs", include_bytes!("diagnostic.rs")),
    ("src/elaborate.rs", include_bytes!("elaborate.rs")),
    ("src/induction.rs", include_bytes!("induction.rs")),
    ("src/lexer.rs", include_bytes!("lexer.rs")),
    ("src/lexical_core.rs", include_bytes!("lexical_core.rs")),
    ("src/lib.rs", include_bytes!("lib.rs")),
    (
        "src/mini_determinator.rs",
        include_bytes!("mini_determinator.rs"),
    ),
    ("src/parser.rs", include_bytes!("parser.rs")),
    ("src/paths.rs", include_bytes!("paths.rs")),
    (
        "src/semantic_readers.rs",
        include_bytes!("semantic_readers.rs"),
    ),
    ("src/source.rs", include_bytes!("source.rs")),
    ("src/substance.rs", include_bytes!("substance.rs")),
    ("src/syntax.rs", include_bytes!("syntax.rs")),
    (
        "src/syntax_certificate.rs",
        include_bytes!("syntax_certificate.rs"),
    ),
    ("src/syntax_grammar.rs", include_bytes!("syntax_grammar.rs")),
    (
        "src/syntax_lowering.rs",
        include_bytes!("syntax_lowering.rs"),
    ),
    ("src/views.rs", include_bytes!("views.rs")),
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
