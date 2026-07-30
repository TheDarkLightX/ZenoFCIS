# RC3 authoring and composition contract

ZenoFCIS `1.0.0-rc.3` is the usability candidate before stable V1. It adds a
bounded source language and tool-facing shell without changing any existing
semantic, composition, canonical-byte, evidence, receipt, or authority format.

## Frozen pipeline

```text
inert UTF-8 .zeno source
    -> bounded lexer and parser
    -> ParsedProject + accumulated diagnostics
    -> elaboration
    -> canonical typed ProjectSpec
    -> existing Schema / ProjectProfile / ProjectCatalog / CompositionSpec
    -> generated const-generic Rust bindings
    -> ComposedDomainProgram::try_new with concrete machines
```

Only `ProjectSpec` has canonical identity. Comments, whitespace, and
declaration order do not affect its bytes. Explicit precedence and composition
merge order do affect semantics. Parser and builder construction share the
same elaborator and must produce identical bytes for equivalent projects.

The compiler is `no_std + alloc`, forbids unsafe code, and has no filesystem,
environment, clock, randomness, networking, process, thread, or async access.
The process adapters and CLI are separate `std` crates.

## Fixed limits

- source: at most 1 MiB;
- lexer output: at most 262,144 tokens;
- retained diagnostics: at most 256;
- project declarations: at most 65,536;
- components and ports: bounded by `ProjectLimits` and the existing
  composition limits;
- formulas and evaluation: explicit node, depth, quantifier, sum, predicate,
  and logical-step limits.

Exhaustion is a diagnostic or `Indeterminate`; it never becomes acceptance.

## Authority boundary

Authoring may construct typed descriptions, derive conservative footprints,
generate source, render graphs, and propose formal evidence. It cannot create
`BackendCertificate`, production evidence, authority, receipts, commits, or
shell publication capability. Generated source still passes through existing
constructors. Concrete machine binding still ends at
`ComposedDomainProgram::try_new`.

Graphs, Markdown, diagnostics, generated code, BDD success, finite temporal
checks, and solver agreement are review evidence with limited scope. None
grants production authority. External tools remain untrusted processes whose
identity and output are rechecked for every invocation.

## Pattern selection record

RC3 applies these functional-core patterns:

- pure compiler core: parsing, elaboration, canonicalization, formula
  evaluation, Mini Determinator execution, and derived views are total or
  bounded pure functions over explicit values;
- explicit effects as data: generated outputs and formal-tool requests are
  inert values until the CLI shell interprets them;
- error-as-data: compiler diagnostics accumulate deterministically, and
  evaluator or tool uncertainty remains blocked;
- capability containment: only `zeno-fcis-formal-tools` owns process access,
  and `.zeno` files cannot choose paths, arguments, or environment values;
- atomic persistence: generated files and retained evidence use temp-file,
  flush, rename publication, while `generate --check` is read-only;
- deterministic replay: canonical inputs, exact tool identity, outputs,
  models, traces, and generated artifacts are retained by content hash.

The Mini Determinator is an executable semantic reference. RC3 also includes
an isolated QEMU demonstration under `demos/`. That demonstration boots a
freestanding Rust kernel through OVMF and calls the public semantic reference.
It stays outside the public package set and authority path.

The demonstration has no ZenoFCIS bootloader, threads, hardware scheduler,
wall-clock semantics, or private-project dependency. Its finite worker
interpreter tests spawn snapshots, private get/put workspaces, returns,
canonical joins, rollback, budgets, and schedule invariance. QEMU adds evidence
that the same code can run in a freestanding guest. It does not extend those
semantic claims to physical hardware or a complete operating system.

## Release boundary

RC3 remains a prerelease until exact-head CI, ATDD, independent review,
package reproducibility, SBOM/provenance, and publication checks pass. Stable
`1.0.0` follows only if RC3 requires no further breaking public API change.
