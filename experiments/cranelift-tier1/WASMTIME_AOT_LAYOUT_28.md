# Why the shorter Wasmtime function did not shrink the AOT module

The [portable in-process CI run 41767562971](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41767562971) proved execution reachability and finite-test correctness on portable x86-64 AOT objects, but the candidate/baseline time ratio 1.0038 (min 0.9833, max 1.0266 over paired blocks) is **inconclusive**, not a speed win.

The [synthetic multi-function experiment 41769602461](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41769602461) successfully compiled and independently rechecked the same signed i64 clamp in 1, 16, 64, 256 and **1,024** distinct exported Wasm functions:

| Exported copies | Sum of function-region bytes saved | Sum of bytes through first return saved | ELF .text change | Whole .cwasm change |
|---:|---:|---:|---:|---:|
| 1 | 10 | 4 | 0 | 0 |
| 16 | 160 | 64 | 0 | 0 |
| 64 | 640 | 256 | 0 | 0 |
| 256 | 2,560 | 1,024 | 0 | 0 |
| 1,024 | 10,240 | 4,096 | 0 | 0 |

These are **four different metrics**. Summed disassembled function bytes do not prove total file or text-section size reduction. The experiment is synthetic and independent finite Wasm execution is not an exhaustive semantics proof.

Crucial open question: is the unchanged ELF .text due to function-block alignment, unchanged allocation, padding, or another layout detail? No cause can be declared from the aggregate alone.

The new **read-only layout audit** (qualify_aot_layout_28.py and .github/workflows/tier1-wasmtime-aot-layout-28.yml) reuses the **exact existing archived CI binaries** from run 41769602461 instead of rebuilding or assuming a candidate win. It recomputes section byte sizes and text-region hashes directly from ELF, collects function symbol totals via readelf -Ws, and cross-checks recorded object SHAs and prior independent receipts. It records cases in which text content changes while its size does not and refuses discrepancies.

Even a successful layout recheck does not establish why bytes are allocated as they are, why execution performance is neutral, or that a real application benefits. A production-focused contribution should not claim application footprint or speed improvements on these receipts alone. Retain the source-pinned catalog and pivot to a target-relevant lowering whose actual emitted layout or measured execution benefit survives verification. The VeriISLE named-rule proof remains conditional on the local provisional i64 select model.

This audit is isolated in ZenoFCIS research. It opens no upstream PR, modifies no protected branch, and does not independently claim published application-level savings.

## Independent read-only ELF layout result (2026-11-04)

[GitHub Actions #41770266100](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41770266100)
completed successfully using **the same exact archived AOT binaries** from the
previous scaling experiment and checked their source receipt, original SHA-256
and the independently generated cost receipt.

| Exported copies | ELF .text baseline and candidate | Serialized AOT baseline and candidate | ELF function-symbol size total saved |
|---:|---:|---:|---:|
| 1 | 4,096 B / 4,096 B | 13,704 B / 13,704 B | 12 B |
| 16 | 8,192 B / 8,192 B | 19,928 B / 19,928 B | 192 B |
| 64 | 20,480 B / 20,480 B | 47,408 B / 47,408 B | 768 B |
| 256 | 69,632 B / 69,632 B | 146,792 B / 146,792 B | 3,072 B |
| 1,024 | 270,336 B / 270,336 B | 568,168 B / 568,168 B | 12,288 B |

The ELF .text **contents differ** for every case despite identical .text
*section lengths*. Therefore the negative file-size result is not simply
a mistaken comparison of two byte-identical compilations. Function symbol
sizes changed and disassembly first-return bytes shrank, but **neither ELF
executable-section allocation nor serialized-module size decreased** in
these tests. This indicates a layout/allocation effect, but identifying
the exact padding, alignment or allocator cause requires additional
evidence; do not infer the cause or its generality from this audit alone.

The 12 B per function ELF symbol-size delta, 10 B disassembler function
region delta and 4 B first-return code delta are intentionally reported
separately: they are measurements of **different boundaries** and should
not be interchanged.

**Current upstream-submission verdict: Not yet.** The verified mid-end
rewrite remains a valid experiment and the code-size facts are useful
negative controls, but no smaller Wasmtime AOT executable footprint
and no proven runtime gain has been shown. The next Tier-1 candidate
should be evaluated against a **real generated workload objective** where
improved emitted assembly survives the module layout, or on controlled
hardware for instruction latency/throughput where a speed claim can be
qualified. Retain this branch and its catalog; avoid repeating this
synthetic scaling exercise as though it were independent application
evidence.
