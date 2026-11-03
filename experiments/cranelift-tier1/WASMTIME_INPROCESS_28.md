# In-process Wasmtime runtime qualification (2.8, Tier 1)

**Motivation:** The prior real Wasmtime frontend test reached the mid-end
rule and independently verified its finite arithmetic behavior. It measured
64→52 bytes for the signed saturating-add AOT *function* but no change to
the page-aligned 13,912-byte serialized module. An exploratory CLI timing
ratio near 1.001 was dominated by startup/loading; it was not evidence of
instruction speed or throughput.

This experiment removes Wasmtime CLI startup and module loading from the
timed region. It uses the **exact** source-pinned AOT ELF modules preserved
by [GitHub Actions run 41724158745](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41724158745),
whose SHA-256 digests are explicitly checked, then loads each through a
single Wasmtime runtime engine compiled from the same pinned upstream
revision `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`.
The benchmark's Cargo package explicitly avoids enabling an unrelated
Wasm compiler backend. Its generated dependency lockfile and build logs
are archived with every CI experiment.

The benchmark first checks **1,036 finite signed-add pairs** plus eight
Wasm-loop counts **per module**, against a separate i128 reference.
The guest-loop benchmark then runs one precompiled Wasm workload from a
long-lived `Instance`/`Store`, using balanced ABBA/BAAB orders and no
console output inside timing blocks. It retains every raw duration, a
checksum and the source/binary identities. The evaluator refuses incomplete
or forged balanced orders, missing prior finite evidence, altered object
hashes, negative/zero/invalid durations, incorrect checksums and invalid
budgets.

This is **exploratory in-process guest workload duration**, not a direct
instruction-latency or peak throughput benchmark, and certainly not a
generic speedup claim. A shared GitHub-hosted CPU has variable clock and
contention. Candidate and baseline must be compared over actual paired
blocks, with first/second-half medians and full dispersion disclosed. Both
implementations are retained in the catalog regardless of result.

The targeted workflow is `.github/workflows/tier1-wasmtime-inprocess-28.yml`.
It is research-only; it does not modify main, release/2.7.0, or upstream
Wasmtime. The named ISLE proof remains conditional on a provisional local
`select` specification. Follow-up for upstream: independent authoritative
select modeling, representative workloads and performance on controlled
hardware, and maintainers' feedback.

CI status is **not asserted** by this source file; consult the exact
workflow run and raw receipt.
