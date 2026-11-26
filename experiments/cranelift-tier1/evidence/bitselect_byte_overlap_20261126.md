# 2026-11-26 — byte-addressed memory verification and workload CLI correction

Resume draft PR #197; no upstream submission. Current main inspected at 4160086fb9d19f3ebe25047e69b1caa467aa74fd. Research branch before this checkpoint at 6cccd061580dceb8ac2059401ef94b4eff34c879. Latest inspected upstream head b738a89287bb78e51c06ac9624014bbda3c1429f. Prior machine-synthesis and compiler gates remain passed at their own source pins, not re-proved for main.

## Independent stronger replay (executed)

Source objects are the 48 ELF objects of run 45700821940, upstream 6ddc5743e5e8cd3130151e30c1cd2b96b1d17e63 and ZenoFCIS e5e7e69018e7dff0cec89a61120cf2f8b7f33808. Rechecked object/fixture SHA-256, decoded contiguous symbol bytes and instruction counts against original catalog, frame/return, and absence of text relocation. Linked each original object with gcc and invoked under x86-64 SysV. Four independent deterministic random 24-byte initial buffers and 64-bit stores (Python Random seed 261126) per object, all 17^3 triples of starting byte offsets 0..16, observing return and entire buffer. 48 * 4 * 4913 = **943,296 fresh native invocations, zero mismatches**. Includes unaligned and partially overlapping 8-byte accesses, previously excluded by the word-slot proof. Both baseline and XOR objects pass.

Independent symbolic model uses QF_AUFBV: initial memory is an arbitrary array from BV64 addresses to BV8; little-endian load concatenates select at addr+7 through addr+0; store updates eight byte cells with corresponding extracts. Symbolic p,q,r,new are unconstrained BV64. Decode actual mov/and/or/xor/not bodies and track registers and memory. Reference executes CLIF load/store ordering and (mask & true) | (~mask & false). Ask for return mismatch OR final-array mismatch. Deduplicate by exact SMT-LIB SHA-256. Z3 4.13.3.0 (libz3.so.4), 15s per query, **10/10 UNSAT**, no unknown, across 48 instances. Negative control swaps first mask load and following store in xor-store_between-speed-bmi1_false: SAT. This admits arbitrary byte overlap, not merely equal/disjoint 64-bit words. Conditional on decoded semantics, valid accessible nonconcurrent memory, nonwrapping real addresses (model itself uses modular address arithmetic), ABI frame assumptions; no fault-order, external flags or full compiler refinement proof. No new synthesis or compilation was performed in this replay.

Replay archive delivered with this run: bitselect-byte-overlap-replay-20261126.zip (includes original objects, original replay helper, partial_overlap.py, receipt, 10 SMT-LIB queries and negative control). Run `python partial_overlap.py ci fresh-output` on Linux x86-64 with gcc, binutils, libz3.so.4. SHA-256:
- archive 904cfdd8c070d298fcc2af4c4dfc35cbf84ad4e4272197e467e1f5aab37e010f
- partial_overlap.py 13993783f96a08e427f9238d80681674253cb603218db41c1852173c706f1366
- replay.py 2ef7c755744b9b38d5ccca160ec0802e50939edb8f86357424c206700c9d7317
- byte-proof/receipt.json 27488f8c18dd6980df8579d615d3f68b73e457f88c2f52cabb851a933b566d16
- ci/catalog.json 182f487553421c224f1a98576bb090f11f29b3949d1a48ec4339cd4cf510beb2
- ci/experimental.patch 731c9589ae4d0e247c2de07d8367d1b638884306386c51d524965427d501e86b

## Workload experiment

Run 45746822754 failed before compiling the first workload: Wasmtime CLI rejected `-Oopt-level=speed` (only 0,1,2,s accepted). Baseline build succeeded; no workload comparisons. Pinned upstream crates/cli-flags/src/lib.rs TOML tests explicitly map 2 to Speed and s to SpeedAndSize. Corrected only CLI spelling in commit 6cccd061580dceb8ac2059401ef94b4eff34c879. Run 45795585224 confirmed in progress. No corpus result yet. Resume from this run, not a duplicate. If corpus text identical, record limited reachability negative evidence and consider scalar users or a different candidate; don't assert absence of scalar bitselect solely from pre-opt CLIF dumps. VeriISLE missing specifications and personal review remain outstanding.
