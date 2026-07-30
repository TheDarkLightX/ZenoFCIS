# ZenoFCIS documentation

## Start here

1. [Installation](INSTALLATION.md)
2. [Quickstart](QUICKSTART.md)
3. [API reference](API_REFERENCE.md)
4. [Crate map](CRATE_MAP.md)
5. [Feature matrix](FEATURE_MATRIX.md)
6. [Architecture](ARCHITECTURE.md)
7. [LLM usage](LLM_USAGE.md)
8. [V1 product contract](V1_PRODUCT_CONTRACT.md)
9. [BDD and acceptance testing](ACCEPTANCE_TESTING.md)
10. [Deterministic developer guardrails](DEVELOPER_GUARDRAILS.md)
11. [RC3 release notes](RC3_RELEASE_NOTES.md)
12. [RC3 authoring contract](RC3_AUTHORING_CONTRACT.md)
13. [`.zeno` language v1](ZENO_LANGUAGE_V1.md)
14. [Temporal logic v1](TEMPORAL_LOGIC_V1.md)
15. [Formal tools](FORMAL_TOOLS_RC3.md)
16. [Mini Determinator](MINI_DETERMINATOR.md)
17. [Mini Determinator QEMU kernel demo](QEMU_MINI_DETERMINATOR.md)
18. [CLI reference](CLI_REFERENCE.md)
19. [CLI and QEMU marketing captures](assets/marketing/README.md)
20. [Historical RC2 release notes](RC2_RELEASE_NOTES.md)
21. [Historical RC1 release notes](RC1_RELEASE_NOTES.md)
22. [V1 release checklist](V1_RELEASE_CHECKLIST.md)
23. [Packaging](PACKAGING.md)
24. [Release assurance](RELEASE_ASSURANCE.md)
25. [RC3 readiness review](RC3_READINESS_REVIEW.md)

## Project definition and typed construction

- [Language tutorial](tutorials/LANGUAGE.md)
- [Composition tutorial](tutorials/COMPOSITION.md)
- [CLI tutorial](tutorials/CLI.md)
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
- [Complete static footprint witnesses](COMPLETE_FOOTPRINT_WITNESS.md)
- [Fixed-size state domain machines](FIXED_STATE_DOMAIN_MACHINES.md)
- [Composed domain program](COMPOSED_DOMAIN_PROGRAM.md)
- [Composed root projection conformance](COMPOSED_ROOT_PROJECTION_CONFORMANCE.md)
- [Project relational laws](PROJECT_RELATIONAL_LAWS.md)
- [Catalog authorization boundary](CATALOG_AUTHORIZATION_BOUNDARY.md)
- [Policy-bound genesis authorization](GENESIS_AUTHORIZATION.md)
- [Candidate and commit boundary](CANDIDATE_COMMIT_BOUNDARY.md)

## Formal tools and runtime refinement

- [Temporal tutorial](tutorials/TEMPORAL.md)
- [Formal-tools tutorial](tutorials/FORMAL_TOOLS.md)
- [Mini Determinator tutorial](tutorials/MINI_DETERMINATOR.md)
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
- [Outbox delivery identity](OUTBOX_DELIVERY_IDENTITY.md)
- [Authenticated-state adapter](AUTHENTICATED_STATE_ADAPTER.md)
- [Authenticated sparse-proof context](AUTHENTICATED_PROOF_CONTEXT.md)
- [Candidate-bound authenticated authority](AUTHENTICATED_AUTHORITY_BOUNDARY.md)
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

The workspace is version `1.0.0-rc.3`, the current public API and packaging
candidate. The documentation describes implemented APIs and explicit
boundaries. Final Cargo V1 stability begins only at `1.0.0`. The RC does not
claim general deployment qualification, a bundled concurrent runtime, or an
end-to-end proof for arbitrary downstream projects.
