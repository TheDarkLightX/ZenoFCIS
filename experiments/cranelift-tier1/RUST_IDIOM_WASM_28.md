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
