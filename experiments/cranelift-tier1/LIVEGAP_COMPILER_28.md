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
