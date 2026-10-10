# Changelog

All notable Rust API and packaging changes are recorded here. Canonical
protocol compatibility is governed separately by the identifiers and versions
embedded in ZenoFCIS values.

## 1.0.0-rc.1 - 2026-07-28

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
- reference authenticated state, persistent collections, SQLite publication,
  mounted-runtime adapters, secret handling, and side/covert-channel policy;
- 32 publishable crates, a diagnostic ZenoDEX mount binary, complete public API
  rustdoc pages, examples, checksums, package/source archives, CycloneDX SBOM
  generation, and provenance inputs. The reproducible offline rustdoc archive
  explicitly excludes Rustdoc `1.97.1`'s nondeterministic global-search index.
- candidate-derived outbox delivery identities shared byte-for-byte by the
  reference and SQLite shells, with SQLite schema v3 rejecting old
  authorization-derived identities pending explicit migration.

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
