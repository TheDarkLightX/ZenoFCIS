# Validated Refinement and Exhaustive Coverage

## Scope

This boundary closes issue #61 for the V1 core release candidate. It separates
untrusted runtime transport from production promotion evidence.

## Inputs

A validated decision consumes:

- one structurally normalized but untrusted runtime decision;
- the exact expected candidate bindings, including profile, command, context,
  precedence, algorithm, and deterministic-budget commitments;
- the exact canonical pre-state and state-root domain;
- a nominal approved commitment-provider witness;
- explicit receipt and bundle decode limits.

An exhaustive promotion consumes:

- independently validated model and runtime decisions for every case;
- a canonical finite-domain manifest;
- a pinned domain definition, enumeration algorithm, and toolchain;
- the exact canonical set of input commitments;
- an independent coverage artifact for the complete manifest and case set;
- a promotion policy, source revision, importer identity, and identified
  verifier.

## Outputs

Successful reconstruction creates a privately constructible
`ValidatedNormalizedDecision`. Successful case construction derives its input
and case identities from the validated invocation and complete decisions.
Promotion produces a content-addressed report binding the policy, evidence,
source revision, importer, verifier, and every blocker.

## Authority boundary

`DecisionArtifacts` and `NormalizedDecision` remain untrusted transport and
diagnostic values. Equality between two such values is useful differential
information but grants no promotion or commit authority.

Only strict receipt or bundle decoding followed by exact re-normalization can
create `ValidatedNormalizedDecision`. Only validated decisions enter the new
promotion path. The external verifier decides whether a retained formal or
enumeration artifact establishes its exact claim. The verifier does not choose
the domain, policy, source revision, importer, or cases.

The legacy `evaluate_promotion` path now always reports unvalidated decision
artifacts and cannot promote bounded, proof-assisted, or cardinality-only
exhaustive evidence. It remains available for compatibility and diagnostics.

## Trusted dependencies

- `zeno-fcis-receipt` supplies strict receipt and complete bundle decoders.
- `zeno-fcis-patch` supplies the canonical pre-state root calculation.
- `zeno-fcis-crypto` supplies sealed approved-provider identities and
  known-answer-tested provider witnesses.
- `zeno-fcis-codec` supplies canonical encodings and domain-separated
  commitments.

No solver, theorem prover, ESSO installation, runtime, filesystem, network, or
database is embedded in the semantic crate.

## Deterministic resource bounds

- At most 1,000,000 retained refinement cases or manifest inputs.
- At most 64 tool-evidence records.
- At most 64 MiB per normalized component and explicit nested receipt, patch,
  plan, and bundle decoder limits.
- Domain names use the codec's bounded ASCII `Domain` contract.
- Manifest inputs must be strictly increasing and duplicate-free.

The total work of exhaustive validation is linear in retained cases plus
manifest inputs, with bounded per-artifact decoding.

## Laws

1. A validated rejection reconstructs from the exact canonical rejection
   receipt, has the expected bindings, and is rooted in the expected pre-state.
2. A validated committed result reconstructs from the exact complete bundle,
   including patch, plans, receipt, candidate, roots, and same-candidate laws.
3. Re-normalizing the reconstructed artifact equals the submitted transport
   value byte for byte.
4. Model and runtime cases must share one exact validation binding.
5. Case and input identities are derived, never caller-selected.
6. An exhaustive manifest is canonical, duplicate-free, content-addressed, and
   bound to its profile, domain definition, enumeration algorithm, toolchain,
   and exact inputs.
7. Exhaustive promotion requires exact manifest/case set equality and an
   independently verified exact coverage claim.
8. Empty exhaustive domains require a nonzero, profile-bound empty-domain
   declaration and independent coverage verification.
9. Promotion report identity changes when the policy, evidence, source,
   importer, verifier, approved provider, cases, tools, coverage, or blockers
   change.

## Negative cases

The permanent test surface rejects:

- fabricated but mutually equal model/runtime artifacts;
- substitutions of candidate, roots, patch, plans, receipt, or bundle;
- wrong command, context, profile, precedence, algorithm, budget, domain, or
  provider binding;
- duplicate case inputs under different case identifiers;
- arbitrary cardinality-only exhaustive claims;
- missing, duplicate, noncanonical, or extra manifest members;
- unverified enumeration evidence;
- empty domains without an explicit verified declaration;
- verifier-identity, source-revision, and importer substitutions.

## Assumptions

The approved provider implementation, strict decoders, canonical codec,
external proof verifier, enumeration toolchain, and supplied exact pre-state
are trusted according to their separately stated boundaries. The manifest
definition remains a reviewed project protocol choice.

## Explicit nonclaims

- This boundary does not prove a runtime transition correct merely because one
  or more cases match.
- It does not prove that an external solver, theorem prover, ESSO checker, or
  importer is sound; exact independently verified evidence is still required.
- It does not infer a finite domain from source code.
- It does not claim unbounded coverage from a bounded manifest.
- It does not authorize state publication or external effects.
- It does not replace project invariants, conservation laws, catalog
  authorization, or independent release audit.
