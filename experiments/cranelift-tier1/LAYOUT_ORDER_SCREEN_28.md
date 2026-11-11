# Tier-1: module-construction-order counterbalance for same-Wasm timing

**Status: experimental source checkpoint; CI results must be checked.** This
experiment extends, without reinterpreting, the independently qualified
[three-mode timing receipt](evidence/rust_reconstruction_sham_timing_20261111.json).
Pinned compiler producer: Wasmtime/Cranelift
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, optimized Rust
1.97.1-generated Wasm, identical input AOT pair from
[successful #42951667233](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42951667233).
The unmodified producer, oracle, AOT hashes, and native byte-count baseline
are checked before each timing run.

## Why

The earlier A/B, baseline/baseline and patched/patched sham measurements
differed by roughly 2–3% on the same CI host, including **identical** AOT
files. Therefore run-order balancing alone does not isolate compiler code
quality from guest instance placement. We must test whether reversing which
module is *loaded first* changes the measured direction.

## Bounded method and falsifiable controls

The same Rust timing binary now accepts six modes:

| Mode | Left measured slot | Right measured slot | Creation order |
| --- | --- | --- | --- |
| `ab` | baseline | patched | left, right |
| `ab_rev` | baseline | patched | right, left |
| `aa` | baseline | baseline | left, right |
| `aa_rev` | baseline | baseline | right, left |
| `bb` | patched | patched | left, right |
| `bb_rev` | patched | patched | right, left |

"Left" and "right" are logical timing slots, **not native memory addresses**.
Each guest module is separately deserialized and instantiated; both live
through all 8 balanced ABBA/BAAB blocks per exported function. Within each
mode are 50,000 calls per sample, 6 functions, 32 samples per function:
192 timing samples per mode, 1,152 total samples across all six modes.
Each function is separately checked with finite arithmetic oracles and trap
negative cases. The independent Python auditor rejects incorrect mode
metadata, source/compiler hashes, missing or rearranged balanced samples,
wrong checksums, non-matching source costs, and malformed timing budgets.
Its receipt binds the Rust timing-harness source SHA-256 and records the
actual construction order and all block ratios.

The paired comparison is descriptive. In the result, compare `ab` versus
`ab_rev`, and compare both with the four shams `aa`, `aa_rev`, `bb`,
and `bb_rev` for the **same function**. Do not select a winner from a
favorable ordering; if the sign changes, flag layout sensitivity. If it
does not, shams and host variability still prevent claiming causality without
fixed code-address alignment, several CPU models, uncertainty analysis,
and representative real workloads.

## Reproduction and scope

Targeted workflow:
`.github/workflows/tier1-rust-reconstruction-inprocess-28.yml`, which
downloads the exact prior producer, confirms its pins, builds the same
source-pinned in-process Wasmtime runtime, then archives six raw CSVs,
six independently audited JSON receipts, Cargo.lock and host metadata.
Historical three-mode receipts remain intact. The target is Linux x86-64,
unsigned i64 Rust-to-Wasm quotient/remainder reconstruction with 3 positive
and 3 negative controls. Signed MIN/-1 and zero denominators are tested
as traps, not as defined arithmetic results. This experiment does not
prove arbitrary Cranelift pass correctness, native flags/liveness/poison
semantics, portable speedup, or savings in entire AOT module bytes.
The optimizer rewrite remains manually designed, not synthesized by ZenoFCIS.

Changes are restricted to the existing isolated research branch. No
Bytecode Alliance upstream patch or main/2.7 release mutation.
