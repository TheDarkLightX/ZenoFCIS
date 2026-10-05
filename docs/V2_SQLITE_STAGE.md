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

Schema opening compares every `sqlite_master` object (type, name, table and
SQL, with no name filter) with the same objects built from the library's
fixed schema, so an added trigger or index is refused whatever its name.
Legacy schema v5 is explicitly refused. No unreviewed
automatic rewrite or reinterpretation of old certificates occurs.

## Contract upgrades and the lineage

Schema v10 adds `v2_upgrades`: chained records that move a store from the
contract its current history segment ran to a later contract of its lineage
with the same state schema. A store's segments are its genesis contract and
one more per record, each identified by the full Authority identity stored
with it. `v2::Lineage::bind(catalogs, receipts)` binds an application's
lineage from its checked catalogs, oldest first, with the SHA-256 of the
adoption receipt between each version and the next. The segments must appear
in the lineage in order; a store may skip a version it never ran. Each
segment takes the earliest version that fits, which a lineage that grows at
its end never changes. `Lineage::open` audits the store and returns a typed
handle: a v9 store, whose only operation is `migrate`; a current v10 store,
whose last segment has the last version's identity, which commits and
delivers; or a superseded one, which can be audited and read, or upgraded.
`V2SqliteShell::open` is the one-Authority case for stores that never
upgraded.

`Superseded::upgrade` fully audits the store under the whole lineage and asks
the pure `v2::upgrade::decide` for the record. The decision refuses, in
order, differing canonical state schema bytes, equal identities, and a state
the new contract does not admit. The record's magic is its kind tag, and the
new contract admits the state in one of two ways:

- `program-successor`. The shell computes the new contract's canonical policy
  with the library's serializer after replacing its decision program's
  instructions and roots and its Step limit with the old contract's. If that
  is byte for byte the old policy, the schemas, framing, channel links,
  bindings, branches, reasons, laws and every other limit are unchanged. The
  state at the head was admitted under those same laws, so it is admitted at
  any state with no genesis evaluation. Every adoption produces such a
  successor. The record also states which further premises held (below) and
  binds the digests of the adoption receipts, which `generate contract`
  replayed; the shell replays no receipt.
- `genesis-admission`. Any other contract must admit the state through the
  library's genesis evaluation, so its genesis-applicable laws must hold on
  it. A generated contract's initial-condition law 990 admits only the
  declared genesis state, so such a contract upgrades only stores at that
  state. A refusal carries the library's reason.

A program successor and the version it supersedes have the same reachable
states when five premises hold:

1. Their canonical policies are byte-identical except for the decision
   program's instructions and roots and the Step limit, so the genesis
   literals, the laws and the case table are identical. The shell checks this
   at the upgrade and again at every audit.
2. The identical laws include generated law 991, which pins every committed
   decision to the case table. Generation checks this for every adoption.
3. Each adoption receipt is F3 `Equivalent`: on every tuple of the full
   declared input domain the two programs give identical outputs or identical
   failures, under a declared Step limit that never binds. `generate contract`
   replays every receipt.
4. Neither version's Step limit binds. Steps are charged only for program
   instruction attempts and law nodes. The shell checks that each limit covers
   one Step for every program node and every law node; generation checks that
   each program's largest measured usage plus every law node fits its limit.
5. No law observes Step usage, the one meter reading a program change can
   alter. The shell checks this; generated laws cannot observe usage.

Under these premises the two versions admit the same genesis states and
commit the same decisions, with the same successor states, from the same
states, so they reach the same states. An upgrade at any reachable state
therefore keeps every property of reachable states: every law, every proved
inductive claim and the meaning of every decision example, not only the laws
evaluated on the current state. It does not keep the sealed Step usage. When
the receipt reports that usage differs, the usage observations sealed into
each publication change, and every sealed subject embeds the new contract
identity, so an adoption is a versioned change. The record's premises byte
states whether premises 4 and 5 held; premise 1 defines the kind, and premises
2 and 3 rest on the generation of the build that recorded the upgrade. When a
recorded premise did not hold, the record still shows that the laws are
unchanged, so every later commit passes them, but not that the reachable
states are.

The records are

```text
"ZFCISV2-SUCCESSOR\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity)
  || frame(premises:u8 || receipt_digests)
  || frame(state_root) || frame(previous_chain)

"ZFCISV2-UPGRADE\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity) || frame(genesis_publication)
  || frame(state_root) || frame(previous_chain)
```

where `premises` sets bit 0 for the policy comparison, bit 1 when both Step
limits cover every node and bit 2 when no law observes Step usage, and
`receipt_digests` concatenates the 32-byte SHA-256 of each adoption receipt
between the two versions, oldest first. The new chain tip is
`HashV2Chain(record)`. Certificates begin with `ZFCISV2-CERT\0`, and the three
magics first differ at byte 8, so the preimage sets are disjoint under one
domain. The row stores the ordinal, the head sequence, the identity, the
admission (the compact genesis publication, or the receipt digests), the root,
the previous chain, the record and the chain. The head row's chain moves to
the new tip while its sequence, state and root stay, and the next commit's
certificate extends the upgrade link. Replay walks events in order: the
commits of a segment under that segment's Authority, then every record at the
head it was taken at, recomputed from the stored fields and re-checked by its
kind. A genesis admission replays its genesis publication over the state at
that head; a program successor re-derives the policy comparison and its
premises from the two versions' catalogs and needs one receipt digest per
adoption it spans.
Pending deliveries keep their certificate-bound IDs and order, and are
recomputed under their own segment's Authority before delivery.

A replay key committed before an upgrade, retried with the same request
under the new contract, is an idempotent replay only when the new contract
seals the same publication apart from its identity, as after an adoption that
preserves usage; otherwise the retry is refused as `Replay`. Neither commits
twice.

A private checkpoint carries the chain link it was taken at. Its stored
marker is keyed by head alone, so a save at the same head after an upgrade
replaces the marker. The earlier checkpoint then opens as
`Error::Checkpoint`, and a full audit or a newer checkpoint opens the store.

The chain shows that every segment is valid under the lineage version it
names. It does not show who recorded an upgrade. Records and certificates are
unkeyed SHA-256, so anyone who can write the file can append a correctly
recomputed record, with any receipt digests, or roll the store back to an
earlier valid head. Detecting that needs a tip held outside the file, such as
an `UpgradeReceipt` or a private checkpoint.

A v9 store is exactly the previous shell's catalog. `V2SqliteShell::open`
refuses it as `Schema(9)`, and `Lineage::open` returns it as a v9 store.
`V9Store::migrate` adds the table and sets version 10 in one transaction after
a complete audit under the version whose identity created the store; any other
schema, or a store under no version, refuses and writes nothing. No data is
migrated: a changed state schema is outside this upgrade. Opening a store
never creates a missing file. Upgrade and migration each run in one immediate
transaction, with injected interruption points for the crash tests.

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
