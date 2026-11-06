# Cranelift Tier-1: intentionally invalid matcher-canary diagnostic

**Source-only checkpoint** (November 6, 2026). New result is not
qualified until its exact GitHub Actions run completes and the receipts
are independently inspected. There is no new cost improvement.

## Evidence motivating this diagnostic

The immediately preceding Rust/LLVM overflow-predicate experiment
[`RUST_OVERFLOW_PREDICATE_28.md`](RUST_OVERFLOW_PREDICATE_28.md)
obtained four independent 64-bit QF_BV UNSAT equivalence checks, one
successful 1/1 VeriISLE named-rule check against **unchanged upstream
specifications**, and a successful actual AOT comparison, but **zero
byte difference** for both the Rust debug and optimized source programs.

This leaves an important ambiguity: was the proposed ISLE rule
unreachable at the optimizer stage, or did a reached rule lose during
normal extraction/canonicalization? Source-level matching of a
textual CLIF dump is not an optimizer instrumentation result.

The pinned upstream optimization guidance,
`cranelift/codegen/src/opts/README.md` at
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, states that
rewrites are *directional* and that the RHS should be no worse than the
LHS. It also requires `subsume` when a RHS discards nonconstant
value-uses. Thus a formal algebraic identity by itself is insufficient
to show that the optimizer will select or benefit from a rewrite.

## Strictly diagnostic, **intentionally semantically wrong**

This experiment **must never be treated as an optimization or
submitted upstream**. Only in disposable CI-built Wasmtime compiler
copies, it replaces the RHS of the *exact same* experimental named
predicate rule with `(subsume (all_zero $I8))`.
It also builds a second, broader **also deliberately wrong** control
rewriting any `bxor $I8` to zero. Both are designed to break results,
not preserve semantics.

Three binaries, all built from the *same pinned upstream checkout*,
are compared:

| Compiler | ISLE source delta | Intended use |
|---|---|---|
| baseline | none | Correct finite-output reference |
| exact_canary | exact overflow-rule LHS, incorrect constant-zero RHS | Test observable effect of exact candidate matcher |
| broad_canary | broad i8 XOR LHS, incorrect constant-zero RHS | Calibrate whether this compiler path is sensitive to an ISLE XOR rewrite |

Both candidate programs are classified as
`untrusted-diagnostic-only-compiler-NOT-FOR-USE`. They are **not**
semantic proof candidates, must not appear in a reusable sequence
catalog, and must not be used to generate a software release.

Each compiler receives the **identical**, Rust 1.97.1-generated Wasm
binary per optimization profile, and actual AOT output is measured.
A pure signed-integer oracle verifies the original baseline while
recording, rather than treating as a test failure, any incorrect output
from the two invalid canaries. The second, independent rechecker reads
the actual AOT files, independently recomputes ELF `.text`, re-runs
disassembly, verifies all source and binary hashes, and re-executes a
separately implemented finite reference oracle.

The compiled canary compiler executables are **not uploaded** to
GitHub Actions artifacts. Only the diagnostic raw AOT files, reports,
patches and build logs are retained.

## Interpretation limits

- If the **exact** canary changes generated code and produces deliberately
  incorrect outputs, it establishes an *observable effect* of the
  exact-pattern experiment, strong evidence that matching and
  rewriting were operational on these inputs; it is **not** a full
  independent optimizer match count.
- If the broad control changes outputs but the exact canary does not,
  this narrows the issue to the specific matcher, its stage, or
  extraction. It does not prove the exact rule was never internally
  considered.
- If neither canary changes native code, the experiment is
  **inconclusive** about matcher reachability; no negative universal
  claim follows.
- Whether any canary grows or shrinks a binary is not a code-size
  optimization result. It is intentionally semantically incorrect.
- This is not a substitute for unchanged-spec VeriISLE proof, a
  source-level match counter, or a real cheaper correct target-specific
  compiler rule. It provides no speed/throughput, other-target, ABI,
  flag-liveness, or poison/undefined assurance.

## Reproduction and isolation

The targeted workflow is
`.github/workflows/tier1-overflow-matcher-canary-28.yml`.
It checks out the exact pinned Wasmtime revision, checks the original
`selects.isle` blob, runs the deterministic 64-bit test vectors and
source-refusal tests, builds the three isolated compilers, measures two
Rust-to-Wasm profiles, and independently audits emitted code and finite
results.

Source files:
`install_overflow_match_canary_28.py`,
`qualify_overflow_match_canary_28.py`,
`independent_recheck_overflow_match_canary_28.py`,
and `test_overflow_match_canary_28.py`.

Work exists only on the isolated
`research/2.8-tier1-cranelift-performance-20261102` branch. The
ZenoFCIS main/release branches and Bytecode Alliance upstream are not
modified. The existing draft PR #175 remains the research checkpoint;
no new upstream PR is recommended.

**After passing CI:** use the sensitivity result to decide whether to
instrument the actual optimizer, modify the precise matcher, or abandon
this candidate. Only return to target-cost/semantic qualification with
a **correct** replacement, using independently measured native
objects and source-pinned proof receipts.
