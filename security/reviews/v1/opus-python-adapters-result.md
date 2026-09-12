I've made the fix, but nothing has been run: no build, no Python and no tests. The generated Python adapters now reject values of the wrong primitive type with `AdapterError("type_mismatch")` instead of converting them.

## Changed files
- `crates/zeno-fcis-codegen/src/python.rs`
- `crates/zeno-fcis-generated-code-tests/tests/python_replay.rs`

## How it works
A new helper, `exact_type_check(expr, T)`, writes the Python line `if type(expr) is not T: raise AdapterError("type_mismatch")`. The check is `type(x) is T`, not `isinstance`, on purpose. `bool` is a subclass of `int`, and a subclass can override `<=` or `encode` to get past the range and ASCII checks. For example, an `int` subclass whose `__ge__`/`__le__` always return `True` used to pass `("u128", 2_000_000)` through `try_from_value` as in range.

| Generated type | Outgoing (`to_value`) | Incoming (`try_from_value`) |
|---|---|---|
| Bool | exact `bool` (was `bool(value)`) | exact `bool` |
| U128 / I128 | exact `int` (was `int(value)`), then the range check | exact `int`, then the range check |
| Bytes | constructor no longer calls `bytes(value)`; exact `bytes`, then length | exact `bytes`, then length |
| Text | constructor no longer calls `str(value)`; exact `str`, then ASCII, then length | exact `str`, then ASCII, then length |
| Enum / Sum | `variant` must be a `str`; an unknown name raises `unknown_variant` | type ID must be an exact `int` (inside the existing tag check); ordinal must be an exact `int` before the `unknown_variant` lookup |
| Record | unchanged | each field ID must be an exact `int` before the existing `record_shape` comparison |

**What stays the same:**
- **Checks and error order:** the existing tag check still runs first. Length, `record_shape`, `unknown_variant`, `unexpected_payload`/`missing_payload`, `integer_range`, `length` and `non_ascii_text` checks still run in the same order. The only difference is that the type check now comes right before the value check.
- **APIs:** public names, the codec, vectors and canonical bytes are unchanged. Vector, map and tuple constructors still accept any iterable.
- **Enum API:** the generated enum takes a variant *name* string, so real members (`"Idle"`, `"Active"`) and all generated `VARIANT_*`/`TYPE_*` ints still pass.

## Inputs that are now rejected on purpose
- `Flag(1)`, `Flag(0)`, `Flag("false")`.
- `Amount(True)`, `Amount(2.0)`, `Amount("9")`, and subclasses such as `IntEnum` or numpy integers.
- `Blob(bytearray(...))`, `Blob(memoryview(...))`, `Blob([0, 1])`, `Blob(3)`. Only `bytes` is accepted.
- `Label(None)`, `Label(7)`, `Label(b"a")`. These are no longer turned into strings.
- Constructors no longer convert `.value`. The raw input is stored and checked when `to_value()` is called.
- Wrong-type or unknown names: `Tag(1)` or `Event(True)` now raise `type_mismatch`, and unknown names raise `unknown_variant`. Both used to raise `KeyError`.
- Incoming ordinals or IDs given as `True` or `1.0`, and payloads given as float or bool, are rejected.

## Test additions (in `python_outgoing_schema_bounds_are_enforced`)
The existing seven checks are kept. New cases:
- **Valid values:** outgoing and incoming values at each boundary; exact bytes for `Flag(True)` and `Amount(9)`; and a full `BalanceState` round trip covering every type, checking that encode → decode → `try_from_value` → `to_value` gives back the same bytes.
- **Rejected outgoing:** bool vs int, float vs int, string vs bool, int/list/bytearray vs bytes, None vs text, ordinal passed as a variant name, the lying int subclass, and bad values nested inside tuple, vector, map and sum payload.
- **Rejected incoming:** the same kinds of mix-ups for primitives, enum and sum ordinals and type IDs, and record field IDs given as `True` or `1.0`.
- **Error kinds kept:** `integer_range`, `length`, `non_ascii_text`, wrong type ID, `unknown_variant` for enum and sum, `unexpected_payload`, `missing_payload`, and `record_shape` for both a wrong field count and a wrong integer ID.

## Gaps not covered (outside this bounded task)
- **Wrong wrapper class:** nothing checks that a nested object is the right generated class. For example, `Event("Move", Label("x"))` still produces a text payload.
- **Dropped payload:** an outgoing payload on a no-payload variant is still silently ignored.
- **Short tuples:** incoming tuples that are too short still raise `IndexError`.
- **Name collisions:** a schema type named `type`, `int`, `str`, `bool` or `bytes` would shadow a Python builtin. The checks then reject every value, so they fail safe, but the adapter is unusable. The existing code already has this problem with names like `tuple` and `len`.

## Commands for root
```
git diff --stat            # expect only the two files above
cargo fmt --all -- --check
cargo clippy -p zeno-fcis-codegen -p zeno-fcis-generated-code-tests --all-targets -- -D warnings
cargo test -p zeno-fcis-codegen
cargo test -p zeno-fcis-generated-code-tests          # needs python3 on PATH
cargo test -p zeno-fcis-generated-code-tests --test python_replay -- --nocapture
```
`build.rs` regenerates `crates/zeno-fcis-generated-code-tests/python/` as untracked output, and the Python tests import from it.