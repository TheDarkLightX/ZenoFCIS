//! Exact collection/effect observations for independent relation checks.
//! Caller-proposed movements remain data, never economic or mint authority.
use crate::{Evaluation, Failure, Limits, Work, ledger, run, wire};
use alloc::{collections::BTreeSet, vec, vec::Vec};
use zeno_fcis_schema::{TypeId, TypeKind};
use zeno_fcis_value::{Field, MapEntry, Value, ValueRef};
fn record(xs: Vec<Value>) -> Result<Value, Failure> {
    Value::record_canonical(
        xs.into_iter()
            .enumerate()
            .map(|(i, v)| Field::new(i as u16 + 1, v))
            .collect(),
    )
    .map_err(|_| Failure::Canonical)
}
fn fields(v: &Value) -> Result<&[Field], Failure> {
    match v.view() {
        ValueRef::Record(fs) => Ok(fs),
        _ => Err(Failure::Domain),
    }
}
fn entries(v: &Value) -> Result<&[MapEntry], Failure> {
    match v.view() {
        ValueRef::Map(es) => Ok(es),
        _ => Err(Failure::Domain),
    }
}
fn own(v: &Value, w: &mut Work) -> Result<Value, Failure> {
    let n = wire::size(v, w, 0)?;
    w.charge(1, 0, n)?;
    Ok(v.clone())
}
fn maybe(ty: u32, v: Option<&Value>, w: &mut Work) -> Result<Value, Failure> {
    Ok(match v {
        None => Value::sum(ty, 0, None),
        Some(x) => Value::sum(ty, 1, Some(own(x, w)?)),
    })
}
fn patch(
    policy: &wire::Policy,
    field: u16,
    key: &Value,
    a: Option<&Value>,
    b: Option<&Value>,
    w: &mut Work,
) -> Result<Value, Failure> {
    w.charge(1, 1, 0)?;
    let ty = match policy
        .schema()
        .type_by_id(TypeId::new(wire::PATCH))
        .ok_or(Failure::Domain)?
        .kind()
    {
        TypeKind::Sum { variants } => variants
            .iter()
            .find(|v| v.id().get() == field)
            .and_then(|v| v.payload())
            .ok_or(Failure::Domain)?,
        _ => return Err(Failure::Domain),
    };
    let mt = match policy
        .schema()
        .type_by_id(ty)
        .ok_or(Failure::Domain)?
        .kind()
    {
        TypeKind::Record { fields } => fields[1].type_id().get(),
        _ => return Err(Failure::Domain),
    };
    Ok(Value::sum(
        wire::PATCH,
        field,
        Some(record(vec![
            own(key, w)?,
            maybe(mt, a, w)?,
            maybe(mt, b, w)?,
        ])?),
    ))
}
fn movements(legs: &[ledger::Movement], w: &mut Work) -> Result<Value, Failure> {
    let n = u64::try_from(legs.len()).map_err(|_| Failure::Capacity)?;
    w.charge(n, n, n.checked_mul(256).ok_or(Failure::Arithmetic)?)?;
    let mut result = Vec::with_capacity(legs.len());
    for leg in legs {
        let account = |id: Option<crate::Id>| -> Result<Value, Failure> {
            Ok(match id {
                None => Value::sum(wire::MAYBE_ACCOUNT, 0, None),
                Some(id) => Value::sum(
                    wire::MAYBE_ACCOUNT,
                    1,
                    Some(Value::bytes(id.to_vec()).map_err(|_| Failure::Canonical)?),
                ),
            })
        };
        result.push(record(vec![
            account(leg.from)?,
            account(leg.to)?,
            Value::sum(
                wire::QUANTITY,
                leg.quantity.asset() as u16,
                Some(Value::unsigned(leg.quantity.atoms())),
            ),
        ])?);
    }
    Value::vector(result).map_err(|_| Failure::Canonical)
}
/// Preparation utility: compute canonical field/encoded-key patches and retain
/// the exact input movement order. No netting, alternate recipient or extra key.
pub fn observe(
    policy: &wire::Policy,
    pre: &Value,
    post: &Value,
    legs: &[ledger::Movement],
    limits: Limits,
) -> Evaluation<Value> {
    run(limits, |w| {
        let _post = w.invoke(|l| policy.preflight(pre, post, l))?;
        let a = fields(pre)?;
        let b = fields(post)?;
        let mut patches = Vec::new();
        for (old, new) in a.iter().zip(b) {
            let n = wire::size(old.value(), w, 0)?
                .checked_add(wire::size(new.value(), w, 0)?)
                .ok_or(Failure::Arithmetic)?;
            w.charge(n, n, n)?;
            if old.value() == new.value() {
                continue;
            }
            if old.id() == 1 || old.id() == 4 {
                patches.push(patch(
                    policy,
                    old.id(),
                    &Value::unit(),
                    Some(old.value()),
                    Some(new.value()),
                    w,
                )?);
                continue;
            }
            let aa = entries(old.value())?;
            let bb = entries(new.value())?;
            let mut i = 0;
            let mut j = 0;
            while i < aa.len() || j < bb.len() {
                w.charge(1, 1, 128)?;
                let l = aa.get(i);
                let r = bb.get(j);
                let cmp = match (l, r) {
                    (Some(x), Some(y)) => x.encoded_key().cmp(y.encoded_key()),
                    (Some(_), None) => core::cmp::Ordering::Less,
                    _ => core::cmp::Ordering::Greater,
                };
                match cmp {
                    core::cmp::Ordering::Less => {
                        let x = l.ok_or(Failure::Domain)?;
                        patches.push(patch(policy, old.id(), x.key(), Some(x.value()), None, w)?);
                        i += 1;
                    }
                    core::cmp::Ordering::Greater => {
                        let y = r.ok_or(Failure::Domain)?;
                        patches.push(patch(policy, old.id(), y.key(), None, Some(y.value()), w)?);
                        j += 1;
                    }
                    core::cmp::Ordering::Equal => {
                        let x = l.ok_or(Failure::Domain)?;
                        let y = r.ok_or(Failure::Domain)?;
                        w.charge(n, n, n)?;
                        if x.value() != y.value() {
                            patches.push(patch(
                                policy,
                                old.id(),
                                x.key(),
                                Some(x.value()),
                                Some(y.value()),
                                w,
                            )?);
                        }
                        i += 1;
                        j += 1;
                    }
                }
            }
        }
        let observation = record(vec![
            own(pre, w)?,
            own(post, w)?,
            Value::vector(patches).map_err(|_| Failure::Canonical)?,
            movements(legs, w)?,
        ])?;
        policy.validate(wire::OBSERVATION, &observation, w)?;
        let bytes = wire::size(&observation, w, 0)?
            .checked_add(48)
            .ok_or(Failure::Arithmetic)?;
        w.charge(1, 0, bytes)?;
        // The same explicit enclosing limit applies to prepared observations.
        let _framed = w.invoke(|l| policy.frame(wire::OBSERVATION, &observation, l))?;
        Ok(observation)
    })
}
/// Independent extensional observation relation. It enumerates the union of
/// encoded keys and compares full tagged lookups, independently of merge order.
/// It never calls observe/preflight or a producer case predicate.
pub fn check(
    policy: &wire::Policy,
    pre: &Value,
    post: &Value,
    legs: &[ledger::Movement],
    candidate: &Value,
    limits: Limits,
) -> Evaluation<()> {
    run(limits, |w| {
        policy.validate(wire::STATE, pre, w)?;
        policy.validate(wire::STATE, post, w)?;
        policy.original_bounds(pre, w)?;
        policy.original_bounds(post, w)?;
        policy.validate(wire::OBSERVATION, candidate, w)?;
        let c = fields(candidate)?;
        let n = wire::size(candidate, w, 0)?;
        w.charge(n, n, n)?;
        if c[0].value() != pre || c[1].value() != post || !movement_relation(c[3].value(), legs, w)?
        {
            return Err(Failure::Law(2100));
        }
        let given = match c[2].value().view() {
            ValueRef::Vector(xs) => xs,
            _ => return Err(Failure::Domain),
        };
        let unit = Value::unit();
        let mut expected = Vec::new();
        for (old, new) in fields(pre)?.iter().zip(fields(post)?) {
            let n = wire::size(old.value(), w, 0)?
                .checked_add(wire::size(new.value(), w, 0)?)
                .ok_or(Failure::Arithmetic)?;
            w.charge(n, n, n)?;
            if old.id() == 1 || old.id() == 4 {
                if old.value() != new.value() {
                    expected.push((old.id(), &unit, Some(old.value()), Some(new.value())));
                }
                continue;
            }
            let aa = entries(old.value())?;
            let bb = entries(new.value())?;
            let mut keys = BTreeSet::new();
            for row in aa.iter().chain(bb) {
                w.charge(1, 1, row.encoded_key().len() as u64)?;
                keys.insert(row.encoded_key().to_vec());
            }
            for key in keys {
                w.charge(
                    (aa.len() + bb.len()) as u64,
                    (aa.len() + bb.len()) as u64,
                    n,
                )?;
                let a = aa.iter().find(|r| r.encoded_key() == key);
                let b = bb.iter().find(|r| r.encoded_key() == key);
                if a.map(|e| e.value()) == b.map(|e| e.value()) {
                    continue;
                }
                let k = a.or(b).ok_or(Failure::Domain)?.key();
                expected.push((old.id(), k, a.map(|e| e.value()), b.map(|e| e.value())));
            }
        }
        w.charge(n, n, n)?;
        if expected.len() != given.len() {
            return Err(Failure::Law(2101));
        }
        for ((field, key, before, after), got) in expected.into_iter().zip(given) {
            let payload = match got.view() {
                ValueRef::Sum {
                    type_id,
                    variant,
                    payload: Some(v),
                } if type_id == wire::PATCH && variant == field => v,
                _ => return Err(Failure::Law(2101)),
            };
            let fs = fields(payload)?;
            w.charge(3, 3, 0)?;
            if fs[0].value() != key
                || !option_relation(fs[1].value(), before)
                || !option_relation(fs[2].value(), after)
            {
                return Err(Failure::Law(2101));
            }
        }
        Ok(())
    })
}
fn option_relation(v: &Value, expected: Option<&Value>) -> bool {
    match (v.view(), expected) {
        (
            ValueRef::Sum {
                variant: 0,
                payload: None,
                ..
            },
            None,
        ) => true,
        (
            ValueRef::Sum {
                variant: 1,
                payload: Some(v),
                ..
            },
            Some(e),
        ) => v == e,
        _ => false,
    }
}
fn account_relation(v: &Value, expected: Option<crate::Id>) -> bool {
    match (v.view(), expected) {
        (
            ValueRef::Sum {
                variant: 0,
                payload: None,
                ..
            },
            None,
        ) => true,
        (
            ValueRef::Sum {
                variant: 1,
                payload: Some(v),
                ..
            },
            Some(id),
        ) => matches!(v.view(),ValueRef::Bytes(b) if b==id),
        _ => false,
    }
}
fn movement_relation(
    value: &Value,
    legs: &[ledger::Movement],
    w: &mut Work,
) -> Result<bool, Failure> {
    let xs = match value.view() {
        ValueRef::Vector(xs) => xs,
        _ => return Ok(false),
    };
    if xs.len() != legs.len() {
        return Ok(false);
    }
    for (v, leg) in xs.iter().zip(legs) {
        w.charge(3, 3, 0)?;
        let fs = fields(v)?;
        if !account_relation(fs[0].value(), leg.from) || !account_relation(fs[1].value(), leg.to) {
            return Ok(false);
        }
        let (tag, amount) = match leg.quantity {
            ledger::Quantity::Zusd(n) => (0, n),
            ledger::Quantity::Agrs(n) => (1, n),
            ledger::Quantity::Zdex(n) => (2, n),
        };
        if !matches!(fs[2].value().view(),ValueRef::Sum{type_id,variant,payload:Some(x)} if type_id==wire::QUANTITY&&variant==tag&&matches!(x.view(),ValueRef::U128(n) if n==amount))
        {
            return Ok(false);
        }
    }
    Ok(true)
}
