# Strict live-gap reconstruction pilot: experimental compilation only

A fresh optimized Rust 1.97.1 -> Wasm fixture at pinned ZenoFCIS
`25f803b3b1a2f39320dbb49928bd713e2a1b3dd4` produced three
single-quotient native functions with the following exact dataflow in emitted
CLIF, even though no patch was applied:

```text
q = udiv.i64 x,y
m = imul q,y
c = iconst.i64 7
rot = rotl m,c
r = isub.i64 x,m
```

The original experimental compiler matches only three **adjacent**
instructions. A real live use of `m` (the `rotl`) breaks adjacency.
The new installer `install_udivrem_livegap_28.py` deliberately **does not**
attempt general instruction scheduling or arbitrary side-effect analysis:
it recognizes only the exact five-instruction same-block sequence
`udiv; imul; iconst; rotl(m,c); isub`, checks all operand edges,
and retains the multiply for its live users. It reuses the original pinned
source-installer and fails closed on changed git blobs, nonpristine checkout,
wrong source patterns, or unexpected instruction identities. Because only
`iconst` and `rotl` may intervene, the pilot never deliberately crosses a
memory operation, call, potential trap or terminator.

This is an **unproved compiler-pass experiment**, not a formal machine ISA
or full pass proof, and source location/SSA/debug and flags/liveness/target
safety beyond these specific cases remain open. There is not yet a verified
native saving for the new patch. The independent negative `wrong_multiplier`
control should not match; `live_two_chains` is intentionally not supported
by this minimal two-operation gap grammar.

## Same-Wasm A/B acceptance

Targeted workflow `.github/workflows/tier1-rust-livegap-28.yml` checks out
Wasmtime `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, verifies its
revision and unmodified status before the installer, applies the exact
source-pinned patch only to that disposable checkout, builds Wasmtime with
Rust 1.97.1, then compares it to the already-qualified pristine compiler
from producer run #42951667233. It checks compiler binary hashes, compiles
**identical** optimized Rust-generated Wasm with both compilers and exercises
all five exports using both independent arithmetic oracles and fresh native
Wasm inputs. Original and patched AOT files, emitted CLIF, disassemblies,
patch diff, executable text and AOT sizes, static x86 division counts,
source hashes and receipt are archived.

Whole-module metrics cannot establish per-function profitability or
register-allocation safety. A new difference, including in a negative
control, requires manual per-function disassembly and verification. No
upstream submission until semantic obligations and representative cost
results are independently qualified. This branch remains separate from
ZenoFCIS main, release/2.7.0 and Bytecode Alliance upstream.

## Qualified outcome (2026-11-13 UTC)

The exact two-op live-gap experimental compiler **built and passed** at
[CI #43361471035](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43361471035),
pinned ZenoFCIS source `9e918603e0dda3e04177b884280db7e58086df02`,
Wasmtime `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`,
Rust 1.97.1. The producer used the same generated optimized Rust Wasm
for baseline and patched Wasmtime, with all five exports sharing one
module. Producer checks passed 480 finite return values and 60 zero-divisor
trap/panic cases **per compiler**. The independent rechecker passed 410
additional native executions; the two differently constructed arithmetic
oracles agreed on 1,310 controls.

**Important metric correction:** original producer summary shows zero
difference in whole-AOT byte size (65,696 each), executable ELF
`.text` size (28,672 each), and module-wide `divq` count (6 each).
This does **not** mean the native functions were identical. Their AOT hashes
differ and they contain different native machine code.

Independent per-export reinspection of the **original downloaded artifact**
passed separately in [CI #43362353414](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43362353414),
using a strict hash-bound native-instruction parser and negative controls:

| Rust-generated export | Decoded x86 guest bytes | Instructions | Static `divq` |
| --- | ---: | ---: | ---: |
| `live_original_rem` | 101 → 92 (**−9**) | 31 → 28 (**−3**) | 1 → 1 |
| `live_q_m_r` | 101 → 92 (**−9**) | 31 → 28 (**−3**) | 1 → 1 |
| `live_reversed_mul` | 101 → 92 (**−9**) | 31 → 28 (**−3**) | 1 → 1 |
| `live_two_chains` | 157 → 157 | 48 → 48 | 2 → 2 |
| `wrong_multiplier` (negative control) | 104 → 104 | 32 → 32 | 1 → 1 |

These are **per-export decoded instruction bytes, including nonexecuted
trap-path instructions**, not dynamic bytes executed, throughput, native
CLIF costs, or whole-module size. Three *manually constructed* Rust source
forms do now reach the experimental compiler rewrite while retaining the
intermediate multiplication's other SSA use. The two negative controls
remain unchanged in cost. The original run's [raw AOT/CLIF/disassembly
artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43361471035/artifacts/14007454958)
and the [separate replay audit artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43362353414/artifacts/14009065903)
have independent archive SHA-256 checks.

Full contracts, exact pins, artifact digests and audit status are preserved
in [the source-controlled evidence record](evidence/rust_livegap_compiler_20261113.json).
This **does not** qualify a real-world runtime speedup, whole-pass
SSA/liveness/flag/poison/target correctness or a general-purpose verifier
result. The rule was designed manually, not synthesized by ZenoFCIS.
No upstream PR should be submitted until those remaining gates pass.
