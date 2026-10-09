# ZenoFCIS value-movement security audit

**Audited revision:** `47c3b659dda8dbd37f3294d090554cb3b2493bbb`  
**Additional scope:** open Solidity, Solana/Anchor, and generated-fixture stacks as of 2026-07-27  
**Audit objective:** determine what prevents ZenoFCIS from claiming that every value-moving transition accepted by a production shell is safe and correct by construction.

## Executive decision

ZenoFCIS has a strong semantic foundation: owned immutable values, canonical encodings, schema admission, preconditioned non-overlapping patches, catalog validation, deterministic reason precedence, same-candidate sealing, atomic reference-shell semantics, crash-atomic SQLite publication, and strict patch/plan decoders.

The requested guarantee is **not yet supportable**. The most important blocker is not a low-level arithmetic bug. It is an authority-boundary gap:

> A caller can construct a self-consistent raw `CommitBundle` with arbitrary bindings, patch, effects, and outbox entries and submit it directly to a shell without proving that it was produced by the reviewed `ProjectCatalog` transition path.

That means the existing types prove *cryptographic and structural consistency*, but the production shell boundary does not yet prove *semantic authorization*.

This audit records one P0 construction bypass, several P1 concrete integrity/authorization bugs, and a set of P2 assurance gaps. The P0 and P1 items must close before any general “safe by construction” value-movement claim.

## Guarantee to be proved

A production claim should have a narrow, testable form:

For every production-authorized transition and every adversarial input, either:

1. the command is rejected with no candidate, state change, authoritative effect, or delivery obligation; or
2. exactly one candidate is committed whose state patch and value-moving plans:
   - derive from the exact admitted pre-state, command, authenticated context, policy, catalog, algorithm, interpreter, and deployment bindings;
   - preserve all declared state and accounting invariants;
   - execute atomically and at most once according to the reviewed replay policy;
   - cannot be widened, substituted, reordered, partially published, or reinterpreted by the shell;
   - are reproduced by every promoted implementation.

The guarantee must apply to the **only production entry point**. It is insufficient for one high-level API to be safe while lower-level safe-Rust APIs can create and commit unauthorized value movement.

## Threat model

Treat the following as adversarial:

- commands, context claims, oracle facts, signatures, wire bytes, stored rows, replay identifiers, generated project logic, effect payloads, destinations, and adapter outputs;
- accidental use of a lower-level public API by an application or coding agent;
- database corruption or a compromised process with write access to persistent rows;
- stale, substituted, or cross-profile artifacts;
- malicious implementations of public extension traits unless an approved implementation witness is required;
- compiler, dependency, chain, and deployment variation not bound into the candidate identity.

Memory corruption in safe Rust is out of scope, but safe-API misuse is in scope because the target claim is “correct by construction.”

---

# First-pass findings

## VM-001 — P0: raw `CommitBundle` bypasses catalog and transition authorization

### Location

- `crates/zeno-fcis-plan/src/lib.rs`
  - public `Effect::new`
  - public `OutboxEntry::new`
  - `CommitPlan::try_new` and `OutboxPlan::try_new` enforce ordering, not project policy
- `crates/zeno-fcis-receipt/src/lib.rs`
  - public raw `CandidateBindings`
  - public `CandidateBuilder::seal`
  - `CommitBundle::validate_and_apply`
- `crates/zeno-fcis-shell/src/lib.rs`
  - `commit` accepts `&CommitBundle`
- `crates/zeno-fcis-shell-sqlite/src/lib.rs`
  - `SqliteShell::commit` accepts `&CommitBundle`
- `fuzz/fuzz_targets/candidate_bundle.rs`
  - demonstrates the raw path and treats it as a valid commit

### Why this is a defect

`CandidateBuilder::seal` proves that a patch applies and that the body, component commitments, receipt, and candidate ID agree. It does **not** prove that:

- the profile, context, algorithm, precedence, or budget hashes are the expected ones;
- effects and outbox entries belong to an exact reviewed catalog;
- the patch was produced by reviewed transition logic;
- the successor preserves project invariants;
- the command or context corresponds to the invocation being authorized.

Both shell implementations accept that raw self-consistent bundle. The catalog-aware `TransitionArtifacts` validation is bypassable because the shell does not require it.

### Exploit sketch

1. Obtain the current semantic state and root.
2. Construct a `CanonicalPatch` that increases a balance, changes ownership, or otherwise creates value.
3. Construct optional raw effects/outbox entries using public constructors.
4. Invent nonzero `CandidateBindings`.
5. Call `CandidateBuilder::seal`.
6. Submit the resulting bundle to `zeno_fcis_shell::commit` or `SqliteShell::commit`.

The shell validates same-candidate consistency and commits the unauthorized transition.

### Blast radius

- arbitrary semantic-state mutation when the raw shell API is reachable;
- arbitrary catalog-shaped or uncatalogued authoritative effects in shells that interpret `CommitPlan`;
- arbitrary external delivery obligations;
- complete bypass of project business predicates and invariant proofs.

### Required repair

Introduce a nominal, privately constructible production witness such as:

```text
CatalogAuthorizedTransition
    = exact invocation witness
    + validated TransitionDecision
    + catalog/profile/provider/interpreter/deployment bindings
    + independently revalidated invariant evidence
```

Production shells must accept only that witness. Raw bundles should be one of:

- crate-private;
- available only behind an explicitly non-production `unchecked-primitives` feature;
- accepted only by a clearly named reference/testing API that cannot be confused with the production commit port.

`CommitBundle` remains useful as immutable data, but it must not itself confer commit authority.

### Mandatory regression tests

- a raw sealed balance-increasing patch cannot be passed to the production shell;
- a bundle with invented catalog/profile/context hashes cannot receive an authorization witness;
- a valid bundle from catalog A cannot commit through a shell pinned to catalog B;
- a valid command artifact cannot be substituted for a different command/context invocation;
- compile-fail tests prove the authorized witness cannot be constructed outside its validator.

---

## VM-002 — P1: SQLite outbox rows are not cross-bound to the committed bundle

### Location

`crates/zeno-fcis-shell-sqlite/src/lib.rs`:

- SQL schema for `bundles`, `replay`, and `outbox`;
- `next_pending`;
- `deliver_next`;
- idempotent replay branch in `commit_with_crash_point`.

### Why this is a defect

The outbox table checks row shape, candidate existence, and `(candidate_id, ordinal)` uniqueness. `next_pending` recomputes the row’s entry hash and delivery ID, but it does not prove that the row occurs in the decoded committed bundle’s exact `OutboxPlan`.

The replay table also stores redundant bundle bytes. An idempotent replay compares the attempted bundle to the replay row, but does not establish the completeness and exactness of the associated bundle/receipt/outbox rows.

### Exploit sketch

With database write access or an internal row-construction bug:

1. choose an existing candidate ID;
2. insert an additional outbox row at an unused ordinal;
3. choose arbitrary channel, destination, and payload;
4. compute the correct row-local `entry_hash` and `delivery_id`;
5. call `deliver_next`.

The row passes local checks and is delivered although it was never authorized by the functional core.

A dual attack deletes a required outbox row; an idempotent replay may still report success while a committed obligation is missing.

### Blast radius

Arbitrary external delivery or value transfer through a destination adapter; loss of exactly-once obligation completeness; false successful replay.

### Required repair

- add strict bounded decoders for `Receipt`, `CommitBundle`, and the production authorized-transition envelope;
- on startup, replay, pending-read, and acknowledgement, verify every stored row against the exact decoded bundle;
- verify exact outbox set equality, not merely row-local hashes;
- eliminate redundant replay bundle bytes or require exact equality with the canonical `bundles` row;
- validate receipt bytes and candidate ID against the bundle;
- add a full persistent-state invariant checker and run it before delivery.

### Mandatory adversarial tests

- injected extra row;
- missing row;
- changed channel/destination/payload with recomputed local hashes;
- replay row and bundle row disagreement;
- changed receipt bytes;
- duplicate delivery identity across candidate rows;
- acknowledged row whose content no longer matches its bundle;
- crash at every write point followed by invariant reconstruction.

---

## VM-003 — P1: frame authorization uses overlap instead of containment

### Location

`crates/zeno-fcis-compose/src/lib.rs`, `verify_assume_guarantee`.

### Why this is a defect

`AccessPath::covers` expresses “the declared protected path contains the requested destination.” The verifier instead grants a wiring write when `frame.protected().overlaps(wiring.destination_path())`.

Overlap is symmetric. Authorization is not.

### Exploit sketch

A frame protects and authorizes writes to a narrow descendant, for example `balances[user]`. A wiring requests a broader ancestor such as `balances` or the complete state root. The paths overlap, so the current check can authorize the broader write.

### Blast radius

A composition report can approve writes beyond the reviewed frame, invalidating component isolation and deterministic-parallel assumptions.

### Required repair

Replace the authorization predicate with containment:

```rust
frame.protected().covers(wiring.destination_path())
```

Retain `overlaps` only for conflict detection.

### Mandatory tests

- exact frame authorizes exact destination;
- broad frame authorizes a descendant;
- narrow frame does not authorize an ancestor;
- sibling path is rejected;
- wildcard frame behavior is explicit and tested.

---

## VM-004 — P1: deterministic-parallel verification omits effect/outbox conflicts

### Location

- `crates/zeno-fcis-compose/src/lib.rs`: `Footprint`, `conflicts`, `verify_deterministic_parallel`
- `crates/zeno-fcis-transition/src/lib.rs`: `emit`, `enqueue`, observed footprint construction

### Why this is a defect

`Footprint` contains effect paths, but `conflicts` checks only state read/write relations. Outbox obligations have no footprint class. The transition builder records an effect path only by operation identifier and records no outbox destination at all.

Two tasks can therefore be declared parallel while performing order-sensitive value movement against the same asset, authority, subject, destination, nonce, allowance, or external system.

### Exploit sketch

Parallelize two state-disjoint components that both transfer from the same vault or emit mint/burn operations affecting one supply invariant. State conflict checks are clean, but effect order changes success, amount, fees, or external state.

### Required repair

- define an explicit effect conflict key containing the policy-relevant authority/subject/asset dimensions;
- add channel/destination outbox footprints;
- default to conflict for two value-moving effects unless the catalog supplies an independently verified commutativity law;
- bind the exact merge order into the candidate and parity evidence.

---

## VM-005 — P1: sequential-parity evidence is self-asserted equality

### Location

`crates/zeno-fcis-compose/src/lib.rs`:

- `CompositionEvidence::sequential_commitment`
- `CompositionEvidence::composed_commitment`
- `verify_deterministic_parallel`.

### Why this is a defect

The verifier accepts parity when two caller-supplied hashes are equal. Neither hash is checked by `EvidenceVerifier`, nor is it bound to the composition spec, input/domain coverage, merge order, execution traces, or result artifact.

### Exploit sketch

Set both commitments to the same arbitrary value. The parity check passes regardless of actual sequential and composed behavior.

### Required repair

Create a complete parity claim value containing at least:

- composition-spec commitment;
- exact input/domain or coverage declaration;
- component and merge-order commitments;
- normative sequential result;
- composed result;
- tool/checker identity and retained artifact.

Require an external verifier to validate that exact claim.

---

## VM-006 — P1: assumption-discharge proof is not bound to its provider set

### Location

`crates/zeno-fcis-compose/src/lib.rs`, `AssumptionDischarge` and `verify_assume_guarantee`.

### Why this is a defect

The verifier checks the artifact against only the assumption claim. The listed provider guarantees are merely checked for existence somewhere in the spec. The artifact is not required to prove that those exact providers discharge the assumption under the exact wiring/spec.

### Exploit sketch

Reuse an artifact produced under one provider set, attach a different set of existing guarantee hashes, and obtain an accepted discharge.

### Required repair

Canonicalize and verify a discharge claim over the component, assumption, sorted provider set, wiring, profile/spec identity, and coverage.

---

## VM-007 — P1: artifact validation does not require the expected invocation

### Location

`crates/zeno-fcis-transition/src/lib.rs`:

- `TransitionArtifacts::validate`
- `TransitionReject::validate`.

### Why this is a defect

Validation reads `command_hash` and `context_hash` from the artifact’s own receipt/bundle and feeds those same values back into expected binding reconstruction. It proves internal consistency, not that the artifact matches the command and context the caller intended to authorize.

### Exploit sketch

Substitute a valid artifact for the same pre-state/catalog but a different command, caller, oracle snapshot, nonce, or policy context. A consumer that invokes only `validate` may accept it.

### Required repair

Require a separately admitted immutable `InvocationWitness` containing the expected command/context/profile/domain/replay bindings. Validation must compare against that external witness and consume or otherwise one-shot-bind it at commit.

### Mandatory tests

Swap commands, context values, callers, nonces, and oracle versions between otherwise valid decisions and require deterministic rejection.

---

## VM-008 — P2: transition resource accounting remains caller-asserted

### Location

- `crates/zeno-fcis-core/src/lib.rs`: `Budget`, `BudgetedDecision`
- `crates/zeno-fcis-transition/src/lib.rs`: `CataloguedTransitionBuilder::try_new`, `TransitionResourceReport`.

### Why this blocks the guarantee

The core now has a good non-cloneable execution-local meter, but the catalogued transition builder accepts a raw caller-supplied `BudgetUsed`. The resource report does not include and check the corresponding `BudgetLimits`, and structural validation cannot establish that every operation was charged.

### Required repair

Make the high-level path consume a `BudgetedDecision` or a private metered-execution witness. Derive charges automatically from produced artifacts where possible, retain immutable limits, and verify `used <= limits` for every resource.

---

## VM-009 — P2: approved hash-provider identity can be impersonated

### Location

- `crates/zeno-fcis-codec/src/lib.rs`: public `CommitmentHasher`
- catalog/profile/generator APIs parameterized by arbitrary `H`.

### Why this blocks the guarantee

An external type can claim the same `ALGORITHM_ID` as an approved implementation while returning a constant, nondeterministic, or otherwise incorrect hash. String equality does not establish implementation identity.

### Required repair

Keep the generic trait for research/testing, but require a sealed approved-provider witness at every production authority boundary. Bind exact implementation/toolchain evidence into the deployment profile. Add a negative “lying provider” test against production APIs.

---

## VM-010 — P2: authenticated-state projector and proof context are not fully bound

### Location

`crates/zeno-fcis-authenticated/src/lib.rs`:

- `StateProjector`;
- `AuthenticatedProfile`;
- `SparseProof::verify`.

### Why this blocks the guarantee

The profile and planned commit do not bind the exact projector implementation/definition. `SparseProof::verify` recomputes a root but does not accept or enforce the expected tree identity, profile, version, root, or key as external arguments. The proof value also omits `tree_id`.

### Risk

A proof or projected tree can be replayed or interpreted under the wrong semantic projection/profile if consumers omit manual comparisons.

### Required repair

Bind a nonzero projector-definition commitment into `AuthenticatedProfile`, plans, and proofs. Return a `VerifiedSparseProof` only from `verify_against(expected_profile, expected_version, expected_root, expected_key)`.

---

## VM-011 — P1 assurance gap: strict receipt/bundle/authorized-artifact decoding is missing

Strict canonical patch and plan decoders now exist, but no equivalent complete decoder currently admits untrusted bytes into:

- `Receipt`;
- `RejectReceipt`;
- `CommitBundle`;
- `TransitionArtifacts` or the future production authorization envelope.

This is required both for safe transport admission and for SQLite cross-table validation.

The decoder must be bounded, reconstruct through private smart constructors, require exact re-encoding equality, and perform same-candidate plus catalog/invocation validation.

---

## VM-012 — P2: static assurance and fuzzing do not cover authority topology

### Location

- `tools/check_assurance.py`
- `fuzz/Cargo.toml`
- `fuzz/fuzz_targets/`.

### Gaps

The static checker finds ambient effects and dependency-ring violations, but does not detect:

- production shell functions accepting raw `CommitBundle`;
- direct raw `Effect::new` or `OutboxEntry::new` use in production code;
- unauthorized `CandidateBuilder::seal` call sites;
- public provider implementations impersonating approved identities;
- missing invariant checks or missing command/context comparisons;
- effect/outbox omissions from footprints;
- semantic use of wrapping/saturating arithmetic;
- callbacks or trait implementations that reintroduce authority;
- generated source and build-script escape hatches.

Only codec and candidate-bundle fuzz targets are registered. The candidate target exercises the raw bypass as expected behavior.

### Required repair

Add an authority-topology checker and fuzz targets for:

- patch and plan decoders;
- receipt/bundle/authorized-envelope decoding;
- catalogued transition validation and invocation substitution;
- shell replay/acknowledgement;
- SQLite corruption and row injection;
- composition frames/parity/discharges/effect conflicts;
- authenticated proof context;
- generated Solidity/Solana differential behavior.

---

# Open on-chain stack findings

These findings apply to the open Solidity/Solana generator stack and must be closed before those profiles can inherit any value-movement guarantee.

## VM-013 — P1: candidate identity does not bind the complete effect interpreter/deployment

The shared machine hash describes semantic fields, reasons, events, and capability policy. Chain-specific token addresses, runtime code hashes, program IDs, mints, vaults, token programs, compiler output, and deployment configuration are backend bindings rather than part of the shared machine identity.

A production candidate must additionally bind an exact `EFFECT_INTERPRETER_HASH` and `DEPLOYMENT_BINDING_HASH`. Otherwise the same semantic decision identity may execute different value movement after a proxy upgrade, program upgrade, binding change, or deployment substitution.

## VM-014 — P1: token call success is not exact-value-transfer semantics

### Solidity

`SafeERC20` establishes call compatibility, not exact economic semantics. Fee-on-transfer, rebasing, blacklist, callback, proxy-upgrade, and nonstandard balance behavior can make “transfer amount X” differ from the recipient’s actual balance delta.

Checking a proxy address’s runtime code hash does not bind its implementation, admin, or mutable storage.

### Solana

Exact mint/program/vault checks do not by themselves constrain Token-2022 transfer fees, transfer hooks, confidential transfer behavior, interest-bearing semantics, permanent delegates, or other extensions.

### Required repair

Use explicit token-behavior profiles and exact allowed extension sets. For exact-transfer capabilities, measure and validate pre/post source and destination balance deltas. Reject upgradeable or behavior-changing assets unless implementation/admin/config state is fully bound and governed by reviewed policy.

## VM-015 — P1: fixed-shape plan policy has no valid no-op representation

The current model describes fixed-shape plans as padded, but generated validation requires active counts to equal the maximum and then validates each counted effect as a real nonzero capability. This can force real effects or make padding impossible.

Add an explicit canonical `NoOp` slot that is never interpreted, or separate encoded slot count from active effect count with an authenticated active mask. Test zero-effect, partially filled, and fully filled plans.

## VM-016 — P1 assurance gap: relational accounting invariants are not generated or proved

A capability-bound transfer can still be emitted without a corresponding decrease in the relevant semantic balance unless project logic happens to enforce that relation. Schema validity and token capability bounds do not prove conservation.

The generator needs first-class relational laws such as:

```text
semantic debit == planned transfer + explicit fee
sum of balance deltas == mint - burn
asset identity in state == asset identity in capability
recipient/subject/authority derivations agree
```

These laws should produce property tests, mutation tests, and proof obligations across Rust, Solidity, and Solana renderings.

---

# Second-pass review

The second pass deliberately ignored local constructor correctness and instead followed every route by which authority can enter, persist, be replayed, or be promoted.

It confirmed the first-pass P0 raw-bundle bypass and added four cross-layer conclusions:

1. **The shell is not pinned to the complete authorization environment.** `ShellState` stores state/root/replay/receipts/bundles/outbox but not an exact catalog, provider, state-domain, interpreter, deployment, or invocation policy. SQLite stores a runtime state-domain setting but not the complete profile/catalog/interpreter identity in the database.
2. **Policy commitments identify meaning but do not execute or prove it.** Effect `policy_hash` and reason `predicate_hash` are valuable identities, but a nonzero hash does not establish that generated or handwritten logic implements the identified predicate/policy.
3. **Generated typed APIs reduce accidental shape errors but do not prove business predicates.** A caller still supplies booleans to reason methods and can emit a type-correct effect whose relation to state/command is wrong.
4. **Promotion values are only as strong as the importer/verifier.** Evidence, backend, composition, and security extension traits must be pinned to exact approved implementations and artifacts before their reports can confer production authority.

No second-pass evidence invalidated the core structural strengths. The problem is that those strengths are not yet the sole path to value movement.

---

# Hotspot map for a second audit agent

Audit in this order. Do not begin with style, panics, or micro-optimizations.

## Tier 0 — production authority entry points

Search:

```text
commit(
commit_with_crash_point(
deliver_next(
acknowledge(
CandidateBuilder::seal
Effect::new
OutboxEntry::new
CommitBundle
```

For each call site, answer:

1. What exact type grants authority?
2. Who can construct it?
3. Does validation compare against externally expected catalog, invocation, provider, interpreter, deployment, and replay policy?
4. Can the object validate using only values copied from itself?
5. Can database rows or wire bytes create an equivalent object without the functional core?

Any production path accepting raw values instead of a private authorization witness is a stop-ship finding.

## Tier 1 — value conservation and authority derivation

Inspect every effect/channel definition and every generated `emit_effect_*`, `_effect*`, token/CPI interpreter, and destination adapter.

Check:

- amount sign, width, unit, scale, rounding, fee, and overflow;
- debit/credit conservation;
- mint/burn supply conservation;
- authority and subject provenance;
- asset/mint/token identity agreement;
- exact recipient derivation;
- duplicate effect and ordinal behavior;
- partial-fill and multi-effect interactions;
- token behavior and upgradeability assumptions.

## Tier 2 — context and replay

Trace every fact from capture to candidate commitment:

```text
caller/signer
chain/network/program/contract identity
block/slot/time
tx/replay nonce
oracle value + publication time + source + confidence + finality
configuration/governance version
compiler/interpreter/deployment identity
```

Look for values that are read but not committed, committed but not validated against an external expectation, or validated only against themselves.

## Tier 3 — persistence/refinement

For every stored table/record, derive the invariant from the canonical bundle and verify both directions:

```text
bundle -> every required row exists exactly once
row -> row belongs to exactly one decoded authorized bundle
```

Mutation-test extra, missing, reordered, stale, and cross-candidate records.

## Tier 4 — composition and promotion

For every proof/evidence report:

- compute the exact claim value;
- ensure the artifact is verified against the complete claim, not one field;
- bind provider sets, spec, coverage, source revision, tool identity, assumptions, and deployment;
- reject equality of two caller-supplied hashes as evidence;
- distinguish observed footprint from complete static footprint.

## Tier 5 — decoder/canonicalization attack surface

Require:

- complete-input bounds before allocation;
- nested and aggregate bounds;
- exact reconstruction through smart constructors;
- exact canonical re-encoding equality;
- no duplicate/alternate order;
- no trailing bytes;
- no unknown tags/IDs;
- cross-language negative-vector parity.

---

# High-probability anti-patterns to search

```text
public raw constructor + later optional validation
self-validation using actual values as expected values
nonzero hash treated as authenticated authority
policy hash stored but never enforced or proved
safe high-level API beside a bypassable low-level production API
row-local integrity without aggregate/bundle completeness
contains/overlaps confusion in authorization logic
observed footprint used as a complete footprint
caller-reported resource usage
operation-ID-only effect conflicts
outbox delivery with no decoded bundle membership proof
provider selected by generic type at the final authority boundary
proxy code hash treated as immutable implementation identity
call success treated as exact economic transfer
schema validity treated as invariant preservation
bounded tests described as universal proof
```

---

# Missing adversarial test families

## Construction-bypass tests

- build every authoritative object without using the intended high-level API;
- attempt to cross each shell/adapter boundary;
- compile-fail when a private witness is required.

## Metamorphic binding tests

Change exactly one item and require identity or validation to change:

- command;
- context/caller/oracle/nonce;
- catalog/profile/precedence;
- hash provider;
- state domain;
- effect interpreter/deployment;
- token implementation/configuration;
- budget limits/usage;
- proof assumptions/coverage/provider set.

## Conservation tests

Generate random accepted transitions and assert project-specific conservation laws. Mutate one debit, credit, fee, mint, burn, recipient, authority, or asset field and require rejection.

## Persistence corruption tests

Mutate each table independently, recompute local hashes where possible, reopen the shell, and require fail-closed behavior before delivery.

## Cross-implementation tests

For every promoted transition compare complete:

```text
decision kind/reason
pre/post state and roots
patch
commit and outbox plans
footprint/resources
candidate/receipt/bundle
interpreted effect result
```

Run Rust reference, mounted runtime, Solidity, Solana, and any proof model over the same vectors.

## Rejection-precedence tests

Activate every pair and larger subset of reasons, independent of evaluation order. Verify that rejection cannot retain staged state/effects/outbox and that committed failure retains only its explicitly reviewed transition.

## Boundary tests

For every count, length, depth, amount, time, version, and sequence field: zero, one, maximum, maximum+1, conversion-width boundaries, and overflow.

---

# What prevents the guarantee today

The blocking conditions are:

1. no exclusive nominal production authorization type;
2. raw `CommitBundle` accepted by shells;
3. no exact catalog/invocation/provider/interpreter/deployment pin at the commit port;
4. no complete bundle/authorized-artifact decoder;
5. SQLite row set not cross-validated against decoded bundles;
6. no general mechanized invariant-preservation and conservation proof for project transitions;
7. incomplete effect/outbox composition model;
8. caller-asserted predicates, context provenance, and resource accounting remain in trusted computing base;
9. on-chain interpreter/token semantics are not fully bound;
10. generated compiler tests, chain instruction tests, verified builds, and independent review remain incomplete for the open chain stack.

## Minimum closure sequence

1. Close VM-001 with a private production authorization witness and pinned commit port.
2. Add complete strict artifact decoding and close VM-002.
3. Fix VM-003, VM-004, VM-005, and VM-006 before deterministic-parallel promotion is used.
4. Introduce exact invocation and approved-provider witnesses.
5. Add executable/project-specific invariants and conservation laws.
6. Bind effect interpreters and deployments, then harden token behavior profiles.
7. Run the adversarial matrix and a genuinely independent audit against an exact release candidate.

Only after those steps should the claim be narrowed to a specific release, profile, compiler/toolchain, effect interpreter, deployment configuration, and set of proved invariants.
