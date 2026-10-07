# Actual byte, decision and law composition

This bounded library unit composes the existing canonical readers, finite
interpreter, typed decision constructor, account abstraction and law engine.
It does not qualify combined V2 or grant external authority. The public runtime
registration is root-owned. `composition::bind` borrows an immutable complete
descriptor; the returned core, candidate and execution reports have private
construction and retain only library-produced results.

## Mathematical object and admission

The subject is a total relation from descriptor, exact original byte slices,
eight-counter initial usage and retained trace prefixes to either the complete
candidate or a technical refusal, plus exact usage and trace suffixes. Every
reachable executable function has zero `requires` and exact postconditions.
Semantic models preserve raw types, all candidate components and failure order.
Private law frames normalize their actual referenced data; their contracts link
the actual `laws::evaluate_into` result through that normalization. This is not
an additional extensional uniqueness theorem over arbitrary equal referenced
frames. Verus, Z3, vstd, Rust, allocator and platform behavior remain in the TCB.

Admission checks the full original State/Command/Context schemas and every
branch, even unselected branches. State must explicitly be a flat record.
Command and context may be original scalar roots. A root selector remains
distinct from every u16 field ID; law footprints use 65536 for roots. No input
re-encoder, project callback, supplied scalar output, usage report or invented
transition frame participates in production execution.

Finite ABI bindings cover every input exactly once. Reification retains actual
Bool/I128/Enum/Sum values and original type/variant IDs. Complete variant maps
have distinct IDs and distinct codes within the declared inclusive interval,
and cardinality equals that interval width. Arbitrary map order is supported.
An original Enum/Sum map with precisely the 0/1 code domain may feed a Bool ABI
input, preserving the original typed atom for laws. A plain I128 leaf cannot
use this bridge. This admits the unchanged durable-counter graph and original
Sum command and Bool context. The catalog must bind the complete mapping,
including type101, variant120 -> 0, variant121 -> 1, to the authored policy.
This unit does not certify that external lowering step.

Channel admission validates numeric bounds, declared variants, ordered payload
fields, destination and idempotency types, and every constant in every branch.
Text is ASCII. Every law Text literal is scanned, including unused nodes and
inapplicable laws. Text/Bytes length bounds from the original canonical catalog
are a further root-owned descriptor/policy check because this lower-level
Domain::Text/Bytes does not carry a length bound.

## Actual execution and refusal order

Descriptor admission precedes protected input work. Framed execution charges
min(received length,48) Byte before checking each complete original envelope,
in State/Command/Context order, before any payload decoding. Payloads are the
actual borrowed suffixes and final reports retain whole original envelopes.
Raw record projection charges its byte length before headers, then one Read
before each field. Scalar roots charge their full byte length and one Read
before interpretation. Candidate construction follows eager graph evaluation;
all earlier graph nodes run even if unused by a root. Output reification and
complete construction precede delivery-domain validation and actual laws.

The one private meter crosses every stage. The existing constructor charges
Candidate once, Write for each assignment and Effect for each attempted effect
or outbox entry, retaining permitted/denied attempts. Laws see decision-stage
usage, then charge the same meter for actual predicate nodes and observations.
Final usage includes those law costs. Private helpers retain arbitrary initial
counters and prefixes; refusals expose no partial candidate. Ordinary Reject
exposes no successor, patch or delivery. CommittedFailure remains an ordinary
checked complete candidate with only its declared modifications. WitnessByte
and Depth are unchanged by these composition stages. Logical charges exclude
metadata scanning, allocation, hashing, physical instruction and memory costs.

Genuine Genesis admits only the actual initial state and evaluates initial
conditions/invariants. It does not manufacture command or context values.
All applicable law success comes from the actual evaluator; the imported law
unit supplies its successful-applicable-prefix semantic lemma.

The account bridge preserves complete original raw i128 bounds, previous
last_seen, command root and context. It feeds checked Boolean facts into the
actual fixed 21-node graph and reifies its branch plus checked arithmetic
outputs. Ingress costs 97 Byte, 6 Read and 5 Step; successful graph evaluation
adds 21 Step before candidate construction. Lawful and non-lawful pre-state
tuples remain in the admitted raw domain.

## Evidence and controls

Run from the repository with the existing pinned offline tool cache:

```
flock /tmp/zenofcis-v2-parallel/heavy-check.lock python3 tools/check_composition_v2.py --out /tmp/zenofcis-v2-parallel/composition/NEW-EVIDENCE/receipt.json
python3 -m unittest discover -s tools -p test_check_composition_v2.py
```

The evidence directory must be new. The gate validates tool file hashes,
whole-crate Verus success, the complete translated function/contract/Spec-body
inventory and every Exec body fingerprint. It never regenerates its own frozen
profile. Source hashes include the dependency tree, native crate dependencies,
registrations, original template artifacts, native tests and gate/profile/docs.
It refuses source drift and rechecks tools at completion. Detailed command
receipts preserve stdout/stderr and exit status. Cargo uses +1.97.1 --locked
--offline, the shared target, one build job and one test thread.

Independent original-wire integration tests cover inventory864, order1728
(including 288 invariant-invalid pre-state tuples), account14580 boundary
tuples and all20 retained examples, and durable-counter64 tuples and all12
retained examples. Retained examples include all notification/alert columns.
They compare complete decisions, not production evaluator output used as an
oracle. Descriptor fixtures and hand-authored law programs are not proof of
legacy frontend/law lowering. The order fixture checks the original invariant
plus mandatory structural laws; the independent raw policy oracle separately
checks its complete decisions. The durable fixture uses explicit U128(0)
idempotency; correspondence to a legacy idempotency policy remains separate.

Private native controls exercise nonzero prefixes, counter overflow, eager
unused traps/unavailable observations, every law literal, arbitrary variant map
order and binary-map refusal, actual graph facts, full previous_seen/deadline,
original framed byte custody, and actual Genesis. External compile controls
check successful public reads and refuse private construction, candidate/report
mutation, producer access and mutation through a borrowed descriptor.

Eighteen semantic production mutants must both fail an SMT obligation and fail
a meaningful native test; parser/compiler errors are not accepted as proof
kills. Five controls must still verify but be refused by the independent
coverage gate: weakened contract, narrowed domain, uncontracted helper, changed
spec body, and interpretation moved before the header charge. Their retained
sources and logs identify exactly what each control changed. These bounded
controls are regression evidence, not exhaustive fault discovery or an
independent human review.

## Simplification and intentional changes

Direct vector initialization replaces empty-vector-plus-push bookkeeping in
four places; a redundant borrow and two-arm error-forwarding match were also
removed. Ordered values, counters, refusals, canonical identity, independent
admission checks and all contracts remain unchanged. A ghost sequence equality
assertion retains solver evidence for the singleton vector. No new cache,
mutable state or abstraction was introduced and no acceptance check removed.
Whole-source proofs and native full-output comparisons supply the before/after
behavior evidence. Body/source identities intentionally change and must be
refreshed by root in integrated identity registrations.

The Bool closed-map bridge and universal law-text scan are intentional admission
changes, not behavior-preserving simplifications. Root approved the bridge;
nonbinary/plain-integer aliases remain refused. This unit's frozen profile is
separate from predecessor decision/law/abstraction profiles, which root must
refresh against the combined integrated source when their imports grow.

## Remaining integration obligations

Root owns original catalog extraction, complete ID/policy/root/channel binding,
original law lowering, mandatory private framing, canonical identities,
authority/sealing/recompute and persisted replay, ledger effects and migration.
Caller-provided framing on this low-level inspection API has no authority.
Global ATDD, Miri, fresh template certificates, exact-head replay, independent
review, CI and any commits or deployment are root responsibilities. The
composition receipt cannot stand in for those gates or combined V2 readiness.
