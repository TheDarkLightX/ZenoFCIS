//! The reviewed rules file, `v2/policy.json`: leaf bindings, variables, the
//! ordered decision cases, the genesis state and the kind of every law.
//!
//! Unknown and duplicate keys are refused, so no entry is silently ignored
//! or overridden.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use super::ContractError;
use super::declarations::Leaf;
use super::expr::{self, Ast};

/// The rules file format this generator reads.
pub(super) const RULES_SCHEMA: &str = "zeno-fcis/template-declarative-policy/2";

/// Framework law IDs used when the rules file names none.
const FAILURE_LAW: u32 = 908;
const REJECT_LAW: u32 = 909;

/// Decision class of a case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Class {
    Accept,
    Reject,
    CommittedFailure,
}

impl Class {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Reject => "Reject",
            Self::CommittedFailure => "CommittedFailure",
        }
    }

    /// The value the `Class` law observation reads.
    pub(super) fn code(self) -> i128 {
        match self {
            Self::Accept => 0,
            Self::Reject => 1,
            Self::CommittedFailure => 2,
        }
    }
}

/// Law kinds the library defines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LawKind {
    StateInvariant,
    AssetConservation,
    MintBurnAuthorization,
    DebitCreditEffectEquality,
    FeeAndRounding,
    AuthoritySubjectRecipient,
    RejectNoAuthority,
    CommittedFailureEffects,
    DecisionConformance,
    InitialCondition,
}

impl LawKind {
    const ALL: [Self; 10] = [
        Self::StateInvariant,
        Self::AssetConservation,
        Self::MintBurnAuthorization,
        Self::DebitCreditEffectEquality,
        Self::FeeAndRounding,
        Self::AuthoritySubjectRecipient,
        Self::RejectNoAuthority,
        Self::CommittedFailureEffects,
        Self::DecisionConformance,
        Self::InitialCondition,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::StateInvariant => "StateInvariant",
            Self::AssetConservation => "AssetConservation",
            Self::MintBurnAuthorization => "MintBurnAuthorization",
            Self::DebitCreditEffectEquality => "DebitCreditEffectEquality",
            Self::FeeAndRounding => "FeeAndRounding",
            Self::AuthoritySubjectRecipient => "AuthoritySubjectRecipient",
            Self::RejectNoAuthority => "RejectNoAuthority",
            Self::CommittedFailureEffects => "CommittedFailureEffects",
            Self::DecisionConformance => "DecisionConformance",
            Self::InitialCondition => "InitialCondition",
        }
    }
}

/// A genesis value: a JSON boolean or integer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Constant {
    Bool(bool),
    Int(i128),
}

#[derive(Debug)]
pub(super) struct Case {
    pub(super) when: Ast,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// Successor value of every state field, by field ID; empty for rejects.
    pub(super) post: BTreeMap<u16, Ast>,
    pub(super) outbox: Vec<Delivery>,
}

#[derive(Debug)]
pub(super) struct Delivery {
    pub(super) ordinal: u32,
    pub(super) channel: u32,
    pub(super) destination: String,
    pub(super) payload: BTreeMap<u16, Ast>,
    pub(super) idempotency: u128,
}

#[derive(Debug)]
pub(super) struct Rules {
    pub(super) template: String,
    pub(super) leaf_bindings: BTreeMap<u32, Leaf>,
    pub(super) variables: BTreeMap<String, Ast>,
    pub(super) cases: Vec<Case>,
    pub(super) genesis: BTreeMap<u16, Constant>,
    pub(super) law_kinds: BTreeMap<u32, LawKind>,
    pub(super) failure_law: u32,
    pub(super) reject_law: u32,
}

impl Rules {
    pub(super) fn read(source: &str) -> Result<Self, ContractError> {
        let json: Json = serde_json::from_str(source)
            .map_err(|error| ContractError::new(FILE, error.to_string()))?;
        let file = Object::new(&json, FILE)?;
        file.only(&[
            "schema",
            "template",
            "roots",
            "leaf_bindings",
            "variables",
            "cases",
            "genesis",
            "law_kinds",
            "framework_failure_law",
            "framework_reject_law",
            // Reviewer notes; they do not affect the contract.
            "idempotency",
            "original_domain",
        ])?;
        let schema = file.text("schema")?;
        if schema != RULES_SCHEMA {
            return Err(file.error(
                "schema",
                format!("expected `{RULES_SCHEMA}`, found `{schema}`"),
            ));
        }
        let template = file.text("template")?;
        if template.is_empty() {
            return Err(file.error("template", "must name the application"));
        }
        for note in ["idempotency", "original_domain"] {
            if file.optional(note).is_some() {
                file.text(note)?;
            }
        }
        let roots = file.object("roots")?;
        roots.only(&["state", "command", "context"])?;
        if (
            roots.number::<u32>("state")?,
            roots.number::<u32>("command")?,
            roots.number::<u32>("context")?,
        ) != (100, 101, 102)
        {
            return Err(file.error(
                "roots",
                "state, command and context must be types 100, 101 and 102",
            ));
        }
        let bindings = file.object("leaf_bindings")?;
        let leaf_bindings = bindings
            .entries()
            .map(|(key, value)| {
                let place = bindings.place_of(key);
                Ok((id(key, &place)?, leaf(value, &place)?))
            })
            .collect::<Result<_, ContractError>>()?;
        let declared = file.object("variables")?;
        let variables = declared
            .entries()
            .map(|(name, value)| {
                let place = declared.place_of(name);
                let root = name.split('.').next().unwrap_or("");
                if matches!(root, "pre" | "post" | "command" | "context") {
                    return Err(ContractError::new(
                        place,
                        "variable names may not start with a root",
                    ));
                }
                Ok((name.to_owned(), expression(value, &place)?))
            })
            .collect::<Result<_, ContractError>>()?;
        let cases = file
            .array("cases")?
            .iter()
            .enumerate()
            .map(|(index, case)| self::case(case, &format!("{FILE} cases[{index}]")))
            .collect::<Result<Vec<_>, ContractError>>()?;
        if cases.is_empty() {
            return Err(file.error("cases", "needs at least one case"));
        }
        let initial = file.object("genesis")?;
        let genesis = initial
            .entries()
            .map(|(key, value)| {
                let place = initial.place_of(key);
                let constant = match value {
                    Json::Bool(value) => Constant::Bool(*value),
                    Json::Int(value) => Constant::Int(*value),
                    _ => {
                        return Err(ContractError::new(
                            place,
                            "must be a JSON boolean or integer",
                        ));
                    }
                };
                Ok((id(key, &place)?, constant))
            })
            .collect::<Result<_, ContractError>>()?;
        let kinds = file.object("law_kinds")?;
        let law_kinds = kinds
            .entries()
            .map(|(key, value)| {
                let place = kinds.place_of(key);
                let name = text(value, &place)?;
                let kind = LawKind::ALL
                    .into_iter()
                    .find(|kind| kind.name() == name)
                    .ok_or_else(|| {
                        ContractError::new(&place, format!("unknown law kind `{name}`"))
                    })?;
                Ok((id(key, &place)?, kind))
            })
            .collect::<Result<_, ContractError>>()?;
        let law = |key: &str, default: u32| match file.optional(key) {
            Some(_) => file.number(key),
            None => Ok(default),
        };
        Ok(Self {
            template: template.to_owned(),
            leaf_bindings,
            variables,
            cases,
            genesis,
            law_kinds,
            failure_law: law("framework_failure_law", FAILURE_LAW)?,
            reject_law: law("framework_reject_law", REJECT_LAW)?,
        })
    }
}

const FILE: &str = "v2/policy.json";

fn case(json: &Json, place: &str) -> Result<Case, ContractError> {
    let case = Object::new(json, place)?;
    // `rule` names the rule for reviewers; it does not affect the contract.
    case.only(&["when", "class", "reason", "post", "outbox", "rule"])?;
    if case.optional("rule").is_some() {
        case.text("rule")?;
    }
    let class = match case.text("class")? {
        "Accept" => Class::Accept,
        "Reject" => Class::Reject,
        "CommittedFailure" => Class::CommittedFailure,
        other => {
            return Err(case.error(
                "class",
                format!("`{other}` is not Accept, Reject or CommittedFailure"),
            ));
        }
    };
    let reason = match case.field("reason")? {
        Json::Null => None,
        _ => Some(case.number("reason")?),
    };
    let outbox = case
        .array("outbox")?
        .iter()
        .enumerate()
        .map(|(index, delivery)| {
            let delivery = Object::new(delivery, &format!("{place}.outbox[{index}]"))?;
            delivery.only(&[
                "ordinal",
                "channel",
                "destination",
                "payload",
                "idempotency_ordinal",
            ])?;
            Ok(Delivery {
                ordinal: delivery.number("ordinal")?,
                channel: delivery.number("channel")?,
                destination: delivery.text("destination")?.to_owned(),
                payload: fields(&delivery.object("payload")?)?,
                idempotency: delivery.number("idempotency_ordinal")?,
            })
        })
        .collect::<Result<_, ContractError>>()?;
    Ok(Case {
        when: expression(case.field("when")?, &case.place_of("when"))?,
        class,
        reason,
        post: fields(&case.object("post")?)?,
        outbox,
    })
}

/// A JSON boolean or integer is a constant; text is a rule expression.
fn expression(value: &Json, place: &str) -> Result<Ast, ContractError> {
    match value {
        Json::Bool(value) => Ok(Ast::Bool(*value)),
        Json::Int(value) => Ok(Ast::Int(*value)),
        Json::Text(text) => expr::parse(text).map_err(|reason| ContractError::new(place, reason)),
        _ => Err(ContractError::new(
            place,
            "must be a boolean, an integer or an expression",
        )),
    }
}

/// Values by field ID.
fn fields(object: &Object<'_>) -> Result<BTreeMap<u16, Ast>, ContractError> {
    object
        .entries()
        .map(|(key, value)| {
            let place = object.place_of(key);
            Ok((id(key, &place)?, expression(value, &place)?))
        })
        .collect()
}

/// A canonical decimal ID key: digits only, no sign or leading zero.
fn id<T: TryFrom<i128>>(key: &str, place: &str) -> Result<T, ContractError> {
    let canonical = !key.is_empty()
        && key.bytes().all(|byte| byte.is_ascii_digit())
        && (key == "0" || !key.starts_with('0'));
    canonical
        .then(|| key.parse::<i128>().ok())
        .flatten()
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| ContractError::new(place, format!("`{key}` is not a decimal ID")))
}

fn text<'j>(value: &'j Json, place: &str) -> Result<&'j str, ContractError> {
    match value {
        Json::Text(text) => Ok(text),
        _ => Err(ContractError::new(place, "must be a string")),
    }
}

fn leaf(value: &Json, place: &str) -> Result<Leaf, ContractError> {
    let items = match value {
        Json::Array(items) => items.as_slice(),
        _ => &[],
    };
    let bound = |index: usize| match items.get(index) {
        Some(Json::Int(value)) => Some(*value),
        _ => None,
    };
    let leaf = match (items.first(), items.len()) {
        (Some(Json::Text(kind)), 1) if kind == "Bool" => Some(Leaf::Bool),
        (Some(Json::Text(kind)), 3) if kind == "I128" => bound(1)
            .zip(bound(2))
            .filter(|(min, max)| min <= max)
            .map(|(min, max)| Leaf::I128 { min, max }),
        (Some(Json::Text(kind)), 3) if kind == "Text" => bound(1)
            .zip(bound(2))
            .and_then(|(min, max)| u32::try_from(min).ok().zip(u32::try_from(max).ok()))
            .filter(|(min, max)| min <= max)
            .map(|(min, max)| Leaf::Text { min, max }),
        _ => None,
    };
    leaf.ok_or_else(|| {
        ContractError::new(
            place,
            r#"expected ["Bool"], ["I128", min, max] or ["Text", min, max] with min <= max"#,
        )
    })
}

/// One JSON object of the rules file, with the place it was read from.
struct Object<'j> {
    place: String,
    entries: &'j [(String, Json)],
}

impl<'j> Object<'j> {
    fn new(json: &'j Json, place: &str) -> Result<Self, ContractError> {
        match json {
            Json::Object(entries) => Ok(Self {
                place: place.to_owned(),
                entries,
            }),
            _ => Err(ContractError::new(place, "must be an object")),
        }
    }

    /// `v2/policy.json cases`, then `v2/policy.json cases[0].post`.
    fn place_of(&self, key: &str) -> String {
        let separator = if self.place == FILE { ' ' } else { '.' };
        format!("{}{separator}{key}", self.place)
    }

    fn error(&self, key: &str, reason: impl Into<String>) -> ContractError {
        ContractError::new(self.place_of(key), reason)
    }

    /// Refuses keys outside `allowed`.
    fn only(&self, allowed: &[&str]) -> Result<(), ContractError> {
        match self
            .entries
            .iter()
            .find(|(key, _)| !allowed.contains(&key.as_str()))
        {
            Some((key, _)) => Err(ContractError::new(
                &self.place,
                format!("unknown key `{key}`"),
            )),
            None => Ok(()),
        }
    }

    fn entries(&self) -> impl Iterator<Item = (&'j str, &'j Json)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    fn optional(&self, key: &str) -> Option<&'j Json> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    fn field(&self, key: &str) -> Result<&'j Json, ContractError> {
        self.optional(key)
            .ok_or_else(|| ContractError::new(&self.place, format!("missing key `{key}`")))
    }

    fn text(&self, key: &str) -> Result<&'j str, ContractError> {
        text(self.field(key)?, &self.place_of(key))
    }

    fn object(&self, key: &str) -> Result<Object<'j>, ContractError> {
        Object::new(self.field(key)?, &self.place_of(key))
    }

    fn array(&self, key: &str) -> Result<&'j [Json], ContractError> {
        match self.field(key)? {
            Json::Array(items) => Ok(items),
            _ => Err(self.error(key, "must be an array")),
        }
    }

    fn number<T: TryFrom<i128> + fmt::Display + Bounded>(
        &self,
        key: &str,
    ) -> Result<T, ContractError> {
        match self.field(key)? {
            Json::Int(value) => T::try_from(*value).ok(),
            _ => None,
        }
        .ok_or_else(|| {
            self.error(
                key,
                format!("must be an integer from {} to {}", T::MIN, T::MAX),
            )
        })
    }
}

/// Integer field types, for range messages.
trait Bounded: Sized {
    const MIN: Self;
    const MAX: Self;
}

impl Bounded for u32 {
    const MIN: Self = u32::MIN;
    const MAX: Self = u32::MAX;
}

impl Bounded for u128 {
    const MIN: Self = u128::MIN;
    const MAX: Self = u128::MAX;
}

/// A parsed JSON document. Object keys keep file order, and a repeated key
/// is refused while parsing rather than overriding the first.
enum Json {
    Null,
    Bool(bool),
    Int(i128),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A fraction or exponent is refused by the default `visit_f64`.
        formatter.write_str("JSON with integer numbers and unique object keys")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Json, E> {
        Ok(Json::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Json, E> {
        Ok(Json::Int(i128::from(value)))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Json, E> {
        Ok(Json::Int(i128::from(value)))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Json, E> {
        Ok(Json::Text(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Json, E> {
        Ok(Json::Text(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Json, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = items.next_element()? {
            values.push(value);
        }
        Ok(Json::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut entries: Vec<(String, Json)> = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Json>()? {
            if entries.iter().any(|(existing, _)| *existing == key) {
                return Err(de::Error::custom(format!("duplicate key `{key}`")));
            }
            entries.push((key, value));
        }
        Ok(Json::Object(entries))
    }
}
