# idempotency-slot — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `Slot` (root 100): field 110 `seen` (bool type 106), field 111 `key`
  and field 112 `value` (integer type 105 bounded `0..=C`).
- Command `Record` (root 101): field 120 `key` and field 121 `value`, both
  integer type 105 bounded `0..=C`.
- Supplied context `Operator` (root 102): field 130 `authorized`, bool
  type 108.
- Input tuple order: `[seen, key, value, command.key, command.value,
  authorized]`; `state_width` = 3.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `seen && state.key = command.key && state.value != command.value` →
     Reject 201 `conflicting_replay`, state unchanged.
  3. otherwise → Accept, `seen' = true`, `key' = command.key`,
     `value' = command.value`.
- A repeated matching pair accepts with an identical successor state.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `seen = false`, `key = 0`, `value = 0`. No effects, outbox, or
  deliveries ever.

## Laws

- 500 `bounded` on commit,genesis: `0 <= key' && key' <= C &&
  0 <= value' && value' <= C` (all numeric post fields; `seen` is bool).
- 501 `authorized_bounded_result` on accept: `authorized = true` and all
  numeric post fields within `0..=C` (strengthened so the law reads state,
  not only context — required by the substantive-law linter).
- 502 `exact_record` on accept: `post.seen = true`,
  `post.key = command.key`, `post.value = command.value`, and
  `pre.seen && pre.key = command.key` implies `post.value = pre.value`.

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity. Treating it as authentication is a misuse of this component.
- The slot remembers only the latest key, not every historical key, and does
  not guarantee global exactly-once execution.
- Component 400 `slot` owns state 100, reads `pre.100`, writes `post.100`,
  consumes `context.102`; step budget 128; `merge [400]`.
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
