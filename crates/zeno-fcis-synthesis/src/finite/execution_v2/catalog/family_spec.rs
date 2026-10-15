//! Exact closed-family catalog relation, preserving the independent legacy bind model.
use super::*;
use composition::descriptor::{DescriptorView, spec as data};
use composition::{DescriptorData, DescriptorProjection, Domain, NumericDescriptor, Schema};
use vstd::prelude::*;
verus! {
pub type CatalogView<'a,D>=(Seq<u8>,Seq<u8>,&'a D,&'a Framing,Seq<(u32,u32,u32)>);
pub(crate) open spec fn numeric_field(defs:Seq<schema::Definition>,f:schema::Field,a:super::super::input_view::Field<Domain>)->bool {
    f.id==a.id&&match spec::find(defs,f.type_id){Some(d)=>spec::domain(d,a.leaf),None=>false}
}
pub(crate) open spec fn numeric_fields(defs:Seq<schema::Definition>,fs:Seq<schema::Field>,actual:Seq<super::super::input_view::Field<Domain>>)->bool {
    fs.len()==actual.len()&&forall|i:int|0<=i<fs.len() ==> numeric_field(defs,fs[i],actual[i])
}
pub(crate) open spec fn numeric_schema(description:schema::Description,root:u32,actual:Schema<Domain>)->bool {
    let defs=description.definitions@;
    if let Schema::Original{checked,root:actual_root,..}=actual {
        actual_root==root&&schema::spec::encoding_matches(checked.view().0,description,u64::MAX)
    } else {match spec::find(defs,root){None=>false,Some(d)=>match(d.kind,actual){
        (schema::Kind::Record(fs),Schema::Record(a))=>numeric_fields(defs,fs@,a@),
        (_,Schema::Leaf(l))=>spec::domain(d,*l),_=>false,
    }}}
}
pub(super) open spec fn roots(description:schema::Description,d:&DescriptorProjection,f:&Framing)->bool {
    match d.view {
        DescriptorView::Legacy(d)=>spec::roots(description,d,f),
        DescriptorView::Numeric(d)=>description.root==f.state.root&&(matches!(d.state,Schema::Record(_))||matches!(d.state,Schema::Original{checked,root,..} if composition::ingress::original::spec::fields(checked,root).is_some()))
            &&numeric_schema(description,f.state.root,d.state)
            &&numeric_schema(description,f.command.root,d.command)
            &&numeric_schema(description,f.context.root,d.context),
    }
}
#[verifier::opaque]
pub open(in super) spec fn bind<'a,D:DescriptorData<'a>+'a>(checked:(Seq<u8>,schema::Description),contract:Seq<u8>,d:&'a D,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)->Result<CatalogView<'a,D>,Failure> {
    let p=d.model();
    if contract.len()>max{Err(Failure::Size)}else if !p.admitted(){Err(Failure::Descriptor)}
    else if !roots(checked.1,&p,f){Err(Failure::Roots)}
    else if !spec::channels(checked.1.definitions@,data::channels(&p)@,ls){Err(Failure::Channels)}
    else if !spec::branches(checked.1.definitions@,data::branches(&p)@,ls){Err(Failure::Deliveries)}
    else{match authority::family_spec::policy(&p,checked.0,f,ls){None=>Err(Failure::Encoding),Some(actual)=>if actual!=contract{Err(Failure::Policy)}else{Ok((checked.0,contract,d,f,ls))}}}
}
#[verifier::opaque]
pub open(in super) spec fn bind_original<'a,D:DescriptorData<'a>+'a>(original:Seq<u8>,description:schema::Description,limits:schema::Limits,
    contract:Seq<u8>,d:&'a D,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)->Result<CatalogView<'a,D>,Failure> {
    if contract.len()>max{Err(Failure::Size)}else{match schema::admission_result(original,description,limits){
        Err(e)=>Err(Failure::Schema(e)),Ok(checked)=>bind(checked,contract,d,f,ls,max),
    }}
}
pub proof fn legacy_bind<'a>(checked:(Seq<u8>,schema::Description),contract:Seq<u8>,d:&'a Descriptor<'a>,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)
    ensures bind(checked,contract,d,f,ls,max)==spec::bind(checked,contract,d,f,ls,max),
{
    reveal(bind);reveal(<Descriptor as DescriptorData>::model);composition::descriptor::legacy_admitted(d);authority::family_spec::legacy_policy_value(d,checked.0,f,ls);
}
pub proof fn legacy_bind_original<'a>(original:Seq<u8>,description:schema::Description,limits:schema::Limits,contract:Seq<u8>,d:&'a Descriptor<'a>,f:&'a Framing,ls:Seq<(u32,u32,u32)>,max:u64)
    ensures bind_original(original,description,limits,contract,d,f,ls,max)==spec::bind_original(original,description,limits,contract,d,f,ls,max),
{
    reveal(bind_original);
    if let Ok(checked)=schema::admission_result(original,description,limits){legacy_bind(checked,contract,d,f,ls,max);}
}
}
