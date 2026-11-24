# Scalar i64 bitselect: preflight checkpoint (2026-11-23 local)

Continue this branch rather than duplicate the experiment. Source pins: ZenoFCIS main 4160086fb9d19f3ebe25047e69b1caa467aa74fd, research base 77010b8afd4010332dc39d045dd90ac401802e48, Wasmtime cef75a9fea591317bb076ec04d1f1a99ccfedb63, x64/lower.isle blob 545c79528cbb10fb1280623f23f53880b9762e49. Existing scalar bitselect rule computes (c & t) | (~c & f). Nearby band-not rules explicitly use BMI1 ANDN; the scalar bitselect rule does not.

Independent local preflight (not ZenoFCIS synthesis): exact dynamic programming over eight-row truth-table functions, tree cost one per NOT/AND/OR/XOR (and ANDN on BMI1), no constants, no shared subexpression accounting. Minimum 3 tree ops, both palettes; generic witness f XOR (c AND (t XOR f)). Enumerated 7,083 generic and 17,304 BMI1 composition attempts through cost 4. Pointwise truth-table identity verified in all 8 cases; intentionally omitted final XOR fails. This gives an all-width pure-value argument, not a decoder or full compiler proof.

Standalone x86-64 SysV assembly, arguments c=RDI,t=RSI,f=RDX:
- Baseline mov rax,rdi; not rax; and rax,rdx; and rdi,rsi; or rax,rdi; ret: 16 bytes.
- XOR mov rax,rsi; xor rax,rdx; and rax,rdi; xor rax,rdx; ret: 13 bytes.
- BMI1 andn rax,rdi,rdx; and rdi,rsi; or rax,rdi; ret: 12 bytes.

GCC 14.2.0, 101,728 native checks each (12^3 edge grid + 100,000 seeded random), zero mismatches, 305,184 total. Assembled object SHA256 a08d9e4946d33bf135fb9631af28b1322e969072f8410ba9e44054521f4b21ab. No signedness dependence or overflow; no trap/poison in pure word contract, flags dead at return, BMI1 required for ANDN. These are manual stand-alone sizes, not Cranelift emitted savings.

Local single-host AMD EPYC 9V74 paired randomized-by-balanced-order timing, 36 blocks, 32*32768 calls per measurement. Geometric baseline/candidate time ratios: XOR 0.935 (paired bootstrap 95% interval 0.864–1.008); BMI1 1.023 (0.924–1.124). No reliable throughput improvement in either. No A/A or cross-host control, no end-to-end claim. Raw code, native object, timing CSV and independent replay scripts archived separately in conversation checkpoint cranelift-tier1-bitselect-20261123.zip.

CI harness commit 4d1aefa659a3ad0fc8320e46c79f4c1bf15eb37e; workflow commit f386fac25b059ac68e09e1980eb06814cd6650f6. It is designed to run actual unseeded Zeno session.search on 3-input XOR/AND palette and exact same-CLIF baseline/experimental compiler A/B with and without BMI1, independent SMT, finite native oracle, dedup keys and separate byte measures. No executed CI or compiler A/B result yet observed; PR-only workflow lookup returned empty, not evidence of success or failure. Verify workflow status and fix any harness errors before further conclusions. No VeriISLE result yet; the BMI1 ANDN instruction is outside the current registered Zeno encoder model. Do not conflate its Boolean proof with a registered machine proof. No PR yet. No modifications to main or upstream; upstream human-only contribution policy applies.
