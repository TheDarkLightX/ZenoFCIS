//! Exact private typed graph producer; outputs cannot be supplied by callers.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

#[cfg(any(test, verus_keep_ghost))]
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::field_code(fields@,codes@,id),))]
fn field_code(fields: &[InputField], codes: &[i128], id: u16) -> Option<i128> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),forall|j:int|0<=j<i==>fields@[j].id!=id,decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {assert(admission::spec::first(fields@,id,i as int));assert(exists|j:int|admission::spec::first(fields@,id,j));admission::spec::first_chosen(fields@,id,i as int);}
            return if i < codes.len() {
                Some(codes[i])
            } else {
                None
            };
        }
        i += 1;
    }
    None
}
#[cfg(any(test, verus_keep_ghost))]
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::scalar(schema,decoded,selector),))]
fn scalar(schema: Schema<'_>, decoded: &ingress::Decoded, selector: Selector) -> Option<i128> {
    match (schema, selector) {
        (Schema::Leaf(_), Selector::Root) => {
            if decoded.scalars.len() == 1 {
                Some(decoded.scalars[0])
            } else {
                None
            }
        }
        (Schema::Record(fields), Selector::Field(id)) => field_code(fields, &decoded.scalars, id),
        _ => None,
    }
}
#[cfg(any(test, verus_keep_ghost))]
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None::<Seq<i128>>})==spec::tuple(d,state,command,context,d.bindings@.len()),))]
pub(super) fn tuple(
    d: &Descriptor<'_>,
    state: &ingress::Decoded,
    command: &ingress::Decoded,
    context: &ingress::Decoded,
) -> Option<Vec<i128>> {
    let mut output = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.bindings.len(),spec::tuple(d,state,command,context,i as nat)==Some(output@),decreases d.bindings.len()-i,))]
    while i < d.bindings.len() {
        let b = d.bindings[i];
        let value = match b.source {
            Source::State => scalar(d.state, state, b.selector),
            Source::Command => scalar(d.command, command, b.selector),
            Source::Context => scalar(d.context, context, b.selector),
        };
        let Some(v) = value else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::tuple_failed(d,state,command,context,(i+1) as nat,d.bindings@.len());}
            return None;
        };
        output.push(v);
        i += 1;
    }
    Some(output)
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures spec::input_view(result)==ingress::spec::value(*input),))]
pub(super) fn value<'fields, 'a>(
    input: &'fields ingress::Value<'a>,
) -> decision::RootView<'fields, 'a> {
    match input {
        ingress::Value::Leaf(a) => decision::RootView::Leaf(*a),
        ingress::Value::Record(fields) => decision::RootView::Record(fields),
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct Produced<'a> {
    pub candidate: Candidate<'a>,
    pub command: ingress::Value<'a>,
    pub context: ingress::Value<'a>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct Trace {
    pub reads: Vec<ReadAttempt>,
    pub attempts: Vec<Attempt>,
    pub ingress: Option<Usage>,
    pub decision: Option<Usage>,
}
