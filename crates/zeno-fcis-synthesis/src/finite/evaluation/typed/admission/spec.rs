//! Independent static kinds; literal provenance is not inferred from legacy atoms.
use super::*;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn domain_kind(d:Domain)->Option<Kind> {
 match d {Domain::Bool=>Some(Kind::Bool),Domain::I128{min,max}=>if min<=max{Some(Kind::Signed)}else{None},
 Domain::U128{min,max}=>if min<=max{Some(Kind::Unsigned)}else{None},_=>None}
}
pub(crate) open spec fn get(k:Seq<Kind>,id:u16)->Result<Kind,AdmissionFailure> {
 if id<k.len(){Ok(k[id as int])}else{Err(AdmissionFailure::NodeReference)}
}
pub(crate) open spec fn require_kind(k:Seq<Kind>,id:u16,want:Kind)->Result<(),AdmissionFailure> {
 match get(k,id){Err(e)=>Err(e),Ok(actual)=>if actual==want{Ok(())}else{Err(AdmissionFailure::TypeMismatch)}}
}
pub(crate) open spec fn pair(k:Seq<Kind>,a:u16,b:u16,want:Kind,result:Kind)->Result<Kind,AdmissionFailure> {
 match require_kind(k,a,want){Err(e)=>Err(e),Ok(())=>match require_kind(k,b,want){Err(e)=>Err(e),Ok(())=>Ok(result)}}
}
pub(crate) open spec fn signed_triple(k:Seq<Kind>,a:u16,b:u16,d:u16)->Result<Kind,AdmissionFailure> {
 match require_kind(k,a,Kind::Signed){Err(e)=>Err(e),Ok(())=>match require_kind(k,b,Kind::Signed){Err(e)=>Err(e),Ok(())=>match require_kind(k,d,Kind::Signed){Err(e)=>Err(e),Ok(())=>Ok(Kind::Signed)}}}
}
pub(crate) open spec fn kind(op:NumericOp,inputs:Seq<Domain>,previous:Seq<Kind>)->Result<Kind,AdmissionFailure> {
 match op {
 NumericOp::Input(i)=>if i<inputs.len(){match domain_kind(inputs[i as int]){Some(k)=>Ok(k),None=>Err(AdmissionFailure::TypeMismatch)}}else{Err(AdmissionFailure::InputReference)},
 NumericOp::Bool(_)=>Ok(Kind::Bool),NumericOp::I128(_)=>Ok(Kind::Signed),NumericOp::U128(_)=>Ok(Kind::Unsigned),
 NumericOp::Add(w,a,b)|NumericOp::Sub(w,a,b)=>{
 let k=if matches!(w,IntegerWidth::Unsigned128){Kind::Unsigned}else{Kind::Signed};pair(previous,a,b,k,k)
 },
 NumericOp::Mul(w,a,b)|NumericOp::DivFloor(w,a,b)|NumericOp::DivCeil(w,a,b)=>{
     let k=if w==IntegerWidth::Unsigned128{Kind::Unsigned}else{Kind::Signed};pair(previous,a,b,k,k)
 },
 NumericOp::ToI128(a)=>match require_kind(previous,a,Kind::Unsigned){Err(e)=>Err(e),Ok(())=>Ok(Kind::Signed)},
 NumericOp::ToU128(a)=>match require_kind(previous,a,Kind::Signed){Err(e)=>Err(e),Ok(())=>Ok(Kind::Unsigned)},
 NumericOp::SignedMulDivFloor(a,b,d)=>signed_triple(previous,a,b,d),
 NumericOp::NonnegativeMulDivFloor(w,a,b,d)|NumericOp::NonnegativeMulDivCeil(w,a,b,d)=>{
     if w==IntegerWidth::Unsigned128 {Err(AdmissionFailure::TypeMismatch)}else{signed_triple(previous,a,b,d)}
 },
 NumericOp::NonnegativeProductsGe(a,b)=>{
     match signed_triple(previous,a[0],a[1],a[2]){Err(e)=>Err(e),Ok(_)=>match signed_triple(previous,b[0],b[1],b[2]){Err(e)=>Err(e),Ok(_)=>Ok(Kind::Bool)}}
 },
 NumericOp::And(a,b)=>pair(previous,a,b,Kind::Bool,Kind::Bool),
 NumericOp::MulDivFloor(a,b,den)|NumericOp::MulDivCeil(a,b,den)=>{
     match require_kind(previous,a,Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,b,Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,den,Kind::Unsigned){Err(e)=>Err(e),Ok(())=>Ok(Kind::Unsigned)}}}
 },
 NumericOp::ProductsGe(left,right)=>{
     match require_kind(previous,left[0],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,left[1],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,left[2],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,right[0],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,right[1],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>
     match require_kind(previous,right[2],Kind::Unsigned){Err(e)=>Err(e),Ok(())=>Ok(Kind::Bool)}}}}}}
 },
 NumericOp::Not(a)=>match require_kind(previous,a,Kind::Bool){Ok(())=>Ok(Kind::Bool),Err(e)=>Err(e)},
 NumericOp::Eq(a,b)|NumericOp::Lt(a,b)=>match get(previous,a){Err(e)=>Err(e),Ok(a)=>match get(previous,b){Err(e)=>Err(e),Ok(b)=>{
 if a==b && (!matches!(op,NumericOp::Lt(..))||a!=Kind::Bool){Ok(Kind::Bool)}else{Err(AdmissionFailure::TypeMismatch)}
 }}},
 NumericOp::Select(c,a,b)=>match require_kind(previous,c,Kind::Bool) {
     Err(e)=>Err(e),
     Ok(())=>match get(previous,a) {
         Err(e)=>Err(e),
         Ok(a)=>match get(previous,b) {
             Err(e)=>Err(e),
             Ok(b)=>if a==b{Ok(a)}else{Err(AdmissionFailure::TypeMismatch)},
         },
     },
 },
 }
}
pub(crate) open spec fn valid_domains(d:Seq<Domain>)->bool {
 forall|i:int|0<=i<d.len() ==> domain_kind(d[i]).is_some()
}
pub(crate) open spec fn counts(inputs:nat,outputs:nat,nodes:nat,roots:nat)->bool {
  inputs<=MAX_NUMERIC_INPUTS && 0<outputs<=MAX_NUMERIC_OUTPUTS && 0<nodes<=MAX_NUMERIC_NODES && outputs==roots
}
pub(crate) open spec fn prefix(inputs:Seq<Domain>,nodes:Seq<NumericOp>,count:nat)->Result<Seq<Kind>,AdmissionFailure>
 recommends count<=nodes.len(),decreases count,
{
 if count==0{Ok(Seq::empty())}else{match prefix(inputs,nodes,(count-1) as nat){Err(e)=>Err(e),Ok(k)=>match kind(nodes[count as int-1],inputs,k){Ok(v)=>Ok(k.push(v)),Err(e)=>Err(e)}}}
}
pub(crate) proof fn failure_persists(inputs:Seq<Domain>,nodes:Seq<NumericOp>,failed:nat,count:nat,e:AdmissionFailure)
 requires failed<=count<=nodes.len(),prefix(inputs,nodes,failed)==Err(e),
 ensures prefix(inputs,nodes,count)==Err(e),decreases count-failed,
{if count>failed{failure_persists(inputs,nodes,failed,(count-1) as nat,e);}}
pub(crate) open spec fn root_valid(outputs:Seq<Domain>,roots:Seq<u16>,kinds:Seq<Kind>,i:int)->bool {
 0<=i<outputs.len() && i<roots.len() && roots[i]<kinds.len() && Some(kinds[roots[i] as int])==domain_kind(outputs[i])
}
pub(crate) open spec fn program(inputs:Seq<Domain>,outputs:Seq<Domain>,nodes:Seq<NumericOp>,roots:Seq<u16>)->Result<(),AdmissionFailure> {
 if !counts(inputs.len(),outputs.len(),nodes.len(),roots.len())||!valid_domains(inputs)||!valid_domains(outputs){Err(AdmissionFailure::Shape)}else{
 match prefix(inputs,nodes,nodes.len()){Err(e)=>Err(e),Ok(k)=>if forall|i:int|0<=i<roots.len() ==> root_valid(outputs,roots,k,i){Ok(())}else{Err(AdmissionFailure::OutputType)}}
 }
}
}
