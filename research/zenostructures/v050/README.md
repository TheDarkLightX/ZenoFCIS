# ZenoStructures 0.5.0 research catalogue

This directory records the public review boundary for the ZenoStructures 0.5.0 deterministic-data-structure research release. It is stacked on the deterministic-witness-structures work in PR #95 and is deliberately **research-only and non-authoritative**.

## Catalogue

The executable active or challenged hypotheses are:

1. `SIDF` — Stutter-Invariant Divergence Forest
2. `RSPT` — Retraction-Sealed Patch Trie
3. `CRQG` — Certified Retraction Quotient Graph
4. `CWET` — Closed-World Evidence Trie
5. `CWCRM` — Canonical Witness-Carrying Reconciliation Map
6. `FQAT` — Frontier-Qualified Absence Trie
7. `PFCT` — Projection-Fiber Collision Trie
8. `CSIM` — Cut-Sealed Invalidation Map
9. `AIRB` — Authority-Indexed Refinement Braid

The formal-design queue is `PFPL`, `ICRWT`, and `PCFL`. Seven additional executable structures are retained as reusable components and four as prior-art baselines rather than being promoted as active novelty hypotheses.

## Executed evidence

```text
209 Python tests passed
86% source statement coverage
2,815 bounded finite cases
19 named laws
19/19 semantic mutants killed
0 bounded survivors
11,264 contract configurations searched
4 independent PYTHONHASHSEED processes
1 campaign digest
CWCRM grouping-phase benchmark, 6,144 keys: 12.537x versus retained quadratic oracle
```

The sealed release identities are retained for reproducibility:

```text
ZIP SHA-256
c58fecb910fdfecc3e3433338664eb70002716c63ef0d3f3353177cb1d7a8daf

source TAR SHA-256
d947d572fef84a181d87d28982cedc485835666dbf97c6d0508a2c775b3108fb
```

## Public/private boundary

The public ZenoFCIS repository contains only candidate contracts, public evidence summaries, prior-art pressure, implementation handoff material, and explicit nonclaims.

The invention machinery is private. This includes abductive generators, search grammars and operators, scoring and selection policies, internal proposal archives, private counterexample-mining procedures, private research-backend adapters, and detailed private-tool replay traces. Those materials are maintained separately from ZenoFCIS, like the private Morph, ESSO, and LEAP systems.

A public evidence receipt may state that a bounded private campaign ran and may bind its output digest. It must not reveal the private implementation, prompts, internal search state, operator catalogue, proposal frontier, or tool-specific trace unless the owner explicitly approves publication.

## Claim boundary

The documented bounded search did not find a reviewed source matching each active candidate's **complete observable contract**. This does not establish worldwide novelty, patentability, freedom to operate, unbounded correctness, production readiness, or deployment authority.

No value or root in this research overlay may enter a ZenoFCIS production commit port merely because it validates. The ordinary chain remains mandatory:

```text
authenticated command and current state/context
  -> deterministic evaluation
  -> nominal authorization
  -> exact receipt and bundle lineage
  -> atomic publication and recovery
  -> committed outbox effects only
  -> no alternate acceptance path
```
