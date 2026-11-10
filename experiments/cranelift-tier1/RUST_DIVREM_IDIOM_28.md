# Cranelift issue #5623: Rust-generated Wasm division/remainder reachability

This is the next targeted experiment after the independently rechecked
[synthetic live-value timing](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42751309712),
which did not establish a consistent runtime speedup under register pressure.
This experiment asks a separate question: does LLVM preserve an ordinary
Rust source-level quotient/remainder pair in generated Wasm, and does the
experimental adjacent Cranelift fusion change the resulting native object?

Pinned Wasmtime/Cranelift: `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.
The unchanged/patched compiler binaries are retrieved from
[previously qualified run 42596373104](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42596373104);
the new runner checks their exact SHA-256 against the original source receipt
*before executing them*. Pinned Rust 1.97.1 creates separate
`wasm32-unknown-unknown` modules at opt-level 0 and 3. Each module is
then given **unchanged** to both compiler variants.

The authored Rust experiment exports an unsigned quotient/remainder pair,
a live-use variant, a reversed source-order variant, a swapped-operand negative
control, a quotient-only control, and a signed pair. A Python producer checks
finite native outputs against a declared 64-bit oracle, including zero-divisor
and signed-MIN/-1 trap-or-panic cases. It records exact Rust-produced Wasm,
emitted CLIF, native disassembly, hashes, whole ELF .text, serialized AOT byte
counts, and module-wide static division instructions. A separate Python
rechecker reopens the archived original artifacts, verifies their hashes and
costs, and tests fresh input vectors with a separately formulated oracle.

A positive size saving is **not assumed or required**. The Rust frontend may
already combine or transform divisions; a source-level expression need not
be an adjacent pair in emitted Cranelift IR. Module-wide static div counts
do not establish which function or ISLE rule was involved. A finite witness
set is not a compiler-pass proof, and a Rust panic is not necessarily an x86
division trap. This is not a representative external application, a general
throughput result, a generic ISLE verification, or a claim of broad real-world
opportunity prevalence.

Source:
`idiom_divrem_28.rs`,
`qualify_rust_divrem_28.py`,
`recheck_rust_divrem_28.py`, and
`.github/workflows/tier1-udivrem-rust-idiom-28.yml`.

Run evidence must be checked before promoting a result to the deduplicated
proof-and-cost catalog. Nothing is changed in main, the release branch, or
Bytecode Alliance upstream.
