# Tier-1 Cranelift: real Rust-generated Wasm saturation idiom screen

**Scope:** an experiment-authored, idiomatic-language compiler-output
probe, not a production application. It follows the entirely unchanged
12/12 external Wasmtime WAT regression fixture results
(EXTERNAL_WAT_CORPUS_28.md). This is a distinct question: whether an
ordinary language compiler's i64::saturating_add construction actually
lowers to a pattern recognized by the current experimental Cranelift
ISLE rewrite.

## Two independent compiler stages

First, pinned Rust 1.97.1 compiles the exact
idiom_saturating_add_28.rs source to native **WebAssembly** using target
wasm32-unknown-unknown. The same Rust source is compiled at opt-level=0
and opt-level=3; the different LLVM results are **not** compared to
one another as equivalent test artifacts. Rust outputs are archived
with exact hashes and command arguments.

Second, for each resulting WebAssembly module independently,
unmodified vs one-rule-patched Wasmtime/Cranelift at
b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49 receives exactly the
**same Wasm bytes** and target x86_64-unknown-linux. The rule is
tier1_i64_signed_saturation_clamp. The original Wasmtime checkout is
modified only in the detached experimental copy, and each compiler's
binary hash and version are recorded.

## Declared function contracts

The Rust cdylib exports these five total signed-i64 functions:

| Export | Contract |
|---|---|
| sat_add | mathematical clamp(x + y, -2^63, 2^63 - 1) |
| sat_add_xor | bitwise signed-i64 XOR of saturated result and 1 |
| sat_add_mix | saturated result plus ((x as u64) >> 7), then modulo 2^64 |
| sign_clamp | MIN for x negative, MAX otherwise |
| wrap_add | signed i64 wraparound addition modulo 2^64 (negative control) |

Signedness, overflow order, shift signedness and result bitpatterns
are deliberate. No uninitialized Rust data, trap-prone operations,
exposed x86 flags or undefined behavior are present in these
declared exported computations.

## Verification and cost

The qualification workflow:
.github/workflows/tier1-cranelift-rust-idiom-28.yml

Source/oracle controls:
test_rust_idiom_28.py

Producer:
qualify_rust_idiom_wasm_28.py

Independent semantic/native-cost rechecker:
independent_recheck_rust_idiom_28.py

Both checks execute finite arithmetic witnesses on original and
patched compiled Wasmtime AOT objects; an independently implemented
signed/modular oracle checks additional values. The verifier refuses
missing or altered Rust/Wasm/AOT/native-disassembly bytes, wrong compiler
or Rust revision, incomplete finite checks, or fabricated ELF .text
and complete .cwasm deltas. It does **not** silently infer an AOT saving
from smaller disassembled function symbols. It also does not claim
any general-purpose throughput advantage.

**Major limitations:** the Rust source is a small synthetic set of
idiomatic constructs, not an external application. An LLVM-produced
Wasm pattern may differ from the hand-authored ZenoFCIS WAT; there is
no presumption that the target rule will match. The named VeriISLE
result still assumes an experimentally supplied scalar select model,
so neither this test nor the earlier pure-result QF_BV identity
certifies arbitrary CLIF poison, flags, ABI, optimizer chains or
different target architectures.

## Execution checkpoint

The initial targeted CI was
https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41964211060
(source commit a44c291a7dfb13e3ff9ef1bd8f1de36010def15f).
Its exact status/results must be checked before claiming any effect.

**Next reputation gate:** independently reproducible impact on
an external, realistic compiled application; authoritative select
semantics; current-upstream compatibility and performance tradeoffs
where a benefit is observed. Do not upstream speculative rules.


## Executed result — November 5, 2026 UTC

The [exact-source GitHub Actions run #41964211060](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41964211060) at source commit [a44c291a7dfb](https://github.com/TheDarkLightX/ZenoFCIS/commit/a44c291a7dfb13e3ff9ef1bd8f1de36010def15f) **PASSED**. All four source/reference/refusal tests, the pinned Rust 1.97.1 wasm32-unknown-unknown target, both original/one-rule-patched pinned Wasmtime builds, both opt profile Wasm builds, finite native output witnesses and the independently executed full-object cost rechecker completed successfully. [Retained original Rust/Wasm/native-AOT/patch/cost artifact #13386632486](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41964211060/artifacts/13386632486).

**Measured baseline-minus-patched results:**

| Rust compilation profile | Exact generated Wasm compared under baseline versus patched Cranelift | ELF .text saving | Complete .cwasm saving |
|---|---|---:|---:|
| Rust opt-level=0 | byte-identical native AOT | 0 bytes | 0 bytes |
| Rust opt-level=3 | byte-identical native AOT | 0 bytes | 0 bytes |

The source producer reported binary identity and zero measured native ELF .text and complete .cwasm delta in both cases. The **separately executed rechecker confirmed both exact identities and zero deltas**. Independent native arithmetic oracle checks passed across all exported functions on finite boundary cases. The result is **not a counterexample to the rewrite's correctness**, nor proof that common Rust apps never use this pattern. It does mean this exact ordinary Rust i64 saturating-add family, under these two LLVM optimization settings, obtained **no benefit at all** from the single experimental Cranelift rule.

Combined with the preceding unchanged 12/12 external Wasmtime WAT regression tests, the current evidence favors **investigating pattern reachability and prevalence rather than upstreaming this narrow rewrite**. Do not describe the successful synthetic multi-export AOT reductions as real-world application benefits. The actual condition-chain match vs LLVM output is not established by native cost equality alone; an exact Cranelift IR-source audit or search of representative external applications is needed. No speed or broader semantics claim, no upstream/protected branch mutation.
