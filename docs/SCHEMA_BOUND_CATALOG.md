# Schema-bound project catalogs

`zeno-fcis-project` makes stable project identity and registry commitments explicit. `zeno-fcis-catalog` makes the reason, effect, and channel portions of those commitments executable.

## Authority chain

```text
closed Schema
+ ReasonDefinition[]
+ EffectDefinition[]
+ ChannelDefinition[]
        |
        v
CatalogManifest
    precedence hash
    effect-registry hash
    channel-registry hash
    exact RegistryEntry values
        |
        v
ProjectProfile
    schema binding
    precedence binding
    effect binding
    channel binding
        |
        v
ProjectCatalog
    validates complete cross-binding
    validates reasons by decision class
    validates CommitPlan
    validates OutboxPlan
    reports exact bounded metrics
```

The catalog does not replace project business rules. It prevents a shell or adapter from silently changing the set or shape of operations after a transition has produced a plan.

`ProjectCatalog::try_new` also closes the schema/profile type boundary. The profile's state, command, and context type identifiers must exist in the schema, and the profile's state type must equal the schema's declared root type. Effect payload types and channel destination and payload types must likewise exist before the catalog can be constructed.

## Reason definitions

Each reason binds:

- a nonzero stable semantic ID;
- a normalized stable name;
- `Reject` or `CommittedFailure` disposition;
- one position in an exact gap-free total precedence order;
- a nonzero commitment to the applicability predicate.

The manifest derives the profile's `precedence_hash` from definitions sorted by precedence. A reason catalogued as an ordinary rejection cannot be used for a committed failure, and no reason is valid for `Accept`.

## Effect definitions

Each effect binds:

- the numeric operation ID used by `Effect`;
- a stable name;
- an exact payload type in the closed schema;
- authority and subject commitment requirements;
- an explicit `NonValue` or canonical asset-scoped `ValueFlow` classification;
- a nonzero project-policy commitment.

Authority and subject requirements are closed values:

```text
Any
Absent
Present
Exact(nonzero hash)
```

A commit plan fails closed when an operation is unknown, its payload does not satisfy the schema, or its authority/subject commitments violate the definition.

Value classifications distinguish transfer, mint, burn, escrow, fee,
settlement, external-delivery, and custom registered relations. They are part
of catalog format 2 and therefore part of the effect registry and project
identity. See [Catalog-derived economic law requirements](ECONOMIC_LAW_DERIVATION.md).

## Channel definitions

Each channel binds:

- the numeric channel ID used by `OutboxEntry`;
- a stable name;
- an exact destination schema type;
- an exact payload schema type;
- an explicit `NonValue` or canonical asset-scoped `ValueFlow` classification;
- a nonzero delivery-policy commitment.

The delivery-policy commitment may identify project-specific retry, acknowledgement, ordering, privacy, retention, or destination-idempotency rules. The generic catalog binds that policy but does not claim that an external destination obeys it.

## Resource envelope

`CatalogLimits` bounds:

- effect count;
- outbox-entry count;
- recursive value depth;
- nodes per value;
- nodes across both plans;
- aggregate bytes/text/map-key payload bytes;
- children in any one collection.

Validation returns `CatalogMetrics`, which can be retained in receipts, evidence, performance reports, or deterministic budget accounting.

## Inputs, outputs, and laws

Catalog construction consumes one owned `ProjectProfile`, one owned closed `Schema`, one owned `CatalogManifest`, explicit `CatalogLimits`, and a selected `CommitmentHasher`. Its authoritative output is either an immutable `ProjectCatalog` or one typed `CatalogError`. Plan admission consumes immutable `CommitPlan` and `OutboxPlan` values and returns exact `CatalogMetrics` only after every ID, type, commitment requirement, and bound succeeds.

The implemented laws are:

- definition declaration order does not change the normalized manifest or its bytes;
- reason precedence is exactly the gap-free range `0..reason_count`;
- the profile schema, precedence, effect-registry, and channel-registry commitments equal the catalog commitments;
- the profile state type equals the schema root, while command, context, effect, destination, and payload types all exist in that schema;
- the profile contains every exact reason, effect, and channel entry and no hidden entries in those namespaces;
- each admitted plan ID is nonzero and registered, each value satisfies its declared schema, and each effect satisfies its authority and subject rules;
- returned metrics equal the validated plan pair and never exceed the configured per-value or aggregate envelope.
- value-flow sets are nonempty, bounded, canonical, and committed by the exact
  effect or channel registry identity;
- economic reclassification changes the manifest, profile, catalog, and every
  downstream verified-law-set identity.

Negative tests cover noncontiguous precedence, wrong profile bindings, hidden profile effects, schema-root divergence, unknown effect and channel IDs, wrong effect authority and subject commitments, wrong effect/channel payload shapes, cross-class reasons, and aggregate payload overflow. Existing `CommitPlan` and `OutboxPlan` constructors separately reject duplicate ordinals and canonicalize operation order.

## Trusted dependencies and assumptions

The crate adds no external dependency. It reuses the workspace's closed value, canonical codec, schema, profile, plan, and decision crates with default features disabled. It assumes the selected commitment provider is collision resistant for promoted use, the profile's definition and policy commitments identify independently reviewed meaning, and callers retain the resulting catalog identity with any validation evidence they treat as authoritative.

Catalog validation is deterministic and performs no I/O, time reads, randomness, threading, global mutation, interior mutation, or external delivery. Work is bounded by the definition, plan-item, depth, node, byte, and collection limits declared above.

## Construction order

Projects should construct values in this order:

1. Build the closed schema.
2. Build reason, effect, and channel definitions with explicit reviewed
   `OperationSemantics` classifications.
3. Build `CatalogManifest` under the selected commitment provider.
4. Copy the manifest's hashes into `ProfileBindings`.
5. Add the manifest's exact registry entries to `ProjectProfile`.
6. Build `ProjectCatalog`, which rechecks every relationship.
7. Require catalog validation before a shell interprets a commit or outbox plan.

## Project examples

### ZenoDEX

Effects can represent mint, burn, collateral transfer, fee allocation, settlement, liquidation compensation, and authenticated-state publication. Channels can represent audit, oracle, settlement, and notification delivery.

### ZenoStorage

Effects can represent agreement updates, provider obligations, repair plans, settlement evidence, and repository publication. Channels can bind provider placement, retrieval, repair, payment, and audit destinations.

### ZenoMail

Effects can represent mailbox, device, key-epoch, and delivery-state transitions. Channels can bind encrypted delivery, notification, storage placement, and acknowledgement payloads.

### PopperPad

Effects can represent claim/evidence publication, quarantine, supersession, challenge, and bounty lifecycle changes. Channels can bind verifier execution, artifact storage, anchoring, and federation.

### Helix

Effects can represent deterministic policy, case, claim, evidence, automation, and guarded-autopilot changes. Channels can bind collection, webhook, on-chain, federation, and operator-confirmation delivery.

### LucyOS

Effects can represent capability/object lifecycle and scheduler decisions. Channels can represent IPC or inter-kernel messages. Hardware-facing machine operations should use a separately reviewed Lucy machine catalog with architecture-specific refinement evidence.

## Nonclaims

A valid catalog establishes that a plan uses reviewed IDs, types, bindings, and resource limits. It does not establish that:

- a project's business algorithm is correct;
- a policy commitment describes a safe policy;
- an external delivery succeeds;
- a shell correctly interprets an operation;
- a classification commitment proves that an operation is non-value;
- a migration preserves semantics;
- a proof or runtime refinement has been completed.

Those claims require project-specific invariants, runtime refinement, evidence, and promotion policy.
