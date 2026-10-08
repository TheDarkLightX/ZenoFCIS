//! The relay protocol exercised through the durable-counter V2 contract: the
//! export stream, acknowledgment by delivery ID and payload hash, and each
//! refusal with no write.
use rusqlite::{Connection, types::Value as Cell};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_shell_sqlite::v2::{
    Error, V2SqliteShell,
    relay::{self, EXPORT_SCHEMA},
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition::{FrameBinding, Raw},
};
use zeno_fcis_value::{Field, Value};

/// Actual generated declarative contract; this is not an application evaluator.
#[allow(dead_code, missing_docs, unreachable_pub)]
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
mod durable;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}

fn envelope(binding: FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}

fn counter(count: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(110, Value::signed(count)),
        Field::new(111, Value::signed(0)),
    ]));
    envelope(durable::FRAMING.state, value)
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "zenofcis-relay-{}-{}.db",
            std::process::id(),
            FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

/// Every row of every table, read through a separate connection.
fn rows(file: &StoreFile) -> Vec<(String, Vec<Vec<Cell>>)> {
    let connection = ok(Connection::open(&file.0));
    let tables: Vec<String> = {
        let mut statement =
            ok(connection
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"));
        ok(ok(statement.query_map([], |r| r.get(0))).collect())
    };
    tables
        .into_iter()
        .map(|table| {
            let mut statement = ok(connection.prepare(&format!("SELECT * FROM \"{table}\"")));
            let columns = statement.column_count();
            let values: Vec<Vec<Cell>> =
                ok(ok(statement
                    .query_map([], |r| (0..columns).map(|i| r.get::<_, Cell>(i)).collect()))
                .collect());
            (table, values)
        })
        .collect()
}

/// A store with `deliveries` commits, each owning one pending delivery.
fn with_store(deliveries: i128, run: impl FnOnce(&Authority<'_>, &StoreFile)) {
    let contract = durable::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(durable::checked_authority(&descriptor));
    let file = StoreFile::new();
    let initial = counter(0);
    let PublicationOutcome::Commit(genesis) = authority.publish_genesis(&initial) else {
        panic!("genesis refused");
    };
    let mut shell = ok(V2SqliteShell::create(&file.0, &authority, genesis));
    let command = envelope(durable::FRAMING.command, Value::sum(101, 120, None));
    let context = envelope(durable::FRAMING.context, Value::boolean(true));
    for step in 0..deliveries {
        let state = ok(shell.snapshot()).state().to_vec();
        let PublicationOutcome::Commit(publication) = authority.publish(Raw {
            state: &state,
            command: &command,
            context: &context,
        }) else {
            panic!("publication {step} refused");
        };
        ok(shell.commit(Hash32::new([step as u8 + 1; 32]), publication));
    }
    assert_eq!(ok(shell.snapshot()).pending(), deliveries as u64);
    drop(shell);
    run(&authority, &file);
}

#[test]
fn export_lists_every_pending_delivery_in_commit_order_with_the_same_identity_each_time() {
    with_store(3, |authority, file| {
        let mut shell = ok(V2SqliteShell::open(&file.0, authority));
        let before = rows(file);
        let exported = ok(shell.export_pending());
        assert_eq!(exported.len(), 3);
        let order: Vec<u64> = exported.iter().map(|d| d.version()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        // Export writes nothing, and a second export, from a reopened
        // handle, repeats every line byte for byte.
        assert_eq!(rows(file), before);
        drop(shell);
        let mut reopened = ok(V2SqliteShell::open(&file.0, authority));
        let again = ok(reopened.export_pending());
        assert_eq!(again, exported);
        let lines: Vec<String> = exported.iter().map(relay::export_line).collect();
        let repeated: Vec<String> = again.iter().map(relay::export_line).collect();
        assert_eq!(lines, repeated);
        for (delivery, line) in exported.iter().zip(&lines) {
            assert!(line.starts_with(&format!("{{\"schema\":\"{EXPORT_SCHEMA}\"")));
            assert!(!line.contains('\n'));
            let id = relay::hex(delivery.delivery_id().as_bytes());
            let hash = relay::hex(relay::payload_sha256(delivery.payload()).as_bytes());
            assert!(line.contains(&format!("\"delivery_id\":\"{id}\"")));
            assert!(line.contains(&format!("\"payload_sha256\":\"{hash}\"")));
            assert!(line.contains(&format!(
                "\"payload\":\"{}\"",
                relay::hex(delivery.payload())
            )));
        }
        // The token lifecycle issues the same oldest entry first.
        let first = ok(reopened.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        assert_eq!(first.delivery(), &exported[0]);
    });
}

#[test]
fn acknowledgment_by_id_and_payload_hash_leaves_the_entry_out_of_later_exports() {
    with_store(2, |authority, file| {
        let mut shell = ok(V2SqliteShell::open(&file.0, authority));
        let exported = ok(shell.export_pending());
        // Out of commit order: the relay may acknowledge any exported entry.
        let second = &exported[1];
        ok(relay::acknowledge(
            &mut shell,
            second.delivery_id(),
            relay::payload_sha256(second.payload()),
        ));
        assert_eq!(ok(shell.export_pending()), vec![exported[0].clone()]);
        assert_eq!(ok(shell.snapshot()).pending(), 1);
        ok(shell.audit());
    });
}

#[test]
fn forged_mismatched_and_repeated_acknowledgments_are_refused_by_name_and_write_nothing() {
    with_store(1, |authority, file| {
        let mut shell = ok(V2SqliteShell::open(&file.0, authority));
        let delivery = ok(shell.export_pending()).remove(0);
        let id = delivery.delivery_id();
        let hash = relay::payload_sha256(delivery.payload());
        let before = rows(file);
        // An ID the store never exported.
        let forged = Hash32::new([0xA5; 32]);
        assert!(matches!(
            relay::acknowledge(&mut shell, forged, hash),
            Err(Error::UnknownDelivery)
        ));
        // The entry hash or another payload's hash in place of the payload's.
        for wrong in [
            delivery.entry_hash(),
            relay::payload_sha256(b"another payload"),
            Hash32::ZERO,
        ] {
            assert!(matches!(
                relay::acknowledge(&mut shell, id, wrong),
                Err(Error::PayloadMismatch)
            ));
        }
        assert_eq!(rows(file), before);
        assert_eq!(ok(shell.export_pending()), vec![delivery.clone()]);
        ok(relay::acknowledge(&mut shell, id, hash));
        let acknowledged = rows(file);
        assert_ne!(acknowledged, before);
        // A repeat of the same, correct acknowledgment.
        assert!(matches!(
            relay::acknowledge(&mut shell, id, hash),
            Err(Error::AlreadyAcknowledged)
        ));
        assert!(matches!(
            shell.pending_by_id(id),
            Err(Error::AlreadyAcknowledged)
        ));
        assert_eq!(rows(file), acknowledged);
        assert!(ok(shell.export_pending()).is_empty());
    });
}

#[test]
fn an_acknowledgment_another_process_made_first_refuses_the_late_token() {
    with_store(1, |authority, file| {
        let mut first = ok(V2SqliteShell::open(&file.0, authority));
        let mut second = ok(V2SqliteShell::open(&file.0, authority));
        let delivery = ok(first.export_pending()).remove(0);
        let id = delivery.delivery_id();
        let hash = relay::payload_sha256(delivery.payload());
        // Both handles hold a relayed token for the entry; the second
        // acknowledges first.
        let late = ok(ok(first.pending_by_id(id)).relayed(hash));
        ok(relay::acknowledge(&mut second, id, hash));
        let acknowledged = rows(file);
        assert!(matches!(
            late.acknowledge(),
            Err(Error::AlreadyAcknowledged)
        ));
        // The late handle first audits the store the other connection wrote,
        // and that audit records its checkpoint, as every audit does. The
        // refusal itself changes no other row.
        let without_checkpoints = |all: Vec<(String, Vec<Vec<Cell>>)>| {
            all.into_iter()
                .filter(|(table, _)| table != "v2_checkpoints")
                .collect::<Vec<_>>()
        };
        assert_eq!(
            without_checkpoints(rows(file)),
            without_checkpoints(acknowledged)
        );
        ok(first.audit());
    });
}

#[test]
fn a_relayed_token_with_the_wrong_hash_is_never_issued() {
    with_store(1, |authority, file| {
        let mut shell = ok(V2SqliteShell::open(&file.0, authority));
        let delivery = ok(shell.export_pending()).remove(0);
        let pending = ok(shell.pending_by_id(delivery.delivery_id()));
        assert!(matches!(
            pending.relayed(delivery.entry_hash()),
            Err(Error::PayloadMismatch)
        ));
        assert_eq!(ok(shell.snapshot()).pending(), 1);
    });
}
