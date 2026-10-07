//! Exact emitted channel admission, including full numeric/variant domains and ASCII text.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::atom(domain,value),))]
pub(super) fn atom(domain: Domain<'_>, value: Atom<'_>) -> bool {
    match (domain, value) {
        (Domain::Bool, Atom::Bool(_)) | (Domain::Bytes, Atom::Bytes(_)) => true,
        (Domain::Text, Atom::Text(bytes)) => super::super::util::ascii(bytes),
        (Domain::I128 { min, max }, Atom::I128(v)) => min <= v && v <= max,
        (Domain::U128 { min, max }, Atom::U128(v)) => min <= v && v <= max,
        (
            Domain::Enum { type_id, variants },
            Atom::Enum {
                type_id: t,
                variant,
            },
        )
        | (
            Domain::Sum { type_id, variants },
            Atom::Sum {
                type_id: t,
                variant,
            },
        ) => type_id == t && super::super::util::contains(variants, variant),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::payload(fields@,schema@),))]
fn payload(fields: &[Field<'_>], schema: &[TypedField<'_>]) -> bool {
    if fields.len() != schema.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),fields.len()==schema.len(),forall|j:int|0<=j<i==>spec::payload_entry(fields@,schema@,j),decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].id != schema[i].field || !atom(schema[i].domain, fields[i].value) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::payload_entry(fields@,schema@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::deliveries(d,input@.map(|_:int,x:decision::Delivery|decision::spec::delivery_view(x))),))]
fn deliveries(d: &Descriptor<'_>, input: &[decision::Delivery<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=input.len(),forall|j:int|0<=j<i==>spec::delivery(d,decision::spec::delivery_view(input@[j])),decreases input.len()-i,))]
    while i < input.len() {
        let p = &input[i];
        let Some(c) = admission::channel(d, p.channel) else {
            #[cfg(verus_keep_ghost)]
            proof! {let mapped=input@.map(|_:int,x:decision::Delivery|decision::spec::delivery_view(x));assert(mapped.len()==input@.len());assert(mapped[i as int]==decision::spec::delivery_view(input@[i as int]));assert(!spec::delivery(d,mapped[i as int]));assert(exists|j:int|0<=j<mapped.len()&&!spec::delivery(d,mapped[j]));}
            return false;
        };
        if !atom(c.destination, p.destination)
            || !atom(c.idempotency, p.idempotency)
            || !payload(&p.payload, c.payload)
        {
            #[cfg(verus_keep_ghost)]
            proof! {let mapped=input@.map(|_:int,x:decision::Delivery|decision::spec::delivery_view(x));assert(mapped.len()==input@.len());assert(mapped[i as int]==decision::spec::delivery_view(input@[i as int]));assert(!spec::delivery(d,mapped[i as int]));assert(exists|j:int|0<=j<mapped.len()&&!spec::delivery(d,mapped[j]));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::candidate(d,decision::spec::candidate_view(*input)),))]
pub(super) fn candidate(d: &Descriptor<'_>, input: &Candidate<'_>) -> bool {
    deliveries(d, input.effects()) && deliveries(d, input.outbox())
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::expression(domain,expr),))]
pub(super) fn expression(domain: Domain<'_>, expr: Expr<'_>) -> bool {
    match expr {
        Expr::Constant(a) => atom(domain, a),
        _ => true,
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::law_nodes(nodes@),))]
fn law_nodes(nodes: &[laws::Op<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=nodes.len(),forall|j:int|0<=j<i==>spec::law_literal(nodes@[j]),decreases nodes.len()-i,))]
    while i < nodes.len() {
        let valid = match nodes[i] {
            laws::Op::Literal(laws::Atom::Text(bytes)) => super::super::util::ascii(bytes),
            _ => true,
        };
        if !valid {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::law_literal(nodes@[i as int]));}
            return false;
        }
        i += 1;
    }
    true
}

// Validate every literal, including unused eager nodes and inapplicable laws.
// Other atom variants already contain canonical scalar values or opaque bytes.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::law_literals(definitions@),))]
pub(super) fn law_literals(definitions: &[laws::Law<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=definitions.len(),forall|j:int|0<=j<i==>spec::law_nodes(definitions@[j].program.nodes@),decreases definitions.len()-i,))]
    while i < definitions.len() {
        if !law_nodes(definitions[i].program.nodes) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::law_nodes(definitions@[i as int].program.nodes@));}
            return false;
        }
        i += 1;
    }
    true
}
