# UDiv/URem CLIF observation: source-pinned next gate

This is an **unmodified-compiler CLIF observability experiment**, not a proof
that an automatic optimizer is implemented. Its purpose is to determine whether
the earlier WAT program's two divisions are visible together in the printed
Cranelift IR on a **single exact pinned revision**, rather than rely on native
`divq` counts alone. Upstream issue: [bytecodealliance/wasmtime#5623](https://github.com/bytecodealliance/wasmtime/issues/5623).

- Upstream source: `bytecodealliance/wasmtime@b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.
- ZenoFCIS source: isolated `research/2.8-tier1-cranelift-performance-20261102`.
- Input: exact **direct** WAT of `qualify_udivrem_source_palette_28.py` (not
  the manually rewritten source). Native AOT and `--emit-clif` are produced
  by one unchanged pinned compiler.
- `scan_divrem_clif_28.py` conservatively records only the same ordered
  SSA operands (`udiv vX, vY` and `urem vX, vY`) in the *same printed block*,
  including both instruction lines and the raw CLIF/file hashes.
- Zero observed pairs is an admissible negative result, **not** a reason to
  invent an inferred ISLE match. The tool does not parse arbitrary CFGs or
  claim dominance, trap ordering, liveness, poison, signedness beyond these
  operation names, or cost savings.
- Source-only regression tests require correct positive and negative matching,
  refusal on upstream-pin mismatch, and refusal to overwrite receipts.

The workflow `.github/workflows/tier1-udivrem-clif-observation-28.yml`
archives its actual emitted CLIF, Wasmtime AOT object, WAT source, and the
observation JSON. Inspect the completed exact-head CI before citing any hits.

**Next:** if the emitted real CLIF contains this pattern, design and verify a
trap-preserving, SSA-dominance-aware *Cranelift* optimizer or multi-result
lowering. Any code-size claim still requires an identical-Wasm baseline-versus-
patched-compiler test, codegen replay, other-target handling and real
performance measurement. If no pair appears, diagnose the emit phase before
assuming the egraph cannot see the original pattern. No Bytecode Alliance
repository is modified by this diagnostic.

## Confirmed pinned executable outcome (2026-11-07 UTC)

**Exact-head [CI #42385675790](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42385675790) passed** both the six-test source scanner job and real unchanged pinned Wasmtime compiler build/CLIF capture. Its [raw artifact #13579406740](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42385675790/artifacts/13579406740) was independently downloaded; archive SHA-256 `44f6bef898e5bdba674957c4254ba0d90871300c7cd56b74dac90eb38c52331d` matches GitHub's declared digest.

The captured *unmodified* direct-WAT Cranelift file `wasm[0]--function[0].clif` (SHA-256 `95a0ab4f6cff3f98e1d75b4be969565a0d3f786aac6c612d0eee59f91b56d7a9`) contains this exact same-block pair:

```text
block0(v0: i64, v1: i64, v2: i64, v3: i64):
@0028                               v4 = udiv v2, v3
@002d                               v5 = urem v2, v3
```

The source-pinned scanner records **one** same-block identical-operand pair, and archived native disassembly shows **two static `divq`** instructions. This is a concrete compiler opportunity, **not** a compiler optimization. The original observation job [#42385101549](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42385101549) reported zero scanner matches because the initial lexical regex failed to admit the `@0028` CLIF source offsets. The scanner and six controls were corrected and requalified in the later exact-head run. See the [machine-readable qualification](evidence/udivrem_emitted_clif_qualified_20261107.json) with exact source pins, input/output hashes, receipts, static instruction count, and explicit proof limits.

Still required: a genuine trap-preserving SSA/CFG-aware Cranelift change, independently verified semantics, identical input-Wasm A/B against original compiler, register-liveness/cost qualification, and cross-target cases. The egraph's additive extraction cost and multi-result accounting require design attention; upstream [issue #5623](https://github.com/bytecodealliance/wasmtime/issues/5623) identifies these as unsolved implementation questions. No source branch outside this experiment was changed.
