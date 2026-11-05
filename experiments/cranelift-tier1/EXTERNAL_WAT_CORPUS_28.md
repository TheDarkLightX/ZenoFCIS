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


## Executed qualification — November 5, 2026 UTC

**Exact-head [CI run #41963407858](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41963407858) PASSED**, at experiment commit [158af1cf7b4a](https://github.com/TheDarkLightX/ZenoFCIS/commit/158af1cf7b4aebc220878177fa9ec4fda1cbeab1). The [raw archive](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41963407858/artifacts/13386776114) contains all twelve unchanged upstream WAT sources, baseline/patched native AOT binaries, disassemblies, original Git blob pins, one-rule patch, compiler logs, producer receipt and independently executed recheck.

All five manifest/oracle/refusal controls passed. Both pinned Wasmtime CLI builds passed. Every one of the twelve external upstream WAT fixtures compiled successfully with the identical source under original versus patched compiler. The independent recheck and extra finite bswap/simple export executions passed.

**Measured result: no code or artifact change for this external test corpus.**

| Exact source population | Count | Native disassembly changed | ELF .text saving | Complete .cwasm saving |
|---|---:|---:|---:|---:|
| Pinned upstream Wasmtime compiler WAT fixtures | 12 | **0** | **0 bytes** | **0 bytes** |

The producer reported complete AOT binary identity for **all 12/12** baseline-patched pairs, and zero actual ELF .text and complete .cwasm size difference per pair. The separately executed independent verifier reported 0 changed disassemblies, 12 successful audits, and a 0-byte summed complete-AOT delta. Every exact pair's entire native AOT bytes agreed, not merely its file length.

This is **not an optimization regression or a rule counterexample**. The fixtures do not establish the target pattern's application prevalence; equally, this negative control does not invalidate the previous synthetic 4,096-byte mixed-module saving. It does prevent treating that earlier artificial saving as evidence of a general application benefit. Retain both positive and negative cases in the source/proof/cost catalog. The rule has **not** earned a general-purpose upstream optimization PR.

Remaining gates: test idiomatic compiler output (for example Rust i64 signed saturating_add lowered to real Wasm); source real external application modules that include the target arithmetic; independently qualify pattern frequency and actual ELF/whole AOT costs; validate authoritative select semantics and the named VeriISLE rule; and measure runtime if an application saving emerges. No throughput/power/all-target claim or protected/upstream branch edit.
