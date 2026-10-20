use super::super::meter::Meter;
use super::schema::record_field;
use super::*;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits};
use zeno_fcis_collections::{
    BTreeMapBackend, LogicalEntry,
    bounded::{BoundedError, BoundedMap},
};

type Map = BoundedMap<BTreeMapBackend>;
pub(crate) fn step(m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Step, 1).map_err(Into::into)
}
pub(crate) fn visit(m: &mut Meter) -> Result<(), Failure> {
    step(m)?;
    m.charge(Resource::Read, 1)?;
    Ok(())
}
fn add(a: u64, b: u64) -> Result<u64, Failure> {
    a.checked_add(b).ok_or(Failure::Arithmetic)
}
/// Size derivation itself is a charged, bounded Value traversal.
pub(crate) fn size(v: &Value, m: &mut Meter) -> Result<u64, Failure> {
    step(m)?;
    Ok(match v.view() {
        ValueRef::Unit | ValueRef::Bool(_) => 1,
        ValueRef::I128(_) | ValueRef::U128(_) => 17,
        ValueRef::Bytes(b) => add(5, b.len() as u64)?,
        ValueRef::Text(b) => add(5, b.len() as u64)?,
        ValueRef::Tuple(xs) | ValueRef::Vector(xs) => {
            let mut n = 5;
            for x in xs {
                n = add(n, size(x, m)?)?;
            }
            n
        }
        ValueRef::Record(fs) => {
            let mut n = 5;
            for f in fs {
                step(m)?;
                n = add(add(n, 2)?, size(f.value(), m)?)?;
            }
            n
        }
        ValueRef::Map(es) => {
            let mut n = 5;
            for e in es {
                visit(m)?;
                n = add(add(add(n, 8)?, size(e.key(), m)?)?, size(e.value(), m)?)?;
            }
            n
        }
        ValueRef::Sum { payload, .. } => match payload {
            Some(v) => add(8, size(v, m)?)?,
            None => 8,
        },
        _ => return Err(Failure::Kind),
    })
}
pub(crate) fn reserve(v: &Value, m: &mut Meter) -> Result<(), Failure> {
    let n = size(v, m)?;
    m.charge(Resource::Byte, n)?;
    Ok(())
}
pub(crate) fn encode(v: &Value, m: &mut Meter) -> Result<Vec<u8>, Failure> {
    reserve(v, m)?;
    v.canonical_bytes().map_err(|_| Failure::Wire)
}
pub(crate) fn copy(v: &Value, m: &mut Meter) -> Result<Value, Failure> {
    reserve(v, m)?;
    Ok(v.clone())
}
pub(crate) fn decode(
    d: &Descriptor,
    root: u32,
    bytes: &[u8],
    m: &mut Meter,
) -> Result<Value, Failure> {
    m.charge(Resource::Read, 1)?;
    m.charge(Resource::Byte, bytes.len() as u64)?;
    let start = m.used.used(Resource::Step);
    let end = d
        .with_schema(|s| {
            super::super::compound::walk(bytes, s.definitions, 0, bytes.len(), root, m)
        })
        .map_err(|e| match e {
            super::super::compound::Failure::Wire => Failure::Wire,
            super::super::compound::Failure::Budget(e) => Failure::Budget(e),
        })?;
    if end != bytes.len() {
        return Err(Failure::Wire);
    }
    // Reserve the second decode and its canonical equality pass before allocation.
    let actions = m
        .used
        .used(Resource::Step)
        .checked_sub(start)
        .ok_or(Failure::Arithmetic)?;
    m.charge(
        Resource::Step,
        actions.checked_mul(2).ok_or(Failure::Arithmetic)?,
    )?;
    m.charge(
        Resource::Byte,
        (bytes.len() as u64)
            .checked_mul(2)
            .ok_or(Failure::Arithmetic)?,
    )?;
    zeno_fcis_codec::decode_value(bytes, DecodeLimits::default()).map_err(|_| Failure::Wire)
}
fn bounded(e: BoundedError) -> Failure {
    match e {
        BoundedError::ArithmeticOverflow => Failure::Arithmetic,
        _ => Failure::Wire,
    }
}
fn error(e: BoundedError) -> Result<EditError, Failure> {
    match e {
        BoundedError::ItemTooLarge => Ok(EditError::ItemTooLarge),
        BoundedError::Full => Ok(EditError::Full),
        BoundedError::SnapshotTooLarge => Ok(EditError::SnapshotTooLarge),
        e => Err(bounded(e)),
    }
}
pub(crate) fn entries(v: &Value) -> Result<&[MapEntry], Failure> {
    match v.view() {
        ValueRef::Map(es) => Ok(es),
        _ => Err(Failure::Kind),
    }
}
fn compare(a: &[u8], b: &[u8], m: &mut Meter) -> Result<(), Failure> {
    step(m)?;
    m.charge(Resource::Byte, add(a.len() as u64, b.len() as u64)?)?;
    Ok(())
}
fn charge_copy(v: &Value, m: &mut Meter) -> Result<(), Failure> {
    for e in entries(v)? {
        visit(m)?;
        reserve(e.key(), m)?;
        reserve(e.value(), m)?;
    }
    Ok(())
}
pub(crate) fn reconstruct(
    d: &Descriptor,
    id: u32,
    v: &Value,
    m: &mut Meter,
) -> Result<Map, Failure> {
    let (kt, vt, set, limits) = d.table(id)?;
    let es = entries(v)?;
    if es.len() as u64 > u64::from(limits.max_entries) {
        return Err(Failure::Wire);
    }
    let mut map = Map::empty(limits).map_err(bounded)?;
    let mut previous: Option<&[u8]> = None;
    for entry in es {
        visit(m)?;
        d.check_value(kt, entry.key(), m)?;
        d.check_value(vt, entry.value(), m)?;
        if set && !matches!(entry.value().view(), ValueRef::Unit) {
            return Err(Failure::Wire);
        }
        if let Some(p) = previous {
            compare(p, entry.encoded_key(), m)?;
            if p >= entry.encoded_key() {
                return Err(Failure::Wire);
            }
        }
        previous = Some(entry.encoded_key());
        for old in es.iter().take(map.len()) {
            visit(m)?;
            reserve(old.key(), m)?;
            reserve(old.value(), m)?;
        }
        reserve(entry.key(), m)?;
        let key = encode(entry.key(), m)?;
        let item = LogicalEntry::try_new(key, copy(entry.key(), m)?, copy(entry.value(), m)?)
            .map_err(|_| Failure::Wire)?;
        map = map.insert(item).map_err(bounded)?;
    }
    Ok(map)
}
pub(crate) fn check_state(d: &Descriptor, v: &Value, m: &mut Meter) -> Result<(), Failure> {
    if d.ordered() {
        check_literal(d, d.roots[0], v, m, 0)?;
        if let Some(spec) = &d.order
            && !super::ordered::valid(d, spec, v, m)?
        {
            return Err(Failure::Law(980));
        }
        return Ok(());
    }
    for f in d.record(d.roots[0])? {
        if matches!(d.ty(f.type_id)?.shape, Shape::Table { .. }) {
            reconstruct(d, f.type_id, record_field(v, f.id)?, m)?;
        }
    }
    Ok(())
}
/// Schema validation alone does not establish a literal's collection byte limits.
pub(crate) fn check_literal(
    d: &Descriptor,
    id: u32,
    v: &Value,
    m: &mut Meter,
    depth: usize,
) -> Result<(), Failure> {
    if depth > zeno_fcis_value::ValueLimits::default().max_depth as usize {
        return Err(Failure::Descriptor);
    }
    step(m)?;
    match &d.ty(id)?.shape {
        Shape::Table { .. } => {
            reconstruct(d, id, v, m)?;
        }
        Shape::Vector {
            element,
            max,
            bytes,
        } => {
            let ValueRef::Vector(xs) = v.view() else {
                return Err(Failure::Kind);
            };
            if xs.len() > *max as usize || size(v, m)? > *bytes {
                return Err(Failure::Wire);
            }
            for x in xs {
                visit(m)?;
                check_literal(d, *element, x, m, depth + 1)?;
            }
        }
        Shape::Maybe(element) => {
            if let ValueRef::Sum {
                payload: Some(x), ..
            } = v.view()
            {
                check_literal(d, *element, x, m, depth + 1)?;
            }
        }
        Shape::Record(fields) => {
            for f in fields {
                check_literal(d, f.type_id, record_field(v, f.id)?, m, depth + 1)?;
            }
        }
        Shape::Tuple(ids) => {
            let ValueRef::Tuple(values) = v.view() else {
                return Err(Failure::Kind);
            };
            for (id, v) in ids.iter().zip(values) {
                check_literal(d, *id, v, m, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn lookup(
    d: &Descriptor,
    id: u32,
    v: &Value,
    key: &Value,
    m: &mut Meter,
) -> Result<Option<Value>, Failure> {
    let (kt, _, _, _) = d.table(id)?;
    d.check_value(kt, key, m)?;
    let q = encode(key, m)?;
    let mut found = None;
    for e in entries(v)? {
        visit(m)?;
        m.charge(
            Resource::Byte,
            add(q.len() as u64, e.encoded_key().len() as u64)?,
        )?;
        if q == e.encoded_key() {
            found = Some(copy(e.value(), m)?);
        }
    }
    Ok(found)
}
pub(crate) fn len(v: &Value, m: &mut Meter) -> Result<i128, Failure> {
    let mut n = 0i128;
    for _ in entries(v)? {
        visit(m)?;
        n = n.checked_add(1).ok_or(Failure::Arithmetic)?;
    }
    Ok(n)
}
pub(crate) fn edit(
    d: &Descriptor,
    id: u32,
    v: &Value,
    key: &Value,
    value: Option<&Value>,
    operation: u8,
    m: &mut Meter,
) -> Result<Result<Value, EditError>, Failure> {
    let (kt, vt, set, limits) = d.table(id)?;
    d.check_value(kt, key, m)?;
    // BoundedSet checks the incoming key/item before canonical membership work.
    // This charged size walk needs no entry reads, even when the set is full.
    if set && operation == 0 && add(17, size(key, m)?)? > limits.max_item_bytes {
        return Ok(Err(EditError::ItemTooLarge));
    }
    let old = lookup(d, id, v, key, m)?;
    if operation == 1 && old.is_none() {
        return Ok(Err(EditError::MissingKey));
    }
    let mut map = reconstruct(d, id, v, m)?;
    let key_bytes = encode(key, m)?;
    let next = if operation == 2 {
        charge_copy(v, m)?;
        map.remove(&key_bytes)
    } else {
        let unit = Value::unit();
        let new = if set {
            &unit
        } else {
            value.ok_or(Failure::Kind)?
        };
        d.check_value(vt, new, m)?;
        // Copy/materialization work is reserved before the backend, also on refusal.
        charge_copy(v, m)?;
        reserve(key, m)?;
        reserve(new, m)?;
        let entry = LogicalEntry::try_new(key_bytes, copy(key, m)?, copy(new, m)?)
            .map_err(|_| Failure::Wire)?;
        map.insert(entry)
    };
    map = match next {
        Ok(n) => n,
        Err(e) => return Ok(Err(error(e)?)),
    };
    // Reserve the complete output bound before fallible backend materialization.
    // Snapshot framing is larger than Value map framing, so this covers every byte.
    for _ in 0..3 {
        charge_copy(v, m)?;
        reserve(key, m)?;
        if let Some(value) = value {
            reserve(value, m)?;
        }
    }
    m.charge(Resource::Byte, map.snapshot_len())?;
    let out = map.try_to_value_map().map_err(|_| Failure::Wire)?;
    reserve(&out, m)?;
    Ok(Ok(out))
}
pub(crate) fn frame(
    pre: &Value,
    post: &Value,
    keys: &[Value],
    m: &mut Meter,
) -> Result<bool, Failure> {
    let a = entries(pre)?;
    let b = entries(post)?;
    let mut i = 0;
    let mut j = 0;
    let writes: Result<Vec<Vec<u8>>, Failure> = keys.iter().map(|k| encode(k, m)).collect();
    let writes = writes?;
    while i < a.len() || j < b.len() {
        visit(m)?;
        let order = match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) => {
                compare(x.encoded_key(), y.encoded_key(), m)?;
                x.encoded_key().cmp(y.encoded_key())
            }
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            _ => break,
        };
        let (key, left, right) = match order {
            core::cmp::Ordering::Less => {
                let e = &a[i];
                i += 1;
                (e.encoded_key(), Some(e.value()), None)
            }
            core::cmp::Ordering::Greater => {
                let e = &b[j];
                j += 1;
                (e.encoded_key(), None, Some(e.value()))
            }
            core::cmp::Ordering::Equal => {
                let x = &a[i];
                let y = &b[j];
                i += 1;
                j += 1;
                (x.encoded_key(), Some(x.value()), Some(y.value()))
            }
        };
        let mut written = false;
        for w in &writes {
            compare(w, key, m)?;
            written |= w.as_slice() == key;
        }
        if !written {
            if let Some(v) = left {
                reserve(v, m)?;
            }
            if let Some(v) = right {
                reserve(v, m)?;
            }
            if left != right {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
