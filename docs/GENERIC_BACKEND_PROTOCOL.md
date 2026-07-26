# Generic checked backend protocol

ZenoFCIS does not grant authority to a particular synthesis engine, theorem prover, compiler, optimizer, LLM, ESSO installation, Morph installation, or private service. `zeno-fcis-backend` defines one project-neutral exchange through which any such engine may propose closed artifacts for independent validation.

## Authority boundary

```text
reviewed ProjectProfile
+ reviewed specification and authenticated context
+ explicit logical resource limits
    -> BackendRequest
    -> mounted BackendEngine proposal
    -> BackendResponse bound to exact engine identity and usage
    -> independent BackendVerifier
    -> BackendCertificate
    -> project-specific promotion or synthesis gate
```

The engine may not choose:

- project schemas or stable semantic identifiers;
- the profile, specification, context, or resource limits bound by the request;
- whether its response is independently attested;
- theorem, refinement, composition, or promotion truth;
- protocol migration or release status.

## Backend identity

Every mounted implementation declares:

- normalized backend family and version names;
- backend protocol version;
- binary, source, and configuration commitments;
- a canonical closed set of advertised operations.

The complete identity is content-addressed. The same family name with different source, binary, configuration, capabilities, or protocol version is a different backend identity.

## Operations

The initial closed operation registry includes:

- synthesis;
- proof or claim verification;
- implementation/runtime refinement;
- assume-guarantee composition;
- semantics-preserving transformation;
- constrained optimization;
- counterexample minimization;
- bounded design generation.

A backend response is rejected if the engine did not advertise the requested operation.

## Resource model

Requests authorize logical fuel, candidate count, output bytes, and trace entries. Responses report exact usage and fail closed when usage or canonical output exceeds the request. Wall-clock deadlines, host memory exhaustion, process crashes, and service unavailability remain shell failures and never become evidence that a semantic search was complete.

## Outcome algebra

```text
Accepted
    closed artifact
    independent reference/refinement claim
    composition/contract claim
    optional additional claims
    exact trace commitment

Rejected
    normalized counterexample
    exact trace commitment

Incomplete
    retained search/proof frontier
    exact trace commitment

Indeterminate
    stable non-authoritative failure classification
    exact trace commitment
```

`Incomplete` and `Indeterminate` grant no proof or promotion authority. They are distinct so resource-bounded incompleteness is not confused with crash, timeout, disagreement, or protocol failure.

## Independent verification

A `BackendVerifier` is separate from the engine. It receives the exact request and complete response and returns one of:

- an attestation claim;
- a content-bound refutation;
- indeterminate.

Only a nonzero attestation creates a `BackendCertificate`. The certificate binds request, response, backend implementation, independent verifier, and verification claim.

## ESSO and Morph

Private ESSO and Morph deployments should implement `BackendEngine` in their private repositories or service adapters. Their private types, prompts, solvers, proof state, and internal representations must not cross the protocol boundary. They translate from the closed request into private execution and return only a bounded `BackendResponse`.

Recommended division:

```text
ESSO
    candidate-domain construction and deterministic synthesis proposals

Morph
    semantics-preserving reformulation, transformation, and equivalence proposals

independent project validators
    reference execution, SMT/proof checking, composition, runtime refinement,
    canonical encoding, and promotion decisions
```

The public ZenoFCIS crate contains no private source and does not assume either engine exists.

## Deterministic synthesis integration

`SynthesisBackendChecker` adapts a verified generic backend to the existing `CandidateChecker` interface. Canonical assignment ordering and search completeness remain owned by `zeno-fcis-synthesis`. The mounted backend checks one assignment at a time; it cannot reorder, omit, or terminate the outer complete-within-bounds search.

The backend and synthesis crates support `no_std + alloc`. Concrete engines normally live in `std` shell crates because they invoke processes, services, solvers, compilers, or models.

## Project use

- **ZenoDEX:** check generated transition leaves, arithmetic candidates, refinements, and composition obligations.
- **ZenoStorage:** check provider-selection, repair, settlement, evidence, and agreement-state proposals.
- **ZenoMail:** check delivery, key-epoch, mailbox, trust, and policy transitions.
- **PopperPad:** check claim relations, evidence admission, recipe results, and theorem/reproduction proposals.
- **Helix:** check ranking, policy, automation, reasoning, and on-chain action proposals.
- **LucyOS:** check capability derivations, system descriptions, scheduling policies, machine plans, and implementation refinements.

## Nonclaims

A valid backend certificate establishes that one pinned verifier attested one exact response under one exact request. It does not make an incorrect specification true, prove a grammar complete outside its declared bounds, establish compiler correctness, or authorize production deployment by itself.
