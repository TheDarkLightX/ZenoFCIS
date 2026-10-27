//! Independent exact typed policy token sequence; legacy policy remains version one.
use super::super::super::{
    evaluation::typed::NumericOp,
    value::{Domain, IntegerWidth},
};
use super::super::{
    composition::{NumericDescriptor, Projection, Schema},
    input_view::Field,
    numeric::NumericProgram,
};
use super::{canonical::Part, descriptor_spec as descriptor, metadata_spec as common};
use vstd::prelude::*;
verus! {
pub(super) open spec fn width(w:IntegerWidth)->u128 {match w{IntegerWidth::Signed64=>0,IntegerWidth::Signed128=>1,IntegerWidth::Unsigned128=>2}}
pub(super) open spec fn op<'a>(op:NumericOp)->Seq<Part<'a>> {
    match op {
        NumericOp::ToI128(i)=>seq![Part::Word(21),Part::Word(i as u128)],
        NumericOp::ToU128(i)=>seq![Part::Word(22),Part::Word(i as u128)],
        NumericOp::Input(i)=>seq![Part::Word(0),Part::Word(i as u128)],
        NumericOp::Bool(v)=>seq![Part::Word(1),Part::Word(if v{1}else{0})],
        NumericOp::I128(v)=>seq![Part::Word(2),Part::Word(v as u128)],
        NumericOp::U128(v)=>seq![Part::Word(3),Part::Word(v)],
        NumericOp::Add(w,a,b)=>seq![Part::Word(4),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::Sub(w,a,b)=>seq![Part::Word(5),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::Eq(a,b)=>seq![Part::Word(6),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::Lt(a,b)=>seq![Part::Word(7),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::And(a,b)=>seq![Part::Word(8),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::Not(a)=>seq![Part::Word(9),Part::Word(a as u128)],
        NumericOp::Select(c,a,b)=>seq![Part::Word(10),Part::Word(c as u128),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::MulDivFloor(a,b,den)=>seq![Part::Word(11),Part::Word(a as u128),Part::Word(b as u128),Part::Word(den as u128)],
        NumericOp::MulDivCeil(a,b,den)=>seq![Part::Word(12),Part::Word(a as u128),Part::Word(b as u128),Part::Word(den as u128)],
        NumericOp::SignedMulDivFloor(a,b,d)=>seq![Part::Word(14),Part::Word(a as u128),Part::Word(b as u128),Part::Word(d as u128)],
        NumericOp::Mul(w,a,b)=>seq![Part::Word(15),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::DivFloor(w,a,b)=>seq![Part::Word(16),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::DivCeil(w,a,b)=>seq![Part::Word(17),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128)],
        NumericOp::NonnegativeMulDivFloor(w,a,b,d)=>seq![Part::Word(18),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128),Part::Word(d as u128)],
        NumericOp::NonnegativeMulDivCeil(w,a,b,d)=>seq![Part::Word(19),Part::Word(width(w)),Part::Word(a as u128),Part::Word(b as u128),Part::Word(d as u128)],
        NumericOp::NonnegativeProductsGe(a,b)=>seq![Part::Word(20),Part::Word(a[0] as u128),Part::Word(a[1] as u128),Part::Word(a[2] as u128),Part::Word(b[0] as u128),Part::Word(b[1] as u128),Part::Word(b[2] as u128)],
        NumericOp::ProductsGe(left,right)=>seq![Part::Word(13),Part::Word(left[0] as u128),Part::Word(left[1] as u128),Part::Word(left[2] as u128),Part::Word(right[0] as u128),Part::Word(right[1] as u128),Part::Word(right[2] as u128)],
    }
}
pub(super) open spec fn domains_prefix<'a>(ds:Seq<Domain<'a>>,n:nat)->Seq<Part<'a>> recommends n<=ds.len(),decreases n {
    if n==0{Seq::empty()}else{domains_prefix(ds,(n-1) as nat)+common::domain(ds[n as int-1])}
}
pub(super) open spec fn domains<'a>(ds:Seq<Domain<'a>>)->Seq<Part<'a>>{seq![Part::Word(ds.len() as u128)]+domains_prefix(ds,ds.len())}
pub open spec fn input_prefix<'a>(fs:Seq<Field<Domain<'a>>>,n:nat)->Seq<Part<'a>> recommends n<=fs.len(),decreases n {
    if n==0{Seq::empty()}else{input_prefix(fs,(n-1) as nat)+seq![Part::Word(fs[n as int-1].id as u128)]+common::domain(fs[n as int-1].leaf)}
}
pub open spec fn input_fields<'a>(fs:Seq<Field<Domain<'a>>>)->Seq<Part<'a>>{seq![Part::Word(fs.len() as u128)]+input_prefix(fs,fs.len())}
pub open spec fn original<'a>(c:&crate::finite::canonical_v2::schema::Checked<'a>,root:u32,p:Projection<'a>)->Seq<Part<'a>> {
    seq![Part::Word(2),Part::Bytes(crate::finite::execution_v2::compound::spec::slice(c.view().0)),Part::Word(root as u128)]+
    match p{Projection::None=>seq![Part::Word(0)],Projection::Root(d)=>seq![Part::Word(1)]+common::domain(*d),Projection::Fields(f)=>seq![Part::Word(2)]+input_fields(f@)}
}
pub(super) open spec fn schema<'a>(s:Schema<'a,Domain<'a>>)->Seq<Part<'a>>{match s{Schema::Original{checked,root,projection}=>original(checked,root,projection),Schema::Leaf(l)=>seq![Part::Word(0)]+common::domain(*l),Schema::Record(f)=>seq![Part::Word(1)]+input_fields(f@)}}
pub(super) open spec fn ops_prefix<'a>(ns:Seq<NumericOp>,n:nat)->Seq<Part<'a>> recommends n<=ns.len(),decreases n{
    if n==0{Seq::empty()}else{ops_prefix(ns,(n-1) as nat)+op(ns[n as int-1])}
}
pub(super) open spec fn program<'a>(p:NumericProgram<'a>)->Seq<Part<'a>> {
    domains(p.inputs@)+domains(p.outputs@)+seq![Part::Word(p.nodes@.len() as u128)]+ops_prefix(p.nodes@,p.nodes@.len())+common::variants_ids(p.roots@)
}
pub(super) open spec fn fields<'a>(d:&NumericDescriptor<'a>)->Seq<Part<'a>> {
    schema(d.state)+schema(d.command)+schema(d.context)+program(d.program)+descriptor::bindings(d.bindings@)+domains(d.output_types@)
    +seq![Part::Word(d.decision_output as u128)]+common::branches(d.branches@)+descriptor::reasons(d.reasons@)+descriptor::channels(d.channels@)
    +common::law_list(d.laws@)+common::ids(d.required@)+common::limits(d.limits)
}
}
