//! Numeric shape/type admission, separate from defensive raw execution.
use super::super::super::value::{Domain, IntegerWidth};
use super::super::admission::AdmissionFailure;
use super::{Instruction, Kind, NumericOp, kind_equals, option_kind_equals, typed_instruction};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "admission/spec.rs"]
pub(crate) mod spec;

/// Maximum inputs in the full-width numeric profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_NUMERIC_INPUTS: usize = 64;
/// Maximum outputs in the full-width numeric profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_NUMERIC_OUTPUTS: usize = 64;
/// Maximum eager nodes in the full-width numeric profile; references remain u16.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const MAX_NUMERIC_NODES: usize = 4096;

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::domain_kind(domain),))]
fn domain_kind(domain: Domain<'_>) -> Option<Kind> {
    match domain {
        Domain::Bool => Some(Kind::Bool),
        Domain::I128 { min, max } if min <= max => Some(Kind::Signed),
        Domain::U128 { min, max } if min <= max => Some(Kind::Unsigned),
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::get(kinds@,id),))]
fn get(kinds: &[Kind], id: u16) -> Result<Kind, AdmissionFailure> {
    if (id as usize) < kinds.len() {
        Ok(kinds[id as usize])
    } else {
        Err(AdmissionFailure::NodeReference)
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::require_kind(kinds@,id,expected),))]
fn require_kind(kinds: &[Kind], id: u16, expected: Kind) -> Result<(), AdmissionFailure> {
    if kind_equals(get(kinds, id)?, expected) {
        Ok(())
    } else {
        Err(AdmissionFailure::TypeMismatch)
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::kind(op,inputs@,previous@),))]
fn kind(op: NumericOp, inputs: &[Domain<'_>], previous: &[Kind]) -> Result<Kind, AdmissionFailure> {
    match typed_instruction(op) {
        Instruction::Input(i) => {
            if (i as usize) < inputs.len() {
                match domain_kind(inputs[i as usize]) {
                    Some(k) => Ok(k),
                    None => Err(AdmissionFailure::TypeMismatch),
                }
            } else {
                Err(AdmissionFailure::InputReference)
            }
        }
        Instruction::Literal(_, kind) => Ok(kind),
        // These adapters are never produced by typed_instruction. Refuse them
        // here rather than extending the existing public numeric instruction set.
        Instruction::ScalarMul(..)
        | Instruction::ScalarDiv(..)
        | Instruction::ScalarMulDiv(..)
        | Instruction::ScalarProductsGe(..) => Err(AdmissionFailure::TypeMismatch),
        Instruction::MulDiv(_, a, b, den) => {
            require_kind(previous, a, Kind::Unsigned)?;
            require_kind(previous, b, Kind::Unsigned)?;
            require_kind(previous, den, Kind::Unsigned)?;
            Ok(Kind::Unsigned)
        }
        Instruction::ProductsGe(left, right) => {
            require_kind(previous, left[0], Kind::Unsigned)?;
            require_kind(previous, left[1], Kind::Unsigned)?;
            require_kind(previous, left[2], Kind::Unsigned)?;
            require_kind(previous, right[0], Kind::Unsigned)?;
            require_kind(previous, right[1], Kind::Unsigned)?;
            require_kind(previous, right[2], Kind::Unsigned)?;
            Ok(Kind::Bool)
        }
        Instruction::Arithmetic(width, _, a, b) => {
            let expected = match width {
                IntegerWidth::Unsigned128 => Kind::Unsigned,
                _ => Kind::Signed,
            };
            require_kind(previous, a, expected)?;
            require_kind(previous, b, expected)?;
            Ok(expected)
        }
        Instruction::Eq(_, a, b) | Instruction::Lt(_, a, b) => {
            let left = get(previous, a)?;
            let right = get(previous, b)?;
            if !kind_equals(left, right)
                || (matches!(op, NumericOp::Lt(..)) && kind_equals(left, Kind::Bool))
            {
                Err(AdmissionFailure::TypeMismatch)
            } else {
                Ok(Kind::Bool)
            }
        }
        Instruction::And(_, a, b) => {
            require_kind(previous, a, Kind::Bool)?;
            require_kind(previous, b, Kind::Bool)?;
            Ok(Kind::Bool)
        }
        Instruction::Not(_, a) => {
            require_kind(previous, a, Kind::Bool)?;
            Ok(Kind::Bool)
        }
        Instruction::Select(_, c, a, b) => {
            require_kind(previous, c, Kind::Bool)?;
            let a = get(previous, a)?;
            let b = get(previous, b)?;
            if kind_equals(a, b) {
                Ok(a)
            } else {
                Err(AdmissionFailure::TypeMismatch)
            }
        }
    }
}

/// Complete static check; does not execute, allocate authority, or alter raw ABI limits.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::program(inputs@,outputs@,nodes@,roots@),))]
pub(crate) fn validate(
    inputs: &[Domain<'_>],
    outputs: &[Domain<'_>],
    nodes: &[NumericOp],
    roots: &[u16],
) -> Result<(), AdmissionFailure> {
    if inputs.len() > MAX_NUMERIC_INPUTS
        || outputs.is_empty()
        || outputs.len() > MAX_NUMERIC_OUTPUTS
        || nodes.is_empty()
        || nodes.len() > MAX_NUMERIC_NODES
        || outputs.len() != roots.len()
    {
        return Err(AdmissionFailure::Shape);
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=inputs.len(),
        forall|j:int|0<=j<i ==> spec::domain_kind(inputs@[j]).is_some(),decreases inputs.len()-i,))]
    while i < inputs.len() {
        if domain_kind(inputs[i]).is_none() {
            return Err(AdmissionFailure::Shape);
        }
        i += 1;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=outputs.len(),spec::valid_domains(inputs@),
        forall|j:int|0<=j<i ==> spec::domain_kind(outputs@[j]).is_some(),decreases outputs.len()-i,))]
    while i < outputs.len() {
        if domain_kind(outputs[i]).is_none() {
            return Err(AdmissionFailure::Shape);
        }
        i += 1;
    }
    let mut kinds = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=nodes.len(),kinds@.len()==i,
        spec::counts(inputs@.len(),outputs@.len(),nodes@.len(),roots@.len()),
        spec::valid_domains(inputs@),spec::valid_domains(outputs@),
        spec::prefix(inputs@,nodes@,i as nat)==Ok(kinds@),decreases nodes.len()-i,))]
    while i < nodes.len() {
        match kind(nodes[i], inputs, &kinds) {
            Ok(k) => kinds.push(k),
            Err(e) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::failure_persists(inputs@,nodes@,(i+1) as nat,nodes@.len(),e);}
                return Err(e);
            }
        }
        i += 1;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=roots.len(),
        spec::counts(inputs@.len(),outputs@.len(),nodes@.len(),roots@.len()),
        spec::valid_domains(inputs@),spec::valid_domains(outputs@),
        spec::prefix(inputs@,nodes@,nodes@.len())==Ok(kinds@),
        forall|j:int|0<=j<i ==> spec::root_valid(outputs@,roots@,kinds@,j),decreases roots.len()-i,))]
    while i < roots.len() {
        if (roots[i] as usize) >= kinds.len()
            || !option_kind_equals(Some(kinds[roots[i] as usize]), domain_kind(outputs[i]))
        {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(!spec::root_valid(outputs@,roots@,kinds@,i as int));
                assert(exists|j:int|0<=j<roots@.len() && !spec::root_valid(outputs@,roots@,kinds@,j));
                assert(spec::counts(inputs@.len(),outputs@.len(),nodes@.len(),roots@.len()));
                assert(spec::valid_domains(inputs@));
                assert(spec::valid_domains(outputs@));
                assert(spec::prefix(inputs@,nodes@,nodes@.len())==Ok(kinds@));
                match spec::prefix(inputs@,nodes@,nodes@.len()) {
                    Ok(all_kinds)=>{
                        assert(all_kinds==kinds@);
                        assert(!spec::root_valid(outputs@,roots@,all_kinds,i as int));
                        assert(!(forall|j:int|0<=j<roots@.len() ==> spec::root_valid(outputs@,roots@,all_kinds,j)));
                    },
                    Err(_)=>{assert(false);},
                }
                reveal(spec::program);
                assert(spec::program(inputs@,outputs@,nodes@,roots@)==Err(AdmissionFailure::OutputType));
            }
            return Err(AdmissionFailure::OutputType);
        }
        i += 1;
    }
    Ok(())
}
