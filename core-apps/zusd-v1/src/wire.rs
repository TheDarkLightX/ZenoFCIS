//! Closed original-wire/schema preparation and deterministic output preflight.
//! This does not authenticate a state, select a live profile or publish effects.
use crate::{Evaluation, Failure, Limits, Q, Work, run};
use alloc::{vec, vec::Vec};
use zeno_fcis_codec::{
    CommitmentHasher, DecodeLimits, Envelope, Hash32, RustCryptoSha256, decode_envelope,
};
use zeno_fcis_collections::{
    BTreeMapBackend, LogicalEntry,
    bounded::{BoundedError, BoundedMap, CollectionLimits},
};
use zeno_fcis_schema::{
    EnumVariantDef, FieldDef, FieldId, Schema, SchemaError, SchemaLimits, SumVariantDef as Sum,
    TypeDef, TypeId, TypeKind as Kind, ValidationLimits, VariantId,
};
use zeno_fcis_value::{Value, ValueRef};
#[path = "schema_generated.rs"]
mod generated;
pub use generated::{COMMAND, MAYBE_ACCOUNT, MOVEMENT, OBSERVATION, PATCH, QUANTITY, STATE};

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// PROFILE.json table order, including the Order vector at index 2.
    pub tables: [CollectionLimits; 14],
    pub sequence_bytes: u64,
    pub frame_bytes: u64,
    pub bounded_bytes: u32,
}
/// Immutable prepared schema. Preparation is separate from decision authority.
pub struct Policy {
    schema: Schema,
    bounds: Bounds,
    hash: Hash32,
    identity: Hash32,
}
impl Policy {
    pub fn prepare(bounds: Bounds) -> Result<Self, Failure> {
        if bounds.tables[0].max_entries != bounds.tables[1].max_entries
            || bounds.tables[0].max_entries != bounds.tables[2].max_entries
        {
            return Err(Failure::Domain);
        }
        if bounds.tables.iter().any(|l| l.max_snapshot_bytes < 29) || bounds.frame_bytes < 48 {
            return Err(Failure::Domain);
        }
        let schema = generated::build(bounds).map_err(|_| Failure::Domain)?;
        let hash = schema
            .schema_hash::<RustCryptoSha256>()
            .map_err(|_| Failure::Domain)?;
        let mut preimage = Vec::from(&b"zenofcis.zusd.native-policy.v1\0"[..]);
        preimage.extend_from_slice(hash.as_bytes());
        for l in bounds.tables {
            preimage.extend_from_slice(&l.max_entries.to_be_bytes());
            preimage.extend_from_slice(&l.max_item_bytes.to_be_bytes());
            preimage.extend_from_slice(&l.max_snapshot_bytes.to_be_bytes());
        }
        preimage.extend_from_slice(&bounds.sequence_bytes.to_be_bytes());
        preimage.extend_from_slice(&bounds.frame_bytes.to_be_bytes());
        preimage.extend_from_slice(&bounds.bounded_bytes.to_be_bytes());
        let identity = RustCryptoSha256::hash(&preimage);
        Ok(Self {
            schema,
            bounds,
            hash,
            identity,
        })
    }
    pub fn identity(&self) -> Hash32 {
        self.identity
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn hash(&self) -> Hash32 {
        self.hash
    }
    pub(crate) fn validate(&self, ty: u32, value: &Value, w: &mut Work) -> Result<(), Failure> {
        let n = size(value, w, 0)?;
        w.charge(
            n.checked_mul(self.schema.types().len() as u64 + 1)
                .ok_or(Failure::Arithmetic)?,
            n,
            0,
        )?;
        self.schema
            .validate_value(
                TypeId::new(ty),
                value,
                ValidationLimits {
                    max_depth: 64,
                    max_nodes: n,
                },
            )
            .map_err(|_| Failure::Domain)?;
        let profile = if ty == STATE {
            Some(fields(fields(value)?[0].value())?[0].value())
        } else if ty == COMMAND {
            Some(fields(value)?[3].value())
        } else {
            None
        };
        if let Some(v) = profile
            && (!matches!(v.view(),ValueRef::Bytes(b) if b==self.identity.as_bytes()))
        {
            return Err(Failure::Domain);
        }
        Ok(())
    }
    /// Serialization utility returning owned bytes, never a receipt.
    pub fn frame(&self, ty: u32, value: &Value, limits: Limits) -> Evaluation<Vec<u8>> {
        run(limits, |w| {
            self.validate(ty, value, w)?;
            if ty == STATE {
                self.original_bounds(value, w)?;
            }
            let n = size(value, w, 0)?
                .checked_add(48)
                .ok_or(Failure::Arithmetic)?;
            w.charge(n, n, n.checked_mul(4).ok_or(Failure::Arithmetic)?)?;
            if n > self.bounds.frame_bytes {
                return Err(Failure::Capacity);
            }
            Envelope::new(ty, self.hash, value.clone())
                .canonical_bytes_with_limits(DecodeLimits {
                    max_input_bytes: self.bounds.frame_bytes,
                    ..Default::default()
                })
                .map_err(|_| Failure::Canonical)
        })
    }
    pub fn admit(&self, ty: u32, bytes: &[u8], limits: Limits) -> Evaluation<Value> {
        run(limits, |w| {
            let n = bytes.len() as u64;
            if n > self.bounds.frame_bytes {
                return Err(Failure::Capacity);
            }
            // Bytes bound the decoded node count. Charge decoder and owned copy
            // before allocation, including malformed and trailing-byte refusals.
            w.charge(
                n.checked_mul(4).ok_or(Failure::Arithmetic)?,
                n,
                n.checked_mul(4).ok_or(Failure::Arithmetic)?,
            )?;
            let envelope = decode_envelope(
                bytes,
                DecodeLimits {
                    max_input_bytes: self.bounds.frame_bytes,
                    ..Default::default()
                },
            )
            .map_err(|_| Failure::Canonical)?;
            if envelope.type_id() != ty || envelope.schema_hash() != self.hash {
                return Err(Failure::Domain);
            }
            self.validate(ty, envelope.value(), w)?;
            if ty == STATE {
                self.original_bounds(envelope.value(), w)?;
            }
            w.charge(0, 0, n)?;
            Ok(envelope.value().clone())
        })
    }
    pub(crate) fn original_bounds(&self, value: &Value, w: &mut Work) -> Result<(), Failure> {
        let fs = fields(value)?;
        for i in 0..14 {
            let v = fs.get(i + 1).ok_or(Failure::Domain)?.value();
            let n = size(v, w, 0)?;
            if i == 2 {
                if n > self.bounds.sequence_bytes {
                    return Err(Failure::Capacity);
                }
                continue;
            }
            let es = entries(v)?;
            let mut map = BoundedMap::<BTreeMapBackend>::empty(self.bounds.tables[i])
                .map_err(|_| Failure::Domain)?;
            for e in es {
                w.charge(
                    n + 1,
                    es.len() as u64,
                    n.checked_mul(4).ok_or(Failure::Arithmetic)?,
                )?;
                map = map.insert(logical(e)?).map_err(|_| Failure::Domain)?;
            }
        }
        Ok(())
    }
    pub fn preflight(&self, pre: &Value, post: &Value, limits: Limits) -> Evaluation<Value> {
        run(limits, |w| self.preflight_inner(pre, post, w))
    }
    fn preflight_inner(&self, pre: &Value, post: &Value, w: &mut Work) -> Result<Value, Failure> {
        self.validate(STATE, pre, w)?;
        self.original_bounds(pre, w)?;
        let before = fields(pre)?;
        let after = fields(post)?;
        if before.len() != 15
            || after.len() != 15
            || after
                .iter()
                .enumerate()
                .any(|(i, f)| f.id() != i as u16 + 1)
        {
            return Err(Failure::Domain);
        }
        for i in 0..14 {
            let a = before[i + 1].value();
            let b = after[i + 1].value();
            let na = size(a, w, 0)?;
            let nb = size(b, w, 0)?;
            w.charge(na + nb, na + nb, na + nb)?;
            if a == b {
                continue;
            }
            if i == 2 {
                if !matches!(b.view(), ValueRef::Vector(_)) {
                    return Err(Failure::Domain);
                }
                if nb > self.bounds.sequence_bytes {
                    return Err(Failure::Reject(1063));
                }
                continue;
            }
            let aa = entries(a)?;
            let bb = entries(b)?;
            let mut map = BoundedMap::<BTreeMapBackend>::empty(self.bounds.tables[i])
                .map_err(|_| Failure::Domain)?;
            for row in aa {
                w.charge(
                    na + 1,
                    aa.len() as u64,
                    na.checked_mul(4).ok_or(Failure::Arithmetic)?,
                )?;
                map = map.insert(logical(row)?).map_err(|_| Failure::Domain)?;
            }
            let map_ty = match self
                .schema
                .type_by_id(TypeId::new(STATE))
                .ok_or(Failure::Domain)?
                .kind()
            {
                Kind::Record { fields } => fields[i + 1].type_id(),
                _ => return Err(Failure::Domain),
            };
            let (kt, vt) = match self
                .schema
                .type_by_id(map_ty)
                .ok_or(Failure::Domain)?
                .kind()
            {
                Kind::Map { key, value, .. } => (*key, *value),
                _ => return Err(Failure::Domain),
            };
            let mut x = 0;
            let mut y = 0;
            while x < aa.len() || y < bb.len() {
                w.charge(1, 1, 0)?;
                let old = aa.get(x);
                let new = bb.get(y);
                let ordering = match (old, new) {
                    (Some(a), Some(b)) => a.encoded_key().cmp(b.encoded_key()),
                    (Some(_), None) => core::cmp::Ordering::Less,
                    _ => core::cmp::Ordering::Greater,
                };
                let current = size(a, w, 0)?;
                w.charge(
                    current + nb + 1,
                    map.len() as u64,
                    current
                        .checked_mul(4)
                        .and_then(|n| nb.checked_mul(4).and_then(|b| n.checked_add(b)))
                        .ok_or(Failure::Arithmetic)?,
                )?;
                if ordering == core::cmp::Ordering::Less {
                    map = map
                        .remove(old.ok_or(Failure::Domain)?.encoded_key())
                        .map_err(edit_error)?;
                    x += 1;
                } else {
                    let row = new.ok_or(Failure::Domain)?;
                    self.validate(kt.get(), row.key(), w)?;
                    self.validate(vt.get(), row.value(), w)?;
                    map = map.insert(logical(row)?).map_err(edit_error)?;
                    if ordering == core::cmp::Ordering::Equal {
                        x += 1;
                    }
                    y += 1;
                }
            }
        }
        self.validate(STATE, post, w)?;
        let n = size(post, w, 0)?
            .checked_add(48)
            .ok_or(Failure::Arithmetic)?;
        if n > self.bounds.frame_bytes {
            return Err(Failure::Capacity);
        }
        w.charge(1, 0, n)?;
        Ok(post.clone())
    }
}
fn edit_error(e: BoundedError) -> Failure {
    match e {
        BoundedError::ItemTooLarge => Failure::Reject(1060),
        BoundedError::Full => Failure::Reject(1061),
        BoundedError::SnapshotTooLarge => Failure::Reject(1062),
        _ => Failure::Domain,
    }
}
fn logical(e: &zeno_fcis_value::MapEntry) -> Result<LogicalEntry, Failure> {
    LogicalEntry::try_new(e.encoded_key().to_vec(), e.key().clone(), e.value().clone())
        .map_err(|_| Failure::Canonical)
}
fn fields(v: &Value) -> Result<&[zeno_fcis_value::Field], Failure> {
    match v.view() {
        ValueRef::Record(fs) => Ok(fs),
        _ => Err(Failure::Domain),
    }
}
fn entries(v: &Value) -> Result<&[zeno_fcis_value::MapEntry], Failure> {
    match v.view() {
        ValueRef::Map(es) => Ok(es),
        _ => Err(Failure::Domain),
    }
}
/// Exact ZCVE size with charged bounded traversal; do not serialize to learn size.
pub(crate) fn size(v: &Value, w: &mut Work, depth: u16) -> Result<u64, Failure> {
    w.charge(1, 1, 0)?;
    if depth > 64 {
        return Err(Failure::Domain);
    }
    let n = match v.view() {
        ValueRef::Unit | ValueRef::Bool(_) => 1,
        ValueRef::U128(_) | ValueRef::I128(_) => 17,
        ValueRef::Enum { .. } => 7,
        ValueRef::Bytes(b) => 5 + b.len() as u64,
        ValueRef::Text(t) => 5 + t.len() as u64,
        ValueRef::Sum { payload, .. } => {
            8 + if let Some(x) = payload {
                size(x, w, depth + 1)?
            } else {
                0
            }
        }
        ValueRef::Tuple(xs) | ValueRef::Vector(xs) => {
            let mut n = 5u64;
            for x in xs {
                n = n
                    .checked_add(size(x, w, depth + 1)?)
                    .ok_or(Failure::Arithmetic)?;
            }
            n
        }
        ValueRef::Record(fs) => {
            let mut n = 5u64;
            for f in fs {
                n = n
                    .checked_add(2 + size(f.value(), w, depth + 1)?)
                    .ok_or(Failure::Arithmetic)?;
            }
            n
        }
        ValueRef::Map(es) => {
            let mut n = 5u64;
            for e in es {
                n = n
                    .checked_add(8 + size(e.key(), w, depth + 1)? + size(e.value(), w, depth + 1)?)
                    .ok_or(Failure::Arithmetic)?;
            }
            n
        }
        _ => return Err(Failure::Domain),
    };
    Ok(n)
}
