# Strict canonical plan decoders

`zeno-fcis-plan` exposes `decode_commit_plan(bytes, limits)` and
`decode_outbox_plan(bytes, limits)` for admitting untrusted wire bytes into the
existing `CommitPlan` and `OutboxPlan` types. Both decoders are pure
`no_std + alloc` boundaries. They do not perform I/O or change either canonical
plan format.

## Inputs and outputs

Each function accepts one immutable byte slice and one explicit
`PlanDecodeLimits` value. A successful commit decode returns a `CommitPlan`. A
successful outbox decode returns an `OutboxPlan`. Failure returns one typed
`PlanDecodeError` and no partial plan.

The limits bind:

- complete plan bytes;
- effects per commit plan;
- entries per outbox plan;
- aggregate decoded value nodes across the complete plan;
- aggregate decoded byte and text payload bytes across the complete plan;
- the existing per-value ZCVE depth, node, payload, collection, and input bounds.

The reviewed defaults admit at most 64 MiB of complete input, 4,096 effects,
4,096 outbox entries, 1,000,000 aggregate decoded value nodes, and 64 MiB of
aggregate decoded value payload. Each nested value also remains subject to the
reviewed default `DecodeLimits` unless a caller supplies a tighter profile.

## Authority boundary

The decoder admits bytes into existing immutable plan values. It grants no
authority to execute an effect, publish a plan, or deliver an outbox entry. A
shell must still validate the plan against the selected project catalog, bind it
to the exact candidate, publish through expected-root atomic compare-and-swap,
and deliver committed entries through the replay-safe outbox boundary.

## Trusted dependencies

The implementation reuses:

- `zeno-fcis-codec` for strict ZCVE/1 value decoding and canonical encoding;
- `zeno-fcis-value` for immutable closed values;
- the existing `CommitPlan::try_new` and `OutboxPlan::try_new` constructors for
  canonical ordinal ordering and duplicate rejection.

No dependency was added. External library types do not cross this boundary.

## Deterministic resource bounds

The complete input length and declared item count are checked before allocating
the corresponding item vector. Each destination and payload is decoded under
the selected per-value bounds. Node and payload metrics are accumulated with
checked arithmetic across the complete plan. Any limit violation returns before
a plan is constructed.

Host time and allocation strategy are not protocol evidence. The explicit
logical bounds are deterministic inputs to the decoder.

## Laws

For every admitted plan `p` under limits `L`:

```text
decode_commit_plan(encode(p), L) = p
decode_outbox_plan(encode(p), L) = p
encode(decode(bytes, L)) = bytes
```

The final equality rejects alternate ordinal order and every other encoding
that reconstructs to the same logical plan but is not the canonical byte form.
Every nested value must independently satisfy strict ZCVE admission.

## Negative cases

The regression matrix covers:

- exact and one-over complete-input limits;
- exact and one-over effect and outbox-entry limits;
- exact and one-over aggregate value-node and payload-byte limits;
- propagation of nested ZCVE collection and input bounds;
- duplicate effect and outbox ordinals;
- noncanonical effect and outbox order;
- unknown nested value tags;
- trailing bytes inside an item and after a complete plan;
- truncated length-delimited items;
- empty canonical plans.

## Assumptions

- The input is intended to use the existing canonical plan format.
- `CommitPlan::try_new`, `OutboxPlan::try_new`, and ZCVE/1 encoding retain their
  documented semantics.
- Project-specific operation, authority, subject, channel, destination, and
  payload rules are validated separately by the exact `ProjectCatalog`.

## Explicit nonclaims

- This does not validate a plan against a project catalog.
- This does not bind a plan to a candidate, receipt, patch, or bundle.
- This does not decode receipts or complete commit bundles.
- This does not execute effects or deliver outbox entries.
- This does not prove availability, liveness, constant-time behavior, or
  production suitability.
- Bounded regression tests are not an unbounded proof of decoder correctness.
