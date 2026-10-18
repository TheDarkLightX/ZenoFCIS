//! The closed tables/1 compiler. Scalar-only consumers reject its rules schema.
use super::{
    ContractError,
    rules::{Json, Object},
};
use t::{Expr, Plan, Shape, Value};
use zeno_fcis_spec::{ProjectLimits, SourceLimits, TypeKind, elaborate_project, parse_project};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
const FILE: &str = "v2/policy.json";
pub const ORDERED_RULES_SCHEMA: &str = "zeno-fcis/tables-ordered-policy/1";
pub const RULES_SCHEMA: &str = "zeno-fcis/tables-policy/1";
fn err(message: impl ToString) -> ContractError {
    ContractError::new(FILE, message.to_string())
}
fn lib<T>(result: Result<T, t::Failure>) -> Result<T, ContractError> {
    result.map_err(|e| err(format!("tables/1 refused: {e:?}")))
}
fn parse(source: &str) -> Result<Json, ContractError> {
    serde_json::from_str(source).map_err(err)
}
pub fn recognizes(source: &str) -> Result<bool, ContractError> {
    let json = parse(source)?;
    let object = Object::new(&json, FILE)?;
    Ok(matches!(
        object.text("schema")?,
        RULES_SCHEMA | ORDERED_RULES_SCHEMA
    ))
}
fn num<T: TryFrom<i128>>(j: &Json) -> Result<T, ContractError> {
    match j {
        Json::Int(n) => T::try_from(*n).map_err(|_| err("integer out of range")),
        _ => Err(err("expected an integer")),
    }
}
fn text(j: &Json) -> Result<&str, ContractError> {
    match j {
        Json::Text(s) => Ok(s),
        _ => Err(err("expected text")),
    }
}
fn array(j: &Json) -> Result<&[Json], ContractError> {
    match j {
        Json::Array(s) => Ok(s),
        _ => Err(err("expected an array")),
    }
}
fn field<'a>(a: &'a [Json], i: usize) -> Result<&'a Json, ContractError> {
    a.get(i).ok_or_else(|| err("missing operator argument"))
}
fn vec(xs: Vec<Value>) -> Result<Value, ContractError> {
    Value::vector(xs).map_err(err)
}
fn n(j: &Json) -> Result<Value, ContractError> {
    Ok(Value::signed(num(j)?))
}
fn id(s: &str) -> Result<u32, ContractError> {
    let n = s.parse::<u32>().map_err(err)?;
    if n.to_string() != s {
        return Err(err("noncanonical decimal ID"));
    }
    Ok(n)
}

pub struct Compiled {
    pub descriptor: t::Descriptor,
    pub schema: Vec<u8>,
    pub policy: Vec<u8>,
    pub source: String,
}
fn value(types: &[t::Type], tid: u32, j: &Json) -> Result<Value, ContractError> {
    let shape = &types
        .iter()
        .find(|t| t.id == tid)
        .ok_or_else(|| err("unknown type"))?
        .shape;
    Ok(match shape {
        Shape::Unit => {
            if !matches!(j, Json::Null) {
                return Err(err("Unit must be null"));
            }
            Value::unit()
        }
        Shape::Bool => {
            let Json::Bool(b) = j else {
                return Err(err("expected Bool"));
            };
            Value::boolean(*b)
        }
        Shape::I128 { min, max } => {
            let n = num::<i128>(j)?;
            if n < *min || n > *max {
                return Err(err("integer outside declared range"));
            }
            Value::signed(n)
        }
        Shape::Bytes { min, max } => {
            let bytes = array(j)?.iter().map(num).collect::<Result<Vec<u8>, _>>()?;
            if bytes.len() < *min as usize || bytes.len() > *max as usize {
                return Err(err("bytes outside declared range"));
            }
            Value::bytes(bytes).map_err(err)?
        }
        Shape::Vector { element, max, .. } => {
            let xs = array(j)?;
            if xs.len() > *max as usize {
                return Err(err("vector count"));
            }
            Value::vector(
                xs.iter()
                    .map(|x| value(types, *element, x))
                    .collect::<Result<_, _>>()?,
            )
            .map_err(err)?
        }
        Shape::Maybe(element) => {
            if matches!(j, Json::Null) {
                Value::sum(tid, 0, None)
            } else {
                Value::sum(tid, 1, Some(value(types, *element, j)?))
            }
        }
        Shape::Tuple(ids) => {
            let a = array(j)?;
            if ids.len() != a.len() {
                return Err(err("tuple arity"));
            }
            Value::tuple(
                ids.iter()
                    .zip(a)
                    .map(|(id, v)| value(types, *id, v))
                    .collect::<Result<_, _>>()?,
            )
            .map_err(err)?
        }
        Shape::Record(fields) => {
            let o = Object::new(j, FILE)?;
            let allowed = fields.iter().map(|f| f.id.to_string()).collect::<Vec<_>>();
            o.only(&allowed.iter().map(String::as_str).collect::<Vec<_>>())?;
            Value::record_canonical(
                fields
                    .iter()
                    .map(|f| {
                        Ok(t::Field::new(
                            f.id,
                            value(types, f.type_id, o.field(&f.id.to_string())?)?,
                        ))
                    })
                    .collect::<Result<_, ContractError>>()?,
            )
            .map_err(err)?
        }
        Shape::Table { key, value: vt, .. } => Value::map_canonical(
            array(j)?
                .iter()
                .map(|j| {
                    let a = array(j)?;
                    if a.len() != 2 {
                        return Err(err("map entry needs key and value"));
                    }
                    t::MapEntry::try_new(value(types, *key, &a[0])?, value(types, *vt, &a[1])?)
                        .map_err(err)
                })
                .collect::<Result<_, _>>()?,
        )
        .map_err(err)?,
    })
}
fn expression(types: &[t::Type], j: &Json) -> Result<Expr, ContractError> {
    let a = array(j)?;
    let name = text(field(a, 0)?)?;
    let tag = match name {
        "literal" => 0,
        "input" => 1,
        "payload" => 2,
        "key" => 3,
        "value" => 4,
        "tuple" => 5,
        "eq" => 6,
        "lt" => 7,
        "add" => 8,
        "sub" => 9,
        "and" => 10,
        "not" => 11,
        "empty" => 12,
        "get" => 13,
        "contains" => 14,
        "len" => 15,
        "is_empty" => 16,
        "insert" => 17,
        "update" => 18,
        "remove" => 19,
        "match_lookup" => 20,
        "match_edit" => 21,
        "error_reason" => 22,
        "all" => 23,
        "any" => 24,
        "count" => 25,
        "sum_i128" => 26,
        "reason" => 27,
        "project" => 28,
        "match_bool" => 29,
        "ordered" => 30,
        _ => return Err(err(format!("unknown table expression {name}"))),
    };
    let mut xs = std::vec![Value::signed(tag)];
    match tag {
        0 => {
            if a.len() != 3 {
                return Err(err("literal arity"));
            }
            let tid = num(field(a, 1)?)?;
            xs.push(Value::signed(i128::from(tid)));
            xs.push(value(types, tid, field(a, 2)?)?);
        }
        1 => {
            if a.len() != 3 {
                return Err(err("input arity"));
            }
            let root = match text(field(a, 1)?)? {
                "pre" => 0,
                "command" => 1,
                "context" => 2,
                "post" => 3,
                _ => return Err(err("unknown input root")),
            };
            xs.push(Value::signed(root));
            xs.push(vec(array(field(a, 2)?)?
                .iter()
                .map(n)
                .collect::<Result<_, _>>()?)?);
        }
        5 => {
            if a.len() != 3 {
                return Err(err("tuple expression arity"));
            }
            xs.push(n(field(a, 1)?)?);
            xs.push(vec(array(field(a, 2)?)?
                .iter()
                .map(|e| lib(expression(types, e)?.encoded()))
                .collect::<Result<_, _>>()?)?);
        }
        30 => {
            if a.len() != 6 {
                return Err(err("ordered operator arity"));
            }
            for x in &a[1..5] {
                xs.push(n(x)?);
            }
            xs.push(vec(array(&a[5])?
                .iter()
                .map(|e| lib(expression(types, e)?.encoded()))
                .collect::<Result<_, _>>()?)?);
        }
        28 => {
            if a.len() != 3 {
                return Err(err("projection arity"));
            }
            xs.push(lib(expression(types, &a[1])?.encoded())?);
            xs.push(n(&a[2])?);
        }
        12..=19 => {
            xs.push(n(field(a, 1)?)?);
            for e in a.iter().skip(2) {
                xs.push(lib(expression(types, e)?.encoded())?);
            }
        }
        _ => {
            for e in a.iter().skip(1) {
                xs.push(lib(expression(types, e)?.encoded())?);
            }
        }
    }
    lib(Expr::from_encoded(&vec(xs)?))
}
fn plan(types: &[t::Type], j: &Json) -> Result<Plan, ContractError> {
    let a = array(j)?;
    let name = text(field(a, 0)?)?;
    match name {
        "reject" if a.len() == 2 => Ok(Plan::Reject(num(&a[1])?)),
        "reject_edit" if a.len() == 1 => Ok(Plan::RejectEdit),
        "accept" if a.len() == 2 => {
            let o = Object::new(&a[1], FILE)?;
            let mut xs = o
                .entries()
                .map(|(key, v)| Ok((u16::try_from(id(key)?).map_err(err)?, expression(types, v)?)))
                .collect::<Result<Vec<_>, ContractError>>()?;
            xs.sort_by_key(|x| x.0);
            Ok(Plan::Accept(xs))
        }
        "match_lookup" | "match_edit" if a.len() == 4 => {
            let e = expression(types, &a[1])?;
            let left = Box::new(plan(types, &a[2])?);
            let right = Box::new(plan(types, &a[3])?);
            Ok(if name == "match_lookup" {
                Plan::MatchLookup(e, left, right)
            } else {
                Plan::MatchEdit(e, left, right)
            })
        }
        _ => Err(err("unknown plan or wrong arity")),
    }
}

pub fn compile(project: &str, rules: &str) -> Result<Compiled, ContractError> {
    let json = parse(rules)?;
    let o = Object::new(&json, FILE)?;
    o.only(&[
        "schema",
        "bindings",
        "genesis",
        "cases",
        "laws",
        "budgets",
        "frame_bytes",
        "ordered_view",
    ])?;
    let ordered = o.text("schema")? == ORDERED_RULES_SCHEMA;
    if !ordered && o.text("schema")? != RULES_SCHEMA {
        return Err(err("not tables rules"));
    }
    let p = elaborate_project(
        parse_project(project, SourceLimits::default()).map_err(err)?,
        ProjectLimits::default(),
    )
    .map_err(err)?;
    if !p.variants().is_empty()
        || !p.channels().is_empty()
        || !p.effects().is_empty()
        || !p.claims().is_empty()
        || !p.components().is_empty()
        || !p.composition().wirings().is_empty()
        || !p.laws().is_empty()
    {
        return Err(err(
            "tables/1 does not lower variants, channels, effects, components, claims, composition or scalar law formulas; declare table laws in the reviewed policy",
        ));
    }
    for (root, kind) in [
        (100, TypeKind::State),
        (101, TypeKind::Command),
        (102, TypeKind::Context),
    ] {
        if !p
            .types()
            .iter()
            .any(|t| t.id().get() == root && t.kind() == kind)
        {
            return Err(err("state/command/context roots must be 100/101/102"));
        }
    }
    let bindings = o.object("bindings")?;
    let mut consumed = std::collections::BTreeSet::new();
    let mut types = Vec::new();
    for t in p.types() {
        let tid = t.id().get();
        let fields = p
            .fields()
            .iter()
            .filter(|f| f.owner() == t.id())
            .map(|f| {
                Ok(t::FieldType {
                    id: u16::try_from(f.id().get()).map_err(err)?,
                    name: f.name().as_str().as_bytes().to_vec(),
                    type_id: f.field_type().get(),
                })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        let binding = bindings.optional(&tid.to_string());
        let shape = if !fields.is_empty()
            || matches!(
                t.kind(),
                TypeKind::State | TypeKind::Command | TypeKind::Context
            ) {
            if binding.is_some() {
                return Err(err("binding overrides record"));
            }
            Shape::Record(fields)
        } else if let Some(range) = t.range() {
            if binding.is_some() {
                return Err(err("binding overrides integer range"));
            }
            Shape::I128 {
                min: range.min(),
                max: range.max(),
            }
        } else if t.kind() == TypeKind::Bool {
            if binding.is_some() {
                return Err(err("binding overrides Bool"));
            }
            Shape::Bool
        } else {
            let b = Object::new(
                binding.ok_or_else(|| err(format!("type {tid} needs binding")))?,
                FILE,
            )?;
            consumed.insert(tid);
            match b.text("kind")? {
                "unit" => {
                    b.only(&["kind"])?;
                    Shape::Unit
                }
                "bytes" => {
                    b.only(&["kind", "min", "max"])?;
                    Shape::Bytes {
                        min: b.number("min")?,
                        max: b.number("max")?,
                    }
                }
                "tuple" => {
                    b.only(&["kind", "items"])?;
                    Shape::Tuple(
                        b.array("items")?
                            .iter()
                            .map(num)
                            .collect::<Result<_, _>>()?,
                    )
                }
                "vector" if ordered => {
                    b.only(&["kind", "element", "max", "bytes"])?;
                    Shape::Vector {
                        element: b.number("element")?,
                        max: b.number("max")?,
                        bytes: b.number("bytes")?,
                    }
                }
                "maybe" if ordered => {
                    b.only(&["kind", "element"])?;
                    Shape::Maybe(b.number("element")?)
                }
                "map" | "set" => {
                    b.only(&[
                        "kind",
                        "key",
                        "value",
                        "capacity",
                        "item_bytes",
                        "snapshot_bytes",
                    ])?;
                    Shape::Table {
                        key: b.number("key")?,
                        value: b.number("value")?,
                        set: b.text("kind")? == "set",
                        limits: t::CollectionLimits {
                            max_entries: b.number("capacity")?,
                            max_item_bytes: b.number("item_bytes")?,
                            max_snapshot_bytes: b.number("snapshot_bytes")?,
                        },
                    }
                }
                _ => return Err(err("unsupported table binding")),
            }
        };
        types.push(t::Type {
            id: tid,
            name: t.name().as_str().as_bytes().to_vec(),
            shape,
        });
    }
    types.sort_by_key(|t| t.id);
    for (key, _) in bindings.entries() {
        if !consumed.contains(&id(key)?) {
            return Err(err("unused binding"));
        }
    }
    let genesis = value(&types, 100, o.field("genesis")?)?;
    let cases = o
        .array("cases")?
        .iter()
        .map(|j| {
            let o = Object::new(j, FILE)?;
            o.only(&["when", "then"])?;
            Ok(t::Case {
                when: expression(&types, o.field("when")?)?,
                plan: plan(&types, o.field("then")?)?,
            })
        })
        .collect::<Result<_, ContractError>>()?;
    let laws = o
        .array("laws")?
        .iter()
        .map(|j| {
            let o = Object::new(j, FILE)?;
            let id = o.number("id")?;
            if o.optional("frame").is_some() {
                o.only(&["id", "frame", "keys"])?;
                Ok(t::Law::Frame {
                    id,
                    field: o.number("frame")?,
                    keys: o
                        .array("keys")?
                        .iter()
                        .map(|j| expression(&types, j))
                        .collect::<Result<_, _>>()?,
                })
            } else {
                o.only(&["id", "scope", "assert"])?;
                Ok(t::Law::Predicate {
                    id,
                    scope: match o.text("scope")? {
                        "state" => t::Scope::State,
                        "accept" => t::Scope::Accept,
                        "reject" => t::Scope::Reject,
                        "any" => t::Scope::Any,
                        "genesis" => t::Scope::Genesis,
                        _ => return Err(err("unsupported law scope")),
                    },
                    expression: expression(&types, o.field("assert")?)?,
                })
            }
        })
        .collect::<Result<_, ContractError>>()?;
    let budgets = o.object("budgets")?;
    let names = [
        "read",
        "write",
        "candidate",
        "effect",
        "byte",
        "witness_byte",
        "depth",
        "step",
    ];
    budgets.only(&names)?;
    let mut limits = t::zero_limits();
    for (key, resource) in names.iter().zip([
        t::Resource::Read,
        t::Resource::Write,
        t::Resource::Candidate,
        t::Resource::Effect,
        t::Resource::Byte,
        t::Resource::WitnessByte,
        t::Resource::Depth,
        t::Resource::Step,
    ]) {
        limits = limits.with_limit(resource, budgets.number(key)?);
    }
    let frames = o.array("frame_bytes")?;
    if frames.len() != 3 {
        return Err(err("three frame limits required"));
    }
    let order = if ordered {
        let view = o.object("ordered_view")?;
        view.only(&["comparator", "map", "sequence"])?;
        Some(t::OrderSpec {
            mode: match view.text("comparator")? {
                "i128-priority/id/1" => 0,
                "i128-priority/committed/1" => 1,
                "zusd-v1-nicr/committed/1" => 2,
                _ => return Err(err("unknown comparator")),
            },
            map: view.number("map")?,
            sequence: match view.optional("sequence") {
                None | Some(Json::Null) => None,
                Some(x) => Some(num(x)?),
            },
        })
    } else {
        if o.optional("ordered_view").is_some() {
            return Err(err("ordered binding requires ordered rules"));
        }
        None
    };
    let descriptor = t::Descriptor {
        order,
        profile: p.name().as_str().as_bytes().to_vec(),
        types,
        roots: [100, 101, 102],
        frame_bytes: [num(&frames[0])?, num(&frames[1])?, num(&frames[2])?],
        genesis,
        reasons: p.reasons().iter().map(|r| r.id().get()).collect(),
        cases,
        laws,
        limits,
    };
    // Every explicit rejection must be declared in the original project.
    let declared = p
        .reasons()
        .iter()
        .map(|r| r.id().get())
        .collect::<std::collections::BTreeSet<_>>();
    fn reasons(p: &Plan, out: &mut Vec<u32>) {
        match p {
            Plan::Reject(n) => out.push(*n),
            Plan::RejectEdit => out.extend([303, 306, 307, 308]),
            Plan::Accept(_) => {}
            Plan::MatchLookup(_, a, b) | Plan::MatchEdit(_, a, b) => {
                reasons(a, out);
                reasons(b, out);
            }
        }
    }
    let mut used = Vec::new();
    for c in &descriptor.cases {
        reasons(&c.plan, &mut used)
    }
    if used.iter().any(|n| !declared.contains(n)) {
        return Err(err("undeclared table rejection reason"));
    }
    if ordered
        && [303, 306, 307, 308, 314, 315]
            .iter()
            .any(|r| !declared.contains(r))
    {
        return Err(err("ordered reason catalog incomplete"));
    }
    let schema = lib(descriptor.schema_bytes())?;
    let policy = lib(descriptor.policy_bytes())?;
    lib(t::Authority::bind(&schema, &policy))?;
    let source="// Generated tables/1 binding. Finite qualification is separate from scalar proofs.\nuse zeno_fcis_synthesis::finite::v2_authority::tables::{Authority,Failure};\npub fn bind() -> Result<Authority,Failure> { Authority::bind(include_bytes!(\"../v2/schema.zcve\"), include_bytes!(\"../v2/policy.zcve\")) }\n".to_owned();
    let source = if ordered {
        source.replacen("Generated tables/1", "Generated tables-ordered/1", 1)
    } else {
        source
    };
    Ok(Compiled {
        descriptor,
        schema,
        policy,
        source,
    })
}
