# V2 durable publication refinement

This development stage connects the library-owned V2 Authority to SQLite
schema v10 (v7 added the hash chain and checkpoints, v9 compact publications,
v10 the chained contract upgrade records). It is not a proof of SQLite, the
operating system, SHA-256, a remote destination or the full V2 release. The
actual implementation lives in `crates/zeno-fcis-shell-sqlite/src/v2.rs`, with
the pure upgrade decision in `src/v2/upgrade.rs`.

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

Schema opening compares the actual table/index/trigger definitions with the
library's fixed schema. Legacy schema v5 is explicitly refused. No unreviewed
automatic rewrite or reinterpretation of old certificates occurs.

## Contract upgrades and the lineage

Schema v10 adds `v2_upgrades`: chained records that move a store from the
contract its current history segment ran to another contract with the same
state schema. A store's segments are its genesis contract and one more per
record, each identified by the full Authority identity stored with it. A handle
is bound to a lineage of Authorities, oldest first; the segments must appear
in that lineage in order (a store may skip a version it never ran), and for
`open_lineage` the last segment must be the lineage's last member.
`V2SqliteShell::open` is the one-member case for stores that never upgraded.

`V2SqliteShell::upgrade(path, catalogs)` takes the application's whole lineage
as checked catalogs, binds their Authorities through the library, fully audits
the store, and asks the pure `v2::upgrade::decide` for the record. The decision
refuses, in order, differing canonical state schema bytes, equal identities,
and a current state the new contract's genesis publication does not admit; the
publication comes from the library's genesis evaluation, so the new contract's
genesis-flagged laws must hold on the state. The record is

```text
"ZFCISV2-UPGRADE\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity) || frame(genesis_publication)
  || frame(state_root) || frame(previous_chain)
```

and the new chain tip is `HashV2Chain(record)`; certificates begin with
`ZFCISV2-CERT\0`, so the two preimage sets are disjoint under one domain. The
row stores the ordinal, the head sequence, the identity, the compact genesis
publication, the root, the previous chain, the record and the chain; the head
row's chain moves to the new tip while its sequence, state and root stay. The
next commit's certificate extends the upgrade link. Replay walks events in
order: the commits of a segment under that segment's Authority, then every
record at the head it was taken at, recomputed from the stored fields and the
Authority it names, including `replay_genesis_publication` over the state at
that head. Pending deliveries keep their certificate-bound IDs and order, and
are recomputed under their own segment's Authority before delivery.
Checkpoints taken before or after an upgrade at the same head are told apart by
the chain link they carry.

A v9 store is exactly the previous shell's catalog; `open`, `open_lineage` and
`upgrade` refuse it as `Schema(9)`. `migrate_v9(path, lineage)` adds the table
and sets version 10 in one transaction after a complete audit under the member
whose identity created the store; any other schema, or a store under none of
the members, refuses and writes nothing. No data is migrated: a changed state
schema is outside this upgrade.

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
