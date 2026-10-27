//! Independent decoding and exact embedding of every raw legacy leaf descriptor.
use super::super::super::super::canonical_v2::spec as integer;
use super::super::super::spec as logical;
use super::super::spec as legacy;
use super::*;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn decode(bytes:Seq<u8>,offset:usize,leaf:LeafView)->Option<(Atom<'static>,usize)> {
    match leaf {
        LeafView::Legacy(l)=>match legacy::scalar(bytes,offset,*l){Some((v,end))=>Some((Atom::I128(v as i128),end)),None=>None},
        LeafView::Numeric(domain)=>match integer::read_unsigned(bytes,offset,1){None=>None,Some((tag,start))=>match domain {
            Domain::Sum{type_id,variants}=>if tag!=10{None}else{match integer::read_unsigned(bytes,start,4){None=>None,Some((actual,start))=>match integer::read_unsigned(bytes,start,2){None=>None,Some((variant,end))=>match integer::read_unsigned(bytes,end,1){None=>None,Some((payload,end))=>if actual==type_id as int&&payload==0&&(exists|i:int|0<=i<variants@.len()&&variants@[i] as int==variant){Some((Atom::I128(variant as i128),end))}else{None}}}}},
            Domain::Bool=>if tag==1{Some((Atom::Bool(false),start))}else if tag==2{Some((Atom::Bool(true),start))}else{None},
            Domain::I128{min,max}=>if tag!=4{None}else{match integer::read_signed(bytes,start){Some((v,end))=>if min<=v<=max{Some((Atom::I128(v),end))}else{None},None=>None}},
            Domain::U128{min,max}=>if tag!=3{None}else{match integer::read_unsigned(bytes,start,16){Some((v,end))=>if min<=v<=max{Some((Atom::U128(v),end))}else{None},None=>None}},
            _=>None,
        }},
    }
}
pub(crate) proof fn legacy_embedding(bytes:Seq<u8>,offset:usize,leaf:&Leaf)
    ensures match decode(bytes,offset,LeafView::Legacy(leaf)) {
        Some((Atom::I128(v),end))=>i128::MIN<=v<=i128::MAX && legacy::scalar(bytes,offset,*leaf)==Some((v as i128,end)),
        None=>legacy::scalar(bytes,offset,*leaf).is_none(),_=>false,
    },
{}
}

verus! {
pub(crate) open spec fn fields<'a>(view:FieldsView<'a>)->Seq<(u16,LeafView<'a>)> {
    match view {
        FieldsView::Legacy(fs)=>Seq::new(fs@.len(),|i:int|(fs@[i].id,LeafView::Legacy(&fs[i].leaf))),
        FieldsView::Numeric(fs)=>Seq::new(fs@.len(),|i:int|(fs@[i].id,LeafView::Numeric(fs@[i].leaf))),
    }
}
pub(crate) open spec fn numeric_field_valid(fs:Seq<Field<Domain>>,i:int)->bool {
    0<=i<fs.len() && (i==0 || fs[i-1].id<fs[i].id) && match fs[i].leaf {
        Domain::Sum{variants,..}=>0<variants@.len()&&(forall|i:int|0<i<variants@.len()==>variants@[i-1]< #[trigger] variants@[i]),Domain::Bool=>true,Domain::I128{min,max}=>min<=max,Domain::U128{min,max}=>min<=max,_=>false,
    }
}
pub(crate) open spec fn valid(view:FieldsView)->bool {
    match view {
        FieldsView::Legacy(fs)=>legacy::schema_valid(fs@),
        FieldsView::Numeric(fs)=>forall|i:int|0<=i<fs@.len() ==> numeric_field_valid(fs@,i),
    }
}
/// Instantiate the unchanged universal schema predicate at a concrete record.
pub(crate) proof fn numeric_schema_valid(fs:&[Field<Domain>])
    ensures valid(FieldsView::Numeric(fs))==(forall|i:int|0<=i<fs@.len() ==> numeric_field_valid(fs@,i)),
{
    assert(valid(FieldsView::Numeric(fs))==(forall|i:int|0<=i<fs@.len() ==> numeric_field_valid(fs@,i))) by (compute);
}
/// One rejected field witnesses failure of the unchanged universal schema law.
pub(crate) proof fn numeric_schema_rejected(fs:&[Field<Domain>],i:int)
    requires 0<=i<fs@.len(),!numeric_field_valid(fs@,i),
    ensures !valid(FieldsView::Numeric(fs)),
{
    numeric_schema_valid(fs);
    if valid(FieldsView::Numeric(fs)) {assert(numeric_field_valid(fs@,i));}
}
pub(crate) open spec fn field(bytes:Seq<u8>,offset:usize,id:u16,leaf:LeafView)->Option<(Atom<'static>,usize)> {
    match integer::read_unsigned(bytes,offset,2) {
        Some((actual,start))=>if actual==id as u128 {decode(bytes,start,leaf)}else{None},None=>None,
    }
}
pub(crate) open(in super::super) spec fn prefix(bytes:Seq<u8>,fs:Seq<(u16,LeafView)>,count:nat,offset:usize,limits:Seq<u64>,used:Seq<u64>)
    ->(Result<(Seq<Atom<'static>>,usize),Failure>,Seq<u64>,Seq<AccessAttempt>)
    recommends count<=fs.len(),limits.len()==8,used.len()==8,
    decreases count,
{
    if count==0 {(Ok((Seq::empty(),offset)),used,Seq::empty())} else {
        let previous=prefix(bytes,fs,(count-1) as nat,offset,limits,used);
        match previous.0 {
            Err(_)=>previous,
            Ok((values,position))=>{
                let (id,leaf)=fs[count as int-1];
                let charged=logical::charge(limits,previous.1,Resource::Read,1);
                let attempts=previous.2.push(AccessAttempt{field_id:id,permitted:charged.0.is_ok()});
                match charged.0 {
                    Err(e)=>(Err(Failure::Budget(e)),charged.1,attempts),
                    Ok(())=>match field(bytes,position,id,leaf) {
                        None=>(Err(Failure::Field(id)),charged.1,attempts),
                        Some((v,end))=>(Ok((values.push(v),end)),charged.1,attempts),
                    },
                }
            },
        }
    }
}
pub(crate) proof fn prefix_failure(bytes:Seq<u8>,fs:Seq<(u16,LeafView)>,failed:nat,count:nat,offset:usize,limits:Seq<u64>,used:Seq<u64>)
    requires failed<=count<=fs.len(),limits.len()==8,used.len()==8,prefix(bytes,fs,failed,offset,limits,used).0.is_err(),
    ensures prefix(bytes,fs,count,offset,limits,used)==prefix(bytes,fs,failed,offset,limits,used),
    decreases count-failed,
{if count>failed {prefix_failure(bytes,fs,failed,(count-1) as nat,offset,limits,used);}}
pub(crate) open(in super::super::super) spec fn projection(bytes:Seq<u8>,view:FieldsView,limits:Seq<u64>,used:Seq<u64>)
    ->(Result<Seq<Atom<'static>>,Failure>,Seq<u64>,Seq<AccessAttempt>) {
    if !valid(view){(Err(Failure::Schema),used,Seq::empty())}else {
        let charged=logical::charge(limits,used,Resource::Byte,bytes.len() as u64);
        match charged.0 {
            Err(e)=>(Err(Failure::Budget(e)),charged.1,Seq::empty()),
            Ok(())=>match legacy::header(bytes,fields(view).len() as usize) {
                None=>(Err(Failure::Header),charged.1,Seq::empty()),
                Some(offset)=>{
                    let p=prefix(bytes,fields(view),fields(view).len(),offset,limits,charged.1);
                    (match p.0{Err(e)=>Err(e),Ok((values,end))=>if end as int==bytes.len(){Ok(values)}else{Err(Failure::Trailing)}},p.1,p.2)
                },
            },
        }
    }
}
pub(crate) open spec fn lift(values:Seq<i128>)->Seq<Atom<'static>> {
    values.map(|_:int,v:i128|Atom::I128(v as i128))
}
/// Lifting a decoded prefix preserves its final append.
pub(crate) proof fn lift_push(values:Seq<i128>,value:i128)
    ensures lift(values.push(value))==lift(values).push(Atom::I128(value as i128)),
{
    assert(lift(values.push(value)) =~= lift(values).push(Atom::I128(value as i128)));
}
/// The tagged primitive decoder preserves an original record field exactly.
proof fn legacy_field(bytes:Seq<u8>,offset:usize,descriptor:&Field)
    ensures field(bytes,offset,descriptor.id,LeafView::Legacy(&descriptor.leaf))==
        (match legacy::field(bytes,offset,*descriptor){None=>None,Some((v,end))=>Some((Atom::I128(v as i128),end))}),
{
    if let Some((id,start))=integer::read_unsigned(bytes,offset,2) {
        legacy_embedding(bytes,start,&descriptor.leaf);
    }
}
proof fn legacy_prefix(bytes:Seq<u8>,fs:&[Field],count:nat,offset:usize,limits:Seq<u64>,used:Seq<u64>)
    requires count<=fs@.len(),limits.len()==8,used.len()==8,
    ensures prefix(bytes,fields(FieldsView::Legacy(fs)),count,offset,limits,used)=={
        let p=legacy::prefix(bytes,fs@,count,offset,limits,used);
        (match p.0{Err(e)=>Err(e),Ok((v,end))=>Ok((lift(v),end))},p.1,p.2)
    },
    decreases count,
{
    reveal_with_fuel(prefix,2);
    reveal_with_fuel(legacy::prefix,2);
    if count==0 {
        assert(lift(Seq::<i128>::empty()) =~= Seq::<Atom<'static>>::empty());
    } else {
        legacy_prefix(bytes,fs,(count-1) as nat,offset,limits,used);
        let previous=legacy::prefix(bytes,fs@,(count-1) as nat,offset,limits,used);
        let descriptor=&fs[(count-1) as int];
        assert(fields(FieldsView::Legacy(fs))[count as int-1]==(descriptor.id,LeafView::Legacy(&descriptor.leaf)));
        if let Ok((values,position))=previous.0 {
            legacy_field(bytes,position,descriptor);
            if let Some((value,end))=legacy::field(bytes,position,*descriptor) {
                lift_push(values,value);
            }
        }
    }
}
/// Both the public legacy wrapper and typed record view use the same schema predicate.
pub(crate) proof fn legacy_schema_valid(fs:&[Field])
    ensures valid(FieldsView::Legacy(fs))==super::super::schema_valid(fs@),
{
    reveal(super::super::schema_valid);
}
pub(crate) proof fn legacy_projection(bytes:Seq<u8>,fs:&[Field],limits:Seq<u64>,used:Seq<u64>)
    requires limits.len()==8,used.len()==8,
    ensures projection(bytes,FieldsView::Legacy(fs),limits,used)=={
        let p=super::super::projection(bytes,fs@,limits,used);
        (match p.0{Err(e)=>Err(e),Ok(v)=>Ok(lift(v))},p.1,p.2)
    },
{
    reveal(super::super::projection);
    let charged=logical::charge(limits,used,Resource::Byte,bytes.len() as u64);
    if let Some(offset)=legacy::header(bytes,fs@.len() as usize) {
        legacy_prefix(bytes,fs,fs@.len(),offset,limits,charged.1);
    }
}
}
