# Tier-1 Cranelift: one-Store, two-Instance timing screen

**Status:** source-only experimental extension; no compiled result or speedup
is claimed until an exact-head CI run, its raw artifact and a separate audit
pass. This is not a new ZenoFCIS-synthesized instruction sequence.

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
