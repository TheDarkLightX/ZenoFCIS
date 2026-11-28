# CLZ real-workload qualification — 2026-11-27

Resume #197. Main observed 4160086fb9d19f3ebe25047e69b1caa467aa74fd, research head 313a9ab69bffd56d8969b5eb804723edb8e084de, upstream b738a89287bb78e51c06ac9624014bbda3c1429f, Sightglass f0d2040f4b1c521be5b763241f5c3b1146efff97. This is the independent abstract-palette CLZ discovery, not native Zeno BSR synthesis.

Push [CI 45935963871](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45935963871) succeeded at e5de4a1272be88a1aa2274049379b315ae40cdf0. Downloaded [artifact 15170846394](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45935963871/artifacts/15170846394); measured ZIP SHA256 dae08c287643e20fd422bfc3ddba2e2a406fa8162b0853c38b5f19509bd79955 matches API; experimental patch SHA256 0b4ae4f1935953ca40d3eaa18e701d946ac02657755092f757025d8d8977be54 matches previous fixture qualification.

10 completed A/B pairs (five modules, two optimization settings), all .text section lengths and serialized lengths unchanged; 8/10 text hashes changed, remaining two (online stats, zero CLZ) identical. Sixth module Kotlin Richards failed on all four attempts because GC array types are disabled. No runtime corpus test. LZCNT explicitly disabled.

Independently parsed raw readelf FUNC symbols by name, requiring unique names, identical symbol sets and matching section indices. Sum of function symbol lengths (not whole section length, may include intrafunction padding) for each setting separately:

| Workload | Functions | Baseline | XOR | Smaller | Larger |
|---|---:|---:|---:|---:|---:|
| XChaCha20 |101|56236|56204|3|0|
| libsodium core5 |133|87959|87741|8|0|
| online stats |94|13456|13456|0|0|
| hex SIMD |382|129171|129173|2|2|
| intgemm SIMD |1113|540679|540581|12|0|
| Total |1823|827501|827155|25|2|

Both settings give identical sizes, not independent workloads. Net -346 bytes per setting (-0.0418% of symbol sum), 1796 unchanged symbols, two larger hex SIMD functions (dlmalloc +8 and escape_debug_ext +10). Whole section net zero. No attribution of every delta to a local rule: register allocation and alignment may contribute. Actual reachability demonstrated, not broad profitability. Static CLZ count 58 per setting.

Reproduce symbol analysis with Python stdlib against artifact: for each baseline-i-opt/symbols.txt and xor-i-opt/symbols.txt, parse readelf lines with regex `^\\s*\\d+:\\s+([0-9a-f]+)\\s+(\\d+)\\s+FUNC\\s+\\S+\\s+\\S+\\s+(\\d+)\\s+(.+)$`, capture address, size, section, name, assert unique name and matching name sets/section indices; sum sizes and count signed per-name differences. Do not sum both optimization settings as unique workloads.

Fresh native timing screen (Intel Xeon Platinum 8573C virtual machine, affinity pinned, plain actual compiler-generated i32/i64 no-LZCNT objects from prior independent replay; renamed symbols only and linked shared libraries; randomized paired order, two reversed library loading orders, 40 pairs each per width/dependency regime, 1,048,576 calls per timed block, matching checksums). Median candidate/base percentage changes with percentile bootstrap interval:
- i32 independent: +5.594 [-7.849,17.025], -2.034 [-22.455,12.946].
- i32 dependent: +8.189 [-4.094,23.327], +2.380 [-5.810,12.823].
- i64 independent: +26.232 [12.521,41.535], +18.653 [1.731,36.455].
- i64 dependent: +0.925 [-8.234,19.282], -1.093 [-19.859,14.041].

i64 independent slowdown repeated in this noisy indirect-call harness; treat as a negative screen, not CPU-general throughput or workload result. Need direct/inlined loop measurement with register-allocation and BSR destination false dependency controls before any speed-default proposal; consider size-only guard if justified, and do not submit speculative patch. Raw timing and replay script delivered separately.

Source audit confirms i8/i16 and i128 lowerings call do_clz I64 I64; upstream x64/clz.clif precise-output expectations necessarily change and must be explicitly reviewed, not silently blessed. Existing LZCNT path is separate. Next run full x64/runtests A/B, specific narrow/i128 semantic controls, explicit exact-output failure classification. No upstream submission or main mutation.
