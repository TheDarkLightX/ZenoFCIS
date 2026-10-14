//! Exact original-schema membership, projection and refusal accounting.
use super::super::spec::ValueView;
use super::*;
use crate::finite::execution_v2::spec::charge;
use vstd::prelude::*;
verus! {
pub(in super::super::super::super) open spec fn fields<'a>(c:&schema::Checked<'a>,root:u32)->Option<&'a [schema::Field<'a>]> {
    match compound::spec::find(c.view().1.definitions@,root,c.view().1.definitions@.len()) {
        Some(d)=>match d.kind {schema::Kind::Record(f)=>Some(f),_=>None},None=>None,
    }
}
pub(in super::super::super::super) open spec fn field_type(fs:Seq<schema::Field>,id:u16,n:nat)->Option<u32>
    recommends n<=fs.len(),decreases n,
{if n==0 {None} else {match field_type(fs,id,(n-1) as nat) {Some(v)=>Some(v),None=>if fs[n as int-1].id==id {Some(fs[n as int-1].type_id)} else {None}}}}
pub(in super::super::super::super) proof fn field_found(fs:Seq<schema::Field>,id:u16,n:nat,total:nat)
    requires n<=total<=fs.len(),field_type(fs,id,n).is_some(),
    ensures field_type(fs,id,total)==field_type(fs,id,n),decreases total-n,
{if total>n {field_found(fs,id,n,(total-1) as nat);}}
pub(in super::super::super::super) open spec fn selected_type(c:&schema::Checked,root:u32,s:Selector)->Option<u32> {
    match s {Selector::Root=>Some(root),Selector::Field(id)=>match fields(c,root){Some(f)=>field_type(f@,id,f@.len()),None=>None},Selector::PayloadField(_)=>None}
}
pub(in super::super::super::super) open spec fn variants(a:Seq<schema::Variant>,b:Seq<u16>)->bool {a.len()==b.len()&&forall|i:int|0<=i<a.len()==>a[i].id==b[i]}
pub(in super::super::super::super) open spec fn matches_type(c:&schema::Checked,id:u32,d:Domain)->bool {
    match compound::spec::find(c.view().1.definitions@,id,c.view().1.definitions@.len()) {None=>false,Some(t)=>match(t.kind,d) {
        (schema::Kind::Bool,Domain::Bool)=>true,
        (schema::Kind::U128{min:a,max:b},Domain::U128{min:c,max:d})=>a==c&&b==d,
        (schema::Kind::I128{min:a,max:b},Domain::I128{min:c,max:d})=>a==c&&b==d,
        (schema::Kind::Enum(v),Domain::Enum{type_id,variants:vs})|(schema::Kind::Sum(v),Domain::Sum{type_id,variants:vs})=>id==type_id&&variants(v@,vs@),
        (schema::Kind::Unit|schema::Kind::Bytes{..}|schema::Kind::Text{..}|schema::Kind::Tuple(_)|schema::Kind::Record(_)|schema::Kind::SumPayload(_)|schema::Kind::Vector{..}|schema::Kind::Map{..},Domain::Canonical{schema,type_id})=>id==type_id&&c.view().0==schema@,
        _=>false,
    }}
}
pub(in super::super::super::super) open spec fn kind(c:&schema::Checked,id:u32)->Option<super::super::super::admission::Kind> {
    use super::super::super::admission::Kind;
    match compound::spec::find(c.view().1.definitions@,id,c.view().1.definitions@.len()) {None=>None,Some(t)=>Some(match t.kind {
        schema::Kind::Bool=>Kind::Bool,schema::Kind::I128{..}=>Kind::I128,schema::Kind::U128{..}=>Kind::U128,
        schema::Kind::Enum(_)=>Kind::Enum(id),schema::Kind::Sum(_)=>Kind::Sum(id),_=>Kind::Canonical(id),
    })}
}
pub(in super::super::super::super) open spec fn projection_field(c:&schema::Checked,root:u32,fs:Seq<input_view::Field<Domain>>,i:int)->bool {
    (i==0||fs[i-1].id<fs[i].id)&&super::super::super::admission::numeric::spec::leaf(fs[i].leaf)&&
    match selected_type(c,root,Selector::Field(fs[i].id)){Some(t)=>matches_type(c,t,fs[i].leaf),None=>false}
}
pub(in super::super::super::super) open spec fn valid(c:&schema::Checked,root:u32,p:Projection)->bool {
    compound::spec::find(c.view().1.definitions@,root,c.view().1.definitions@.len()).is_some()&&match p {
        Projection::None=>true,
        Projection::Root(d)=>super::super::super::admission::numeric::spec::leaf(*d)&&matches_type(c,root,*d),
        Projection::Fields(f)=>fields(c,root).is_some()&&(forall|i:int|0<=i<f@.len()==>projection_field(c,root,f@,i)),
    }
}
pub(super) proof fn valid_field(c:&schema::Checked,root:u32,p:Projection,fs:&[input_view::Field<Domain>],j:int)
    requires valid(c,root,p),p==Projection::Fields(fs),0<=j<fs@.len(),
    ensures projection_field(c,root,fs@,j),
{
    // The definition's quantifier triggers on the matched slice's view; name it.
    if let Projection::Fields(f)=p {assert(f@==fs@);}
}
pub(in super::super::super::super) open spec fn actual_field<'a>(fs:Seq<Field<'a>>,id:u16,n:nat)->Option<Atom<'a>>
    recommends n<=fs.len(),decreases n,
{if n==0 {None} else {match actual_field(fs,id,(n-1) as nat){Some(v)=>Some(v),None=>if fs[n as int-1].id==id {Some(fs[n as int-1].value)}else{None}}}}
pub(in super::super::super::super) proof fn actual_found(fs:Seq<Field>,id:u16,n:nat,total:nat)
    requires n<=total<=fs.len(),actual_field(fs,id,n).is_some(),ensures actual_field(fs,id,total)==actual_field(fs,id,n),decreases total-n,
{if total>n{actual_found(fs,id,n,(total-1) as nat);}}
pub(in super::super::super::super) open spec fn code(d:Domain,a:Atom)->Option<Atom<'static>> {
    match(d,a){(Domain::Bool,Atom::Bool(v))=>Some(Atom::Bool(v)),
        (Domain::U128{min,max},Atom::U128(v))=>if min<=v<=max{Some(Atom::U128(v))}else{None},
        (Domain::I128{min,max},Atom::I128(v))=>if min<=v<=max{Some(Atom::I128(v))}else{None},_=>None}
}
pub(in super::super::super::super) type Codes=(Result<Seq<Atom<'static>>,Failure>,Seq<u64>);
#[verifier::opaque]
pub(in super::super::super::super) open spec fn codes_fields(fs:Seq<input_view::Field<Domain>>,actual:Seq<Field>,n:nat,limits:Seq<u64>,used:Seq<u64>)->Codes
    recommends n<=fs.len(),decreases n,
{
    if n==0{(Ok(Seq::empty()),used)}else{let p=codes_fields(fs,actual,(n-1) as nat,limits,used);match p.0{
        Err(_)=>p,Ok(v)=>{let s=charge(limits,p.1,Resource::Step,1);match s.0{Err(e)=>(Err(Failure::Budget(e)),s.1),Ok(())=>{
            let f=fs[n as int-1];let a=match actual_field(actual,f.id,actual.len()){Some(a)=>code(f.leaf,a),None=>None};
            match a{Some(a)=>(Ok(v.push(a)),s.1),None=>(Err(Failure::Reification),s.1)}
        }}}
    }}
}
pub(in super::super::super::super) proof fn codes_failed(fs:Seq<input_view::Field<Domain>>,actual:Seq<Field>,n:nat,total:nat,limits:Seq<u64>,used:Seq<u64>)
    requires n<=total<=fs.len(),codes_fields(fs,actual,n,limits,used).0.is_err(),ensures codes_fields(fs,actual,total,limits,used)==codes_fields(fs,actual,n,limits,used),decreases total-n,
{reveal_with_fuel(codes_fields,2);if total>n{codes_failed(fs,actual,n,(total-1) as nat,limits,used);}}
pub(in super::super::super::super) open spec fn codes(p:Projection,r:compound::spec::RootView,limits:Seq<u64>,used:Seq<u64>)->Codes {
    match p{
        Projection::None=>(Ok(Seq::empty()),used),
        Projection::Root(d)=>{let s=charge(limits,used,Resource::Step,1);match s.0{Err(e)=>(Err(Failure::Budget(e)),s.1),Ok(())=>match code(*d,r.0){Some(a)=>(Ok(seq![a]),s.1),None=>(Err(Failure::Reification),s.1)}}},
        Projection::Fields(f)=>match r.1{Some(a)=>codes_fields(f@,a,f@.len(),limits,used),None=>(Err(Failure::Reification),used)},
    }
}
pub(in super::super::super::super) open spec fn attempts(source:u8,a:Seq<compound::Access>)->Seq<Read>{a.map(|_:int,a:compound::Access|{let(s,p)=match a{
    compound::Access::Root(p)=>(Selector::Root,p),compound::Access::Field(id,p)=>(Selector::Field(id),p),compound::Access::PayloadField(id,p)=>(Selector::PayloadField(id),p)};Read{source,selector:s,permitted:p}})}
pub(in super::super::super::super) open spec fn project<'a>(bytes:&'a [u8],c:&schema::Checked<'a>,root:u32,p:Projection<'a>,source:u8,limits:Seq<u64>,used:Seq<u64>)
 ->(Result<(ValueView<'a>,Seq<Atom<'static>>),Failure>,Seq<u64>,Seq<Read>) {
    let b=charge(limits,used,Resource::Byte,bytes@.len() as u64);match b.0{Err(e)=>(Err(Failure::Budget(e)),b.1,Seq::empty()),Ok(())=>{
        let d=compound::spec::decode_root(bytes,c,root,limits,b.1);let reads=attempts(source,d.2);
        match d.0{Err(compound::Failure::Wire)=>(Err(Failure::Root),d.1,reads),Err(compound::Failure::Budget(e))=>(Err(Failure::Budget(e)),d.1,reads),Ok(r)=>{
            let v=codes(p,r,limits,d.1);match v.0{Err(e)=>(Err(e),v.1,reads),Ok(codes)=>(Ok((match r.1{Some(f)=>ValueView::Record(f),None=>ValueView::Leaf(r.0)},codes)),v.1,reads)}
        }}
    }}
}
}
