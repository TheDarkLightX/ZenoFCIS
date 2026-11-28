# CLZ dependency/width screen — 2026-11-27

Resume draft #197, head before this run 96119b115c13f84a29d98c6cd5389da722d9c17b, pinned upstream b738a89287bb78e51c06ac9624014bbda3c1429f. Earlier native Zeno bitselect gate remains passed; CLZ is independent abstract search, not native Zeno BSR synthesis. Previous Intel Xeon 8573C 18.65–26.23% independent i64 timing regression remains unresolved. Regression workflow push execution not confirmed by connector (commit workflow listing only exposes PR-triggered runs).

New finite screen: six fixed SysV ABI i64 kernels, common compiler frame, 64-byte aligned symbols in one shared library, reverse symbol layout in second pass. Baseline and xor64 byte-for-byte match actual earlier compiled object symbol bytes (baseline SHA256 15b4d93affd68f70bc4c60dad7b71eecaf0d3516c96d5526d442053c3201b136; xor64 4d6e1fc7710df369b066234ab9d37d433388710847739cbdb1aeeb60c9c34e4a). Others are hand assembled and not compiler output.

| Variant | Function bytes | Core instructions | independent median delta A/B % | dependent median delta A/B % |
|---|---:|---:|---:|---:|
| baseline |32|5|0/0|0/0|
| xor64 |26|4|0.589/1.028|2.259/7.530|
| xor32 |24|4|-1.174/-2.639|-1.948/3.481|
| reset32 |26|5|-2.787/1.826|1.618/7.929|
| scratch32 |26|5|1.329/3.328|-0.462/0.928|
| reset64 |28|5|-1.424/3.202|0.682/2.172|

xor32: mov edx,127; bsr rax,rdi; cmovz eax,edx; xor eax,63.
reset32 adds xor eax,eax before BSR; reset64 same reset before original xor64. scratch32 uses rsi/esi instead of rax/eax for BSR/CMOV/XOR, then mov eax,esi. Flags dead; CMOVZ consumes BSR ZF; reset preceding BSR cannot destroy its flags. On zero BSR dest arbitrary, CMOVZ discards it. Nonzero index in 0..63, so 32-bit truncation is lossless; final EAX write zero extends. 65 semantic classes checked (64 highest-bit indices plus zero), not a full ISA/VeriISLE proof. All 698,340 native oracle checks across the two passes passed. AMD EPYC 9V74 virtual host, fixed CPU affinity, 160 randomized paired measurement blocks (40 per mode and layout), 524288 indirect native calls per variant per block. All xor32 and xor64 percentile bootstrap intervals straddle zero, except no such exception for either; reset32 dependent layout B interval [1.002,13.990] only, not reproduced. No repeat of prior Intel slowdown on this AMD host, not evidence that Intel regression is fixed. Timing includes indirect-call overhead and VM noise. No throughput superiority claim or global optimum claim.

Replay archive clz-dependency-screen-20261127.zip SHA256 ed21eed3dee5cf38381399aa92059afb3d127f1687c534481c5374bc71b4b872 contains raw timings, source, object fixtures, environments and disassembly; delivered separately. Next qualify xor32 via x64_bsr I64 and cmove I32 in a with_flags_reg pair, followed by x64_xor I32; use actual compiler bytes and LZCNT controls, rerun Intel timing if available. Do not submit upstream until profitability and personal review. No main mutation or upstream PR.
