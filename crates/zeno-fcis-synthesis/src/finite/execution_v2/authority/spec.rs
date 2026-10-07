//! Exact mathematical canonical bytes, independent of native allocation.
use super::canonical::Part;
use vstd::prelude::*;
verus! {
pub open spec fn word(value:u128)->Seq<u8> { seq![
    (value >> 120u32) as u8,
    (value >> 112u32) as u8,
    (value >> 104u32) as u8,
    (value >> 96u32) as u8,
    (value >> 88u32) as u8,
    (value >> 80u32) as u8,
    (value >> 72u32) as u8,
    (value >> 64u32) as u8,
    (value >> 56u32) as u8,
    (value >> 48u32) as u8,
    (value >> 40u32) as u8,
    (value >> 32u32) as u8,
    (value >> 24u32) as u8,
    (value >> 16u32) as u8,
    (value >> 8u32) as u8,
    (value >> 0u32) as u8
] }
pub open spec fn part(p:Part)->Seq<u8> {
    match p {Part::Word(v)=>seq![0u8]+word(v),
        Part::Bytes(b)=>seq![1u8]+word(b@.len() as u128)+b@}
}
pub open spec fn length(parts:Seq<Part>,n:nat)->nat
    recommends n<=parts.len(), decreases n,
{
    if n==0 {0} else {length(parts,(n-1) as nat)+17+
        match parts[n-1] {Part::Word(_)=>0nat,Part::Bytes(b)=>b@.len()}}
}
pub proof fn length_monotone(parts:Seq<Part>,a:nat,b:nat)
    requires a<=b<=parts.len(),
    ensures length(parts,a)<=length(parts,b), decreases b-a,
{
    if a<b {length_monotone(parts,a,(b-1) as nat);}
}
pub open spec fn size(parts:Seq<Part>)->Option<usize> {
    let n=length(parts,parts.len()); if n<=usize::MAX {Some(n as usize)} else {None}
}
pub open spec fn prefix(parts:Seq<Part>,n:nat)->Seq<u8>
    recommends n<=parts.len(), decreases n,
{if n==0 {Seq::empty()} else {prefix(parts,(n-1) as nat)+part(parts[n-1])}}
pub open spec fn encode(parts:Seq<Part>)->Option<Seq<u8>> {
    if length(parts,parts.len())<=usize::MAX {Some(prefix(parts,parts.len()))} else {None}
}
pub proof fn prefix_equal(a:Seq<Part>,b:Seq<Part>,n:nat)
    requires n<=a.len(),n<=b.len(),forall|i:int|0<=i<n ==> a[i]==b[i],
    ensures prefix(a,n)==prefix(b,n),length(a,n)==length(b,n),decreases n,
{if n>0{prefix_equal(a,b,(n-1) as nat);}}
/// Normative token data has no native references or chosen slice witnesses.
pub enum Token{Word(u128),Bytes(Seq<u8>)}
pub open spec fn token(p:Part)->Token{match p{Part::Word(v)=>Token::Word(v),Part::Bytes(b)=>Token::Bytes(b@)}}
pub open spec fn tokens(parts:Seq<Part>)->Seq<Token>{parts.map(|_:int,p:Part|token(p))}
pub open spec fn token_part(p:Token)->Seq<u8>{match p{Token::Word(v)=>seq![0u8]+word(v),Token::Bytes(b)=>seq![1u8]+word(b.len() as u128)+b}}
pub open spec fn token_length(parts:Seq<Token>,n:nat)->nat recommends n<=parts.len(),decreases n,
{if n==0{0}else{token_length(parts,(n-1) as nat)+17+match parts[n-1]{Token::Word(_)=>0nat,Token::Bytes(b)=>b.len()}}}
pub open spec fn token_prefix(parts:Seq<Token>,n:nat)->Seq<u8> recommends n<=parts.len(),decreases n,
{if n==0{Seq::empty()}else{token_prefix(parts,(n-1) as nat)+token_part(parts[n-1])}}
pub open spec fn token_encode(parts:Seq<Token>)->Option<Seq<u8>>{
    if token_length(parts,parts.len())<=usize::MAX{Some(token_prefix(parts,parts.len()))}else{None}
}
pub proof fn token_correspondence_prefix(parts:Seq<Part>,n:nat)
    requires n<=parts.len(),
    ensures prefix(parts,n)==token_prefix(tokens(parts),n),length(parts,n)==token_length(tokens(parts),n),decreases n,
{if n>0{token_correspondence_prefix(parts,(n-1) as nat);}}
pub proof fn token_correspondence(parts:Seq<Part>)
    ensures encode(parts)==token_encode(tokens(parts)),
{token_correspondence_prefix(parts,parts.len());}
pub proof fn tokens_append(a:Seq<Part>,b:Seq<Part>)
    ensures tokens(a+b)==tokens(a)+tokens(b),
{assert(tokens(a+b)=~=tokens(a)+tokens(b));}
}

use super::super::decision::{Atom, Candidate, Class, Delivery, Field, Patch};
verus! {
pub open spec fn class(c:Class)->u128 {match c {Class::Accept=>0,Class::Reject=>1,Class::CommittedFailure=>2}}
pub open spec fn atom<'a>(v:Atom<'a>)->Seq<Part<'a>> {
    match v {
        Atom::Bool(v)=>seq![Part::Word(0),Part::Word(v as u128)],
        Atom::I128(v)=>seq![Part::Word(1),Part::Word(v as u128)],
        Atom::U128(v)=>seq![Part::Word(2),Part::Word(v)],
        Atom::Enum{type_id,variant}=>seq![Part::Word(3),Part::Word(type_id as u128),Part::Word(variant as u128)],
        Atom::Sum{type_id,variant}=>seq![Part::Word(4),Part::Word(type_id as u128),Part::Word(variant as u128)],
        Atom::Bytes(v)=>seq![Part::Word(5),Part::Bytes(v)],
        Atom::Text(v)=>seq![Part::Word(6),Part::Bytes(v)],

    }
}

pub open spec fn fields_prefix<'a>(values:Seq<Field<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(), decreases n,
{ if n==0 {Seq::empty()} else {let v=values[n-1];fields_prefix(values,(n-1) as nat)+seq![Part::Word(v.id as u128)]+atom(v.value)} }
pub open spec fn fields<'a>(values:Seq<Field<'a>>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+fields_prefix(values,values.len())}

pub open spec fn patches_prefix<'a>(values:Seq<Patch<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(), decreases n,
{ if n==0 {Seq::empty()} else {let v=values[n-1];patches_prefix(values,(n-1) as nat)+seq![Part::Word(v.field as u128)]+atom(v.before)+atom(v.after)} }
pub open spec fn patches<'a>(values:Seq<Patch<'a>>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+patches_prefix(values,values.len())}

pub open spec fn deliveries_prefix<'a>(values:Seq<Delivery<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(), decreases n,
{ if n==0 {Seq::empty()} else {let v=values[n-1];deliveries_prefix(values,(n-1) as nat)+seq![Part::Word(v.ordinal as u128),Part::Word(v.channel as u128)]+atom(v.destination)+fields(v.payload@)+atom(v.idempotency)} }
pub open spec fn deliveries<'a>(values:Seq<Delivery<'a>>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+deliveries_prefix(values,values.len())}

pub open spec fn candidate<'a>(c:&Candidate<'a>)->Seq<Part<'a>> {
    let v=c.view();
    seq![Part::Word(0x5a434432),Part::Word(1),Part::Word(class(v.0))]+
        (match v.1 {None=>seq![Part::Word(0)],Some(r)=>seq![Part::Word(1),Part::Word(r as u128)]})+
        fields(v.2)+fields(v.3)+patches(v.4)+deliveries(v.5)+deliveries(v.6)
}
pub open spec fn delivery_views_prefix<'a>(values:Seq<super::super::decision::spec::DeliveryView<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let v=values[n-1];delivery_views_prefix(values,(n-1) as nat)+seq![Part::Word(v.0 as u128),Part::Word(v.1 as u128)]+atom(v.2)+fields(v.3)+atom(v.4)}}
pub open spec fn delivery_views<'a>(values:Seq<super::super::decision::spec::DeliveryView<'a>>)->Seq<Part<'a>>{
    seq![Part::Word(values.len() as u128)]+delivery_views_prefix(values,values.len())
}
pub open spec fn candidate_projection_parts<'a>(v:super::super::decision::spec::CandidateView<'a>)->Seq<Part<'a>>{
    seq![Part::Word(0x5a434432),Part::Word(1),Part::Word(class(v.0))]+
        (match v.1{None=>seq![Part::Word(0)],Some(r)=>seq![Part::Word(1),Part::Word(r as u128)]})+
        fields(v.2)+fields(v.3)+patches(v.4)+delivery_views(v.5)+delivery_views(v.6)
}
pub(super) proof fn delivery_projection<'a>(values:Seq<Delivery<'a>>,n:nat)
    requires n<=values.len(),
    ensures deliveries_prefix(values,n)==delivery_views_prefix(values.map(|_:int,v:Delivery<'a>|super::super::decision::spec::delivery_view(v)),n),
    decreases n,
{if n>0{delivery_projection(values,(n-1) as nat);}}
pub closed spec fn projected_candidate<'a>(c:&Candidate<'a>)->Seq<Part<'a>>{
    candidate_projection_parts(super::super::decision::spec::candidate_view(*c))
}
pub proof fn candidate_projection<'a>(c:&Candidate<'a>)
    ensures candidate(c)==projected_candidate(c),
        candidate(c)==candidate_projection_parts(super::super::composition::candidate_result(Ok(c)).unwrap()),
{
    reveal(projected_candidate);
    let v=c.view();
    delivery_projection(v.5,v.5.len());delivery_projection(v.6,v.6.len());
    let normalized=super::super::composition::candidate_result(Ok(c)).unwrap();
    assert(normalized.5 =~= super::super::decision::spec::candidate_view(*c).5);
    assert(normalized.6 =~= super::super::decision::spec::candidate_view(*c).6);
    assert(normalized==super::super::decision::spec::candidate_view(*c));
}
}

use super::super::decision::Attempt;
use super::super::{MeterFailure, Resource, Usage, laws};
verus! {
pub open spec fn resource(r:Resource)->u128 {crate::resource::resource_index(r) as u128}
pub open spec fn usage_view<'a>(u:Seq<u64>)->Seq<Part<'a>> {seq![Part::Word(u[0] as u128),Part::Word(u[1] as u128),Part::Word(u[2] as u128),Part::Word(u[3] as u128),Part::Word(u[4] as u128),Part::Word(u[5] as u128),Part::Word(u[6] as u128),Part::Word(u[7] as u128)]}
pub open spec fn usage<'a>(u:Usage)->Seq<Part<'a>> {usage_view(u.view())}
pub open spec fn optional_usage<'a>(u:Option<Seq<u64>>)->Seq<Part<'a>>{match u{None=>seq![Part::Word(0)],Some(v)=>seq![Part::Word(1)]+usage_view(v)}}
pub open spec fn budget<'a>(f:MeterFailure)->Seq<Part<'a>> {seq![Part::Word(resource(f.resource)),Part::Word(f.limit as u128),Part::Word(f.attempted as u128),Part::Word(f.overflow as u128)]}
pub open spec fn law_failure<'a>(f:laws::Failure)->Seq<Part<'a>> {match f {laws::Failure::Metadata=>seq![Part::Word(0)],laws::Failure::Frame=>seq![Part::Word(1)],laws::Failure::Undefined=>seq![Part::Word(2)],laws::Failure::Violated=>seq![Part::Word(3)],laws::Failure::Budget(b)=>seq![Part::Word(4)]+budget(b),}}
pub open spec fn observation<'a>(o:laws::Observation)->Seq<Part<'a>> {match o {laws::Observation::PreRoot=>seq![Part::Word(0)],
laws::Observation::CommandRoot=>seq![Part::Word(1)],
laws::Observation::ContextRoot=>seq![Part::Word(2)],
laws::Observation::PostRoot=>seq![Part::Word(3)],
laws::Observation::InitialRoot=>seq![Part::Word(4)],
laws::Observation::Pre(a)=>seq![Part::Word(5),Part::Word(a as u128)],
laws::Observation::Command(a)=>seq![Part::Word(6),Part::Word(a as u128)],
laws::Observation::Context(a)=>seq![Part::Word(7),Part::Word(a as u128)],
laws::Observation::Post(a)=>seq![Part::Word(8),Part::Word(a as u128)],
laws::Observation::Initial(a)=>seq![Part::Word(9),Part::Word(a as u128)],
laws::Observation::Class=>seq![Part::Word(10)],
laws::Observation::HasReason=>seq![Part::Word(11)],
laws::Observation::Reason=>seq![Part::Word(12)],
laws::Observation::PostLength=>seq![Part::Word(13)],
laws::Observation::PatchLength=>seq![Part::Word(14)],
laws::Observation::EffectLength=>seq![Part::Word(15)],
laws::Observation::OutboxLength=>seq![Part::Word(16)],
laws::Observation::PatchField(a)=>seq![Part::Word(17),Part::Word(a as u128)],
laws::Observation::PatchBefore(a)=>seq![Part::Word(18),Part::Word(a as u128)],
laws::Observation::PatchAfter(a)=>seq![Part::Word(19),Part::Word(a as u128)],
laws::Observation::EffectOrdinal(a)=>seq![Part::Word(20),Part::Word(a as u128)],
laws::Observation::EffectChannel(a)=>seq![Part::Word(21),Part::Word(a as u128)],
laws::Observation::EffectDestination(a)=>seq![Part::Word(22),Part::Word(a as u128)],
laws::Observation::EffectPayload(a,b)=>seq![Part::Word(23),Part::Word(a as u128),Part::Word(b as u128)],
laws::Observation::EffectIdempotency(a)=>seq![Part::Word(24),Part::Word(a as u128)],
laws::Observation::OutboxOrdinal(a)=>seq![Part::Word(25),Part::Word(a as u128)],
laws::Observation::OutboxChannel(a)=>seq![Part::Word(26),Part::Word(a as u128)],
laws::Observation::OutboxDestination(a)=>seq![Part::Word(27),Part::Word(a as u128)],
laws::Observation::OutboxPayload(a,b)=>seq![Part::Word(28),Part::Word(a as u128),Part::Word(b as u128)],
laws::Observation::OutboxIdempotency(a)=>seq![Part::Word(29),Part::Word(a as u128)],
laws::Observation::ReadLength=>seq![Part::Word(30)],
laws::Observation::WriteLength=>seq![Part::Word(31)],
laws::Observation::EffectAttemptLength=>seq![Part::Word(32)],
laws::Observation::ReadSource(a)=>seq![Part::Word(33),Part::Word(a as u128)],
laws::Observation::ReadId(a)=>seq![Part::Word(34),Part::Word(a as u128)],
laws::Observation::ReadPermitted(a)=>seq![Part::Word(35),Part::Word(a as u128)],
laws::Observation::WriteSource(a)=>seq![Part::Word(36),Part::Word(a as u128)],
laws::Observation::WriteId(a)=>seq![Part::Word(37),Part::Word(a as u128)],
laws::Observation::WritePermitted(a)=>seq![Part::Word(38),Part::Word(a as u128)],
laws::Observation::EffectAttemptSource(a)=>seq![Part::Word(39),Part::Word(a as u128)],
laws::Observation::EffectAttemptId(a)=>seq![Part::Word(40),Part::Word(a as u128)],
laws::Observation::EffectAttemptPermitted(a)=>seq![Part::Word(41),Part::Word(a as u128)],
laws::Observation::Usage(a)=>seq![Part::Word(42),Part::Word(resource(a))],
}}
pub open spec fn diagnostic<'a>(d:laws::Diagnostic)->Seq<Part<'a>> {
seq![Part::Word(d.id as u128)]+match d.verdict {
    laws::Verdict::Skipped=>seq![Part::Word(0)],
    laws::Verdict::Satisfied=>seq![Part::Word(1)],
    laws::Verdict::Refused(f)=>seq![Part::Word(2)]+law_failure(f),}}
pub open spec fn decision_attempt<'a>(a:Attempt)->Seq<Part<'a>> {match a {
Attempt::Candidate(p)=>seq![Part::Word(0),Part::Word(p as u128)],
Attempt::Write(id,p)=>seq![Part::Word(1),Part::Word(id as u128),Part::Word(p as u128)],
Attempt::Effect(o,id,p)=>seq![Part::Word(2),Part::Word(o as u128),Part::Word(id as u128),Part::Word(p as u128)],}}
pub open spec fn diagnostics_prefix<'a>(values:Seq<laws::Diagnostic>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0 {Seq::empty()} else {let v=values[n-1];diagnostics_prefix(values,(n-1) as nat)+diagnostic(v)}}
pub open spec fn diagnostics<'a>(values:Seq<laws::Diagnostic>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+diagnostics_prefix(values,values.len())}
pub open spec fn decision_attempts_prefix<'a>(values:Seq<Attempt>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0 {Seq::empty()} else {let v=values[n-1];decision_attempts_prefix(values,(n-1) as nat)+decision_attempt(v)}}
pub open spec fn decision_attempts<'a>(values:Seq<Attempt>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+decision_attempts_prefix(values,values.len())}
pub open spec fn law_reads_prefix<'a>(values:Seq<laws::ReadAttempt>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0 {Seq::empty()} else {let v=values[n-1];law_reads_prefix(values,(n-1) as nat)+seq![Part::Word(v.law as u128),Part::Word(v.node as u128)]+observation(v.observation)+seq![Part::Word(v.permitted as u128)]}}
pub open spec fn law_reads<'a>(values:Seq<laws::ReadAttempt>)->Seq<Part<'a>> {seq![Part::Word(values.len() as u128)]+law_reads_prefix(values,values.len())}
}
use super::framing::Kind;
verus! {
pub open spec fn identity(descriptor:Seq<u8>,evaluator:Seq<u8>)->Option<Seq<u8>> {
    let n=68+descriptor.len()+evaluator.len();
    if n<=usize::MAX {
        Some(seq![0u8]+word(0x5a494432)+seq![0u8]+word(1)+seq![1u8]+word(descriptor.len() as u128)+descriptor+seq![1u8]+word(evaluator.len() as u128)+evaluator)
    } else {None}
}
pub open spec fn subject(kind:Kind,identity:Seq<u8>,state:Seq<u8>,command:Seq<u8>,context:Seq<u8>,artifact:Seq<u8>)->Option<Seq<u8>> {
    let n=136+identity.len()+state.len()+command.len()+context.len()+artifact.len();
    if n<=usize::MAX {
        Some(seq![0u8]+word(0x5a525032)+(seq![0u8]+word(1)+(seq![0u8]+word(match kind {Kind::Genesis=>0,Kind::Transition=>1})+
            blobs(seq![identity,state,command,context,artifact]))))
    } else {None}
}
}

verus! {
pub proof fn word_injective(a:u128,b:u128)
 requires word(a)==word(b), ensures a==b,
{
    assert(((a >> 120u32) as u8)==((b >> 120u32) as u8));
    assert(((a >> 112u32) as u8)==((b >> 112u32) as u8));
    assert(((a >> 104u32) as u8)==((b >> 104u32) as u8));
    assert(((a >> 96u32) as u8)==((b >> 96u32) as u8));
    assert(((a >> 88u32) as u8)==((b >> 88u32) as u8));
    assert(((a >> 80u32) as u8)==((b >> 80u32) as u8));
    assert(((a >> 72u32) as u8)==((b >> 72u32) as u8));
    assert(((a >> 64u32) as u8)==((b >> 64u32) as u8));
    assert(((a >> 56u32) as u8)==((b >> 56u32) as u8));
    assert(((a >> 48u32) as u8)==((b >> 48u32) as u8));
    assert(((a >> 40u32) as u8)==((b >> 40u32) as u8));
    assert(((a >> 32u32) as u8)==((b >> 32u32) as u8));
    assert(((a >> 24u32) as u8)==((b >> 24u32) as u8));
    assert(((a >> 16u32) as u8)==((b >> 16u32) as u8));
    assert(((a >> 8u32) as u8)==((b >> 8u32) as u8));
    assert(((a >> 0u32) as u8)==((b >> 0u32) as u8));
    assert(a==b) by(bit_vector) requires ((a >> 120u32) as u8)==((b >> 120u32) as u8),
        ((a >> 112u32) as u8)==((b >> 112u32) as u8),
        ((a >> 104u32) as u8)==((b >> 104u32) as u8),
        ((a >> 96u32) as u8)==((b >> 96u32) as u8),
        ((a >> 88u32) as u8)==((b >> 88u32) as u8),
        ((a >> 80u32) as u8)==((b >> 80u32) as u8),
        ((a >> 72u32) as u8)==((b >> 72u32) as u8),
        ((a >> 64u32) as u8)==((b >> 64u32) as u8),
        ((a >> 56u32) as u8)==((b >> 56u32) as u8),
        ((a >> 48u32) as u8)==((b >> 48u32) as u8),
        ((a >> 40u32) as u8)==((b >> 40u32) as u8),
        ((a >> 32u32) as u8)==((b >> 32u32) as u8),
        ((a >> 24u32) as u8)==((b >> 24u32) as u8),
        ((a >> 16u32) as u8)==((b >> 16u32) as u8),
        ((a >> 8u32) as u8)==((b >> 8u32) as u8),
        ((a >> 0u32) as u8)==((b >> 0u32) as u8);
}
}
verus! {
/// Fixed words cannot be confused with a neighboring field or suffix.
pub proof fn word_prefix_cancel(a:u128,b:u128,left:Seq<u8>,right:Seq<u8>)
    requires word(a)+left==word(b)+right,
    ensures a==b,left==right,
{
    assert(word(a).len()==16 && word(b).len()==16);
    assert((word(a)+left).take(16) =~= word(a));
    assert((word(b)+right).take(16) =~= word(b));
    word_injective(a,b);
    assert((word(a)+left).skip(16) =~= left);
    assert((word(b)+right).skip(16) =~= right);
}
/// Length framing preserves empty payloads and prevents boundary shifting.
pub proof fn blob_prefix_cancel(a:Seq<u8>,b:Seq<u8>,left:Seq<u8>,right:Seq<u8>)
    requires a.len()<=u128::MAX,b.len()<=u128::MAX,
        seq![1u8]+word(a.len() as u128)+a+left==seq![1u8]+word(b.len() as u128)+b+right,
    ensures a==b,left==right,
{
    assert((seq![1u8]+word(a.len() as u128)+a+left).skip(1) =~= word(a.len() as u128)+(a+left));
    assert((seq![1u8]+word(b.len() as u128)+b+right).skip(1) =~= word(b.len() as u128)+(b+right));
    word_prefix_cancel(a.len() as u128,b.len() as u128,a+left,b+right);
    assert(a.len()==b.len());
    assert((a+left).take(a.len() as int) =~= a);
    assert((b+right).take(b.len() as int) =~= b);
    assert((a+left).skip(a.len() as int) =~= left);
    assert((b+right).skip(b.len() as int) =~= right);
}
}

verus! {
pub open spec fn part_equal(a:Part,b:Part)->bool {
    match (a,b) {(Part::Word(x),Part::Word(y))=>x==y,(Part::Bytes(x),Part::Bytes(y))=>x@==y@,_=>false}
}
pub proof fn part_prefix_cancel(a:Part,b:Part,left:Seq<u8>,right:Seq<u8>)
    requires part(a)+left==part(b)+right,
        match a {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true},
        match b {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true},
    ensures part_equal(a,b),left==right,
{
    match (a,b) {
        (Part::Word(x),Part::Word(y))=>{
            assert((part(a)+left).skip(1) =~= word(x)+left);
            assert((part(b)+right).skip(1) =~= word(y)+right);
            word_prefix_cancel(x,y,left,right);
        },
        (Part::Bytes(x),Part::Bytes(y))=>blob_prefix_cancel(x@,y@,left,right),
        (Part::Word(x),Part::Bytes(y))=>{
            assert((part(a)+left)[0]==0u8);
            assert((part(b)+right)[0]==1u8);
        },
        (Part::Bytes(x),Part::Word(y))=>{
            assert((part(a)+left)[0]==1u8);
            assert((part(b)+right)[0]==0u8);
        },
    }
}
pub proof fn prefix_length(parts:Seq<Part>,n:nat)
    requires n<=parts.len(),
    ensures prefix(parts,n).len()==length(parts,n),length(parts,n)>=17*n,
    decreases n,
{if n>0 {prefix_length(parts,(n-1) as nat);}}
pub proof fn prefix_front(parts:Seq<Part>,n:nat)
    requires 0<n<=parts.len(),
    ensures prefix(parts,n)==part(parts[0])+prefix(parts.skip(1),(n-1) as nat),
    decreases n,
{
    reveal_with_fuel(prefix,2);
    if n>1 {
        prefix_front(parts,(n-1) as nat);
        assert(parts.skip(1)[n-2]==parts[n-1]);
        assert((part(parts[0])+prefix(parts.skip(1),(n-2) as nat))+part(parts[n-1]) =~=
            part(parts[0])+(prefix(parts.skip(1),(n-2) as nat)+part(parts[n-1])));
    } else {
        assert(Seq::<u8>::empty()+part(parts[0]) =~= part(parts[0])+Seq::<u8>::empty());
    }
}
pub proof fn encoding_injective(a:Seq<Part>,b:Seq<Part>)
    requires prefix(a,a.len())==prefix(b,b.len()),
        forall|i:int| 0<=i<a.len() ==> match #[trigger] a[i] {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true},
        forall|i:int| 0<=i<b.len() ==> match #[trigger] b[i] {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true},
    ensures a.len()==b.len(),forall|i:int| 0<=i<a.len() ==> part_equal(a[i],b[i]),
    decreases a.len()+b.len(),
{
    prefix_length(a,a.len());prefix_length(b,b.len());
    if a.len()>0 && b.len()>0 {
        prefix_front(a,a.len());prefix_front(b,b.len());
        part_prefix_cancel(a[0],b[0],prefix(a.skip(1),(a.len()-1) as nat),prefix(b.skip(1),(b.len()-1) as nat));
        assert forall|i:int| 0<=i<a.skip(1).len() implies match #[trigger] a.skip(1)[i] {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true} by {assert(a.skip(1)[i]==a[i+1]);}
        assert forall|i:int| 0<=i<b.skip(1).len() implies match #[trigger] b.skip(1)[i] {Part::Bytes(x)=>x@.len()<=u128::MAX,_=>true} by {assert(b.skip(1)[i]==b[i+1]);}
        encoding_injective(a.skip(1),b.skip(1));
        assert forall|i:int| 0<=i<a.len() implies part_equal(a[i],b[i]) by {
            if i>0 {assert(a.skip(1)[i-1]==a[i]);assert(b.skip(1)[i-1]==b[i]);}
        }
    }
}
}

verus! {
pub open spec fn blob(bytes:Seq<u8>)->Seq<u8> {
    seq![1u8]+word(bytes.len() as u128)+bytes
}
pub proof fn tagged_word_prefix_cancel(a:u128,b:u128,left:Seq<u8>,right:Seq<u8>)
    requires seq![0u8]+word(a)+left==seq![0u8]+word(b)+right,
    ensures a==b,left==right,
{
    assert((seq![0u8]+word(a)+left).skip(1) =~= word(a)+left);
    assert((seq![0u8]+word(b)+right).skip(1) =~= word(b)+right);
    word_prefix_cancel(a,b,left,right);
}
/// Exact identity equality fixes both components, without a hash assumption.
pub proof fn identity_injective(d1:Seq<u8>,e1:Seq<u8>,d2:Seq<u8>,e2:Seq<u8>)
    requires identity(d1,e1).is_some(),identity(d1,e1)==identity(d2,e2),
    ensures d1==d2,e1==e2,
{
    hide(word);
    let tail1=blob(d1)+blob(e1);
    let tail2=blob(d2)+blob(e2);
    assert(identity(d2,e2).is_some());
    assert(identity(d1,e1).unwrap() =~= seq![0u8]+word(0x5a494432)+(seq![0u8]+word(1)+tail1));
    assert(identity(d2,e2).unwrap() =~= seq![0u8]+word(0x5a494432)+(seq![0u8]+word(1)+tail2));
    tagged_word_prefix_cancel(0x5a494432,0x5a494432,
        seq![0u8]+word(1)+tail1,seq![0u8]+word(1)+tail2);
    tagged_word_prefix_cancel(1,1,tail1,tail2);
    blob_prefix_cancel(d1,d2,blob(e1),blob(e2));
    blob_prefix_cancel(e1,e2,Seq::empty(),Seq::empty());
}
/// Prefix-framed byte lists are injective, including empty fields.
pub open spec fn blobs(values:Seq<Seq<u8>>)->Seq<u8>
    decreases values.len(),
{
    if values.len()==0 {Seq::empty()} else {blob(values[0])+blobs(values.skip(1))}
}
pub proof fn blobs_length(values:Seq<Seq<u8>>)
    ensures blobs(values).len()>=17*values.len(),
    decreases values.len(),
{
    if values.len()>0 {blobs_length(values.skip(1));}
}
pub proof fn blobs_injective(left:Seq<Seq<u8>>,right:Seq<Seq<u8>>)
    requires blobs(left)==blobs(right),
        forall|i:int| 0<=i<left.len() ==> (#[trigger] left[i]).len()<=u128::MAX,
        forall|i:int| 0<=i<right.len() ==> (#[trigger] right[i]).len()<=u128::MAX,
    ensures left==right,
    decreases left.len()+right.len(),
{
    hide(word);
    blobs_length(left);blobs_length(right);
    if left.len()>0 && right.len()>0 {
        blob_prefix_cancel(left[0],right[0],blobs(left.skip(1)),blobs(right.skip(1)));
        assert forall|i:int| 0<=i<left.skip(1).len() implies (#[trigger] left.skip(1)[i]).len()<=u128::MAX by {assert(left.skip(1)[i]==left[i+1]);}
        assert forall|i:int| 0<=i<right.skip(1).len() implies (#[trigger] right.skip(1)[i]).len()<=u128::MAX by {assert(right.skip(1)[i]==right[i+1]);}
        blobs_injective(left.skip(1),right.skip(1));
        assert(left.len()==left.skip(1).len()+1);
        assert(right.len()==right.skip(1).len()+1);
        assert forall|j:int| 0<=j<left.len() implies left[j]==right[j] by {
            if j>0 {assert(left.skip(1)[j-1]==left[j]);assert(right.skip(1)[j-1]==right[j]);}
        }
        assert(left =~= right);
    }
}
/// Every canonical subject component is fixed by successful full equality.
#[verifier::rlimit(20)]
pub proof fn subject_injective(k1:Kind,i1:Seq<u8>,s1:Seq<u8>,c1:Seq<u8>,x1:Seq<u8>,a1:Seq<u8>,
    k2:Kind,i2:Seq<u8>,s2:Seq<u8>,c2:Seq<u8>,x2:Seq<u8>,a2:Seq<u8>)
    requires subject(k1,i1,s1,c1,x1,a1).is_some(),
        subject(k1,i1,s1,c1,x1,a1)==subject(k2,i2,s2,c2,x2,a2),
    ensures k1==k2,i1==i2,s1==s2,c1==c2,x1==x2,a1==a2,
{
    hide(word);
    let left=seq![i1,s1,c1,x1,a1];
    let right=seq![i2,s2,c2,x2,a2];
    let tail1=blobs(left);
    let tail2=blobs(right);
    let n1=match k1 {Kind::Genesis=>0u128,Kind::Transition=>1u128};
    let n2=match k2 {Kind::Genesis=>0u128,Kind::Transition=>1u128};
    assert(subject(k2,i2,s2,c2,x2,a2).is_some());
    assert(subject(k1,i1,s1,c1,x1,a1).unwrap() =~= seq![0u8]+word(0x5a525032)+(seq![0u8]+word(1)+(seq![0u8]+word(n1)+tail1)));
    assert(subject(k2,i2,s2,c2,x2,a2).unwrap() =~= seq![0u8]+word(0x5a525032)+(seq![0u8]+word(1)+(seq![0u8]+word(n2)+tail2)));
    tagged_word_prefix_cancel(0x5a525032,0x5a525032,
        seq![0u8]+word(1)+(seq![0u8]+word(n1)+tail1),
        seq![0u8]+word(1)+(seq![0u8]+word(n2)+tail2));
    tagged_word_prefix_cancel(1,1,seq![0u8]+word(n1)+tail1,seq![0u8]+word(n2)+tail2);
    tagged_word_prefix_cancel(n1,n2,tail1,tail2);
    match (k1,k2) { (Kind::Genesis,Kind::Transition)=>{},(Kind::Transition,Kind::Genesis)=>{},_=>{} }
    assert forall|j:int| 0<=j<left.len() implies (#[trigger] left[j]).len()<=u128::MAX by {}
    assert forall|j:int| 0<=j<right.len() implies (#[trigger] right[j]).len()<=u128::MAX by {}
    blobs_injective(left,right);
}

}
