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

## Real Wasmtime AOT checkpoint (November 3, 2026)

The [targeted Wasmtime AOT workflow run #41724158745](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41724158745)
**passed** at research source `75506dd85f1ac0b34dfdde30d072e5c3e09ad6ae`.
The genuine Wasm frontend emitted different CLIF for `wasm[0]::function[0]::sat`,
and an ISLE-patched full Wasmtime binary emitted different AOT machine code
from its same-source unpatched binary. Both AOT modules passed finite
independent tests: 12 signed saturation vectors and 8 loop counts **per
variant**. The emitted-function and whole-module costs must be kept separate:

| Pinned x86-64 Wasmtime AOT scope | Unmodified | ISLE-patched |
| --- | ---: | ---: |
| `sat` function symbol | **64 bytes** | **52 bytes** |
| Straight-line executed prefix through `ret` | **56 bytes** | **52 bytes** |
| Post-`ret` literal/padding bytes | 8 | 0 |
| Other Wasm `workload` function symbol | 160 | 160 |
| Whole-module page-aligned `.text` section | 4096 | 4096 |
| Whole serialized `.cwasm` file | 13,912 | 13,912 |

Thus the actual WebAssembly function has **12 fewer symbol bytes (18.75%)**
and **4 fewer executed instruction-prefix bytes**. In this case there is
**no whole-module binary-size saving** because of alignment and metadata.
The recorded whole-process candidate/baseline timing median was approximately
1.001×, which establishes **neither a speedup nor a slowdown**.
The [original AOT artifact #13273568497](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41724158745/artifacts/13273568497)
contains the two compiled ELF modules, emitted CLIF, disassemblies, compiler
pins and raw timing samples. The additional source-bound function-size
qualifier is `qualify_wasmtime_symbols_28.py`; it rejects changed objects,
incorrect target, other changed function sizes, source-provenance drift and
misreported post-return bytes. Its own hosted run must be checked independently
rather than assuming this new validator has already passed.

**Open qualification:** the named VeriISLE rule proof remains conditional on a
provisional scalar select model; production breadth, poison/flags/ABI
conditions, and realistic runtime benefits are not established. No upstream
Wasmtime PR is justified solely by this small example.
