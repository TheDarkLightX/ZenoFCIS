use super::super::super::spec::charge;
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn schema_valid(schema:Schema)->bool {
    match schema {Schema::Leaf(leaf)=>input_view::spec::leaf_valid(*leaf),Schema::Record(fields)=>input_view::schema_valid(fields@),}
}

pub(in super::super) open spec fn inverse(variants:Seq<super::super::super::InputVariant>,code:i64,n:nat)->Option<u16>
    recommends n<=variants.len(), decreases n,
{
    if n==0 {None} else {match inverse(variants,code,(n-1) as nat) {Some(v)=>Some(v),None=>if variants[n as int-1].code==code {Some(variants[n as int-1].id)} else {None}}}
}
pub(in super::super) proof fn inverse_found(variants:Seq<super::super::super::InputVariant>,code:i64,n:nat,total:nat)
    requires n<=total<=variants.len(),inverse(variants,code,n).is_some(),
    ensures inverse(variants,code,total)==inverse(variants,code,n),decreases total-n,
{if total>n {inverse_found(variants,code,n,(total-1) as nat);}}
/// Every selected inverse is an actual original table entry, in any table order.
pub(in super::super) proof fn inverse_member(variants:Seq<super::super::super::InputVariant>,code:i64,n:nat,id:u16)
    requires n<=variants.len(),inverse(variants,code,n)==Some(id),
    ensures exists|i:int|0<=i<n&&variants[i].code==code&&variants[i].id==id,
    decreases n,
{
    if n>0 {
        if inverse(variants,code,(n-1) as nat).is_some(){inverse_member(variants,code,(n-1) as nat,id);}
        else {assert(variants[n as int-1].code==code&&variants[n as int-1].id==id);assert(exists|i:int|0<=i<n&&variants[i].code==code&&variants[i].id==id);}
    }
}
/// Actual decoding and reification are inverse on every original variant identifier.
pub(in super::super) proof fn inverse_lookup(variants:Seq<super::super::super::InputVariant>,min:i64,max:i64,n:nat,id:u16,code:i64)
    requires n<=variants.len(),input_view::spec::variants_prefix(variants,min,max,n),input_view::spec::lookup(variants,id,n)==Some(code),
    ensures inverse(variants,code,n)==Some(id),
    decreases n,
{
    if n>0 {
        if input_view::spec::lookup(variants,id,(n-1) as nat).is_some(){
            inverse_lookup(variants,min,max,(n-1) as nat,id,code);
        }else{
            assert(variants[n as int-1].id==id&&variants[n as int-1].code==code);
            if let Some(prior)=inverse(variants,code,(n-1) as nat){
                inverse_member(variants,code,(n-1) as nat,prior);
                let i=choose|i:int|0<=i<n-1&&variants[i].code==code&&variants[i].id==prior;
                assert(variants[i].code!=variants[n as int-1].code);
            }
        }
    }
}
pub(in super::super) open spec fn reify(leaf:InputLeaf,code:i64)->Option<Atom<'static>> {
    match leaf {
        InputLeaf::Bool=>if code==0 {Some(Atom::Bool(false))} else if code==1 {Some(Atom::Bool(true))} else {None},
        InputLeaf::U128{min,max}=>if 0<=code && min<=code as u128<=max {Some(Atom::U128(code as u128))} else {None},
        InputLeaf::I128{min,max}=>if min<=code<=max {Some(Atom::I128(code as i128))} else {None},
        InputLeaf::Enum{type_id,variants,..}=>match inverse(variants@,code,variants@.len()) {Some(variant)=>Some(Atom::Enum{type_id,variant}),None=>None},
        InputLeaf::Sum{type_id,variants,..}=>match inverse(variants@,code,variants@.len()) {Some(variant)=>Some(Atom::Sum{type_id,variant}),None=>None},
    }
}
pub(in super::super) open spec fn scalar_atom(leaf:InputLeaf,code:i64,actual:Atom)->bool{match(leaf,actual){
    (InputLeaf::Bool,Atom::Bool(v))=>code==if v{1i64}else{0i64},
    (InputLeaf::U128{min,max},Atom::U128(v))=>0<=code&&min<=code as u128<=max&&v==code as u128,
    (InputLeaf::I128{min,max},Atom::I128(v))=>min<=code<=max&&v==code as i128,
    (InputLeaf::Enum{type_id,variants,..},Atom::Enum{type_id:t,variant})|
    (InputLeaf::Sum{type_id,variants,..},Atom::Sum{type_id:t,variant})=>type_id==t&&input_view::spec::lookup(variants@,variant,variants@.len())==Some(code),
    _=>false,
}}
pub(in super::super) proof fn scalar_reification(leaf:InputLeaf,code:i64,actual:Atom<'static>)
    requires input_view::spec::leaf_valid(leaf),scalar_atom(leaf,code,actual),
    ensures reify(leaf,code)==Some(actual),
{
    match(leaf,actual){
        (InputLeaf::Enum{min,max,variants,..},Atom::Enum{variant,..})|
        (InputLeaf::Sum{min,max,variants,..},Atom::Sum{variant,..})=>inverse_lookup(variants@,min,max,variants@.len(),variant,code),
        _=>{},
    }
}
pub(in super::super) open spec fn fields(descriptor:Seq<InputField>,codes:Seq<i64>,n:nat)->Option<Seq<Field<'static>>>
    recommends n<=descriptor.len(),decreases n,
{
    if descriptor.len()!=codes.len() {None} else if n==0 {Some(Seq::empty())} else {
        match fields(descriptor,codes,(n-1) as nat) {None=>None,Some(prior)=>match reify(descriptor[n as int-1].leaf,codes[n as int-1]) {None=>None,Some(value)=>Some(prior.push(Field{id:descriptor[n as int-1].id,value}))}}
    }
}
pub(in super::super) proof fn fields_failed(descriptor:Seq<InputField>,codes:Seq<i64>,n:nat,total:nat)
    requires n<=total<=descriptor.len(),fields(descriptor,codes,n).is_none(),
    ensures fields(descriptor,codes,total).is_none(),decreases total-n,
{if total>n {fields_failed(descriptor,codes,n,(total-1) as nat);}}
pub(in super::super) open spec fn attempts(source:u8,attempts:Seq<super::super::super::AccessAttempt>)->Seq<Read> {
    attempts.map(|_:int,a:super::super::super::AccessAttempt|Read{source,selector:Selector::Field(a.view().0),permitted:a.view().1})
}
pub enum ValueView<'a>{Leaf(Atom<'a>),Record(Seq<Field<'a>>)}
pub(in super::super) open spec fn value<'a>(value:Value<'a>)->ValueView<'a> {match value {Value::Leaf(a)=>ValueView::Leaf(a),Value::Record(f)=>ValueView::Record(f@),}}
pub(in super::super) open spec fn decoded<'a>(d:Decoded<'a>)->(ValueView<'a>,Seq<i64>) {(value(d.value),d.scalars@)}
pub(in super::super) open spec fn project<'a>(bytes:Seq<u8>,schema:Schema<'a>,source:u8,limits:Seq<u64>,used:Seq<u64>)->(Result<(ValueView<'a>,Seq<i64>),Failure>,Seq<u64>,Seq<Read>) {
    if !schema_valid(schema) {(Err(Failure::Schema),used,Seq::empty())} else {
        match schema {

            Schema::Record(descriptor)=>{
                let r=input_view::projection(bytes,descriptor@,limits,used);
                let reads=attempts(source,r.2);
                match r.0 {
                    Err(e)=>(Err(Failure::Record(e)),r.1,reads),
                    Ok(codes)=>match fields(descriptor@,codes,descriptor@.len()) {
                        None=>(Err(Failure::Reification),r.1,reads),
                        Some(values)=>(Ok((ValueView::Record(values),codes)),r.1,reads),
                    },
                }
            },
            Schema::Leaf(leaf)=>{
                let b=charge(limits,used,Resource::Byte,bytes.len() as u64);
                match b.0 {Err(e)=>(Err(Failure::Budget(e)),b.1,Seq::empty()),Ok(())=>{
                    let r=charge(limits,b.1,Resource::Read,1);
                    let reads=seq![Read{source,selector:Selector::Root,permitted:r.0.is_ok()}];
                    match r.0 {Err(e)=>(Err(Failure::Budget(e)),r.1,reads),Ok(())=>{
                        match input_view::spec::scalar(bytes,0,*leaf) {
                            None=>(Err(Failure::Root),r.1,reads),Some((code,end))=>{
                                if end!=bytes.len() {(Err(Failure::Root),r.1,reads)} else {
                                    match reify(*leaf,code) {None=>(Err(Failure::Reification),r.1,reads),Some(atom)=>(Ok((ValueView::Leaf(atom),seq![code])),r.1,reads)}
                                }
                            },
                        }
                    }}
                }}
            },
        }
    }
}
}
