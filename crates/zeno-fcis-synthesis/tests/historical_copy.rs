//! The copied interpreter remains byte bound to its released source and recorded edits.
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

#[path = "test-data/historical_24_copy.rs"]
mod expected;

#[test]
fn copied_source_matches_release() {
    for (name, bytes, expected) in expected::SOURCES {
        let actual: String = RustCryptoSha256::hash(bytes)
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(&actual, expected, "copied source: {name}");
    }
}
