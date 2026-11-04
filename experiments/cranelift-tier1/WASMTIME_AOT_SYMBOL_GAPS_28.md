# Alignment hypothesis: signed clamp improvements swallowed by Wasmtime AOT layout

The [Nov 4 independently rechecked AOT experiment](WASMTIME_AOT_LAYOUT_28.md)
reports a **12-byte signed i64 clamp function-symbol saving** per exported
function, but unchanged ELF `.text` and serialized `.cwasm` sizes even with
1,024 exported copies. This is a negative application-footprint result, not
a runtime speedup.

**A source-level mechanism is now identified at pinned Wasmtime
`b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49`:**

- `cranelift/codegen/src/isa/x64/inst/mod.rs` advertises a *preferred*
  function alignment of 32 bytes (minimum one byte). Preferred alignment is
  not proof that every compiled function used it.
- `cranelift/codegen/src/machinst/buffer.rs`
  `MachTextSectionBuilder::append` explicitly calls
  `self.buf.align_to(align)` **before** writing each function body.
- `crates/cranelift/src/obj.rs` uses `compiler.page_size_align()`
  for executable/non-Pulley `.text` alignment and passes the result
  to `section_mut(...).set_data(...)`.

The observed multiples of 4096 ELF `.text` bytes and unchanged section
lengths are **consistent with** these explicit mechanisms. Source alone
does not distinguish padding between individual functions from page
alignment at the end of `.text`, nor rule out trampolines, veneers,
relocations, or other object-layout behavior.

The new source-bound executable audit,
`qualify_aot_symbol_gaps_28.py`, examines the **same preserved ELF
binaries** from [run #41769602461](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41769602461).
It independently checks Git blob pins and architectural source assumptions,
re-runs the previous complete AOT receipt audit, then compares `readelf`
Wasm-named function symbols across baseline and patched binaries. It tests
whether their **start offsets are unchanged**, the function sizes shrink,
and the extra bytes appear precisely in the previously unused interval
before the next named symbol. It also reports alignment observations.

If names are absent or symbol starts differ, it **refuses causal
attribution** and reports inconclusive findings rather than manufacturing
a padding explanation. The script is tested with positive and deliberately
corrupted parser fixtures, and CI preserves its full result.

The target is *diagnosis*, not a speculative upstream optimization.
Even a confirmed local gap pattern will not imply that disabling alignment
is desirable: Cranelift explicitly notes a performance rationale for
preferring 32-byte boundaries. The next independent candidate should be
judged using target-appropriate actual footprint and controlled execution
measurements, with substantive verification of exact CLIF/ISLE semantics.

**Status:** Check the exact pinned
`tier1-wasmtime-aot-symbol-gaps-28` GitHub Actions run before describing
the attribution as executed or proven. No changes to ZenoFCIS main/release,
Bytecode Alliance upstream or any production compiler.

## Pinned source-and-real-ELF result — November 4, 2026

The targeted [Actions run #41803963706](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41803963706)
**passed** on research commit
[`75c7cefaffe2`](https://github.com/TheDarkLightX/ZenoFCIS/commit/75c7cefaffe2abee3cd15b22762a9a3e0fc7ca60).
It downloaded the existing qualified AOT objects from run #41769602461,
checked the pinned upstream Git commit and exact three source blobs, passed
**five independent parser/refusal controls**, and independently recomputed
the complete ELF binary/source receipts before comparing symbol spacings.

For all 16, 64, 256 and 1,024-export test cases, **each exported function's
ELF symbol start offset was identical** between the two compiler versions.
The signed-saturating function body symbol was 12 bytes shorter with the
experimental rule, while every measurable interval to its next named
function grew by exactly 12 bytes. Therefore function-size savings were
absorbed by extra *inter-symbol gap bytes* rather than shifting subsequent
function starts:

| Copies | Symbol-body reduction | Measured extra gap bytes | Measurable adjacent pairs |
|---:|---:|---:|---:|
| 1 | 12 | 0 | 0 (no next symbol; inconclusive) |
| 16 | 192 | 180 | 15 of 15 |
| 64 | 768 | 756 | 63 of 63 |
| 256 | 3,072 | 3,060 | 255 of 255 |
| 1,024 | 12,288 | 12,276 | 1,023 of 1,023 |

The final function in each module has no following named function, so its
remaining 12 bytes cannot be attributed by this particular pairwise
measurement. The existing independent audit separately confirms unchanged
`.text` size and unchanged full `.cwasm` size for all cases.

This is now an **observed cause of the lost between-function size reduction**,
not merely a guessed explanation from identical aggregate section lengths.
The pinned source's 32-byte preferred function alignment and explicit
`align_to` call supply a direct plausible mechanism; the audit does not
prove the provenance of each gap byte or conclusively separate all
allocator/veneer/page-alignment contributions.

**Engineering decision:** a Tier-1 cost objective must distinguish raw
machine-code bytes, actual function-symbol bytes, **the inter-function
aligned allocation**, whole executable `.text`, and complete AOT file
size. Prioritize candidates that actually cross an allocation boundary,
reduce executable-section pages, or have independently reproducible speed
benefits, rather than optimizing symbolic bytes alone. Do not propose
disabling preferred alignment as an optimization: it may harm branch
prediction or cache alignment. The existing clamp rule remains a valid
bounded semantics/code-size research result but is not yet a defensible
upstream application performance improvement.

The [raw qualified source/ELF audit artifact](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41803963706/artifacts/13308522856)
retains exact identities and case-level observations.
