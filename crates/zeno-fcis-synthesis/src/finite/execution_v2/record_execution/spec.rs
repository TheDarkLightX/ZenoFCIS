//! Complete metadata admission and exact source/field ABI binding.

use super::super::input_view::{self, AccessAttempt, Field};
use super::{Binding, Decoded, Domain, Failure, Invocation, ScalarProgram, Source};
use vstd::prelude::*;

verus! {
pub open spec fn fields(invocation: &Invocation, source: Source) -> Seq<Field> {
    match source {
        Source::State => invocation.state.fields@,
        Source::Command => invocation.command.fields@,
        Source::Context => invocation.context.fields@,
    }
}
pub open spec fn find_field(descriptor: Seq<Field>, id: u16, count: nat) -> Option<usize>
    recommends count <= descriptor.len(),
    decreases count,
{
    if count == 0 { None }
    else {
        match find_field(descriptor, id, (count - 1) as nat) {
            Some(index) => Some(index),
            None => if descriptor[count as int - 1].id == id {
                Some((count - 1) as usize)
            } else { None },
        }
    }
}
pub proof fn find_persists(descriptor: Seq<Field>, id: u16, found: nat, count: nat, index: usize)
    requires found <= count <= descriptor.len(),
        find_field(descriptor, id, found) == Some(index),
    ensures find_field(descriptor, id, count) == Some(index),
    decreases count - found,
{
    if count > found {
        find_persists(descriptor, id, found, (count - 1) as nat, index);
    }
}
pub open spec fn bound_domain(invocation: &Invocation, binding: Binding) -> Option<Domain> {
    let descriptor = fields(invocation, binding.source);
    match find_field(descriptor, binding.field, descriptor.len()) {
        Some(index) => Some(input_view::leaf_domain(descriptor[index as int].leaf)),
        None => None,
    }
}
pub open spec fn has_pair(bindings: Seq<Binding>, source: Source, id: u16, count: nat) -> bool
    recommends count <= bindings.len(),
    decreases count,
{
    count > 0 && (has_pair(bindings, source, id, (count - 1) as nat)
        || (bindings[count as int - 1].source == source && bindings[count as int - 1].field == id))
}
pub proof fn pair_persists(bindings: Seq<Binding>, source: Source, id: u16, found: nat, count: nat)
    requires found <= count <= bindings.len(), has_pair(bindings, source, id, found),
    ensures has_pair(bindings, source, id, count),
    decreases count - found,
{
    if count > found { pair_persists(bindings, source, id, found, (count - 1) as nat); }
}
pub open spec fn covered(descriptor: Seq<Field>, source: Source, bindings: Seq<Binding>, count: nat)
    -> bool
    recommends count <= descriptor.len(),
    decreases count,
{
    count == 0 || (covered(descriptor, source, bindings, (count - 1) as nat)
        && has_pair(bindings, source, descriptor[count as int - 1].id, bindings.len()))
}
pub proof fn coverage_failure(descriptor: Seq<Field>, source: Source, bindings: Seq<Binding>,
    failed: nat, count: nat)
    requires failed <= count <= descriptor.len(), !covered(descriptor, source, bindings, failed),
    ensures !covered(descriptor, source, bindings, count),
    decreases count - failed,
{
    if count > failed { coverage_failure(descriptor, source, bindings, failed, (count - 1) as nat); }
}
pub open spec fn binding_prefix(invocation: &Invocation, domains: Seq<Domain>,
    bindings: Seq<Binding>, count: nat) -> bool
    recommends count <= bindings.len(), count <= domains.len(),
    decreases count,
{
    if count == 0 { true }
    else {
        let binding = bindings[count as int - 1];
        binding_prefix(invocation, domains, bindings, (count - 1) as nat)
            && bound_domain(invocation, binding) == Some(domains[count as int - 1])
            && !has_pair(bindings, binding.source, binding.field, (count - 1) as nat)
    }
}
pub proof fn binding_failure(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>,
    failed: nat, count: nat)
    requires failed <= count <= bindings.len(), count <= domains.len(),
        !binding_prefix(invocation, domains, bindings, failed),
    ensures !binding_prefix(invocation, domains, bindings, count),
    decreases count - failed,
{
    if count > failed { binding_failure(invocation, domains, bindings, failed, (count - 1) as nat); }
}
pub open spec fn valid_bindings(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>)
    -> bool {
    domains.len() == bindings.len()
        && binding_prefix(invocation, domains, bindings, bindings.len())
        && covered(invocation.state.fields@, Source::State, bindings, invocation.state.fields@.len())
        && covered(invocation.command.fields@, Source::Command, bindings, invocation.command.fields@.len())
        && covered(invocation.context.fields@, Source::Context, bindings, invocation.context.fields@.len())
}
pub open spec fn metadata(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>)
    -> Result<(), Failure> {
    if !input_view::schema_valid(invocation.state.fields@) { Err(Failure::Schema(Source::State)) }
    else if !input_view::schema_valid(invocation.command.fields@) { Err(Failure::Schema(Source::Command)) }
    else if !input_view::schema_valid(invocation.context.fields@) { Err(Failure::Schema(Source::Context)) }
    else if !valid_bindings(invocation, domains, bindings) { Err(Failure::Binding) }
    else { Ok(()) }
}
pub open spec fn select_attempts(reports: (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>),
    source: Source) -> Seq<AccessAttempt> {
    match source { Source::State => reports.0, Source::Command => reports.1, Source::Context => reports.2 }
}
pub open spec fn append_attempts(before: (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>),
    after: (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>))
    -> (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>) {
    (before.0 + after.0, before.1 + after.1, before.2 + after.2)
}
pub(super) open spec fn values(decoded: &Decoded, source: Source) -> Seq<i64> {
    match source { Source::State => decoded.state@, Source::Command => decoded.command@,
        Source::Context => decoded.context@ }
}
pub(super) open spec fn decoded(decoded: &Decoded) -> (Seq<i64>, Seq<i64>, Seq<i64>) {
    (decoded.state@, decoded.command@, decoded.context@)
}
pub open spec fn select_values(decoded: (Seq<i64>, Seq<i64>, Seq<i64>), source: Source) -> Seq<i64> {
    match source { Source::State => decoded.0, Source::Command => decoded.1, Source::Context => decoded.2 }
}
pub open spec fn bound_scalar(invocation: &Invocation, decoded: (Seq<i64>, Seq<i64>, Seq<i64>),
    binding: Binding) -> Option<i64> {
    let descriptor = fields(invocation, binding.source);
    let scalars = select_values(decoded, binding.source);
    match find_field(descriptor, binding.field, descriptor.len()) {
        Some(index) => if (index as int) < scalars.len() { Some(scalars[index as int]) } else { None },
        None => None,
    }
}
pub open spec fn tuple_prefix(invocation: &Invocation, decoded: (Seq<i64>, Seq<i64>, Seq<i64>),
    bindings: Seq<Binding>, count: nat) -> Option<Seq<i64>>
    recommends count <= bindings.len(),
    decreases count,
{
    if count == 0 { Some(Seq::empty()) }
    else {
        match tuple_prefix(invocation, decoded, bindings, (count - 1) as nat) {
            None => None,
            Some(previous) => match bound_scalar(invocation, decoded, bindings[count as int - 1]) {
                None => None, Some(value) => Some(previous.push(value)),
            },
        }
    }
}
pub proof fn tuple_failure(invocation: &Invocation, decoded: (Seq<i64>, Seq<i64>, Seq<i64>),
    bindings: Seq<Binding>, failed: nat, count: nat)
    requires failed <= count <= bindings.len(), tuple_prefix(invocation, decoded, bindings, failed) == None,
    ensures tuple_prefix(invocation, decoded, bindings, count) == None,
    decreases count - failed,
{
    if count > failed { tuple_failure(invocation, decoded, bindings, failed, (count - 1) as nat); }
}
pub open spec fn records(invocation: &Invocation, limits: Seq<u64>, used: Seq<u64>)
    -> (Result<(Seq<i64>, Seq<i64>, Seq<i64>), Failure>, Seq<u64>,
        (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>)) {
    let state = input_view::projection(invocation.state.bytes@, invocation.state.fields@, limits, used);
    match state.0 {
        Err(refusal) => (Err(Failure::Record { source: Source::State, refusal }), state.1,
            (state.2, Seq::empty(), Seq::empty())),
        Ok(state_values) => {
            let command = input_view::projection(invocation.command.bytes@, invocation.command.fields@,
                limits, state.1);
            match command.0 {
                Err(refusal) => (Err(Failure::Record { source: Source::Command, refusal }), command.1,
                    (state.2, command.2, Seq::empty())),
                Ok(command_values) => {
                    let context = input_view::projection(invocation.context.bytes@, invocation.context.fields@,
                        limits, command.1);
                    let result = match context.0 {
                        Err(refusal) => Err(Failure::Record { source: Source::Context, refusal }),
                        Ok(context_values) => Ok((state_values, command_values, context_values)),
                    };
                    (result, context.1, (state.2, command.2, context.2))
                },
            }
        },
    }
}
pub open spec fn execution(invocation: &Invocation, program: &ScalarProgram, bindings: Seq<Binding>,
    limits: Seq<u64>, used: Seq<u64>)
    -> (Result<Seq<i64>, Failure>, Seq<u64>,
        (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>)) {
    match metadata(invocation, program.inputs@, bindings) {
        Err(error) => (Err(error), used, (Seq::empty(), Seq::empty(), Seq::empty())),
        Ok(()) => {
            let projected = records(invocation, limits, used);
            match projected.0 {
                Err(error) => (Err(error), projected.1, projected.2),
                Ok(decoded) => match tuple_prefix(invocation, decoded, bindings, bindings.len()) {
                    None => (Err(Failure::Binding), projected.1, projected.2),
                    Some(input) => {
                        let evaluated = super::super::execution(program.inputs@, program.outputs@,
                            program.nodes@, program.roots@, input, limits, projected.1);
                        let result = match evaluated.0 {
                            Err(error) => Err(Failure::Execution(error)), Ok(output) => Ok(output),
                        };
                        (result, evaluated.1, projected.2)
                    },
                },
            }
        },
    }
}
pub proof fn find_has_identity(descriptor: Seq<Field>, id: u16, count: nat, index: usize)
    requires count <= descriptor.len(), count <= usize::MAX,
        find_field(descriptor, id, count) == Some(index),
    ensures index < count, descriptor[index as int].id == id,
    decreases count,
{
    if count > 0 {
        match find_field(descriptor, id, (count - 1) as nat) {
            Some(previous) => find_has_identity(descriptor, id, (count - 1) as nat, previous),
            None => {},
        }
    }
}
pub proof fn binding_at(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>,
    count: nat, index: nat)
    requires index < count <= bindings.len(), count <= domains.len(),
        binding_prefix(invocation, domains, bindings, count),
    ensures bound_domain(invocation, bindings[index as int]) == Some(domains[index as int]),
    decreases count - index,
{
    if index < count - 1 { binding_at(invocation, domains, bindings, (count - 1) as nat, index); }
}
pub open spec fn typed_values(descriptor: Seq<Field>, values: Seq<i64>) -> bool {
    descriptor.len() == values.len()
        && forall|i: int| 0 <= i < descriptor.len() ==>
            super::super::super::evaluation::spec::contains(input_view::leaf_domain(descriptor[i].leaf), values[i])
}
pub open spec fn typed_records(invocation: &Invocation, decoded: (Seq<i64>, Seq<i64>, Seq<i64>)) -> bool {
    typed_values(invocation.state.fields@, decoded.0)
        && typed_values(invocation.command.fields@, decoded.1)
        && typed_values(invocation.context.fields@, decoded.2)
}
pub proof fn bound_scalar_is_typed(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>,
    decoded: (Seq<i64>, Seq<i64>, Seq<i64>), count: nat, position: nat)
    requires position < count <= bindings.len(), count <= domains.len(),
        binding_prefix(invocation, domains, bindings, count), typed_records(invocation, decoded),
    ensures match bound_scalar(invocation, decoded, bindings[position as int]) {
        None => false,
        Some(value) => super::super::super::evaluation::spec::contains(domains[position as int], value),
    },
{
    binding_at(invocation, domains, bindings, count, position);
    let binding = bindings[position as int];
    let descriptor = fields(invocation, binding.source);
    let scalars = select_values(decoded, binding.source);
    assert(typed_values(descriptor, scalars)) by { match binding.source { Source::State => {},
        Source::Command => {}, Source::Context => {} } }
    match binding.source {
        Source::State => vstd::slice::axiom_spec_len(invocation.state.fields),
        Source::Command => vstd::slice::axiom_spec_len(invocation.command.fields),
        Source::Context => vstd::slice::axiom_spec_len(invocation.context.fields),
    }
    assert(descriptor.len() <= usize::MAX);
    match find_field(descriptor, binding.field, descriptor.len()) {
        None => {},
        Some(index) => {
            find_has_identity(descriptor, binding.field, descriptor.len(), index);
            assert(super::super::super::evaluation::spec::contains(
                input_view::leaf_domain(descriptor[index as int].leaf), scalars[index as int]));
        },
    }
}
pub proof fn tuple_is_typed(invocation: &Invocation, domains: Seq<Domain>, bindings: Seq<Binding>,
    decoded: (Seq<i64>, Seq<i64>, Seq<i64>), count: nat)
    requires count <= bindings.len(), count <= domains.len(),
        binding_prefix(invocation, domains, bindings, count), typed_records(invocation, decoded),
    ensures match tuple_prefix(invocation, decoded, bindings, count) {
        None => false,
        Some(input) => input.len() == count && forall|i: int| 0 <= i < count ==>
            super::super::super::evaluation::spec::contains(domains[i], input[i]),
    },
    decreases count,
{
    if count > 0 {
        tuple_is_typed(invocation, domains, bindings, decoded, (count - 1) as nat);
        bound_scalar_is_typed(invocation, domains, bindings, decoded, count, (count - 1) as nat);
        match (tuple_prefix(invocation, decoded, bindings, (count - 1) as nat),
            bound_scalar(invocation, decoded, bindings[count as int - 1])) {
            (Some(previous), Some(value)) => {
                assert(tuple_prefix(invocation, decoded, bindings, count) == Some(previous.push(value)));
                assert forall|i: int| 0 <= i < count implies
                    super::super::super::evaluation::spec::contains(domains[i], previous.push(value)[i]) by {
                    if i < count - 1 {
                        assert(super::super::super::evaluation::spec::contains(domains[i], previous[i]));
                    }
                }
            }
            _ => {},
        }
    }
}
}
