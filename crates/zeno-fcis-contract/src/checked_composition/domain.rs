//! The composed-domain profile: a `zeno-fcis/checked-composition/3`
//! manifest, read in its closed form and bound to each component's
//! compiled contract.
//!
//! Components are listed in merge order. Each names its contract directory
//! with the SHA-256 of `project.zeno`, of `v2/policy.json` and of the bound
//! program's identity, and is compiled and bound exactly as a `/1` leaf is.
//! For each component the manifest gives a `participates` formula, a
//! binding for each command and context field, an assumption over those
//! fields, an invariant over its state fields and the state fields it owns.
//! A component that owns none of its state fields is read-only: the
//! manifest also binds each state field, to earlier components' state, and
//! no committing decision may change it. The manifest also declares the
//! request's fields, the couplings and, optionally, lowered limits.
//!
//! A participation formula and a binding read request fields by name and
//! earlier components' pre-state fields as `component.field`; a read-only
//! state binding reads earlier components' pre-state only. Only components
//! that own their state can be read. An assumption reads the component's own
//! command and context fields and an invariant its own state fields, by
//! bare name. A coupling's invariant reads its members' state fields as
//! `component.field`. A scalar command or context root is one field named
//! `command` or `context`.
//!
//! Every key is required except `limits`, no other key is admitted and no
//! key repeats. Names are lowercase identifiers; formulas carry no leading
//! or trailing space; coupling names and each coupling's members are in
//! increasing order (members in merge order). One manifest has exactly one
//! canonical form, whose SHA-256 is its commitment.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde_json::{Value, json};

use super::formula::{Axis, Formula, Intake};
use super::leaf::{self, Bound, number, sha256};
use super::manifest::{DOMAIN_SCHEMA, pin, safe_path};
use super::refusal::{Obligation, Refusal, Stage};
use super::{LeafSources, hex};
use crate::internals::declarations::{ROOTS, Source};
use crate::internals::model::Contract;
use crate::internals::policy;
use crate::internals::review::domain::LeafDomain;
use crate::internals::rules::Class;

/// The check algorithm a composed-domain manifest asks for.
pub const DOMAIN_CHECKER: &str = "zeno-fcis/composed-domain-checker/1";
/// The compiled ceiling of one obligation's size; a manifest may lower it.
pub const MAX_OBLIGATION: u64 = 1 << 22;
/// The compiled ceiling of the sum of every obligation's size.
pub const MAX_TOTAL: u64 = 1 << 25;
const MAGIC: &[u8] = b"ZFCIS-COMPOSED-DOMAIN\0";
const MAX_COMPONENTS: usize = 64;
const MAX_REQUEST: usize = 64;
const MAX_COUPLINGS: usize = 256;

/// The values a field or a request field takes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldDomain {
    /// 0 and 1.
    Bool,
    /// Every integer from `min` to `max`.
    Int {
        /// The least value.
        min: i128,
        /// The greatest value.
        max: i128,
    },
    /// The listed variant IDs of a sum type.
    Variants(Vec<i128>),
}

impl FieldDomain {
    /// Every value in enumeration order.
    #[must_use]
    pub fn values(&self) -> Vec<i128> {
        match self {
            Self::Bool => vec![0, 1],
            Self::Int { min, max } => (*min..=*max).collect(),
            Self::Variants(variants) => variants.clone(),
        }
    }

    /// Whether `value` lies in the domain.
    #[must_use]
    pub fn contains(&self, value: i128) -> bool {
        match self {
            Self::Bool => value == 0 || value == 1,
            Self::Int { min, max } => (*min..=*max).contains(&value),
            Self::Variants(variants) => variants.contains(&value),
        }
    }

    /// Cardinality capped at u128::MAX; values above every configured budget refuse.
    #[must_use]
    pub fn size(&self) -> u128 {
        match self {
            Self::Bool => 2,
            Self::Int { min, max } => {
                if min > max {
                    0
                } else {
                    let lower = (*min as u128) ^ (1u128 << 127);
                    let upper = (*max as u128) ^ (1u128 << 127);
                    (upper - lower).checked_add(1).unwrap_or(u128::MAX)
                }
            }
            Self::Variants(variants) => variants.len() as u128,
        }
    }

    /// The value at `offset` in enumeration order; `offset` is below
    /// [`Self::size`].
    pub(crate) fn nth(&self, offset: u128) -> i128 {
        match self {
            Self::Bool => i128::from(offset != 0),
            Self::Int { min, .. } => {
                i128::try_from(i128::from(*min) + i128::try_from(offset).unwrap_or(i128::MAX))
                    .unwrap_or(i128::MAX)
            }
            Self::Variants(variants) => usize::try_from(offset)
                .ok()
                .and_then(|at| variants.get(at).copied())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn interval(&self) -> Option<(i128, i128)> {
        match self {
            Self::Int { min, max } => Some((*min, *max)),
            _ => None,
        }
    }

    pub(crate) fn json(&self) -> Value {
        match self {
            Self::Bool => json!({"kind": "bool"}),
            Self::Int { min, max } => {
                json!({"kind": "int", "min": super::json_integer(*min), "max": super::json_integer(*max)})
            }
            Self::Variants(variants) => {
                json!({"kind": "variants", "values": super::json_integers(variants)})
            }
        }
    }

    fn from_leaf(domain: &LeafDomain) -> Self {
        match domain {
            LeafDomain::Bool => Self::Bool,
            LeafDomain::Int { min, max } => Self::Int {
                min: *min,
                max: *max,
            },
            LeafDomain::Sum { variants, .. } => {
                Self::Variants(variants.iter().map(|id| i128::from(*id)).collect())
            }
        }
    }

    fn canonical(&self, out: &mut Vec<u8>) {
        match self {
            Self::Bool => out.push(0),
            Self::Int { min, max } => {
                out.push(1);
                out.extend_from_slice(&min.to_be_bytes());
                out.extend_from_slice(&max.to_be_bytes());
            }
            Self::Variants(variants) => {
                out.push(2);
                count(out, variants.len());
                for variant in variants {
                    out.extend_from_slice(&variant.to_be_bytes());
                }
            }
        }
    }
}

/// One request field.
#[derive(Clone, Debug)]
pub(crate) struct RequestField {
    pub(crate) name: String,
    pub(crate) domain: FieldDomain,
}

/// One component as the manifest lists it.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) project_sha256: [u8; 32],
    pub(crate) policy_sha256: [u8; 32],
    pub(crate) program_sha256: [u8; 32],
    /// `component.field` names, as written.
    pub(crate) owns: Vec<String>,
    pub(crate) participates: Formula,
    /// Each bound field with its expression, in field-name byte order.
    pub(crate) bind: Vec<(String, Formula)>,
    pub(crate) assume: Formula,
    pub(crate) invariant: Formula,
}

#[derive(Clone, Debug)]
pub(crate) struct CouplingEntry {
    pub(crate) name: String,
    pub(crate) members: Vec<String>,
    pub(crate) invariant: Formula,
}

/// The limits the planner applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    pub(crate) max_obligation: u64,
    pub(crate) max_total: u64,
    /// Whether the manifest lowered them.
    pub(crate) lowered: bool,
}

/// A composed-domain manifest in its closed form.
#[derive(Clone, Debug)]
pub(crate) struct DomainManifest {
    pub(crate) name: String,
    pub(crate) request: Vec<RequestField>,
    pub(crate) components: Vec<Entry>,
    pub(crate) couplings: Vec<CouplingEntry>,
    pub(crate) limits: Limits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    schema: String,
    checker: String,
    name: String,
    request: Vec<RequestFile>,
    components: Vec<ComponentFile>,
    couplings: Vec<CouplingFile>,
    limits: Option<LimitsFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestFile {
    name: String,
    domain: DomainFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainFile {
    kind: String,
    min: Option<i128>,
    max: Option<i128>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentFile {
    name: String,
    path: String,
    project_sha256: String,
    policy_sha256: String,
    program_sha256: String,
    owns: Vec<String>,
    participates: String,
    bind: StrictMap,
    assume: String,
    invariant: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CouplingFile {
    name: String,
    members: Vec<String>,
    invariant: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsFile {
    max_obligation: u64,
    max_total: u64,
}

/// A JSON object of strings whose keys may not repeat.
struct StrictMap(Vec<(String, String)>);

impl<'de> Deserialize<'de> for StrictMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Strict;
        impl<'de> Visitor<'de> for Strict {
            type Value = StrictMap;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object of field names to expressions")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StrictMap, A::Error> {
                let mut entries: Vec<(String, String)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if entries.iter().any(|(seen, _)| *seen == key) {
                        return Err(de::Error::custom(format!("duplicate key `{key}`")));
                    }
                    entries.push((key, value));
                }
                Ok(StrictMap(entries))
            }
        }
        deserializer.deserialize_map(Strict)
    }
}

pub(crate) fn invalid(place: impl Into<String>, reason: impl Into<String>) -> Refusal {
    Refusal::new(
        Stage::Manifest,
        Obligation::ClosedManifestForm,
        place,
        reason,
    )
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn formula(place: &str, text: &str) -> Result<Formula, Refusal> {
    if text.is_empty() || text.trim() != text {
        return Err(invalid(
            place,
            "must be a formula without leading or trailing space; write `true` for none",
        ));
    }
    Formula::parse(text).map_err(|intake| match intake {
        Intake::Syntax(reason) => {
            Refusal::new(Stage::Manifest, Obligation::DefinedPredicate, place, reason)
        }
        Intake::TooLarge(reason) => invalid(place, reason),
    })
}

impl DomainManifest {
    /// Reads and validates the closed form.
    ///
    /// # Errors
    /// The first key, value or order the closed form refuses.
    pub(crate) fn read(text: &str) -> Result<Self, Refusal> {
        let file: ManifestFile = serde_json::from_str(text)
            .map_err(|error| invalid("composition manifest", error.to_string()))?;
        if file.schema != DOMAIN_SCHEMA {
            return Err(invalid(
                "schema",
                format!("expected `{DOMAIN_SCHEMA}`, found `{}`", file.schema),
            ));
        }
        if file.checker != DOMAIN_CHECKER {
            return Err(invalid(
                "checker",
                format!("expected `{DOMAIN_CHECKER}`, found `{}`", file.checker),
            ));
        }
        if file.name.is_empty() {
            return Err(invalid("name", "must name the composition"));
        }
        if file.request.len() > MAX_REQUEST {
            return Err(invalid(
                "request",
                format!("may list at most {MAX_REQUEST} fields"),
            ));
        }
        let mut request = Vec::with_capacity(file.request.len());
        for (index, field) in file.request.into_iter().enumerate() {
            let place = format!("request[{index}]");
            if !identifier(&field.name) {
                return Err(invalid(&place, "a name is a lowercase identifier"));
            }
            if request
                .iter()
                .any(|seen: &RequestField| seen.name == field.name)
            {
                return Err(invalid(
                    &place,
                    format!("repeats the request field `{}`", field.name),
                ));
            }
            let domain = match (
                field.domain.kind.as_str(),
                field.domain.min,
                field.domain.max,
            ) {
                ("bool", None, None) => FieldDomain::Bool,
                ("int", Some(min), Some(max)) if min <= max => FieldDomain::Int { min, max },
                _ => {
                    return Err(invalid(
                        format!("{place}.domain"),
                        "is {\"kind\": \"bool\"} or {\"kind\": \"int\", \"min\": a, \"max\": b} with a <= b",
                    ));
                }
            };
            request.push(RequestField {
                name: field.name,
                domain,
            });
        }
        if file.components.is_empty() || file.components.len() > MAX_COMPONENTS {
            return Err(invalid(
                "components",
                format!("must list 1 to {MAX_COMPONENTS} components"),
            ));
        }
        let mut components: Vec<Entry> = Vec::with_capacity(file.components.len());
        for (index, component) in file.components.into_iter().enumerate() {
            let place = |key: &str| format!("components[{index}].{key}");
            if !identifier(&component.name) {
                return Err(invalid(place("name"), "a name is a lowercase identifier"));
            }
            if components.iter().any(|seen| seen.name == component.name) {
                return Err(invalid(
                    place("name"),
                    format!("two components are named `{}`", component.name),
                ));
            }
            if !safe_path(&component.path) {
                return Err(invalid(
                    place("path"),
                    "must be a relative path of plain names, without `.` or `..`",
                ));
            }
            if components.iter().any(|seen| seen.path == component.path) {
                return Err(invalid(place("path"), "two components name the same path"));
            }
            let mut bind = component
                .bind
                .0
                .into_iter()
                .map(|(field, text)| {
                    let at = place(&format!("bind.{field}"));
                    Ok((field, formula(&at, &text)?))
                })
                .collect::<Result<Vec<_>, Refusal>>()?;
            bind.sort_by(|left, right| left.0.cmp(&right.0));
            components.push(Entry {
                project_sha256: pin(&place("project_sha256"), &component.project_sha256)?,
                policy_sha256: pin(&place("policy_sha256"), &component.policy_sha256)?,
                program_sha256: pin(&place("program_sha256"), &component.program_sha256)?,
                participates: formula(&place("participates"), &component.participates)?,
                assume: formula(&place("assume"), &component.assume)?,
                invariant: formula(&place("invariant"), &component.invariant)?,
                name: component.name,
                path: component.path,
                owns: component.owns,
                bind,
            });
        }
        if file.couplings.len() > MAX_COUPLINGS {
            return Err(invalid(
                "couplings",
                format!("may list at most {MAX_COUPLINGS}"),
            ));
        }
        let mut couplings: Vec<CouplingEntry> = Vec::with_capacity(file.couplings.len());
        for (index, coupling) in file.couplings.into_iter().enumerate() {
            let place = |key: &str| format!("couplings[{index}].{key}");
            if !identifier(&coupling.name) {
                return Err(invalid(place("name"), "a name is a lowercase identifier"));
            }
            if couplings
                .last()
                .is_some_and(|previous| previous.name >= coupling.name)
            {
                return Err(invalid(
                    "couplings",
                    "names must be distinct and in increasing order",
                ));
            }
            couplings.push(CouplingEntry {
                invariant: formula(&place("invariant"), &coupling.invariant)?,
                name: coupling.name,
                members: coupling.members,
            });
        }
        let limits = match file.limits {
            None => Limits {
                max_obligation: MAX_OBLIGATION,
                max_total: MAX_TOTAL,
                lowered: false,
            },
            Some(limits) => {
                if limits.max_obligation == 0
                    || limits.max_obligation > MAX_OBLIGATION
                    || limits.max_total == 0
                    || limits.max_total > MAX_TOTAL
                {
                    return Err(invalid(
                        "limits",
                        format!(
                            "may only lower the ceilings: 1 <= max_obligation <= {MAX_OBLIGATION} and 1 <= max_total <= {MAX_TOTAL}"
                        ),
                    ));
                }
                Limits {
                    max_obligation: limits.max_obligation,
                    max_total: limits.max_total,
                    lowered: true,
                }
            }
        };
        Ok(Self {
            name: file.name,
            request,
            components,
            couplings,
            limits,
        })
    }

    /// The canonical bytes: every field, in the manifest's canonical order.
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&3u16.to_be_bytes());
        text(&mut out, DOMAIN_CHECKER);
        text(&mut out, &self.name);
        count(&mut out, self.request.len());
        for field in &self.request {
            text(&mut out, &field.name);
            field.domain.canonical(&mut out);
        }
        count(&mut out, self.components.len());
        for entry in &self.components {
            text(&mut out, &entry.name);
            text(&mut out, &entry.path);
            for pinned in [
                &entry.project_sha256,
                &entry.policy_sha256,
                &entry.program_sha256,
            ] {
                out.extend_from_slice(pinned);
            }
            let mut owns = entry.owns.clone();
            owns.sort();
            count(&mut out, owns.len());
            for owned in &owns {
                text(&mut out, owned);
            }
            text(&mut out, &entry.participates.text);
            count(&mut out, entry.bind.len());
            for (field, expression) in &entry.bind {
                text(&mut out, field);
                text(&mut out, &expression.text);
            }
            text(&mut out, &entry.assume.text);
            text(&mut out, &entry.invariant.text);
        }
        count(&mut out, self.couplings.len());
        for coupling in &self.couplings {
            text(&mut out, &coupling.name);
            count(&mut out, coupling.members.len());
            for member in &coupling.members {
                text(&mut out, member);
            }
            text(&mut out, &coupling.invariant.text);
        }
        out.push(u8::from(self.limits.lowered));
        out.extend_from_slice(&self.limits.max_obligation.to_be_bytes());
        out.extend_from_slice(&self.limits.max_total.to_be_bytes());
        out
    }
}

/// The component directories a composed-domain manifest names, in merge
/// order, for the caller that reads their files.
///
/// # Errors
/// The manifest's refusal.
pub fn domain_paths(text: &str) -> Result<Vec<String>, Refusal> {
    Ok(DomainManifest::read(text)?
        .components
        .into_iter()
        .map(|entry| entry.path)
        .collect())
}

/// A value a formula reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Read {
    /// A request field, by its position in the request.
    Request(usize),
    /// A pre-state field of an earlier component, by component and state
    /// position.
    State { component: usize, field: usize },
    /// A position of the component's own local tuple.
    Local(usize),
}

/// A formula with what each of its names reads.
#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub(crate) formula: Formula,
    pub(crate) reads: Vec<Read>,
}

/// One field of a component's local tuple.
#[derive(Clone, Debug)]
pub(crate) struct LocalField {
    pub(crate) name: String,
    pub(crate) source: Source,
    pub(crate) domain: FieldDomain,
}

/// A component bound to its compiled contract.
#[derive(Debug)]
pub(crate) struct Component {
    pub(crate) name: String,
    pub(crate) bound: Bound,
    pub(crate) project_sha256: [u8; 32],
    pub(crate) policy_sha256: [u8; 32],
    pub(crate) program_sha256: [u8; 32],
    /// The local tuple's fields: state, then command, then context.
    pub(crate) fields: Vec<LocalField>,
    pub(crate) state_width: usize,
    /// Whether it owns its state; otherwise its state is read-only.
    pub(crate) owns: bool,
    pub(crate) participates: Resolved,
    /// One binding for each command and context field, in local order.
    pub(crate) bindings: Vec<Resolved>,
    /// For a read-only component, one binding for each state field.
    pub(crate) state_bindings: Vec<Resolved>,
    pub(crate) assume: Resolved,
    pub(crate) invariant: Resolved,
    /// Whether its contract has a CommittedFailure case.
    pub(crate) committed_failure: bool,
    /// The components its participation and bindings read, in merge order.
    pub(crate) providers: Vec<usize>,
    /// The request fields they read, in request order.
    pub(crate) cone: Vec<usize>,
    /// The declared genesis state, in state order.
    pub(crate) genesis: Vec<i128>,
}

impl Component {
    pub(crate) fn state_domain_size(&self) -> Option<u128> {
        self.fields[..self.state_width]
            .iter()
            .try_fold(1u128, |size, field| size.checked_mul(field.domain.size()))
    }

    pub(crate) fn local_domain_size(&self) -> Option<u128> {
        self.fields
            .iter()
            .try_fold(1u128, |size, field| size.checked_mul(field.domain.size()))
    }

    pub(crate) fn state_fields(&self) -> &[LocalField] {
        &self.fields[..self.state_width]
    }

    pub(crate) fn input_fields(&self) -> &[LocalField] {
        &self.fields[self.state_width..]
    }

    pub(crate) fn contract(&self) -> Result<Contract<'_>, Refusal> {
        Contract::build(
            &self.bound.declarations,
            &self.bound.rules,
            self.bound.commitment,
        )
        .map_err(|error| {
            Refusal::new(
                Stage::Binding,
                Obligation::OrdinaryContractRoute,
                format!("component `{}` {}", self.name, error.place()),
                error.reason(),
            )
        })
    }
}

/// A coupling bound to its members.
#[derive(Debug)]
pub(crate) struct Coupling {
    pub(crate) name: String,
    /// Members in merge order.
    pub(crate) members: Vec<usize>,
    pub(crate) invariant: Resolved,
}

/// A validated composition: the manifest with every component bound.
#[derive(Debug)]
pub(crate) struct Composition {
    pub(crate) manifest: DomainManifest,
    pub(crate) manifest_sha256: [u8; 32],
    pub(crate) commitment: [u8; 32],
    pub(crate) components: Vec<Component>,
    pub(crate) couplings: Vec<Coupling>,
}

impl Composition {
    /// Reads the manifest, binds every component and validates every name
    /// and scope. Nothing is decided.
    ///
    /// # Errors
    /// The manifest's refusal, a component's binding refusal, or the first
    /// name or scope the profile refuses.
    pub(crate) fn bind(manifest_text: &str, sources: &[LeafSources]) -> Result<Self, Refusal> {
        let manifest = DomainManifest::read(manifest_text)?;
        if sources.len() != manifest.components.len() {
            return Err(invalid(
                "components",
                "the files of every component, and only those, must be supplied",
            ));
        }
        let mut layouts = Vec::with_capacity(sources.len());
        for (entry, files) in manifest.components.iter().zip(sources) {
            let label = format!("component `{}`", entry.name);
            let bound = leaf::bind(
                &label,
                false,
                &entry.project_sha256,
                &entry.policy_sha256,
                &files.project,
                &files.rules,
            )?;
            let fields = local_fields(&label, &bound)?;
            program_identity(&label, entry, &bound)?;
            layouts.push((bound, fields));
        }
        ownership(&manifest, &layouts)?;
        let mut components = Vec::with_capacity(layouts.len());
        for (index, (bound, fields)) in layouts.into_iter().enumerate() {
            components.push(resolve(&manifest, index, bound, fields, &components)?);
        }
        let couplings = manifest
            .couplings
            .iter()
            .map(|coupling| resolve_coupling(coupling, &components))
            .collect::<Result<Vec<_>, _>>()?;
        let commitment = sha256(&manifest.canonical_bytes());
        Ok(Self {
            manifest_sha256: sha256(manifest_text.as_bytes()),
            commitment,
            manifest,
            components,
            couplings,
        })
    }

    /// The values a read takes.
    pub(crate) fn axis(&self, owner: &Component, read: Read) -> Axis {
        let values = match read {
            Read::Request(at) => self.manifest.request[at].domain.values(),
            Read::State { component, field } => {
                self.components[component].fields[field].domain.values()
            }
            Read::Local(at) => owner.fields[at].domain.values(),
        };
        Axis { values }
    }

    /// The number of cells of a formula's table: the product of the sizes
    /// of the domains it reads; `None` when it overflows `u128`.
    pub(crate) fn formula_size(&self, owner: &Component, resolved: &Resolved) -> Option<u128> {
        resolved.reads.iter().try_fold(1u128, |size, read| {
            let domain = match *read {
                Read::Request(at) => &self.manifest.request[at].domain,
                Read::State { component, field } => {
                    &self.components[component].fields[field].domain
                }
                Read::Local(at) => &owner.fields[at].domain,
            };
            size.checked_mul(domain.size())
        })
    }

    /// The axes of a resolved formula of `owner`.
    pub(crate) fn axes(&self, owner: &Component, resolved: &Resolved) -> Vec<Axis> {
        resolved
            .reads
            .iter()
            .map(|read| self.axis(owner, *read))
            .collect()
    }
}

/// The local tuple's fields of a bound contract, each named by its declared
/// field name, or `command` or `context` for a scalar root.
fn local_fields(label: &str, bound: &Bound) -> Result<Vec<LocalField>, Refusal> {
    let mut fields: Vec<LocalField> = Vec::with_capacity(bound.positions.len());
    for position in &bound.positions {
        let root = ROOTS
            .iter()
            .find(|(source, _)| *source == position.source)
            .map_or(0, |(_, root)| *root);
        let name = match position.field {
            Some(id) => bound
                .declarations
                .fields(root)
                .ok()
                .and_then(|declared| declared.iter().find(|field| field.id == id))
                .map(|field| field.name.clone())
                .ok_or_else(|| {
                    Refusal::new(
                        Stage::Binding,
                        Obligation::CheckedProfile,
                        format!("{label} project.zeno"),
                        format!("field {id} of type {root} has no declared name"),
                    )
                })?,
            None => match position.source {
                Source::State => "state",
                Source::Command => "command",
                Source::Context => "context",
            }
            .to_owned(),
        };
        if fields.iter().any(|seen| seen.name == name) {
            return Err(Refusal::new(
                Stage::Binding,
                Obligation::CheckedProfile,
                format!("{label} project.zeno"),
                format!(
                    "two of its state, command and context fields are named `{name}`; the profile reads fields by name"
                ),
            ));
        }
        fields.push(LocalField {
            name,
            source: position.source,
            domain: FieldDomain::from_leaf(&position.domain),
        });
    }
    Ok(fields)
}

/// The bound program's identity against the manifest's pin.
fn program_identity(label: &str, entry: &Entry, bound: &Bound) -> Result<(), Refusal> {
    let route = |error: crate::ContractError| {
        Refusal::new(
            Stage::Binding,
            Obligation::OrdinaryContractRoute,
            format!("{label} {}", error.place()),
            error.reason(),
        )
    };
    let contract =
        Contract::build(&bound.declarations, &bound.rules, bound.commitment).map_err(route)?;
    let program = policy::with_authority(&contract, &bound.schema, |authority| {
        sha256(authority.identity())
    })
    .map_err(route)?;
    if program != entry.program_sha256 {
        return Err(Refusal::new(
            Stage::Binding,
            Obligation::ProgramBinding,
            label.to_owned(),
            format!(
                "the bound program identity has SHA-256 {}, the manifest pins {}",
                hex(&program),
                hex(&entry.program_sha256)
            ),
        )
        .with_witness(json!({"bound_program_sha256": hex(&program)})));
    }
    Ok(())
}

/// Every owned field is a state field of the component that owns it, owned
/// once; a component owns all of its state fields or none.
fn ownership(
    manifest: &DomainManifest,
    layouts: &[(Bound, Vec<LocalField>)],
) -> Result<(), Refusal> {
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in &manifest.components {
        for owned in &entry.owns {
            if let Some(first) = owners.insert(owned, &entry.name) {
                return Err(Refusal::new(
                    Stage::Ownership,
                    Obligation::DisjointOwnership,
                    format!("component `{}` owns", entry.name),
                    format!(
                        "two components own the field `{owned}`: `{first}` and `{}`",
                        entry.name
                    ),
                ));
            }
        }
    }
    for (entry, (_, fields)) in manifest.components.iter().zip(layouts) {
        let place = format!("component `{}` owns", entry.name);
        let state: Vec<&LocalField> = fields
            .iter()
            .filter(|field| field.source == Source::State)
            .collect();
        for owned in &entry.owns {
            let (component, field) = owned.split_once('.').unwrap_or(("", owned));
            if component != entry.name {
                return Err(Refusal::new(
                    Stage::Ownership,
                    Obligation::DisjointOwnership,
                    &place,
                    format!(
                        "`{owned}` is not one of its own fields; a component owns only its own state"
                    ),
                ));
            }
            if !state.iter().any(|candidate| candidate.name == field) {
                return Err(Refusal::new(
                    Stage::Ownership,
                    Obligation::CompleteFootprint,
                    &place,
                    format!("`{owned}` is not one of its state fields"),
                ));
            }
        }
        if !entry.owns.is_empty() && entry.owns.len() != state.len() {
            return Err(Refusal::new(
                Stage::Ownership,
                Obligation::CompleteFootprint,
                &place,
                "a component owns every one of its state fields, or none and is read-only",
            ));
        }
    }
    Ok(())
}

/// Binds a component's formulas to what they read, refusing every name out
/// of scope; `earlier` are the components before it in merge order.
fn resolve(
    manifest: &DomainManifest,
    index: usize,
    bound: Bound,
    fields: Vec<LocalField>,
    earlier: &[Component],
) -> Result<Component, Refusal> {
    let entry = &manifest.components[index];
    let place = |key: &str| format!("component `{}` {key}", entry.name);
    let state_width = fields
        .iter()
        .take_while(|field| field.source == Source::State)
        .count();
    let owns = !entry.owns.is_empty();
    let out_of_scope = |key: &str, reason: String| {
        Refusal::new(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            place(key),
            reason,
        )
    };
    // A request field, or an earlier component's state field.
    let wiring = |key: &str, name: &str, request: bool| -> Result<Read, Refusal> {
        if let Some((owner, field)) = name.split_once('.') {
            let Some(provider) = manifest
                .components
                .iter()
                .position(|candidate| candidate.name == owner)
            else {
                return Err(out_of_scope(
                    key,
                    format!("reads `{name}`, and no component is named `{owner}`"),
                ));
            };
            if provider >= index {
                return Err(out_of_scope(
                    key,
                    format!(
                        "reads `{name}`: a binding reads the pre-state of earlier components only, never of a later one or of the component itself"
                    ),
                ));
            }
            let provider_component = &earlier[provider];
            if !provider_component.owns {
                return Err(out_of_scope(
                    key,
                    format!(
                        "reads `{name}`, but `{owner}` owns no state; only owned state can be read"
                    ),
                ));
            }
            let Some(at) = provider_component
                .state_fields()
                .iter()
                .position(|candidate| candidate.name == field)
            else {
                return Err(out_of_scope(
                    key,
                    format!("reads `{name}`, which is not a state field of `{owner}`"),
                ));
            };
            return Ok(Read::State {
                component: provider,
                field: at,
            });
        }
        match manifest
            .request
            .iter()
            .position(|candidate| candidate.name == name)
        {
            Some(at) if request => Ok(Read::Request(at)),
            Some(_) => Err(out_of_scope(
                key,
                format!(
                    "reads the request field `{name}`; a read-only state binding reads earlier components' state only"
                ),
            )),
            None => Err(out_of_scope(
                key,
                format!(
                    "reads `{name}`, which is neither a request field nor an earlier component's state field"
                ),
            )),
        }
    };
    let local = |key: &str, name: &str, source_state: bool| -> Result<Read, Refusal> {
        fields
            .iter()
            .position(|field| field.name == name && (field.source == Source::State) == source_state)
            .map(Read::Local)
            .ok_or_else(|| {
                out_of_scope(
                    key,
                    format!(
                        "reads `{name}`, which is not one of its own {} fields",
                        if source_state {
                            "state"
                        } else {
                            "command or context"
                        }
                    ),
                )
            })
    };
    let declarations = &bound.declarations;
    let compiled = |key: &str, formula: &Formula, value: bool| -> Result<(), Refusal> {
        let checked = if value {
            formula.check_value()
        } else {
            formula.check_predicate(declarations)
        };
        checked.map_err(|reason| {
            Refusal::new(
                Stage::Manifest,
                Obligation::DefinedPredicate,
                place(key),
                reason,
            )
        })
    };
    let resolve_with = |key: &str,
                        formula: &Formula,
                        value: bool,
                        read: &dyn Fn(&str) -> Result<Read, Refusal>| {
        let reads = formula
            .names
            .iter()
            .map(|name| read(name))
            .collect::<Result<Vec<_>, _>>()?;
        compiled(key, formula, value)?;
        Ok::<_, Refusal>(Resolved {
            formula: formula.clone(),
            reads,
        })
    };
    let participates = resolve_with("participates", &entry.participates, false, &|name| {
        wiring("participates", name, true)
    })?;
    // Every command and context field is bound, and, for a read-only
    // component, every state field; nothing else is.
    let mut expected: Vec<&LocalField> = fields[state_width..].iter().collect();
    if !owns {
        expected.extend(&fields[..state_width]);
    }
    for (field, _) in &entry.bind {
        if !expected.iter().any(|candidate| candidate.name == *field) {
            return Err(out_of_scope(
                &format!("bind.{field}"),
                if owns {
                    format!("`{field}` is not one of its command or context fields")
                } else {
                    format!("`{field}` is not one of its fields")
                },
            ));
        }
    }
    let binding = |field: &LocalField, request: bool| -> Result<Resolved, Refusal> {
        let key = format!("bind.{}", field.name);
        let Some((_, formula)) = entry.bind.iter().find(|(name, _)| *name == field.name) else {
            return Err(out_of_scope(
                "bind",
                format!("binds no expression to its field `{}`", field.name),
            ));
        };
        resolve_with(&key, formula, true, &|name| wiring(&key, name, request))
    };
    let bindings = fields[state_width..]
        .iter()
        .map(|field| binding(field, true))
        .collect::<Result<Vec<_>, _>>()?;
    let state_bindings = if owns {
        Vec::new()
    } else {
        fields[..state_width]
            .iter()
            .map(|field| binding(field, false))
            .collect::<Result<Vec<_>, _>>()?
    };
    let assume = resolve_with("assume", &entry.assume, false, &|name| {
        local("assume", name, false)
    })?;
    let invariant = resolve_with("invariant", &entry.invariant, false, &|name| {
        local("invariant", name, true)
    })?;
    let mut providers = BTreeSet::new();
    let mut cone = BTreeSet::new();
    for resolved in std::iter::once(&participates)
        .chain(&bindings)
        .chain(&state_bindings)
    {
        for read in &resolved.reads {
            match read {
                Read::Request(at) => {
                    cone.insert(*at);
                }
                Read::State { component, .. } => {
                    providers.insert(*component);
                }
                Read::Local(_) => {}
            }
        }
    }
    let contract =
        Contract::build(declarations, &bound.rules, bound.commitment).map_err(|error| {
            Refusal::new(
                Stage::Binding,
                Obligation::OrdinaryContractRoute,
                place(error.place()),
                error.reason(),
            )
        })?;
    let mut genesis = Vec::with_capacity(state_width);
    for position in bound.positions.iter().take(state_width) {
        let value = contract
            .genesis
            .iter()
            .find(|(id, _)| Some(*id) == position.field)
            .and_then(|(_, value)| number(value));
        genesis.push(value.ok_or_else(|| {
            Refusal::new(
                Stage::Binding,
                Obligation::GenuineGenesis,
                place("project.zeno"),
                format!("{} has no declared genesis value", position.name),
            )
        })?);
    }
    let committed_failure = bound
        .rules
        .cases
        .iter()
        .any(|case| case.class == Class::CommittedFailure);
    Ok(Component {
        name: entry.name.clone(),
        project_sha256: entry.project_sha256,
        policy_sha256: entry.policy_sha256,
        program_sha256: entry.program_sha256,
        state_width,
        owns,
        participates,
        bindings,
        state_bindings,
        assume,
        invariant,
        committed_failure,
        providers: providers.into_iter().collect(),
        cone: cone.into_iter().collect(),
        genesis,
        fields,
        bound,
    })
}

fn resolve_coupling(
    coupling: &CouplingEntry,
    components: &[Component],
) -> Result<Coupling, Refusal> {
    let place = format!("coupling `{}`", coupling.name);
    let refuse = |reason: String| {
        Refusal::new(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            &place,
            reason,
        )
    };
    let mut members = Vec::with_capacity(coupling.members.len());
    for member in &coupling.members {
        let Some(at) = components
            .iter()
            .position(|component| component.name == *member)
        else {
            return Err(refuse(format!(
                "names `{member}`, which is not a component"
            )));
        };
        if !components[at].owns {
            return Err(refuse(format!(
                "names `{member}`, which owns no state; a coupling relates owned state"
            )));
        }
        if members.last().is_some_and(|previous| *previous >= at) {
            return Err(refuse(
                "lists its members once each, in merge order".to_owned(),
            ));
        }
        members.push(at);
    }
    if members.is_empty() {
        return Err(refuse("needs at least one member".to_owned()));
    }
    let mut reads = Vec::with_capacity(coupling.invariant.names.len());
    for name in &coupling.invariant.names {
        let member = name.split_once('.').and_then(|(owner, field)| {
            members.iter().find_map(|at| {
                (components[*at].name == owner)
                    .then(|| {
                        components[*at]
                            .state_fields()
                            .iter()
                            .position(|candidate| candidate.name == field)
                            .map(|position| Read::State {
                                component: *at,
                                field: position,
                            })
                    })
                    .flatten()
            })
        });
        reads.push(member.ok_or_else(|| {
            refuse(format!(
                "reads `{name}`, which is not a state field of one of its members"
            ))
        })?);
    }
    coupling
        .invariant
        .check_predicate(&components[members[0]].bound.declarations)
        .map_err(|reason| {
            Refusal::new(
                Stage::Manifest,
                Obligation::DefinedPredicate,
                &place,
                reason,
            )
        })?;
    Ok(Coupling {
        name: coupling.name.clone(),
        members,
        invariant: Resolved {
            formula: coupling.invariant.clone(),
            reads,
        },
    })
}

fn text(out: &mut Vec<u8>, value: &str) {
    count(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

fn count(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes());
}
