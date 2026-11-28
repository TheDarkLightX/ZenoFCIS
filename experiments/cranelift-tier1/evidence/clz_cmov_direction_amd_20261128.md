# CLZ reversed conditional move: AMD loop screen, 2026-11-28

Resume draft #197; no upstream submission. Main observed at 4160086fb9d19f3ebe25047e69b1caa467aa74fd. Wasmtime main and experimental checkout pinned at b738a89287bb78e51c06ac9624014bbda3c1429f, x64/lower.isle blob 545c79528cbb10fb1280623f23f53880b9762e49. inst.isle confirms cmove takes consequent then alternative, and CC.NZ maps to cmovnel. Previous narrow patch qualification remains valid only in its recorded scope.

## New evidence

Direct compiler loop run 46103120209 failed at fixture parsing, not at semantic verification: unknown opcode iadd_imm. Upstream metadata defines iadd with two SSA operands and iconst for constants (blob 1fcac42ebfb4b5a7c9220cd71ddd7688776cd05d). Fixed generator in a80227ba83c2c7a1ff69cb842873f7ac32367ff0; run 46130416457 in progress at inspection. No compiler loop results claimed.

Independently rebuilt and executed the previous six hand-assembled loop variants on AMD EPYC 9V74: 960 oracle cases, 320 randomized blocks, zero mismatches. Narrow XOR single-destination independent inputs +29.89% / +29.59% relative to baseline across forward/reverse layouts; narrow + reset -40.84% / -41.12%. Four-destination narrow -5.54% / -5.64%. This reproduces the single-destination performance concern on a second processor family in this loop harness, despite earlier indirect-call results being inconclusive on AMD.

New candidate (human-proposed, not a native ZenoFCIS machine synthesis result): keep BSR destination separate from final output by reversing the conditional move. For i64, e.g.:
    mov edx, 127
    bsr r8, rcx
    cmovnz edx, r8d
    xor edx, 63
The scan index is never overwritten by CMOV or XOR. Its false destination dependency may now be shorter. With zero input, ZF=1 and arbitrary BSR output is discarded. With nonzero input the index is in 0..63 and 63 XOR idx = 63 - idx. 32-bit writes zero-extend; flags and scratch registers are dead after the sequence. For i32 analogous sentinel 63, mask 31. Independent finite semantic quotient check passed 98 classes across widths, zero-sentinel mutant failed at each width. No SMT/VeriISLE or full ISA/ABI proof claimed for this new candidate.

Extended the same loop screen to eight variants (adds narrow_swap and narrow_swap_reset), freshly assembled with GCC; 1280 native oracle cases passed, 320 randomized blocks, 2,097,152 CLZ operations per variant per timed block. AMD results (median elapsed time change versus original, forward / reverse layouts):
| i64 variant | independent loop bytes (both layouts) | independent single | independent four | dependent single | dependent four |
|---|---:|---:|---:|---:|---:|
| Original | 168 | 0 | 0 | 0 | 0 |
| Narrow XOR | 144 | +29.65 / +29.69 | -5.61 / -5.77 | -0.16 / +1.03 | -0.23 / +0.28 |
| Narrow + reset | 156 | -40.89 / -40.39 | -11.58 / -11.98 | +0.24 / -0.67 | -0.24 / -0.41 |
| Reversed CMOV | 140 | -27.40 / -27.74 | -6.49 / -7.61 | -0.14 / +1.05 | -0.16 / -0.53 |
| Reversed + reset | 152 | -40.91 / -39.64 | -11.69 / -11.52 | -0.44 / +1.02 | -0.34 / -0.76 |

Reversed CMOV independent single bootstrap95 intervals [-27.82,-27.04] / [-29.52,-27.00]; independent four [-7.01,-5.88] / [-11.16,-5.81]. Dependent intervals include zero. Independent loop symbol sizes include the loop scaffold; the extra 4-byte savings depends on physical register choices and is not claimed for actual Cranelift output. Virtualized single CPU, wall-clock measurements; no production throughput guarantee. No Intel run of reversed CMOV yet.

Replay bundle (attached to 2026-11-28 research response): clz-cmov-direction-amd-20261128.zip SHA256 87bddadc16890e8f3bdc2e0d1672f926bf1a72f956ca972a0fb61e7b73b929e5; contains both replay.py scripts, raw timing JSON, native shared objects and disassembly, CPU and GCC pins, quotient.py and receipt. New compiler A/B/C workflow b9c8271e48ebb2baa50b6b6eda3c5b6766b3b1f5 compares baseline, narrow XOR, reversed CMOV, keeps previous patch SHA check, records new patch hash and native oracle results. It changes only i64 CMOV direction relative to the narrow patch; i32 remains the same XOR control. Execution not confirmed. Follow up on both workflows before any new duplicate experiments. If actual emitted register assignment defeats separation or Intel results regress, retain negative evidence and do not submit. No main mutation or upstream merge.
