# Strict artifact decoding and SQLite history

## Purpose

This boundary closes the gap between an internally valid database row and the
exact transition authorized by the functional core. Persisted data is treated
as untrusted input. Reopening, replaying, reading pending delivery work, and
acknowledging delivery all fail closed unless the database reconstructs the
same authority-bearing history.

## Inputs and outputs

The strict receipt boundary accepts canonical bytes plus explicit byte and
nested decoder limits. It returns a `Receipt`, `RejectReceipt`, or
`CommitBundle` only after complete decoding, reconstruction through private
smart constructors, and exact canonical re-encoding.

The authority boundary accepts canonical authorization bytes, the expected
pre-state, and explicit decode limits. It re-admits the persisted command and
context through the authority-owned schema and provider, executes the pinned
transition program and law engine, and returns a nominal
`CatalogAuthorizedTransition` only when the reconstructed authorization bytes
match exactly.

SQLite schema v5 records one positive, gap-free `state_version` for every
authorized transition. Reopen reconstructs the history from genesis through
that sequence and returns a shell only after the final state, root, and version
match `semantic_state` exactly.

## Authority boundary

Canonical bytes and nonzero hashes are bindings. They are not commit authority.
Decoders never deserialize directly into a nominal authorization witness.
Only the existing authority, which owns the reviewed program, catalog, law
engine, provider, interpreter binding, deployment binding, and resource limits,
can reauthorize persisted bytes.

The SQLite shell caches only transitions that were reauthorized during reopen
or supplied as nominal authorizations during the current process. Every read
of a persisted candidate compares the exact authorization, bundle, receipt,
replay, and outbox rows against that cache before use.

## Deterministic resource bounds

`ReceiptDecodeLimits`, `BundleDecodeLimits`, and `AuthorizationDecodeLimits`
bound canonical input bytes before allocation. Nested patch and plan decoding
uses their existing operation-count, path-depth, atom-count, value-size, and
collection limits. Schema admission uses the authority-owned transition
limits. SQLite additionally uses checked integer conversions and requires a
gap-free state-version sequence.

The current default complete-bundle and authorization input limit is 128 MiB.
Projects with narrower reviewed envelopes should construct smaller explicit
limits. Host time and memory pressure are operational concerns and do not
become semantic evidence.

## Laws

1. Decoding succeeds only for complete canonical input with no trailing bytes.
2. Every decoded candidate identity equals the identity recomputed from its
   candidate body.
3. Every decoded bundle is reconstructed by `CandidateBuilder::seal`; patch,
   plan, receipt, roots, and candidate identity therefore bind one candidate.
4. Persisted authorization re-entry re-executes the authority-owned transition
   and laws for the exact admitted invocation.
5. For every committed candidate, the authorization, bundle, receipt, replay,
   and complete outbox row set are equal to the reconstructed authorization.
6. Every required row exists exactly once, and no extra candidate row is
   accepted.
7. State versions are exactly `1..=N`; applying that sequence from authorized
   genesis yields the stored current state, root, and version.
8. `next_pending` returns an entry only when it is a member of the exact
   authorized bundle and its candidate-derived delivery identity matches.
9. Idempotent replay succeeds only after complete persisted-candidate
   revalidation.

## Negative cases

Permanent tests reject truncated and trailing receipt, bundle, and
authorization bytes; malformed decision flags; candidate, receipt, state,
invocation, policy, and nested-limit substitution; extra and missing outbox
rows; changed destinations or payloads with attacker-recomputed row hashes;
redundant bundle-byte changes; noncontiguous state versions; live current-state
replacement; version-zero genesis replacement; policy substitution; and every
injected pre-commit crash.

## Trusted dependencies and assumptions

The boundary assumes collision resistance for the approved SHA-256 provider,
correctness of the reviewed transition program and law engine, and the documented
transaction and durability behavior of the pinned `rusqlite` and bundled
SQLite closure. The host filesystem, process isolation, and delivery transport
remain deployment responsibilities.

## Explicit nonclaims

This work does not provide an online migration from schemas v4 or earlier,
prove SQLite or filesystem correctness, implement replication or multi-process
hardening, bound retained history length or total reopen work, authenticate a
persisted acknowledgement against direct database writes, qualify a production
delivery destination, or replace independent review. Re-execution is exact for
the pinned implementation and inputs; it is not an unbounded proof that the
implementation realizes every registered law.
