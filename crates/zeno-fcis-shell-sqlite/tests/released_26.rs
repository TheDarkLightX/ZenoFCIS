//! Stores created by the released 2.6.0 build are refused by name, unchanged.
//!
//! The retained stores come from `tools/check_contract_upgrade_26.py`: the
//! 2.6.0 CLI generated and ran the withdrawal-queue contract at version 1.
//! Their identity differs from this build's only in the trailing evaluator
//! digest, so neither version of the current lineage replays them.
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zeno_fcis_shell_sqlite::v2::{Error, Lineage, Step};
#[path = "../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted-2.5/src/v2_contract.rs"]
/// Current metadata of the same contract, adopted under this build.
pub mod adopted;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

struct StoreFile(PathBuf);
impl StoreFile {
    fn released(dump: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let file = Self(std::env::temp_dir().join(format!(
            "released-26-store-{}-{}.sqlite",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        let connection = ok(Connection::open(&file.0));
        ok(connection.execute_batch(dump));
        file
    }
    fn bytes(&self) -> Vec<u8> {
        ok(std::fs::read(&self.0))
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

const GENESIS: &str = include_str!("test-data/released-26/genesis.sql");
const PENDING: &str = include_str!("test-data/released-26/pending.sql");

fn receipt() -> [u8; 32] {
    let text = adopted::ADOPTION_RECEIPTS[0];
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ok(u8::from_str_radix(&text[index * 2..index * 2 + 2], 16));
    }
    bytes
}

/// The current contract alone (version 1), then with its adoption (both).
fn with_lineages(f: impl Fn(&Lineage<'_, '_>)) {
    ok(adopted::with_lineage(|(catalogs, _)| {
        f(&ok(Lineage::bind_steps(&catalogs[..1], &[])));
        f(&ok(Lineage::bind_steps(
            catalogs,
            &[Step::Adoption(receipt())],
        )));
    }));
}

#[test]
fn retained_inputs_match_the_release_record() {
    use zeno_fcis_codec::CommitmentHasher;
    let manifest = include_str!("test-data/released-26/manifest.json");
    assert!(manifest.contains("\"release\": \"2.6.0\""));
    for (name, text) in [("genesis.sql", GENESIS), ("pending.sql", PENDING)] {
        let hash = zeno_fcis_crypto::RustCryptoSha256::hash(text.as_bytes());
        let hex: String = hash.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert!(
            manifest.contains(&format!("\"{name}\": \"{hex}\"")),
            "{name}"
        );
    }
}

#[test]
fn released_stores_are_refused_by_name_and_left_unchanged() {
    for dump in [GENESIS, PENDING] {
        let file = StoreFile::released(dump);
        let before = file.bytes();
        with_lineages(|lineage| {
            assert!(matches!(
                lineage.audit_read_only(&file.0),
                Err(Error::OtherRelease)
            ));
            assert!(matches!(lineage.open(&file.0), Err(Error::OtherRelease)));
        });
        assert_eq!(file.bytes(), before);
    }
}

#[test]
fn another_contract_is_still_a_different_contract() {
    let file = StoreFile::released(PENDING);
    let connection = ok(Connection::open(&file.0));
    let mut identity: Vec<u8> = ok(connection.query_row(
        "SELECT identity FROM v2_genesis WHERE singleton=1",
        [],
        |row| row.get(0),
    ));
    // A byte of the contract, before the evaluator digest, differs.
    let index = identity.len() - 33;
    identity[index] ^= 1;
    ok(connection.execute(
        "UPDATE v2_genesis SET identity=?1 WHERE singleton=1",
        [identity],
    ));
    with_lineages(|lineage| {
        assert!(matches!(lineage.open(&file.0), Err(Error::Identity)));
    });
}
