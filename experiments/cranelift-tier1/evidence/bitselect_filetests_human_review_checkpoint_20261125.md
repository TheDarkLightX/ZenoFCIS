# Tier-1 bitselect — regression qualification and human-review checkpoint (2026-11-25)

## Pinned sources and evidence
- ZenoFCIS main inspected at `4160086fb9d19f3ebe25047e69b1caa467aa74fd`; README explicitly scopes 2.7 guarantees. The historical ZAL machine-code WIP head is `47470843f69a8cfe5b8555818170d0a77da01955`. The actual synthesis and prior independent artifact evidence on this research branch, not the release number, establish the bounded experimental readiness gate.
- Existing draft research PR #197; filetest workflow revision `206940f6bdba528d65afa8dca26305c624372def`.
- Successful push CI [45643127305](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45643127305), job 139306821486, upstream `fcbcdd4d2d9b60b4a843dba07517924d71deb908`, rustc 1.98.0 (88d9e12ae), x64 lowering blob `545c79528cbb10fb1280623f23f53880b9762e49`. Retrieved job log confirms all eight invocations exited zero: baseline and XOR each reported 5 targeted bitselect tests, 1 new compile-only multi-load file, 222 x64 tests, 420 runtests. Suites overlap; do not sum as unique tests. No automatic blessing. Artifact [15036347296](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45643127305/artifacts/15036347296), service-reported SHA256 d0ec876b4e71df2fe53f099e5b8ead7bf0869e0d33e923730eaa569d6fdce3c8 (not independently downloaded in this run).
- Independently reran the archived prior context audit on 112 ELF objects, checking hashes, exact symbol bytes, recompiling shared libraries, 229,376 native calls, 21 distinct QF_BV queries UNSAT and deliberately incorrect final-OR mutant SAT, libz3 4.13.3.0. This is a replay of the prior object set, not 112 new compiled objects. 56 pairs: 44 smaller, 12 equal, 1684 -> 1456 symbol bytes, aggregate instruction count delta -76. Valid stable single loads, decoded subset, conditional return-value semantics only.
- Current Wasmtime main inspected `6ddc5743e5e8cd3130151e30c1cd2b96b1d17e63`, two commits ahead of tested filetests; exact lower.isle blob unchanged, but alias analysis and memflags changed. Do not infer the earlier qualification covers these changes.

## New targeted current-source qualification
[Workflow commit e5e7e69018e7dff0cec89a61120cf2f8b7f33808](https://github.com/TheDarkLightX/ZenoFCIS/commit/e5e7e69018e7dff0cec89a61120cf2f8b7f33808), [run 45700821940](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45700821940), was in progress when inspected. Planned 48 actual compiled objects / 24 A/B pairs, four memory patterns, three opt levels, BMI1 on/off, all 27 three-pointer alias assignments, 1424 value samples per assignment. Observe both return and every memory slot. Includes mutation control for moving a load across a store. No throughput/full refinement/fault behavior claim. Inspect actual output before claiming any results; if failure, preserve exact witness and diagnose baseline vs XOR. Retain negative sizes rather than suppressing them.

## Reviewable candidate (not an upstream submission)
The exact experimental change is the named higher-priority i64-only rule below. It deliberately avoids the previous repeated load-sinking conversion by explicitly materializing all operands before using the false value twice. The original general scalar rule remains fallback. It is not a general vector, float, i128, or BMI1-specific rewrite.

```lisp
(rule tier1_i64_bitselect_xor 6 (lower (bitselect $I64 c t f))
      (let ((cg Gpr (put_in_gpr c))
            (tg Gpr (put_in_gpr t))
            (fg Gpr (put_in_gpr f))
            (d Gpr (x64_xor $I64 tg fg))
            (a Gpr (x64_and $I64 cg d)))
        (x64_xor $I64 a fg)))
```

Review: per-bit result is f XOR (c AND (t XOR f)). Mask zero selects f, mask one selects t. Signedness irrelevant; no arithmetic overflow or poison in total bitwise contract; x86 flags are not the CLIF result. Three dependent Boolean operations may be less profitable than parallel ANDs under some CPU schedules. Explicit registers may lose load folding; native memory CI now measures this and checks aliasing. No general throughput benefit established, no VeriISLE proof (helper specification gap). Retain prior satadd negative evidence. Do not add unrelated verifier infrastructure to a small lowering PR without maintainer input.

## Submission boundary
Verified the official [Bytecode Alliance AI Tool Use Policy](https://github.com/bytecodealliance/governance/blob/main/AI_TOOL_POLICY.md), blob `35222c2eb0cde8b00a56b8b4027c8ae3f5759306`. It requires the human contributor to personally read and review generated code and text before soliciting project review; autonomous publishing without human approval is prohibited. General project authorization does not establish personal review of this exact patch. No upstream PR/comment, no upstream branch mutation, no main merge. Keep the existing ZenoFCIS draft as staging; present the candidate, tests, limitations and final patch for human review before any upstream submission. Next: inspect native-memory CI, replay resulting objects independently, decide whether profitability merits upstream review, add narrow regression filetest as appropriate, and obtain specific human review of final patch and text.
