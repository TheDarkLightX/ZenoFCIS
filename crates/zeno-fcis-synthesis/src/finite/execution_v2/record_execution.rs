//! Original raw records and complete source/field bindings for V2 execution.

use super::super::evaluation::{Domain, Op};
use super::input_view::{self, AccessAttempt, Field};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
mod spec;

/// The original invocation record containing one declared field.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// Original pre-state record.
    State,
    /// Original command record.
    Command,
    /// Original context record; authentication remains an admission obligation.
    Context,
}

/// One program-input position's exact source and stable field identifier.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The invocation record to use.
    pub source: Source,
    /// Exact field identifier within that record, including generic ID zero.
    pub field: u16,
}

/// Original canonical bytes and the complete closed descriptor of one record.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct RawRecord<'a> {
    /// Original record slice; this API does not re-encode a Value envelope.
    pub bytes: &'a [u8],
    /// Complete canonical field descriptors, validated before ingress.
    pub fields: &'a [Field],
}

/// Three original record slices and descriptors, before authority admission.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Invocation<'a> {
    /// Original state record.
    pub state: RawRecord<'a>,
    /// Original command record.
    pub command: RawRecord<'a>,
    /// Original context record.
    pub context: RawRecord<'a>,
}

/// Borrowed scalar graph; this entry handles malformed graphs defensively.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct ScalarProgram<'a> {
    /// Exact scalar domains in declared ABI order.
    pub inputs: &'a [Domain],
    /// Exact result domains in root order.
    pub outputs: &'a [Domain],
    /// Eager instruction sequence, including unused and unselected nodes.
    pub nodes: &'a [Op],
    /// Result references, in declared order.
    pub roots: &'a [u16],
}

/// Refusal by metadata, a source-tagged record, or eager execution.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// First invalid descriptor in fixed State/Command/Context order.
    Schema(Source),
    /// Incomplete, duplicate, unknown or domain-mismatched ABI bindings.
    Binding,
    /// The exact record and its decoder or logical charge refusal.
    Record {
        /// Invocation record whose projection refused.
        source: Source,
        /// Exact protected-record refusal, including retained budget details.
        refusal: input_view::Failure,
    },
    /// Exact scalar or Step refusal after all records were projected.
    Execution(super::Failure),
}

/// Library-computed source-tagged logical request sequences.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Attempts {
    state: Vec<AccessAttempt>,
    command: Vec<AccessAttempt>,
    context: Vec<AccessAttempt>,
}

impl Attempts {
    /// Reads one source's retained requests, including a denied Read.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == spec::select_attempts(self.view(), source),
    ))]
    pub fn for_source(&self, source: Source) -> &[AccessAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Attempts::view); }
        match source {
            Source::State => &self.state,
            Source::Command => &self.command,
            Source::Context => &self.context,
        }
    }
}

/// Complete graph output and owned accounting; this is not authorization.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Outcome {
    result: Result<Vec<i64>, Failure>,
    usage: super::Usage,
    attempts: Attempts,
}

impl Outcome {
    /// Reads retained logical usage without permitting replacement.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.view() == self.view().1,
    ))]
    pub fn usage(&self) -> super::Usage {
        self.usage
    }

    /// Reads retained descriptor requests for the specified source.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == spec::select_attempts(self.view().2, source),
    ))]
    pub fn attempts(&self, source: Source) -> &[AccessAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Outcome::view); reveal(Attempts::view); }
        self.attempts.for_source(source)
    }

    /// Consumes the result together with its retained usage and source reports.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures (match result.0 { Ok(values) => Ok(values@), Err(error) => Err(error) },
            result.1.view(), result.2.view()) == self.view(),
    ))]
    pub fn into_parts(self) -> (Result<Vec<i64>, Failure>, super::Usage, Attempts) {
        (self.result, self.usage, self.attempts)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
struct Decoded {
    state: Vec<i64>,
    command: Vec<i64>,
    context: Vec<i64>,
}

#[cfg(verus_keep_ghost)]
verus! {
impl Attempts {
    pub closed spec fn view(&self) -> (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>) {
        (self.state@, self.command@, self.context@)
    }
}
impl Outcome {
    pub closed spec fn view(&self) -> (Result<Seq<i64>, Failure>, Seq<u64>,
        (Seq<AccessAttempt>, Seq<AccessAttempt>, Seq<AccessAttempt>)) {
        (match self.result { Ok(values) => Ok(values@), Err(error) => Err(error) },
            self.usage.view(), self.attempts.view())
    }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == spec::fields(invocation, source),
))]
fn fields<'a>(invocation: &Invocation<'a>, source: Source) -> &'a [Field] {
    match source {
        Source::State => invocation.state.fields,
        Source::Command => invocation.command.fields,
        Source::Context => invocation.context.fields,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (left == right),
))]
fn same_domain(left: Domain, right: Domain) -> bool {
    match (left, right) {
        (Domain::Bool, Domain::Bool) => true,
        (Domain::Int { min: a, max: b }, Domain::Int { min: c, max: d }) => a == c && b == d,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (left == right),
))]
fn same_source(left: Source, right: Source) -> bool {
    matches!(
        (left, right),
        (Source::State, Source::State)
            | (Source::Command, Source::Command)
            | (Source::Context, Source::Context)
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::find_field(descriptor@, id, descriptor@.len()),
        match result { Some(index) => index < descriptor@.len(), None => true },
))]
fn find_field(descriptor: &[Field], id: u16) -> Option<usize> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= descriptor.len(),
            spec::find_field(descriptor@, id, index as nat) == None::<usize>,
        decreases descriptor.len() - index,
    ))]
    while index < descriptor.len() {
        if descriptor[index].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::find_persists(descriptor@, id, (index + 1) as nat,
                    descriptor@.len() as nat, index);
            }
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::bound_domain(invocation, binding),
))]
fn bound_domain(invocation: &Invocation<'_>, binding: Binding) -> Option<Domain> {
    let descriptor = fields(invocation, binding.source);
    let index = find_field(descriptor, binding.field)?;
    Some(input_view::scalar_domain(&descriptor[index].leaf))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::has_pair(bindings@, source, id, bindings@.len()),
))]
fn has_pair(bindings: &[Binding], source: Source, id: u16) -> bool {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bindings.len(),
            !spec::has_pair(bindings@, source, id, index as nat),
        decreases bindings.len() - index,
    ))]
    while index < bindings.len() {
        if same_source(bindings[index].source, source) && bindings[index].field == id {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::pair_persists(bindings@, source, id, (index + 1) as nat,
                    bindings@.len() as nat);
            }
            return true;
        }
        index += 1;
    }
    false
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::covered(descriptor@, source, bindings@, descriptor@.len()),
))]
fn covered(descriptor: &[Field], source: Source, bindings: &[Binding]) -> bool {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= descriptor.len(),
            spec::covered(descriptor@, source, bindings@, index as nat),
        decreases descriptor.len() - index,
    ))]
    while index < descriptor.len() {
        if !has_pair(bindings, source, descriptor[index].id) {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::coverage_failure(descriptor@, source, bindings@, (index + 1) as nat,
                    descriptor@.len() as nat);
            }
            return false;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::valid_bindings(invocation, domains@, bindings@),
))]
fn validate_bindings(
    invocation: &Invocation<'_>,
    domains: &[Domain],
    bindings: &[Binding],
) -> bool {
    if domains.len() != bindings.len() {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bindings.len(), domains.len() == bindings.len(),
            spec::binding_prefix(invocation, domains@, bindings@, index as nat),
        decreases bindings.len() - index,
    ))]
    while index < bindings.len() {
        let binding = bindings[index];
        let Some(actual) = bound_domain(invocation, binding) else {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::binding_failure(invocation, domains@, bindings@, (index + 1) as nat,
                    bindings@.len() as nat);
            }
            return false;
        };
        if !same_domain(actual, domains[index]) {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::binding_failure(invocation, domains@, bindings@, (index + 1) as nat,
                    bindings@.len() as nat);
            }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior <= index < bindings.len(), domains.len() == bindings.len(),
                binding == bindings@[index as int],
                spec::bound_domain(invocation, binding) == Some(domains@[index as int]),
                spec::binding_prefix(invocation, domains@, bindings@, index as nat),
                !spec::has_pair(bindings@, binding.source, binding.field, prior as nat),
            decreases index - prior,
        ))]
        while prior < index {
            if same_source(bindings[prior].source, binding.source)
                && bindings[prior].field == binding.field
            {
                #[cfg(verus_keep_ghost)]
                proof! {
                    spec::pair_persists(bindings@, binding.source, binding.field,
                        (prior + 1) as nat, index as nat);
                    spec::binding_failure(invocation, domains@, bindings@, (index + 1) as nat,
                        bindings@.len() as nat);
                }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    covered(invocation.state.fields, Source::State, bindings)
        && covered(invocation.command.fields, Source::Command, bindings)
        && covered(invocation.context.fields, Source::Context, bindings)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::metadata(invocation, domains@, bindings@),
))]
fn metadata(
    invocation: &Invocation<'_>,
    domains: &[Domain],
    bindings: &[Binding],
) -> Result<(), Failure> {
    if !input_view::validate_schema(invocation.state.fields) {
        return Err(Failure::Schema(Source::State));
    }
    if !input_view::validate_schema(invocation.command.fields) {
        return Err(Failure::Schema(Source::Command));
    }
    if !input_view::validate_schema(invocation.context.fields) {
        return Err(Failure::Schema(Source::Context));
    }
    if !validate_bindings(invocation, domains, bindings) {
        return Err(Failure::Binding);
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == spec::values(decoded, source),
))]
fn values(decoded: &Decoded, source: Source) -> &[i64] {
    match source {
        Source::State => &decoded.state,
        Source::Command => &decoded.command,
        Source::Context => &decoded.context,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::bound_scalar(invocation, spec::decoded(decoded), binding),
))]
fn bound_scalar(invocation: &Invocation<'_>, decoded: &Decoded, binding: Binding) -> Option<i64> {
    let index = find_field(fields(invocation, binding.source), binding.field)?;
    let scalars = values(decoded, binding.source);
    if index < scalars.len() {
        Some(scalars[index])
    } else {
        None
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Ok(()) => Some(final(output)@), Err(()) => None::<Seq<i64>> })
        == spec::tuple_prefix(invocation, spec::decoded(decoded), bindings@, bindings@.len()),
        result.is_err() ==> final(output)@ == Seq::<i64>::empty(),
))]
fn project_tuple(
    invocation: &Invocation<'_>,
    decoded: &Decoded,
    bindings: &[Binding],
    output: &mut Vec<i64>,
) -> Result<(), ()> {
    output.clear();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bindings.len(),
            spec::tuple_prefix(invocation, spec::decoded(decoded), bindings@, index as nat) == Some(output@),
        decreases bindings.len() - index,
    ))]
    while index < bindings.len() {
        let Some(value) = bound_scalar(invocation, decoded, bindings[index]) else {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::tuple_failure(invocation, spec::decoded(decoded), bindings@, (index + 1) as nat,
                    bindings@.len() as nat);
            }
            output.clear();
            return Err(());
        };
        output.push(value);
        index += 1;
    }
    Ok(())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@, final(attempts).view()) == {
                let finished = spec::execution(invocation, program, bindings@,
                    old(meter).limits.counters@, old(meter).used.counters@);
                (finished.0, finished.1, spec::append_attempts(old(attempts).view(), finished.2))
            },
        result.is_err() ==> final(output)@ == Seq::<i64>::empty(),
))]
pub(super) fn execute_into(
    invocation: &Invocation<'_>,
    program: &ScalarProgram<'_>,
    bindings: &[Binding],
    meter: &mut super::meter::Meter,
    output: &mut Vec<i64>,
    attempts: &mut Attempts,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost initial_used = meter.used.counters@;
        let ghost initial_attempts = attempts.view();
    }
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Attempts::view); }
    output.clear();
    metadata(invocation, program.inputs, bindings)?;
    let mut decoded = Decoded {
        state: Vec::new(),
        command: Vec::new(),
        context: Vec::new(),
    };
    if let Err(refusal) = input_view::project_into(
        invocation.state.bytes,
        invocation.state.fields,
        meter,
        &mut decoded.state,
        &mut attempts.state,
    ) {
        return Err(Failure::Record {
            source: Source::State,
            refusal,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost after_state = meter.used.counters@; }
    if let Err(refusal) = input_view::project_into(
        invocation.command.bytes,
        invocation.command.fields,
        meter,
        &mut decoded.command,
        &mut attempts.command,
    ) {
        return Err(Failure::Record {
            source: Source::Command,
            refusal,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost after_command = meter.used.counters@; }
    if let Err(refusal) = input_view::project_into(
        invocation.context.bytes,
        invocation.context.fields,
        meter,
        &mut decoded.context,
        &mut attempts.context,
    ) {
        return Err(Failure::Record {
            source: Source::Context,
            refusal,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        input_view::projected_is_typed(invocation.state.bytes@, invocation.state.fields@,
            meter.limits.counters@, initial_used, decoded.state@);
        input_view::projected_is_typed(invocation.command.bytes@, invocation.command.fields@,
            meter.limits.counters@, after_state, decoded.command@);
        input_view::projected_is_typed(invocation.context.bytes@, invocation.context.fields@,
            meter.limits.counters@, after_command, decoded.context@);
        assert(spec::typed_records(invocation, spec::decoded(&decoded)));
        spec::tuple_is_typed(invocation, program.inputs@, bindings@, spec::decoded(&decoded), bindings@.len());
    }
    let mut input = Vec::new();
    if project_tuple(invocation, &decoded, bindings, &mut input).is_err() {
        return Err(Failure::Binding);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(super::super::evaluation::spec::admitted(program.inputs@, input@));
    }
    let mut scratch = Vec::new();
    match super::evaluate_into(
        program.inputs,
        program.outputs,
        program.nodes,
        program.roots,
        &input,
        meter,
        &mut scratch,
        output,
    ) {
        Ok(()) => Ok(()),
        Err(error) => Err(Failure::Execution(error)),
    }
}

/// Executes all original records and the bound eager graph with one private meter.
///
/// This entry does not perform catalog, envelope, authentication or authority
/// admission. It preserves the existing low-level malformed-graph behavior.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.view() == spec::execution(invocation, program, bindings@,
        limits.view(), Seq::new(8, |_: int| 0u64)),
))]
pub fn execute(
    invocation: &Invocation<'_>,
    program: &ScalarProgram<'_>,
    bindings: &[Binding],
    limits: super::Limits,
) -> Outcome {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Outcome::view); reveal(Attempts::view); reveal(super::Limits::view); reveal(super::Usage::view); }
    let mut meter = super::meter::new(limits);
    let mut output = Vec::new();
    let mut attempts = Attempts {
        state: Vec::new(),
        command: Vec::new(),
        context: Vec::new(),
    };
    let result = match execute_into(
        invocation,
        program,
        bindings,
        &mut meter,
        &mut output,
        &mut attempts,
    ) {
        Ok(()) => Ok(output),
        Err(error) => Err(error),
    };
    Outcome {
        result,
        usage: meter.used,
        attempts,
    }
}

#[cfg(test)]
mod tests;
