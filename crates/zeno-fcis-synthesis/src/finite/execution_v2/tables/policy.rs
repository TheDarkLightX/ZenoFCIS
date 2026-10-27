use super::*;
use alloc::vec;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits};
fn n(v: impl Into<i128>) -> Value {
    Value::signed(v.into())
}
fn v(xs: Vec<Value>) -> Result<Value, Failure> {
    Value::vector(xs).map_err(|_| Failure::Policy)
}
fn bytes(b: &[u8]) -> Result<Value, Failure> {
    Value::bytes(b.to_vec()).map_err(|_| Failure::Policy)
}
fn seq(x: &Value) -> Result<&[Value], Failure> {
    match x.view() {
        ValueRef::Vector(xs) => Ok(xs),
        _ => Err(Failure::Policy),
    }
}
fn at(xs: &[Value], i: usize) -> Result<&Value, Failure> {
    xs.get(i).ok_or(Failure::Policy)
}
fn int(x: &Value) -> Result<i128, Failure> {
    match x.view() {
        ValueRef::I128(n) => Ok(n),
        _ => Err(Failure::Policy),
    }
}
fn num<T: TryFrom<i128>>(x: &Value) -> Result<T, Failure> {
    T::try_from(int(x)?).map_err(|_| Failure::Policy)
}
fn blob(x: &Value) -> Result<Vec<u8>, Failure> {
    match x.view() {
        ValueRef::Bytes(b) => Ok(b.to_vec()),
        _ => Err(Failure::Policy),
    }
}
fn exact(x: &Value, len: usize) -> Result<&[Value], Failure> {
    let xs = seq(x)?;
    if xs.len() != len {
        return Err(Failure::Policy);
    }
    Ok(xs)
}
impl Expr {
    pub fn encoded(&self) -> Result<Value, Failure> {
        let id_a = |tag: i128, id: u32, a: &Expr| v(vec![n(tag), n(id), a.encoded()?]);
        let id_ab = |tag: i128, id: u32, a: &Expr, b: &Expr| {
            v(vec![n(tag), n(id), a.encoded()?, b.encoded()?])
        };
        let binary = |tag: i128, a: &Expr, b: &Expr| v(vec![n(tag), a.encoded()?, b.encoded()?]);
        match self {
            Self::Literal(id, x) => v(vec![n(0), n(*id), x.clone()]),
            Self::Input(source, path) => v(vec![
                n(1),
                n(*source),
                v(path.iter().map(|x| n(*x)).collect())?,
            ]),
            Self::Payload => v(vec![n(2)]),
            Self::EntryKey => v(vec![n(3)]),
            Self::EntryValue => v(vec![n(4)]),
            Self::Tuple(id, es) => v(vec![
                n(5),
                n(*id),
                v(es.iter().map(Self::encoded).collect::<Result<_, _>>()?)?,
            ]),
            Self::Eq(a, b) => binary(6, a, b),
            Self::Lt(a, b) => binary(7, a, b),
            Self::Add(a, b) => binary(8, a, b),
            Self::Sub(a, b) => binary(9, a, b),
            Self::And(a, b) => binary(10, a, b),
            Self::Not(a) => v(vec![n(11), a.encoded()?]),
            Self::Empty(id) => v(vec![n(12), n(*id)]),
            Self::Get(id, a, b) => id_ab(13, *id, a, b),
            Self::Contains(id, a, b) => id_ab(14, *id, a, b),
            Self::Len(id, a) => id_a(15, *id, a),
            Self::IsEmpty(id, a) => id_a(16, *id, a),
            Self::Insert(id, a, b, c) | Self::Update(id, a, b, c) => v(vec![
                n(if matches!(self, Self::Insert(..)) {
                    17
                } else {
                    18
                }),
                n(*id),
                a.encoded()?,
                b.encoded()?,
                c.encoded()?,
            ]),
            Self::Remove(id, a, b) => id_ab(19, *id, a, b),
            Self::MatchLookup(a, b, c) | Self::MatchEdit(a, b, c) => v(vec![
                n(if matches!(self, Self::MatchLookup(..)) {
                    20
                } else {
                    21
                }),
                a.encoded()?,
                b.encoded()?,
                c.encoded()?,
            ]),
            Self::ErrorReason => v(vec![n(22)]),
            Self::Reason => v(vec![n(27)]),
            Self::Project(e, index) => v(vec![n(28), e.encoded()?, n(*index)]),
            Self::MatchBool(a, b, c) => v(vec![n(29), a.encoded()?, b.encoded()?, c.encoded()?]),
            Self::All(a, b) => binary(23, a, b),
            Self::Any(a, b) => binary(24, a, b),
            Self::Count(a, b) => binary(25, a, b),
            Self::SumI128(a, b) => binary(26, a, b),
            Self::SumU128(a, b) => binary(31, a, b),
            Self::Ordered(op, mode, view, extra, args) => v(vec![
                n(30),
                n(*op),
                n(*mode),
                n(*view),
                n(*extra),
                v(args.iter().map(Self::encoded).collect::<Result<_, _>>()?)?,
            ]),
        }
    }
    pub fn from_encoded(value: &Value) -> Result<Self, Failure> {
        let xs = seq(value)?;
        let tag = int(at(xs, 0)?)?;
        let len = match tag {
            0 | 1 | 5 | 6..=10 | 15 | 16 | 23..=26 | 28 | 31 => 3,
            2..=4 | 22 | 27 => 1,
            11 | 12 => 2,
            13 | 14 | 19..=21 | 29 => 4,
            17 | 18 => 5,
            30 => 6,
            _ => return Err(Failure::Policy),
        };
        exact(value, len)?;
        let expr = |i| Self::from_encoded(at(xs, i)?).map(Box::new);
        let id = || num(at(xs, 1)?);
        Ok(match tag {
            0 => Self::Literal(id()?, at(xs, 2)?.clone()),
            1 => Self::Input(
                num(at(xs, 1)?)?,
                seq(at(xs, 2)?)?.iter().map(num).collect::<Result<_, _>>()?,
            ),
            2 => Self::Payload,
            3 => Self::EntryKey,
            4 => Self::EntryValue,
            5 => Self::Tuple(
                id()?,
                seq(at(xs, 2)?)?
                    .iter()
                    .map(Self::from_encoded)
                    .collect::<Result<_, _>>()?,
            ),
            6 => Self::Eq(expr(1)?, expr(2)?),
            7 => Self::Lt(expr(1)?, expr(2)?),
            8 => Self::Add(expr(1)?, expr(2)?),
            9 => Self::Sub(expr(1)?, expr(2)?),
            10 => Self::And(expr(1)?, expr(2)?),
            11 => Self::Not(expr(1)?),
            12 => Self::Empty(id()?),
            13 => Self::Get(id()?, expr(2)?, expr(3)?),
            14 => Self::Contains(id()?, expr(2)?, expr(3)?),
            15 => Self::Len(id()?, expr(2)?),
            16 => Self::IsEmpty(id()?, expr(2)?),
            17 => Self::Insert(id()?, expr(2)?, expr(3)?, expr(4)?),
            18 => Self::Update(id()?, expr(2)?, expr(3)?, expr(4)?),
            19 => Self::Remove(id()?, expr(2)?, expr(3)?),
            20 => Self::MatchLookup(expr(1)?, expr(2)?, expr(3)?),
            21 => Self::MatchEdit(expr(1)?, expr(2)?, expr(3)?),
            22 => Self::ErrorReason,
            23 => Self::All(expr(1)?, expr(2)?),
            24 => Self::Any(expr(1)?, expr(2)?),
            25 => Self::Count(expr(1)?, expr(2)?),
            26 => Self::SumI128(expr(1)?, expr(2)?),
            31 => Self::SumU128(expr(1)?, expr(2)?),
            27 => Self::Reason,
            28 => Self::Project(expr(1)?, num(&xs[2])?),
            29 => Self::MatchBool(expr(1)?, expr(2)?, expr(3)?),
            30 => Self::Ordered(
                num(&xs[1])?,
                num(&xs[2])?,
                num(&xs[3])?,
                num(&xs[4])?,
                seq(&xs[5])?
                    .iter()
                    .map(Self::from_encoded)
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(Failure::Policy),
        })
    }
}
impl Plan {
    pub fn encoded(&self) -> Result<Value, Failure> {
        match self {
            Self::Reject(id) => v(vec![n(0), n(*id)]),
            Self::RejectEdit => v(vec![n(1)]),
            Self::Accept(xs) => v(vec![
                n(2),
                v(xs.iter()
                    .map(|(id, e)| v(vec![n(*id), e.encoded()?]))
                    .collect::<Result<_, _>>()?)?,
            ]),
            Self::MatchLookup(e, a, b) | Self::MatchEdit(e, a, b) => v(vec![
                n(if matches!(self, Self::MatchLookup(..)) {
                    3
                } else {
                    4
                }),
                e.encoded()?,
                a.encoded()?,
                b.encoded()?,
            ]),
        }
    }
    pub fn from_encoded(value: &Value) -> Result<Self, Failure> {
        let xs = seq(value)?;
        let tag = int(at(xs, 0)?)?;
        let len = match tag {
            0 | 2 => 2,
            1 => 1,
            3 | 4 => 4,
            _ => return Err(Failure::Policy),
        };
        exact(value, len)?;
        Ok(match tag {
            0 => Self::Reject(num(at(xs, 1)?)?),
            1 => Self::RejectEdit,
            2 => Self::Accept(
                seq(at(xs, 1)?)?
                    .iter()
                    .map(|x| {
                        let p = exact(x, 2)?;
                        Ok((num(&p[0])?, Expr::from_encoded(&p[1])?))
                    })
                    .collect::<Result<_, Failure>>()?,
            ),
            3 => Self::MatchLookup(
                Expr::from_encoded(&xs[1])?,
                Box::new(Self::from_encoded(&xs[2])?),
                Box::new(Self::from_encoded(&xs[3])?),
            ),
            4 => Self::MatchEdit(
                Expr::from_encoded(&xs[1])?,
                Box::new(Self::from_encoded(&xs[2])?),
                Box::new(Self::from_encoded(&xs[3])?),
            ),
            _ => return Err(Failure::Policy),
        })
    }
}
impl Descriptor {
    fn encoded(&self) -> Result<Value, Failure> {
        let types = self
            .types
            .iter()
            .map(|t| {
                let shape = match &t.shape {
                    Shape::Unit => v(vec![n(0)])?,
                    Shape::Bool => v(vec![n(1)])?,
                    Shape::U128 { min, max } => {
                        v(vec![n(2), Value::unsigned(*min), Value::unsigned(*max)])?
                    }
                    Shape::I128 { min, max } => v(vec![n(3), n(*min), n(*max)])?,
                    Shape::Bytes { min, max } => v(vec![n(4), n(*min), n(*max)])?,
                    Shape::Vector {
                        element,
                        max,
                        bytes,
                    } => v(vec![n(10), n(*element), n(*max), n(*bytes)])?,
                    Shape::Maybe(element) => v(vec![n(9), n(*element)])?,
                    Shape::Tuple(ids) => v(vec![n(7), v(ids.iter().map(|x| n(*x)).collect())?])?,
                    Shape::Record(fields) => v(vec![
                        n(8),
                        v(fields
                            .iter()
                            .map(|f| v(vec![n(f.id), bytes(&f.name)?, n(f.type_id)]))
                            .collect::<Result<_, _>>()?)?,
                    ])?,
                    Shape::Table {
                        key,
                        value,
                        set,
                        limits,
                    } => v(vec![
                        n(if *set { 12 } else { 11 }),
                        n(*key),
                        n(*value),
                        n(limits.max_entries),
                        n(limits.max_item_bytes),
                        n(limits.max_snapshot_bytes),
                    ])?,
                };
                v(vec![n(t.id), bytes(&t.name)?, shape])
            })
            .collect::<Result<_, Failure>>()?;
        let cases = self
            .cases
            .iter()
            .map(|c| v(vec![c.when.encoded()?, c.plan.encoded()?]))
            .collect::<Result<_, Failure>>()?;
        let laws = self
            .laws
            .iter()
            .map(|l| match l {
                Law::Predicate {
                    id,
                    scope,
                    expression,
                } => v(vec![n(0), n(*id), n(*scope as i128), expression.encoded()?]),
                Law::Frame { id, field, keys } => v(vec![
                    n(1),
                    n(*id),
                    n(*field),
                    v(keys.iter().map(Expr::encoded).collect::<Result<_, _>>()?)?,
                ]),
            })
            .collect::<Result<_, Failure>>()?;
        let mut fields = vec![
            n(if self.ordered() { 4 } else { POLICY_VERSION }),
            bytes(&self.profile)?,
            v(types)?,
            v(self.roots.iter().map(|x| n(*x)).collect())?,
            v(self.frame_bytes.iter().map(|x| n(*x)).collect())?,
            self.genesis.clone(),
            v(cases)?,
            v(laws)?,
            v(self.limits.counters.iter().map(|x| n(*x)).collect())?,
            v(self.reasons.iter().map(|x| n(*x)).collect())?,
            bytes(if self.ordered() {
                b"tables-ordered/1"
            } else {
                PROFILE.as_bytes()
            })?,
        ];
        if self.ordered() {
            let spec = self.order.as_ref().ok_or(Failure::Descriptor)?;
            fields.push(v(vec![
                n(spec.mode),
                n(spec.map),
                n(spec.sequence.map(i128::from).unwrap_or(-1)),
            ])?);
        }
        v(fields)
    }
    pub fn policy_bytes(&self) -> Result<Vec<u8>, Failure> {
        self.validate()?;
        self.encoded()?
            .canonical_bytes()
            .map_err(|_| Failure::Policy)
    }
    pub fn from_policy(bytes: &[u8]) -> Result<Self, Failure> {
        let value = zeno_fcis_codec::decode_value(
            bytes,
            DecodeLimits {
                max_input_bytes: 1 << 20,
                ..DecodeLimits::default()
            },
        )
        .map_err(|_| Failure::Policy)?;
        let xs = seq(&value)?;
        if xs.len() != 11 && xs.len() != 12 {
            return Err(Failure::Policy);
        }
        let ordered = blob(&xs[10])? == b"tables-ordered/1" && int(&xs[0])? == 4;
        if !ordered && (blob(&xs[10])? != PROFILE.as_bytes() || int(&xs[0])? != POLICY_VERSION) {
            return Err(Failure::Policy);
        }
        if xs.len() != if ordered { 12 } else { 11 } {
            return Err(Failure::Policy);
        }
        let order = if ordered {
            let spec = exact(&xs[11], 3)?;
            Some(OrderSpec {
                mode: num(&spec[0])?,
                map: num(&spec[1])?,
                sequence: if int(&spec[2])? == -1 {
                    None
                } else {
                    Some(num(&spec[2])?)
                },
            })
        } else {
            None
        };
        let types = seq(&xs[2])?
            .iter()
            .map(|t| {
                let x = exact(t, 3)?;
                let form = seq(&x[2])?;
                let tag = int(at(form, 0)?)?;
                let shape = match tag {
                    0 | 1 => {
                        exact(&x[2], 1)?;
                        if tag == 0 { Shape::Unit } else { Shape::Bool }
                    }
                    2 => {
                        exact(&x[2], 3)?;
                        let unsigned = |v: &Value| match v.view() {
                            ValueRef::U128(n) => Ok(n),
                            _ => Err(Failure::Policy),
                        };
                        Shape::U128 {
                            min: unsigned(&form[1])?,
                            max: unsigned(&form[2])?,
                        }
                    }
                    3 => {
                        exact(&x[2], 3)?;
                        Shape::I128 {
                            min: int(&form[1])?,
                            max: int(&form[2])?,
                        }
                    }
                    4 => {
                        exact(&x[2], 3)?;
                        Shape::Bytes {
                            min: num(&form[1])?,
                            max: num(&form[2])?,
                        }
                    }
                    7 => {
                        exact(&x[2], 2)?;
                        Shape::Tuple(seq(&form[1])?.iter().map(num).collect::<Result<_, _>>()?)
                    }
                    8 => {
                        exact(&x[2], 2)?;
                        Shape::Record(
                            seq(&form[1])?
                                .iter()
                                .map(|f| {
                                    let f = exact(f, 3)?;
                                    Ok(FieldType {
                                        id: num(&f[0])?,
                                        name: blob(&f[1])?,
                                        type_id: num(&f[2])?,
                                    })
                                })
                                .collect::<Result<_, Failure>>()?,
                        )
                    }
                    9 if ordered => {
                        exact(&x[2], 2)?;
                        Shape::Maybe(num(&form[1])?)
                    }
                    10 if ordered => {
                        exact(&x[2], 4)?;
                        Shape::Vector {
                            element: num(&form[1])?,
                            max: num(&form[2])?,
                            bytes: num(&form[3])?,
                        }
                    }
                    11 | 12 => {
                        exact(&x[2], 6)?;
                        Shape::Table {
                            key: num(&form[1])?,
                            value: num(&form[2])?,
                            set: tag == 12,
                            limits: CollectionLimits {
                                max_entries: num(&form[3])?,
                                max_item_bytes: num(&form[4])?,
                                max_snapshot_bytes: num(&form[5])?,
                            },
                        }
                    }
                    _ => return Err(Failure::Policy),
                };
                Ok(Type {
                    id: num(&x[0])?,
                    name: blob(&x[1])?,
                    shape,
                })
            })
            .collect::<Result<_, Failure>>()?;
        let roots = exact(&xs[3], 3)?;
        let frames = exact(&xs[4], 3)?;
        let counters = exact(&xs[8], 8)?;
        let cases = seq(&xs[6])?
            .iter()
            .map(|c| {
                let c = exact(c, 2)?;
                Ok(Case {
                    when: Expr::from_encoded(&c[0])?,
                    plan: Plan::from_encoded(&c[1])?,
                })
            })
            .collect::<Result<_, Failure>>()?;
        let laws = seq(&xs[7])?
            .iter()
            .map(|l| {
                let x = exact(l, 4)?;
                match int(&x[0])? {
                    0 => Ok(Law::Predicate {
                        id: num(&x[1])?,
                        scope: match int(&x[2])? {
                            0 => Scope::State,
                            1 => Scope::Accept,
                            2 => Scope::Reject,
                            3 => Scope::Any,
                            4 => Scope::Genesis,
                            _ => return Err(Failure::Policy),
                        },
                        expression: Expr::from_encoded(&x[3])?,
                    }),
                    1 => Ok(Law::Frame {
                        id: num(&x[1])?,
                        field: num(&x[2])?,
                        keys: seq(&x[3])?
                            .iter()
                            .map(Expr::from_encoded)
                            .collect::<Result<_, _>>()?,
                    }),
                    _ => Err(Failure::Policy),
                }
            })
            .collect::<Result<_, _>>()?;
        let mut limits = zero_limits();
        for (i, c) in counters.iter().enumerate() {
            limits.counters[i] = num(c)?;
        }
        let d = Self {
            order,
            profile: blob(&xs[1])?,
            types,
            roots: [num(&roots[0])?, num(&roots[1])?, num(&roots[2])?],
            frame_bytes: [num(&frames[0])?, num(&frames[1])?, num(&frames[2])?],
            genesis: xs[5].clone(),
            reasons: seq(&xs[9])?.iter().map(num).collect::<Result<_, _>>()?,
            cases,
            laws,
            limits,
        };
        if d.ordered() != ordered {
            return Err(Failure::Policy);
        }
        d.validate()?;
        Ok(d)
    }
}
