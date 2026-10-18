//! One primitive byte decoder with explicit legacy versus typed value encoding.
use super::super::super::value::{Atom, Domain};
use super::super::{Resource, meter::Meter};
use super::{AccessAttempt, Failure, Field};
use super::{Leaf, decode_variant, read_big_endian, read_signed_128};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "typed/spec.rs"]
pub(in super::super) mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum LeafView<'a> {
    Legacy(&'a Leaf),
    Numeric(Domain<'a>),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum FieldsView<'a> {
    Legacy(&'a [Field]),
    Numeric(&'a [Field<Domain<'a>>]),
}

impl FieldsView<'_> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result as nat==spec::fields(*self).len(),))]
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Legacy(f) => f.len(),
            Self::Numeric(f) => f.len(),
        }
    }
}
impl<'a> FieldsView<'a> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==if i<spec::fields(*self).len(){Some(spec::fields(*self)[i as int])}else{None},))]
    pub(crate) fn field(&self, i: usize) -> Option<(u16, LeafView<'a>)> {
        match self {
            Self::Legacy(f) => {
                if i < f.len() {
                    Some((f[i].id, LeafView::Legacy(&f[i].leaf)))
                } else {
                    None
                }
            }
            Self::Numeric(f) => {
                if i < f.len() {
                    Some((f[i].id, LeafView::Numeric(f[i].leaf)))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::valid(view),))]
pub(crate) fn valid(view: FieldsView<'_>) -> bool {
    match view {
        FieldsView::Legacy(fields) => super::validate_schema(fields),
        FieldsView::Numeric(fields) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant view==FieldsView::Numeric(fields),i<=fields.len(),forall|j:int|0<=j<i ==> spec::numeric_field_valid(fields@,j),decreases fields.len()-i,))]
            while i < fields.len() {
                if i > 0 && fields[i - 1].id >= fields[i].id {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        assert(!spec::numeric_field_valid(fields@,i as int));
                        spec::numeric_schema_rejected(fields,i as int);
                    }
                    return false;
                }
                let admitted = match fields[i].leaf {
                    Domain::Sum { variants, .. } => {
                        !variants.is_empty() && variants.windows(2).all(|w| w[0] < w[1])
                    }
                    Domain::Bool => true,
                    Domain::I128 { min, max } => min <= max,
                    Domain::U128 { min, max } => min <= max,
                    _ => false,
                };
                if !admitted {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        assert(!spec::numeric_field_valid(fields@,i as int));
                        spec::numeric_schema_rejected(fields,i as int);
                    }
                    return false;
                }
                i += 1;
            }
            #[cfg(verus_keep_ghost)]
            proof! {spec::numeric_schema_valid(fields);}
            true
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::field(bytes@,offset,id,leaf),))]
fn decode_field(
    bytes: &[u8],
    offset: usize,
    id: u16,
    leaf: LeafView<'_>,
) -> Option<(Atom<'static>, usize)> {
    let (actual, start) = read_big_endian(bytes, offset, 2)?;
    if actual != id as u128 {
        return None;
    }
    decode_value(bytes, start, leaf)
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(end)=>Ok((final(output)@,end)),Err(e)=>Err(e)},final(meter).used.counters@,final(attempts)@)=={
        let r=spec::prefix(bytes@,spec::fields(fields),spec::fields(fields).len(),offset,old(meter).limits.counters@,old(meter).used.counters@);
        (r.0,r.1,old(attempts)@+r.2)},))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(160))]
fn project_fields(
    bytes: &[u8],
    fields: FieldsView<'_>,
    offset: usize,
    meter: &mut Meter,
    output: &mut Vec<Atom<'static>>,
    attempts: &mut Vec<AccessAttempt>,
) -> Result<usize, Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost used=meter.used.counters@;let ghost initial=attempts@;}
    output.clear();
    let mut i = 0usize;
    let mut position = offset;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=spec::fields(fields).len(),meter.limits==old(meter).limits,
        used==old(meter).used.counters@,initial==old(attempts)@,
        used.len()==8,meter.limits.counters@.len()==8,meter.used.counters@.len()==8,
        spec::prefix(bytes@,spec::fields(fields),i as nat,offset,meter.limits.counters@,used).0==Ok((output@,position)),
        spec::prefix(bytes@,spec::fields(fields),i as nat,offset,meter.limits.counters@,used).1==meter.used.counters@,
        attempts@==initial+spec::prefix(bytes@,spec::fields(fields),i as nat,offset,meter.limits.counters@,used).2,
        decreases spec::fields(fields).len()-i,))]
    while i < fields.len() {
        #[cfg(verus_keep_ghost)]
        proof! {reveal_with_fuel(spec::prefix,2);}
        let Some((id, leaf)) = fields.field(i) else {
            return Err(Failure::Schema);
        };
        let charged = meter.charge(Resource::Read, 1);
        attempts.push(AccessAttempt {
            field_id: id,
            permitted: charged.is_ok(),
        });
        if let Err(e) = charged {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(spec::prefix(bytes@,spec::fields(fields),(i+1) as nat,offset,meter.limits.counters@,used).0==Err(Failure::Budget(e)));
                spec::prefix_failure(bytes@,spec::fields(fields),(i+1) as nat,spec::fields(fields).len(),offset,meter.limits.counters@,used);
            }
            return Err(Failure::Budget(e));
        }
        let Some((value, end)) = decode_field(bytes, position, id, leaf) else {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(spec::prefix(bytes@,spec::fields(fields),(i+1) as nat,offset,meter.limits.counters@,used).0==Err(Failure::Field(id)));
                spec::prefix_failure(bytes@,spec::fields(fields),(i+1) as nat,spec::fields(fields).len(),offset,meter.limits.counters@,used);
            }
            return Err(Failure::Field(id));
        };
        output.push(value);
        position = end;
        i += 1;
    }
    Ok(position)
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(())=>Ok(final(output)@),Err(e)=>Err(e)},final(meter).used.counters@,final(attempts)@)=={
        let r=spec::projection(bytes@,fields,old(meter).limits.counters@,old(meter).used.counters@);
        (r.0,r.1,old(attempts)@+r.2)},result.is_err()==>final(output)@==Seq::<Atom>::empty(),))]
pub(in super::super) fn project_into(
    bytes: &[u8],
    fields: FieldsView<'_>,
    meter: &mut Meter,
    output: &mut Vec<Atom<'static>>,
    attempts: &mut Vec<AccessAttempt>,
) -> Result<(), Failure> {
    output.clear();
    if !valid(fields) {
        return Err(Failure::Schema);
    }
    if let Err(e) = meter.charge(Resource::Byte, bytes.len() as u64) {
        return Err(Failure::Budget(e));
    }
    let Some(offset) = super::decode_header(bytes, fields.len()) else {
        return Err(Failure::Header);
    };
    match project_fields(bytes, fields, offset, meter, output, attempts) {
        Ok(end) => {
            if end == bytes.len() {
                Ok(())
            } else {
                output.clear();
                Err(Failure::Trailing)
            }
        }
        Err(e) => {
            output.clear();
            Err(e)
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::decode(bytes@,offset,leaf),))]
pub(crate) fn decode_value(
    bytes: &[u8],
    offset: usize,
    leaf: LeafView<'_>,
) -> Option<(Atom<'static>, usize)> {
    let (tag, payload_offset) = read_big_endian(bytes, offset, 1)?;
    match leaf {
        LeafView::Legacy(Leaf::U128 { min, max }) => {
            if tag != 0x03 {
                return None;
            }
            let (value, end) = read_big_endian(bytes, payload_offset, 16)?;
            if value < *min || value > *max || value > i128::MAX as u128 {
                return None;
            }
            Some((Atom::I128(value as i128), end))
        }
        LeafView::Legacy(Leaf::I128 { min, max }) => {
            if tag != 0x04 {
                return None;
            }
            let (value, end) = read_signed_128(bytes, payload_offset)?;
            if value < *min || value > *max {
                return None;
            }
            Some((Atom::I128(value), end))
        }
        LeafView::Legacy(Leaf::Bool) => match tag {
            0x01 => Some((Atom::I128(0), payload_offset)),
            0x02 => Some((Atom::I128(1), payload_offset)),
            _ => None,
        },
        LeafView::Legacy(Leaf::Enum {
            type_id, variants, ..
        }) => {
            if tag != 0x07 {
                return None;
            }
            let (code, end) = decode_variant(bytes, payload_offset, *type_id, variants, false)?;
            Some((Atom::I128(code), end))
        }
        LeafView::Legacy(Leaf::Sum {
            type_id, variants, ..
        }) => {
            if tag != 0x0a {
                return None;
            }
            let (code, end) = decode_variant(bytes, payload_offset, *type_id, variants, true)?;
            Some((Atom::I128(code), end))
        }
        LeafView::Numeric(Domain::Sum { type_id, variants }) => {
            if tag != 0x0a {
                return None;
            }
            let (actual, start) = read_big_endian(bytes, payload_offset, 4)?;
            let (variant, end) = read_big_endian(bytes, start, 2)?;
            let (payload, end) = read_big_endian(bytes, end, 1)?;
            if actual != u128::from(type_id)
                || payload != 0
                || !variants.iter().any(|v| u128::from(*v) == variant)
            {
                return None;
            }
            Some((Atom::I128(variant as i128), end))
        }
        LeafView::Numeric(Domain::Bool) => match tag {
            0x01 => Some((Atom::Bool(false), payload_offset)),
            0x02 => Some((Atom::Bool(true), payload_offset)),
            _ => None,
        },
        LeafView::Numeric(Domain::I128 { min, max }) => {
            if tag != 0x04 {
                return None;
            }
            let (value, end) = read_signed_128(bytes, payload_offset)?;
            if value < min || value > max {
                return None;
            }
            Some((Atom::I128(value), end))
        }
        LeafView::Numeric(Domain::U128 { min, max }) => {
            if tag != 0x03 {
                return None;
            }
            let (value, end) = read_big_endian(bytes, payload_offset, 16)?;
            if value < min || value > max {
                return None;
            }
            Some((Atom::U128(value), end))
        }
        LeafView::Numeric(_) => None,
    }
}
