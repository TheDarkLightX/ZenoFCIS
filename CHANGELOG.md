# Changelog

All notable Rust API and packaging changes are recorded here. Canonical
protocol compatibility is governed separately by the identifiers and versions
embedded in ZenoFCIS values.

## Unreleased

No changes yet.

## 1.0.0-rc.3 - release date pending

Authoring and composition usability candidate before stable V1.

### Added

- versioned bounded `.zeno` parsing, canonical typed project ASTs, accumulated
  diagnostics, builders, relational logic, and explicit finite/unbounded
  temporal modes in the pure `zeno-fcis-spec` crate;
- deterministic CVC5 1.3.3, Z3 4.16.0, and Lean 4.30.0 exporters and
  fail-closed process adapters in `zeno-fcis-formal-tools`;
- the `zeno-fcis` CLI for templates, checking, generation/drift, graphs,
  explanations, formal tools, and backend diagnosis;
- the executable Mini Determinator private-workspace, get/put, return,
  canonical join, conflict, rollback, budget, and replay reference;
- sixteen RC3 BDD/ATDD scenarios, tutorials, official formal-tool checksum
  workflow, and a 36-crate public package inventory.

### Compatibility and limits

- existing protocol IDs and canonical formats are unchanged;
- `.zeno` is non-authoritative authoring input and only the lowered AST has
  canonical identity;
- finite temporal checks, generated views, and tool agreement do not grant
  production authority;
- the Mini Determinator is a semantic model. Its isolated QEMU kernel is a demonstration and carries no production OS or hardware claim.

## 1.0.0-rc.2 - 2026-07-29

Second public release candidate for the reusable ZenoFCIS core library family.

### Changed

- Catalog format 3 requires every effect and channel to bind explicit reviewed
  `OperationSemantics`, including canonical asset-scoped value-flow sets.
- Verified project laws now derive non-waivable economic families and
  committing-decision coverage from the exact catalog. Custom value relations
  require one exact registered claim with independently retained evidence.
- `CommitPlan` is explicitly non-executable committed evidence. Every effect
  definition is constructor-fixed to `CommitEffectSemantics::EvidenceOnly`,
  project catalogs reject value-moving effects, and external value movement
  must use a durable value-classified outbox channel.
- Production authority now names and binds the concrete outbox-delivery
  interpreter through `BoundDeliveryInterpreter`.
- Release-candidate packaging now uses the version-neutral
  `tools/rc_package.py` entry point and current-version-derived artifact names.
- RC2 now freezes its reusable product surface explicitly and binds nine
  human-readable BDD scenarios to a closed executable ATDD registry.
- Optional deterministic coding-agent guardrails pin Node `22.23.1` and
  Probity `1.10.0`; hostile command and transcript mutations are permanent
  acceptance evidence and AI-judged TDD is not enabled.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree and included SQLite deployment remain bounded
  reference surfaces unless separately qualified;
- concrete formal-tool adapters and project proof artifacts remain
  project-owned;
- signing, hosted provenance, crates.io publication, and independent exact-head
  review remain owner or external actions.

## 1.0.0-rc.1 - 2026-07-29

First public release candidate for the reusable ZenoFCIS core library family.

### Included

- immutable values, canonical ZCVE/1 encoding, deterministic budgets, and the
  `Accept | Reject | CommittedFailure` decision algebra;
- preconditioned canonical patches, closed effect and outbox plans, receipts,
  candidate sealing, and nominal production commit authorization;
- project profiles, schemas, catalogs, generated typed transitions, relational
  laws, and fixed-size composable domain machines;
- proof-carrying composition with complete static footprint evidence and
  deterministic-parallel authorization;
- tool-neutral evidence, refinement, synthesis, and checked-backend protocols;
- strict invocation-bound decision reconstruction, derived refinement cases,
  canonical exhaustive-domain manifests, independently verified coverage, and
  content-addressed promotion reports;
- reference authenticated state, persistent collections, SQLite publication,
  mounted-runtime adapters, secret handling, and side/covert-channel policy;
- 33 publishable crates, a diagnostic ZenoDEX mount binary, complete public API
  rustdoc pages, examples, checksums, package/source archives, CycloneDX SBOM
  generation, and provenance inputs. The reproducible offline rustdoc archive
  explicitly excludes Rustdoc `1.97.1`'s nondeterministic global-search index.
- candidate-derived outbox delivery identities shared byte-for-byte by the
  reference and SQLite shells, with SQLite schema v3 rejecting old
  authorization-derived identities pending explicit migration.
- policy-bound, law-verified nominal genesis authorization; one-time pure and
  SQLite shell creation; strict receipt, bundle, and authorization decoding;
  and SQLite schema v5 full-history reauthorization with exact
  authorization/bundle/receipt/replay/outbox set reconstruction. Schema v4 and
  earlier stores are rejected pending explicit migration.
- canonical sparse-proof and authenticated-plan decoding with explicit resource
  limits, exact round-trip admission, and non-authoritative decoded plan values;
- retained-evidence projector qualification, required per-transition projection
  relations, nominal candidate-bound authenticated commits, and a production-facing
  authenticated publication port.
- a checked V1 release runbook, declared code ownership, and read-only
  reproducible package assembly for immutable `v1.0.0-rc.*` tags.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree remains a bounded reference implementation;
- SQLite qualification is limited to the documented reference deployment;
- concrete Lean, SMT, CVC5, Kani, Flux, or private ESSO adapters and proof
  artifacts remain project-owned;
- concurrent execution and effect interpretation remain external shell work;
- final independent exact-head review and release attestation remain required
  before `1.0.0`.
