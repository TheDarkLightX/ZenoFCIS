# Project Relational Laws

## Purpose

Schemas, catalogs, patches, and plans establish bounded structural validity.
They do not establish relations between the admitted pre-state, command,
authenticated context, successor state, commit evidence, and durable external obligations. This
package makes those project laws mandatory inputs to production authorization.

## Inputs

- One exact `ProjectCatalog`.
- A complete `LawManifest`.
- One nonzero source-commit identity.
- Retained evidence for every proof-required law.
- One exact reviewed `ProjectLawEngine`.
- One nonzero law-engine build identity.
- For genesis:
  - one exact policy and genesis-binding identity;
  - one schema-admitted initial root state;
  - an explicit required-or-inapplicable declaration on every law.
- For each invocation:
  - admitted pre-state;
  - admitted command;
  - admitted authenticated context;
  - selected decision and reason;
  - successor state for committing decisions;
  - exact patch, commit plan, and outbox plan.

## Outputs

- `VerifiedProjectLaws<H, L>`, which owns the reviewed law engine and binds:
  - the exact catalog/profile/schema/algorithm;
  - the complete law manifest;
  - exact retained evidence;
  - the law-engine type and build identity.
- `LawEvaluation`, which exists only when every applicable required law
  deterministically reports `Satisfied`.
- `GenesisLawEvaluation`, which exists only when every genesis-required law
  deterministically reports `Satisfied` for the exact initial state and policy.
- A law-set identity and evaluation identity included in every production
  authorization and rejection.

## Authority boundary

`LawDefinition`, `LawManifest`, evidence envelopes, and observations are
inspectable data. None grants commit authority.

`verify_project_laws` is the only constructor for `VerifiedProjectLaws`. It
checks exact proof subjects, rehashes retained artifact bytes, and calls the
production-owned `LawEvidenceVerifier` for every proof-required artifact.
`CatalogCommitAuthority` owns the resulting verified law set and invokes its
runtime law engine during the separate genesis ceremony and after structural
transition validation. A successful genesis evaluation is bound into private
`CatalogAuthorizedGenesis`; a successful invocation evaluation is bound into
private transition authorization.

The runtime law engine and formal-evidence verifier are trusted project
adapters. Their identities are bound into policy, but a hash is not
self-authenticating evidence. Production promotion still requires independently
reviewed implementations and retained proof artifacts where the manifest
requires them.

## Formal checker integration

The law manifest is tool-neutral. It specifies the exact claim, reviewed
checker semantics, and required coverage. A separate evidence policy selects
the concrete checker implementation for a release.

Public deployments can mount Lean, Z3, CVC5, Kani, Flux, or another available
checker. ESSO remains private and optional: users who have it can implement the
same `LawEvidenceVerifier` interface in a private repository without adding an
ESSO dependency, source file, stable identifier, or release requirement to
ZenoFCIS.

Every checker receives:

```text
source/profile/schema/algorithm bindings
catalog and law-manifest identities
law ID and exact claim
checker-profile identity
query and ordered assumptions
coverage declaration
runtime law-engine build identity
producer envelope
exact retained artifact bytes
```

It returns `Attested`, `Refuted`, or `Indeterminate`. Missing tools, unsupported
input, solver `unknown`, timeout, crash, malformed output, and disagreement
grant no authority. A formal certificate never replaces the fresh executable
law evaluation for a concrete invocation.

[Flux: Liquid Types for Rust](https://doi.org/10.1145/3591283) is relevant for
ownership-aware refinement checking and strong updates inside reviewed Rust
implementations. Flux evidence must bind the exact annotated source, Flux and
Rust toolchains, configuration, target, trusted/ignored items, solver, and
assumptions. It does not by itself prove deployment identity, external effects,
composition, or requirement completeness.

## Trusted dependencies

- `zeno-fcis-codec` for canonical bytes and commitments.
- `zeno-fcis-project` and `zeno-fcis-catalog` for profile and registry identity.
- `zeno-fcis-evidence` for source, tool, artifact, and coverage envelopes.
- `zeno-fcis-transition`, `zeno-fcis-patch`, and `zeno-fcis-plan` for the exact
  normalized decision surface.
- The selected commitment provider and project law engine.

No new external dependency is introduced.

## Deterministic resource bounds

- At most 4,096 law definitions.
- Exactly one family policy for every closed `LawKind`.
- At most one retained evidence envelope per law.
- At most 64 MiB of retained proof artifacts per law set by default.
- At most 4,096 observations per invocation, further restricted by
  `LawLimits`.
- Genesis uses the same observation bound and must return exactly the complete
  required genesis-law identifier set.
- Canonical law names and evidence values retain their existing bounded forms.
- No wall clock, timeout, thread, filesystem, network, randomness, or ambient
  process state is observed by the semantic law layer.

## Laws

1. The manifest policy commitment equals the profile policy binding.
2. The manifest-generated claim registry equals the profile's complete
   `RegistryKind::Claim` set. Missing and hidden extra claims fail.
3. Every closed law family is either required or explicitly not applicable
   under a nonzero rationale commitment.
4. State invariants, rejection purity, and committed-failure effects are
   always required.
5. A required family has at least one definition; a not-applicable family has
   none.
6. Every proof-required law has exactly one evidence envelope binding the exact
   source, profile, schema, algorithm, claim, and declared coverage.
7. Runtime-only laws reject retained proof evidence so stale optional artifacts
   cannot silently affect identity.
8. The selected formal verifier independently replays every retained proof
   artifact against the complete law proof subject before a verified law set
   exists.
9. Every applicable law appears exactly once in an invocation evaluation.
10. Missing, duplicate, extra, violated, or indeterminate observations fail
    closed.
11. Ordinary rejection exposes no patch, successor, commit plan, or outbox
    plan; this law is evaluated by the framework itself.
12. A production authorization binds the exact law-set verification and
    invocation evaluation identities.
13. Every law explicitly declares whether it applies to genesis. State
    invariants are always required; reject-purity and committed-failure laws
    are always inapplicable there.
14. Genesis evaluation returns every required genesis law exactly once and no
    other law; missing, duplicate, extra, violated, or indeterminate results
    fail closed.
15. Value-moving effect and channel classifications mechanically derive their
    minimum economic law families from the exact catalog.
16. Every catalog-derived economic family covers both `Accept` and
    `CommittedFailure`; one committing decision cannot be omitted.
17. A custom value relation binds one exact registered law and claim whose
    retained evidence is independently checked.

## Negative cases

- Missing family policy.
- Duplicate law ID or name.
- Required family with no definition.
- Not-applicable family with a hidden definition.
- Policy hash or claim-registry mismatch.
- Stale source, profile, schema, or algorithm evidence.
- Wrong claim or coverage.
- Missing, duplicate, hidden extra, or rejected proof evidence.
- Artifact digest mismatch, verifier refutation, or verifier indeterminacy.
- Tool, query, assumption, coverage, or checker-profile substitution.
- Missing, duplicate, hidden extra, violated, or indeterminate observations.
- Missing or invalid genesis applicability, or a genesis observation set that
  differs from the exact required set.
- Debit, credit, fee, mint, burn, asset, recipient, authority, subject, and
  effect-count mutations in the executable fixture law engine.
- Aggregate multi-effect imbalance.
- Value classification with missing conservation, debit/credit, mint/burn,
  fee/rounding, authority, or committed-failure coverage.
- Custom value classification with a missing law, substituted claim,
  insufficient scope, or runtime-only evidence.
- Attempt to authorize an ordinary rejection carrying authority artifacts,
  which is unrepresentable in `LawDecisionView`.

## Assumptions

- A project's law engine implements the claims committed by its manifest.
- Source and build identities are selected by an external release process.
- The production owner selects an evidence verifier that actually checks
  retained artifacts and pins its complete execution environment.
- Project-specific numeric units, rounding, asset semantics, and authority
  derivations are supplied by the reviewed law engine.
- Effect and channel value classifications are selected and independently
  reviewed by the project owner; their commitments are identity bindings, not
  proofs by themselves.

## Explicit nonclaims

- This is not an automatic theorem prover or a universal financial semantics.
- A successful executable check is not an unbounded proof.
- A build hash does not attest the running binary by itself.
- `StructuralChecker` is not proof authority.
- ESSO is not bundled, required, or treated as a universal checker.
- This package does not qualify Solidity, Solana, or other deployment
  interpreters.
- This package does not repair SQLite row-set integrity, parallel effect
  conflicts, exhaustive refinement promotion, or authenticated projector
  context.
- This package does not make ZenoFCIS production-ready without independent
  exact-head review and the remaining release gates.
