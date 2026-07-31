# ZenoStructures v0.6.1 executed repair status

**Status:** `TESTED_ONLY / RESEARCH_ONLY / UNMOUNTED`  
**Date:** 2026-07-31

The repaired public source tree and its sealed public archive completed the validation contract in this directory.

## Executed gates

```text
public/private publication-boundary check          PASS
FCIS source and named-falsifier ratchet            PASS
Python compileall                                  PASS
complete Python test suite                         PASS
unified ZenoDEX review-derived regression lane     PASS
LFRL authority mutation campaign                   PASS
four independent PYTHONHASHSEED full-suite runs    PASS
one campaign digest across those runs              PASS
public ZIP checksum and structural integrity       PASS
clean public-ZIP extraction                        PASS
clean extracted-tree full-suite replay             PASS
second independent extracted-tree replay           PASS
```

The named regression lane prevents a broad aggregate test count from replacing the exact attacks that motivated v0.6.1. The LFRL mutation lane rejects forged discharge receipts, stale current-context receipts, crossed closed-state rollback permits, and permit reuse.

## Public archive boundary

The public source and repair bundle contain the selected structure implementations, public tests, repair ledger, validation contracts and receipts, and selected formal companion specifications. They contain no owner-private invention/search implementation or internal execution trace.

Exact archive byte lengths, SHA-256 values, test inventory, campaign identities, command results, source-manifest root, and tool availability are retained in the external public release receipt accompanying this branch.

## Formal-tool status

Prepared Lean, Julia, and ESSO companion source is not represented as an executed proof or solver result unless an exact toolchain receipt says otherwise. The Python execution record does not promote those companions.

## Authority nonclaim

Passing these gates does not authorize use in `CatalogCommitAuthority`, authenticated publication, SQLite publication, migration, recovery, delivery, or ZenoDEX runtime paths. A Rust port and production mount require separate nominal construction, strict decoding, exact vectors, no-std/Miri evidence, authenticated-source integration, datastore/crash refinement, no-bypass review, and exact-head approval.
