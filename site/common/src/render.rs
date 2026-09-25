//! The report's values, named as `project.zeno` names them.

use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fmt::Write;
use zeno_fcis_laws::{LawEvaluation, LawManifest, LawStatus};
use zeno_fcis_spec::ProjectSpec;
use zeno_fcis_value::Value;

/// The names the authored project and the law manifest give to what a
/// report shows: reasons, laws, record fields, enumerated variants, and
/// channels.
pub struct Names {
    reasons: BTreeMap<u32, String>,
    laws: BTreeMap<u32, String>,
    fields: BTreeMap<u16, String>,
    /// Keyed by the owning type and the variant, as a value carries them.
    variants: BTreeMap<(u32, u16), String>,
    channels: BTreeMap<u32, String>,
}

impl Names {
    /// Reads the names from the authored project and the law manifest, which
    /// also names the laws that have no formula.
    #[must_use]
    pub fn new(project: &ProjectSpec, manifest: &LawManifest) -> Self {
        let text = |name: &str| name.to_owned();
        Self {
            reasons: project
                .reasons()
                .iter()
                .map(|reason| (reason.id().get(), text(reason.name().as_str())))
                .collect(),
            laws: manifest
                .definitions()
                .iter()
                .map(|law| (law.id().get(), text(law.name().as_str())))
                .collect(),
            fields: project
                .fields()
                .iter()
                .filter_map(|field| {
                    let id = u16::try_from(field.id().get()).ok()?;
                    Some((id, text(field.name().as_str())))
                })
                .collect(),
            variants: project
                .variants()
                .iter()
                .filter_map(|variant| {
                    let id = u16::try_from(variant.id().get()).ok()?;
                    Some(((variant.owner().get(), id), text(variant.name().as_str())))
                })
                .collect(),
            channels: project
                .channels()
                .iter()
                .map(|channel| (channel.id().get(), text(channel.name().as_str())))
                .collect(),
        }
    }

    /// A reason with its name.
    #[must_use]
    pub fn reason(&self, id: u32) -> Json {
        json!({ "id": id, "name": self.reasons.get(&id) })
    }

    /// A channel's name.
    #[must_use]
    pub fn channel(&self, id: u32) -> Option<&str> {
        self.channels.get(&id).map(String::as_str)
    }

    /// Each law's status, as the authority's law evaluation reports it.
    #[must_use]
    pub fn laws(&self, evaluation: &LawEvaluation) -> Json {
        Json::Array(
            evaluation
                .observations()
                .iter()
                .map(|observation| {
                    let id = observation.law_id().get();
                    let status = match observation.status() {
                        LawStatus::Satisfied => "Satisfied",
                        LawStatus::Violated => "Violated",
                        LawStatus::Indeterminate => "Indeterminate",
                    };
                    json!({ "id": id, "name": self.laws.get(&id), "status": status })
                })
                .collect(),
        )
    }

    /// A value as the page shows it: a record as an object keyed by field
    /// name, an enumerated value as its variant's name, an integer as a
    /// number, text as a string.
    #[must_use]
    pub fn render(&self, value: &Value) -> Json {
        match value {
            Value::Unit => Json::Null,
            Value::Bool(flag) => Json::Bool(*flag),
            Value::I128(integer) => number(*integer),
            Value::U128(integer) => u64::try_from(*integer)
                .map_or_else(|_| Json::String(integer.to_string()), Json::from),
            Value::Bytes(bytes) => {
                Json::String(bytes.iter().fold(String::new(), |mut hex, byte| {
                    // Writing into a String cannot fail.
                    let _ = write!(hex, "{byte:02x}");
                    hex
                }))
            }
            Value::Text(text) => Json::String(text.to_string()),
            Value::Enum { type_id, variant }
            | Value::Sum {
                type_id,
                variant,
                payload: None,
            } => self.variant(*type_id, *variant),
            Value::Sum {
                type_id,
                variant,
                payload: Some(payload),
            } => {
                json!({ "variant": self.variant(*type_id, *variant), "payload": self.render(payload) })
            }
            Value::Tuple(items) | Value::Vector(items) => {
                Json::Array(items.iter().map(|item| self.render(item)).collect())
            }
            Value::Record(fields) => Json::Object(
                fields
                    .iter()
                    .map(|field| (self.field(field.id()), self.render(field.value())))
                    .collect(),
            ),
            Value::Map(entries) => Json::Array(
                entries
                    .iter()
                    .map(|entry| json!([self.render(entry.key()), self.render(entry.value())]))
                    .collect(),
            ),
        }
    }

    fn variant(&self, type_id: u32, variant: u16) -> Json {
        self.variants.get(&(type_id, variant)).map_or_else(
            || json!({ "type": type_id, "variant": variant }),
            |name| Json::String(name.clone()),
        )
    }

    fn field(&self, id: u16) -> String {
        self.fields
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("field {id}"))
    }
}

/// An integer as JSON; a value beyond `i64`, which no schema here admits, is
/// rendered as a decimal string rather than lost.
#[must_use]
pub fn number(value: i128) -> Json {
    i64::try_from(value).map_or_else(|_| Json::String(value.to_string()), Json::from)
}
