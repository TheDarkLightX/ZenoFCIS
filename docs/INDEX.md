# ZenoFCIS documentation

This index groups the guides by what you want to do. New readers should start
with [Get started](#get-started), then follow a tutorial.

## Get started

- [Installation](INSTALLATION.md)
- [Quickstart](QUICKSTART.md)
- [ZenoFCIS 2.4.0 release notes](RELEASE_NOTES.md)
- [Earlier public release candidates](RELEASE_CANDIDATES.md)
- [V2 normal program API migration](V2_PROGRAM_API_MIGRATION.md)

## Tutorials

- [Write and check a `.zeno` project](tutorials/LANGUAGE.md)
- [Connect components and inspect the result](tutorials/COMPOSITION.md)
- [Use the CLI as a checked workflow](tutorials/CLI.md)
- [Run finite time claims](tutorials/TEMPORAL.md)
- [Run formal tools and read each result](tutorials/FORMAL_TOOLS.md)
- [Replay the Mini Determinator](tutorials/MINI_DETERMINATOR.md)

## Build and maintain applications

- [Generated durable application milestone](GENERATED_APPLICATION_MILESTONE.md)
- [Packaged application qualification](PACKAGED_APPLICATION_QUALIFICATION.md)
- [Symbolic per-case checks](SYMBOLIC_CHECKS.md)
- [Bounded completion and preparation](BOUNDED_COMPLETION.md)
- [Wide values and larger decisions](WIDE_VALUE_EXECUTION.md): development
  profile support, capacity and assurance boundaries
- [Determinism](DETERMINISM.md)
- [Using ZenoFCIS with coding and reasoning models](LLM_USAGE.md)
- [Synthesis-first skill and MCP tools](LLM_SYNTHESIS.md)

## Core components and coupled applications

- [Core component catalogue](../core-components/README.md)
- [Source-bound finite family certificates](../core-components/FINITE_PROOFS.md)
- Coupled application profiles: [API quota](../core-apps/api-quota/README.md),
  [document release](../core-apps/document-release/README.md),
  [equipment booking](../core-apps/equipment-booking/README.md),
  [expense approval](../core-apps/expense-approval/README.md),
  [reliable jobs](../core-apps/reliable-jobs/README.md) and
  [warehouse reservation](../core-apps/warehouse-reservation/README.md)

## Reference

- [API reference](API_REFERENCE.md)
- [CLI reference](CLI_REFERENCE.md)
- [Crate map](CRATE_MAP.md)
- [Feature matrix](FEATURE_MATRIX.md)
- [Contract rules reference: `v2/policy.json`](CONTRACT_RULES.md)
- [Contract judges: specification gaps, state profile, lemmas, scenarios and probes](CONTRACT_JUDGES.md)
- [Composed-domain composition](COMPOSED_DOMAIN_COMPOSITION.md)
- [RC3 authoring contract](RC3_AUTHORING_CONTRACT.md)
- [`.zeno` language v1](ZENO_LANGUAGE_V1.md)
- [Temporal logic v1](TEMPORAL_LOGIC_V1.md)
- [Formal tools](FORMAL_TOOLS_RC3.md)
- [Canonical bytes and admission](CANONICAL_BYTES.md)
- [Mini Determinator](MINI_DETERMINATOR.md)
- [Mini Determinator QEMU kernel demo](QEMU_MINI_DETERMINATOR.md)
- [CLI and QEMU captures](assets/marketing/README.md)

## Behavior dialogue (ZAL research prototype)

- [ZAL: terminal-first behavior review](zal/README.md)
- [ZAL specification](zal/SPECIFICATION.md)
- [`zal/finite-fsm/1` grammar and limits](zal/GRAMMAR.md)
- [ZAL harness integration](zal/HARNESS_INTEGRATION.md)
- [ZAL prototype source map](ZAL_SOURCE_MAP.md)

## Architecture and design records

- [Architecture](ARCHITECTURE.md)
- [Design record index](adr/README.md)
- [0001: Meaning-first assurance](adr/0001-meaning-first-assurance.md)
- [0002: Principles for the V2 assurance program](adr/0002-principles.md)
- [0003: Epistemic status of evidence and witnesses](adr/0003-epistemic-status.md)
- [0004: V2 ledger of deferred breaking changes](adr/0004-v2-ledger.md)
- [0005: Library-owned decision gate](adr/0005-decision-gate.md)
- [Finite interpreter assurance notes](FINITE_INTERPRETER_ASSURANCE.md)

## Assurance and verification

- [Release assurance](RELEASE_ASSURANCE.md)
- [BDD and acceptance testing](ACCEPTANCE_TESTING.md)
- [Deterministic developer guardrails](DEVELOPER_GUARDRAILS.md)
- [Law and claim substance](CLAIM_SUBSTANCE.md)
- [Inductive claims](INDUCTIVE_CLAIMS.md)
- [Law path resolution](LAW_PATH_RESOLUTION.md)
- [System properties of finite transitions](SYSTEM_PROPERTIES.md)
- [Kernel law harnesses](KERNEL_LAW_HARNESSES.md)
- [Shared-source Verus verification](../verification/verus/README.md)
- [Generic backend protocol](GENERIC_BACKEND_PROTOCOL.md)
- [Evidence importers](EVIDENCE_IMPORTERS.md)
- [Deterministic synthesis](DETERMINISTIC_SYNTHESIS.md)
- [Language-neutral synthesis and target conformance](LANGUAGE_NEUTRAL_SYNTHESIS.md)
- [Validated refinement and exhaustive coverage](VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md)
- [Rust typestate and checked newtypes skill](../skills/rust-typestate-newtype/SKILL.md)
- [Optional finite Tau kit](TAU_KIT.md)

Private checkers are optional. Users who have one can implement the public
backend or evidence traits in a private crate. Lean, SMT/Z3, CVC5, Kani, Flux,
and other tools can use the same public boundaries.

## Security

- [LLM cybersecurity review orchestrator](LLM_CYBERSECURITY_REVIEW.md)
- [Evidence-first security review playbook](SECURITY_REVIEW_PLAYBOOK.md)
- [EPI hotspot and exploit-chain model](SECURITY_HOTSPOT_MODEL.md)
- [Security standards and threat-intelligence snapshot](SECURITY_STANDARDS_SNAPSHOT.md)
- [Secret handling and constant time](SECRET_HANDLING_AND_CONSTANT_TIME.md)
- [Side-channel and covert-channel assurance](SIDE_CHANNEL_COVERT_CHANNEL_SECURITY.md)
- [SHA-256 provider policy](SHA256_PROVIDER_POLICY.md)
- [Reserved commitment-domain namespace](RESERVED_DOMAIN_NAMESPACE.md)
- [Supply-chain exceptions](SUPPLY_CHAIN_EXCEPTIONS.md)

## Library boundaries

These guides document individual protocols and library boundaries. Guides for
earlier protocols are historical: they are not instructions to restore removed
callback authority or authenticated commit constructors. The
[V2 program API migration guide](V2_PROGRAM_API_MIGRATION.md) describes the
supported V2 route.

### Project definition and typed construction

- [Universal project profiles](UNIVERSAL_PROJECT_PROFILES.md)
- [Project-neutral umbrella boundary](PROJECT_NEUTRAL_UMBRELLA.md)
- [Schema and code-generation boundary](SCHEMA_CODEGEN_BOUNDARY.md)
- [Typed adapter code generation](CODEGEN_TYPED_ADAPTERS.md)
- [Schema-bound catalog](SCHEMA_BOUND_CATALOG.md)
- [Project bootstrap generator](PROJECT_BOOTSTRAP_GENERATOR.md)
- [Catalogued transition builder](CATALOGUED_TRANSITION_BUILDER.md)
- [Immutable transition budget boundary](IMMUTABLE_TRANSITION_BUDGET.md)
- [Generated catalog transition](GENERATED_CATALOG_TRANSITION.md)
- [Generated typed reasons](GENERATED_TYPED_REASONS.md)
- [Generated typed effects and channels](GENERATED_TYPED_EFFECTS_CHANNELS.md)
- [Generated typed root reads](GENERATED_TYPED_ROOT_READS.md)
- [Generated typed root updates](GENERATED_TYPED_ROOT_UPDATES.md)
- [Generated typed context observations](GENERATED_TYPED_CONTEXT_OBSERVATIONS.md)
- [Generated command and context envelopes](GENERATED_COMMAND_CONTEXT_ENVELOPES.md)
- [Generated root-envelope smart constructors](GENERATED_ROOT_ENVELOPE.md)
- [Generated high-level mutation boundary](GENERATED_NO_RAW_MUTATIONS.md)

### Composition, laws, and authority

- [Formal composition v2](FORMAL_COMPOSITION_V2.md)
- [Complete static footprint witnesses](COMPLETE_FOOTPRINT_WITNESS.md)
- [Fixed-size state domain machines](FIXED_STATE_DOMAIN_MACHINES.md)
- [Composed domain program](COMPOSED_DOMAIN_PROGRAM.md)
- [Composed root projection conformance](COMPOSED_ROOT_PROJECTION_CONFORMANCE.md)
- [Project relational laws](PROJECT_RELATIONAL_LAWS.md)
- [Catalog-derived economic law requirements](ECONOMIC_LAW_DERIVATION.md)
- [Catalog authorization boundary](CATALOG_AUTHORIZATION_BOUNDARY.md)
- [Policy-bound genesis authorization](GENESIS_AUTHORIZATION.md)
- [Candidate and commit boundary](CANDIDATE_COMMIT_BOUNDARY.md)
- [Commit evidence and durable outbox model](COMMIT_EVIDENCE_AND_OUTBOX_MODEL.md)
- [Owned footprint sealing](OWNED_FOOTPRINT_SEALING.md)

### Persistence, shells, and mounted runtimes

- [SQLite shell refinement](SQLITE_SHELL_REFINEMENT.md)
- [Strict artifact decoding and SQLite history](STRICT_ARTIFACT_AND_SQLITE_HISTORY.md)
- [Outbox delivery identity](OUTBOX_DELIVERY_IDENTITY.md)
- [Authenticated-state adapter](AUTHENTICATED_STATE_ADAPTER.md)
- [Authenticated sparse-proof context](AUTHENTICATED_PROOF_CONTEXT.md)
- [Candidate-bound authenticated authority](AUTHENTICATED_AUTHORITY_BOUNDARY.md)
- [Persistent collections](PERSISTENT_COLLECTIONS.md)
- [Fallible persistent-map API](FALLIBLE_PERSISTENT_MAP_API.md)
- [Bounded collections and profile-bound pipes](BOUNDED_COLLECTIONS_AND_PIPES.md)
- [Mounted ZenoDEX adapter](MOUNTED_ZENODEX_ADAPTER.md)
- [Mounted ZenoDEX zUSD v1](MOUNTED_ZENODEX_ZUSD_V1.md)

### Canonical admission and decoding

- [Admitted values](ADMITTED_VALUE_WITNESS.md)
- [Admitted envelopes](ADMITTED_ENVELOPE.md)
- [Schema-bound admitted envelope](SCHEMA_BOUND_ENVELOPE.md)
- [Bounded text](BOUNDED_TEXT_ADMISSION.md)
- [Bounded bytes](BOUNDED_BYTE_ADMISSION.md)
- [Decoder allocation hardening](DECODER_ALLOCATION_HARDENING.md)
- [Strict canonical patch decoder](STRICT_CANONICAL_PATCH_DECODER.md)
- [Strict canonical plan decoders](STRICT_CANONICAL_PLAN_DECODERS.md)
- [Correct-by-construction map entries](CORRECT_BY_CONSTRUCTION_MAP_ENTRIES.md)

## Integrations and demo site

- [OCaml scalar consumer](../integrations/ocaml/README.md)
- [Experimental MAP-Elites archive](../integrations/elite_archive/README.md)
- [Demo site](../site/README.md) and its [browser API](../site/ABI.md)

## Releases, history, and policy

- [Release packaging](PACKAGING.md)
- [V1 product contract](V1_PRODUCT_CONTRACT.md)
- [Historical V1.1 release notes](V1_1_RELEASE_NOTES.md)
- [Historical V1 release notes](V1_RELEASE_NOTES.md)
- [Changelog](../CHANGELOG.md)
- [Contributing](../CONTRIBUTING.md)
- [Agent handoff boundary](AGENT_HANDOFF_BOUNDARY.md)
- [Public documentation policy](PUBLICATION_POLICY.md)

## Status

The workspace is version `2.4.0`, the current release, following the first
stable V2 line, 2.3.0.
Its V2 API differs from the historical crates.io `1.1.0` API and is used from the matching `v2.4.0` source tag; see
[installation](INSTALLATION.md) and the
[2.4.0 release notes](RELEASE_NOTES.md). The earlier public candidates
`v2.2.0-rc.1` and `v2.3.0-rc.1` were prereleases; their
[candidate record](RELEASE_CANDIDATES.md) keeps their sources and limits.

The documentation describes implemented APIs and explicit boundaries. It does
not claim general deployment qualification, a bundled concurrent runtime, or
an end-to-end proof for arbitrary downstream projects.
