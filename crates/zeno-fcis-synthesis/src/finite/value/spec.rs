//! Independent mathematical integer semantics; erased from native builds.
use super::{ArithmeticOp, Atom, Domain, IntegerWidth, NumericFailure};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn admitted(domain:Domain,value:Atom) -> bool {
    match (domain,value) {
        (Domain::Canonical{schema,type_id},Atom::Canonical(v)) => schema@ == v.view().0 && type_id == v.view().1,
        (Domain::Bool,Atom::Bool(_)) | (Domain::Bytes,Atom::Bytes(_)) | (Domain::Text,Atom::Text(_)) => true,
        (Domain::I128{min,max},Atom::I128(v)) => min <= v <= max,
        (Domain::U128{min,max},Atom::U128(v)) => min <= v <= max,
        (Domain::Enum{type_id,variants},Atom::Enum{type_id:t,variant}) |
        (Domain::Sum{type_id,variants},Atom::Sum{type_id:t,variant}) => type_id == t && variants@.contains(variant),
        _ => false,
    }
}

pub(crate) open spec fn operation(op: ArithmeticOp, left: int, right: int) -> int {
    match op {
        ArithmeticOp::Add => left + right,
        ArithmeticOp::Sub => left - right,
    }
}

pub(crate) open spec fn signed_range(width: IntegerWidth, value: int) -> bool {
    match width {
        IntegerWidth::Signed64 => i64::MIN <= value <= i64::MAX,
        IntegerWidth::Signed128 => i128::MIN <= value <= i128::MAX,
        IntegerWidth::Unsigned128 => false,
    }
}

pub(crate) open spec fn arithmetic<'a>(
    width: IntegerWidth, op: ArithmeticOp, left: Atom<'a>, right: Atom<'a>,
) -> Result<Atom<'a>, NumericFailure> {
    match (width, left, right) {
        (IntegerWidth::Signed64 | IntegerWidth::Signed128, Atom::I128(a), Atom::I128(b)) => {
            let result = operation(op, a as int, b as int);
            if signed_range(width, a as int) && signed_range(width, b as int)
                && signed_range(width, result) {
                Ok(Atom::I128(result as i128))
            } else {
                Err(NumericFailure::Range)
            }
        },
        (IntegerWidth::Unsigned128, Atom::U128(a), Atom::U128(b)) => {
            let result = operation(op, a as int, b as int);
            if 0 <= result <= u128::MAX {
                Ok(Atom::U128(result as u128))
            } else {
                Err(NumericFailure::Range)
            }
        },
        _ => Err(NumericFailure::Kind),
    }
}

pub(crate) proof fn scalar_embedding(op: ArithmeticOp, left: i128, right: i128)
    ensures match arithmetic(IntegerWidth::Signed128, op, Atom::I128(left), Atom::I128(right)) {
        Ok(Atom::I128(value)) => value as int == operation(op, left as int, right as int),
        _ => !(i128::MIN <= operation(op, left as int, right as int) <= i128::MAX),
    },
{
}

pub(crate) open spec fn legacy(op: ArithmeticOp, left: i64, right: i64) -> Option<i64> {
    let result = operation(op, left as int, right as int);
    if i64::MIN <= result <= i64::MAX { Some(result as i64) } else { None }
}

pub(crate) proof fn legacy_embedding(op: ArithmeticOp, left: i64, right: i64)
    ensures match arithmetic(IntegerWidth::Signed64, op,
        Atom::I128(left as i128), Atom::I128(right as i128)) {
        Ok(Atom::I128(value)) => i64::MIN <= value <= i64::MAX
            && legacy(op, left, right) == Some(value as i64),
        _ => legacy(op, left, right) == None::<i64>,
    },
{
}

}
