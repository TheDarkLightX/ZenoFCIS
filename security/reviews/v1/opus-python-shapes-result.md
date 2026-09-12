I've made the changes in both files, but I haven't compiled or run anything (no shell). Nothing has been checked against old source either: my claims that the new tests would fail there come from reading the code.

## `crates/zeno-fcis-codegen/src/python.rs`

**New helpers in the generated module** (emitted right after `AdapterError`):
- `_is_tuple(value, size)`: the value must be a plain `tuple` of exactly `size` items.
- `_is_tagged(value, tag, size)`: same, and item 0 must be a `str` equal to `tag`.
- `_child_value(child, expected)`: the child must be exactly the `expected` class, otherwise `type_mismatch`; then it returns `child.to_value()`.

**Outgoing (`to_value`)**
- **Tuple, record, vector:** each child goes through `_child_value` with its declared class. Checks still run in the same order (length first for vectors).
- **Map:** each entry must be exactly the `<Map>Entry` class. `to_entry` checks the key and the value against their declared classes.
- **Sum:**
  - An empty variant with any payload other than `None` gives `unexpected_payload`.
  - A variant that needs a payload but gets `None` gives `missing_payload`.
  - Otherwise the payload goes through `_child_value(self.payload, ptype)`.
- Vector and map constructors still accept any iterable (`list(value)`).

**Incoming (`try_from_value`)**
- Every kind now checks the exact tuple size before reading any item: unit 1, primitives and collections 2, enum 3, sum 4. Short, extra, non-tuple and wrongly tagged inputs all give `type_mismatch`.
- Tuple, record, vector and map payloads must be exactly a `list`, otherwise `type_mismatch`. The existing `record_shape` and `length` checks then run as before.
- Each record field must be a 2-tuple. The existing checks follow in the same order: field id type (`type_mismatch`), field id value (`record_shape`), then the child. Decoded children now go into a `values` list and the record is built with `cls(*values)`, so field names are never used as local variables.
- Each map entry must be a 3-tuple.

**Name validation** (`validate_python_names` now also takes `spec`)
- **Type names** are rejected if they:
  - start with `__`;
  - are Python builtins the module uses (`type`, `int`, `list`, `len`, `bytes`, …);
  - are local names in generated methods that also refer to classes (`self`, `cls`, `v`, `x`, `n`, `e`, `t`, `kv`, `items`, `fields`, `field`, `values`, `entries`).
- **Global collisions:** a set of module-level names catches duplicates between type names, `<Map>Entry` classes, `TYPE_`/`FIELD_`/`VARIANT_` constants, and fixed runtime names (`encode`, `AdapterError`, `VECTORS`, `replay`, `_child_value`, …). Any duplicate gives `InvalidIdentifier`.
- **Field names** are rejected only for `self`, `to_value`, `try_from_value`, or a leading `__`.
- **Variant names** keep the existing keyword check only.
- **Module name:** a Python module named `zcve` gives `InvalidModuleName`, because it would replace the codec module the adapter imports.

## `crates/zeno-fcis-generated-code-tests/tests/python_replay.rs`

- **Setup:** I added `fixture_python_dir` and `run_python_script` helpers. The existing test's Python script is unchanged; only its runner now uses the helper.
- **`python_composite_values_reject_malformed_structure`:**
  - Valid values of every kind still round-trip to identical bytes, and the iterable constructors still work.
  - Wrong child classes, including subclasses and raw value tuples, are rejected.
  - The sum payload errors and check ordering are covered.
  - Incoming short/extra/non-tuple/tuple-subclass inputs, non-list payloads, and malformed field pairs and map entries are rejected.
  - Existing error kinds are checked, e.g. a bad first child still wins over a bad second field id.
- **`python_generation_rejects_shadowing_names`:**
  - Covers builtin, runtime and local type names, `__Hidden`, and collisions with type, field and variant constants and with `ScoresEntry`.
  - Covers the rejected field names and the `zcve` module name.
  - Includes a positive control that a plain schema still generates.
- **`python_innocent_names_generate_and_replay`:**
  - The schema uses field names `v`, `cls`, `fields`, `field`, `values`, `items`, `value`, `len`, `encode` and `Amount`, and variant names `to_value`, `payload` and `variant`.
  - The test writes the generated Python into `CARGO_TARGET_TMPDIR` and runs `replay()`, a full round-trip and a few rejections.
  - Under the old code, the `Amount` field would crash `try_from_value` with `UnboundLocalError`.

## Limitations
- **Stricter than before:** tuple or list subclasses, and tuple (not list) payloads, are now rejected on input. Canonical decoded values are unaffected.
- **Error kind:** I chose `type_mismatch` for malformed field pairs and map entries.
- **Maps, incoming:** the encoded key is still ignored, and duplicate or unsorted keys are still accepted. On output, duplicate keys still fail inside `zcve.encode` with `DecodeError`, not `AdapterError`.
- **Not handled:** replacing `.value` on a collection after construction with a non-iterable still raises `TypeError`, and very deep or cyclic values can still raise `RecursionError`.
- **Maintenance:** the reserved names are hand-kept lists that must stay in step with the emitters. Anything more general would need a real namespace redesign, which I didn't attempt. Rust-side name collisions are also out of scope.
- **Where `zcve` is rejected:** in `generate`, not in `GenerationSpec::try_new`, because `model.rs` was off-limits.
- **Formatting:** I formatted by hand to rustfmt style; rustfmt and clippy haven't been run.