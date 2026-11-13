# Tier-1 adjacent `udiv` + `urem` → dual-result division (2026-11-13)

**Status:** a reproducible, narrowly measured compiler experiment, **not an upstream-qualified rewrite** or a new machine-code synthesis discovery.

The existing [Cranelift issue #5623](https://github.com/bytecodealliance/wasmtime/issues/5623) explicitly identifies the redundant pair: on x86-64 one `div` produces both quotient and remainder, but separate CLIF `udiv` and `urem` lower to two divisions. The [pinned upstream filetest](https://github.com/bytecodealliance/wasmtime/blob/b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49/cranelift/filetests/filetests/isa/x64/udivrem.clif) reproduces this behavior. Its filename does **not** mean `udivrem` was already an upstream CLIF opcode.

## Exact run and qualification

- ZenoFCIS: `a96e6bec01c075436fb7a71fb99ba41e54732ffd`, isolated `research/2.8-tier1-cranelift-performance-20261102`.
- Baseline Wasmtime: `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, pristine before applying the experimental patch. Upstream main was also inspected separately at `68f6475eac03b53a863c3e9c83ec587265fc70a4`.
- Rust `1.97.1`; x86-64; two independently built `clif-util` binaries; identical source CLIF; `opt_level=speed_and_size`.
- [Passing CI #43457217650](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43457217650) and [original compiler, object, disassembly and oracle artifacts](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43457217650/artifacts/14053864852) (SHA-256 `bdce437ea0506e0637195f6a7684badc1d98daa8b84378bf6d5cf20fb199b269`).
- Machine-readable checkpoint: [`evidence/udivrem_pair_clif_qualified_20261113.json`](evidence/udivrem_pair_clif_qualified_20261113.json).

For the single positive unsigned-i64 CLIF case, emitted native code is **36 → 21 decoded instruction bytes**, **14 → 9 decoded instructions**, and **2 → 1 x86-64 `div` instructions**. This is not an AOT-size reduction, a throughput measurement, or a CLIF-level cost model. The alternative is a *human-authored, bounded two-form palette*, implemented using a temporary experimental multi-result opcode and x64 lowering. It is not a newly discovered globally minimal sequence.

The CI verified 65,280 exhaustive **8-bit** nonzero-divisor input pairs and modeled 256 divisor-zero traps in its independent bit-serial reference. The two actual native object sets were linked into a separate GCC-compiled C oracle: **243,339 sampled 64-bit value cases passed** across eight fixtures, with 189 explicitly excluded inputs (zero denominators, `0/0` in a negative fixture, or signed `INT64_MIN/-1`). This is finite execution, not a 64-bit SMT or machine-ISA proof. I independently downloaded the original successful CI ZIP and recalculated all 16 native object hashes, byte and instruction counts, and eight CLIF source hashes.

## Failed control and correction

[CI #43454406968](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43454406968) failed correctly when a purported negative test put a **pure, live `iconst`** between `udiv` and `urem`, yet the patched compiler still emitted one division instead of two. The initial strict negative-control assumption was invalid: source-level separation by a pure operation is not necessarily a durable post-optimization barrier. The actual optimizer-phase cause has **not** been traced.

This fixture is now called `exploratory_elidable_iconst_gap`, separately classified as optimizer-sensitive rather than counted as a negative or an ordinary positive. Its observed native difference is **40 → 25 decoded bytes**, **2 → 1 divisions**. Six truly dissimilar source-operand/opcode negative controls still must remain native-byte-identical or the check fails closed. The successful CI additionally requires that the primary positive case save one division and native bytes; silently changing the expectation is not sufficient.

## Reproduce

Run [`.github/workflows/tier1-udivrem-pair-clif-matrix-28.yml`](../../.github/workflows/tier1-udivrem-pair-clif-matrix-28.yml), which checks out both pinned revisions, verifies SHA-bound sources, builds baseline and patched Cranelift from the same input, emits and compiles eight CLIF graphs, independently audits native objects, and executes [`native_verify_udivrem_pair_28.py`](native_verify_udivrem_pair_28.py) against a separate GCC C reference. The artifact includes `matrix-receipt.json`, `independent-native-receipt.json`, compiler hashes, all CLIF files and both sets of disassembly.

## Gate before upstream contribution

We still need independent review of the experimental multi-result instruction and its compiler-pass integration: dominance and SSA use-def invariants, exact trap location/metadata, intermediate side effects, flags and undefined/poison behavior, register allocation and liveness, x86 code-generation contexts, other targets, and representative Rust/Wasm reachability and profitability. [VeriISLE](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/isle/veri/README.md) cannot be assumed to certify this transformation: current x64 rule coverage is limited, and an unreviewed local model is not upstream validation.

No upstream PR has been submitted. The 2.7 release branch, main, and Bytecode Alliance source were not changed by this work; only an isolated research branch contains the prototype.
