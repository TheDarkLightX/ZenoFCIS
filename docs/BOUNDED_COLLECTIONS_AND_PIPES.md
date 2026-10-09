# Bounded collections and profile-bound pipes (ZBC1)

This additive API is in `zeno_fcis_collections::bounded` and `::pipe`.
It does not change the existing `PersistentMap` or ZCVE/1 wire format.
No new backend, external dependency, authority capability or shell is introduced.

## Complete transition contract

`BoundedMap<M>` wraps the existing sealed `PersistentMap`. New keys insert;
existing keys replace even at capacity. `BoundedSet<M>` uses unit-valued entries;
a duplicate insert is a no-op. Their iteration order is canonical encoded-key
order. A missing removal is a no-op. `BoundedFifo` appends values in arrival order
and pops only the head. Empty head/pop returns `Empty`. Zero capacity admits an
empty collection but no new item. Every method borrows the immutable pre-state
and returns a new owned state; every previous version remains usable.

A `ProfileBoundPipe` retains immutable `PipeMessage` values. The caller supplies
32-byte domain, schema and message identifiers. The domain/schema pair binds
identity by equality, **not authentication or payload schema validation**.
Payload admissibility means precisely default `ValueLimits` plus the explicit
collection limits below. Application-specific schema checks belong upstream;
this component does not execute a supplied validator or trust a boolean verdict.

Enqueue checks profile identity, then pending message ID. Exact duplicate pending
messages return the same logical state, without moving the message; the same ID
with a different payload returns `IdConflict`. A fresh ID appends only after
resource checks. A retry is another read of the unchanged head. Acknowledge
checks profile identity, nonempty queue and complete head-message equality, in
that order. Only that head is removed. Acknowledgment does not establish that
anything was delivered. After acknowledgment, the removed ID can be reused;
there is no replay protection outside the pending window.

## Resource policy and refusal order

`CollectionLimits` contains maximum entry count (`u32`), maximum framed item
bytes (`u64`), and maximum complete snapshot bytes (`u64`). An empty ordinary
snapshot takes 29 bytes; an empty pipe takes 93. Smaller policies are refused
at construction. No other consistency relation is required: a policy can admit
an empty state but no items. Arithmetic is checked. The exact byte length is
privately retained and updated by subtracting the old framed item length and
adding the new one; callers cannot inject measurements or mutable aliases.

For ordinary insertion, validation of the incoming `Value` against default
structural limits precedes item-byte, entry-count and snapshot-byte checks.
Map keys were already canonically validated by `LogicalEntry::try_new`.
Set keys are structurally validated before canonical-key construction.
Pipe enqueue performs the identity/pending-ID rules first, then the same
resource checks. Checked arithmetic/representation failures are technical
refusals before the comparison that needs their result. Original states are
unchanged on every refusal. `Value` limits and encoding failures are returned
as typed errors, never converted to permissive decisions.

Byte limits bound canonical retained data, not actual allocator usage, CPU,
wall-clock time, or work-meter authority. Values are supplied already allocated.
Preflight validates/visits input values and may compare existing values; it is
not free. For admitted values, default depth/node/collection limits bound each
visit. Map backend clone/update costs remain as documented in
[PERSISTENT_COLLECTIONS.md](PERSISTENT_COLLECTIONS.md). Queue updates copy
retained items and pipe duplicate checks scan at most the retained item count.
Snapshot serialization temporarily allocates canonical bytes and is fallible.
These APIs do not supply FCIS resource-meter or publication authority.

## Canonical snapshot specification

All integers below are unsigned big-endian fixed-width. These are diagnostic,
versioned collection snapshots, not a new `Value` encoding or authorization token.
There is no snapshot decoder or persisted-state admission route in this change.

- Four ASCII magic/version bytes `ZBC1`
- Kind byte: map 1, set 2, FIFO 3, pipe 4
- Policy: max entries (4 bytes), max item bytes (8), max snapshot bytes (8)
- Retained item count (4 bytes)
- Pipe only: domain (32 bytes), schema (32 bytes)
- Body in the order specified below

A blob is an 8-byte length followed by exact default canonical ZCVE/1 bytes.
Map and set bodies contain key blob then value blob in encoded-key-sorted order;
set values are always unit. Their framed item size is 16 plus key and value
lengths. FIFO bodies contain one value blob per item in insertion order, with
framed size 8 plus value length. Pipe bodies contain raw 32-byte message ID then
payload blob, with framed size 40 plus payload length. Profile fields need not
repeat per item because admission binds every item's full profile to the header.
Kind and policy differences always change the snapshot. Equal map logical
entries and policy yield equal snapshots across histories and enabled backends;
FIFO permutation generally changes its snapshot.

## Evidence and limits

`tests/bounded_contract.rs` contains an independently encoded integer reference
model, all-backend operation-trace comparison, retained-snapshot checks, map
insertion permutations, exact and one-byte-under boundaries, zero capacity,
precedence, set duplicate semantics, FIFO order, profile mismatch, conflicting
IDs, full retries, exact acknowledgments and permitted post-ack replay.
`tools/check_bounded_mutations.py` runs concrete faulty-source mutations against
that acceptance suite. These finite tests detect the listed faults; they are
not universal proofs, cryptographic guarantees, transport delivery tests or
performance measurements. Independent review remains required.

There is no clock, randomness, network, scheduler, retry timer, liveness,
consensus, durability, exactly-once delivery, atomic external effect, ownership
transfer, physical backpressure, or ZOS integration claim. A successful enqueue
or acknowledgment is ordinary pure state computation, never a publication.

A future decoder must independently reject unknown versions/kinds, truncated or
trailing bytes, noncanonical/duplicate keys or IDs, profile mismatch and
policy/length violations before retained allocation. It must reconstruct the
private invariants rather than trust stored measurements. Those obligations
are not implemented or qualified by this encoding-only tranche.

Set insertion checks structural and framed item limits before allocating key
encoding. Capacity and remaining snapshot capacity follow canonical-key lookup
so a full set can still accept duplicates. These later refusals can therefore
allocate canonical key bytes; refusal is immutable, not allocation-free.
