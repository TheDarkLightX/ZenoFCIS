# finite-phase-machine — bounded declarative core

Schema `zeno-fcis/core-seed-family/1`, version 1. Supported parameters are
exactly `C` in `{1,2,3,4,5,6,7,8}` — eight instances `c1`..`c8`, no claim
beyond them.

## Behavior

- State `PhaseMachine` (root 100): field 110 `phase`, integer type 105
  bounded `0..=C`.
- Command `Step` (root 101): field 120 `reset` (bool type 106).
- Supplied context `Operator` (root 102): field 130 `authorized`, bool
  type 108.
- Input tuple order: `[phase, reset, authorized]`; `state_width` = 1.
- Decision precedence:
  1. `authorized = false` → Reject 200 `not_authorized`, state unchanged.
  2. `reset && phase != C` → Reject 201 `premature_reset`, state unchanged.
  3. `!reset && phase = C` → Reject 202 `at_final_phase`, state unchanged.
  4. `reset` (reachable only when `phase = C`) → Accept, `phase' = 0`.
  5. otherwise → Accept, `phase' = phase + 1`.
- Every reject keeps `post` empty: the state is unchanged.
- Genesis: `phase = 0`. No effects, outbox, or deliveries ever.

## Laws

- 500 `bounded` on commit,genesis: `0 <= phase' && phase' <= C`.
- 501 `authorized_bounded_result` on accept: `authorized = true` and
  `0 <= phase' && phase' <= C` (strengthened so the law reads state, not
  only context — required by the substantive-law linter).
- 502 `exact_transition` on accept: accepted `reset` requires
  `pre.phase = C` and yields `phase' = 0`; accepted non-reset yields
  `phase' = pre.phase + 1`.

## Assumptions and ownership

- `authorized` is an explicitly supplied assumption, NOT authenticated
  identity. Treating it as authentication is a misuse of this component.
- This is a logical finite state machine; reaching phase `C` does not
  establish completion of any external effect.
- Component 400 `phase_machine` owns state 100, reads `pre.100`, writes
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
