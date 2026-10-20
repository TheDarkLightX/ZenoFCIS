//! Scan-profile ordered operations. Scratch is invocation-owned; hints grant no authority.
//! V1 cuts transcribe SortedTroves.sol at 8f52f2906f99414c0b1c3a84c95c74c319b7a8c6,
//! SHA256 480a8a4bb3f396e9279d9025843c388fd9d625749e780373d103290d17fbafc0.
use super::super::meter::Meter;
use super::collection::{self, copy, encode, reserve, size, step, visit};
use super::schema::{record_field, record_replace};
use super::*;
use alloc::vec;
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Priority {
    Signed(i128),
    Unsigned(u128),
}

pub(super) fn fields(d: &Descriptor, s: &OrderSpec) -> Result<(u32, Option<u32>), Failure> {
    let fs = d.record(d.roots[0])?;
    let map = fs
        .iter()
        .find(|f| f.id == s.map)
        .ok_or(Failure::Descriptor)?
        .type_id;
    let seq = s
        .sequence
        .map(|id| {
            fs.iter()
                .find(|f| f.id == id)
                .map(|f| f.type_id)
                .ok_or(Failure::Descriptor)
        })
        .transpose()?;
    Ok((map, seq))
}
pub(super) fn admit(d: &Descriptor, s: &OrderSpec) -> Result<(), Failure> {
    if s.mode > 2 || (s.mode == 0) != s.sequence.is_none() || s.sequence == Some(s.map) {
        return Err(Failure::Descriptor);
    }
    let (map, seq) = fields(d, s)?;
    let (key, item, set, l) = d.table(map)?;
    if set {
        return Err(Failure::Descriptor);
    }
    let fs = d.record(item)?;
    if fs.len() != 3
        || !matches!(
            d.ty(fs[0].type_id)?.shape,
            Shape::I128 { .. } | Shape::U128 { .. }
        )
    {
        return Err(Failure::Descriptor);
    }
    if s.mode == 2
        && !match d.ty(fs[0].type_id)?.shape {
            Shape::I128 { min, .. } => min > 0,
            Shape::U128 { min, .. } => min > 0,
            _ => false,
        }
    {
        return Err(Failure::Descriptor);
    }
    for t in &d.types {
        match t.shape {
            Shape::Maybe(element) if element != key => return Err(Failure::Descriptor),
            Shape::Vector { element, max, .. } => {
                let pair =
                    matches!(&d.ty(element)?.shape,Shape::Tuple(ids) if ids.as_slice()==[key,item]);
                if max != l.max_entries || (element != key && !pair) {
                    return Err(Failure::Descriptor);
                }
            }
            _ => {}
        }
    }
    if let Some(seq) = seq
        && !matches!(d.ty(seq)?.shape,Shape::Vector{element,max,..} if element==key && max==l.max_entries)
    {
        return Err(Failure::Descriptor);
    }
    Ok(())
}
pub(super) fn command_types(d: &Descriptor, s: &OrderSpec, tid: u32) -> Result<(), Failure> {
    let (map, _) = fields(d, s)?;
    let (key, item, _, _) = d.table(map)?;
    let fs = d.record(tid)?;
    if fs.len() != if s.mode == 0 { 3 } else { 5 } {
        return Err(Failure::Kind);
    }
    if !matches!(d.ty(fs[0].type_id)?.shape, Shape::I128 { min: 0, max: 3 })
        || fs[1].type_id != key
        || fs[2].type_id != item
    {
        return Err(Failure::Kind);
    }
    if s.mode != 0 {
        for f in &fs[3..] {
            if !matches!(d.ty(f.type_id)?.shape,Shape::Maybe(x) if x==key) {
                return Err(Failure::Kind);
            }
        }
    }
    Ok(())
}
pub(super) fn spec(d: &Descriptor, mode: u8, view: u32) -> Result<&OrderSpec, Failure> {
    let s = d.order.as_ref().ok_or(Failure::Descriptor)?;
    if mode != s.mode || view != d.roots[0] {
        return Err(Failure::Kind);
    }
    Ok(s)
}
pub(super) fn entries(v: &Value) -> Result<&[MapEntry], Failure> {
    collection::entries(v)
}
pub(super) fn slots(v: &Value) -> Result<&[Value], Failure> {
    match v.view() {
        ValueRef::Vector(xs) => Ok(xs),
        _ => Err(Failure::Kind),
    }
}
pub(super) fn integer(v: &Value) -> Result<i128, Failure> {
    match v.view() {
        ValueRef::I128(x) => Ok(x),
        _ => Err(Failure::Kind),
    }
}
pub(super) fn priority(item: &Value, m: &mut Meter) -> Result<Priority, Failure> {
    step(m)?;
    let ValueRef::Record(fs) = item.view() else {
        return Err(Failure::Kind);
    };
    match fs.first().ok_or(Failure::Kind)?.value().view() {
        ValueRef::I128(n) => Ok(Priority::Signed(n)),
        ValueRef::U128(n) => Ok(Priority::Unsigned(n)),
        _ => Err(Failure::Kind),
    }
}
pub(super) fn compare(a: &Value, b: &Value, m: &mut Meter) -> Result<Ordering, Failure> {
    step(m)?;
    reserve(a, m)?;
    reserve(b, m)?;
    Ok(encode(a, m)?.cmp(&encode(b, m)?))
}
fn pcompare(a: &Value, b: &Value, m: &mut Meter) -> Result<Ordering, Failure> {
    let ValueRef::Record(af) = a.view() else {
        return Err(Failure::Kind);
    };
    let ValueRef::Record(bf) = b.view() else {
        return Err(Failure::Kind);
    };
    step(m)?;
    reserve(af[0].value(), m)?;
    reserve(bf[0].value(), m)?;
    Ok(priority(a, m)?.cmp(&priority(b, m)?))
}
fn read_entry(e: &MapEntry, m: &mut Meter) -> Result<(), Failure> {
    visit(m)?;
    reserve(e.key(), m)?;
    reserve(e.value(), m)
}
pub(super) fn parts<'a>(
    s: &OrderSpec,
    v: &'a Value,
) -> Result<(&'a Value, Option<&'a Value>), Failure> {
    Ok((
        record_field(v, s.map)?,
        s.sequence.map(|id| record_field(v, id)).transpose()?,
    ))
}
pub(super) fn valid(
    d: &Descriptor,
    s: &OrderSpec,
    v: &Value,
    m: &mut Meter,
) -> Result<bool, Failure> {
    let (map, seq) = parts(s, v)?;
    let (mt, _) = fields(d, s)?;
    let Some(seq) = seq else { return Ok(true) };
    let xs = slots(seq)?;
    let es = entries(map)?;
    if xs.len() != es.len() {
        return Ok(false);
    }
    let mut previous: Option<Value> = None;
    for (i, id) in xs.iter().enumerate() {
        visit(m)?;
        for other in &xs[..i] {
            visit(m)?;
            visit(m)?;
            if compare(id, other, m)? == Ordering::Equal {
                return Ok(false);
            }
        }
        let Some(item) = collection::lookup(d, mt, map, id, m)? else {
            return Ok(false);
        };
        if let Some(prev) = &previous
            && pcompare(prev, &item, m)? == Ordering::Less
        {
            return Ok(false);
        }
        previous = Some(item);
    }
    Ok(true)
}
pub(super) fn minimum(
    d: &Descriptor,
    s: &OrderSpec,
    v: &Value,
    m: &mut Meter,
) -> Result<Option<(Value, Value)>, Failure> {
    let (map, seq) = parts(s, v)?;
    let (mt, _) = fields(d, s)?;
    if let Some(seq) = seq {
        return match slots(seq)?.last() {
            None => Ok(None),
            Some(id) => {
                visit(m)?;
                let item = collection::lookup(d, mt, map, id, m)?.ok_or(Failure::Undefined)?;
                Ok(Some((copy(id, m)?, item)))
            }
        };
    }
    let mut best: Option<(&Value, &Value)> = None;
    for e in entries(map)? {
        read_entry(e, m)?;
        let wins = match best {
            None => true,
            Some((id, item)) => match pcompare(e.value(), item, m)? {
                Ordering::Less => true,
                Ordering::Greater => false,
                Ordering::Equal => compare(e.key(), id, m)? == Ordering::Less,
            },
        };
        if wins {
            best = Some((e.key(), e.value()));
        }
    }
    best.map(|(id, item)| Ok((copy(id, m)?, copy(item, m)?)))
        .transpose()
}
pub(super) fn items(
    d: &Descriptor,
    s: &OrderSpec,
    v: &Value,
    m: &mut Meter,
) -> Result<Value, Failure> {
    let (map, seq) = parts(s, v)?;
    let (mt, _) = fields(d, s)?;
    let n = entries(map)?.len();
    // Reserve all pair slots and copied operands before output allocation.
    m.charge(
        Resource::Byte,
        5u64.checked_add((n as u64).checked_mul(5).ok_or(Failure::Arithmetic)?)
            .ok_or(Failure::Arithmetic)?,
    )?;
    let (_, _, _, limits) = d.table(mt)?;
    m.charge(Resource::Byte, limits.max_snapshot_bytes)?;
    let mut out = Vec::new();
    if let Some(seq) = seq {
        for id in slots(seq)?.iter().rev() {
            visit(m)?;
            let item = collection::lookup(d, mt, map, id, m)?.ok_or(Failure::Undefined)?;
            out.push(Value::tuple(vec![copy(id, m)?, item]).map_err(|_| Failure::Wire)?);
        }
    } else {
        let mut last: Option<(Value, Value)> = None;
        for _ in 0..n {
            let mut best: Option<(&Value, &Value)> = None;
            for e in entries(map)? {
                read_entry(e, m)?;
                if let Some((id, item)) = &last {
                    let p = pcompare(e.value(), item, m)?;
                    if p == Ordering::Less
                        || (p == Ordering::Equal && compare(e.key(), id, m)? != Ordering::Greater)
                    {
                        continue;
                    }
                }
                let wins = match best {
                    None => true,
                    Some((id, item)) => {
                        let p = pcompare(e.value(), item, m)?;
                        p == Ordering::Less
                            || (p == Ordering::Equal && compare(e.key(), id, m)? == Ordering::Less)
                    }
                };
                if wins {
                    best = Some((e.key(), e.value()));
                }
            }
            let (id, item) = best.ok_or(Failure::Undefined)?;
            last = Some((copy(id, m)?, copy(item, m)?));
            out.push(Value::tuple(vec![copy(id, m)?, copy(item, m)?]).map_err(|_| Failure::Wire)?);
        }
    }
    Value::vector(out).map_err(|_| Failure::Wire)
}
pub(super) fn maybe(v: &Value) -> Result<Option<&Value>, Failure> {
    match v.view() {
        ValueRef::Sum {
            variant: 0,
            payload: None,
            ..
        } => Ok(None),
        ValueRef::Sum {
            variant: 1,
            payload: Some(x),
            ..
        } => Ok(Some(x)),
        _ => Err(Failure::Kind),
    }
}
type CommandFields<'a> = (
    u8,
    &'a Value,
    &'a Value,
    Option<&'a Value>,
    Option<&'a Value>,
);
pub(super) fn command<'a>(
    d: &Descriptor,
    tid: u32,
    v: &'a Value,
) -> Result<CommandFields<'a>, Failure> {
    let fs = d.record(tid)?;
    let op = u8::try_from(integer(record_field(v, fs[0].id)?)?).map_err(|_| Failure::Kind)?;
    Ok((
        op,
        record_field(v, fs[1].id)?,
        record_field(v, fs[2].id)?,
        if fs.len() == 5 {
            maybe(record_field(v, fs[3].id)?)?
        } else {
            None
        },
        if fs.len() == 5 {
            maybe(record_field(v, fs[4].id)?)?
        } else {
            None
        },
    ))
}
// No hidden rank map: every ID location and priority lookup pays a full logical scan.
fn position(xs: &[Value], id: Option<&Value>, m: &mut Meter) -> Result<Option<usize>, Failure> {
    let Some(id) = id else { return Ok(None) };
    let mut result = None;
    for (i, x) in xs.iter().enumerate() {
        visit(m)?;
        if compare(x, id, m)? == Ordering::Equal {
            result = Some(i)
        }
    }
    Ok(result)
}
fn at_priority(
    d: &Descriptor,
    mt: u32,
    map: &Value,
    xs: &[Value],
    i: usize,
    m: &mut Meter,
) -> Result<Priority, Failure> {
    visit(m)?;
    priority(
        &collection::lookup(d, mt, map, &xs[i], m)?.ok_or(Failure::Undefined)?,
        m,
    )
}
pub(super) fn number_cmp(a: Priority, b: Priority, m: &mut Meter) -> Result<Ordering, Failure> {
    step(m)?;
    m.charge(Resource::Byte, 34)?;
    match (a, b) {
        (Priority::Signed(a), Priority::Signed(b)) => Ok(a.cmp(&b)),
        (Priority::Unsigned(a), Priority::Unsigned(b)) => Ok(a.cmp(&b)),
        _ => Err(Failure::Kind),
    }
}
fn gap(
    d: &Descriptor,
    mt: u32,
    map: &Value,
    xs: &[Value],
    p: Priority,
    (upper, lower): (Option<usize>, Option<usize>),
    m: &mut Meter,
) -> Result<bool, Failure> {
    step(m)?;
    Ok(match (upper, lower) {
        (None, None) => xs.is_empty(),
        (None, Some(j)) => {
            j == 0 && number_cmp(p, at_priority(d, mt, map, xs, j, m)?, m)? != Ordering::Less
        }
        (Some(i), None) => {
            i + 1 == xs.len()
                && number_cmp(p, at_priority(d, mt, map, xs, i, m)?, m)? != Ordering::Greater
        }
        (Some(i), Some(j)) => {
            i + 1 == j
                && number_cmp(at_priority(d, mt, map, xs, i, m)?, p, m)? != Ordering::Less
                && number_cmp(p, at_priority(d, mt, map, xs, j, m)?, m)? != Ordering::Less
        }
    })
}
/// Exact V1 validInsertPosition / sanitized-hint ascend-or-descend behavior.
pub(super) fn cut(
    d: &Descriptor,
    (mt, map): (u32, &Value),
    xs: &[Value],
    p: Priority,
    (upper, lower): (Option<&Value>, Option<&Value>),
    mode: u8,
    m: &mut Meter,
) -> Result<usize, Failure> {
    let mut u = position(xs, upper, m)?;
    let mut l = position(xs, lower, m)?;
    let endpoints_exist = upper.is_none_or(|_| u.is_some()) && lower.is_none_or(|_| l.is_some());
    if endpoints_exist && gap(d, mt, map, xs, p, (u, l), m)? {
        return Ok(l.unwrap_or(xs.len()));
    }
    if mode == 1 {
        for j in 0..=xs.len() {
            if gap(
                d,
                mt,
                map,
                xs,
                p,
                (j.checked_sub(1), if j < xs.len() { Some(j) } else { None }),
                m,
            )? {
                return Ok(j);
            }
        }
        return Err(Failure::Undefined);
    }
    if let Some(i) = u {
        step(m)?;
        if number_cmp(p, at_priority(d, mt, map, xs, i, m)?, m)? == Ordering::Greater {
            u = None;
        }
    }
    if let Some(i) = l {
        step(m)?;
        if number_cmp(p, at_priority(d, mt, map, xs, i, m)?, m)? == Ordering::Less {
            l = None;
        }
    }
    if xs.is_empty() {
        return Ok(0);
    }
    if u.is_none() && l.is_some() {
        let start = l.ok_or(Failure::Undefined)?;
        if start + 1 == xs.len()
            && number_cmp(p, at_priority(d, mt, map, xs, start, m)?, m)? != Ordering::Greater
        {
            return Ok(xs.len());
        }
        for j in (0..=start).rev() {
            if gap(d, mt, map, xs, p, (j.checked_sub(1), Some(j)), m)? {
                return Ok(j);
            }
        }
    } else {
        let start = u.unwrap_or(0);
        if start == 0 && number_cmp(p, at_priority(d, mt, map, xs, 0, m)?, m)? != Ordering::Less {
            return Ok(0);
        }
        for j in start + 1..=xs.len() {
            if gap(
                d,
                mt,
                map,
                xs,
                p,
                (Some(j - 1), if j < xs.len() { Some(j) } else { None }),
                m,
            )? {
                return Ok(j);
            }
        }
    }
    Err(Failure::Undefined)
}
pub(super) fn edit(
    d: &Descriptor,
    s: &OrderSpec,
    ct: u32,
    pre: &Value,
    cmd: &Value,
    m: &mut Meter,
) -> Result<Result<Value, EditError>, Failure> {
    step(m)?;
    let (mt, st) = fields(d, s)?;
    let (map, seq) = parts(s, pre)?;
    let (kt, vt, _, limits) = d.table(mt)?;
    let (op, id, incoming, upper, lower) = command(d, ct, cmd)?;
    let mut key = copy(id, m)?;
    let mut new = None;
    // New item preflight precedes membership, including replacement at capacity.
    if op == 0 {
        d.check_value(kt, &key, m)?;
        d.check_value(vt, incoming, m)?;
        let kb = size(&key, m)?;
        let vb = size(incoming, m)?;
        let n = 16u64
            .checked_add(kb)
            .and_then(|x| x.checked_add(vb))
            .ok_or(Failure::Arithmetic)?;
        if n > limits.max_item_bytes {
            return Ok(Err(EditError::ItemTooLarge));
        }
        new = Some(copy(incoming, m)?);
    }
    if op == 2 {
        let Some(old) = collection::lookup(d, mt, map, &key, m)? else {
            return Ok(Err(EditError::MissingKey));
        };
        let fs = d.record(vt)?;
        let p = record_field(incoming, fs[0].id)?;
        reserve(&old, m)?;
        new = Some(record_replace(&old, &[(fs[0].id, copy(p, m)?)])?);
    }
    if op == 3 {
        let Some((id, _)) = minimum(d, s, pre, m)? else {
            return Ok(Err(EditError::Empty));
        };
        key = id;
    }
    let out = match collection::edit(
        d,
        mt,
        map,
        &key,
        new.as_ref(),
        if op == 1 || op == 3 { 2 } else { 0 },
        m,
    )? {
        Ok(x) => x,
        Err(e) => return Ok(Err(e)),
    };
    let mut replacements = vec![(s.map, out)];
    if let (Some(seq), Some(st)) = (seq, st) {
        reserve(seq, m)?;
        let mut xs = Vec::new();
        for id in slots(seq)? {
            visit(m)?;
            if compare(id, &key, m)? != Ordering::Equal {
                xs.push(copy(id, m)?);
            }
        }
        if let Some(item) = new {
            let p = priority(&item, m)?;
            let j = cut(
                d,
                (mt, &replacements[0].1),
                &xs,
                p,
                (upper, lower),
                s.mode,
                m,
            )?;
            reserve(&key, m)?;
            xs.insert(j, key);
        }
        let Shape::Vector { bytes, .. } = d.ty(st)?.shape else {
            return Err(Failure::Kind);
        };
        let sequence = Value::vector(xs).map_err(|_| Failure::Wire)?;
        if size(&sequence, m)? > bytes {
            return Ok(Err(EditError::SequenceTooLarge));
        }
        replacements.push((s.sequence.ok_or(Failure::Descriptor)?, sequence));
    }
    reserve(pre, m)?;
    for (_, value) in &replacements {
        reserve(value, m)?;
    }
    let post = record_replace(pre, &replacements)?;
    reserve(&post, m)?;
    Ok(Ok(post))
}
