# SQLite shell refinement

## Boundary

`zeno-fcis-shell-sqlite` is a concrete imperative-shell adapter over pinned
`rusqlite` with bundled SQLite. Semantic decisions, patches, plans, candidate
identity, receipts, and bundles are supplied by the pure ZenoFCIS crates. The
database does not recalculate protocol meaning.

## Atomic set

One `BEGIN IMMEDIATE` transaction validates the expected semantic root and
publishes:

- canonical semantic state bytes, root, and monotonically increasing version;
- replay ID to candidate ID and exact complete bundle bytes;
- canonical receipt and complete bundle;
- every outbox entry, delivery ID, entry hash, destination, and payload.

The transaction commits only after all rows are written. Dropping the
transaction at any injected pre-commit crash point rolls back the complete set.
A crash immediately after commit leaves the new state and pending outbox rows,
so retry is an idempotent replay and delivery recovery is deterministic.

## Replay and delivery

A replay succeeds only when replay ID, candidate ID, and complete bundle bytes
all match. Candidate and `(candidate, ordinal)` uniqueness are database
constraints. Destination acknowledgement must return the exact committed
outbox-entry hash. `MemoryDestination` is the included deterministic idempotent
destination stub and rejects delivery-ID collisions.

## Failure model

Permanent fault-injection tests cover:

1. before transaction;
2. after bundle validation;
3. after semantic state/root update;
4. after replay, receipt, and bundle rows;
5. after outbox rows;
6. immediately before commit;
7. immediately after commit and before delivery.

The first six preserve the prior snapshot and publish no rows. The seventh
recovers through exact replay and the pending outbox.

## Trusted dependencies and bounds

SQLite transaction, locking, WAL/rollback behavior, the host filesystem, and
the pinned `rusqlite`/bundled SQLite closure enter the shell trusted computing
base. Every loaded state and outbox value is decoded with ZCVE full-consumption
and canonical re-encoding checks. SQLite integer conversions are checked.

## Nonclaims

The unit-test trace comparison covers the declared fixtures and crash points;
it is bounded testing, not a proof of SQLite or filesystem correctness. The
adapter does not implement replication, multi-process operational hardening,
online migration, backup/restore, key management, or a production delivery
transport. Production promotion still requires mounted runtime refinement,
filesystem fault testing, dependency review, and an audited migration plan.
