# Narrow CLZ fallback: Intel dependency screen — 2026-11-27

Resume existing draft PR #197, research branch only. ZenoFCIS main observed 4160086fb9d19f3ebe25047e69b1caa467aa74fd; prior narrow evidence 07f35d65b73fbc093a73e3ef987426accedf09cb. Wasmtime main/build pin b738a89287bb78e51c06ac9624014bbda3c1429f; x64 lower.isle blob 545c79528cbb10fb1280623f23f53880b9762e49 independently fetched this run.

## Narrow variant regression evidence

[CI 46069109068](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/46069109068) job 140741487317 succeeded **under a policy permitting unchanged snapshot failures to be retained**. Artifact 15232635529 independently downloaded, SHA256 matched 60e15fd7a9dc5733913af451e6774e453b33e65110eb3cc2060138c2cfed61e6. All six baseline invocations pass. Candidate CLZ runtime, eight new i128 cases, LZCNT exact, and full runtime (420 tests) pass. Exact fallback 1/1 fails; full x64 2/224 fail (clz.clif and i128.clif); independently inspected logs show changed emitted assembly expectations, not a runtime mismatch. No blessing, no clean x64 claim. Patch pin 4660b63ade3d188b47c1dac5a420072922cbdf68055b6674090d4d6652d9fab3.

## Independent local experiment

Intel Xeon Platinum 8573C, KVM, GCC 14.2.0, CPU affinity fixed. Hand-assembled four-CLZ loops using register-renamed core sequences from actual prior compiler objects; NOT Cranelift-compiled loops. Equal load/loop scaffold; 1 versus 4 BSR destination registers, independent input loads plus sum versus carried true dependency via input XOR preceding result. Each block times 128 calls of a 4096-iteration loop = 2,097,152 CLZs; calls amortized, no per-CLZ indirect call. Reversed function link order in two shared libraries; random permutation per block, 40 blocks per mode/layout/link order. 320 blocks per experiment, two experiments. 480 + 960 native cases match independent Python bit_length oracle, including all-zero, all-one, variable-length and seeded inputs; timed blocks also check returned aggregate. Positive nonzero iteration count, valid nonconcurrent memory, modular words, flags dead except BSR ZF and loop branch. No new full ISA proof. Bootstrap intervals are screening evidence on one virtualized CPU, not universal throughput estimates.

Expanded experiment median elapsed-time change vs original (percent, positive slower):
| independent layout / variant | forward | reverse |
|---|---:|---:|
| one dest narrow | +25.18 | +24.63 |
| one dest narrow + zero idiom | -61.56 | -60.74 |
| one dest original + zero idiom | -63.55 | -62.61 |
| one dest original source tied | -61.88 | -61.01 |
| one dest narrow source tied | -58.49 | -58.69 |
| four dest narrow | +7.46 | +6.78 |
| four dest narrow + zero idiom | +7.72 | +7.39 |
| four dest original + zero idiom | +5.11 | +4.84 |
| four dest original source tied | +8.38 | +6.93 |
| four dest narrow source tied | +17.21 | +16.50 |

All ten independent-layout pairs have bootstrap intervals excluding zero in both link orders; dependent cases all include zero. Original one-destination false-dependency hazard is itself relieved by reset/tied variants; narrow reset is not a unique gain and does not win uniformly. Evidence supports register-dependency sensitivity, not a definitive causal microarchitectural measurement. Hand-assembled independent loop symbol sizes baseline 168, narrow 144, narrow reset 156, original reset 184, original tied 168, narrow tied 144 bytes (includes scaffold/padding). Do not transfer hand-assembled sizes or timing to actual compiler output. No reason to submit current unconditional optimization.

Replay bundle delivered in conversation: clz-intel-loop-screen-20261127.zip SHA256 a1dceb966e23646f294a6f43dacffa6a19bf49d70be32c660c46d6be2d541d5c; contains both replay.py scripts, generated assembly, shared objects, disassembly, raw timing JSON, CPU info and summary. Expanded replay.py SHA256 f2d2d79a02f919abe0c49d2218adab9b69d6974ea2e6519609ea805c5f410c7a; results.json SHA256 75c83c6199555cc26c82879425f792e1278ad615e4eb35edee70e77a0543b8e5. Run on x86-64 Linux with Python 3/GCC/binutils; no LZCNT needed.

## Next qualification

Committed directly compiled four-lane CLIF timing workflow 7895c6d025459580c128f280dd64e7c0561d1210 (exact pinned narrow patch, same source and build pin, both optimization modes, LZCNT controls, decoded object receipts, independent runtime oracle, paired timing in two library load orders). Specific workflow execution not confirmed at checkpoint. First inspect its results; only then decide whether a size-only rule, register allocation-aware tie/reset lowering, or a new candidate is warranted. If zero idiom is considered, require actual physical destination tie and dead pre-BSR flags; avoid a free virtual-register reset that the allocator may discard or separate. Compare reset against original-reset controls; do not mistake a generic BSR dependency optimization for a narrow-XOR-specific speedup. Keep prior negative whole-module size and throughput evidence. No upstream submission or change to main.
