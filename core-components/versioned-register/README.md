# versioned-register — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `Register` (root 100): field 110 `value` and field 111 `version`,
  both integer type 105 bounded `0..=C`.
- Command `Write` (root 101): field 120 `expected_version`, field 121
  `new_value`, both integer type 105 bounded `0..=C`.
- Supplied context `Operator` (root 102): field 130 `authorized`, bool
  type 108.
- Input tuple order: `[value, version, expected_version, new_value,
  authorized]`; `state_width` = 2.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `expected_version != version` → Reject 201 `version_mismatch`, state
     unchanged.
  3. `version = C` → Reject 202 `at_max_version`, state unchanged.
  4. otherwise → Accept, `value' = new_value`, `version' = version + 1`.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `value = 0`, `version = 0`. No effects, outbox, or deliveries
  ever.

## Laws

- 500 `bounded` on commit,genesis: `0 <= value' && value' <= C &&
  0 <= version' && version' <= C` (all numeric post fields).
- 501 `authorized_bounded_result` on accept: `authorized = true` and all
  numeric post fields within `0..=C` (strengthened so the law reads state,
  not only context — required by the substantive-law linter).
- 502 `exact_write` on accept: `expected_version = pre.version`,
  `post.value = command.new_value`, `post.version = pre.version + 1`.

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity. Treating it as authentication is a misuse of this component.
- This models logical optimistic concurrency only; it does not perform an
  atomic store operation.
- Component 400 `register` owns state 100, reads `pre.100`, writes
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
