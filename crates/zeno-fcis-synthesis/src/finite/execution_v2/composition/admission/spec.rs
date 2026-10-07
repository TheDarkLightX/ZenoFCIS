use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn domain_kind(d:Domain)->Kind{match d{Domain::Bool=>Kind::Bool,Domain::I128{..}=>Kind::I128,Domain::U128{..}=>Kind::U128,Domain::Enum{type_id,..}=>Kind::Enum(type_id),Domain::Sum{type_id,..}=>Kind::Sum(type_id),Domain::Bytes=>Kind::Bytes,Domain::Text=>Kind::Text}}
pub(in super::super) open spec fn leaf_kind(d:InputLeaf)->Kind{match d{InputLeaf::Bool=>Kind::Bool,InputLeaf::I128{..}=>Kind::I128,InputLeaf::U128{..}=>Kind::U128,InputLeaf::Enum{type_id,..}=>Kind::Enum(type_id),InputLeaf::Sum{type_id,..}=>Kind::Sum(type_id)}}
pub(in super::super) open spec fn atom_kind(d:Atom)->Kind{match d{Atom::Bool(_)=>Kind::Bool,Atom::I128(_)=>Kind::I128,Atom::U128(_)=>Kind::U128,Atom::Enum{type_id,..}=>Kind::Enum(type_id),Atom::Sum{type_id,..}=>Kind::Sum(type_id),Atom::Bytes(_)=>Kind::Bytes,Atom::Text(_)=>Kind::Text,}}
pub(in super::super) open spec fn schema<'a>(d:&Descriptor<'a>,source:Source)->Schema<'a>{match source{Source::State=>d.state,Source::Command=>d.command,Source::Context=>d.context}}
pub(in super::super) open spec fn first(fields:Seq<InputField>,id:u16,i:int)->bool{0<=i<fields.len()&&fields[i].id==id&&(forall|j:int|0<=j<i==>fields[j].id!=id)}
pub(in super::super) open spec fn field_leaf<'a>(fields:Seq<InputField>,id:u16)->Option<&'a InputLeaf>{
    if exists|i:int|first(fields,id,i){let i=choose|i:int|first(fields,id,i);Some(&fields[i].leaf)}else{None}
}
pub(in super::super) open spec fn find<'a>(schema:Schema<'a>,selector:Selector)->Option<&'a InputLeaf>{match(schema,selector){
    (Schema::Leaf(leaf),Selector::Root)=>Some(leaf),
    (Schema::Record(fields),Selector::Field(id))=>field_leaf(fields@,id),_=>None,
}}
pub(in super::super) open spec fn expression(d:&Descriptor,expr:Expr)->Option<Kind>{match expr{
    Expr::Input(s,id)=>match find(schema(d,s),Selector::Field(id)){Some(l)=>Some(leaf_kind(*l)),None=>None},
    Expr::Root(s)=>match find(schema(d,s),Selector::Root){Some(l)=>Some(leaf_kind(*l)),None=>None},
    Expr::Output(i)=>if i<d.output_types@.len(){Some(leaf_kind(d.output_types@[i as int]))}else{None},
    Expr::Constant(a)=>Some(atom_kind(a)),
}}
pub(in super::super) open spec fn domain_valid(d:Domain)->bool{match d{
    Domain::I128{min,max}=>min<=max,Domain::U128{min,max}=>min<=max,
    Domain::Enum{variants,..}|Domain::Sum{variants,..}=>unique_variants(variants@),_=>true,
}}
pub(in super::super) open spec fn has_variant(v:Seq<super::super::super::InputVariant>,id:u16)->bool{exists|j:int|0<=j<v.len()&&v[j].id==id}
pub(in super::super) open spec fn variants_match(ids:Seq<u16>,v:Seq<super::super::super::InputVariant>)->bool{ids.len()==v.len()&&unique_variants(ids)&&(forall|j:int|0<=j<ids.len()==>has_variant(v,ids[j]))}
pub(in super::super) open spec fn matches_leaf(d:Domain,l:InputLeaf)->bool{match(d,l){
    (Domain::Bool,InputLeaf::Bool)=>true,(Domain::U128{min:a,max:b},InputLeaf::U128{min:c,max:d})=>a==c&&b==d,(Domain::I128{min:a,max:b},InputLeaf::I128{min:c,max:d})=>a==c as i128&&b==d as i128,
    (Domain::Enum{type_id:a,variants:x},InputLeaf::Enum{type_id:b,variants:y,..})|(Domain::Sum{type_id:a,variants:x},InputLeaf::Sum{type_id:b,variants:y,..})=>a==b&&variants_match(x@,y@),_=>false,
}}
pub(in super::super) open spec fn same_binding(a:Binding,b:Binding)->bool{a.source==b.source&&a.selector==b.selector}
pub(in super::super) open spec fn count(s:Schema)->usize{match s{Schema::Leaf(_)=>1,Schema::Record(f)=>f.len(),}}
pub(in super::super) open spec fn same_domain(a:ScalarDomain,b:ScalarDomain)->bool{a==b}
pub(in super::super) open spec fn input_domain(leaf:InputLeaf,domain:ScalarDomain)->bool{
    input_view::leaf_domain(leaf)==domain || match(leaf,domain){
        (InputLeaf::Enum{min,max,..}|InputLeaf::Sum{min,max,..},ScalarDomain::Bool)=>min==0&&max==1,_=>false}
}
pub(in super::super) open spec fn binding_valid(d:&Descriptor,i:int)->bool{
    (match find(schema(d,d.bindings@[i].source),d.bindings@[i].selector){Some(l)=>input_domain(*l,d.program.inputs@[i]),None=>false})
    &&(forall|j:int|0<=j<i==>!same_binding(d.bindings@[j],d.bindings@[i]))
}
pub(in super::super) open spec fn bindings(d:&Descriptor)->bool{
    d.bindings@.len()==d.program.inputs@.len()&&d.bindings@.len()==count(d.state) as int+count(d.command) as int+count(d.context) as int
    &&(forall|j:int|0<=j<d.bindings@.len()==>binding_valid(d,j))
}
pub(in super::super) open spec fn output_valid(d:&Descriptor,i:int)->bool{input_view::spec::leaf_valid(d.output_types@[i])&&input_view::leaf_domain(d.output_types@[i])==d.program.outputs@[i]}
pub(in super::super) open spec fn outputs(d:&Descriptor)->bool{
    d.output_types@.len()==d.program.outputs@.len()&&d.decision_output<d.output_types@.len()&&d.output_types@[d.decision_output as int] is I128
    &&(forall|j:int|0<=j<d.output_types@.len()==>output_valid(d,j))
}
pub(in super::super) open spec fn first_channel(d:&Descriptor,id:u32,i:int)->bool{0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i==>d.channels@[j].id!=id)}
pub(in super::super) open spec fn channel<'a>(d:&Descriptor<'a>,id:u32)->Option<Channel<'a>>{
    if exists|i:int|first_channel(d,id,i){Some(d.channels@[choose|i:int|first_channel(d,id,i)])}else{None}
}
pub(in super::super) open spec fn payload_entry(d:&Descriptor,f:Seq<PayloadField>,s:Seq<TypedField>,j:int)->bool{f[j].field==s[j].field&&expression(d,f[j].value)==Some(domain_kind(s[j].domain))&&schema_validation::spec::expression(s[j].domain,f[j].value)}
pub(in super::super) open spec fn payload(d:&Descriptor,f:Seq<PayloadField>,s:Seq<TypedField>)->bool{f.len()==s.len()&&(forall|j:int|0<=j<f.len()==>payload_entry(d,f,s,j))}
pub(in super::super) open spec fn delivery_valid(d:&Descriptor,p:Seq<DeliveryPlan>,i:int)->bool{
    (i==0||p[i-1].ordinal<p[i].ordinal)&&match channel(d,p[i].channel){None=>false,Some(c)=>expression(d,p[i].when)==Some(Kind::Bool)&&expression(d,p[i].destination)==Some(domain_kind(c.destination))&&expression(d,p[i].idempotency)==Some(domain_kind(c.idempotency))&&schema_validation::spec::expression(c.destination,p[i].destination)&&schema_validation::spec::expression(c.idempotency,p[i].idempotency)&&payload(d,p[i].payload@,c.payload@)}
}
pub(in super::super) open spec fn deliveries(d:&Descriptor,p:Seq<DeliveryPlan>)->bool{forall|i:int|0<=i<p.len()==>delivery_valid(d,p,i)}
pub(in super::super) open spec fn assignment_valid(d:&Descriptor,a:Seq<Assignment>,f:Seq<InputField>,i:int)->bool{a[i].field==f[i].id&&matches_leaf(a[i].domain,f[i].leaf)&&expression(d,a[i].value)==Some(leaf_kind(f[i].leaf))&&schema_validation::spec::expression(a[i].domain,a[i].value)}
pub(in super::super) open spec fn assignments(d:&Descriptor,a:Seq<Assignment>,f:Seq<InputField>)->bool{a.len()==f.len()&&(forall|i:int|0<=i<f.len()==>assignment_valid(d,a,f,i))}
pub(in super::super) open spec fn reason(d:&Descriptor,c:Class,id:Option<u32>)->bool{match(c,id){(Class::Accept,None)=>true,(Class::Reject,Some(id))|(Class::CommittedFailure,Some(id))=>exists|i:int|0<=i<d.reasons@.len()&&d.reasons@[i].id==id&&d.reasons@[i].class==c,_=>false}}
pub(in super::super) open spec fn branch_valid(d:&Descriptor,f:Seq<InputField>,min:i64,max:i64,i:int)->bool{
    let b=d.branches@[i];min as i128<=b.code<=max as i128&&(i==0||d.branches@[i-1].code<b.code)&&reason(d,b.class,b.reason)
    &&if b.class==Class::Reject{b.assignments@.len()==0&&b.effects@.len()==0&&b.outbox@.len()==0}else{assignments(d,b.assignments@,f)&&deliveries(d,b.effects@)&&deliveries(d,b.outbox@)}
}
pub(in super::super) open spec fn branches(d:&Descriptor,f:Seq<InputField>)->bool{
    d.decision_output<d.output_types@.len()&&match d.output_types@[d.decision_output as int]{InputLeaf::I128{min,max}=>min<=max&&max as int-min as int+1==d.branches@.len()&&(forall|i:int|0<=i<d.branches@.len()==>branch_valid(d,f,min,max,i)),_=>false}
}
pub(in super::super) open spec fn reason_valid(r:Seq<Reason>,i:int)->bool{r[i].id!=0&&r[i].class!=Class::Accept&&(i==0||r[i-1].id<r[i].id)}
pub(in super::super) open spec fn reasons(r:Seq<Reason>)->bool{forall|i:int|0<=i<r.len()==>reason_valid(r,i)}
pub(in super::super) open spec fn channel_field_valid(f:Seq<TypedField>,i:int)->bool{domain_valid(f[i].domain)&&(i==0||f[i-1].field<f[i].field)}
pub(in super::super) open spec fn channel_fields(f:Seq<TypedField>)->bool{forall|i:int|0<=i<f.len()==>channel_field_valid(f,i)}
pub(in super::super) open spec fn channel_valid(c:Seq<Channel>,i:int)->bool{c[i].id!=0&&(i==0||c[i-1].id<c[i].id)&&domain_valid(c[i].destination)&&domain_valid(c[i].idempotency)&&channel_fields(c[i].payload@)}
pub(in super::super) open spec fn channels(c:Seq<Channel>)->bool{forall|i:int|0<=i<c.len()==>channel_valid(c,i)}
pub(in super::super) open spec fn admitted(d:&Descriptor)->bool{
    match d.state{Schema::Leaf(_)=>false,Schema::Record(fields)=>
    ingress::spec::schema_valid(d.state)&&ingress::spec::schema_valid(d.command)&&ingress::spec::schema_valid(d.context)&&(bindings(d)&&outputs(d))
    &&super::super::super::super::evaluation::admission::spec::program(d.program.inputs@,d.program.outputs@,d.program.nodes@,d.program.roots@).is_ok()
    &&reasons(d.reasons@)&&channels(d.channels@)&&branches(d,fields@)&&laws::spec::metadata(d.laws@,d.required@)&&schema_validation::spec::law_literals(d.laws@)}
}
}

verus! {
pub(in super::super) proof fn first_chosen(fields:Seq<InputField>,id:u16,i:int)
    requires first(fields,id,i),
    ensures (choose|j:int|first(fields,id,j))==i,
{
    let j=choose|j:int|first(fields,id,j);
    if j<i {assert(fields[j].id!=id);} else if i<j {assert(fields[i].id!=id);}
}
pub(in super::super) proof fn channel_chosen(d:&Descriptor,id:u32,i:int)
    requires first_channel(d,id,i),
    ensures (choose|j:int|first_channel(d,id,j))==i,
{
    let j=choose|j:int|first_channel(d,id,j);
    if j<i {assert(d.channels@[j].id!=id);} else if i<j {assert(d.channels@[i].id!=id);}
}
}

verus! { pub(in super::super) open spec fn unique_variants(v:Seq<u16>)->bool{v.len()>0&&(forall|j:int,k:int|0<=j<k<v.len()==>v[j]!=v[k])} }
