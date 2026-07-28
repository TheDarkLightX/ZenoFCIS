# ZenoFCIS Value-Movement Security Re-audit — 2026-07-28

## Scope

This re-audit evaluates two distinct repository states:

1. **Current default branch**: `main` at `64c53e43c8110a441cf9a72174efb599030dccf6`.
2. **Integrated improvement candidate**: PR #70 at exact head `aea9d9db3ff596ba09afed1f2f1b9b13c914f651`.

The previous audit reviewed `main` at `47c3b659dda8dbd37f3294d090554cb3b2493bbb` and opened issues #54–#58, #61, and #62. PR #60 subsequently fixed directional composition-frame authorization and was merged to `main`. The larger authority, law, composition, and fixed-domain-machine improvements remain on draft PRs #63–#70.

This review is source- and authority-topology-focused. It evaluates whether the improved API makes unsafe value movement unrepresentable or inadmissible. It is not an independent formal proof, a deployment audit, or a review of a concrete project's complete economic specification.

## Executive verdict

The integrated candidate is **substantially stronger** than the prior revision.

It now contains real construction barriers that were previously missing:

- a private nominal `CatalogAuthorizedTransition`;
- a shell-owned transition program rather than caller-supplied transition decisions;
- externally derived command/context commitments;
- sealed approved commitment providers;
- exact policy bindings for catalog, state domain, transition, interpreter, deployment, replay policy, and law set;
- fresh per-invocation relational-law evaluation;
- production SQLite commit accepting only the nominal authorization value;
- proof-carrying composition claims with directional frames, provider-bound assumption discharge, conservative effect/outbox conflicts, and verified sequential parity;
- fixed-size executable domain-machine reference semantics.

Those are meaningful improvements rather than cosmetic wrappers.

However, the requested statement remains unsupported:

> All value-moving authoritative state and effects are safe and correct by construction.

The integrated candidate still permits several authority assumptions that are represented only by caller-supplied hashes, trusted implementation values, or profile declarations. It also retains an unverified genesis path and an incomplete persistent-store integrity relation.

### Current safe claim

For a **trusted ingress authenticator, trusted reviewed transition value, trusted law engine/evidence verifier, trusted interpreter/deployment configuration, schema- and law-correct genesis, and untampered SQLite store**, PR #70 makes it substantially harder to commit a structurally inconsistent, wrong-command, wrong-context, wrong-catalog, wrong-provider, or law-rejected transition through the new authorized ports.

### Claim that is still blocked

The library cannot yet claim, without those trusted assumptions, that:

- the principal or authentication evidence is genuine;
- the replay identity is correctly derived;
- the concrete transition/checker/verifier/interpreter value matches its claimed build/configuration hash;
- every value-moving catalog is forced to declare the necessary conservation and debit/effect laws;
- genesis satisfies the same law and authority regime as subsequent transitions;
- persisted authorization/bundle/receipt/outbox rows are an exact decoded set;
- reference and SQLite outbox delivery identities are identical;
- declared composition footprints are complete;
- composition proof is required for production authorization;
- exhaustive refinement, authenticated projector correctness, and chain deployment semantics are closed.

## Prior finding status

| Prior finding | Status on PR #70 | Re-audit conclusion |
|---|---|---|
| VM-001 raw `CommitBundle` production bypass | Substantially fixed | New concrete production ports require a private nominal authorization. Not on `main`; genesis and persistence remain separate authority paths. |
| VM-002 SQLite bundle/outbox completeness | Open | More provenance is stored, but exact decoded row-set equality is still absent. |
| VM-003 frame overlap used as authorization | Fixed | `covers` is used and tested. |
| VM-004 effect/outbox conflict omission | Fixed at composition-spec verifier | Effects/outbox conflict conservatively, but declared footprint completeness and production integration remain open. |
| VM-005 self-asserted parity hashes | Fixed at composition-spec verifier | Parity is now a bound claim checked through an external verifier. |
| VM-006 assumption proof not bound to provider set | Fixed at composition-spec verifier | Provider component/guarantee set is part of the exact claim. |
| VM-007 command/context self-validation | Fixed in integrated authority path | Externally derived expected command/context bindings are used. Principal/authentication/replay authenticity remains assumed. |
| VM-008 caller-reported resource usage | Open | Resource report remains caller-reported. |
| VM-009 hash-provider impersonation | Fixed | Production authorization uses sealed approved providers and known-answer tokens. |
| VM-010 authenticated projector/proof context | Open | Tracked by #62. |
| VM-017 fabricated exhaustive refinement | Open | Tracked by #61. |
| VM-018 fabricated normalized decisions | Open | Tracked by #61. |

## New and residual findings

### RA-001 — P1: production genesis is schema-admitted but not law-authorized

#### Location

- `zeno-fcis-authority::AuthorizedShellState::new`
- `zeno-fcis-shell-sqlite::SqliteShell::open`
- `zeno-fcis-shell-sqlite::SqliteShell::open_in_memory`
- `zeno-fcis-shell-sqlite::initialize_or_validate`

#### Problem

Production transitions require `CatalogAuthorizedTransition`, but initialization accepts a schema-admitted root value and derives its hash. It does not require a nominal genesis authorization, evaluate genesis-applicable state/supply/authority laws, or bind the exact genesis root into the policy.

Schema validity establishes value shape. It does not establish that balances, supply, reserves, ownership, treasury addresses, or deployment authorities are economically correct.

#### Exploit/failure sketch

A bootstrap path initializes a schema-valid state with:

- arbitrary or inflated balances;
- incorrect total supply;
- reserves below liabilities;
- an attacker-controlled owner;
- a wrong treasury/vault;
- invalid initial sequence, oracle, or governance state.

Every later transition may preserve its declared invariants, but the unauthorized value or authority entered before the first authorized transition.

#### Blast radius

Complete initial supply and administrative authority for any project whose deployment/bootstrap path uses these APIs.

#### Required repair

Introduce a private `CatalogAuthorizedGenesis<H, P, L, I>` that binds:

- exact initial state/root and state domain;
- catalog/profile/schema/provider;
- deployment/interpreter/replay policy;
- initial principal/governance/treasury configuration;
- all genesis-applicable invariant, supply, reserve, conservation, and authority laws;
- source, build, verifier, and evidence identities.

Separate `create_new` from `open_existing`. Existing stores should be reopened from persisted genesis/policy history, not re-authorized by a newly supplied `initial_state` parameter.

Tracked by #72.

---

### RA-002 — P1: principal, authentication evidence, and replay identity are asserted, not verified

#### Location

- `CatalogCommitAuthority::admit_invocation`
- `InvocationWitness`
- `LawCheckInput`
- `docs/CATALOG_AUTHORIZATION_BOUNDARY.md`

#### Problem

`admit_invocation` accepts:

- `principal_hash`;
- `authentication_evidence_hash`;
- `replay_id`.

The API checks only that they are nonzero, then commits them into context/invocation identity. A commitment proves that the same assertion was carried forward. It does not prove that an approved authenticator established the principal, that the evidence corresponds to the context/command, or that the replay key follows the reviewed derivation rule.

The design document explicitly lists ingress authentication as a trusted assumption.

`LawCheckInput` carries the raw context and opaque invocation identity but does not expose the principal, authentication evidence, replay key, and execution bindings as separate exact fields. A project law therefore cannot reliably express or verify all required cross-relations.

#### Exploit/failure sketch

A caller with access to the authority API supplies the hash of an administrator principal, an arbitrary nonzero evidence hash, and a fresh arbitrary replay ID. If the context schema/transition logic does not independently and correctly re-establish the principal relation, the request is bound but not authenticated.

#### Blast radius

Any value-moving command gated by caller/signer role, nonce/replay policy, oracle attestation, governance identity, or authenticated external evidence.

#### Required repair

Replace the raw hash arguments with privately constructed witnesses:

```text
ApprovedAuthenticator + admitted request/context
    -> AuthenticatedInvocation

ApprovedReplayPolicy + AuthenticatedInvocation
    -> VerifiedReplayIdentity
```

The production law subject should expose the exact principal, authentication method/evidence, signer/caller, nonce/replay derivation, oracle evidence, and relevant execution bindings.

Residual tracked under #57.

---

### RA-003 — P1: concrete runtime values are not proven to match claimed build/configuration hashes

#### Location

- `AuthorizationPolicy::try_new`
- `CatalogCommitAuthority::try_new`
- `verify_project_laws`
- `CatalogCommitAuthority::bind_interpreter`
- `SqliteShell::open` / reopen
- public `ProjectLawEngine`, `LawEvidenceVerifier`, and `CatalogTransitionProgram` traits

#### Problem

The policy commits nonzero hashes for transition build, law engine build, evidence verifier, interpreter profile, deployment, and replay policy. The concrete implementations are largely tied by Rust generic *types*, not by a verified identity produced by the concrete values.

Examples:

- `P` is not required to expose a canonical identity equal to `transition_build_hash`.
- `verify_project_laws` accepts an engine `L` beside a caller-supplied `engine_build_hash`.
- the evidence verifier reports its own identity.
- `bind_interpreter` accepts any value of type `I`; it does not verify that its configuration equals `interpreter_profile_hash`.
- a reopened SQLite shell may receive a different same-type interpreter/configuration while retaining the old policy ID.

A nominal type prevents substitution by a different Rust type. It does not prevent a configurable same-type value, changed binary, or permit-all implementation from claiming the reviewed hash.

#### Exploit/failure sketch

A law engine that returns `Satisfied` for every expected law ID is supplied alongside the approved engine-build hash. Or a same-type interpreter is configured with another endpoint/token/vault and is bound using the old policy token. The authorization type remains correct even though the runtime behavior no longer matches the reviewed implementation/configuration.

#### Blast radius

Relational-law enforcement, transition behavior, proof import, external value delivery, and deployment-specific effect interpretation.

#### Required repair

Each production implementation boundary should return a private verified identity token whose canonical identity is checked against the policy:

```text
VerifiedTransitionProgram<P>
VerifiedLawEngine<L>
VerifiedEvidenceVerifier<V>
VerifiedInterpreter<I>
VerifiedDeployment
```

The token should bind source/build/configuration/artifact identity and, where meaningful, independently checked vectors/refinement evidence. Persist and revalidate exact identities across restart.

Residual tracked under #57 and #58.

---

### RA-004 — P1: a value-moving catalog can waive the important economic law families

#### Location

- `zeno-fcis-laws::LawKind`
- `LawKind::mandatory`
- `LawFamilyPolicy`
- `LawManifest::try_new`
- `zeno-fcis-catalog::EffectDefinition`
- `ChannelDefinition`

#### Problem

Only `StateInvariant`, `RejectNoAuthority`, and `CommittedFailureEffects` are intrinsically mandatory.

A profile can mark these families not applicable with only a nonzero rationale commitment:

- asset conservation;
- mint/burn authorization;
- debit/credit/effect equality;
- fee/rounding;
- authority/subject/recipient derivation.

The catalog does not classify an operation's value semantics, so the law manifest cannot mechanically derive which families are required.

#### Exploit/failure sketch

A project defines transfer, mint, burn, withdrawal, payout, fee, or settlement effects but marks every economic family not applicable. The runtime engine only has to satisfy the remaining declared laws. A transfer may then differ from its semantic debit while still receiving `CatalogAuthorizedTransition`.

#### Blast radius

All value-moving operations in an under-specified profile.

#### Required repair

Classify catalog effects/channels/capabilities with closed value semantics and derive a canonical minimum law-family set. `NotApplicable` should require mechanical non-value classification or independently checked proof, not an arbitrary nonzero rationale.

Tracked by #74 and umbrella #58.

---

### RA-005 — P1: SQLite integrity is still row-local rather than exact authorized-bundle set equality

#### Location

- SQLite schema v2
- `initialize_or_validate`
- `snapshot`
- idempotent branch in `commit_with_crash_point`
- `next_pending`
- `validate_authorization_mapping`

#### Problem

The new schema stores stronger provenance, but it does not strictly decode and reconstruct every authorization/bundle/receipt or compare the outbox table with the exact decoded `OutboxPlan` in both directions.

`next_pending` checks:

- authorization row exists;
- policy and candidate match;
- row-local entry hash matches;
- row-local delivery ID matches.

It does not prove that the row appears in the authorized bundle. Startup/reopen does not reject extra or missing rows. Idempotent replay compares redundant bytes but does not prove complete table consistency.

#### Exploit/failure sketch

With database write access or an internal storage defect:

- add an extra row under a real authorization and recompute its local hashes;
- delete a required row;
- replace redundant bytes consistently in only some tables;
- return idempotent replay although a committed obligation is absent.

#### Blast radius

Unauthorized delivery, lost obligations, false replay success, and persistent divergence from the pure model.

#### Required repair

Strictly decode the complete authorization/bundle/receipt surface and enforce exact set equality among authorization, replay, bundle, receipt, and outbox tables before replay success or delivery.

Tracked by #55.

---

### RA-006 — P1: reference and SQLite shells derive different delivery IDs

#### Location

- `OutboxEntry::delivery_id`
- `zeno-fcis-shell::apply_reference_bundle`
- `zeno-fcis-shell-sqlite::insert_outbox`
- `SqliteShell::next_pending`

#### Problem

The API defines delivery identity from candidate ID plus canonical entry.

The reference shell passes `CandidateId`.

The authorized SQLite shell passes `AuthorizationId`.

These IDs normally differ, so the same semantic outbox obligation obtains two idempotency keys across the reference and concrete shells.

#### Impact

Broken exact refinement, possible duplicate delivery after failover/migration, and incompatible retained delivery identity.

#### Required repair

Choose one versioned normative identity: candidate-based or authorization-based. Update all reference/concrete semantics, vectors, migrations, and documentation consistently.

Tracked by #73.

---

### RA-007 — P1 for parallel production claims: composition proof is not part of production authorization

#### Location

- `zeno-fcis-compose`
- `zeno-fcis-domain`
- `zeno-fcis-authority`
- PR #70 integration boundary

#### Improvement

The composition verifier now correctly binds exact claims, uses directional frames, models effects/outbox conservatively, requires exact conflict laws, and verifies parity evidence.

#### Remaining problem

- Component footprints remain declared static values. No theorem establishes that every runtime-observed read/write/context/effect/outbox path is covered for all admitted inputs.
- A verified composition report is not required to construct `CatalogAuthorizedTransition`.
- The fixed-domain-machine executor is explicitly reference-only.

#### Exploit/failure sketch

A rare branch emits a transfer or outbox obligation omitted from the declared footprint. The composition verifier sees no conflict, or a caller bypasses composition verification entirely and authorizes the component transition through the ordinary production authority.

#### Required repair

Require a `CompleteFootprintWitness` per component and a subject-specific `VerifiedCompositionExecution`/report as an input to composed production authorization.

Tracked by #67 and remaining scope of #56.

---

### RA-008 — P1 integration hazard: PR #66 is a competing law path based on the old invocation-validation surface

#### Location

- PR #66 `zeno-fcis-cbc`
- `LawSubject::from_transition`

#### Problem

The alternative CBC branch is based on `main`, not the integrated authority stack. Its `LawSubject::from_transition` calls the older `validate_transition_decision` without externally expected invocation bindings and then copies command/context hashes from the artifact.

If merged independently as an authority-bearing law layer, it reintroduces the prior self-validation problem.

#### Recommendation

Do not merge PR #66 as a second independent law/authority protocol. Preserve useful ideas—complete subject reconstruction, explicit executable/evidence requirements, exact law claims—but reconcile them into the #65/#70 expected-invocation and nominal authorization path.

---

### RA-009 — P2: deterministic transition resource usage remains caller-reported

#### Location

- `TransitionResourceReport::budget_used`
- `CataloguedTransitionBuilder::try_new`
- project transition implementations

#### Problem

Transition limits are shell-owned and checked, but exact usage is supplied as `BudgetUsed` by the transition implementation. The authority verifies the report's consistency and limit identity, not whether every logical operation was charged.

#### Impact

A dishonest or defective transition may under-report logical work, weakening deterministic resource/termination claims and enabling denial-of-service behavior even when value relations remain correct.

#### Required repair

Make the high-level generated transition surface own the meter, or require a nominal metered-execution witness derived from instrumented/generated control flow. Mutation-test omitted charges.

---

## What the green CI establishes—and does not establish

All permanent workflows at the exact integrated head passed, including formatting, Clippy, workspace tests, documentation, `no_std`, Miri, fuzz-build, authority, law, composition, domain-machine, and mounted-ZenoDEX workflows.

This is valuable evidence that the implementation is internally buildable and its retained tests pass. It does not establish:

- ingress authenticity;
- genesis correctness;
- concrete checker/interpreter binary/configuration identity;
- completeness of project economic laws;
- exhaustive law or refinement coverage;
- persistent row-set integrity under attacker-recomputed values;
- deployed Solidity/Solana behavior.

## High-probability audit hotspots after the improvements

1. **Every `try_new` that accepts both a concrete implementation and a hash allegedly identifying it.**
2. **Every API accepting `Hash32` for principal, signature, evidence, nonce, deployment, interpreter, or build without a private verifier-produced token.**
3. **Initialization and migration paths that write authoritative state outside transition authorization.**
4. **`NotApplicable` law-family declarations and nonzero rationale hashes.**
5. **Persistent tables with redundant bytes and row-local hashes but no aggregate exact-set reconstruction.**
6. **Reference/concrete identity derivations that changed from candidate identity to authorization identity.**
7. **Static footprint declarations not derived from code or proved complete.**
8. **Reference-only proof/execution APIs accidentally exported as production authority.**
9. **Competing law/authorization protocols with different invocation-binding rules.**
10. **Caller-reported budget/resource observations.**

## Adversarial test backlog

### Genesis

- initialize with inflated balance/supply while remaining schema valid;
- initialize with wrong owner/treasury/vault;
- reopen with another supplied initial state;
- mutate persisted genesis authorization, root, law set, deployment, or provider.

### Ingress and runtime identity

- claim an administrator principal with arbitrary nonzero auth evidence;
- reuse auth evidence with another command/context;
- choose arbitrary replay IDs for the same authenticated request;
- use same-type transition/law/interpreter values with changed configuration;
- provide permit-all law engine with the approved claimed build hash;
- provide always-attest verifier with the approved claimed verifier identity.

### Law completeness

- catalog a transfer while conservation/debit-effect/authority laws are N/A;
- catalog mint/burn while supply laws are N/A;
- catalog fee-bearing transfer while fee/rounding is N/A;
- add a second value-moving effect without aggregate conservation;
- mutate debit, credit, amount, asset, recipient, authority, subject, fee, rounding remainder, supply delta, or effect count.

### Persistence

- add an extra locally valid outbox row;
- delete one required row;
- substitute authorization/candidate mapping and recompute local hashes;
- alter redundant bundle/receipt bytes in one table;
- crash at every point, reopen, and run complete invariant reconstruction;
- fail over from reference to SQLite destination state and verify exact idempotency identity.

### Composition

- hide effect/outbox access behind rare command, rejection, or committed-failure branch;
- change program build after footprint proof;
- use a verified composition report from another input domain or merge order;
- authorize a component transition without composition witness;
- mutate verifier implementation while preserving its claimed identity.

## Minimum closure order

1. Merge/rebase the nominal authority and law stack only after re-audit findings are accepted.
2. Add law-authorized genesis (#72).
3. Replace raw ingress hashes and self-identified runtime values with verified nominal witnesses (#57 residual).
4. Derive mandatory economic laws from value semantics (#74 / #58).
5. Complete persistent decoded row-set integrity (#55).
6. Unify delivery-ID semantics (#73).
7. Add footprint-completeness proof and composed production authorization (#67 / #56).
8. Close exhaustive refinement and authenticated projector/proof context (#61/#62).
9. Qualify concrete Solidity/Solana interpreters and verified deployments.
10. Freeze one exact release candidate and perform an independent final audit over source, proofs, generated artifacts, persisted schemas, and deployed binaries.

## Final assessment

The improvements materially change the result of the audit:

- The former raw-bundle production bypass is no longer the dominant defect on the integrated candidate.
- Command/context self-validation and hash-provider impersonation are substantially repaired.
- Relational laws and proof-carrying composition now exist as real first-class structures.

The remaining blockers are now concentrated at **genesis, ingress authenticity, implementation attestation, law-set completeness, persistence exactness, and integration of composition/deployment evidence into production authority**.

That is substantial progress. It is not yet a construction theorem for all value movement.
