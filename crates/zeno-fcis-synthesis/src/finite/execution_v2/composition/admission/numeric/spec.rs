//! Independent whole numeric descriptor predicate, including unused metadata.
use super::super::spec as legacy;
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super::super::super) open spec fn leaf(d:Domain)->bool {
    match d {Domain::Sum{variants,..}=>0<variants@.len()&&(forall|i:int|0<i<variants@.len()==>variants@[i-1]<variants@[i]),Domain::Bool=>true,Domain::I128{min,max}=>min<=max,Domain::U128{min,max}=>min<=max,_=>false}
}
pub(in super::super::super) open spec fn same_domain(a:Domain,b:Domain)->bool {
    match(a,b){
        (Domain::Sum{type_id:a,variants:x},Domain::Sum{type_id:b,variants:y})=>a==b&&x@==y@,
        (Domain::Bool,Domain::Bool)=>true,
        (Domain::I128{min:a,max:b},Domain::I128{min:c,max:d})=>a==c&&b==d,
        (Domain::U128{min:a,max:b},Domain::U128{min:c,max:d})=>a==c&&b==d,_=>false}
}
pub(in super::super::super) open spec fn code_matches(original:Domain,numeric:Domain)->bool {
    match(original,numeric){(Domain::Sum{variants,..},Domain::I128{min,max})=>0<variants@.len()&&variants@[0] as int==min&&variants@[variants@.len()-1] as int==max,_=>same_domain(original,numeric)}
}
pub(in super::super::super) open spec fn field_valid(f:Seq<input_view::Field<Domain>>,i:int)->bool {
    leaf(f[i].leaf)&&(i==0||f[i-1].id<f[i].id)
}
pub(in super::super::super) open spec fn schema_valid(s:Schema<Domain>)->bool {
    match s{Schema::Original{checked,root,projection}=>original::spec::valid(checked,root,projection),Schema::Leaf(d)=>leaf(*d),Schema::Record(f)=>forall|i:int|0<=i<f@.len()==>field_valid(f@,i)}
}
pub(in super::super::super) proof fn record_field_valid(fields:&[input_view::Field<Domain>],j:int)
    requires schema_valid(Schema::Record(fields)),0<=j<fields@.len(),
    ensures field_valid(fields@,j),
{
    assert(schema_valid(Schema::Record(fields)) == (forall|i:int|0<=i<fields@.len() ==> field_valid(fields@,i))) by (compute);
    assert(field_valid(fields@,j));
}
pub(in super::super::super) open spec fn count(s:Schema<Domain>)->usize {
    match s{Schema::Original{projection,..}=>match projection{Projection::None=>0,Projection::Root(_)=>1,Projection::Fields(f)=>f.len()},Schema::Leaf(_)=>1,Schema::Record(f)=>f.len()}
}
pub(in super::super::super) open spec fn schema<'a>(d:&NumericDescriptor<'a>,s:Source)->Schema<'a,Domain<'a>> {
    match s{Source::State=>d.state,Source::Command=>d.command,Source::Context=>d.context}
}
pub(in super::super::super) open spec fn first(f:Seq<input_view::Field<Domain>>,id:u16,i:int)->bool {
    0<=i<f.len()&&f[i].id==id&&(forall|j:int|0<=j<i==>f[j].id!=id)
}
pub(in super::super::super) proof fn first_chosen(f:Seq<input_view::Field<Domain>>,id:u16,i:int)
    requires first(f,id,i),
    ensures (choose|j:int|first(f,id,j))==i,
{
    let j=choose|j:int|first(f,id,j);
    if j<i {assert(f[j].id!=id);} else if i<j {assert(f[i].id!=id);}
}
// Separate so the quantifier's trigger names plain parameters: an or-pattern
// binding is encoded as a conditional, which Z3 rejects inside triggers.
pub(in super::super::super) open spec fn find_field<'a>(f:Seq<input_view::Field<Domain<'a>>>,id:u16)->Option<Domain<'a>> {
    if exists|i:int|first(f,id,i){Some(f[choose|i:int|first(f,id,i)].leaf)}else{None}
}
pub(in super::super::super) open spec fn find<'a>(s:Schema<'a,Domain<'a>>,selector:Selector)->Option<Domain<'a>> {
    match(s,selector){(Schema::Leaf(d),Selector::Root)|(Schema::Original{projection:Projection::Root(d),..},Selector::Root)=>Some(*d),
        (Schema::Record(f),Selector::Field(id))|(Schema::Original{projection:Projection::Fields(f),..},Selector::Field(id))=>find_field(f@,id),_=>None}
}
pub(in super::super::super) proof fn find_first<'a>(s:Schema<'a,Domain<'a>>,selector:Selector,f:&'a [input_view::Field<Domain<'a>>],id:u16,i:int)
    requires selected_fields(s)==Some(f),selector==Selector::Field(id),first(f@,id,i),
    ensures find(s,selector)==Some(f@[i].leaf),
{
    first_chosen(f@,id,i);
    assert(exists|j:int|first(f@,id,j));
}
pub(in super::super::super) open spec fn selected_fields<'a>(s:Schema<'a,Domain<'a>>)->Option<&'a [input_view::Field<Domain<'a>>]> {
    match s {Schema::Record(f)|Schema::Original{projection:Projection::Fields(f),..}=>Some(f),_=>None}
}
pub(in super::super::super) open spec fn input_kind(s:Schema<Domain>,selector:Selector)->Option<Kind> {
    match s {Schema::Original{checked,root,..}=>match original::spec::selected_type(checked,root,selector){Some(id)=>original::spec::kind(checked,id),None=>None},
        _=>match find(s,selector){Some(v)=>Some(legacy::domain_kind(v)),None=>None}}
}
pub(in super::super::super) open spec fn expression(d:&NumericDescriptor,e:Expr)->Option<Kind> {
    match e{Expr::Input(s,id)=>input_kind(schema(d,s),Selector::Field(id)),
        Expr::Root(s)=>input_kind(schema(d,s),Selector::Root),
        Expr::Output(i)=>if i<d.output_types@.len(){Some(legacy::domain_kind(d.output_types@[i as int]))}else{None},
        Expr::Constant(a)=>Some(legacy::atom_kind(a))}
}
pub(in super::super::super) open spec fn binding_valid(d:&NumericDescriptor,i:int)->bool {
    (match find(schema(d,d.bindings@[i].source),d.bindings@[i].selector){Some(v)=>code_matches(v,d.program.inputs@[i]),None=>false})
        &&(forall|j:int|0<=j<i==>!legacy::same_binding(d.bindings@[j],d.bindings@[i]))
}
pub(in super::super::super) open spec fn bindings(d:&NumericDescriptor)->bool {
    d.bindings@.len()==d.program.inputs@.len()
        &&d.bindings@.len()==count(d.state) as int+count(d.command) as int+count(d.context) as int
        &&(forall|i:int|0<=i<d.bindings@.len()==>binding_valid(d,i))
}
pub(in super::super::super) open spec fn output_valid(d:&NumericDescriptor,i:int)->bool {
    leaf(d.output_types@[i])&&code_matches(d.output_types@[i],d.program.outputs@[i])
}
pub(in super::super::super) open spec fn outputs(d:&NumericDescriptor)->bool {
    d.output_types@.len()==d.program.outputs@.len()&&d.decision_output<d.output_types@.len()
        &&d.output_types@[d.decision_output as int] is I128
        &&(forall|i:int|0<=i<d.output_types@.len()==>output_valid(d,i))
}
pub(in super::super::super) open spec fn first_channel(d:&NumericDescriptor,id:u32,i:int)->bool {
    0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i==>d.channels@[j].id!=id)
}
pub(in super::super::super) proof fn channel_chosen(d:&NumericDescriptor,id:u32,i:int)
    requires first_channel(d,id,i),
    ensures (choose|j:int|first_channel(d,id,j))==i,
{
    let j=choose|j:int|first_channel(d,id,j);
    if j<i {assert(d.channels@[j].id!=id);} else if i<j {assert(d.channels@[i].id!=id);}
}
pub(in super::super::super) open spec fn channel<'a>(d:&NumericDescriptor<'a>,id:u32)->Option<Channel<'a>> {
    if exists|i:int|first_channel(d,id,i){Some(d.channels@[choose|i:int|first_channel(d,id,i)])}else{None}
}
pub(in super::super::super) open spec fn payload_entry(d:&NumericDescriptor,f:Seq<PayloadField>,s:Seq<TypedField>,i:int)->bool {
    f[i].field==s[i].field&&expression(d,f[i].value)==Some(legacy::domain_kind(s[i].domain))
        &&schema_validation::spec::expression(s[i].domain,f[i].value)
}
pub(in super::super::super) open spec fn payload(d:&NumericDescriptor,f:Seq<PayloadField>,s:Seq<TypedField>)->bool {
    f.len()==s.len()&&(forall|i:int|0<=i<f.len()==>payload_entry(d,f,s,i))
}
pub(in super::super::super) open spec fn delivery_valid(d:&NumericDescriptor,p:Seq<DeliveryPlan>,i:int)->bool {
    (i==0||p[i-1].ordinal<p[i].ordinal)&&match channel(d,p[i].channel){None=>false,Some(c)=>
        expression(d,p[i].when)==Some(Kind::Bool)&&expression(d,p[i].destination)==Some(legacy::domain_kind(c.destination))
        &&expression(d,p[i].idempotency)==Some(legacy::domain_kind(c.idempotency))
        &&schema_validation::spec::expression(c.destination,p[i].destination)
        &&schema_validation::spec::expression(c.idempotency,p[i].idempotency)&&payload(d,p[i].payload@,c.payload@)}
}
pub(in super::super::super) open spec fn deliveries(d:&NumericDescriptor,p:Seq<DeliveryPlan>)->bool {
    forall|i:int|0<=i<p.len()==>delivery_valid(d,p,i)
}
pub(in super::super::super) open spec fn assignment_valid(d:&NumericDescriptor,a:Seq<Assignment>,f:Seq<input_view::Field<Domain>>,i:int)->bool {
    a[i].field==f[i].id&&same_domain(a[i].domain,f[i].leaf)&&expression(d,a[i].value)==Some(legacy::domain_kind(f[i].leaf))
        &&schema_validation::spec::expression(a[i].domain,a[i].value)
}
pub(in super::super::super) open spec fn original_assignment(d:&NumericDescriptor,a:Seq<Assignment>,f:Seq<crate::finite::canonical_v2::schema::Field>,c:&crate::finite::canonical_v2::schema::Checked,i:int)->bool {
    a[i].field==f[i].id&&original::spec::matches_type(c,f[i].type_id,a[i].domain)
        &&expression(d,a[i].value)==Some(legacy::domain_kind(a[i].domain))&&schema_validation::spec::expression(a[i].domain,a[i].value)
}
pub(in super::super::super) open spec fn original_assignments(d:&NumericDescriptor,a:Seq<Assignment>,c:&crate::finite::canonical_v2::schema::Checked,root:u32)->bool {
    match original::spec::fields(c,root){None=>false,Some(f)=>a.len()==f@.len()&&(forall|i:int|0<=i<f@.len()==>original_assignment(d,a,f@,c,i))}
}
pub(in super::super::super) open spec fn assignments(d:&NumericDescriptor,a:Seq<Assignment>,f:Seq<input_view::Field<Domain>>)->bool {
    match d.state {Schema::Original{checked,root,..}=>original_assignments(d,a,checked,root),_=>
    a.len()==f.len()&&(forall|i:int|0<=i<f.len()==>assignment_valid(d,a,f,i))}
}
pub(in super::super::super) open spec fn reason(d:&NumericDescriptor,c:Class,id:Option<u32>)->bool {
    match(c,id){(Class::Accept,None)=>true,(Class::Reject,Some(id))|(Class::CommittedFailure,Some(id))=>
        exists|i:int|0<=i<d.reasons@.len()&&d.reasons@[i].id==id&&d.reasons@[i].class==c,_=>false}
}
pub(in super::super::super) open spec fn branch_valid(d:&NumericDescriptor,f:Seq<input_view::Field<Domain>>,min:i128,max:i128,i:int)->bool {
    let b=d.branches@[i];min<=b.code<=max&&(i==0||d.branches@[i-1].code<b.code)&&reason(d,b.class,b.reason)
        &&if b.class==Class::Reject{b.assignments@.len()==0&&b.effects@.len()==0&&b.outbox@.len()==0}
        else{assignments(d,b.assignments@,f)&&deliveries(d,b.effects@)&&deliveries(d,b.outbox@)}
}
pub(in super::super::super) open spec fn branches(d:&NumericDescriptor,f:Seq<input_view::Field<Domain>>)->bool {
    d.decision_output<d.output_types@.len()&&match d.output_types@[d.decision_output as int]{Domain::I128{min,max}=>
        min<=max&&max as int-min as int+1==d.branches@.len()
        &&(forall|i:int|0<=i<d.branches@.len()==>branch_valid(d,f,min,max,i)),_=>false}
}
pub(in super::super::super) open spec fn admitted(d:&NumericDescriptor)->bool {
    let fs=match d.state {Schema::Leaf(_)=>None,Schema::Record(f)=>Some(f@),Schema::Original{checked,root,..}=>
        if original::spec::fields(checked,root).is_some(){Some(Seq::empty())}else{None}};
    match fs {None=>false,Some(f)=>schema_valid(d.state)&&schema_valid(d.command)&&schema_valid(d.context)
        &&bindings(d)&&outputs(d)&&d.program.admission().is_ok()
        &&legacy::reasons(d.reasons@)&&legacy::channels(d.channels@)&&branches(d,f)
        &&laws::spec::metadata(d.laws@,d.required@)&&schema_validation::spec::law_literals(d.laws@)}
}
}
