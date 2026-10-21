//! Complete numeric descriptor admission; no program-only admission shortcut.
use super::super::ingress::original;
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "numeric/spec.rs"]
pub(crate) mod spec;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::leaf(domain),))]
pub(in super::super) fn leaf(domain: Domain<'_>) -> bool {
    match domain {
        Domain::Bool => true,
        Domain::I128 { min, max } => min <= max,
        Domain::U128 { min, max } => min <= max,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::same_domain(a,b),))]
pub(in super::super) fn same_domain(a: Domain<'_>, b: Domain<'_>) -> bool {
    match (a, b) {
        (Domain::Bool, Domain::Bool) => true,
        (Domain::I128 { min: a, max: b }, Domain::I128 { min: c, max: d }) => a == c && b == d,
        (Domain::U128 { min: a, max: b }, Domain::U128 { min: c, max: d }) => a == c && b == d,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::schema_valid(schema),))]
pub(in super::super) fn schema_valid(schema: Schema<'_, Domain<'_>>) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(spec::schema_valid);}
    match schema {
        Schema::Original {
            checked,
            root,
            projection,
        } => original::valid(checked, root, projection),
        Schema::Leaf(domain) => leaf(*domain),
        Schema::Record(fields) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=fields.len(),schema==(Schema::Record(fields)),forall|j:int|0<=j<i ==> spec::field_valid(fields@,j),decreases fields.len()-i,))]
            while i < fields.len() {
                if !leaf(fields[i].leaf) || (i > 0 && fields[i - 1].id >= fields[i].id) {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        reveal(spec::schema_valid);
                        assert(schema == Schema::Record(fields));
                        assert(!spec::field_valid(fields@,i as int));
                        assert(exists|j:int|0<=j<fields@.len() && !spec::field_valid(fields@,j));
                        if spec::schema_valid(schema) {
                            spec::record_field_valid(fields,i as int);
                        }
                        assert(!spec::schema_valid(schema));
                    }
                    return false;
                }
                i += 1;
            }
            true
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::count(schema),))]
fn count(schema: Schema<'_, Domain<'_>>) -> usize {
    match schema {
        Schema::Original { projection, .. } => match projection {
            Projection::None => 0,
            Projection::Root(_) => 1,
            Projection::Fields(f) => f.len(),
        },
        Schema::Leaf(_) => 1,
        Schema::Record(fields) => fields.len(),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::schema(d,source),))]
fn schema<'a>(d: &NumericDescriptor<'a>, source: Source) -> Schema<'a, Domain<'a>> {
    match source {
        Source::State => d.state,
        Source::Command => d.command,
        Source::Context => d.context,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::find(schema,selector),))]
pub(in super::super) fn find<'a>(
    schema: Schema<'a, Domain<'a>>,
    selector: Selector,
) -> Option<Domain<'a>> {
    match (schema, selector) {
        (Schema::Leaf(domain), Selector::Root)
        | (
            Schema::Original {
                projection: Projection::Root(domain),
                ..
            },
            Selector::Root,
        ) => Some(*domain),
        (Schema::Record(fields), Selector::Field(id))
        | (
            Schema::Original {
                projection: Projection::Fields(fields),
                ..
            },
            Selector::Field(id),
        ) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),spec::selected_fields(schema)==Some(fields),selector==Selector::Field(id),forall|j:int|0<=j<i ==> fields@[j].id!=id,decreases fields.len()-i,))]
            while i < fields.len() {
                if fields[i].id == id {
                    #[cfg(verus_keep_ghost)]
                    proof! {spec::find_first(schema,selector,fields,id,i as int);}
                    return Some(fields[i].leaf);
                }
                i += 1;
            }
            None
        }
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::input_kind(s,selector),))]
fn input_kind(s: Schema<'_, Domain<'_>>, selector: Selector) -> Option<Kind> {
    match s {
        Schema::Original { checked, root, .. } => {
            original::kind(checked, original::selected_type(checked, root, selector)?)
        }
        _ => Some(domain_kind(find(s, selector)?)),
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::expression(d,expr),))]
fn expression(d: &NumericDescriptor<'_>, expr: Expr<'_>) -> Option<Kind> {
    match expr {
        Expr::Input(source, id) => input_kind(schema(d, source), Selector::Field(id)),
        Expr::Root(source) => input_kind(schema(d, source), Selector::Root),
        Expr::Output(i) => {
            if i < d.output_types.len() {
                Some(domain_kind(d.output_types[i]))
            } else {
                None
            }
        }
        Expr::Constant(atom) => Some(atom_kind(atom)),
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==(spec::expression(d,expr)==Some(kind)),))]
fn expression_is(d: &NumericDescriptor<'_>, expr: Expr<'_>, kind: Kind) -> bool {
    match expression(d, expr) {
        Some(actual) => same_kind(actual, kind),
        None => false,
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::bindings(d),))]
fn bindings(d: &NumericDescriptor<'_>) -> bool {
    if d.bindings.len() != d.program.inputs.len()
        || d.bindings.len() as u128
            != count(d.state) as u128 + count(d.command) as u128 + count(d.context) as u128
    {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.bindings.len(),d.bindings.len()==d.program.inputs.len(),d.bindings@.len()==spec::count(d.state) as int+spec::count(d.command) as int+spec::count(d.context) as int,forall|j:int|0<=j<i ==> spec::binding_valid(d,j),decreases d.bindings.len()-i,))]
    while i < d.bindings.len() {
        let b = d.bindings[i];
        let Some(domain) = find(schema(d, b.source), b.selector) else {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::binding_valid(d,i as int));}
            return false;
        };
        if !same_domain(domain, d.program.inputs[i]) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::binding_valid(d,i as int));}
            return false;
        }
        let mut j = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(invariant j<=i<d.bindings.len(),b==d.bindings@[i as int],d.bindings.len()==d.program.inputs.len(),d.bindings@.len()==spec::count(d.state) as int+spec::count(d.command) as int+spec::count(d.context) as int,spec::find(spec::schema(d,b.source),b.selector)==Some(domain),spec::same_domain(domain,d.program.inputs@[i as int]),forall|k:int|0<=k<i ==> spec::binding_valid(d,k),forall|k:int|0<=k<j ==> !super::spec::same_binding(d.bindings@[k],b),decreases i-j,))]
        while j < i {
            if same_binding(d.bindings[j], b) {
                #[cfg(verus_keep_ghost)]
                proof! {assert(!spec::binding_valid(d,i as int));}
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::outputs(d),))]
fn outputs(d: &NumericDescriptor<'_>) -> bool {
    if d.output_types.len() != d.program.outputs.len() || d.decision_output >= d.output_types.len()
    {
        return false;
    }
    if !matches!(d.output_types[d.decision_output], Domain::I128 { .. }) {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.output_types.len(),d.output_types.len()==d.program.outputs.len(),d.decision_output<d.output_types.len(),d.output_types@[d.decision_output as int] is I128,forall|j:int|0<=j<i ==> spec::output_valid(d,j),decreases d.output_types.len()-i,))]
    while i < d.output_types.len() {
        if !leaf(d.output_types[i]) || !same_domain(d.output_types[i], d.program.outputs[i]) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::output_valid(d,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::channel(d,id),))]
fn channel<'a>(d: &NumericDescriptor<'a>, id: u32) -> Option<Channel<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.channels.len(),forall|j:int|0<=j<i ==> d.channels@[j].id!=id,decreases d.channels.len()-i,))]
    while i < d.channels.len() {
        if d.channels[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {spec::channel_chosen(d,id,i as int);}
            return Some(d.channels[i]);
        }
        i += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::payload(d,fields@,schema@),))]
fn payload(
    d: &NumericDescriptor<'_>,
    fields: &[PayloadField<'_>],
    schema: &[TypedField<'_>],
) -> bool {
    if fields.len() != schema.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),fields.len()==schema.len(),forall|j:int|0<=j<i ==> spec::payload_entry(d,fields@,schema@,j),decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].field != schema[i].field
            || !expression_is(d, fields[i].value, domain_kind(schema[i].domain))
            || !schema_validation::expression(schema[i].domain, fields[i].value)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::payload_entry(d,fields@,schema@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::deliveries(d,plans@),))]
fn deliveries(d: &NumericDescriptor<'_>, plans: &[DeliveryPlan<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=plans.len(),forall|j:int|0<=j<i ==> spec::delivery_valid(d,plans@,j),decreases plans.len()-i,))]
    while i < plans.len() {
        let p = plans[i];
        if i > 0 && plans[i - 1].ordinal >= p.ordinal {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::delivery_valid(d,plans@,i as int));}
            return false;
        }
        let Some(c) = channel(d, p.channel) else {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::delivery_valid(d,plans@,i as int));}
            return false;
        };
        if !expression_is(d, p.when, Kind::Bool)
            || !expression_is(d, p.destination, domain_kind(c.destination))
            || !expression_is(d, p.idempotency, domain_kind(c.idempotency))
            || !schema_validation::expression(c.destination, p.destination)
            || !schema_validation::expression(c.idempotency, p.idempotency)
            || !payload(d, p.payload, c.payload)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::delivery_valid(d,plans@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::original_assignments(d,plan@,checked,root),))]
fn original_assignments(
    d: &NumericDescriptor<'_>,
    plan: &[Assignment<'_>],
    checked: &crate::finite::historical_i64::canonical_v2::schema::Checked<'_>,
    root: u32,
) -> bool {
    let Some(fields) = original::fields(checked, root) else {
        return false;
    };
    if plan.len() != fields.len() {
        return false;
    }
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),plan.len()==fields.len(),original::spec::fields(checked,root)==Some(fields),forall|j:int|0<=j<i==>spec::original_assignment(d,plan@,fields@,checked,j),decreases fields.len()-i,))]
    while i < fields.len() {
        let a = plan[i];
        if a.field != fields[i].id
            || !original::matches_type(checked, fields[i].type_id, a.domain)
            || !expression_is(d, a.value, domain_kind(a.domain))
            || !schema_validation::expression(a.domain, a.value)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::original_assignment(d,plan@,fields@,checked,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::assignments(d,plan@,fields@),))]
fn assignments(
    d: &NumericDescriptor<'_>,
    plan: &[Assignment<'_>],
    fields: &[input_view::Field<Domain<'_>>],
) -> bool {
    if let Schema::Original { checked, root, .. } = d.state {
        return original_assignments(d, plan, checked, root);
    }
    if plan.len() != fields.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),plan.len()==fields.len(),!(d.state is Original),forall|j:int|0<=j<i ==> spec::assignment_valid(d,plan@,fields@,j),decreases fields.len()-i,))]
    while i < fields.len() {
        let a = plan[i];
        if a.field != fields[i].id
            || !same_domain(a.domain, fields[i].leaf)
            || !expression_is(d, a.value, domain_kind(fields[i].leaf))
            || !schema_validation::expression(a.domain, a.value)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::assignment_valid(d,plan@,fields@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::reason(d,class,reason_id),))]
fn reason(d: &NumericDescriptor<'_>, class: Class, reason_id: Option<u32>) -> bool {
    match (class, reason_id) {
        (Class::Accept, None) => true,
        (Class::Reject, Some(id)) | (Class::CommittedFailure, Some(id)) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.reasons.len(),reason_id==Some(id),class==Class::Reject||class==Class::CommittedFailure,forall|j:int|0<=j<i ==> !(d.reasons@[j].id==id && d.reasons@[j].class==class),decreases d.reasons.len()-i,))]
            while i < d.reasons.len() {
                if d.reasons[i].id == id && same_class(d.reasons[i].class, class) {
                    #[cfg(verus_keep_ghost)]
                    proof! {assert(exists|j:int|0<=j<d.reasons@.len()&&d.reasons@[j].id==id&&d.reasons@[j].class==class);}
                    return true;
                }
                i += 1;
            }
            false
        }
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::branches(d,fields@),))]
fn branches(d: &NumericDescriptor<'_>, fields: &[input_view::Field<Domain<'_>>]) -> bool {
    if d.decision_output >= d.output_types.len() {
        return false;
    }
    let Domain::I128 { min, max } = d.output_types[d.decision_output] else {
        return false;
    };
    // A complete explicit branch table has usize length, so a signed-width
    // cardinality overflow cannot equal its length. Never wrap or clamp.
    if min > max {
        return false;
    }
    let branch_count = d.branches.len();
    let Some(distance) = max.checked_sub(min) else {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(max as int-min as int>=0);
            assert(max as int-min as int>i128::MAX as int);
            assert((usize::MAX as int) < (i128::MAX as int));
            assert(d.branches@.len()<=usize::MAX as int);
            assert(max as int-min as int+1>d.branches@.len());
        }
        return false;
    };
    let Some(cardinality) = distance.checked_add(1) else {
        #[cfg(verus_keep_ghost)]
        proof! {assert(max as int-min as int+1>d.branches@.len());}
        return false;
    };
    if cardinality != branch_count as i128 {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.branches.len(),d.decision_output<d.output_types.len(),d.output_types@[d.decision_output as int]==(Domain::I128{min,max}),min<=max,max as int-min as int+1==d.branches@.len(),forall|j:int|0<=j<i ==> spec::branch_valid(d,fields@,min,max,j),decreases d.branches.len()-i,))]
    while i < d.branches.len() {
        let b = d.branches[i];
        if b.code < min
            || b.code > max
            || (i > 0 && d.branches[i - 1].code >= b.code)
            || !reason(d, b.class, b.reason)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::branch_valid(d,fields@,min,max,i as int));}
            return false;
        }
        if matches!(b.class, Class::Reject) {
            if !b.assignments.is_empty() || !b.effects.is_empty() || !b.outbox.is_empty() {
                #[cfg(verus_keep_ghost)]
                proof! {assert(!spec::branch_valid(d,fields@,min,max,i as int));}
                return false;
            }
        } else if !assignments(d, b.assignments, fields)
            || !deliveries(d, b.effects)
            || !deliveries(d, b.outbox)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::branch_valid(d,fields@,min,max,i as int));}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::admitted(d),))]
pub(in super::super) fn admitted(d: &NumericDescriptor<'_>) -> bool {
    let fields = match d.state {
        Schema::Record(fields) => fields,
        Schema::Original { checked, root, .. } => {
            if original::fields(checked, root).is_none() {
                return false;
            }
            &[]
        }
        Schema::Leaf(_) => return false,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        if !(d.state is Record) {
            assert(fields@.len()==0);
            assert(fields@=~=Seq::empty());
        }
    }
    schema_valid(d.state)
        && schema_valid(d.command)
        && schema_valid(d.context)
        && bindings(d)
        && outputs(d)
        && d.program.admit().is_ok()
        && super::reasons(d.reasons)
        && super::channels(d.channels)
        && branches(d, fields)
        && laws::metadata(d.laws, d.required)
        && schema_validation::law_literals(d.laws)
}
