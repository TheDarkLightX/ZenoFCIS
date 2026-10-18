//! Closed schema view driving the one original-byte ingress path.
use super::super::super::input_view::typed::{self, FieldsView, LeafView};
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "shared_spec.rs"]
pub(in super::super) mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum SchemaView<'a> {
    Legacy(Schema<'a>),
    Numeric(Schema<'a, Domain<'a>>),
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape<'a> {
    Empty,
    Leaf(LeafView<'a>),
    Record(FieldsView<'a>),
}
impl<'a> SchemaView<'a> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::shape(*self),))]
    pub(in super::super) fn shape(&self) -> Shape<'a> {
        match *self {
            Self::Legacy(Schema::Original { .. }) => Shape::Empty,
            Self::Numeric(Schema::Original { projection, .. }) => match projection {
                Projection::None => Shape::Empty,
                Projection::Root(d) => Shape::Leaf(LeafView::Numeric(*d)),
                Projection::Fields(f) => Shape::Record(FieldsView::Numeric(f)),
            },
            Self::Legacy(Schema::Leaf(l)) => Shape::Leaf(LeafView::Legacy(l)),
            Self::Legacy(Schema::Record(f)) => Shape::Record(FieldsView::Legacy(f)),
            Self::Numeric(Schema::Leaf(l)) => Shape::Leaf(LeafView::Numeric(*l)),
            Self::Numeric(Schema::Record(f)) => Shape::Record(FieldsView::Numeric(f)),
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::valid(schema),))]
pub(in super::super) fn valid(schema: SchemaView<'_>) -> bool {
    match schema {
        SchemaView::Legacy(s) => super::valid(s),
        SchemaView::Numeric(s) => super::super::admission::numeric::schema_valid(s),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==spec::reify(leaf,code),))]
pub(in super::super) fn reify(leaf: LeafView<'_>, code: Atom<'static>) -> Option<Atom<'static>> {
    match leaf {
        LeafView::Legacy(l) => match code {
            Atom::I128(v) => super::reify(l, v),
            _ => None,
        },
        LeafView::Numeric(Domain::Sum { type_id, variants }) => match code {
            Atom::I128(v) => {
                let variant = u16::try_from(v).ok()?;
                if variants.contains(&variant) {
                    Some(Atom::Sum { type_id, variant })
                } else {
                    None
                }
            }
            _ => None,
        },
        LeafView::Numeric(d) => {
            if super::super::schema_validation::atom(d, code) {
                Some(code)
            } else {
                None
            }
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None})==spec::fields(fields,codes@),))]
fn reify_fields(fields: FieldsView<'_>, codes: &[Atom<'static>]) -> Option<Vec<Field<'static>>> {
    match fields {
        FieldsView::Legacy(fs) => {
            let mut values = Vec::new();
            if !super::super::super::numeric::project_legacy(codes, &mut values) {
                return None;
            }
            super::reify_fields(fs, &values)
        }
        FieldsView::Numeric(fs) => {
            if fs.len() != codes.len() {
                return None;
            }
            let mut result = Vec::new();
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=fs.len(),fs.len()==codes.len(),result@==spec::numeric_fields(fs@,codes@,i as nat),decreases fs.len()-i,))]
            while i < fs.len() {
                result.push(Field {
                    id: fs[i].id,
                    value: reify(LeafView::Numeric(fs[i].leaf), codes[i])?,
                });
                i += 1;
            }
            Some(result)
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    (match result{Ok(v)=>Ok(spec::decoded(v)),Err(e)=>Err(e)},final(meter).used.counters@,final(reads)@)=={
        let r=spec::project(bytes@,schema,source,old(meter).limits.counters@,old(meter).used.counters@);(r.0,r.1,old(reads)@+r.2)},))]
pub(in super::super) fn project<'a>(
    bytes: &'a [u8],
    schema: SchemaView<'a>,
    source: u8,
    meter: &mut Meter,
    reads: &mut Vec<Read>,
) -> Result<Decoded<'a, Atom<'static>>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost limits=meter.limits.counters@;let ghost used=meter.used.counters@;let ghost initial=reads@;}
    if !valid(schema) {
        return Err(Failure::Schema);
    }
    if let SchemaView::Numeric(Schema::Original {
        checked,
        root,
        projection,
    }) = schema
    {
        #[cfg(verus_keep_ghost)]
        proof! { crate::finite::execution_v2::compound::spec::slice_equal(bytes@, bytes); }
        return original::project(bytes, checked, root, projection, source, meter, reads);
    }
    match schema.shape() {
        Shape::Empty => Err(Failure::Schema),
        Shape::Record(fields) => {
            let mut scalars = Vec::new();
            let mut attempts = Vec::new();
            let projected = typed::project_into(bytes, fields, meter, &mut scalars, &mut attempts);
            super::append_reads(source, &attempts, reads);
            if let Err(e) = projected {
                return Err(Failure::Record(e));
            }
            let Some(values) = reify_fields(fields, &scalars) else {
                return Err(Failure::Reification);
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::decoded_record(&values,&scalars);
                assert(spec::project(bytes@,schema,source,limits,used).0==Ok((super::spec::ValueView::Record(values@),scalars@)));
                assert(spec::project(bytes@,schema,source,limits,used).1==meter.used.counters@);
                assert(reads@==initial+spec::project(bytes@,schema,source,limits,used).2);
            }
            Ok(Decoded {
                value: Value::Record(values),
                scalars,
            })
        }
        Shape::Leaf(leaf) => {
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
            let Some((code, end)) = typed::decode_value(bytes, 0, leaf) else {
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
            proof! {
                spec::decoded_leaf(atom,&scalars);
                assert(scalars@==seq![code]);
                assert(spec::shape(schema)==Shape::Leaf(leaf));
                assert(spec::reify(leaf,code)==Some(atom));
                assert(typed::spec::decode(bytes@,0,leaf)==Some((code,end)));
                assert(spec::project(bytes@,schema,source,limits,used).0==Ok((super::spec::ValueView::Leaf(atom),scalars@)));
                assert(spec::project(bytes@,schema,source,limits,used).1==meter.used.counters@);
                assert(reads@==initial+spec::project(bytes@,schema,source,limits,used).2);
            }
            Ok(Decoded {
                value: Value::Leaf(atom),
                scalars,
            })
        }
    }
}
