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
- a nonzero project-policy commitment.

Authority and subject requirements are closed values:

```text
Any
Absent
Present
Exact(nonzero hash)
```

A commit plan fails closed when an operation is unknown, its payload does not satisfy the schema, or its authority/subject commitments violate the definition.

## Channel definitions

Each channel binds:

- the numeric channel ID used by `OutboxEntry`;
- a stable name;
- an exact destination schema type;
- an exact payload schema type;
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

## Construction order

Projects should construct values in this order:

1. Build the closed schema.
2. Build reason, effect, and channel definitions.
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
- a migration preserves semantics;
- a proof or runtime refinement has been completed.

Those claims require project-specific invariants, runtime refinement, evidence, and promotion policy.
