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
  doctor, backend inspection, and Lean runtime inventory;
- umbrella feature `authoring`, language version 1, project-AST format 1,
  temporal-spec format 1, tools-manifest format 2, CLI JSON schema 1, and
  counterexample schema 1;
- twenty-five closed BDD/ATDD scenarios with fixed argv bindings and hostile-tag
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

The README and tutorials include real xterm screenshots of CLI help, Mini
Determinator checking, accumulated diagnostics, and Mermaid graph output. A
bounded completion handshake prevents partial terminal captures.

Parser nesting, operator chains, finite horizons, and formal export work have
explicit hard limits, including live Lean render budgets. Formal-tool timeouts
cover input delivery, and process-group cleanup also runs after successful
completion. Solver names use an injective encoding. The version check and
requested run execute from the same private copy of admitted binary bytes.
Lean uses a private runtime snapshot that is rechecked after execution. Both
SMT phases are retained, contradictory results block, and complete evidence
bundles are published atomically by content hash. `KernelChecked` also requires
the qualified official Lean `4.30.0` Linux x86-64 tree identity.

The release packager now creates one uniquely named archive for each declared
binary and derives every retained binary command from the same inventory. Its
self-test opens the archives and checks their executable members. It also
fetches the locked external graph, unpacks all 36 public crate archives into a
resolver-3 workspace, and compiles all features across every library, test,
example, benchmark, and binary target offline.

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
