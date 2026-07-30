# API reference

## Hosted reference

After publication, rustdoc for the umbrella crate is available at:

```text
https://docs.rs/zeno-fcis/1.0.0-rc.3/zeno_fcis/
```

Every public subcrate is published at the same exact version and receives its
own docs.rs reference.

## Local reference

```bash
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc \
  --workspace --all-features --locked --no-deps --open
```

The RC bundle also contains a static rustdoc archive generated from the exact
release commit.

## Recommended entry points

| Goal | Entry point |
|---|---|
| Basic decision and budget algebra | `zeno_fcis::core` |
| Canonical admitted values | `zeno_fcis::value`, `zeno_fcis::codec` |
| `.zeno` parsing and typed authoring | `zeno_fcis::spec::{parse_project, elaborate_project, ProjectSpecBuilder}` with feature `authoring` |
| Bounded relational and temporal evaluation | `zeno_fcis::spec::{evaluate_relational, evaluate_temporal}` |
| Mini Determinator semantic reference | `zeno_fcis::spec::{MiniDeterminator, WorkerProgram, WorkerInstruction}` |
| Project schema and policy | `zeno_fcis::project`, `schema`, `catalog` |
| Pure transition construction | `zeno_fcis::transition` |
| Project invariants and conservation | `zeno_fcis::laws` |
| Nominal genesis and commit authorization | `zeno_fcis::authority` |
| Strict authenticated proof/plan decoding | `zeno_fcis::authenticated::{AuthenticatedDecodeLimits, decode_sparse_proof, decode_authenticated_plan}` |
| Qualified candidate-bound authenticated publication | `zeno_fcis::authenticated_authority::{AuthenticatedCommitAuthority, CatalogAuthorizedAuthenticatedCommit, ProductionAuthenticatedCommitPort}` |
| Strict receipt and bundle decoding | `zeno_fcis::receipt::{ReceiptDecodeLimits, BundleDecodeLimits, decode_receipt, decode_reject_receipt, decode_commit_bundle}` |
| Persisted authorization re-entry | `zeno_fcis::authority::{AuthorizationDecodeLimits, CatalogCommitAuthority::reauthorize_canonical_transition}` |
| Fixed domain machines | `zeno_fcis::domain` |
| Global composed program | `zeno_fcis::composed_program` |
| Composition proof obligations | `zeno_fcis::compose` |
| Formal tool protocol | `zeno_fcis::backend`, `evidence`, `refine` |
| CVC5, Z3, and Lean process adapters | package `zeno-fcis-formal-tools` |
| Deterministic authoring CLI | package `zeno-fcis-cli`, binary `zeno-fcis` |
| Strict runtime decision reconstruction | `zeno_fcis::refine::{ValidatedNormalizedDecision, DecisionValidationLimits}` |
| Verified finite-domain promotion | `zeno_fcis::refine::{ExhaustiveDomainManifest, ValidatedRefinementCase, ValidatedPromotionEvidence, evaluate_validated_promotion}` |
| Reference and concrete shells | `zeno_fcis::shell`, `zeno-fcis-shell-sqlite` |

Prefer the [quickstart](QUICKSTART.md) for the first implementation, then use
the [crate map](CRATE_MAP.md) and generated rustdoc for exact signatures.
The [canonical-bytes guide](CANONICAL_BYTES.md) explains ZCVE/1 admission,
decode/re-encode enforcement, commitments, and the boundary between byte
identity and semantic authority.
The [V1 product contract](V1_PRODUCT_CONTRACT.md) identifies the supported
adopter journeys, and the [acceptance guide](ACCEPTANCE_TESTING.md) maps each
journey to fixed executable commands.
The [genesis authorization guide](GENESIS_AUTHORIZATION.md) documents the
required one-time initial-state ceremony and SQLite reopen contract. The
[strict artifact and SQLite history guide](STRICT_ARTIFACT_AND_SQLITE_HISTORY.md)
documents persisted-artifact reauthorization and complete row-set validation.
The [validated refinement guide](VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md)
documents the separation between untrusted mounted transport, strict artifact
reconstruction, canonical domain manifests, and independently verified
promotion evidence.
The [authenticated authority guide](AUTHENTICATED_AUTHORITY_BOUNDARY.md)
documents retained projector qualification, per-transition projection laws,
strict plan reauthorization, and nominal authenticated publication.

## Stability

`1.0.0-rc.3` freezes a candidate Rust API for review. Corrections may change
that API in a later release candidate. Stable protocol identifiers remain
independent of Cargo versions and may not be silently reinterpreted.
