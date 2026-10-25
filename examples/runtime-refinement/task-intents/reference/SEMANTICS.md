# Two-slot durable intent reference

This is an authored finite protocol specification, not the task application,
SQLite interpreter, authentication adapter or production receiver. The candidate
representation and its abstraction map are separate, untrusted proposals.

## State and events

Fields, in enumeration order, are `phase1`, `phase2`, `packet`, `capability`.
Each slot's phase is 0 Absent, 1 ApprovedPending, 2 ReceiverAppliedPending,
or 3 Acked. Packet is 0 None, 1 Slot1 or 2 Slot2. Capability is 0 or 1.
Genesis is `(0,0,0,0)`. The raw state ordinal is
`(((phase1 * 4 + phase2) * 3 + packet) * 2 + capability)`.
There are 96 raw states and 12 input tuples; context is the singleton zero.

A missing packet requires capability zero. A packet names a slot in phase 1
or 2; capability one additionally requires that slot to be in phase 2.
Law 500 applies to every decision and technically refuses invalid pre-states.
Law 501 requires valid committed successors and genesis. All 96 raw states
remain in the comparison domain, including the 56 invalid representations.
The other 40 states are reachable from genesis.

| Action | Event | Admitted transition |
|---|---|---|
| 0, 1 | Commit1, Commit2 | The named absent slot becomes phase 1; all other fields stay unchanged. |
| 2, 3 | Dispatch1, Dispatch2 | With no packet and the named slot in phase 1 or 2, set its packet and clear capability. |
| 4 | Apply | With a packet, phase 1 becomes 2 and emits one application event; phase 2 accepts without a new event. Set capability to 1. |
| 5 | Ack | Capability 1 and its matching phase-2 packet permit phase 3; clear packet and capability. |
| 6, 7 | Crash, TransportFailure | Clear packet and capability only; both durable phases remain unchanged. |
| 8, 9 | StaleAck1, StaleAck2 | Always reject with reason 201: these are explicitly invalid capability events, even if the slot happens to match. |
| 10 | Conflict | Reject with reason 202: a same-ID conflicting payload observation. |
| 11 | CapacityProbe | Reject with reason 203: a third identity is outside this two-slot profile. |

Any other unavailable operation rejects with reason 200 and changes nothing.
Invalid raw pre-states instead receive the technical law refusal; ordinary
rejection is not a substitute. Commit of the other slot is allowed while one
packet is in flight. An old pending slot can complete after the newer slot.
An Acked slot never becomes pending again. Crash represents an atomic boundary
followed by reopening; it does not model an unavailable host interval or prove
storage survives power loss.

The reference's channel 300, destination `local-test`, payload field 140 (slot),
ordinal 0 and idempotency ordinal 0 describe a **receiver-applied observation**.
This is not the application's channel-300 approved-intent delivery and is not
additional executable work. Its occurrence count is used by the safety observer.

## Assumptions connecting the model to an application

Commit means that an independently authorized publication has already made an
intent durable atomically with its application state. This model cannot create
that authority. Slot numbers abstract two fixed, distinct delivery identities
and their fixed payloads; they are not cryptographic identities or receipt
signatures. The binding from actual identity, payload, revision and attempt to
these slots needs separate evidence.

Apply atomically retains the receiver's application fact and deduplication
fact. A duplicate yields no second application event. This is a receiver
contract assumption until separately related to the actual implementation.
Capability denotes a matching successful transport observation, not an input
Boolean that may grant production permissions. StaleAck is an adversarial model
event, not a newly installed production API validation gate. Conflict does not
prove actual collision detection. TransportFailure is uncertain: it never
rolls back receiver application or permits treating a task as failed.

There is no task business status in this machine. An application result,
including Failed or Unknown, requires its own authorized transition. Two
attempts may have separate outstanding identities; this reference does not
justify retrying arbitrary external work or undoing an old pending delivery.

## Checks and exact claim

Use the existing native CLI, with fresh output paths:

```sh
zeno-fcis contract review REFERENCE --max-tuples 1152 --out review.json
zeno-fcis contract scenario REFERENCE --out scenarios.json
zeno-fcis contract runs REFERENCE --max-tuples 40000 --out runs.json
```

`v2/run-properties.json` checks per-slot at-most-once application, prior durable
approval, nondecreasing durable phases and preservation of Acked slots. Its
largest declared observer product has 31,104 tuples. The native state lemma
for the cross-field validity formula in law 501 checks initiation and closure.
An `inductive` result establishes these properties for arbitrary-length runs
of this fixed finite machine, not unbounded identities or an arbitrary runtime.
The ordinary authoring `--require-substantive` heuristic labels law 500 as
ignoring a transition: that pre-state-only rejection guard is intentional and
must not be removed to obtain a passing authoring heuristic.

The separate concrete comparison must cover all 1,152 state/input pairs,
including technical refusals, and check exact successors and observations.
Matching genesis plus complete one-step refinement permits trace induction
inside this machine. Missing rows, changed sources, meter exhaustion, timeout,
unknown or inconclusive results do not establish refinement. Mapping-defined
states are not necessarily law-valid states: an all-96-state mapping includes
all 56 invalid representations.

Native model checking, finite implementation differentials, actual SQLite and
receiver process-loss tests, and parameterized Verus theorems are separate
kinds of evidence. This reference supplies no publication authority, OS sandbox,
power-loss guarantee, external exactly-once claim, fairness or liveness proof.
