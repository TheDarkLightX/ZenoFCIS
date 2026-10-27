//! Operation-by-operation mathematical correspondence, including checked casts.
use crate::arithmetic_spec::*;
use crate::evaluation::typed::{self, BooleanEncoding, Instruction};
use crate::evaluation::{Failure, Op};
use crate::value::{self, ArithmeticOp, Atom, IntegerWidth, NumericFailure};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn arithmetic_result(v:Option<i128>)->Result<i128,Failure> {
    match v {Some(n)=>Ok(n),None=>Err(Failure::Arithmetic)}
}

/// Complete A1 legacy node: integers, explicit references and exact failure kinds.
pub(crate) open spec fn abstract_node(op:Op,input:Seq<i128>,v:Seq<i128>)->Result<i128,Failure> {
    match op {
        Op::Input(i)=>if i<input.len(){Ok(input[i as int])}else{Err(Failure::Reference)},
        Op::Int(n)=>Ok(n),Op::Bool(b)=>Ok(if b{1i128}else{0i128}),
        Op::Not(i)=>if i<v.len(){Ok(if v[i as int]==0{1i128}else{0i128})}else{Err(Failure::Reference)},
        Op::Select(c,a,b)=>if c<v.len() && a<v.len() && b<v.len(){
            Ok(if v[c as int]==1{v[a as int]}else{v[b as int]})
        }else{Err(Failure::Reference)},
        Op::Add(a,b)|Op::Sub(a,b)|Op::Mul(a,b)|Op::DivFloor(a,b)|Op::DivCeil(a,b)
            |Op::Eq(a,b)|Op::Lt(a,b)|Op::And(a,b)=>{
            if a>=v.len() || b>=v.len(){Err(Failure::Reference)}else{
                let x=v[a as int];let y=v[b as int];
                match op {
                    Op::Add(..)=>arithmetic_result(checked_signed(x as int+y as int)),
                    Op::Sub(..)=>arithmetic_result(checked_signed(x as int-y as int)),
                    Op::Mul(..)=>arithmetic_result(checked_signed(x as int*y as int)),
                    Op::DivFloor(..)|Op::DivCeil(..)=>arithmetic_result(
                        signed_division(matches!(op,Op::DivCeil(..)),x as int,y as int)),
                    Op::Eq(..)=>Ok(if x==y{1i128}else{0i128}),
                    Op::Lt(..)=>Ok(if x<y{1i128}else{0i128}),
                    _=>Ok(if x==1 && y==1{1i128}else{0i128}),
                }
            }
        },
        Op::MulDivFloor(a,b,d)|Op::MulDivCeil(a,b,d)|Op::SignedMulDivFloor(a,b,d)=>{
            if a>=v.len() || b>=v.len() || d>=v.len(){Err(Failure::Reference)}else{
                arithmetic_result(if matches!(op,Op::SignedMulDivFloor(..)){
                    signed_full_fused(v[a as int] as int,v[b as int] as int,v[d as int] as int)
                }else{signed_fused(matches!(op,Op::MulDivCeil(..)),
                    v[a as int] as int,v[b as int] as int,v[d as int] as int)})
            }
        },
        Op::ProductsGe(a,b)=>{
            if a[0]>=v.len() || a[1]>=v.len() || a[2]>=v.len() ||
                b[0]>=v.len() || b[1]>=v.len() || b[2]>=v.len(){Err(Failure::Reference)}
            else if v[a[0] as int]<0 || v[a[1] as int]<0 || v[a[2] as int]<0 ||
                v[b[0] as int]<0 || v[b[1] as int]<0 || v[b[2] as int]<0 {Err(Failure::Arithmetic)}
            else {Ok(if v[a[0] as int] as int*v[a[1] as int] as int*v[a[2] as int] as int >=
                v[b[0] as int] as int*v[b[1] as int] as int*v[b[2] as int] as int{1i128}else{0i128})}
        },
    }
}

pub(crate) proof fn legacy_node_correspondence(op:Op,input:Seq<i128>,v:Seq<i128>)
    ensures crate::evaluation::spec::node(op,input,v)==abstract_node(op,input,v),
{
    match op {
        Op::DivFloor(a,b)|Op::DivCeil(a,b)=>{
            if a<v.len() && b<v.len(){
                signed_division_correspondence(matches!(op,Op::DivCeil(..)),v[a as int],v[b as int]);
            }
        },
        Op::MulDivFloor(a,b,d)|Op::MulDivCeil(a,b,d)|Op::SignedMulDivFloor(a,b,d)=>{
            if a<v.len() && b<v.len() && d<v.len(){
                signed_fused_correspondence(matches!(op,Op::MulDivCeil(..)),v[a as int],v[b as int],v[d as int]);
            }
        },
        _=>{},
    }
}

pub(crate) open spec fn unsigned_result(v:Option<u128>)->Result<Atom<'static>,typed::Failure>{
    match v{Some(n)=>Ok(Atom::U128(n)),None=>Err(typed::Failure::Arithmetic)}
}
pub(crate) open spec fn signed_result(v:Option<i128>)->Result<Atom<'static>,typed::Failure>{
    match v{Some(n)=>Ok(Atom::I128(n)),None=>Err(typed::Failure::Arithmetic)}
}

/// Casts preserve the exact integer, and refuse precisely the projection range.
pub(crate) proof fn cast_correspondence(u:u128,i:i128)
    ensures
        typed::spec::node(Instruction::Cast(true,0),Seq::empty(),seq![Atom::U128(u)])
            ==signed_result(checked_signed(u as int)),
        typed::spec::node(Instruction::Cast(false,0),Seq::empty(),seq![Atom::I128(i)])
            ==unsigned_result(checked_unsigned(i as int)),
{}

pub(crate) proof fn unsigned_binary_correspondence(op:ArithmeticOp,a:u128,b:u128)
    ensures value::spec::arithmetic(IntegerWidth::Unsigned128,op,Atom::U128(a),Atom::U128(b))==
        match checked_unsigned(match op{ArithmeticOp::Add=>a as int+b as int,ArithmeticOp::Sub=>a as int-b as int}){
            Some(n)=>Ok(Atom::U128(n)),None=>Err(NumericFailure::Range),
        },
{}

pub(crate) proof fn unsigned_nodes_correspondence(a:u128,b:u128,d:u128,up:bool)
    ensures
        typed::spec::node(Instruction::ScalarMul(IntegerWidth::Unsigned128,0,1),Seq::empty(),seq![Atom::U128(a),Atom::U128(b)])
            ==unsigned_result(checked_unsigned(a as int*b as int)),
        typed::spec::node(Instruction::ScalarDiv(IntegerWidth::Unsigned128,up,0,1),Seq::empty(),seq![Atom::U128(a),Atom::U128(b)])
            ==unsigned_result(if b==0{None}else{checked_unsigned(rounded(up,a as int,b as int))}),
        typed::spec::node(Instruction::MulDiv(up,0,1,2),Seq::empty(),seq![Atom::U128(a),Atom::U128(b),Atom::U128(d)])
            ==unsigned_result(unsigned_fused(up,a as int,b as int,d as int)),
{
    unsigned_correspondence(a,b,d);
    if b>0{rounding_interval(a as int,b as int);}
}

pub(crate) open spec fn width_fits(w:IntegerWidth,n:int)->bool{
    match w{IntegerWidth::Signed64=>i64::MIN<=n<=i64::MAX,
        IntegerWidth::Signed128=>signed_fit(n),IntegerWidth::Unsigned128=>false}
}
pub(crate) open spec fn width_result(w:IntegerWidth,v:Option<i128>)->Result<Atom<'static>,typed::Failure>{
    match v{Some(n)=>if width_fits(w,n as int){Ok(Atom::I128(n))}else{Err(typed::Failure::Arithmetic)},
        None=>Err(typed::Failure::Arithmetic)}
}
pub(crate) proof fn signed_width_nodes_correspondence(w:IntegerWidth,a:i128,b:i128,d:i128,up:bool)
    ensures
        typed::spec::node(Instruction::ScalarMul(w,0,1),Seq::empty(),seq![Atom::I128(a),Atom::I128(b)])==
            if width_fits(w,a as int) && width_fits(w,b as int){width_result(w,checked_signed(a as int*b as int))}
                else{Err(typed::Failure::Arithmetic)},
        typed::spec::node(Instruction::ScalarDiv(w,up,0,1),Seq::empty(),seq![Atom::I128(a),Atom::I128(b)])==
            if width_fits(w,a as int) && width_fits(w,b as int){width_result(w,signed_division(up,a as int,b as int))}
                else{Err(typed::Failure::Arithmetic)},
        typed::spec::node(Instruction::ScalarMulDiv(w,up,0,1,2),Seq::empty(),seq![Atom::I128(a),Atom::I128(b),Atom::I128(d)])==
            if width_fits(w,a as int) && width_fits(w,b as int) && width_fits(w,d as int){
                width_result(w,signed_fused(up,a as int,b as int,d as int))}else{Err(typed::Failure::Arithmetic)},
{
    signed_division_correspondence(up,a,b);
    signed_fused_correspondence(up,a,b,d);
}

pub(crate) proof fn signed_binary_correspondence(w:IntegerWidth,op:ArithmeticOp,a:i128,b:i128)
    ensures value::spec::arithmetic(w,op,Atom::I128(a),Atom::I128(b))==
        if w==IntegerWidth::Unsigned128 {Err(NumericFailure::Kind)}else{
            let n=match op{ArithmeticOp::Add=>a as int+b as int,ArithmeticOp::Sub=>a as int-b as int};
            if width_fits(w,a as int) && width_fits(w,b as int) && width_fits(w,n){Ok(Atom::I128(n as i128))}
            else{Err(NumericFailure::Range)}
        },
{}

/// Concrete simultaneous premises and range refusal cases, at the real node spec.
pub(crate) proof fn typed_witnesses()
    ensures
        typed::spec::node(Instruction::Cast(true,0),Seq::empty(),seq![Atom::U128(7u128)])==Ok(Atom::I128(7i128)),
        typed::spec::node(Instruction::Cast(true,0),Seq::empty(),seq![Atom::U128(u128::MAX)])==Err(typed::Failure::Arithmetic),
        typed::spec::node(Instruction::Cast(false,0),Seq::empty(),seq![Atom::I128(-1i128)])==Err(typed::Failure::Arithmetic),
        typed::spec::node(Instruction::Cast(false,0),Seq::empty(),seq![Atom::I128(7i128)])==Ok(Atom::U128(7u128)),
        typed::spec::node(Instruction::Cast(true,0),Seq::empty(),Seq::empty())==Err(typed::Failure::Reference),
        typed::spec::node(Instruction::Cast(true,0),Seq::empty(),seq![Atom::Bool(false)])==Err(typed::Failure::Kind),
{}

pub(crate) proof fn legacy_witnesses()
    ensures
        abstract_node(Op::Input(0),seq![4i128],Seq::empty())==Ok(4i128),
        abstract_node(Op::Int(4),Seq::empty(),Seq::empty())==Ok(4i128),
        abstract_node(Op::Bool(true),Seq::empty(),Seq::empty())==Ok(1i128),
        abstract_node(Op::Add(0,1),Seq::empty(),seq![1i128,2i128])==Ok(3i128),
        abstract_node(Op::Sub(0,1),Seq::empty(),seq![1i128,2i128])==Ok(-1i128),
        abstract_node(Op::Mul(0,1),Seq::empty(),seq![2i128,3i128])==Ok(6i128),
        abstract_node(Op::Eq(0,1),Seq::empty(),seq![1i128,1i128])==Ok(1i128),
        abstract_node(Op::Lt(0,1),Seq::empty(),seq![1i128,2i128])==Ok(1i128),
        abstract_node(Op::And(0,1),Seq::empty(),seq![1i128,1i128])==Ok(1i128),
        abstract_node(Op::Not(0),Seq::empty(),seq![0i128])==Ok(1i128),
        abstract_node(Op::Select(0,1,2),Seq::empty(),seq![1i128,2i128,3i128])==Ok(2i128),
        abstract_node(Op::Select(0,1,2),Seq::empty(),seq![0i128,2i128,3i128])==Ok(3i128),
        abstract_node(Op::Add(0,1),Seq::empty(),seq![i128::MAX,1i128])==Err(Failure::Arithmetic),
        abstract_node(Op::Sub(0,1),Seq::empty(),seq![i128::MIN,1i128])==Err(Failure::Arithmetic),
        abstract_node(Op::Input(0),Seq::empty(),Seq::empty())==Err(Failure::Reference),
        abstract_node(Op::Select(0,1,2),Seq::empty(),seq![1i128,2i128])==Err(Failure::Reference),
{}

}
