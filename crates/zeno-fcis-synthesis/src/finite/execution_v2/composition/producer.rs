//! Exact private typed graph producer; outputs cannot be supplied by callers.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::field_code(fields@,codes@,id),))]
fn field_code(fields: &[InputField], codes: &[i64], id: u16) -> Option<i64> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),forall|j:int|0<=j<i==>fields@[j].id!=id,decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {assert(admission::spec::first(fields@,id,i as int));assert(exists|j:int|admission::spec::first(fields@,id,j));admission::spec::first_chosen(fields@,id,i as int);}
            return if i < codes.len() {
                Some(codes[i])
            } else {
                None
            };
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::scalar(schema,decoded,selector),))]
fn scalar(schema: Schema<'_>, decoded: &ingress::Decoded, selector: Selector) -> Option<i64> {
    match (schema, selector) {
        (Schema::Leaf(_), Selector::Root) => {
            if decoded.scalars.len() == 1 {
                Some(decoded.scalars[0])
            } else {
                None
            }
        }
        (Schema::Record(fields), Selector::Field(id)) => field_code(fields, &decoded.scalars, id),
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None::<Seq<i64>>})==spec::tuple(d,state,command,context,d.bindings@.len()),))]
pub(super) fn tuple(
    d: &Descriptor<'_>,
    state: &ingress::Decoded,
    command: &ingress::Decoded,
    context: &ingress::Decoded,
) -> Option<Vec<i64>> {
    let mut output = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.bindings.len(),spec::tuple(d,state,command,context,i as nat)==Some(output@),decreases d.bindings.len()-i,))]
    while i < d.bindings.len() {
        let b = d.bindings[i];
        let value = match b.source {
            Source::State => scalar(d.state, state, b.selector),
            Source::Command => scalar(d.command, command, b.selector),
            Source::Context => scalar(d.context, context, b.selector),
        };
        let Some(v) = value else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::tuple_failed(d,state,command,context,(i+1) as nat,d.bindings@.len());}
            return None;
        };
        output.push(v);
        i += 1;
    }
    Some(output)
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None::<Seq<Atom>>})==spec::atoms(types@,codes@,types@.len()),))]
fn atoms(types: &[InputLeaf], codes: &[i64]) -> Option<Vec<Atom<'static>>> {
    if types.len() != codes.len() {
        return None;
    }
    let mut output = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=types.len(),types.len()==codes.len(),spec::atoms(types@,codes@,i as nat)==Some(output@),decreases types.len()-i,))]
    while i < types.len() {
        let Some(value) = ingress::reify(&types[i], codes[i]) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::atoms_failed(types@,codes@,(i+1) as nat,types@.len());}
            return None;
        };
        output.push(value);
        i += 1;
    }
    Some(output)
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures spec::input_view(result)==ingress::spec::value(*input),))]
pub(super) fn value<'fields, 'a>(
    input: &'fields ingress::Value<'a>,
) -> decision::RootView<'fields, 'a> {
    match input {
        ingress::Value::Leaf(a) => decision::RootView::Leaf(*a),
        ingress::Value::Record(fields) => decision::RootView::Record(fields),
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct Produced<'a> {
    pub candidate: Candidate<'a>,
    pub command: ingress::Value<'a>,
    pub context: ingress::Value<'a>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct Trace {
    pub reads: Vec<ReadAttempt>,
    pub attempts: Vec<Attempt>,
    pub ingress: Option<Usage>,
    pub decision: Option<Usage>,
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    spec::produce(core.descriptor_view(),raw,old(meter).limits.counters@,old(meter).used.counters@,spec::trace(old(trace)),
    (match result{Ok(p)=>Ok(spec::produced(p)),Err(e)=>Err(e)},final(meter).used.counters@,spec::trace(final(trace)))),))]
pub(super) fn produce<'a>(
    core: &BoundCore<'a>,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut Trace,
) -> Result<Produced<'a>, Failure> {
    let d = core.descriptor;
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);}
    let state = match ingress::project(raw.state, d.state, 0, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(0, e)),
    };
    let command = match ingress::project(raw.command, d.command, 1, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(1, e)),
    };
    let context = match ingress::project(raw.context, d.context, 2, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(2, e)),
    };
    finish(d, state, command, context, meter, trace)
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    spec::after_ingress(d,ingress::spec::decoded(state),ingress::spec::decoded(command),ingress::spec::decoded(context),
    old(meter).limits.counters@,old(meter).used.counters@,spec::trace(old(trace)),
    (match result{Ok(p)=>Ok(spec::produced(p)),Err(e)=>Err(e)},final(meter).used.counters@,spec::trace(final(trace)))),))]
fn finish<'a>(
    d: &Descriptor<'a>,
    state: ingress::Decoded<'a>,
    command: ingress::Decoded<'a>,
    context: ingress::Decoded<'a>,
    meter: &mut Meter,
    trace: &mut Trace,
) -> Result<Produced<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Usage::view);}
    trace.ingress = Some(meter.used);
    let Some(input) = tuple(d, &state, &command, &context) else {
        return Err(Failure::Binding);
    };
    let mut scratch = Vec::new();
    let mut output = Vec::new();
    if let Err(e) = super::super::evaluate_into(
        d.program.inputs,
        d.program.outputs,
        d.program.nodes,
        d.program.roots,
        &input,
        meter,
        &mut scratch,
        &mut output,
    ) {
        return Err(Failure::Execution(e));
    }
    let Some(atoms) = atoms(d.output_types, &output) else {
        return Err(Failure::Output);
    };
    from_atoms(
        d,
        state.value,
        command.value,
        context.value,
        &atoms,
        meter,
        trace,
    )
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    spec::from_atoms(d,ingress::spec::value(state),ingress::spec::value(command),ingress::spec::value(context),atoms@,
        old(meter).limits.counters@,old(meter).used.counters@,spec::trace(old(trace)),
        (match result{Ok(p)=>Ok(spec::produced(p)),Err(e)=>Err(e)},final(meter).used.counters@,spec::trace(final(trace)))),))]
pub(super) fn from_atoms<'a>(
    d: &Descriptor<'a>,
    state: ingress::Value<'a>,
    command: ingress::Value<'a>,
    context: ingress::Value<'a>,
    atoms: &[Atom<'a>],
    meter: &mut Meter,
    trace: &mut Trace,
) -> Result<Produced<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Usage::view);}
    if d.decision_output >= atoms.len() {
        return Err(Failure::Output);
    }
    let Atom::I128(code) = atoms[d.decision_output] else {
        return Err(Failure::Output);
    };
    let pre = match &state {
        ingress::Value::Record(f) => f.as_slice(),
        _ => return Err(Failure::Binding),
    };
    let inputs = decision::Inputs {
        state: pre,
        command: value(&command),
        context: value(&context),
    };
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost actual_inputs=inputs;let ghost before_decision=meter.used.counters@;}
    let candidate = match decision::construct(
        inputs,
        atoms,
        code,
        d.branches,
        meter,
        &mut trace.attempts,
    ) {
        Ok(c) => c,
        Err(e) => {
            #[cfg(verus_keep_ghost)]
            proof! {assert(spec::input_view(actual_inputs.command)==ingress::spec::value(command));assert(spec::input_view(actual_inputs.context)==ingress::spec::value(context));}
            return Err(Failure::Decision(e));
        }
    };
    trace.decision = Some(meter.used);
    #[cfg(verus_keep_ghost)]
    proof! {assert(spec::input_view(actual_inputs.command)==ingress::spec::value(command));assert(spec::input_view(actual_inputs.context)==ingress::spec::value(context));}
    Ok(Produced {
        candidate,
        command,
        context,
    })
}
