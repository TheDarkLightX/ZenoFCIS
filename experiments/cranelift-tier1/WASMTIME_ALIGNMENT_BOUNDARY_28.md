# First Tier-1 Cranelift: x86-64 AOT alignment-boundary counterfactual

**Purpose:** determine experimentally whether ZenoFCIS's already verified
signed-i64 saturating-add mid-end ISLE candidate can produce *actual* AOT
footprint savings when a function crosses a compiler layout threshold. Do
not infer smaller applications, speed, real-world prevalence or upstream
merit from a shorter raw instruction sequence.

## Why the previous result was negative

At pinned upstream Wasmtime
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, the
[previous gap audit (successful CI #41803963706)](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41803963706)
showed that the original function's 12 fewer symbol bytes were taken up by
**inter-symbol gap bytes** for all observed adjacent function pairs (up to
1,023 pairs for 1,024 exports). The optimized source emitted fewer
instructions but **zero less** actual ELF `.text` and zero smaller
serialized `.cwasm`.

Pinned x86-64 Cranelift advertises *preferred* 32-byte function alignment
and emits alignment before each function. An instruction-byte reduction
that stays on the same alignment side may not shrink the object. A
**strictly bounded, synthetic, live computation** can test the boundary
hypothesis without changing either compiler binary between compared cases.

## Controlled counterfactual

The new `qualify_wasmtime_alignment_boundary_28.py` renders one of two
full signed-i64 function contracts:

1. **plain**: `saturate_i64(x+y)`, the original exported WAT function.
2. **xor1**: `saturate_i64(x+y) XOR 1`, the same source function with a
   **live** `i64.const 1; i64.xor` after its final select.

These are **different programs**. No bytes are compared *across* the
plain/xor1 programs as if they shared semantics. Within each program,
the original and one-rule patched compiler receive **identical, hashed
WAT source**, with the same pinned compiler revision, target triple and
settings. We compare the **baseline and candidate** for each semantic
contract separately. There are 1, 16, 256 and 1,024 unique exported
functions per case, chosen to expose both function and page boundaries.

The xor1 suffix deliberately uses a result-dependent computation that
cannot simply be dropped as dead WAT. Whether Cranelift instead changes
the rule application or lowers it differently is **empirical**, not assumed.
The experiment has no positive-saving assertion: *zero, negative and
inconclusive savings are all legitimate reported outcomes*.

The independent Python i64 oracle uses arbitrary-precision signed
saturation followed by signed two's-complement XOR. Exact fixture inputs
are the prior 12 signed-boundary pairs on the first, middle and final
export. This is **finite execution**, not all-2^128-input checking.
Separate earlier QF_BV/VeriISLE evidence for the mid-end rule remains
conditional on the provisional select semantics, and does not become an
unconditional proof by composing these finite observations. Wasm i64
bitwise operations are total; no x86 flags or host undefined behavior
are part of the user-visible WAT contract.

The producer archives every actual emitted `.cwasm`, source WAT,
disassembly, finite check, compiler/source hashes, all byte measurements,
and receipt. The **separate** `independent_recheck_alignment_boundary_28.py`
reconstructs every case from the pinned template, verifies hashes,
independently parses disassembly, calls `readelf` on every actual ELF,
recomputes both the **whole .cwasm** and **.text** sizes, and refuses
fabricated positive deltas. It never converts a function-symbol saving
into a whole-module saving by multiplication.

The benchmark also records instruction-prefix sums and symbol region
sums; these do **not** substitute for ELF/text/entire-module size.
No runtime timing is measured here. None of this recommends reducing
alignment preference or changes the upstream implementation.

## Repeatable check

Targeted workflow: `.github/workflows/tier1-wasmtime-alignment-boundary-28.yml`.
It separately builds same-config baseline and experimental Cranelift from
the pinned upstream on a GitHub x86-64 Linux runner, runs independent
negative controls, compiles all cases, checks finite semantics, audits
objects independently, and retains all artifacts. The exact workflow
run and outcome (including failures) must be inspected before claiming
the experiment succeeded.

**Potential next decision:**
- If the *xor1* case crosses a boundary and exhibits a genuine object
  saving, use the resulting code-length window as a calibrated
  *allocation-aware cost-model test*. It is still only an artificial
  example, not an upstream-ready optimization.
- If the actual ELF/text/module sizes remain unchanged, record the
  negative result and move on to a genuinely different lowering.
- In either case, prioritize finding realistic Wasm/CLIF patterns,
  authoritative select-rule proof coverage, controlled throughput and
  a verified no-regressions case before any Bytecode Alliance PR.

All changes are isolated to
`research/2.8-tier1-cranelift-performance-20261102`; no push or merge
to ZenoFCIS main/release or upstream Wasmtime.

## Executed result — November 4, 2026 UTC

The exact [GitHub Actions run #41856852872](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41856852872)
**completed successfully**, producing and independently rechecking all eight
real Wasmtime AOT comparisons. Source commit
[`f85ae333d745`](https://github.com/TheDarkLightX/ZenoFCIS/commit/f85ae333d7457905d55a122e8962e70596852eed).
The negative-control tests (five new + five previously qualified scaling
controls) passed before building source-pinned baseline and one-rule-patched
Wasmtime. Every reported variant passed finite signed-output checks.
The [retained artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41856852872/artifacts/13337816985)
contains the original emitted objects, hashed WAT, disassemblies, build
logs, all per-case measurements, independent recheck receipt and exact
rule patch.

**Measured baseline minus experimental candidate, in bytes:**

| Contract | Exported copies | Sum executable prefixes through return | Sum disassembler function-region bytes | ELF `.text` | Whole `.cwasm` |
|---|---:|---:|---:|---:|---:|
| signed saturation alone | 1 | 4 | 10 | 0 | 0 |
| signed saturation alone | 16 | 64 | 160 | 0 | 0 |
| signed saturation alone | 256 | 1,024 | 2,560 | 0 | 0 |
| signed saturation alone | 1,024 | 4,096 | 10,240 | 0 | 0 |
| saturation then XOR 1 | 1 | 4 | 14 | 0 | 0 |
| saturation then XOR 1 | 16 | 64 | 224 | 0 | 0 |
| saturation then XOR 1 | 256 | 1,024 | 3,584 | **8,192** | **8,272** |
| saturation then XOR 1 | 1,024 | 4,096 | 14,336 | **32,768** | **32,848** |

**New result:** an otherwise similar 4-byte-per-function executed-prefix
improvement produces *zero* full-object benefit in the original fixture,
but the live-XOR contract produces a **32-byte average ELF `.text`
benefit per exported function** at 256 and 1,024 copies. This is
quantized by actual layout: executable symbol bytes, function allocation
boundaries and page-aligned AOT sections do **not** have interchangeable
marginal costs. The extra 80-byte full-`.cwasm` reduction relative to
`.text` in the larger cases is an observed object delta, **not**
automatically attributable to instruction bytes.

This positively establishes an **actual AOT-footprint reduction in a
controlled synthetic program**, overcoming the narrower original
negative result. It does **not** establish the rewrite benefits ordinary
Wasm programs, is faster, or is upstream-ready. The named VeriISLE
proof's provisional `select` semantics remain an important
qualification. Do not upstream on this result alone.

**Decision:** retain both cases (including original zeros) in the
evidence catalog and make measured aligned allocation / actual module
size an explicit cost objective alongside raw machine-code bytes.
Continue with representative real programs, independent semantic proof,
regression studies across targets, and controlled speed measurement.
