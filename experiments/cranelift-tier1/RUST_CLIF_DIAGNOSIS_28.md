# Tier-1 Cranelift: investigate LLVM-produced Wasm and exact CLIF reachability

**Status: diagnostic instrumentation committed; do not claim new results until
[Actions run #42004612090](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42004612090)
completes and its receipts are inspected.**

## Why this is the current priority

The first candidate, `tier1_i64_signed_saturation_clamp`, was checked on
bounded ZenoFCIS machine-code candidates, independently proved as a pure
bitvector clamp identity, exercised with real single-rule-patched Cranelift,
and qualified with native and WebAssembly outputs. A pinned named VeriISLE
check passed one rule expansion only with a provisional, locally authored
scalar `select` specification; it is not unconditional upstream verification.

Positive code-size evidence is limited to deliberately constructed sources,
including heterogeneous integer microkernels (4 KiB fewer whole AOT bytes).
By contrast, two real source-backed negative screens passed:

- 12 pinned, unchanged Wasmtime upstream WAT fixtures: all native AOT byte
  pairs identical ([CI #41963407858](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41963407858)).
- A pinned Rust 1.97.1 source using `i64::saturating_add` and three derived
  operations, compiled to LLVM-produced wasm32 Wasm at opt 0 and opt 3:
  baseline/one-rule-patched Wasmtime AOT identical in both profiles
  ([CI #41964211060](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41964211060)).

Neither negative screen establishes that the ISLE rule was never evaluated,
nor proves non-prevalence of the pattern in real applications. They do
demonstrate the lack of any observable native emitted-code benefit there.

## New source-pinned experiment

`diagnose_rust_clif_shape_28.py` reuses the exact *already-qualified* Rust
Wasm artifact and AOT producer receipt, checks the pinned Wasmtime source,
both compiler binary identities, Rust source and hashes, and reruns
`wasmtime compile --emit-clif` for each baseline/patched compiler and
each of the Rust opt 0/3 profiles. It captures all emitted textual Cranelift
IR files and exact hashes, counts/locates selected operators for inspection,
and records whether corresponding baseline/patch IR files are byte-identical.

A separate **positive codegen control** captures Cranelift IR using the
previously pinned, hand-authored Zeno Wasm saturating-add WAT. This control
is synthetic and does not establish occurrence in LLVM-produced Wasm.

`test_diagnose_rust_clif_28.py` provides parser checks against comment
tokens, substring false positives, empty IR and unbounded examples. The
targeted existing Rust-idiom CI workflow now checks these tests, independently
requalifies the earlier real AOT binary results, runs the CLIF capture, and
archives both the old and new receipts and the exact dumped IR.

**Crucial assurance boundary:** textual occurrence or absence of `select`,
`icmp` or an overflow op does not amount to an ISLE pattern match proof.
CLIF may be emitted before the stage applying the candidate rewrite.
Absence of printed tokens cannot establish a general compiler-rewrite
unreachability theorem. A compiler-side match counter, exact optimization
trace, or an independently validated structural IR matcher will be needed
for that stronger claim.

## Follow-up acceptance

If the new diagnostic CI passes, inspect the archived exact IR for each
Rust export and profile, compare its dependency graph to the rule's LHS,
and identify whether to narrow, generalize or replace this candidate.
A wider rule must be proved under actual scalar `select` semantics and
tested against cross-target, undefined/poison, flag/ABI and real program
performance boundaries. No speculative upstream PR.

All work remains on the separate
`research/2.8-tier1-cranelift-performance-20261102` branch. Neither
ZenoFCIS main/release nor the Bytecode Alliance upstream branch was edited.

## Executed source-and-IR result (2026-11-05 UTC)

Exact pinned [GitHub Actions #42004612090](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42004612090) at experiment source commit [`a90a754c`](https://github.com/TheDarkLightX/ZenoFCIS/commit/a90a754c179225691a149e5dbba743519f4634e5) **passed every job step**. This included the original Rust-to-Wasm finite output checks and independent native cost replay, the new CLIF textual diagnostic tests, and the real baseline/patched `--emit-clif` compilations. The [raw CI artifact #13403722616](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42004612090/artifacts/13403722616) contains original Rust-generated Wasm, actual .cwasm pairs, emitted CLIF, source pins, and original/diagnostic receipts. The downloaded artifact ZIP independently hashes to SHA-256
`13fceda835cb98fa9832ac9f4e36ae5171ff74d551f30bb88c94d74ba162f05f`; the recorded diagnostic JSON hashes to
`72a9c220fac2b607e30dafd1983b5d5f1ca7609df8618d6ac39fdc893e97a265`.

**New concrete explanation for the original rewrite's lack of observable impact on the tested Rust code: the actual LLVM-produced CLIF has a different overflow/clamp dataflow graph from the experimental ISLE left-hand side.** Both compilers produced byte-identical emitted CLIF for all files in each profile (103 files for Rust opt 0, 12 for Rust opt 3); the original separately qualified complete AOT objects also remained byte-identical.

In both Rust profiles, the `sat_add`, `sat_add_xor`, and `sat_add_mix` exported functions share this observed signed-i64 CLIF core (shown here as mathematical pseudocode, **not** asserted to be a Cranelift source-level optimizer rule):

```text
s           = iadd(x, y)                     // 64-bit modular sum
overflow    = (y <signed 0) XOR (s <signed x)
clamp       = (s >>arithmetic 63) XOR INT64_MIN
saturated   = select(overflow, clamp, s)
```

The patched rule instead recognizes an inner `select(x <signed 0, INT64_MIN, INT64_MAX)` clamp. The **hand-authored WAT positive control** emits that specific inner select, while none of the three Rust saturating-add functions has it in the archived CLIF. The positive-control baseline/patched native AOT binaries differ, confirming this exact experiment can observe a compiler codegen effect; the Rust ones do not.

Independent source identities for the selected emitted CLIF:

| Emitted baseline CLIF | SHA-256 |
|---|---|
| Rust optimized `sat_add` | `16d25e0750eb38358de6d3e21e300f656077daa89cc9924e7e3b26451345ef54` |
| Rust optimized `sat_add_xor` | `406067ab99aea33cc4ac9388dd510b5097b58bf17cf1d58d6649557a8f66fed9` |
| Rust optimized `sat_add_mix` | `2e59545a0e5f6ca3bdc18333a2e179e7bd541bb4ddcf8d9d2c77442fdbba3130` |
| WAT positive-control `sat` | `41fe7c06dac196d36b18250e7ff2406d0b6fa811aa32d5075e71c455abfa60ec` |

The Rust optimized `sign_clamp` also already uses a shift/XOR algebraic form rather than an inner `select`, further constraining the claimed scope of the original rule.

**Assurance boundary:** emitted CLIF is a stage-specific representation, not a verifier-certified negative ISLE match count or a whole-compiler reachability theorem. These outputs do not establish which optimizer stage did/did not attempt the rule. The existing named VeriISLE check remains conditional on a provisional select spec. The test still establishes **zero realized compiler benefit for the specific pinned idiomatic Rust inputs**, along with a source-backed mismatch to guide a distinct candidate vocabulary.

**Next practical search/verification direction:** formalize the LLVM-emitted modulo/signed overflow and sign-mask contract above, compare proposed equivalent constructions under a bounded, pinned search or targeted lowering experiment, and require independent SMT proof and actual baseline-vs-patch native AOT cost before selecting a new rule. Do **not** widen the old `select(x < 0, MIN, MAX)` rewrite from its synthetic evidence alone; no upstream submission is justified.

## Independent all-input semantics qualification (2026-11-05 UTC)

The separate [pinned GitHub Actions #42005719011](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42005719011) **passed** at source commit [`001c8bb3`](https://github.com/TheDarkLightX/ZenoFCIS/commit/001c8bb3bddc25251d08dfae0088da580be3a607). The new
[`verify_rust_llvm_clamp_28.py`](https://github.com/TheDarkLightX/ZenoFCIS/blob/001c8bb3bddc25251d08dfae0088da580be3a607/experiments/cranelift-tier1/verify_rust_llvm_clamp_28.py)
ran using the repo's hash-pinned Z3 4.15.4 wheel with explicit `QF_BV` logic. It models *all* pairs of signed 64-bit inputs through the four observed LLVM/CLIF arithmetic operations and compares them with an **independent 65-bit sign-extended mathematical-saturation reference**. Two separate counterexample queries returned **UNSAT**:

1. The observed LLVM/CLIF core `select( (y<0) XOR ((x+y)<x), ( (x+y) >>arithmetic 63 ) XOR MIN, x+y)` is equivalent on **all 2^128 signed-i64 input pairs** to mathematical signed-i64 saturating addition, under the admitted total bitvector semantics.
2. The two exact sign-mask clamps `MIN XOR (sum >>arithmetic 63)` and `MIN ADD (sum >>arithmetic 63)` are equivalent for all signed 64-bit sums, **only as semantic expressions**; neither has a demonstrated lower native cost.

Two deliberately wrong constructions (substituting logical for arithmetic right shift, and using an incorrect AND-based overflow predicate) both produced genuine **SAT counterexamples**, checking the solver harness's ability to refute non-equivalent candidates. No `unknown` was accepted.

[Original raw solver artifact #13404527967](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42005719011/artifacts/13404527967) includes full counterexample query texts, solver version, SAT witnesses, source hashes and verdicts. Independently downloaded `receipt.json` SHA-256:
`3987a0a0a5a6d0994048a3c2e0f1f072a06d740230c3d135697092b645a2107e`.
The proof source SHA-256 in the receipt is `8103e9241253549e3f91deaf0c9989edf8ee1933779367fa3c6f121761dbc35a`.

**This closes the pure i64 value-equivalence gap for the *observed Rust/LLVM computation graph***, not the named ISLE rule's provisional `select` specification or arbitrary compiler passes. The exact CLIF/source linkage remains a hash-bound human structural audit; we did not prove that Rust/LLVM emits this graph for all programs. No x86 flag/ABI/poison state, native rewrite, general machine ISA, power or throughput proof follows. An `ADD` replacement is a semantic search candidate, **not a verified cheaper code sequence** until independent same-CLIF baseline/patched costs are measured. Existing negative application-cost screens remain in force.
