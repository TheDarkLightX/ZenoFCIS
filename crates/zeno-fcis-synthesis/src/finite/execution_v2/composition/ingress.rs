//! Root-aware projection of original canonical bytes, with no envelope rewrite.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Original canonical scalar root, flat record, or complete checked schema.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Schema<'a> {
    /// One original primitive, Enum, or payload-free Sum root.
    Leaf(&'a InputLeaf),
    /// Every declared field in canonical identifier order.
    Record(&'a [InputField]),
}
/// Protected input selector; a root is distinct from every record identifier.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Selector {
    /// Complete scalar root.
    Root,
    /// Declared record field.
    Field(u16),
}
/// An actual protected input read, including denied attempts.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Read {
    /// State is 0, command is 1, and context is 2.
    pub source: u8,
    /// Exact original root or field selector.
    pub selector: Selector,
    /// Whether the shared meter granted the read before interpretation.
    pub permitted: bool,
}
/// Reified actual ingress value, owned by the library producer.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) enum Value<'a> {
    /// Original scalar root with its original type and variant identifiers.
    Leaf(Atom<'a>),
    /// Complete ordered original record.
    Record(Vec<Field<'a>>),
}
/// Typed actual value and scalar ABI values obtained from the same bytes.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(super) struct Decoded<'a> {
    /// Complete typed original value.
    pub value: Value<'a>,
    /// Scalar codes in schema order.
    pub scalars: Vec<i64>,
}
/// Typed ingress refusal before a candidate can be exposed.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// The complete declared ingress schema is invalid.
    Schema,
    /// Actual canonical record projection refused.
    Record(input_view::Failure),
    /// The original scalar root is malformed or outside its declared domain.
    Root,
    /// A scalar code has no corresponding declared typed value.
    Reification,
    /// The shared meter denied byte or root-read work.
    Budget(super::super::MeterFailure),
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::schema_valid(schema),))]
pub(super) fn valid(schema: Schema<'_>) -> bool {
    match schema {
        Schema::Leaf(leaf) => input_view::validate_leaf(leaf),
        Schema::Record(fields) => input_view::validate_schema(fields),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::inverse(variants@,code,variants@.len()),))]
fn inverse(variants: &[super::super::InputVariant], code: i64) -> Option<u16> {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=variants.len(),spec::inverse(variants@,code,i as nat)==None::<u16>,decreases variants.len()-i,))]
    while i < variants.len() {
        if variants[i].code == code {
            #[cfg(verus_keep_ghost)]
            proof! {spec::inverse_found(variants@,code,(i+1) as nat,variants@.len());}
            return Some(variants[i].id);
        }
        i += 1;
    }
    None
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::reify(*leaf,code),))]
pub(super) fn reify(leaf: &InputLeaf, code: i64) -> Option<Atom<'static>> {
    match leaf {
        InputLeaf::Bool => {
            if code == 0 {
                Some(Atom::Bool(false))
            } else if code == 1 {
                Some(Atom::Bool(true))
            } else {
                None
            }
        }
        InputLeaf::U128 { min, max } => {
            if code >= 0 && *min <= code as u128 && code as u128 <= *max {
                Some(Atom::U128(code as u128))
            } else {
                None
            }
        }
        InputLeaf::I128 { min, max } => {
            if *min <= code && code <= *max {
                Some(Atom::I128(code as i128))
            } else {
                None
            }
        }
        InputLeaf::Enum {
            type_id, variants, ..
        } => Some(Atom::Enum {
            type_id: *type_id,
            variant: inverse(variants, code)?,
        }),
        InputLeaf::Sum {
            type_id, variants, ..
        } => Some(Atom::Sum {
            type_id: *type_id,
            variant: inverse(variants, code)?,
        }),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures (match result {Some(v)=>Some(v@),None=>None::<Seq<Field>>}) == spec::fields(fields@,codes@,fields@.len()),))]
fn reify_fields(fields: &[InputField], codes: &[i64]) -> Option<Vec<Field<'static>>> {
    if fields.len() != codes.len() {
        return None;
    }
    let mut output = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=fields.len(),fields.len()==codes.len(),spec::fields(fields@,codes@,i as nat)==Some(output@),decreases fields.len()-i,))]
    while i < fields.len() {
        let Some(value) = reify(&fields[i].leaf, codes[i]) else {
            #[cfg(verus_keep_ghost)]
            proof! {spec::fields_failed(fields@,codes@,(i+1) as nat,fields@.len());}
            return None;
        };
        output.push(Field {
            id: fields[i].id,
            value,
        });
        i += 1;
    }
    Some(output)
}
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(reads)@==old(reads)@+spec::attempts(source,attempts@),))]
fn append_reads(source: u8, attempts: &[super::super::AccessAttempt], reads: &mut Vec<Read>) {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=attempts.len(),reads@==old(reads)@+spec::attempts(source,attempts@.take(i as int)),decreases attempts.len()-i,))]
    while i < attempts.len() {
        reads.push(Read {
            source,
            selector: Selector::Field(attempts[i].field_id()),
            permitted: attempts[i].permitted(),
        });
        i += 1;
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures final(meter).limits==old(meter).limits,
    (match result {Ok(v)=>Ok(spec::decoded(v)),Err(e)=>Err(e)},final(meter).used.counters@,final(reads)@)=={
        let r=spec::project(bytes@,schema,source,old(meter).limits.counters@,old(meter).used.counters@);
        (r.0,r.1,old(reads)@+r.2)
    },))]
pub(super) fn project<'a>(
    bytes: &'a [u8],
    schema: Schema<'a>,
    source: u8,
    meter: &mut Meter,
    reads: &mut Vec<Read>,
) -> Result<Decoded<'a>, Failure> {
    if !valid(schema) {
        return Err(Failure::Schema);
    }
    match schema {
        Schema::Record(fields) => {
            let mut codes = Vec::new();
            let mut attempts = Vec::new();
            let projected =
                input_view::project_into(bytes, fields, meter, &mut codes, &mut attempts);
            append_reads(source, &attempts, reads);
            if let Err(e) = projected {
                return Err(Failure::Record(e));
            }
            let Some(values) = reify_fields(fields, &codes) else {
                return Err(Failure::Reification);
            };
            Ok(Decoded {
                value: Value::Record(values),
                scalars: codes,
            })
        }
        Schema::Leaf(leaf) => {
            if let Err(e) = meter.charge(Resource::Byte, bytes.len() as u64) {
                return Err(Failure::Budget(e));
            }
            let charged = meter.charge(Resource::Read, 1);
            reads.push(Read {
                source,
                selector: Selector::Root,
                permitted: charged.is_ok(),
            });
            if let Err(e) = charged {
                return Err(Failure::Budget(e));
            }
            let Some((code, end)) = input_view::decode_scalar(bytes, 0, leaf) else {
                return Err(Failure::Root);
            };
            if end != bytes.len() {
                return Err(Failure::Root);
            }
            let Some(atom) = reify(leaf, code) else {
                return Err(Failure::Reification);
            };
            let scalars = alloc::vec![code];
            #[cfg(verus_keep_ghost)]
            proof! {assert(scalars@=~=seq![code]);}
            Ok(Decoded {
                value: Value::Leaf(atom),
                scalars,
            })
        }
    }
}
