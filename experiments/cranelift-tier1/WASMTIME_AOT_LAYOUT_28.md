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
