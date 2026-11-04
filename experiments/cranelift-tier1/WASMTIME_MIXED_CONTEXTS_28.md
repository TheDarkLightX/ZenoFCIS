# Tier-1 continuation: heterogeneous Wasm contexts and controls

**Status: experimental source checkpoint only. No new CI or measured result
is claimed until the targeted workflow passes.**

This follows the verified alignment-boundary experiment
(WASMTIME_ALIGNMENT_BOUNDARY_28.md). The earlier synthetic saturation
case saved no complete AOT bytes; an added live XOR produced real
32-byte-per-export .text savings above an alignment boundary.
That result did not establish real-program prevalence or speed.

## Test question

Does the same pinned single-rule Cranelift rewrite affect actual ELF
.text and whole compiled Wasmtime .cwasm size when mixed with other
integer operations and unrelated functions, rather than repeated identical
saturating-add exports?

Pinned upstream: bytecodealliance/wasmtime SHA
b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49.
Isolated ZenoFCIS branch:
research/2.8-tier1-cranelift-performance-20261102.
Only the experimental tier1_i64_signed_saturation_clamp rule is
installed into a detached checkout. Baseline and patched compiler
receive exactly the same, hashed WAT for each case.

| Workload | Exports | Signed saturation contexts | Unrelated controls |
|---|---:|---:|---:|
| mixed32 | 32 | 16 | 16 |
| mixed128 | 128 | 64 | 64 |
| controls32 | 32 | 0 | 32 |

All exports use signed i64 x signed i64 -> signed i64. The inherited
satN export prefix is only the existing disassembly parser convention;
unrelated control functions do **not** implement saturation.

Eight function families: original saturating add; saturating add then
live XOR; saturating add then unsigned shift/XOR/add; saturating add
then signed-condition branch/XOR; plain XOR/add; wrapping multiply,
add and XOR; signed select/add; rotate/XOR. Each has an exact software
oracle. Signed add saturates before consumers where declared. Other
Wasm integer operations have bitvector modulo-2^64 semantics.
No externally observable x86 flags or poison states occur in the WAT
contract.

## Evidence and refusals

The new scripts are:
- qualify_wasmtime_mixed_contexts_28.py: exact generated WAT, two pinned
  real Wasmtime CLI builds, finite output tests, real archived AOT files,
  disassembly, ELF .text, whole .cwasm and all actual signed byte deltas.
- independent_recheck_mixed_contexts_28.py: independently reparse every
  archived AOT binary/ELF section/disassembly, verify pins, identity
  and source hashes, recompute claimed deltas, and re-execute selected
  exports under a separately implemented independent bitvector oracle.
- test_mixed_contexts_28.py: deterministic reference/oracle agreement
  over boundary and seeded random inputs, exact source/cardinality
  controls and deliberate refusal paths.

The test is a **constructed heterogeneous microkernel screen**, not
a representative production Wasmtime application. There is no inferred
throughput, power or CPU-latency result. Finite executable checks do
not replace whole-state proof; the existing named-rule VeriISLE
qualification still assumes an unreviewed local scalar-select spec.

Persist separately: function disassembly region bytes; instruction
bytes through first return; observed ELF .text length; actual whole
compiled .cwasm length. Never extrapolate one metric into another.
The unrelated controls-only case is deliberately not assumed to show
zero cost changes.

## Execution

Targeted workflow:
.github/workflows/tier1-wasmtime-mixed-contexts-28.yml

This workflow will build pinned baseline/patched Wasmtime and execute
both finite checks and independent receipt verification on x86-64.
The exact GitHub Actions run must be inspected before reporting a pass,
measurement or regression. The current source is a checkpoint until
then. No protected ZenoFCIS branch or upstream Wasmtime branch is
changed. No upstream optimization PR is authorized by this suite alone.

**Next gate:** representative externally sourced Wasm programs,
independent authoritative select-spec qualification, and targeted
cross-target/runtime regression checks before upstream consideration.
