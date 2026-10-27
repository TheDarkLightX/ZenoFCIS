//! Complete original-schema ingress into the existing descriptor pipeline.
//! All mutable buffers are fresh invocation-local scratch. Returned spans borrow
//! immutable input/schema bytes; neither policy nor input aliases are mutable.
use super::super::super::compound;
use super::*;
use crate::finite::historical_i64::canonical_v2::schema;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(crate) mod spec;

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::fields(checked,root),))]
pub(in super::super::super) fn fields<'a>(
    checked: &schema::Checked<'a>,
    root: u32,
) -> Option<&'a [schema::Field<'a>]> {
    match compound::find(checked.description().definitions, root)?.kind {
        schema::Kind::Record(f) => Some(f),
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::field_type(fs@,id,fs@.len()),))]
fn field_type(fs: &[schema::Field<'_>], id: u16) -> Option<u32> {
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fs.len(),spec::field_type(fs@,id,i as nat).is_none(),decreases fs.len()-i,))]
    while i < fs.len() {
        if fs[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::field_found(fs@,id,(i+1) as nat,fs@.len());}
            return Some(fs[i].type_id);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::selected_type(checked,root,selector),))]
pub(in super::super::super) fn selected_type(
    checked: &schema::Checked<'_>,
    root: u32,
    selector: Selector,
) -> Option<u32> {
    match selector {
        Selector::Root => Some(root),
        Selector::Field(id) => field_type(fields(checked, root)?, id),
        Selector::PayloadField(_) => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::variants(original@,actual@),))]
fn variants(original: &[schema::Variant<'_>], actual: &[u16]) -> bool {
    if original.len() != actual.len() {
        return false;
    }
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=original.len(),original.len()==actual.len(),forall|j:int|0<=j<i==>original@[j].id==actual@[j],decreases original.len()-i,))]
    while i < original.len() {
        if original[i].id != actual[i] {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::matches_type(checked,id,domain),))]
pub(in super::super::super) fn matches_type(
    checked: &schema::Checked<'_>,
    id: u32,
    domain: Domain<'_>,
) -> bool {
    let Some(d) = compound::find(checked.description().definitions, id) else {
        return false;
    };
    match (d.kind, domain) {
        (schema::Kind::Bool, Domain::Bool) => true,
        (schema::Kind::U128 { min: a, max: b }, Domain::U128 { min: c, max: d }) => {
            a == c && b == d
        }
        (schema::Kind::I128 { min: a, max: b }, Domain::I128 { min: c, max: d }) => {
            a == c && b == d
        }
        (
            schema::Kind::Enum(v),
            Domain::Enum {
                type_id,
                variants: vs,
            },
        )
        | (
            schema::Kind::Sum(v),
            Domain::Sum {
                type_id,
                variants: vs,
            },
        ) => id == type_id && variants(v, vs),
        (
            schema::Kind::Unit
            | schema::Kind::Bytes { .. }
            | schema::Kind::Text { .. }
            | schema::Kind::Tuple(_)
            | schema::Kind::Record(_)
            | schema::Kind::SumPayload(_)
            | schema::Kind::Vector { .. }
            | schema::Kind::Map { .. },
            Domain::Canonical { schema, type_id },
        ) => id == type_id && super::super::super::util::bytes_equal(checked.original(), schema),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::kind(checked,id),))]
pub(in super::super::super) fn kind(
    checked: &schema::Checked<'_>,
    id: u32,
) -> Option<super::super::admission::Kind> {
    use super::super::admission::Kind;
    Some(
        match compound::find(checked.description().definitions, id)?.kind {
            schema::Kind::Bool => Kind::Bool,
            schema::Kind::I128 { .. } => Kind::I128,
            schema::Kind::U128 { .. } => Kind::U128,
            schema::Kind::Enum(_) => Kind::Enum(id),
            schema::Kind::Sum(_) => Kind::Sum(id),
            _ => Kind::Canonical(id),
        },
    )
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::valid(checked,root,projection),))]
pub(in super::super::super) fn valid(
    checked: &schema::Checked<'_>,
    root: u32,
    projection: Projection<'_>,
) -> bool {
    if compound::find(checked.description().definitions, root).is_none() {
        return false;
    }
    match projection {
        Projection::None => true,
        Projection::Root(d) => {
            super::super::admission::numeric::leaf(*d) && matches_type(checked, root, *d)
        }
        Projection::Fields(fs) => {
            if fields(checked, root).is_none() {
                return false;
            }
            let mut i = 0;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fs.len(),projection==Projection::Fields(fs),
                compound::spec::find(checked.view().1.definitions@,root,checked.view().1.definitions@.len()).is_some(),spec::fields(checked,root).is_some(),
                forall|j:int|0<=j<i==>spec::projection_field(checked,root,fs@,j),decreases fs.len()-i,))]
            while i < fs.len() {
                if (i > 0 && fs[i - 1].id >= fs[i].id)
                    || !super::super::admission::numeric::leaf(fs[i].leaf)
                {
                    #[cfg(verus_keep_ghost)]
                    proof! {assert(!spec::projection_field(checked,root,fs@,i as int));if spec::valid(checked,root,projection){spec::valid_field(checked,root,projection,fs,i as int);}}
                    return false;
                }
                let Some(id) = selected_type(checked, root, Selector::Field(fs[i].id)) else {
                    #[cfg(verus_keep_ghost)]
                    proof! {assert(!spec::projection_field(checked,root,fs@,i as int));if spec::valid(checked,root,projection){spec::valid_field(checked,root,projection,fs,i as int);}}
                    return false;
                };
                if !matches_type(checked, id, fs[i].leaf) {
                    #[cfg(verus_keep_ghost)]
                    proof! {assert(!spec::projection_field(checked,root,fs@,i as int));if spec::valid(checked,root,projection){spec::valid_field(checked,root,projection,fs,i as int);}}
                    return false;
                }
                i += 1;
            }
            true
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::actual_field(fs@,id,fs@.len()),))]
fn actual_field<'a>(fs: &[Field<'a>], id: u16) -> Option<Atom<'a>> {
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fs.len(),spec::actual_field(fs@,id,i as nat).is_none(),decreases fs.len()-i,))]
    while i < fs.len() {
        if fs[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::actual_found(fs@,id,(i+1) as nat,fs@.len());}
            return Some(fs[i].value);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::code(domain,atom),))]
fn code(domain: Domain<'_>, atom: Atom<'_>) -> Option<Atom<'static>> {
    match (domain, atom) {
        (Domain::Bool, Atom::Bool(v)) => Some(Atom::Bool(v)),
        (Domain::U128 { min, max }, Atom::U128(v)) if min <= v && v <= max => Some(Atom::U128(v)),
        (Domain::I128 { min, max }, Atom::I128(v)) if min <= v && v <= max => Some(Atom::I128(v)),
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(v)=>Ok(v@),Err(e)=>Err(e)},final(meter).used.counters@)==spec::codes(projection,compound::spec::root_view(*root),old(meter).limits.counters@,old(meter).used.counters@),))]
fn codes(
    projection: Projection<'_>,
    root: &compound::DecodedRoot<'_>,
    meter: &mut Meter,
) -> Result<Vec<Atom<'static>>, Failure> {
    match projection {
        Projection::None => Ok(Vec::new()),
        Projection::Root(d) => {
            if let Err(e) = meter.charge(Resource::Step, 1) {
                return Err(Failure::Budget(e));
            }
            let Some(v) = code(*d, root.atom) else {
                return Err(Failure::Reification);
            };
            let out = alloc::vec![v];
            #[cfg(verus_keep_ghost)]
            proof! {assert(out@==seq![v]);}
            Ok(out)
        }
        Projection::Fields(fs) => {
            let Some(ref actual) = root.fields else {
                return Err(Failure::Reification);
            };
            let mut values = Vec::new();
            let mut i = 0;
            #[cfg(verus_keep_ghost)]
            proof! {reveal_with_fuel(spec::codes_fields,1);}
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fs.len(),meter.limits==old(meter).limits,
                projection==Projection::Fields(fs),compound::spec::root_view(*root).1==Some(actual@),
                spec::codes_fields(fs@,actual@,i as nat,old(meter).limits.counters@,old(meter).used.counters@)==(Ok(values@),meter.used.counters@),decreases fs.len()-i,))]
            while i < fs.len() {
                #[cfg(verus_keep_ghost)]
                proof! {reveal_with_fuel(spec::codes_fields,2);}
                if let Err(e) = meter.charge(Resource::Step, 1) {
                    #[cfg(verus_keep_ghost)]
                    proof! {spec::codes_failed(fs@,actual@,(i+1) as nat,fs@.len(),meter.limits.counters@,old(meter).used.counters@);}
                    return Err(Failure::Budget(e));
                }
                let v = match actual_field(actual, fs[i].id) {
                    Some(a) => code(fs[i].leaf, a),
                    None => None,
                };
                let Some(v) = v else {
                    #[cfg(verus_keep_ghost)]
                    proof! {spec::codes_failed(fs@,actual@,(i+1) as nat,fs@.len(),meter.limits.counters@,old(meter).used.counters@);}
                    return Err(Failure::Reification);
                };
                values.push(v);
                i += 1;
            }
            Ok(values)
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(reads)@==old(reads)@+spec::attempts(source,accesses@),))]
fn append_reads(source: u8, accesses: &[compound::Access], reads: &mut Vec<Read>) {
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=accesses.len(),reads@==old(reads)@+spec::attempts(source,accesses@.take(i as int)),decreases accesses.len()-i,))]
    while i < accesses.len() {
        let (selector, permitted) = match accesses[i] {
            compound::Access::Root(p) => (Selector::Root, p),
            compound::Access::Field(id, p) => (Selector::Field(id), p),
            compound::Access::PayloadField(id, p) => (Selector::PayloadField(id), p),
        };
        reads.push(Read {
            source,
            selector,
            permitted,
        });
        i += 1;
    }
}
#[allow(clippy::too_many_arguments)]
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(v)=>Ok(shared::spec::decoded(v)),Err(e)=>Err(e)},final(meter).used.counters@,final(reads)@)=={
        let r=spec::project(bytes,checked,root,projection,source,old(meter).limits.counters@,old(meter).used.counters@);(r.0,r.1,old(reads)@+r.2)},))]
pub(in super::super) fn project<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    root: u32,
    projection: Projection<'a>,
    source: u8,
    meter: &mut Meter,
    reads: &mut Vec<Read>,
) -> Result<Decoded<'a, Atom<'static>>, Failure> {
    if let Err(e) = meter.charge(Resource::Byte, bytes.len() as u64) {
        return Err(Failure::Budget(e));
    }
    let mut accesses = Vec::new();
    let decoded = compound::decode_root(bytes, checked, root, meter, &mut accesses);
    append_reads(source, &accesses, reads);
    let actual = match decoded {
        Ok(v) => v,
        Err(compound::Failure::Wire) => return Err(Failure::Root),
        Err(compound::Failure::Budget(e)) => return Err(Failure::Budget(e)),
    };
    let scalars = codes(projection, &actual, meter)?;
    let value = match actual.fields {
        Some(f) => Value::Record(f),
        None => Value::Leaf(actual.atom),
    };
    Ok(Decoded { value, scalars })
}
