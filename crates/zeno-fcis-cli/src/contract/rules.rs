//! The reviewed rules file, `v2/policy.json`: leaf bindings, variables, the
//! ordered decision cases, the genesis state, the kind of every law, the
//! adopted candidate programs and the earlier contracts a behaviour change
//! replaced.
//!
//! Unknown and duplicate keys are refused, so no entry is silently ignored
//! or overridden.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use super::ContractError;
use super::declarations::Leaf;
use super::expr::{self, Ast};

/// The rules file format this generator reads. The optional `adoptions` list
/// is part of this version: a reader that does not know it refuses the key.
pub(super) const RULES_SCHEMA: &str = "zeno-fcis/template-declarative-policy/2";

/// Whether an adopted candidate uses the same Steps as the program it
/// replaced on every input, as its receipt reports. The label only describes
/// the adoption: generation refuses `preserved` when the receipt reports
/// otherwise, and nothing else depends on it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Usage {
    /// Equal Step usage on every input: the usage observations sealed into a
    /// publication do not change, although every sealed subject embeds the
    /// new contract identity.
    Preserved,
    /// Step usage differs on some input, so sealed usage observations change.
    NewVersion,
}

impl Usage {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::NewVersion => "new-version",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "preserved" => Some(Self::Preserved),
            "new-version" => Some(Self::NewVersion),
            _ => None,
        }
    }
}

/// One adopted candidate program, in adoption order. Adoption `n` keeps its
/// candidate at `v2/adoptions/n/program.zcve` and the equivalence receipt
/// that compared it with the program it replaced at
/// `v2/adoptions/n/receipt.json`; both files are bound by their SHA-256.
/// It also binds the SHA-256 of the policy of version `n`, the version it
/// superseded, as adoption wrote it: stores may run that version, so
/// generation refuses any later edit that would change it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Adoption {
    pub(crate) candidate_sha256: String,
    pub(crate) receipt_sha256: String,
    pub(crate) usage: Usage,
    pub(crate) superseded_policy_sha256: String,
}

/// One earlier contract that `zeno-fcis contract evolve` replaced, in
/// evolution order. Evolution `n` keeps that contract's `project.zeno`,
/// `v2/policy.json` and adoptions under `v2/evolutions/n/`, and the
/// plain-language diff of the change at `v2/evolutions/n/review.txt`. It
/// binds the SHA-256 of the replaced contract's last policy and of the review
/// text, and its kind: a behaviour change when `kind` is absent, a rename, or
/// a migration, which also binds the SHA-256 of `migration.json` and of each
/// shortcut it keeps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Evolution {
    pub(crate) superseded_policy_sha256: String,
    pub(crate) review_sha256: String,
    pub(crate) kind: EvolutionKind,
}

/// How a store follows an evolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EvolutionKind {
    /// A reviewed rule change, admitted when the new contract's state laws
    /// and claims hold on the store's state.
    BehaviourChange,
    /// Only names change; the store's state is framed again.
    Rename,
    /// A data migration, admitted by forward simulation.
    Migration {
        /// The SHA-256 of `migration.json`.
        migration_sha256: String,
        /// The shortcuts kept beside it, each from an earlier version.
        shortcuts: Vec<Shortcut>,
    },
}

impl EvolutionKind {
    /// The kind's name in reports and in `v2/policy.json`.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::BehaviourChange => "behaviour-change",
            Self::Rename => "rename",
            Self::Migration { .. } => "migration",
        }
    }
}

/// A migration directly from an earlier version, kept beside an
/// evolution's migration as `shortcuts/from-{from_version}.json`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Shortcut {
    pub(crate) from_version: u32,
    pub(crate) sha256: String,
}

/// The versioned schema of a migration file.
pub(crate) const MIGRATION_SCHEMA: &str = "zeno-fcis/migration/1";
/// The keys a migration file may hold.
pub(super) const MIGRATION_KEYS: [&str; 3] = ["schema", "from_version", "state"];
/// The keys of one entry of a migration file's `state`.
pub(super) const MIGRATION_FIELD_KEYS: [&str; 3] = ["from", "default", "map"];

/// A value a migration file writes: a Boolean, or an integer, which is a
/// variant's ID for a variant field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MigrationValue {
    Bool(bool),
    Int(i128),
}

/// Where a new state field takes its value from, as a migration file writes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum MigrationSource {
    /// `{"from": f}`: old field `f`'s value.
    From(u16),
    /// `{"default": v}`: the value `v`.
    Default(MigrationValue),
    /// `{"from": f, "map": {"old value": v, ..}}`: a table over old field
    /// `f`'s values; keys are decimal integers, or `false` and `true`.
    Map(u16, Vec<(String, MigrationValue)>),
}

/// A parsed migration file, `zeno-fcis/migration/1`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MigrationFile {
    /// For a shortcut, the earlier version it migrates from.
    pub(super) from_version: Option<u32>,
    /// Each new state field, in file order.
    pub(super) state: Vec<(u16, MigrationSource)>,
}

/// Reads a migration file; `place` names it in refusals.
///
/// # Errors
/// A refusal naming the entry: unknown or duplicate keys, a wrong schema, a
/// field ID that is not a `u16`, or an entry that is not one of the three
/// forms.
pub(super) fn read_migration(source: &str, place: &str) -> Result<MigrationFile, ContractError> {
    let json: Json = serde_json::from_str(source)
        .map_err(|error| ContractError::new(place, error.to_string()))?;
    let file = Object::new(&json, place)?;
    file.only(&MIGRATION_KEYS)?;
    if file.text("schema")? != MIGRATION_SCHEMA {
        return Err(file.error("schema", format!("must be \"{MIGRATION_SCHEMA}\"")));
    }
    let from_version = match file.optional("from_version") {
        None => None,
        Some(_) => Some(file.number::<u32>("from_version")?),
    };
    let state = file.object("state")?;
    let value = |json: &Json, place: &str| match json {
        Json::Bool(value) => Ok(MigrationValue::Bool(*value)),
        Json::Int(value) => Ok(MigrationValue::Int(*value)),
        _ => Err(ContractError::new(
            place,
            "must be true, false or an integer (a variant's ID for a variant field)",
        )),
    };
    let mut fields = Vec::new();
    for (key, entry) in state.entries() {
        let place = state.place_of(key);
        let id = self::id::<u16>(key, &place)?;
        let entry = Object::new(entry, &place)?;
        entry.only(&MIGRATION_FIELD_KEYS)?;
        let source = match (
            entry.optional("from"),
            entry.optional("default"),
            entry.optional("map"),
        ) {
            (Some(_), None, None) => MigrationSource::From(entry.number::<u16>("from")?),
            (None, Some(default), None) => {
                MigrationSource::Default(value(default, &entry.place_of("default"))?)
            }
            (Some(_), None, Some(_)) => {
                let map = entry.object("map")?;
                let cases = map
                    .entries()
                    .map(|(key, json)| Ok((key.to_owned(), value(json, &map.place_of(key))?)))
                    .collect::<Result<_, ContractError>>()?;
                MigrationSource::Map(entry.number::<u16>("from")?, cases)
            }
            _ => {
                return Err(ContractError::new(
                    &place,
                    "must be {\"from\": old field}, {\"default\": value} or {\"from\": old field, \"map\": {old value: new value, ..}}",
                ));
            }
        };
        fields.push((id, source));
    }
    Ok(MigrationFile {
        from_version,
        state: fields,
    })
}

/// Framework law IDs used when the rules file names none.
const FAILURE_LAW: u32 = 908;
const REJECT_LAW: u32 = 909;

/// The keys a rules file may hold; `docs/CONTRACT_RULES.md` documents each.
pub(super) const FILE_KEYS: [&str; 14] = [
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
    "adoptions",
    "evolutions",
    // Reviewer notes; they do not affect the contract.
    "idempotency",
    "original_domain",
];
/// The keys of `roots`.
pub(super) const ROOT_KEYS: [&str; 3] = ["state", "command", "context"];
/// The keys of one case; `rule` names the rule for reviewers only.
pub(super) const CASE_KEYS: [&str; 6] = ["when", "class", "reason", "post", "outbox", "rule"];
/// The keys of one delivery in a case's `outbox`.
pub(super) const DELIVERY_KEYS: [&str; 5] = [
    "ordinal",
    "channel",
    "destination",
    "payload",
    "idempotency_ordinal",
];
/// The keys of one entry of `adoptions`.
pub(super) const ADOPTION_KEYS: [&str; 4] = [
    "candidate_sha256",
    "receipt_sha256",
    "usage",
    "superseded_policy_sha256",
];

/// The keys of one entry of `evolutions`.
pub(super) const EVOLUTION_KEYS: [&str; 5] = [
    "superseded_policy_sha256",
    "review_sha256",
    "kind",
    "migration_sha256",
    "shortcuts",
];
/// The keys of one entry of an evolution's `shortcuts`.
pub(super) const SHORTCUT_KEYS: [&str; 2] = ["from_version", "sha256"];

/// Decision class of a case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Class {
    Accept,
    Reject,
    CommittedFailure,
}

impl Class {
    pub(super) const ALL: [Self; 3] = [Self::Accept, Self::Reject, Self::CommittedFailure];

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
    pub(super) const ALL: [Self; 10] = [
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

#[derive(Clone, Debug)]
pub(super) struct Case {
    /// The `rule` note naming the rule for reviewers; not part of the contract.
    pub(super) rule: Option<String>,
    pub(super) when: Ast,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// Successor value of every state field, by field ID; empty for rejects.
    pub(super) post: BTreeMap<u16, Ast>,
    pub(super) outbox: Vec<Delivery>,
}

#[derive(Clone, Debug)]
pub(super) struct Delivery {
    pub(super) ordinal: u32,
    pub(super) channel: u32,
    pub(super) destination: String,
    pub(super) payload: BTreeMap<u16, Ast>,
    pub(super) idempotency: u128,
}

#[derive(Clone, Debug)]
pub(super) struct Rules {
    pub(super) template: String,
    pub(super) leaf_bindings: BTreeMap<u32, Leaf>,
    pub(super) variables: BTreeMap<String, Ast>,
    pub(super) cases: Vec<Case>,
    pub(super) genesis: BTreeMap<u16, Constant>,
    pub(super) law_kinds: BTreeMap<u32, LawKind>,
    pub(super) failure_law: u32,
    pub(super) reject_law: u32,
    pub(super) adoptions: Vec<Adoption>,
    pub(super) evolutions: Vec<Evolution>,
}

impl Rules {
    pub(super) fn read(source: &str) -> Result<Self, ContractError> {
        let json = parse(source)?;
        let file = Object::new(&json, FILE)?;
        file.only(&FILE_KEYS)?;
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
        roots.only(&ROOT_KEYS)?;
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
        // The library refuses law ID 0.
        let law = |key: &str, default: u32| match file.optional(key) {
            Some(_) => match file.number(key)? {
                0 => Err(file.error(key, "must be a nonzero law ID")),
                id => Ok(id),
            },
            None => Ok(default),
        };
        let adoptions = match file.optional("adoptions") {
            None => Vec::new(),
            Some(_) => file
                .array("adoptions")?
                .iter()
                .enumerate()
                .map(|(index, entry)| adoption(entry, &format!("{FILE} adoptions[{index}]")))
                .collect::<Result<_, ContractError>>()?,
        };
        let evolutions = match file.optional("evolutions") {
            None => Vec::new(),
            Some(_) => file
                .array("evolutions")?
                .iter()
                .enumerate()
                .map(|(index, entry)| evolution(entry, &format!("{FILE} evolutions[{index}]")))
                .collect::<Result<_, ContractError>>()?,
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
            adoptions,
            evolutions,
        })
    }
}

const FILE: &str = "v2/policy.json";

fn parse(source: &str) -> Result<Json, ContractError> {
    serde_json::from_str(source).map_err(|error| ContractError::new(FILE, error.to_string()))
}

/// The file parsed and rendered again, for checking that `with_adoption`
/// keeps a reviewed file's layout.
#[cfg(test)]
pub(super) fn reformat(source: &str) -> Result<String, ContractError> {
    Ok(parse(source)?.render())
}

/// The rules file with one more adoption appended. The whole file is
/// rendered again in the templates' layout, two-space indentation and one
/// entry per line, so its own spacing and line breaks are not kept; object
/// keys keep their order and `adoptions` is added last when absent. The
/// result is validated separately when it is generated from.
pub(crate) fn with_adoption(source: &str, adoption: &Adoption) -> Result<String, ContractError> {
    let json = parse(source)?;
    let Json::Object(mut entries) = json else {
        return Err(ContractError::new(FILE, "must be an object"));
    };
    let entry = Json::Object(vec![
        (
            "candidate_sha256".to_owned(),
            Json::Text(adoption.candidate_sha256.clone()),
        ),
        (
            "receipt_sha256".to_owned(),
            Json::Text(adoption.receipt_sha256.clone()),
        ),
        (
            "usage".to_owned(),
            Json::Text(adoption.usage.name().to_owned()),
        ),
        (
            "superseded_policy_sha256".to_owned(),
            Json::Text(adoption.superseded_policy_sha256.clone()),
        ),
    ]);
    match entries.iter_mut().find(|(key, _)| key == "adoptions") {
        Some((_, Json::Array(items))) => items.push(entry),
        Some(_) => {
            return Err(ContractError::new(
                format!("{FILE} adoptions"),
                "must be an array",
            ));
        }
        None => entries.push(("adoptions".to_owned(), Json::Array(vec![entry]))),
    }
    Ok(Json::Object(entries).render())
}

/// The rules file with each adoption's `receipt_sha256` replaced, in order,
/// rendered again as `with_adoption` renders it. The result is validated
/// when it is generated from.
pub(super) fn with_receipt_digests(
    source: &str,
    digests: &[String],
) -> Result<String, ContractError> {
    let json = parse(source)?;
    let Json::Object(mut entries) = json else {
        return Err(ContractError::new(FILE, "must be an object"));
    };
    let place = format!("{FILE} adoptions");
    let items = match entries.iter_mut().find(|(key, _)| key == "adoptions") {
        Some((_, Json::Array(items))) if items.len() == digests.len() => items,
        _ => {
            return Err(ContractError::new(
                place,
                format!("must list {} adoptions", digests.len()),
            ));
        }
    };
    for (index, (item, digest)) in items.iter_mut().zip(digests).enumerate() {
        let receipt = match item {
            Json::Object(fields) => fields
                .iter_mut()
                .find(|(key, _)| key == "receipt_sha256")
                .map(|(_, value)| value),
            _ => None,
        }
        .ok_or_else(|| ContractError::new(format!("{place}[{index}]"), "has no receipt_sha256"))?;
        *receipt = Json::Text(digest.clone());
    }
    Ok(Json::Object(entries).render())
}

/// The rules file with one more evolution appended and every adoption
/// removed, rendered as `with_adoption` renders it: the rules of a contract
/// that evolved, kept as the current ones, whose own adoptions start again.
/// `evolutions` takes the place of `adoptions` in the key order, or is added
/// last.
pub(crate) fn with_evolution(
    source: &str,
    earlier: &[Evolution],
    evolution: &Evolution,
) -> Result<String, ContractError> {
    let json = parse(source)?;
    let Json::Object(entries) = json else {
        return Err(ContractError::new(FILE, "must be an object"));
    };
    let items: Vec<Json> = earlier
        .iter()
        .chain(std::iter::once(evolution))
        .map(|evolution| {
            let mut entry = vec![
                (
                    "superseded_policy_sha256".to_owned(),
                    Json::Text(evolution.superseded_policy_sha256.clone()),
                ),
                (
                    "review_sha256".to_owned(),
                    Json::Text(evolution.review_sha256.clone()),
                ),
            ];
            // A behaviour change, the first kind, writes no kind.
            match &evolution.kind {
                EvolutionKind::BehaviourChange => {}
                EvolutionKind::Rename => {
                    entry.push(("kind".to_owned(), Json::Text("rename".to_owned())));
                }
                EvolutionKind::Migration {
                    migration_sha256,
                    shortcuts,
                } => {
                    entry.push(("kind".to_owned(), Json::Text("migration".to_owned())));
                    entry.push((
                        "migration_sha256".to_owned(),
                        Json::Text(migration_sha256.clone()),
                    ));
                    if !shortcuts.is_empty() {
                        entry.push((
                            "shortcuts".to_owned(),
                            Json::Array(
                                shortcuts
                                    .iter()
                                    .map(|shortcut| {
                                        Json::Object(vec![
                                            (
                                                "from_version".to_owned(),
                                                Json::Int(i128::from(shortcut.from_version)),
                                            ),
                                            (
                                                "sha256".to_owned(),
                                                Json::Text(shortcut.sha256.clone()),
                                            ),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ));
                    }
                }
            }
            Json::Object(entry)
        })
        .collect();
    let mut kept: Vec<(String, Json)> = entries
        .into_iter()
        .filter(|(key, _)| key != "adoptions" && key != "evolutions")
        .collect();
    kept.push(("evolutions".to_owned(), Json::Array(items)));
    Ok(Json::Object(kept).render())
}

/// The rules file without its `evolutions`, rendered as `with_adoption`
/// renders it: a replaced contract's rules as `v2/evolutions/n/` keeps them,
/// its own adoptions included.
pub(crate) fn without_evolutions(source: &str) -> Result<String, ContractError> {
    let json = parse(source)?;
    let Json::Object(entries) = json else {
        return Err(ContractError::new(FILE, "must be an object"));
    };
    Ok(Json::Object(
        entries
            .into_iter()
            .filter(|(key, _)| key != "evolutions")
            .collect(),
    )
    .render())
}

/// The rules file without its `adoptions` and `evolutions`, rendered as
/// `with_adoption` renders it: the first version of the contract it
/// declares, as a lineage of its own.
pub(super) fn first_version(source: &str) -> Result<String, ContractError> {
    let json = parse(source)?;
    let Json::Object(entries) = json else {
        return Err(ContractError::new(FILE, "must be an object"));
    };
    Ok(Json::Object(
        entries
            .into_iter()
            .filter(|(key, _)| key != "adoptions" && key != "evolutions")
            .collect(),
    )
    .render())
}

fn sha256_text(entry: &Object<'_>, key: &str) -> Result<String, ContractError> {
    let text = entry.text(key)?;
    let hex = text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hex {
        Ok(text.to_owned())
    } else {
        Err(entry.error(key, "must be a lowercase hexadecimal SHA-256"))
    }
}

fn evolution(json: &Json, place: &str) -> Result<Evolution, ContractError> {
    let entry = Object::new(json, place)?;
    entry.only(&EVOLUTION_KEYS)?;
    let kind = match entry.optional("kind") {
        None => None,
        Some(_) => Some(entry.text("kind")?),
    };
    let only_migration = |key: &str| match (entry.optional(key), kind) {
        (Some(_), Some("migration")) | (None, _) => Ok(()),
        (Some(_), _) => Err(entry.error(key, "is written only for a migration")),
    };
    only_migration("migration_sha256")?;
    only_migration("shortcuts")?;
    let kind = match kind {
        None => EvolutionKind::BehaviourChange,
        Some("rename") => EvolutionKind::Rename,
        Some("migration") => EvolutionKind::Migration {
            migration_sha256: sha256_text(&entry, "migration_sha256")?,
            shortcuts: match entry.optional("shortcuts") {
                None => Vec::new(),
                Some(_) => entry
                    .array("shortcuts")?
                    .iter()
                    .enumerate()
                    .map(|(index, json)| {
                        let shortcut = Object::new(
                            json,
                            &format!("{}[{index}]", entry.place_of("shortcuts")),
                        )?;
                        shortcut.only(&SHORTCUT_KEYS)?;
                        Ok(Shortcut {
                            from_version: shortcut.number::<u32>("from_version")?,
                            sha256: sha256_text(&shortcut, "sha256")?,
                        })
                    })
                    .collect::<Result<_, ContractError>>()?,
            },
        },
        Some(_) => {
            return Err(entry.error(
                "kind",
                "must be \"rename\" or \"migration\"; a behaviour change writes no kind",
            ));
        }
    };
    Ok(Evolution {
        superseded_policy_sha256: sha256_text(&entry, "superseded_policy_sha256")?,
        review_sha256: sha256_text(&entry, "review_sha256")?,
        kind,
    })
}

fn adoption(json: &Json, place: &str) -> Result<Adoption, ContractError> {
    let entry = Object::new(json, place)?;
    entry.only(&ADOPTION_KEYS)?;
    let digest = |key: &str| sha256_text(&entry, key);
    let usage = entry.text("usage")?;
    Ok(Adoption {
        candidate_sha256: digest("candidate_sha256")?,
        receipt_sha256: digest("receipt_sha256")?,
        usage: Usage::parse(usage).ok_or_else(|| {
            entry.error(
                "usage",
                format!("`{usage}` is not preserved or new-version"),
            )
        })?,
        superseded_policy_sha256: digest("superseded_policy_sha256")?,
    })
}

fn case(json: &Json, place: &str) -> Result<Case, ContractError> {
    let case = Object::new(json, place)?;
    // `rule` names the rule for reviewers; it does not affect the contract.
    case.only(&CASE_KEYS)?;
    let rule = match case.optional("rule") {
        Some(_) => Some(case.text("rule")?.to_owned()),
        None => None,
    };
    let written = case.text("class")?;
    let class = Class::ALL
        .into_iter()
        .find(|class| class.name() == written)
        .ok_or_else(|| {
            case.error(
                "class",
                format!("`{written}` is not Accept, Reject or CommittedFailure"),
            )
        })?;
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
            delivery.only(&DELIVERY_KEYS)?;
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
        rule,
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

/// The leaves `leaf_bindings` may name: `["Bool"]`, `["I128", min, max]`
/// and `["Text", min, max]`.
pub(super) const LEAVES: [&str; 3] = ["Bool", "I128", "Text"];

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
        (Some(Json::Text(kind)), 1) if kind == LEAVES[0] => Some(Leaf::Bool),
        (Some(Json::Text(kind)), 3) if kind == LEAVES[1] => bound(1)
            .zip(bound(2))
            .filter(|(min, max)| min <= max)
            .map(|(min, max)| Leaf::I128 { min, max }),
        (Some(Json::Text(kind)), 3) if kind == LEAVES[2] => bound(1)
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

impl Bounded for u16 {
    const MIN: Self = u16::MIN;
    const MAX: Self = u16::MAX;
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

impl Json {
    /// The document as the templates are written: two-space indentation, one
    /// entry per line, empty containers inline, and a final newline.
    fn render(&self) -> String {
        let mut text = String::new();
        self.write(&mut text, 0);
        text.push('\n');
        text
    }

    fn write(&self, out: &mut String, indent: usize) {
        let open = |out: &mut String, bracket: char| {
            out.push(bracket);
            out.push('\n');
        };
        let close = |out: &mut String, bracket: char| {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            out.push(bracket);
        };
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Int(value) => out.push_str(&value.to_string()),
            Self::Text(value) => out.push_str(&serde_json::Value::from(value.as_str()).to_string()),
            Self::Array(items) if items.is_empty() => out.push_str("[]"),
            Self::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Self::Array(items) => {
                open(out, '[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push_str(",\n");
                    }
                    out.push_str(&" ".repeat(indent + 2));
                    item.write(out, indent + 2);
                }
                close(out, ']');
            }
            Self::Object(entries) => {
                open(out, '{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push_str(",\n");
                    }
                    out.push_str(&" ".repeat(indent + 2));
                    out.push_str(&serde_json::Value::from(key.as_str()).to_string());
                    out.push_str(": ");
                    value.write(out, indent + 2);
                }
                close(out, '}');
            }
        }
    }
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
