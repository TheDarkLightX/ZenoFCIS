# Outbox Delivery Identity v1

Status: normative V1 protocol repair for issue #73.

## Purpose

One canonical outbox obligation must retain the same idempotency identity when
it moves between the pure reference shell and an authorized concrete shell.
Deployment authorization is provenance for the obligation. It does not change
the obligation's implementation-neutral identity.

The normative preimage is:

```text
ZCVE domain "zeno-fcis/delivery", version 1
+ CandidateId
+ canonical OutboxEntry bytes
    -> DeliveryId
```

## Inputs and Outputs

Inputs are the exact candidate identity and canonical outbox entry already
sealed into one `CommitBundle`. The output is one `DeliveryId`.

The authorization ID, policy ID, interpreter identity, deployment identity,
and replay identity remain separately persisted and validated. They do not
enter the delivery-ID preimage.

## Authority Boundary

`OutboxEntry::delivery_id` accepts the candidate hash named by its public API.
The pure reference shell and SQLite shell both pass the exact
`CommitBundle::candidate_id`. The SQLite shell still checks that the stored
authorization maps to that candidate under the shell's policy before exposing
or delivering the row.

The destination receives the delivery ID, exact entry hash, and exact entry.
Acknowledgement succeeds only for that identity and entry hash.

## Version and Migration

SQLite schema v3 is the first schema with the candidate-derived delivery
identity. Schema v2 used the deployment-specific authorization hash even
though the plan API specified a candidate hash.

ZenoFCIS does not reinterpret existing v2 rows. Opening a v2 store fails with
`UnsupportedSchemaVersion(2)` until an independently reviewed migration
reconstructs each exact authorized bundle and rewrites every delivery identity.

## Deterministic Resource Bounds

The repair adds no collections or unbounded input. Each delivery identity hashes
one fixed-size candidate ID followed by one already bounded canonical outbox
entry. SQLite ordering remains the canonical `(candidate_id, ordinal)` order.

## Laws and Negative Cases

1. Reference and SQLite shells produce byte-identical delivery IDs for the same
   candidate and entry.
2. Changing the candidate changes the delivery identity.
3. Substituting the authorization ID for the candidate ID does not reproduce
   the accepted identity.
4. Entry-hash acknowledgement remains exact and idempotent.
5. Schema v2 is rejected rather than silently reinterpreted.
6. Authorization-to-candidate mapping remains mandatory SQLite provenance.

Permanent SQLite tests compare the reference and concrete pending-delivery
artifacts and retain the candidate-versus-authorization substitution mutant.

## Trusted Dependencies and Assumptions

The law assumes collision resistance for the approved SHA-256 provider,
canonical `OutboxEntry` encoding, exact candidate sealing, and SQLite's
documented transaction behavior.

## Explicit Nonclaims

- SQLite schema v5 separately closes complete authorization/bundle/receipt/
  replay/outbox row-set reconstruction before delivery.
- It does not qualify a production destination or prove exactly-once behavior
  beyond the documented idempotent acknowledgement protocol.
- It does not supply a migration from schemas v4 or earlier.
- It does not make `AuthorizationId` portable across deployments.
