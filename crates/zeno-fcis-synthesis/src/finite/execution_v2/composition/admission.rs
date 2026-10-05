//! Complete structural contract admission, including every unused branch.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Bool,
    I128,
    U128,
    Enum(u32),
    Sum(u32),
    Bytes,
    Text,
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::domain_kind(domain),))]
fn domain_kind(domain: Domain<'_>) -> Kind {
    match domain {
        Domain::Bool => Kind::Bool,
        Domain::I128 { .. } => Kind::I128,
        Domain::U128 { .. } => Kind::U128,
        Domain::Enum { type_id, .. } => Kind::Enum(type_id),
        Domain::Sum { type_id, .. } => Kind::Sum(type_id),
        Domain::Bytes => Kind::Bytes,
        Domain::Text => Kind::Text,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::leaf_kind(*leaf),))]
fn leaf_kind(leaf: &InputLeaf) -> Kind {
    match leaf {
        InputLeaf::Bool => Kind::Bool,
        InputLeaf::I128 { .. } => Kind::I128,
        InputLeaf::U128 { .. } => Kind::U128,
        InputLeaf::Enum { type_id, .. } => Kind::Enum(*type_id),
        InputLeaf::Sum { type_id, .. } => Kind::Sum(*type_id),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::atom_kind(atom),))]
fn atom_kind(atom: Atom<'_>) -> Kind {
    match atom {
        Atom::Bool(_) => Kind::Bool,
        Atom::I128(_) => Kind::I128,
        Atom::U128(_) => Kind::U128,
        Atom::Enum { type_id, .. } => Kind::Enum(type_id),
        Atom::Sum { type_id, .. } => Kind::Sum(type_id),
        Atom::Bytes(_) => Kind::Bytes,
        Atom::Text(_) => Kind::Text,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==(a==b),))]
fn same_kind(a: Kind, b: Kind) -> bool {
    match (a, b) {
        (Kind::Bool, Kind::Bool)
        | (Kind::I128, Kind::I128)
        | (Kind::U128, Kind::U128)
        | (Kind::Bytes, Kind::Bytes)
        | (Kind::Text, Kind::Text) => true,
        (Kind::Enum(a), Kind::Enum(b)) | (Kind::Sum(a), Kind::Sum(b)) => a == b,
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::schema(d,source),))]
fn schema<'a>(d: &Descriptor<'a>, source: Source) -> Schema<'a> {
    match source {
        Source::State => d.state,
        Source::Command => d.command,
        Source::Context => d.context,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::field_leaf(fields@,id),))]
fn field_leaf(fields: &[InputField], id: u16) -> Option<&InputLeaf> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),forall|j:int|0<=j<i==>fields@[j].id!=id,decreases fields.len()-i,))]
    while i < fields.len() {
        if fields[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {assert(spec::first(fields@,id,i as int));assert(exists|j:int|spec::first(fields@,id,j));spec::first_chosen(fields@,id,i as int);}
            return Some(&fields[i].leaf);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::find(schema,selector),))]
pub(super) fn find<'a>(schema: Schema<'a>, selector: Selector) -> Option<&'a InputLeaf> {
    match (schema, selector) {
        (Schema::Leaf(leaf), Selector::Root) => Some(leaf),
        (Schema::Record(fields), Selector::Field(id)) => field_leaf(fields, id),
        _ => None,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::expression(d,expr),))]
fn expression(d: &Descriptor<'_>, expr: Expr<'_>) -> Option<Kind> {
    match expr {
        Expr::Input(s, id) => Some(leaf_kind(find(schema(d, s), Selector::Field(id))?)),
        Expr::Root(s) => Some(leaf_kind(find(schema(d, s), Selector::Root)?)),
        Expr::Output(i) => {
            if i < d.output_types.len() {
                Some(leaf_kind(&d.output_types[i]))
            } else {
                None
            }
        }
        Expr::Constant(a) => Some(atom_kind(a)),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==(spec::expression(d,expr)==Some(kind)),))]
fn expression_is(d: &Descriptor<'_>, expr: Expr<'_>, kind: Kind) -> bool {
    match expression(d, expr) {
        Some(k) => same_kind(k, kind),
        None => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::domain_valid(domain),))]
fn domain_valid(domain: Domain<'_>) -> bool {
    match domain {
        Domain::I128 { min, max } => min <= max,
        Domain::U128 { min, max } => min <= max,
        Domain::Enum { variants, .. } | Domain::Sum { variants, .. } => unique_variants(variants),
        _ => true,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::unique_variants(variants@),))]
fn unique_variants(variants: &[u16]) -> bool {
    if variants.is_empty() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=variants.len(),variants.len()>0,
        forall|j:int,k:int|0<=j<k<i==>variants@[j]!=variants@[k],decreases variants.len()-i,))]
    while i < variants.len() {
        let mut j = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(invariant j<=i<variants.len(),variants.len()>0,
            forall|k:int|0<=k<j==>variants@[k]!=variants@[i as int],decreases i-j,))]
        while j < i {
            if variants[j] == variants[i] {
                #[cfg(verus_keep_ghost)]
                proof! {assert(!spec::unique_variants(variants@));}
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::variants_match(ids@,variants@),))]
fn variants_match(ids: &[u16], variants: &[super::super::InputVariant]) -> bool {
    if ids.len() != variants.len() || !unique_variants(ids) {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=ids.len(),ids.len()==variants.len(),spec::unique_variants(ids@),forall|j:int|0<=j<i==>spec::has_variant(variants@,ids@[j]),decreases ids.len()-i,))]
    while i < ids.len() {
        if !has_variant(variants, ids[i]) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::variants_match(ids@,variants@));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::has_variant(variants@,id),))]
fn has_variant(variants: &[super::super::InputVariant], id: u16) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=variants.len(),forall|j:int|0<=j<i==>variants@[j].id!=id,decreases variants.len()-i,))]
    while i < variants.len() {
        if variants[i].id == id {
            #[cfg(verus_keep_ghost)]
            proof! {assert(exists|j:int|0<=j<variants@.len()&&variants@[j].id==id);}
            return true;
        }
        i += 1;
    }
    false
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::matches_leaf(domain,*leaf),))]
fn matches_leaf(domain: Domain<'_>, leaf: &InputLeaf) -> bool {
    match (domain, leaf) {
        (Domain::Bool, InputLeaf::Bool) => true,
        (Domain::U128 { min: a, max: b }, InputLeaf::U128 { min: c, max: d }) => a == *c && b == *d,
        (Domain::I128 { min: a, max: b }, InputLeaf::I128 { min: c, max: d }) => {
            a == *c as i128 && b == *d as i128
        }
        (
            Domain::Enum {
                type_id: a,
                variants: x,
            },
            InputLeaf::Enum {
                type_id: b,
                variants: y,
                ..
            },
        )
        | (
            Domain::Sum {
                type_id: a,
                variants: x,
            },
            InputLeaf::Sum {
                type_id: b,
                variants: y,
                ..
            },
        ) => a == *b && variants_match(x, y),
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::same_binding(a,b),))]
fn same_binding(a: Binding, b: Binding) -> bool {
    let source = matches!(
        (a.source, b.source),
        (Source::State, Source::State)
            | (Source::Command, Source::Command)
            | (Source::Context, Source::Context)
    );
    source
        && match (a.selector, b.selector) {
            (Selector::Root, Selector::Root) => true,
            (Selector::Field(a), Selector::Field(b)) => a == b,
            _ => false,
        }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::count(schema),))]
fn count(schema: Schema<'_>) -> usize {
    match schema {
        Schema::Leaf(_) => 1,
        Schema::Record(fields) => fields.len(),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::same_domain(a,b),))]
fn same_domain(a: ScalarDomain, b: ScalarDomain) -> bool {
    match (a, b) {
        (ScalarDomain::Bool, ScalarDomain::Bool) => true,
        (ScalarDomain::Int { min: a, max: b }, ScalarDomain::Int { min: c, max: d }) => {
            a == c && b == d
        }
        _ => false,
    }
}
// A closed variant map may be the original representation of a Boolean ABI
// input. Only the complete {0,1} code domain qualifies; raw variants remain
// distinct typed atoms in the candidate and law frame.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::input_domain(*leaf,domain),))]
fn input_domain(leaf: &InputLeaf, domain: ScalarDomain) -> bool {
    if same_domain(input_view::scalar_domain(leaf), domain) {
        return true;
    }
    match (leaf, domain) {
        (
            InputLeaf::Enum { min, max, .. } | InputLeaf::Sum { min, max, .. },
            ScalarDomain::Bool,
        ) => *min == 0 && *max == 1,
        _ => false,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::bindings(d),))]
pub(super) fn bindings(d: &Descriptor<'_>) -> bool {
    if d.bindings.len() != d.program.inputs.len()
        || d.bindings.len() as u128
            != count(d.state) as u128 + count(d.command) as u128 + count(d.context) as u128
    {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.bindings.len(),d.bindings.len()==d.program.inputs.len(),forall|j:int|0<=j<i==>spec::binding_valid(d,j),decreases d.bindings.len()-i,))]
    while i < d.bindings.len() {
        let b = d.bindings[i];
        let Some(leaf) = find(schema(d, b.source), b.selector) else {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::binding_valid(d,i as int));}
            return false;
        };
        if !input_domain(leaf, d.program.inputs[i]) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::binding_valid(d,i as int));}
            return false;
        }
        let mut j = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(invariant j<=i<d.bindings.len(),b==d.bindings@[i as int],d.bindings.len()==d.program.inputs.len(),spec::find(spec::schema(d,b.source),b.selector)==Some(leaf),spec::input_domain(*leaf,d.program.inputs@[i as int]),forall|k:int|0<=k<j==>!spec::same_binding(d.bindings@[k],d.bindings@[i as int]),decreases i-j,))]
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
fn outputs(d: &Descriptor<'_>) -> bool {
    if d.output_types.len() != d.program.outputs.len() || d.decision_output >= d.output_types.len()
    {
        return false;
    }
    if !matches!(d.output_types[d.decision_output], InputLeaf::I128 { .. }) {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.output_types.len(),d.output_types.len()==d.program.outputs.len(),forall|j:int|0<=j<i==>spec::output_valid(d,j),decreases d.output_types.len()-i,))]
    while i < d.output_types.len() {
        if !input_view::validate_leaf(&d.output_types[i])
            || !same_domain(
                input_view::scalar_domain(&d.output_types[i]),
                d.program.outputs[i],
            )
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::output_valid(d,i as int));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::channel(d,id),))]
pub(super) fn channel<'a>(d: &Descriptor<'a>, id: u32) -> Option<Channel<'a>> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.channels.len(),forall|j:int|0<=j<i==>d.channels@[j].id!=id,decreases d.channels.len()-i,))]
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
fn payload(d: &Descriptor<'_>, fields: &[PayloadField<'_>], schema: &[TypedField<'_>]) -> bool {
    if fields.len() != schema.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),fields.len()==schema.len(),forall|j:int|0<=j<i==>spec::payload_entry(d,fields@,schema@,j),decreases fields.len()-i,))]
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
fn deliveries(d: &Descriptor<'_>, plans: &[DeliveryPlan<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=plans.len(),forall|j:int|0<=j<i==>spec::delivery_valid(d,plans@,j),decreases plans.len()-i,))]
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
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::assignments(d,plan@,fields@),))]
fn assignments(d: &Descriptor<'_>, plan: &[Assignment<'_>], fields: &[InputField]) -> bool {
    if plan.len() != fields.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),plan.len()==fields.len(),forall|j:int|0<=j<i==>spec::assignment_valid(d,plan@,fields@,j),decreases fields.len()-i,))]
    while i < fields.len() {
        let a = plan[i];
        if a.field != fields[i].id
            || !matches_leaf(a.domain, &fields[i].leaf)
            || !expression_is(d, a.value, leaf_kind(&fields[i].leaf))
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
fn reason(d: &Descriptor<'_>, class: Class, reason_id: Option<u32>) -> bool {
    match (class, reason_id) {
        (Class::Accept, None) => true,
        (Class::Reject, Some(id)) | (Class::CommittedFailure, Some(id)) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.reasons.len(),reason_id==Some(id),class==Class::Reject||class==Class::CommittedFailure,forall|j:int|0<=j<i==>d.reasons@[j].id!=id||d.reasons@[j].class!=class,decreases d.reasons.len()-i,))]
            while i < d.reasons.len() {
                let same = same_class(class, d.reasons[i].class);
                if d.reasons[i].id == id && same {
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
fn branches(d: &Descriptor<'_>, fields: &[InputField]) -> bool {
    if d.decision_output >= d.output_types.len() {
        return false;
    }
    let InputLeaf::I128 { min, max } = d.output_types[d.decision_output] else {
        return false;
    };
    if min > max || max as i128 - min as i128 + 1 != d.branches.len() as i128 {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.branches.len(),d.decision_output<d.output_types.len(),d.output_types@[d.decision_output as int]==(InputLeaf::I128{min,max}),min<=max,max as int-min as int+1==d.branches@.len(),forall|j:int|0<=j<i==>spec::branch_valid(d,fields@,min,max,j),decreases d.branches.len()-i,))]
    while i < d.branches.len() {
        let b = d.branches[i];
        if b.code < min as i128
            || b.code > max as i128
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
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::reasons(d.reasons@),))]
fn reasons(d: &Descriptor<'_>) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.reasons.len(),forall|j:int|0<=j<i==>spec::reason_valid(d.reasons@,j),decreases d.reasons.len()-i,))]
    while i < d.reasons.len() {
        if d.reasons[i].id == 0
            || matches!(d.reasons[i].class, Class::Accept)
            || (i > 0 && d.reasons[i - 1].id >= d.reasons[i].id)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::reason_valid(d.reasons@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::channel_fields(fields@),))]
fn channel_fields(fields: &[TypedField<'_>]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fields.len(),forall|j:int|0<=j<i==>spec::channel_field_valid(fields@,j),decreases fields.len()-i,))]
    while i < fields.len() {
        if !domain_valid(fields[i].domain) || (i > 0 && fields[i - 1].field >= fields[i].field) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::channel_field_valid(fields@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::channels(d.channels@),))]
fn channels(d: &Descriptor<'_>) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=d.channels.len(),forall|j:int|0<=j<i==>spec::channel_valid(d.channels@,j),decreases d.channels.len()-i,))]
    while i < d.channels.len() {
        let c = d.channels[i];
        if c.id == 0
            || (i > 0 && d.channels[i - 1].id >= c.id)
            || !domain_valid(c.destination)
            || !domain_valid(c.idempotency)
            || !channel_fields(c.payload)
        {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::channel_valid(d.channels@,i as int));}
            return false;
        }
        i += 1;
    }
    true
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::admitted(d),))]
pub(super) fn admitted(d: &Descriptor<'_>) -> bool {
    let Schema::Record(fields) = d.state else {
        return false;
    };
    ingress::valid(d.state)
        && ingress::valid(d.command)
        && ingress::valid(d.context)
        && bindings(d)
        && outputs(d)
        && super::super::super::evaluation::admission::validate_program(
            d.program.inputs,
            d.program.outputs,
            d.program.nodes,
            d.program.roots,
        )
        .is_ok()
        && reasons(d)
        && channels(d)
        && branches(d, fields)
        && laws::metadata(d.laws, d.required)
        && schema_validation::law_literals(d.laws)
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==(a==b),))]
fn same_class(a: Class, b: Class) -> bool {
    matches!(
        (a, b),
        (Class::Accept, Class::Accept)
            | (Class::Reject, Class::Reject)
            | (Class::CommittedFailure, Class::CommittedFailure)
    )
}
