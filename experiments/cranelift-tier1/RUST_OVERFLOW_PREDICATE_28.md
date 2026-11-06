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


## Qualification completed — November 6, 2026 UTC

Two exact source-pinned GitHub Actions runs completed **successfully**:

- [Real native AOT and QF_BV proof #42153504030](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42153504030), ZenoFCIS source `369d19e1b07ffb04591eee580843967cf6242e20`: QF_BV gave four `unsat` equivalence checks over the declared complete 64-bit input domain and a real `sat` negative control, Z3 4.15.4. Both original and one-rule-patched Wasmtime CLIs built from pinned upstream; independently rechecked executable finite tests, hashed objects, and byte comparisons all passed.
- [Named unchanged-spec VeriISLE #42153664266](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42153664266), experimental source `a6548bc227b05da2ae97f1ba00ac9513d353adda`: the **actual named mid-end rule** passed **1/1 applicable** VeriISLE expansion, **0 failed, 0 unknown, 0 expansion errors**, without any locally supplied or changed verifier specifications. This advances the proof state beyond the prior *different* clamp rule's provisional `select`-spec limitation. It is not a whole-backend or machine-flag/ABI/poison proof.

**Measured real costs:** both compiled Rust Wasm profiles produced byte-identical original/candidate AOT binaries and native disassemblies:

| Rust profile | Baseline/candidate complete `.cwasm` | Baseline/candidate ELF `.text` | Savings |
|---|---:|---:|---:|
| debug | 66,016 / 66,016 B | 28,672 / 28,672 B | **0 B** |
| optimized | 14,472 / 14,472 B | 4,096 / 4,096 B | **0 B** |

The compiler binaries themselves have **different** hashes (the experimental compiler was rebuilt), but their emitted objects for these inputs are identical. The result is **semantic verification without a measured optimization**. It cannot establish whether the intended rule actually matched, or whether a matched rule was later canonicalized away. A compiler-side match trace or exact post-opt CLIF audit is needed to distinguish those explanations. No throughput/latency claim or upstream submission is justified.

A durable machine-readable [source, proof, cost and artifact-hash checkpoint](evidence/rust_overflow_predicate_qualified_20261106.json) records the complete AOT SHA-256 identities, QF_BV receipt, 1/1 VeriISLE coverage, actual 0-byte deltas, independent audit, artifact ZIP hashes, and nonclaims. Original [AOT/raw proof artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42153504030/artifacts/13476297121) and [VeriISLE raw logs](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42153664266/artifacts/13476302260) remain available subject to artifact retention.

**Next direction:** avoid submitting an x86 target-size optimization until a source-pinned reachable rule is observed to change code and independently lower actual emitted cost. Trace rule application/optimizer-stage IR on the actual Rust CLIF, or pursue a distinct target-aware lowering with an explicit cost case. Preserve current honest negative catalog result.
