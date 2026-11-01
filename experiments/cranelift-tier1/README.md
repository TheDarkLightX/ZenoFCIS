# First Tier-1 Cranelift experiment: signed i64 saturating add

**Status:** isolated experimental checkpoint, **not an upstream optimization or a release**. ZenoFCIS source parent: `31afaef8d9fb723b44918f430e10d63f0cb93ccf` (draft release PR #174). Upstream Wasmtime/Cranelift pin: `b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`. This project deliberately starts without waiting for a 2.7 tag or unrelated benchmarks.

## Validated hypothesis versus unvalidated Cranelift claim

The ZenoFCIS machine-code library already contains two proposals for **the same** signed-saturating `i64 + i64 -> i64` semantics (profile `x86_64-sysv-register/2`). Its older `signed-saturating-add-x86` emits **31** raw x86 bytes (7 body instructions); `signed-saturating-add-adc-x86` emits **22** raw x86 bytes (6 body instructions), including the `ret`. The 22-byte version is already an existing **proposal**, not a result discovered by this Cranelift project. The 9-byte difference is **only between those two stand-alone, fixed-ABI kernels**.

An independent local Linux x86-64 test assembled and called both exact byte sequences through the SysV ABI, with an independent unbounded-Python signed reference and separately compiled GCC `-O2` `__int128` C reference. The 17x17 boundary grid and 20,000 deterministic pseudo-random input pairs (20,289 total per function) all agreed. See `evidence/native_check.json` and executable `native_check.py`. This is **finite native testing**, not 2^128 exhaustive proof or a measured Cranelift improvement.

The ZenoFCIS independent profile-2 SMT checker can quantify all 64-bit x/y inputs and initial scratch/flag states under its modeled ISA. `probe.py` regenerates those fresh checker decisions and a deduplicated receipt-bearing catalog on a qualified CI host, runs an actual bounded candidate-palette search, and emits/replays a chosen artifact. Each recorded `unknown`/budget outcome is an **inconclusive** result, not a correctness or optimality proof. Native and SMT evidence are not interchangeable.

## Real Cranelift connection, with the correct abstraction boundary

The upstream `cranelift/codegen/src/isa/x64/lower.isle` already lowers `sadd_overflow` with x86 overflow flags; `cranelift/codegen/src/opts/arithmetic.isle` contains preexisting arithmetic rewrites. Scalar saturating i64 addition is **not** an invented scalar `sadd_sat` instruction; `satadd.clif` expresses its semantics using `sadd_overflow`, an input-sign check, and selects. The targeted workflow checks out the exact pinned upstream, tests **both** pure CLIF formulations and their finite execution fixtures, regenerates a fresh **QF_BV equivalence proof** tied to those exact expression bodies, compiles both under identical `x86_64` / `opt_level=speed_and_size` settings, and records whole-function symbol bytes plus disassemblies and hashes. **This describes configured CI steps, not a passed run or a measured reduction until actual workflow artifacts are inspected.** A successful Cranelift build is **a baseline measurement**, not evidence that the 22-byte Zeno kernel is a safe drop-in ISLE rewrite. That would require register-allocation constraints, flags liveness, semantic matching, independent VeriISLE/SMT checking and a same-configuration A/B whole-function benchmark.

In particular, native x86 bytes, CLIF operation counts, ISLE pattern cost, and whole-function emitted byte counts are **different** metrics. The project records them separately and refuses claims of upstream savings until a real Cranelift A/B exists.

## Reproduction

From the ZenoFCIS checkout at the experimental branch:

```sh
python3 -B experiments/cranelift-tier1/native_check.py --out /tmp/tier1-native
python3 -B experiments/cranelift-tier1/test_probe.py
python3 -B experiments/cranelift-tier1/probe.py --out /tmp/tier1-probe
```

For the last command, use the repository's hash-pinned Z3 wheel, as in `.github/workflows/machine-code.yml`, and an x86-64 Linux runner. It produces `catalog.json`, per-sequence verification receipts, a search result and an emitted/replayed artifact. It refuses missing/changed semantic requirements or non-equivalent checks. The GitHub CI additionally runs pinned Cranelift `clif-util` tests/compile on `satadd.clif` and `satadd_shift.clif` and produces a receipt-bound A/B via `measure_ab.py`; inspect both object files and the actual JSON report **before** claiming CLIF code-size savings or changing any ISLE rule. The CLI `--set opt_level=speed_and_size` is intentional: the pinned upstream `cranelift/src/compile.rs` selects command-line ISA settings, which otherwise can supersede the `.clif` file's header settings.

## Acceptance gates for an upstream draft PR

1. Fresh Zeno profile-2 SMT equivalence on both exact native sequences, no `unknown`, with signedness and overflow declared; finite native checks separately.
2. At least one bounded executable palette search under pinned source/solver, honest stopping reason and cost; content-key deduplicated catalog binds contracts, raw machine bytes, solver/source identities and each fresh verification receipt. (A proposer-created 22-byte program is **not** called an enumerator discovery.)
3. Compile **both** exact CLIF formulations with the same Cranelift target and explicit optimization setting. Bind the QF_BV receipt to the exact source bodies and record full object/symbol sizes, disassembly, compiler pins and actual delta. A smaller CLIF result is *not* automatically a beneficial or safe ISLE change.
4. Verify the rewrite against the relevant CLIF/ISLE semantics and current upstream VeriISLE coverage (or state unproven); compare real emitted bytes and timings with repetitions. No speculative upstream PR without a reliable saving.

We have permission to edit our experimental ZenoFCIS branch only; there is no push access to `bytecodealliance/wasmtime`. Never alter ZenoFCIS `main`, the 2.7 release draft, or upstream in this experiment.
