//! Closed original-schema correspondence and exact ordered admission result.
use super::super::{InputField, InputLeaf, InputVariant, authority, composition};
use super::{Failure, schema};
use composition::{
    Atom, Branch, Channel, DeliveryPlan, Descriptor, Domain, Expr, Framing, Schema, TypedField,
};
use schema::{Definition, Description, Kind, Variant};
use vstd::prelude::*;

verus! {
pub type CatalogView<'a> = (Seq<u8>, Seq<u8>, &'a Descriptor<'a>, &'a Framing, Seq<(u32,u32,u32)>);
pub open spec fn first(defs:Seq<Definition>, id:u32, i:int)->bool {
    0<=i<defs.len() && defs[i].id==id && forall|j:int| 0<=j<i ==> defs[j].id!=id
}
pub open spec fn find(defs:Seq<Definition>, id:u32)->Option<Definition> {
    if exists|i:int| first(defs,id,i) { Some(defs[choose|i:int| first(defs,id,i)]) } else { None }
}
pub proof fn first_chosen(defs:Seq<Definition>,id:u32,i:int)
    requires first(defs,id,i), ensures find(defs,id)==Some(defs[i]),
{
    let j=choose|j:int| first(defs,id,j);
    assert(i==j);
}
pub open spec fn input_has(values:Seq<InputVariant>, id:u16)->bool {
    exists|j:int| 0<=j<values.len() && values[j].id==id
}
pub open spec fn input_variants(original:Seq<Variant>, actual:Seq<InputVariant>)->bool {
    original.len()==actual.len() && forall|i:int| 0<=i<original.len() ==> input_has(actual,original[i].id)
}
pub open spec fn domain_variants(original:Seq<Variant>, actual:Seq<u16>)->bool {
    original.len()==actual.len() && forall|i:int| 0<=i<original.len() ==> actual.contains(original[i].id)
}
pub open spec fn input_leaf(def:Definition, leaf:InputLeaf)->bool {
    match (def.kind,leaf) {
        (Kind::Bool,InputLeaf::Bool)=>true,
        (Kind::U128{min:a,max:b},InputLeaf::U128{min:c,max:d})=>a==c&&b==d,
        (Kind::I128{min:a,max:b},InputLeaf::I128{min:c,max:d})=>a==c as int && b==d as int,
        (Kind::Enum(v),InputLeaf::Enum{type_id,variants,..}) |
        (Kind::Sum(v),InputLeaf::Sum{type_id,variants,..})=>def.id==type_id && input_variants(v@,variants@),
        _=>false,
    }
}
pub open spec fn field(defs:Seq<Definition>, original:schema::Field, actual:InputField)->bool {
    original.id==actual.id && match find(defs,original.type_id) {Some(d)=>input_leaf(d,actual.leaf),None=>false}
}
pub open spec fn input_fields(defs:Seq<Definition>,original:Seq<schema::Field>,actual:Seq<InputField>)->bool {
    original.len()==actual.len() && forall|i:int|0<=i<original.len() ==> field(defs,original[i],actual[i])
}
pub open spec fn input_schema(defs:Seq<Definition>,root:u32,actual:Schema)->bool {
    match find(defs,root) {Some(d)=>match (d.kind,actual) {
        (Kind::Record(fields),Schema::Record(input))=>input_fields(defs,fields@,input@),
        (_,Schema::Leaf(leaf))=>input_leaf(d,*leaf),_=>false,
    },None=>false}
}
pub open spec fn roots(description:Description,d:&Descriptor,f:&Framing)->bool {
    description.root==f.state.root && matches!(d.state,Schema::Record(_))
    && input_schema(description.definitions@,f.state.root,d.state)
    && input_schema(description.definitions@,f.command.root,d.command)
    && input_schema(description.definitions@,f.context.root,d.context)
}
pub open spec fn domain(def:Definition,actual:Domain)->bool {
    match (def.kind,actual) {
        (Kind::Bool,Domain::Bool) | (Kind::Bytes{..},Domain::Bytes) | (Kind::Text{..},Domain::Text)=>true,
        (Kind::I128{min:a,max:b},Domain::I128{min:c,max:d})=>a==c && b==d,
        (Kind::U128{min:a,max:b},Domain::U128{min:c,max:d})=>a==c && b==d,
        (Kind::Enum(v),Domain::Enum{type_id,variants}) |
        (Kind::Sum(v),Domain::Sum{type_id,variants})=>def.id==type_id && domain_variants(v@,variants@),
        _=>false,
    }
}
pub open spec fn typed_field(defs:Seq<Definition>,original:schema::Field,actual:TypedField)->bool {
    original.id==actual.field && match find(defs,original.type_id) {Some(d)=>domain(d,actual.domain),None=>false}
}
pub open spec fn typed_fields(defs:Seq<Definition>,original:Seq<schema::Field>,actual:Seq<TypedField>)->bool {
    original.len()==actual.len() && forall|i:int|0<=i<original.len() ==> typed_field(defs,original[i],actual[i])
}
pub open spec fn channel(defs:Seq<Definition>,c:Channel,link:(u32,u32,u32))->bool {
    c.id==link.0 && match (find(defs,link.1),find(defs,link.2)) {
        (Some(dest),Some(payload))=>domain(dest,c.destination) && match payload.kind {
            Kind::Record(fields)=>typed_fields(defs,fields@,c.payload@),_=>false,
        },_=>false,
    }
}
pub open spec fn channel_entry(defs:Seq<Definition>,cs:Seq<Channel>,ls:Seq<(u32,u32,u32)>,i:int)->bool {
    (i==0 || ls[i-1].0<ls[i].0) && channel(defs,cs[i],ls[i])
}
pub open spec fn channels(defs:Seq<Definition>,cs:Seq<Channel>,ls:Seq<(u32,u32,u32)>)->bool {
    cs.len()==ls.len() && forall|i:int|0<=i<cs.len() ==> channel_entry(defs,cs,ls,i)
}
pub open spec fn ascii(bytes:Seq<u8>)->bool {forall|i:int|0<=i<bytes.len() ==> bytes[i]<128}
pub open spec fn bounded(def:Definition,e:Expr)->bool {
    match def.kind {
        Kind::Bytes{min,max}=>match e {Expr::Constant(Atom::Bytes(b))=>min<=b@.len()<=max,_=>false},
        Kind::Text{min,max}=>match e {Expr::Constant(Atom::Text(b))=>min<=b@.len()<=max && ascii(b@),_=>false},
        _=>true,
    }
}
pub open spec fn first_link(ls:Seq<(u32,u32,u32)>,id:u32,i:int)->bool {
    0<=i<ls.len() && ls[i].0==id && forall|j:int|0<=j<i ==> ls[j].0!=id
}
pub open spec fn link(ls:Seq<(u32,u32,u32)>,id:u32)->Option<(u32,u32)> {
    if exists|i:int|first_link(ls,id,i) {let x=ls[choose|i:int|first_link(ls,id,i)];Some((x.1,x.2))} else {None}
}
pub proof fn link_chosen(ls:Seq<(u32,u32,u32)>,id:u32,i:int)
    requires first_link(ls,id,i), ensures link(ls,id)==Some((ls[i].1,ls[i].2)),
{let j=choose|j:int|first_link(ls,id,j);assert(i==j);}
pub open spec fn payload_field(defs:Seq<Definition>,f:schema::Field,p:composition::PayloadField)->bool {
    f.id==p.field && match find(defs,f.type_id) {Some(d)=>bounded(d,p.value),None=>false}
}
pub open spec fn payload(defs:Seq<Definition>,fs:Seq<schema::Field>,ps:Seq<composition::PayloadField>)->bool {
    fs.len()==ps.len() && forall|i:int|0<=i<fs.len() ==> payload_field(defs,fs[i],ps[i])
}
pub open spec fn delivery(defs:Seq<Definition>,p:DeliveryPlan,ls:Seq<(u32,u32,u32)>)->bool {
    match link(ls,p.channel) {Some((a,b))=>match (find(defs,a),find(defs,b)) {
        (Some(dest),Some(body))=>bounded(dest,p.destination) && match body.kind {
            Kind::Record(fs)=>payload(defs,fs@,p.payload@),_=>false,
        },_=>false,
    },None=>false}
}
pub open spec fn deliveries(defs:Seq<Definition>,ps:Seq<DeliveryPlan>,ls:Seq<(u32,u32,u32)>)->bool {
    forall|i:int|0<=i<ps.len() ==> delivery(defs,ps[i],ls)
}
pub open spec fn branch(defs:Seq<Definition>,b:Branch,ls:Seq<(u32,u32,u32)>)->bool {
    deliveries(defs,b.effects@,ls) && deliveries(defs,b.outbox@,ls)
}
pub open spec fn branches(defs:Seq<Definition>,bs:Seq<Branch>,ls:Seq<(u32,u32,u32)>)->bool {
    forall|i:int|0<=i<bs.len() ==> branch(defs,bs[i],ls)
}
pub open spec fn bind<'a>(checked:(Seq<u8>,Description),contract:Seq<u8>,d:&'a Descriptor<'a>,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)->Result<CatalogView<'a>,Failure> {
    if contract.len()>max {Err(Failure::Size)}
    else if !composition::descriptor_admitted(d) {Err(Failure::Descriptor)}
    else if !roots(checked.1,d,f) {Err(Failure::Roots)}
    else if !channels(checked.1.definitions@,d.channels@,ls) {Err(Failure::Channels)}
    else if !branches(checked.1.definitions@,d.branches@,ls) {Err(Failure::Deliveries)}
    else {match authority::policy_value(d,checked.0,f,ls) {
        None=>Err(Failure::Encoding),Some(actual)=>if actual!=contract {Err(Failure::Policy)}
            else {Ok((checked.0,contract,d,f,ls))},
    }}
}
pub open spec fn bind_original<'a>(original:Seq<u8>,description:Description,limits:schema::Limits,
    contract:Seq<u8>,d:&'a Descriptor<'a>,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)->Result<CatalogView<'a>,Failure> {
    if contract.len()>max {Err(Failure::Size)} else {
        match schema::admission_result(original,description,limits) {
            Err(e)=>Err(Failure::Schema(e)),
            Ok(checked)=>bind(checked,contract,d,f,ls,max),
        }
    }
}
}
