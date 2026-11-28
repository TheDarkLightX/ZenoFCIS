# Scalar i64 bitselect — second CPU replay and context-matrix checkpoint (2026-11-24)

## Source pins and unchanged admission gate

- ZenoFCIS main: `4160086fb9d19f3ebe25047e69b1caa467aa74fd`.
- Machine-code WIP `3267216ee68bed6eeb541ef5e20dc3fa1e599b98`; ZAL machine-code WIP `47470843f69a8cfe5b8555818170d0a77da01955`; contract-guided research `1d641324ce8a18801f70269def68a479eaad1907`; proof composition research `b6fd7b4fd53eba519a746f8793c3d07f68915b99`. These are inspected branch tips, not claims that each was executed.
- Executable readiness remains scoped to successful bitselect [CI #45365538489](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45365538489) (ZenoFCIS `f438b716495b7e790cf454412e81bc8ababeea4e`, Wasmtime `cef75a9fea591317bb076ec04d1f1a99ccfedb63`), independently replayed catalog, and prior reports. No version-only claim.
- Wasmtime main now `d49635a71351df82f3320095801848d8f128b938` (four commits ahead of tested upstream). x64 lowering blob is still `545c79528cbb10fb1280623f23f53880b9762e49`, but x64 ABI/emit and other compiler files changed. Rebuild, do not extrapolate old emitted bytes.
- Upstream `cranelift/isle/veri/README.md` at the new pin explicitly supports named-rule verification with `--name x64 --rule ...` and documents incomplete x64 coverage. CLIF structural verification is not VeriISLE.

## Independent second-host executable replay

Reused exact original CI artifact SHA-256 `e567593075fcdbb0b83215826e63027e136c0a0c0684dd6c4d406ee836f15434`. Both archived audit scripts ran to completion on a CPU-affined Intel Xeon Platinum 8573C VM, GCC 14.2.0, with original and reversed library/config order. Independently rechecked hashes, ELF symbol sizes, instruction counts and one-bit decoded bodies; 102,197 inputs per object per run, four objects, two runs: 817,576 additional native calls, zero mismatches. Same deterministic input set as earlier audit, not 817,576 distinct test triples. No new compiler build or whole-machine proof.

Candidate/baseline elapsed-time ratios, medians of 40 paired alternating AB/BA blocks, 393,216 calls per timing measurement, percentile bootstrap 95% intervals (conditional on sampled blocks; VM and function-call overhead limits apply):

| Config / workload | Forward order | Reverse order |
|---|---|---|
| Generic independent | 0.994 [0.959,1.023] | 1.001 [0.981,1.018] |
| Generic dependent | 1.002 [0.982,1.031] | 1.008 [0.980,1.026] |
| BMI1 independent | 1.001 [0.976,1.026] | 1.022 [0.980,1.037] |
| BMI1 dependent | 0.996 [0.974,1.022] | 0.997 [0.974,1.024] |
| A/A independent | 0.986 [0.964,1.020] | 0.984 [0.962,1.022] |
| A/A dependent | 0.989 [0.961,0.998] | 1.000 [0.969,1.031] |

All candidate intervals cross 1 in both orders. No demonstrated throughput gain or regression on this host. Not a noninferiority test; A/A control also occasionally excludes 1. Code size is still 24 -> 21 bytes generic, 24 -> 23 BMI1, for this *historical single fixture*. Earlier AMD VM results remain inconclusive. No claim of universal speed, verified ABI or compiler refinement.

Portable conversation checkpoint `cranelift-bitselect-intel-replay-20261124.zip` SHA-256 `899eeac002a01fa722a64a5e707516c1f88b33051086f287cbaf45ede68c9f5c` contains original artifact, both audit scripts, native harness, raw 240 timing rows per order, receipts and manifest. Raw forward CSV SHA-256 `ce19c92af735cec20483d9bf3096d30d131867ca63c1951bc6c58f1778c9b60f`; reversed `6b786b05aec725e37a5375111ad190a1b85d61cf62d210ab8f3b6cee56f22af8`. Forward receipt `47d8203a09dc64f9cf54fa2c58ece2c8fb069bb7101af5d88a01a0f6157251bf`; reversed `6e55f09409e2640f847c6ca129801e1ccf718a9976c148617e865adbcbed13d3`. To replay, extract original nested CI zip into each of the intel-forward and intel-reverse directories before running their audit scripts.

## Next experiment committed, not yet qualified

- [Context matrix script](../bitselect_context_matrix_20261124.py), [CI workflow](../../../.github/workflows/tier1-bitselect-context-20261124.yml), workflow commit `156f485297cac1f74ff69aeaee0d7098bade349d`.
- Separates actual synthesized XOR rule from prior BMI1 ANDN hand-proposed rule. Named i64-only rule, same source blob guard, current Wasmtime pin. No changes to float, vector, i128 rules.
- 11 fixtures: plain, each operand live after select, all live, each load position, six-argument register-pressure proxy, comparison live, constant mask. 2 optimization modes x 2 BMI1 configurations x 2 compiler builds = 88 planned objects. Exact symbol bytes and instruction counts, raw disassembly/relocations, finite oracle, scoped contracts, dedup keys, partial receipts. The pressure fixture is a proxy, not a guarantee of spill behavior. Valid 8-byte loads only; no claim about invalid/trapping addresses, volatile or concurrent accesses.
- Independent named-rule VeriISLE job, using upstream documented CLI, pinned solver versions and tool hashes, explicit wall limit, positive proof-count guard and separate unknown/failure counters. Missing expansions/specs must be examined even if CI passes.
- No current-source matrix or named-rule proof result claimed at commit time. Local container has no Cargo and cannot resolve GitHub for a fresh build. CI is the executable path. Resume this workflow on the existing branch and review all context regressions before upstream submission.
- Keep [draft research PR #197](https://github.com/TheDarkLightX/ZenoFCIS/pull/197) draft. Do not submit speculative upstream changes. No main/upstream modifications.
