# Law-aware production authority integration

## Purpose

The production authority boundary reviewed in PR #63 closes the raw `CommitBundle` publication path by requiring a nominal catalog-authorized transition. The formal CBC layer adds a stronger nominal witness: `LawVerifiedTransition`.

The production boundary must ultimately require **both** catalog authorization and complete required law verification. It must not authorize a raw `TransitionDecision`, `TransitionArtifacts`, `CommitBundle`, or caller-supplied successful report.

## Required authority object

A law-aware authority instance should own immutable references or owned values for:

```text
ProjectCatalog
AuthorizationPolicy
LawSet
concrete CatalogTransitionProgram
concrete TransitionProvider
concrete LawChecker
concrete LawEvidenceVerifier
concrete EvidenceProvider or closed evidence set
state root domain
invocation-binding policy
```

The caller may supply only the invocation inputs already admitted by the authority API:

```text
principal
replay identity
pre-state
command
context
```

The caller must not choose:

- the catalog;
- the law set;
- the transition build;
- checker or verifier implementations;
- evidence coverage/toolchain identities;
- state/effect/outbox interpreters;
- successful evaluation reports;
- commit bundles.

## Authorization pipeline

```text
owned transition program
    -> TransitionDecision

TransitionDecision
+ owned catalog
+ admitted pre-state
    -> validate_transition_decision
    -> LawSubject

LawSubject
+ owned complete LawSet
+ owned LawChecker
+ owned LawEvidenceVerifier
+ authority-selected LawEvidence
    -> LawVerificationOutcome

Verified(LawVerifiedTransition)
+ principal/replay/invocation policy
    -> CatalogAuthorizedTransition
    -> ProductionCommitPort
```

A rejected law evaluation returns the original decision and full report to the shell or caller as diagnostic evidence, but it creates no production commit authority.

## API direction

The authority layer should evolve from a method shaped like:

```rust
fn authorize(
    &self,
    invocation: Invocation,
) -> Result<CatalogAuthorizedTransition, AuthorizationError>;
```

to an internal pipeline equivalent to:

```rust
let decision = self.program.execute(...)?;
let evidence = self.evidence_provider.evidence_for(...)?;
let verified = verify_transition_laws::<H, _, _>(
    &self.law_set,
    &self.catalog,
    invocation.pre_state(),
    self.state_domain,
    decision,
    &evidence,
    &self.checker,
    &self.verifier,
)?;
let verified = match verified {
    LawVerificationOutcome::Verified(value) => value,
    LawVerificationOutcome::Rejected(failure) => {
        return Err(AuthorizationError::LawViolation(failure));
    }
};
self.authorize_verified(invocation, verified)
```

`authorize_verified` must be private or crate-private. No public constructor may fabricate either nominal witness.

## Binding requirements

The authority must check or inherit all of the following bindings:

```text
law_set.profile_hash == catalog.profile_hash
law_set.schema_hash == catalog.schema_hash
law_set.catalog_hash == catalog commitment
law_set.policy_hash == catalog profile policy hash
law_set.transition_build_hash == program build identity
subject.command/context == admitted invocation command/context
subject.pre_state/root == invocation pre-state/root
subject.candidate == exact transition candidate
report.law_set/subject == exact verified inputs
principal/replay == authorization invocation
```

The final authorized witness should expose the law-set, subject, and report commitments so receipts, audit records, runtime refinement, and release evidence can retain them.

## Evidence-provider requirements

A production authority must not accept arbitrary law evidence directly from an untrusted caller.

Permitted designs include:

1. an immutable evidence registry compiled into or loaded by the authority and bound to exact source/profile/build identities;
2. a checked backend that returns a verified certificate whose claims are adapted into `LawEvidence`;
3. a runtime verifier that independently replays the exact subject before authorization;
4. a hybrid policy with executable checks per invocation and theorem evidence for unbounded laws.

Evidence lookup must be deterministic and fail closed for missing, duplicate, stale, wrong-build, wrong-profile, wrong-coverage, or wrong-toolchain artifacts.

## Law completeness and evolution

The authority should reject construction when:

- the profile has no `Claim` entries;
- the law definitions do not exactly reconstruct all `Claim` entries;
- a successor profile adds/removes/rebinds a law without the required profile-evolution evidence;
- the transition build changes without a new law-set/build binding;
- a required decision class has no applicable law.

Profile migration must carry a separate theorem or executable migration law relating old state, migration command/context, and new state.

## Receipt and audit integration

A later receipt/bundle protocol version should optionally or mandatorily bind:

```text
law_set_hash
law_subject_hash
law_evaluation_report_hash
composition_spec_hash
parallel_parity_claim_hash (when parallel execution was used)
```

Until that protocol change is reviewed, the authority and shell must at least retain those identities in a sidecar audit record atomically associated with the exact candidate and replay binding. The sidecar must not be mistaken for consensus meaning unless its protocol role is explicitly versioned.

## Error model

Recommended authorization errors:

```text
TransitionConstruction
TransitionValidation
LawSetMismatch
LawEvidenceUnavailable
LawViolation(LawEvaluationFailure)
InvocationMismatch
CatalogAuthorization
Publication
```

`LawViolation` is not ordinary business rejection unless the project profile explicitly defines and executes it as such. A failed internal law check generally represents a safety stop, implementation defect, stale evidence, or configuration error and must create no effects.

## Required tests

- raw decisions and bundles cannot reach the commit port;
- law-verified witnesses cannot be constructed outside the CBC crate;
- catalog-authorized witnesses cannot be constructed outside authority;
- changing program build invalidates the law set;
- changing principal/replay/command/context/pre-state invalidates authorization;
- omitting one profile claim prevents authority construction;
- changing a state delta, effect payload, or outbox destination blocks authorization;
- evidence from another profile/build/subject/coverage/toolchain is rejected;
- law violation creates no database/outbox mutation;
- crash/retry cannot reuse a witness for another invocation;
- successful publication atomically retains law audit identities.

## Stacking plan

This integration should be implemented as a small PR after both of these dependencies are reviewed:

1. PR #63 — catalog-authorized production commit witness;
2. PR #66 — formal correctness-by-construction laws.

The integration PR should not duplicate either package. It should add only authority ownership, evidence selection, nominal-witness flow, error propagation, and atomic audit retention.
