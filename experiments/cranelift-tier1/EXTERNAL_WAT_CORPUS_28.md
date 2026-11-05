# Tier-1 qualification: external source-pinned Wasmtime WAT corpus

This suite is an **external compiler-regression-fixture screen**, not a
representative production application benchmark and not an upstream-ready
optimization. It follows the independently checked mixed 32/128-function AOT
size reductions, which used ZenoFCIS-authored synthetic workloads.

## Experiment

Upstream compiler/source: Bytecode Alliance Wasmtime
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.
ZenoFCIS: isolated
`research/2.8-tier1-cranelift-performance-20261102` branch.
Only `tier1_i64_signed_saturation_clamp` is installed in a detached
checkout's `cranelift/codegen/src/opts/selects.isle`.
Same exact WAT is compiled by unchanged and one-rule-patched Wasmtime CLI
under `x86_64-unknown-linux`; no upstream file is modified.

The explicitly reviewed 12-source manifest lives in
`qualify_external_wat_corpus_28.py`. The corpus contains ten unmodified
`tests/disas/*.wat` upstream compiler fixtures (arithmetic, bswap,
Fibonacci, branches, calls, traps, ctz/clz, dead code, globals and
memory loads) and two `tests/all/cli_tests/*.wat` CLI fixtures
(Fibonacci and a simple multitype module). Each upstream Git blob SHA
is required to match the manifest; the archiver preserves the original
WAT bytes without stripping `;;!` or other comments.

All twelve fixture compilations are **required**. Failure of any is a
failed run, not an omitted data point. A nonzero compiler exit does
not get counted as an optimization win, even if a later file compiles.

## Executable evidence and measured metrics

Source producer: `qualify_external_wat_corpus_28.py`.
The targeted CI job checks the exact pinned upstream revision, both
compiler binary identities and one-rule patch, then compiles each WAT,
archives each complete `.cwasm` ELF object, disassembly, source
bytes and build outputs. Four native numeric smoke cases plus a zero
case run on the external bswap32/bswap64 and simple exported functions.
Other external fixtures are **compile-only**, not claimed to pass finite
output equivalence. Reproducing the smoke cases does not establish
whole-program or memory/trap equivalence.

Independent rechecker:
`independent_recheck_external_wat_28.py`.
It independently verifies original Git blobs against the checkout,
WAT copies, compiler object SHA-256 and version, complete native AOT
bytes, ELF `.text` bytes, archived disassembly hashes, original
producer receipt and all measured signed deltas. It executes *additional*
bswap/simple input pairs using a distinct byte-reversal oracle.

Each result records **baseline minus patched**, separately:
- actual ELF `.text` bytes
- entire serialized `.cwasm` bytes
- full native binary hash equality
- archived disassembly equality

No metric derives complete AOT savings from x86 instruction-prefix
savings, and aggregate totals sum **separate modules**; they do
not describe a single linked application. It is possible for all
twelve external inputs to show zero difference without contradicting
the artificial mixed-module win. If so, this is negative evidence
about the specific pattern's occurrence, not a formal proof or
an application-speed result.

The existing QF_BV clamp identity and bounded native ZenoFCIS search
are separate receipts. The actual named VeriISLE opt rule passed
only under a local provisional scalar `select` specification; this
corpus does not turn that conditional result into an unconditional
upstream verifier proof. No general x64 flags/ABI, undefined/poison
or other-target equivalence follows from these tests.

## Execution and preservation

Workflow:
`.github/workflows/tier1-cranelift-external-wat-28.yml`.

The exact CI run and archived artifacts must be inspected before
asserting a passed regression screen or any byte reduction.
No fork, protected main/release branch or Bytecode Alliance upstream
branch was modified; no upstream PR was opened by this experiment.

**Next threshold:** an externally sourced realistic application that
actually exercises the precise rewrite with independently verified
benefit; stronger authoritative select semantics, cross-target
correctness and controlled runtime checks before any upstream proposal.
