//! The one composition pipeline over the two closed descriptor data families.
use super::super::super::evaluation::typed::Values;
use super::super::input_view::typed::LeafView;
use super::super::numeric;
use super::descriptor::{DescriptorProjection, OutputTypes};
use super::ingress::shared::{self as input, SchemaView, Shape};
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "shared_spec.rs"]
pub(super) mod spec;

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::scalar(schema,decoded.scalars@,selector),))]
fn scalar(
    schema: SchemaView<'_>,
    decoded: &ingress::Decoded<'_, Atom<'static>>,
    selector: Selector,
) -> Option<Atom<'static>> {
    match (schema.shape(), selector) {
        (Shape::Leaf(_), Selector::Root) => {
            if decoded.scalars.len() == 1 {
                Some(decoded.scalars[0])
            } else {
                None
            }
        }
        (Shape::Record(fields), Selector::Field(id)) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant input::spec::shape(schema)==Shape::Record(fields),selector==Selector::Field(id),i<=input_view::typed::spec::fields(fields).len(),forall|j:int|0<=j<i ==> input_view::typed::spec::fields(fields)[j].0!=id,decreases input_view::typed::spec::fields(fields).len()-i,))]
            while i < fields.len() {
                let (field, _) = fields.field(i)?;
                if field == id {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        let fs = input_view::typed::spec::fields(fields);
                        assert(spec::first(fs, id, i as int));
                        assert(exists|j:int|spec::first(fs,id,j));
                        let selected = choose|j: int| spec::first(fs, id, j);
                        if selected < i { assert(fs[selected].0 != id); }
                        if i < selected { assert(fs[i as int].0 != id); }
                        assert(selected == i);
                    }
                    return if i < decoded.scalars.len() {
                        Some(decoded.scalars[i])
                    } else {
                        None
                    };
                }
                i += 1;
            }
            None
        }
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None})==spec::tuple(d,state.scalars@,command.scalars@,context.scalars@,descriptor::spec::bindings(d)@.len()),))]
fn tuple(
    d: &DescriptorProjection<'_>,
    state: &ingress::Decoded<'_, Atom<'static>>,
    command: &ingress::Decoded<'_, Atom<'static>>,
    context: &ingress::Decoded<'_, Atom<'static>>,
) -> Option<Vec<Atom<'static>>> {
    let bindings = d.bindings();
    let mut values = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=bindings.len(),bindings==descriptor::spec::bindings(d),spec::tuple(d,state.scalars@,command.scalars@,context.scalars@,i as nat)==Some(values@),decreases bindings.len()-i,))]
    while i < bindings.len() {
        let b = bindings[i];
        let value = match b.source {
            Source::State => scalar(d.state(), state, b.selector),
            Source::Command => scalar(d.command(), command, b.selector),
            Source::Context => scalar(d.context(), context, b.selector),
        };
        let Some(v) = value else {
            #[cfg(verus_keep_ghost)]
            proof! {
                reveal_with_fuel(spec::tuple, 2);
                assert(spec::tuple(d,state.scalars@,command.scalars@,context.scalars@,(i+1) as nat).is_none());
                spec::tuple_failed(d,state.scalars@,command.scalars@,context.scalars@,(i+1) as nat,bindings@.len());
            }
            return None;
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal_with_fuel(spec::tuple, 2);
            assert(spec::tuple(d,state.scalars@,command.scalars@,context.scalars@,(i+1) as nat)==Some(values@.push(v)));
        }
        values.push(v);
        i += 1;
    }
    Some(values)
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None})==spec::outputs(types,codes@),))]
fn outputs(types: OutputTypes<'_>, codes: &[Atom<'static>]) -> Option<Vec<Atom<'static>>> {
    let count = match types {
        OutputTypes::Legacy(t) => t.len(),
        OutputTypes::Numeric(t) => t.len(),
    };
    if count != codes.len() {
        return None;
    }
    let mut output = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=count,count==codes.len(),count==spec::output_count(types),spec::output_prefix(types,codes@,i as nat)==Some(output@),decreases count-i,))]
    while i < count {
        let leaf = match types {
            OutputTypes::Legacy(t) => LeafView::Legacy(&t[i]),
            OutputTypes::Numeric(t) => LeafView::Numeric(t[i]),
        };
        let Some(value) = input::reify(leaf, codes[i]) else {
            #[cfg(verus_keep_ghost)]
            proof! { spec::outputs_failed(types,codes@,(i+1) as nat,count as nat); }
            return None;
        };
        output.push(value);
        i += 1;
    }
    Some(output)
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==numeric::spec::legacy_failure(error),))]
fn execution_failure(error: numeric::Failure) -> super::super::Failure {
    match error {
        numeric::Failure::InputDomain => super::super::Failure::InputDomain,
        numeric::Failure::Reference => super::super::Failure::Reference,
        numeric::Failure::Kind | numeric::Failure::Arithmetic => super::super::Failure::Arithmetic,
        numeric::Failure::OutputDomain => super::super::Failure::OutputDomain,
        numeric::Failure::Budget(e) => super::super::Failure::Budget(e),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    spec::produce(&core.descriptor_view().model(),raw,old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),
    (match result{Ok(p)=>Ok(producer::spec::produced(p)),Err(e)=>Err(e)},final(meter).used.counters@,producer::spec::trace(final(trace)))),))]
fn produce<'p: 'a, 'a, D: DescriptorData<'p> + 'p>(
    core: &BoundCore<'p, D>,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
) -> Result<producer::Produced<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);}
    let d = core.descriptor.project();
    let state = match input::project(raw.state, d.state(), 0, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(0, e)),
    };
    let command = match input::project(raw.command, d.command(), 1, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(1, e)),
    };
    let context = match input::project(raw.context, d.context(), 2, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(2, e)),
    };
    trace.ingress = Some(meter.used);
    let Some(values) = tuple(&d, &state, &command, &context) else {
        return Err(Failure::Binding);
    };
    let mut scratch = Vec::new();
    let mut output = Vec::new();
    if let Err(e) = numeric::evaluate(
        d.program(),
        Values::Typed(&values),
        meter,
        &mut scratch,
        &mut output,
    ) {
        return Err(Failure::Execution(execution_failure(e)));
    }
    let Some(atoms) = outputs(d.outputs(), &output) else {
        return Err(Failure::Output);
    };
    let selected = d.decision_output();
    if selected >= atoms.len() {
        return Err(Failure::Output);
    }
    let Atom::I128(code) = atoms[selected] else {
        return Err(Failure::Output);
    };
    let ingress::Value::Record(pre) = &state.value else {
        return Err(Failure::Binding);
    };
    let inputs = decision::Inputs {
        state: pre,
        command: producer::value(&command.value),
        context: producer::value(&context.value),
    };
    let candidate = match decision::construct(
        inputs,
        &atoms,
        code,
        d.branches(),
        meter,
        &mut trace.attempts,
    ) {
        Ok(c) => c,
        Err(e) => return Err(Failure::Decision(e)),
    };
    trace.decision = Some(meter.used);
    Ok(producer::Produced {
        candidate,
        command: command.value,
        context: context.value,
    })
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(c)=>Some(*c),None=>None})==spec::channel(channels@,id),))]
fn channel<'a>(channels: &'a [Channel<'a>], id: u32) -> Option<&'a Channel<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=channels.len(),forall|j:int|0<=j<i ==> channels[j].id!=id,decreases channels.len()-i,))]
    while i < channels.len() {
        if channels[i].id == id {
            return Some(&channels[i]);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::deliveries(channels@,values@.map(|_:int,d:decision::Delivery|decision::spec::delivery_view(d))),))]
fn deliveries(channels: &[Channel<'_>], values: &[decision::Delivery<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),forall|j:int|0<=j<i ==> spec::delivery(channels@,decision::spec::delivery_view(values[j])),decreases values.len()-i,))]
    while i < values.len() {
        let d = &values[i];
        let Some(c) = channel(channels, d.channel) else {
            #[cfg(verus_keep_ghost)]
            proof! {
                let ps = values@.map(|_:int,d:decision::Delivery|decision::spec::delivery_view(d));
                assert(!spec::delivery(channels@,ps[i as int]));
                assert(!spec::deliveries(channels@,ps));
            }
            return false;
        };
        if !schema_validation::atom(c.destination, d.destination)
            || !schema_validation::atom(c.idempotency, d.idempotency)
            || !schema_validation::payload(&d.payload, c.payload)
        {
            #[cfg(verus_keep_ghost)]
            proof! {
                let ps = values@.map(|_:int,d:decision::Delivery|decision::spec::delivery_view(d));
                assert(!spec::delivery(channels@,ps[i as int]));
                assert(!spec::deliveries(channels@,ps));
            }
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::candidate(d,decision::spec::candidate_view(*value)),))]
pub(super) fn candidate(d: &DescriptorProjection<'_>, value: &Candidate<'_>) -> bool {
    deliveries(d.channels(), value.effects()) && deliveries(d.channels(), value.outbox())
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    spec::evaluated(&core.descriptor_view().model(),kind,raw,old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),old(diagnostics)@,old(law_reads)@,
    (match result{Ok(c)=>Ok(decision::spec::candidate_view(c)),Err(e)=>Err(e)},final(meter).used.counters@,producer::spec::trace(final(trace)),final(diagnostics)@,final(law_reads)@)),))]
pub(super) fn run<'p: 'a, 'a, D: DescriptorData<'p> + 'p>(
    core: &BoundCore<'p, D>,
    kind: Kind,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);reveal(Usage::view);}
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial_diagnostics = diagnostics@; }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial_law_reads = law_reads@; }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial_usage = meter.used.counters@; let ghost initial_trace = producer::spec::trace(trace); }
    let d = core.descriptor.project();
    #[cfg(verus_keep_ghost)]
    proof! { assert(d.admitted()); }
    match kind {
        Kind::Genesis => {
            let decoded = match input::project(raw.state, d.state(), 0, meter, &mut trace.reads) {
                Ok(v) => v,
                Err(e) => return Err(Failure::Ingress(0, e)),
            };
            trace.ingress = Some(meter.used);
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost decoded_view = ingress::spec::value(decoded.value); }
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost projected = input::spec::project(raw.state@,descriptor::spec::state(&d),0,meter.limits.counters@,initial_usage); }
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(projected.0 == Ok((decoded_view, decoded.scalars@)));
                assert(projected.1 == meter.used.counters@);
                assert(producer::spec::trace(trace) == (initial_trace.0 + projected.2, initial_trace.1, Some(projected.1), initial_trace.3));
            }
            let frame = laws::Frame::Genesis {
                initial: producer::value(&decoded.value),
            };
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost phase_usage = meter.used.counters@; }
            #[cfg(verus_keep_ghost)]
            proof! { assert(super::spec::genesis_frame(&frame,ingress::spec::value(decoded.value))); }
            let law_result = laws::evaluate_into(
                d.laws(),
                d.required(),
                &frame,
                meter,
                diagnostics,
                law_reads,
            );
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(spec::genesis_evaluated(&d,ingress::spec::value(decoded.value),meter.limits.counters@,phase_usage,
                    initial_diagnostics,initial_law_reads,(law_result,meter.used.counters@,diagnostics@,law_reads@)));
                let finished = (match law_result {Ok(()) => Ok(super::spec::identity_candidate(decoded_view)), Err(e) => Err(Failure::Law(e))},
                    meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@);
                assert(finished.2 == (initial_trace.0 + projected.2, initial_trace.1, Some(projected.1), initial_trace.3));
                spec::finish_genesis(&d,raw.state@,meter.limits.counters@,initial_usage,initial_trace,initial_diagnostics,initial_law_reads,
                    (decoded_view,decoded.scalars@),law_result,finished);
                assert(spec::genesis(&d,raw.state@,meter.limits.counters@,initial_usage,initial_trace,initial_diagnostics,initial_law_reads,finished));
            }
            if let Err(e) = law_result {
                return Err(Failure::Law(e));
            }
            let fields = match decoded.value {
                ingress::Value::Record(f) => f,
                ingress::Value::Leaf(_) => Vec::new(),
            };
            let candidate = decision::identity(fields);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(decision::spec::candidate_view(candidate)==super::spec::identity_candidate(decoded_view));
                assert(spec::genesis(&d,raw.state@,meter.limits.counters@,initial_usage,initial_trace,initial_diagnostics,initial_law_reads,
                    (Ok(decision::spec::candidate_view(candidate)),meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@)));
            }
            Ok(candidate)
        }
        Kind::Transition => {
            let produced = produce(core, raw, meter, trace)?;
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost middle = (Ok(producer::spec::produced(produced)),meter.used.counters@,producer::spec::trace(trace)); }
            if !candidate(&d, &produced.candidate) {
                return Err(Failure::Schema);
            }
            let observation = laws::Candidate {
                class: produced.candidate.class(),
                reason: produced.candidate.reason(),
                post: decision::RootView::Record(produced.candidate.post()),
                patch: produced.candidate.patch(),
                effects: produced.candidate.effects(),
                outbox: produced.candidate.outbox(),
                reads: &trace.reads,
                attempts: &trace.attempts,
                usage: meter.used,
            };
            let frame = laws::Frame::Transition {
                pre: decision::RootView::Record(produced.candidate.pre()),
                command: producer::value(&produced.command),
                context: producer::value(&produced.context),
                candidate: &observation,
            };
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost phase_usage = meter.used.counters@; }
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost phase_trace = producer::spec::trace(trace); }
            #[cfg(verus_keep_ghost)]
            proof! { assert(super::spec::transition_frame(&frame,producer::spec::produced(produced),phase_trace,phase_usage)); }
            let law_result = laws::evaluate_into(
                d.laws(),
                d.required(),
                &frame,
                meter,
                diagnostics,
                law_reads,
            );
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(spec::frame_evaluated(&d,producer::spec::produced(produced),phase_trace,phase_usage,meter.limits.counters@,
                    initial_diagnostics,initial_law_reads,(law_result,meter.used.counters@,diagnostics@,law_reads@)));
            }
            if let Err(e) = law_result {
                return Err(Failure::Law(e));
            }
            #[cfg(verus_keep_ghost)]
            proof! {
                let finished=(Ok(decision::spec::candidate_view(produced.candidate)),meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@);
                assert(spec::produce(&d,raw,meter.limits.counters@,initial_usage,initial_trace,middle));
                assert(spec::after_producer(&d,middle,initial_diagnostics,initial_law_reads,meter.limits.counters@,finished));
                assert(spec::execution(&d,raw,meter.limits.counters@,initial_usage,initial_trace,initial_diagnostics,initial_law_reads,finished));
            }
            Ok(produced.candidate)
        }
    }
}
