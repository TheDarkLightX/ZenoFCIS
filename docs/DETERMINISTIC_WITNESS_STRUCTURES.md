# Deterministic witness structures

This checkpoint adopts two narrow contracts from the experimental
ZenoStructures work:

1. compatible merge and canonical conflict evidence for existing
   `CanonicalPatch` values;
2. globally minimal divergence evidence over committed implementation traces.

The implementation is Rust-native and uses existing ZenoFCIS values,
commitments, canonical encoding, and no-std boundaries. It does not import the
Python package or its authority model.

## Pattern and authority record

| Surface | Pattern | Invariant owner | Authority effect |
|---|---|---|---|
| Patch merge | Immutable aggregate plus witness | `zeno-fcis-patch` | None |
| Divergence forest | Immutable derived index plus verifier | `zeno-fcis-diagnostics` | None |

The patch crate already checks all operation preconditions against one
unchanged pre-state and applies a successor atomically. The added merge
operation owns only combination:

```text
same state type
+ same expected pre-root
+ nonoverlapping operations
-> one canonical patch
```

Compatible merges are commutative, idempotent, and associative. Incompatible
merges return an operand-order-independent witness. Path-level witnesses retain
the two exact operations and their common prefix.

The diagnostics crate receives already-computed `Hash32` observations. It
does not interpret or bless them. It:

```text
exact implementation IDs
+ equal-length ordered traces
-> monotone prefix partitions
-> earliest divergent step
-> lexicographically first divergent implementation pair
```

The witness verifier recomputes the globally canonical answer from the owned
traces. Later equal outputs cannot merge implementations whose earlier
prefixes differ.

## Failure and commit semantics

Both surfaces are pure:

- construction returns an immutable value or a closed error;
- failure produces no successor or external effect;
- neither surface performs IO, publication, locking, retries, or recovery;
- neither surface creates nominal ZenoFCIS authority;
- callers must still pass patches through the existing authorization and
  atomic shell boundary.

The patch merge requires equal expected pre-roots. This prevents combining
operations evaluated against different snapshots and then presenting the
result as one atomic patch.

## ZenoDEX use

The patch merge is suitable for independently evaluated ZenoDEX components
whose operations are already bound to one authenticated pre-state. It can
replace ad hoc list concatenation and produce a stable conflict receipt.

The divergence forest is suitable for Python/Rust/Lean/Julia replay campaigns
and migration shadow checks. Each runtime first commits its complete normalized
observation under the relevant ZenoDEX profile. The forest then identifies the
globally earliest disagreement and retains enough committed prefix data for
replay.

Neither structure should be mounted directly into settlement. A ZenoDEX
adapter must bind:

```text
profile + algorithm + schema + input identity + observation codec
```

before an observation commitment can support a promotion decision.

## Deferred candidates

The ZenoStructures Footprint Schedule Forest was not imported. ZenoFCIS
already owns footprint conflicts, precedence, composition order, and
parallel-authorization evidence in `zeno-fcis-compose`.

The Occurrence Ledger was not imported. ZenoFCIS already owns strict replay,
receipts, nullifiers, durable history, and outbox reconstruction. Its
experimental fixed-universe branch semantics need a concrete missing
ZenoFCIS use case before adding another lifecycle model.

These are deliberate scope decisions. They avoid parallel abstractions with
unclear authority ownership.

## Evidence

The focused test surface includes:

- patch merge commutativity, idempotence, and associativity;
- same-path and ancestor/descendant conflict symmetry;
- metadata conflict symmetry;
- trace input-order invariance;
- earliest-step and canonical-pair selection;
- no historical remerge after later convergence;
- equal-trace, duplicate-ID, length, text-profile, and prefix-bound cases.

The complete workspace ATDD command remains the final pre-commit gate.
