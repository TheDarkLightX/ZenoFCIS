# Tier-1: correct forced-vs-unforced signed-overflow rewrite

## Triggering executable fact

The previous disposable **intentionally unsound** exact-pattern sentinel is
[now qualified by independent CI #42242608526](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42242608526).
It changed actual native disassembly and produced 12 wrong finite arithmetic
witnesses for *each* of the Rust debug and optimized Wasm modules, with
source-pinned same-Wasm original-vs-canary comparison and second-oracle replay.
This demonstrates an observable effect of the exact named matcher when
\`subsume\` forces its output. It does **not** establish any meaningful
optimization from the intentionally invalid rule.

The ordinary **semantically correct** xor-and predicate rule had previously
passed four all-64-bit-input QF_BV queries and unchanged-spec named VeriISLE,
but it produced **byte-identical native AOT objects** on those two Rust
profiles. Thus distinguish "rule cannot reach this graph" from "ordinary
equivalent rewrite is not chosen/is optimized back/is cost neutral."

## New bounded candidate experiment

Pinned source: Wasmtime b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49;
ZenoFCIS isolated branch
research/2.8-tier1-cranelift-performance-20261102;
Rust 1.97.1 -> wasm32-unknown-unknown -> Cranelift x86_64-unknown-linux.

Exact alternative palette is three compilers built from one detached source:
1. \`baseline\`: unchanged upstream (correct).
2. \`unforced\`: original equivalent named xor-and ISLE rule (correct).
3. \`forced\`: same exact left-hand side and same equivalent xor-and
   right-hand side, now wrapped by \`subsume\` (correct *only if* named rule
   VeriISLE verification independently passes).

All three compile exactly the same generated Rust Wasm within each debug and
optimized profile; do not compare unlike profiles as equal source programs.
The exact rule replacement is \`tier1_rust_llvm_i64_overflow_xorand_forced\`.

The old strict QF_BV equivalences for the two pure expressions are rerun using
the hash-pinned Z3 requirement. The *new* forced named rule is separately
verified under **unchanged upstream VeriISLE specifications**; failed,
unsupported or unknown results are not admitted as proof. Both real AOT
modules are checked against an independent arbitrary-precision signed-i64
reference and re-executed in a second checker. The producer records each
original whole \`.cwasm\`, actual ELF \`.text\` size, disassembly, object
SHA-256, source/solver/tool pins and a content-key deduplicated artifact-cost
catalog. The independent audit refuses altered binaries, catalogs, hashes,
finite receipts and cost deltas, and recalculates actual byte costs rather
than extrapolating raw instruction lengths.

This may demonstrate a forced exact-rule selection without proving the forced
form is cheaper. The result could be zero cost improvement or a regression;
the experiment must not assume that e-graph substitution yields lower x86
bytes, fewer executed instructions, faster performance, or a more
upstream-appropriate canonical form. Flag, ABI, generic poison, non-i64,
other targets, and runtime speed remain outside the admitted proof.

### Executable entry points

- \`install_overflow_forced_28.py\` (source-pinned exact closed palette)
- \`qualify_overflow_forced_28.py\` (real Rust-to-Wasm AOT three-way producer)
- \`independent_recheck_overflow_forced_28.py\` (second native/ELF/cost oracle)
- \`verify_overflow_forced_veriisle_28.py\` (named unchanged-spec proof)
- \`test_overflow_forced_28.py\` (grammar and failure controls)
- \`.github/workflows/tier1-rust-overflow-forced-28.yml\` (source-pinned CI)

### November 6 qualification state

[Targeted run #42292776837](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42292776837)
was initiated on source commit
\`26627fa770e4f74d7ddc67aea53e84b8d0ac91d1\`.
The **named forced rule passed its own VeriISLE job** with one applicable
and one verified case, zero unknown, failed and expansion errors;
[full raw verifier artifact #13539499948](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42292776837/artifacts/13539499948).
The concurrent real three-compiler AOT job was **still running at the
inspection**. Its native correctness, independent cost, catalog and full
test outcome MUST be checked independently before claiming any new saving.

This remains research-only on the separate ZenoFCIS branch. It must not
change the main or release branches, or submit/merge any Wasmtime upstream
PR. No user action required.
