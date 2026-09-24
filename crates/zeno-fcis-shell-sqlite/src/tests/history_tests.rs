//! Historical reads must preserve exact bindings and snapshot consistency.

use super::transaction_tests::authorized_from_state;
use super::*;
use zeno_fcis_transition::ExpectedInvocationBindings;

fn bindings(
    authority: &TestAuthority,
    catalog: &ProjectCatalog,
    replay: u8,
) -> ExpectedInvocationBindings {
    authority
        .request_bindings(
            &command(catalog),
            &context(catalog),
            hash(60),
            hash(61),
            hash(replay),
        )
        .unwrap_or_else(|e| panic!("bindings: {e}"))
}

fn genesis(authority: &TestAuthority, catalog: &ProjectCatalog) -> Hash32 {
    authority
        .authorize_genesis(initial_state(catalog))
        .unwrap_or_else(|e| panic!("genesis: {e}"))
        .genesis_id()
        .hash()
}

#[test]
fn lost_reply_lookup_survives_later_commits_and_reopen_without_writes() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    let first = authorized(&authority, &catalog, 9);
    let receipt = first
        .bundle()
        .receipt()
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("receipt: {e}"));
    let candidate = first.bundle().candidate_id();
    let authorization = first.authorization_id().hash();
    let invocation = first.body().invocation_id();
    assert!(matches!(
        database.commit_with_crash_point(first, Some(CrashPoint::AfterCommit)),
        Err(SqliteShellError::InjectedCrash(CrashPoint::AfterCommit))
    ));
    let mut database = reopen(database, &authority);
    database
        .commit(authorized_from_state(
            &authority,
            &catalog,
            Value::U128(11),
            10,
        ))
        .unwrap_or_else(|e| panic!("second commit: {e}"));
    let before = database
        .snapshot()
        .unwrap_or_else(|e| panic!("snapshot: {e}"));
    let changes = database.connection.total_changes();
    let historical = database
        .lookup_committed(
            &authority,
            genesis(&authority, &catalog),
            hash(9),
            bindings(&authority, &catalog, 9),
        )
        .unwrap_or_else(|e| panic!("lookup: {e}"));
    let record = historical
        .committed()
        .unwrap_or_else(|| panic!("lost committed receipt"));
    assert_eq!(record.receipt_bytes(), receipt);
    assert_eq!(record.candidate_id(), candidate);
    assert_eq!(record.authorization_id(), authorization);
    assert_eq!(record.invocation_id(), invocation);
    assert_eq!(record.replay_id(), hash(9));
    assert_eq!(record.state_version(), 1);
    assert_eq!(historical.observed_version(), 2);
    assert_eq!(historical.observed_root(), before.root());
    assert_eq!(historical.genesis_id(), database.genesis_id());
    assert_eq!(historical.policy_id(), authority.policy().policy_id());
    assert_eq!(
        before,
        database
            .snapshot()
            .unwrap_or_else(|e| panic!("after snapshot: {e}"))
    );
    assert_eq!(changes, database.connection.total_changes());
    assert!(database.connection.is_autocommit());
    let mut reopened = reopen(database, &authority);
    assert_eq!(
        historical,
        reopened
            .lookup_committed(
                &authority,
                genesis(&authority, &catalog),
                hash(9),
                bindings(&authority, &catalog, 9)
            )
            .unwrap_or_else(|e| panic!("reopened lookup: {e}"))
    );
}

#[test]
fn lookup_matches_complete_original_request_and_policy() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    database
        .commit(authorized(&authority, &catalog, 9))
        .unwrap_or_else(|e| panic!("commit: {e}"));
    for changed in 0..5 {
        let admit_bool = |type_id, value| {
            SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
                catalog.schema(),
                TypeId::new(type_id),
                Value::Bool(value),
                ValidationLimits::default(),
            )
            .unwrap_or_else(|e| panic!("envelope: {e}"))
        };
        let expected = authority
            .request_bindings(
                &admit_bool(2, changed != 0),
                &admit_bool(3, changed != 1),
                hash(if changed == 2 { 63 } else { 60 }),
                hash(if changed == 3 { 64 } else { 61 }),
                hash(if changed == 4 { 10 } else { 9 }),
            )
            .unwrap_or_else(|e| panic!("changed bindings: {e}"));
        assert!(
            matches!(
                database.lookup_committed(
                    &authority,
                    genesis(&authority, &catalog),
                    hash(9),
                    expected
                ),
                Err(SqliteShellError::ReplayConflict)
            ),
            "changed binding {changed}"
        );
        assert!(database.connection.is_autocommit());
    }
    let other = super::authority(&catalog, 54);
    assert!(matches!(
        database.lookup_committed(
            &other,
            genesis(&authority, &catalog),
            hash(9),
            bindings(&other, &catalog, 9)
        ),
        Err(SqliteShellError::PolicyMismatch)
    ));
    assert!(matches!(
        database.lookup_committed(
            &authority,
            genesis(&authority, &catalog),
            hash(9),
            bindings(&other, &catalog, 9)
        ),
        Err(SqliteShellError::ReplayConflict)
    ));
}

#[test]
fn absence_has_validated_scope_and_zero_replay_is_an_error() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    let expected = bindings(&authority, &catalog, 9);
    let result = database
        .lookup_committed(&authority, genesis(&authority, &catalog), hash(9), expected)
        .unwrap_or_else(|e| panic!("absent: {e}"));
    assert!(result.committed().is_none());
    assert_eq!(result.observed_version(), 0);
    assert_eq!(result.genesis_id(), database.genesis_id());
    assert_eq!(result.policy_id(), authority.policy().policy_id());
    assert!(matches!(
        database.lookup_committed(
            &authority,
            genesis(&authority, &catalog),
            Hash32::ZERO,
            expected
        ),
        Err(SqliteShellError::InvalidReplayId)
    ));
}

#[test]
fn corrupt_history_never_becomes_a_hit_or_absence() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mutations = [
        "UPDATE genesis SET authorization_bytes = X'00'",
        "DELETE FROM genesis",
        "UPDATE semantic_state SET version = version + 1",
        "UPDATE semantic_state SET state_bytes = X'00'",
        "UPDATE shell_identity SET policy_id = zeroblob(32)",
        "UPDATE authorizations SET receipt_bytes = X'00'",
        "DELETE FROM replay",
        "DELETE FROM authorizations",
        "DELETE FROM outbox",
        "INSERT INTO replay SELECT zeroblob(32), zeroblob(32), candidate_id, authorization_bytes, bundle_bytes FROM replay LIMIT 1",
        "INSERT INTO authorizations SELECT zeroblob(32), 99, policy_id, invocation_id, zeroblob(32), zeroblob(32), authorization_bytes, bundle_bytes, receipt_bytes FROM authorizations LIMIT 1",
        "PRAGMA user_version = 4",
    ];
    for mutation in mutations {
        for replay in [9, 10] {
            let mut database = shell(&authority, &catalog);
            database
                .commit(authorized(&authority, &catalog, 9))
                .unwrap_or_else(|e| panic!("commit: {e}"));
            database
                .connection
                .execute_batch("PRAGMA foreign_keys = OFF;")
                .unwrap_or_else(|e| panic!("corruption setup: {e}"));
            database
                .connection
                .execute_batch(mutation)
                .unwrap_or_else(|e| panic!("mutation {mutation}: {e}"));
            let changes = database.connection.total_changes();
            assert!(
                database
                    .lookup_committed(
                        &authority,
                        genesis(&authority, &catalog),
                        hash(replay),
                        bindings(&authority, &catalog, replay)
                    )
                    .is_err(),
                "accepted {mutation}, replay {replay}"
            );
            assert_eq!(changes, database.connection.total_changes());
            assert!(database.connection.is_autocommit());
        }
    }
}

fn file_shell(
    authority: &TestAuthority,
    catalog: &ProjectCatalog,
    label: &str,
) -> (std::path::PathBuf, TestShell) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|e| panic!("clock: {e}"))
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "zeno-history-{label}-{}-{nonce}.db",
        std::process::id()
    ));
    let genesis = authority
        .authorize_genesis(initial_state(catalog))
        .unwrap_or_else(|e| panic!("genesis: {e}"));
    let database = TestShell::create(
        &path,
        authority,
        genesis,
        authority.bind_delivery_interpreter(MemoryDestination::default()),
    )
    .unwrap_or_else(|e| panic!("file shell: {e}"));
    (path, database)
}

#[test]
fn another_writer_requires_revalidation_before_reporting_absence() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let (path, mut reader) = file_shell(&authority, &catalog, "writer");
    let mut writer = TestShell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(MemoryDestination::default()),
    )
    .unwrap_or_else(|e| panic!("writer: {e}"));
    writer
        .commit(authorized(&authority, &catalog, 9))
        .unwrap_or_else(|e| panic!("write: {e}"));
    for replay in [9, 10] {
        assert!(matches!(
            reader.lookup_committed(
                &authority,
                genesis(&authority, &catalog),
                hash(replay),
                bindings(&authority, &catalog, replay)
            ),
            Err(SqliteShellError::CorruptHistory)
        ));
    }
    drop(writer);
    drop(reader);
    std::fs::remove_file(path).unwrap_or_else(|e| panic!("cleanup: {e}"));
}

#[test]
fn a_wal_write_after_snapshot_does_not_mix_histories() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let (path, mut database) = file_shell(&authority, &catalog, "snapshot");
    database
        .connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap_or_else(|e| panic!("wal: {e}"));
    database
        .commit(authorized(&authority, &catalog, 9))
        .unwrap_or_else(|e| panic!("commit: {e}"));
    let writer = Connection::open(&path).unwrap_or_else(|e| panic!("writer: {e}"));
    let changed = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&changed);
    database
        .connection
        .authorizer(Some(move |context: AuthContext<'_>| {
            if matches!(
                context.action,
                AuthAction::Read {
                    table_name: "authorizations",
                    ..
                }
            ) && !signal.swap(true, Ordering::SeqCst)
            {
                writer
                    .execute("UPDATE authorizations SET receipt_bytes = X'00'", [])
                    .unwrap_or_else(|e| panic!("interleaved write: {e}"));
            }
            Authorization::Allow
        }))
        .unwrap_or_else(|e| panic!("schedule: {e}"));
    let result = database
        .lookup_committed(
            &authority,
            genesis(&authority, &catalog),
            hash(9),
            bindings(&authority, &catalog, 9),
        )
        .unwrap_or_else(|e| panic!("snapshot lookup: {e}"));
    assert!(changed.load(Ordering::SeqCst));
    assert!(result.committed().is_some());
    assert_eq!(result.observed_version(), 1);
    database
        .connection
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .unwrap_or_else(|e| panic!("remove schedule: {e}"));
    assert!(
        database
            .lookup_committed(
                &authority,
                genesis(&authority, &catalog),
                hash(9),
                bindings(&authority, &catalog, 9)
            )
            .is_err()
    );
    drop(database);
    std::fs::remove_file(path).unwrap_or_else(|e| panic!("cleanup: {e}"));
}

#[test]
fn a_valid_older_store_does_not_provide_rollback_freshness() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let (path, mut database) = file_shell(&authority, &catalog, "rollback");
    let backup = path.with_extension("backup");
    std::fs::copy(&path, &backup).unwrap_or_else(|e| panic!("backup: {e}"));
    database
        .commit(authorized(&authority, &catalog, 9))
        .unwrap_or_else(|e| panic!("commit: {e}"));
    drop(database);
    std::fs::copy(&backup, &path).unwrap_or_else(|e| panic!("restore: {e}"));
    let mut restored = TestShell::open_existing(
        &path,
        &authority,
        authority.bind_delivery_interpreter(MemoryDestination::default()),
    )
    .unwrap_or_else(|e| panic!("reopen old store: {e}"));
    let result = restored
        .lookup_committed(
            &authority,
            genesis(&authority, &catalog),
            hash(9),
            bindings(&authority, &catalog, 9),
        )
        .unwrap_or_else(|e| panic!("lookup old store: {e}"));
    assert!(result.committed().is_none());
    assert_eq!(result.observed_version(), 0);
    drop(restored);
    for file in [path, backup] {
        std::fs::remove_file(file).unwrap_or_else(|e| panic!("cleanup: {e}"));
    }
}

#[test]
fn different_genesis_never_turns_a_historical_request_into_absence() {
    let catalog = catalog();
    let predecessor = authority(&catalog, 53);
    let original_genesis = genesis(&predecessor, &catalog);
    let mut old = shell(&predecessor, &catalog);
    old.commit(authorized(&predecessor, &catalog, 9))
        .unwrap_or_else(|e| panic!("old commit: {e}"));
    let successor = authority(&catalog, 54);
    let mut new = shell(&successor, &catalog);
    // Model a caller incorrectly deriving new bindings after changing stores.
    assert!(matches!(
        new.lookup_committed(
            &successor,
            original_genesis,
            hash(9),
            bindings(&successor, &catalog, 9)
        ),
        Err(SqliteShellError::LineageMismatch)
    ));
    assert!(new.connection.is_autocommit());
    assert_eq!(
        new.snapshot()
            .unwrap_or_else(|e| panic!("new snapshot: {e}"))
            .version(),
        0
    );
    assert!(
        old.lookup_committed(
            &predecessor,
            original_genesis,
            hash(9),
            bindings(&predecessor, &catalog, 9)
        )
        .unwrap_or_else(|e| panic!("old lookup: {e}"))
        .committed()
        .is_some()
    );
}

#[test]
fn lookup_paths_release_transactions_and_preserve_commit_and_delivery() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    for outcome in ["genesis", "hit", "miss", "conflict", "lineage"] {
        let mut database = shell(&authority, &catalog);
        if outcome != "genesis" {
            database
                .commit(authorized(&authority, &catalog, 9))
                .unwrap_or_else(|e| panic!("initial commit: {e}"));
        }
        let before = database
            .snapshot()
            .unwrap_or_else(|e| panic!("snapshot: {e}"));
        let history = database.validated_history.clone();
        let changes = database.connection.total_changes();
        let replay = if outcome == "miss" { 10 } else { 9 };
        let expected = bindings(
            &authority,
            &catalog,
            if outcome == "conflict" { 11 } else { replay },
        );
        let scope = if outcome == "lineage" {
            hash(99)
        } else {
            genesis(&authority, &catalog)
        };
        let result = database.lookup_committed(&authority, scope, hash(replay), expected);
        match outcome {
            "conflict" => assert!(matches!(result, Err(SqliteShellError::ReplayConflict))),
            "lineage" => assert!(matches!(result, Err(SqliteShellError::LineageMismatch))),
            _ => {
                assert!(result.is_ok());
            }
        }
        assert!(database.connection.is_autocommit());
        assert_eq!(database.connection.total_changes(), changes);
        assert_eq!(database.validated_history, history);
        assert_eq!(
            database.snapshot().unwrap_or_else(|e| panic!("after: {e}")),
            before
        );
        database
            .commit(authorized_from_state(
                &authority,
                &catalog,
                before.state().clone(),
                12,
            ))
            .unwrap_or_else(|e| panic!("subsequent commit after {outcome}: {e}"));
        assert!(
            database
                .deliver_next()
                .unwrap_or_else(|e| panic!("subsequent delivery after {outcome}: {e}"))
        );
    }
}
