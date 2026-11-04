# Tier-1 Wasmtime: portable-target in-process A/B (2026-11-04)

## Why the first in-process experiment failed

- Original [run 41734044291](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41734044291) compiled the runtime harness and successfully rechecked the archived input hashes and six refusal tests.
- Wasmtime refused to deserialize the **baseline** AOT module before any native guest timing, with `compilation setting "has_avx512bitalg" is enabled, but not available on the host`.
- The original AOT binaries were **host-ISA-specialized** and had been compiled on a different GitHub runner. They were not portable across heterogeneous x86-64 runner CPUs.
- Follow-up [diagnostic run 41765733742](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41765733742) captured the precise engine safety check. **Do not force-enable AVX512 or bypass it.** No runtime speed claim from either failed run is justified.

## Corrected experiment

A new single-job pipeline `.github/workflows/tier1-wasmtime-portable-inprocess-28.yml` checks out a **fixed upstream revision** `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, builds a baseline Wasmtime CLI and a copy differing solely by the ISLE rule installed by the existing source-guarded script. It compiles the **same checked-in Wasm source** with the explicit `--target x86_64-unknown-linux`, preventing host-specific AVX512 auto-selection. The two new AOT artifacts are not interchangeable with the earlier, specialized outputs: they have their own hash and cost receipts.

The producer independently checks the Wasm results on the bounded signed arithmetic and guest-loop corpus, captures exact CLI binary/source/toolchain identities, records post-optimization CLIF file hashes (which may differ after a successful rewrite) and records function symbol bytes, executable prefix bytes, AOT ELF `.text`, whole `.cwasm` file bytes, and disassembly hashes. These are **different metrics** and must remain separate.

The same CI job then compiles a *runtime-only* Wasmtime harness using the pinned source and its archived generated Cargo.lock, deserializes those exact newly produced portable modules, and checks 1,036 arithmetic pairs and eight loop counts **per implementation** against an independent i128 oracle before any timing. The benchmark runs paired ABBA/BAAB blocks in long-lived runtime instances, with checksums and no logging in timed blocks.

A second Python qualifier recomputes artifact and disassembly hashes, reinterprets disassembly independent of the producer, checks per-function and whole-module cost deltas, and validates timing shape/correctness. It preserves every sample and reports first-half, second-half and overall ratios. A successful run is an **exploratory one-host workload timing**, not a hardware-isolated instruction-throughput result or a generally applicable speedup claim.

## Remaining acceptance gates

Portable ISA compatibility, actual runtime execution, full timing/cost receipts and conditional-proof limitation require checked hosted logs before the experiment is considered complete. Maintain the narrow known scope; production suitability would also require independent select-semantics refinement, verified generic operands/poison and regression tests on representative Wasm workloads, further targets, and controlled performance measurements.

Neither workflow changes an upstream branch or ZenoFCIS main. No upstream PR is opened. The prior draft [ZenoFCIS PR #175](https://github.com/TheDarkLightX/ZenoFCIS/pull/175) retains its older experiment context. If the portable workflow fails, inspect its exact failure before modifying or expanding claims.

### Source-evidence correction

The first portable attempt [run 41766350827](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41766350827) rebuilt both CLI binaries and passed finite Wasm oracle checks, but its Python qualifier wrongly treated byte-identical **post-optimization emitted CLIF** as a prerequisite. This is not an input equivalence condition: a correct mid-end ISLE rule may change emitted CLIF while the source Wasm, target, toolchain and configuration remain identical. The revised qualifier preserves both CLIF sets, independently rehashes every emitted CLIF file, checks the equality boolean for consistency, and does not demand identical optimized output. This is not evidence of a semantic mismatch.

### Runtime configuration compatibility

The second portable run [41766982512](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41766982512) passed baseline/patched portable compilation and finite Wasm CLI checks, then built the runtime-only harness. Deserialization was correctly refused because the CLI producer enabled *concurrency support* while the minimal Rust harness lacked that feature. In the pinned Wasmtime source, the `concurrency_support` tunable defaults to `cfg!(feature = "component-model-async")`. The harness now requests precisely that supported runtime capability, without enabling an extra Wasm compiler or bypassing compatibility checks. This is a Wasmtime configuration matter, not a signed-add counterexample or evidence of a speedup.
