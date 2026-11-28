# CLZ fallback qualification — 2026-11-26

Resume draft #197, no upstream submission or main mutation. Current main observed at 4160086fb9d19f3ebe25047e69b1caa467aa74fd; experiment pin bcc6c19cdcd7eb4166d6ee822f2bc1c8e916f599; upstream b738a89287bb78e51c06ac9624014bbda3c1429f; lower.isle blob 545c79528cbb10fb1280623f23f53880b9762e49. Earlier ZenoFCIS bounded-synthesis readiness remains established by prior bitselect receipts; this CLZ candidate specifically comes from the independent bounded abstract BSR-result palette (40,960 combinations, two hits), not a registered native Zeno BSR encoder.

[Push CI 45897388551](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45897388551) job 140174561047 succeeded. [Artifact 15153688695](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45897388551/artifacts/15153688695) downloaded; independently measured ZIP SHA256 916dbca841d96386304f20ef028ec6894f6841135cc4184963a40f8d772eda5d agrees with API. Catalog patch SHA256 0b4ae4f1935953ca40d3eaa18e701d946ac02657755092f757025d8d8977be54; proof JSON SHA256 71ae8c796a30004ddb5eecad17bf39df7be60eb1238e73a1d21d63a1aa238c6e.

## Actual emitted ELF function symbols

Both speed and speed_and_size gave same sizes in this matrix; 12/12 no-LZCNT pairs smaller, all 12 LZCNT-enabled controls identical body hashes. Aggregate no-LZCNT 392 -> 314 bytes (19.90%), instructions 128 -> 116, with framing included; not workload-weighted.

| Width/context | Baseline bytes | XOR bytes |
|---|---:|---:|
| i32 plain |29|23|
| i32 live |32|25|
| i32 load |33|25|
| i64 plain |32|26|
| i64 live |35|29|
| i64 load |35|29|

203,616 CI native checks, all pass. Independent fresh relink and execution of all 48 objects on Intel Xeon Platinum 8573C: 560,736 fresh native oracle calls, including every highest-set-bit class with randomized lower bits, zero mismatches. Object and fixture hashes, exact decoded contiguous symbol bytes, instruction counts and body hashes independently verified against catalog. Z3 4.13.3.0 via libz3 independently replayed supplied SMT2: both candidates UNSAT, both mutants SAT. Independent quotient evaluation: 33 classes at i32, 65 at i64; nonzero index i in [0,w-1], (w-1) XOR i = w-1-i; zero case BSR destination arbitrary, CMOVE on BSR ZF selects 2w-1, XOR yields w. Flags dead after operation, unsigned bit pattern, modular result. No full decoded instruction SMT semantics, full ABI/compiler refinement, fault-order, VeriISLE or throughput claim. Valid readable load domain only. Independent bundle clz-fallback-independent-replay-20261126.zip SHA256 6e5ea73aef03a85026033e5f6fac898bf3145f6b961ea68a115b56649afd69de includes fresh replay script and receipt, raw objects, formulas, fixtures, and compiler pins; delivered separately.

## Reachability and continuation

Previously downloaded Sightglass catalog (bitselect workload experiment) at speed contains CLZ SSA assignment counts 5 XChaCha20, 31 libsodium core5 (3 explicitly i32 and 12 explicitly i64), 0 cm-online-stats, 8 hex-simd, 14 intgemm-simd = 58 static entries across five modules. Not dynamic counts, and unsuffixed types not classified. New [generic x64 workload A/B workflow](https://github.com/TheDarkLightX/ZenoFCIS/commit/e5de4a1272be88a1aa2274049379b315ae40cdf0) explicitly sets has_lzcnt=false using pinned CLI codegen flag API, records CLZ lines, native section hashes, symbol tables and partial failures. Execution not confirmed at checkpoint. The sixth GC workload may remain incomplete; do not count missing pairs as successes. Next inspect exact push-triggered run, verify patch hash and artifact, examine actual symbol costs (section padding can mask savings), run broad upstream filetests including narrow and i128 CLZ callers and named-rule verification with explicit missing specs, and time actual fallback kernels before proposing human-reviewed upstream submission. No action required yet.
