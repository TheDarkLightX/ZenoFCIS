# Catalog-derived economic law requirements

## Purpose

A project may not classify an operation as value-moving while marking the
relational laws that govern that value as inapplicable. Catalog format 2 binds
closed economic semantics into every effect and channel definition. The laws
crate derives the minimum non-waivable law families from the exact catalog
before it constructs `VerifiedProjectLaws`.

## Inputs and outputs

Each `EffectDefinition` and `ChannelDefinition` contains one
`OperationSemantics` value:

```text
NonValue(classification commitment)
Value(canonical ValueFlow set, classification commitment)
```

A `ValueFlow` binds an exact asset-domain commitment and one closed kind:

```text
Transfer
Mint
Burn
EscrowLock
EscrowRelease
FeeCharge
Settlement
ExternalValueDelivery
Custom(exact law ID and claim commitment)
```

The output is either a catalog-compatible `VerifiedProjectLaws` or a typed
failure identifying the missing family, insufficient decision scope, missing
custom law, wrong custom claim, or absent independent evidence requirement.

## Authority boundary

`LawManifest` remains inspectable untrusted data. Its caller may describe any
complete required-or-inapplicable policy, but `verify_project_laws` is the only
constructor for `VerifiedProjectLaws`. It derives requirements from the
production-owned `ProjectCatalog` and rejects a weaker manifest before
evidence verification or runtime authorization.

Reclassifying an effect or channel changes its definition commitment, registry
commitment, `ProjectProfile`, `ProjectCatalog`, verified-law-set identity, and
every downstream authorization bound to those identities.

The classification commitment records reviewed evidence. A hash alone does
not prove that a `NonValue` classification is true. Catalog ownership and
independent release review remain trusted authority boundaries.

## Derived laws

Every value flow requires:

- `AssetConservation`;
- `AuthoritySubjectRecipient`;
- the already mandatory committing `StateInvariant`.

Additional requirements are derived as follows:

| Flow | Additional required families |
| --- | --- |
| Transfer, escrow lock/release, external delivery | `DebitCreditEffectEquality` |
| Mint, burn | `MintBurnAuthorization` |
| Fee charge | `DebitCreditEffectEquality`, `FeeAndRounding` |
| Settlement | `MintBurnAuthorization`, `DebitCreditEffectEquality`, `FeeAndRounding` |
| Custom | all closed economic families plus the exact custom law |

Every derived economic family's definitions must collectively cover both
`Accept` and `CommittedFailure`. An `Accept`-only family cannot authorize a
value-bearing committed failure. A custom law must itself bind the
exact claim committed by the flow and require retained independently checked
evidence.

## Deterministic bounds

- At most 64 distinct value flows per effect or channel.
- Flow sets are sorted and duplicate-free.
- Asset domains, classification commitments, and custom claim commitments are
  nonzero.
- Catalog and law verification remain `no_std + alloc`, deterministic, and free
  of ambient I/O, clocks, randomness, threads, and mutable globals.
- Law derivation is linear in the bounded number of definitions and flows.

## Negative cases

The implementation rejects:

- empty, duplicate, or oversized value-flow sets;
- `Custom` without an exact registered law ID and claim;
- zero asset, classification, or claim commitments;
- value-moving catalogs that mark a derived family inapplicable;
- economic definitions that cover only one committing decision;
- missing or mismatched custom laws;
- custom laws that do not require retained independent evidence.

Mutation tests remove conservation, debit/credit, mint/burn, fee/rounding, and
authority families individually. They also narrow every economic family to
`Accept`, exercise value-bearing channels, and mutate custom evidence binding.

## Trusted dependencies and assumptions

No external dependency is added. The design uses canonical encoding and
commitments from `zeno-fcis-codec`, stable registries from `zeno-fcis-project`,
catalog definitions from `zeno-fcis-catalog`, and evidence-checked law sets
from `zeno-fcis-laws`.

The project owner must classify operations honestly, define correct asset
domains, provide sound executable laws, and select an independent evidence
verifier for proof-required claims.

## Explicit nonclaims

- Classification evidence commitments are not self-authenticating proofs.
- The generic library does not infer financial semantics from payload field
  names or source code.
- Derived law families do not prove that a project law engine implements them
  correctly.
- This change does not define how `CommitPlan` effects are executed. That
  separate V1 blocker is specified by issue #76.
- No downstream project or deployment becomes production-qualified solely by
  constructing these values.
