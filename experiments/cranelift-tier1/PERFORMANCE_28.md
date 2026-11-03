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


## Correction: prior AB/BA order bias and locally executed balanced follow-up (Nov 2)

The original v1 timing harness' comment called its two-position alternating comparisons "ABBA," but its executable loop actually emits one **AB or BA pair** per round/mode. Reanalysis of the existing 17-round raw receipt, partitioning by whether the candidate ran first or second, gives the following **descriptive ratios** of candidate-time / baseline-time (NOT measured performance benefits):

| Mode | Candidate first, median | Candidate second, median |
|---|---:|---:|
| chain_nonoverflow | 0.676 | 1.321 |
| chain_saturating | 0.915 | 1.158 |
| parallel8 | 0.918 | 1.065 |

The massive order sensitivity for the non-overflow chain invalidates treating the former aggregate ratio of 1.116 as a stable slowdown. The existing size and pure-result equivalence proofs are unaffected.

To address the methodological flaw without mutating old evidence, this separate 2.8 research branch adds:

- benchmark_order_balanced.c: executable, four-position **ABBA/BAAB** alternating blocks, rotating the three workloads, no printing inside timed blocks, reuse of the exact v1 semantic oracle and workloads.
- benchmark_order_balanced.py: rechecks the **unchanged** pinned ELF object SHA-256 values, compiles the harness, runs original Cranelift objects after symbol-name-only renaming, checks checksums and source identities, and records every position-tagged observation.
- test_benchmark_order_balanced.py: seven locally exercised positive/tamper tests, rejecting incomplete records, incorrect candidate order, altered checksums, reordered positions, invalid times and schemas.
- evidence/performance_balanced_v2.json.gz: the *complete*, compressed raw receipt, not merely derived medians; exact uncompressed SHA-256 2b760abd2567fa96d2c60bd51438dcb0605cb9927d3e61a7aae4d96e860da4c8, original gzip Git blob b6da9267e328a0a7712384fa8e956717eb1825f7.

A **local** exploratory run on AMD EPYC 9V74 with the original pinned objects, 16 rounds and 1,600,000 calls per timed sample, generated 192 position-tagged timed samples and passed all seven new tampering checks. Its within-block balanced median candidate/baseline ratios were:

| Mode | Median within-block candidate / baseline | Range of 16 block ratios |
|---|---:|---:|
| chain_nonoverflow | 0.993 | 0.882–1.184 |
| chain_saturating | 1.030 | 0.727–1.842 |
| parallel8 | 0.969 | 0.800–1.046 |

The large sample ranges and uncontrolled virtualized CPU frequency/environment **still do not justify a speedup or a slowdown claim**. Neither v1 nor v2 measured pure instruction latency, peak throughput, realistic Wasmtime workload performance, or the **same-CLIF patched-vs-unpatched** compiler rewrite.

To replay the recorded source, restore both exact object files under the already existing evidence/objects directory, then run:

~~~sh
python3 -B experiments/cranelift-tier1/test_benchmark_order_balanced.py
python3 -B experiments/cranelift-tier1/benchmark_order_balanced.py \
  --objects experiments/cranelift-tier1/evidence/objects \
  --out /tmp/tier1-balanced-reproduction.json \
  --calls 1600000 --rounds 16 --pin-cpu
~~~

The output is a **new host-specific screening**, not expected to reproduce the archived timing values. The original code and object SHA-256 evidence, source and solver pins, CLIF size comparisons, and unqualified ISLE status remain intact. No upstream PR, 2.7 release mutation, or broad CI run was initiated.


## Balanced v2 cost-catalog replay (Nov 2 continuation)

The earlier cost qualifier consumed **v1** timing, whose order confounding was
subsequently established. The isolated 2.8 branch now also includes
qualify_balanced_v2.py and test_qualify_balanced_v2.py to replay the *v2*
ABBA/BAAB receipt as the preferred performance **observation**. It validates
the exact uncompressed timing-file SHA-256, all 192 source-bound samples and
block positions, the complete recorded summaries, the benchmark-source
SHA-256 identities, the pinned host CPU and clock, original ELF-object hashes,
the qualified Cranelift size record, and both original ELF files. It refuses
forged performance qualification rather than inferring a speed winner.

Replay using only the already committed evidence:

~~~sh
python3 -B -m unittest discover \
  -s experiments/cranelift-tier1 \
  -p 'test_qualify_balanced_v2.py'
python3 -B experiments/cranelift-tier1/qualify_balanced_v2.py \
  --size experiments/cranelift-tier1/evidence/qualified_clif_ab.json \
  --timing experiments/cranelift-tier1/evidence/performance_balanced_v2.json.gz \
  --objects experiments/cranelift-tier1/evidence/objects \
  --out /tmp/tier1-v2-cost-qualification.json
~~~

In independent local execution, **11/11 new tests passed**; the existing seven
order-replay tests also passed. The generated report preserves the *first-half*
and *second-half* medians to expose time dependence: approximately 0.984 vs
1.020 (non-overflow chain), 1.126 vs 0.973 (saturating chain), and 0.908 vs
0.977 (eight chains). These are **exploratory call-level ratios**, not CPU
latency or throughput claims; no runtime winner, Pareto dominance, or
production-compiler improvement is qualified. The source files were committed
to this isolated branch, not to the 2.7 release or the upstream Wasmtime repo.


## Current rule-verification checkpoint (November 2)

The earlier [pinned same-CLIF compiler-patch run #41382217051](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41382217051) now reports success. It emitted **56 versus 39 function-symbol bytes** after installing the single experimental mid-end rule. The later [2.8 patched-object audit](recheck_patched_compiler.py) checked the original/patched objects against a finite native oracle; the balanced call timings are still exploratory and show no qualified speedup.

**New pending assurance gate:** [verify_isle_rule_28.py](verify_isle_rule_28.py) runs the pinned upstream VeriISLE checker on the **actual named opt simplify rule**, not merely the separate hand-written QF_BV clamp identity. It refuses an incorrect upstream SHA or ISLE file blob, extra tracked source changes, an altered rule body, nonzero verifier exit status, zero applicable expansions, failed/unknown checks, expansion errors, or ambiguous/incomplete counters. A successful result is recorded only with the full verifier stdout/stderr hashes, tool versions, source/rule hashes, and applicable instantiation counts. [Parser refusal tests](test_verify_isle_rule_28.py) do **not** establish a successful actual VeriISLE run.

From a checkout of this isolated 2.8 research branch, on a host with Rust/Cargo and the cvc5/z3 executables installed:

    git clone https://github.com/bytecodealliance/wasmtime.git /tmp/tier1-veriisle-wasmtime
    git -C /tmp/tier1-veriisle-wasmtime checkout --detach b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49
    python3 -B experiments/cranelift-tier1/test_verify_isle_rule_28.py
    python3 -B experiments/cranelift-tier1/install_isle_candidate.py \
      --upstream /tmp/tier1-veriisle-wasmtime --out /tmp/tier1-install-receipt.json
    python3 -B experiments/cranelift-tier1/verify_isle_rule_28.py \
      --upstream /tmp/tier1-veriisle-wasmtime --out /tmp/tier1-veriisle.json

The runner writes raw logs alongside the requested receipt; **absence of a receipt means no successful qualification**. The execution must still happen on a qualified build host. For the exact invocation and coverage limits see pinned [VeriISLE documentation](https://github.com/bytecodealliance/wasmtime/blob/b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49/cranelift/isle/veri/README.md). Even a clean named opt verification will not establish x86 flag/ABI/poison coverage, an application speedup, all-target suitability, or upstream acceptance. No new PR, merge, or broad CI trigger was initiated here.


### Explicit verifier-coverage limitation and experimental follow-up

The isolated [VeriISLE check #41571642965](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41571642965) reached the actual named-rule checker, which rejected the rule with **one expansion error and zero applicable cases**: the pinned mid-end model has **no spec for the scalar CLIF select extractor**. This is **not a discovered semantic counterexample**; neither is it a VeriISLE proof.

A subsequent isolated experiment adds a deliberately provisional scalar-i64 select spec to the detached pinned checkout. Its source is [install_veriisle_select_spec_28.py](install_veriisle_select_spec_28.py). The [VeriISLE runner](verify_isle_rule_28.py) now requires the exact original upstream spec blob and exact experimental addendum, reports the added spec as an assumption, and refuses other source drift. The condition and branch semantics follow [Cranelift's documented select operation](https://docs.wasmtime.dev/api/cranelift_codegen/ir/trait.InstBuilder.html), but the handwritten verifier spec is *not* an independently validated authoritative definition. Even if a later named-rule run passes, the result must be labeled **verified relative to the provisional select spec**, not unconditionally VeriISLE-proven. A fully authoritative select-model qualification remains a separate gate.


### Pinned VeriISLE result with explicit provisional model (November 2, New York time)

[Isolated GitHub Actions run #41572121953](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41572121953) **passed**, with the exact experimental scalar i64 select-model addendum. The named rule tier1_i64_signed_saturation_clamp had **1 applicable typed expansion, 1 verification pass, 0 failures, 0 unknowns and 0 expansion errors**, using cvc5 1.1.2 and Rust/Cargo 1.97.1. The complete raw logs, source/tool pins and local spec receipt are in [the hosted artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41572121953/artifacts/13201368995). The durable [qualified checkpoint](evidence/isle_patch_20261102/veriisle_provisional_checkpoint.json) records exact receipt/log SHA-256 values and disclaims unsupported conclusions.

The comparison is epistemically important: the *unmodified* upstream VeriISLE specifications could not expand the named rule because **select lacked a model** (run #41571642965: 0 applicable, 1 expansion error). The passing run is a **conditional proof relative to a newly supplied, unreviewed select spec** based on Cranelift's documented conditional-select semantics. It is *not* unconditional verification against the original upstream specification set. The independent QF_BV arithmetic proof, 56→39 code-size measurement and finite native checks remain separate evidence.

This is a reproducible compiler research milestone, **not yet an upstream-ready contribution**. Next: independently qualify the select specification itself (including type/condition-domain and undefined/poison semantics), test cross-target and context performance, and obtain compiler-maintainer review before proposing a general-purpose mid-end rewrite. No user action is required.


### Source-backed check of the provisional i64 `select` model (November 3, 2026 UTC)

The [isolated CI run #41613550610](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41613550610) **passed** at experimental source `3d0f9edbb9b5f0afa0a68f437c40ec26b5fdcae2`. Five source-negative-control tests passed, followed by an independent source-bound interpretation check of the *provisional* VeriISLE `select` model against the exact upstream Cranelift instruction declaration (`ScalarTruthy`, true/false branches), interpreter `Opcode::Select`, and the `DataValue::I8` nonzero truth conversion. The check covered **all 256 scalar-i8 condition values**, with symbolic branch choice and 6,400 finite representative branch-value checks. The inspected upstream is still `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`, and all three interpreting source blobs plus the original spec blob are explicitly pinned.

The exact workflow receipts are in [artifact #13219952662](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41613550610/artifacts/13219952662); the [durable checkpoint](evidence/isle_patch_20261102/select_model_source_audit_20261103.json) records the pinned sources and limits. Reproduce with `python3 -B experiments/cranelift-tier1/test_qualify_select_model_28.py` and `python3 -B experiments/cranelift-tier1/qualify_select_model_28.py --upstream /path/to/exact/pinned/wasmtime --out /tmp/new-select-audit.json`.

This is **stronger evidence for the local assumption** than an unreviewed handwritten spec alone. It is still *not* the same as upstream-approved VeriISLE semantics, a complete proof of poison/undefined behavior, a multi-rule equivalence proof, or any measured speed benefit. The named rule's previous one-applicable-case pass remains **conditional on the local select spec**. No upstream submission or release/main modification follows from this result.
