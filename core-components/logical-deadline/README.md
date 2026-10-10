# logical-deadline — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `Deadline` (root 100): field 110 `deadline` and field 111
  `last_seen` (integer type 105 bounded `0..=C`), field 112 `reached`
  (bool type 106).
- Command `Tick` (root 101): field 120 `now`, integer type 105 bounded
  `0..=C`.
- Supplied context `Operator` (root 102): field 130 `authorized`, bool
  type 108.
- Input tuple order: `[deadline, last_seen, reached, now, authorized]`;
  `state_width` = 3.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `now < last_seen` → Reject 201 `regressive_now`, state unchanged.
  3. `reached` → Reject 202 `already_reached`, state unchanged.
  4. `now < deadline` → Reject 203 `deadline_not_met`, state unchanged.
  5. otherwise → Accept, `deadline' = deadline`, `last_seen' = now`,
     `reached' = true`.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `deadline = C`, `last_seen = 0`, `reached = false`. No effects,
  outbox, or deliveries ever.
- All declared state tuples are admitted inputs; in particular
  `last_seen <= deadline` is NOT an input restriction.

## Laws

- 500 `bounded` on commit,genesis: `0 <= deadline' && deadline' <= C &&
  0 <= last_seen' && last_seen' <= C` (all numeric post fields; `reached`
  is bool).
- 501 `authorized_bounded_result` on accept: `authorized = true` and all
  numeric post fields within `0..=C` (strengthened so the law reads state,
  not only context — required by the substantive-law linter).
- 502 `exact_deadline` on accept: `now >= pre.last_seen` and
  `now >= pre.deadline`, `post.deadline = pre.deadline`,
  `post.last_seen = command.now`, `post.reached = true`.

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity. Treating it as authentication is a misuse of this component.
- `now` is a supplied logical value — not a clock read and not evidence of
  physical time passing.
- Component 400 `deadline` owns state 100, reads `pre.100`, writes
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
