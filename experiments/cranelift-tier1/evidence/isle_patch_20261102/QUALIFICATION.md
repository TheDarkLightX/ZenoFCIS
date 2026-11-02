# Tier-1: exact same-CLIF ISLE-patched compiler checkpoint

**2026-11-02 UTC; experimental, NOT upstream qualified.**
This is a continuation of ZenoFCIS draft [PR #175](https://github.com/TheDarkLightX/ZenoFCIS/pull/175), on the separate 2.8 research branch only. Neither main nor the 2.7 release line nor bytecodealliance/wasmtime has been changed.

## Actual compiler experiment (not two manually selected CLIF programs)

GitHub Actions [run 41382217051, attempt 2](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41382217051) **succeeded** in all three targeted jobs. Its [original artifact 13182951192](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41382217051/artifacts/13182951192) contains the exact before/after objects, patch, source-identity receipt, independent Z3 proof receipt, disassemblies and cost receipt.

- ZenoFCIS experiment head: e9d97eb758d885cc242999991b7cda3b1eae94c8
- Wasmtime/Cranelift: b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49
- Exact source: same satadd.clif input for both compiler builds, x86_64 baseline ISA, opt_level=speed_and_size
- Single mid-end ISLE rule: tier1_i64_signed_saturation_clamp in cranelift/codegen/src/opts/selects.isle
- Rule SHA-256: f3ef723b7578f3b848f06f5882cb89eb85b1f37c6478405a74bb53627ea4342a
- Independent clamp QF_BV proof: unsat counterexample query; **not** a VeriISLE rule-chain or whole-machine proof.
- Actual unpatched/ISLE-patched function-symbol bytes: **56 / 39** (17-byte / 30.4% reduction).
- Actual executable prefix through ret: **46 / 39** bytes; the 10-byte baseline tail is in-symbol data/padding, not executed instructions.
- Original ELF SHA-256: 1d3b385c60fac3f0687d1b71d44a085beb09cbbeb8078510e7aebf334daf6880
- Patched ELF SHA-256: affc09034d1bfcbff3c8c2c69913dc2da8935f39009daa97842d18ef0db53d90

Both exact ELF object files are retained beside this note, independently hash-checked and independently examined using nm/objdump. They are not the older manually chosen two-CLIF objects. This is the **first measured same-input automatic compiler rewrite** in this experiment.

## Independent native execution and runtime screening

The standalone recheck_patched_compiler.py independently validates the patch/cost/QF_BV receipt linkage, ELF hashes and symbol sizes, then dynamically calls both exact pinned SysV objects against an **unbounded Python integer signed-saturation oracle**. Result: **253,136 / 253,136 input pairs per object passed**, comprising 3,136 boundary-grid pairs plus 250,000 seeded pseudorandom pairs. Finite testing is not a complete machine-code or ABI proof.

The separate benchmark_patched_compiler.py runs existing position-balanced ABBA/BAAB call-level workloads on these **actual** patched/unpatched compiler objects (after symbol-name-only temporary renaming, never changing the archived originals). Three independent 16-round x 3-workload x 4-position screens, each with 1,600,000 calls per timed observation, were run on a Linux x86_64 AMD EPYC 9V74 virtualized host with CPU affinity. They are **exploratory** only:

| Workload | Run 1 candidate/baseline median | Run 2 | Run 3 |
|---|---:|---:|---:|
| Non-overflow dependency chain | 1.261 | 1.034 | 1.069 |
| Saturating dependency chain | 1.294 | 1.042 | 1.100 |
| Eight independent chains | 0.922 | 0.908 | 0.965 |

Ratios include call/loop overhead. The substantial within-run spread and cross-run variation mean **no qualified speedup or slowdown**, latency figure, or peak throughput conclusion. The candidate is **size-qualified but performance-unqualified**; no Pareto dominance is established.

Reproduction after extracting the artifact named above (which includes patch-receipt.json, rule-proof.json, patched-cost.json and the two original .o files):

~~~sh
python3 -B experiments/cranelift-tier1/recheck_patched_compiler.py \
  --dir /path/to/extracted/ci-artifact \
  --random-pairs 250000 --out /tmp/unique-isle-native-check.json
python3 -B experiments/cranelift-tier1/benchmark_patched_compiler.py \
  --artifacts /path/to/extracted/ci-artifact \
  --source experiments/cranelift-tier1 \
  --calls 1600000 --rounds 16 --pin-cpu \
  --out /tmp/unique-isle-timing.json
~~~

Native check is fail-closed on receipt, compiler pin, original ELF bytes and symbol sizes, including under optimized Python (-O). Keep its qualified domain explicit.

**Before upstream submission:** independently verify the actual ISLE rule/expansion with VeriISLE where supported (record precise unsupported edges otherwise), qualify the full lowering/ABI/flags/undefined-state assumptions, measure repeated architecture-specific performance and Wasmtime regressions, and confirm applicability on the contemporary upstream revision. No performance win or upstream readiness has been claimed.
