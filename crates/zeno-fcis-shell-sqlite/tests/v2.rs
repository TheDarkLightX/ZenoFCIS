//! SQLite refinement exercised through the actual durable-counter V2 contract.
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Envelope, Hash32, commitment, decode_value};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    v2::{CommitReceipt, Error, V2SqliteShell},
};
use zeno_fcis_synthesis::finite::{
    V2Resource as Resource,
    canonical_v2::schema,
    v2_authority::{self as authority, Authority, Publication, PublicationOutcome},
    v2_catalog as catalog,
    v2_composition::{self as c, Raw},
};
use zeno_fcis_value::{Field, Value};

/// Actual generated declarative contract; this is not an application evaluator.
#[path = "../../zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
pub mod durable_counter;

fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("unexpected refusal: {error:?}"))
}
fn with_authority(step: Option<u64>, run: impl FnOnce(&Authority<'_>)) {
    let contract = durable_counter::Contract::new();
    let mut descriptor = contract.descriptor();
    if let Some(steps) = step {
        descriptor.limits = descriptor.limits.with_limit(Resource::Step, steps);
    }
    // Fixture policy encoding is supplied by the actual library, rather than a
    // shell-local serializer. Retained production bytes are checked separately
    // by the six-template source-bound qualification.
    let policy = authority::policy_bytes(
        &descriptor,
        durable_counter::ORIGINAL_SCHEMA,
        &durable_counter::FRAMING,
        durable_counter::CHANNEL_ROOTS,
    )
    .unwrap_or_else(|| panic!("policy encoding refused"));
    let catalog = ok(catalog::bind_original(
        durable_counter::ORIGINAL_SCHEMA,
        &durable_counter::DESCRIPTION,
        catalog::Limits {
            schema: schema::Limits {
                bytes: durable_counter::ORIGINAL_SCHEMA.len() as u64,
                types: 6,
                fields: 2,
                variants: 2,
            },
            contract_bytes: policy.len() as u64,
        },
        &policy,
        &descriptor,
        &durable_counter::FRAMING,
        durable_counter::CHANNEL_ROOTS,
    ));
    let bound = ok(authority::bind(&catalog));
    run(&bound);
}
fn envelope(binding: c::FrameBinding, value: Value) -> Vec<u8> {
    ok(Envelope::new(binding.root, Hash32::new(binding.schema), value).canonical_bytes())
}
fn state(count: i128, failures: i128) -> Vec<u8> {
    let value = ok(Value::record_canonical(vec![
        Field::new(110, Value::signed(count)),
        Field::new(111, Value::signed(failures)),
    ]));
    envelope(durable_counter::FRAMING.state, value)
}
fn command(variant: u16) -> Vec<u8> {
    envelope(
        durable_counter::FRAMING.command,
        Value::sum(101, variant, None),
    )
}
fn context(allowed: bool) -> Vec<u8> {
    envelope(durable_counter::FRAMING.context, Value::boolean(allowed))
}
fn genesis<'a>(a: &'a Authority<'_>, initial: &'a [u8]) -> Publication<'a> {
    match a.publish_genesis(initial) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("genesis refused: {refused:?}"),
    }
}
fn publication<'a>(
    a: &'a Authority<'_>,
    initial: &'a [u8],
    command: &'a [u8],
    context: &'a [u8],
) -> Publication<'a> {
    match a.publish(Raw {
        state: initial,
        command,
        context,
    }) {
        PublicationOutcome::Commit(p) => p,
        refused => panic!("publication refused: {refused:?}"),
    }
}
static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
struct StoreFile(PathBuf);
impl StoreFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zenofcis-v2-sqlite-{}-{}.db",
            std::process::id(),
            FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        ok(std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path));
        Self(path)
    }
}
impl Drop for StoreFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn actual_contract_commits_and_replays_without_duplicating_deliveries() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        let key = Hash32::new([1; 32]);
        let first = ok(db.commit(key, publication(a, &initial, &cmd, &ctx)));
        assert_eq!(first.status(), CommitStatus::Committed);
        let snapshot = ok(db.snapshot());
        assert_eq!(snapshot.version(), 1);
        assert_eq!(snapshot.state(), state(1, 0));
        assert_eq!(snapshot.pending(), 1);
        let second = ok(db.commit(key, publication(a, &initial, &cmd, &ctx)));
        assert_eq!(second.status(), CommitStatus::IdempotentReplay);
        assert_eq!(second.chain(), first.chain());
        assert_eq!(ok(db.snapshot()), snapshot);
        assert!(matches!(
            db.commit(Hash32::new([2; 32]), publication(a, &initial, &cmd, &ctx)),
            Err(Error::PreState)
        ));
        let failure = command(121);
        assert!(matches!(
            db.commit(key, publication(a, &initial, &failure, &ctx)),
            Err(Error::Replay)
        ));
        assert_eq!(ok(db.snapshot()), snapshot);
    });
}

#[test]
fn every_injected_commit_interruption_is_atomic() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::AfterOutboxWrite,
            CrashPoint::BeforeCommit,
            CrashPoint::AfterCommit,
        ] {
            let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
            let result = db.commit_with_crash(
                Hash32::new([3; 32]),
                publication(a, &initial, &cmd, &ctx),
                Some(point),
            );
            assert!(matches!(result,Err(Error::InjectedCrash(actual)) if actual==point));
            let snapshot = ok(db.snapshot());
            if point == CrashPoint::AfterCommit {
                assert_eq!(snapshot.version(), 1);
                assert_eq!(snapshot.state(), state(1, 0));
                assert_eq!(snapshot.pending(), 1);
            } else {
                assert_eq!(snapshot.version(), 0);
                assert_eq!(snapshot.state(), initial);
                assert_eq!(snapshot.pending(), 0);
            }
            ok(db.audit());
        }
    });
}

#[test]
fn deliveries_follow_commit_order_even_when_delivery_hashes_are_reversed() {
    with_authority(None, |a| {
        let cmd = command(120);
        let ctx = context(true);
        let mut witnessed = false;
        for failures in 0..=3 {
            let initial = state(0, failures);
            let post = state(1, failures);
            // The original genesis law requires both counters to be zero.
            // Reach each tested state through genuine failure publications.
            let start = state(0, 0);
            let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &start)));
            let failure = command(121);
            for previous in 0..failures {
                let before = state(0, previous);
                ok(db.commit(
                    Hash32::new([30 + previous as u8; 32]),
                    publication(a, &before, &failure, &ctx),
                ));
            }
            while let Some(pending) = ok(db.next_pending()) {
                ok(db.acknowledge(pending.delivery_id(), pending.entry_hash()));
            }
            ok(db.commit(Hash32::new([4; 32]), publication(a, &initial, &cmd, &ctx)));
            let first = ok(db.next_pending()).unwrap_or_else(|| panic!("missing first delivery"));
            ok(db.commit(Hash32::new([6; 32]), publication(a, &post, &cmd, &ctx)));
            assert_eq!(ok(db.next_pending()), Some(first.clone()));
            ok(db.acknowledge(first.delivery_id(), first.entry_hash()));
            let second = ok(db.next_pending()).unwrap_or_else(|| panic!("missing second delivery"));
            assert_eq!(
                (first.version(), second.version()),
                (failures as u64 + 1, failures as u64 + 2)
            );
            witnessed |= first.delivery_id() > second.delivery_id();
        }
        assert!(
            witnessed,
            "corpus must actually distinguish commit order from hash order"
        );
    });
}

#[test]
fn destination_retry_and_acknowledgement_bind_exact_committed_content() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([6; 32]), publication(a, &initial, &cmd, &ctx)));
        let p = ok(db.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        assert!(matches!(
            db.acknowledge(p.delivery_id(), Hash32::ZERO),
            Err(Error::Delivery)
        ));
        assert_eq!(ok(db.next_pending()), Some(p));
        let mut destination = MemoryDestination::default();
        assert!(ok(db.deliver_next_memory(&mut destination)));
        assert!(!ok(db.deliver_next_memory(&mut destination)));
        assert_eq!(destination.delivered_count(), 1);
        ok(db.audit());
    });
}

#[test]
fn reopen_and_private_checkpoint_replay_the_exact_tail() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([7; 32]), publication(a, &initial, &cmd, &ctx)));
        let checkpoint = ok(db.checkpoint());
        let post = state(1, 0);
        ok(db.commit(Hash32::new([8; 32]), publication(a, &post, &cmd, &ctx)));
        let snapshot = ok(db.snapshot());
        drop(db);
        let mut reopened = ok(V2SqliteShell::open(&file.0, a));
        assert_eq!(ok(reopened.snapshot()), snapshot);
        drop(reopened);
        let mut resumed = ok(V2SqliteShell::open_at_checkpoint(&file.0, a, &checkpoint));
        assert_eq!(ok(resumed.snapshot()), snapshot);
        ok(resumed.audit());
    });
}

#[test]
fn external_certificate_tampering_invalidates_live_cache_and_reopen() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([9; 32]), publication(a, &initial, &cmd, &ctx)));
        let external = ok(Connection::open(&file.0));
        ok(external.execute(
            "UPDATE v2_commits SET certificate=x'00' WHERE sequence=1",
            [],
        ));
        drop(external);
        assert!(matches!(db.snapshot(), Err(Error::History)));
        drop(db);
        assert!(matches!(
            V2SqliteShell::open(&file.0, a),
            Err(Error::History)
        ));
    });
}

#[test]
fn altered_delivery_bytes_and_schema_triggers_are_refused() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([10; 32]), publication(a, &initial, &cmd, &ctx)));
        drop(db);
        let external = ok(Connection::open(&file.0));
        ok(external.execute("UPDATE v2_deliveries SET payload=x'00'", []));
        assert!(matches!(
            V2SqliteShell::open(&file.0, a),
            Err(Error::Delivery)
        ));
        ok(external.execute_batch(
            "CREATE TRIGGER evil AFTER INSERT ON v2_commits BEGIN DELETE FROM v2_deliveries; END;",
        ));
        drop(external);
        assert!(matches!(
            V2SqliteShell::open(&file.0, a),
            Err(Error::Schema(9))
        ));
    });
}

#[test]
fn reject_has_no_publication_and_other_policy_capabilities_are_refused() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let denied = context(false);
        assert!(matches!(
            a.publish(Raw {
                state: &initial,
                command: &cmd,
                context: &denied
            }),
            PublicationOutcome::Reject(_)
        ));
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        with_authority(Some(9999), |other| {
            assert_ne!(a.identity(), other.identity());
            assert!(matches!(
                db.commit(
                    Hash32::new([11; 32]),
                    publication(other, &initial, &cmd, &ctx)
                ),
                Err(Error::Identity)
            ));
            assert!(matches!(
                V2SqliteShell::create_in_memory(a, genesis(other, &initial)),
                Err(Error::Identity)
            ));
        });
        assert_eq!(ok(db.snapshot()).version(), 0);
    });
}

#[test]
fn schema_v5_is_never_silently_upgraded_or_reinterpreted() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let connection = ok(Connection::open(&file.0));
        ok(connection.execute_batch("CREATE TABLE legacy(x BLOB); PRAGMA user_version=5;"));
        drop(connection);
        assert!(matches!(
            V2SqliteShell::open(&file.0, a),
            Err(Error::Schema(5))
        ));
        let initial = state(0, 0);
        assert!(matches!(
            V2SqliteShell::create(&file.0, a, genesis(a, &initial)),
            Err(Error::Schema(5))
        ));
        let connection = ok(Connection::open(&file.0));
        let version: i64 = ok(connection.query_row("PRAGMA user_version", [], |r| r.get(0)));
        assert_eq!(version, 5);
    });
}

#[test]
fn triggers_whose_names_merely_resemble_sqlite_internals_are_refused() {
    // `LIKE 'sqlite_%'` would hide these: `_` matches any character and ASCII
    // case is ignored. SQLite itself reserves only the literal prefix
    // `sqlite_`, so these are ordinary user objects.
    for name in ["sqlitex", "SQLiteX", "sqlite1"] {
        with_authority(None, |a| {
            let file = StoreFile::new();
            let initial = state(0, 0);
            let cmd = command(120);
            let ctx = context(true);
            let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
            let external = ok(Connection::open(&file.0));
            ok(external.execute_batch(&format!(
                "CREATE TRIGGER {name} AFTER INSERT ON v2_deliveries \
                 BEGIN UPDATE v2_deliveries SET acknowledged=1 WHERE rowid=new.rowid; END;"
            )));
            assert!(
                matches!(
                    db.commit(Hash32::new([13; 32]), publication(a, &initial, &cmd, &ctx)),
                    Err(Error::Schema(9))
                ),
                "{name}"
            );
            drop(db);
            drop(external);
            assert!(
                matches!(V2SqliteShell::open(&file.0, a), Err(Error::Schema(9))),
                "{name}"
            );
        });
    }
}

#[test]
fn external_trigger_is_refused_by_live_operations_before_it_can_modify_a_commit() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        let external = ok(Connection::open(&file.0));
        ok(external.execute_batch(
            "CREATE TRIGGER evil AFTER INSERT ON v2_commits BEGIN DELETE FROM v2_deliveries; END;",
        ));
        assert!(matches!(db.snapshot(), Err(Error::Schema(9))));
        assert!(matches!(db.checkpoint(), Err(Error::Schema(9))));
        assert!(matches!(
            db.commit(Hash32::new([12; 32]), publication(a, &initial, &cmd, &ctx)),
            Err(Error::Schema(9))
        ));
        let sequence: i64 = ok(external.query_row(
            "SELECT sequence FROM v2_state WHERE singleton=1",
            [],
            |r| r.get(0),
        ));
        assert_eq!(sequence, 0);
    });
}

#[test]
fn two_live_handles_revalidate_external_commits_and_refuse_stale_capabilities() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut first = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        let mut second = ok(V2SqliteShell::open(&file.0, a));
        ok(first.commit(Hash32::new([13; 32]), publication(a, &initial, &cmd, &ctx)));
        assert_eq!(ok(second.snapshot()), ok(first.snapshot()));
        assert!(matches!(
            second.commit(Hash32::new([14; 32]), publication(a, &initial, &cmd, &ctx)),
            Err(Error::PreState)
        ));
        let post = state(1, 0);
        ok(second.commit(Hash32::new([14; 32]), publication(a, &post, &cmd, &ctx)));
        assert_eq!(ok(first.snapshot()), ok(second.snapshot()));
        let pending = ok(first.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        assert_eq!(pending.version(), 1);
        ok(first.acknowledge(pending.delivery_id(), pending.entry_hash()));
        assert_eq!(ok(second.snapshot()).pending(), 1);
        assert_eq!(ok(second.next_pending()).map(|p| p.version()), Some(2));
    });
}

#[test]
fn full_audit_returns_a_usable_private_checkpoint_without_a_separate_marker_call() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([15; 32]), publication(a, &initial, &cmd, &ctx)));
        let checkpoint = ok(db.audit());
        let post = state(1, 0);
        ok(db.commit(Hash32::new([16; 32]), publication(a, &post, &cmd, &ctx)));
        let expected = ok(db.snapshot());
        drop(db);
        let mut resumed = ok(V2SqliteShell::open_at_checkpoint(&file.0, a, &checkpoint));
        assert_eq!(ok(resumed.snapshot()), expected);
    });
}

#[test]
fn outbox_id_matches_the_original_library_entry_and_delivery_before_ack_can_retry() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let p = publication(a, &initial, &cmd, &ctx);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        let receipt = ok(db.publication_bundle(0, Hash32::new([17; 32]), &p))
            .receipt()
            .to_vec();
        let mut framed = Vec::new();
        for part in [p.subject(), &receipt] {
            framed.extend_from_slice(&(part.len() as u64).to_be_bytes());
            framed.extend_from_slice(part);
        }
        let candidate = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::V2_PUBLICATION,
            &framed,
        ));
        let wire = p
            .outbox()
            .first()
            .unwrap_or_else(|| panic!("missing outbox"));
        let entry = OutboxEntry::new(
            wire.ordinal(),
            wire.channel(),
            ok(decode_value(wire.destination(), DecodeLimits::default())),
            ok(decode_value(wire.payload(), DecodeLimits::default())),
        );
        let expected_id = ok(entry.delivery_id::<RustCryptoSha256>(candidate));
        let expected_hash = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::OUTBOX_ENTRY,
            &ok(entry.canonical_bytes()),
        ));
        ok(db.commit(Hash32::new([17; 32]), p));
        let pending = ok(db.next_pending()).unwrap_or_else(|| panic!("missing delivery"));
        assert_eq!(
            (pending.delivery_id(), pending.entry_hash()),
            (expected_id, expected_hash)
        );
        // Simulate the window after destination delivery and before DB ack.
        let mut destination = MemoryDestination::default();
        assert_eq!(
            ok(db.deliver_next_memory_unacknowledged(&mut destination)),
            Some((expected_id, expected_hash))
        );
        assert_eq!(ok(db.snapshot()).pending(), 1);
        assert!(ok(db.deliver_next_memory(&mut destination)));
        assert_eq!(destination.delivered_count(), 1);
        assert_eq!(ok(db.snapshot()).pending(), 0);
        ok(db.audit());
    });
}

#[test]
fn changed_delivery_interpreter_refuses_open_checkpoint_and_live_delivery() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        ok(db.commit(Hash32::new([18; 32]), publication(a, &initial, &cmd, &ctx)));
        let checkpoint = ok(db.checkpoint());
        let external = ok(Connection::open(&file.0));
        let actual: Vec<u8> = ok(external.query_row(
            "SELECT delivery_interpreter FROM v2_genesis WHERE singleton=1",
            [],
            |row| row.get(0),
        ));
        // Independently frame the exact checked-in interpreter and provider ID.
        let source = include_bytes!("../../zeno-fcis-shell/src/delivery.rs");
        let provider =
            <RustCryptoSha256 as zeno_fcis_codec::CommitmentHasher>::ALGORITHM_ID.as_bytes();
        let mut preimage = Vec::new();
        for part in [provider, source.as_slice()] {
            preimage.extend_from_slice(&ok(u64::try_from(part.len())).to_be_bytes());
            preimage.extend_from_slice(part);
        }
        let expected = ok(commitment::<RustCryptoSha256>(
            zeno_fcis_codec::domains::DELIVERY_INTERPRETER,
            &preimage,
        ));
        assert_eq!(actual, expected.as_bytes());
        ok(external.execute(
            "UPDATE v2_genesis SET delivery_interpreter=zeroblob(32) WHERE singleton=1",
            [],
        ));
        assert!(matches!(
            V2SqliteShell::open(&file.0, a),
            Err(Error::Interpreter)
        ));
        assert!(matches!(
            V2SqliteShell::open_at_checkpoint(&file.0, a, &checkpoint),
            Err(Error::Interpreter)
        ));
        let mut destination = MemoryDestination::default();
        assert!(matches!(
            db.deliver_next_memory(&mut destination),
            Err(Error::Interpreter)
        ));
        assert_eq!(destination.delivered_count(), 0);
        let acknowledged: i64 = ok(external.query_row(
            "SELECT sum(acknowledged) FROM v2_deliveries",
            [],
            |row| row.get(0),
        ));
        assert_eq!(acknowledged, 0);
    });
}

/// Every authoritative row, read through an independent connection.
fn stored_rows(path: &Path) -> Vec<(&'static str, Vec<rusqlite::types::Value>)> {
    let external = ok(Connection::open(path));
    let mut rows = Vec::new();
    for table in [
        "v2_genesis",
        "v2_state",
        "v2_commits",
        "v2_deliveries",
        "v2_checkpoints",
    ] {
        let mut statement = ok(external.prepare(&format!("SELECT * FROM {table} ORDER BY rowid")));
        let width = statement.column_count();
        let mut query = ok(statement.query([]));
        while let Some(row) = ok(query.next()) {
            rows.push((table, (0..width).map(|i| ok(row.get(i))).collect()));
        }
    }
    rows
}
/// Reach one public ordinary commit entry point at expected version zero.
fn commit_through(
    entry: &str,
    db: &mut V2SqliteShell<'_, '_>,
    key: Hash32,
    p: Publication<'_>,
    crash: Option<CrashPoint>,
) -> Result<CommitReceipt, Error> {
    match entry {
        "commit" => db.commit(key, p),
        "commit_at" => db.commit_at(0, key, p),
        "commit_with_crash" => db.commit_with_crash(key, p, crash),
        "commit_with_crash_at" => db.commit_with_crash_at(0, key, p, crash),
        "commit_bounded_at" => db.commit_bounded_at(0, key, p, usize::MAX, crash),
        other => panic!("unknown commit entry point {other}"),
    }
}

#[test]
fn transition_publication_cannot_initialize_a_memory_or_file_store() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let file = StoreFile::new();
        let memory =
            V2SqliteShell::create_in_memory(a, publication(a, &initial, &cmd, &ctx)).map(|_| ());
        let stored =
            V2SqliteShell::create(&file.0, a, publication(a, &initial, &cmd, &ctx)).map(|_| ());
        // Opening the path is an external shell effect; refusal installs no schema or row.
        let external = ok(Connection::open(&file.0));
        let objects: i64 =
            ok(external.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0)));
        let version: i64 = ok(external.query_row("PRAGMA user_version", [], |r| r.get(0)));
        drop(external);
        assert!(
            matches!(
                (&memory, &stored, objects, version),
                (Err(Error::InvocationKind), Err(Error::InvocationKind), 0, 0)
            ),
            "memory={memory:?} file={stored:?} objects={objects} user_version={version}"
        );
        let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        assert_eq!(ok(db.snapshot()).state(), initial);
        with_authority(Some(9999), |other| {
            assert!(matches!(
                V2SqliteShell::create_in_memory(a, publication(other, &initial, &cmd, &ctx)),
                Err(Error::Identity)
            ));
        });
    });
}

#[test]
fn genesis_publication_cannot_commit_through_any_ordinary_entry_point() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let boundary = Some(CrashPoint::BeforeTransaction);
        let entries = [
            "commit",
            "commit_at",
            "commit_with_crash",
            "commit_with_crash_at",
            "commit_bounded_at",
        ];
        let mut observed = Vec::new();
        for entry in entries {
            let file = StoreFile::new();
            let mut db = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
            let head = ok(db.snapshot());
            let rows = stored_rows(&file.0);
            // A wrong kind never reaches the requested first commit crash boundary.
            let wrong = genesis(a, &initial);
            let refused = commit_through(entry, &mut db, Hash32::new([19; 32]), wrong, boundary);
            let unchanged =
                matches!(db.snapshot(), Ok(now) if now == head) && stored_rows(&file.0) == rows;
            // The same entry point still commits a genuine transition and its delivery.
            let valid = publication(a, &initial, &cmd, &ctx);
            let committed = matches!(
                commit_through(entry, &mut db, Hash32::new([20; 32]), valid, None),
                Ok(receipt) if receipt.status() == CommitStatus::Committed
            ) && matches!(
                db.snapshot(),
                Ok(now) if now.version() == 1 && now.state() == state(1, 0) && now.pending() == 1
            );
            let kind = matches!(refused, Err(Error::InvocationKind));
            observed.push((entry, format!("{refused:?}"), kind, unchanged, committed));
        }
        assert!(
            observed
                .iter()
                .all(|(_, _, kind, unchanged, committed)| *kind && *unchanged && *committed),
            "{observed:#?}"
        );
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        with_authority(Some(9999), |other| {
            for entry in entries {
                let foreign = genesis(other, &initial);
                let refused =
                    commit_through(entry, &mut db, Hash32::new([19; 32]), foreign, boundary);
                assert!(
                    matches!(refused, Err(Error::Identity)),
                    "{entry}: {refused:?}"
                );
            }
        });
        assert_eq!(ok(db.snapshot()).version(), 0);
    });
}

#[test]
fn publication_bundle_refuses_genesis_as_an_ordinary_transition() {
    with_authority(None, |a| {
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let key = Hash32::new([21; 32]);
        let mut db = ok(V2SqliteShell::create_in_memory(a, genesis(a, &initial)));
        let head = ok(db.snapshot());
        let refused = db
            .publication_bundle(0, key, &genesis(a, &initial))
            .map(|_| ());
        assert!(matches!(refused, Err(Error::InvocationKind)), "{refused:?}");
        assert_eq!(ok(db.snapshot()), head);
        ok(db.publication_bundle(0, key, &publication(a, &initial, &cmd, &ctx)));
        with_authority(Some(9999), |other| {
            let refused = db
                .publication_bundle(0, key, &genesis(other, &initial))
                .map(|_| ());
            assert!(matches!(refused, Err(Error::Identity)), "{refused:?}");
        });
    });
}

#[test]
fn wrong_kind_is_refused_before_cache_refresh_or_replay_lookup() {
    with_authority(None, |a| {
        let file = StoreFile::new();
        let initial = state(0, 0);
        let cmd = command(120);
        let ctx = context(true);
        let key = Hash32::new([22; 32]);
        let mut stale = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
        let mut writer = ok(V2SqliteShell::open(&file.0, a));
        ok(writer.commit(key, publication(a, &initial, &cmd, &ctx)));
        let rows = stored_rows(&file.0);
        // Refreshing the stale handle would audit and save a checkpoint, and the
        // reused key would reach replay comparison. A wrong kind reaches neither.
        let refused = stale.commit(key, genesis(a, &initial));
        let unchanged = stored_rows(&file.0) == rows;
        assert!(
            matches!(refused, Err(Error::InvocationKind)) && unchanged,
            "{refused:?} unchanged={unchanged}"
        );
        assert_eq!(ok(stale.snapshot()), ok(writer.snapshot()));
    });
}

#[test]
fn stale_publication_bundle_refuses_wrong_kind_before_refresh() {
    with_authority(None, |a| {
        with_authority(Some(9999), |other| {
            let file = StoreFile::new();
            let initial = state(0, 0);
            let post = state(1, 0);
            let cmd = command(120);
            let ctx = context(true);
            let key = Hash32::new([23; 32]);
            let mut writer = ok(V2SqliteShell::create(&file.0, a, genesis(a, &initial)));
            // Every stale handle opens before the writer commits and makes one wrong-kind call.
            let mut stale: Vec<_> = (0..4)
                .map(|_| ok(V2SqliteShell::open(&file.0, a)))
                .collect();
            ok(writer.commit(key, publication(a, &initial, &cmd, &ctx)));
            // A refresh saves the new head checkpoint, so the first refresh changes these rows.
            let rows = stored_rows(&file.0);
            let cases = [(false, 0), (false, 1), (true, 0), (true, 1)];
            let mut observed = Vec::new();
            for (db, (foreign, expected)) in stale.iter_mut().zip(cases) {
                let wrong = if foreign {
                    genesis(other, &initial)
                } else {
                    genesis(a, &initial)
                };
                let refused = db.publication_bundle(expected, key, &wrong).map(|_| ());
                // Read rows before any call that could itself refresh a handle.
                let unchanged = stored_rows(&file.0) == rows;
                let refusal = if foreign {
                    matches!(refused, Err(Error::Identity))
                } else {
                    matches!(refused, Err(Error::InvocationKind))
                };
                observed.push((
                    foreign,
                    expected,
                    format!("{refused:?}"),
                    refusal,
                    unchanged,
                ));
            }
            assert!(
                observed
                    .iter()
                    .all(|(_, _, _, refusal, unchanged)| *refusal && *unchanged),
                "{observed:#?}"
            );
            // A correct-kind transition keeps the original order on a still-stale handle:
            // refresh (saving the checkpoint), then Concurrent, Identity and PreState.
            let db = &mut stale[0];
            let refused = db
                .publication_bundle(0, key, &publication(a, &initial, &cmd, &ctx))
                .map(|_| ());
            assert!(matches!(refused, Err(Error::Concurrent)), "{refused:?}");
            assert_ne!(stored_rows(&file.0), rows);
            let refused = db
                .publication_bundle(1, key, &publication(other, &post, &cmd, &ctx))
                .map(|_| ());
            assert!(matches!(refused, Err(Error::Identity)), "{refused:?}");
            let refused = db
                .publication_bundle(1, key, &publication(a, &initial, &cmd, &ctx))
                .map(|_| ());
            assert!(matches!(refused, Err(Error::PreState)), "{refused:?}");
            ok(db.publication_bundle(1, key, &publication(a, &post, &cmd, &ctx)));
        });
    });
}
