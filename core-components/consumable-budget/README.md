# consumable-budget — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `Budget` (root 100): field 110 `remaining`, integer type 105 bounded
  `0..=C`.
- Command `Spend` (root 101): field 120 `amount`, the same integer type 105.
- Supplied context `Operator` (root 102): field 130 `authorized`, bool type 108.
- Input tuple order: `[remaining, amount, authorized]`; `state_width` = 1.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `amount > remaining` → Reject 201 `insufficient_budget`, state unchanged.
  3. otherwise → Accept, `remaining' = remaining - amount`.
- Spending zero is accepted when authorized, even when `remaining = 0`.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `remaining = C`. No refill command; no effects, outbox, or
  deliveries ever.

## Laws

- 500 `bounded` on commit,genesis: `0 <= remaining' && remaining' <= C`.
- 501 `conservation` on accept: `remaining' + amount = remaining`.
- 502 `authorized_nonincreasing_budget` on accept: `authorized = true` and
  `remaining' <= remaining` (strengthened so the law reads state, not only
  context — required by the substantive-law linter).

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity.
- These are bounded resource units; this component does not claim financial
  settlement.
- Component 400 `budget` owns state 100, reads `pre.100`, writes `post.100`,
  consumes `context.102`; step budget 64; `merge [400]`.
- All root/type/field/reason IDs are fixed local IDs. They MUST be rebound
  explicitly in any future composition; isolated proofs of this family will
  not establish cross-component laws.

## Status

The ordinary installer requires a fresh source-bound finite certificate and
refuses missing or stale evidence. Its stored reference is **Identified**;
actual exhaustive replay establishes **Proved for the closed finite scope**
under the [named trusted base](../FINITE_PROOFS.md). The independent Root
reference and the complete example corpus check every declared input tuple.
`authority` remains none and `family_theorem` remains null: this is not an
unbounded parameter theorem, Lean `KernelChecked`, authentication or release
approval. See the [catalogue workflow](../README.md) for discovery, installation
and current-build replay.
