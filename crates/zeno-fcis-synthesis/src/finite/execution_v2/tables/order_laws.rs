//! Independent extensional ordered relation. Does not call producer edit/minimum/cut.
//! It selects the required gap by its source-defined direction and constructs the
//! canonical union-domain successor directly; no case-conformance law is involved.
use super::super::meter::Meter;
use super::collection::{copy, encode, reserve, size, step, visit};
use super::ordered::{self, fields, integer, parts, priority, slots};
use super::schema::{record_field, record_replace};
use super::*;
use alloc::vec;

/// Independent admission for an original value's complete 48-byte envelope.
/// Size derivation is metered; reserve the full framed operand before comparing.
/// No producer execution, caller Usage or temporary serialization is required.
pub(super) fn admit_frame(
    d: &Descriptor,
    source: usize,
    value: &Value,
    m: &mut Meter,
) -> Result<(), Failure> {
    let bytes = size(value, m)?.checked_add(48).ok_or(Failure::Arithmetic)?;
    step(m)?;
    m.charge(Resource::Byte, bytes)?;
    if bytes > *d.frame_bytes.get(source).ok_or(Failure::Kind)? {
        return Err(Failure::Wire);
    }
    Ok(())
}

fn cut(
    d: &Descriptor,
    (mt, map): (u32, &Value),
    ids: &[Value],
    p: ordered::Priority,
    (upper, lower): (Option<&Value>, Option<&Value>),
    mode: u8,
    m: &mut Meter,
) -> Result<usize, Failure> {
    m.charge(
        Resource::Byte,
        (ids.len() as u64)
            .checked_mul(17)
            .ok_or(Failure::Arithmetic)?,
    )?;
    m.charge(
        Resource::Byte,
        (ids.len() as u64 + 1)
            .checked_mul(8)
            .ok_or(Failure::Arithmetic)?,
    )?;
    let mut priorities = Vec::new();
    let mut u = None;
    let mut l = None;
    for (i, id) in ids.iter().enumerate() {
        visit(m)?;
        if let Some(hint) = upper {
            step(m)?;
            reserve(id, m)?;
            reserve(hint, m)?;
            if id == hint {
                u = Some(i)
            }
        }
        if let Some(hint) = lower {
            step(m)?;
            reserve(id, m)?;
            reserve(hint, m)?;
            if id == hint {
                l = Some(i)
            }
        }
        priorities.push(priority(
            &super::collection::lookup(d, mt, map, id, m)?.ok_or(Failure::Undefined)?,
            m,
        )?);
    }
    let mut gaps = Vec::new();
    for j in 0..=ids.len() {
        step(m)?;
        let mut ok = true;
        if j > 0 {
            visit(m)?;
            ok &= ordered::number_cmp(priorities[j - 1], p, m)? != core::cmp::Ordering::Less;
        }
        if j < ids.len() {
            visit(m)?;
            ok &= ordered::number_cmp(p, priorities[j], m)? != core::cmp::Ordering::Less;
        }
        if ok {
            gaps.push(j)
        }
    }
    // Reserve both bounded scratch passes: hint membership and directional selection.
    for _ in 0..2 {
        for _ in &gaps {
            visit(m)?;
            m.charge(Resource::Byte, 8)?;
        }
    }
    let hinted = match (u, l) {
        (None, None) if upper.is_none() && lower.is_none() && ids.is_empty() => Some(0),
        (None, Some(0)) if upper.is_none() => Some(0),
        (Some(i), None) if lower.is_none() && i + 1 == ids.len() => Some(ids.len()),
        (Some(i), Some(j)) if i + 1 == j => Some(j),
        _ => None,
    };
    if let Some(j) = hinted
        && gaps.contains(&j)
    {
        return Ok(j);
    }
    if mode == 1 {
        return gaps.first().copied().ok_or(Failure::Undefined);
    }
    // Independent V1 transcription: remove impossible anchors, then choose first
    // admissible gap encountered in the linked-list search direction.
    if let Some(i) = u
        && ordered::number_cmp(priorities[i], p, m)? == core::cmp::Ordering::Less
    {
        u = None;
    }
    if let Some(i) = l
        && ordered::number_cmp(priorities[i], p, m)? == core::cmp::Ordering::Greater
    {
        l = None;
    }
    let selected = match (u, l) {
        (None, Some(j)) => {
            if j + 1 == ids.len()
                && ordered::number_cmp(p, priorities[j], m)? != core::cmp::Ordering::Greater
            {
                Some(ids.len())
            } else {
                gaps.iter().rev().copied().find(|x| *x <= j)
            }
        }
        (u, _) => {
            let i = u.unwrap_or(0);
            if i == 0
                && match priorities.first() {
                    None => true,
                    Some(x) => ordered::number_cmp(p, *x, m)? != core::cmp::Ordering::Less,
                }
            {
                Some(0)
            } else {
                gaps.iter().copied().find(|x| *x > i)
            }
        }
    };
    selected.ok_or(Failure::Undefined)
}
pub(super) fn outcome(
    d: &Descriptor,
    s: &OrderSpec,
    ct: u32,
    pre: &Value,
    cmd: &Value,
    context: &Value,
    m: &mut Meter,
) -> Result<Result<Value, EditError>, Failure> {
    for (source, value) in [pre, cmd, context].into_iter().enumerate() {
        admit_frame(d, source, value, m)?;
    }
    let (mt, st) = fields(d, s)?;
    let (map, seq) = parts(s, pre)?;
    let (_, vt, _, limits) = d.table(mt)?;
    let (op, id, incoming, upper, lower) = ordered::command(d, ct, cmd)?;
    let es = ordered::entries(map)?;
    let mut old = None;
    // Incoming insert's byte preflight is deliberately before the lookup pass.
    if op == 0 {
        let kb = size(id, m)?;
        let vb = size(incoming, m)?;
        if 16u64
            .checked_add(kb)
            .and_then(|x| x.checked_add(vb))
            .ok_or(Failure::Arithmetic)?
            > limits.max_item_bytes
        {
            return Ok(Err(EditError::ItemTooLarge));
        }
    }
    for e in es {
        visit(m)?;
        reserve(e.key(), m)?;
        reserve(e.value(), m)?;
        step(m)?;
        reserve(id, m)?;
        if e.key() == id {
            old = Some(e.value())
        }
    }
    if op == 2 && old.is_none() {
        return Ok(Err(EditError::MissingKey));
    }
    let mut target = copy(id, m)?;
    if op == 3 {
        if es.is_empty() {
            return Ok(Err(EditError::Empty));
        }
        if let Some(seq) = seq {
            visit(m)?;
            target = copy(slots(seq)?.last().ok_or(Failure::Undefined)?, m)?;
        } else {
            let mut chosen: Option<&MapEntry> = None;
            for e in es {
                visit(m)?;
                reserve(e.key(), m)?;
                reserve(e.value(), m)?;
                let better = if let Some(c) = chosen {
                    let a = priority(e.value(), m)?;
                    let b = priority(c.value(), m)?;
                    step(m)?;
                    a < b || (a == b && encode(e.key(), m)? < encode(c.key(), m)?)
                } else {
                    true
                };
                if better {
                    chosen = Some(e)
                }
            }
            target = copy(chosen.ok_or(Failure::Undefined)?.key(), m)?;
        }
    }
    let replacement = match op {
        0 => Some(copy(incoming, m)?),
        2 => {
            let f = d.record(vt)?[0].id;
            {
                let old = old.ok_or(Failure::Undefined)?;
                reserve(old, m)?;
                Some(record_replace(
                    old,
                    &[(f, copy(record_field(incoming, f)?, m)?)],
                )?)
            }
        }
        1 | 3 => None,
        _ => return Err(Failure::Kind),
    };
    if let Some(item) = &replacement {
        let kb = size(&target, m)?;
        let vb = size(item, m)?;
        let n = 16u64
            .checked_add(kb)
            .and_then(|x| x.checked_add(vb))
            .ok_or(Failure::Arithmetic)?;
        if n > limits.max_item_bytes {
            return Ok(Err(EditError::ItemTooLarge));
        }
        if op == 0 && old.is_none() && es.len() >= limits.max_entries as usize {
            return Ok(Err(EditError::Full));
        }
    }
    let q = encode(&target, m)?;
    let mut out = Vec::new();
    let mut inserted = false;
    for e in es {
        visit(m)?;
        reserve(e.key(), m)?;
        reserve(e.value(), m)?;
        step(m)?;
        m.charge(
            Resource::Byte,
            q.len() as u64 + e.encoded_key().len() as u64,
        )?;
        if !inserted && e.encoded_key() >= q.as_slice() {
            if let Some(x) = &replacement {
                out.push(
                    MapEntry::try_new(copy(&target, m)?, copy(x, m)?).map_err(|_| Failure::Wire)?,
                );
            }
            inserted = true;
        }
        if e.key() != &target {
            out.push(
                MapEntry::try_new(copy(e.key(), m)?, copy(e.value(), m)?)
                    .map_err(|_| Failure::Wire)?,
            );
        }
    }
    if !inserted && let Some(x) = replacement.as_ref() {
        out.push(MapEntry::try_new(copy(&target, m)?, copy(x, m)?).map_err(|_| Failure::Wire)?);
    }
    let mut snapshot = 29u64;
    for e in &out {
        visit(m)?;
        let a = size(e.key(), m)?;
        let b = size(e.value(), m)?;
        snapshot = snapshot
            .checked_add(16)
            .and_then(|x| x.checked_add(a))
            .and_then(|x| x.checked_add(b))
            .ok_or(Failure::Arithmetic)?;
    }
    if snapshot > limits.max_snapshot_bytes {
        return Ok(Err(EditError::SnapshotTooLarge));
    }
    let out = Value::map_canonical(out).map_err(|_| Failure::Wire)?;
    let mut replacements = vec![(s.map, out)];
    if let (Some(seq), Some(st)) = (seq, st) {
        let mut ids = Vec::new();
        reserve(seq, m)?;
        for x in slots(seq)? {
            visit(m)?;
            step(m)?;
            reserve(x, m)?;
            reserve(&target, m)?;
            if x != &target {
                ids.push(copy(x, m)?);
            }
        }
        if let Some(item) = &replacement {
            let p = priority(item, m)?;
            let j = cut(
                d,
                (mt, &replacements[0].1),
                &ids,
                p,
                (upper, lower),
                s.mode,
                m,
            )?;
            ids.insert(j, copy(&target, m)?);
        }
        let out = Value::vector(ids).map_err(|_| Failure::Wire)?;
        let Shape::Vector { bytes, .. } = d.ty(st)?.shape else {
            return Err(Failure::Kind);
        };
        if size(&out, m)? > bytes {
            return Ok(Err(EditError::SequenceTooLarge));
        }
        replacements.push((s.sequence.ok_or(Failure::Descriptor)?, out));
    }
    reserve(pre, m)?;
    for (_, value) in &replacements {
        reserve(value, m)?;
    }
    let post = record_replace(pre, &replacements)?;
    admit_frame(d, 0, &post, m)?;
    Ok(Ok(post))
}
pub(super) fn law(
    d: &Descriptor,
    s: &OrderSpec,
    kind: u32,
    args: &[Value],
    context: &Value,
    m: &mut Meter,
) -> Result<bool, Failure> {
    let expected = outcome(d, s, d.roots[1], &args[0], &args[1], context, m)?;
    let reason = integer(&args[3])?;
    step(m)?;
    if let Ok(expected) = &expected {
        reserve(expected, m)?;
        reserve(&args[2], m)?;
    }
    Ok(match kind {
        0 => reason != 0 || expected.is_ok(), // Accept implies eligibility.
        1 => reason != 0 || expected.as_ref().is_ok_and(|x| x == &args[2]), // Exact union-domain outcome.
        2 => expected.is_err() || reason == 0, // Legitimate input cannot be rejected.
        r => reason != i128::from(r) || expected.is_err_and(|e| e.reason() == r),
    })
}
/// Extensional result law, independent of producer minimum and edit.
pub(super) fn removed(
    d: &Descriptor,
    s: &OrderSpec,
    pre: &Value,
    cmd: &Value,
    reason: Option<u32>,
    m: &mut Meter,
) -> Result<Option<Value>, Failure> {
    if reason.is_some() {
        return Ok(None);
    }
    let (op, _, _, _, _) = ordered::command(d, d.roots[1], cmd)?;
    if op != 3 {
        return Ok(None);
    }
    let (map, seq) = parts(s, pre)?;
    let mut best: Option<&MapEntry> = None;
    let tail = seq.map(slots).transpose()?.and_then(|xs| xs.last());
    for entry in ordered::entries(map)? {
        visit(m)?;
        reserve(entry.key(), m)?;
        reserve(entry.value(), m)?;
        let choose = if let Some(tail) = tail {
            step(m)?;
            reserve(tail, m)?;
            entry.key() == tail
        } else if let Some(old) = best {
            let a = priority(entry.value(), m)?;
            let b = priority(old.value(), m)?;
            step(m)?;
            a < b || (a == b && encode(entry.key(), m)? < encode(old.key(), m)?)
        } else {
            true
        };
        if choose {
            best = Some(entry)
        }
    }
    let best = best.ok_or(Failure::Undefined)?;
    Ok(Some(
        Value::tuple(vec![copy(best.key(), m)?, copy(best.value(), m)?])
            .map_err(|_| Failure::Wire)?,
    ))
}
