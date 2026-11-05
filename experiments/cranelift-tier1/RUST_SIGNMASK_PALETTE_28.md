# Tier-1: measured-cost follow-up for the *observed* Rust/LLVM clamp

**Status: source-only checkpoint until the targeted CI returns an independently
checked result. This is not an upstream performance submission.**

## Motivation

The earlier synthetic signed i64 saturating-add select rewrite can reduce
native function size and in controlled alignment-threshold cases real AOT
footprint. However, the 12 unmodified upstream WAT fixtures and the tested
idiomatic Rust i64 saturating_add examples produced no realized improvement.

The independently captured Rust-generated Cranelift IR uses:

    sum      = modular_i64_add(x, y)
    overflow = signed_lt(y, 0) XOR signed_lt(sum, x)
    clamp    = arithmetic_shift_right(sum, 63) XOR INT64_MIN
    result   = select(overflow, clamp, sum)

The actual CLIF trace and all-input value semantics were qualified earlier
in RUST_CLIF_DIAGNOSIS_28.md and source-pinned CI #42005719011.

This checkpoint now focuses on a *narrow, bounded, human-proposed*
candidate vocabulary for the **clamp subexpression actually present** in
LLVM-generated CLIF, rather than expanding an irrelevant synthetic rule.
The alternatives are not claimed to have been discovered by a machine-code
search or to be cheaper before actual compiler output measurement.

## Three closed candidates

For any unrestricted 64-bit bitvector s, with two's-complement arithmetic:

1. original: (s arithmetic-shift-right 63) XOR INT64_MIN
2. add:      (s arithmetic-shift-right 63) ADD INT64_MIN, modulo 2^64
3. sub:      INT64_MIN SUB (s logical-shift-right 63), modulo 2^64

All three produce INT64_MAX if the sign bit is 1 and INT64_MIN otherwise.
The pre-existing Rust overflow condition then chooses this clamp only when
overflow is true, preserving the original Rust saturated sum contract.

New source-backed scripts:

- install_rust_signmask_palette_28.py: change only the precisely pinned
  selects.isle of detached Wasmtime at
  b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49, building *independent*
  original/add/sub compiler binaries.
- verify_rust_signmask_palette_28.py: independently ask Z3 4.15.4/QF_BV
  whether the exact 64-bit expression differs for any input; require UNSAT
  for add and sub and SAT for a deliberate wrong variant. Each candidate
  is independently checked; no solver unknown is a pass.
- qualify_rust_signmask_palette_28.py: pin exact Rust 1.97.1 source, emit
  wasm32-unknown-unknown at opt 0 and opt 3, run the same Wasm binary through
  each of the three source-pinned Wasmtime compilers, finite-check all five
  exported functions, and store complete .cwasm and ELF .text bytes,
  disassemblies and per-profile baseline-relative native cost deltas.
  It records a content-keyed deduplicated *AOT* artifact catalog, not a
  new globally optimal library of machine-code instruction sequences.
- independent_recheck_rust_signmask_palette_28.py: separately examine every
  full compiled AOT binary/ELF section and native disassembly, recompute all
  recorded signed cost differences, re-execute additional real Wasmtime
  inputs against a separately implemented arbitrary-precision reference,
  and refuse omitted/substituted artifacts, source pins, false savings or
  duplicate catalog rows.
- test_rust_signmask_palette_28.py: no-dependency boundary and bounded
  vocabulary refusal tests.

Targeted workflow: .github/workflows/tier1-rust-signmask-palette-28.yml.

## Limits and decision rule

A pure i64 bitvector theorem is **not** a proof of an actual compiler rule,
native ABI or flags, Cranelift poison/undefined semantics or arbitrary
optimizer interactions. This experiment changes the signed-clamp expression
in a specific experimental mid-end ISLE rewrite; it does not change the
result selection itself, but still requires actual named-rule verification
and evidence of application/matcher reachability before upstream.

Each profile's *identical* LLVM-produced Wasm is compiled under baseline
and candidate compilers. Never compare different Rust opt profiles as
though they were the same module. A smaller disassembly does not imply
smaller AOT; positive, zero or negative ELF and complete file size deltas
are all legitimate findings. Native performance, energy, real application
prevalence and cross-target cost are not measured by this job.

There is no assumption that add or sub will beat the existing original
CLIF optimizer or register allocator. If both produce identical AOT,
record an explicit negative result; do not widen the patch speculatively.
Only advance to a candidate upstream review after authoritatively qualified
ISLE semantics, real repeatable savings on relevant programs, cross-target
regression checks and performance tradeoff evaluation.

All changes are on isolated
research/2.8-tier1-cranelift-performance-20261102.
Do not merge to ZenoFCIS main, retarget release/2.7, or push to Bytecode
Alliance Wasmtime. The existing draft research PR #175 is the ledger.
