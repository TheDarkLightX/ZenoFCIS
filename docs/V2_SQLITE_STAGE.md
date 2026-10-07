# V2 durable publication refinement

This development stage connects the library-owned V2 Authority to SQLite schema
v7. It is not a proof of SQLite, the operating system, SHA-256, a remote
destination or the full V2 release. The actual implementation lives in
`crates/zeno-fcis-shell-sqlite/src/v2.rs`.

## Object and obligations

The functional core produces a private, non-Clone `Publication` or
`GenesisPublication`. A publication retains the exact bound identity, original
inputs, complete audited decision, next-state envelope and both ordered delivery
lanes. Business Reject and technical refusal supply no committing capability.

For a fresh commit, the shell must establish:

```text
publication.identity == store.authority.identity
publication.original_state == store.current_state
certificate.pre_root == HashV2State(store.current_state)
certificate.publication_digest == HashV2Publication(publication.subject)
next_sequence == previous_sequence + 1
next_chain == HashV2Chain(certificate)
```

The certificate includes the sequence, replay key, pre-state root, post-state
root, previous chain link and complete publication digest. Original state bytes
remain available for core recomputation, while the durable certificate binds the
pre-state root. Replay keys are host inputs, not proof of caller authentication.

An immediate SQLite transaction publishes the state, certificate, replay
binding, complete publication and every delivery together. The in-memory anchor
advances only after SQLite commit. A requested interruption after commit reports
an interruption while retaining the durable result. Reusing a replay key succeeds
only for the exact original input/publication, and returns the original position
without duplicating delivery obligations. An old pre-state with a fresh key is
refused.

## Delivery and recovery

Pending items sort by commit sequence, lane and ordinal. The declared
idempotency atom is retained as a policy marker; it is not treated as a globally
unique ID. Actual outbox IDs use the existing domain-separated convention over
the genuine V2 publication digest followed by the original `OutboxEntry` bytes.
Effects have a distinct fixed domain so identical entries in the two lanes do
not alias. The V2 publication digest is a new identity; it is not represented as
a legacy 1.x candidate ID.

Before returning or acknowledging an item, the shell recomputes its owner's
complete publication and checks all stored lane bytes, type roots, markers and
content commitments. The supplied observed commitment must match. The built-in
memory destination is a concrete library interpreter; external delivery and host
authentication remain trusted integration responsibilities. Acknowledgement
bytes alone cannot prove that a real-world effect occurred.

Ordinary reopening re-admits genesis and replays every transition, certificate,
chain link and delivery, including acknowledged data. Normal calls compare the
cached validated tip instead of replaying every decision. Another connection's
write invalidates that cache and triggers a coherent audit. Every operation
checks the database version and current tip after taking its immediate
transaction. The cached version is captured while the validated transaction
still excludes other writers, never after releasing it. SQLite documents that
another connection's commit changes this connection's `data_version`, whereas
its own commits do not. See the [SQLite PRAGMA documentation](https://sqlite.org/pragma.html#pragma_data_version).

Only a previously validated shell can construct a private `Checkpoint`. A
checkpoint can reopen a store by checking its saved position and replaying the
tail. This deliberately trusts the checkpoint's provenance and previously
validated prefix; it does not newly audit the prefix's historical storage bytes.
`audit()` performs a full replay when that is required. A database row alone
cannot create a checkpoint capability.

Schema opening compares every `sqlite_master` object (type, name, table and
SQL, with no name filter) with the same objects built from the library's
fixed schema, so an added trigger or index is refused whatever its name.
Legacy schema v5 is explicitly refused. No unreviewed
automatic rewrite or reinterpretation of old certificates occurs.

## Qualification still required

At the native-04 source snapshot, all 13 new V2 tests, 29 legacy SQLite
regressions and strict all-target Clippy passed without source drift across
452 retained source/pin files. This is native refinement evidence, not a formal
proof of SQLite atomicity. Actual-contract native tests cover
commit/replay conflicts, all seven injected interruptions, a concrete hash-order
counterexample, destination acknowledgement, full reopening, checkpoints,
external history/delivery tampering, schema triggers and cross-policy custody.
Additional regressions exercise two live handles, live schema replacement,
usable audit checkpoints, and the destination-delivered but not yet acknowledged
retry window. The delivery ID oracle uses the existing library `OutboxEntry`
implementation.
They must pass on the final combined source, along with strict Clippy, existing
SQLite regressions, template acceptance and independent review. No native test
is relabelled as a formal atomicity proof.

The closed domain registry, mandatory public V2 API cutover, migration guide,
final source identity, complete proof/control qualification and release checks
remain separate V2 obligations. This stage does not close them by itself.
