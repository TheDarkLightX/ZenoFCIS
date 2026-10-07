//! Total closed metadata admission for the original schema witness.
use super::{Definition, Description, Field, Kind, SumVariant, Variant};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Construction-time schema policy; these limits do not authorize execution.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum complete original schema bytes.
    pub bytes: u64,
    /// Maximum complete type definitions.
    pub types: u32,
    /// Maximum fields in each record or items in each tuple.
    pub fields: u32,
    /// Maximum variants in each enum or sum.
    pub variants: u32,
}

/// Exact construction-time refusal order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The complete schema exceeds its byte policy.
    Size,
    /// The complete proposed metadata is invalid, including unknown or cyclic references.
    Metadata,
    /// The proposal is not the exact original canonical encoding.
    Encoding,
}

/// A schema whose complete metadata and original bytes have been checked.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Checked<'a> {
    original: &'a [u8],
    description: &'a Description<'a>,
}

impl<'a> Checked<'a> {
    /// Gets the complete original canonical schema bytes.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().0,))]
    pub fn original(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Checked::view); }
        self.original
    }

    /// Gets the complete description bound to those original bytes.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures *result == self.view().1,))]
    pub fn description(&self) -> &'a Description<'a> {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Checked::view); }
        self.description
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a> Checked<'a> {
    pub closed spec fn view(&self) -> (Seq<u8>, Description<'a>) {
        (self.original@, *self.description)
    }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left@ == right@),))]
fn equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= left.len(), left.len() == right.len(),
            forall|j:int| 0 <= j < index ==> left@[j] == right@[j],
        decreases left.len()-index,
    ))]
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(left@ =~= right@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::name_valid(bytes@),))]
fn name_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 96 {
        return false;
    }
    let first = bytes[0];
    if !matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'_') {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bytes.len(), 0 < bytes.len() <= 96,
            spec::first(bytes@[0]),
            forall|j:int| 0 <= j < index ==> spec::character(bytes@[j]),
        decreases bytes.len()-index,
    ))]
    while index < bytes.len() {
        let b = bytes[index];
        if !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_') {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::find(definitions@, id, definitions@.len()),
    match result { Some(index) => index < definitions@.len() && definitions@[index as int].id == id, None => true },))]
pub(super) fn find(definitions: &[Definition<'_>], id: u32) -> Option<usize> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= definitions.len(),spec::find(definitions@,id,index as nat)==None::<usize>,
        decreases definitions.len()-index,
    ))]
    while index < definitions.len() {
        if definitions[index].id == id {
            #[cfg(verus_keep_ghost)]
            proof! { spec::find_persists(definitions@,id,(index+1) as nat,definitions@.len() as nat,index); }
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::leaf(*kind),))]
fn leaf(kind: &Kind<'_>) -> bool {
    !matches!(kind, Kind::Record(_))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::variants(variants@, max),))]
fn variants_valid(variants: &[Variant<'_>], max: u32) -> bool {
    if variants.len() as u64 > max as u64 {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index<=variants.len(),variants.len()<=max,
            spec::variants_prefix(variants@,index as nat),
        decreases variants.len()-index,
    ))]
    while index < variants.len() {
        if !name_valid(variants[index].name)
            || (index > 0 && variants[index - 1].id >= variants[index].id)
        {
            #[cfg(verus_keep_ghost)]
            proof! { spec::variants_failure(variants@,(index+1) as nat,variants@.len() as nat); }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior<=index<variants.len(),spec::variants_prefix(variants@,index as nat),
                spec::name_valid(variants@[index as int].name@),
                index==0 || variants@[index as int-1].id<variants@[index as int].id,
                forall|j:int| 0<=j<prior ==> #[trigger] variants@[j].name@ != variants@[index as int].name@,
            decreases index-prior,
        ))]
        while prior < index {
            if equal(variants[prior].name, variants[index].name) {
                #[cfg(verus_keep_ghost)]
                proof! { spec::variants_failure(variants@,(index+1) as nat,variants@.len() as nat); }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::sum_variants(variants@, max),))]
fn sum_variants_valid(variants: &[SumVariant<'_>], max: u32) -> bool {
    if variants.len() as u64 > max as u64 {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index<=variants.len(),variants.len()<=max,
            spec::sum_variants_prefix(variants@,index as nat),
        decreases variants.len()-index,
    ))]
    while index < variants.len() {
        if !name_valid(variants[index].name)
            || (index > 0 && variants[index - 1].id >= variants[index].id)
        {
            #[cfg(verus_keep_ghost)]
            proof! { spec::sum_variants_failure(variants@,(index+1) as nat,variants@.len() as nat); }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior<=index<variants.len(),spec::sum_variants_prefix(variants@,index as nat),
                spec::name_valid(variants@[index as int].name@),
                index==0 || variants@[index as int-1].id<variants@[index as int].id,
                forall|j:int| 0<=j<prior ==> #[trigger] variants@[j].name@ != variants@[index as int].name@,
            decreases index-prior,
        ))]
        while prior < index {
            if equal(variants[prior].name, variants[index].name) {
                #[cfg(verus_keep_ghost)]
                proof! { spec::sum_variants_failure(variants@,(index+1) as nat,variants@.len() as nat); }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::fields(fields@,definitions@,max),))]
fn fields_valid(fields: &[Field<'_>], definitions: &[Definition<'_>], max: u32) -> bool {
    if fields.len() as u64 > max as u64 {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index<=fields.len(),fields.len()<=max,
            spec::fields_prefix(fields@,definitions@,index as nat),
        decreases fields.len()-index,
    ))]
    while index < fields.len() {
        if !name_valid(fields[index].name)
            || (index > 0 && fields[index - 1].id >= fields[index].id)
        {
            #[cfg(verus_keep_ghost)]
            proof! { spec::fields_failure(fields@,definitions@,(index+1) as nat,fields@.len() as nat); }
            return false;
        }
        let target = match find(definitions, fields[index].type_id) {
            Some(target) => target,
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::fields_failure(fields@,definitions@,(index+1) as nat,fields@.len() as nat); }
                return false;
            }
        };
        if target >= definitions.len() {
            #[cfg(verus_keep_ghost)]
            proof! { spec::fields_failure(fields@,definitions@,(index+1) as nat,fields@.len() as nat); }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior<=index<fields.len(),spec::fields_prefix(fields@,definitions@,index as nat),
                spec::field_valid(fields@[index as int],definitions@),
                index==0 || fields@[index as int-1].id<fields@[index as int].id,
                forall|j:int| 0<=j<prior ==> #[trigger] fields@[j].name@ != fields@[index as int].name@,
            decreases index-prior,
        ))]
        while prior < index {
            if equal(fields[prior].name, fields[index].name) {
                #[cfg(verus_keep_ghost)]
                proof! { spec::fields_failure(fields@,definitions@,(index+1) as nat,fields@.len() as nat); }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::kind_valid(*kind,definitions@,limits),))]
fn kind_valid(kind: &Kind<'_>, definitions: &[Definition<'_>], limits: Limits) -> bool {
    match kind {
        Kind::Unit | Kind::Bool => true,
        Kind::U128 { min, max } => min <= max,
        Kind::I128 { min, max } => min <= max,
        Kind::Bytes { min, max } | Kind::Text { min, max } => min <= max,
        Kind::Enum(v) | Kind::Sum(v) => variants_valid(v, limits.variants),
        Kind::Record(f) => fields_valid(f, definitions, limits.fields),
        Kind::Tuple(items) => items.len() as u64 <= limits.fields as u64,
        Kind::SumPayload(v) => sum_variants_valid(v, limits.variants),
        Kind::Vector { min, max, .. } | Kind::Map { min, max, .. } => min <= max,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as nat==spec::field_count(*kind),))]
fn field_count(kind: &Kind<'_>) -> u64 {
    if leaf(kind) {
        0
    } else {
        match kind {
            Kind::Record(fields) => fields.len() as u64,
            _ => 0,
        }
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as nat==spec::variant_count(*kind),))]
fn variant_count(kind: &Kind<'_>) -> u64 {
    match kind {
        Kind::Enum(v) | Kind::Sum(v) => v.len() as u64,
        Kind::SumPayload(v) => v.len() as u64,
        _ => 0,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::definitions_valid(definitions@,limits),))]
fn definitions_valid(definitions: &[Definition<'_>], limits: Limits) -> bool {
    if definitions.is_empty() || definitions.len() as u64 > limits.types as u64 {
        return false;
    }
    let mut index = 0usize;
    let mut fields = 0u64;
    let mut variants = 0u64;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index<=definitions.len(),0<definitions.len()<=limits.types,
            fields<=u32::MAX,variants<=u32::MAX,
            fields as nat==spec::total_fields(definitions@,index as nat),
            variants as nat==spec::total_variants(definitions@,index as nat),
            spec::definitions_prefix(definitions@,limits,index as nat),
        decreases definitions.len()-index,
    ))]
    while index < definitions.len() {
        if !name_valid(definitions[index].name)
            || (index > 0 && definitions[index - 1].id >= definitions[index].id)
            || !kind_valid(&definitions[index].kind, definitions, limits)
        {
            #[cfg(verus_keep_ghost)]
            proof! { spec::definitions_failure(definitions@,limits,(index+1) as nat,definitions@.len() as nat); }
            return false;
        }
        let count_fields = field_count(&definitions[index].kind);
        let count_variants = variant_count(&definitions[index].kind);
        #[cfg(verus_keep_ghost)]
        proof! { spec::kind_counts_bounded(definitions@[index as int].kind,definitions@,limits); }
        fields += count_fields;
        variants += count_variants;
        if fields > u32::MAX as u64 || variants > u32::MAX as u64 {
            #[cfg(verus_keep_ghost)]
            proof! { spec::definitions_failure(definitions@,limits,(index+1) as nat,definitions@.len() as nat); }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior<=index<definitions.len(),spec::definitions_prefix(definitions@,limits,index as nat),
                spec::name_valid(definitions@[index as int].name@),
                spec::kind_valid(definitions@[index as int].kind,definitions@,limits),
                index==0 || definitions@[index as int-1].id<definitions@[index as int].id,
                fields as nat==spec::total_fields(definitions@,(index+1) as nat),fields<=u32::MAX,
                variants as nat==spec::total_variants(definitions@,(index+1) as nat),variants<=u32::MAX,
                forall|j:int| 0<=j<prior ==> #[trigger] definitions@[j].name@ != definitions@[index as int].name@,
            decreases index-prior,
        ))]
        while prior < index {
            if equal(definitions[prior].name, definitions[index].name) {
                #[cfg(verus_keep_ghost)]
                proof! { spec::definitions_failure(definitions@,limits,(index+1) as nat,definitions@.len() as nat); }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == spec::references(*kind),))]
fn references(kind: &Kind<'_>) -> alloc::vec::Vec<u32> {
    let mut result = alloc::vec::Vec::new();
    match kind {
        Kind::Tuple(items) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=items.len(), result@==items@.take(i as int), decreases items.len()-i,))]
            while i < items.len() {
                result.push(items[i]);
                i += 1;
            }
        }
        Kind::Record(fields) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=fields.len(), result@==spec::field_references(fields@.take(i as int)), decreases fields.len()-i,))]
            while i < fields.len() {
                result.push(fields[i].type_id);
                i += 1;
            }
        }
        Kind::SumPayload(variants) => {
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=variants.len(), result@==spec::sum_references(variants@,i as nat), decreases variants.len()-i,))]
            while i < variants.len() {
                if let Some(id) = variants[i].payload {
                    result.push(id);
                }
                i += 1;
            }
        }
        Kind::Vector { element, .. } => result.push(*element),
        Kind::Map { key, value, .. } => {
            result.push(*key);
            result.push(*value);
        }
        _ => {}
    }
    result
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result {Some(edges)=>Some(edges@),None=>None})==spec::edge_indices(definitions@,ids@,ids@.len()),
))]
fn edge_indices(definitions: &[Definition<'_>], ids: &[u32]) -> Option<alloc::vec::Vec<usize>> {
    let mut edges = alloc::vec::Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=ids.len(),Some(edges@)==spec::edge_indices(definitions@,ids@,i as nat),
        decreases ids.len()-i,
    ))]
    while i < ids.len() {
        let Some(index) = find(definitions, ids[i]) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::edges_failure(definitions@,ids@,(i+1) as nat,ids@.len());}
            return None;
        };
        edges.push(index);
        i += 1;
    }
    Some(edges)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result {Some(rows)=>Some(spec::graph_view(rows@)),None=>None})==spec::graph_prefix(definitions@,definitions@.len()),
))]
fn index_graph(definitions: &[Definition<'_>]) -> Option<alloc::vec::Vec<alloc::vec::Vec<usize>>> {
    let mut rows = alloc::vec::Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=definitions.len(),Some(spec::graph_view(rows@))==spec::graph_prefix(definitions@,i as nat),
        decreases definitions.len()-i,
    ))]
    while i < definitions.len() {
        let ids = references(&definitions[i].kind);
        let Some(row) = edge_indices(definitions, &ids) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::graph_failure(definitions@,(i+1) as nat,definitions@.len());}
            return None;
        };
        rows.push(row);
        #[cfg(verus_keep_ghost)]
        proof! {assert(spec::graph_view(rows@) =~= spec::graph_prefix(definitions@,(i+1) as nat).unwrap());}
        i += 1;
    }
    Some(rows)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::row_prefix(row@,resolved@,row@.len()),))]
fn row_ready(row: &[usize], resolved: &[bool]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=row.len(),spec::row_prefix(row@,resolved@,i as nat),
        decreases row.len()-i,
    ))]
    while i < row.len() {
        if row[i] >= resolved.len() || !resolved[row[i]] {
            #[cfg(verus_keep_ghost)]
            proof! {spec::row_failure(row@,resolved@,(i+1) as nat,row@.len());}
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::closed(definitions@),))]
fn closed(definitions: &[Definition<'_>]) -> bool {
    let Some(graph) = index_graph(definitions) else {
        return false;
    };
    let mut resolved = alloc::vec::Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=graph.len(),resolved@==Seq::new(i as nat,|_:int|false),
        decreases graph.len()-i,
    ))]
    while i < graph.len() {
        resolved.push(false);
        i += 1;
    }
    let mut round = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant round<=definitions.len(),resolved.len()==graph.len(),
            resolved@==spec::settled(spec::graph_view(graph@),round as nat),
            Some(spec::graph_view(graph@))==spec::graph_prefix(definitions@,definitions@.len()),
        decreases definitions.len()-round,
    ))]
    while round < definitions.len() {
        let mut next = alloc::vec::Vec::new();
        let mut node = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant node<=graph.len(),resolved.len()==graph.len(),
                next@==spec::advance(spec::graph_view(graph@),resolved@).take(node as int),
            decreases graph.len()-node,
        ))]
        while node < graph.len() {
            next.push(resolved[node] || row_ready(&graph[node], &resolved));
            node += 1;
        }
        resolved = next;
        round += 1;
    }
    let mut node = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant node<=resolved.len(),spec::all_prefix(resolved@,node as nat),
            resolved@==spec::settled(spec::graph_view(graph@),definitions@.len()),
            Some(spec::graph_view(graph@))==spec::graph_prefix(definitions@,definitions@.len()),
        decreases resolved.len()-node,
    ))]
    while node < resolved.len() {
        if !resolved[node] {
            #[cfg(verus_keep_ghost)]
            proof! {spec::all_failure(resolved@,(node+1) as nat,resolved@.len());}
            return false;
        }
        node += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==spec::metadata(*description,limits),))]
fn metadata(description: &Description<'_>, limits: Limits) -> bool {
    name_valid(description.profile)
        && definitions_valid(description.definitions, limits)
        && find(description.definitions, description.root).is_some()
        && closed(description.definitions)
}

/// Admits a complete original schema and retains exact immutable custody.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result {Ok(checked)=>Ok(checked.view()),Err(f)=>Err(f)})
        ==spec::admit(bytes@,*description,limits),
))]
pub fn admit<'a>(
    bytes: &'a [u8],
    description: &'a Description<'a>,
    limits: Limits,
) -> Result<Checked<'a>, Failure> {
    if bytes.len() as u64 > limits.bytes {
        return Err(Failure::Size);
    }
    if !metadata(description, limits) {
        return Err(Failure::Metadata);
    }
    if !super::encoding_matches(bytes, description, limits.bytes) {
        return Err(Failure::Encoding);
    }
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Checked::view); }
    Ok(Checked {
        original: bytes,
        description,
    })
}
