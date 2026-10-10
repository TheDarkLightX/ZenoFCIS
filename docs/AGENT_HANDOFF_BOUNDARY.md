# Agent handoff boundary

This document defines the point at which remaining ZenoFCIS work may be delegated without reopening the semantic architecture.

## Fixed architecture

Agents must preserve these decisions:

1. The functional core is pure, deterministic, total on its declared domain, and bounded by logical resources.
2. Decisions are exactly `Accept | Reject | CommittedFailure`.
3. Protocol values are transitively owned and immutable.
4. ZCVE/1 bytes, domain-separated preimages, stable reason precedence, schema identifiers, and profile roots are protocol meaning—not implementation details.
5. State changes cross the boundary as a preconditioned canonical patch.
6. Authoritative effects and outbox obligations are closed data, never executable closures.
7. Patch, plans, receipt, replay binding, and `CommitBundle` share one candidate identity.
8. Shell publication is expected-root atomic compare-and-swap with idempotent outbox delivery.
9. Parallel composition requires complete read/write/context/effect footprints and sequential-result equality.
10. A production claim requires exact mounted-runtime refinement and the promotion evidence required by its profile.

## Forbidden shortcuts

Agents may not:

- add `unsafe` to a semantic crate;
- add ambient time, randomness, I/O, threads, async runtimes, global state, or interior mutability to the core;
- hand-roll cryptography;
- use Serde or a collection crate's internal shape as consensus encoding;
- weaken stable rejection precedence;
- compare only roots when full-decision refinement is required;
- mark bounded tests as unbounded proofs;
- let an LLM choose schemas, synthesis grammars, wiring, proof claims, or release status;
- merge temporary payloads, write-enabled assembly workflows, generated caches, or diagnostic archives.

## Delegable work packages

After the schema/code-generation layer is green, separate agents may implement one package per stacked draft PR:

- generated conversion adapters and negative codec vectors;
- ZenoDEX mounted JSON-line and callable adapters;
- JMT/sparse-Merkle storage planning behind `CanonicalPatch`;
- persistent-collection benchmark adapters;
- Kani, Lean, Z3, CVC5, and translation-validation evidence importers;
- ESSO synthesis/counterexample integration;
- concrete database, crash-recovery, and outbox refinements;
- fuzzing, Miri, dependency review, and reproducible release tooling;
- additional project profiles after ZenoDEX zUSD.

Every PR must state its exact authority boundary, assumptions, nonclaims, pinned toolchains, evidence identities, and the laws it adds. An agent must not broaden scope when a prerequisite is missing; it must fail closed and leave an explicit blocker.
