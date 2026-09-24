# 0003: Epistemic status of evidence and witnesses

## Status

Accepted on 2026-09-23. This record defines the vocabulary and classifies the
V1.1 types. Computing levels in code is later work (see Consequences).

Amended on 2026-09-23 after an independent review of commit `521b768`. The
changes:

- every classification names its scope, assumptions, and trusted base;
- the composite rule states when it applies;
- `ValidatedCoverage` is classified per variant;
- `SystemVerdict` witnesses count as Checked only because models are now
  checked against their declared domains before replay.

Corrected on 2026-09-23: the authorization row said the authority re-executes
the program and compares. It runs the program itself; only re-authorization of
a persisted transition re-executes it and compares the bytes.

## Context

V1 names its results after who produced them or after the tool's own word:
`Verified*`, `Validated*`, `EvidenceResult::Proven`, `KernelChecked`. These
names do not say what was established. Three examples:

- `VerifiedLawEvidence` holds whatever a pluggable `LawEvidenceVerifier`
  answered.
- `EvidenceResult::Proven` is a tool's report, recorded as reported.
- `KernelChecked` is a Lean-checked proof of an obligation that contains no
  system model (see [law and claim substance](../CLAIM_SUBSTANCE.md)).

A reader cannot tell from a type name whether library code computed a result,
recomputed a component's answer, or only recorded it.

## Decision

Every claim a status supports has exactly one of these levels. A level belongs
to a claim, not to a type. It is derived from how the evidence was produced,
never from a name.

| Level | Meaning |
| --- | --- |
| Identified | A hash or name says which artifact is meant. Nothing checks that the running artifact is that one, or what it does. |
| Attested | A component outside library control reported the result. Library code checked only its shape, binding, and identity, and did not recompute it. |
| Checked | Library code computed the result itself, on the inputs it examined. For a claim about those inputs, this settles it. For a claim about all inputs, it is a sample. |
| Proved | The claim holds for every input in its stated scope. A small, named trusted base re-checks the argument: exhaustive enumeration by a library interpreter, or a proof checked by a proof checker. A solver's bare `unsat` is not enough. |
| Accepted | A named owner accepts a residual risk that no evidence covers. It is a decision, not evidence, and it is outside the order of the other levels. |

A level alone is not a classification. Each one also states:

- **scope**: the statement and the inputs it covers;
- **assumptions**: what must be true for the result to mean what it says;
- **trusted base**: the code, tools, and components a reader must trust.

Identified, Attested, Checked, and Proved are ordered from weakest to strongest.
Three rules apply:

1. **A composite claim is at most as strong as its weakest part, and only
   when the parts fit together.** The parts must cover the same scope, their
   assumptions must be discharged or carried forward, and the step that joins
   them must itself be established. Otherwise the composite has no level until
   that step is checked. For example, the authority runs the program itself
   (Checked), while law satisfaction comes from the project's law engine
   (Attested). Both concern the same invocation, so "this commit satisfies the
   project laws" is Attested.
2. **Scope is part of the claim.** A Proved result about a model-free
   obligation holds for every system alike. It establishes its statement, but
   it is not evidence about any one system. Checks that require system
   evidence treat such results as failures, as `--require-substantive` does for
   vacuous laws and claims.
3. **An empty blocker list inherits its parts' levels.**
   `CompositionReport`, `PromotionReport`, `ValidatedPromotionReport`,
   `SecurityPromotionReport`, and `CompatibilityReport` pass when no blocker is
   present. A pass means each required part is present at its own level, not
   that each part is Proved.

### Classification of V1.1 types

Admission and integrity:

| Type | Claim and scope | Level | Trusted base and assumptions |
| --- | --- | --- | --- |
| `BoundedVec` | This vector's length lies within the declared bounds | Checked | The constructors check the length. |
| `AdmittedValue`, `AdmittedEnvelope` | This value is canonical and within the default limits | Checked | Construction runs one complete validation in the value and codec crates. |
| `VerifiedProvider` | This hash provider computes the fixed known answers in this build | Checked | Scope: the fixed vectors, in the current process. It is not binary or hardware attestation. |
| `ProviderParityReport` | Two providers agree on the bytes checked | Checked | Scope: those bytes only. |
| `ValidatedNormalizedDecision` | This decision matches its receipt or bundle, pre-state root, and bindings | Checked | The library decodes the artifacts and rebuilds the decision. It says nothing about whether the decision is right. |

Authorization:

| Type | Claim and scope | Level | Trusted base and assumptions |
| --- | --- | --- | --- |
| `CatalogAuthorizedTransition`, `CatalogAuthorizedReject` | The decision is the program's output for this invocation, and its bindings and chain are coherent | Checked | The authority runs the reviewed program itself, so no caller supplies the decision. Persisted transitions are re-executed and compared byte for byte when they are re-authorized. Trusted: the authority crate and the program's own code. Assumes the program is deterministic. |
| | The project laws hold for this invocation | Attested | `LawStatus` values come from the bound `ProjectLawEngine`, which is trusted. |
| | The program is the reviewed build | Identified | `CatalogTransitionProgram::transition_build_hash` is reported by the program and only compared with the policy. |
| `CatalogAuthorizedGenesis` | The initial state passes schema admission | Checked | Library schema validation. |
| | Genesis laws hold | Attested | `ProjectLawEngine::evaluate_genesis`, which is trusted. |
| `CatalogAuthorizedAuthenticatedCommit` | The projection relation holds for this commit | Attested | A `ProjectionRelationEngine` reports the `ProjectionRelationEvaluation`. |
| `TransitionResourceReport`, `MachineExecutionReport` | Usage is within the limits | Attested | Limits are checked against a usage figure that the caller supplies. |
| `BoundDeliveryInterpreter` | The delivery interpreter is correct | Identified | Any value of the interpreter type can be bound. Its identity is not checked when it is bound. |

Pluggable verifiers and tools:

| Type | Claim and scope | Level | Trusted base and assumptions |
| --- | --- | --- | --- |
| `LawStatus` | One law's verdict for one invocation | Attested | Reported by the project law engine. |
| `VerifiedProjectLaws` | The law manifest matches the catalog's requirements | Checked | Library validation. |
| | The law engine is the reviewed build | Identified | `engine_build_hash` is a caller-supplied argument. |
| `VerifiedLawEvidence`, `LawProofDecision` | The retained artifact establishes the law's proof subject | Attested | `LawEvidenceVerifier` receives the artifact bytes, but the library does not recheck its answer. |
| `VerifiedBackendRun` | The response is well formed for the request and backend identity | Checked | `BackendResponse::validate_for`. |
| | The response is correct | Attested | `BackendVerifier` returns a `VerificationDecision`. |
| `BackendOutcome`, `VerificationDecision` | The backend's result, and the verifier's decision on it | Attested | Reported by the mounted backend and the pluggable verifier. |
| `CompleteFootprintWitness`, `DecisionCoverageStatus` | The static footprint covers every admitted input, including classes claimed unreachable | Attested | `FootprintEvidenceVerifier::verify` returns a Boolean for a claim and an artifact hash. |
| `ParallelParityEvidence` | Parallel and sequential execution agree | Attested | The caller supplies the compared hashes. |
| `EvidenceResult` | A tool's outcome | Attested | Recorded as the tool reported it. |
| `ValidatedRefinementCase`, `RefinementReport` | Model and runtime agree on this case | Checked | Scope: this case. It says nothing about other inputs or about the model's correctness. |
| `ValidatedCoverage::Exhaustive` | Every manifest input was replayed exactly once | Checked | The strict promotion path compares the cases with the manifest. |
| | The manifest enumerates the whole input domain | Attested | Rests on the variant's `ToolEvidence`. |
| `ValidatedCoverage::Bounded` | The nonempty case set stays within its budget | Checked | It makes no completeness claim. Each case is Checked on its own. |
| `ValidatedCoverage::ProofAssisted` | A theorem covers the input domain | Attested | Rests on the theorem's tool evidence. |

Formal results:

| Type | Claim and scope | Level | Trusted base and assumptions |
| --- | --- | --- | --- |
| `ToolRunStatus::ProposedUnsat` | The exported SMT obligation is valid | Attested | cvc5's `unsat` answer. Its proof text is not checked. |
| `ToolRunStatus::KernelChecked` | The exported Lean theorem holds | Proved | Scope: the theorem as emitted, over observation assignments without a system model. Trusted: the pinned Lean kernel and runtime, the axioms the check allows, and the translation from `.zeno` to Lean. |
| `ToolRunStatus::Refuted` | The claim is false at this observation assignment | Checked | The model is replayed through the evaluator. The assignment may be unreachable in the system. |
| `finite::Outcome::Selected` | The program satisfies the contract on every admitted input | Proved | Exhaustive check. Trusted: the `finite-i64/1` interpreter and enumeration. |
| `finite::Outcome::NoSolution` | No program in the grammar satisfies the contract | Proved | Every candidate is refuted. Same trusted base. |
| `finite::Outcome::Unrealizable`, `finite::Witness` | This input has no admitted output, or this candidate fails here | Checked | Concrete witness. |
| `VerifiedCompletion` | Every state has a strictly decreasing path to a terminal state | Proved | `verify_completion` checks every row and command. Trusted: the interpreter. |

Results added on this branch:

| Result | Claim and scope | Level | Trusted base and assumptions |
| --- | --- | --- | --- |
| `Substance` | The formula's value cannot depend on a transition | Checked | Syntactic and diagnostic only. Assumes every observation is present and evaluation stays within its limits. |
| `PathResolution` | The path names declared types and fields | Checked | Names only, not how a law engine binds values to paths. |
| `SystemCheck::SystemProperty`, `SystemCheck::DomainImplied` | The property holds on every admitted input of the finite program, and does or does not depend on the transition | Proved | Scope: the finite program and its declared domains. Trusted: the interpreter and enumeration. Assumes the program's inputs model the executed invocation; the Rust adapter is outside this scope. |
| `SystemCheck::NotTotal`, `Violated`, `Undefined` | Failure at this input | Checked | The interpreter produced the witness. |
| `SystemVerdict::SystemProperty`, `SystemVerdict::DomainImplied` | As for `SystemCheck` | Attested | Totality, the property, and `domain-implied` rest on solver `unsat` answers that nothing rechecks. The domain-only witness behind `system-property` is replayed. The pinned differential tests compare the routes over stated collections. |
| `SystemVerdict::NotTotal`, `Violated`, `Undefined` | Failure at this input | Checked | The model's values are checked against the declared domains, then the interpreter reproduces the failure. Before this check, an input outside the domain was reported as a totality failure. |
| Exhaustive kernel law harness | The law holds on every input of its stated domain | Proved | Trusted: the Rust compiler, the kernel crate, and the harness predicate as a statement of the law. |
| Random kernel law harness | The law holds on the sampled inputs | Checked | A sample; each run draws fresh inputs. |
| `DeterminismProbe` | Repeated executions of one invocation produced identical canonical decision bytes | Checked | Scope: these executions, in one process. It says nothing about other runs, environments, inputs, or machines. Trusted: the authority crate. |
| `zeno-fcis purity` result `clean` or `confined` | Every file in scope was read and no error-level rule in the table matched; warnings are reported but leave the result clean. `confined` adds a library-only package rooted at `src/lib.rs` with no binary target, unconditional `no_std`, `forbid(unsafe_code)`, no `extern crate std`, no `include` or `#[path]`, and a completely read manifest whose dependencies are all named as semantic crates | Checked | Scope: the listed files and the rule table. Anything unread makes the result `unreadable`. Macros are scanned, not expanded; dependencies and unlisted files are not read; dependencies are identified by name, not by source. It is not a proof of determinism. |

`SystemCheck` and `SystemVerdict` share verdict codes but not levels. This is
intended: the level follows the route, not the code.

Accepted has no V1 representation. Accepted risks live in prose, in each
package's "Assumptions" and "Explicit nonclaims" sections.

## Consequences

- New status types state the level, scope, assumptions, and trusted base of
  each claim they support in their documentation. They use `Verified`,
  `Proved`, or `Proven` in a name only for Checked or Proved claims.
- The V1 names stay for compatibility. Renaming is recorded in the
  [V2 ledger](0004-v2-ledger.md).
- A later assurance-case package computes levels in code from evidence kind and
  records Accepted items with a named owner and an expiry.
- Moving a claim up one level is a concrete work item:
  - law verdicts from Attested to Checked, through a library-interpreted law
    engine;
  - `ProposedUnsat` from Attested to Proved, through checked Alethe proofs;
  - program identity from Identified to Checked, through a library-computed
    program identity.

## Evidence

The classifications follow these constructors and signatures:

- `verify_project_laws` takes `engine_build_hash` as an argument.
- `LawEvidenceVerifier::verify` returns a `LawProofDecision`.
- `verify_backend_response` accepts `VerificationDecision::Attested`.
- `verify_complete_footprint` accepts `FootprintEvidenceVerifier::verify`.
- `bind_delivery_interpreter` binds any value of the interpreter type.
- `replay_model` replays solver models.
- `ValidatedCoverage::Bounded` is documented as "a nonempty bounded case set
  without a completeness claim".
- `verify_completion` checks every state row.
- `check_system_property` enumerates every admitted input.
- `system_verdict` checks every model against the declared domains before
  replaying it.
