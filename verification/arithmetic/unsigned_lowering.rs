//! Mathematical erasure obligations for the full-width unsigned compiler path.
//! This subject is pending verification together with tables at the 2.5.1 freeze.
use crate::evaluation::typed::{self, Failure, Instruction};
use crate::value::{ArithmeticOp, Atom, IntegerWidth};
use vstd::prelude::*;
verus! {
pub open spec fn checked_unsigned(value:int)->Result<Atom<'static>,Failure>{
    if 0<=value<=u128::MAX {Ok(Atom::U128(value as u128))}else{Err(Failure::Arithmetic)}
}
pub proof fn add_sub_erasure(a:u128,b:u128)
    ensures typed::spec::node(Instruction::Arithmetic(IntegerWidth::Unsigned128,ArithmeticOp::Add,0,1),Seq::empty(),seq![Atom::U128(a),Atom::U128(b)])==checked_unsigned(a as int+b as int),
            typed::spec::node(Instruction::Arithmetic(IntegerWidth::Unsigned128,ArithmeticOp::Sub,0,1),Seq::empty(),seq![Atom::U128(a),Atom::U128(b)])==checked_unsigned(a as int-b as int),
{}
pub proof fn conversion_partitions(u:u128,i:i128)
    ensures typed::spec::node(Instruction::Cast(true,0),Seq::empty(),seq![Atom::U128(u)])==if u<=i128::MAX as u128{Ok(Atom::I128(u as i128))}else{Err(Failure::Arithmetic)},
            typed::spec::node(Instruction::Cast(false,0),Seq::empty(),seq![Atom::I128(i)])==if i>=0{Ok(Atom::U128(i as u128))}else{Err(Failure::Arithmetic)},
{}
pub proof fn scaled_product_identity(a:int,b:int,d:int,factor:int)
    requires a>=0,b>=0,d>0,factor>0,
    ensures (a*b*factor)/d==((a*factor)*b)/d,
{
    assert(a*b*factor==(a*factor)*b) by(nonlinear_arith);
}
pub proof fn scale_fit_partition(lo:u128,hi:u128,factor:u128,x:u128)
    requires lo<=x<=hi,factor>0,hi as int*factor as int<=u128::MAX,
    ensures 0<=x as int*factor as int<=u128::MAX,
{
    assert(x as int*factor as int<=hi as int*factor as int) by(nonlinear_arith);
}
pub proof fn fused_floor_erasure(a:u128,b:u128,d:u128)
    ensures typed::spec::node(Instruction::MulDiv(false,0,1,2),Seq::empty(),seq![Atom::U128(a),Atom::U128(b),Atom::U128(d)])==if d==0{Err(Failure::Arithmetic)}else{checked_unsigned((a as int*b as int)/d as int)},
{}
pub proof fn products_compare_erasure(a:u128,b:u128,c:u128,d:u128,e:u128,f:u128)
    ensures typed::spec::node(Instruction::ProductsGe([0,1,2],[3,4,5]),Seq::empty(),seq![Atom::U128(a),Atom::U128(b),Atom::U128(c),Atom::U128(d),Atom::U128(e),Atom::U128(f)])==Ok(Atom::Bool(a as int*b as int*c as int>=d as int*e as int*f as int)),
{}
}
