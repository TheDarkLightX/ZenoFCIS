//! Eager closed predicates with actual shared-meter charging.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::at(spec::values(values@),index),
))]
fn at<'a>(values: &[Atom<'a>], index: usize) -> Result<Atom<'a>, Failure> {
    if index < values.len() {
        Ok(values[index])
    } else {
        Err(Failure::Undefined)
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::pure_node(op,spec::values(values@)),
))]
fn pure_node<'a>(op: &Op<'a>, values: &[Atom<'a>]) -> Result<Atom<'a>, Failure> {
    match *op {
        Op::Literal(value) => Ok(value),
        Op::Observe(_) | Op::ObserveWhen(..) => Err(Failure::Undefined),
        Op::Add(a, b) => super::atoms::binary(0, at(values, a)?, at(values, b)?),
        Op::Mul(a, b) => super::atoms::binary(5, at(values, a)?, at(values, b)?),
        Op::Div(mode, a, b) => super::atoms::divide(mode, at(values, a)?, at(values, b)?),
        Op::ToI128(a) => super::atoms::to_i128(at(values, a)?),
        Op::Sub(a, b) => super::atoms::binary(1, at(values, a)?, at(values, b)?),
        Op::Eq(a, b) => super::atoms::binary(2, at(values, a)?, at(values, b)?),
        Op::Lt(a, b) => super::atoms::binary(3, at(values, a)?, at(values, b)?),
        Op::And(a, b) => super::atoms::binary(4, at(values, a)?, at(values, b)?),
        Op::Not(a) => match at(values, a)? {
            Atom::Bool(v) => Ok(Atom::Bool(!v)),
            _ => Err(Failure::Undefined),
        },
        Op::Select(a, b, c) => {
            let condition = at(values, a)?;
            let yes = at(values, b)?;
            let no = at(values, c)?;
            if !super::atoms::same_type(yes, no) {
                return Err(Failure::Undefined);
            }
            match condition {
                Atom::Bool(true) => Ok(yes),
                Atom::Bool(false) => Ok(no),
                _ => Err(Failure::Undefined),
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (spec::value_result(result), final(meter).used.counters@, final(reads)@) ==
            spec::node(op,spec::values(values@),frame,law,index,old(meter).limits.counters@,
                old(meter).used.counters@,old(reads)@),
))]
fn node<'a>(
    op: &Op<'a>,
    values: &[Atom<'a>],
    frame: &Frame<'a>,
    law: u32,
    index: usize,
    meter: &mut Meter,
    reads: &mut Vec<ReadAttempt>,
) -> Result<Atom<'a>, Failure> {
    if let Err(e) = meter.charge(Resource::Step, 1) {
        return Err(Failure::Budget(e));
    }
    let observation = match *op {
        Op::Observe(observation) => observation,
        Op::ObserveWhen(guard, observation, default) => {
            if guard >= index {
                return Err(Failure::Undefined);
            }
            match at(values, guard)? {
                Atom::Bool(false) => return Ok(default),
                Atom::Bool(true) => observation,
                _ => return Err(Failure::Undefined),
            }
        }
        _ => return pure_node(op, values),
    };
    let permission = meter.charge(Resource::Read, 1);
    reads.push(ReadAttempt {
        law,
        node: index,
        observation,
        permitted: permission.is_ok(),
    });
    match permission {
        Err(e) => Err(Failure::Budget(e)),
        Ok(()) => match super::frame::observe(frame, observation) {
            Some(value) => Ok(value),
            None => Err(Failure::Undefined),
        },
    }
}

// The complete policy admits ordinary Text literals as ASCII; all other Atom
// variants are already exact scalar values or opaque bytes. Check inactive
// defaults too, during shape admission, without changing legacy Literal nodes.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::default_text_valid(bytes@),
))]
fn default_text_valid(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= bytes.len(), forall|j:int| 0 <= j < i ==> bytes@[j] < 128,
        decreases bytes.len() - i,
    ))]
    while i < bytes.len() {
        if bytes[i] >= 128 {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::shape(program),
))]
pub(super) fn shape(program: &Program<'_>) -> bool {
    if program.root >= program.nodes.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant program.root<program.nodes.len(), i<=program.nodes.len(),
            forall|j:int| 0<=j<i ==> spec::node_shape(program.nodes@[j],j as nat),
        decreases program.nodes.len()-i,
    ))]
    while i < program.nodes.len() {
        let valid = match program.nodes[i] {
            Op::Literal(_) | Op::Observe(_) => true,
            Op::ObserveWhen(guard, _, default) => {
                guard < i
                    && match default {
                        Atom::Text(bytes) => default_text_valid(bytes),
                        _ => true,
                    }
            }
            Op::Add(a, b)
            | Op::Sub(a, b)
            | Op::Mul(a, b)
            | Op::Div(_, a, b)
            | Op::Eq(a, b)
            | Op::Lt(a, b)
            | Op::And(a, b) => a < i && b < i,
            Op::Not(a) | Op::ToI128(a) => a < i,
            Op::Select(a, b, c) => a < i && b < i && c < i,
        };
        if !valid {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (result,final(meter).used.counters@,final(reads)@) ==
            spec::predicate(program,frame,law,old(meter).limits.counters@,old(meter).used.counters@,old(reads)@),
))]
pub(super) fn evaluate<'a>(
    program: &Program<'a>,
    frame: &Frame<'a>,
    law: u32,
    meter: &mut Meter,
    reads: &mut Vec<ReadAttempt>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost start_used=meter.used.counters@;let ghost start_reads=reads@;}
    let mut values = Vec::new();
    let mut i = 0usize;
    #[cfg(verus_keep_ghost)]
    proof! {assert(spec::values(values@) =~= Seq::<spec::Value>::empty());}
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=program.nodes.len(), values@.len()==i,
            meter.limits==old(meter).limits, start_used==old(meter).used.counters@,start_reads==old(reads)@,
            spec::prefix(program.nodes@,frame,law,i as nat,meter.limits.counters@,start_used,start_reads)
                ==(Ok(spec::values(values@)),meter.used.counters@,reads@),
        decreases program.nodes.len()-i,
    ))]
    while i < program.nodes.len() {
        match node(&program.nodes[i], &values, frame, law, i, meter, reads) {
            Ok(value) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::values_push(values@,value);}
                values.push(value);
            }
            Err(e) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::failed_prefix(program.nodes@,frame,law,(i+1) as nat,program.nodes@.len(),meter.limits.counters@,start_used,start_reads,e);}
                return Err(e);
            }
        }
        i += 1;
    }
    match at(&values, program.root)? {
        Atom::Bool(true) => Ok(()),
        Atom::Bool(false) => Err(Failure::Violated),
        _ => Err(Failure::Undefined),
    }
}
