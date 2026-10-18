use super::super::meter::{self, Meter};
use super::collection::{self, step};
use super::schema::{record_field, record_replace};
use super::*;

/// Checked carry addition, scoped to the law accumulator rather than Value arithmetic.
fn add_unsigned_sum(sum: [u128; 2], value: u128) -> Result<[u128; 2], Failure> {
    let (low, carry) = sum[0].overflowing_add(value);
    let high = sum[1]
        .checked_add(u128::from(carry))
        .ok_or(Failure::Arithmetic)?;
    Ok([low, high])
}
fn numeric_compare(a: Data, b: Data) -> Result<core::cmp::Ordering, Failure> {
    fn unsigned(data: Data) -> Result<[u128; 2], Failure> {
        match data {
            Data::WideSum(sum) => Ok(sum),
            Data::Value(v) => match v.view() {
                ValueRef::U128(n) => Ok([n, 0]),
                _ => Err(Failure::Kind),
            },
            _ => Err(Failure::Kind),
        }
    }
    if let (Data::Value(a), Data::Value(b)) = (&a, &b) {
        if let (ValueRef::I128(a), ValueRef::I128(b)) = (a.view(), b.view()) {
            return Ok(a.cmp(&b));
        }
    }
    let a = unsigned(a)?;
    let b = unsigned(b)?;
    Ok((a[1], a[0]).cmp(&(b[1], b[0])))
}

#[derive(Clone, Debug)]
enum Ty {
    Bool,
    Int(i128, i128),
    Unsigned(Option<u128>),
    WideSum,
    Bytes,
    Unit,
    Canonical(u32),
    Lookup(Box<Ty>),
    Edit(u32),
}
fn ty(d: &Descriptor, id: u32) -> Result<Ty, Failure> {
    Ok(match d.ty(id)?.shape {
        Shape::Bool => Ty::Bool,
        Shape::I128 { min, max } => Ty::Int(min, max),
        Shape::U128 { max, .. } => Ty::Unsigned(Some(max)),
        Shape::Bytes { .. } => Ty::Bytes,
        Shape::Unit => Ty::Unit,
        _ => Ty::Canonical(id),
    })
}
fn same(a: &Ty, b: &Ty) -> bool {
    matches!(
        (a, b),
        (Ty::Bool, Ty::Bool)
            | (Ty::Int(..), Ty::Int(..))
            | (Ty::Unsigned(_), Ty::Unsigned(_))
            | (Ty::WideSum, Ty::WideSum)
            | (Ty::Bytes, Ty::Bytes)
            | (Ty::Unit, Ty::Unit)
    ) || matches!((a,b),(Ty::Canonical(x),Ty::Canonical(y)) if x==y)
}
fn join(a: Ty, b: Ty) -> Result<Ty, Failure> {
    if !same(&a, &b) {
        return Err(Failure::Kind);
    }
    Ok(match (a, b) {
        (Ty::Int(a, b), Ty::Int(c, d)) => Ty::Int(a.min(c), b.max(d)),
        (Ty::Unsigned(a), Ty::Unsigned(b)) => Ty::Unsigned(a.zip(b).map(|(a, b)| a.max(b))),
        (a, _) => a,
    })
}
fn boolean(t: Ty) -> Result<(), Failure> {
    if matches!(t, Ty::Bool) {
        Ok(())
    } else {
        Err(Failure::Kind)
    }
}
fn integer(t: Ty) -> Result<(i128, i128), Failure> {
    if let Ty::Int(a, b) = t {
        Ok((a, b))
    } else {
        Err(Failure::Kind)
    }
}
fn canonical(t: Ty, id: u32) -> Result<(), Failure> {
    if matches!(t,Ty::Canonical(n) if n==id) {
        Ok(())
    } else {
        Err(Failure::Kind)
    }
}
struct Check<'a> {
    d: &'a Descriptor,
    payload: Option<Ty>,
    entry: Option<(u32, u32)>,
    error: bool,
    nodes: usize,
    sources: u8,
    candidate: bool,
    law: bool,
}
impl Check<'_> {
    fn expr(&mut self, e: &Expr, depth: usize) -> Result<Ty, Failure> {
        self.nodes += 1;
        if depth > 48 || self.nodes > 16384 {
            return Err(Failure::Descriptor);
        }
        Ok(match e {
            Expr::Ordered(op, mode, view, extra, args) => {
                let spec = super::ordered::spec(self.d, *mode, *view)?;
                let (map, _) = super::ordered::fields(self.d, spec)?;
                let (key, item, _, limits) = self.d.table(map)?;
                let count = if *op == 1 {
                    2
                } else if *op == 4 {
                    4
                } else {
                    1
                };
                if args.len() != count {
                    return Err(Failure::Kind);
                }
                canonical(self.expr(&args[0], depth + 1)?, *view)?;
                match op {
                    0 => {
                        if *extra != 0 {
                            return Err(Failure::Kind);
                        }
                        Ty::Bool
                    }
                    1 => {
                        if *extra != self.d.roots[1] {
                            return Err(Failure::Kind);
                        }
                        super::ordered::command_types(self.d, spec, *extra)?;
                        canonical(self.expr(&args[1], depth + 1)?, *extra)?;
                        Ty::Edit(*view)
                    }
                    2 | 3 => {
                        let pair = if *op == 2 {
                            *extra
                        } else {
                            let Shape::Vector { element, max, .. } = self.d.ty(*extra)?.shape
                            else {
                                return Err(Failure::Kind);
                            };
                            if max != limits.max_entries {
                                return Err(Failure::Kind);
                            }
                            element
                        };
                        if !matches!(&self.d.ty(pair)?.shape,Shape::Tuple(ids) if ids.as_slice()==[key,item])
                        {
                            return Err(Failure::Kind);
                        }
                        if *op == 2 {
                            Ty::Lookup(Box::new(ty(self.d, pair)?))
                        } else {
                            Ty::Canonical(*extra)
                        }
                    }
                    4 => {
                        if *extra > 2 && !self.d.reasons.contains(extra) {
                            return Err(Failure::Kind);
                        }
                        super::ordered::command_types(self.d, spec, self.d.roots[1])?;
                        canonical(self.expr(&args[1], depth + 1)?, self.d.roots[1])?;
                        canonical(self.expr(&args[2], depth + 1)?, *view)?;
                        integer(self.expr(&args[3], depth + 1)?)?;
                        Ty::Bool
                    }
                    _ => return Err(Failure::Kind),
                }
            }
            Expr::Literal(id, v) => {
                let mut m = meter::new(super::schema::all_limits());
                self.d.check_value(*id, v, &mut m)?;
                collection::check_literal(self.d, *id, v, &mut m, 0)?;
                if self.d.order.is_some() && *id == self.d.roots[0] {
                    collection::check_state(self.d, v, &mut m)?;
                }
                ty(self.d, *id)?
            }
            Expr::Input(source, path) => {
                if *source > 3 || self.sources & (1 << source) == 0 {
                    return Err(Failure::Kind);
                }
                let mut id = match source {
                    0 | 3 => self.d.roots[0],
                    1 => self.d.roots[1],
                    2 => self.d.roots[2],
                    _ => return Err(Failure::Descriptor),
                };
                for field in path {
                    id = self
                        .d
                        .record(id)?
                        .iter()
                        .find(|f| f.id == *field)
                        .ok_or(Failure::Descriptor)?
                        .type_id;
                }
                ty(self.d, id)?
            }
            Expr::Payload => self.payload.clone().ok_or(Failure::Kind)?,
            Expr::EntryKey => ty(self.d, self.entry.ok_or(Failure::Kind)?.0)?,
            Expr::EntryValue => ty(self.d, self.entry.ok_or(Failure::Kind)?.1)?,
            Expr::Tuple(id, es) => {
                let Shape::Tuple(ids) = &self.d.ty(*id)?.shape else {
                    return Err(Failure::Kind);
                };
                if ids.len() != es.len() {
                    return Err(Failure::Kind);
                }
                for (e, id) in es.iter().zip(ids) {
                    let expected = ty(self.d, *id)?;
                    if !same(&self.expr(e, depth + 1)?, &expected) {
                        return Err(Failure::Kind);
                    }
                }
                Ty::Canonical(*id)
            }
            Expr::Eq(a, b) => {
                let a = self.expr(a, depth + 1)?;
                let b = self.expr(b, depth + 1)?;
                if !same(&a, &b)
                    && !matches!(
                        (&a, &b),
                        (Ty::WideSum, Ty::Unsigned(_)) | (Ty::Unsigned(_), Ty::WideSum)
                    )
                {
                    return Err(Failure::Kind);
                }
                Ty::Bool
            }
            Expr::Lt(a, b) => {
                let a = self.expr(a, depth + 1)?;
                let b = self.expr(b, depth + 1)?;
                if !matches!(
                    (a, b),
                    (Ty::Int(..), Ty::Int(..))
                        | (Ty::Unsigned(_), Ty::Unsigned(_))
                        | (Ty::WideSum, Ty::WideSum)
                        | (Ty::WideSum, Ty::Unsigned(_))
                        | (Ty::Unsigned(_), Ty::WideSum)
                ) {
                    return Err(Failure::Kind);
                }
                Ty::Bool
            }
            Expr::Add(a, b) | Expr::Sub(a, b) => {
                let a = self.expr(a, depth + 1)?;
                let b = self.expr(b, depth + 1)?;
                if let (Ty::Unsigned(a), Ty::Unsigned(b)) = (&a, &b) {
                    return Ok(Ty::Unsigned(if matches!(e, Expr::Add(..)) {
                        a.zip(*b).and_then(|(a, b)| a.checked_add(b))
                    } else {
                        *a
                    }));
                }
                let (a0, a1) = integer(a)?;
                let (b0, b1) = integer(b)?;
                let bounds = if matches!(e, Expr::Add(..)) {
                    a0.checked_add(b0).zip(a1.checked_add(b1))
                } else {
                    a0.checked_sub(b1).zip(a1.checked_sub(b0))
                };
                let (lo, hi) = bounds.unwrap_or((i128::MIN, i128::MAX));
                Ty::Int(lo, hi)
            }
            Expr::And(a, b) => {
                boolean(self.expr(a, depth + 1)?)?;
                boolean(self.expr(b, depth + 1)?)?;
                Ty::Bool
            }
            Expr::Not(a) => {
                boolean(self.expr(a, depth + 1)?)?;
                Ty::Bool
            }
            Expr::Empty(id) => {
                self.d.table(*id)?;
                Ty::Canonical(*id)
            }
            Expr::Get(id, a, k) | Expr::Contains(id, a, k) | Expr::Remove(id, a, k) => {
                let (kt, vt, _, _) = self.d.table(*id)?;
                canonical(self.expr(a, depth + 1)?, *id)?;
                let expected = ty(self.d, kt)?;
                if !same(&self.expr(k, depth + 1)?, &expected) {
                    return Err(Failure::Kind);
                }
                match e {
                    Expr::Get(..) => Ty::Lookup(Box::new(ty(self.d, vt)?)),
                    Expr::Contains(..) => Ty::Bool,
                    _ => Ty::Edit(*id),
                }
            }
            Expr::Len(id, a) | Expr::IsEmpty(id, a) => {
                let (_, _, _, l) = self.d.table(*id)?;
                canonical(self.expr(a, depth + 1)?, *id)?;
                if matches!(e, Expr::Len(..)) {
                    Ty::Int(0, l.max_entries.into())
                } else {
                    Ty::Bool
                }
            }
            Expr::Insert(id, a, k, v) | Expr::Update(id, a, k, v) => {
                let (kt, vt, set, _) = self.d.table(*id)?;
                canonical(self.expr(a, depth + 1)?, *id)?;
                let key = ty(self.d, kt)?;
                let value = ty(self.d, vt)?;
                if !same(&self.expr(k, depth + 1)?, &key)
                    || !same(&self.expr(v, depth + 1)?, &value)
                {
                    return Err(Failure::Kind);
                }
                if set && matches!(e, Expr::Update(..)) {
                    return Err(Failure::Kind);
                }
                Ty::Edit(*id)
            }
            Expr::MatchLookup(a, no, yes) => {
                let Ty::Lookup(p) = self.expr(a, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                let left = self.expr(no, depth + 1)?;
                let old = self.payload.replace(*p);
                let right = self.expr(yes, depth + 1)?;
                self.payload = old;
                join(left, right)?
            }
            Expr::MatchEdit(a, ok, err) => {
                let Ty::Edit(id) = self.expr(a, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                let old = self.payload.replace(Ty::Canonical(id));
                let left = self.expr(ok, depth + 1)?;
                self.payload = old;
                let old = self.error;
                self.error = true;
                let right = self.expr(err, depth + 1)?;
                self.error = old;
                join(left, right)?
            }
            Expr::ErrorReason => {
                if !self.error {
                    return Err(Failure::Kind);
                }
                Ty::Int(303, if self.d.ordered() { 315 } else { 308 })
            }
            Expr::Reason => {
                if !self.candidate {
                    return Err(Failure::Kind);
                }
                Ty::Int(0, u32::MAX.into())
            }
            Expr::Project(e, index) => {
                let Ty::Canonical(id) = self.expr(e, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                let id = match &self.d.ty(id)?.shape {
                    Shape::Tuple(ids) => *ids.get(*index as usize).ok_or(Failure::Kind)?,
                    Shape::Record(fs) => {
                        fs.iter()
                            .find(|f| f.id == *index)
                            .ok_or(Failure::Kind)?
                            .type_id
                    }
                    _ => return Err(Failure::Kind),
                };
                ty(self.d, id)?
            }
            Expr::MatchBool(e, a, b) => {
                boolean(self.expr(e, depth + 1)?)?;
                let a = self.expr(a, depth + 1)?;
                let b = self.expr(b, depth + 1)?;
                join(a, b)?
            }
            Expr::All(a, body)
            | Expr::Any(a, body)
            | Expr::Count(a, body)
            | Expr::SumI128(a, body)
            | Expr::SumU128(a, body) => {
                if self.entry.is_some() {
                    return Err(Failure::Descriptor);
                }
                let Ty::Canonical(id) = self.expr(a, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                let (kt, vt, _, limits) = self.d.table(id)?;
                self.entry = Some((kt, vt));
                let body_ty = self.expr(body, depth + 1)?;
                self.entry = None;
                if matches!(e, Expr::SumU128(..)) {
                    if !self.law {
                        return Err(Failure::Kind);
                    }
                    let Ty::Unsigned(maximum) = body_ty else {
                        return Err(Failure::Kind);
                    };
                    // C is u32, hence C*B < 2^160. Exact multiplication supplies
                    // the U256 bound; lack of a static B refuses even for C=0.
                    let maximum = maximum.ok_or(Failure::Arithmetic)?;
                    let _bound = super::super::super::value::wide::multiply(
                        limits.max_entries.into(),
                        maximum,
                    );
                    Ty::WideSum
                } else if matches!(e, Expr::SumI128(..)) {
                    let (lo, hi) = integer(body_ty)?;
                    if lo == i128::MIN || hi == i128::MIN {
                        return Err(Failure::Arithmetic);
                    }
                    let abs = lo.unsigned_abs().max(hi.unsigned_abs());
                    let maximum = abs
                        .checked_mul(limits.max_entries.into())
                        .ok_or(Failure::Arithmetic)?;
                    if maximum > i128::MAX as u128 {
                        return Err(Failure::Arithmetic);
                    }
                    let b = maximum as i128;
                    Ty::Int(-b, b)
                } else {
                    boolean(body_ty)?;
                    if matches!(e, Expr::Count(..)) {
                        Ty::Int(0, limits.max_entries.into())
                    } else {
                        Ty::Bool
                    }
                }
            }
        })
    }
    fn plan(&mut self, p: &Plan, depth: usize) -> Result<(), Failure> {
        self.nodes += 1;
        if depth > 48 || self.nodes > 16384 {
            return Err(Failure::Descriptor);
        }
        match p {
            Plan::Reject(n) => {
                if !self.d.reasons.contains(n) {
                    return Err(Failure::Descriptor);
                }
            }
            Plan::RejectEdit => {
                if !self.error
                    || [303, 306, 307, 308]
                        .iter()
                        .any(|n| !self.d.reasons.contains(n))
                {
                    return Err(Failure::Kind);
                }
            }
            Plan::Accept(values) => {
                if values.windows(2).any(|w| w[0].0 >= w[1].0) {
                    return Err(Failure::Descriptor);
                }
                for (id, e) in values {
                    let f = self
                        .d
                        .record(self.d.roots[0])?
                        .iter()
                        .find(|f| f.id == *id)
                        .ok_or(Failure::Kind)?;
                    let t = ty(self.d, f.type_id)?;
                    if !same(&self.expr(e, depth + 1)?, &t) {
                        return Err(Failure::Kind);
                    }
                }
            }
            Plan::MatchLookup(e, no, yes) => {
                let Ty::Lookup(t) = self.expr(e, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                self.plan(no, depth + 1)?;
                let old = self.payload.replace(*t);
                self.plan(yes, depth + 1)?;
                self.payload = old;
            }
            Plan::MatchEdit(e, ok, err) => {
                let Ty::Edit(id) = self.expr(e, depth + 1)? else {
                    return Err(Failure::Kind);
                };
                let old = self.payload.replace(Ty::Canonical(id));
                self.plan(ok, depth + 1)?;
                self.payload = old;
                let old = self.error;
                self.error = true;
                self.plan(err, depth + 1)?;
                self.error = old;
            }
        }
        Ok(())
    }
}
pub(crate) fn admit(d: &Descriptor) -> Result<(), Failure> {
    let mut c = Check {
        d,
        payload: None,
        entry: None,
        error: false,
        nodes: 0,
        sources: 7,
        candidate: false,
        law: false,
    };
    for case in &d.cases {
        boolean(c.expr(&case.when, 0)?)?;
        c.plan(&case.plan, 0)?;
    }
    for law in &d.laws {
        match law {
            Law::Predicate {
                expression, scope, ..
            } => {
                c.law = true;
                c.sources = match scope {
                    Scope::State | Scope::Genesis => 9,
                    Scope::Reject => 7,
                    _ => 15,
                };
                c.candidate = matches!(scope, Scope::Accept | Scope::Reject | Scope::Any);
                boolean(c.expr(expression, 0)?)?;
            }
            Law::Frame { field, keys, .. } => {
                c.law = false;
                c.sources = 7;
                c.candidate = false;
                let f = d
                    .record(d.roots[0])?
                    .iter()
                    .find(|f| f.id == *field)
                    .ok_or(Failure::Descriptor)?;
                let (kt, _, _, _) = d.table(f.type_id)?;
                let expected = ty(d, kt)?;
                for key in keys {
                    if !same(&c.expr(key, 0)?, &expected) {
                        return Err(Failure::Kind);
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) struct Env<'a> {
    pub pre: &'a Value,
    pub command: Option<&'a Value>,
    pub context: Option<&'a Value>,
    pub post: Option<&'a Value>,
    pub payload: Option<Value>,
    pub entry: Option<(Value, Value)>,
    pub error: Option<EditError>,
    pub candidate_reason: Option<i128>,
}
impl Env<'_> {
    pub(crate) fn eval(
        &mut self,
        d: &Descriptor,
        e: &Expr,
        m: &mut Meter,
    ) -> Result<Data, Failure> {
        step(m)?;
        Ok(match e {
            Expr::Ordered(op, mode, view, extra, args) => {
                let spec = super::ordered::spec(d, *mode, *view)?;
                let args = args
                    .iter()
                    .map(|e| self.eval(d, e, m)?.value())
                    .collect::<Result<Vec<_>, _>>()?;
                match op {
                    0 => Data::Value(Value::boolean(super::ordered::valid(d, spec, &args[0], m)?)),
                    1 => Data::Edit(super::ordered::edit(
                        d, spec, *extra, &args[0], &args[1], m,
                    )?),
                    2 => Data::Lookup(
                        super::ordered::minimum(d, spec, &args[0], m)?
                            .map(|(key, item)| {
                                Value::tuple(alloc::vec![key, item]).map_err(|_| Failure::Wire)
                            })
                            .transpose()?,
                    ),
                    3 => {
                        let out = super::ordered::items(d, spec, &args[0], m)?;
                        d.check_value(*extra, &out, m)?;
                        collection::check_literal(d, *extra, &out, m, 0)?;
                        Data::Value(out)
                    }
                    4 => Data::Value(Value::boolean(super::order_laws::law(
                        d,
                        spec,
                        *extra,
                        &args,
                        self.context.ok_or(Failure::Undefined)?,
                        m,
                    )?)),
                    _ => return Err(Failure::Kind),
                }
            }
            Expr::Literal(_, v) => Data::Value(collection::copy(v, m)?),
            Expr::Payload => Data::Value(collection::copy(
                self.payload.as_ref().ok_or(Failure::Undefined)?,
                m,
            )?),
            Expr::EntryKey => Data::Value(collection::copy(
                &self.entry.as_ref().ok_or(Failure::Undefined)?.0,
                m,
            )?),
            Expr::EntryValue => Data::Value(collection::copy(
                &self.entry.as_ref().ok_or(Failure::Undefined)?.1,
                m,
            )?),
            Expr::Input(source, path) => {
                let mut v = match source {
                    0 => self.pre,
                    1 => self.command.ok_or(Failure::Undefined)?,
                    2 => self.context.ok_or(Failure::Undefined)?,
                    3 => self.post.ok_or(Failure::Undefined)?,
                    _ => return Err(Failure::Kind),
                };
                m.charge(Resource::Read, 1)?;
                for id in path {
                    m.charge(Resource::Read, 1)?;
                    v = record_field(v, *id)?;
                }
                Data::Value(collection::copy(v, m)?)
            }
            Expr::Tuple(id, xs) => {
                let values = xs
                    .iter()
                    .map(|e| self.eval(d, e, m)?.value())
                    .collect::<Result<Vec<_>, _>>()?;
                let v = Value::tuple(values).map_err(|_| Failure::Wire)?;
                d.check_value(*id, &v, m)?;
                Data::Value(v)
            }
            Expr::Eq(a, b) => {
                let a = self.eval(d, a, m)?;
                if !matches!(a, Data::Value(_) | Data::WideSum(_)) {
                    return Err(Failure::Kind);
                }
                let b = self.eval(d, b, m)?;
                let equal = if matches!(a, Data::WideSum(_)) || matches!(b, Data::WideSum(_)) {
                    m.charge(Resource::Byte, 64)?;
                    numeric_compare(a, b)? == core::cmp::Ordering::Equal
                } else {
                    let a = a.value()?;
                    let b = b.value()?;
                    collection::reserve(&a, m)?;
                    collection::reserve(&b, m)?;
                    a == b
                };
                Data::Value(Value::boolean(equal))
            }
            Expr::Lt(a, b) => {
                let a = self.eval(d, a, m)?;
                match &a {
                    Data::WideSum(_) => {}
                    Data::Value(v) if matches!(v.view(), ValueRef::I128(_) | ValueRef::U128(_)) => {
                    }
                    _ => return Err(Failure::Kind),
                }
                let b = self.eval(d, b, m)?;
                Data::Value(Value::boolean(
                    numeric_compare(a, b)? == core::cmp::Ordering::Less,
                ))
            }
            Expr::Add(a, b) | Expr::Sub(a, b) => {
                let a = self.eval(d, a, m)?.value()?;
                if !matches!(a.view(), ValueRef::I128(_) | ValueRef::U128(_)) {
                    return Err(Failure::Kind);
                }
                let b = self.eval(d, b, m)?.value()?;
                if let (ValueRef::U128(a), ValueRef::U128(b)) = (a.view(), b.view()) {
                    let n = if matches!(e, Expr::Add(..)) {
                        a.checked_add(b)
                    } else {
                        a.checked_sub(b)
                    }
                    .ok_or(Failure::Arithmetic)?;
                    return Ok(Data::Value(Value::unsigned(n)));
                }
                let a = Data::Value(a).integer()?;
                let b = Data::Value(b).integer()?;
                let v = if matches!(e, Expr::Add(..)) {
                    a.checked_add(b)
                } else {
                    a.checked_sub(b)
                }
                .ok_or(Failure::Arithmetic)?;
                Data::Value(Value::signed(v))
            }
            Expr::And(a, b) => {
                let a = self.eval(d, a, m)?.boolean()?;
                let b = self.eval(d, b, m)?.boolean()?;
                Data::Value(Value::boolean(a && b))
            }
            Expr::Not(a) => Data::Value(Value::boolean(!self.eval(d, a, m)?.boolean()?)),
            Expr::Empty(id) => {
                d.table(*id)?;
                Data::Value(Value::map_canonical(Vec::new()).map_err(|_| Failure::Wire)?)
            }
            Expr::Get(id, a, k) | Expr::Contains(id, a, k) => {
                let table = self.eval(d, a, m)?.value()?;
                let key = self.eval(d, k, m)?.value()?;
                let v = collection::lookup(d, *id, &table, &key, m)?;
                if matches!(e, Expr::Contains(..)) {
                    Data::Value(Value::boolean(v.is_some()))
                } else {
                    Data::Lookup(v)
                }
            }
            Expr::Len(_, a) | Expr::IsEmpty(_, a) => {
                let table = self.eval(d, a, m)?.value()?;
                let n = collection::len(&table, m)?;
                Data::Value(if matches!(e, Expr::Len(..)) {
                    Value::signed(n)
                } else {
                    Value::boolean(n == 0)
                })
            }
            Expr::Insert(id, a, k, v) | Expr::Update(id, a, k, v) => {
                let table = self.eval(d, a, m)?.value()?;
                let key = self.eval(d, k, m)?.value()?;
                let val = self.eval(d, v, m)?.value()?;
                Data::Edit(collection::edit(
                    d,
                    *id,
                    &table,
                    &key,
                    Some(&val),
                    u8::from(matches!(e, Expr::Update(..))),
                    m,
                )?)
            }
            Expr::Remove(id, a, k) => {
                let table = self.eval(d, a, m)?.value()?;
                let key = self.eval(d, k, m)?.value()?;
                Data::Edit(collection::edit(d, *id, &table, &key, None, 2, m)?)
            }
            Expr::MatchLookup(e, no, yes) => {
                let Data::Lookup(v) = self.eval(d, e, m)? else {
                    return Err(Failure::Kind);
                };
                match v {
                    None => self.eval(d, no, m)?,
                    Some(v) => {
                        let old = self.payload.replace(v);
                        let r = self.eval(d, yes, m);
                        self.payload = old;
                        r?
                    }
                }
            }
            Expr::MatchEdit(e, ok, err) => {
                let Data::Edit(v) = self.eval(d, e, m)? else {
                    return Err(Failure::Kind);
                };
                match v {
                    Ok(v) => {
                        let old = self.payload.replace(v);
                        let r = self.eval(d, ok, m);
                        self.payload = old;
                        r?
                    }
                    Err(e) => {
                        let old = self.error.replace(e);
                        let r = self.eval(d, err, m);
                        self.error = old;
                        r?
                    }
                }
            }
            Expr::Reason => Data::Value(Value::signed(
                self.candidate_reason.ok_or(Failure::Undefined)?,
            )),
            Expr::Project(e, index) => {
                let v = self.eval(d, e, m)?.value()?;
                m.charge(Resource::Read, 1)?;
                let child = match v.view() {
                    ValueRef::Tuple(xs) => xs.get(*index as usize).ok_or(Failure::Undefined)?,
                    ValueRef::Record(_) => record_field(&v, *index)?,
                    _ => return Err(Failure::Kind),
                };
                Data::Value(collection::copy(child, m)?)
            }
            Expr::MatchBool(e, a, b) => {
                if self.eval(d, e, m)?.boolean()? {
                    self.eval(d, a, m)?
                } else {
                    self.eval(d, b, m)?
                }
            }
            Expr::ErrorReason => Data::Value(Value::signed(
                self.error.ok_or(Failure::Undefined)?.reason().into(),
            )),
            Expr::All(table, body)
            | Expr::Any(table, body)
            | Expr::Count(table, body)
            | Expr::SumI128(table, body)
            | Expr::SumU128(table, body) => {
                let table = self.eval(d, table, m)?.value()?;
                let ValueRef::Map(entries) = table.view() else {
                    return Err(Failure::Kind);
                };
                let mut all = true;
                let mut any = false;
                let mut n = 0i128;
                let mut wide = [0u128; 2];
                if matches!(e, Expr::SumU128(..)) {
                    m.charge(Resource::Byte, 32)?;
                }
                for entry in entries {
                    collection::visit(m)?;
                    let old = self.entry.replace((
                        collection::copy(entry.key(), m)?,
                        collection::copy(entry.value(), m)?,
                    ));
                    let value = self.eval(d, body, m);
                    self.entry = old;
                    if matches!(e, Expr::SumU128(..)) {
                        step(m)?;
                        let value = value?.value()?;
                        let ValueRef::U128(value) = value.view() else {
                            return Err(Failure::Kind);
                        };
                        wide = add_unsigned_sum(wide, value)?;
                    } else if matches!(e, Expr::SumI128(..)) {
                        step(m)?;
                        n = n
                            .checked_add(value?.integer()?)
                            .ok_or(Failure::Arithmetic)?;
                    } else {
                        let value = value?.boolean()?;
                        all &= value;
                        any |= value;
                        if value {
                            step(m)?;
                            n = n.checked_add(1).ok_or(Failure::Arithmetic)?;
                        }
                    }
                }
                if matches!(e, Expr::SumU128(..)) {
                    return Ok(Data::WideSum(wide));
                }
                Data::Value(match e {
                    Expr::All(..) => Value::boolean(all),
                    Expr::Any(..) => Value::boolean(any),
                    _ => Value::signed(n),
                })
            }
        })
    }
    pub(crate) fn plan(
        &mut self,
        d: &Descriptor,
        p: &Plan,
        m: &mut Meter,
    ) -> Result<(Option<u32>, Option<Value>), Failure> {
        step(m)?;
        match p {
            Plan::Reject(reason) => Ok((Some(*reason), None)),
            Plan::RejectEdit => Ok((Some(self.error.ok_or(Failure::Undefined)?.reason()), None)),
            Plan::Accept(assignments) => {
                let mut values = Vec::new();
                for (id, e) in assignments {
                    m.charge(Resource::Write, 1)?;
                    let value = self.eval(d, e, m)?.value()?;
                    let field = d
                        .record(d.roots[0])?
                        .iter()
                        .find(|f| f.id == *id)
                        .ok_or(Failure::Kind)?;
                    d.check_value(field.type_id, &value, m)?;
                    values.push((*id, value));
                }
                collection::reserve(self.pre, m)?;
                Ok((None, Some(record_replace(self.pre, &values)?)))
            }
            Plan::MatchLookup(e, no, yes) => {
                let Data::Lookup(v) = self.eval(d, e, m)? else {
                    return Err(Failure::Kind);
                };
                match v {
                    None => self.plan(d, no, m),
                    Some(v) => {
                        let old = self.payload.replace(v);
                        let r = self.plan(d, yes, m);
                        self.payload = old;
                        r
                    }
                }
            }
            Plan::MatchEdit(e, ok, err) => {
                let Data::Edit(v) = self.eval(d, e, m)? else {
                    return Err(Failure::Kind);
                };
                match v {
                    Ok(v) => {
                        let old = self.payload.replace(v);
                        let r = self.plan(d, ok, m);
                        self.payload = old;
                        r
                    }
                    Err(e) => {
                        let old = self.error.replace(e);
                        let r = self.plan(d, err, m);
                        self.error = old;
                        r
                    }
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct LawReport {
    pub id: u32,
    pub holds: bool,
}
pub(super) fn state_laws(d: &Descriptor, state: &Value, m: &mut Meter) -> Result<(), Failure> {
    let mut env = Env {
        pre: state,
        command: None,
        context: None,
        post: Some(state),
        payload: None,
        entry: None,
        error: None,
        candidate_reason: None,
    };
    for law in &d.laws {
        if let Law::Predicate {
            id,
            scope: Scope::State,
            expression,
        } = law
        {
            if !env.eval(d, expression, m)?.boolean()? {
                return Err(Failure::Law(*id));
            }
        }
    }
    Ok(())
}
pub(crate) fn laws(
    d: &Descriptor,
    env: &mut Env<'_>,
    reason: Option<u32>,
    genesis: bool,
    m: &mut Meter,
) -> Result<Vec<LawReport>, Failure> {
    let mut out = Vec::new();
    for law in &d.laws {
        let holds = match law {
            Law::Predicate {
                scope, expression, ..
            } => {
                let applies = match scope {
                    Scope::State => false,
                    Scope::Genesis => genesis,
                    Scope::Accept => !genesis && reason.is_none(),
                    Scope::Reject => !genesis && reason.is_some(),
                    Scope::Any => !genesis,
                };
                if !applies {
                    continue;
                }
                env.eval(d, expression, m)?.boolean()?
            }
            Law::Frame { field, keys, .. } => {
                if genesis || reason.is_some() {
                    continue;
                }
                let keys = keys
                    .iter()
                    .map(|e| env.eval(d, e, m)?.value())
                    .collect::<Result<Vec<_>, _>>()?;
                collection::frame(
                    record_field(env.pre, *field)?,
                    record_field(env.post.ok_or(Failure::Undefined)?, *field)?,
                    &keys,
                    m,
                )?
            }
        };
        out.push(LawReport {
            id: law.id(),
            holds,
        });
        if !holds {
            return Err(Failure::Law(law.id()));
        }
    }
    Ok(out)
}
/// Nonauthorizing law probe for finite judges. It cannot construct an Evaluation.
pub fn check_laws(
    d: &Descriptor,
    pre: &Value,
    command: &Value,
    context: &Value,
    post: Option<&Value>,
    reason: Option<u32>,
) -> Result<Vec<LawReport>, Failure> {
    d.validate()?;
    probe(d, pre, command, context, post, reason)
}
pub(super) fn probe(
    d: &Descriptor,
    pre: &Value,
    command: &Value,
    context: &Value,
    post: Option<&Value>,
    reason: Option<u32>,
) -> Result<Vec<LawReport>, Failure> {
    let mut meter = meter::new(d.limits);
    probe_meter(d, pre, command, context, post, reason, &mut meter)
}
pub(super) fn probe_meter(
    d: &Descriptor,
    pre: &Value,
    command: &Value,
    context: &Value,
    post: Option<&Value>,
    reason: Option<u32>,
    meter: &mut Meter,
) -> Result<Vec<LawReport>, Failure> {
    for (source, (id, value)) in [
        (d.roots[0], pre),
        (d.roots[1], command),
        (d.roots[2], context),
    ]
    .into_iter()
    .enumerate()
    {
        if d.order.is_some() {
            super::order_laws::admit_frame(d, source, value, meter)?;
        }
        d.check_value(id, value, meter)?;
    }
    collection::check_state(d, pre, meter)?;
    if let Some(post) = post {
        if d.order.is_some() {
            super::order_laws::admit_frame(d, 0, post, meter)?;
        }
        d.check_value(d.roots[0], post, meter)?;
        collection::check_state(d, post, meter)?;
    }
    if reason.is_some_and(|r| !d.reasons.contains(&r)) || reason.is_some() == post.is_some() {
        return Err(Failure::Kind);
    }
    state_laws(d, pre, meter)?;
    if let Some(post) = post {
        state_laws(d, post, meter)?;
    }
    laws(
        d,
        &mut Env {
            pre,
            command: Some(command),
            context: Some(context),
            post,
            payload: None,
            entry: None,
            error: None,
            candidate_reason: Some(reason.map(i128::from).unwrap_or(0)),
        },
        reason,
        false,
        meter,
    )
}
