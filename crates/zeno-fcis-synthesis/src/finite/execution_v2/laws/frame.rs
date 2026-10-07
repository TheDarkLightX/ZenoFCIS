//! Exact selection from the borrowed full decision or actual initial state.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::optional(result) == spec::field(fields@,id,fields@.len()),
))]
fn field<'a>(fields: &'a [Field<'a>], id: u16) -> Option<Atom<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=fields.len(), spec::field(fields@,id,i as nat)==None::<spec::Value>,
        decreases fields.len()-i,
    ))]
    while i < fields.len() {
        if fields[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! { spec::field_found(fields@,id,(i+1) as nat,fields@.len()); }
            return Some(fields[i].value);
        }
        i += 1;
    }
    None
}
/// Exact protected-observation selector tag; a root never aliases a field.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::selector_id(selector),))]
fn selector_id(selector: Selector) -> u32 {
    match selector {
        Selector::Root => 65536,
        Selector::Field(id) => id as u32,
    }
}
/// One (source tag, stable ID, permission) triple of an actual write attempt.
type AttemptTag = (u8, u32, bool);
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result as nat==spec::write_views(attempts@,attempts@.len()).len(),))]
fn write_count(attempts: &[Attempt]) -> usize {
    let mut count = 0usize;
    let mut k = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant k<=attempts.len(),count<=k,count as nat==spec::write_views(attempts@,k as nat).len(),
        decreases attempts.len()-k,
    ))]
    while k < attempts.len() {
        match attempts[k] {
            Attempt::Candidate(..) => count += 1,
            Attempt::Write(..) => count += 1,
            Attempt::Effect(..) => {}
        }
        k += 1;
    }
    count
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result as nat==spec::effect_views(attempts@,attempts@.len()).len(),))]
fn effect_count(attempts: &[Attempt]) -> usize {
    let mut count = 0usize;
    let mut k = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant k<=attempts.len(),count<=k,count as nat==spec::effect_views(attempts@,k as nat).len(),
        decreases attempts.len()-k,
    ))]
    while k < attempts.len() {
        if let Attempt::Effect(..) = attempts[k] {
            count += 1;
        }
        k += 1;
    }
    count
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures match result{Some(t)=>
    spec::write_views(attempts@,attempts@.len()).len()>i&&spec::write_views(attempts@,attempts@.len())[i as int]==t,
    None=>i>=spec::write_views(attempts@,attempts@.len()).len()},))]
fn write_entry(attempts: &[Attempt], i: usize) -> Option<AttemptTag> {
    let mut count = 0usize;
    let mut selected = None;
    let mut k = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant k<=attempts.len(),count<=k,count as nat==spec::write_views(attempts@,k as nat).len(),
            match selected {Some(t)=>i<count && spec::write_views(attempts@,k as nat)[i as int]==t, None=>i>=count},
        decreases attempts.len()-k,
    ))]
    while k < attempts.len() {
        match attempts[k] {
            Attempt::Candidate(permitted) => {
                if count == i {
                    selected = Some((0, 0, permitted));
                }
                count += 1;
            }
            Attempt::Write(id, permitted) => {
                if count == i {
                    selected = Some((1, id as u32, permitted));
                }
                count += 1;
            }
            Attempt::Effect(..) => {}
        }
        k += 1;
    }
    selected
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures match result{Some(t)=>
    spec::effect_views(attempts@,attempts@.len()).len()>i&&spec::effect_views(attempts@,attempts@.len())[i as int]==t,
    None=>i>=spec::effect_views(attempts@,attempts@.len()).len()},))]
fn effect_entry(attempts: &[Attempt], i: usize) -> Option<AttemptTag> {
    let mut count = 0usize;
    let mut selected = None;
    let mut k = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant k<=attempts.len(),count<=k,count as nat==spec::effect_views(attempts@,k as nat).len(),
            match selected {Some(t)=>i<count && spec::effect_views(attempts@,k as nat)[i as int]==t, None=>i>=count},
        decreases attempts.len()-k,
    ))]
    while k < attempts.len() {
        if let Attempt::Effect(outbox, id, permitted) = attempts[k] {
            if count == i {
                selected = Some((if outbox { 3 } else { 2 }, id, permitted));
            }
            count += 1;
        }
        k += 1;
    }
    selected
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::optional(result) == spec::observe(frame,observation),
))]
pub(super) fn observe<'a>(frame: &Frame<'a>, observation: Observation) -> Option<Atom<'a>> {
    match frame {
        Frame::Genesis { initial } => match observation {
            Observation::Initial(id) | Observation::Post(id) => view_field(*initial, id),
            Observation::PostLength => record_length(*initial),
            Observation::InitialRoot | Observation::PostRoot => view_atom(*initial),
            _ => None,
        },
        Frame::Transition {
            pre,
            command,
            context,
            candidate: c,
        } => match observation {
            Observation::Pre(id) => view_field(*pre, id),
            Observation::Command(id) => view_field(*command, id),
            Observation::Context(id) => view_field(*context, id),
            Observation::Post(id) => view_field(c.post, id),
            Observation::Initial(_) | Observation::InitialRoot => None,
            Observation::PreRoot => view_atom(*pre),
            Observation::CommandRoot => view_atom(*command),
            Observation::ContextRoot => view_atom(*context),
            Observation::PostRoot => view_atom(c.post),
            Observation::Class => Some(Atom::I128(match c.class {
                Class::Accept => 0,
                Class::Reject => 1,
                Class::CommittedFailure => 2,
            })),
            Observation::HasReason => Some(Atom::Bool(c.reason.is_some())),
            Observation::Reason => {
                let id = c.reason?;
                Some(Atom::I128(id as i128))
            }
            Observation::PostLength => record_length(c.post),
            Observation::PatchLength => Some(Atom::U128(c.patch.len() as u128)),
            Observation::EffectLength => Some(Atom::U128(c.effects.len() as u128)),
            Observation::OutboxLength => Some(Atom::U128(c.outbox.len() as u128)),
            Observation::ReadLength => Some(Atom::U128(c.reads.len() as u128)),
            Observation::WriteLength => Some(Atom::U128(write_count(c.attempts) as u128)),
            Observation::EffectAttemptLength => Some(Atom::U128(effect_count(c.attempts) as u128)),
            Observation::PatchField(i) => {
                if i < c.patch.len() {
                    Some(Atom::I128(c.patch[i].field as i128))
                } else {
                    None
                }
            }
            Observation::PatchBefore(i) => {
                if i < c.patch.len() {
                    Some(c.patch[i].before)
                } else {
                    None
                }
            }
            Observation::PatchAfter(i) => {
                if i < c.patch.len() {
                    Some(c.patch[i].after)
                } else {
                    None
                }
            }
            Observation::EffectOrdinal(i) => {
                if i < c.effects.len() {
                    Some(Atom::I128(c.effects[i].ordinal as i128))
                } else {
                    None
                }
            }
            Observation::EffectChannel(i) => {
                if i < c.effects.len() {
                    Some(Atom::I128(c.effects[i].channel as i128))
                } else {
                    None
                }
            }
            Observation::EffectDestination(i) => {
                if i < c.effects.len() {
                    Some(c.effects[i].destination)
                } else {
                    None
                }
            }
            Observation::EffectIdempotency(i) => {
                if i < c.effects.len() {
                    Some(c.effects[i].idempotency)
                } else {
                    None
                }
            }
            Observation::OutboxOrdinal(i) => {
                if i < c.outbox.len() {
                    Some(Atom::I128(c.outbox[i].ordinal as i128))
                } else {
                    None
                }
            }
            Observation::OutboxChannel(i) => {
                if i < c.outbox.len() {
                    Some(Atom::I128(c.outbox[i].channel as i128))
                } else {
                    None
                }
            }
            Observation::OutboxDestination(i) => {
                if i < c.outbox.len() {
                    Some(c.outbox[i].destination)
                } else {
                    None
                }
            }
            Observation::OutboxIdempotency(i) => {
                if i < c.outbox.len() {
                    Some(c.outbox[i].idempotency)
                } else {
                    None
                }
            }
            Observation::ReadSource(i) => {
                if i < c.reads.len() {
                    Some(Atom::I128(c.reads[i].source as i128))
                } else {
                    None
                }
            }
            Observation::ReadId(i) => {
                if i < c.reads.len() {
                    Some(Atom::I128(selector_id(c.reads[i].selector) as i128))
                } else {
                    None
                }
            }
            Observation::ReadPermitted(i) => {
                if i < c.reads.len() {
                    Some(Atom::Bool(c.reads[i].permitted))
                } else {
                    None
                }
            }
            Observation::WriteSource(i) => {
                let (source, _, _) = write_entry(c.attempts, i)?;
                Some(Atom::I128(source as i128))
            }
            Observation::WriteId(i) => {
                let (_, id, _) = write_entry(c.attempts, i)?;
                Some(Atom::I128(id as i128))
            }
            Observation::WritePermitted(i) => {
                let (_, _, permitted) = write_entry(c.attempts, i)?;
                Some(Atom::Bool(permitted))
            }
            Observation::EffectAttemptSource(i) => {
                let (source, _, _) = effect_entry(c.attempts, i)?;
                Some(Atom::I128(source as i128))
            }
            Observation::EffectAttemptId(i) => {
                let (_, id, _) = effect_entry(c.attempts, i)?;
                Some(Atom::I128(id as i128))
            }
            Observation::EffectAttemptPermitted(i) => {
                let (_, _, permitted) = effect_entry(c.attempts, i)?;
                Some(Atom::Bool(permitted))
            }
            Observation::EffectPayload(i, id) => {
                if i < c.effects.len() {
                    field(&c.effects[i].payload, id)
                } else {
                    None
                }
            }
            Observation::OutboxPayload(i, id) => {
                if i < c.outbox.len() {
                    field(&c.outbox[i].payload, id)
                } else {
                    None
                }
            }
            Observation::Usage(resource) => Some(Atom::U128(c.usage.used(resource) as u128)),
        },
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::record_valid(fields@),))]
fn record_valid(fields: &[Field<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=fields.len(),forall|j:int| 0<j<i ==> fields@[j-1].id<#[trigger] fields@[j].id,
        decreases fields.len()-i,
    ))]
    while i < fields.len() {
        if i > 0 && fields[i - 1].id >= fields[i].id {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::deliveries_valid(deliveries@),))]
fn deliveries_valid(deliveries: &[Delivery<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=deliveries.len(),forall|j:int| #![trigger deliveries@[j]] 0<=j<i ==> spec::record_valid(deliveries@[j].payload@)
            && (j>0 ==> deliveries@[j-1].ordinal<deliveries@[j].ordinal),
        decreases deliveries.len()-i,
    ))]
    while i < deliveries.len() {
        if (i > 0 && deliveries[i - 1].ordinal >= deliveries[i].ordinal)
            || !record_valid(&deliveries[i].payload)
        {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::patch_valid(patch@),))]
fn patch_valid(patch: &[Patch<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=patch.len(),forall|j:int| #![trigger patch@[j]] 0<=j<i ==> spec::same_type(spec::atom(patch@[j].before),spec::atom(patch@[j].after))
            && (j>0 ==> patch@[j-1].field<patch@[j].field),
        decreases patch.len()-i,
    ))]
    while i < patch.len() {
        if (i > 0 && patch[i - 1].field >= patch[i].field)
            || !super::atoms::same_type(patch[i].before, patch[i].after)
        {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::matching_fields(pre@,post@),))]
fn matching_fields(pre: &[Field<'_>], post: &[Field<'_>]) -> bool {
    if pre.len() != post.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=pre.len(),pre.len()==post.len(),forall|j:int| 0<=j<i ==> pre@[j].id==post@[j].id
            && spec::same_type(spec::atom(pre@[j].value),spec::atom(post@[j].value)),
        decreases pre.len()-i,
    ))]
    while i < pre.len() {
        if pre[i].id != post[i].id || !super::atoms::same_type(pre[i].value, post[i].value) {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::frame_valid(frame),))]
pub(super) fn valid(frame: &Frame<'_>) -> bool {
    match frame {
        Frame::Genesis { initial } => root_valid(*initial),
        Frame::Transition {
            pre,
            command,
            context,
            candidate: c,
        } => {
            let class_valid = match c.class {
                Class::Accept => c.reason.is_none(),
                Class::Reject | Class::CommittedFailure => match c.reason {
                    Some(id) => id != 0,
                    None => false,
                },
            };
            if !class_valid
                || !root_valid(*pre)
                || !root_valid(*command)
                || !root_valid(*context)
                || !root_valid(c.post)
                || !patch_valid(c.patch)
                || !deliveries_valid(c.effects)
                || !deliveries_valid(c.outbox)
            {
                return false;
            }
            match c.class {
                Class::Reject => {
                    empty_record(c.post)
                        && c.patch.is_empty()
                        && c.effects.is_empty()
                        && c.outbox.is_empty()
                }
                _ => matching_roots(*pre, c.post),
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures spec::optional(result)==spec::view_field(value,id),))]
fn view_field<'a>(value: RootView<'a, 'a>, id: u16) -> Option<Atom<'a>> {
    match value {
        RootView::Record(fields) => field(fields, id),
        RootView::Leaf(_) => None,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures spec::optional(result)==spec::view_atom(value),))]
fn view_atom<'a>(value: RootView<'a, 'a>) -> Option<Atom<'a>> {
    match value {
        RootView::Leaf(atom) => Some(atom),
        RootView::Record(_) => None,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures spec::optional(result)==spec::record_length(value),))]
fn record_length<'a>(value: RootView<'a, 'a>) -> Option<Atom<'a>> {
    match value {
        RootView::Record(fields) => Some(Atom::U128(fields.len() as u128)),
        RootView::Leaf(_) => None,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::root_valid(value),))]
fn root_valid(value: RootView<'_, '_>) -> bool {
    match value {
        RootView::Record(fields) => record_valid(fields),
        RootView::Leaf(_) => true,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::empty_record(value),))]
fn empty_record(value: RootView<'_, '_>) -> bool {
    match value {
        RootView::Record(fields) => fields.is_empty(),
        RootView::Leaf(_) => false,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::matching_roots(pre,post),))]
fn matching_roots(pre: RootView<'_, '_>, post: RootView<'_, '_>) -> bool {
    match (pre, post) {
        (RootView::Record(a), RootView::Record(b)) => matching_fields(a, b),
        (RootView::Leaf(a), RootView::Leaf(b)) => super::atoms::same_type(a, b),

        _ => false,
    }
}
