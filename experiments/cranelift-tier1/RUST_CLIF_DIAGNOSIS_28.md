# Tier-1 Cranelift: investigate LLVM-produced Wasm and exact CLIF reachability

**Status: diagnostic instrumentation committed; do not claim new results until
[Actions run #42004612090](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42004612090)
completes and its receipts are inspected.**

## Why this is the current priority

The first candidate, `tier1_i64_signed_saturation_clamp`, was checked on
bounded ZenoFCIS machine-code candidates, independently proved as a pure
bitvector clamp identity, exercised with real single-rule-patched Cranelift,
and qualified with native and WebAssembly outputs. A pinned named VeriISLE
check passed one rule expansion only with a provisional, locally authored
scalar `select` specification; it is not unconditional upstream verification.

Positive code-size evidence is limited to deliberately constructed sources,
including heterogeneous integer microkernels (4 KiB fewer whole AOT bytes).
By contrast, two real source-backed negative screens passed:

- 12 pinned, unchanged Wasmtime upstream WAT fixtures: all native AOT byte
  pairs identical ([CI #41963407858](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41963407858)).
- A pinned Rust 1.97.1 source using `i64::saturating_add` and three derived
  operations, compiled to LLVM-produced wasm32 Wasm at opt 0 and opt 3:
  baseline/one-rule-patched Wasmtime AOT identical in both profiles
  ([CI #41964211060](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41964211060)).

Neither negative screen establishes that the ISLE rule was never evaluated,
nor proves non-prevalence of the pattern in real applications. They do
demonstrate the lack of any observable native emitted-code benefit there.

## New source-pinned experiment

`diagnose_rust_clif_shape_28.py` reuses the exact *already-qualified* Rust
Wasm artifact and AOT producer receipt, checks the pinned Wasmtime source,
both compiler binary identities, Rust source and hashes, and reruns
`wasmtime compile --emit-clif` for each baseline/patched compiler and
each of the Rust opt 0/3 profiles. It captures all emitted textual Cranelift
IR files and exact hashes, counts/locates selected operators for inspection,
and records whether corresponding baseline/patch IR files are byte-identical.

A separate **positive codegen control** captures Cranelift IR using the
previously pinned, hand-authored Zeno Wasm saturating-add WAT. This control
is synthetic and does not establish occurrence in LLVM-produced Wasm.

`test_diagnose_rust_clif_28.py` provides parser checks against comment
tokens, substring false positives, empty IR and unbounded examples. The
targeted existing Rust-idiom CI workflow now checks these tests, independently
requalifies the earlier real AOT binary results, runs the CLIF capture, and
archives both the old and new receipts and the exact dumped IR.

**Crucial assurance boundary:** textual occurrence or absence of `select`,
`icmp` or an overflow op does not amount to an ISLE pattern match proof.
CLIF may be emitted before the stage applying the candidate rewrite.
Absence of printed tokens cannot establish a general compiler-rewrite
unreachability theorem. A compiler-side match counter, exact optimization
trace, or an independently validated structural IR matcher will be needed
for that stronger claim.

## Follow-up acceptance

If the new diagnostic CI passes, inspect the archived exact IR for each
Rust export and profile, compare its dependency graph to the rule's LHS,
and identify whether to narrow, generalize or replace this candidate.
A wider rule must be proved under actual scalar `select` semantics and
tested against cross-target, undefined/poison, flag/ABI and real program
performance boundaries. No speculative upstream PR.

All work remains on the separate
`research/2.8-tier1-cranelift-performance-20261102` branch. Neither
ZenoFCIS main/release nor the Bytecode Alliance upstream branch was edited.
