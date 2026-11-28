# Context matrix qualification checkpoint — 2026-11-24

Resume [draft PR #197](https://github.com/TheDarkLightX/ZenoFCIS/pull/197). Do not promote the original lowering upstream.

- ZenoFCIS main: 4160086fb9d19f3ebe25047e69b1caa467aa74fd, unchanged.
- Wasmtime main: fcbcdd4d2d9b60b4a843dba07517924d71deb908 (seven commits beyond tested d49635a71351df82f3320095801848d8f128b938). Changes include dominator/post-dominator analysis. x64 lower.isle blob remains 545c79528cbb10fb1280623f23f53880b9762e49. Do not infer compiler output at latest HEAD from this.
- Original context workflow source 156f485297cac1f74ff69aeaee0d7098bade349d, [run 45489781758](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45489781758), both jobs failed.
- Compiler artifact [14979103616](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45489781758/artifacts/14979103616), independently downloaded SHA-256 38c182b97150ee0419befafad163c5aaba488b2d8697cb814e1b81098e098ae7 (matches GitHub digest). 72 of 88 planned objects completed, 419328 CI finite native executions, 28 matched A/B comparisons. 24 smaller, four unchanged; incomplete matrix, not a profitability conclusion. Identical deltas across speed/speed_and_size and BMI1 flags for these seven contexts.

| Context | Baseline bytes | XOR bytes |
|---|---:|---:|
| plain | 24 | 21 |
| mask_live | 31 | 25 |
| true_live | 28 | 25 |
| false_live | 28 | 22 |
| all_live | 38 | 35 |
| load_mask | 33 | 21 |
| load_true | 24 | 24 |

Independent audit linked all 72 completed objects from the artifact, verified object and fixture hashes, and executed 163008 finite oracle checks (2264 per object; seed 42501; 6^3 edge triples plus 2048 random samples; zero mismatches). It does not test missing objects, memory traps, or prove compiler refinement. No throughput test in this matrix.

## Concrete compiler counterexample
First failing command: clif-util compile --target x86_64 --set opt_level=speed --set has_bmi1=false -D -o xor-load_false-speed-bmi1_false.o load_false.clif. Archived compile log: panic at cranelift/codegen/src/machinst/lower.rs:1746:13, assertion failed: !self.inst_sunk.contains(&inst). The original expression uses f twice, while CLIF sees one use; the implicit memory operand conversion can sink the load and then a second conversion tries to access it. Source supports this diagnosis, but a fixed rerun is necessary.

Experimental correction ba424c4beb1f053dae4724d847b2535979245c06 materializes all three values with put_in_gpr before computing XOR/AND/XOR, including the repeated f. All-register choice intentionally avoids alias/sinking hazards at potential cost of load folding. Adds three alias-load fixtures (14 contexts, 112 planned objects); some may be simplified before lowering. It also prints failing subprocess log tails. The fix has not yet been compiled or verified. Compare corrected costs, especially loads, against identical baselines, and keep draft until all checks pass.

## VeriISLE coverage failure
Job 138784686189, artifact 14979710760. Logs explicitly show expansion error #308: expanding constructor 'x64_xor': no spec for term x64_xor. Passed 0, failed 0, unknown 0, expansion errors 1. No SMT query completed for this rule. This is a missing specification, not a semantic counterexample or a verification success. Do not add a circular assumption claiming the entire candidate is correct. Investigate supported lower-level instruction specifications or independently validate helper refinements before attempting named rule proof again.

## Resume
Inspect push-triggered workflow for ba424c4 (not only PR-triggered CI listing); audit the complete matrix or further failures. Preserve the rejected original memory-folding rule as negative memory. The old synthesis/verification/cost/catalog readiness gate is unchanged within its declared experimental scope. No upstream submission or main mutation.
