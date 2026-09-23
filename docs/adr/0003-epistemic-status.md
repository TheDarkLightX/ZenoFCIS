# 0003: Epistemic status of evidence and witnesses

## Status

Accepted on 2026-09-23. This record defines the vocabulary and classifies the
V1.1 types. Computing levels in code is later work (see Consequences).

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

Identified, Attested, Checked, and Proved are ordered from weakest to strongest.
Three rules apply:

1. **A composite claim has the lowest level among the parts it relies on.**
   Authorization re-executes the program (Checked), but law satisfaction comes
   from the project's law engine (Attested). So "this commit satisfies the
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

| Type | Claim | Level | Why |
| --- | --- | --- | --- |
| `BoundedVec` | Length is within the declared bounds | Checked | Constructors check the length |
| `AdmittedValue`, `AdmittedEnvelope` | The value is canonical and within the default limits | Checked | Construction runs one complete validation |
| `VerifiedProvider` | The hash provider computes the fixed known answers | Checked | The library runs the known-answer suite in this build. It is not binary or hardware attestation. |
| `ProviderParityReport` | Two providers agree on the bytes checked | Checked | The library compares their outputs |
| `ValidatedNormalizedDecision` | The decision matches its receipt or bundle, pre-state root, and bindings | Checked | The library decodes the artifacts and rebuilds the decision |

Authorization:

| Type | Claim | Level | Why |
| --- | --- | --- | --- |
| `CatalogAuthorizedTransition`, `CatalogAuthorizedReject` | The decision is the program's output for this invocation, and bindings and chain are coherent | Checked | The authority re-executes the program and compares |
| | The project laws hold for this invocation | Attested | `LawStatus` values come from the bound `ProjectLawEngine` |
| | The program is the reviewed build | Identified | `CatalogTransitionProgram::transition_build_hash` is reported by the program and only compared with the policy |
| `CatalogAuthorizedGenesis` | The initial state passes schema admission | Checked | Library schema validation |
| | Genesis laws hold | Attested | `ProjectLawEngine::evaluate_genesis` |
| `CatalogAuthorizedAuthenticatedCommit` | The projection relation holds | Attested | `ProjectionRelationEvaluation` comes from a `ProjectionRelationEngine` |
| `TransitionResourceReport`, `MachineExecutionReport` | Usage is within the limits | Attested | Limits are checked against a usage figure the caller supplies |
| `BoundDeliveryInterpreter` | The delivery interpreter is correct | Identified | Any value of the interpreter type can be bound. Its identity is not checked when it is bound. |

Pluggable verifiers and tools:

| Type | Claim | Level | Why |
| --- | --- | --- | --- |
| `LawStatus` | One law's verdict for one invocation | Attested | Reported by the project law engine |
| `VerifiedProjectLaws` | The law manifest matches the catalog's requirements | Checked | Library validation |
| | The law engine is the reviewed build | Identified | `engine_build_hash` is a caller-supplied argument |
| `VerifiedLawEvidence`, `LawProofDecision` | The retained artifact establishes the law's proof subject | Attested | `LawEvidenceVerifier` receives the artifact bytes, but the library does not recheck its answer |
| `VerifiedBackendRun` | The response is well formed for the request and backend identity | Checked | `BackendResponse::validate_for` |
| | The response is correct | Attested | `BackendVerifier` returns a `VerificationDecision` |
| `BackendOutcome`, `VerificationDecision` | The backend's result, and the verifier's decision on it | Attested | Reported by the mounted backend and the pluggable verifier |
| `CompleteFootprintWitness`, `DecisionCoverageStatus` | The static footprint covers every admitted input, including classes claimed unreachable | Attested | `FootprintEvidenceVerifier::verify` returns a Boolean for a claim and an artifact hash |
| `ParallelParityEvidence` | Parallel and sequential execution agree | Attested | The caller supplies the compared hashes |
| `EvidenceResult` | A tool's outcome | Attested | Recorded as the tool reported it |
| `ValidatedRefinementCase`, `RefinementReport` | Model and runtime agree on this case | Checked | The library compares the decisions. It says nothing about other inputs or about the model. |
| `ValidatedCoverage` | Coverage of the input domain | Attested | Exhaustive and proof-assisted coverage rest on `ToolEvidence`. Bounded coverage samples the domain. |

Formal results:

| Type | Claim | Level | Why |
| --- | --- | --- | --- |
| `ToolRunStatus::ProposedUnsat` | The exported obligation is valid | Attested | cvc5's answer and proof text are not checked |
| `ToolRunStatus::KernelChecked` | The exported obligation is valid | Proved | The Lean kernel checks the proof. The scope is `without-system-model`. |
| `ToolRunStatus::Refuted` | The claim is false at this observation assignment | Checked | The model is replayed through the evaluator. It may be unreachable in the system. |
| `finite::Outcome::Selected` | The program satisfies the contract on every admitted input | Proved | Exhaustive check. Trusted base: the `finite-i64/1` interpreter. |
| `finite::Outcome::NoSolution` | No program in the grammar satisfies the contract | Proved | Every candidate is refuted |
| `finite::Outcome::Unrealizable`, `finite::Witness` | This input has no admitted output, or this candidate fails here | Checked | Concrete witness |
| `VerifiedCompletion` | Every state has a strictly decreasing path to a terminal state | Proved | `verify_completion` checks every row and command. Trusted base: the interpreter. |

Types added on this branch:

| Type | Claim | Level | Why |
| --- | --- | --- | --- |
| `Substance` | The formula's value cannot depend on a transition | Checked | Syntactic classification. It is diagnostic only. |
| `SystemCheck::SystemProperty`, `SystemCheck::DomainImplied` | The property holds on every admitted input, and it does or does not depend on the transition | Proved | Exhaustive enumeration. Trusted base: the interpreter. |
| `SystemCheck::NotTotal`, `Violated`, `Undefined` | Failure at this input | Checked | The interpreter produced the witness |
| `SystemVerdict::SystemProperty`, `SystemVerdict::DomainImplied` | As for `SystemCheck` | Attested | They rest on solver `unsat` answers that nothing rechecks. The pinned differential tests compare them with the exhaustive route for the shipped programs. |
| `SystemVerdict::NotTotal`, `Violated`, `Undefined` | Failure at this input | Checked | The interpreter reproduces the solver's model before the verdict is returned |

`SystemCheck` and `SystemVerdict` share verdict codes but not levels. This is
intended: the level follows the route, not the code.

Accepted has no V1 representation. Accepted risks live in prose, in each
package's "Assumptions" and "Explicit nonclaims" sections.

## Consequences

- New status types state the level of each claim they support in their
  documentation. They use `Verified`, `Proved`, or `Proven` in a name only for
  Checked or Proved claims.
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
- `verify_completion` checks every state row.
- `check_system_property` enumerates every admitted input.
