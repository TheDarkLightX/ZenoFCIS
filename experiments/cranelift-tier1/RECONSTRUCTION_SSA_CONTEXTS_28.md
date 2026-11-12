# Reconstruction compiler: live-SSA context and trap qualification

**Status: qualified narrow, negative reachability experiment (November 12, 2026).** This is
a follow-up to the independently qualified 13 decoded x86-64 instruction-byte
saving in three *optimized Rust-generated Wasm* functions. It addresses a
different upstream-review gate: whether the manually developed
\`udiv(x,y); imul(q,y); isub(x,m)\` → multi-result \`udivrem(x,y)\`
experiment preserves other SSA uses of the quotient, multiply and remainder.

The previously tested Rust functions returned a combination of quotient and
remainder; this small **separate** WAT-based matrix constructs five declared
contexts. Every context passes the **identical WAT** to the unchanged and
experimental Wasmtime binaries and executes both against an independent
Python fixed-width oracle:

- \`live_q_m_r\`: the quotient, intermediate multiplication and reconstructed
  remainder are all independently consumed after their definitions.
- \`reversed_mul\`: multiply operands are interchanged, which is in the
  matcher's explicitly allowed commutative form.
- \`wrong_multiplier\`: multiply the quotient by x rather than y, then
  subtract; a valid \`udivrem\` replacement cannot match this operation.
- \`stateful_between\`: write a mutable exported global between quotient
  and multiplication, and subsequently consume the global.
- \`double_reconstruction\`: two quotients and two complete remainder
  reconstructions with distinct numerators, both live.

The function's return is a distinct rotation/XOR of the observed values;
the operand domain is unsigned 64-bit, with wrapping multiply/subtract and
rotate, and \`y=0\` must trap. Inputs include signed CLI encodings of unsigned
edge values and pseudorandom values. Every compiler binary is checked
against the SHA-256 found in the exact producer's qualification receipt;
WAT, compiled AOT, emitted CLIF and native disassembly are saved with hashes.

**Compiler pin:** \`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49\`
(Wasmtime/Cranelift). **Source of the compiler binaries:** completed
GitHub Actions
[run #42951667233](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/42951667233),
artifact \`tier1-udivrem-llvm-reconstruction-28\`. The workflow downloads
this exact prior artifact, not the separate adjacent-udiv/urem compiler
artifact. The new source is
\`qualify_reconstruction_ssa_28.py\` and the targeted workflow is
\`.github/workflows/tier1-reconstruction-ssa-28.yml\`.

**Interpretation contract:** A native output match is finite differential
testing; it is *not* universal compiler-pass, x86 ISA, flag, poison,
debug-location or trap-position verification. A change in disassembly
alone is not proof of a targeted matcher hit or speedup. The output
distinguishes **guest function decoded x86 bytes**, guest static division
instructions, and **whole AOT bytes**, and it does not treat one as a proxy
for the other. Negative fixtures fail closed on any baseline/patched
whole-AOT change, for investigation; even a legitimate egraph optimization
would require examination before accepting a changed case. If the WAT
frontend lowers a case to an unrelated CLIF form, it is a valid non-reachability
finding, not proof that the transformation is useless.

The broader target remains a safe upstream-relevant rewrite, but the
current candidate is **manually selected** and has no demonstrated runtime
benefit. The next gate remains SSA/trap/target correctness (including
register allocation under real uses) and representative workloads,
not additional synthetic timing shams. No Wasmtime upstream,
ZenoFCIS main, or release/2.7.0 modification is authorized by this test.

## Completed qualification (2026-11-12 UTC)

[CI #43312470294](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43312470294)
passed at exact experimental source \`000ed60ef503df479223cb07cfbe2ad675f863db\`.
The workflow retrieved the original reconstruction compiler artifact
\`#13825142647\` and verified its Actions ZIP digest and pinned compiler
SHA-256 receipt. Both compiler variants executed **the same WAT per case**.
The direct producer checked **660 finite native return values and 100
divide-by-zero traps**. A separately written rechecker in the same CI job
reopened **65 source, CLIF, AOT and disassembly files**, independently parsed
all five native cost receipts and checked **600 fresh finite native values
and 120 fresh zero-divisor traps**. All passed.

**Result:** All five cases produced **identical baseline and experimental
AOT files**, with zero differences in decoded guest-function x86 bytes,
decoded instruction count, static guest \`divq\` count, and whole AOT bytes.
This is an informative **negative reachability result** for these direct
WAT forms, *not* evidence that the manually developed compiler pass is
generally correct or that the previously qualified **optimized Rust-generated**
three-function 13-byte savings have disappeared. It indicates that a WAT
expression alone is insufficient to reproduce the observed LLVM-produced
optimized CLIF matcher input.

[Original qualification + independent recheck ZIP](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/43312470294/artifacts/13985723546)
is 60,574 bytes; Actions upload SHA-256:
\`095783398657d11d88e53e46984f2f7854c616f9e0dc5e35506df6cd6cc6f99e\`.
The source- and tool-pinned interpretation is in
\`evidence/reconstruction_ssa_20261112.json\`. The independent recheck
was performed within the successful CI job on the just-produced files,
not by a separate external ZIP downloader.

Next: construct equivalent **optimized Rust-generated** cases with live
intermediate values, examine their exact CLIF and real patched lowering,
then assess trap/SSA correctness and realistic profitability. No upstream
submission is justified from this screen.
