# Decoder allocation hardening

This package hardens count-prefixed collection allocation in the pure canonical
decoders for ZCVE/1 values, canonical patches, commit plans, and outbox plans.
It changes initial allocation strategy only. Canonical bytes, decoded values,
resource-limit semantics, rejection precedence, and public error variants stay
unchanged.

## Inputs and outputs

The inputs remain immutable untrusted bytes plus the decoder's explicit limits.
Successful outputs remain the same admitted `Value`, `CanonicalPatch`,
`CommitPlan`, or `OutboxPlan`. Malformed or truncated inputs return the existing
typed decode error and no partial authoritative artifact.

## Allocation rule

A declared collection count is attacker-controlled until the corresponding wire
items have been admitted. Every affected decoder now computes its initial vector
capacity as:

```text
min(declared_count, remaining_wire_bytes / minimum_wire_bytes_per_item)
```

The minimum widths are structural lower bounds from the existing formats:

| Collection | Minimum wire bytes per declared item |
| --- | ---: |
| ZCVE tuple or vector | 1 value tag |
| ZCVE record | 2-byte field ID + 1 value tag |
| ZCVE map | two 4-byte blob lengths + two value tags |
| Canonical patch operations | 4-byte operation-blob length |
| Value-path segments | 1 path tag |
| Commit-plan effects | 4-byte effect-blob length |
| Outbox-plan entries | 4-byte entry-blob length |

The declared protocol count remains the loop bound and is still checked against
the selected logical limit. The wire-derived value controls only initial
reservation. A well-formed vector may grow while items are decoded.

## Authority boundary

This change is inside pure admission code and grants no state, effect, outbox,
or shell authority. The existing canonical reconstruction and complete
re-encoding checks remain authoritative for accepted values. Patch application,
catalog validation, candidate sealing, and production publication are unchanged.

## Deterministic bounds

Initial collection reservation is bounded by both the admitted count and the
complete input that remains at the declaration point. A short count-only input
cannot cause a reservation proportional to the configured collection maximum.
Checked division rejects an invalid internal zero-width bound rather than
panicking.

No new dependency is used. The crates remain `no_std + alloc` when built without
their `std` feature.

## Laws and negative cases

The executable regression suite covers:

1. Reservation never exceeds the declared count.
2. Reservation never exceeds the number of minimally encoded items supported
   by remaining wire bytes.
3. Zero minimum width returns the existing length-overflow error.
4. Count-only declarations at reviewed maximum cardinalities fail as truncated
   inputs without reserving the declared number of elements.
5. Existing canonical round trips, exact limit boundaries, malformed-input
   rejection, patch overlap rejection, and duplicate plan-ordinal rejection
   continue to pass.

The permanent repository assurance checker also binds every audited production
`Vec::with_capacity` call site directly to the wire-bounded helper. Its self-test
requires a raw-count mutant to fail, so reverting a call site while leaving the
helper tests intact is detected.

These are bounded executable checks for the exact implementation. They are not
an unbounded parser proof.

## Assumptions

- Each minimum width remains a lower bound for the existing canonical format.
- Callers select complete-input and logical limits appropriate to their
  deployment.
- Rust allocation failure remains a host concern; it is not reclassified as a
  semantic rejection.

## Explicit nonclaims

- This does not establish a process-wide memory or allocator quota.
- A large valid input can still require memory proportional to its admitted
  values and plans.
- This does not make wall-clock time or allocation behavior protocol meaning.
- This does not change ZCVE/1, patch, or plan canonical bytes.
- This does not authorize decoded artifacts or qualify ZenoFCIS for production.
