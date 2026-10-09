# V2 durable publication refinement

This development stage connects the library-owned V2 Authority to SQLite
schema v10 (v7 added the hash chain and checkpoints, v9 compact publications,
v10 the chained contract upgrade records). It is not a proof of SQLite, the
operating system, SHA-256, a remote destination or the full V2 release. The
actual implementation lives in `crates/zeno-fcis-shell-sqlite/src/v2.rs`, with
the pure upgrade decision in `src/v2/upgrade.rs` and the delivery lifecycle in
`src/v2/delivery.rs`.

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
content commitments. The commitment the destination reported must match. The
built-in memory destination is a concrete library interpreter; external
delivery and host authentication remain trusted integration responsibilities.
Acknowledgement bytes alone cannot prove that a real-world effect occurred.

### Typed delivery lifecycle

Delivery is a lifecycle of consuming tokens, `Pending` → `Delivered` →
acknowledged, in `src/v2/delivery.rs`:

```rust
impl<'a, 'p> V2SqliteShell<'a, 'p> {
    pub fn next_pending(&mut self) -> Result<Option<Pending<'_, 'a, 'p>>, Error>;
}
impl<'s, 'a, 'p> Pending<'s, 'a, 'p> {
    pub fn delivery(&self) -> &Delivery;
    pub fn deliver(self, destination: &mut MemoryDestination)
        -> Result<Delivered<'s, 'a, 'p>, Error>;
}
impl Delivered<'_, '_, '_> {
    pub fn delivery(&self) -> &Delivery;
    pub fn acknowledge(self) -> Result<(), Error>;
}
```

`next_pending` issues the oldest pending entry as a `Pending` token after
replaying its owning commit. `deliver` hands the entry to the library memory
interpreter and returns a `Delivered` token without writing the store.
`acknowledge` marks the entry acknowledged in one immediate transaction. A
`Delivery` is the stored record as plain data, with its position, content and
certificate-bound ID; it grants nothing. Delivering every pending entry is:

```rust
while let Some(pending) = shell.next_pending()? {
    pending.deliver(&mut destination)?.acknowledge()?;
}
```

The types enforce four things:

- Only the store issues tokens, through `next_pending` or `pending_by_id`.
  Their fields are private, and they have no public constructor, `Clone`,
  `Default` or conversion from a `Delivery`, so a `Delivered` token exists
  only after `deliver` or `relayed`.
- Only `Delivered` has `acknowledge`, so acknowledging a `Pending` token
  directly does not compile. The types do not show that the entry reached
  anything: `relayed` makes a `Delivered` token from the SHA-256 of the
  stored payload, which any holder of the token can compute, so an
  acknowledgment through `relayed` rests on the relay's report of its
  outbound call, not on the types.
- Transitions take their token by value, so a consumed token cannot be used
  again and one token cannot acknowledge twice.
- A token holds the exclusive borrow of the handle that issued it, and both
  transitions act through that handle. No operation takes a token and a
  store, so a token cannot reach another store or another lineage's handle.
  It cannot outlive its handle, and the handle issues no second token while
  one is live.

Rustdoc `compile_fail` examples on `v2::Pending` and `v2::Delivered` show
each misuse failing with its stated error code (E0599, E0382, E0451, E0499,
E0505), each beside a compiling example written the same way apart from the
misuse. Stable rustdoc checks only that a `compile_fail` example fails, not
why, so `python3 tools/check_compile_fail.py` checks the reason on the pinned
stable toolchain. It reads every example from these doc comments, requires the
blocks it found to be the ones rustdoc lists, compiles each against the built
library with `rustc --error-format=json`, and requires each misuse to fail
with exactly one error carrying its stated code, and each paired example to
compile. The ATDD SQLite scenario and the strict SQLite history workflow run
it, with planted controls in `tools/test_check_compile_fail.py`.

Run-time checks stay where the types cannot reach:

- `next_pending` audits a store another connection wrote and requires the
  store to run the handle's current contract.
- `deliver` binds the interpreter by its identity and decodes the stored
  destination and payload; the destination refuses a delivery ID it already
  holds with other content.
- `acknowledge` checks the cached head again, replays the owning commit and
  requires the entry hash the destination reported to equal the stored one.
  A refusal leaves the entry pending. After another connection upgrades the
  store, a token of a handle bound to the old version is refused as
  `Identity`.

The types reach one handle, not the file. Two handles or two processes can
each deliver an entry and acknowledge it; the second acknowledgment finds it
acknowledged and leaves it so, except that a relay's acknowledgment is then
refused as `AlreadyAcknowledged` (see the relay protocol below). A dropped token or a failed transition leaves
the entry pending, and the store issues it again, so delivery is at least
once; the memory destination honours delivery IDs and records one effect. A
token cannot survive a restart: after a crash between delivery and
acknowledgment, the reopened store issues the entry again with the same ID. A
`Delivered` token shows that the library's memory destination accepted the
entry in this process, not that an external system acted on it.

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

### Relay protocol

A relay delivers a store's entries to an external system from another
process, through the same lifecycle. The protocol is in `src/v2/relay.rs`
and adds no dependency; an in-process HTTP client would change `Cargo.lock`
and so the evaluator identity.

```rust
impl V2SqliteShell<'_, '_> {
    pub fn export_pending(&mut self) -> Result<Vec<Delivery>, Error>;
    pub fn pending_by_id(&mut self, id: Hash32) -> Result<Pending<'_, '_, '_>, Error>;
}
impl<'s, 'a, 'p> Pending<'s, 'a, 'p> {
    pub fn relayed(self, payload_sha256: Hash32) -> Result<Delivered<'s, 'a, 'p>, Error>;
}
pub mod relay {
    pub fn export_line(delivery: &Delivery) -> String;
    pub fn acknowledge(shell: &mut V2SqliteShell<'_, '_>, id: Hash32,
                       payload_sha256: Hash32) -> Result<(), Error>;
}
```

- **Export.** `export_pending` lists every pending entry in commit order
  (commit, lane, ordinal), after replaying each owning commit, and writes
  nothing. `export_line` writes one as a line of JSON with the schema
  `zeno-fcis/relay-export/1`: `commit`, `lane`, `ordinal`, `delivery_id`,
  `channel`, `destination_root`, `payload_root`, `destination` and `payload`
  (canonical bytes in hexadecimal), `payload_sha256` and `entry_hash`. The
  stream is durable because the store is: an entry appears in every export
  until it is acknowledged, with the same ID and bytes.
- **Acknowledge.** `relay::acknowledge` takes a delivery ID and the SHA-256
  of the payload the relay sent. `pending_by_id` issues that entry's
  `Pending` token after replaying its commit, `relayed` turns it into a
  `Delivered` token, and `acknowledge` marks it. Three refusals are named:
  `UnknownDelivery` for an ID the store does not hold, `PayloadMismatch` for
  a hash other than the SHA-256 of the stored payload, and
  `AlreadyAcknowledged` for an entry already acknowledged. `acknowledge`
  repeats the last two checks in its immediate transaction, so an
  acknowledgment that another process made first is refused too. A refusal
  changes no delivery and no commit. When another connection wrote the store
  first, the call audits it before anything else, and that audit records its
  checkpoint, as every audit does.

The reference relay, `tools/relay.py`, uses only the Python standard library.
It runs an application's export command, and for each line in order makes one
outbound call: an HTTP POST, with the delivery ID as the `Idempotency-Key`,
or an appended line in a local queue file. Every attempt sends the same
bytes. Only a 2xx answer to the POST itself counts as sent; the relay
follows no redirect and treats a 3xx answer as a final refusal, since
a client that follows a 301, 302 or 303 (urllib, for one) repeats it as a
GET that has no body. A queue
append does not join a cut line to the next: a short write is cut back to the
file's previous size, and an append to a file that does not end in a
newline starts with one. A queue consumer keeps the first line per
`delivery_id` and skips any line that is not complete JSON. `--timeout`
limits a whole HTTP call, from connecting to the last byte of the answer,
not each socket read; at the deadline the relay shuts the connection down
and the call counts as timed out. A store command still running after
`--command-timeout` seconds is killed and counts as failed. A failed or
timed-out call is retried with bounded exponential backoff; after the last attempt the entry stays pending and the relay stops.
After a successful call it runs the application's acknowledgment command
with the delivery ID and the SHA-256 of the payload it sent. The
withdrawal-queue template has both commands (`--relay-export` and
`--relay-acknowledge`); the generated application's command line (G14.3)
is planned to use the relay for its `deliver`.

**Strength.** Transport is at least once, and every attempt carries the same
identity: the same delivery ID and the same payload. A relay that stops
after its export, after an outbound call or after an acknowledgment loses no
delivery, because the store keeps the entry pending until the
acknowledgment commits; on its next run the relay sends again what was not
acknowledged. A receiver that honours idempotency keys sees each effect
once; one that does not may act twice, and a queue file may hold the same
line twice. The shell cannot observe the external system: an acknowledgment
records that a relay presented an exported ID with the stored payload's
hash, not that a receiver acted on it. The hash binding catches a relay that
mixes up entries or stores; it is not authentication, since anyone who can
run the acknowledgment command can present an exported line.

What was run: `crates/zeno-fcis-shell-sqlite/tests/relay.rs` checks the
export order and repeatability and each named refusal with an unchanged
store; `tools/test_relay.py` checks the relay and the test receiver
(`tools/relay_receiver.py`) against a stand-in store, including a crash at
each step, a timeout, a receiver that answers a byte at a time past the
deadline, and a hung store command; and `tools/check_app_journey.py` runs the relay
against the withdrawal-queue application's real SQLite store, with injected
crashes at each step, a receiver that fails once and times out once, and
forged and mismatched acknowledgments, and requires each payout to reach the
receiver exactly once under the ID its commit bound. These are tests of the
listed cases, not a proof over every interleaving.

## Contract upgrades and the lineage

Schema v10 adds `v2_upgrades`: chained records that move a store from the
contract its current history segment ran to a later contract of its lineage,
with the same state schema unless the lineage declares a data migration or a
rename (see [Data migrations and renames](#data-migrations-and-renames)). A store's segments are its genesis contract and
one more per record, each identified by the full Authority identity stored
with it. `v2::Lineage::bind(catalogs, receipts)` binds an application's
lineage from its checked catalogs, oldest first, with the SHA-256 of the
adoption receipt between each version and the next, and a cap on the input
tuples one comparison of two decision programs may enumerate: by default
100,000,000, the default of `zeno-fcis transform check`, or another value
through `Lineage::bind_with_cap`. `Lineage::bind_steps(catalogs, steps)`
binds a lineage whose `v2::Step` between two versions is an adoption, with
its receipt digest; a behaviour change: a reviewed rule change, with the
owner's review text and the inductive claims of the contract it leads to,
compiled to law programs over the state (`v2::behaviour::Claim`); a data
migration (`v2::migration::Migration`); or a rename. `zeno-fcis contract
evolve` records each of the last three, and a generated contract's
`with_lineage` passes them. The segments must appear
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
new contract admits the state in one of five ways. An upgrade across a
migration or rename step takes one hop per such step and one per run of
other steps between them, each hop its own record at the same head and each
decided as below, all in one transaction; a span without either is one hop,
exactly as before. The decision's schema check applies to every hop but a
migration or rename.

- `migration` and `rename`, for a hop across a step the lineage declares as
  one; nothing else is tried. See [Data migrations and
  renames](#data-migrations-and-renames).
- `behaviour-change`, whenever the lineage declares a behaviour change
  within the hop; neither of the last two is then tried. See [Behaviour
  changes](#behaviour-changes).

- `program-successor`, only when the shell itself establishes all five
  premises below from the two versions' bound catalogs
  (`v2::upgrade::Successor::establish`). The upgrade is then admitted at any
  state with no genesis evaluation.
- `genesis-admission`, whenever a premise is missing. The new contract must
  admit the state through the library's genesis evaluation, so its
  genesis-applicable laws must hold on it. A generated contract's
  initial-condition law 990 admits only the declared genesis state, so such a
  contract upgrades only stores at that state. A refusal names the first
  missing premise and carries the library's reason, and writes nothing.

The premises, as the shell establishes them, in the order it checks them:

- Premise 1, policy. The shell computes the new contract's canonical policy with the
   library's serializer after replacing its decision program's instructions
   and roots and its Step limit with the old contract's, and requires it to
   be byte for byte the old policy. The schemas, framing, channel links,
   bindings, branches, reasons, genesis literals, laws, required law list,
   every other limit and the program's input and output domains are then
   identical.
- Premise 2, decision law. Each contract declares law 991 with kind
   `DecisionConformance`, scope `Always` and not at genesis, and lists it as
   required. This is the generator's own test for law 991, plus the required
   list. It does not show what the law's predicate says: the shell has no
   case table, so it cannot check that the predicate encodes one. Because the
   laws are identical by premise 1, both versions evaluate the same predicate
   on every decision.
- Premise 4, Step limits. Each contract's Step limit covers one Step for every program
   node and every law node, the most one evaluation can charge, so neither
   limit can refuse a decision.
- Premise 5, Step usage. No law of either contract observes Step usage, the one meter
   reading a program change can alter.
- Premise 3, equivalence, checked last because it is the costly one. The shell
   compares the two decision programs itself, with `v2::equivalence::compare`,
   on every tuple of the ordered product of their declared input domains,
   through the library evaluator at the full budget of 256 Steps, so no Step
   refusal can mask a difference. Each tuple must give identical output
   tuples or the identical evaluator failure. The domain's size is computed
   with checked arithmetic; above the lineage's cap the premise is not
   established. This is the predicate of `zeno-fcis transform check` (F3)
   without its declared Step limit, which premise 4 replaces; a test in the
   CLI crate compiles the module and checks that both give the same verdict
   on F3's known answers, the 32 benchmark cases, planted defects and the
   withdrawal-queue adoption. For the withdrawal queue the shell compares the
   two programs on 1,296,000 input tuples. It is an exhaustive finite
   comparison by a tested checker, not a formal proof.

Under these premises the two versions admit the same genesis states and
commit the same decisions, with the same successor states, from the same
states, so they reach the same states. An upgrade at any reachable state
therefore keeps every property of reachable states: every law, every proved
inductive claim and the meaning of every decision example, not only the laws
evaluated on the current state. It does not keep the sealed Step usage. When
the programs' usage differs, the usage observations sealed into each
publication change, and every sealed subject embeds the new contract
identity, so an adoption is a versioned change.

The adoption receipt digests are provenance, not evidence. The shell replays
no receipt: it establishes equivalence itself. A program-successor record
stores the digests the upgrading lineage declares for the adoptions it
spans, and an audit requires them to be, value for value, the digests the
auditing lineage declares; a lineage with the same catalogs and other
digests does not audit the store. They name which `transform` receipts the
application's generation replayed.

The records are

```text
"ZFCISV2-SUCCESSOR\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity)
  || frame(premises:u8 || tuples:u64be || receipt_digests)
  || frame(state_root) || frame(previous_chain)

"ZFCISV2-UPGRADE\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity) || frame(genesis_publication)
  || frame(state_root) || frame(previous_chain)
```

where `premises` is `0x1f`, bits 0 to 4 for premises 1 to 5, all set because
a record exists only when all five held; `tuples` is the number of input
tuples the shell compared; and `receipt_digests` concatenates the 32-byte
SHA-256 of each adoption receipt between the two versions, oldest first. The
new chain tip is
`HashV2Chain(record)`. Certificates begin with `ZFCISV2-CERT\0`, and every
magic, the behaviour change's below included, first differs from the others
at byte 8, so the preimage sets are disjoint under one domain. The row stores the ordinal, the head sequence, the identity, the
admission (the compact genesis publication, or the receipt digests), the root,
the previous chain, the record and the chain. The head row's chain moves to
the new tip while its sequence, state and root stay, and the next commit's
certificate extends the upgrade link. Replay walks events in order: the
commits of a segment under that segment's Authority, then every record at the
head it was taken at, recomputed from the stored fields and re-checked by its
kind. A genesis admission replays its genesis publication over the state at
that head. A program successor establishes all five premises again from the
auditing lineage's catalogs under its cap, exactly as the upgrade did,
including the full comparison of the two programs, and requires the stored
digests to be that lineage's. When either fails, the store may still be
intact, so the open ends with `Error::Succession`, not `Error::History`. It
carries `upgrade::Unsupported::Receipts` for a record that is exact under
the digests it stores but names other digests than the lineage declares, or
`upgrade::Unsupported::Premise` with the premise the lineage's own catalogs
do not establish, such as a domain above its comparison cap. A record that
does not match its own stored digests is still `Error::History`. Nothing is
written.

The cost is plain. Every process that upgrades a store, or fully opens a
store that holds a program-successor record, compares the two programs once
on every input tuple of their domain. A bound `Lineage` keeps each pair's
outcome, failures included, in memory only, so its later opens and audits
compare nothing again; another `Lineage` value, as in another process,
compares again, and nothing is ever kept in the store. Two programs with
identical instructions and roots, as when only the Step limit changes, are
equal without enumeration; there is no other shortcut, such as sampling or
skipped inputs. For the withdrawal queue's 1,296,000 tuples, one comparison
took about 3 seconds in a release build of a generated application and
about 4 seconds in a debug build, whose manifest optimises
`zeno-fcis-synthesis`, measured on a shared machine. A checkpoint open
compares only for records after its checkpoint, usually none. Enumeration
runs on one thread; parallel enumeration is planned, not built.

No comparison runs while a write transaction is open. What a comparison
establishes depends only on two catalogs, so an upgrade or a full open first
finds the pairs it needs with plain reads and establishes them. Its
immediate transaction then re-reads the store, as before, and only looks the
pairs up without waiting for the shared memo. If another connection changed
the store so that a pair is missing, or a comparison has the memo locked,
the attempt writes nothing and releases its transaction before establishing
or waiting for the pair. It then retries within the existing lineage-length
bound; repeated store changes or comparison-cache contention can exhaust
that bound and return `Error::Unsettled`. The write
lock is therefore held about as long as before comparisons existed: tens of
milliseconds for an upgrade or an open of the withdrawal queue in the
measurements.
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
recomputed record, with the lineage's receipt digests, or roll the store back to an
earlier valid head. Detecting that needs a tip held outside the file, such as
an `UpgradeReceipt` or a private checkpoint.

A v9 store is exactly the previous shell's catalog. `V2SqliteShell::open`
refuses it as `Schema(9)`, and `Lineage::open` returns it as a v9 store.
`V9Store::migrate` adds the table and sets version 10 in one transaction after
a complete audit under the version whose identity created the store; any other
schema, or a store under no version, refuses and writes nothing. No data is
migrated by this step: a changed state schema is a declared data migration,
below. Opening a store
never creates a missing file. Upgrade and migration each run in one immediate
transaction, with injected interruption points for the crash tests.

## Behaviour changes

A behaviour change is a rule change the owner reviewed: the classifier of
`zeno-fcis contract diff` called it `rule-change`, so the schema and the
channels are unchanged, and `contract evolve` recorded the classifier's
plain-language account as the review. The decisions may change, so neither a
program succession nor a forward simulation describes it, and a generated
contract's law 990 would refuse any store away from its declared genesis
state.

`Superseded::upgrade` admits it, with `v2::behaviour`, when on the store's
current state:

- every *state law* of the new contract holds: each law that applies at
  genesis and to every committing decision (scope `Committing` or `Always`),
  other than an `InitialCondition` law; and
- every claim the lineage declares for the new contract holds. A generated
  contract declares each of its inductive claims, restated over the state,
  whether or not `prove` ran for it.

The shell checks them with the library's law evaluator through the verified
core's own genesis framing, the public `v2_composition::bind` and
`BoundCore::frame`, so no entry point is added to the core. It binds a copy of
the new contract's descriptor in which every state law keeps its program,
every other law's program is the literal `true`, each claim joins as a state
invariant, and the Read and Step limits grow by one for each claim node. The
core then decodes the state under the new contract's schema and framing,
which also checks that every value is admissible, and evaluates every law on
that one frame. Any law or claim that is false, has no value or exceeds the
budget refuses the upgrade, naming it (`upgrade::Refusal::Behaviour`), and
nothing is written. A claim whose ID is a law's cannot be evaluated beside it
and refuses as `Unevaluable`; generation refuses such a claim first.

Genesis exactness, law 990, applies only to new stores. Its predicate, like
that of every other law that applies at genesis but is not a state law, is
replaced by `true`, so it is never evaluated on the store's state; the record
lists these laws as not evaluated. No decision programs are compared, so a
behaviour-change upgrade, and an audit of its record, never wait on a
comparison or on the lineage's comparison memo.

The record is

```text
"ZFCISV2-BEHAVIOUR\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity)
  || frame(0x01 || list(state_law_ids) || list(claim_ids)
           || list(unevaluated_law_ids) || count:u32be || review_digests)
  || frame(state_root) || frame(previous_chain)
```

where each `list` is a `u32be` count and `u32be` IDs, the law lists in the
new contract's law order and the claims in their declared order, and
`review_digests` concatenates the 32-byte SHA-256 of each review between the
two versions, oldest first. Its magic first differs from the other three at
byte 8. The row stores the review texts beside the record in its admission
column, each a `u64be` length and its bytes. An audit evaluates the laws and
claims on the state at the record's head again, with the auditing lineage's
catalogs and claims, recomputes the record and requires it, and the stored
texts, to be exactly what that lineage declares. A record exact under the
texts stored beside it but with other reviews than the lineage declares, or
under a lineage that declares an adoption for the step, ends the open with
`Error::Succession(upgrade::Unsupported::Reviews)`: the store may be intact.
Any other mismatch is `Error::History`.

What a behaviour-change upgrade establishes, at its true strength:

- every law holds on every state committed after it, because the Authority
  checks the laws at each commit, as for any store;
- each declared claim holds on the upgrade state, by this check, and on every
  later committed state when its induction step holds for the laws it
  assumes, which `prove` attests with a solver; the upgrade itself checks
  only the upgrade state;
- facts that rest only on reachability from the new contract's genesis do
  not carry over: the history was made under the old rules, so the store may
  hold a state the new contract could never reach from its own genesis.

`Lineage::audit_read_only` audits a store at any version of the lineage
without writing: it opens the file read-only, replays every segment in one
read transaction and saves no checkpoint. A new build therefore checks an
old store before it upgrades it; the generated application's `--audit` uses
it.

## Data migrations and renames

A data migration changes the state's layout between two consecutive
versions: a declared `v2::migration::Migration` gives every field of the new
state record a value from the old state, an old field's value (carried or
renamed), a constant (an added field's default) or a total table over one
old field's values (one part of a split). Values are numbers: 0 or 1 for a
Boolean, the integer, a variant's ID. `zeno-fcis contract evolve
--migration` records it from a `zeno-fcis/migration/1` file, and generation
compiles it into the application's `STATE_STEPS`.

The shell admits a migration only by forward simulation
(`v2::migration::simulate`), from the two versions' bound catalogs and
Authorities:

- every state of the old version's declared state domain on which its state
  laws hold, as `v2::behaviour` evaluates them, is kept. A store's state
  satisfies them: its genesis and commits are checked against them, a
  behaviour-change hop checks the new laws on the state it keeps, a rename
  keeps the laws, and the next item makes a migrated state satisfy the new
  laws;
- for each kept state `s` the old genesis evaluation admits, the new one must
  admit `m(s)`; with law 990 this means `m` maps the old genesis state to the
  new one;
- for each kept state `s`, every state law of the new version must hold on
  `m(s)`. A migration that maps a state the old laws allow to one the new
  laws forbid is refused naming the observation `new-state-laws`, even when
  no commit of the old version reaches that state. The check is made here,
  over every kept state, and not on the store's head at the hop: the hop
  and every audit re-run this simulation, so a migrated head satisfies the
  new laws because the head satisfied the old ones;
- for each kept state and every command and context of the declared domain,
  the new Authority's publication over `m(s)` must give the old one's
  technical refusal, or its decision class and reason, its deliveries in
  order (lane, ordinal, channel, destination, payload and idempotency value
  of the library's candidate) and, for a commit, a successor state equal to
  `m` of the old successor, byte for byte.

The whole domain, every state, command and context, must have at most 2^20
tuples, the cap of `zeno-fcis contract review`; a larger one is refused as
inconclusive, with nothing enumerated. No sampling, boundary set or solver
result is accepted in its place. Command and context schemas must be
identical apart from the schema commitment in their envelopes. Under a
successful simulation every state the old version's laws allow maps to one
the new version's laws allow, and from a mapped state the new version makes the same
decisions and sends the same deliveries. The guarantee covers those
observations only: identities, certificates, the delivery IDs of later
commits and Step usage may differ.

The simulation runs before any write transaction, like a program
comparison, and the lineage keeps its outcome per step, refusals included,
in memory; `Lineage::simulations` counts the runs. The upgrade then
migrates the store's current state (`v2::migration::migrate_state`, which
decodes the old envelope with the library's record projection and frames
the new one with its encoders) and moves the head to it. A refusal
(`upgrade::Refusal::Migration`) names the observation that differs, the
field without a value in its domain, or a migrated state with no encoding
under the new framing, with the position of the input in the
enumeration (`v2::migration::inputs_at` turns it back into field values),
and writes nothing.

A rename (`Step::Rename`) needs no simulation: the new catalog's policy,
with the old schema and its commitments in place of its own, must be byte
for byte the old policy (`v2::migration::rename_exact`, the classifier's
own comparison). The state's payload is kept and only its envelope is framed
again under the new schema (`v2::migration::reframe`); the upgrade is
admitted at any state. A step declared as a rename that is not one refuses
as `upgrade::Refusal::Rename`.

The records are

```text
"ZFCISV2-MIGRATION\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity)
  || frame(0x01 || migration_digest || states:u64be || admitted:u64be
           || genesis:u64be || tuples:u64be || observations:u8
           || migrated_state_root)
  || frame(state_root) || frame(previous_chain)

"ZFCISV2-RENAME\0" || ordinal:u64be || head_sequence:u64be
  || frame(from_identity) || frame(identity)
  || frame(0x01 || reframed_state_root)
  || frame(state_root) || frame(previous_chain)
```

where `migration_digest` is the SHA-256 of the migration's canonical
encoding (`Migration::encode`: the magic `ZFCIS-MIGRATION\0`, format 1, the
field count and each field's ID and source), the counts are the old states
of the declared domain, those its state laws hold on, the genesis states
among those and the input tuples compared, and `observations` is the bit set
of the observations compared (genesis 1, decision class and reason 2,
deliveries 4, successor state 8, the new version's state laws on every
migrated state 32; all five are set, 47). `state_root` is the root
before the hop. Both magics first differ from the others at byte 8. The row's
admission column stores two framed parts, each a `u64be` length and its
bytes: the migration's encoding (empty for a rename) and the state the hop
moved the head to. The head row's state, root and chain move to the new
state and tip. The next commit's pre-state is that state, and a pending
delivery's commit is checked against the state its own commit started from,
so pending deliveries keep their certificate-bound IDs and order across the
hop.

An audit re-derives each record. For a migration it takes the auditing
lineage's simulation of the step, running it before the read or write
transaction when it has not yet, migrates the recorded state again,
recomputes the record, and requires the stored encoding and state to be
exactly the lineage's. The simulation is re-run on every audit of a fresh
lineage value rather than only in an explicit audit command, so that no
open trusts a recorded count, and it is kept per lineage value exactly as a
program comparison is. Its cost is linear in the domain, up to the cap; the
timings for the spend-approval domain are below, and a domain near the cap
would cost about 72 times as much on every open (an extrapolation, not a
measurement). A rename checks exactness from the two catalogs and frames
the recorded state again. A lineage that declares another migration, a
rename or another kind of step for the versions a migration record spans, or
another kind of step for a rename, ends the open with
`Error::Succession(upgrade::Unsupported::Migration)`: the store may be
intact. A stored state or encoding that does not match its own record is
`Error::History`. A checkpoint taken after a hop carries the moved state, and
one taken before it at the same head opens as `Error::Checkpoint` once a
later save replaced the marker.

For the spend-approval migration that adds a Boolean state field, the
simulation compares 14,592 input tuples: 57 of the 80 declared states satisfy
the state laws, each with 256 commands and contexts. A whole `--audit` of a
migrated store, which runs it, took about 3 seconds in a debug build of the
generated application, whose manifest optimises `zeno-fcis-synthesis`, and
the simulation took about 15 seconds in a debug build of the CLI, which does
not, measured on a shared machine.

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
