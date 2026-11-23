# Signed-i64 saturation: independent throughput replication and flag scheduling (2026-11-23)

Research checkpoint, not a Wasmtime contribution. ZenoFCIS main 4160086fb9d19f3ebe25047e69b1caa467aa74fd; prior research head 6809e312b0ff2b11e438264b8a8b350837beccb3. Wasmtime current cef75a9fea591317bb076ec04d1f1a99ccfedb63 is five commits ahead of the compiler artifact source fb792bf7a143801c22c0ba5b16fe2c6085a8598b. Compare lists no changed cranelift/ paths; selects.isle has identical blob 5ec428ceefedfc89c49392875a3fad7299a5f884 at both revisions. This is source transport evidence, not a new compiler build.

## Independent runner result

[CI 45154966339](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45154966339) completed successfully, job 137625469991, at 88d5999334d1ede538badeb8ba72c44df6d80448. Logs show the two actual compiler objects from CI 45107263287 passed the SHA256 guards (pristine 5179f3985e49772b4656ca261e64eb0d8ba57802b78abc9c1ee02d2d7cf03268; experimental 06e9bb768ebb06a1067b1a1bb23bcb48e5bfbcd04e4281db07332e22d501b960), then ran 24 blocks at 20 repetitions per workload in both library orders.

Baseline elapsed / candidate elapsed, as printed by job:

| workload | forward | reversed | A/A |
|---|---:|---:|---:|
| mixed_random | .8574 | .8566 | 1.0080 |
| negative_overflow | .8564 | .8576 | .9997 |
| no_overflow | .8569 | .8553 | .9972 |
| positive_overflow | .8568 | .8589 | 1.0080 |

Experimental is approximately 16.4–16.9% slower in this function-pointer microbenchmark. Artifact 14840757220, reported ZIP SHA256 443fe53c77b50aa9772f35fce63d5304d756b68417c8a3e2646c3c960f67b72e, 32 files. This checkpoint reads the logs and metadata, not a fresh independent artifact download. The smaller compiler rewrite is not a demonstrated throughput win. No claim about all hosts or whole Wasmtime programs.

## Six new manually proposed straight-line candidates

Independent supplemental assembly screen, not Zeno synthesis or compiler emission. Same prologue push rbp; mov rbp,rsp, epilogue mov rsp,rbp; pop rbp; ret. Intel syntax; semicolons below separate instructions. All candidates use RDI/RSI input and RAX output with caller-saved scratch; two's complement modulo 2^64; ADD OF/SF, BT CF, ADC, and CMOV semantics manually modeled. Flags after return not observable. Valid stack required; no poison or other undefined inputs admitted. Conditional QF_BV return proof only, not binary-to-model refinement, ABI or ISLE correctness proof.

| name | body | bytes including frame and RET |
|---|---|---:|
| bt_adc | movabs rax,0x7fffffffffffffff; bt rdi,63; adc rax,0; add rdi,rsi; cmovno rax,rdi | 35 |
| preselect_lea | movabs rax,0x7fffffffffffffff; lea rcx,[rax+1]; test rdi,rdi; cmovs rax,rcx; add rdi,rsi; cmovno rax,rdi | 37 |
| postselect_lea | movabs rax,0x7fffffffffffffff; lea rcx,[rax+1]; add rdi,rsi; cmovns rax,rcx; cmovno rax,rdi | 34 |
| postselect_const | movabs rax,0x7fffffffffffffff; movabs rcx,0x8000000000000000; add rdi,rsi; cmovns rax,rcx; cmovno rax,rdi | 40 |
| preselect_sar | movabs rax,0x7fffffffffffffff; mov rcx,rdi; sar rcx,63; sub rax,rcx; add rdi,rsi; cmovno rax,rdi | 36 |
| preselect_shr | movabs rax,0x7fffffffffffffff; mov rcx,rdi; shr rcx,63; add rax,rcx; add rdi,rsi; cmovno rax,rdi | 36 |

All six manually modeled differences against signed 65-bit exact addition/clamp returned UNSAT (libz3 4.13.3.0); inverted output-sign clamp mutant SAT. Six distinct .text hashes. Native oracle passed 131072 inputs per candidate (786432 candidate calls plus matching baseline checks), using __int128 oracle in bench_satadd_objects_20261123.c. The two postselect candidates exploit the fact that on signed overflow the wrapped sum's sign is opposite the original operands' common sign. This avoids the pre-ADD sign extraction dependency but still incurs serial conditional selection.

One virtual AMD EPYC 9V74 host, one loading order, 12 paired blocks at 12 repetitions. Reference time / candidate time (higher better), order: no overflow, positive overflow, negative overflow, mixed:

- bt_adc: .880 / .906 / .896 / .881
- preselect_lea: .930 / .881 / .924 / 1.110
- postselect_lea: .882 / .915 / .876 / .899
- postselect_const: 1.023 / .943 / .707 / .738
- preselect_sar: .789 / .958 / .872 / .997
- preselect_shr: .864 / .870 / .883 / .913
- A/A: 1.084 / 1.048 / .990 / 1.005

No general profitable candidate, no reason to adapt these to compiler ISLE yet. Reproduce by assembling each function with symbol "%tier1_signed_satadd", GNU cc -c -fPIC then cc -shared; use exact pinned pristine.o and the existing bench_satadd_objects_20261123.c benchmark (12 12 arguments). The local checkpoint contains the six exact assembly files, SMT-LIB formulas, native CSV observations, replay scripts, tool/host pins and deduplicated catalog (qualified-catalog.json SHA256 fc1e55ee6b7f380c67f50df83bc6959a6e9e0d6b9655ec7f9c80b34270676328). This markdown records the summary only. No upstream changes or submission.

Next search should consider a different narrow lowering rather than repeatedly shrinking saturation at the expense of throughput. Retain this negative evidence under the exact declared target and cost context; do not treat it as a theorem about all saturated-add implementations.
