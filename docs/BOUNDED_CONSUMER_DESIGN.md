# Bounded consumers: delivery, composition and collection evidence

Status: design proposals, not new SDK APIs or completed assurance gates.
This note records requirements exposed by the bounded policy consumer in
[ZenoStorage](https://github.com/TheDarkLightX/ZenoStorage/pull/13) and the
[ZenoMail migration](https://github.com/TheDarkLightX/ZenoMail/pull/6).
The application pins `78e4b59fb64b525bd318d0713818788506027f5b`.

## Delivery progress is separate from receipt safety

`SqliteShell::next_pending` selects unacknowledged records in candidate/ordinal
order. `deliver_next` acknowledges after its interpreter succeeds. Candidate
hash order is not causal or commit order, and these APIs do not isolate blocked
destinations. Their existing semantics must remain explicit.

The consumer retains four exact receipts, allowing at most three whose
predecessor is beyond its current tip. This reserves capacity for the direct
successor, but does not make a sender offer it. A strict retry-head scheduler
offering five chained activations as 5,4,3,2,1 stores 5,4,3, then repeatedly
rejects 2 while never offering 1. A selected schedule is a progress counterexample,
not proof that particular candidate hashes have that order. The application
retains executable consumer traces and a 120-permutation scheduler model.

An additive delivery capability should select an explicitly identified stream
in committed authorization-version/outbox-ordinal order and preserve exact
delivery IDs and entry hashes. Existing selection need not change silently.
A stream must bind channel and canonical destination, plus the relevant source
lineage. The database schema, joins, integer bounds, integrity validation and
duplicate semantics require implementation review before choosing an API.
Fair scheduling among streams is a separate shell obligation.

Acceptance must cover lost acknowledgements, restart, competing deliverers,
backpressure, an unsupported target blocking one stream, and another stream
making progress. No-capacity cannot authorize dropping an entry. Rejected
formats, scope mismatches or conflicts cannot be acknowledged merely to unblock
delivery: any terminal disposition needs a bounded, authenticated, durable
record and reconciliation rule. Unauthenticated input must not halt a valid
stream. Progress additionally assumes reachable destinations, supported targets,
eventually valid readiness, fair retries and available proof material.

## Adoption and fresh admission need a common ordering rule

A policy hash can recur. A request prepared during A1 may become admissible
again after A1→B→A2 if admission compares only policy hashes. Existing committed
requests must still recover their original receipts under their original terms.

The preferred next application design is a composite authority state containing
the adopted activation reference and new placement-admission state. New requests
bind consumer genesis, activation sequence, policy/profile and authenticated
epoch. Exact historical recovery precedes this gate. Adoption and admission must
be ordered by the same publication authority, with `CompleteFootprint`,
`SequentialParity`, independent complete-decision laws and enforced resource
bounds for the actual composition. Merely sharing a database proves none of these.

A separate generation lease is insufficient when its fence reaches placement
storage after the policy component declares adoption effective. Such a design
must define adoption's linearization point at enforcement of the fence and prove
its protocol, including delayed delivery and crashes. Do not retrofit generation
fields by reinterpreting an existing canonical request ABI.

Foreign receipt compatibility is a versioned relation over exact profiles,
lineages, terms and rights. Equality with the latest activation may constrain
new paired admission under a particular profile; it is not a universal condition
for historical lookup, retrieval, repair or verification of an old obligation.

## Bounded collection lowering needs a checked relation

The application's authored-schema lowerer cannot currently represent the desired
vector/map leaf bindings. The consumer uses four typed optional receipt slots
and projects them into a 17-input, four-output finite gate. That gate is emitted
by ZenoFCIS, but its scalar proofs do not prove the projection or reconstruction.

A candidate collection facility should declare capacity, element schema, order
key, uniqueness and empty-slot encoding, then supply:

1. Canonical encoding and decoding, with both round trips on their stated domains.
2. A correspondence between valid typed states and admitted scalar encodings.
3. Whole-decision simulation, including rejection precedence, complete post-state,
   effects, outbox, bindings and resource behavior.
4. Evidence bound to the source, catalog, lowering algorithm, domain and verifier.

Reuse the [validated refinement and coverage machinery](VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md).
Matching representative cases is differential evidence. It is not exhaustive
coverage of all byte strings or bounded integers. A reduced equality/order domain
requires a proved abstraction preserving constants, admissible encodings, finite
endpoints and gaps, equality/order types and outputs. Syntactic operator checks
alone do not establish that theorem. Missing correspondence evidence remains a
promotion blocker, even when all scalar solver obligations pass.

## Explicit successor continuity

Changing an installed code/support set changes the source or deployment identity.
A successor protocol must qualify sealing, unique authorized succession,
historical request/nullifier continuity, retained obligations and modules,
pending inbox receipts, undelivered effects, and independently recoverable proof
material. Draining every outbox can fail to terminate; carrying pending work
requires a proved ownership-transfer and replay rule. Parameterized interpreters
are another possible profile, with separate domain and semantics proofs.

These proposals reduce future application-specific machinery by making the
required boundaries explicit. They do not add a production finality verifier,
prove distributed liveness, provide arbitrary migration, or establish unattended
operation of the complete mail/storage network.
