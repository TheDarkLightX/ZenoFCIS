# ZenoFCIS 1.0.0-rc.2 release notes

This is the second release candidate for the reusable ZenoFCIS core library.
It retains the complete RC1 package family and closes two authority-model gaps
found during review: project law coverage is now derived from catalogued value
semantics, and generic external work has one durable outbox path.

The [V1 release checklist](V1_RELEASE_CHECKLIST.md) is the normative owner
runbook for exact-source validation, signed tagging, dependency-ordered crate
publication, retained evidence, and rollback. Repository workflows remain
read-only and perform none of those owner actions.

## Changes since RC1

- Catalog format 3 requires every effect and channel to declare reviewed
  `OperationSemantics`, including canonical asset-scoped value-flow sets.
- Project law manifests derive the complete required economic law families and
  committing-decision coverage from the exact catalog. A custom value relation
  requires one exact registered claim and independently retained evidence.
- `CommitPlan` records are explicitly non-executable committed evidence.
  Catalog construction rejects value-moving effects, and every external
  operation, including value movement, must use a durable outbox channel.
- `BoundDeliveryInterpreter` names the concrete policy-owned interpreter that
  validates and delivers candidate-bound outbox obligations. Committing a
  transition never invokes that interpreter.
- Release packaging is version-neutral through `tools/rc_package.py`; the
  retained Rustdoc archive fixes regular-file and directory modes as well as
  timestamps and ownership, so builder umasks do not change archive digests.

## Complete candidate surface

RC2 contains 33 version-aligned publishable Rust crates and one private compiled
code-generation fixture. The public family includes:

- immutable bounded values, ZCVE/1 canonical encoding, deterministic budgets,
  preconditioned patches, plans, receipts, and candidate bundles;
- project profiles, schemas, catalogs, generated typed transitions, derived
  relational laws, nominal genesis and transition authorization, fixed-size
  domain machines, and explicit global composition;
- proof-carrying deterministic-parallel planning with complete static footprint
  evidence and canonical sequential parity requirements;
- tool-neutral protocols for Lean, SMT/Z3, CVC5, Kani, Flux, private ESSO, and
  other independently checked backends;
- strict invocation-bound mounted-decision reconstruction, canonical exhaustive
  domains, independently verified coverage, and content-addressed promotion;
- bounded reference authenticated state, candidate-bound authenticated
  authority, persistent collections, and crash-atomic SQLite schema v5 history
  reconstruction with durable outbox delivery;
- checked examples, human and LLM documentation, complete public Rustdoc,
  `.crate`, source, Rustdoc, and Linux diagnostic-binary archives, SHA-256
  manifests, a CycloneDX SBOM, and provenance inputs.

## Supported core claim

RC2 provides bounded, immutable, canonically encoded construction and validation
primitives for FCIS applications. Production commit authority is a private
nominal value created only through the documented catalog, invocation, provider,
law, delivery-interpreter, deployment, and genesis checks.

`CommitPlan` does not grant execution authority. State publication atomically
records candidate-bound evidence and durable outbox obligations. External work
occurs later through a policy-owned, replay-safe delivery interpreter. The
generic library does not claim atomicity between database publication and an
external system.

## Remaining blockers for 1.0.0

1. Merge the exact reviewed stacked PR series onto `main` without semantic
   drift and rerun every permanent gate at that exact commit.
2. Complete the independent exact-head API, authority, persistence, and
   execution-model review of the final source.
3. Qualify each selected production authenticated-state, persistence, delivery,
   and recovery implementation for its deployment rather than promoting the
   bounded reference backends implicitly.
4. Retain concrete evidence for every verifier and target advertised by a
   production profile.
5. Produce signed release artifacts and hosted provenance from the protected
   immutable tag.

These blockers constrain final and deployment claims. They do not prevent
downstream users from evaluating the RC2 core-library API.

## Explicit nonclaims

RC2 is not an independent audit, a production deployment authorization, a
project-specific economic proof, a side-channel qualification, or approval of
an external JMT, ESSO, solver, prover, compiler, LLM, database, or delivery
runtime. No concrete Lean, SMT, CVC5, Kani, Flux, or private ESSO engine is
bundled. A checker adapter grants no authority unless the selected project owns
and verifies the exact evidence policy.
