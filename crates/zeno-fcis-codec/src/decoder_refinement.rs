// Relations between production results, consumed input and abstract parser results.
verus! {

pub open spec fn decoded_result(result: &Result<Value, DecodeError>, used: nat) -> Option<(ModelValue, nat)> {
    match result { Err(_) => None, Ok(value) => Some((value.model(), used)) }
}

pub open spec fn decoded_metrics_result(result: &Result<(Value, ValueMetrics), DecodeError>, used: nat)
    -> Option<(ModelValue, nat)> {
    match result { Err(_) => None, Ok((value, _)) => Some((value.model(), used)) }
}

spec fn parsed_result(result: &Result<Value, DecodeError>, used: nat, nodes: nat, payload: nat)
    -> Option<decoder_spec::Parsed> {
    match result {
        Err(_) => None,
        Ok(value) => Some(decoder_spec::Parsed { value: value.model(), used, nodes, payload }),
    }
}

spec fn parsed_items_result(result: &Result<Vec<Value>, DecodeError>, used: nat, nodes: nat, payload: nat)
    -> Option<decoder_spec::ParsedItems> {
    match result {
        Err(_) => None,
        Ok(values) => Some(decoder_spec::ParsedItems { values: values_model(values@), used, nodes, payload }),
    }
}

spec fn parsed_fields_result(result: &Result<Vec<Field>, DecodeError>, used: nat, nodes: nat, payload: nat)
    -> Option<decoder_spec::ParsedFields> {
    match result {
        Err(_) => None,
        Ok(values) => Some(decoder_spec::ParsedFields { values: fields_model(values@), used, nodes, payload }),
    }
}

spec fn parsed_entries_result(result: &Result<Vec<MapEntry>, DecodeError>, used: nat, nodes: nat, payload: nat)
    -> Option<decoder_spec::ParsedEntries> {
    match result {
        Err(_) => None,
        Ok(values) => Some(decoder_spec::ParsedEntries { values: entries_model(values@), used, nodes, payload }),
    }
}

spec fn prepend_items(prefix: Seq<ModelValue>, used: nat, rest: Option<decoder_spec::ParsedItems>)
    -> Option<decoder_spec::ParsedItems> {
    match rest {
        None => None,
        Some(rest) => Some(decoder_spec::ParsedItems { values: prefix + rest.values,
            used: used + rest.used, nodes: rest.nodes, payload: rest.payload }),
    }
}

spec fn prepend_fields(prefix: Seq<(u16, ModelValue)>, used: nat, rest: Option<decoder_spec::ParsedFields>)
    -> Option<decoder_spec::ParsedFields> {
    match rest {
        None => None,
        Some(rest) => Some(decoder_spec::ParsedFields { values: prefix + rest.values,
            used: used + rest.used, nodes: rest.nodes, payload: rest.payload }),
    }
}

spec fn prepend_entries(prefix: Seq<(ModelValue, ModelValue)>, used: nat, rest: Option<decoder_spec::ParsedEntries>)
    -> Option<decoder_spec::ParsedEntries> {
    match rest {
        None => None,
        Some(rest) => Some(decoder_spec::ParsedEntries { values: prefix + rest.values,
            used: used + rest.used, nodes: rest.nodes, payload: rest.payload }),
    }
}

proof fn prepend_items_associative(prefix: Seq<ModelValue>, first_used: nat, head: ModelValue,
    next_used: nat, rest: Option<decoder_spec::ParsedItems>)
    ensures prepend_items(prefix, first_used, prepend_items(seq![head], next_used, rest))
        == prepend_items(prefix.push(head), first_used + next_used, rest),
{
    match rest { None => {}, Some(rest) => {
        assert(prefix.push(head) + rest.values == prefix + (seq![head] + rest.values));
    } }
}

proof fn prepend_fields_associative(prefix: Seq<(u16, ModelValue)>, first_used: nat, head: (u16, ModelValue),
    next_used: nat, rest: Option<decoder_spec::ParsedFields>)
    ensures prepend_fields(prefix, first_used, prepend_fields(seq![head], next_used, rest))
        == prepend_fields(prefix.push(head), first_used + next_used, rest),
{
    match rest { None => {}, Some(rest) => {
        assert(prefix.push(head) + rest.values == prefix + (seq![head] + rest.values));
    } }
}

proof fn prepend_entries_associative(prefix: Seq<(ModelValue, ModelValue)>, first_used: nat, head: (ModelValue, ModelValue),
    next_used: nat, rest: Option<decoder_spec::ParsedEntries>)
    ensures prepend_entries(prefix, first_used, prepend_entries(seq![head], next_used, rest))
        == prepend_entries(prefix.push(head), first_used + next_used, rest),
{
    match rest { None => {}, Some(rest) => {
        assert(prefix.push(head) + rest.values == prefix + (seq![head] + rest.values));
    } }
}

proof fn tail_composes(bytes: Seq<u8>, first: nat, second: nat)
    requires first + second <= bytes.len(),
    ensures tail(tail(bytes, first), second) == tail(bytes, first + second),
{
}

}
