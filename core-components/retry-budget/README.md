# retry-budget — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `RetryBudget` (root 100): field 110 `remaining` (integer type 105
  bounded `0..=C`) and field 111 `closed` (bool type 106).
- Command `Attempt` (root 101): field 120 `finish` (bool type 106).
- Supplied context `Operator` (root 102): field 130 `authorized`, bool
  type 108.
- Input tuple order: `[remaining, closed, finish, authorized]`;
  `state_width` = 2.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `closed` → Reject 201 `closed`, state unchanged.
  3. `!finish && remaining = 0` → Reject 202 `retries_exhausted`, state
     unchanged.
  4. `finish` → Accept, `remaining' = remaining`, `closed' = true`.
  5. otherwise → Accept, `remaining' = remaining - 1`, `closed' = false`.
- A `finish` command is accepted even when `remaining = 0`; closing the
  budget does not consume a unit.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `remaining = C`, `closed = false`. No effects, outbox, or
  deliveries ever.

## Laws

- 500 `bounded` on commit,genesis: `0 <= remaining' && remaining' <= C`
  (all numeric post fields; `closed` is bool).
- 501 `authorized_bounded_result` on accept: `authorized = true` and all
  numeric post fields within `0..=C` (strengthened so the law reads state,
  not only context — required by the substantive-law linter).
- 502 `exact_finish` on accept: `finish` implies `post.remaining =
  pre.remaining` and `post.closed = true`.
- 503 `exact_attempt` on accept: `!finish` implies
  `post.remaining + 1 = pre.remaining` and `post.closed = false`.

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity. Treating it as authentication is a misuse of this component.
- This is a logical retry allowance only; it neither schedules nor performs
  retries.
- Component 400 `retry_budget` owns state 100, reads `pre.100`, writes
  `post.100`, consumes `context.102`; step budget 128; `merge [400]`.
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
