# Complete decision construction stage

Parent 1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c. This stage owns a
library-private, immutable complete candidate constructor. It cannot authorize,
hash, seal, or deliver a transition. Root owns byte admission, exact producer
composition, catalog identity and authority integration.

## Contract and proof plan

- Values retain full I128/U128, Boolean, Enum/Sum IDs and borrowed Bytes/Text.
  Borrowing pins the immutable input lifetime; outputs own their vector structure.
  No project callback supplies a value, verdict, footprint or usage report.
- A branch chooses Accept with no reason, Reject with a nonzero reason, or
  CommittedFailure with a nonzero reason. Reject has no successor, patch,
  effects or outbox. Complete committed successors cover all pre-state fields,
  in strictly increasing field order, including generic zero IDs and empty state.
- Assignments resolve checked output indices, exact source/field references or
  contract constants, then check the declared complete value domain. Canonical
  patches retain only unequal before/after pairs, in field order.
- Deliveries have ordered ordinals, nonzero channels, complete destination,
  ordered payload fields and an explicit idempotency value. Effects and outbox
  are separate ordered vectors; conditions are exact Boolean values.
- One private meter is passed through. Candidate is charged once before branch
  selection; each assignment charges Write before value resolution, even if
  unchanged. Enabled deliveries charge Effect before payload construction.
  Refusals retain prior charges and denied attempts but return no candidate.
  WitnessByte/Depth remain unchanged; metadata/allocation/hash/physical cost
  are outside this logical profile.
- Proof proceeds by exact total leaf resolution and domain checks, prefix
  induction for record assignments/deliveries, then composition. Every reachable
  executable body receives an exact contract with zero executable requires.
  A separate translated body manifest fixes reviewed operational charge order.

Native oracles compare every component independently; mutations challenge
branch/reason, post-state, patch, delivery order, attempts and usage. The source
must pass pinned whole-source Verus and native checks before this unit is called
checked. The full V2 release, independent review and original-byte/schema/output
composition are separate obligations.

## Encoding and admission boundaries

This is a typed candidate theorem. No byte-level canonical encoder theorem is
claimed. Bytes/Text are immutable admitted payloads; catalog/type admission and
UTF-8 validation occur in the producer and require an explicit bridge. The
proposed future encoding is versioned: class tags 0/1/2; reason presence plus
u32; big-endian u16 field IDs, u32 type/channel/ordinal IDs, u64 vector/byte
lengths, I128/U128 16-byte words; ordered record/patch/delivery vectors. Original
ZCVE payload encodings and authority receipt identities must be bound by root
before this candidate can be sealed. This document is not an authorization.

## Exact private port and precedence

`construct(Inputs, &[Atom], branch_code: i128, &[Branch], &mut Meter,
&mut Vec<Attempt>) -> Result<Candidate, Failure>` is visible only within
`execution_v2`. It takes the actual shared meter, never a supplied usage report.
The public Candidate has only borrowed getters. Its structure owns its post,
patch and delivery vectors; byte/text leaves borrow immutable producer storage.
Rust lifetimes prevent mutation or destruction of that storage while referenced.
Both Accept and CommittedFailure construct the full post/patch/effect/outbox tuple.
A successful Reject carries the immutable pre-state but an empty committing tuple.
Errors return no candidate; the private meter and attempts retain exact prefixes.

The constructor checks State/Command/Context field ordering first, then charges
Candidate before branch lookup. All branch codes must be strictly increasing;
missing/duplicate/unordered codes refuse. Only the selected branch's reason and
plan are interpreted. Closed all-branch catalog admission remains root-owned.
Reasons are None for Accept and nonzero Some(u32) for both failure classes.
Reject refuses any nonempty committing plan.

For committing branches, assignment count must equal pre-state field count.
Before each Write charge, its field ID must equal the corresponding state ID.
A permitted Write precedes expression resolution, pre-value domain checking,
and post-value domain checking, in that order. Reference errors precede domain
errors. The patch omits values equal by typed semantics (including byte content);
an unchanged Accept still consumes all declared Write attempts.

Effects precede outbox. In each sequence, channel must be nonzero and ordinals
strictly increase; ordinal gaps are legal and retained. The condition must
resolve to an exact Boolean. Disabled entries do not evaluate destination,
idempotency or payload and do not consume Effect. Enabled entries charge one
Effect, resolve destination, resolve idempotency, then construct the payload in
strict field order. Any later refusal discards every staged committing artifact.
A denied charge is retained as an attempt with permitted=false; no charge is
rolled back. No input field/value/domain is clamped or narrowed for the proof.

Attempt events here are Candidate, Write(field,permitted) and
Effect(is_outbox,ordinal,permitted). Ingress Read attempts remain the exact prior
record/domain report and must be joined by root. The decision theorem returns
(candidate, final eight counters, complete appended attempts) as one relation.
Laws inspect the frozen decision-stage usage. The final outcome reports usage
from the same meter after law evaluation; these two observations are distinct.

## Implementation evidence and integration obligations

The actual registered production source and inherited closure first passed
198 pinned Verus obligations, with zero errors and no executable requires.
Proof/coverage/native mutation qualification is reported in the separate
hash-bound decision evidence receipt, never inferred from this count.
Whole-source coverage includes all translated functions and executable bodies.
No independently checked SMT proof object is supplied; ADR 0003's separate
status requirements remain applicable.

Root must copy the new owned files unchanged, register `mod decision;`, bind
constructor Inputs/output/code to exact admitted execution products (including
Enum/Sum output conversion and full-width account facts), bind every Domain and
Branch to the catalog/contract, and construct the law frame directly from the
actual candidate and reports. Root must implement/prove canonical bytes and
identity, expose only the sealed authority outcome, and re-run whole combined
coverage/native/Miri/no-std/Clippy/template/ATDD/replay gates after integration.
Do not expose the private constructor or accept caller-created Atom outputs as
a substitute for that composition.

The supported candidate value language is flat field records with the eight
Atom cases. Recursive records, payload-bearing Sum, dynamic maps/sequences,
computed variable-length bytes/text and arbitrary nested effect schemas are
not implemented here. Existing opaque Bytes/Text payloads are preserved exactly;
their producer's canonicality, UTF-8 and schema validation remain explicit.
The complete V2 track therefore stays open until required template schemas and
producer/encoder/authority bridges are covered; this bounded constructor alone
does not close it.

The order native corpus includes all 1,728 schema-admitted tuples: six statuses,
four attempt counts, six actions, four callback counts and three callers. It
includes the 288 tuples omitted by the legacy reachable-state corpus because
of law500. No constructor precondition removes them. An independently written
full-decision oracle compares class/reason, typed Enum successor, complete patch,
ordered payment/shipping outbox, idempotency, observations and exact logical
usage against the emitted-program fixture plus actual typed constructor.
This is typed-constructor conformance; law500 enforcement on the actual full
candidate belongs to the root decision/law composition gate.
