# ZenoFCIS 2.8 Tier-1: actual Wasmtime frontend reachability

This continuation is a **bounded experimental checkpoint**, not an upstream
Cranelift submission. It tests the same WebAssembly fixture under unmodified
and one-rule-patched Wasmtime/Cranelift, pinned at
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.

Why: 56→39 bytes of single-function x86 CLIF and 36→32 bytes on AArch64
are qualified observations, **not proof that real WebAssembly workloads cause
this mid-end rewrite to fire**. In fact WebAssembly lacks a `sadd_overflow`
opcode. A genuine zero-delta Wasmtime result would be important evidence
against claiming the rewrite is broadly useful.

The targeted workflow `.github/workflows/tier1-wasmtime-frontend-28.yml`
rebuilds Wasmtime CLI before/after an isolated experimental ISLE rule, compiles
the *same* `satadd_wasmtime_28.wat` source to precompiled AOT with
`--emit-clif`, archives both CLIFs and Wasmtime native disassembly,
and measures **whole-module** AOT ELF `.text` and serialized `.cwasm` size.
Neither figure is equivalent to a single hot function's emitted bytes, much
less cycles or throughput.

It independently checks finite overflow/nonoverflow values and results of
an iterative Wasm arithmetic loop against unbounded Python integer semantics.
Balanced ABBA/BAAB exploratory timing invokes each compiled AOT module.
Those timing samples include Wasmtime CLI startup, loading, and invocation;
a measured ratio **must never be represented as a machine-code latency or
application speedup**. One WAT module is not a full production workload suite.

This experiment retains all existing evidence and branches, does not alter
ZenoFCIS main, the 2.7 release branch, or Bytecode Alliance upstream.
The earlier named VeriISLE rule pass remains conditional on a provisional
locally supplied scalar-`select` specification. No architecture-general
ABI/flags/undefined/poison assurance is claimed.

**Go/no-go after execution:** an unchanged native AOT/CLIF output means this
fixture does not demonstrate real Wasm frontend reachability. A changed output
must pass the finite oracle and be separately cost-qualified and assessed for
performance regressions. No upstream PR should be opened solely on this
single fixture.
