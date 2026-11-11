# Tier-1: same-module code-sharing sham controls (November 11, 2026)

**Status:** new independent diagnostic source checkpoint, not a qualified result until its exact-head Actions job and archived receipts pass. Branch `research/2.8-tier1-cranelift-performance-20261102`. Keep all historical benchmark samples/receipts unchanged.

## Motivation and falsifiable experiment

The successful [order-reversal qualification #43047879710](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43047879710) demonstrated that swapping which independently deserialized guest module is created first can reverse A/B and A/A timing direction. The earlier A/A and B/B shams each deserialized **two copies** of the same AOT file. Thus they did not hold compiled guest code placement fixed.

Run the original six comparison modes unchanged in meaning, then four additional modes:
- `aa_shared` and `aa_shared_rev`: two independent Store/Instance contexts from **one deserialized baseline Module**, built in opposite instance creation orders.
- `bb_shared` and `bb_shared_rev`: analogous patched-module checks.

For the new shams both logical slots must reference the *same Module object* and compiled guest code allocation, not just have identical on-disk bytes. The benchmark source implements this by constructing one Module and passing `&module` twice to `Guest::from_module`. Guest Stores, Instances, VMContexts and Wasmtime call infrastructure still differ. No memory-address / trampoline equivalence or complete layout control is claimed.

The ten modes retain the **same** independently source-hash-qualified producer #42951667233, Wasmtime commit `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, Rust 1.97.1, identical optimized Rust-generated Wasm per compiler, finite arithmetic/trap preflight, 50,000 timed calls/sample and 8 ABBA/BAAB blocks per export. Each mode has six exports and 192 samples. All ten require 1,920 samples and 96 million timed guest calls. The source and independent Python auditor now use receipt schema `zenofcis/tier1-rust-reconstruction-call-timing/2` to avoid treating fresh controls as old `/1` evidence.

## Interpret outcomes conservatively

If same-module A/A and B/B controls show markedly reduced slot drift compared with independently deserialized A/A and B/B, that supports **a placement confound hypothesis**, but does not by itself establish a patched-code speedup. If same-module shams still show comparable drift, Wasmtime call path, Store/Instance state, host jitter or other placement differences remain plausible. In either case do **not** compare the original and patched symbols as if they shared a code address. Do not infer target-wide throughput, ISA/pass verification, or an application benefit from this microbenchmark.

The independent auditor checks all ten exact mode labels, order positions, arithmetic checksums, original producer/AOT/tool hashes, native instruction-byte negative controls, and each emitted CSV's SHA. The workflow archives raw per-mode samples and receipts. A future report must cite an actual completed CI run and downloaded/inspected artifacts. No upstream submission is justified by this diagnostic alone.
