//! Total correspondence checks over complete original declarations.
use super::super::{InputField, InputLeaf, InputVariant};
use super::composition::{
    Atom, Branch, Channel, DeliveryPlan, Descriptor, Domain, Expr, Framing, PayloadField, Schema,
    TypedField,
};
use super::schema::{Definition, Description, Field, Kind, Variant};
#[cfg(verus_keep_ghost)]
use {super::spec, vstd::prelude::*};

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::find(defs@,id),))]
fn find<'a>(defs: &[Definition<'a>], id: u32) -> Option<Definition<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=defs.len(),
        forall|j:int|0<=j<i ==> defs@[j].id!=id, decreases defs.len()-i,))]
    while i < defs.len() {
        if defs[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::first_chosen(defs@,id,i as int);}
            return Some(defs[i]);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::input_has(values@,id),))]
fn input_has(values: &[InputVariant], id: u16) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),
        forall|j:int|0<=j<i ==> values@[j].id!=id, decreases values.len()-i,))]
    while i < values.len() {
        if values[i].id == id {
            return true;
        }
        i += 1;
    }
    false
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::input_variants(original@,actual@),))]
fn input_variants(original: &[Variant<'_>], actual: &[InputVariant]) -> bool {
    if original.len() != actual.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=original.len(),original.len()==actual.len(),
        forall|j:int|0<=j<i ==> spec::input_has(actual@,original@[j].id),decreases original.len()-i,))]
    while i < original.len() {
        if !input_has(actual, original[i].id) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::domain_variants(original@,actual@),))]
fn domain_variants(original: &[Variant<'_>], actual: &[u16]) -> bool {
    if original.len() != actual.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=original.len(),original.len()==actual.len(),
        forall|j:int|0<=j<i ==> actual@.contains(original@[j].id),decreases original.len()-i,))]
    while i < original.len() {
        if !super::super::util::contains(actual, original[i].id) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::input_leaf(def,*leaf),))]
fn input_leaf(def: Definition<'_>, leaf: &InputLeaf) -> bool {
    match (def.kind, leaf) {
        (Kind::Bool, InputLeaf::Bool) => true,
        (Kind::U128 { min: a, max: b }, InputLeaf::U128 { min: c, max: d }) => a == *c && b == *d,
        (Kind::I128 { min: a, max: b }, InputLeaf::I128 { min: c, max: d }) => {
            a == *c as i128 && b == *d as i128
        }
        (
            Kind::Enum(v),
            InputLeaf::Enum {
                type_id, variants, ..
            },
        )
        | (
            Kind::Sum(v),
            InputLeaf::Sum {
                type_id, variants, ..
            },
        ) => def.id == *type_id && input_variants(v, variants),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::input_fields(defs@,original@,actual@),))]
fn input_fields(defs: &[Definition<'_>], original: &[Field<'_>], actual: &[InputField]) -> bool {
    if original.len() != actual.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=original.len(),original.len()==actual.len(),
        forall|j:int|0<=j<i ==> spec::field(defs@,original@[j],actual@[j]),decreases original.len()-i,))]
    while i < original.len() {
        if original[i].id != actual[i].id {
            return false;
        }
        let Some(def) = find(defs, original[i].type_id) else {
            return false;
        };
        if !input_leaf(def, &actual[i].leaf) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::input_schema(defs@,root,actual),))]
fn input_schema(defs: &[Definition<'_>], root: u32, actual: Schema<'_>) -> bool {
    let Some(def) = find(defs, root) else {
        return false;
    };
    match (def.kind, actual) {
        (Kind::Record(fields), Schema::Record(input)) => input_fields(defs, fields, input),
        (_, Schema::Leaf(leaf)) => input_leaf(def, leaf),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::roots(*description,d,f),))]
pub(super) fn roots(description: &Description<'_>, d: &Descriptor<'_>, f: &Framing) -> bool {
    description.root == f.state.root
        && matches!(d.state, Schema::Record(_))
        && input_schema(description.definitions, f.state.root, d.state)
        && input_schema(description.definitions, f.command.root, d.command)
        && input_schema(description.definitions, f.context.root, d.context)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::domain(def,actual),))]
fn domain(def: Definition<'_>, actual: Domain<'_>) -> bool {
    match (def.kind, actual) {
        (Kind::Bool, Domain::Bool)
        | (Kind::Bytes { .. }, Domain::Bytes)
        | (Kind::Text { .. }, Domain::Text) => true,
        (Kind::I128 { min: a, max: b }, Domain::I128 { min: c, max: d }) => a == c && b == d,
        (Kind::U128 { min: a, max: b }, Domain::U128 { min: c, max: d }) => a == c && b == d,
        (Kind::Enum(v), Domain::Enum { type_id, variants })
        | (Kind::Sum(v), Domain::Sum { type_id, variants }) => {
            def.id == type_id && domain_variants(v, variants)
        }
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::typed_fields(defs@,original@,actual@),))]
fn typed_fields(
    defs: &[Definition<'_>],
    original: &[Field<'_>],
    actual: &[TypedField<'_>],
) -> bool {
    if original.len() != actual.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=original.len(),original.len()==actual.len(),
        forall|j:int|0<=j<i ==> spec::typed_field(defs@,original@[j],actual@[j]),decreases original.len()-i,))]
    while i < original.len() {
        if original[i].id != actual[i].field {
            return false;
        }
        let Some(def) = find(defs, original[i].type_id) else {
            return false;
        };
        if !domain(def, actual[i].domain) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::channel(defs@,c,link),))]
fn channel(defs: &[Definition<'_>], c: Channel<'_>, link: (u32, u32, u32)) -> bool {
    if c.id != link.0 {
        return false;
    }
    let Some(dest) = find(defs, link.1) else {
        return false;
    };
    let Some(payload) = find(defs, link.2) else {
        return false;
    };
    if !domain(dest, c.destination) {
        return false;
    }
    match payload.kind {
        Kind::Record(fields) => typed_fields(defs, fields, c.payload),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::channels(defs@,cs@,ls@),))]
pub(super) fn channels(
    defs: &[Definition<'_>],
    cs: &[Channel<'_>],
    ls: &[(u32, u32, u32)],
) -> bool {
    if cs.len() != ls.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=cs.len(),cs.len()==ls.len(),
        forall|j:int|0<=j<i ==> spec::channel_entry(defs@,cs@,ls@,j),decreases cs.len()-i,))]
    while i < cs.len() {
        if (i > 0 && ls[i - 1].0 >= ls[i].0) || !channel(defs, cs[i], ls[i]) {
            #[cfg(verus_keep_ghost)]
            proof! { assert(!spec::channel_entry(defs@,cs@,ls@,i as int)); }
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::bounded(def,e),))]
fn bounded(def: Definition<'_>, e: Expr<'_>) -> bool {
    match def.kind {
        Kind::Bytes { min, max } => match e {
            Expr::Constant(Atom::Bytes(b)) => {
                min as u64 <= b.len() as u64 && b.len() as u64 <= max as u64
            }
            _ => false,
        },
        Kind::Text { min, max } => match e {
            Expr::Constant(Atom::Text(b)) => {
                min as u64 <= b.len() as u64
                    && b.len() as u64 <= max as u64
                    && super::super::util::ascii(b)
            }
            _ => false,
        },
        _ => true,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::link(ls@,id),))]
fn link(ls: &[(u32, u32, u32)], id: u32) -> Option<(u32, u32)> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=ls.len(),
        forall|j:int|0<=j<i ==> ls@[j].0!=id,decreases ls.len()-i,))]
    while i < ls.len() {
        if ls[i].0 == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::link_chosen(ls@,id,i as int);}
            return Some((ls[i].1, ls[i].2));
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::payload(defs@,fs@,ps@),))]
fn payload(defs: &[Definition<'_>], fs: &[Field<'_>], ps: &[PayloadField<'_>]) -> bool {
    if fs.len() != ps.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=fs.len(),fs.len()==ps.len(),
        forall|j:int|0<=j<i ==> spec::payload_field(defs@,fs@[j],ps@[j]),decreases fs.len()-i,))]
    while i < fs.len() {
        if fs[i].id != ps[i].field {
            return false;
        }
        let Some(def) = find(defs, fs[i].type_id) else {
            return false;
        };
        if !bounded(def, ps[i].value) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::delivery(defs@,p,ls@),))]
fn delivery(defs: &[Definition<'_>], p: DeliveryPlan<'_>, ls: &[(u32, u32, u32)]) -> bool {
    let Some((a, b)) = link(ls, p.channel) else {
        return false;
    };
    let Some(dest) = find(defs, a) else {
        return false;
    };
    let Some(body) = find(defs, b) else {
        return false;
    };
    if !bounded(dest, p.destination) {
        return false;
    }
    match body.kind {
        Kind::Record(fs) => payload(defs, fs, p.payload),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::deliveries(defs@,ps@,ls@),))]
fn deliveries(defs: &[Definition<'_>], ps: &[DeliveryPlan<'_>], ls: &[(u32, u32, u32)]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=ps.len(),
        forall|j:int|0<=j<i ==> spec::delivery(defs@,ps@[j],ls@),decreases ps.len()-i,))]
    while i < ps.len() {
        if !delivery(defs, ps[i], ls) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::branches(defs@,bs@,ls@),))]
pub(super) fn branches(defs: &[Definition<'_>], bs: &[Branch<'_>], ls: &[(u32, u32, u32)]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=bs.len(),
        forall|j:int|0<=j<i ==> spec::branch(defs@,bs@[j],ls@),decreases bs.len()-i,))]
    while i < bs.len() {
        if !deliveries(defs, bs[i].effects, ls) || !deliveries(defs, bs[i].outbox, ls) {
            return false;
        }
        i += 1;
    }
    true
}
