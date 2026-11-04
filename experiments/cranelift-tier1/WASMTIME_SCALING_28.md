# Tier-1 2.8: source-pinned code-size multiplicity (synthetic)

This is a follow-up **research-only source and AOT binary test** of the pinned
Wasmtime/Cranelift ISLE rule at
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.
The real Wasmtime frontend and portable in-process experiments measured
a smaller signed-i64 saturating-add AOT function, but no established speedup
and no smaller 13,912-byte single-module container.

**Question:** if the same pattern occurs multiple times, does Cranelift
save actual emitted AOT executable text and/or entire serialized module
size, rather than merely changing a single function's disassembly?
This test is deliberately not a claim about realistic workload prevalence.

The generator accepts only the exact SHA-256 pinned original WAT source,
extracts the signed saturation function without changing its computation,
creates 1, 16, 64, 256 and 1,024 *distinct, exported* function copies, and emits
one identical WAT source for baseline and experimental compilers. Every
copy is thus eligible to be code-generated, with export names that prevent
merging a single exported symbol into the fixture. The only compiler source
delta is the existing, pinned mid-end ISLE rule. The WAT target is explicitly
portable `x86_64-unknown-linux`, not host feature auto-detection.

The first executable checks each AOT module with a finite signed arithmetic
oracle at representative exported indices; the producer measures actual
ELF `.text` and full serialized `.cwasm` lengths with SHA-256 pins for every
object and disassembly. The **separate rechecker** opens all archived WAT,
AOT and disassemblies, recomputes hashes, independently calls `readelf -SW`
for executable-text size, reconstructs whole file lengths, reparses every exported function's disassembled code and first return, checks finite
coverage attestations and refuses any discrepant deltas. Report both
metrics for every count, including zero savings or regressions. In
particular, do **not** multiply the single-function result by N and present
it as actual savings.

This experiment cannot establish throughput, instruction latency, broad
optimization efficacy, real-world code prevalence, or unconditional
semantic correctness. VeriISLE's exact named-rule proof remains conditional
on the local provisional `select` specification. The portable in-process
result on one GitHub runner was near 1.004x candidate/baseline and
classified as exploratory, not a runtime winner.

Source + workflow:
`qualify_wasmtime_scaling_28.py`,
`test_qualify_wasmtime_scaling_28.py`,
`independent_recheck_scaling_28.py`, and
`.github/workflows/tier1-wasmtime-scaling-28.yml`.

**Status:** consult the linked CI run and independent receipt before
asserting measurements. Nothing is submitted to upstream Wasmtime or merged
to ZenoFCIS release/main.

## First source-bound observed outcome (2026-11-04)

[Successful CI run 41768961465](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41768961465)
measured independently rechecked **zero difference in ELF `.text`
and entire `.cwasm` file size** for 1, 16, 64 and 256 distinct
exported copies. This is a meaningful negative result: savings seen in an
individual function's disassembly do **not** automatically turn into smaller
AOT sections or serialized modules, even at this small synthetic scale.
It does not yet distinguish section alignment from shared-code production
or nonapplication of the rule across repeated exported functions.

The next revision adds **1,024 copies** and independently reparses all
function disassemblies, reporting *both* pre-return code-byte savings
and whole-object/section changes. These are distinct metrics; the
latter are the application-footprint evidence. No additional results
are asserted until its CI workflow passes.
