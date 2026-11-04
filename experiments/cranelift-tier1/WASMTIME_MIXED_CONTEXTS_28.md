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


## Executed first heterogeneous result — November 4, 2026 UTC

The exact pinned [GitHub Actions run #41863329060](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41863329060)
on [source commit `aa611fed9265`](https://github.com/TheDarkLightX/ZenoFCIS/commit/aa611fed92658aa889670c5e372468448d5a9311)
**completed successfully**. The 7 source/oracle/refusal controls passed,
as did both real baseline/patch compiler builds, all finite Wasm witness
checks, the separately executed rechecker, and artifact preservation.
[Raw AOT artifacts, WAT, patch and receipts](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41863329060/artifacts/13339949452)
are saved by GitHub Actions.

**Measured *baseline minus patched*, exact identical WAT within each case:**

| Case | Emitted executable-prefix bytes | Function-region bytes | Actual ELF `.text` | Whole `.cwasm` |
|---|---:|---:|---:|---:|
| mixed32 | 52 saved | 194 saved | **4,096 saved** | **4,096 saved** |
| mixed128 | 208 saved | 758 saved | **4,096 saved** | **4,096 saved** |
| controls32 | 0 | 0 | **0** | **0** |

The full binary sizes independently recorded from the archived outputs
are `30,448 → 26,352` bytes for mixed32, `77,464 → 73,368` for
mixed128, and `22,256 → 22,256` for controls32.
The actual ELF `.text` sizes were `12,288 → 8,192`,
`36,864 → 32,768`, and `8,192 → 8,192` bytes respectively.
The equal 4 KiB savings in mixed32 and mixed128 do **not** imply constant
savings per function; page and function alignment make the whole-artifact
cost discontinuous.

The producer checked **192 output/input pairs per compiler** in each
mixed case and **96 per compiler** in the controls-only case.
The separately executed independent oracle checked an additional 48
pairs per compiler in each mixed case and 24 per compiler in controls32.
These are finite executions on selected functions, not all possible
64-bit input pairs or all exported functions.

I also downloaded the original artifact and separately invoked GNU
`readelf -SW` on all six archived `.cwasm` files, confirming the reported
ELF `.text` sizes and calculating actual whole-file byte lengths directly.
Each archived AOT object's SHA-256 agreed with the pinned producer receipt.
The independent rechecker receipt SHA-256 reference to the complete
producer record also matched. This extra inspection is a separate local
read-only source/object audit, not a new hosted test or full semantic proof.

**Interpretation:** unlike the identical-function no-footprint case,
these three constructed mixed/control modules provide positive evidence
that the rewrite can lower a real heterogeneous ELF/AOT allocation,
while leaving the unrelated-only suite's exact AOT bytes unchanged.
This remains **synthetic** and does not establish a realistic application
frequency, sustained performance advantage, or unconditional VeriISLE proof.

An additional per-function byte-selectivity script and targeted CI replay
were added after the first successful run. Their results must be inspected
separately before claiming that each unrelated control function's bytes
are unchanged in a mixed module. No upstream PR is justified yet.


## Exact emitted-code selectivity audit — November 4, 2026 UTC

The second, targeted [Actions run #41919592018](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41919592018)
at [source commit `6b61b82bbf74`](https://github.com/TheDarkLightX/ZenoFCIS/commit/6b61b82bbf74c4c0a4ec883916123468f925ebd7)
**passed all jobs**, now additionally running
`audit_mixed_code_selectivity_28.py` *on the archived real native code*.
[Full source/ELF/WAT/disassembly/independent selectivity artifacts](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41919592018/artifacts/13365943081)
are retained.

The CI-created audit ZIP includes the original binaries, generated WAT,
producer cost receipt and separate independently executed ELF/finite-result
recheck. The new offline audit rejects missing, duplicated, reordered or
corrupt per-function emitted code, checks the receipt and binary hashes,
sums every function's recorded byte lengths, then compares exact instruction
byte sequences *for every exported function*.

| Case | Signed-saturation-containing functions with changed code | Unrelated control functions with changed code | Whole `.cwasm` saved |
|---|---:|---:|---:|
| mixed32 | 16 of 16 | **0 of 16** | 4,096 B |
| mixed128 | 64 of 64 | **0 of 64** | 4,096 B |
| controls32 | 0 | **0 of 32** | 0 B |

Among the four saturation-containing function shapes, the observed
**executable-prefix savings per function** were **4, 4, 1, and 4 bytes**,
respectively. This demonstrates context-dependent *machine instruction*
cost, rather than assigning a constant four-byte cost credit to every
instance. The unrelated controls are machine-byte-identical within the
tested compiled functions, even in mixed modules where the complete
AOT files differed. This is an independently **replayed archived-byte
comparison**, not a statement that arbitrary unrelated IR always remains
unchanged.

The source-pinned compiler A/B, finite oracles, independently recomputed
ELF and full-AOT sizes, and new per-function checks all passed. There is
still no qualified runtime-speed comparison for this heterogeneous suite,
no empirical evidence of broad real-application occurrence, and no
unconditional named-rule VeriISLE proof. Do not propose the rule
upstream solely on these constructed data.
