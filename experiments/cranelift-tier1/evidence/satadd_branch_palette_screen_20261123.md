# Tier-1 signed-i64 satadd: bounded branch/bit palette (2026-11-23)

**Status: isolated research and negative throughput screen.** This is *manually authored/assembled* x86-64 code, not ZenoFCIS synthesis, not a CLIF/ISLE lowering, and not an independently established Cranelift improvement. It extends [research PR #195](https://github.com/TheDarkLightX/ZenoFCIS/pull/195) without modifying Wasmtime or ZenoFCIS `main`.

## Source / cost pins

- Wasmtime `fb792bf7a143801c22c0ba5b16fe2c6085a8598b`, `selects.isle` Git blob `5ec428ceefedfc89c49392875a3fad7299a5f884`.
- ZenoFCIS `main` `4160086fb9d19f3ebe25047e69b1caa467aa74fd`; this research branch inherits the exact-clif compiler A/B experiment from [CI #45107263287](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45107263287).
- Exact compiler object SHA-256s: pristine `5179f3985e49772b4656ca261e64eb0d8ba57802b78abc9c1ee02d2d7cf03268`; single-rule experiment `06e9bb768ebb06a1067b1a1bb23bcb48e5bfbcd04e4281db07332e22d501b960`.
- Independent local host: Linux x86-64 SysV, AMD EPYC 9V74 KVM guest; GNU assembler/GCC 14.2.0; Z3 C library 4.13.3.0.
- Cost is actual instruction bytes **through RET**, including push/mov frame prologue and mov/pop/ret epilogue: compiler pristine **46 bytes** (ELF symbol **56 bytes**), compiler experimental **39 bytes** (ELF symbol **39 bytes**). The new eight *manual* alternatives are **37, 39, 34, 32, 39, 39, 40, 37 bytes**, respectively. These are **not** before/after compiler outputs.

## Newly checked 32-byte manual candidate

Assembly below is GNU x86-64 Intel syntax. Preserve the SysV callee-saved registers, take signed `i64` operands in RDI/RSI, and return signed saturating addition in RAX. No non-stack memory access. Only the returned integer and ABI-preserved state are in contract; x86 flags at return are **not** preserved or compared, and `btc` leaves several flags undefined. `jno` consumes the preceding ADD overflow flag before any flag-writing instruction intervenes.

```asm
.intel_syntax noprefix
.text
.globl "%tier1_signed_satadd"
.type "%tier1_signed_satadd", @function
"%tier1_signed_satadd":
    push rbp
    mov rbp,rsp
    mov rax,rdi
    add rax,rsi
    jno done
    sar rdi,63
    not rdi
    btc rdi,63
    mov rax,rdi
done:
    mov rsp,rbp
    pop rbp
    ret
.size "%tier1_signed_satadd", .-"%tier1_signed_satadd"
.section .note.GNU-stack,"",@progbits
```

SHA-256 of this candidate GAS object: `770f9f362d1bee66dc4d43025c565a67123217ec0b19f50b143696f9cede6e9a`. A second, **34-byte** branch candidate uses `sar rdi,63; movabs rax,INT64_MAX; sub rax,rdi` in the taken-overflow block; its object SHA-256 is `55c95d08fcf0178dce1c13681dbaf4da2575067a4bf939e31e423403f5d700ad`.

## Semantic and executable qualification

- **Conditional full-input QF_BV result:** Z3 4.13.3 returned `unsat` for disjunction of inequalities between the hand-modeled 32-byte and 34-byte functions and mathematical saturation over all `2^128` input pairs. SMT text SHA-256: `ad3a3baed1b96cebe13878db9370139b85397718afdc00516343ddae2ab9e99f`.
- **Negative control:** omitting the `btc` bit toggle produces a `sat` counterexample. Mutant SMT text SHA-256: `7e65999155d810a71a2d29b4d5d29c6635b88760dfa42eb51f4b821cb39b7e1a`.
- Executable A/B checks compared 32,768 deterministic inputs per workload across four workloads against independent C `__int128` saturation, for every tested alternative; sampled finite native execution is **not** exhaustive native-machine proof.
- This remains hand-modeled return-value equivalence and exact recognized disassembly, **not** formal correspondence of arbitrary x86 binaries to SMT, flag/ABI/poison equivalence in a compiler's full lowering chain, or a compiler-produced optimization.

## Single-host throughput: no profitable result

Pinned compiler-pristine time / 32-byte manual-candidate time (less than 1 = manual candidate slower). Each of the two orderings uses 24 paired blocks, 32,768 input pairs per profile and 20 repetitions per block, on one affined CPU.

| Workload | Pristine first | Candidate first |
|---|---:|---:|
| No overflow (signed 32-bit inputs) | 0.884 | 0.858 |
| Positive overflow | 0.895 | 0.895 |
| Negative overflow | 0.897 | 0.864 |
| Uniform signed 64-bit | 0.784 | 0.723 |

The 34-byte alternative likewise showed no improvement: no-overflow `0.893 / 0.885`, positive overflow `0.880 / 0.900`, negative overflow `0.903 / 0.889`, mixed `0.712 / 0.747`. An identical pristine-vs-pristine control ranged `0.962–1.023`. Time ratios include dynamic function-pointer overhead and virtual-host noise; bootstrap intervals describe within-run paired blocks, not independent hosts. No whole-Wasmtime, branch-profile-general, or architecture-wide throughput claim is supported.

**Decision:** Archive the shorter native pattern as a correctly checked *negative* profitability result; do not propose an upstream optimization on byte counts alone. The already compiled 56→39 symbol-byte ISLE trial is also currently slower in its one-host throughput screen. Future work should search under a joint size/throughput target cost and recompile *identical CLIF* with any resulting compiler rule before promotion. The independent script, raw CSVs, exact ELF objects, SMT text and machine-readable receipts are in the separately exported local audit checkpoint (the GitHub research record does not claim those local raw files were committed). No Bytecode Alliance submission/comment was made.
