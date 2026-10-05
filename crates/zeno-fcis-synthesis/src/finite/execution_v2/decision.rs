//! Complete typed candidates. Only the library composition route may construct one.
use super::meter::{Meter, MeterFailure, Resource};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Complete typed value used by decision expressions and candidates.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Atom<'a> {
    /// Boolean value.
    Bool(bool),
    /// Signed 128-bit value.
    I128(i128),
    /// Unsigned 128-bit value.
    U128(u128),
    /// Original declared Enum type and variant.
    Enum {
        /// Original type identifier.
        type_id: u32,
        /// Original variant identifier.
        variant: u16,
    },
    /// Original declared payload-free Sum type and variant.
    Sum {
        /// Original type identifier.
        type_id: u32,
        /// Original variant identifier.
        variant: u16,
    },
    /// Complete uninterpreted bytes.
    Bytes(&'a [u8]),
    /// Complete text bytes, validated by the consuming schema profile.
    Text(&'a [u8]),
}
/// Original declared field and its complete typed value.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Field<'a> {
    /// Original field identifier.
    pub id: u16,
    /// Complete field value.
    pub value: Atom<'a>,
}
/// One original invocation source.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
    /// Original state.
    State,
    /// Original command.
    Command,
    /// Original context.
    Context,
}
/// Actual input root; records retain their original field identifiers.
/// Shared by decision construction and law frames as the one borrowed root.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub enum RootView<'fields, 'data> {
    /// One original scalar root.
    Leaf(Atom<'data>),
    /// Complete original record.
    Record(&'fields [Field<'data>]),
}
/// Borrowed original typed inputs with independent field and data lifetimes.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(super) struct Inputs<'fields, 'data> {
    /// Complete original state fields.
    pub state: &'fields [Field<'data>],
    /// Original command root or record.
    pub command: RootView<'fields, 'data>,
    /// Original context root or record.
    pub context: RootView<'fields, 'data>,
}
/// Closed expression over actual original inputs and actual graph outputs.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Expr<'a> {
    /// Declared field of an original record source.
    Input(Source, u16),
    /// Complete scalar root, distinct from every record field identifier.
    Root(Source),
    /// Typed actual graph output in output ABI order.
    Output(usize),
    /// Complete declared literal value.
    Constant(Atom<'a>),
}
/// Complete allowed value domain for a decision slot.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Domain<'a> {
    /// Both Boolean values.
    Bool,
    /// Inclusive signed range.
    I128 {
        /// Inclusive lower bound.
        min: i128,
        /// Inclusive upper bound.
        max: i128,
    },
    /// Inclusive unsigned range.
    U128 {
        /// Inclusive lower bound.
        min: u128,
        /// Inclusive upper bound.
        max: u128,
    },
    /// Original Enum type and complete allowed variant identifier set.
    Enum {
        /// Original type identifier.
        type_id: u32,
        /// Complete allowed variants; ordering has no semantic significance.
        variants: &'a [u16],
    },
    /// Original payload-free Sum type and complete variant identifier set.
    Sum {
        /// Original type identifier.
        type_id: u32,
        /// Complete allowed variants; ordering has no semantic significance.
        variants: &'a [u16],
    },
    /// Complete uninterpreted byte values.
    Bytes,
    /// Complete text values, subject to the consuming schema profile.
    Text,
}
/// A declared complete successor field.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Assignment<'a> {
    /// Original state field identifier.
    pub field: u16,
    /// Actual expression producing the complete successor value.
    pub value: Expr<'a>,
    /// Complete declared successor domain.
    pub domain: Domain<'a>,
}
/// A declared field in a complete delivery payload.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct PayloadField<'a> {
    /// Original payload field identifier.
    pub field: u16,
    /// Actual expression producing the complete payload value.
    pub value: Expr<'a>,
}
/// Complete conditional effect or outbox declaration.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct DeliveryPlan<'a> {
    /// Declared ordinal within this delivery lane.
    pub ordinal: u32,
    /// Original channel identifier.
    pub channel: u32,
    /// Actual Boolean expression controlling emission.
    pub when: Expr<'a>,
    /// Complete destination expression.
    pub destination: Expr<'a>,
    /// Complete ordered payload field declarations.
    pub payload: &'a [PayloadField<'a>],
    /// Complete idempotency expression.
    pub idempotency: Expr<'a>,
}
/// Complete business decision class; technical refusal is reported separately.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Class {
    /// Successful committing decision.
    Accept,
    /// Business rejection with no staged authority.
    Reject,
    /// Business failure that commits its declared successor and deliveries.
    CommittedFailure,
}
/// Complete decision branch selected by the actual graph code.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Branch<'a> {
    /// Actual graph output code selecting this branch.
    pub code: i128,
    /// Business decision class.
    pub class: Class,
    /// Original declared reason identifier, if applicable.
    pub reason: Option<u32>,
    /// Complete successor assignments in original field order.
    pub assignments: &'a [Assignment<'a>],
    /// Complete effect declarations in ordinal order.
    pub effects: &'a [DeliveryPlan<'a>],
    /// Complete outbox declarations in ordinal order.
    pub outbox: &'a [DeliveryPlan<'a>],
}
/// One canonical state change, preserving complete before and after values.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Patch<'a> {
    /// Original changed field identifier.
    pub field: u16,
    /// Complete original field value.
    pub before: Atom<'a>,
    /// Complete successor field value.
    pub after: Atom<'a>,
}
/// Complete actual delivery staged in an effect or outbox lane.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Delivery<'a> {
    /// Declared lane ordinal.
    pub ordinal: u32,
    /// Original declared channel identifier.
    pub channel: u32,
    /// Complete destination value.
    pub destination: Atom<'a>,
    /// Complete ordered payload fields and values.
    pub payload: Vec<Field<'a>>,
    /// Complete idempotency value.
    pub idempotency: Atom<'a>,
}
/// Actual protected decision construction attempt, including refusals.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Attempt {
    /// Candidate construction permission.
    Candidate(bool),
    /// Original state field and write permission.
    Write(u16, bool),
    /// Outbox flag, original lane ordinal, and emission permission.
    Effect(bool, u32, bool),
}
/// Technical refusal during complete decision construction.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// Original record fields were not canonical.
    InputOrder,
    /// Actual graph code had no declared branch.
    Branch,
    /// Reason declaration did not agree with the decision class.
    Reason,
    /// Rejection attempted to stage a successor or delivery.
    RejectPlan,
    /// The successor assignment family was incomplete or unordered.
    Successor,
    /// An expression referenced an unavailable input or output.
    Reference,
    /// A complete value was outside its declared domain.
    Domain,
    /// Complete delivery declarations were invalid.
    Delivery,
    /// The shared meter refused before protected work.
    Budget(MeterFailure),
}

/// Immutable completed candidate; its constructor is private to library composition.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Candidate<'a> {
    class: Class,
    reason: Option<u32>,
    pre: Vec<Field<'a>>,
    post: Vec<Field<'a>>,
    patch: Vec<Patch<'a>>,
    effects: Vec<Delivery<'a>>,
    outbox: Vec<Delivery<'a>>,
}

/// Complete identity candidate for genuine genesis: an unchanged initial
/// record with no reason, patch or deliveries. Construction is administrative
/// genesis work after the initial laws already passed; it is not metered.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==(Class::Accept,None,fields@,fields@,Seq::<Patch>::empty(),Seq::<Delivery>::empty(),Seq::<Delivery>::empty()),))]
pub(super) fn identity<'a>(fields: Vec<Field<'a>>) -> Candidate<'a> {
    let post = copy_fields(&fields);
    Candidate {
        class: Class::Accept,
        reason: None,
        pre: fields,
        post,
        patch: Vec::new(),
        effects: Vec::new(),
        outbox: Vec::new(),
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a> Candidate<'a> {
    pub closed spec fn view(&self) -> (Class,Option<u32>,Seq<Field<'a>>,Seq<Field<'a>>,Seq<Patch<'a>>,Seq<Delivery<'a>>,Seq<Delivery<'a>>) {
        (self.class,self.reason,self.pre@,self.post@,self.patch@,self.effects@,self.outbox@)
    }
}
}
impl<'a> Candidate<'a> {
    /// Actual business decision class.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().0,))]
    pub fn class(&self) -> Class {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        self.class
    }
    /// Actual original reason identifier, if applicable.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().1,))]
    pub fn reason(&self) -> Option<u32> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        self.reason
    }
    /// Complete owned original state, retained independently of temporary ingress buffers.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().2,))]
    pub fn pre(&self) -> &[Field<'a>] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        &self.pre
    }
    /// Complete successor; empty for a business rejection.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().3,))]
    pub fn post(&self) -> &[Field<'a>] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        &self.post
    }
    /// Complete canonical changed-field patch with original before and after values.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().4,))]
    pub fn patch(&self) -> &[Patch<'a>] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        &self.patch
    }
    /// Complete staged effects in declared lane order.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().5,))]
    pub fn effects(&self) -> &[Delivery<'a>] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        &self.effects
    }
    /// Complete staged outbox deliveries in declared lane order.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().6,))]
    pub fn outbox(&self) -> &[Delivery<'a>] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Candidate::view); }
        &self.outbox
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::equal(a, b),))]
fn equal(a: Atom<'_>, b: Atom<'_>) -> bool {
    match (a, b) {
        (Atom::Bool(a), Atom::Bool(b)) => a == b,
        (Atom::I128(a), Atom::I128(b)) => a == b,
        (Atom::U128(a), Atom::U128(b)) => a == b,
        (
            Atom::Enum {
                type_id: a,
                variant: x,
            },
            Atom::Enum {
                type_id: b,
                variant: y,
            },
        ) => a == b && x == y,
        (
            Atom::Sum {
                type_id: a,
                variant: x,
            },
            Atom::Sum {
                type_id: b,
                variant: y,
            },
        ) => a == b && x == y,
        (Atom::Bytes(a), Atom::Bytes(b)) | (Atom::Text(a), Atom::Text(b)) => {
            super::util::bytes_equal(a, b)
        }
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::ordered(fields@),))]
fn ordered(fields: &[Field<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= fields.len(),
        forall|j:int| 0 < j < i ==> fields@[j-1].id < #[trigger] fields@[j].id,
        decreases fields.len() - i,))]
    while i < fields.len() {
        if i > 0 && fields[i - 1].id >= fields[i].id {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::lookup(fields@, id, fields@.len()),))]
fn lookup<'a>(fields: &[Field<'a>], id: u16) -> Option<Atom<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= fields.len(),
        spec::lookup(fields@,id,i as nat) == None::<Atom>, decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! { spec::lookup_found(fields@,id,(i+1) as nat,fields@.len()); }
            return Some(fields[i].value);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == fields@,))]
fn copy_fields<'a>(fields: &[Field<'a>]) -> Vec<Field<'a>> {
    let mut result = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= fields.len(),
        result@ == fields@.take(i as int), decreases fields.len()-i,))]
    while i < fields.len() {
        result.push(fields[i]);
        i += 1;
    }
    result
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::ordered_value(value),))]
fn ordered_value(value: RootView<'_, '_>) -> bool {
    match value {
        RootView::Leaf(_) => true,
        RootView::Record(fields) => ordered(fields),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::lookup_value(value,id),))]
fn lookup_value<'a>(value: RootView<'_, 'a>, id: u16) -> Option<Atom<'a>> {
    match value {
        RootView::Leaf(_) => None,
        RootView::Record(fields) => lookup(fields, id),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::root_value(value),))]
fn root_value<'a>(value: RootView<'_, 'a>) -> Option<Atom<'a>> {
    match value {
        RootView::Leaf(atom) => Some(atom),
        RootView::Record(_) => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::resolve(inputs, output@, expr),))]
fn resolve<'a>(inputs: Inputs<'_, 'a>, output: &[Atom<'a>], expr: Expr<'a>) -> Option<Atom<'a>> {
    match expr {
        Expr::Input(source, id) => match source {
            Source::State => lookup(inputs.state, id),
            Source::Command => lookup_value(inputs.command, id),
            Source::Context => lookup_value(inputs.context, id),
        },
        Expr::Root(source) => match source {
            Source::State => None,
            Source::Command => root_value(inputs.command),
            Source::Context => root_value(inputs.context),
        },
        Expr::Output(i) => {
            if i < output.len() {
                Some(output[i])
            } else {
                None
            }
        }
        Expr::Constant(value) => Some(value),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::admitted(domain,value),))]
fn admitted(domain: Domain<'_>, value: Atom<'_>) -> bool {
    match (domain, value) {
        (Domain::Bool, Atom::Bool(_))
        | (Domain::Bytes, Atom::Bytes(_))
        | (Domain::Text, Atom::Text(_)) => true,
        (Domain::I128 { min, max }, Atom::I128(v)) => min <= v && v <= max,
        (Domain::U128 { min, max }, Atom::U128(v)) => min <= v && v <= max,
        (
            Domain::Enum { type_id, variants },
            Atom::Enum {
                type_id: t,
                variant,
            },
        )
        | (
            Domain::Sum { type_id, variants },
            Atom::Sum {
                type_id: t,
                variant,
            },
        ) => type_id == t && super::util::contains(variants, variant),
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::select_branch(branches@,code,branches@.len()),
        match result { Some(i) => i < branches.len(), None => true },
))]
fn select_branch(branches: &[Branch<'_>], code: i128) -> Option<usize> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal_with_fuel(spec::select_branch,2); }
    let mut found = None;
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= branches.len(),
        found == spec::select_branch(branches@,code,i as nat),
        match found { Some(j) => j < i, None => true },
        spec::branch_order(branches@,i as nat),
        decreases branches.len()-i,))]
    while i < branches.len() {
        #[cfg(verus_keep_ghost)]
        proof! { reveal_with_fuel(spec::select_branch,2); }
        if i > 0 && branches[i - 1].code >= branches[i].code {
            #[cfg(verus_keep_ghost)]
            proof! { spec::choose_invalid(branches@,code,(i+1) as nat,branches@.len()); }
            return None;
        }
        if branches[i].code == code {
            found = Some(i);
        }
        i += 1;
    }
    found
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::reason_valid(class,reason),
))]
fn reason_valid(class: Class, reason: Option<u32>) -> bool {
    match (class, reason) {
        (Class::Accept, None) => true,
        (Class::Reject, Some(r)) | (Class::CommittedFailure, Some(r)) => r != 0,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Ok(v) => Ok(v@), Err(e) => Err(e) })
        == spec::payload(inputs,output@,fields@,fields@.len()),
))]
fn payload<'a>(
    inputs: Inputs<'_, 'a>,
    output: &[Atom<'a>],
    fields: &[PayloadField<'a>],
) -> Result<Vec<Field<'a>>, Failure> {
    let mut result = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= fields.len(),
        spec::payload(inputs,output@,fields@,i as nat) == Ok(result@),
        decreases fields.len()-i,))]
    while i < fields.len() {
        if i > 0 && fields[i - 1].field >= fields[i].field {
            #[cfg(verus_keep_ghost)]
            proof! { spec::payload_failure(inputs,output@,fields@,(i+1) as nat,fields@.len()); }
            return Err(Failure::Delivery);
        }
        let Some(value) = resolve(inputs, output, fields[i].value) else {
            #[cfg(verus_keep_ghost)]
            proof! { spec::payload_failure(inputs,output@,fields@,(i+1) as nat,fields@.len()); }
            return Err(Failure::Reference);
        };
        result.push(Field {
            id: fields[i].field,
            value,
        });
        i += 1;
    }
    Ok(result)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok((post,patch)) => Ok((post@,patch@)), Err(e) => Err(e) },
            final(meter).used.counters@, final(attempts)@) == {
                let r = spec::assignments(inputs,output@,plan@,plan@.len(),
                    old(meter).limits.counters@,old(meter).used.counters@);
                (r.0,r.1,old(attempts)@ + r.2)
            },
))]
fn assignments<'a>(
    inputs: Inputs<'_, 'a>,
    output: &[Atom<'a>],
    plan: &[Assignment<'a>],
    meter: &mut Meter,
    attempts: &mut Vec<Attempt>,
) -> Result<(Vec<Field<'a>>, Vec<Patch<'a>>), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal_with_fuel(spec::assignments,2); }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial = meter.used.counters@; let ghost prefix = attempts@; }
    if plan.len() != inputs.state.len() {
        return Err(Failure::Successor);
    }
    let mut post = Vec::new();
    let mut patch = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= plan.len(), plan.len() == inputs.state.len(),
        meter.limits == old(meter).limits,
        initial == old(meter).used.counters@, initial.len() == 8, prefix == old(attempts)@,
        spec::assignments(inputs,output@,plan@,i as nat,meter.limits.counters@,initial).0 == Ok((post@,patch@)),
        spec::assignments(inputs,output@,plan@,i as nat,meter.limits.counters@,initial).1 == meter.used.counters@,
        attempts@ == prefix + spec::assignments(inputs,output@,plan@,i as nat,meter.limits.counters@,initial).2,
        decreases plan.len()-i,))]
    while i < plan.len() {
        #[cfg(verus_keep_ghost)]
        proof! { reveal_with_fuel(spec::assignments,2); }
        if plan[i].field != inputs.state[i].id {
            #[cfg(verus_keep_ghost)]
            proof! { spec::assignments_failure(inputs,output@,plan@,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
            return Err(Failure::Successor);
        }
        let charge = meter.charge(Resource::Write, 1);
        attempts.push(Attempt::Write(plan[i].field, charge.is_ok()));
        if let Err(e) = charge {
            #[cfg(verus_keep_ghost)]
            proof! { spec::assignments_failure(inputs,output@,plan@,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
            return Err(Failure::Budget(e));
        }
        let Some(value) = resolve(inputs, output, plan[i].value) else {
            #[cfg(verus_keep_ghost)]
            proof! { spec::assignments_failure(inputs,output@,plan@,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
            return Err(Failure::Reference);
        };
        if !admitted(plan[i].domain, inputs.state[i].value) || !admitted(plan[i].domain, value) {
            #[cfg(verus_keep_ghost)]
            proof! { spec::assignments_failure(inputs,output@,plan@,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
            return Err(Failure::Domain);
        }
        post.push(Field {
            id: plan[i].field,
            value,
        });
        if !equal(inputs.state[i].value, value) {
            patch.push(Patch {
                field: plan[i].field,
                before: inputs.state[i].value,
                after: value,
            });
        }
        i += 1;
    }
    Ok((post, patch))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(v) => Ok(v@.map(|_:int,d:Delivery| spec::delivery_view(d))), Err(e) => Err(e) },
            final(meter).used.counters@, final(attempts)@) == {
                let r = spec::deliveries(inputs,output@,plan@,outbox,plan@.len(),
                    old(meter).limits.counters@,old(meter).used.counters@);
                (r.0,r.1,old(attempts)@ + r.2)
            },
))]
fn deliveries<'a>(
    inputs: Inputs<'_, 'a>,
    output: &[Atom<'a>],
    plan: &[DeliveryPlan<'a>],
    outbox: bool,
    meter: &mut Meter,
    attempts: &mut Vec<Attempt>,
) -> Result<Vec<Delivery<'a>>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal_with_fuel(spec::deliveries,2); }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial = meter.used.counters@; let ghost prefix = attempts@; }
    let mut result = Vec::new();
    let mut i = 0usize;
    #[cfg(verus_keep_ghost)]
    proof! { assert(result@.map(|_:int,d:Delivery| spec::delivery_view(d)) =~= Seq::empty()); }
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= plan.len(), meter.limits == old(meter).limits,
        initial == old(meter).used.counters@, initial.len() == 8, prefix == old(attempts)@,
        spec::deliveries(inputs,output@,plan@,outbox,i as nat,meter.limits.counters@,initial).0 == Ok(result@.map(|_:int,d:Delivery| spec::delivery_view(d))),
        spec::deliveries(inputs,output@,plan@,outbox,i as nat,meter.limits.counters@,initial).1 == meter.used.counters@,
        attempts@ == prefix + spec::deliveries(inputs,output@,plan@,outbox,i as nat,meter.limits.counters@,initial).2,
        decreases plan.len()-i,))]
    while i < plan.len() {
        #[cfg(verus_keep_ghost)]
        proof! { reveal_with_fuel(spec::deliveries,2); }
        let entry = plan[i];
        if entry.channel == 0 || (i > 0 && plan[i - 1].ordinal >= entry.ordinal) {
            #[cfg(verus_keep_ghost)]
            proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
            return Err(Failure::Delivery);
        }
        let enabled = match resolve(inputs, output, entry.when) {
            Some(Atom::Bool(b)) => b,
            _ => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
                return Err(Failure::Domain);
            }
        };
        if enabled {
            let charge = meter.charge(Resource::Effect, 1);
            attempts.push(Attempt::Effect(outbox, entry.ordinal, charge.is_ok()));
            if let Err(e) = charge {
                #[cfg(verus_keep_ghost)]
                proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
                return Err(Failure::Budget(e));
            }
            let Some(destination) = resolve(inputs, output, entry.destination) else {
                #[cfg(verus_keep_ghost)]
                proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
                return Err(Failure::Reference);
            };
            let Some(idempotency) = resolve(inputs, output, entry.idempotency) else {
                #[cfg(verus_keep_ghost)]
                proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
                return Err(Failure::Reference);
            };
            let fields = match payload(inputs, output, entry.payload) {
                Ok(v) => v,
                Err(e) => {
                    #[cfg(verus_keep_ghost)]
                    proof! { spec::deliveries_failure(inputs,output@,plan@,outbox,(i+1) as nat,plan@.len(),meter.limits.counters@,initial); }
                    return Err(e);
                }
            };
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost previous_entries = result@; let ghost field_values = fields@; }
            result.push(Delivery {
                ordinal: entry.ordinal,
                channel: entry.channel,
                destination,
                payload: fields,
                idempotency,
            });
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(result@.map(|_:int,d:Delivery| spec::delivery_view(d)) =~=
                    previous_entries.map(|_:int,d:Delivery| spec::delivery_view(d)).push(
                        (entry.ordinal,entry.channel,destination,field_values,idempotency)));
            }
        }
        i += 1;
    }
    Ok(result)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(c) => Ok(spec::candidate_view(c)), Err(e) => Err(e) },
            final(meter).used.counters@,final(attempts)@) == {
                let r = spec::construct(inputs,output@,code,branches@,
                    old(meter).limits.counters@,old(meter).used.counters@);
                (r.0,r.1,old(attempts)@+r.2)
            },
))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(30))]
pub(super) fn construct<'a>(
    inputs: Inputs<'_, 'a>,
    output: &[Atom<'a>],
    code: i128,
    branches: &[Branch<'a>],
    meter: &mut Meter,
    attempts: &mut Vec<Attempt>,
) -> Result<Candidate<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Candidate::view); }
    if !ordered(inputs.state) || !ordered_value(inputs.command) || !ordered_value(inputs.context) {
        return Err(Failure::InputOrder);
    }
    let charge = meter.charge(Resource::Candidate, 1);
    attempts.push(Attempt::Candidate(charge.is_ok()));
    if let Err(e) = charge {
        return Err(Failure::Budget(e));
    }
    let Some(index) = select_branch(branches, code) else {
        return Err(Failure::Branch);
    };
    let branch = branches[index];
    if !reason_valid(branch.class, branch.reason) {
        return Err(Failure::Reason);
    }
    if matches!(branch.class, Class::Reject) {
        if !branch.assignments.is_empty() || !branch.effects.is_empty() || !branch.outbox.is_empty()
        {
            return Err(Failure::RejectPlan);
        }
        let effects = Vec::new();
        let outbox = Vec::new();
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(effects@.map(|_:int,d:Delivery| spec::delivery_view(d)) =~= Seq::empty());
            assert(outbox@.map(|_:int,d:Delivery| spec::delivery_view(d)) =~= Seq::empty());
        }
        return Ok(Candidate {
            class: branch.class,
            reason: branch.reason,
            pre: copy_fields(inputs.state),
            post: Vec::new(),
            patch: Vec::new(),
            effects,
            outbox,
        });
    }
    let (post, patch) = assignments(inputs, output, branch.assignments, meter, attempts)?;
    let effects = deliveries(inputs, output, branch.effects, false, meter, attempts)?;
    let outbox = deliveries(inputs, output, branch.outbox, true, meter, attempts)?;
    Ok(Candidate {
        class: branch.class,
        reason: branch.reason,
        pre: copy_fields(inputs.state),
        post,
        patch,
        effects,
        outbox,
    })
}

#[cfg(test)]
mod tests;
