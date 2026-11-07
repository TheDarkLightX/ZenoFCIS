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
