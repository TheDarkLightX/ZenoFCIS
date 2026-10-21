//! One root/record byte model, with an exact old scalar projection theorem.
use super::super::super::super::spec::charge;
use super::super::spec as legacy;
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super::super) open spec fn shape<'a>(schema:SchemaView<'a>)->Shape<'a> {
    match schema {
        SchemaView::Legacy(Schema::Original{..})=>Shape::Empty,
        SchemaView::Numeric(Schema::Original{projection,..})=>match projection{Projection::None=>Shape::Empty,Projection::Root(d)=>Shape::Leaf(LeafView::Numeric(*d)),Projection::Fields(f)=>Shape::Record(FieldsView::Numeric(f))},
        SchemaView::Legacy(Schema::Leaf(l))=>Shape::Leaf(LeafView::Legacy(l)),
        SchemaView::Legacy(Schema::Record(f))=>Shape::Record(FieldsView::Legacy(f)),
        SchemaView::Numeric(Schema::Leaf(l))=>Shape::Leaf(LeafView::Numeric(*l)),
        SchemaView::Numeric(Schema::Record(f))=>Shape::Record(FieldsView::Numeric(f)),
    }
}
pub(in super::super::super) open spec fn valid(schema:SchemaView)->bool {
    match schema {
        SchemaView::Legacy(s)=>legacy::schema_valid(s),
        SchemaView::Numeric(s)=>super::super::super::admission::numeric::spec::schema_valid(s),
    }
}
pub(in super::super::super) open spec fn reify(leaf:LeafView,code:Atom<'static>)->Option<Atom<'static>> {
    match leaf {
        LeafView::Legacy(l)=>match code {Atom::I128(v)=>if i64::MIN<=v<=i64::MAX {legacy::reify(*l,v as i64)}else{None},_=>None},
        LeafView::Numeric(d)=>if super::super::super::schema_validation::spec::atom(d,code){Some(code)}else{None},
    }
}
pub(in super::super::super) open spec fn numeric_fields(fs:Seq<input_view::Field<Domain>>,codes:Seq<Atom<'static>>,n:nat)->Seq<Field<'static>> {
    Seq::new(n,|i:int|Field{id:fs[i].id,value:codes[i]})
}
pub(in super::super::super) open spec fn fields(fs:FieldsView,codes:Seq<Atom<'static>>)->Option<Seq<Field<'static>>> {
    match fs {
        FieldsView::Legacy(fs)=>match super::super::super::super::numeric::spec::project_all(codes) {
            None=>None,Some(values)=>legacy::fields(fs@,values,fs@.len()),
        },
        FieldsView::Numeric(fs)=>if fs@.len()!=codes.len(){None}else{Some(numeric_fields(fs@,codes,fs@.len()))},
    }
}
pub(in super::super::super) open spec fn decoded<'a>(d:Decoded<'a,Atom<'static>>)->(legacy::ValueView<'a>,Seq<Atom<'static>>) {
    (legacy::value(d.value),d.scalars@)
}
/// Constructor reductions keep the generic decoder view exact.
pub(in super::super::super) proof fn decoded_record<'a>(values:&Vec<Field<'a>>,codes:&Vec<Atom<'static>>)
    ensures decoded(Decoded{value:Value::Record(*values),scalars:*codes})==(legacy::ValueView::Record(values@),codes@),
{
    assert(decoded(Decoded{value:Value::Record(*values),scalars:*codes})==(legacy::ValueView::Record(values@),codes@)) by (compute);
}
pub(in super::super::super) proof fn decoded_leaf(atom:Atom<'static>,codes:&Vec<Atom<'static>>)
    ensures decoded(Decoded{value:Value::Leaf(atom),scalars:*codes})==(legacy::ValueView::Leaf(atom),codes@),
{
    assert(decoded(Decoded{value:Value::Leaf(atom),scalars:*codes})==(legacy::ValueView::Leaf(atom),codes@)) by (compute);
}
pub(in super::super::super) open spec fn project<'a>(bytes:Seq<u8>,schema:SchemaView<'a>,source:u8,limits:Seq<u64>,used:Seq<u64>)
 ->(Result<(legacy::ValueView<'a>,Seq<Atom<'static>>),Failure>,Seq<u64>,Seq<Read>) {
    if !valid(schema){(Err(Failure::Schema),used,Seq::empty())}else if let SchemaView::Numeric(Schema::Original{checked,root,projection})=schema {
        super::super::original::spec::project(crate::finite::historical_i64::execution_v2::compound::spec::slice(bytes),checked,root,projection,source,limits,used)
    }else {
        match shape(schema) {
            Shape::Empty=>(Err(Failure::Schema),used,Seq::empty()),
            Shape::Record(fs)=>{
                let r=typed::spec::projection(bytes,fs,limits,used);let reads=legacy::attempts(source,r.2);
                match r.0 {
                    Err(e)=>(Err(Failure::Record(e)),r.1,reads),
                    Ok(codes)=>match fields(fs,codes) {
                        None=>(Err(Failure::Reification),r.1,reads),
                        Some(values)=>(Ok((legacy::ValueView::Record(values),codes)),r.1,reads),
                    },
                }
            },
            Shape::Leaf(leaf)=>{
                let b=charge(limits,used,Resource::Byte,bytes.len() as u64);
                match b.0{Err(e)=>(Err(Failure::Budget(e)),b.1,Seq::empty()),Ok(())=>{
                    let r=charge(limits,b.1,Resource::Read,1);
                    let reads=seq![Read{source,selector:Selector::Root,permitted:r.0.is_ok()}];
                    match r.0{Err(e)=>(Err(Failure::Budget(e)),r.1,reads),Ok(())=>{
                        match typed::spec::decode(bytes,0,leaf) {
                            None=>(Err(Failure::Root),r.1,reads),
                            Some((code,end))=>if end!=bytes.len(){(Err(Failure::Root),r.1,reads)}else {
                                match reify(leaf,code) {
                                    None=>(Err(Failure::Reification),r.1,reads),
                                    Some(atom)=>(Ok((legacy::ValueView::Leaf(atom),seq![code])),r.1,reads),
                                }
                            },
                        }
                    }}
                }}
            },
        }
    }
}
/// Lifting old scalar codes preserves every value, including the empty tuple.
pub(in super::super::super) proof fn legacy_lift(values:Seq<i64>)
    ensures super::super::super::super::numeric::spec::project_all(typed::spec::lift(values))==Some(values),
{
    assert(typed::spec::lift(values) =~= super::super::super::super::super::evaluation::typed::spec::lift(values));
    super::super::super::super::numeric::spec::lift_projects(values);
}
pub(in super::super::super) proof fn legacy_embedding<'a>(bytes:Seq<u8>,schema:Schema<'a>,source:u8,limits:Seq<u64>,used:Seq<u64>)
    requires limits.len()==8,used.len()==8,
    ensures project(bytes,SchemaView::Legacy(schema),source,limits,used)=={
        let r=legacy::project(bytes,schema,source,limits,used);
        (match r.0{Err(e)=>Err(e),Ok((v,codes))=>Ok((v,typed::spec::lift(codes)))},r.1,r.2)
    },
{
    match schema {
        Schema::Original{..}=>{},
        Schema::Record(fs)=>{
            typed::spec::legacy_schema_valid(fs);
            typed::spec::legacy_projection(bytes,fs,limits,used);
            if let Ok(codes)=super::super::super::super::input_view::projection(bytes,fs@,limits,used).0 {
                legacy_lift(codes);
            }
        },
        Schema::Leaf(leaf)=>{
            typed::spec::legacy_embedding(bytes,0,leaf);
            if let Some((code,end))=input_view::spec::scalar(bytes,0,*leaf) {
                assert(typed::spec::lift(seq![code]) =~= seq![Atom::I128(code as i128)]);
            }
        },
    }
}
}
