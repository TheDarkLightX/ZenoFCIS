# ZenoFCIS documentation

## Start here

1. [Quickstart](QUICKSTART.md)
2. [Crate map](CRATE_MAP.md)
3. [Feature matrix](FEATURE_MATRIX.md)
4. [Architecture](ARCHITECTURE.md)
5. [LLM usage](LLM_USAGE.md)
6. [Release assurance](RELEASE_ASSURANCE.md)

## Project definition and typed construction

- [Universal project profiles](UNIVERSAL_PROJECT_PROFILES.md)
- [Schema and code-generation boundary](SCHEMA_CODEGEN_BOUNDARY.md)
- [Schema-bound catalog](SCHEMA_BOUND_CATALOG.md)
- [Project bootstrap generator](PROJECT_BOOTSTRAP_GENERATOR.md)
- [Catalogued transition builder](CATALOGUED_TRANSITION_BUILDER.md)
- [Generated catalog transition](GENERATED_CATALOG_TRANSITION.md)
- [Generated typed reasons](GENERATED_TYPED_REASONS.md)
- [Generated typed effects and channels](GENERATED_TYPED_EFFECTS_CHANNELS.md)
- [Generated typed root reads](GENERATED_TYPED_ROOT_READS.md)
- [Generated typed root updates](GENERATED_TYPED_ROOT_UPDATES.md)
- [Generated typed context observations](GENERATED_TYPED_CONTEXT_OBSERVATIONS.md)

## Composition and production authority

- [Formal composition v2](FORMAL_COMPOSITION_V2.md)
- [Fixed-size state domain machines](FIXED_STATE_DOMAIN_MACHINES.md)
- [Composed domain program](COMPOSED_DOMAIN_PROGRAM.md)
- [Composed root projection conformance](COMPOSED_ROOT_PROJECTION_CONFORMANCE.md)
- [Project relational laws](PROJECT_RELATIONAL_LAWS.md)
- [Catalog authorization boundary](CATALOG_AUTHORIZATION_BOUNDARY.md)
- [Candidate and commit boundary](CANDIDATE_COMMIT_BOUNDARY.md)

## Formal tools and runtime refinement

- [Generic backend protocol](GENERIC_BACKEND_PROTOCOL.md)
- [Deterministic synthesis](DETERMINISTIC_SYNTHESIS.md)
- [Evidence importers](EVIDENCE_IMPORTERS.md)
- [Mounted ZenoDEX adapter](MOUNTED_ZENODEX_ADAPTER.md)
- [Mounted ZenoDEX zUSD v1](MOUNTED_ZENODEX_ZUSD_V1.md)

ESSO is a private optional checker. Users who have it can implement the public
backend or evidence traits in a private crate. Lean, SMT/Z3, CVC5, Kani, Flux,
and other tools can use the same public boundaries.

## Persistence, shells, and security

- [SQLite shell refinement](SQLITE_SHELL_REFINEMENT.md)
- [Authenticated-state adapter](AUTHENTICATED_STATE_ADAPTER.md)
- [Authenticated sparse-proof context](AUTHENTICATED_PROOF_CONTEXT.md)
- [Persistent collections](PERSISTENT_COLLECTIONS.md)
- [SHA-256 provider policy](SHA256_PROVIDER_POLICY.md)
- [Secret handling and constant time](SECRET_HANDLING_AND_CONSTANT_TIME.md)
- [Side/covert-channel security](SIDE_CHANNEL_COVERT_CHANNEL_SECURITY.md)

## Canonical admission and decoding

- [Admitted values](ADMITTED_VALUE_WITNESS.md)
- [Admitted envelopes](ADMITTED_ENVELOPE.md)
- [Bounded text](BOUNDED_TEXT_ADMISSION.md)
- [Bounded bytes](BOUNDED_BYTE_ADMISSION.md)
- [Decoder allocation hardening](DECODER_ALLOCATION_HARDENING.md)
- [Strict canonical patch decoder](STRICT_CANONICAL_PATCH_DECODER.md)
- [Strict canonical plan decoders](STRICT_CANONICAL_PLAN_DECODERS.md)
- [Correct-by-construction map entries](CORRECT_BY_CONSTRUCTION_MAP_ENTRIES.md)

## Status

The workspace remains version `0.1.0` and pre-release. The documentation
describes implemented APIs and explicit boundaries. It does not claim a stable
Cargo V1 API, general production qualification, a concurrent runtime, or an
end-to-end proof for arbitrary downstream projects.
