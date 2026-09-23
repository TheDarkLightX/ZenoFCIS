# 0001: Meaning-first assurance

## Status

Accepted on 2026-09-23. The review baseline is v1.1.0 (`3b2224d`).

## Context

ZenoFCIS seals decisions very well. It has one canonical encoding, sealed
candidates, strict decoders, nominal authority, re-execution at authorization,
and atomic publication with a durable outbox. A review of all 37 crates, the
V1 specification, and the composition audit found that the meaning those seals
protect is mostly supplied by project code the library cannot inspect.

| Assurance link | V1.1 state |
| --- | --- |
| Meaning | Split across `.zeno`, the finite IR, hand-written Rust, and `build.rs` |
| Decision | Strong: pure step, three-way algebra, re-execution |
| Conformance | Weak: law verdicts are self-reported, and proofs checked validity only |
| Integrity | Excellent: ZCVE/1, sealed candidates, strict decoders |
| Authority | Excellent plumbing, but every semantic judgment is delegated |
| Realization | Good: durable outbox and delivery identity |
| Accountability | Partial: re-running the full stack is the only way to check |

The most important finding concerned the formal pipeline. The SMT and Lean
exports contained only the claim, over unconstrained observations. So a result
never depended on the system:

- A proof established a domain validity. The shipped example claims were
  tautologies such as `always atom(pre.100 == pre.100)`.
- A counterexample could be unreachable. The true invariant `count <= 3` was
  refuted with `post.count = 4`.

At RC3 the only claim with exact-tool acceptance evidence was the tautology
claim 501, so the pipeline's acceptance checks passed.

Other findings:

- **Purity and budgets:** user code's purity and budget use are conventions,
  not enforced.
- **Kernel evidence:** the kernel carries little of it. There are no property
  or bounded-proof harnesses, and fuzz targets are built but never run.
- **Accretion:** over 500 public types, 47 workflows, and 117 documents
  accumulated one boundary at a time.
- **Recomputation cost:** each SQLite call revalidates the whole history, so N
  commits cost O(N²).

## Decision

Invest where the weakest links are, meaning and conformance, before adding
further plumbing. The program follows the principles in
[0002](0002-principles.md). It proceeds narrow path first:

1. make vacuous evidence visible;
2. check properties against the transition itself;
3. validate meaning with independently authored examples;
4. only then decide, with evidence, whether a new semantic core and language
   version are needed.

## Consequences

Delivered so far on `agent/v2-assurance-20260923`:

- Vacuity is reported. `zeno-fcis check` classifies every law and claim, and
  `prove` states that current obligations contain no system model. See
  [law and claim substance](../CLAIM_SUBSTANCE.md).
- Properties are checked against the exact finite transition program in two
  ways that must agree: by exhaustive enumeration and by SMT obligations that
  contain the transition relation. See
  [system properties](../SYSTEM_PROPERTIES.md).
- The library commitment-domain namespace is reserved. See
  [reserved domain namespace](../RESERVED_DOMAIN_NAMESPACE.md).

Deferred until the in-flight v1.2 template and shell work lands:

- an exhaustive differential of the template's output adapter;
- deriving the initial state and input domains from executed genesis and
  admission;
- the pure shell core with deterministic simulation.

## Evidence

- Solver runs made with cvc5 during the review, which are not stored in the
  repository. Without a transition relation, cvc5 proved `pre == pre` and
  refuted the true invariant `count <= 3` with the unreachable `post.count = 4`.
  With a transition relation, it proved the inductive step and found a planted
  bug. The in-repository reproductions are the system-property tests below.
- An independent adversarial review by a second model, run in three passes.
  It graded the plan 6 of 10, then 8 of 10 after corrections. Its five
  factual corrections were reproduced against the code before being accepted.
- Tests: `substance_*`, `system::tests::*`,
  `counter_system_properties_are_checked_against_the_exact_program`, and the
  pinned solver differentials.
