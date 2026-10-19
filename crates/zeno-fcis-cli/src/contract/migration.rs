//! Migration files, `zeno-fcis/migration/1` and `/2`, compiled against the two
//! contracts' declarations into the SQLite shell's migration, and the
//! shell's own forward simulation run over them.
//!
//! A migration file names every field of the new state record by its ID and
//! says where its value comes from: `{"from": f}` carries old field `f`
//! over, under its own or a new name; `{"default": v}` gives an added field
//! the value `v`; and `{"from": f, "map": {..}}` takes the value from a
//! table over old field `f`'s values, which is how a field splits into
//! several. Version 2 adds closed library programs and complete, lossless
//! per-channel payload representation maps. Values are written as the rules file writes them: `true` or
//! `false`, an integer, or a variant's ID. A table must list every value of
//! the old field's domain. Compilation checks the file's shape and types;
//! the simulation, which `generate contract` runs again on every
//! generation, decides admission.

use std::collections::BTreeMap;

use zeno_fcis_synthesis::finite::{
    Domain as ScalarDomain, Op, Program, v2_authority as authority, v2_catalog::BoundCatalog,
};

use super::ContractError;
use super::declarations::{Declarations, Input, Kind, ROOTS, Source};
use super::expr::Ast;
use super::graph::{Graph, Kind as GraphKind, ScalarGraph};
use super::layout::Syntax;
use super::rules::{MigrationFile, MigrationSource, MigrationValue};
use crate::shell_v2::migration::{self as shell, Observation, Unsimulated};

/// Where a new state field takes its value from, compiled to numbers: 0 or 1
/// for a Boolean, the integer, a variant's ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Field {
    From(u16),
    Value(i128),
    Table(u16, Vec<(i128, i128)>),
    Expression(Vec<u16>, Program),
}

/// A compiled migration: every new state field in ID order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Compiled {
    pub(crate) format: u8,
    pub(crate) fields: Vec<(u16, Field)>,
    pub(crate) dropped: Vec<u16>,
    pub(crate) payloads: Vec<(u32, Vec<(u16, Field)>)>,
}

impl Compiled {
    /// Runs `use_migration` on the shell's view of this migration.
    pub(crate) fn with_shell<R>(
        &self,
        use_migration: impl FnOnce(&shell::Migration<'_>) -> R,
    ) -> R {
        Self::with_targets(&self.fields, |targets| {
            use_migration(&shell::Migration { fields: targets })
        })
    }

    fn with_targets<'a, R>(
        fields: &'a [(u16, Field)],
        use_targets: impl FnOnce(&[shell::Target<'a>]) -> R,
    ) -> R {
        let targets: Vec<shell::Target<'_>> = fields
            .iter()
            .map(|(id, field)| shell::Target {
                id: *id,
                source: match field {
                    Field::From(field) => shell::Source::Field(*field),
                    Field::Value(value) => shell::Source::Value(*value),
                    Field::Table(field, cases) => shell::Source::Table {
                        field: *field,
                        cases,
                    },
                    Field::Expression(fields, program) => {
                        shell::Source::Program { fields, program }
                    }
                },
            })
            .collect();
        use_targets(&targets)
    }

    /// Borrows the same compiled programs into the pure shell simulation.
    pub(crate) fn with_payloads<R>(
        &self,
        use_plan: impl FnOnce(&shell::Migration<'_>, &[shell::Payload<'_>]) -> R,
    ) -> R {
        let tables = self.payload_targets();
        let payloads: Vec<shell::Payload<'_>> = tables
            .iter()
            .map(|(channel, fields)| shell::Payload {
                channel: *channel,
                fields,
            })
            .collect();
        self.with_shell(|migration| use_plan(migration, &payloads))
    }

    pub(crate) fn payload_targets(&self) -> Vec<(u32, Vec<shell::Target<'_>>)> {
        self.payloads
            .iter()
            .map(|(channel, fields)| {
                (
                    *channel,
                    Self::with_targets(fields, |targets| targets.to_vec()),
                )
            })
            .collect()
    }

    pub(super) fn extended(&self) -> bool {
        self.format == 2
    }

    /// The migration as the generated contract declares it: each field's ID,
    /// its source's tag (0 an old field, 1 a value, 2 a table), the old field
    /// or the value, and the table.
    pub(super) fn syntax(&self) -> Syntax {
        Syntax::slice(
            self.fields
                .iter()
                .map(|(id, field)| {
                    let (tag, operand, table): (u8, i128, &[(i128, i128)]) = match field {
                        Field::From(field) => (0, i128::from(*field), &[]),
                        Field::Value(value) => (1, *value, &[]),
                        Field::Table(field, cases) => (2, i128::from(*field), cases),
                        Field::Expression(..) => (3, 0, &[]),
                    };
                    Syntax::tuple(vec![
                        Syntax::literal(id),
                        Syntax::literal(tag),
                        Syntax::literal(operand),
                        Syntax::slice(
                            table
                                .iter()
                                .map(|(old, new)| {
                                    Syntax::tuple(vec![Syntax::literal(old), Syntax::literal(new)])
                                })
                                .collect(),
                        ),
                    ])
                })
                .collect(),
        )
    }

    /// Explicit v2 metadata, leaving legacy STATE_STEPS unchanged.
    pub(super) fn record_syntax(&self, version: u32) -> Syntax {
        Syntax::tuple(vec![
            Syntax::literal(version),
            fields_syntax(&self.fields),
            Syntax::slice(
                self.payloads
                    .iter()
                    .map(|(channel, fields)| {
                        Syntax::tuple(vec![Syntax::literal(channel), fields_syntax(fields)])
                    })
                    .collect(),
            ),
        ])
    }
}

/// The state record's fields by ID, with their names and types.
fn record_fields(
    declarations: &Declarations,
    root: u32,
) -> Result<BTreeMap<u16, (&str, u32)>, ContractError> {
    Ok(declarations
        .fields(root)?
        .iter()
        .map(|field| (field.id, (field.name.as_str(), field.type_id)))
        .collect())
}

/// Every value of a scalar kind as a number, when there are at most `limit`.
fn values(kind: Kind<'_>, limit: u128) -> Option<Vec<i128>> {
    match kind {
        Kind::Bool => Some(vec![0, 1]),
        Kind::I128 { min, max } => {
            let width = u128::try_from(max.checked_sub(min)?).ok()?;
            (width < limit).then(|| (min..=max).collect())
        }
        Kind::Sum(variants) => Some(
            variants
                .iter()
                .map(|variant| i128::from(variant.id))
                .collect(),
        ),
        Kind::Text => None,
    }
}

/// The number a migration value stands for in a field of `kind`.
fn number(value: MigrationValue, kind: Kind<'_>) -> Result<i128, String> {
    match (value, kind) {
        (MigrationValue::Bool(value), Kind::Bool) => Ok(i128::from(value)),
        (MigrationValue::Int(value), Kind::I128 { min, max }) if (min..=max).contains(&value) => {
            Ok(value)
        }
        (MigrationValue::Int(value), Kind::I128 { min, max }) => Err(format!(
            "{value} is outside the field's range {min}..={max}"
        )),
        (MigrationValue::Int(value), Kind::Sum(variants))
            if variants
                .iter()
                .any(|variant| i128::from(variant.id) == value) =>
        {
            Ok(value)
        }
        (MigrationValue::Int(value), Kind::Sum(_)) => {
            Err(format!("{value} is not a variant ID of the field's type"))
        }
        (_, Kind::Bool) => Err("the field is a Boolean: write true or false".to_owned()),
        (_, Kind::Text) => Err("a text field has no finite domain and is not migrated".to_owned()),
        (_, _) => Err("the field is not a Boolean: write an integer".to_owned()),
    }
}

/// The number a table key stands for in an old field of `kind`.
fn key(text: &str, kind: Kind<'_>) -> Result<i128, String> {
    let value = match (text, kind) {
        ("false", Kind::Bool) => MigrationValue::Bool(false),
        ("true", Kind::Bool) => MigrationValue::Bool(true),
        (text, _) => MigrationValue::Int(
            text.parse::<i128>()
                .map_err(|_| format!("key `{text}` is not true, false or a decimal integer"))?,
        ),
    };
    number(value, kind).map_err(|reason| format!("key `{text}`: {reason}"))
}

/// Compiles `file` from the state of `old` to the state of `new`; `place`
/// names the file in refusals.
///
/// # Errors
/// A new field the file does not give a value, or one the new state does
/// not have; an old field that does not exist, or that no new field
/// carries; a value or table key outside its field's type; a table that
/// misses an old value; or a field kind that changes, such as a Boolean
/// carried into an integer field.
pub(super) fn compile(
    file: &MigrationFile,
    old: &Declarations,
    new: &Declarations,
    place: &str,
) -> Result<Compiled, ContractError> {
    let fields = compile_fields(
        &file.state,
        &file.dropped,
        old,
        ROOTS[0].1,
        new,
        ROOTS[0].1,
        &format!("{place} state"),
    )?;
    let mut payloads = BTreeMap::new();
    for (channel, fields) in &file.payloads {
        let at = format!("{place} payloads.{channel}");
        let old_channel = old.channel(*channel)?;
        let new_channel = new.channel(*channel)?;
        let compiled = compile_fields(
            fields,
            &[],
            old,
            old_channel.payload,
            new,
            new_channel.payload,
            &at,
        )?;
        if payloads.insert(*channel, compiled).is_some() {
            return Err(ContractError::new(at, "lists the channel more than once"));
        }
    }
    Ok(Compiled {
        format: file.format,
        fields,
        dropped: file.dropped.clone(),
        payloads: payloads.into_iter().collect(),
    })
}

fn compile_fields(
    sources: &[(u16, MigrationSource)],
    dropped: &[u16],
    old: &Declarations,
    old_root: u32,
    new: &Declarations,
    new_root: u32,
    place: &str,
) -> Result<Vec<(u16, Field)>, ContractError> {
    let (old_fields, new_fields) = (record_fields(old, old_root)?, record_fields(new, new_root)?);
    for id in dropped {
        if !old_fields.contains_key(id) {
            return Err(ContractError::new(
                place,
                format!("dropped old state field {id} does not exist"),
            ));
        }
    }
    let mut given: BTreeMap<u16, &MigrationSource> = BTreeMap::new();
    for (id, source) in sources {
        if given.insert(*id, source).is_some() {
            return Err(ContractError::new(
                place,
                format!("lists field {id} more than once"),
            ));
        }
    }
    let mut fields = Vec::new();
    let mut carried = std::collections::BTreeSet::new();
    for (id, source) in &given {
        let at = format!("{place}.{id}");
        let refuse = |reason: String| ContractError::new(&at, reason);
        let Some((name, type_id)) = new_fields.get(id) else {
            return Err(refuse("is not a field of the new state".to_owned()));
        };
        let kind = new.kind(*type_id)?;
        let old_field = |field: u16| {
            old_fields
                .get(&field)
                .map(|(_, type_id)| *type_id)
                .ok_or_else(|| refuse(format!("old state field {field} does not exist")))
        };
        let field = match source {
            MigrationSource::From(field) => {
                let old_kind = old.kind(old_field(*field)?)?;
                if std::mem::discriminant(&old_kind) != std::mem::discriminant(&kind) {
                    return Err(refuse(format!(
                        "carries old field {field} into `{name}`, a field of another kind; use a map"
                    )));
                }
                carried.insert(*field);
                Field::From(*field)
            }
            MigrationSource::Default(value) => Field::Value(
                number(*value, kind).map_err(|reason| refuse(format!("default: {reason}")))?,
            ),
            MigrationSource::Map(field, cases) => {
                let old_kind = old.kind(old_field(*field)?)?;
                let mut table = BTreeMap::new();
                for (text, value) in cases {
                    let old_value = key(text, old_kind).map_err(&refuse)?;
                    let new_value = number(*value, kind)
                        .map_err(|reason| refuse(format!("map `{text}`: {reason}")))?;
                    if table.insert(old_value, new_value).is_some() {
                        return Err(refuse(format!("map lists old value {old_value} twice")));
                    }
                }
                let domain = values(old_kind, 1 << 16).ok_or_else(|| {
                    refuse(format!(
                        "old field {field} has more than 65536 values, too many for a map"
                    ))
                })?;
                if let Some(missing) = domain.iter().find(|value| !table.contains_key(value)) {
                    return Err(refuse(format!(
                        "map gives no value for old value {missing} of field {field}"
                    )));
                }
                if table.len() != domain.len() {
                    return Err(refuse(format!(
                        "map lists a value outside old field {field}'s domain"
                    )));
                }
                carried.insert(*field);
                Field::Table(*field, table.into_iter().collect())
            }
            MigrationSource::Expression(ast) => {
                let (inputs, program) = expression(ast, old, &old_fields, kind)
                    .map_err(|reason| refuse(format!("expr: {reason}")))?;
                carried.extend(inputs.iter().copied());
                Field::Expression(inputs, program)
            }
        };
        fields.push((*id, field));
    }
    if let Some((id, (name, _))) = new_fields.iter().find(|(id, _)| !given.contains_key(id)) {
        return Err(ContractError::new(
            place,
            format!("gives new state field {id} `{name}` no value"),
        ));
    }
    if let Some(id) = dropped.iter().find(|id| carried.contains(id)) {
        return Err(ContractError::new(
            place,
            format!("declares dropped field {id}, but a new field still uses it"),
        ));
    }
    if let Some((id, (name, _))) = old_fields
        .iter()
        .find(|(id, _)| !carried.contains(id) && !dropped.contains(id))
    {
        return Err(ContractError::new(
            place,
            format!(
                "does not carry old state field {id} `{name}` into the new state; list it in `drop` in a version 2 state migration; a payload map must carry every old field"
            ),
        ));
    }
    Ok(fields)
}

fn expression(
    ast: &Ast,
    old: &Declarations,
    fields: &BTreeMap<u16, (&str, u32)>,
    target: Kind<'_>,
) -> Result<(Vec<u16>, Program), String> {
    fn names(ast: &Ast, found: &mut std::collections::BTreeSet<String>) {
        match ast {
            Ast::Name(name) => {
                found.insert(name.clone());
            }
            Ast::Not(inner) | Ast::Neg(inner) => names(inner, found),
            Ast::Binary(_, left, right) | Ast::Div(_, left, right) => {
                names(left, found);
                names(right, found);
            }
            Ast::Choose(condition, then, otherwise) => {
                names(condition, found);
                names(then, found);
                names(otherwise, found);
            }
            Ast::MulDiv(_, a, b, d)
            | Ast::SignedMulDivFloor(a, b, d)
            | Ast::ScaledMulDiv(_, _, a, b, d, _)
            | Ast::RepeatMulDiv(_, a, b, d, _) => {
                names(a, found);
                names(b, found);
                names(d, found);
            }
            Ast::ProductsGe(arguments) => {
                for a in arguments.iter() {
                    names(a, found);
                }
            }
            Ast::Bool(_) | Ast::Int(_) | Ast::DecisionReason => {}
        }
    }
    let mut found = std::collections::BTreeSet::new();
    names(ast, &mut found);
    let mut ids = std::collections::BTreeSet::new();
    for name in found {
        let id = name
            .strip_prefix("old.")
            .and_then(|id| id.parse::<u16>().ok())
            .ok_or_else(|| format!("`{name}` is not old.<field ID>"))?;
        if !fields.contains_key(&id) || name != format!("old.{id}") {
            return Err(format!("`{name}` is not a canonical old record field"));
        }
        ids.insert(id);
    }
    let ids: Vec<u16> = ids.into_iter().collect();
    let inputs: Vec<Input> = ids
        .iter()
        .map(|id| Input {
            name: format!("old.{id}"),
            source: Source::State,
            field: Some(*id),
            type_id: fields[id].1,
        })
        .collect();
    let domains = inputs
        .iter()
        .map(|input| scalar_domain(old.kind(input.type_id).map_err(|error| error.to_string())?))
        .collect::<Result<Vec<_>, _>>()?;
    let output = scalar_domain(target)?;
    let mut graph = ScalarGraph::new(old, &inputs);
    let root = graph.compile(ast)?;
    let root = if matches!(target, Kind::Bool) {
        if root.kind != GraphKind::Bool {
            return Err("a Boolean target requires a Boolean expression".to_owned());
        }
        root
    } else {
        graph.numeric(root)?
    };
    let nodes = graph
        .table
        .nodes
        .iter()
        .map(super::policy::scalar_op)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if nodes.iter().any(Op::requires_arithmetic_profile) {
        return Err("arithmetic extension is outside the closed migration profile".to_owned());
    }
    let root =
        u16::try_from(root.index).map_err(|_| "the expression has too many nodes".to_owned())?;
    let program = Program::try_new_extended(domains, vec![output], nodes, vec![root])
        .map_err(|error| format!("the library refuses the closed expression: {error}"))?;
    Ok((ids, program))
}

fn scalar_domain(kind: Kind<'_>) -> Result<ScalarDomain, String> {
    let (min, max) = match kind {
        Kind::Bool => return Ok(ScalarDomain::Bool),
        Kind::I128 { min, max } => (min, max),
        Kind::Sum(variants) => (
            i128::from(
                variants
                    .iter()
                    .map(|variant| variant.id)
                    .min()
                    .ok_or("empty variant domain")?,
            ),
            i128::from(
                variants
                    .iter()
                    .map(|variant| variant.id)
                    .max()
                    .ok_or("empty variant domain")?,
            ),
        ),
        Kind::Text => {
            return Err("text has no supported finite migration expression domain".to_owned());
        }
    };
    Ok(ScalarDomain::Int { min, max })
}

fn fields_syntax(fields: &[(u16, Field)]) -> Syntax {
    let domain = |domain: ScalarDomain| match domain {
        ScalarDomain::Bool => Syntax::path("ScalarDomain::Bool"),
        ScalarDomain::Int { min, max } => Syntax::record(
            "ScalarDomain::Int",
            vec![("min", Syntax::literal(min)), ("max", Syntax::literal(max))],
        ),
        _ => Syntax::path("unsupported_migration_domain"),
    };
    let op = |op: &Op| {
        let call = |name: &str, values: &[u16]| {
            Syntax::call(name, values.iter().map(Syntax::literal).collect())
        };
        match *op {
            Op::Input(value) => call("Op::Input", &[value]),
            Op::Int(value) => Syntax::call("Op::Int", vec![Syntax::literal(value)]),
            Op::Bool(value) => Syntax::call("Op::Bool", vec![Syntax::literal(value)]),
            Op::Add(a, b) => call("Op::Add", &[a, b]),
            Op::Sub(a, b) => call("Op::Sub", &[a, b]),
            Op::Eq(a, b) => call("Op::Eq", &[a, b]),
            Op::Lt(a, b) => call("Op::Lt", &[a, b]),
            Op::And(a, b) => call("Op::And", &[a, b]),
            Op::Not(a) => call("Op::Not", &[a]),
            Op::Select(c, a, b) => call("Op::Select", &[c, a, b]),
            _ => Syntax::path("unsupported_migration_instruction"),
        }
    };
    Syntax::slice(
        fields
            .iter()
            .map(|(id, field)| {
                let (tag, operand, table): (u8, i128, &[(i128, i128)]) = match field {
                    Field::From(field) => (0, i128::from(*field), &[]),
                    Field::Value(value) => (1, *value, &[]),
                    Field::Table(field, cases) => (2, i128::from(*field), cases),
                    Field::Expression(..) => (3, 0, &[]),
                };
                let program = match field {
                    Field::Expression(fields, program) => Syntax::call(
                        "Some",
                        vec![Syntax::tuple(vec![
                            Syntax::slice(fields.iter().map(Syntax::literal).collect()),
                            Syntax::slice(program.inputs().iter().copied().map(domain).collect()),
                            domain(program.outputs()[0]),
                            Syntax::slice(program.nodes().iter().map(op).collect()),
                            Syntax::literal(program.roots()[0]),
                        ])],
                    ),
                    _ => Syntax::path("None"),
                };
                Syntax::tuple(vec![
                    Syntax::literal(id),
                    Syntax::literal(tag),
                    Syntax::literal(operand),
                    Syntax::slice(
                        table
                            .iter()
                            .map(|(old, new)| {
                                Syntax::tuple(vec![Syntax::literal(old), Syntax::literal(new)])
                            })
                            .collect(),
                    ),
                    program,
                ])
            })
            .collect(),
    )
}

/// The names of each root's fields, for reports.
pub(super) fn field_names(declarations: &Declarations) -> [BTreeMap<u16, String>; 3] {
    ROOTS.map(|(_, root)| {
        declarations
            .fields(root)
            .map(|fields| {
                fields
                    .iter()
                    .map(|field| (field.id, field.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// A tuple's values as a report names them: `state {status 151, tier 3},
/// command {..}, context {..}`.
fn tuple(from: &BoundCatalog<'_>, ordinal: u64, names: &[BTreeMap<u16, String>; 3]) -> String {
    let Some(inputs) = shell::inputs_at(from, ordinal) else {
        return format!("input tuple {ordinal}");
    };
    let parts: Vec<String> = ["state", "command", "context"]
        .iter()
        .zip(&inputs)
        .zip(names)
        .map(|((root, values), names)| {
            let values: Vec<String> = values
                .iter()
                .map(|(id, value)| match names.get(id) {
                    Some(name) => format!("{name} {value}"),
                    None => format!("{id} {value}"),
                })
                .collect();
            format!("{root} {{{}}}", values.join(", "))
        })
        .collect();
    parts.join(", ")
}

/// A refusal of the simulation, in words, with the input it happened at.
pub(super) fn describe(
    unsimulated: Unsimulated,
    from: &BoundCatalog<'_>,
    names: &[BTreeMap<u16, String>; 3],
) -> String {
    match unsimulated {
        Unsimulated::Differs {
            observation: Observation::Genesis,
            ordinal,
        } => format!(
            "the migration does not map the old genesis state to the new one: the old contract's genesis state ({}) maps to a state the new contract's genesis laws refuse",
            tuple(from, ordinal, names)
        ),
        Unsimulated::Differs {
            observation: Observation::StateLaws,
            ordinal,
        } => format!(
            "the migration breaks a state law of the new contract: the old state in the input {} satisfies every state law of the old contract, but a state law of the new contract does not hold on the state the migration maps it to, so a store holding it could not continue under the new contract; change the migration or the new contract's laws",
            tuple(from, ordinal, names)
        ),
        Unsimulated::Unframed { ordinal } => format!(
            "the migrated state of the old state ({}) has no encoding under the new contract's state framing, for instance because it exceeds the state root's byte bound",
            tuple(from, ordinal, names)
        ),
        Unsimulated::Differs {
            observation,
            ordinal,
        } => format!(
            "the observation `{}` differs at {}: the new contract on the migrated state does not do what the old contract did, so this is a behaviour change, not a migration; take it with `contract evolve` without --migration if the layout is unchanged, or change the migration or the new contract",
            observation.tag(),
            tuple(from, ordinal, names)
        ),
        Unsimulated::Value { field, ordinal } => format!(
            "the migration gives new state field {field} a value outside its domain at {}",
            tuple(from, ordinal, names)
        ),
        Unsimulated::DomainTooLarge { .. } => format!(
            "{unsimulated}; solver evidence is not accepted for a migration, so shrink the declared domains or take the change another way"
        ),
        other => other.to_string(),
    }
}

/// Runs the shell's forward simulation of `migration` from `from` to `to`.
///
/// # Errors
/// The library's refusal to bind either catalog, or the simulation's
/// refusal, in words.
pub(super) fn simulate(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    migration: &Compiled,
    names: &[BTreeMap<u16, String>; 3],
    place: &str,
) -> Result<shell::Simulated, ContractError> {
    let bind = |catalog| {
        authority::bind(catalog).map_err(|refusal| {
            ContractError::new(
                place,
                format!("the library refuses to bind a contract: {refusal:?}"),
            )
        })
    };
    let (old, new) = (bind(from)?, bind(to)?);
    migration
        .with_payloads(|migration, payloads| {
            shell::simulate_with_payloads(from, &old, to, &new, migration, payloads)
        })
        .map_err(|unsimulated| ContractError::new(place, describe(unsimulated, from, names)))
}
