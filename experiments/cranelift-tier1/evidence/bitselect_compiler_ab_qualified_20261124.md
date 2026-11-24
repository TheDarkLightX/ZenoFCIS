# Qualified scalar i64 bitselect compiler A/B — 2026-11-24

Research only; no upstream submission or main/release changes.

## Exact executable evidence

- [Successful CI #45365538489](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45365538489), job 138346165995: bounded synthesis, replay, independent proof, pristine and patched CLIF verifier/build, native oracle, artifact upload all succeeded.
- ZenoFCIS tested commit `f438b716495b7e790cf454412e81bc8ababeea4e`; Wasmtime `cef75a9fea591317bb076ec04d1f1a99ccfedb63`; x64/lower.isle git blob `545c79528cbb10fb1280623f23f53880b9762e49`. On Nov 24 upstream main `b6ddf6f3213659bc9838a9006c554a232173242a` has the same lowering blob, but has not been built here.
- [Original 41-member artifact #14907240518](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45365538489/artifacts/14907240518), archive SHA-256 `e567593075fcdbb0b83215826e63027e136c0a0c0684dd6c4d406ee836f15434` independently matched. Catalog SHA-256 `50f5454ac98405dfee53ce186aed7841d8fa46f0ced2e870e12273ce01d06ce4`.
- Identical CLIF, x86_64, speed_and_size, separately with BMI1 disabled/enabled. Compiler emitted complete ELF functions; no comparison against standalone kernel size.

| Configuration | Symbol / executable bytes through RET | Instructions including frame/return |
|---|---|---|
| Generic | 24 -> 21 | 10 -> 9 |
| BMI1 | 24 -> 23 | 10 -> 9 |

CI native checks: 21,728 per object, 86,912 total. Independent local replay: rehashed exact four ELF objects, independently decoded each contiguous function via objdump and nm, checked catalog content keys, linked original objects using GCC and executed 102,197 inputs per object (13^3 edges + 100,000 seeded random) in each of two runs; 817,576 total local calls, zero mismatches. Independently evaluated the actual decoded bitwise body on all eight one-bit input assignments, lifting by bit independence to all 64-bit triples under the modeled operation semantics. Frame/return, ABI/liveness, flags, decoder correctness and complete compiler refinement are not proved by that check. All inputs are defined bitvectors; no overflow or signed interpretation matters; flags dead at return; BMI1 gated. No VeriISLE run in this experiment; CLIF structural verifier is distinct.

Synthesis gate: 511 proposals, 491 concrete refutations, 18 objective-pruned, 2 solver-equivalent, grammar exhausted (not globally optimal). Registered machine profile 2 proof checks return rax under stated SysV inputs, all admitted initial registers/flags. Z3 4.15.4 independent pure-expression checks UNSAT for XOR and ANDN formulations; deliberate mutant SAT; ANDN expression proof is the direct definition, not a new synthesized ANDN machine sequence. The XOR sequence is the actual synthesized candidate. Compiler rows have three unique decoded-byte contract keys (the baseline is shared across configurations).

## Native timing: not yet a robust win

On one virtualized AMD EPYC 9V74 CPU-affined host, 40 alternating AB/BA paired blocks per configuration/mode, 393,216 function-pointer calls per measurement, same-function A/A control, independent random and result-dependent true-input chain. Median paired candidate/base time ratios (bootstrap percentile 95% intervals):

| Config / workload | Forward load order | Reverse load/config order |
|---|---|---|
| generic independent | 0.860 [0.750,0.984] | 0.830 [0.724,1.182] |
| generic dependent | 0.981 [0.969,0.999] | 0.958 [0.754,1.384] |
| BMI1 independent | 1.430 [1.192,1.469] | 1.084 [0.863,1.333] |
| BMI1 dependent | 0.718 [0.698,0.757] | 1.063 [0.812,1.480] |
| A/A independent | 0.987 [0.953,1.004] | 0.976 [0.940,1.013] |
| A/A dependent | 0.997 [0.973,1.008] | 0.990 [0.962,1.034] |

No reproducible speed advantage across both runs; BMI1 results especially unstable. Code-size measurements remain exact for this single fixture. No claim of representative workload speed, other bit widths, all contexts, CLIF cost or whole-AOT savings. Raw timings and scripts preserved in separate local checkpoint. Next: isolate generic XOR vs BMI1 (do not promote BMI1 as preferred based on this data), qualify actual emitted code in live-mask/true/false and load-folding contexts, size and speed settings, add targeted upstream filetest, check VeriISLE coverage explicitly, then decide on upstream proposal. Do not upstream on the present one-fixture result.
