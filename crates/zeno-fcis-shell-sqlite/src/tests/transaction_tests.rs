//! Transaction checks must retain corruption rejection and retry behavior.

use super::*;

#[test]
fn replay_and_acknowledgement_preserve_first_history_errors() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let cases = [
        (
            "authorization-id",
            "UPDATE authorizations SET authorization_id = zeroblob(32)",
            "CorruptHistory",
        ),
        (
            "authorization-version",
            "UPDATE authorizations SET state_version = -1",
            "IntegerRange",
        ),
        (
            "authorization-policy",
            "UPDATE authorizations SET policy_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "authorization-invocation",
            "UPDATE authorizations SET invocation_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "authorization-replay",
            "UPDATE authorizations SET replay_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "authorization-candidate",
            "UPDATE authorizations SET candidate_id = zeroblob(32)",
            "CorruptHistory",
        ),
        (
            "authorization-bytes",
            "UPDATE authorizations SET authorization_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "authorization-bundle",
            "UPDATE authorizations SET bundle_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "authorization-receipt",
            "UPDATE authorizations SET receipt_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "bundle-candidate",
            "UPDATE bundles SET candidate_id = zeroblob(32)",
            "CorruptHistory",
        ),
        (
            "bundle-authorization",
            "UPDATE bundles SET authorization_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "bundle-bytes",
            "UPDATE bundles SET bundle_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "bundle-receipt",
            "UPDATE bundles SET receipt_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "replay-id",
            "UPDATE replay SET replay_id = zeroblob(32)",
            "CorruptHistory",
        ),
        (
            "replay-authorization",
            "UPDATE replay SET authorization_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "replay-candidate",
            "UPDATE replay SET candidate_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "replay-authorization-bytes",
            "UPDATE replay SET authorization_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "replay-bundle",
            "UPDATE replay SET bundle_bytes = X'00'",
            "CorruptHistory",
        ),
        (
            "outbox-delivery",
            "UPDATE outbox SET delivery_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "outbox-authorization",
            "UPDATE outbox SET authorization_id = X'00'",
            "InvalidHashLength",
        ),
        (
            "outbox-candidate",
            "UPDATE outbox SET candidate_id = zeroblob(32)",
            "CorruptOutbox",
        ),
        (
            "outbox-ordinal",
            "UPDATE outbox SET ordinal = -1",
            "IntegerRange",
        ),
        (
            "outbox-channel",
            "UPDATE outbox SET channel = -1",
            "IntegerRange",
        ),
        (
            "outbox-destination",
            "UPDATE outbox SET destination_bytes = X'00'",
            "CorruptOutbox",
        ),
        (
            "outbox-payload",
            "UPDATE outbox SET payload_bytes = X'00'",
            "CorruptOutbox",
        ),
        (
            "outbox-hash",
            "UPDATE outbox SET entry_hash = X'00'",
            "InvalidHashLength",
        ),
        (
            "outbox-acknowledged",
            "UPDATE outbox SET acknowledged = 2",
            "CorruptOutbox",
        ),
        (
            "missing-authorization",
            "DELETE FROM authorizations",
            "CorruptHistory",
        ),
        ("missing-bundle", "DELETE FROM bundles", "CorruptHistory"),
        ("missing-replay", "DELETE FROM replay", "CorruptHistory"),
        ("missing-outbox", "DELETE FROM outbox", "CorruptOutbox"),
        (
            "extra-authorization",
            "INSERT INTO authorizations SELECT zeroblob(32), 2, policy_id, invocation_id, zeroblob(32), zeroblob(32), authorization_bytes, bundle_bytes, receipt_bytes FROM authorizations",
            "CorruptHistory",
        ),
        (
            "extra-bundle",
            "INSERT INTO bundles SELECT zeroblob(32), zeroblob(32), bundle_bytes, receipt_bytes FROM bundles",
            "CorruptHistory",
        ),
        (
            "extra-replay",
            "INSERT INTO replay SELECT zeroblob(32), zeroblob(32), candidate_id, authorization_bytes, bundle_bytes FROM replay",
            "CorruptHistory",
        ),
        (
            "extra-outbox",
            "INSERT INTO outbox SELECT zeroblob(32), zeroblob(32), zeroblob(32), ordinal, channel, destination_bytes, payload_bytes, entry_hash, acknowledged FROM outbox",
            "CorruptHistory",
        ),
        (
            "policy-before-outbox",
            "UPDATE authorizations SET policy_id = X'00'; UPDATE outbox SET acknowledged = 2",
            "InvalidHashLength",
        ),
        (
            "bundle-before-outbox",
            "UPDATE bundles SET bundle_bytes = X'00'; UPDATE outbox SET entry_hash = X'00'",
            "CorruptHistory",
        ),
        (
            "row-count-before-row-fields",
            "UPDATE outbox SET entry_hash = X'00'; INSERT INTO outbox SELECT zeroblob(32), authorization_id, candidate_id, 1, channel, destination_bytes, payload_bytes, entry_hash, acknowledged FROM outbox",
            "CorruptOutbox",
        ),
    ];
    for (name, mutation, expected) in cases {
        for operation in ["replay", "acknowledge"] {
            let mut database = shell(&authority, &catalog);
            database
                .commit(authorized(&authority, &catalog, 9))
                .unwrap_or_else(|error| panic!("initial commit: {error}"));
            let pending = database
                .next_pending()
                .unwrap_or_else(|error| panic!("pending: {error}"))
                .unwrap_or_else(|| panic!("missing pending delivery"));
            let history = database.validated_history.clone();
            let root = database.validated_state_root;
            let version = database.validated_state_version;
            // Model stored corruption, including rows another writer could
            // create without honoring this connection's SQL constraints.
            database
                .connection
                .execute_batch("PRAGMA foreign_keys = OFF; PRAGMA ignore_check_constraints = ON;")
                .unwrap_or_else(|error| panic!("corruption setup: {error}"));
            database
                .connection
                .execute_batch(mutation)
                .unwrap_or_else(|error| panic!("{name} mutation: {error}"));
            let changes = database.connection.total_changes();
            let result = if operation == "replay" {
                database
                    .commit(authorized(&authority, &catalog, 9))
                    .map(|_| ())
            } else {
                // A simultaneous caller error must follow stored corruption.
                database.acknowledge(pending.delivery_id(), Hash32::ZERO)
            };
            let Err(error) = result else {
                panic!("{name} {operation} accepted corrupted history");
            };
            let actual = format!("{error:?}");
            assert_eq!(actual, expected, "{name} {operation}");
            assert_eq!(database.connection.total_changes(), changes);
            assert_eq!(database.validated_history, history);
            assert_eq!(database.validated_state_root, root);
            assert_eq!(database.validated_state_version, version);
            assert!(database.connection.is_autocommit());
            println!("OBS {name} {operation}: {actual}");
        }
    }
}

#[test]
fn failed_sqlite_commit_preserves_memory_and_allows_retry() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    let before = database
        .snapshot()
        .unwrap_or_else(|error| panic!("before: {error}"));
    let history = database.validated_history.clone();
    let root = database.validated_state_root;
    let version = database.validated_state_version;
    // A deferred foreign key allows every write to complete but rejects COMMIT.
    // Temporary objects leave the shell's persistent schema unchanged.
    database
        .connection
        .execute_batch(
            "CREATE TEMP TABLE commit_parent (id INTEGER PRIMARY KEY);
         CREATE TEMP TABLE commit_child (
             id INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TEMP TRIGGER reject_commit AFTER UPDATE ON main.semantic_state
         BEGIN INSERT INTO commit_child(id) VALUES (1); END;",
        )
        .unwrap_or_else(|error| panic!("commit failure setup: {error}"));
    assert!(matches!(
        database.commit_with_crash_point(
            authorized(&authority, &catalog, 9),
            Some(CrashPoint::BeforeCommit),
        ),
        Err(SqliteShellError::InjectedCrash(CrashPoint::BeforeCommit))
    ));
    assert!(matches!(
        database.commit(authorized(&authority, &catalog, 9)),
        Err(SqliteShellError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
            if error.code == rusqlite::ErrorCode::ConstraintViolation
    ));
    assert!(database.connection.is_autocommit());
    assert_eq!(database.validated_history, history);
    assert_eq!(database.validated_state_root, root);
    assert_eq!(database.validated_state_version, version);
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("after: {error}")),
        before
    );
    database
        .connection
        .execute_batch("DROP TRIGGER reject_commit;")
        .unwrap_or_else(|error| panic!("remove commit failure: {error}"));
    assert_eq!(
        database
            .commit(authorized(&authority, &catalog, 9))
            .unwrap_or_else(|error| panic!("retry: {error}")),
        CommitStatus::Committed
    );
    assert_eq!(
        database
            .commit(authorized(&authority, &catalog, 9))
            .unwrap_or_else(|error| panic!("replay: {error}")),
        CommitStatus::IdempotentReplay
    );
    let after = database
        .snapshot()
        .unwrap_or_else(|error| panic!("committed: {error}"));
    assert_eq!(after.version(), 1);
    assert_eq!(after.bundle_count(), 1);
    assert_eq!(after.replay_count(), 1);
    assert_eq!(after.pending_outbox(), 1);
}
