//! Python replay integration test.
//!
//! Runs the generated Python test module's `replay()` function via `python3` and
//! asserts that all vectors replay with the expected decode outcomes. The
//! Python files are generated at build time by `build.rs` into the `python/`
//! directory next to this crate's `Cargo.toml`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use zeno_fcis_codegen::{CodegenError, GeneratedBundle, GenerationSpec, generate};
use zeno_fcis_schema::{
    EnumVariantDef, FieldDef, FieldId, Schema, SchemaLimits, SumVariantDef, TypeDef, TypeId,
    TypeKind, VariantId,
};

fn fixture_python_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR not set")),
    );
    manifest_dir.join("python")
}

/// Runs `script` with `python3 -c` from `python_dir` and returns its stdout.
fn run_python_script(python_dir: &Path, script: &str, label: &str) -> String {
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .current_dir(python_dir)
        .output()
        .unwrap_or_else(|_| panic!("failed to run python3"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{label} failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    stdout
}

#[test]
fn python_vector_replay_succeeds() {
    let python_dir = fixture_python_dir();
    let module_path = python_dir.join("codegen_fixture.py");
    assert!(
        module_path.exists(),
        "generated python fixture not found at {module_path:?}"
    );

    let output = Command::new("python3")
        .arg(&module_path)
        .current_dir(&python_dir)
        .output()
        .unwrap_or_else(|_| panic!("failed to run python3"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "python replay failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("vectors replayed:"),
        "missing replay confirmation in stdout: {stdout}"
    );
}

#[test]
fn python_outgoing_schema_bounds_are_enforced() {
    let script = r#"
from codegen_fixture import (
    AdapterError, Amount, Signed, Blob, Label, Flag, Nil, Tag, Point, Event, Labels, Scores,
    ScoresEntry, BalanceState, TYPE_TAG, TYPE_EVENT, VARIANT_TAG_IDLE, VARIANT_TAG_ACTIVE,
    VARIANT_EVENT_STOP, VARIANT_EVENT_MOVE,
)
from zcve import encode, decode

def rejects(call, kind):
    try:
        call()
    except AdapterError as error:
        assert error.kind == kind, (error.kind, kind)
        return
    raise AssertionError(f"expected {kind}")

rejects(lambda: Amount(1_000_001).to_value(), "integer_range")
rejects(lambda: Signed(-1_001).to_value(), "integer_range")
rejects(lambda: Blob(bytes(33)).to_value(), "length")
rejects(lambda: Label("").to_value(), "length")
rejects(lambda: Label("é").to_value(), "non_ascii_text")
rejects(lambda: Labels([Label("a")] * 5).to_value(), "length")
rejects(lambda: Scores([ScoresEntry(Amount(i), Amount(i)) for i in range(5)]).to_value(), "length")

class LyingInt(int):
    def __le__(self, other): return True
    def __ge__(self, other): return True

# Valid values keep their exact values and canonical bytes.
assert Flag(True).to_value() == ("bool", True)
assert Flag(False).to_value() == ("bool", False)
assert encode(Flag(True).to_value()) == bytes([0x02])
assert Amount(0).to_value() == ("u128", 0)
assert Amount(1_000_000).to_value() == ("u128", 1_000_000)
assert encode(Amount(9).to_value()) == bytes([0x03]) + (9).to_bytes(16, "big")
assert Signed(-1_000).to_value() == ("i128", -1_000)
assert Blob(b"").to_value() == ("bytes", b"")
assert Blob(bytes(32)).to_value() == ("bytes", bytes(32))
assert Label("a").to_value() == ("text", "a")
assert Tag("Idle").to_value() == ("enum", TYPE_TAG, VARIANT_TAG_IDLE)
assert Tag("Active").to_value() == ("enum", TYPE_TAG, VARIANT_TAG_ACTIVE)
assert Event("Stop").to_value() == ("sum", TYPE_EVENT, VARIANT_EVENT_STOP, None)
assert Event("Move", Amount(9)).to_value() == ("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, ("u128", 9))
assert Flag.try_from_value(("bool", False)).value is False
assert Amount.try_from_value(("u128", 1_000_000)).value == 1_000_000
assert Signed.try_from_value(("i128", -1_000)).value == -1_000
assert Blob.try_from_value(("bytes", b"\x01")).value == b"\x01"
assert Label.try_from_value(("text", "a")).value == "a"
assert Tag.try_from_value(("enum", TYPE_TAG, VARIANT_TAG_ACTIVE)).variant == "Active"
moved = Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, ("u128", 9)))
assert moved.variant == "Move" and moved.payload.value == 9

state = BalanceState(
    Amount(9), Signed(-1_000), Label("a"), Blob(b"\x01"), Flag(True), Nil(), Tag("Active"),
    Point(Amount(0), Tag("Idle")), Event("Move", Amount(1)), Labels([Label("x")]),
    Scores([ScoresEntry(Amount(2), Amount(3)), ScoresEntry(Amount(1), Amount(4))]),
)
state_value = state.to_value()
state_bytes = encode(state_value)
assert decode(state_bytes) == state_value
assert encode(BalanceState.try_from_value(decode(state_bytes)).to_value()) == state_bytes

# Outgoing: caller data of the wrong primitive type is rejected, never coerced.
rejects(lambda: Flag("false").to_value(), "type_mismatch")
rejects(lambda: Flag(1).to_value(), "type_mismatch")
rejects(lambda: Flag(None).to_value(), "type_mismatch")
rejects(lambda: Amount(True).to_value(), "type_mismatch")
rejects(lambda: Amount(2.9).to_value(), "type_mismatch")
rejects(lambda: Amount(2.0).to_value(), "type_mismatch")
rejects(lambda: Amount("9").to_value(), "type_mismatch")
rejects(lambda: Amount(1_000_001.0).to_value(), "type_mismatch")
rejects(lambda: Amount(LyingInt(9)).to_value(), "type_mismatch")
rejects(lambda: Signed(False).to_value(), "type_mismatch")
rejects(lambda: Signed(-1.0).to_value(), "type_mismatch")
rejects(lambda: Blob(3).to_value(), "type_mismatch")
rejects(lambda: Blob([0, 1]).to_value(), "type_mismatch")
rejects(lambda: Blob(bytearray(2)).to_value(), "type_mismatch")
rejects(lambda: Blob("ab").to_value(), "type_mismatch")
rejects(lambda: Label(None).to_value(), "type_mismatch")
rejects(lambda: Label(b"a").to_value(), "type_mismatch")
rejects(lambda: Label(7).to_value(), "type_mismatch")
rejects(lambda: Tag(VARIANT_TAG_IDLE).to_value(), "type_mismatch")
rejects(lambda: Tag(True).to_value(), "type_mismatch")
rejects(lambda: Tag("Bogus").to_value(), "unknown_variant")
rejects(lambda: Event(VARIANT_EVENT_STOP).to_value(), "type_mismatch")
rejects(lambda: Event("Move", Amount(2.9)).to_value(), "type_mismatch")
rejects(lambda: Point(Amount(True), Tag("Idle")).to_value(), "type_mismatch")
rejects(lambda: Labels([Label(None)]).to_value(), "type_mismatch")
rejects(lambda: Scores([ScoresEntry(Amount(1.0), Amount(1))]).to_value(), "type_mismatch")

# Incoming: primitive payloads and enum/sum/record identifiers must be exact.
rejects(lambda: Flag.try_from_value(("bool", "false")), "type_mismatch")
rejects(lambda: Flag.try_from_value(("bool", 0)), "type_mismatch")
rejects(lambda: Amount.try_from_value(("u128", 2.9)), "type_mismatch")
rejects(lambda: Amount.try_from_value(("u128", 2.0)), "type_mismatch")
rejects(lambda: Amount.try_from_value(("u128", True)), "type_mismatch")
rejects(lambda: Amount.try_from_value(("u128", LyingInt(2_000_000))), "type_mismatch")
rejects(lambda: Signed.try_from_value(("i128", False)), "type_mismatch")
rejects(lambda: Signed.try_from_value(("i128", -1.0)), "type_mismatch")
rejects(lambda: Blob.try_from_value(("bytes", 3)), "type_mismatch")
rejects(lambda: Blob.try_from_value(("bytes", [0, 1])), "type_mismatch")
rejects(lambda: Blob.try_from_value(("bytes", "ab")), "type_mismatch")
rejects(lambda: Label.try_from_value(("text", None)), "type_mismatch")
rejects(lambda: Label.try_from_value(("text", b"a")), "type_mismatch")
rejects(lambda: Tag.try_from_value(("enum", TYPE_TAG, True)), "type_mismatch")
rejects(lambda: Tag.try_from_value(("enum", TYPE_TAG, 1.0)), "type_mismatch")
rejects(lambda: Tag.try_from_value(("enum", TYPE_TAG, "Idle")), "type_mismatch")
rejects(lambda: Tag.try_from_value(("enum", float(TYPE_TAG), VARIANT_TAG_IDLE)), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, True, None)), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, 2.0, ("u128", 9))), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", float(TYPE_EVENT), VARIANT_EVENT_STOP, None)), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, ("u128", True))), "type_mismatch")

def with_first_field_id(field_id):
    fields = list(state_value[1])
    fields[0] = (field_id, fields[0][1])
    return ("record", fields)

rejects(lambda: BalanceState.try_from_value(with_first_field_id(True)), "type_mismatch")
rejects(lambda: BalanceState.try_from_value(with_first_field_id(1.0)), "type_mismatch")

# Existing tag, shape, variant, payload, range, and text checks keep their kinds.
rejects(lambda: Amount.try_from_value(("i128", 9)), "type_mismatch")
rejects(lambda: Amount.try_from_value(("u128", 1_000_001)), "integer_range")
rejects(lambda: Label.try_from_value(("text", "")), "length")
rejects(lambda: Label.try_from_value(("text", "é")), "non_ascii_text")
rejects(lambda: Tag.try_from_value(("enum", TYPE_EVENT, VARIANT_TAG_IDLE)), "type_mismatch")
rejects(lambda: Tag.try_from_value(("enum", TYPE_TAG, 999)), "unknown_variant")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, 999, None)), "unknown_variant")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_STOP, ("unit",))), "unexpected_payload")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, None)), "missing_payload")
rejects(lambda: BalanceState.try_from_value(("record", list(state_value[1])[:-1])), "record_shape")
rejects(lambda: BalanceState.try_from_value(with_first_field_id(99)), "record_shape")
"#;
    run_python_script(
        &fixture_python_dir(),
        script,
        "python outgoing-bound checks",
    );
}

#[test]
fn python_composite_values_reject_malformed_structure() {
    let script = r#"
from codegen_fixture import (
    AdapterError, Amount, Signed, Blob, Label, Flag, Nil, Tag, Point, Event, Labels, Scores,
    ScoresEntry, BalanceState, TYPE_TAG, TYPE_EVENT, VARIANT_TAG_IDLE, VARIANT_EVENT_STOP,
    VARIANT_EVENT_MOVE, FIELD_BALANCESTATE_AMOUNT,
)
from zcve import encode, decode

def rejects(call, kind):
    try:
        call()
    except AdapterError as error:
        assert error.kind == kind, (error.kind, kind)
        return
    raise AssertionError(f"expected {kind}")

def state(**changes):
    parts = dict(
        amount=Amount(9), signed=Signed(-1_000), label=Label("a"), blob=Blob(b"\x01"),
        flag=Flag(True), nil=Nil(), tag=Tag("Active"), point=Point(Amount(0), Tag("Idle")),
        event=Event("Move", Amount(1)), labels=Labels([Label("x")]),
        scores=Scores([ScoresEntry(Amount(2), Amount(3)), ScoresEntry(Amount(1), Amount(4))]),
    )
    parts.update(changes)
    return BalanceState(**parts)

def replays(adapter, value):
    data = encode(value)
    assert decode(data) == value
    assert encode(adapter.try_from_value(decode(data)).to_value()) == data

class AmountSubclass(Amount):
    pass

class ValueTuple(tuple):
    pass

# Valid composite values still replay to identical canonical bytes.
state_value = state().to_value()
point_value = Point(Amount(0), Tag("Idle")).to_value()
replays(BalanceState, state_value)
replays(Point, point_value)
replays(Event, Event("Stop").to_value())
replays(Event, Event("Move", Amount(1_000_000)).to_value())
replays(Labels, Labels([]).to_value())
replays(Labels, Labels([Label("x"), Label("y")]).to_value())
replays(Scores, Scores([ScoresEntry(Amount(2), Amount(3)), ScoresEntry(Amount(1), Amount(4))]).to_value())

# Collection constructors keep accepting any iterable.
assert Labels(Label(s) for s in "xy").to_value() == ("vector", [("text", "x"), ("text", "y")])
assert Scores(iter([ScoresEntry(Amount(1), Amount(2))])).to_value() == (
    "map", [(encode(("u128", 1)), ("u128", 1), ("u128", 2))]
)

# Outgoing: every child must be exactly the declared generated class.
rejects(lambda: Point(Signed(0), Tag("Idle")).to_value(), "type_mismatch")
rejects(lambda: Point(Amount(0), Event("Stop")).to_value(), "type_mismatch")
rejects(lambda: Point(AmountSubclass(0), Tag("Idle")).to_value(), "type_mismatch")
rejects(lambda: Point(0, Tag("Idle")).to_value(), "type_mismatch")
rejects(lambda: Point(("u128", 0), Tag("Idle")).to_value(), "type_mismatch")
rejects(lambda: state(amount=Signed(9)).to_value(), "type_mismatch")
rejects(lambda: state(label=Blob(b"a")).to_value(), "type_mismatch")
rejects(lambda: state(nil=None).to_value(), "type_mismatch")
rejects(lambda: state(tag=Event("Stop")).to_value(), "type_mismatch")
rejects(lambda: state(point=point_value).to_value(), "type_mismatch")
rejects(lambda: state(labels=[Label("x")]).to_value(), "type_mismatch")
rejects(lambda: state(scores=Labels([])).to_value(), "type_mismatch")
rejects(lambda: Event("Move", Signed(1)).to_value(), "type_mismatch")
rejects(lambda: Event("Move", AmountSubclass(1)).to_value(), "type_mismatch")
rejects(lambda: Event("Move", ("u128", 1)).to_value(), "type_mismatch")
rejects(lambda: Labels([Blob(b"x")]).to_value(), "type_mismatch")
rejects(lambda: Labels(["x"]).to_value(), "type_mismatch")
rejects(lambda: Labels([Label("x"), Amount(1)]).to_value(), "type_mismatch")
rejects(lambda: Scores([(Amount(1), Amount(2))]).to_value(), "type_mismatch")
rejects(lambda: Scores([ScoresEntry(Signed(1), Amount(2))]).to_value(), "type_mismatch")
rejects(lambda: Scores([ScoresEntry(Amount(1), Label("a"))]).to_value(), "type_mismatch")
# Earlier checks keep precedence: collection bounds, then children in order.
rejects(lambda: Labels([Blob(b"x")] * 5).to_value(), "length")
rejects(lambda: state(amount=Amount(1_000_001), signed=Blob(b"")).to_value(), "integer_range")

# Outgoing sums: payload presence must match the declared variant.
rejects(lambda: Event("Stop", Amount(1)).to_value(), "unexpected_payload")
rejects(lambda: Event("Stop", ("unit",)).to_value(), "unexpected_payload")
rejects(lambda: Event("Stop", False).to_value(), "unexpected_payload")
rejects(lambda: Event("Move").to_value(), "missing_payload")
rejects(lambda: Event("Move", None).to_value(), "missing_payload")
rejects(lambda: Event("Bogus", Amount(1)).to_value(), "unknown_variant")

# Incoming: short, extra, non-tuple, and wrongly tagged values are adapter errors.
valid_values = [
    (Nil, ("unit",)),
    (Flag, ("bool", True)),
    (Amount, ("u128", 1)),
    (Signed, ("i128", -1)),
    (Blob, ("bytes", b"")),
    (Label, ("text", "a")),
    (Tag, ("enum", TYPE_TAG, VARIANT_TAG_IDLE)),
    (Point, point_value),
    (Event, ("sum", TYPE_EVENT, VARIANT_EVENT_STOP, None)),
    (Labels, ("vector", [])),
    (Scores, ("map", [])),
    (BalanceState, state_value),
]
for adapter, value in valid_values:
    replays(adapter, value)
    for malformed in (
        value[:-1], value + (None,), list(value), ValueTuple(value), (None,) + value[1:],
        value[0], (), None, 0,
    ):
        rejects(lambda: adapter.try_from_value(malformed), "type_mismatch")

# Incoming composite payloads must be lists.
for adapter, tag in ((Point, "tuple"), (Labels, "vector"), (Scores, "map"), (BalanceState, "record")):
    for payload in (None, 3, "ab", b"ab", (), {}):
        rejects(lambda: adapter.try_from_value((tag, payload)), "type_mismatch")

rejects(lambda: Point.try_from_value(("tuple", [("u128", 0)])), "record_shape")
rejects(lambda: Point.try_from_value(("tuple", point_value[1] + [("unit",)])), "record_shape")
rejects(lambda: Point.try_from_value(("tuple", [("u128", 0), ("u128", 0)])), "type_mismatch")
rejects(lambda: Labels.try_from_value(("vector", [None] * 5)), "length")
rejects(lambda: Labels.try_from_value(("vector", [("text", "a"), ("text",)])), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, ("u128",))), "type_mismatch")
rejects(lambda: Event.try_from_value(("sum", TYPE_EVENT, VARIANT_EVENT_MOVE, ("u128", 1, 2))), "type_mismatch")

# Incoming record fields must be exact (identifier, value) pairs.
def with_first_field(pair):
    fields = list(state_value[1])
    fields[0] = pair
    return ("record", fields)

amount_value = ("u128", 9)
replays(BalanceState, with_first_field((FIELD_BALANCESTATE_AMOUNT, amount_value)))
for pair in (
    None, 0, "ab", (), (FIELD_BALANCESTATE_AMOUNT,), (FIELD_BALANCESTATE_AMOUNT, amount_value, None),
    [FIELD_BALANCESTATE_AMOUNT, amount_value],
):
    rejects(lambda: BalanceState.try_from_value(with_first_field(pair)), "type_mismatch")
rejects(lambda: BalanceState.try_from_value(with_first_field((FIELD_BALANCESTATE_AMOUNT, ("u128",)))), "type_mismatch")
rejects(lambda: BalanceState.try_from_value(("record", list(state_value[1]) + [None])), "record_shape")

def bad_first_child_and_second_id():
    fields = list(state_value[1])
    fields[0] = (FIELD_BALANCESTATE_AMOUNT, ("u128", 1_000_001))
    fields[1] = (99, fields[1][1])
    return ("record", fields)

rejects(lambda: BalanceState.try_from_value(bad_first_child_and_second_id()), "integer_range")

# Incoming map entries must be exact (encoded key, key, value) triples.
entry = Scores([ScoresEntry(Amount(1), Amount(2))]).to_value()[1][0]
replays(Scores, ("map", [entry]))
for malformed_entry in (None, 0, "abc", (), entry[:2], entry + (None,), list(entry)):
    rejects(lambda: Scores.try_from_value(("map", [entry, malformed_entry])), "type_mismatch")
rejects(lambda: Scores.try_from_value(("map", [(entry[0], ("i128", 1), entry[2])])), "type_mismatch")
rejects(lambda: Scores.try_from_value(("map", [(entry[0], entry[1], ("u128", 1_000_001))])), "integer_range")
rejects(lambda: Scores.try_from_value(("map", [None] * 5)), "length")

# Derived encoded keys must agree with the key value and remain canonical.
for encoded_key in (None, 1, "key", bytearray(entry[0])):
    rejects(lambda: Scores.try_from_value(("map", [(encoded_key, entry[1], entry[2])])), "type_mismatch")
rejects(lambda: Scores.try_from_value(("map", [(b"wrong", entry[1], entry[2])])), "non_canonical_map")
rejects(lambda: Scores.try_from_value(("map", [entry, entry])), "non_canonical_map")
second = ScoresEntry(Amount(2), Amount(3)).to_entry()
rejects(lambda: Scores.try_from_value(("map", [second, entry])), "non_canonical_map")
rejects(lambda: Scores([ScoresEntry(Amount(1), Amount(2)), ScoresEntry(Amount(1), Amount(3))]).to_value(), "non_canonical_map")
replays(Scores, ("map", [entry, second]))
"#;
    run_python_script(
        &fixture_python_dir(),
        script,
        "python malformed-structure checks",
    );
}

fn type_def(id: u32, name: &str, kind: TypeKind) -> TypeDef {
    TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default())
        .unwrap_or_else(|error| panic!("type {name} rejected: {error}"))
}

fn amount(id: u32, name: &str) -> TypeDef {
    type_def(id, name, TypeKind::U128 { min: 0, max: 1_000 })
}

fn amount_map() -> TypeKind {
    TypeKind::Map {
        key: TypeId::new(1),
        value: TypeId::new(1),
        min_len: 0,
        max_len: 4,
    }
}

/// Generates a schema whose root `State` record (type 100) has one field per
/// `(name, type id)`, with field ids assigned in order.
fn generate_state(
    mut types: Vec<TypeDef>,
    fields: &[(&str, u32)],
    python_module: &str,
) -> Result<GeneratedBundle, CodegenError> {
    let fields: Vec<FieldDef> = fields
        .iter()
        .zip(1_u16..)
        .map(|((name, type_id), field_id)| {
            FieldDef::try_new(FieldId::new(field_id), *name, TypeId::new(*type_id))
                .unwrap_or_else(|error| panic!("field {name} rejected: {error}"))
        })
        .collect();
    types.push(type_def(
        100,
        "State",
        TypeKind::Record {
            fields: fields.into_boxed_slice(),
        },
    ));
    let schema = Schema::try_new(
        "NameProbe",
        1,
        TypeId::new(100),
        types,
        SchemaLimits::default(),
    )
    .unwrap_or_else(|error| panic!("schema rejected: {error}"));
    let spec = GenerationSpec::try_new("name_probe", python_module)
        .unwrap_or_else(|error| panic!("generation spec rejected: {error}"));
    generate(&schema, &spec)
}

#[test]
fn python_generation_rejects_shadowing_names() {
    assert!(generate_state(vec![amount(1, "Amount")], &[("amount", 1)], "name_probe").is_ok());

    // Type names that would shadow builtins, generated runtime names, or
    // locals of generated methods that reference classes.
    for name in [
        "int",
        "len",
        "bytes",
        "list",
        "AdapterError",
        "VectorCase",
        "VECTORS",
        "GENERATOR_ID",
        "encode",
        "replay",
        "_child_value",
        "items",
        "values",
        "field",
        "__Hidden",
    ] {
        let result = generate_state(vec![amount(1, name)], &[("amount", 1)], "name_probe");
        assert_eq!(
            result.err(),
            Some(CodegenError::InvalidIdentifier),
            "type {name}"
        );
    }

    // Type names that collide with generated constants or map entry classes.
    let collisions = [
        (
            vec![amount(1, "Amount"), amount(2, "TYPE_AMOUNT")],
            "type constant",
        ),
        (
            vec![amount(1, "Amount"), amount(2, "FIELD_STATE_AMOUNT")],
            "field constant",
        ),
        (
            vec![
                amount(1, "Amount"),
                type_def(
                    2,
                    "Mode",
                    TypeKind::Enum {
                        variants: vec![
                            EnumVariantDef::try_new(VariantId::new(1), "On")
                                .unwrap_or_else(|error| panic!("variant rejected: {error}")),
                        ]
                        .into_boxed_slice(),
                    },
                ),
                amount(3, "VARIANT_MODE_ON"),
            ],
            "variant constant",
        ),
        (
            vec![
                amount(1, "Amount"),
                type_def(2, "Scores", amount_map()),
                amount(3, "ScoresEntry"),
            ],
            "map entry class",
        ),
    ];
    for (types, label) in collisions {
        let fields: Vec<(&str, u32)> = [("amount", 1), ("other", 2), ("third", 3)]
            .into_iter()
            .take(types.len())
            .collect();
        let result = generate_state(types, &fields, "name_probe");
        assert_eq!(
            result.err(),
            Some(CodegenError::InvalidIdentifier),
            "{label}"
        );
    }

    // Field names that would shadow generated methods or be mangled/special.
    for field in ["to_value", "try_from_value", "__dict__", "__init__"] {
        let result = generate_state(vec![amount(1, "Amount")], &[(field, 1)], "name_probe");
        assert_eq!(
            result.err(),
            Some(CodegenError::InvalidIdentifier),
            "field {field}"
        );
    }

    // The adapter module cannot replace the `zcve` codec it imports.
    let result = generate_state(vec![amount(1, "Amount")], &[("amount", 1)], "zcve");
    assert_eq!(result.err(), Some(CodegenError::InvalidModuleName));
}

#[test]
fn python_innocent_names_generate_and_replay() {
    let types = vec![
        amount(1, "Amount"),
        type_def(
            2,
            "Mode",
            TypeKind::Enum {
                variants: ["v", "cls", "to_value"]
                    .into_iter()
                    .zip(1_u16..)
                    .map(|(name, id)| {
                        EnumVariantDef::try_new(VariantId::new(id), name)
                            .unwrap_or_else(|error| panic!("variant {name} rejected: {error}"))
                    })
                    .collect(),
            },
        ),
        type_def(
            3,
            "Choice",
            TypeKind::Sum {
                variants: vec![
                    SumVariantDef::try_new(VariantId::new(1), "payload", Some(TypeId::new(1)))
                        .unwrap_or_else(|error| panic!("variant rejected: {error}")),
                    SumVariantDef::try_new(VariantId::new(2), "variant", None)
                        .unwrap_or_else(|error| panic!("variant rejected: {error}")),
                ]
                .into_boxed_slice(),
            },
        ),
        type_def(
            4,
            "Pair",
            TypeKind::Tuple {
                items: vec![TypeId::new(1), TypeId::new(2)].into_boxed_slice(),
            },
        ),
        type_def(
            5,
            "List",
            TypeKind::Vector {
                element: TypeId::new(1),
                min_len: 0,
                max_len: 4,
            },
        ),
        type_def(6, "Table", amount_map()),
    ];
    // Field names that match generated locals, builtins, and a class name.
    let fields = [
        ("v", 1),
        ("cls", 2),
        ("fields", 3),
        ("field", 4),
        ("values", 5),
        ("items", 6),
        ("value", 1),
        ("len", 1),
        ("encode", 1),
        ("Amount", 3),
    ];
    let bundle = generate_state(types, &fields, "innocent_names")
        .unwrap_or_else(|error| panic!("innocent names rejected: {error}"));

    let python_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("python_innocent_names");
    fs::create_dir_all(&python_dir)
        .unwrap_or_else(|error| panic!("create {python_dir:?}: {error}"));
    for file in bundle.files() {
        if let Some(file_name) = file.path().strip_prefix("python/") {
            fs::write(python_dir.join(file_name), file.bytes())
                .unwrap_or_else(|error| panic!("write {file_name}: {error}"));
        }
    }

    let script = r#"
from innocent_names import (
    AdapterError, Amount, Mode, Choice, Pair, List, Table, TableEntry, State, replay,
)
from zcve import encode, decode

replay()
state = State(
    Amount(1), Mode("cls"), Choice("payload", Amount(2)), Pair(Amount(3), Mode("to_value")),
    List([Amount(4)]), Table([TableEntry(Amount(5), Amount(6))]), Amount(7), Amount(8),
    Amount(9), Choice("variant"),
)
value = state.to_value()
data = encode(value)
assert decode(data) == value
back = State.try_from_value(decode(data))
assert encode(back.to_value()) == data
assert back.v.value == 1 and back.cls.variant == "cls" and back.fields.payload.value == 2
assert back.field.field_0.value == 3 and back.field.field_1.variant == "to_value"
assert back.values.value[0].value == 4 and back.items.value[0].value.value == 6
assert back.value.value == 7 and back.len.value == 8 and back.encode.value == 9
assert back.Amount.variant == "variant" and back.Amount.payload is None

for call, kind in (
    (lambda: State.try_from_value(("record", [])), "record_shape"),
    (lambda: State(*[Amount(0)] * 10).to_value(), "type_mismatch"),
    (lambda: Choice("variant", Amount(1)).to_value(), "unexpected_payload"),
    (lambda: Mode.try_from_value(("enum",)), "type_mismatch"),
):
    try:
        call()
    except AdapterError as error:
        assert error.kind == kind, (error.kind, kind)
    else:
        raise AssertionError(f"expected {kind}")
print("innocent names ok")
"#;
    let stdout = run_python_script(&python_dir, script, "python innocent-name checks");
    assert!(
        stdout.contains("vectors replayed:") && stdout.contains("innocent names ok"),
        "missing innocent-name confirmation in stdout: {stdout}"
    );
}
