# V2 native constructor custody stage

The normal low-level packages no longer export the native budget constructor,
budget-finished decision, transition callback, catalogued transition builder,
domain-machine callback, composed native program, or native authenticated and
catalog commit authorities. `CandidateBuilder` and `CandidateBuilder::seal` are
crate-private in the receipt transport package. Receipt decoding still
reconstructs and checks complete historical bytes; a decoded bundle is inert
data and cannot become a V2 publication capability.

`zeno-fcis-authority` directly reexports the existing checked `Authority`,
`Publication`, `GenesisPublication`, outcomes, refusal, and binding operations.
There is no conversion from a native candidate, reported usage, closure, or raw
state authoring object into these capabilities. The publication types remain
non-Clone and have private fields. Normal authority evaluation receives original
wire inputs under complete checked declarations.

## Retained original oracle

`verification/kernel-laws` is a separate, nonpublished verification package.
Its library exports no oracle. Complete retained native algorithms live only
under its private `#[cfg(test)]` module; all supporting packages are development
dependencies. The normal workspace has no dependency or call edge into it.
There is no feature, hidden public factory, key, or downstream callback that
opens this boundary.

The oracle retains 146 original tests, both original minimal and bounded-fold
examples as executable tests, and one new relocation-adapter test. One original
provider-binding test requires the existing `libcrux` feature. One original
manual vector exporter remains marked ignored for ordinary execution and is
also run explicitly during this stage. Its 18 footprint/resource/body vectors
match the independently executed original native reference in exact order.

The original fold algorithm calls the actual finite `Program` evaluator through
a private trait adapter. Its public `evaluate` operation invokes the same
internal evaluator and preserves the tuple or error observed by the fold. The
adapter clears output before evaluation, preserves refusal without committing
the local accumulator, and discards node scratch. Internal allocation-capacity
reuse differs; host allocation is not the legacy logical reservation report.
No second expression evaluator was introduced.

Original sources, checked Value/provider caller adaptations, ownership/import
rewrites, mechanical inherent-method borrow cleanup, and formatting are recorded
separately. The existing finite-bounds ghost annotations are retained; the
verification manifest recognizes their cfg name without enabling that cfg.
This native relocation is not a new formal proof of the original algorithms.

## Evidence boundaries

The stage records exact original and final sources, seed-relative patches,
resolved package graphs, intended-diagnostic external privacy controls, normal
publication byte oracles, native tests, no-std checks, and strict lint results.
Native qualification uses the pinned toolchain and reviewed locks, one shared
target, serialized heavy checks, and unchanged source hashes across each run.
Cross-worktree stale metadata was detected by an external control and rejected;
its diagnostic remains retained. Subsequent qualification records byte-preserving
source mtime refreshes and actual compiler/extern provenance.

The authority getter regenerated for this development candidate is not final
combined source/proof qualification. Root integration must include its final
actual closure and rerun the required proof and control gates.

## Obligations still open

- The normal core still has the historical seven-resource data types. A shared
  nominal eight-resource type, including `Step`, must live in a lower layer and
  be consumed by the actual checked evaluator without an upward dependency.
  Resource indexing, source identity, proof, native, and Wasm correspondence
  remain part of that coordinated change.
- `LawCheckInput::pre_state` and the standalone native `ProjectLawEngine` surface
  still exist in the laws package. They do not construct V2 capabilities, but
  their literal ADR 0004 retirement is not discharged by this stage.
- Root-owned generated helpers, legacy facade callers, templates, shell/SQLite
  fixtures, fuzz targets, and the external-consumer fixture require their exact
  coordinated caller cutover. Contextual proposals accompany this stage.
- The complete composed, authenticated, prepared, and native application
  domains remain obligations for the checked program family. Preserving their
  reference algorithms and tests is not their checked production replacement.
  Their original policies, law matrices, and legal cases remain retained; these
  domains have not been withdrawn or deferred to a later version.

This stage establishes the specified constructor custody boundary. It does
not close the whole V2 ledger, establish full shell assurance, or qualify a
combined release.
