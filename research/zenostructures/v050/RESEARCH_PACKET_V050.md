# ZenoStructures 0.5.0 research packet

**Date:** 2026-07-31  
**Status:** `TESTED_ONLY / PROVISIONAL COMPOUND-CONTRACT HYPOTHESES`  
**Mount status:** `UNMOUNTED / NO_RUNTIME_AUTHORITY`

## Research method

The programme treats a data structure as an observable contract rather than a name or storage backend:

```text
admitted immutable values
+ operations
+ algebraic laws
+ canonical normalization
+ typed rejection and witness precedence
+ semantic projections
+ exact lineage/authority fibers
+ canonical encoding and roots
+ deterministic resource bounds
```

For each candidate the process was:

```text
problem discovery
  -> nearest-family and prior-art attack
  -> semantic fingerprint
  -> smallest executable reference
  -> bounded counterexample search
  -> mutation and complexity pressure
  -> formal obligations
  -> retain, revise, demote, or reject
```

A candidate remains provisional until independent review of citation graphs, patents, standards, dissertations, non-English sources, and implementation equivalence is complete.

## Active executable hypotheses

### SIDF — Stutter-Invariant Divergence Forest

**Problem:** Different runtimes may insert retries, reopens, or no-op delivery attempts before the same real transition. Raw trace comparison therefore reports false divergence, while ordinary stutter erasure destroys exact replay positions.

**Contract:** Erase only events with fresh certificates that before and after observations are equal. Retain a semantic progress trace and an exact half-open source-span fiber. Select the globally earliest semantic divergence and then the lexicographically first implementation pair. Keep semantic-witness and lineage-witness roots distinct.

**First falsifier:** Inserting a certified stutter changes the semantic witness, or removing an uncertified event does not.

### RSPT — Retraction-Sealed Patch Trie

**Problem:** A logically correct patch does not prove that every durable row, authority record, receipt, and provenance item exists exactly once.

**Contract:** Admit durable layouts only when `encode(reopen(d)) = d`. Check every patch precondition against one unchanged logical pre-state, apply atomically, encode the complete successor layout, reopen it, and emit one seal over all pre/post logical and durable roots.

**First falsifier:** A missing or surplus durable row is accepted because the selected logical root remains equal.

### CRQG — Certified Retraction Quotient Graph

**Problem:** Runtime graphs contain retry/reopen/idempotent-delivery cycles that should be observational stutters, but arbitrary cycle deletion can erase state-changing authority edges.

**Contract:** Generate quotient equivalence only from proof-relevant observational-identity edges. Keep progress edges visible, retain exact event lineage separately, and emit one canonical minimal progress-cycle witness.

**First falsifier:** An observation-changing edge is collapsed or a certified stutter changes the quotient progress graph.

### CWET — Closed-World Evidence Trie

**Problem:** An authenticated dictionary can prove a key is absent from the current tree without proving that the key belonged to the reviewed evidence universe or that absence was explicitly declared.

**Contract:** Commit separately to a finite reviewed key universe and to a total assignment over that universe. Queries distinguish present, declared absent, and outside universe. Authority and provenance slots remain explicit.

**First falsifier:** An omitted in-universe key and an out-of-universe key produce the same accepted answer.

### CWCRM — Canonical Witness-Carrying Reconciliation Map

**Problem:** Last-writer-wins and ordinary materialized maps erase disagreement and exact contributor history. A resolution policy can also silently rewrite the replicated base.

**Contract:** Merge exact contributions by ACI set union over a closed key manifest. For each key derive canonical semantic value classes with exact contributor fibers. Return a canonical minimal conflict basis. Maintain separate manifest, semantic, lineage, and conflict roots. Resolution is projection-only and must be bound to the exact current conflict root; it never mutates base history.

**First falsifier:** Merge order changes the map, a same-ID/different-content contribution is accepted, or a stale resolution plan succeeds.

**Complexity repair:** The first reference rescanned all contributions for every manifest key. A retained quadratic oracle now checks a single-pass grouping implementation. At 6,144 keys and 12,288 contributions, the measured grouping phase was 12.537× faster in the retained CPython run. This is a phase-specific prototype measurement, not a production performance claim.

### FQAT — Frontier-Qualified Absence Trie

**Problem:** A non-membership proof does not distinguish explicit authoritative absence from an unobserved key, an outside-universe key, or a query made against a stale caller-selected causal frontier.

**Contract:** Combine a closed universe, exact causal events, and a separately sealed frontier. Query included maximal events and return exactly one of `PRESENT`, `DECLARED_ABSENT`, `CONFLICT`, `UNOBSERVED`, `OUTSIDE_UNIVERSE`, or `UNSEALED_FRONTIER`.

**First falsifier:** A caller-selected stale frontier manufactures `DECLARED_ABSENT` for a key with an unseen present event.

### PFCT — Projection-Fiber Collision Trie

**Problem:** Materialized-view or projection roots are often used as if they uniquely identify exact base records even when the projection is non-injective.

**Contract:** Retain exact records and a closed manifest of deterministic projections. Freshly recompute every projection. Comparison returns exactly `IDENTICAL`, `DISTINCT`, or `COLLISION`. A collision binds both exact roots, the equal projection value, and the canonical first exact-record difference.

**First falsifier:** Different exact records with equal projections are reported as identical or have no replay-checkable witness.

### CSIM — Cut-Sealed Invalidation Map

**Problem:** Dependency invalidation can be sound yet invalidate too much, silently rewrite unaffected derivation steps, or claim incremental parity without complete recomputation.

**Contract:** For changed base claims, compute the least forward invalidation closure in a closed acyclic single-writer derivation manifest. Preserve every unaffected claim and derivation step byte-for-byte. Emit the survivor-to-affected cut and cause basis. Merge compare-and-replace update plans by ACI union or return a symmetric conflict. Seal the result against transparent full recomputation.

**First falsifier:** An unaffected derivation step changes, the affected set is not least, or the incremental successor differs from full recomputation.

### AIRB — Authority-Indexed Refinement Braid

**Problem:** Schema and implementation migrations need to preserve one semantic history while representations, permitted writers, and authority phases change.

**Contract:** Store an immutable position-indexed braid of legacy/successor representation cells and lifecycle edges over the exact sequence `LEGACY -> SHADOW_REPLAY -> DUAL_CHECK -> QUIESCED -> AUTHORITY_SWITCH -> POST_SWITCH_VALIDATION -> LEGACY_DISABLED`. Dual representation cells must commute to the same semantic before/after pair. Semantic, authority, and exact lineage roots are distinct. Rollbacks crossing the switch require a certified cut and explicit override.

**First falsifier:** An old writer commits after the authority switch, a phase is skipped, representations disagree semantically, or rollback restores balances while erasing configuration or lineage.

## Formal-design queue

- `PFPL` — Potential-Fibered Persistence Ledger: a formal hypothesis relating persistent version DAGs, resource potential, and exact spend authority.
- `ICRWT` — Intervention-Closed Recovery Word Tree: a formal recovery-language structure whose leaves classify every bounded crash/intervention word as PRE, POST, or a minimal bad prefix.
- `PCFL` — Proof-Context Fiber Lattice: a formal product-lattice structure in which only the maximal complete proof context authorizes use and the least differing dimension witnesses substitution.

These designs are not counted as executable candidates and have no proof receipt.

## Component-only structures

`WPT`, `MDF`, `CPSQ`, `CDRT`, `SCR`, `DRFI`, and `CCAL` remain useful executable components but were demoted because their central operations have stronger overlap with established patch, divergence, stutter quotient, canonical reconstruction, trace-monoid, retry/idempotency, or counterexample-antichain families.

## Prior-art pressure

The current search materially narrowed the claims:

- StateFuse and CRDT reconstruction work strongly pressure broad CWCRM novelty; the residual hypothesis is the exact closed-manifest, four-root, conflict-basis, projection-only-resolution contract.
- Vector clocks, causal contexts, OR-sets, authenticated dictionaries, and non-membership proofs pressure FQAT; the residual hypothesis is its sealed-frontier six-way epistemic result algebra.
- Materialized views and provenance pressure PFCT; the residual hypothesis is the total exact/projection trichotomy with a canonical collision fiber witness.
- Self-adjusting computation, view maintenance, build DAGs, and dynamic slicing pressure CSIM; the residual hypothesis is least closure plus exact survivor-step retention, explicit cut, compare-replace algebra, and full recomputation seal.
- Schema-change and refinement literature pressure AIRB; its claim is narrowed to the immutable position-indexed authority braid with three distinct roots and rollback-cut semantics.

No reviewed source matched the complete observable contract of an active candidate. Most ingredients are anticipated, so absolute novelty is not claimed.

## Executed evidence

```text
209 unit/adversarial tests passed
86% source statement coverage
2,815 bounded finite cases
19 named bounded laws
19/19 named semantic mutants killed
0 bounded survivors
11,264 contract-lattice configurations
4 independent Python hash seeds
1 campaign digest
novelty metadata gate: PASS
clean ZIP extraction and checksum replay: PASS
```

## Tool artifacts

The sealed release contains:

- a Research Kernel claim/evidence/contradiction graph;
- seven LEAP surprise packets;
- nine Morph source/target relation cards;
- eight ESSO state-machine models;
- a ZAG/Zenith rejected-neighbor archive and run manifest;
- theorem-oriented Lean source without `sorry`, not compiled here;
- an independent Julia oracle, not executed here;
- a ZenoFCIS adapter specification and 76-task dependency graph.

Prepared inputs are not represented as successful tool executions. Consensus was attempted, but its connected monthly quota was exhausted; public primary sources supplied the retained search record.

## ZenoFCIS integration boundary

The research values create no command, state, candidate, receipt, bundle, database, migration, or effect authority. Any Rust port must remain isolated behind a non-default research feature and preserve exact bytes, typed failures, root separation, and Python/Rust/Julia vectors. Production promotion requires independent prior-art review, concrete Lean or equivalent proofs where claimed, strict decoders, no-std and Miri evidence, mutation coverage, authenticated-source integration, no-bypass audit, and exact-head review.
