# Tier-1 next candidate: unsigned quotient/remainder result reuse

**Source-only experimental checkpoint.** Do **not** report a compiler optimization, throughput improvement, or successful hosted measurement before checking the exact CI result and original AOT receipts.

## Why pivot

The earlier signed-i64 overflow-xorand rewrite passed named VeriISLE under unchanged upstream specifications, but a forced correct implementation emitted **longer** native instruction prefixes on the tested Rust sources while saving **zero** complete AOT bytes. It remains a retained, independently checked negative result, not an upstream proposal.

The next candidate is grounded in an actual upstream Cranelift request: [bytecodealliance/wasmtime issue #5623](https://github.com/bytecodealliance/wasmtime/issues/5623), currently open. The issue identifies redundant generation of unsigned division and remainder for the same operands as a potential shared-result optimization. A fused multi-result IR operation may ultimately be needed; this experiment **does not** claim that implementation exists.

## Explicit bounded source-formulation palette

One pinned unchanged upstream compiler: `bytecodealliance/wasmtime@b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`. Target: x86_64-unknown-linux. Two **different** hand-authored WAT sources are compiled under identical settings, each exposing one i64 function `work(x,y)`.

- **direct:** computes unsigned quotient `q=x /u y` and remainder `r=x %u y`, returning `q XOR rotl64(r,17)`.
- **quotient_reuse:** computes one unsigned quotient `q=x /u y` then derives `r=x - q*y` with modular 64-bit operations, returning the same function.

Both **trap on divisor zero** (the division occurs first). For nonzero divisor, Euclidean division gives `x=q*y+r`, hence `r=x-q*y`; because `0<=r<y`, this is exact in 64-bit unsigned arithmetic. Signed i64 host argument strings are merely bit-pattern input/output transport: division is **unsigned**, and the returned word is printed as a signed i64.

The experiment is limited to these two source formulations. It is **not** a same-Wasm original-versus-patched Cranelift A/B, not proof that automatic optimization exists, and not a global minimum search.

## Semantic evidence

- `test_udivrem_source_palette_28.py`: exhaustively checks the unsigned 8-bit quotient/remainder identity over **65,280 pairs** with nonzero divisor, plus signed display and negative controls.
- `verify_udivrem_source_palette_28.py`: separately attempts direct Z3 QF_BV UDiv/URem equivalence at widths 8,16,64, records `sat`/`unsat`/`unknown` distinctly, and proves a width-64 modular algebraic consequence **conditional on Euclidean quotient/remainder semantics**. The script cannot turn a timeout into a proof. Divide-by-zero trap behavior is not represented by the QF_BV total-function model and is checked with actual Wasmtime separately.

## Target-relevant cost evidence

- `qualify_udivrem_source_palette_28.py`: builds both real Wasmtime AOT objects from separate WAT source files using **one unchanged pinned compiler**, finite-executes nonzero inputs, checks zero-divisor traps, archives actual binary hashes/disassembly, and measures the complete serialized `.cwasm`, ELF `.text`, static machine instruction prefix bytes, and static decoded `div` count. Deduplicated content-keys persist source/target/compiler/native hashes and qualification status.
- `independent_recheck_udivrem_source_palette_28.py`: checks source/compiler identities, independently rereads real ELF sections and native disassembly, recomputes the exact byte/cost deltas and deduplicated catalog, then executes additional signed-argument witnesses and the zero trap. It never multiplies per-function bytes into fabricated application savings.

Static `div` instruction count can indicate a candidate for future expensive instruction reuse; it is **not** a measurement of retired instructions, latency, energy, throughput or CPU speed. Reducing `div` count may increase shorter ops or register pressure. Even a source-formulation win is not sufficient for an upstream PR.

## Execution and next gate

Run targeted `.github/workflows/tier1-udivrem-source-palette-28.yml`. Source pins and actual success must be checked before any measured claim.

If this screen demonstrates a meaningful target-code distinction, next build the **same-Wasm unmodified/Cranelift-patched** compiler comparison, with an independently verified multi-result/liveness-aware IR transformation, other-target regressions and controlled runtime benchmarks. If Cranelift already merges the two or chooses equivalent code, record the negative result without speculative upstream submission.

Only the isolated `research/2.8-tier1-cranelift-performance-20261102` branch is edited. No ZenoFCIS main/release or Bytecode Alliance upstream branch is modified.
