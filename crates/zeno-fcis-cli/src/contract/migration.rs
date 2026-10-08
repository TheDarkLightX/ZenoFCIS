//! Migration files, `zeno-fcis/migration/1`, compiled against the two
//! contracts' declarations into the SQLite shell's migration, and the
//! shell's own forward simulation run over them.
//!
//! A migration file names every field of the new state record by its ID and
//! says where its value comes from: `{"from": f}` carries old field `f`
//! over, under its own or a new name; `{"default": v}` gives an added field
//! the value `v`; and `{"from": f, "map": {..}}` takes the value from a
//! table over old field `f`'s values, which is how a field splits into
//! several. Values are written as the rules file writes them: `true` or
//! `false`, an integer, or a variant's ID. A table must list every value of
//! the old field's domain. Compilation checks the file's shape and types;
//! the simulation, which `generate contract` runs again on every
//! generation, decides admission.

use std::collections::BTreeMap;

use zeno_fcis_synthesis::finite::{v2_authority as authority, v2_catalog::BoundCatalog};

use super::ContractError;
use super::declarations::{Declarations, Kind, ROOTS};
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
}

/// A compiled migration: every new state field in ID order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Compiled {
    pub(crate) fields: Vec<(u16, Field)>,
}

impl Compiled {
    /// Runs `use_migration` on the shell's view of this migration.
    pub(crate) fn with_shell<R>(
        &self,
        use_migration: impl FnOnce(&shell::Migration<'_>) -> R,
    ) -> R {
        let targets: Vec<shell::Target<'_>> = self
            .fields
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
                },
            })
            .collect();
        use_migration(&shell::Migration { fields: &targets })
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
}

/// The state record's fields by ID, with their names and types.
fn state_fields(declarations: &Declarations) -> Result<BTreeMap<u16, (&str, u32)>, ContractError> {
    Ok(declarations
        .fields(ROOTS[0].1)?
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
    let (old_fields, new_fields) = (state_fields(old)?, state_fields(new)?);
    let mut given: BTreeMap<u16, &MigrationSource> = BTreeMap::new();
    for (id, source) in &file.state {
        given.insert(*id, source);
    }
    let mut fields = Vec::new();
    let mut carried = std::collections::BTreeSet::new();
    for (id, source) in &given {
        let at = format!("{place} state.{id}");
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
        };
        fields.push((*id, field));
    }
    if let Some((id, (name, _))) = new_fields.iter().find(|(id, _)| !given.contains_key(id)) {
        return Err(ContractError::new(
            format!("{place} state"),
            format!("gives new state field {id} `{name}` no value"),
        ));
    }
    if let Some((id, (name, _))) = old_fields.iter().find(|(id, _)| !carried.contains(id)) {
        return Err(ContractError::new(
            format!("{place} state"),
            format!(
                "does not carry old state field {id} `{name}` into the new state; a migration renames, adds and splits fields, and drops none"
            ),
        ));
    }
    Ok(Compiled { fields })
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
        .with_shell(|migration| shell::simulate(from, &old, to, &new, migration))
        .map_err(|unsimulated| ContractError::new(place, describe(unsimulated, from, names)))
}
