# Optimized Rust live-SSA reconstruction: source-to-CLIF reachability

This is a **new executable experiment**, not yet a qualified result. It
continues the LLVM-produced Wasm investigation after the five direct-WAT
live-SSA fixtures showed no modified native code in CI #43312470294.

## Exact question

Does Rust 1.97.1 at `-C opt-level=3` preserve a Cranelift-matchable
unsigned i64 graph when the quotient, multiplication product and remainder
are all independently live? A single-module five-export source distinguishes:

- `live_q_m_r`: quotient, product and reconstructed remainder live.
- `live_reversed_mul`: opposite product argument order.
- `live_original_rem`: Rust's explicit remainder expression alongside a live product.
- `live_two_chains`: two distinct wrapped numerators and both quotient/product/remainder chains.
- `wrong_multiplier`: negative control (product of q and x, **not** divisor y).

The Rust arithmetic is unsigned 64-bit, multiplication/subtraction explicitly
wrap, quotient is unsigned truncating integer division, and y=0 must trap or
panic. Target x86-64; no user-visible flags, x86 poison semantics or undefined
native behavior is asserted for these source-level expressions. The exact same
Rust-generated Wasm bytes are compiled by both original and experimental
Wasmtime binaries in a given profile. Inputs are executed against an
independent mathematical oracle, including zero divisors. Source pins,
compiler SHA-256s, Wasm hashes, emitted CLIF, actual native disassembly,
whole AOT bytes, executable .text size and static divisions are recorded.

## Reproducibility

The source is `rust_live_reconstruction_28.rs`; the harness is
`qualify_rust_live_reconstruction_28.py`. It reuses the existing pinned
`qualify_rust_divrem_28.py` native producer and
`recheck_rust_divrem_28.py` independent file/cost/fresh-input auditor
with explicit module-local source, export, profile and oracle rebindings.
Both arithmetic oracles are new, separately formulated and cross-checked.
This avoids copying or mutating old qualification receipts.

The upstream compiler source is pinned to
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, and the reconstruction
compiler pair is retrieved by `actions/download-artifact` from successful
producer run `42951667233`. Rust 1.97.1 with
`wasm32-unknown-unknown` is installed by the targeted workflow
`.github/workflows/tier1-rust-live-reconstruction-28.yml`.
The run checks the downloaded compiler pair against its archived SHA-256
qualification and fails closed on mismatches.

```sh
python3 -B experiments/cranelift-tier1/qualify_rust_live_reconstruction_28.py --self-test
# With exact archive artifact (compilers/qualification.json and both binaries):
python3 -B experiments/cranelift-tier1/qualify_rust_live_reconstruction_28.py --archive /path/to/compilers --out /tmp/live-results
python3 -B experiments/cranelift-tier1/qualify_rust_live_reconstruction_28.py --archive /path/to/compilers --results /tmp/live-results --recheck
```

## Evidence boundaries

The shared Wasm module contains all five exports. Whole-module code size,
static div counts and ELF text-size deltas **cannot be assigned to one
function without separate native symbol attribution**. This is an intentional
first reachability screen; a reported change requires a follow-up
per-function disassembly and structural CLIF audit before it counts as an
optimization or negative-control selectivity result. Aot-byte identity may
reflect compiler normalization instead of failure of an original-source
pattern to reach CLIF. A different native output does not prove a rewrite
matched, nor does it prove runtime speedup.

Finite executable tests and a source-pinned independent recheck are not
whole-pass, machine-ISA, trap-location or register-liveness proof. This
work does not claim ZenoFCIS synthesised the manually designed Cranelift
reconstruction rule. No Bytecode Alliance upstream PR or protected branch
is touched. Await actual CI logs and raw result artifacts before reporting
any observed cost, reachability, or behavior.
