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
11. [Release candidates](RELEASE_CANDIDATES.md)
12. [Historical V1.1 release notes](V1_1_RELEASE_NOTES.md)
13. [Historical V1 release notes](V1_RELEASE_NOTES.md)
14. [RC3 authoring contract](RC3_AUTHORING_CONTRACT.md)
15. [`.zeno` language v1](ZENO_LANGUAGE_V1.md)
16. [Temporal logic v1](TEMPORAL_LOGIC_V1.md)
17. [Formal tools](FORMAL_TOOLS_RC3.md); [symbolic per-case checks](SYMBOLIC_CHECKS.md)
18. [Mini Determinator](MINI_DETERMINATOR.md)
19. [Mini Determinator QEMU kernel demo](QEMU_MINI_DETERMINATOR.md)
20. [CLI reference](CLI_REFERENCE.md)
21. [CLI and QEMU marketing captures](assets/marketing/README.md)
22. [Packaging](PACKAGING.md)
23. [Release assurance](RELEASE_ASSURANCE.md)
24. [LLM cybersecurity review orchestrator](LLM_CYBERSECURITY_REVIEW.md)
25. [Security review playbook](SECURITY_REVIEW_PLAYBOOK.md)
26. [Security hotspot and exploit-chain model](SECURITY_HOTSPOT_MODEL.md)
27. [Security standards snapshot](SECURITY_STANDARDS_SNAPSHOT.md)

## Project definition and typed construction

- [Language tutorial](tutorials/LANGUAGE.md)
- [Composition tutorial](tutorials/COMPOSITION.md)
- [CLI tutorial](tutorials/CLI.md)
- [Universal project profiles](UNIVERSAL_PROJECT_PROFILES.md)
- [Schema and code-generation boundary](SCHEMA_CODEGEN_BOUNDARY.md)
- [Schema-bound catalog](SCHEMA_BOUND_CATALOG.md)
- [Project bootstrap generator](PROJECT_BOOTSTRAP_GENERATOR.md)
- [Contract rules reference: `v2/policy.json`](CONTRACT_RULES.md)
- [Generated durable application milestone](GENERATED_APPLICATION_MILESTONE.md)
- [Packaged application qualification](PACKAGED_APPLICATION_QUALIFICATION.md)
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
- [Owned footprint sealing](OWNED_FOOTPRINT_SEALING.md)

## Formal tools and runtime refinement

- [Temporal tutorial](tutorials/TEMPORAL.md)
- [Formal-tools tutorial](tutorials/FORMAL_TOOLS.md)
- [Mini Determinator tutorial](tutorials/MINI_DETERMINATOR.md)
- [Law and claim substance](CLAIM_SUBSTANCE.md)
- [Inductive claims](INDUCTIVE_CLAIMS.md)
- [Law path resolution](LAW_PATH_RESOLUTION.md)
- [Kernel law harnesses](KERNEL_LAW_HARNESSES.md)
- [Shared-source Verus verification](../verification/verus/README.md)
- [System properties of finite transitions](SYSTEM_PROPERTIES.md)
- [Generic backend protocol](GENERIC_BACKEND_PROTOCOL.md)
- [Deterministic synthesis](DETERMINISTIC_SYNTHESIS.md)
- [Language-neutral synthesis and target conformance](LANGUAGE_NEUTRAL_SYNTHESIS.md)
- [Synthesis-first skill and MCP tools](LLM_SYNTHESIS.md)
- [Rust typestate and checked newtypes skill](../skills/rust-typestate-newtype/SKILL.md)
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
- [Reserved commitment-domain namespace](RESERVED_DOMAIN_NAMESPACE.md)
- [Secret handling and constant time](SECRET_HANDLING_AND_CONSTANT_TIME.md)
- [Side/covert-channel security](SIDE_CHANNEL_COVERT_CHANNEL_SECURITY.md)
- [LLM cybersecurity review orchestrator](LLM_CYBERSECURITY_REVIEW.md)
- [Evidence-first security review playbook](SECURITY_REVIEW_PLAYBOOK.md)
- [EPI hotspot and exploit-chain model](SECURITY_HOTSPOT_MODEL.md)
- [Security standards and threat-intelligence snapshot](SECURITY_STANDARDS_SNAPSHOT.md)

## Canonical admission and decoding

- [Admitted values](ADMITTED_VALUE_WITNESS.md)
- [Admitted envelopes](ADMITTED_ENVELOPE.md)
- [Bounded text](BOUNDED_TEXT_ADMISSION.md)
- [Bounded bytes](BOUNDED_BYTE_ADMISSION.md)
- [Decoder allocation hardening](DECODER_ALLOCATION_HARDENING.md)
- [Strict canonical patch decoder](STRICT_CANONICAL_PATCH_DECODER.md)
- [Strict canonical plan decoders](STRICT_CANONICAL_PLAN_DECODERS.md)
- [Correct-by-construction map entries](CORRECT_BY_CONSTRUCTION_MAP_ENTRIES.md)

- [Public documentation policy](PUBLICATION_POLICY.md)

## Design records

- [Design record index](adr/README.md)
- [0001: Meaning-first assurance](adr/0001-meaning-first-assurance.md)
- [0002: Principles for the V2 assurance program](adr/0002-principles.md)
- [0003: Epistemic status of evidence and witnesses](adr/0003-epistemic-status.md)
- [0004: V2 ledger of deferred breaking changes](adr/0004-v2-ledger.md)

## Status

The workspace is version `1.1.0`. The crates.io `1.1.0` API is the historical
stable line; the current supported V2 source API ships in the pinned public
candidates `v2.2.0-rc.1` and `v2.3.0-rc.1` and differs from `1.1.0`. See
[release candidates](RELEASE_CANDIDATES.md) and
[installation](INSTALLATION.md). Neither candidate is a stable V2 release.

The documentation describes implemented APIs and explicit boundaries. It does
not claim general deployment qualification, a bundled concurrent runtime, or
an end-to-end proof for arbitrary downstream projects.

- [Bounded completion and preparation](BOUNDED_COMPLETION.md)
