// The representation relation from immutable production storage to abstract values.
use vstd::string::StringSliceAdditionalSpecFns;

verus! {

impl Value {
    pub closed spec fn model(&self) -> ModelValue
        decreases self,
    {
        match &self.repr {
            ValueRepr::Unit => ModelValue::Unit,
            ValueRepr::Bool(value) => ModelValue::Bool(*value),
            ValueRepr::U128(value) => ModelValue::Unsigned(*value),
            ValueRepr::I128(value) => ModelValue::Signed(*value),
            ValueRepr::Bytes(bytes) => ModelValue::Bytes(bytes@),
            ValueRepr::Text(text) => ModelValue::Text(text.spec_bytes()),
            ValueRepr::Enum { type_id, variant } => ModelValue::Enum(*type_id, *variant),
            ValueRepr::Tuple(items) => ModelValue::Tuple(values_model(items@)),
            ValueRepr::Record(fields) => ModelValue::Record(fields_model(fields@)),
            ValueRepr::Sum { type_id, variant, payload } => ModelValue::Sum(*type_id, *variant,
                match payload { None => Seq::empty(), Some(value) => seq![value.model()] }),
            ValueRepr::Vector(items) => ModelValue::Vector(values_model(items@)),
            ValueRepr::Map(entries) => ModelValue::Map(entries_model(entries@)),
        }
    }

    /// Cached map-key bytes agree with the semantic key at every descendant.
    #[verifier::opaque]
    pub closed spec fn storage_valid(&self) -> bool
        decreases self,
    {
        match &self.repr {
            ValueRepr::Tuple(items) =>
                forall|i: int| 0 <= i < items@.len() ==> (#[trigger] items@[i]).storage_valid(),
            ValueRepr::Vector(items) =>
                forall|i: int| 0 <= i < items@.len() ==> (#[trigger] items@[i]).storage_valid(),
            ValueRepr::Record(fields) =>
                forall|i: int| 0 <= i < fields@.len() ==> (#[trigger] fields@[i]).value.storage_valid(),
            ValueRepr::Sum { payload: Some(value), .. } => value.storage_valid(),
            ValueRepr::Map(entries) =>
                forall|i: int| 0 <= i < entries@.len() ==> (#[trigger] entries@[i]).storage_valid(),
            _ => true,
        }
    }

    /// The exact constructor invariant, including the cached representation relation.
    pub closed spec fn representation_valid(&self) -> bool {
        structurally_valid(self.model()) && self.storage_valid()
    }
}

impl MapEntry {
    #[verifier::opaque]
    pub closed spec fn storage_valid(&self) -> bool
        decreases self,
    {
        self.key.storage_valid() && self.value.storage_valid()
            && self.encoded_key@ == spec_encode(self.key.model())
    }

    pub closed spec fn representation_valid(&self) -> bool {
        self.storage_valid() && structurally_valid(self.key.model())
            && structurally_valid(self.value.model()) && self.encoded_key@.len() <= u32::MAX
    }
}

pub open spec fn values_model(values: Seq<Value>) -> Seq<ModelValue>
    decreases values,
{
    Seq::new(values.len(), |i: int| if 0 <= i < values.len() { values[i].model() } else { ModelValue::Unit })
}

pub open spec fn fields_model(fields: Seq<Field>) -> Seq<(u16, ModelValue)>
    decreases fields,
{
    Seq::new(fields.len(), |i: int| if 0 <= i < fields.len() {
        (fields[i].spec_id(), fields[i].spec_value().model())
    } else { (0, ModelValue::Unit) })
}

pub open spec fn entries_model(entries: Seq<MapEntry>) -> Seq<(ModelValue, ModelValue)>
    decreases entries,
{
    Seq::new(entries.len(), |i: int| if 0 <= i < entries.len() {
        (entries[i].spec_key().model(), entries[i].spec_value().model())
    } else { (ModelValue::Unit, ModelValue::Unit) })
}

pub open spec fn values_valid(values: Seq<Value>) -> bool {
    forall|i: int| 0 <= i < values.len() ==> #[trigger] values[i].representation_valid()
}

pub open spec fn fields_valid(fields: Seq<Field>) -> bool {
    forall|i: int| 0 <= i < fields.len() ==> #[trigger] fields[i].spec_value().representation_valid()
}

pub open spec fn entries_valid(entries: Seq<MapEntry>) -> bool {
    forall|i: int| 0 <= i < entries.len() ==> #[trigger] entries[i].representation_valid()
}

pub open spec fn stored_keys_ordered(entries: Seq<MapEntry>) -> bool {
    forall|i: int| 0 <= i && i + 1 < entries.len() ==>
        lex_less((#[trigger] entries[i]).spec_encoded_key(), entries[i + 1].spec_encoded_key())
}

pub open spec fn field_ids_ordered(fields: Seq<Field>) -> bool {
    forall|i: int| 0 <= i && i + 1 < fields.len() ==>
        (#[trigger] fields[i]).spec_id() < fields[i + 1].spec_id()
}

proof fn fields_order_agrees(fields: Seq<Field>)
    ensures fields_ordered(fields_model(fields)) == field_ids_ordered(fields),
{
    fields_model_properties(fields);
    if fields_ordered(fields_model(fields)) {
        assert forall|i: int| 0 <= i && i + 1 < fields.len() implies
            (#[trigger] fields[i]).spec_id() < fields[i + 1].spec_id() by {
            assert(fields_model(fields)[i].0 < fields_model(fields)[i + 1].0);
        }
    }
    if field_ids_ordered(fields) {
        assert forall|i: int| 0 <= i && i + 1 < fields_model(fields).len() implies
            (#[trigger] fields_model(fields)[i]).0 < fields_model(fields)[i + 1].0 by {
            assert(fields[i].spec_id() < fields[i + 1].spec_id());
        }
    }
}

pub open spec fn entry_values_fit(entries: Seq<MapEntry>) -> bool {
    forall|i: int| 0 <= i < entries.len() ==>
        spec_encode((#[trigger] entries[i]).spec_value().model()).len() <= u32::MAX
}

pub open spec fn optional_model(value: &Option<Value>) -> Seq<ModelValue> {
    match value { None => Seq::empty(), Some(value) => seq![value.model()] }
}

pub open spec fn optional_valid(value: &Option<Value>) -> bool {
    match value { None => true, Some(value) => value.representation_valid() }
}


pub proof fn values_model_properties(values: Seq<Value>)
    ensures values_model(values).len() == values.len(),
        forall|i: int| 0 <= i < values.len() ==> #[trigger] values_model(values)[i] == values[i].model(),
{}

pub proof fn fields_model_properties(fields: Seq<Field>)
    ensures fields_model(fields).len() == fields.len(),
        forall|i: int| 0 <= i < fields.len() ==> #[trigger] fields_model(fields)[i]
            == (fields[i].spec_id(), fields[i].spec_value().model()),
{}

pub proof fn entries_model_properties(entries: Seq<MapEntry>)
    ensures entries_model(entries).len() == entries.len(),
        forall|i: int| 0 <= i < entries.len() ==> #[trigger] entries_model(entries)[i]
            == (entries[i].spec_key().model(), entries[i].spec_value().model()),
{}


proof fn entry_children_valid(entry: MapEntry)
    requires entry.representation_valid(),
    ensures entry.spec_key().representation_valid(), entry.spec_value().representation_valid(),
        entry.spec_encoded_key() == spec_encode(entry.spec_key().model()),
        entry.spec_encoded_key().len() <= u32::MAX,
{
    reveal(MapEntry::representation_valid);
    reveal(MapEntry::storage_valid);
    reveal(Value::representation_valid);
    reveal(Value::storage_valid);
}

proof fn values_valid_parts(items: Seq<Value>)
    requires values_valid(items),
    ensures forall|i: int| 0 <= i < items.len() ==> structurally_valid(#[trigger] values_model(items)[i]),
        forall|i: int| 0 <= i < items.len() ==> (#[trigger] items[i]).storage_valid(),
{
    values_model_properties(items);
    reveal(Value::representation_valid);
    assert forall|i: int| 0 <= i < items.len() implies structurally_valid(#[trigger] values_model(items)[i]) by {
        assert(items[i].representation_valid());
    }
    assert forall|i: int| 0 <= i < items.len() implies (#[trigger] items[i]).storage_valid() by {
        assert(items[i].representation_valid());
    }
}

proof fn fields_valid_parts(fields: Seq<Field>)
    requires fields_valid(fields),
    ensures forall|i: int| 0 <= i < fields.len() ==> structurally_valid((#[trigger] fields_model(fields)[i]).1),
        forall|i: int| 0 <= i < fields.len() ==> (#[trigger] fields[i]).value.storage_valid(),
{
    fields_model_properties(fields);
    reveal(Value::representation_valid);
    assert forall|i: int| 0 <= i < fields.len() implies structurally_valid((#[trigger] fields_model(fields)[i]).1) by {
        assert(fields[i].spec_value().representation_valid());
    }
    assert forall|i: int| 0 <= i < fields.len() implies (#[trigger] fields[i]).value.storage_valid() by {
        assert(fields[i].spec_value().representation_valid());
    }
}

proof fn entries_valid_parts(entries: Seq<MapEntry>)
    requires entries_valid(entries),
    ensures entries_ordered(entries_model(entries)) == stored_keys_ordered(entries),
        forall|i: int| 0 <= i < entries.len() ==> {
            &&& structurally_valid((#[trigger] entries_model(entries)[i]).0)
            &&& structurally_valid(entries_model(entries)[i].1)
            &&& spec_encode(entries_model(entries)[i].0).len() <= u32::MAX
        },
        forall|i: int| 0 <= i < entries.len() ==> (#[trigger] entries[i]).storage_valid(),
{
    entries_model_properties(entries);
    reveal(MapEntry::representation_valid);
    reveal(MapEntry::storage_valid);
    assert forall|i: int| 0 <= i < entries.len() implies {
        &&& structurally_valid((#[trigger] entries_model(entries)[i]).0)
        &&& structurally_valid(entries_model(entries)[i].1)
        &&& spec_encode(entries_model(entries)[i].0).len() <= u32::MAX
    } by { assert(entries[i].representation_valid()); }
    assert forall|i: int| 0 <= i < entries.len() implies (#[trigger] entries[i]).storage_valid() by {
        assert(entries[i].representation_valid());
    }
    assert forall|i: int| 0 <= i < entries.len() implies
        (#[trigger] entries[i]).spec_encoded_key() == spec_encode(entries_model(entries)[i].0) by {
        assert(entries[i].representation_valid());
    }
    if entries_ordered(entries_model(entries)) {
        assert forall|i: int| 0 <= i && i + 1 < entries.len() implies
            lex_less((#[trigger] entries[i]).spec_encoded_key(), entries[i + 1].spec_encoded_key()) by {
            assert(lex_less(spec_encode(entries_model(entries)[i].0), spec_encode(entries_model(entries)[i + 1].0)));
        }
    }
    if stored_keys_ordered(entries) {
        assert forall|i: int| 0 <= i && i + 1 < entries_model(entries).len() implies
            lex_less(spec_encode((#[trigger] entries_model(entries)[i]).0), spec_encode(entries_model(entries)[i + 1].0)) by {
            assert(lex_less(entries[i].spec_encoded_key(), entries[i + 1].spec_encoded_key()));
        }
    }
}

proof fn map_model_valid(entries: Seq<MapEntry>)
    requires entries_valid(entries), entry_values_fit(entries), entries.len() <= u32::MAX,
        entries_ordered(entries_model(entries)),
    ensures structurally_valid(ModelValue::Map(entries_model(entries))),
        forall|i: int| 0 <= i < entries.len() ==> (#[trigger] entries[i]).storage_valid(),
{
    entries_model_properties(entries);
    entries_valid_parts(entries);
    assert forall|i: int| 0 <= i < entries.len() implies
        spec_encode((#[trigger] entries_model(entries)[i]).1).len() <= u32::MAX by {
        assert(spec_encode(entries[i].spec_value().model()).len() <= u32::MAX);
    }
}

proof fn value_children_valid(value: Value)
    requires value.representation_valid(),
    ensures match &value.repr {
        ValueRepr::Tuple(items) => values_valid(items@),
        ValueRepr::Vector(items) => values_valid(items@),
        ValueRepr::Record(fields) => fields_valid(fields@),
        ValueRepr::Sum { payload: Some(child), .. } => child.representation_valid(),
        ValueRepr::Map(entries) => entries_valid(entries@) && entry_values_fit(entries@),
        _ => true,
    },
{
    reveal(Value::model);
    reveal(Value::representation_valid);
    reveal_with_fuel(Value::storage_valid, 2);
    reveal(MapEntry::representation_valid);
    reveal(MapEntry::storage_valid);
    assert(structurally_valid(value.model()));
    assert(value.storage_valid());
    match &value.repr {
        ValueRepr::Tuple(items) => {
            values_model_properties(items@);
            assert(value.model() == ModelValue::Tuple(values_model(items@)) || value.model() == ModelValue::Vector(values_model(items@)));
            assert(forall|i: int| 0 <= i < items@.len() ==> structurally_valid(values_model(items@)[i]));
            assert forall|i: int| 0 <= i < items@.len() implies #[trigger] items@[i].representation_valid() by {
                assert(structurally_valid(values_model(items@)[i]));
                assert(values_model(items@)[i] == items@[i].model());
                assert(items@[i].storage_valid());
                assert(items@[i].representation_valid());
            }
        },
        ValueRepr::Vector(items) => {
            values_model_properties(items@);
            assert(value.model() == ModelValue::Tuple(values_model(items@)) || value.model() == ModelValue::Vector(values_model(items@)));
            assert(forall|i: int| 0 <= i < items@.len() ==> structurally_valid(values_model(items@)[i]));
            assert forall|i: int| 0 <= i < items@.len() implies #[trigger] items@[i].representation_valid() by {
                assert(structurally_valid(values_model(items@)[i]));
                assert(values_model(items@)[i] == items@[i].model());
                assert(items@[i].storage_valid());
                assert(items@[i].representation_valid());
            }
        },
        ValueRepr::Record(fields) => {
            fields_model_properties(fields@);
            assert forall|i: int| 0 <= i < fields@.len() implies #[trigger] fields@[i].spec_value().representation_valid() by {
                assert(structurally_valid(fields_model(fields@)[i].1));
                assert(fields@[i].spec_value() == fields@[i].value);
                assert(fields@[i].value.storage_valid());
            }
        },
        ValueRepr::Map(entries) => {
            entries_model_properties(entries@);
            assert forall|i: int| 0 <= i < entries@.len() implies #[trigger] entries@[i].representation_valid() by {
                assert(entries@[i].storage_valid());
                assert(structurally_valid(entries_model(entries@)[i].0));
                assert(structurally_valid(entries_model(entries@)[i].1));
                assert(spec_encode(entries_model(entries@)[i].0).len() <= u32::MAX);
            }
            assert forall|i: int| 0 <= i < entries@.len() implies
                spec_encode((#[trigger] entries@[i]).spec_value().model()).len() <= u32::MAX by {
                assert(spec_encode(entries_model(entries@)[i].1).len() <= u32::MAX);
            }
        },
        ValueRepr::Sum { type_id, variant, payload: Some(child) } => {
            assert(value.model() == ModelValue::Sum(*type_id, *variant, seq![child.model()]));
            assert(seq![child.model()][0] == child.model());
            assert(structurally_valid(seq![child.model()][0]));
            assert(structurally_valid(child.model()));
            assert(child.storage_valid());
            assert(child.representation_valid());
        },
        _ => {},
    }
}

pub proof fn values_model_push(values: Seq<Value>, value: Value)
    ensures values_model(values.push(value)) == values_model(values).push(value.model()),
        values_valid(values) && value.representation_valid() ==> values_valid(values.push(value)),
{
    values_model_properties(values);
    values_model_properties(values.push(value));
    assert(values_model(values.push(value)) =~= values_model(values).push(value.model()));
}

pub proof fn fields_model_push(fields: Seq<Field>, value: Field)
    ensures fields_model(fields.push(value)) == fields_model(fields).push((value.spec_id(), value.spec_value().model())),
        fields_valid(fields) && value.spec_value().representation_valid() ==> fields_valid(fields.push(value)),
{
    fields_model_properties(fields);
    fields_model_properties(fields.push(value));
    assert(fields_model(fields.push(value)) =~= fields_model(fields).push((value.spec_id(), value.spec_value().model())));
}

pub proof fn entries_model_push(entries: Seq<MapEntry>, value: MapEntry)
    ensures entries_model(entries.push(value)) == entries_model(entries).push((value.spec_key().model(), value.spec_value().model())),
        entries_valid(entries) && value.representation_valid() ==> entries_valid(entries.push(value)),
        entry_values_fit(entries) && spec_encode(value.spec_value().model()).len() <= u32::MAX
            ==> entry_values_fit(entries.push(value)),
{
    entries_model_properties(entries);
    entries_model_properties(entries.push(value));
    assert(entries_model(entries.push(value)) =~= entries_model(entries).push((value.spec_key().model(), value.spec_value().model())));
}

}
