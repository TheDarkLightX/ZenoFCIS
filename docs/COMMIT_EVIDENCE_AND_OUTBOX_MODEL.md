# Commit evidence and durable outbox model

## Purpose

ZenoFCIS V1 uses one closed generic execution model:

```text
CanonicalPatch
+ non-executable CommitPlan evidence
+ durable OutboxPlan obligations
        -> one candidate
        -> atomic authorized publication
        -> replay-safe outbox delivery
```

`CommitPlan` is retained because a transition may need to commit canonical,
candidate-bound evidence about logical decisions, authorities, subjects, or
operation payloads. A shell publishes those bytes atomically with state and
never executes them. Every operation that must occur outside the semantic
state, including every value movement, is an `OutboxEntry`.

## Inputs and outputs

The pure transition supplies a preconditioned patch, a bounded `CommitPlan`,
and a bounded `OutboxPlan`. `ProjectCatalog` validates every identifier,
schema, authority or subject requirement, operation classification, and
resource limit.

`EffectDefinition` is constructor-forced to
`CommitEffectSemantics::EvidenceOnly`. Catalog construction rejects an effect
whose `OperationSemantics` contains any `ValueFlow`. A value-bearing
`ChannelDefinition` remains valid and produces durable candidate-bound outbox
obligations.

The authorized shell atomically publishes state, authorization, receipt,
bundle, commit-evidence bytes, replay binding, and the exact complete outbox
row set. `deliver_next` is the only generic external-work path.

## Authority boundary

The production authority owns the catalog, transition program, project laws,
approved provider, deployment binding, replay policy, and exact
outbox-delivery interpreter profile. The concrete destination instance enters
SQLite only through `BoundDeliveryInterpreter` minted by the same authority.

The delivery interpreter receives only the exact committed `OutboxEntry`, its
candidate-derived delivery ID, and its exact entry hash. Acknowledgement is
accepted only for that hash. `CommitPlan` is never passed to this interpreter.

## Trusted dependencies

This model adds no dependency. It relies on the existing canonical codec,
catalog, candidate, authority, reference-shell, and SQLite history-validation
layers. Project-specific destinations remain outside the pure core and must be
qualified under their deployment threat models.

## Deterministic resource bounds

- `CatalogLimits::max_effects` bounds commit-evidence records.
- `CatalogLimits::max_outbox_entries` bounds durable obligations.
- Existing per-value and aggregate node and byte limits cover both plans.
- `MAX_VALUE_FLOWS` bounds each channel's economic classification.
- Strict plan decoders retain their input, item, depth, node, and payload-byte
  limits.
- Delivery retry count and wall-clock timing are operational policy, never
  semantic completeness evidence.

## Laws

1. A catalogued commit effect always has `EvidenceOnly` execution semantics.
2. A value-moving effect definition prevents `ProjectCatalog` construction.
3. Value-moving channel semantics remain content-addressed catalog meaning.
4. A successful commit atomically retains every outbox obligation or none.
5. Every pending delivery is an exact member of its decoded authorized bundle.
6. Delivery identity binds candidate identity and exact canonical entry bytes.
7. Replay with the same identity but different authorization or content fails.
8. Ordinary `Reject` has neither commit evidence nor outbox obligations.

## Negative cases

The implementation and tests reject value-moving commit evidence, unknown
effect or channel IDs, wrong payload shapes, wrong effect authority or subject,
duplicate ordinals, stale roots, replay collisions, missing or extra persisted
outbox rows, mutated entries, and acknowledgement hashes that differ from the
committed entry.

## Assumptions

- Owners classify every channel and its asset domain honestly.
- Project laws correctly relate semantic debits, credits, minting, burning,
  fees, and rounding to the complete outbox obligations.
- The selected destination implements its reviewed idempotency and delivery
  policy.
- Deployment storage protects the authorized history and is independently
  qualified for its intended environment.

## Explicit nonclaims

- Atomic publication is not atomic completion of an external network, chain,
  filesystem, device, or service operation.
- An acknowledged entry proves only the destination's reported exact entry
  hash under the mounted interpreter contract.
- Commit evidence is not an executable command, pending job, callback, or
  capability to perform work.
- A catalog classification commitment is a binding, not proof that the
  classification is truthful.
- The generic crate does not supply a universal payment, Solidity, Solana,
  storage, mail, device, or operating-system interpreter.
- Bounded tests do not constitute an unbounded proof or production deployment
  qualification.
