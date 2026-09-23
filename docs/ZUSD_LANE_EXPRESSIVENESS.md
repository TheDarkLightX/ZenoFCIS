# zUSD lane expressiveness with V1 representations

## Purpose

This record states what the single-vault zUSD lane needs, and which of those
needs `.zeno` version 1, the `finite-i64/1` IR, and V1 law checking can state
and check. It is evidence for the decision recorded in the
[design records](adr/README.md): extend `.zeno` v1 and the finite IR, or build a
separate semantic core.

## Method

- **Source of truth.** ZenoDEX commit
  `e3cf1aad40e487893230ae0c55f1f0dda62f9955`, the revision the
  [mounted zUSD refinement](MOUNTED_ZENODEX_ZUSD_V1.md) pins:
  - `src/core/zusd.py` is the authority;
  - `rust-runtime/crates/zenodex-runtime-core/src/zusd.rs` is its Rust shadow.
- **Attempt.** `crates/zeno-fcis-spec/tests/data/zusd_lane_v1.zeno` states:
  - the 32 state fields;
  - the command;
  - all 46 reasons of `ZusdRejectV1`, with registry precedence;
  - five state invariants and three action laws.

  It passes `zeno-fcis check`, and every law is `may-constrain-transition`.
- **Tests.** `crates/zeno-fcis-spec/tests/zusd_lane_gaps.rs` pins each language
  finding with the library's own parser and evaluator. A change that closes a
  gap fails its test.
- **Native reason order.** Reproduced by running the pinned Python authority's
  `_step_python` on inputs where two checks fail. The Rust shadow was
  inspected, not run.

## What the lane needs

- **State.** 32 integer fields, one of them Boolean. Every stored amount is at
  most 10^30.
- **Commands.** 12 actions. Numeric arguments are positive integers with no
  upper bound.
- **Arithmetic.**
  - Collateral-ratio products reach about 2^213. Both engines compute them with
    arbitrary-precision integers.
  - Fees use ceiling and floor division, saturating subtraction, and minimum.
- **Decisions.** A command is either accepted, or rejected with the reason of
  its first failing check, in a per-action order. The native code emits 43
  distinct reason codes.
- **Two-phase checks.** `bounded_check_failed` and `invariant_violation` are
  checked before the action, on the pre-state shape. They are checked again
  after it, on the post-state bounds and invariants.

## Findings

| # | Need | `.zeno` v1 | `finite-i64/1` | Evidence |
| --- | --- | --- | --- | --- |
| 1 | 32 fields with amounts up to 10^30 | Declarable. Bounds are laws, because `int` types carry no range. Literals stop at 2^64 − 1, so 10^30 is written as `10^15 * 10^15`. | Not representable. Values are checked i64 (at most about 9.2 × 10^18). A program has at most 32 inputs and 16 outputs, and a synthesis contract at most 16 of each. Exhaustive checks admit at most 65,536 input tuples. The lane needs 35 inputs and 33 outputs. | `zeno_v1_integer_literals_stop_at_the_u64_range`; `MAX_FIELDS` and `MAX_INPUTS` in the finite IR |
| 2 | Collateral-ratio and solvency checks | Writable, but the checked i128 products overflow inside the declared domain. `no_bad_debt` is `Indeterminate(Overflow)` at collateral and price 10^20. | Not representable | `zusd_lane_solvency_law_overflows_inside_the_declared_domain` |
| 3 | Grouped arithmetic | A comparison cannot start with a parenthesized scalar. `(a + b) * c` fails to parse, while `c * (a + b)` parses, so the attempt distributes products by hand. | Not applicable | `zeno_v1_comparisons_cannot_start_with_a_parenthesized_scalar` |
| 4 | Fees and liquidation compensation | `div_ceil` and `div_floor` exist. Minimum and saturation have no operator, conditional expression, or local binding, so each becomes a case split that repeats the whole expression. Not attempted. | Not representable | Language reference |
| 5 | Which reason a rejection carries | Not stateable. A reason declares a name and one global rank, but no condition. Law paths cannot refer to the decision or its reason. So an effect law must admit both outcomes: the deposit law holds for the accepted and the rejected deposit alike. | Output codes can carry a reason, but see row 1 | `zusd_lane_laws_cannot_tell_an_accepted_deposit_from_a_rejected_one` |
| 6 | Reason order | Not reproducible. See [Reason order](#reason-order). | Not applicable | Pinned Python authority |
| 7 | Frame: fields a step leaves unchanged | Expressible only by listing every field. The deposit law needs 64 comparisons, 63 of them frame equalities. | Not applicable | Law 510 in the attempt |
| 8 | Conservation over fixed fields | Expressible: supply conservation, liquidation collateral conservation, and the matching debt burn. Conservation across vaults would need collections, which v1 lacks. | Not representable | Laws 501, 511, and 512 |
| 9 | Checked field references | Elaboration does not resolve paths against the schema: `post.100.999`, `command.101.777`, and `post.555.1.2.3` elaborate without a diagnostic. `zeno-fcis check` now warns about them, and `--require-resolved-paths` refuses them ([law path resolution](LAW_PATH_RESOLUTION.md)). | Not applicable | `zeno_v1_accepts_formula_paths_that_name_no_declared_field` |
| 10 | Unbounded command arguments | A bounded schema rejects a large argument at admission, where the native code rejects it through the action's own checks. The mounted `ZusdCommandV1` holds amounts as `u128`, so an argument of 2^128 or more is not representable. | Not representable | `ZusdCommandV1` |

### Reason order

The transition builder selects the applicable reason with the lowest global
rank. The native code instead returns its first failing check in per-action
order. The pinned Python authority returns these reasons:

| Input | Native reason (rank) | Lowest-ranked failing check |
| --- | --- | --- |
| `bootstrap_oracle` with `price_e8 = 0` on a bootstrapped state | `oracle_already_bootstrapped` (4) | `not_positive_int` (0) |
| `oracle_report` with `price_e8 = 0` on an unbootstrapped state | `oracle_not_bootstrapped` (6) | `not_positive_int` (0) |
| `withdraw_collateral` above the collateral, on a state that breaks supply conservation | `insufficient_collateral` (11) | `invariant_violation` (2) |

In the Rust shadow, `invariant_violation` is also the reason for a malformed
pre-state, checked before any action check. It would need to rank both before
and after the action reasons. No single rank per reason reproduces the native
reason. A ZenoFCIS program must therefore return early in native order, and the
committed precedence then does not describe which reason wins. The mount is
unaffected, because its adapter copies the native reason.

### Registry and code

`ZusdRejectV1` lists 46 reasons. At the pinned revision, three of them appear in
neither engine nor the Python reason mapping:

- `commit_stale_oracle`;
- `liquidate_pending_mismatch`;
- `liquidate_stale_oracle`.

The other 43 appear in both engines. Nothing in V1 checks that a catalogued
reason can occur.

## What V1 carries today

- Every declaration: the 32 fields, the command, and the 46 reasons with
  registry precedence.
- State invariants, checked at realistic magnitudes.
- Conservation laws over fixed fields.
- Action effect laws in the form "the effect happened, or nothing changed". A
  runtime law check detects a wrong effect: a deposit that credits one unit too
  much is `False`.

## Implications for the decision

Stating the lane needs at least these additions, whichever option is chosen:

- reason conditions, with decision and reason observations in laws;
- per-action reason order, or first-failing-check semantics in the order the
  author writes;
- a frame construct;
- path resolution in elaboration (1.x `check` now reports unresolved paths);
- minimum, conditional expressions, and local bindings;
- evaluation beyond i128, and literals beyond u64.

The candidate semantic-core design covers reason conditions, the frame, path
resolution, and conditional expressions. Its decision rows follow one catalog
precedence, so as designed it would not reproduce the per-action order either.
Neither option has wide integers yet.

The finite IR cannot carry the lane's arithmetic. It could carry a Boolean
skeleton of the checks, which condition fails first, only if every arithmetic
comparison became a Boolean input. That would need separate evidence that those
inputs are computed correctly.

## Reproduction

```sh
cargo +1.97.1 test -p zeno-fcis-spec --test zusd_lane_gaps --locked
cargo +1.97.1 run -p zeno-fcis-cli --locked -- check \
  crates/zeno-fcis-spec/tests/data/zusd_lane_v1.zeno --format json
```

To reproduce the native reasons, copy `src/core/zusd.py`,
`src/core/zusd_multi_oracle_commit_mcr.py`,
`src/core/zusd_multi_redeem_selector.py`, and `src/state/canonical.py` from the
pinned commit into an importable `src` package. Then call
`zusd._step_python(state, zusd.ZUSDCommand(tag=..., args=...))` with the inputs
in the table above.

## Explicit nonclaims

- The attempt states a subset of the lane: five invariants and three action
  laws. It is not a complete specification, and the lane's owner has not
  reviewed it.
- The findings describe `.zeno` v1 and `finite-i64/1` at v1.1.0 plus this
  branch. They make no claim about the correctness of ZenoDEX.
- The native reason order was reproduced with the Python authority only.
