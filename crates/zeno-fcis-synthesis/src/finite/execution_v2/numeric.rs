//! Full-width numeric execution through the one V2 metered prefix driver.
use super::super::{
    evaluation::{
        self,
        typed::{self, Instruction, NumericOp, Values},
    },
    value::{self, Atom, Domain},
};
use super::{
    Domain as LegacyDomain, Limits, MeterFailure, Op, Resource, Usage,
    meter::{self, Meter},
};
use alloc::vec::Vec;
pub use typed::admission::{MAX_NUMERIC_INPUTS, MAX_NUMERIC_NODES, MAX_NUMERIC_OUTPUTS};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "numeric/spec.rs"]
pub(crate) mod spec;

/// Borrowed numeric graph. Raw execution is defensive; admission is a separate check.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct NumericProgram<'a> {
    /// Complete input domains. Numeric execution supports Bool, I128 and U128.
    pub inputs: &'a [Domain<'a>],
    /// Complete output domains in root order.
    pub outputs: &'a [Domain<'a>],
    /// Eager instructions, including unused and unselected nodes.
    pub nodes: &'a [NumericOp],
    /// Output node references, in declared order.
    pub roots: &'a [u16],
}

impl NumericProgram<'_> {
    /// Checks bounded graph shape and every static numeric/Boolean kind.
    /// Raw execution remains separate and does not invoke this check implicitly.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.admission(),))]
    pub fn admit(&self) -> Result<(), evaluation::admission::AdmissionFailure> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(NumericProgram::admission); }
        typed::admission::validate(self.inputs, self.outputs, self.nodes, self.roots)
    }
}

/// Nonauthorizing numeric execution refusal.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// Input tuple does not belong to the numeric input domains.
    InputDomain,
    /// An attempted instruction or output names a missing value.
    Reference,
    /// An attempted instruction received incompatible atom kinds.
    Kind,
    /// Checked arithmetic exceeds the declared integer width.
    Arithmetic,
    /// Complete output tuple does not belong to its numeric domains.
    OutputDomain,
    /// A logical charge refused before the associated node was accessed.
    Budget(MeterFailure),
}

/// Full numeric values and immutable actual usage; this cannot authorize publication.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Outcome<'a> {
    result: Result<Vec<Atom<'a>>, Failure>,
    usage: Usage,
}
impl<'a> Outcome<'a> {
    /// Observes actual usage on success or refusal.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.view()==self.view().1,))]
    pub fn usage(&self) -> Usage {
        self.usage
    }
    /// Consumes the outcome into complete numeric values/refusal and usage.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures (match result.0 {Ok(v)=>Ok(v@),Err(e)=>Err(e)},result.1.view())==self.view(),))]
    pub fn into_parts(self) -> (Result<Vec<Atom<'a>>, Failure>, Usage) {
        (self.result, self.usage)
    }
}

// Public contracts conceal private adapters and outcome storage.
#[cfg(verus_keep_ghost)]
verus! {
impl<'a> NumericProgram<'a> {
    pub closed spec fn admission(&self)->Result<(),evaluation::admission::AdmissionFailure> {
        typed::admission::spec::program(self.inputs@,self.outputs@,self.nodes@,self.roots@)
    }
}
impl<'a> Outcome<'a> {
    pub closed spec fn view(&self)->(Result<Seq<Atom<'a>>,Failure>,Seq<u64>) {
        (match self.result {Ok(v)=>Ok(v@),Err(e)=>Err(e)},self.usage.view())
    }
}
pub closed spec fn execution<'a>(program:NumericProgram,input:Seq<Atom<'a>>,limits:Seq<u64>,used:Seq<u64>)
 ->(Result<Seq<Atom<'a>>,Failure>,Seq<u64>) {
    spec::execution(ProgramView::Typed(program),input,limits,used)
}
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) enum Domains<'a> {
    Legacy(&'a [LegacyDomain]),
    Typed(&'a [Domain<'a>]),
}

/// A closed borrowed data view, never an application-supplied evaluator.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) enum ProgramView<'a> {
    Legacy {
        inputs: &'a [LegacyDomain],
        outputs: &'a [LegacyDomain],
        nodes: &'a [Op],
        roots: &'a [u16],
    },
    Typed(NumericProgram<'a>),
}
impl<'a> ProgramView<'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as nat==spec::nodes(self).len(),))]
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Legacy { nodes, .. } => nodes.len(),
            Self::Typed(p) => p.nodes.len(),
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@==spec::roots(self),))]
    pub(crate) fn roots(self) -> &'a [u16] {
        match self {
            Self::Legacy { roots, .. } => roots,
            Self::Typed(p) => p.roots,
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::inputs(self),))]
    pub(crate) fn inputs(self) -> Domains<'a> {
        match self {
            Self::Legacy { inputs, .. } => Domains::Legacy(inputs),
            Self::Typed(p) => Domains::Typed(p.inputs),
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::outputs(self),))]
    pub(crate) fn outputs(self) -> Domains<'a> {
        match self {
            Self::Legacy { outputs, .. } => Domains::Legacy(outputs),
            Self::Typed(p) => Domains::Typed(p.outputs),
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::instruction(self,index as nat),))]
    pub(crate) fn instruction(self, index: usize) -> Option<Instruction> {
        match self {
            Self::Legacy { nodes, .. } => {
                if index < nodes.len() {
                    Some(typed::legacy_instruction(&nodes[index]))
                } else {
                    None
                }
            }
            Self::Typed(p) => {
                if index < p.nodes.len() {
                    Some(typed::typed_instruction(p.nodes[index]))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::admitted(domains,values.view()),))]
pub(crate) fn admitted(domains: Domains<'_>, values: Values<'_, '_>) -> bool {
    let length = match domains {
        Domains::Legacy(d) => d.len(),
        Domains::Typed(d) => d.len(),
    };
    if length != values.len() {
        return false;
    }
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=length,length==values.view().len(),
        length==spec::domain_count(domains),
        forall|j:int|0<=j<i ==> spec::contains(domains,j,values.view()[j]),decreases length-i,))]
    while i < length {
        let Some(atom) = values.get(i) else {
            return false;
        };
        let valid = match domains {
            Domains::Legacy(d) => match typed::project(atom) {
                Some(v) => d[i].contains(v),
                None => false,
            },
            Domains::Typed(d) => match d[i] {
                Domain::Bool | Domain::I128 { .. } | Domain::U128 { .. } => {
                    value::admitted(d[i], atom)
                }
                _ => false,
            },
        };
        if !valid {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result==spec::node_failure(error),))]
fn node_failure(error: typed::Failure) -> Failure {
    match error {
        typed::Failure::Reference => Failure::Reference,
        typed::Failure::Kind => Failure::Kind,
        typed::Failure::Arithmetic => Failure::Arithmetic,
    }
}

/// The single V2 eager driver. Step is charged before instruction conversion/access.
#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures final(meter).limits==old(meter).limits,
    (match result {Ok(())=>Ok(final(output)@),Err(e)=>Err(e)},final(meter).used.counters@)
        ==spec::execution(program,input.view(),old(meter).limits.counters@,old(meter).used.counters@),
    result.is_ok() ==> spec::prefix(spec::nodes(program),input.view(),spec::nodes(program).len(),
        old(meter).limits.counters@,old(meter).used.counters@)
        ==(Ok(final(values)@),final(meter).used.counters@),))]
pub(super) fn evaluate<'a>(
    program: ProgramView<'_>,
    input: Values<'_, 'a>,
    meter: &mut Meter,
    values: &mut Vec<Atom<'a>>,
    output: &mut Vec<Atom<'a>>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial_used=meter.used.counters@;}
    values.clear();
    output.clear();
    // No program/input lifting, reserve or shape admission precedes this test.
    if !admitted(program.inputs(), input) {
        return Err(Failure::InputDomain);
    }
    values.reserve(program.len());
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant
        spec::admitted(spec::inputs(program),input.view()),
        meter.limits==old(meter).limits,initial_used==old(meter).used.counters@,initial_used.len()==8,
        index<=spec::nodes(program).len(),values@.len()==index,output@==Seq::<Atom<'a>>::empty(),
        spec::prefix(spec::nodes(program),input.view(),index as nat,meter.limits.counters@,initial_used)
          ==(Ok(values@),meter.used.counters@),decreases spec::nodes(program).len()-index,))]
    while index < program.len() {
        if let Err(error) = meter.charge(Resource::Step, 1) {
            #[cfg(verus_keep_ghost)]
            proof! {spec::failure_persists(spec::nodes(program),input.view(),(index+1) as nat,
            spec::nodes(program).len(),meter.limits.counters@,initial_used,Failure::Budget(error));}
            return Err(Failure::Budget(error));
        }
        let Some(op) = program.instruction(index) else {
            return Err(Failure::Reference);
        };
        match typed::node(op, input, Values::Typed(values)) {
            Ok(value) => values.push(value),
            Err(error) => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::failure_persists(spec::nodes(program),input.view(),(index+1) as nat,
                spec::nodes(program).len(),meter.limits.counters@,initial_used,spec::node_failure(error));}
                return Err(node_failure(error));
            }
        }
        index += 1;
    }
    let roots = program.roots();
    output.reserve(roots.len());
    let mut root = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant
        spec::admitted(spec::inputs(program),input.view()),meter.limits==old(meter).limits,
        initial_used==old(meter).used.counters@,initial_used.len()==8,
        spec::prefix(spec::nodes(program),input.view(),spec::nodes(program).len(),meter.limits.counters@,initial_used)
          ==(Ok(values@),meter.used.counters@),
        roots@==spec::roots(program),root<=roots.len(),output@.len()==root,
        spec::projection(values@,roots@,root as nat)==Ok(output@),decreases roots.len()-root,))]
    while root < roots.len() {
        match Values::Typed(values).get(roots[root] as usize) {
            Some(value) => output.push(value),
            None => {
                #[cfg(verus_keep_ghost)]
                proof! {spec::projection_failure_persists(values@,roots@,(root+1) as nat,roots@.len());}
                return Err(Failure::Reference);
            }
        }
        root += 1;
    }
    if !admitted(program.outputs(), Values::Typed(output)) {
        return Err(Failure::OutputDomain);
    }
    Ok(())
}

/// Checked old-ABI projection. No typed result is silently narrowed.
#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures
    result ==> spec::project_all(values@)==Some(final(output)@),
    !result ==> spec::project_all(values@).is_none(),))]
pub(super) fn project_legacy(values: &[Atom<'_>], output: &mut Vec<i64>) -> bool {
    output.clear();
    output.reserve(values.len());
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),
        spec::project_prefix(values@,i as nat)==Some(output@),decreases values.len()-i,))]
    while i < values.len() {
        let Some(value) = typed::project(values[i]) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::project_failure_persists(values@,(i+1) as nat,values@.len());}
            return false;
        };
        output.push(value);
        i += 1;
    }
    true
}

/// Executes a full-width numeric graph with a fresh private meter.
///
/// This defensive API does not pre-admit graph shape/kinds. Every attempted node
/// is charged before its reference/kind/range checks. Allocation and domain/root
/// projection are outside the logical Step cost model. No publication authority.
#[must_use]
#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result.view()==execution(
    program,input@,limits.view(),Seq::new(8,|_:int|0u64)),))]
pub fn execute<'a>(program: NumericProgram<'_>, input: &[Atom<'a>], limits: Limits) -> Outcome<'a> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(execution); reveal(Outcome::view); reveal(Limits::view); reveal(Usage::view); }
    let mut meter = meter::new(limits);
    let mut values = Vec::new();
    let mut output = Vec::new();
    let result = match evaluate(
        ProgramView::Typed(program),
        Values::Typed(input),
        &mut meter,
        &mut values,
        &mut output,
    ) {
        Ok(()) => Ok(output),
        Err(error) => {
            values.clear();
            output.clear();
            Err(error)
        }
    };
    Outcome {
        result,
        usage: meter.used,
    }
}

#[cfg(test)]
#[path = "numeric/tests.rs"]
mod tests;
