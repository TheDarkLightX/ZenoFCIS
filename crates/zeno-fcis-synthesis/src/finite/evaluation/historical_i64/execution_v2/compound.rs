//! Complete original typed-wire validation with an iterative, metered stack.
//!
//! Each stack action is charged before interpretation. The caller charges the
//! complete input Byte work and its protected Root or Record-field reads.
//! Child frames never extend beyond a map blob's declared byte boundary.
use super::{MeterFailure, Resource, decision::Atom, meter::Meter};
use crate::finite::historical_i64::canonical_v2::{read_big_endian, read_signed_128, schema};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Complete canonical value bytes admitted against their original schema type.
/// Only this checked decoder can construct the private witness.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Value<'a> {
    schema: &'a [u8],
    type_id: u32,
    wire: &'a [u8],
}
impl<'a> Value<'a> {
    /// Complete original canonical schema bytes.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().0, result == self.raw().0,))]
    pub fn schema(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Value::view);reveal(Value::raw);}
        self.schema
    }
    /// Original type identifier, without a projected replacement type.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().1, result == self.raw().1,))]
    pub fn type_id(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Value::view);reveal(Value::raw);}
        self.type_id
    }
    /// Exact fully validated original canonical value bytes.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().2, result == self.raw().2,))]
    pub fn wire(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Value::view);reveal(Value::raw);}
        self.wire
    }
}
#[cfg(verus_keep_ghost)]
verus! { impl<'a> Value<'a> {
    pub closed spec fn raw(self) -> (&'a [u8],u32,&'a [u8]) { (self.schema,self.type_id,self.wire) }
    pub closed spec fn view(self) -> (Seq<u8>,u32,Seq<u8>) { (self.schema@,self.type_id,self.wire@) }
} }

/// Typed decoding or shared-budget refusal, with no partial exposed value.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Failure {
    /// Malformed, unknown, out-of-domain, unordered or incompletely consumed wire.
    Wire,
    /// A real parser action was denied before interpretation.
    Budget(MeterFailure),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(super) enum Task<'a> {
    Value(u32),
    Tuple(&'a [u32], usize),
    Record(&'a [schema::Field<'a>], usize),
    Vector(u32, u32, u32),
    Map(u32, u32, u32, u32, Option<(usize, usize)>),
    MapValue(u32, u32, u32, u32, usize, usize),
    Boundary(usize, usize),
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct State<'a> {
    pub(super) position: usize,
    pub(super) limit: usize,
    pub(super) tasks: Vec<Task<'a>>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::find(definitions@,id,definitions@.len()),))]
pub(super) fn find<'a>(
    definitions: &[schema::Definition<'a>],
    id: u32,
) -> Option<schema::Definition<'a>> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant index<=definitions.len(),spec::find(definitions@,id,index as nat).is_none(),decreases definitions.len()-index,))]
    while index < definitions.len() {
        if definitions[index].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::find_found(definitions@,id,(index+1) as nat,definitions@.len());}
            return Some(definitions[index]);
        }
        index += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::read(bytes@,position,width,limit),))]
fn read(bytes: &[u8], position: usize, width: usize, limit: usize) -> Option<(u128, usize)> {
    let (value, end) = read_big_endian(bytes, position, width)?;
    if end > limit {
        None
    } else {
        Some((value, end))
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::signed(bytes@,position,limit),))]
fn signed(bytes: &[u8], position: usize, limit: usize) -> Option<(i128, usize)> {
    let (value, end) = read_signed_128(bytes, position)?;
    if end > limit {
        None
    } else {
        Some((value, end))
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::blob(bytes@,position,limit),))]
fn blob(bytes: &[u8], position: usize, limit: usize) -> Option<(usize, usize)> {
    let (length, start) = read(bytes, position, 4, limit)?;
    let end = start.checked_add(length as usize)?;
    if end > limit || end > bytes.len() {
        None
    } else {
        Some((start, end))
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::ascii(bytes@,start,end),))]
fn ascii(bytes: &[u8], start: usize, end: usize) -> bool {
    if start > end || end > bytes.len() {
        return false;
    }
    let mut i = start;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant start<=i<=end<=bytes.len(),forall|j:int|start<=j<i==>bytes@[j]<128,decreases end-i,))]
    while i < end {
        if bytes[i] >= 128 {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::less(bytes@,a,b),))]
fn less(bytes: &[u8], a: (usize, usize), b: (usize, usize)) -> bool {
    if a.0 > a.1 || b.0 > b.1 || a.1 > bytes.len() || b.1 > bytes.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant a.0<=a.1<=bytes.len(),b.0<=b.1<=bytes.len(),i<=a.1-a.0,i<=b.1-b.0,
        forall|j:int|0<=j<i==>#[trigger] bytes@[a.0 as int+j]==bytes@[b.0 as int+j],decreases (a.1-a.0)-i,))]
    while i < a.1 - a.0 && i < b.1 - b.0 {
        if bytes[a.0 + i] < bytes[b.0 + i] {
            #[cfg(verus_keep_ghost)]
            proof! {assert(spec::lex_at(bytes@,a,b,i as int));}
            return true;
        }
        if bytes[a.0 + i] > bytes[b.0 + i] {
            #[cfg(verus_keep_ghost)]
            proof! {spec::lex_different(bytes@,a,b,i as int);}
            return false;
        }
        i += 1;
    }
    a.1 - a.0 < b.1 - b.0
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::variant(variants@,id),))]
fn variant(variants: &[schema::Variant<'_>], id: u16) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=variants.len(),forall|j:int|0<=j<i==>variants@[j].id!=id,decreases variants.len()-i,))]
    while i < variants.len() {
        if variants[i].id == id {
            return true;
        }
        i += 1;
    }
    false
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::sum_variant(variants@,id,variants@.len()),))]
fn sum_variant(variants: &[schema::SumVariant<'_>], id: u16) -> Option<Option<u32>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=variants.len(),spec::sum_variant(variants@,id,i as nat).is_none(),decreases variants.len()-i,))]
    while i < variants.len() {
        if variants[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::sum_found(variants@,id,(i+1) as nat,variants@.len());}
            return Some(variants[i].payload);
        }
        i += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::scan(bytes@,definitions@,position,limit,id),))]
fn scan<'a>(
    bytes: &[u8],
    definitions: &[schema::Definition<'a>],
    position: usize,
    limit: usize,
    id: u32,
) -> Option<(usize, Option<Task<'a>>)> {
    let definition = find(definitions, id)?;
    let (tag, body) = read(bytes, position, 1, limit)?;
    match definition.kind {
        schema::Kind::Unit => {
            if tag == 0 {
                Some((body, None))
            } else {
                None
            }
        }
        schema::Kind::Bool => {
            if tag == 1 || tag == 2 {
                Some((body, None))
            } else {
                None
            }
        }
        schema::Kind::U128 { min, max } => {
            if tag != 3 {
                return None;
            }
            let (value, end) = read(bytes, body, 16, limit)?;
            if min <= value && value <= max {
                Some((end, None))
            } else {
                None
            }
        }
        schema::Kind::I128 { min, max } => {
            if tag != 4 {
                return None;
            }
            let (value, end) = signed(bytes, body, limit)?;
            if min <= value && value <= max {
                Some((end, None))
            } else {
                None
            }
        }
        schema::Kind::Bytes { min, max } | schema::Kind::Text { min, max } => {
            if tag
                != if matches!(definition.kind, schema::Kind::Text { .. }) {
                    6
                } else {
                    5
                }
            {
                return None;
            }
            let (start, end) = blob(bytes, body, limit)?;
            if end - start < min as usize || end - start > max as usize {
                return None;
            }
            if matches!(definition.kind, schema::Kind::Text { .. }) && !ascii(bytes, start, end) {
                return None;
            }
            Some((end, None))
        }
        schema::Kind::Enum(variants) | schema::Kind::Sum(variants) => {
            let sum = matches!(definition.kind, schema::Kind::Sum(_));
            if tag != if sum { 10 } else { 7 } {
                return None;
            }
            let (type_id, next) = read(bytes, body, 4, limit)?;
            if type_id != id as u128 {
                return None;
            }
            let (variant_id, next) = read(bytes, next, 2, limit)?;
            if !variant(variants, variant_id as u16) {
                return None;
            }
            if sum {
                let (present, end) = read(bytes, next, 1, limit)?;
                if present == 0 {
                    Some((end, None))
                } else {
                    None
                }
            } else {
                Some((next, None))
            }
        }
        schema::Kind::SumPayload(variants) => {
            if tag != 10 {
                return None;
            }
            let (type_id, next) = read(bytes, body, 4, limit)?;
            if type_id != id as u128 {
                return None;
            }
            let (variant_id, next) = read(bytes, next, 2, limit)?;
            let (present, end) = read(bytes, next, 1, limit)?;
            match sum_variant(variants, variant_id as u16)? {
                None => {
                    if present == 0 {
                        Some((end, None))
                    } else {
                        None
                    }
                }
                Some(payload) => {
                    if present == 1 {
                        Some((end, Some(Task::Value(payload))))
                    } else {
                        None
                    }
                }
            }
        }
        schema::Kind::Tuple(items) => {
            if tag != 8 {
                return None;
            }
            let (count, end) = read(bytes, body, 4, limit)?;
            if count == items.len() as u128 {
                Some((end, Some(Task::Tuple(items, 0))))
            } else {
                None
            }
        }
        schema::Kind::Record(fields) => {
            if tag != 9 {
                return None;
            }
            let (count, end) = read(bytes, body, 4, limit)?;
            if count == fields.len() as u128 {
                Some((end, Some(Task::Record(fields, 0))))
            } else {
                None
            }
        }
        schema::Kind::Vector { element, min, max } => {
            if tag != 11 {
                return None;
            }
            let (count, end) = read(bytes, body, 4, limit)?;
            if min as u128 <= count && count <= max as u128 {
                Some((end, Some(Task::Vector(element, count as u32, 0))))
            } else {
                None
            }
        }
        schema::Kind::Map {
            key,
            value,
            min,
            max,
        } => {
            if tag != 12 {
                return None;
            }
            let (count, end) = read(bytes, body, 4, limit)?;
            if min as u128 <= count && count <= max as u128 {
                Some((end, Some(Task::Map(key, value, count as u32, 0, None))))
            } else {
                None
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(s)=>Some(spec::state(s)),None=>None::<spec::StateView>})==spec::advance(bytes@,definitions@,spec::state(initial)),))]
fn advance<'a>(
    bytes: &[u8],
    definitions: &[schema::Definition<'a>],
    initial: State<'a>,
) -> Option<State<'a>> {
    let mut state = initial;
    if state.position > state.limit || state.limit > bytes.len() {
        return None;
    }
    let task = state.tasks.pop()?;
    match task {
        Task::Value(id) => {
            let (end, next) = scan(bytes, definitions, state.position, state.limit, id)?;
            state.position = end;
            if let Some(t) = next {
                state.tasks.push(t);
            }
        }
        Task::Tuple(items, index) => {
            if index > items.len() {
                return None;
            }
            if index < items.len() {
                state.tasks.push(Task::Tuple(items, index + 1));
                state.tasks.push(Task::Value(items[index]));
            }
        }
        Task::Record(fields, index) => {
            if index > fields.len() {
                return None;
            }
            if index < fields.len() {
                let (id, end) = read(bytes, state.position, 2, state.limit)?;
                if id != fields[index].id as u128 {
                    return None;
                }
                state.position = end;
                state.tasks.push(Task::Record(fields, index + 1));
                state.tasks.push(Task::Value(fields[index].type_id));
            }
        }
        Task::Vector(element, count, index) => {
            if index > count {
                return None;
            }
            if index < count {
                state.tasks.push(Task::Vector(element, count, index + 1));
                state.tasks.push(Task::Value(element));
            }
        }
        Task::Map(key, value, count, index, prior) => {
            if index > count {
                return None;
            }
            if index < count {
                let (start, end) = blob(bytes, state.position, state.limit)?;
                if match prior {
                    Some(p) => !less(bytes, p, (start, end)),
                    None => false,
                } {
                    return None;
                }
                state
                    .tasks
                    .push(Task::MapValue(key, value, count, index, start, end));
                state.tasks.push(Task::Boundary(end, state.limit));
                state.tasks.push(Task::Value(key));
                state.position = start;
                state.limit = end;
            }
        }
        Task::MapValue(key, value, count, index, start, end) => {
            if index >= count {
                return None;
            }
            let (value_start, value_end) = blob(bytes, state.position, state.limit)?;
            state
                .tasks
                .push(Task::Map(key, value, count, index + 1, Some((start, end))));
            state.tasks.push(Task::Boundary(value_end, state.limit));
            state.tasks.push(Task::Value(value));
            state.position = value_start;
            state.limit = value_end;
        }
        Task::Boundary(end, limit) => {
            if state.position != end || limit < end || limit > bytes.len() {
                return None;
            }
            state.limit = limit;
        }
    }
    Some(state)
}

/// Validate one actual original typed value, retaining real work on failure.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (result,final(meter).used.counters@)==spec::walk(bytes@,definitions@,position,limit,id,old(meter).limits.counters@,old(meter).used.counters@),))]
pub(super) fn walk<'a>(
    bytes: &[u8],
    definitions: &[schema::Definition<'a>],
    position: usize,
    limit: usize,
    id: u32,
    meter: &mut Meter,
) -> Result<usize, Failure> {
    let mut state = State {
        position,
        limit,
        tasks: alloc::vec![Task::Value(id)],
    };
    #[cfg(verus_keep_ghost)]
    proof! {assert(state.tasks@=~=seq![Task::Value(id)]);reveal(spec::run);reveal(walked);}
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant meter.limits==old(meter).limits,
        spec::run(bytes@,definitions@,spec::state(state),meter.limits.counters@,meter.used.counters@)==spec::walk(bytes@,definitions@,position,limit,id,old(meter).limits.counters@,old(meter).used.counters@),
        decreases u64::MAX as int - meter.used.counters@[7] as int,))]
    while !state.tasks.is_empty() {
        #[cfg(verus_keep_ghost)]
        proof! {reveal_with_fuel(spec::run,2);}
        if let Err(e) = meter.charge(Resource::Step, 1) {
            return Err(Failure::Budget(e));
        }
        let Some(next) = advance(bytes, definitions, state) else {
            return Err(Failure::Wire);
        };
        state = next;
    }
    if state.position > state.limit || state.limit > bytes.len() {
        Err(Failure::Wire)
    } else {
        Ok(state.position)
    }
}

/// Materialize primitive atoms and retain every other fully typed value byte-exactly.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::materialize(bytes,checked,id,start,end),))]
fn materialize<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    id: u32,
    start: usize,
    end: usize,
) -> Option<Atom<'a>> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(materialized);}
    if start > end || end > bytes.len() {
        return None;
    }
    let definition = find(checked.description().definitions, id)?;
    let (_, body) = read(bytes, start, 1, end)?;
    match definition.kind {
        schema::Kind::Bool => {
            let (tag, _) = read(bytes, start, 1, end)?;
            Some(Atom::Bool(tag == 2))
        }
        schema::Kind::U128 { .. } => {
            let (value, _) = read(bytes, body, 16, end)?;
            Some(Atom::U128(value))
        }
        schema::Kind::I128 { .. } => {
            let (value, _) = signed(bytes, body, end)?;
            Some(Atom::I128(value))
        }
        schema::Kind::Enum(_) | schema::Kind::Sum(_) => {
            let (_, position) = read(bytes, body, 4, end)?;
            let (variant, _) = read(bytes, position, 2, end)?;
            if matches!(definition.kind, schema::Kind::Enum(_)) {
                Some(Atom::Enum {
                    type_id: id,
                    variant: variant as u16,
                })
            } else {
                Some(Atom::Sum {
                    type_id: id,
                    variant: variant as u16,
                })
            }
        }
        _ => {
            let wire = &bytes[start..end];
            let original = checked.original();
            #[cfg(verus_keep_ghost)]
            proof! {spec::part_equal(bytes,start,end,wire);spec::slice_equal(checked.view().0,original);}
            Some(Atom::Canonical(Value {
                schema: original,
                type_id: id,
                wire,
            }))
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
pub(super) closed spec fn walked<'a>(bytes:Seq<u8>,definitions:Seq<schema::Definition<'a>>,position:usize,limit:usize,id:u32,limits:Seq<u64>,used:Seq<u64>)->(Result<usize,Failure>,Seq<u64>) {
    spec::run(bytes,definitions,(position,limit,seq![Task::Value(id)]),limits,used)
}
pub(super) closed spec fn materialized<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,id:u32,start:usize,end:usize)->Option<Atom<'a>> {
    if start>end || end>bytes@.len() {None} else {match spec::find(checked.view().1.definitions@,id,checked.view().1.definitions@.len()){None=>None,Some(d)=>match spec::read(bytes@,start,1,end){None=>None,Some((tag,body))=>match d.kind {
        schema::Kind::Bool=>Some(Atom::Bool(tag==2)),
        schema::Kind::U128{..}=>match spec::read(bytes@,body,16,end){Some((v,_))=>Some(Atom::U128(v)),None=>None},
        schema::Kind::I128{..}=>match spec::signed(bytes@,body,end){Some((v,_))=>Some(Atom::I128(v)),None=>None},
        schema::Kind::Enum(_)|schema::Kind::Sum(_)=>match spec::read(bytes@,body,4,end){None=>None,Some((_,p))=>match spec::read(bytes@,p,2,end){None=>None,Some((v,_))=>
            if matches!(d.kind,schema::Kind::Enum(_)) {Some(Atom::Enum{type_id:id,variant:v as u16})} else {Some(Atom::Sum{type_id:id,variant:v as u16})}
        }},
        _=>Some(Atom::Canonical(Value{schema:spec::slice(checked.view().0),type_id:id,wire:spec::part(bytes,start,end)})),
    }}}}
}
}

/// Decode a complete typed value under its actual original schema witness.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (result,final(meter).used.counters@)==spec::decode(bytes,checked,start,limit,id,old(meter).limits.counters@,old(meter).used.counters@),))]
pub(super) fn decode<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    start: usize,
    limit: usize,
    id: u32,
    meter: &mut Meter,
) -> Result<(Atom<'a>, usize), Failure> {
    let end = walk(
        bytes,
        checked.description().definitions,
        start,
        limit,
        id,
        meter,
    )?;
    match materialize(bytes, checked, id, start, end) {
        Some(value) => Ok((value, end)),
        None => Err(Failure::Wire),
    }
}

#[cfg(verus_keep_ghost)]
verus! {
pub(super) closed spec fn decoded_root<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,id:u32,limits:Seq<u64>,used:Seq<u64>)->spec::RootResult<'a> {
    match spec::find(checked.view().1.definitions@,id,checked.view().1.definitions@.len()){None=>(Err(Failure::Wire),used,Seq::empty()),Some(d)=>{
        if matches!(d.kind,schema::Kind::Record(_)) {
            let r=spec::record(bytes,checked,0,bytes@.len() as usize,id,false,limits,used);
            match r.0{Err(e)=>(Err(e),r.1,r.2),Ok((atom,fields,end))=>if end!=bytes@.len(){(Err(Failure::Wire),r.1,r.2)} else {(Ok((atom,Some(fields),None)),r.1,r.2)}}
        } else {
            let r=super::spec::charge(limits,used,Resource::Read,1);let reads=seq![Access::Root(r.0.is_ok())];
            match r.0{Err(e)=>(Err(Failure::Budget(e)),r.1,reads),Ok(())=>{
                if matches!(d.kind,schema::Kind::SumPayload(_)) {
                    let s=super::spec::charge(limits,r.1,Resource::Step,1);match s.0{Err(e)=>(Err(Failure::Budget(e)),s.1,reads),Ok(())=>match spec::scan(bytes@,checked.view().1.definitions@,0,bytes@.len() as usize,id){
                        None=>(Err(Failure::Wire),s.1,reads),Some((body,next))=>match spec::read(bytes@,5,2,bytes@.len() as usize){
                            None=>(Err(Failure::Wire),s.1,reads),Some((variant,_))=>{
                                let decoded=match next {
                                    None=>(Ok((None,body)),s.1,Seq::empty()),
                                    Some(Task::Value(type_id))=>match spec::find(checked.view().1.definitions@,type_id,checked.view().1.definitions@.len()){
                                        None=>(Err(Failure::Wire),s.1,Seq::empty()),Some(child)=>if matches!(child.kind,schema::Kind::Record(_)){
                                            let p=spec::record(bytes,checked,body,bytes@.len() as usize,type_id,true,limits,s.1);
                                            (match p.0{Err(e)=>Err(e),Ok((_,fields,end))=>Ok((Some(fields),end))},p.1,p.2)
                                        } else {
                                            let p=spec::decode(bytes,checked,body,bytes@.len() as usize,type_id,limits,s.1);
                                            (match p.0{Err(e)=>Err(e),Ok((_,end))=>Ok((None,end))},p.1,Seq::empty())
                                        },
                                    },Some(_)=>(Err(Failure::Wire),s.1,Seq::empty()),
                                };
                                let all=reads+decoded.2;
                                match decoded.0{Err(e)=>(Err(e),decoded.1,all),Ok((fields,end))=>if end!=bytes@.len(){(Err(Failure::Wire),decoded.1,all)} else {match spec::materialize(bytes,checked,id,0,end){Some(atom)=>(Ok((atom,None,Some((variant as u16,fields)))),decoded.1,all),None=>(Err(Failure::Wire),decoded.1,all)}}}
                            },
                        },
                    }}
                } else {
                    let p=spec::decode(bytes,checked,0,bytes@.len() as usize,id,limits,r.1);
                    match p.0{Err(e)=>(Err(e),p.1,reads),Ok((atom,end))=>if end!=bytes@.len(){(Err(Failure::Wire),p.1,reads)} else {(Ok((atom,None,match atom{Atom::Sum{variant,..}=>Some((variant,None)),_=>None})),p.1,reads)}}
                }
            }}
        }
    }}
}
}

/// Actual root and payload reads, including the first denied attempt.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Access {
    Root(bool),
    Field(u16, bool),
    PayloadField(u16, bool),
}
/// Complete validated root custody and real schema-shaped observations.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct DecodedRoot<'a> {
    pub(super) atom: Atom<'a>,
    pub(super) fields: Option<Vec<super::decision::Field<'a>>>,
    // The correspondence proof retains complete sum-payload custody. Numeric
    // bindings currently project only roots and ordinary record fields.
    #[cfg_attr(not(verus_keep_ghost), allow(dead_code))]
    pub(super) sum: Option<(u16, Option<Vec<super::decision::Field<'a>>>)>,
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(v)=>Ok((v.0@,v.1)),Err(e)=>Err(e)},final(meter).used.counters@,final(accesses)@)=={
        let r=spec::record_fields(bytes,checked,fields@,span.0,span.1,payload,fields@.len(),old(meter).limits.counters@,old(meter).used.counters@);
        (r.0,r.1,old(accesses)@+r.2)
    },))]
fn record_fields<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    fields: &[schema::Field<'a>],
    span: (usize, usize),
    payload: bool,
    meter: &mut Meter,
    accesses: &mut Vec<Access>,
) -> Result<(Vec<super::decision::Field<'a>>, usize), Failure> {
    let (start, limit) = span;
    #[cfg(verus_keep_ghost)]
    proof! {reveal(spec::record_fields);}
    let mut output = Vec::new();
    let mut position = start;
    let mut index = 0usize;
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=meter.used.counters@;let ghost prefix=accesses@;}
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant index<=fields.len(),start==span.0,limit==span.1,meter.limits==old(meter).limits,
        spec::record_fields(bytes,checked,fields@,start,limit,payload,index as nat,meter.limits.counters@,initial)==(Ok((output@,position)),meter.used.counters@,accesses@.skip(prefix.len() as int)),
        accesses@==prefix+spec::record_fields(bytes,checked,fields@,start,limit,payload,index as nat,meter.limits.counters@,initial).2,
        initial==old(meter).used.counters@,prefix==old(accesses)@,
        decreases fields.len()-index,))]
    while index < fields.len() {
        #[cfg(verus_keep_ghost)]
        proof! {reveal_with_fuel(spec::record_fields,2);}
        if let Err(e) = meter.charge(Resource::Step, 1) {
            #[cfg(verus_keep_ghost)]
            proof! {spec::record_failed(bytes,checked,fields@,start,limit,payload,(index+1) as nat,fields@.len(),meter.limits.counters@,initial);}
            return Err(Failure::Budget(e));
        }
        let charged = meter.charge(Resource::Read, 1);
        accesses.push(if payload {
            Access::PayloadField(fields[index].id, charged.is_ok())
        } else {
            Access::Field(fields[index].id, charged.is_ok())
        });
        if let Err(e) = charged {
            #[cfg(verus_keep_ghost)]
            proof! {spec::record_failed(bytes,checked,fields@,start,limit,payload,(index+1) as nat,fields@.len(),meter.limits.counters@,initial);}
            return Err(Failure::Budget(e));
        }
        let Some((id, child)) = read(bytes, position, 2, limit) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::record_failed(bytes,checked,fields@,start,limit,payload,(index+1) as nat,fields@.len(),meter.limits.counters@,initial);}
            return Err(Failure::Wire);
        };
        if id != fields[index].id as u128 {
            #[cfg(verus_keep_ghost)]
            proof! {spec::record_failed(bytes,checked,fields@,start,limit,payload,(index+1) as nat,fields@.len(),meter.limits.counters@,initial);}
            return Err(Failure::Wire);
        }
        let (value, end) = match decode(bytes, checked, child, limit, fields[index].type_id, meter)
        {
            Ok(v) => v,
            Err(e) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::record_failed(bytes,checked,fields@,start,limit,payload,(index+1) as nat,fields@.len(),meter.limits.counters@,initial);}
                return Err(e);
            }
        };
        output.push(super::decision::Field {
            id: fields[index].id,
            value,
        });
        position = end;
        index += 1;
    }
    Ok((output, position))
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok((a,f,end))=>Ok((a,f@,end)),Err(e)=>Err(e)},final(meter).used.counters@,final(accesses)@)=={
        let r=spec::record(bytes,checked,span.0,span.1,id,payload,old(meter).limits.counters@,old(meter).used.counters@);(r.0,r.1,old(accesses)@+r.2)
    },))]
fn record<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    span: (usize, usize),
    id: u32,
    payload: bool,
    meter: &mut Meter,
    accesses: &mut Vec<Access>,
) -> Result<(Atom<'a>, Vec<super::decision::Field<'a>>, usize), Failure> {
    let (start, limit) = span;
    #[cfg(verus_keep_ghost)]
    proof! {reveal(spec::record);}
    if let Err(e) = meter.charge(Resource::Step, 1) {
        return Err(Failure::Budget(e));
    }
    let Some((body, Some(Task::Record(fields, 0)))) =
        scan(bytes, checked.description().definitions, start, limit, id)
    else {
        return Err(Failure::Wire);
    };
    let (values, end) = record_fields(
        bytes,
        checked,
        fields,
        (body, limit),
        payload,
        meter,
        accesses,
    )?;
    if let Err(e) = meter.charge(Resource::Step, 1) {
        return Err(Failure::Budget(e));
    }
    let Some(atom) = materialize(bytes, checked, id, start, end) else {
        return Err(Failure::Wire);
    };
    Ok((atom, values, end))
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(v)=>Ok(spec::root_view(v)),Err(e)=>Err(e)},final(meter).used.counters@,final(accesses)@)=={
        let r=spec::decode_root(bytes,checked,id,old(meter).limits.counters@,old(meter).used.counters@);(r.0,r.1,old(accesses)@+r.2)
    },))]
pub(super) fn decode_root<'a>(
    bytes: &'a [u8],
    checked: &schema::Checked<'a>,
    id: u32,
    meter: &mut Meter,
    accesses: &mut Vec<Access>,
) -> Result<DecodedRoot<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(decoded_root);}
    let Some(definition) = find(checked.description().definitions, id) else {
        return Err(Failure::Wire);
    };
    if matches!(definition.kind, schema::Kind::Record(_)) {
        let (atom, fields, end) =
            record(bytes, checked, (0, bytes.len()), id, false, meter, accesses)?;
        if end != bytes.len() {
            return Err(Failure::Wire);
        }
        return Ok(DecodedRoot {
            atom,
            fields: Some(fields),
            sum: None,
        });
    }
    let charged = meter.charge(Resource::Read, 1);
    accesses.push(Access::Root(charged.is_ok()));
    if let Err(e) = charged {
        return Err(Failure::Budget(e));
    }
    if matches!(definition.kind, schema::Kind::SumPayload(_)) {
        if let Err(e) = meter.charge(Resource::Step, 1) {
            return Err(Failure::Budget(e));
        }
        let Some((body, next)) = scan(bytes, checked.description().definitions, 0, bytes.len(), id)
        else {
            return Err(Failure::Wire);
        };
        let Some((variant, _)) = read(bytes, 5, 2, bytes.len()) else {
            return Err(Failure::Wire);
        };
        let mut payload = None;
        let mut end = body;
        if let Some(task) = next {
            let Task::Value(type_id) = task else {
                return Err(Failure::Wire);
            };
            let Some(child) = find(checked.description().definitions, type_id) else {
                return Err(Failure::Wire);
            };
            if matches!(child.kind, schema::Kind::Record(_)) {
                let (_, fields, after) = record(
                    bytes,
                    checked,
                    (body, bytes.len()),
                    type_id,
                    true,
                    meter,
                    accesses,
                )?;
                payload = Some(fields);
                end = after;
            } else {
                end = decode(bytes, checked, body, bytes.len(), type_id, meter)?.1;
            }
        }
        if end != bytes.len() {
            return Err(Failure::Wire);
        }
        let Some(atom) = materialize(bytes, checked, id, 0, end) else {
            return Err(Failure::Wire);
        };
        return Ok(DecodedRoot {
            atom,
            fields: None,
            sum: Some((variant as u16, payload)),
        });
    }
    let (atom, end) = decode(bytes, checked, 0, bytes.len(), id, meter)?;
    if end != bytes.len() {
        return Err(Failure::Wire);
    }
    Ok(DecodedRoot {
        atom,
        fields: None,
        sum: match atom {
            Atom::Sum { variant, .. } => Some((variant, None)),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests;
