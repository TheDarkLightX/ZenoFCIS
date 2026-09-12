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
        // A text ordinal sorts after the integer ordinal, so the added row is
        // read second: every row is read before any row field is checked.
        (
            "later-row-type-before-earlier-row-field",
            "UPDATE outbox SET entry_hash = X'00'; INSERT INTO outbox SELECT zeroblob(32), authorization_id, candidate_id, 'one', channel, destination_bytes, payload_bytes, entry_hash, acknowledged FROM outbox",
            "Sqlite(InvalidColumnType(3, \"ordinal\", Text))",
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
fn repeated_reads_survive_schema_change_corruption_and_reopen() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    database
        .commit(authorized(&authority, &catalog, 9))
        .unwrap_or_else(|error| panic!("commit: {error}"));
    let snapshot = database
        .snapshot()
        .unwrap_or_else(|error| panic!("first snapshot: {error}"));
    let pending = database
        .next_pending()
        .unwrap_or_else(|error| panic!("first pending: {error}"))
        .unwrap_or_else(|| panic!("missing pending delivery"));
    // Repeating both reads must return the same stored data.
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("repeat snapshot: {error}")),
        snapshot
    );
    assert_eq!(
        database
            .next_pending()
            .unwrap_or_else(|error| panic!("repeat pending: {error}")),
        Some(pending.clone())
    );

    // The same reads must continue to work after a schema change.
    database
        .connection
        .execute_batch("ALTER TABLE outbox ADD COLUMN observed_at INTEGER;")
        .unwrap_or_else(|error| panic!("schema change: {error}"));
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("recompiled snapshot: {error}")),
        snapshot
    );
    assert_eq!(
        database
            .next_pending()
            .unwrap_or_else(|error| panic!("recompiled pending: {error}")),
        Some(pending.clone())
    );

    // Failed recompilation must report the same error as a fresh preparation.
    database
        .connection
        .execute_batch("ALTER TABLE outbox RENAME COLUMN payload_bytes TO missing_payload;")
        .unwrap_or_else(|error| panic!("rename payload: {error}"));
    let Err(warm_error) = database.snapshot() else {
        panic!("missing payload column must fail");
    };
    database.connection.flush_prepared_statement_cache();
    let Err(cold_error) = database.snapshot() else {
        panic!("missing payload column must fail after flushing");
    };
    assert!(matches!(warm_error, SqliteShellError::Sqlite(_)));
    assert_eq!(format!("{warm_error:?}"), format!("{cold_error:?}"));
    database
        .connection
        .execute_batch("ALTER TABLE outbox RENAME COLUMN missing_payload TO payload_bytes;")
        .unwrap_or_else(|error| panic!("restore payload column: {error}"));

    // Later corruption must be rejected, and the repaired row must be read
    // again instead of answered from an earlier result.
    database
        .connection
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .unwrap_or_else(|error| panic!("corruption setup: {error}"));
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("warm snapshot: {error}")),
        snapshot
    );
    database
        .connection
        .execute("UPDATE outbox SET entry_hash = X'00'", [])
        .unwrap_or_else(|error| panic!("corrupt entry hash: {error}"));
    assert!(matches!(
        database.snapshot(),
        Err(SqliteShellError::InvalidHashLength)
    ));
    assert!(matches!(
        database.next_pending(),
        Err(SqliteShellError::InvalidHashLength)
    ));
    database
        .connection
        .execute(
            "UPDATE outbox SET entry_hash = ?1",
            [pending.entry_hash().as_bytes().as_slice()],
        )
        .unwrap_or_else(|error| panic!("repair entry hash: {error}"));
    database
        .connection
        .execute_batch("PRAGMA ignore_check_constraints = OFF;")
        .unwrap_or_else(|error| panic!("corruption teardown: {error}"));
    assert_eq!(
        database
            .next_pending()
            .unwrap_or_else(|error| panic!("repaired pending: {error}")),
        Some(pending)
    );

    // Reopening revalidates stored rows using the same connection.
    let reopened = reopen(database, &authority);
    assert_eq!(
        reopened
            .snapshot()
            .unwrap_or_else(|error| panic!("reopened snapshot: {error}")),
        snapshot
    );
}

fn authorized_from_state(
    authority: &TestAuthority,
    catalog: &ProjectCatalog,
    pre_state: Value,
    replay: u8,
) -> CatalogAuthorizedTransition<RustCryptoSha256, SqliteProgram, SqliteLawEngine, MemoryDestination>
{
    let admitted = SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
        catalog.schema(),
        pre_state,
        ValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("pre-state: {error}"));
    let invocation = authority
        .admit_invocation(
            admitted,
            command(catalog),
            context(catalog),
            hash(60),
            hash(61),
            hash(replay),
        )
        .unwrap_or_else(|error| panic!("invocation: {error}"));
    let decision: CatalogAuthorizationDecision<_, _, _, _> = authority
        .execute(invocation)
        .unwrap_or_else(|error| panic!("execute: {error}"));
    match decision {
        Decision::Accept(accepted) => accepted.into_candidate(),
        Decision::Reject(_) | Decision::CommittedFailure(_) => {
            panic!("test program must accept")
        }
    }
}

#[test]
fn repeated_reads_match_across_a_three_transition_history() {
    let catalog = catalog();
    let authority = authority(&catalog, 53);
    let mut database = shell(&authority, &catalog);
    for (step, replay) in [21_u8, 22, 23].into_iter().enumerate() {
        // Each step starts from the state the previous step committed.
        let pre_state = if step == 0 {
            Value::U128(0)
        } else {
            Value::U128(11)
        };
        let transition = authorized_from_state(&authority, &catalog, pre_state, replay);
        assert_eq!(
            database
                .commit(transition)
                .unwrap_or_else(|error| panic!("commit {step}: {error}")),
            CommitStatus::Committed
        );
    }
    // Distinct replay identities bind distinct candidates even though the
    // test program always produces the same output.
    assert_eq!(database.validated_history.len(), 3);
    let committed = database
        .snapshot()
        .unwrap_or_else(|error| panic!("history snapshot: {error}"));
    assert_eq!(committed.version(), 3);
    assert_eq!(committed.bundle_count(), 3);
    assert_eq!(committed.replay_count(), 3);
    assert_eq!(committed.pending_outbox(), 3);
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("repeat history snapshot: {error}")),
        committed
    );

    // A row outside the first candidate must still be checked after reuse.
    let (last_candidate, last_record) = database
        .validated_history
        .iter()
        .next_back()
        .unwrap_or_else(|| panic!("missing last candidate"));
    let last_candidate = *last_candidate;
    let payload = last_record.bundle.outbox_plan().entries()[0]
        .payload()
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("last payload: {error}"));
    database
        .connection
        .execute(
            "UPDATE outbox SET payload_bytes = X'00' WHERE candidate_id = ?1",
            [last_candidate.hash().as_bytes().as_slice()],
        )
        .unwrap_or_else(|error| panic!("corrupt later candidate: {error}"));
    let changes = database.connection.total_changes();
    assert!(matches!(
        database.snapshot(),
        Err(SqliteShellError::CorruptOutbox)
    ));
    assert_eq!(database.connection.total_changes(), changes);
    database
        .connection
        .execute(
            "UPDATE outbox SET payload_bytes = ?1 WHERE candidate_id = ?2",
            params![payload, last_candidate.hash().as_bytes().as_slice()],
        )
        .unwrap_or_else(|error| panic!("repair later candidate: {error}"));
    assert_eq!(
        database
            .snapshot()
            .unwrap_or_else(|error| panic!("repaired history: {error}")),
        committed
    );

    // Every candidate is revalidated through the same SQL, so a retained
    // binding would answer one candidate's read with another's row.
    for remaining in [3_u64, 2, 1] {
        let pending = database
            .next_pending()
            .unwrap_or_else(|error| panic!("pending {remaining}: {error}"))
            .unwrap_or_else(|| panic!("missing pending delivery {remaining}"));
        assert_eq!(
            database
                .next_pending()
                .unwrap_or_else(|error| panic!("repeat pending {remaining}: {error}")),
            Some(pending.clone())
        );
        let candidate = pending.candidate_id();
        assert!(database.validated_history.contains_key(&candidate));
        assert_eq!(
            database
                .snapshot()
                .unwrap_or_else(|error| panic!("pending count {remaining}: {error}"))
                .pending_outbox(),
            remaining
        );
        database
            .acknowledge(pending.delivery_id(), pending.entry_hash())
            .unwrap_or_else(|error| panic!("acknowledge {remaining}: {error}"));
    }
    assert!(
        database
            .next_pending()
            .unwrap_or_else(|error| panic!("settled pending: {error}"))
            .is_none()
    );
    let settled = database
        .snapshot()
        .unwrap_or_else(|error| panic!("settled snapshot: {error}"));
    assert_eq!(settled.pending_outbox(), 0);

    let reopened = reopen(database, &authority);
    assert_eq!(
        reopened
            .snapshot()
            .unwrap_or_else(|error| panic!("reopened snapshot: {error}")),
        settled
    );
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
