# ZenoFCIS 1.0.0-rc.3 release notes

Release date: pending publication.

RC3 is the authoring and composition usability candidate required before
stable V1. The existing protocol identifiers and canonical formats remain
unchanged.

## Added

- `zeno-fcis-spec`: a pure `no_std + alloc` bounded `.zeno` compiler, canonical
  typed `ProjectSpec`, accumulated diagnostics, builders, relational and
  temporal evaluators, derived composition, code generation, graphs, and the
  Mini Determinator semantic reference;
- `zeno-fcis-formal-tools`: deterministic CVC5 `1.3.3`, Z3 `4.16.0`, and Lean
  `4.30.0` exporters, checked process adapters, model replay, and
  content-addressed retention;
- `zeno-fcis-cli`: the `zeno-fcis` binary with project templates, checking,
  generation and drift detection, graph/explain views, formal-tool commands,
  doctor, and backend inspection;
- umbrella feature `authoring` and RC3 version-1 language, project-AST,
  temporal, tools-manifest, CLI JSON, and counterexample schemas;
- sixteen closed BDD/ATDD scenarios with fixed argv bindings and hostile-tag
  self-tests;
- official pinned formal-tool artifacts and recorded SHA-256 checksums;
- minimal and Mini Determinator `.zeno` examples plus a runnable Rust Mini
  Determinator lifecycle example.

The public package set grows from 33 to 36 crates and now publishes two
binaries: `zeno-fcis` and the existing `mount-zenodex-zusd` diagnostic parity
runner.
## Readiness polish

The pre-publication developer review added process-level CLI journeys for Mini
checking, JSON diagnostics, overwrite refusal, generation, and read-only drift
detection. The isolated downstream consumer now compiles and uses the umbrella
`authoring` API.

Parser nesting, operator chains, finite horizons, and formal export work have
explicit hard limits. Formal-tool timeouts cover input delivery. Solver names
use an injective encoding. The version check and requested run execute from the
same private copy of the admitted binary bytes.

The release packager now creates one uniquely named archive for each declared
binary and derives every retained binary command from the same inventory. Its
self-test opens the archives and checks their executable members.


## Compatibility

`.zeno` is authoring input. Only the lowered typed AST has canonical identity.
All existing manual constructors remain available. Equivalent source,
builder, and manual authoring lower through existing constructors; none can
mint evidence, receipts, commits, or authority.

The refinement AST remains suitable for a future Flux exporter, but RC3 does
not add or qualify Flux.

## Mini Determinator

The reference now executes finite isolated worker programs with explicit
spawn snapshots, get/put private workspaces, returns, canonical join/merge,
stable conflict witnesses, rollback, checked arithmetic, resource budgets, and
canonical replay traces. An isolated optional demo now boots that same public
semantic implementation inside a freestanding x86_64 kernel through OVMF and
QEMU. The demo is outside the public crate package set and does not expand the
semantic or authority claims.

## Release blockers and nonclaims

RC3 is not stable V1, an audit, deployment qualification, an unbounded theorem
for finite checks, an operating system, or a production authorization. Stable
`1.0.0` requires every exact-head workflow, independent review, package
reproducibility, SBOM/provenance, and owner publication check to pass without
another breaking API change.
