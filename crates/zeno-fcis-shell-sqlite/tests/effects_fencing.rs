//! Real dispatcher and SQLite clone/restore observations with a test receiver.
//!
//! The receiver's monotone fence and the adapter's immutable, trusted token are
//! fixture premises. `TrustedDispatcher` does not bind or attest this token.
//! A delayed request is held by the fixture receiver, not an actual network.
#![cfg(feature = "crash-injection")]

#[allow(dead_code, unused_imports)]
mod sim;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, Transaction, params};
use sim::{
    database::{self, Files},
    fixture, key, model, ok,
};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_shell::effects::{
    policy::Policy,
    port::{Code, Delivery, Fault, Report, Sent},
};
use zeno_fcis_shell_sqlite::{
    effects::{
        dispatcher::{Configuration, Error, Limits, Refusal, TrustedDispatcher},
        production::{AdapterProvenance, Transport},
    },
    v2::{Lineage, relay},
};

fn config(epoch: u64) -> Configuration {
    Configuration {
        deployment: key(10),
        tenant: key(20),
        // This is a transcript epoch, not the receiver fence.
        epoch,
        limits: Limits {
            grants: 4,
            intent_bytes: 16_384,
            dispatches: 16,
        },
    }
}

fn policies() -> [Policy; 1] {
    [Policy::AtMostOne { capacity: 2 }]
}

fn with_lineage(run: impl FnOnce(&Lineage<'_, '_>)) {
    fixture::with_catalog(|catalog| run(&ok(Lineage::bind(&[catalog], &[]))));
}

fn seed(files: &Files, lineage: &Lineage<'_, '_>) -> Vec<Delivery> {
    let authority = &lineage.authorities()[0];
    let mut db = sim::create(files, authority);
    let input = fixture::input(authority, key(1), [0, 0], 120);
    assert!(matches!(
        sim::commit(&mut db, authority, &input, 0),
        model::Outcome::Committed(1, _)
    ));
    ok(db.export_pending())
        .iter()
        .map(|d| Delivery {
            id: d.delivery_id(),
            version: d.version(),
            lane: d.lane(),
            ordinal: d.ordinal(),
            destination: d.destination().to_vec(),
            payload: d.payload().to_vec(),
            payload_sha256: relay::payload_sha256(d.payload()),
        })
        .collect()
}

fn reservation(path: &Path) -> Vec<u8> {
    let c = ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ));
    ok(c.query_row(
        "SELECT bytes FROM effect_reservation WHERE id = 1",
        [],
        |r| r.get(0),
    ))
}

// Copy only closed, complete SQLite files. Refuse an uncollected journal/WAL.
fn copy_closed_db(source: &Path, destination: &Path) {
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
        assert!(
            !sidecar.exists(),
            "uncollected SQLite sidecar: {}",
            sidecar.display()
        );
    }
    let c = ok(Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ));
    let check: String = ok(c.query_row("PRAGMA quick_check", [], |r| r.get(0)));
    assert_eq!(check, "ok");
    drop(c);
    assert!(!destination.exists());
    ok(fs::copy(source, destination));
    assert_eq!(ok(fs::read(source)), ok(fs::read(destination)));
}

struct LocalFence(PathBuf);
impl LocalFence {
    fn create(files: &Files) -> Self {
        let path = files.store.with_extension("local-fence.db");
        let c = ok(Connection::open(&path));
        ok(c.execute_batch("CREATE TABLE local_fence (id INTEGER PRIMARY KEY, token INTEGER NOT NULL); INSERT INTO local_fence VALUES (1, 1);"));
        drop(c);
        Self(path)
    }
    fn copied(source: &Self, files: &Files) -> Self {
        let path = files.store.with_extension("local-fence.db");
        copy_closed_db(&source.0, &path);
        Self(path)
    }
    fn read(&self) -> i64 {
        read_local(&self.0)
    }
    fn revalidate(&self, receiver: &Receiver) {
        let current = receiver.view().fence;
        let c = ok(Connection::open(&self.0));
        assert_eq!(
            ok(c.execute(
                "UPDATE local_fence SET token=?1 WHERE id=1 AND token=1",
                [current]
            )),
            1
        );
    }
}
impl Drop for LocalFence {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(format!("{}-journal", self.0.display()));
    }
}

fn read_local(path: &Path) -> i64 {
    let c = ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ));
    let count: i64 = ok(c.query_row("SELECT COUNT(*) FROM local_fence", [], |r| r.get(0)));
    assert_eq!(count, 1);
    ok(c.query_row("SELECT token FROM local_fence WHERE id=1", [], |r| r.get(0)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Guard {
    External,
    RestoredLocal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FencedDelivery {
    token: i64,
    delivery: Delivery,
}

#[derive(Debug, Eq, PartialEq)]
struct ReceiverView {
    fence: i64,
    queued: Vec<FencedDelivery>,
    applied: Vec<FencedDelivery>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiverResult {
    Applied,
    Duplicate,
    Stale,
}

struct Receiver {
    path: PathBuf,
    guard: Guard,
}
fn receiver_connection(path: &Path) -> Connection {
    let c = ok(Connection::open(path));
    ok(c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;"));
    c
}
impl Receiver {
    fn create(path: &Path, guard: Guard) -> Self {
        let c = receiver_connection(path);
        ok(c.execute_batch("
            CREATE TABLE remote_fence (id INTEGER PRIMARY KEY, token INTEGER NOT NULL);
            INSERT INTO remote_fence VALUES (1, 1);
            CREATE TABLE queued (id BLOB PRIMARY KEY, token INTEGER NOT NULL, version INTEGER NOT NULL, lane INTEGER NOT NULL, ordinal INTEGER NOT NULL, destination BLOB NOT NULL, payload BLOB NOT NULL, payload_hash BLOB NOT NULL);
            CREATE TABLE applied (id BLOB PRIMARY KEY, token INTEGER NOT NULL, version INTEGER NOT NULL, lane INTEGER NOT NULL, ordinal INTEGER NOT NULL, destination BLOB NOT NULL, payload BLOB NOT NULL, payload_hash BLOB NOT NULL);"));
        Self {
            path: path.to_path_buf(),
            guard,
        }
    }
    fn advance(&self) {
        let c = receiver_connection(&self.path);
        assert_eq!(
            ok(c.execute("UPDATE remote_fence SET token=2 WHERE id=1 AND token=1", [])),
            1
        );
    }
    fn view(&self) -> ReceiverView {
        let c = ok(Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        ));
        let names = ok(ok(c.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"))
            .query_map([], |r| r.get::<_, String>(0))).map(ok).collect::<Vec<_>>();
        assert_eq!(names, ["applied", "queued", "remote_fence"]);
        let count: i64 = ok(c.query_row("SELECT COUNT(*) FROM remote_fence", [], |r| r.get(0)));
        assert_eq!(count, 1);
        ReceiverView {
            fence: ok(
                c.query_row("SELECT token FROM remote_fence WHERE id=1", [], |r| {
                    r.get(0)
                }),
            ),
            queued: read_rows(&c, "queued"),
            applied: read_rows(&c, "applied"),
        }
    }
    fn stage(&self, item: &FencedDelivery) {
        let c = receiver_connection(&self.path);
        insert(&c, "queued", item);
    }
    fn send(&self, item: &FencedDelivery, local: &Path) -> ReceiverResult {
        let mut c = receiver_connection(&self.path);
        let tx = ok(c.transaction());
        let result = apply(&tx, self.guard, local, item);
        ok(tx.commit());
        result
    }
    fn finish(&self, id: Hash32, local: &Path) -> ReceiverResult {
        let mut c = receiver_connection(&self.path);
        let tx = ok(c.transaction());
        let item = ok(tx.query_row(
            "SELECT id, token, version, lane, ordinal, destination, payload, payload_hash FROM queued WHERE id=?1",
            [&id.as_bytes()[..]], read_item));
        assert_eq!(
            ok(tx.execute("DELETE FROM queued WHERE id=?1", [&id.as_bytes()[..]])),
            1
        );
        let result = apply(&tx, self.guard, local, &item);
        ok(tx.commit());
        result
    }
}

fn read_item(row: &Row<'_>) -> rusqlite::Result<FencedDelivery> {
    let id: Vec<u8> = row.get(0)?;
    let payload_hash: Vec<u8> = row.get(7)?;
    let version: i64 = row.get(2)?;
    let lane: i64 = row.get(3)?;
    let ordinal: i64 = row.get(4)?;
    Ok(FencedDelivery {
        token: row.get(1)?,
        delivery: Delivery {
            id: Hash32::new(
                id.try_into()
                    .unwrap_or_else(|_| panic!("delivery id length")),
            ),
            version: u64::try_from(version).unwrap_or_else(|_| panic!("version")),
            lane: u8::try_from(lane).unwrap_or_else(|_| panic!("lane")),
            ordinal: u32::try_from(ordinal).unwrap_or_else(|_| panic!("ordinal")),
            destination: row.get(5)?,
            payload: row.get(6)?,
            payload_sha256: Hash32::new(
                payload_hash
                    .try_into()
                    .unwrap_or_else(|_| panic!("payload hash length")),
            ),
        },
    })
}

fn read_rows(c: &Connection, table: &str) -> Vec<FencedDelivery> {
    assert!(matches!(table, "queued" | "applied"));
    let sql = format!(
        "SELECT id, token, version, lane, ordinal, destination, payload, payload_hash FROM {table} ORDER BY id"
    );
    ok(ok(c.prepare(&sql)).query_map([], read_item))
        .map(ok)
        .collect()
}

fn insert(c: &Connection, table: &str, item: &FencedDelivery) {
    assert!(matches!(table, "queued" | "applied"));
    let sql = format!("INSERT INTO {table} VALUES (?1,?2,?3,?4,?5,?6,?7,?8)");
    let d = &item.delivery;
    assert_eq!(
        ok(c.execute(
            &sql,
            params![
                &d.id.as_bytes()[..],
                item.token,
                i64::try_from(d.version).unwrap_or_else(|_| panic!("version")),
                i64::from(d.lane),
                i64::from(d.ordinal),
                &d.destination,
                &d.payload,
                &d.payload_sha256.as_bytes()[..]
            ]
        )),
        1
    );
}

fn apply(
    tx: &Transaction<'_>,
    guard: Guard,
    local: &Path,
    item: &FencedDelivery,
) -> ReceiverResult {
    let current: i64 = match guard {
        Guard::External => ok(
            tx.query_row("SELECT token FROM remote_fence WHERE id=1", [], |r| {
                r.get(0)
            }),
        ),
        Guard::RestoredLocal => read_local(local),
    };
    if item.token != current {
        return ReceiverResult::Stale;
    }
    let existing = ok(tx.query_row(
        "SELECT id, token, version, lane, ordinal, destination, payload, payload_hash FROM applied WHERE id=?1",
        [&item.delivery.id.as_bytes()[..]], read_item).optional());
    if let Some(existing) = existing {
        assert_eq!(
            existing.delivery, item.delivery,
            "receiver delivery-id collision"
        );
        return ReceiverResult::Duplicate;
    }
    insert(tx, "applied", item);
    ReceiverResult::Applied
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationKind {
    Staged,
    Applied,
    Duplicate,
    Stale,
}
#[derive(Clone, Debug)]
struct Observation {
    item: FencedDelivery,
    reservation: Vec<u8>,
    kind: ObservationKind,
}
type Calls = Rc<RefCell<Vec<Observation>>>;

struct Wire {
    remote: Receiver,
    local: PathBuf,
    reservation: PathBuf,
    token: i64,
    stage_next: bool,
    calls: Calls,
}
impl Wire {
    fn new(
        files: &Files,
        local: &LocalFence,
        remote: &Receiver,
        stage_next: bool,
        calls: &Calls,
    ) -> Self {
        Self {
            remote: Receiver {
                path: remote.path.clone(),
                guard: remote.guard,
            },
            local: local.0.clone(),
            reservation: files.receiver.clone(),
            token: local.read(),
            stage_next,
            calls: Rc::clone(calls),
        }
    }
}
impl Transport for Wire {
    fn provenance(&self) -> Option<AdapterProvenance> {
        let mut configuration = self.token.to_be_bytes().to_vec();
        configuration.extend_from_slice(&[
            u8::from(self.remote.guard == Guard::External),
            u8::from(self.stage_next),
        ]);
        Some(AdapterProvenance {
            source: RustCryptoSha256::hash(include_bytes!("effects_fencing.rs")),
            configuration,
            seed: 0,
        })
    }
    fn send(&mut self, delivery: &Delivery) -> Sent {
        let item = FencedDelivery {
            token: self.token,
            delivery: delivery.clone(),
        };
        let reservation = reservation(&self.reservation);
        let kind = if self.stage_next {
            self.stage_next = false;
            self.remote.stage(&item);
            ObservationKind::Staged
        } else {
            match self.remote.send(&item, &self.local) {
                ReceiverResult::Applied => ObservationKind::Applied,
                ReceiverResult::Duplicate => ObservationKind::Duplicate,
                ReceiverResult::Stale => ObservationKind::Stale,
            }
        };
        self.calls.borrow_mut().push(Observation {
            item,
            reservation,
            kind,
        });
        match kind {
            ObservationKind::Applied | ObservationKind::Duplicate => Sent::Sent(Report {
                delivery_id: delivery.id,
                payload_sha256: delivery.payload_sha256,
            }),
            // The current Sent vocabulary cannot express receiver-side stale
            // rejection after a request was transmitted; do not claim NotSent.
            ObservationKind::Staged | ObservationKind::Stale => Sent::Unknown(Fault::Io),
        }
    }
}

fn open<'a, 'p>(
    files: &Files,
    lineage: &'a Lineage<'a, 'p>,
    wire: Wire,
    epoch: u64,
) -> TrustedDispatcher<'a, 'p, Wire> {
    ok(TrustedDispatcher::open(
        &files.store,
        &files.receiver,
        lineage,
        wire,
        config(epoch),
        &policies(),
    ))
}

fn all(path: &Path) -> model::Database {
    database::snapshot_matching(path, "*")
}

fn run(guard: Guard) {
    with_lineage(|lineage| {
        let original = Files::new();
        let first = Files::new();
        let second = Files::new();
        let external_files = Files::new();
        let remote = Receiver::create(&external_files.receiver, guard);
        let original_local = LocalFence::create(&original);
        let deliveries = seed(&original, lineage);
        assert!(deliveries.len() >= 2);
        assert_ne!(deliveries[0].id, deliveries[1].id);
        let unused = Calls::default();
        let owner = ok(TrustedDispatcher::create(
            &original.store,
            &original.receiver,
            lineage,
            Wire::new(&original, &original_local, &remote, false, &unused),
            config(30),
            &policies(),
        ));
        drop(owner);
        copy_closed_db(&original.store, &first.store);
        copy_closed_db(&original.receiver, &first.receiver);
        copy_closed_db(&original.store, &second.store);
        copy_closed_db(&original.receiver, &second.receiver);
        let local_first = LocalFence::copied(&original_local, &first);
        let local_second = LocalFence::copied(&original_local, &second);
        let old_calls = Calls::default();
        let new_calls = Calls::default();
        // Both restored copies open simultaneously because their files differ.
        let mut old = open(
            &first,
            lineage,
            Wire::new(&first, &local_first, &remote, true, &old_calls),
            30,
        );
        let mut restored = open(
            &second,
            lineage,
            Wire::new(&second, &local_second, &remote, false, &new_calls),
            31,
        );
        let old_h0 = ok(old.grant(key(40), deliveries[0].id));
        let old_h1 = ok(old.grant(key(40), deliveries[1].id));
        let restored_old_h0 = ok(restored.grant(key(40), deliveries[0].id));
        let old_app = all(&first.store);
        let restored_app = all(&second.store);
        let old_initial = all(&first.receiver);
        let restored_initial = all(&second.receiver);
        assert_eq!(
            ok(old.session(key(20), key(40)).dispatch(&old_h0)),
            Sent::Unknown(Fault::Io)
        );
        let old_reserved = all(&first.receiver);
        assert_ne!(old_reserved, old_initial);
        assert_eq!(all(&first.store), old_app);
        assert_eq!(all(&second.store), restored_app);
        assert_eq!(all(&second.receiver), restored_initial);
        let staged = FencedDelivery {
            token: 1,
            delivery: deliveries[0].clone(),
        };
        assert_eq!(
            remote.view(),
            ReceiverView {
                fence: 1,
                queued: vec![staged.clone()],
                applied: vec![]
            }
        );
        assert_eq!(old_calls.borrow().len(), 1);
        assert_eq!(old_calls.borrow()[0].item, staged);
        assert_eq!(old_calls.borrow()[0].kind, ObservationKind::Staged);
        assert_eq!(
            old_calls.borrow()[0].reservation,
            reservation(&first.receiver)
        );

        remote.advance();
        assert_eq!(local_first.read(), 1);
        assert_eq!(local_second.read(), 1);
        assert_eq!(
            remote.view(),
            ReceiverView {
                fence: 2,
                queued: vec![staged.clone()],
                applied: vec![]
            }
        );
        let outcome = remote.finish(deliveries[0].id, &local_first.0);
        let expected = if guard == Guard::External {
            ReceiverResult::Stale
        } else {
            ReceiverResult::Applied
        };
        assert_eq!(outcome, expected);
        let applied = if guard == Guard::External {
            vec![]
        } else {
            vec![staged.clone()]
        };
        assert_eq!(
            remote.view(),
            ReceiverView {
                fence: 2,
                queued: vec![],
                applied
            }
        );
        assert_eq!(all(&first.store), old_app);
        assert_eq!(all(&first.receiver), old_reserved);
        assert_eq!(all(&second.store), restored_app);
        assert_eq!(all(&second.receiver), restored_initial);
        assert_eq!(
            ok(old.session(key(20), key(40)).dispatch(&old_h0)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(old_calls.borrow().len(), 1);
        assert_eq!(all(&first.receiver), old_reserved);

        // In the negative control, the stale request really changed the
        // independent receiver. This is an expected fixture violation.
        if guard == Guard::RestoredLocal {
            return;
        }

        assert_eq!(
            ok(old.session(key(20), key(40)).dispatch(&old_h1)),
            Sent::Unknown(Fault::Io)
        );
        let old_reserved_both = all(&first.receiver);
        assert_ne!(old_reserved_both, old_reserved);
        assert_eq!(old_calls.borrow().len(), 2);
        assert_eq!(
            old_calls.borrow()[1].item,
            FencedDelivery {
                token: 1,
                delivery: deliveries[1].clone()
            }
        );
        assert_eq!(old_calls.borrow()[1].kind, ObservationKind::Stale);
        assert_eq!(
            old_calls.borrow()[1].reservation,
            reservation(&first.receiver)
        );
        assert_eq!(
            remote.view(),
            ReceiverView {
                fence: 2,
                queued: vec![],
                applied: vec![]
            }
        );
        assert_eq!(
            ok(old.session(key(20), key(40)).dispatch(&old_h1)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(all(&first.receiver), old_reserved_both);
        assert_eq!(old_calls.borrow().len(), 2);

        // Revalidation is supplied by trusted fixture administration, not by
        // the current dispatcher API. It reopens a copied reservation store.
        drop(restored);
        local_second.revalidate(&remote);
        assert_eq!(local_second.read(), 2);
        assert_eq!(local_first.read(), 1);
        let mut fresh = open(
            &second,
            lineage,
            Wire::new(&second, &local_second, &remote, false, &new_calls),
            32,
        );
        assert!(matches!(
            fresh.session(key(20), key(40)).dispatch(&restored_old_h0),
            Err(Error::Refused(Refusal::Owner))
        ));
        let fresh_h0 = ok(fresh.grant(key(40), deliveries[0].id));
        let fresh_app = all(&second.store);
        let fresh_before = all(&second.receiver);
        assert_eq!(
            ok(fresh.session(key(20), key(40)).dispatch(&fresh_h0)),
            Sent::Sent(Report {
                delivery_id: deliveries[0].id,
                payload_sha256: deliveries[0].payload_sha256,
            })
        );
        let fresh_reserved = all(&second.receiver);
        assert_ne!(fresh_reserved, fresh_before);
        assert_eq!(all(&second.store), fresh_app);
        assert_eq!(all(&first.store), old_app);
        assert_eq!(all(&first.receiver), old_reserved_both);
        assert_eq!(new_calls.borrow().len(), 1);
        assert_eq!(
            new_calls.borrow()[0].item,
            FencedDelivery {
                token: 2,
                delivery: deliveries[0].clone()
            }
        );
        assert_eq!(new_calls.borrow()[0].kind, ObservationKind::Applied);
        assert_eq!(
            new_calls.borrow()[0].reservation,
            reservation(&second.receiver)
        );
        let after = ReceiverView {
            fence: 2,
            queued: vec![],
            applied: vec![FencedDelivery {
                token: 2,
                delivery: deliveries[0].clone(),
            }],
        };
        assert_eq!(remote.view(), after);
        assert_eq!(
            ok(fresh.session(key(20), key(40)).dispatch(&fresh_h0)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(new_calls.borrow().len(), 1);
        assert_eq!(all(&second.receiver), fresh_reserved);
        assert_eq!(remote.view(), after);
        // Re-delivery of the exact request observed at the real Transport
        // boundary is independently deduplicated by the fixture receiver.
        let network_duplicate = new_calls.borrow()[0].item.clone();
        assert_eq!(
            remote.send(&network_duplicate, &local_second.0),
            ReceiverResult::Duplicate
        );
        assert_eq!(remote.view(), after);
        drop(fresh);
        let mut reopened = open(
            &second,
            lineage,
            Wire::new(&second, &local_second, &remote, false, &new_calls),
            33,
        );
        let again = ok(reopened.grant(key(40), deliveries[0].id));
        assert_eq!(
            ok(reopened.session(key(20), key(40)).dispatch(&again)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(all(&second.receiver), fresh_reserved);
        assert_eq!(new_calls.borrow().len(), 1);
        assert_eq!(remote.view(), after);
    });
}

#[test]
fn restored_local_fence_accepts_stale_late_publication_negative_control() {
    run(Guard::RestoredLocal);
}

#[test]
fn external_fence_rejects_stale_then_allows_fresh_exact_delivery() {
    run(Guard::External);
}
