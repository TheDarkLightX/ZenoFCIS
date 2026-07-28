# ZenoFCIS 1.0.0-rc.1 release notes

This is the first release candidate for the reusable ZenoFCIS core library.
It is intended for API review, downstream integration, formal-backend adapter
development, and release-process validation.

## What users receive

- 32 version-aligned Rust crates and the `zeno-fcis` umbrella crate;
- `no_std + alloc` foundational and semantic surfaces;
- typed project construction, relational laws, nominal commit authority,
  fixed-size domain machines, and explicit global composition;
- backend-neutral protocols for Lean, SMT/Z3, CVC5, Kani, Flux, private ESSO,
  and other independently checked tools;
- deterministic-parallel planning with complete static footprint evidence;
- human quickstarts, LLM guidance, crate and feature maps, complete public API
  rustdoc pages, and checked external-consumer examples;
- `.crate`, source, rustdoc, and Linux diagnostic-binary archives;
- checksums, source/package manifests, CycloneDX SBOM, and provenance inputs.

The reproducible offline rustdoc archive excludes Rustdoc `1.97.1`'s
nondeterministic merged global-search shards. All public crate API and source
pages remain; the archive includes an exact notice explaining the boundary.

## Supported core claim

The RC provides bounded, immutable, canonically encoded construction and
validation primitives for FCIS applications. Production commit authority is a
private nominal value created only through the documented catalog, invocation,
provider, law, interpreter, and deployment checks.

## Remaining blockers for 1.0.0

1. Merge the exact reviewed stacked PR series onto `main` without semantic
   drift.
2. Complete independent exact-head API and authority review of this RC.
3. Close exhaustive-refinement evidence fabrication and exact SQLite
   bundle/outbox reconstruction for the promoted production profiles.
4. Qualify the selected authenticated-state and persistence deployment rather
   than promoting the bounded reference backends implicitly.
5. Retain concrete evidence for every verifier and target advertised by a
   production profile.
6. Produce signed release artifacts and hosted provenance from the protected
   final tag.

These blockers constrain production claims. They do not prevent downstream
users from evaluating the core library API through this release candidate.
