# V2 original-wire publication capability

This bounded stage connects the actual bound V2 evaluation to the original wire
artifacts through a private library capability. Its evidence is recorded by
`tools/check_publication_v2.py` against the complete production closure in
`verification/verus/publication-v2.json`. It does not qualify the imperative
shell, six-template registration, release, or combined V2 gates.

## Exact object and boundary

Let A be an immutable checked Authority, x the complete original state, command
and context envelopes, and E = A.evaluate(x). If E succeeds with actual candidate
C of class Accept or CommittedFailure, successful publication contains exactly:

- E, including its original inputs, complete sealed evaluation subject, class,
  reason, patch, reads, attempts, usage, diagnostics and law reads;
- A's exact policy and compiled-source identity;
- the original ZFCISV1 envelope of encode_record(C.post), using A's checked state
  root, declared schema32 and complete frame-size cap;
- every C.effects and C.outbox entry, in order, with its original ordinal,
  channel, catalog-bound destination and payload roots, and original ZCVE
  encode_atom(destination), encode_record(payload), encode_atom(idempotency).

The output encoder is the actual `canonical_v2::output` implementation. No caller
candidate, Value conversion callback, supplied identity, law verdict or usage
counter enters this construction. The checked catalog retains the original
channel-root map in Authority. Catalog binding checks its correspondence with
channel domains; the actual composition path checks candidate domains before
applicable laws and sealing. Publication then uses that immutable map. The
original policy binds every root and framing field.

Delivery byte getters expose bare original ZCVE values. They are not envelopes,
and no channel schema32 is invented from a type root. The poststate and Genesis
initial bytes are original ZFCISV1 envelopes. A declared schema32 is an exact
policy field here; this stage does not prove or perform a cryptographic schema
hash recomputation.

## Capability and replay

`Authority::publish(raw)` returns `PublicationOutcome::Commit(Publication)`,
`Reject(Evaluation)`, or `Refused { evaluation, error }`. Publication has private
fields, is non-Clone, and exposes immutable `identity`, `evaluation`, `poststate`,
`effects`, `outbox` and `subject` getters. WireDelivery exposes only exact ordinal,
channel, root and byte getters. Business Reject yields an audited noncommitting
result. No partial wire capability escapes a technical refusal.

The complete publication subject is the exact authority canonical token encoding
of domain word 0x5a505532, version 1, phase 1, the complete audited evaluation
subject, original poststate envelope, and both ordered delivery lanes. Each lane
has its count; every entry has ordinal, channel, destination root, payload root,
destination bytes, payload bytes and idempotency bytes. Length-framed byte tokens
and fixed-width word tokens make every component explicit. The evaluation
subject already includes the exact original inputs and authority identity.

`replay_publication(raw, expected)` reruns actual evaluation and output
construction before comparing the entire resulting publication subject. For
business Reject it compares the complete audited evaluation subject; it never
produces a committing capability. Technical failure takes precedence over replay
comparison. Changes to original inputs, policy/source identity, any wire output,
root metadata, order, framing or phase cannot pass equality with the original
subject unless the recomputed complete subject is equal.

`publish_genesis(initial)` invokes actual initial-state admission and Genesis
laws. A successful private GenesisPublication retains the real audit, exact
initial envelope, exact identity and phase-0 publication subject. Its subject
contains the complete Genesis evaluation subject and original initial frame.
`replay_genesis_publication` recomputes it before exact comparison. A failed
initial law produces no Genesis capability. Neither a transition subject nor a
caller-provided initial verdict can serve as Genesis.

All methods and helpers have exact total contracts: no executable requires,
success-only domain restrictions, application-owned omitted bodies, assume,
admit or external-body exemptions. Whole-harness coverage fixes complete raw VIR
inventory, signatures, contracts, every Exec body and every Spec body. A verifier
success with a different reviewed contract or body is rejected by coverage.

## Failure and resource semantics

The observable order is actual framed evaluation and sealing, Reject handling,
state cap representability, post-record encoding, full post-envelope encoding,
effect entries in order, outbox entries in order, publication-subject encoding,
then replay comparison. Within each delivery, lookup precedes destination,
payload and idempotency encoding. Each output error retains its exact encoder
failure. The audit is available for every technical refusal, including a refusal
after evaluation completed; retained audit is not a committing capability.

State caps remain u64. The implementation checks representability before its
usize conversion; all u64 caps remain representable on the named Linux x86_64
target. Original u32 wire lengths and checked output arithmetic may refuse before
any artifact is exposed. Publication allocations and serialization are outside
the invocation logical meter, as is authority sealing. Existing usage reports
are preserved, not relabeled as physical cost or publication work bounds.

## Evidence and obligations

The dedicated native corpus uses actual original Value and Envelope encoders as
independent output oracles. It covers Accept, Reject, CommittedFailure, complete
replay and policy changes, genuine Genesis, failed law/frame/resource admission,
complete reports, ordered effects/outbox, two channel-root pairs, field IDs 0 and
65535, full signed/unsigned delivery values, Enum/Sum IDs 0 and 65535, bytes/text,
and exact and u64::MAX frame caps. Original codec correctness remains an oracle
assumption, separate from the new total output contracts.

External compilation controls first compile a usable downstream client, then
check that clients cannot fabricate or clone private transition/Genesis
capabilities, mutate authority/result/output custody, supply a candidate or
conversion callback, call the private finisher or read the private source getter.
Semantic controls alter actual publication behavior. Separate coverage specimens
must verify and then fail for the intended raw getter contract, specification
body or added executable inventory difference. Compiler, VIR, timeout, resource
and unrelated proof errors do not count as semantic mutation kills.

The source getter contains the root-approved 83 complete paths and three explicit
pins, including itself at its exact fixed-point length. Its source-list hash is
7c32535fe7188cbfb87ac1e8cc53c40507424f64c4d8cc01d186d7e2948bd1f1. Public
`policy_bytes` only serializes declared policy for reviewed artifact generation;
it does not admit metadata, issue an Authority or guarantee intended meaning.

The shell must borrow a genuine capability, compare the actual persisted
original prestate atomically, and retain the exact identity, subject, original
inputs, poststate and ordered delivery bytes together. Consuming a persisted
subject requires recomputation under the same Authority. Replay binds provided
prestate; it cannot independently observe freshness in an external database.
Atomic commit, durable outbox insertion, delivery/recovery and host authentication
remain shell obligations. Verus/vstd/Z3, Rust/compiler correspondence, allocator,
operating system and hardware remain named trusted boundaries. Prior constructor
and output qualifications are dependencies, not this combined qualification.
