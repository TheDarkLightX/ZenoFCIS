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
