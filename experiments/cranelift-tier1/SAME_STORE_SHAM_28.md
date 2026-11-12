# Tier-1 Cranelift: one-Store, two-Instance timing screen

**Status:** pinned executable CI completed, and original ZIP independently audited.
This is not a ZenoFCIS-synthesized sequence or a demonstrated runtime speedup.

## Purpose and fixed inputs

Previous same-Module shams kept compiled machine code constant but used two
different Wasmtime Stores and Instances. Exact same-Guest/Store/Instance shams
reduced within-run drift, but were not a real compiler A/B. This screen
isolates the *Store* dimension: two separate Instances invoke distinct
TypedFuncs through one shared Store, sequentially (never concurrently).
The pinned Rust-generated optimized Wasm is byte-identical between the
baseline and patched compiler; the two AOT inputs and their native savings
were qualified separately in CI #42951667233.

Six new modes add baseline/patched and same-AOT negatives, each with forward
and reverse INSTANCE construction order:

- `ab_samestore`, `ab_samestore_rev`: baseline and patched Modules,
  one Store and separate Instances. Their guest-code placement may differ.
- `aa_samestore`, `aa_samestore_rev`: one baseline Module, one Store,
  separate Instances, same compiled guest code.
- `bb_samestore`, `bb_samestore_rev`: same as above using patched AOT.

Every mode runs all six Rust exports, the exact independent arithmetic oracle
and zero-divisor/trap preflight, two warmups, eight balanced ABBA/BAAB blocks
of four timing samples, and 50,000 guest calls per sample. An archived
sample includes an independently checked checksum. Same-Store experiments
do not pin executable addresses, control code alignment, eliminate timing
noise or establish compiler-pass safety. The same-Guest modes still compare
one identical function against itself, not baseline against patch.

## Reproduce and admissibility

Run `.github/workflows/tier1-rust-reconstruction-inprocess-28.yml` against
the experimental branch. It checks out pinned
`bytecodealliance/wasmtime@b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`,
hash-checks original producer CI #42951667233, compiles the in-process Rust
harness with Rust 1.97.1, tests each timing mode and runs a source-bound
independent Python checksum/receipt audit. The receipt schema advances to
`zenofcis/tier1-rust-reconstruction-call-timing/3`. Previous archived
receipts remain records under their original schema; do not rewrite them.

The experimental target remains the manually designed unsigned i64
`udiv; imul; isub` to multi-result `udivrem` reconstruction. It showed
13 fewer decoded x86-64 native instruction bytes for three Rust-generated
functions at the pinned compiler configuration, with unchanged complete
AOT size. A useful runtime benefit, formal Rust-pass correctness,
cross-target safety and a verified upstream ISLE rewrite are all unproven.

**No upstream PR or protected-branch changes are authorized by this screen.**

## Exact-head CI outcome and independent archive replay (2026-11-12 UTC)

[Run #43195381344](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43195381344) succeeded at head `882faed05ab852ec98f14ddd45417c4a13cef3a2` with pinned Wasmtime `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, Rust 1.97.1 and an AMD EPYC 7763 hosted VM. [Original artifact #13933376009](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43195381344/artifacts/13933376009) ZIP SHA-256: `decef1b6beb656c19c87d13dd6640a88803836025f29612b0ae3a6ddf79acaf0`.

Independent ZIP and arithmetic replay validated all 20 mode receipts and CSV hashes, 3,840 timed sample checksums using a separately implemented 272-input oracle, ABBA/BAAB ordering of 960 paired blocks, and 120 recomputed function/mode medians. This represents 192,000,000 timed guest invocations across six exported functions. The archive does not contain the producing compiler binaries, so this replay relies on the independently qualified earlier producer artifact for source/binary identity and static native costs. See [machine-readable independent audit](evidence/rust_samestore_independent_archive_audit_20261112.json).

| Mode | Pair | Offset | Reverse |
|---|---:|---:|---:|
| Baseline/patched, separate Stores | 1.000958 | 1.001182 | 1.018788 |
| Baseline/patched, reverse module order | 1.003782 | 1.000316 | 0.982527 |
| **Baseline/patched, same Store** | **0.998893** | **1.003020** | **1.000764** |
| **Baseline/patched, same Store, reversed Instances** | **0.998746** | **1.000248** | **1.002384** |

Values are median paired patched/baseline elapsed-time ratios: below 1 favors the patched compiler, **not necessarily causally**. The four same-Store A/A and B/B shams show largest function-level median deviations of 0.5912%, 0.2092%, 0.5025%, and 0.5574% respectively. In the separate-Store reversed A/B control, the *unchanged-native-byte* signed function's ratio reached 1.2801, a particularly strong confound.

**Conclusion:** This is qualified experimental timing data but no performance win. Previous 13-byte **per-function decoded native instruction** reductions in three narrow optimized Rust fixtures remain established only by the separate native cost audit. No whole-AOT size reduction, proven performance improvement, full compiler-pass verification, or new synthesis result. Further work should prioritize the SSA/trap/target safety case and representative workloads rather than more sham-mode accumulation; no upstream PR is justified by these timings alone.
