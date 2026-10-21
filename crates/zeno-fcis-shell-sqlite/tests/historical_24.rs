//! Replay actual released stores before and after the checked forward upgrade.
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zeno_fcis_shell_sqlite::v2::{Lineage, Opened, Step, Store};
use zeno_fcis_synthesis::finite::{
    V2Resource,
    historical::{self, HistoricalContext},
};
#[path = "../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/src/v2_contract.rs"]
/// Current metadata for the retained adopted contract.
pub mod adopted;
#[path = "../../zeno-fcis-synthesis/tests/test-data/released_24_contract.rs"]
/// Original released declaration data for the retained predecessor.
pub mod released;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

struct StoreFile(PathBuf);
impl StoreFile {
    fn pending() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let file = Self(std::env::temp_dir().join(format!(
            "released-24-store-{}-{}.sqlite",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        let connection = ok(Connection::open(&file.0));
        ok(connection.execute_batch(include_str!(
            "../../zeno-fcis-synthesis/tests/test-data/released-24/pending.sql"
        )));
        file
    }
    fn pending_ids(&self) -> Vec<Vec<u8>> {
        let connection = ok(Connection::open(&self.0));
        let mut query = ok(connection.prepare("SELECT delivery_id FROM v2_deliveries WHERE acknowledged=0 ORDER BY sequence,lane,ordinal"));
        ok(ok(query.query_map([], |row| row.get(0))).collect())
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn receipt() -> [u8; 32] {
    let text = adopted::ADOPTION_RECEIPTS[0];
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ok(u8::from_str_radix(&text[index * 2..index * 2 + 2], 16));
    }
    bytes
}

fn with_receipt(receipt: [u8; 32], f: impl FnOnce(&Lineage<'_, '_>, &HistoricalContext<'_>)) {
    let data = released::Contract::new();
    let descriptor = data.descriptor();
    let catalog = ok(released::checked_catalog(&descriptor));
    let context = ok(historical::bind_released_24(&catalog));
    ok(adopted::with_lineage(|(catalogs, _)| {
        let contexts = [Some(&context), None];
        let steps = [Step::Adoption(receipt)];
        let lineage = ok(Lineage::bind_steps_with_history(
            catalogs, &steps, &contexts,
        ));
        f(&lineage, &context);
    }));
}

#[test]
fn historical_non_genesis_replay() {
    let file = StoreFile::pending();
    with_receipt(receipt(), |lineage, _| {
        let (version, snapshot) = ok(lineage.audit_read_only(&file.0));
        assert_eq!(version, 1);
        assert_eq!(snapshot.version(), 3);
        assert_eq!(snapshot.pending(), 1);
    });
}

#[test]
fn historical_laws_and_usage() {
    with_receipt(receipt(), |_, context| {
        let state = include_bytes!(
            "../../zeno-fcis-synthesis/tests/test-data/released-24/pending-3-state.bin"
        );
        let command = include_bytes!(
            "../../zeno-fcis-synthesis/tests/test-data/released-24/pending-3-command.bin"
        );
        let input = include_bytes!(
            "../../zeno-fcis-synthesis/tests/test-data/released-24/pending-3-context.bin"
        );
        let publication = include_bytes!(
            "../../zeno-fcis-synthesis/tests/test-data/released-24/pending-3-publication.bin"
        );
        let first = context.verify_publication(state, command, input, publication);
        let used = first.usage();
        assert!(used.used(V2Resource::Step) > context.program().nodes.len() as u64);
        assert_eq!(ok(first.into_result()).outbox().len(), 1);
        let refused = context.verify_publication(state, command, input, &[]);
        assert_eq!(refused.usage(), used);
        assert!(refused.into_result().is_err());
        let malformed = context.verify_publication(state, &[], input, publication);
        assert_eq!(malformed.usage().used(V2Resource::Step), 0);
        assert!(malformed.into_result().is_err());
        let again = context.verify_publication(state, command, input, publication);
        assert_eq!(again.usage(), used);
        assert_eq!(ok(again.into_result()).subject(), publication);
    });
}

#[test]
fn historical_pending_ids_survive_upgrade() {
    let file = StoreFile::pending();
    let before = file.pending_ids();
    assert_eq!(before.len(), 1);
    with_receipt(receipt(), |lineage, _| {
        let Opened::V10(Store::Superseded(store)) = ok(lineage.open(&file.0)) else {
            panic!("released store must require an explicit forward upgrade");
        };
        let (mut current, _) = ok(store.upgrade());
        ok(current.audit());
        assert_eq!(ok(current.snapshot()).pending(), 1);
        assert_eq!(file.pending_ids(), before);
        let (version, snapshot) = ok(lineage.audit_read_only(&file.0));
        assert_eq!(version, 2);
        assert_eq!(snapshot.pending(), 1);
    });
}

#[test]
fn tampered_historical_lineage_refused() {
    let file = StoreFile::pending();
    with_receipt(receipt(), |lineage, _| {
        let connection = ok(Connection::open(&file.0));
        let mut bytes: Vec<u8> = ok(connection.query_row(
            "SELECT publication FROM v2_commits WHERE sequence=2",
            [],
            |row| row.get(0),
        ));
        let end = bytes.len() - 1;
        bytes[end] ^= 1;
        ok(connection.execute(
            "UPDATE v2_commits SET publication=?1 WHERE sequence=2",
            [bytes],
        ));
        assert!(lineage.audit_read_only(&file.0).is_err());
        assert!(lineage.open(&file.0).is_err());
        let upgrades: i64 =
            ok(connection.query_row("SELECT count(*) FROM v2_upgrades", [], |row| row.get(0)));
        assert_eq!(upgrades, 0);
    });
    let file = StoreFile::pending();
    with_receipt(receipt(), |lineage, _| {
        let Opened::V10(Store::Superseded(store)) = ok(lineage.open(&file.0)) else {
            panic!("released predecessor");
        };
        ok(store.upgrade());
    });
    let mut forged = receipt();
    forged[0] ^= 1;
    with_receipt(forged, |lineage, _| {
        assert!(lineage.audit_read_only(&file.0).is_err())
    });
}
