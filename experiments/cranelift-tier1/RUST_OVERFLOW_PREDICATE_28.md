# Cranelift Tier-1: exact Rust/LLVM signed-overflow predicate experiment

**Status:** source checkpoint, not a proven emitted-code saving or an upstream
submission. Check the exact targeted Actions run before reporting executable
proof or cost results.

## Motivation and source pins

Actual Rust 1.97.1 wasm32->Cranelift emitted code, captured in
RUST_CLIF_DIAGNOSIS_28.md, uses the following graph for signed i64 addition:

    s = (x+y) modulo 2^64
    overflow = signed_lt(y,0) XOR signed_lt(s,x)
    clamp = (s >>arithmetic 63) XOR INT64_MIN
    result = select(overflow,clamp,s)

The previous three-way sign-mask pilot found no reduction in actual AOT
ELF or serialized Wasmtime size. The next experiment moves to the
**entire signed-overflow predicate**, not merely the constant sign mask.

Compiler pin: bytecodealliance/wasmtime at
b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49.
Source: cranelift/codegen/src/opts/selects.isle, original Git blob
6a6a9129f382d5cd45ae0919ae18683973b31db2.
ZenoFCIS isolated research branch:
research/2.8-tier1-cranelift-performance-20261102.

## Closed candidate vocabulary

Two compiler variants: *baseline* (no rule), *xorand* (one named
mid-end ISLE experimental rule).

    baseline: (y signed_lt 0) XOR (s signed_lt x)
    xorand:   ((s XOR x) AND (s XOR y)) signed_lt 0

Both are boolean predicates over x,y,s, with s constrained to x+y
modulo 2^64. The matcher explicitly demands that sum dependency:
**it must not rewrite an arbitrary pair of unrelated comparisons**.
An equivalent (s XOR x) AND NOT(x XOR y) alternative is independently
proved but not compiled in this first bounded test. A deliberate
incorrect mask is expected to produce a real counterexample.

This identity is tested with exhaustive **8-bit finite enumeration**
(65,536 input pairs), 64-bit boundary/seeded finite checks, and a
distinct fresh all-input **64-bit QF_BV** solver proof of both predicates
and the complete signed-saturation result against a 65-bit signed
arithmetic reference. Unsat is proof only within these admitted
total bitvector expressions. Unknown or SAT for a required equality
is a failure. We do not claim full ISA, poison, flag, ABI, or
optimizer soundness or a VeriISLE success until independently run.

## Reproducible real-target cost test

The targeted workflow
.github/workflows/tier1-rust-overflow-predicate-28.yml:
- checks out exact Wasmtime and the isolated experiment source;
- executes the finite tests and hash-pinned Z3 4.15.4 QF_BV queries;
- builds unchanged and one-rule-patched Wasmtime CLIs with identical
  Rust 1.97.1 toolchains and features;
- compiles exactly the same Rust-generated Wasm (opt 0 and opt 3)
  with each compiler, executes finite native semantic witnesses,
  and archives the actual emitted .cwasm and disassembly;
- independently remeasures both complete .cwasm and actual ELF .text,
  checks source/byte receipts and deduplicated catalog, and reexecutes
  additional Wasmtime functions with a separate oracle.

No positive delta is assumed. Native code may be identical, worse or
better. If the ISLE rule fails to build, match or prove, that is a
qualification failure, not proof of savings. The named VeriISLE
specification coverage still needs independent assessment before
any upstream proposal. The original select-spec proof remains
conditional on a local experimental spec.

The Rust source is a constructed idiom test, not a production Wasm
application. No throughput, application prevalence, power, cross-target
regression or unconditional native machine proof follows from a
positive instruction-size result.

No protected ZenoFCIS branch, release branch or Bytecode Alliance repo
is changed. Draft research PR #175 remains the existing checkpoint.
