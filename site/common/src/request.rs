//! The page's request, read field by field.

use serde_json::{Map, Value as Json};
use std::collections::BTreeSet;

/// A request as the page sends it: a JSON object whose `command` names the
/// command and whose other fields are the command's and the context's, under
/// the names the README uses. Each field is read once, and
/// [`finish`](Self::finish) refuses any field left unread, so a request
/// carries exactly what its command and context read.
pub struct Request<'a> {
    fields: &'a Map<String, Json>,
    read: BTreeSet<String>,
}

impl<'a> Request<'a> {
    /// The request's fields, none read yet.
    #[must_use]
    pub fn new(fields: &'a Map<String, Json>) -> Self {
        Self {
            fields,
            read: BTreeSet::new(),
        }
    }

    fn field(&mut self, name: &str) -> Result<&'a Json, String> {
        self.read.insert(name.to_owned());
        self.fields
            .get(name)
            .ok_or_else(|| format!("\"{name}\" is required"))
    }

    /// The field as an integer.
    ///
    /// # Errors
    ///
    /// A missing field, or one that is not an integer within `i64`.
    pub fn integer(&mut self, name: &str) -> Result<i128, String> {
        self.field(name)?
            .as_i64()
            .map(i128::from)
            .ok_or_else(|| format!("\"{name}\" must be an integer"))
    }

    /// The field as `true` or `false`.
    ///
    /// # Errors
    ///
    /// A missing field, or one that is not a boolean.
    pub fn flag(&mut self, name: &str) -> Result<bool, String> {
        self.field(name)?
            .as_bool()
            .ok_or_else(|| format!("\"{name}\" must be true or false"))
    }

    /// The field as one of the named choices: a command name, or an
    /// enumerated value under its variant's name.
    ///
    /// # Errors
    ///
    /// A missing field, or one that is not one of the choices.
    pub fn choice<T: Clone>(&mut self, name: &str, choices: &[(&str, T)]) -> Result<T, String> {
        let text = self.field(name)?.as_str();
        text.and_then(|text| choices.iter().find(|(label, _)| *label == text))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| {
                let names: Vec<&str> = choices.iter().map(|(label, _)| *label).collect();
                format!("\"{name}\" must be one of {}", names.join(", "))
            })
    }

    /// Refuses a field that neither the command nor the context read.
    ///
    /// # Errors
    ///
    /// The first unread field, by name.
    pub fn finish(self) -> Result<(), String> {
        match self.fields.keys().find(|key| !self.read.contains(*key)) {
            Some(extra) => Err(format!("unexpected field {extra:?}")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(value: Json) -> Map<String, Json> {
        match value {
            Json::Object(fields) => fields,
            _ => unreachable!(),
        }
    }

    #[test]
    fn every_field_is_read_once_and_nothing_else_is_allowed() {
        let fields = fields(json!({ "command": "Ship", "quantity": 2, "authorized": true }));
        let mut request = Request::new(&fields);
        assert_eq!(
            request
                .choice("command", &[("Ship", 1), ("Restock", 2)])
                .unwrap(),
            1
        );
        assert_eq!(request.integer("quantity").unwrap(), 2);
        assert!(request.flag("authorized").unwrap());
        request.finish().unwrap();

        let extra = fields.clone();
        let mut request = Request::new(&extra);
        request.integer("quantity").unwrap();
        assert_eq!(
            request.finish().unwrap_err(),
            "unexpected field \"authorized\""
        );
    }

    #[test]
    fn a_missing_or_mistyped_field_is_named() {
        let fields = fields(json!({ "command": "Nope", "quantity": 1.5, "authorized": "yes" }));
        let mut request = Request::new(&fields);
        assert_eq!(
            request
                .choice("command", &[("Ship", 1), ("Restock", 2)])
                .unwrap_err(),
            "\"command\" must be one of Ship, Restock"
        );
        assert_eq!(
            request.integer("quantity").unwrap_err(),
            "\"quantity\" must be an integer"
        );
        assert_eq!(
            request.flag("authorized").unwrap_err(),
            "\"authorized\" must be true or false"
        );
        assert_eq!(request.integer("lane").unwrap_err(), "\"lane\" is required");
    }
}
