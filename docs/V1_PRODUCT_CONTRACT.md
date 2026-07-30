# ZenoFCIS V1 product contract

This document freezes the reusable core-library scope for `1.0.0-rc.3`. It is
the product-level complement to the protocol and authority documents. A feature
belongs in RC3 only when it supports one of the adopter journeys below without
silently broadening the production claim.

## Intended users

| User | Supported outcome |
|---|---|
| Library adopter | Compile a minimal immutable transition through the umbrella crate and select only the required features. |
| Project author | Create, check, explain, graph, and reproducibly generate a typed composition from one inert `.zeno` file. |
| Software architect | Define reviewed project meaning, narrow fixed domain machines, explicit global composition, laws, and nominal authority. |
| Formal-tool integrator | Mount Lean, SMT/Z3, CVC5, Kani, Flux, private ESSO, or another checker behind the common bounded backend and evidence protocols. |
| Runtime integrator | Compare complete mounted decisions, publish authorized transitions atomically, and deliver exact durable outbox obligations. |
| Release maintainer | Rebuild the complete crate family, documentation, source, binaries, SBOM, provenance inputs, and checksums from one exact revision. |

## RC3 feature freeze

RC3 contains these product features:

1. Immutable bounded values, deterministic logical budgets, the three-way
   decision algebra, canonical encoding, patches, plans, receipts, and bundles.
2. Project-neutral schemas, profiles, catalogs, generated typed transitions,
   fixed domain machines, and explicit global composition.
3. Derived relational-law coverage, policy-bound genesis, exact invocation
   binding, and nominal production commit authorization.
4. Proof-carrying deterministic-parallel planning whose promoted results must
   equal the canonical sequential result under the exact composition context.
5. Tool-neutral bounded backend, evidence, refinement, synthesis, and
   exhaustive-domain protocols. Concrete provers and private ESSO remain
   optional integrations.
6. Bounded authenticated-state reference semantics, candidate-bound
   authenticated authority, persistent collections, and a crash-atomic SQLite
   reference shell with strict history reconstruction and durable outbox.
7. A project bootstrap generator, checked examples, isolated-consumer fixture,
   human and LLM documentation, executable acceptance scenarios, and
   reproducible release-candidate packaging.
8. A bounded `.zeno` compiler, canonical typed project AST, accumulated
   diagnostics, equivalent authoring builders, derived composition views, and
   bounded relational/temporal evaluation.
9. Public deterministic CVC5, Z3, and Lean adapters, a complete `zeno-fcis`
   CLI, and the executable shared-nothing Mini Determinator semantic model.

No new semantic subsystem should enter RC3 after this freeze. A correction to
an authority or protocol defect requires a new exact candidate head and complete
revalidation. A feature expansion belongs in a later release.

## First successful project journey

The shortest supported route is:

```text
install the umbrella crate with composed-program
  -> optionally create and check one inert .zeno project
  -> run minimal_core and the isolated consumer
  -> define reviewed schema/profile/catalog
  -> generate and inspect the project starter
  -> implement typed domain machines and one explicit composition
  -> define complete project laws
  -> bind genesis, invocation, provider, interpreter, and deployment
  -> publish only nominally authorized transitions
  -> mount optional formal and runtime evidence
```

The acceptance scenarios in `acceptance/features/` exercise this route at the
library boundary. Run `python3 tools/atdd.py run --all` for the complete local
portfolio.

## UX acceptance criteria

RC3 is usable when:

- a new evaluator reaches a successful example with one documented command;
- feature selection explains the smallest supported dependency surface;
- every major adopter goal has one named public entry point;
- the bootstrap path clearly separates generated scaffolding from owner-chosen
  protocol meaning;
- backend documentation shows that ESSO, Lean, SMT, CVC5, and other tools share
  one public boundary without bundling or trusting them;
- every production-facing guide identifies the nominal authorization type and
  the raw reference type that must not cross the boundary;
- failures explain the missing authority, evidence, resource, schema, or
  deployment binding rather than suggesting a bypass;
- humans and coding agents can run the same acceptance and release commands.
- equivalent source and builder construction produce the same canonical AST;
- blocked temporal or external-tool outcomes remain visibly blocked;
- the Mini Determinator demonstrates private work, canonical join, conflict,
  and rollback, with an optional freestanding QEMU kernel integration that does
  not claim production operating-system qualification.

## Explicit nonclaims

The product contract does not claim Cargo API stability before `1.0.0`, project
requirement completeness, a bundled solver or prover, a production scheduler,
general deployment qualification, a mechanized end-to-end theorem for arbitrary
projects, or publication of any crate or release artifact. BDD scenarios are
requirements witnesses. Passing ATDD commands is executable release evidence,
not formal proof or independent audit evidence.
