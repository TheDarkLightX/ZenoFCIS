//! Tagged mathematical node semantics and the independent old-wire embedding.
use super::super::Failure as LegacyFailure;
use super::*;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn lift<'a>(values:Seq<i64>)->Seq<Atom<'a>> {
    Seq::new(values.len(), |i:int| Atom::I128(values[i] as i128))
}
impl<'s,'a> Values<'s,'a> {
    pub(crate) open spec fn view(self)->Seq<Atom<'a>> {
        match self { Values::Legacy(v)=>lift(v@), Values::Typed(v)=>v@ }
    }
}
pub(crate) open spec fn at<'a>(values:Seq<Atom<'a>>,i:nat)->Option<Atom<'a>> {
    if i<values.len() {Some(values[i as int])} else {None}
}
pub(crate) open spec fn kind(v:Atom)->Option<Kind> {
    match v {Atom::Bool(_)=>Some(Kind::Bool),Atom::I128(_)=>Some(Kind::Signed),
        Atom::U128(_)=>Some(Kind::Unsigned),_=>None}
}
pub(crate) open spec fn boolean(e:BooleanEncoding,v:bool)->Atom<'static> {
    match e {BooleanEncoding::TypedBool=>Atom::Bool(v),
        BooleanEncoding::LegacyInteger01=>Atom::I128(if v {1} else {0})}
}
pub(crate) open spec fn project(v:Atom)->Option<i64> {
    match v {Atom::I128(n)=>if i64::MIN<=n<=i64::MAX {Some(n as i64)} else {None},_=>None}
}
pub(crate) open spec fn legacy_instruction(op:Op)->Instruction {
    let e=BooleanEncoding::LegacyInteger01;
    match op {
        Op::Input(i)=>Instruction::Input(i),
        Op::Int(v)=>Instruction::Literal(Atom::I128(v as i128),Kind::Signed),
        Op::Bool(v)=>Instruction::Literal(Atom::I128(if v {1} else {0}),Kind::Bool),
        Op::Add(a,b)=>Instruction::Arithmetic(IntegerWidth::Signed64,ArithmeticOp::Add,a,b),
        Op::Sub(a,b)=>Instruction::Arithmetic(IntegerWidth::Signed64,ArithmeticOp::Sub,a,b),
        Op::Eq(a,b)=>Instruction::Eq(e,a,b),Op::Lt(a,b)=>Instruction::Lt(e,a,b),
        Op::And(a,b)=>Instruction::And(e,a,b),Op::Not(a)=>Instruction::Not(e,a),
        Op::Select(c,a,b)=>Instruction::Select(e,c,a,b),
        Op::Mul(a,b)=>Instruction::ScalarMul(a,b),
        Op::DivFloor(a,b)=>Instruction::ScalarDiv(false,a,b),
        Op::DivCeil(a,b)=>Instruction::ScalarDiv(true,a,b),
        Op::MulDivFloor(a,b,d)=>Instruction::ScalarMulDiv(false,a,b,d),
        Op::MulDivCeil(a,b,d)=>Instruction::ScalarMulDiv(true,a,b,d),
        Op::ProductsGe(a,b)=>Instruction::ScalarProductsGe(a,b),
    }
}
pub(crate) open spec fn typed_instruction(op:NumericOp)->Instruction {
    let e=BooleanEncoding::TypedBool;
    match op {
        NumericOp::Input(i)=>Instruction::Input(i),
        NumericOp::I128(v)=>Instruction::Literal(Atom::I128(v),Kind::Signed),
        NumericOp::U128(v)=>Instruction::Literal(Atom::U128(v),Kind::Unsigned),
        NumericOp::Bool(v)=>Instruction::Literal(Atom::Bool(v),Kind::Bool),
        NumericOp::Add(w,a,b)=>Instruction::Arithmetic(w,ArithmeticOp::Add,a,b),
        NumericOp::Sub(w,a,b)=>Instruction::Arithmetic(w,ArithmeticOp::Sub,a,b),
        NumericOp::Eq(a,b)=>Instruction::Eq(e,a,b),NumericOp::Lt(a,b)=>Instruction::Lt(e,a,b),
        NumericOp::And(a,b)=>Instruction::And(e,a,b),NumericOp::Not(a)=>Instruction::Not(e,a),
        NumericOp::Select(c,a,b)=>Instruction::Select(e,c,a,b),
        NumericOp::MulDivFloor(a,b,den)=>Instruction::MulDiv(false,a,b,den),
        NumericOp::MulDivCeil(a,b,den)=>Instruction::MulDiv(true,a,b,den),
        NumericOp::ProductsGe(left,right)=>Instruction::ProductsGe(left,right),
    }
}
pub(crate) open spec fn scalar_result<'a>(value:Option<i64>)->Result<Atom<'a>,Failure> {
    match value {Some(v)=>Ok(Atom::I128(v as i128)),None=>Err(Failure::Arithmetic)}
}
pub(crate) open spec fn node<'a>(op:Instruction,input:Seq<Atom<'a>>,values:Seq<Atom<'a>>)->Result<Atom<'a>,Failure> {
    match op {
        Instruction::Input(i)=>match at(input,i as nat) {
            None=>Err(Failure::Reference),Some(v)=>if kind(v).is_some(){Ok(v)}else{Err(Failure::Kind)},
        },
        Instruction::Literal(v,_)=>Ok(v),
        Instruction::ScalarMul(a,b)|Instruction::ScalarDiv(_,a,b)=>match(at(values,a as nat),at(values,b as nat)) {
            (Some(a),Some(b))=>match(project(a),project(b)) {
                (Some(a),Some(b))=>scalar_result(match op {
                    Instruction::ScalarMul(..)=>value::scalar::multiply_value(a,b),
                    Instruction::ScalarDiv(up,_,_)=>value::scalar::division_value(up,a,b),_=>None,
                }),_=>Err(Failure::Arithmetic),
            },_=>Err(Failure::Reference),
        },
        Instruction::ScalarMulDiv(up,a,b,d)=>match(at(values,a as nat),at(values,b as nat),at(values,d as nat)) {
            (Some(a),Some(b),Some(d))=>match(project(a),project(b),project(d)) {
                (Some(a),Some(b),Some(d))=>scalar_result(value::scalar::mul_div_value(up,a,b,d)),_=>Err(Failure::Arithmetic),
            },_=>Err(Failure::Reference),
        },
        Instruction::ScalarProductsGe(a,b)=>match(at(values,a[0] as nat),at(values,a[1] as nat),at(values,a[2] as nat),at(values,b[0] as nat),at(values,b[1] as nat),at(values,b[2] as nat)) {
            (Some(a),Some(b),Some(c),Some(d),Some(e),Some(f))=>match(project(a),project(b),project(c),project(d),project(e),project(f)) {
                (Some(a),Some(b),Some(c),Some(d),Some(e),Some(f))=>match value::scalar::products_ge_value(a,b,c,d,e,f) {
                    Some(v)=>Ok(boolean(BooleanEncoding::LegacyInteger01,v)),None=>Err(Failure::Arithmetic),
                },_=>Err(Failure::Arithmetic),
            },_=>Err(Failure::Reference),
        },
        Instruction::MulDiv(up,a,b,den)=>match(at(values,a as nat),at(values,b as nat),at(values,den as nat)) {
            (Some(Atom::U128(a)),Some(Atom::U128(b)),Some(Atom::U128(den)))=>{
                let result=if up{value::wide::ceiling_value(a,b,den)}else{value::wide::floor_value(a,b,den)};
                match result{Some(v)=>Ok(Atom::U128(v)),None=>Err(Failure::Arithmetic)}
            },
            (Some(_),Some(_),Some(_))=>Err(Failure::Kind),_=>Err(Failure::Reference),
        },
        Instruction::ProductsGe(left,right)=>match(at(values,left[0] as nat),at(values,left[1] as nat),at(values,left[2] as nat),at(values,right[0] as nat),at(values,right[1] as nat),at(values,right[2] as nat)) {
            (Some(Atom::U128(a)),Some(Atom::U128(b)),Some(Atom::U128(c)),Some(Atom::U128(d)),Some(Atom::U128(e)),Some(Atom::U128(f)))=>Ok(Atom::Bool(a as int*b as int*c as int>=d as int*e as int*f as int)),
            (Some(_),Some(_),Some(_),Some(_),Some(_),Some(_))=>Err(Failure::Kind),_=>Err(Failure::Reference),
        },
        Instruction::Not(e,a)=>match at(values,a as nat) {
            None=>Err(Failure::Reference),Some(v)=>match (e,v) {
                (BooleanEncoding::LegacyInteger01,Atom::I128(x))=>Ok(boolean(e,x==0)),
                (BooleanEncoding::TypedBool,Atom::Bool(x))=>Ok(boolean(e,!x)),_=>Err(Failure::Kind),
            },
        },
        Instruction::Select(e,c,a,b)=>match(at(values,c as nat),at(values,a as nat),at(values,b as nat)) {
            (Some(c),Some(a),Some(b))=>match(e,c,a,b) {
                (BooleanEncoding::LegacyInteger01,Atom::I128(c),Atom::I128(a),Atom::I128(b))=>Ok(Atom::I128(if c==1{a}else{b})),
                (BooleanEncoding::TypedBool,Atom::Bool(c),a,b)=>if kind(a).is_some()&&kind(a)==kind(b){Ok(if c{a}else{b})}else{Err(Failure::Kind)},
                _=>Err(Failure::Kind),
            },_=>Err(Failure::Reference),
        },
        Instruction::Arithmetic(_,_,a,b)|Instruction::Eq(_,a,b)|Instruction::Lt(_,a,b)|Instruction::And(_,a,b)=>{
            match(at(values,a as nat),at(values,b as nat)) {
                (Some(a),Some(b))=>match op {
                    Instruction::Arithmetic(w,operation,_,_)=>match value::spec::arithmetic(w,operation,a,b) {
                        Ok(v)=>Ok(v),Err(NumericFailure::Kind)=>Err(Failure::Kind),Err(NumericFailure::Range)=>Err(Failure::Arithmetic),
                    },
                    Instruction::Eq(e,_,_)=>match(a,b) {
                        (Atom::Bool(a),Atom::Bool(b))=>Ok(boolean(e,a==b)),
                        (Atom::I128(a),Atom::I128(b))=>Ok(boolean(e,a==b)),
                        (Atom::U128(a),Atom::U128(b))=>Ok(boolean(e,a==b)),_=>Err(Failure::Kind),
                    },
                    Instruction::Lt(e,_,_)=>match(a,b) {
                        (Atom::I128(a),Atom::I128(b))=>Ok(boolean(e,a<b)),
                        (Atom::U128(a),Atom::U128(b))=>Ok(boolean(e,a<b)),_=>Err(Failure::Kind),
                    },
                    Instruction::And(e,_,_)=>match(e,a,b) {
                        (BooleanEncoding::LegacyInteger01,Atom::I128(a),Atom::I128(b))=>Ok(boolean(e,a==1&&b==1)),
                        (BooleanEncoding::TypedBool,Atom::Bool(a),Atom::Bool(b))=>Ok(boolean(e,a&&b)),_=>Err(Failure::Kind),
                    },_=>Err(Failure::Kind),
                },_=>Err(Failure::Reference),
            }
        },
    }
}
pub(crate) open spec fn project_result(result:Result<Atom,Failure>)->Result<i64,LegacyFailure> {
    match result {
        Ok(v)=>match project(v){Some(n)=>Ok(n),None=>Err(LegacyFailure::Arithmetic)},
        Err(Failure::Reference)=>Err(LegacyFailure::Reference),
        Err(_)=>Err(LegacyFailure::Arithmetic),
    }
}
pub(crate) proof fn legacy_node_embedding(op:Op,input:Seq<i64>,values:Seq<i64>)
    ensures project_result(node(legacy_instruction(op),lift(input),lift(values)))
        == super::super::spec::node(op,input,values),
        match node(legacy_instruction(op),lift(input),lift(values)) {
            Ok(v)=>project(v).is_some(),Err(e)=>e!=Failure::Kind,
        },
{
    match op {
        Op::Add(a,b)|Op::Sub(a,b)=>{
            if a<values.len()&&b<values.len() {
                value::spec::legacy_embedding(if matches!(op,Op::Add(..)){ArithmeticOp::Add}else{ArithmeticOp::Sub},values[a as int],values[b as int]);
            }
        },
        Op::Input(_)=>{}, Op::Int(_)=>{}, Op::Bool(_)=>{},
        Op::Eq(_,_)=>{}, Op::Lt(_,_)=>{}, Op::And(_,_)=>{}, Op::Not(_)=>{},
        Op::Select(_,_,_)=>{},
        Op::Mul(a,b)=>{},
        Op::DivFloor(a,b)=>{}, Op::DivCeil(a,b)=>{},
        Op::MulDivFloor(a,b,d)=>{}, Op::MulDivCeil(a,b,d)=>{},
        Op::ProductsGe(a,b)=>{},
    }
}
}
