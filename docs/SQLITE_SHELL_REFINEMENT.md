# SQLite shell refinement

## Boundary

`zeno-fcis-shell-sqlite` is a concrete imperative-shell adapter over pinned
`rusqlite` with bundled SQLite. Its public commit port consumes a
private-construction `CatalogAuthorizedTransition<RustCryptoSha256, P, L, I>`.
It does not accept a raw `CommitBundle` or caller-selected replay identity.

Its creation port separately consumes one private-construction
`CatalogAuthorizedGenesis<RustCryptoSha256, P, L, I>`. Reopening an existing
store accepts no initial state and revalidates the persisted initial state,
root, policy, law evaluation, authorization bytes, and `GenesisId` under the
current authority before returning a usable shell.
If semantic state is still at version zero, its canonical bytes and root must
also equal that authorized genesis exactly.

The shell type carries the exact reviewed transition-program type `P` and owns
one concrete delivery-interpreter instance `I`. That instance must arrive in a
private-construction `BoundDeliveryInterpreter` minted by the same commit authority.
Its persisted identity pins the complete authorization policy and state-domain
name/version. Opening under another policy, substituting a token from another
policy, or mutating the stored identity fails closed.

## Atomic set

Creation uses one `BEGIN IMMEDIATE` transaction to publish the shell identity,
one canonical genesis row, and semantic state at version zero. Any populated
store rejects a second creation attempt.

One `BEGIN IMMEDIATE` transaction validates the expected semantic root and
publishes:

- canonical semantic state bytes, root, and monotonically increasing version;
- authorization ID, policy ID, invocation ID, replay ID, candidate ID, exact
  canonical authorization bytes, complete bundle bytes, and receipt bytes;
- redundant exact bundle and replay rows constrained to the authorization;
- every outbox entry, candidate-bound delivery ID, entry hash,
  destination, and payload.

The transaction commits only after all rows are written. Dropping the
transaction at any injected pre-commit crash point rolls back the complete set.
A crash immediately after commit leaves the new state and pending outbox rows,
so retry is an idempotent replay and delivery recovery is deterministic.

## Replay and delivery

A replay succeeds only when replay ID, authorization ID, candidate ID, exact
authorization bytes, and complete bundle bytes all match. Before returning
success, the shell revalidates the candidate's exact authorization, bundle,
receipt, replay, and complete outbox row set against the nominal authorization
held by the shell.

Candidate, authorization, state version, and `(authorization, ordinal)`
uniqueness are database constraints. A pending row must be an exact member of
the reconstructed bundle's `OutboxPlan`; row-local hashes and a candidate
foreign key are insufficient. Destination acknowledgement must return the exact
committed outbox-entry hash and repeats persisted-candidate validation first.

Delivery IDs are derived from the implementation-neutral `CandidateId` and
canonical outbox entry. The deployment-specific `AuthorizationId` remains
separate provenance and must map to that candidate under the shell policy.
This makes pure reference and SQLite delivery identities byte-identical.
`deliver_next` uses the exact owned interpreter instance; it accepts no
caller-supplied destination.
`MemoryDestination` is the included deterministic idempotent destination stub
and rejects delivery-ID collisions.

Schema v5 is explicit in SQLite `user_version`. Each authorization stores a
positive unique `state_version`, and reopen requires the exact gap-free sequence
`1..=N`. Starting from the reauthorized genesis, each persisted authorization
is strictly decoded, re-executed under the current authority, resealed, and
applied. The reconstructed final bytes, root, and version must equal
`semantic_state`.

Schema v4 is rejected pending explicit migration because it lacks that sequence
and was not reopened through complete history reauthorization. Schema v3 has no
nominal genesis authorization row. Schema v2 also used authorization-derived
delivery IDs. Populated unversioned legacy databases are rejected. No implicit
migration assigns authority or silently reinterprets history.

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

Additional creation/reopen tests cover an uninitialized store, one-time
creation, exact genesis revalidation without caller state, authorization-byte
tampering, deployment-policy substitution, and explicit schema-v4 rejection.
Adversarial persistence tests cover missing and extra outbox rows, changed
destination and payload bytes with recomputed row-local hashes, redundant bundle
changes, noncontiguous state versions, live committed-state replacement, and
version-zero genesis replacement.

## Trusted dependencies and bounds

SQLite transaction, locking, WAL/rollback behavior, the host filesystem, and
the pinned `rusqlite`/bundled SQLite closure enter the shell trusted computing
base. Every loaded state and outbox value is decoded with ZCVE full-consumption
and canonical re-encoding checks. Receipt, bundle, and authorization decoders
also require explicit byte and nested component limits, smart-constructor
reconstruction, and exact canonical re-encoding. SQLite integer conversions are
checked. The shell identity, cached reconstructed state/root/version, and exact
candidate row set are revalidated before snapshots, replay, acknowledgements,
and delivery reads.

## Nonclaims

The unit-test trace comparison covers the declared fixtures and crash points;
it is bounded testing, not a proof of SQLite or filesystem correctness. The
adapter does not provide an online migration from schema v4 or earlier,
replication, multi-process qualification, backup and restore evidence, key
management, a retained-history or total-reopen-work bound, persisted
acknowledgement authentication against direct database writes, filesystem fault
qualification, or a production delivery transport. Production promotion still
requires mounted runtime refinement, dependency review, and an audited
migration plan. Per-artifact reconstruction is bounded executable validation;
it is not a proof of SQLite, the filesystem, the transition program, or the law
engine.
