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


## Completed source-pinned cost and semantic qualification — November 6, 2026

The exact [GitHub Actions run #42292776837](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42292776837), ZenoFCIS source **\`26627fa770e4f74d7ddc67aea53e84b8d0ac91d1\`**, **passed both jobs**. The actual unchanged-spec forced-rule VeriISLE verdict is **1 applicable / 1 passed / 0 failed / 0 unknown / 0 expansion errors**; original [proof logs #13539499948](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42292776837/artifacts/13539499948). The full native job independently completed fresh four all-input QF_BV UNSAT checks, its deliberate SAT negative control, all finite Rust-generated Wasm executions, actual ELF/complete AOT cost measurement and the independent second-oracle and artifact catalog replay; original [AOT/raw receipt artifact #13540437568](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42292776837/artifacts/13540437568) (ZIP SHA-256 \`e89f305a6306d5dcccfe1312be9b6bc51319f3ed5d0bdba29f6d3e0c27820506\`).

**Observed codegen result:** The ordinary value-equivalent xor-and rewrite still emitted **byte-identical AOT** to baseline in both Rust profiles. The explicitly \`subsume\`-forced, identically proven expression **changed native instructions and binary hashes**, while remaining correct on all executed finite witnesses. Independently recomputed full \`.cwasm\` and ELF \`.text\` sizes still **saved zero bytes**. This is a controlled counterexample to treating a verified ISLE identity as an automatically cheaper lowering.

| Rust profile | Baseline/unforced/forced complete AOT | Baseline/unforced/forced ELF .text |
|---|---|---|
| Debug | 66,016 / 66,016 / 66,016 bytes | 28,672 / 28,672 / 28,672 bytes |
| Optimized | 14,472 / 14,472 / 14,472 bytes | 4,096 / 4,096 / 4,096 bytes |

I additionally downloaded the original CI-produced archive, checked SHA-256, and decoded the retained per-function disassembly **through each function's first return instruction**. These are target code prefix lengths, **not** whole-object costs, static symbol sizes or benchmarked runtime throughput. No code was regenerated to produce this supplemental inspection.

| Function | Debug baseline → forced bytes / instructions | Optimized baseline → forced bytes / instructions |
|---|---|---|
| \`sat_add\` | 65 → 72 B / 20 → 22 | 51 → 57 B / 16 → 18 |
| \`sat_add_mix\` | 178 → 189 B / 47 → 51 | 58 → 65 B / 18 → 20 |
| \`sat_add_xor\` | 69 → 76 B / 21 → 23 | 54 → 61 B / 17 → 19 |
| **Three-function sum** | **312 → 337 B / 88 → 96** | **163 → 183 B / 51 → 57** |

The source-linked disassembly concretely shows additional materialization/zero-extension and wider \`testl\` in the forced path in the measured x86-64 output. **This candidate makes these three executed instruction prefixes longer by 25 bytes (debug) and 20 bytes (optimized)**, even though full AOT lengths are absorbed by object layout and alignment.

**Decision:** This precise equivalent predicate transformation is **not a Tier-1 code-size optimization** on these workloads. Do not submit it upstream. The canary and forced-equivalent experiments now establish that the matcher is observably reachable when forced, but forcing this mathematically equivalent rewrite selects a more costly native instruction form, not a cheaper one. Keep the valid negative receipt in the catalog, and switch attention to a genuinely target-cost-aware whole lowering/overflow predicate or another prevalent compiler idiom. The named VeriISLE proof is limited to its admitted IR semantics; none of this proves general ISA flag/ABI/poison correctness or any speed advantage. No protected or upstream branch mutation.
