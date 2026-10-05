//! Checked ordered scalar continuation. A complete result remains untrusted data.
//!
//! The graph, original values, context, cursor, accumulator and one actual V2
//! meter are owned here. Drop cancels. Recovery admits original inputs again.
use super::super::evaluation::{self, Domain, Op, admission};
use super::{Failure as EvaluationFailure, Limits, MeterFailure, Resource, Usage, meter};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
mod spec;

/// Exact application context; this module does not establish its provenance.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Context {
    /// Starting semantic state root, including all 32 bytes.
    pub state_root: [u8; 32],
    /// Starting version; equal roots with different versions are stale.
    pub state_version: u64,
    /// Complete application invocation binding, including all 32 bytes.
    pub invocation_hash: [u8; 32],
}

/// Original structural preparation bounds, distinct from actual instruction work.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationLimits {
    /// Maximum owned items, no more than 65,536.
    pub max_items: u32,
    /// Maximum items per nonempty chunk, no more than 65,536.
    pub max_chunk_items: u32,
    /// Maximum canonical `(initial, items)` scalar tuple bytes, up to 16 MiB.
    pub max_input_bytes: u64,
    /// Maximum complete accumulator tuple bytes, up to 16 MiB.
    pub max_output_bytes: u64,
}

/// The original default preparation bounds.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (PreparationLimits { max_items:4096, max_chunk_items:64,
        max_input_bytes:4194304, max_output_bytes:4096 }),
))]
pub const fn default_limits() -> PreparationLimits {
    PreparationLimits {
        max_items: 4096,
        max_chunk_items: 64,
        max_input_bytes: 4194304,
        max_output_bytes: 4096,
    }
}

/// Closed graph admission diagnostics, preserving shape/type/reference precedence.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphFailure {
    /// Invalid shape or domain.
    Shape,
    /// Input reference is absent.
    InputReference,
    /// Earlier node reference is absent.
    NodeReference,
    /// Operand types disagree.
    TypeMismatch,
    /// Output root type disagrees.
    OutputType,
}

/// A structurally admitted, immutable original eager checked-i64 graph.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Graph {
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    nodes: Vec<Op>,
    roots: Vec<u16>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::graph_failure(error),))]
fn graph_failure(error: admission::AdmissionFailure) -> GraphFailure {
    match error {
        admission::AdmissionFailure::Shape => GraphFailure::Shape,
        admission::AdmissionFailure::InputReference => GraphFailure::InputReference,
        admission::AdmissionFailure::NodeReference => GraphFailure::NodeReference,
        admission::AdmissionFailure::TypeMismatch => GraphFailure::TypeMismatch,
        admission::AdmissionFailure::OutputType => GraphFailure::OutputType,
    }
}
/// Uses the same checked admission algorithms and all original graph operations.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        (match result { Ok(g)=>Ok(g.view()), Err(e)=>Err(e) }) ==
            graph_value(inputs@,outputs@,nodes@,roots@),
    ))]
pub fn admit_graph(
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    nodes: Vec<Op>,
    roots: Vec<u16>,
) -> Result<Graph, GraphFailure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Graph::view); reveal(graph_value); }
    match admission::validate_program(&inputs, &outputs, &nodes, &roots) {
        Err(e) => Err(graph_failure(e)),
        Ok(()) => Ok(Graph {
            inputs,
            outputs,
            nodes,
            roots,
        }),
    }
}
/// Original invalid preparation distinctions.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Invalid {
    /// Structural limits are invalid.
    Limits,
    /// A required context binding is zero.
    Context,
    /// Initial tuple or accumulator prefix is invalid.
    Accumulator,
}
/// Original capacity resource distinctions.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capacity {
    /// Item count.
    Items,
    /// Conservative complete node count.
    Steps,
    /// Final canonical accumulator bytes.
    OutputBytes,
    /// Complete canonical input bytes.
    InputBytes,
}
/// A refusal never exposes a temporary accumulator or advances a failed chunk.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The graph failed the original structural admission.
    Graph(GraphFailure),
    /// Invalid limits, context or accumulator.
    Invalid(Invalid),
    /// Original structural capacity diagnostic, including complete required bytes.
    Capacity {
        /// Exceeded capacity.
        resource: Capacity,
        /// Exact complete requirement.
        required: u64,
        /// Declared maximum.
        declared: u64,
    },
    /// Invalid item at this absolute zero-based position; contents remain private.
    InvalidItem {
        /// Original absolute item position.
        item: u32,
    },
    /// Complete logical reservation refused in Read/Write/Candidate/Byte order.
    Budget(MeterFailure),
    /// Original canonical Value validation exceeded one million nodes.
    EncodingNodes {
        /// Original default canonical node limit.
        limit: u64,
        /// First rejected node, not the aggregate node count.
        attempted: u64,
    },
    /// Offset differs from the private cursor; checked before chunk validity.
    WrongOffset {
        /// Actual cursor.
        expected: u32,
        /// Supplied cursor.
        supplied: u32,
    },
    /// Empty, overflowing, oversized or out-of-range chunk.
    InvalidChunk,
    /// Exact metered eager failure at the first absolute item position.
    Evaluation {
        /// Absolute item position, independent of partition.
        item: u32,
        /// Actual V2 evaluation refusal, including exhausted Step.
        source: EvaluationFailure,
    },
    /// Finish was requested before all items were successfully processed.
    Incomplete,
    /// Exact root/version/invocation comparison refused after completeness.
    StaleContext,
}

/// Exclusive ordered preparation. No clone, cursor/accumulator/meter constructor,
/// deserialization, partial result, callback or publication capability is provided.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct PreparedFold {
    graph: Graph,
    initial: Vec<i64>,
    items: Vec<Vec<i64>>,
    accumulator: Vec<i64>,
    context: Context,
    limits: PreparationLimits,
    processed: u32,
    meter: meter::Meter,
    reserved: Usage,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::same_domain(a,b),))]
fn same_domain(a: Domain, b: Domain) -> bool {
    match (a, b) {
        (Domain::Bool, Domain::Bool) => true,
        (Domain::Int { min: a, max: b }, Domain::Int { min: c, max: d }) => a == c && b == d,
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (a@ == b@),
))]
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::context_equal(a,b),))]
fn context_equal(a: Context, b: Context) -> bool {
    a.state_version == b.state_version
        && same_bytes(a.state_root.as_slice(), b.state_root.as_slice())
        && same_bytes(a.invocation_hash.as_slice(), b.invocation_hash.as_slice())
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::context_valid(c),))]
fn context_valid(c: Context) -> bool {
    let zeros = [0u8; 32];
    #[cfg(verus_keep_ghost)]
    proof! { assert(zeros@ =~= Seq::new(32,|_:int|0u8)); }
    !same_bytes(c.state_root.as_slice(), &zeros)
        && !same_bytes(c.invocation_hash.as_slice(), &zeros)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::prefix_matches(inputs@,outputs@),))]
fn prefix_matches(inputs: &[Domain], outputs: &[Domain]) -> bool {
    if inputs.len() < outputs.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=outputs.len()<=inputs.len(),
        forall|j:int| 0<=j<i ==> spec::same_domain(inputs@[j],outputs@[j]),
        decreases outputs.len()-i,))]
    while i < outputs.len() {
        if !same_domain(inputs[i], outputs[i]) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::capacity(resource,required,declared),))]
fn capacity(resource: Capacity, required: u64, declared: u64) -> Result<(), Failure> {
    if required > declared {
        Err(Failure::Capacity {
            resource,
            required,
            declared,
        })
    } else {
        Ok(())
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
fn same_scalars(a: &[i64], b: &[i64]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
fn same_roots(a: &[u16], b: &[u16]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
fn same_counters(a: &[u64], b: &[u64]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a == b),))]
fn same_node(a: Op, b: Op) -> bool {
    match (a, b) {
        (Op::Input(a), Op::Input(b)) | (Op::Not(a), Op::Not(b)) => a == b,
        (Op::Int(a), Op::Int(b)) => a == b,
        (Op::Bool(a), Op::Bool(b)) => a == b,
        (Op::Add(a, b), Op::Add(c, d))
        | (Op::Sub(a, b), Op::Sub(c, d))
        | (Op::Eq(a, b), Op::Eq(c, d))
        | (Op::Lt(a, b), Op::Lt(c, d))
        | (Op::And(a, b), Op::And(c, d)) => a == c && b == d,
        (Op::Select(a, b, c), Op::Select(d, e, f)) => a == d && b == e && c == f,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
fn same_domains(a: &[Domain], b: &[Domain]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if !same_domain(a[i], b[i]) {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
fn same_nodes(a: &[Op], b: &[Op]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]==b@[j], decreases a.len()-i,))]
    while i < a.len() {
        if !same_node(a[i].clone(), b[i].clone()) {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (spec::items(a@) == spec::items(b@)),))]
fn same_items(a: &[Vec<i64>], b: &[Vec<i64>]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=a.len(),a.len()==b.len(),
        forall|j:int| 0<=j<i ==> a@[j]@==b@[j]@, decreases a.len()-i,))]
    while i < a.len() {
        if !same_scalars(&a[i], &b[i]) {
            #[cfg(verus_keep_ghost)]
            proof! { assert(spec::items(a@)[i as int]!=spec::items(b@)[i as int]); }
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(spec::items(a@) =~= spec::items(b@)); }
    true
}

/// Admits owned original inputs and reserves complete modeled costs.
/// Read/write are scalar fields, Candidate is item evaluations, Byte includes
/// all tuple frames. Step counts actual instruction attempts through one meter.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        start_result_value(result) == start_value(graph.view(),initial@,
            items@.map(|_:int,x:Vec<i64>|x@),context,limits,budget.view()),
        match result { Ok(p)=>valid_state_value(p.view()),Err(_)=>true },
    ))]
pub fn start(
    graph: Graph,
    initial: Vec<i64>,
    items: Vec<Vec<i64>>,
    context: Context,
    limits: PreparationLimits,
    budget: Limits,
) -> Result<PreparedFold, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Graph::view); reveal(PreparedFold::view);
    reveal(start_value); reveal(start_result_value); reveal(Limits::view); reveal(valid_state_value); }
    // Reuse complete actual admission rather than assume private constructor custody.
    if let Err(e) =
        admission::validate_program(&graph.inputs, &graph.outputs, &graph.nodes, &graph.roots)
    {
        return Err(Failure::Graph(graph_failure(e)));
    }
    #[cfg(verus_keep_ghost)]
    proof! { spec::admitted_shape(graph.view()); }
    if limits.max_items > 65536
        || limits.max_chunk_items == 0
        || limits.max_chunk_items > 65536
        || limits.max_input_bytes > 16777216
        || limits.max_output_bytes > 16777216
    {
        return Err(Failure::Invalid(Invalid::Limits));
    }
    if !context_valid(context) {
        return Err(Failure::Invalid(Invalid::Context));
    }
    if !prefix_matches(&graph.inputs, &graph.outputs)
        || !evaluation::admitted(&graph.outputs, &initial)
    {
        return Err(Failure::Invalid(Invalid::Accumulator));
    }
    let count = items.len() as u64;
    capacity(Capacity::Items, count, limits.max_items as u64)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(count==items.len()); assert(count<=65536); assert(graph.nodes.len()<=256);
        assert(count as int*graph.nodes.len()<=16777216) by(nonlinear_arith)
            requires count<=65536,graph.nodes.len()<=256;
    }
    let steps = count * graph.nodes.len() as u64;
    capacity(Capacity::Steps, steps, 100000000)?;
    let fields = graph.outputs.len();
    let output_bytes = 5 + 17 * fields as u64;
    capacity(Capacity::OutputBytes, output_bytes, limits.max_output_bytes)?;
    let item_domains = &graph.inputs[fields..];
    let item_bytes = 5 + 17 * item_domains.len() as u64;
    let mut prefix = output_bytes + 10;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(item_bytes<=549);assert(prefix<=287);
        assert(count as int*item_bytes as int<=35979264) by(nonlinear_arith)
            requires count<=65536,item_bytes<=549;
    }
    let total = prefix + count * item_bytes;
    if prefix > limits.max_input_bytes {
        #[cfg(verus_keep_ghost)]
        proof! { spec::item_failure(graph.view(),spec::items(items@),0,items@.len(),limits.max_input_bytes,
        Failure::Capacity{resource:Capacity::InputBytes,required:total,declared:limits.max_input_bytes}); }
        return Err(Failure::Capacity {
            resource: Capacity::InputBytes,
            required: total,
            declared: limits.max_input_bytes,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(spec::ready_inputs(graph.view(),initial@,spec::items(items@),context,limits)); }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=items.len()<=65536,
            fields==graph.outputs.len(),fields<=16,graph.inputs.len()<=32,fields<=graph.inputs.len(),
            item_domains@==graph.inputs@.subrange(fields as int,graph.inputs.len() as int),
            spec::ready_inputs(graph.view(),initial@,spec::items(items@),context,limits),
            count==items.len(), item_bytes==5+17*(graph.inputs.len()-fields),
            output_bytes==5+17*fields,prefix==output_bytes+10+i*item_bytes,
            total==output_bytes+10+items.len()*item_bytes,
            forall|j:int| 0<=j<i ==> evaluation::spec::admitted(item_domains@,items@[j]@),
            spec::item_prefix(graph.view(),spec::items(items@),i as nat,limits.max_input_bytes)==Ok(prefix),
            item_bytes<=549,output_bytes<=277,
            output_bytes+10<=limits.max_input_bytes,
            i>0 ==> prefix<=limits.max_input_bytes,
            decreases items.len()-i,))]
    while i < items.len() {
        if !evaluation::admitted(item_domains, &items[i]) {
            #[cfg(verus_keep_ghost)]
            proof! { spec::item_failure(graph.view(),spec::items(items@),(i+1) as nat,items@.len(),
            limits.max_input_bytes,Failure::InvalidItem{item:i as u32}); }
            return Err(Failure::InvalidItem { item: i as u32 });
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(prefix as int+item_bytes as int<=40000000) by(nonlinear_arith)
                requires prefix==output_bytes+10+i*item_bytes,i<=65536,item_bytes<=549,output_bytes<=277;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert((i as int+1)*item_bytes as int==i as int*item_bytes as int+item_bytes as int) by(nonlinear_arith);
        }
        prefix += item_bytes;
        if prefix > limits.max_input_bytes {
            #[cfg(verus_keep_ghost)]
            proof! { spec::item_failure(graph.view(),spec::items(items@),(i+1) as nat,items@.len(),
            limits.max_input_bytes,Failure::Capacity{resource:Capacity::InputBytes,required:total,declared:limits.max_input_bytes}); }
            return Err(Failure::Capacity {
                resource: Capacity::InputBytes,
                required: total,
                declared: limits.max_input_bytes,
            });
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(count as int*graph.inputs.len()<=2097152) by(nonlinear_arith)
            requires count<=65536,graph.inputs.len()<=32;
        assert(count as int*fields<=1048576) by(nonlinear_arith)
            requires count<=65536,fields<=16;
        assert(total<=40000000) by(nonlinear_arith)
            requires total==output_bytes+10+items.len()*item_bytes,items.len()<=65536,item_bytes<=549,output_bytes<=277;
    }
    let mut meter = meter::new(budget);
    if let Err(e) = meter.charge(Resource::Read, count * graph.inputs.len() as u64) {
        return Err(Failure::Budget(e));
    }
    if let Err(e) = meter.charge(Resource::Write, count * fields as u64) {
        return Err(Failure::Budget(e));
    }
    if let Err(e) = meter.charge(Resource::Candidate, count) {
        return Err(Failure::Budget(e));
    }
    if let Err(e) = meter.charge(Resource::Byte, total + output_bytes) {
        return Err(Failure::Budget(e));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(item_domains.len()<=32);
        assert(count as int*(1+item_domains.len())<=2162688) by(nonlinear_arith)
            requires count<=65536,item_domains.len()<=32;
    }
    // The old input's canonical validation occurs after logical reservation.
    if 3 + fields as u64 + count * (1 + item_domains.len() as u64) > 1000000 {
        return Err(Failure::EncodingNodes {
            limit: 1000000,
            attempted: 1000001,
        });
    }
    let accumulator = initial.clone();
    let reserved = meter.used;
    Ok(PreparedFold {
        graph,
        initial,
        items,
        accumulator,
        context,
        limits,
        processed: 0,
        meter,
        reserved,
    })
}

impl PreparedFold {
    /// Processes the next nonempty range atomically. Work survives all failures.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        advance_value(old(self).view(),final(self).view(),result,offset,count),
        valid_state_value(old(self).view()) ==> valid_state_value(final(self).view()),
    ))]
    pub fn advance(&mut self, offset: u32, count: u32) -> Result<(), Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(Graph::view); reveal(advance_value); reveal(valid_state_value); }
        if offset != self.processed {
            return Err(Failure::WrongOffset {
                expected: self.processed,
                supplied: offset,
            });
        }
        let end = match offset.checked_add(count) {
            Some(e) => e,
            None => return Err(Failure::InvalidChunk),
        };
        if count == 0 || count > self.limits.max_chunk_items || end as usize > self.items.len() {
            return Err(Failure::InvalidChunk);
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost original=self.view(); }
        let mut next = self.accumulator.clone();
        let mut values = Vec::new();
        let mut output = Vec::new();
        let mut position = offset as usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(invariant
            original==old(self).view(),original.used.len()==8,original.budget.len()==8,
            spec::valid_state(original)==spec::valid_state(self.view()),
            count>0,count<=self.limits.max_chunk_items,
            end as int==offset as int+count as int,
            offset==self.processed,position>=offset,position<=end,end<=self.items.len(),
            self.graph.inputs@==original.graph.inputs,self.graph.outputs@==original.graph.outputs,
            self.graph.nodes@==original.graph.nodes,self.graph.roots@==original.graph.roots,
            spec::items(self.items@)==original.items,self.meter.limits.counters@==original.budget,
            self.view().graph==original.graph,self.view().items==original.items,
            self.view().initial==original.initial,self.view().context==original.context,
            self.view().limits==original.limits,self.view().processed==original.processed,
            self.view().accumulator==original.accumulator,self.view().budget==original.budget,
            self.view().reserved==original.reserved,
            spec::fold(original.graph,original.items,offset as nat,position as nat,
                original.accumulator,original.budget,original.used)==(Ok(next@),self.meter.used.counters@),
            decreases end as usize-position,))]
        while position < end as usize {
            let mut environment = next.clone();
            environment.extend_from_slice(&self.items[position]);
            #[cfg(verus_keep_ghost)]
            proof! { assert(environment@==next@+original.items[position as int]); }
            match super::evaluate_into(
                &self.graph.inputs,
                &self.graph.outputs,
                &self.graph.nodes,
                &self.graph.roots,
                &environment,
                &mut self.meter,
                &mut values,
                &mut output,
            ) {
                Err(source) => {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        assert(spec::fold(original.graph,original.items,offset as nat,(position+1) as nat,
                            original.accumulator,original.budget,original.used)
                            ==(Err(Failure::Evaluation{item:position as u32,source}),self.meter.used.counters@));
                        spec::fold_failure(self.graph.view(),spec::items(self.items@),offset as nat,
                            (position+1) as nat,end as nat,original.accumulator,
                            original.budget,original.used,position as u32,source);
                    }
                    return Err(Failure::Evaluation {
                        item: position as u32,
                        source,
                    });
                }
                Ok(()) => {
                    next = output.clone();
                }
            }
            position += 1;
        }
        self.accumulator = next;
        self.processed = end;
        #[cfg(verus_keep_ghost)]
        proof! {
            if spec::valid_state(original) {
                assert(spec::advance_result(original,self.view(),Ok(()),offset,count));
                spec::advance_preserves_prefix(original,self.view(),Ok(()),offset,count);
            }
        }
        Ok(())
    }

    /// Copies the complete result after exact current-context comparison.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        (match result { Ok(v)=>Ok(v@),Err(e)=>Err(e) })==finish_value(self.view(),current),
        match result { Ok(v)=>valid_state_value(self.view()) ==> complete_fold_value(self.view(),v@),Err(_)=>true },
    ))]
    pub fn finish(&self, current: Context) -> Result<Vec<i64>, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(finish_value); reveal(valid_state_value); reveal(complete_fold_value); }
        if self.processed as usize != self.items.len() {
            return Err(Failure::Incomplete);
        }
        if !context_equal(self.context, current) {
            return Err(Failure::StaleContext);
        }
        Ok(self.accumulator.clone())
    }
    /// Exact equality of all immutable original operation data and all eight limits.
    /// Cursor, chunk partition and consumed Step are excluded. No digest assumption
    /// or saved intermediate state is accepted through this identity comparison.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        result==same_operation_value(self.view(),other.view()),
    ))]
    pub fn same_operation(&self, other: &Self) -> bool {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(Graph::view); reveal(same_operation_value); }
        same_domains(&self.graph.inputs, &other.graph.inputs)
            && same_domains(&self.graph.outputs, &other.graph.outputs)
            && same_nodes(&self.graph.nodes, &other.graph.nodes)
            && same_roots(&self.graph.roots, &other.graph.roots)
            && same_scalars(&self.initial, &other.initial)
            && same_items(&self.items, &other.items)
            && context_equal(self.context, other.context)
            && self.limits.max_items == other.limits.max_items
            && self.limits.max_chunk_items == other.limits.max_chunk_items
            && self.limits.max_input_bytes == other.limits.max_input_bytes
            && self.limits.max_output_bytes == other.limits.max_output_bytes
            && same_counters(
                self.meter.limits.counters.as_slice(),
                other.meter.limits.counters.as_slice(),
            )
    }
    /// Exact successfully committed prefix.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==self.view().processed,))]
    pub fn processed_items(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); }
        self.processed
    }
    /// Remaining items. Only start/advance can construct or change the cursor.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
        result == remaining_value(self.view()),
    ))]
    pub fn remaining_items(&self) -> u32 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(remaining_value); }
        (self.items.len() as u32).saturating_sub(self.processed)
    }
    /// Immutable complete logical reservation; Step remains zero in this snapshot.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.view()==self.view().reserved,))]
    pub fn reserved_budget(&self) -> Usage {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(Usage::view); }
        self.reserved
    }
    /// Actual cumulative library usage, including failed chunks and retries.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result.view()==self.view().used,))]
    pub fn usage(&self) -> Usage {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(PreparedFold::view); reveal(Usage::view); }
        self.meter.used
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl Graph { pub closed spec fn view(&self)->spec::GraphView {
    spec::GraphView { inputs:self.inputs@,outputs:self.outputs@,nodes:self.nodes@,roots:self.roots@ }
} }
impl PreparedFold { pub closed spec fn view(&self)->spec::FoldView {
    spec::FoldView { graph:self.graph.view(),initial:self.initial@,items:spec::items(self.items@),
        accumulator:self.accumulator@,context:self.context,limits:self.limits,processed:self.processed,
        budget:self.meter.limits.counters@,used:self.meter.used.counters@,reserved:self.reserved.counters@ }
} }

pub closed spec fn graph_value(a:Seq<Domain>,b:Seq<Domain>,n:Seq<Op>,r:Seq<u16>)->Result<spec::GraphView,GraphFailure> {
    spec::graph(a,b,n,r)
}
pub closed spec fn start_value(g:spec::GraphView,a:Seq<i64>,v:Seq<Seq<i64>>,c:Context,l:PreparationLimits,b:Seq<u64>)->Result<spec::FoldView,Failure> {
    spec::start(g,a,v,c,l,b)
}
pub closed spec fn start_result_value(r:Result<PreparedFold,Failure>)->Result<spec::FoldView,Failure> { spec::start_result(r) }
pub closed spec fn advance_value(a:spec::FoldView,b:spec::FoldView,r:Result<(),Failure>,o:u32,c:u32)->bool { spec::advance_result(a,b,r,o,c) }
pub closed spec fn finish_value(p:spec::FoldView,c:Context)->Result<Seq<i64>,Failure> { spec::finish(p,c) }
pub closed spec fn remaining_value(p:spec::FoldView)->u32 { spec::remaining(p) }
pub closed spec fn same_operation_value(a:spec::FoldView,b:spec::FoldView)->bool { spec::same_operation(a,b) }
pub closed spec fn valid_state_value(p:spec::FoldView)->bool { spec::valid_state(p) }
pub closed spec fn complete_fold_value(p:spec::FoldView,output:Seq<i64>)->bool {
    spec::pure_fold(p.graph,p.items,0,p.items.len(),p.initial)==Ok(output)
}
}

#[cfg(test)]
mod tests;
