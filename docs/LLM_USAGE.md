# Using ZenoFCIS with coding and reasoning models

This guide is for LLMs and the humans reviewing their work. A model can help
propose bounded project artifacts. It must not choose protocol authority or
declare its own output verified.

## Read this first

Use this order:

1. [Quickstart](QUICKSTART.md)
2. [Crate map](CRATE_MAP.md)
3. [Feature matrix](FEATURE_MATRIX.md)
4. The boundary document for the crate being changed
5. Public APIs in the exact source revision
6. Tests and permanent read-only workflows for that boundary

Treat `README.md`, this guide, and generated architecture files as navigation.
Source, canonical protocol values, and independently checked evidence remain
authoritative.

## Safe project workflow

```text
human-reviewed ProjectProfile and ProjectCatalog
    -> generated typed APIs or reviewed typed domain machines
    -> ComposedDomainProgram
    -> complete LawManifest and project-owned law engine
    -> independently checked retained evidence
    -> CatalogCommitAuthority
    -> CatalogAuthorizedTransition
    -> authorized shell
```

An LLM may help write implementations at each step. It may not skip a step by
constructing a lower-level artifact directly.

## What a model may propose

- schema drafts and bounded example values;
- domain decomposition and narrow machine interfaces;
- candidate stable names and identifiers for owner review;
- transition code inside already reviewed types and registries;
- law definitions, counterexamples, and proof obligations;
- composition wiring and candidate merge orders;
- backend requests and adapters;
- tests, negative vectors, documentation, and migration drafts;
- performance or resource-bound hypotheses.

All proposals must be inspectable and deterministic after admission.

## What a model must not decide

A model has no authority to choose or silently change:

- schemas, stable IDs, field/variant IDs, or rejection precedence;
- effect/channel registries, authority rules, or policy commitments;
- accepted business laws or evidence coverage;
- synthesis grammar, finite-domain completeness, or verifier result;
- provider, interpreter, deployment, replay, migration, or activation identity;
- proof, promotion, audit, release, or production status;
- which safety receipts or domain machines a top-level command requires.

These decisions require explicit owner-reviewed inputs and the relevant
checker, authority, or release gate.

## API rules

Prefer:

- generated private-inner transition APIs;
- `DomainMachine` with narrow `MachineInterface`;
- authority-owned `ComposedDomainProgram`;
- complete `LawManifest` and `verify_project_laws`;
- `CatalogCommitAuthority::execute`;
- `CatalogAuthorizedTransition` at a production commit port.

Avoid in production integration:

- raw `Value` when a generated or schema-admitted type exists;
- caller-selected `SemanticId`, `Effect`, or `OutboxEntry`;
- caller-created `TransitionDecision`, `CommitBundle`, or
  `NormalizedDecision` as authority;
- `apply_reference_bundle` as a production commit port;
- `StructuralChecker` as proof authority;
- a generic `CommitmentHasher`, verifier, projector, or interpreter selected
  per request;
- hidden clocks, randomness, I/O, threads, async tasks, mutable globals, or
  executable closures in the semantic core.

For promoted runtime refinement, use `ValidatedNormalizedDecision`, derived
`ValidatedRefinementCase` identities, `ExhaustiveDomainManifest`, and
`evaluate_validated_promotion`. An LLM may propose a manifest or evidence
adapter. It may not declare a domain exhaustive, choose verifier identity, or
convert matching untrusted runtime bytes into promotion authority.

## Formal backend rule

Use the boundary:

```text
tool proposes or checks one bounded artifact
    -> BackendResponse or retained evidence
    -> independent verifier
    -> certificate bound to exact request, tool, source, profile, assumptions,
       artifact, coverage, and verifier
    -> project promotion or authorization policy
```

ESSO is a private optional checker. Owners who have it can implement the public
backend or law-evidence traits in a private crate. Public users can mount Lean,
SMT/Z3, CVC5, Kani, Flux, or another checker. A timeout, crash, disagreement,
unsupported result, or solver `unknown` is indeterminate and grants no
authority.

## Composition rule

Partition by invariant ownership. A domain machine receives only its immutable
state row, command, authenticated context, fixed typed inbox, and deterministic
limits. Cross-domain collaboration uses reviewed ports and one explicit global
composition.

Do not infer parallel safety from disjoint-looking code. Require complete static
read/write/context/effect/outbox footprints, conflict checks, any exact
commutativity evidence, and equality with the canonical sequential result.
For production integration, the authority selects one
`FootprintAuthorityBinding` per component and one
`FootprintEvidenceVerifier`, then passes untrusted
`FootprintCompletenessEvidence` through
`authorize_deterministic_parallel`. Do not treat raw
`verify_deterministic_parallel` success or caller-supplied witness-like data as
production authorization.
ZenoFCIS currently supplies the planning/evidence surfaces and a sequential
executor, not a production concurrent runtime.

## Required response format for implementation work

Report:

1. exact base and head revisions;
2. crates, features, public APIs, schemas, and stable IDs changed;
3. authority boundary and trusted dependencies;
4. deterministic resource bounds;
5. laws and negative cases;
6. exact commands and workflows run;
7. assumptions and explicit nonclaims;
8. unresolved blockers and one bounded next package.

Do not call a branch green unless the permanent read-only workflows pass at the
exact reported head.

## Prompt template

```text
Repository: <exact repository and base revision>
Bounded package: <one crate or adapter>

Reviewed inputs:
- profile/catalog/schema identities:
- stable IDs and precedence:
- authority/interpreter/deployment bindings:
- resource bounds:
- required laws and evidence:

Required output:
- pure inputs and decision:
- patch/effect/outbox behavior:
- public API:
- positive and negative tests:
- no_std or std environment:

Forbidden:
- ambient effects in the core
- raw production authority
- identifier or precedence changes
- self-asserted proof/promotion
- unbounded completion claims

Validation:
- Rust toolchain:
- focused tests:
- workspace tests:
- no_std checks:
- permanent workflow:

Nonclaims:
- <what this package does not establish>
```

## Review checklist

- Does expected input come from an external admitted witness rather than from
  the artifact being validated?
- Can a raw lower-level constructor bypass the intended safe API?
- Are all values transitively owned, immutable, bounded, and canonically
  encoded?
- Does ordinary rejection carry no candidate, state transition, effect, or
  outbox obligation?
- Does committed failure intentionally bind its authoritative changes?
- Are state, command, context, policy, provider, interpreter, deployment,
  replay, laws, and evidence bound to the same invocation?
- Are effect and outbox conflicts included in composition?
- Are project invariants and conservation checked over pre-state, command,
  context, post-state, effects, and outbox together?
- Does the shell publish the exact authorized tuple atomically?
- Are proof scope, test bounds, and nonclaims explicit?
