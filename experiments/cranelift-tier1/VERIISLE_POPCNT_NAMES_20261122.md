# VeriISLE popcount lane-local auxiliary names — pinned Tier-1 experiment

This is a **research checkpoint in ZenoFCIS**, not a Wasmtime PR. The source patch is applied **only to a disposable checkout** by the CI runner. Do not conflate this verifier correctness fix with a profitable Cranelift instruction-selection rewrite.

## Problem and change

At Wasmtime `fb792bf7a143801c22c0ba5b16fe2c6085a8598b`, `solver.rs` passes `x.index()` to the `popcnt` SMT encoder. A `lanemap` can evaluate the same expression id for different lane inputs, producing duplicate `(declare-const popcnt_8_<id> ...)` names. The alternative candidate replaces the id with `self.tmp_idx` and increments that solver-local counter at each popcount encoding. The original named SMT equations and full popcount logic are otherwise unchanged.

The exact tested diff is preserved as [patches/veriisle-popcnt-fresh-names-20261122.patch](patches/veriisle-popcnt-fresh-names-20261122.patch). This is an experiment, not an upstream submission.

## Source and tool identities

- Wasmtime: `fb792bf7a143801c22c0ba5b16fe2c6085a8598b`
- Original `solver.rs` Git blob: `8d3b5eb63482b9b6f438a517fa15e22d9c0accd9`
- Original `encoded/popcnt.rs` Git blob: `418c77c4b7c9b3bd477d31b0757de2488faacf1a`
- Patched `solver.rs` Git blob: `79e6cfe6f3042d082a56f885c6910c4224464aad`
- Runtime: Ubuntu 24.04, Rust 1.98.0, `cvc5` and `z3` installed and their versions recorded in the zipped logs; ZenoFCIS CI checkout pinned by each run's head commit.
- The research runner requires exact upstream Git hash/blob matches. It cannot silently adapt to later upstream changes.

## Red-green executable evidence

Pinned [GitHub Actions run 45037715900](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45037715900) finished **successfully**. Its [artifact 14760193516](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45037715900/artifacts/14760193516) contains the source diff, command receipts and raw verifier logs.

| Check | Pristine | Patched | Notes |
| --- | --- | --- | --- |
| 8-bit × 2-lane popcount | FAIL: solver reports duplicate `popcnt_8_10` declaration (exit 101) | PASS (1 verification) | Regression reproduces before patch |
| 32-bit × 2-lane popcount | Not run before patch | PASS (1 verification) | 64-bit total vector width, fixed input |
| 64-bit scalar popcount identity | Not run before patch | PASS (1 verification) | Same primitive encoded on both sides; **not an independent popcount proof** |
| Existing lane double-negation filetest | Not rerun before patch | PASS (2 verifications) | Basic collateral check |

Every successful verifier result above reported zero expansion errors, zero verification failures and zero unknowns. A second [pinned CI run 45037934928](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45037934928) **passed the complete VeriISLE filetest suite: 17 passed, 0 failed, 0 ignored**. This includes expected-negative fixtures, whose individual reported verification errors are intentional test outcomes. Its [full-suite artifact 14760850243](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45037934928/artifacts/14760850243) has ZIP SHA-256 `d2feca856ff9dde69c9c5e5e0ff0641e4c5812c41cdb244701b4ead1c2b1bed2`, and preserves the regression A/B logs and exact source patch. See [qualified evidence](evidence/veriisle_popcnt_freshnames_qualified_20261122.json).

The first artifact ZIP has SHA-256 `43b53958b7ea416395c439f732fabe19b76b70db22af13c363bd6c8812a9152a`. An independent parser checked all recorded log hashes, the exact baseline diagnostic, the four success logs, and that the patch changes only `solver.rs`.

## Scope and remaining acceptance criteria

This checks the SMT encoder and a set of VeriISLE filetests, **not** target code emission, x86 runtime speed, a register-allocation model or a reduction in executable bytes. The width-64 scalar test is a self-consistency control, not an independently specified proof over all 64-bit patterns. Production unsplit 64-bit VeriISLE queries, timeout behavior, complete upstream CI and target ABI correctness need their own evidence. There are no native poison, flag, trap or undefined-behavior claims in this solver-focused experiment.

Next: review the full-suite result, then run ordinary larger 64-bit verifier queries with CI-bounded budgets against both the fresh-name version and the previous expression-inlining candidate. A contribution to Wasmtime must be prepared and submitted by a human under [its AI-tool policy](https://github.com/bytecodealliance/wasmtime/blob/fb792bf7a143801c22c0ba5b16fe2c6085a8598b/AGENTS.md). The human has already contacted the maintainers; do not submit or comment automatically.
