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
private-construction `BoundInterpreter` minted by the same commit authority.
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
authorization bytes, and complete bundle bytes all match. Candidate,
authorization, and `(authorization, ordinal)` uniqueness are database
constraints. A pending row must map its authorization to the same candidate
under the shell policy. Destination acknowledgement must return the exact
committed outbox-entry hash.

Delivery IDs are derived from the implementation-neutral `CandidateId` and
canonical outbox entry. The deployment-specific `AuthorizationId` remains
separate provenance and must map to that candidate under the shell policy.
This makes pure reference and SQLite delivery identities byte-identical.
`deliver_next` uses the exact owned interpreter instance; it accepts no
caller-supplied destination.
`MemoryDestination` is the included deterministic idempotent destination stub
and rejects delivery-ID collisions.

Schema v4 is explicit in SQLite `user_version`. Schema v3 is rejected pending
an explicit migration because it has no nominal genesis authorization row.
Schema v2 also used authorization-derived delivery IDs and remains unsupported.
A populated unversioned legacy database is also rejected. No implicit migration
assigns genesis authority or silently reinterprets delivery identities.

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
tampering, deployment-policy substitution, and explicit schema-v3 rejection.

## Trusted dependencies and bounds

SQLite transaction, locking, WAL/rollback behavior, the host filesystem, and
the pinned `rusqlite`/bundled SQLite closure enter the shell trusted computing
base. Every loaded state and outbox value is decoded with ZCVE full-consumption
and canonical re-encoding checks. SQLite integer conversions are checked. The
shell identity is revalidated before snapshots, commits, acknowledgements, and
delivery reads.

## Nonclaims

The unit-test trace comparison covers the declared fixtures and crash points;
it is bounded testing, not a proof of SQLite or filesystem correctness. The
adapter does not implement replication, multi-process operational hardening,
online migration, backup/restore, key management, or a production delivery
transport. Production promotion still requires mounted runtime refinement,
filesystem fault testing, dependency review, and an audited migration plan.

The current adapter does not strictly decode `CommitBundle` bytes from storage
and reconstruct bidirectional equality between that decoded outbox plan and the
complete stored outbox row set. Row-local canonical checks, foreign keys,
authorization/candidate mapping, and atomic writes narrow this gap but do not
close issue #55 against an attacker who can rewrite database rows and recompute
local hashes. Delivery qualification remains blocked until strict receipt,
bundle, and authorization-envelope decoders support that reconstruction.
