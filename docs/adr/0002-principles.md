# 0002: Principles for the V2 assurance program

## Status

Accepted on 2026-09-23.

## Context

With LLM-generated code, human attention is the scarce resource. Assurance
becomes affordable only when the part a person must read is small and machines
check that everything else conforms to it.

## Decision

The aim is to make assurance cheap by making review small. For each bounded
context, a person reviews about one page of meaning: types, decision rules,
laws, and claims, plus a short list of trusted components and accepted risks.
Each implementation step is checked by its own mechanism:

| Step | Check |
| --- | --- |
| Contract to finite or semantic program | SMT with the transition relation, or exhaustive enumeration |
| Program to emitted code | Exhaustive or finite target conformance |
| Contract to hand-written Rust | Bounded model checking over the real `execute`, plus differential testing through the real gate |
| Source to binary | Reproducible build or module hash |
| Execution to contract | A library-owned monitor at the authority gate |

The principles:

1. **One denotation for consistency, independent examples for validity.** A
   single typed meaning removes drift between representations. It also
   creates a common mode: a mistranslation flows identically into every check
   derived from it. Owner-authored decision examples, keyed by numeric field
   IDs and compared without the shared resolver, validate meaning
   independently.
2. **Projects own meaning; the library owns judgment.** Project-written
   judgment code is Attested (see [0003](0003-epistemic-status.md)) and counts
   as trusted code.
3. **Enforce, don't request.** Determinism, budgets, and footprints should be
   structural wherever a mechanism can enforce them. Lints are the minimum.
4. **State the epistemic status.** Every status is Identified, Attested,
   Checked, Proved, or Accepted, derived from how the evidence was produced.
   Vacuity is a failure.
5. **Authority is a checkable value.** A certificate plus a small independent
   checker should let a third party re-verify history.
6. **Subtract before adding.** A new abstraction needs a stated need, a
   comparison with the simpler alternative, and evidence. This is the
   repository's AGENTS.md standard applied to architecture.
7. **The kernel carries the strongest evidence.** Each kernel law runs as a
   property test, a fuzz target, and a bounded proof.
8. **Honesty as data.** Assumptions, nonclaims, and residual risks become a
   checkable assurance case.
9. **Every checker ships with controls:**
   - a positive control: a true, non-trivial system property must pass;
   - a negative control: a planted bug must fail with a reachable
     counterexample;
   - a vacuity control: a tautology must be flagged.

## Consequences

Each milestone states the behavior it preserves and names its controls. The
first milestones apply principles 1, 2, 4, and 9 directly. The system-property
checks include a domain-only control and refuse unreplayable solver models.

## Evidence

- The controls in `system::tests` and the CLI synthesis tests.
- The substance analysis tests: `substance_flags_the_shipped_reflexive_examples`
  as the vacuity control, and `substance_keeps_template_transition_laws` as
  the false-alarm control.
