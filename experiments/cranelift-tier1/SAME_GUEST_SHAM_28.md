# Tier-1: identical-Guest timing negative control (2026-11-12 UTC)

**Status: implementation committed; CI not yet independently qualified.** Experimental branch only. No Bytecode Alliance submission. This control investigates noisy runtime measurements, *not* new ZenoFCIS synthesis or a compiler-pass proof.

## Motivation and comparison hierarchy

The prior same-Module negative controls ([successful run #43105645305](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43105645305), [archived source-bound audit](evidence/rust_shared_module_shams_20261111.json)) kept guest code placement constant, but still compared two different `Store` and `Instance` contexts; five of 24 same-module sham function/mode medians were outside an exploratory ±2% ratio band. We need to distinguish that context effect from timing-slot/host noise.

Four additional modes extend the existing ten without altering the prior modes:

- `aa_sameguest`, `aa_sameguest_rev`: one baseline Wasmtime `TypedFunc`, `Store`, `Instance` and deserialized `Module` used for *both* timing labels.
- `bb_sameguest`, `bb_sameguest_rev`: same for the patched AOT.

In both modes a *second* `Guest` is still constructed from the same `Module` (with the usual reverse-order treatment), but its function is not invoked. This preserves much of the prior allocation/setup footprint while ensuring that the two timing slots call one and the same guest function through one and the same store. `baseline` and `patched` labels here are **logical timing slots**, not different native programs. Both slots execute the independently checked arithmetic oracle preflight, warmup and each identical-function timed batch. The balanced ABBA/BAAB measurement schedule, 50,000 calls per sample and eight blocks remain unchanged. The identical guest executes sequentially, never concurrently.

The source-bound Python auditor refuses unknown modes, verifies exact per-export input/output checksums, sample order and native code receipts, and records `same_guest_store_instance_and_typedfunc_for_both_timing_labels` in the output. Four new modes mean **14 total modes**, each with six exports, 192 samples and 9.6 million guest calls, or **2,688 samples / 134.4 million calls** in the combined run. The single-run host and raw sample archive are necessary before any numerical claim.

## Interpreting the result

If the identical-Guest sham medians stay closer to 1 than separate-instance shams measured *in the same CI run*, the residual drift may involve context or placement effects. If they remain noisy, slot order, guest state, host scheduling, CPU frequency, thermal effects, cache state and timer noise remain plausible. Neither outcome proves that these differences caused the earlier A/B effects. The ±2% band is only a diagnostic heuristic, not a significance threshold.

In particular, one guest can change state or warm its caches from earlier calls. Do not claim fixed executable addresses at all call sites, deterministic CPU-frequency control, user-visible throughput, a cross-target speedup or complete compiler correctness.

## Reproduction and qualification

The targeted workflow is [`.github/workflows/tier1-rust-reconstruction-inprocess-28.yml`](../../.github/workflows/tier1-rust-reconstruction-inprocess-28.yml). It checks out Wasmtime at `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, reuses and SHA-checks the same Rust-generated optimized Wasm and compiler/AOT pair from [CI #42951667233](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42951667233), builds the source-pinned Wasmtime runtime and runs all 14 modes with the independent auditor. Check the **exact workflow head**, steps, original artifacts and raw CSV receipts before upgrading status.

Static native-code savings remain *separate* evidence: three positive Rust functions saved 13 decoded x86-64 instruction bytes per function on one pinned compiler configuration, with unchanged whole-AOT size. A causal runtime benefit is still unproven, and this manually developed candidate is not a newly ZenoFCIS-synthesized sequence.
