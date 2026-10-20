use super::super::{
    compound,
    meter::{self, Meter},
};
use super::*;
use crate::finite::canonical_v2::schema as s;

impl Descriptor {
    pub(crate) fn ordered(&self) -> bool {
        self.order.is_some()
            || self
                .types
                .iter()
                .any(|t| matches!(t.shape, Shape::Vector { .. } | Shape::Maybe(_)))
    }
    pub(crate) fn ty(&self, id: u32) -> Result<&Type, Failure> {
        self.types
            .iter()
            .find(|t| t.id == id)
            .ok_or(Failure::Descriptor)
    }
    pub(crate) fn table(&self, id: u32) -> Result<(u32, u32, bool, CollectionLimits), Failure> {
        match self.ty(id)?.shape {
            Shape::Table {
                key,
                value,
                set,
                limits,
            } => Ok((key, value, set, limits)),
            _ => Err(Failure::Kind),
        }
    }
    pub(crate) fn record(&self, id: u32) -> Result<&[FieldType], Failure> {
        match &self.ty(id)?.shape {
            Shape::Record(f) => Ok(f),
            _ => Err(Failure::Kind),
        }
    }
    pub(crate) fn with_schema<R>(&self, f: impl FnOnce(&s::Description<'_>) -> R) -> R {
        let fields: Vec<Vec<s::Field<'_>>> = self
            .types
            .iter()
            .map(|t| match &t.shape {
                Shape::Record(fs) => fs
                    .iter()
                    .map(|v| s::Field {
                        id: v.id,
                        name: &v.name,
                        type_id: v.type_id,
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        let variants: Vec<Vec<s::SumVariant<'_>>> = self
            .types
            .iter()
            .map(|t| match t.shape {
                Shape::Maybe(element) => alloc::vec![
                    s::SumVariant {
                        id: 0,
                        name: b"None",
                        payload: None
                    },
                    s::SumVariant {
                        id: 1,
                        name: b"Some",
                        payload: Some(element)
                    }
                ],
                _ => Vec::new(),
            })
            .collect();
        let definitions: Vec<s::Definition<'_>> = self
            .types
            .iter()
            .enumerate()
            .map(|(i, t)| s::Definition {
                id: t.id,
                name: &t.name,
                kind: match &t.shape {
                    Shape::Unit => s::Kind::Unit,
                    Shape::Bool => s::Kind::Bool,
                    Shape::U128 { min, max } => s::Kind::U128 {
                        min: *min,
                        max: *max,
                    },
                    Shape::I128 { min, max } => s::Kind::I128 {
                        min: *min,
                        max: *max,
                    },
                    Shape::Bytes { min, max } => s::Kind::Bytes {
                        min: *min,
                        max: *max,
                    },
                    Shape::Tuple(items) => s::Kind::Tuple(items),
                    Shape::Vector { element, max, .. } => s::Kind::Vector {
                        element: *element,
                        min: 0,
                        max: *max,
                    },
                    Shape::Maybe(_) => s::Kind::SumPayload(&variants[i]),
                    Shape::Record(_) => s::Kind::Record(&fields[i]),
                    Shape::Table {
                        key, value, limits, ..
                    } => s::Kind::Map {
                        key: *key,
                        value: *value,
                        min: 0,
                        max: limits.max_entries,
                    },
                },
            })
            .collect();
        f(&s::Description {
            profile: &self.profile,
            version: 1,
            root: self.roots[0],
            definitions: &definitions,
        })
    }
    pub fn schema_bytes(&self) -> Result<Vec<u8>, Failure> {
        fn text(out: &mut Vec<u8>, v: &[u8]) -> Result<(), Failure> {
            out.extend_from_slice(
                &u16::try_from(v.len())
                    .map_err(|_| Failure::Schema)?
                    .to_be_bytes(),
            );
            out.extend_from_slice(v);
            Ok(())
        }
        fn count(out: &mut Vec<u8>, n: usize) -> Result<(), Failure> {
            out.extend_from_slice(&u32::try_from(n).map_err(|_| Failure::Schema)?.to_be_bytes());
            Ok(())
        }
        let mut b = b"ZFCISSCHEMA1\0".to_vec();
        text(&mut b, &self.profile)?;
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&self.roots[0].to_be_bytes());
        count(&mut b, self.types.len())?;
        for t in &self.types {
            b.extend_from_slice(&t.id.to_be_bytes());
            text(&mut b, &t.name)?;
            match &t.shape {
                Shape::Unit => b.push(0),
                Shape::Bool => b.push(1),
                Shape::U128 { min, max } => {
                    b.push(2);
                    b.extend_from_slice(&min.to_be_bytes());
                    b.extend_from_slice(&max.to_be_bytes());
                }
                Shape::I128 { min, max } => {
                    b.push(3);
                    b.extend_from_slice(&min.to_be_bytes());
                    b.extend_from_slice(&max.to_be_bytes());
                }
                Shape::Bytes { min, max } => {
                    b.push(4);
                    b.extend_from_slice(&min.to_be_bytes());
                    b.extend_from_slice(&max.to_be_bytes());
                }
                Shape::Vector { element, max, .. } => {
                    b.push(10);
                    b.extend_from_slice(&element.to_be_bytes());
                    b.extend_from_slice(&0u32.to_be_bytes());
                    b.extend_from_slice(&max.to_be_bytes());
                }
                Shape::Maybe(element) => {
                    b.push(9);
                    count(&mut b, 2)?;
                    b.extend_from_slice(&0u16.to_be_bytes());
                    text(&mut b, b"None")?;
                    b.push(0);
                    b.extend_from_slice(&1u16.to_be_bytes());
                    text(&mut b, b"Some")?;
                    b.push(1);
                    b.extend_from_slice(&element.to_be_bytes());
                }
                Shape::Tuple(items) => {
                    b.push(7);
                    count(&mut b, items.len())?;
                    for id in items {
                        b.extend_from_slice(&id.to_be_bytes());
                    }
                }
                Shape::Record(fields) => {
                    b.push(8);
                    count(&mut b, fields.len())?;
                    for v in fields {
                        b.extend_from_slice(&v.id.to_be_bytes());
                        text(&mut b, &v.name)?;
                        b.extend_from_slice(&v.type_id.to_be_bytes());
                    }
                }
                Shape::Table {
                    key, value, limits, ..
                } => {
                    b.push(11);
                    b.extend_from_slice(&key.to_be_bytes());
                    b.extend_from_slice(&value.to_be_bytes());
                    b.extend_from_slice(&0u32.to_be_bytes());
                    b.extend_from_slice(&limits.max_entries.to_be_bytes());
                }
            }
        }
        self.with_schema(|description| {
            s::admit(
                &b,
                description,
                s::Limits {
                    bytes: 1 << 20,
                    types: 256,
                    fields: 256,
                    variants: if self.ordered() { 2 } else { 0 },
                },
            )
            .map(|_| ())
            .map_err(|_| Failure::Schema)
        })?;
        Ok(b)
    }
    pub(super) fn check_value(
        &self,
        type_id: u32,
        value: &Value,
        meter: &mut Meter,
    ) -> Result<(), Failure> {
        let bytes = super::collection::encode(value, meter)?;
        let end = self
            .with_schema(|d| compound::walk(&bytes, d.definitions, 0, bytes.len(), type_id, meter))
            .map_err(|e| match e {
                compound::Failure::Wire => Failure::Wire,
                compound::Failure::Budget(e) => Failure::Budget(e),
            })?;
        if end != bytes.len() {
            return Err(Failure::Wire);
        }
        super::collection::reserve(value, meter)?;
        value
            .validate_limits(zeno_fcis_value::ValueLimits::default())
            .map_err(|_| Failure::Wire)?;
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), Failure> {
        if self.types.len() > 256
            || self.cases.is_empty()
            || self.cases.len() > 256
            || self.laws.len() > 256
            || self.frame_bytes.iter().any(|v| *v < 48)
        {
            return Err(Failure::Descriptor);
        }
        if self.reasons.is_empty()
            || self.reasons.len() > 256
            || self.reasons.contains(&0)
            || self.reasons.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(Failure::Descriptor);
        }
        self.schema_bytes()?;
        for root in self.roots {
            self.record(root)?;
        }
        fn plain(d: &Descriptor, id: u32, depth: usize, key: bool) -> Result<(), Failure> {
            if depth > 16 {
                return Err(Failure::Descriptor);
            }
            match &d.ty(id)?.shape {
                Shape::Unit if !key => Ok(()),
                Shape::Bool | Shape::I128 { .. } | Shape::U128 { .. } | Shape::Bytes { .. } => {
                    Ok(())
                }
                Shape::Tuple(ids) => {
                    for id in ids {
                        plain(d, *id, depth + 1, key)?;
                    }
                    Ok(())
                }
                Shape::Record(fs) if !key => {
                    for field in fs {
                        plain(d, field.type_id, depth + 1, false)?;
                    }
                    Ok(())
                }
                _ => Err(Failure::Descriptor),
            }
        }
        for t in &self.types {
            if let Shape::Table {
                key,
                value,
                set,
                limits,
            } = t.shape
            {
                plain(self, key, 0, true)?;
                plain(self, value, 0, false)?;
                if limits.max_snapshot_bytes < 29
                    || (set && !matches!(self.ty(value)?.shape, Shape::Unit))
                {
                    return Err(Failure::Descriptor);
                }
            }
        }
        fn state(d: &Descriptor, id: u32, depth: usize) -> Result<(), Failure> {
            if depth > 16 {
                return Err(Failure::Descriptor);
            }
            match &d.ty(id)?.shape {
                Shape::Table { .. } => Ok(()),
                Shape::Vector { element, .. } | Shape::Maybe(element) => {
                    plain(d, *element, depth + 1, true)
                }
                Shape::Record(fs) => {
                    for f in fs {
                        state(d, f.type_id, depth + 1)?;
                    }
                    Ok(())
                }
                Shape::Tuple(ids) => {
                    for id in ids {
                        state(d, *id, depth + 1)?;
                    }
                    Ok(())
                }
                _ => plain(d, id, depth, false),
            }
        }
        for t in &self.types {
            if let Shape::Vector { bytes, .. } = t.shape
                && bytes < 5
            {
                return Err(Failure::Descriptor);
            }
        }
        if let Some(spec) = &self.order {
            if [303, 306, 307, 308, 314, 315]
                .iter()
                .any(|r| !self.reasons.contains(r))
            {
                return Err(Failure::Descriptor);
            }
            super::ordered::admit(self, spec)?;
        }
        // Only ordered/1 admits a nested view record containing a map.
        // Tables are state fields only; no hidden nested table admission path.
        for (index, root) in self.roots.iter().enumerate() {
            for field in self.record(*root)? {
                match self.ty(field.type_id)?.shape {
                    Shape::Table { .. } if index == 0 => {}
                    Shape::Table { .. } => return Err(Failure::Descriptor),
                    Shape::Maybe(element) if index == 1 && self.ordered() => {
                        plain(self, element, 0, true)?
                    }
                    _ if index == 0 && self.ordered() => state(self, field.type_id, 0)?,
                    _ => plain(self, field.type_id, 0, false)?,
                }
            }
        }
        let mut m = meter::new(all_limits());
        self.check_value(self.roots[0], &self.genesis, &mut m)?;
        super::collection::check_state(self, &self.genesis, &mut m)?;
        if self.laws.windows(2).any(|p| p[0].id() >= p[1].id())
            || self.laws.iter().any(|v| v.id() == 0)
        {
            return Err(Failure::Descriptor);
        }
        expression::admit(self)?;
        Ok(())
    }
}
pub(crate) fn all_limits() -> Limits {
    let mut l = zero_limits();
    for r in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ] {
        l = l.with_limit(r, u64::MAX);
    }
    l
}
pub(crate) fn record_field(value: &Value, id: u16) -> Result<&Value, Failure> {
    match value.view() {
        ValueRef::Record(fields) => fields
            .iter()
            .find(|f| f.id() == id)
            .map(|f| f.value())
            .ok_or(Failure::Undefined),
        _ => Err(Failure::Kind),
    }
}
pub(crate) fn record_replace(value: &Value, values: &[(u16, Value)]) -> Result<Value, Failure> {
    let ValueRef::Record(fields) = value.view() else {
        return Err(Failure::Kind);
    };
    let out = fields
        .iter()
        .map(|f| {
            Field::new(
                f.id(),
                values
                    .iter()
                    .find(|(id, _)| *id == f.id())
                    .map(|(_, v)| v)
                    .unwrap_or(f.value())
                    .clone(),
            )
        })
        .collect();
    Value::record_canonical(out).map_err(|_| Failure::Wire)
}
