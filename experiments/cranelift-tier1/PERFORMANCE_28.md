# ZenoFCIS 2.8 — Cranelift performance-cost continuation

**State on 2026-11-02:** a source-pinned, **exploratory performance checkpoint**, **not** a demonstrated speedup or an upstream Cranelift optimization. This research branch is forked from the existing [draft PR #175](https://github.com/TheDarkLightX/ZenoFCIS/pull/175) head at e9d97eb758d885cc242999991b7cda3b1eae94c8, rather than opening a duplicate PR or modifying ZenoFCIS main, the 2.7 release branch, or Wasmtime. A supported 2.8 integration base was not found at this inspection, so no release-branch retarget was guessed.

## Semantics and provenance

The contract is signed i64 saturating addition. The original pinned Cranelift is Bytecode Alliance Wasmtime b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49, x86-64 baseline ISA, opt_level=speed_and_size. The existing ZenoFCIS CLIF A/B receipt is [evidence/qualified_clif_ab.json](evidence/qualified_clif_ab.json); the proven equivalence is of two exact pure-result CLIF expressions in QF_BV, *not* of all hardware flags, poison, ABI states or arbitrary ISLE rewrites. The original compiler artifacts were independently executed against a signed saturation oracle on a finite 250,361-pair set each.

The two **original ELF object files** from [successful exact-head CI #41380989705](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41380989705) are saved **unchanged** at [evidence/objects](evidence/objects), with SHA-256:

- satadd.o: fd23bd8da2b5b758112bdab485a1cd1b1f45251f7705af2cc5239fb793dff107
- satadd-shift.o: 8b4e6f7fc04a4d028861cfcffff3b5652a6871ca7b09546dd4e4f77045605d17

Their *separate* original size qualification is **56 vs 39 function-symbol bytes** (17 smaller), including 10 baseline bytes of literal-pool tail/padding; executable prefix through the first return is **46 vs 39 bytes** (7 smaller). The standalone Zeno kernels' 31 vs 22 bytes are an entirely different experiment. **None of these numbers is a speed result.**

## Reproduce the performance screening

Requires Linux x86-64 SysV, GCC, GNU objcopy, Python 3.10+, and the two hash-checked ELF objects in the checkout:

~~~sh
python3 -B experiments/cranelift-tier1/benchmark_exact_objects.py \
  --objects experiments/cranelift-tier1/evidence/objects \
  --out /tmp/tier1-performance-new.json --calls 2400000 --rounds 17 --pin-cpu

gzip -dc experiments/cranelift-tier1/evidence/performance_full_20261102.json.gz \
  > /tmp/tier1-performance-original.json

python3 -B experiments/cranelift-tier1/qualify_pareto.py \
  --size experiments/cranelift-tier1/evidence/qualified_clif_ab.json \
  --timing /tmp/tier1-performance-original.json \
  --out /tmp/tier1-pareto-new.json

python3 -B experiments/cranelift-tier1/test_qualify_pareto.py
~~~

The harness verifies both original object SHA-256 values, renames **symbols only** in temporary copies for linking, performs 32,000 deterministic semantic preflight pairs with a C __int128 oracle, and checks the equality of every benchmark workload result. It collects alternating paired AB/BA **wall-clock** timings under CLOCK_MONOTONIC_RAW for a dependent non-overflow chain, a dependent saturating chain, and eight independent chains. Each call uses the original Cranelift-generated function, not a C reimplementation. It records compiler versions, source hashes, host CPU, CPU affinity, raw samples and per-mode medians, and refuses to overwrite an existing receipt.

**Scope:** these are per-call wall-clock timings *including function-call and loop overhead*, not clean single-instruction latency, peak uop throughput, frequency-controlled measurements, or a Wasmtime application benchmark. CPU contention, turbo/frequency shifts, scheduling, and architecture can mask or reverse the result. No CI timing is a gate for an upstream performance claim.

## Original exploratory run and evidence

On one pinned logical CPU of an **AMD EPYC 9V74** Linux host, 17 rounds of 2.4 million calls per variant per workload produced 102 raw timed observations. The candidate/baseline ratios below are **ratios of sample medians, not qualified speedups or slowdowns**.

| Timed workload | Baseline median ns/call | Candidate median ns/call | Candidate / baseline |
|---|---:|---:|---:|
| Dependent non-overflow | 1.946 | 2.173 | 1.116 |
| Dependent saturating | 2.022 | 2.076 | 1.027 |
| Eight independent chains | 2.240 | 2.279 | 1.017 |

The individual timing ranges overlap substantially, and a separate seven-round screening had different ratios. The experiment therefore **does not establish that either implementation is faster**.

The exact complete timing JSON is [evidence/performance_full_20261102.json.gz](evidence/performance_full_20261102.json.gz) (uncompressed SHA-256 1d9042862ae4389760f6836b415d433faff633fce9524924d6ec5710488c0527); all 102 timing values are independently preserved in compact form at [evidence/performance_raw_compact_20261102.json](evidence/performance_raw_compact_20261102.json). The derived [2.8 cost receipt](evidence/pareto_cost_qualification_20261102.json) classifies **code-size-qualified / runtime-inconclusive**, and the [fail-closed validator](qualify_pareto.py) recomputes performance medians and checks the source provenance. Seven positive/tamper controls passed **locally**; they have **not been claimed as new hosted CI results**.

## 2.8 optimization policy

For a candidate to be called *dominant*, every objective used in the comparison must have compatible, independently qualified measurements under the declared target and workload, and no objective may regress beyond its stated uncertainty/tolerance. The observed smaller symbol **alone cannot establish Pareto dominance or runtime benefit**. Until application-relevant timing is qualified, retain both candidates as potentially useful to different compiler optimization objectives.

Before proposing an upstream rewrite, qualify the actual **same-CLIF-before/after-patched-Cranelift** path. The existing proposed mid-end ISLE rule still needs a completed, pinned compiler build/measurement and all applicable VeriISLE coverage or explicit unsupported-rule limitations. Performance work must then include carefully isolated dependency/independent-stream microbenchmarks, target CPU identification, representative Wasmtime use cases, repeated measurements with uncertainty, flags/ABI/liveness and cross-target checks. The existing [PR #175](https://github.com/TheDarkLightX/ZenoFCIS/pull/175) remains draft; no upstream Wasmtime PR or generic speed claim is justified.

This continuation intentionally has **no new PR or broad CI trigger**, avoiding another repository-wide CI fan-out while the 2.8 integration destination and research workflow isolation are established.
