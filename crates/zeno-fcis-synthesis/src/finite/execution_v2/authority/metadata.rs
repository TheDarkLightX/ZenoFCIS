//! Complete exact policy metadata projection. Pure encoding grants no authority.
use super::super::super::evaluation::{Domain as ScalarDomain, Op as ScalarOp};
use super::super::{
    InputField, InputLeaf, InputVariant, Limits, Resource, ScalarProgram,
    decision::{Assignment, Branch, Class, DeliveryPlan, Domain, Expr, PayloadField, Source},
    laws,
};
use super::candidate::atom as decision_atom;
use super::canonical::Part;
#[cfg(verus_keep_ghost)]
use super::metadata_spec as model;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::source(*value),))]
pub(super) fn source<'a>(parts: &mut Vec<Part<'a>>, value: &Source) {
    match value {
        Source::State => {
            parts.push(Part::Word(0));
        }
        Source::Command => {
            parts.push(Part::Word(1));
        }
        Source::Context => {
            parts.push(Part::Word(2));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::class(*value),))]
pub(super) fn class<'a>(parts: &mut Vec<Part<'a>>, value: &Class) {
    match value {
        Class::Accept => {
            parts.push(Part::Word(0));
        }
        Class::Reject => {
            parts.push(Part::Word(1));
        }
        Class::CommittedFailure => {
            parts.push(Part::Word(2));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::reason(*value),))]
pub(super) fn reason<'a>(parts: &mut Vec<Part<'a>>, value: &Option<u32>) {
    match value {
        None => parts.push(Part::Word(0)),
        Some(v) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*v as u128));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::variants(values@),))]
pub(super) fn variants<'a>(parts: &mut Vec<Part<'a>>, values: &[InputVariant]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::variants_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        parts.push(Part::Word(values[i].code as i128 as u128));
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::leaf(*value),))]
pub(super) fn leaf<'a>(parts: &mut Vec<Part<'a>>, value: &InputLeaf) {
    match value {
        InputLeaf::I128 { min, max } => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(*min as i128 as u128));
            parts.push(Part::Word(*max as i128 as u128));
        }
        InputLeaf::Bool => parts.push(Part::Word(1)),
        InputLeaf::U128 { min, max } => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(*min));
            parts.push(Part::Word(*max));
        }
        InputLeaf::Enum {
            type_id,
            min,
            max,
            variants: vs,
        } => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(*type_id as u128));
            parts.push(Part::Word(*min as i128 as u128));
            parts.push(Part::Word(*max as i128 as u128));
            variants(parts, vs);
        }
        InputLeaf::Sum {
            type_id,
            min,
            max,
            variants: vs,
        } => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(*type_id as u128));
            parts.push(Part::Word(*min as i128 as u128));
            parts.push(Part::Word(*max as i128 as u128));
            variants(parts, vs);
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::leaves(values@),))]
pub(super) fn leaves<'a>(parts: &mut Vec<Part<'a>>, values: &[InputLeaf]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::leaves_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        leaf(parts, &values[i]);
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::leaves);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::input_fields(values@),))]
pub(super) fn input_fields<'a>(parts: &mut Vec<Part<'a>>, values: &[InputField]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::input_fields_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        leaf(parts, &values[i].leaf);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::ids(values@),))]
pub(super) fn ids<'a>(parts: &mut Vec<Part<'a>>, values: &[u32]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::ids_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i] as u128));
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::ids);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::variants_ids(values@),))]
pub(super) fn variants_ids<'a>(parts: &mut Vec<Part<'a>>, values: &[u16]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::variants_ids_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i] as u128));
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::scalar_domain(*value),))]
pub(super) fn scalar_domain<'a>(parts: &mut Vec<Part<'a>>, value: &ScalarDomain) {
    match value {
        ScalarDomain::Bool => {
            parts.push(Part::Word(0));
        }
        ScalarDomain::Int { min, max } => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*min as i128 as u128));
            parts.push(Part::Word(*max as i128 as u128));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::scalar_domains(values@),))]
pub(super) fn scalar_domains<'a>(parts: &mut Vec<Part<'a>>, values: &[ScalarDomain]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::scalar_domains_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        scalar_domain(parts, &values[i]);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::scalar_op(*value),))]
pub(super) fn scalar_op<'a>(parts: &mut Vec<Part<'a>>, value: &ScalarOp) {
    match value {
        ScalarOp::Input(a) => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(*a as u128));
        }
        ScalarOp::Int(a) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*a as i128 as u128));
        }
        ScalarOp::Bool(a) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(*a as u128));
        }
        ScalarOp::Add(a, b) => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        ScalarOp::Sub(a, b) => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        ScalarOp::Eq(a, b) => {
            parts.push(Part::Word(5));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        ScalarOp::Lt(a, b) => {
            parts.push(Part::Word(6));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        ScalarOp::And(a, b) => {
            parts.push(Part::Word(7));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        ScalarOp::Not(a) => {
            parts.push(Part::Word(8));
            parts.push(Part::Word(*a as u128));
        }
        ScalarOp::Select(c, a, b) => {
            parts.push(Part::Word(9));
            parts.push(Part::Word(*c as u128));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::scalar_ops(values@),))]
pub(super) fn scalar_ops<'a>(parts: &mut Vec<Part<'a>>, values: &[ScalarOp]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::scalar_ops_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        scalar_op(parts, &values[i]);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::roots(values@),))]
pub(super) fn roots<'a>(parts: &mut Vec<Part<'a>>, values: &[u16]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::roots_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i] as u128));
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::program(*value),))]
pub(super) fn program<'a>(parts: &mut Vec<Part<'a>>, value: &ScalarProgram<'a>) {
    scalar_domains(parts, value.inputs);
    scalar_domains(parts, value.outputs);
    scalar_ops(parts, value.nodes);
    roots(parts, value.roots);

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::program);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::domain(*value),))]
pub(super) fn domain<'a>(parts: &mut Vec<Part<'a>>, value: &Domain<'a>) {
    match value {
        Domain::Bool => parts.push(Part::Word(0)),
        Domain::I128 { min, max } => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*min as u128));
            parts.push(Part::Word(*max as u128));
        }
        Domain::U128 { min, max } => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(*min));
            parts.push(Part::Word(*max));
        }
        Domain::Enum { type_id, variants } => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(*type_id as u128));
            variants_ids(parts, variants);
        }
        Domain::Sum { type_id, variants } => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(*type_id as u128));
            variants_ids(parts, variants);
        }
        Domain::Bytes => parts.push(Part::Word(5)),
        Domain::Text => parts.push(Part::Word(6)),
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::expr(*value),))]
pub(super) fn expr<'a>(parts: &mut Vec<Part<'a>>, value: &Expr<'a>) {
    match value {
        Expr::Input(s, id) => {
            parts.push(Part::Word(0));
            source(parts, s);
            parts.push(Part::Word(*id as u128));
        }
        Expr::Output(index) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*index as u128));
        }
        Expr::Constant(v) => {
            parts.push(Part::Word(2));
            decision_atom(parts, *v);
        }
        Expr::Root(s) => {
            parts.push(Part::Word(3));
            source(parts, s);
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::assignments(values@),))]
pub(super) fn assignments<'a>(parts: &mut Vec<Part<'a>>, values: &[Assignment<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::assignments_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].field as u128));
        expr(parts, &values[i].value);
        domain(parts, &values[i].domain);
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::assignments);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::payload(values@),))]
pub(super) fn payload<'a>(parts: &mut Vec<Part<'a>>, values: &[PayloadField<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::payload_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].field as u128));
        expr(parts, &values[i].value);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::deliveries(values@),))]
pub(super) fn deliveries<'a>(parts: &mut Vec<Part<'a>>, values: &[DeliveryPlan<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::deliveries_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].ordinal as u128));
        parts.push(Part::Word(values[i].channel as u128));
        expr(parts, &values[i].when);
        expr(parts, &values[i].destination);
        payload(parts, values[i].payload);
        expr(parts, &values[i].idempotency);
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::deliveries);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::branches(values@),))]
pub(super) fn branches<'a>(parts: &mut Vec<Part<'a>>, values: &[Branch<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::branches_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].code as u128));
        class(parts, &values[i].class);
        reason(parts, &values[i].reason);
        assignments(parts, values[i].assignments);
        deliveries(parts, values[i].effects);
        deliveries(parts, values[i].outbox);
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::branches);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_atom(*value),))]
pub(super) fn law_atom<'a>(parts: &mut Vec<Part<'a>>, value: &laws::Atom<'a>) {
    match value {
        laws::Atom::Bool(v) => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(*v as u128));
        }
        laws::Atom::I128(v) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(*v as u128));
        }
        laws::Atom::U128(v) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(*v));
        }
        laws::Atom::Enum { type_id, variant } => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(*type_id as u128));
            parts.push(Part::Word(*variant as u128));
        }
        laws::Atom::Sum { type_id, variant } => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(*type_id as u128));
            parts.push(Part::Word(*variant as u128));
        }
        laws::Atom::Bytes(v) => {
            parts.push(Part::Word(5));
            parts.push(Part::Bytes(v));
        }
        laws::Atom::Text(v) => {
            parts.push(Part::Word(6));
            parts.push(Part::Bytes(v));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::division(*value),))]
pub(super) fn division<'a>(parts: &mut Vec<Part<'a>>, value: &laws::Division) {
    match value {
        laws::Division::Exact => {
            parts.push(Part::Word(0));
        }
        laws::Division::Floor => {
            parts.push(Part::Word(1));
        }
        laws::Division::Ceil => {
            parts.push(Part::Word(2));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_op(*value),))]
pub(super) fn law_op<'a>(parts: &mut Vec<Part<'a>>, value: &laws::Op<'a>) {
    match value {
        laws::Op::Literal(v) => {
            parts.push(Part::Word(0));
            law_atom(parts, v);
        }
        laws::Op::Observe(o) => {
            parts.push(Part::Word(1));
            super::observations::append_observation(parts, *o);
        }
        laws::Op::ObserveWhen(guard, o, default) => {
            parts.push(Part::Word(12));
            parts.push(Part::Word(*guard as u128));
            super::observations::append_observation(parts, *o);
            law_atom(parts, default);
        }
        laws::Op::Add(a, b) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::Sub(a, b) => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::Mul(a, b) => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::Div(round, a, b) => {
            parts.push(Part::Word(5));
            division(parts, round);
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::ToI128(a) => {
            parts.push(Part::Word(6));
            parts.push(Part::Word(*a as u128));
        }
        laws::Op::Eq(a, b) => {
            parts.push(Part::Word(7));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::Lt(a, b) => {
            parts.push(Part::Word(8));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::And(a, b) => {
            parts.push(Part::Word(9));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
        laws::Op::Not(a) => {
            parts.push(Part::Word(10));
            parts.push(Part::Word(*a as u128));
        }
        laws::Op::Select(c, a, b) => {
            parts.push(Part::Word(11));
            parts.push(Part::Word(*c as u128));
            parts.push(Part::Word(*a as u128));
            parts.push(Part::Word(*b as u128));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_ops(values@),))]
pub(super) fn law_ops<'a>(parts: &mut Vec<Part<'a>>, values: &[laws::Op<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::law_ops_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        law_op(parts, &values[i]);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_kind(*value),))]
pub(super) fn law_kind<'a>(parts: &mut Vec<Part<'a>>, value: &laws::Kind) {
    match value {
        laws::Kind::StateInvariant => {
            parts.push(Part::Word(0));
        }
        laws::Kind::AssetConservation => {
            parts.push(Part::Word(1));
        }
        laws::Kind::MintBurnAuthorization => {
            parts.push(Part::Word(2));
        }
        laws::Kind::DebitCreditEffectEquality => {
            parts.push(Part::Word(3));
        }
        laws::Kind::FeeAndRounding => {
            parts.push(Part::Word(4));
        }
        laws::Kind::AuthoritySubjectRecipient => {
            parts.push(Part::Word(5));
        }
        laws::Kind::RejectNoAuthority => {
            parts.push(Part::Word(6));
        }
        laws::Kind::CommittedFailureEffects => {
            parts.push(Part::Word(7));
        }
        laws::Kind::DecisionConformance => {
            parts.push(Part::Word(8));
        }
        laws::Kind::InitialCondition => {
            parts.push(Part::Word(9));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_scope(*value),))]
pub(super) fn law_scope<'a>(parts: &mut Vec<Part<'a>>, value: &laws::Scope) {
    match value {
        laws::Scope::Always => {
            parts.push(Part::Word(0));
        }
        laws::Scope::Accept => {
            parts.push(Part::Word(1));
        }
        laws::Scope::Reject => {
            parts.push(Part::Word(2));
        }
        laws::Scope::CommittedFailure => {
            parts.push(Part::Word(3));
        }
        laws::Scope::Committing => {
            parts.push(Part::Word(4));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::law_list(values@),))]
pub(super) fn law_list<'a>(parts: &mut Vec<Part<'a>>, values: &[laws::Law<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::law_list_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        law_kind(parts, &values[i].kind);
        law_scope(parts, &values[i].scope);
        parts.push(Part::Word(values[i].genesis as u128));
        law_ops(parts, values[i].program.nodes);
        parts.push(Part::Word(values[i].program.root as u128));
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::law_list);}
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::limits(*value),))]
pub(super) fn limits<'a>(parts: &mut Vec<Part<'a>>, value: &Limits) {
    parts.push(Part::Word(value.limit(Resource::Read) as u128));
    parts.push(Part::Word(value.limit(Resource::Write) as u128));
    parts.push(Part::Word(value.limit(Resource::Candidate) as u128));
    parts.push(Part::Word(value.limit(Resource::Effect) as u128));
    parts.push(Part::Word(value.limit(Resource::Byte) as u128));
    parts.push(Part::Word(value.limit(Resource::WitnessByte) as u128));
    parts.push(Part::Word(value.limit(Resource::Depth) as u128));
    parts.push(Part::Word(value.limit(Resource::Step) as u128));

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::limits);}
}

#[cfg(test)]
mod guarded_tests {
    use super::*;

    fn word(bytes: &mut Vec<u8>, value: u128) {
        bytes.push(0);
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    fn blob(bytes: &mut Vec<u8>, value: &[u8]) {
        bytes.push(1);
        bytes.extend_from_slice(&(value.len() as u128).to_be_bytes());
        bytes.extend_from_slice(value);
    }

    #[test]
    fn guarded_policy_tokens_keep_guard_observation_default_type_value_and_law_metadata() {
        use laws::{Atom, Kind, Law, Observation, Op, Program, Scope};
        let values = [
            (Atom::Bool(false), alloc::vec![0, 0], None),
            (Atom::Bool(true), alloc::vec![0, 1], None),
            (Atom::I128(i128::MIN), alloc::vec![1, 1u128 << 127], None),
            (
                Atom::I128(i128::MAX),
                alloc::vec![1, (1u128 << 127) - 1],
                None,
            ),
            (Atom::U128(u128::MAX), alloc::vec![2, u128::MAX], None),
            (
                Atom::Enum {
                    type_id: u32::MAX,
                    variant: u16::MAX,
                },
                alloc::vec![3, u32::MAX as u128, u16::MAX as u128],
                None,
            ),
            (
                Atom::Sum {
                    type_id: 0,
                    variant: 0,
                },
                alloc::vec![4, 0, 0],
                None,
            ),
            (
                Atom::Bytes(&[0, 128, 255]),
                alloc::vec![5],
                Some(&[0, 128, 255][..]),
            ),
            (Atom::Bytes(&[]), alloc::vec![5], Some(&[][..])),
            (
                Atom::Text(b"\0\x7ftext"),
                alloc::vec![6],
                Some(&b"\0\x7ftext"[..]),
            ),
            (Atom::Text(b""), alloc::vec![6], Some(&b""[..])),
        ];
        let kinds = [
            Kind::StateInvariant,
            Kind::AssetConservation,
            Kind::MintBurnAuthorization,
            Kind::DebitCreditEffectEquality,
            Kind::FeeAndRounding,
            Kind::AuthoritySubjectRecipient,
            Kind::RejectNoAuthority,
            Kind::CommittedFailureEffects,
            Kind::DecisionConformance,
            Kind::InitialCondition,
        ];
        let scopes = [
            Scope::Always,
            Scope::Accept,
            Scope::Reject,
            Scope::CommittedFailure,
            Scope::Committing,
        ];
        for (default, expected_words, expected_blob) in values {
            for guard in [0, 255, u32::MAX as usize, usize::MAX] {
                for (kind_id, kind) in kinds.into_iter().enumerate() {
                    for (scope_id, scope) in scopes.into_iter().enumerate() {
                        for genesis in [false, true] {
                            let nodes = [
                                Op::Literal(Atom::Bool(false)),
                                Op::ObserveWhen(
                                    guard,
                                    Observation::OutboxPayload(usize::MAX, u16::MAX),
                                    default,
                                ),
                            ];
                            let definitions = [Law {
                                id: u32::MAX,
                                kind,
                                scope,
                                genesis,
                                program: Program {
                                    nodes: &nodes,
                                    root: 1,
                                },
                            }];
                            let mut actual = alloc::vec![Part::Word(255)];
                            law_list(&mut actual, &definitions);
                            let mut expected = Vec::new();
                            for value in [
                                255,
                                1,
                                u32::MAX as u128,
                                kind_id as u128,
                                scope_id as u128,
                                u128::from(genesis),
                                2,
                                0,
                                0,
                                0,
                                12,
                                guard as u128,
                                28,
                                usize::MAX as u128,
                                u16::MAX as u128,
                            ] {
                                word(&mut expected, value);
                            }
                            for value in &expected_words {
                                word(&mut expected, *value);
                            }
                            if let Some(bytes) = expected_blob {
                                blob(&mut expected, bytes);
                            }
                            word(&mut expected, 1);
                            assert_eq!(super::super::canonical::encode(&actual), Some(expected));
                        }
                    }
                }
            }
        }
    }
}
