//! The aggregate of a checked composition: one ordinary contract whose state
//! is every leaf's owned slots, whose command selects one leaf, and whose
//! cases are each leaf's cases guarded by that selection.
//!
//! The assembly profile fixes the aggregate's field map, so that generator
//! and validator agree on it without a further declaration:
//!
//! - slot `s` is state field `1000 + s`, with the owning leaf field's type;
//! - the leaves' command fields keep their IDs and are shared by ID, so two
//!   leaves that both declare a command field ID must give it the same type;
//! - the target command field, one above the largest aggregate field ID,
//!   ranges over `0..=N-1` and selects the leaf at that position in the
//!   manifest;
//! - a context field is an environment input: an interface field no edge
//!   feeds, numbered after the state and command fields in the order of
//!   leaf and field, with the leaf field's type; with none, one singleton
//!   `unit` field, which nothing reads, keeps the context a record.
//!
//! A leaf's peer view is the provider's state field in the same pre-state:
//! `context.102.c` becomes `pre.100.<provider slot field>`. Each leaf's
//! StateInvariant laws are kept, renamed to its slots; leaves with other law
//! kinds, deliveries or text inputs are outside the profile.
//!
//! Generation is a convenience. The validator in `assembly` decides whether
//! any aggregate, generated here or written by hand, is a machine map onto
//! the composition.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::leaf::Compiled;
use super::manifest::Manifest;
use super::refusal::{Obligation, Refusal, Stage};
use super::structure::Resolved;
use crate::internals::declarations::{Form, Leaf as Scalar, Source};
use crate::internals::expr::{self, Ast, Binary};
use crate::internals::rules::{Class, Constant, LawKind};
use zeno_fcis_spec::LawScope;

/// The state field of slot `s` is `STATE_FIELD_BASE + s`.
pub(crate) const STATE_FIELD_BASE: u16 = 1000;
/// The first law ID the aggregate's renamed leaf laws take.
const FIRST_LAW: u32 = 500;
/// Law IDs the contract generator keeps for its own laws.
const FRAMEWORK_LAWS: [u32; 4] = [908, 909, 990, 991];
const RULES_SCHEMA: &str = "zeno-fcis/template-declarative-policy/2";

/// An aggregate contract's two files, `project.zeno` and `v2/policy.json`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateSources {
    /// `project.zeno`.
    pub project: String,
    /// `v2/policy.json`.
    pub rules: String,
}

/// The assembly profile's field map for one composition.
#[derive(Debug)]
pub(crate) struct Layout {
    /// For each slot: the owning leaf's position and field.
    pub(crate) slots: Vec<(usize, u16)>,
    /// Each leaf's fields and their slots.
    pub(crate) owned: Vec<BTreeMap<u16, usize>>,
    /// The target command field and its type.
    pub(crate) target: (u16, u32),
    /// The shared command fields: ID → (type, name).
    pub(crate) command: BTreeMap<u16, (u32, String)>,
    /// The environment context fields: ID → (type, name).
    pub(crate) context: BTreeMap<u16, (u32, String)>,
    /// The leaf position and field each environment context field carries.
    pub(crate) inputs: BTreeMap<u16, (usize, u16)>,
    /// The singleton context field and its type, when there is no
    /// environment input.
    pub(crate) unit: Option<(u16, u32)>,
    /// For each leaf: its context field → the aggregate input it reads.
    pub(crate) views: Vec<BTreeMap<u16, View>>,
}

/// Where a leaf's context field comes from in the aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    /// A peer's slot in the same pre-state.
    Slot(usize),
    /// An environment input of the aggregate's context.
    Environment(u16),
}

pub(crate) fn state_field(slot: usize) -> u16 {
    STATE_FIELD_BASE.saturating_add(u16::try_from(slot).unwrap_or(u16::MAX))
}

fn profile(place: impl Into<String>, reason: impl Into<String>) -> Refusal {
    Refusal::new(Stage::Assembly, Obligation::AssemblyProfile, place, reason)
}

impl Layout {
    /// The field map of a checked composition: its slots, the leaves'
    /// command fields and each leaf's views.
    pub(crate) fn of(
        manifest: &Manifest,
        compiled: &[Compiled],
        edges: &[Resolved],
    ) -> Result<Self, Refusal> {
        let mut slots: Vec<Option<(usize, u16)>> = Vec::new();
        let mut owned = Vec::with_capacity(manifest.leaves.len());
        for (position, leaf) in manifest.leaves.iter().enumerate() {
            let mut fields = BTreeMap::new();
            for own in &leaf.owns {
                let slot = usize::try_from(own.slot).unwrap_or(usize::MAX);
                if slot >= slots.len() {
                    slots.resize(slot + 1, None);
                }
                slots[slot] = Some((position, own.field));
                fields.insert(own.field, slot);
            }
            owned.push(fields);
        }
        let slots: Vec<(usize, u16)> = slots
            .into_iter()
            .collect::<Option<_>>()
            .ok_or_else(|| profile("slots", "slots must be 0..n without a gap"))?;
        if slots.len() > usize::from(u16::MAX - STATE_FIELD_BASE) {
            return Err(profile("slots", "too many slots for the state field range"));
        }
        let mut command: BTreeMap<u16, (u32, String)> = BTreeMap::new();
        for (position, leaf) in compiled.iter().enumerate() {
            let place = format!("leaf {}", manifest.leaves[position].id);
            for field in leaf.declarations.fields(101).map_err(|error| {
                profile(&place, format!("{}: {}", error.place(), error.reason()))
            })? {
                match command.get(&field.id) {
                    Some((type_id, _)) if !same_type(compiled, *type_id, field.type_id) => {
                        return Err(profile(
                            place,
                            format!(
                                "command field {} has another type than the same field of an earlier leaf",
                                field.id
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        command.insert(field.id, (field.type_id, field.name.clone()));
                    }
                }
            }
        }
        let mut views = Vec::with_capacity(compiled.len());
        let mut unfed: Vec<(usize, u16)> = Vec::new();
        for (position, leaf) in compiled.iter().enumerate() {
            let mut map = BTreeMap::new();
            for field in leaf.fields(Source::Context) {
                let Some(edge) = edges
                    .iter()
                    .find(|edge| edge.consumer == position && edge.interface == field.field)
                else {
                    unfed.push((position, field.field));
                    continue;
                };
                let slot = owned[edge.provider]
                    .get(&edge.view_field)
                    .copied()
                    .ok_or_else(|| {
                        profile("edges", "a view names a field its leaf does not own")
                    })?;
                map.insert(field.field, View::Slot(slot));
            }
            views.push(map);
        }
        let mut used: BTreeSet<u16> = (0..slots.len()).map(state_field).collect();
        for id in command.keys() {
            if !used.insert(*id) {
                return Err(profile(
                    "fields",
                    format!("field {id} collides with the assembly profile's state fields"),
                ));
            }
        }
        // Environment inputs take the next IDs, in leaf and field order.
        let mut context = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        for (position, field) in unfed {
            let id = used
                .iter()
                .next_back()
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| profile("fields", "no field ID is free for an environment input"))?;
            let declared = compiled[position]
                .declarations
                .fields(102)
                .map_err(|error| profile("fields", error.reason()))?
                .iter()
                .find(|candidate| candidate.id == field)
                .map(|candidate| (candidate.type_id, candidate.name.clone()))
                .ok_or_else(|| profile("fields", "an interface field has no declaration"))?;
            used.insert(id);
            context.insert(id, (declared.0, format!("leaf_{position}_{}", declared.1)));
            inputs.insert(id, (position, field));
            views[position].insert(field, View::Environment(id));
        }
        let next_field = used
            .iter()
            .next_back()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| profile("fields", "no field ID is free for the target"))?;
        let types: BTreeSet<u32> = compiled
            .iter()
            .flat_map(|leaf| leaf.declarations.types.keys().copied())
            .collect();
        let next_type = types.iter().next_back().copied().unwrap_or(102).max(102) + 1;
        let unit = if context.is_empty() {
            Some((
                next_field
                    .checked_add(1)
                    .ok_or_else(|| profile("fields", "no field ID is free for the unit context"))?,
                next_type + 1,
            ))
        } else {
            None
        };
        Ok(Self {
            slots,
            owned,
            target: (next_field, next_type),
            command,
            context,
            inputs,
            unit,
            views,
        })
    }

    /// The aggregate name of a leaf's input or observation, or `None` for a
    /// name outside the leaf's roots.
    fn rename(&self, leaf: usize, name: &str) -> Option<String> {
        let field = |rest: &str| rest.parse::<u16>().ok();
        if let Some(rest) = name.strip_prefix("pre.100.") {
            let slot = self.owned[leaf].get(&field(rest)?)?;
            return Some(format!("pre.100.{}", state_field(*slot)));
        }
        if let Some(rest) = name.strip_prefix("post.100.") {
            let slot = self.owned[leaf].get(&field(rest)?)?;
            return Some(format!("post.100.{}", state_field(*slot)));
        }
        if let Some(rest) = name.strip_prefix("command.101.") {
            let id = field(rest)?;
            return self
                .command
                .contains_key(&id)
                .then(|| format!("command.101.{id}"));
        }
        if let Some(rest) = name.strip_prefix("context.102.") {
            return match self.views[leaf].get(&field(rest)?)? {
                View::Slot(slot) => Some(format!("pre.100.{}", state_field(*slot))),
                View::Environment(id) => Some(format!("context.102.{id}")),
            };
        }
        None
    }

    fn renamed(&self, leaf: usize, ast: &Ast) -> Result<Ast, String> {
        Ok(match ast {
            Ast::Name(name) => match self.rename(leaf, name) {
                Some(renamed) => Ast::Name(renamed),
                None if ["pre.", "post.", "command.", "context.", "outbox."]
                    .iter()
                    .any(|root| name.starts_with(root)) =>
                {
                    return Err(format!("`{name}` has no aggregate form"));
                }
                None => ast.clone(),
            },
            Ast::Bool(_) | Ast::Int(_) => ast.clone(),
            Ast::Not(inner) => Ast::Not(Box::new(self.renamed(leaf, inner)?)),
            Ast::Neg(inner) => Ast::Neg(Box::new(self.renamed(leaf, inner)?)),
            Ast::Binary(operator, left, right) => Ast::Binary(
                *operator,
                Box::new(self.renamed(leaf, left)?),
                Box::new(self.renamed(leaf, right)?),
            ),
            Ast::Choose(condition, then, otherwise) => Ast::Choose(
                Box::new(self.renamed(leaf, condition)?),
                Box::new(self.renamed(leaf, then)?),
                Box::new(self.renamed(leaf, otherwise)?),
            ),
            Ast::MulDiv(..) | Ast::ProductsGe(..) => {
                return Err("arithmetic extension is outside the assembly profile".to_owned());
            }
            Ast::Div(rounding, left, right) => Ast::Div(
                *rounding,
                Box::new(self.renamed(leaf, left)?),
                Box::new(self.renamed(leaf, right)?),
            ),
        })
    }
}

/// Whether two leaves' types of one ID are the same type.
pub(crate) fn same_type(compiled: &[Compiled], left: u32, right: u32) -> bool {
    if left != right {
        return false;
    }
    let forms: Vec<String> = compiled
        .iter()
        .filter_map(|leaf| leaf.declarations.types.get(&left))
        .map(|declared| form_key(&declared.form))
        .collect();
    forms.windows(2).all(|pair| pair[0] == pair[1])
}

/// A type's form as a comparable key: its leaf, or its variant IDs; a record
/// compares by its fields.
fn form_key(form: &Form) -> String {
    match form {
        Form::Leaf(Scalar::Bool) => "bool".to_owned(),
        Form::Leaf(Scalar::I128 { min, max }) => format!("int {min} {max}"),
        Form::Leaf(Scalar::Text { min, max }) => format!("text {min} {max}"),
        Form::Sum(variants) => variants
            .iter()
            .map(|variant| variant.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        Form::Record(fields) => fields
            .iter()
            .map(|field| format!("{}:{}", field.id, field.type_id))
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// Generates the aggregate of a checked composition.
///
/// # Errors
/// A leaf outside the assembly profile, or a clash between leaves' types,
/// reasons or names.
pub(crate) fn generate(
    manifest: &Manifest,
    compiled: &[Compiled],
    edges: &[Resolved],
) -> Result<AggregateSources, Refusal> {
    let layout = Layout::of(manifest, compiled, edges)?;
    let count = compiled.len();
    let leaf_place = |position: usize| {
        let leaf = &manifest.leaves[position];
        format!("leaf {} `{}`", leaf.id, leaf.name)
    };

    // Types: every non-root type a field uses, by ID, from its first leaf.
    let mut types: BTreeMap<u32, (&str, &Form)> = BTreeMap::new();
    let mut use_type = |position: usize, type_id: u32| -> Result<(), Refusal> {
        let declared = compiled[position]
            .declarations
            .get(type_id)
            .map_err(|error| profile(leaf_place(position), error.reason()))?;
        match types.get(&type_id) {
            Some((_, form)) if form_key(form) != form_key(&declared.form) => Err(profile(
                leaf_place(position),
                format!("type {type_id} differs from the same type of an earlier leaf"),
            )),
            Some(_) => Ok(()),
            None => {
                types.insert(type_id, (declared.name.as_str(), &declared.form));
                Ok(())
            }
        }
    };
    for (slot, (position, field)) in layout.slots.iter().enumerate() {
        let declared = compiled[*position]
            .declarations
            .fields(100)
            .map_err(|error| profile(leaf_place(*position), error.reason()))?
            .iter()
            .find(|candidate| candidate.id == *field)
            .ok_or_else(|| profile(format!("slot {slot}"), "names a field its leaf lacks"))?;
        use_type(*position, declared.type_id)?;
    }
    for (position, leaf) in compiled.iter().enumerate() {
        for field in leaf
            .declarations
            .fields(101)
            .map_err(|error| profile(leaf_place(position), error.reason()))?
        {
            use_type(position, field.type_id)?;
        }
    }
    for (id, (position, _)) in &layout.inputs {
        let (type_id, _) = layout.context[id];
        use_type(*position, type_id)?;
    }

    let mut project = String::new();
    let name = identifier(&manifest.name);
    project.push_str(&format!(
        "zeno 1;\nproject 1 {name};\nnamespace 2 composition;\n"
    ));
    project.push_str(
        "type 100 state ComposedState;\ntype 101 command ComposedCommand;\ntype 102 context ComposedContext;\n",
    );
    let mut names = Names::default();
    let mut variants = String::new();
    for (type_id, (type_name, form)) in &types {
        let type_name = names.unique(type_name, *type_id);
        match form {
            Form::Leaf(Scalar::Bool) => {
                project.push_str(&format!("type {type_id} bool {type_name};\n"))
            }
            Form::Leaf(Scalar::I128 { min, max }) => project.push_str(&format!(
                "type {type_id} int {type_name} in {min}..={max};\n"
            )),
            Form::Sum(declared) => {
                project.push_str(&format!("type {type_id} data {type_name};\n"));
                for variant in declared {
                    let variant_name = names.unique(&variant.name, u32::from(variant.id));
                    variants.push_str(&format!(
                        "variant {} {type_id} {variant_name} none;\n",
                        variant.id
                    ));
                }
            }
            Form::Leaf(Scalar::Text { .. }) | Form::Record(_) => {
                return Err(profile(
                    format!("type {type_id}"),
                    "text and record fields are outside the assembly profile",
                ));
            }
        }
    }
    let (target_field, target_type) = layout.target;
    let last = count.saturating_sub(1);
    let target_name = names.unique("Target", target_type);
    project.push_str(&format!(
        "type {target_type} int {target_name} in 0..={last};\n"
    ));
    if let Some((_, unit_type)) = layout.unit {
        let unit_name = names.unique("Unit", unit_type);
        project.push_str(&format!("type {unit_type} int {unit_name} in 0..=0;\n"));
    }
    let mut fields = Names::default();
    for (slot, (position, field)) in layout.slots.iter().enumerate() {
        let declared = compiled[*position]
            .declarations
            .fields(100)
            .map_err(|error| profile(leaf_place(*position), error.reason()))?
            .iter()
            .find(|candidate| candidate.id == *field)
            .ok_or_else(|| profile(format!("slot {slot}"), "names a field its leaf lacks"))?;
        let field_name = fields.unique(&format!("slot_{slot}"), u32::from(state_field(slot)));
        project.push_str(&format!(
            "field {} 100 {field_name} {};\n",
            state_field(slot),
            declared.type_id
        ));
    }
    let target_name = fields.unique("target", u32::from(target_field));
    project.push_str(&format!(
        "field {target_field} 101 {target_name} {target_type};\n"
    ));
    for (id, (type_id, field_name)) in &layout.command {
        let field_name = fields.unique(field_name, u32::from(*id));
        project.push_str(&format!("field {id} 101 {field_name} {type_id};\n"));
    }
    for (id, (type_id, field_name)) in &layout.context {
        let field_name = fields.unique(field_name, u32::from(*id));
        project.push_str(&format!("field {id} 102 {field_name} {type_id};\n"));
    }
    if let Some((unit_field, unit_type)) = layout.unit {
        let unit_name = fields.unique("unit", u32::from(unit_field));
        project.push_str(&format!(
            "field {unit_field} 102 {unit_name} {unit_type};\n"
        ));
    }
    project.push_str(&variants);

    // Reasons: the union by ID; a shared ID must keep its name and precedence.
    let mut reasons: BTreeMap<u32, (String, u32)> = BTreeMap::new();
    for (position, leaf) in compiled.iter().enumerate() {
        for (id, declared) in &leaf.declarations.notes.reasons {
            match reasons.get(id) {
                Some(existing) if existing != declared => {
                    return Err(profile(
                        leaf_place(position),
                        format!("reason {id} differs from the same reason of an earlier leaf"),
                    ));
                }
                Some(_) => {}
                None => {
                    reasons.insert(*id, declared.clone());
                }
            }
        }
    }
    for (id, (reason_name, precedence)) in &reasons {
        let reason_name = names.unique(reason_name, *id);
        project.push_str(&format!(
            "reason {id} {reason_name} precedence {precedence};\n"
        ));
    }
    project.push_str(
        "component 400 composed {\n  owns 100;\n  reads pre.100;\n  writes post.100;\n  contexts context.102;\n  budget steps 4096;\n}\nmerge [400];\n",
    );

    // Laws: each leaf's StateInvariant laws, renamed to its slots.
    let mut law_kinds = Map::new();
    let mut next_law = FIRST_LAW;
    for (position, leaf) in compiled.iter().enumerate() {
        if !leaf.rules.delivery_laws.is_empty() {
            return Err(profile(
                leaf_place(position),
                "delivery laws are outside the assembly profile",
            ));
        }
        for law in &leaf.declarations.laws {
            if leaf.rules.law_kinds.get(&law.id) != Some(&LawKind::StateInvariant) {
                return Err(profile(
                    leaf_place(position),
                    format!(
                        "law {} is not a StateInvariant; the assembly profile keeps only those",
                        law.id
                    ),
                ));
            }
            while FRAMEWORK_LAWS.contains(&next_law) {
                next_law += 1;
            }
            if next_law >= 900 {
                return Err(profile(
                    "laws",
                    "too many leaf laws for the aggregate's law IDs",
                ));
            }
            let formula = layout.renamed(position, &law.formula).map_err(|reason| {
                profile(format!("{} law {}", leaf_place(position), law.id), reason)
            })?;
            let law_name = names.unique(
                &format!(
                    "{}_{}",
                    identifier(&manifest.leaves[position].name),
                    law.name
                ),
                next_law,
            );
            project.push_str(&format!(
                "law {next_law} {law_name} on {}{} = {};\n",
                scope(law.scope),
                if law.genesis { ", genesis" } else { "" },
                expr::render(&formula)
            ));
            law_kinds.insert(next_law.to_string(), json!("StateInvariant"));
            next_law += 1;
        }
    }

    // Cases: each leaf's cases, in order, guarded by the target.
    let route = |position: usize| {
        Ast::Binary(
            Binary::Eq,
            Box::new(Ast::Name(format!("command.101.{target_field}"))),
            Box::new(Ast::Int(i128::try_from(position).unwrap_or(i128::MAX))),
        )
    };
    let mut cases = Vec::new();
    for (position, leaf) in compiled.iter().enumerate() {
        let rules = &leaf.rules;
        let last_case = rules.cases.len().saturating_sub(1);
        for (index, case) in rules.cases.iter().enumerate() {
            let place = format!("{} cases[{index}]", leaf_place(position));
            if !case.outbox.is_empty() {
                return Err(profile(
                    place,
                    "deliveries are outside the assembly profile",
                ));
            }
            let expand = |ast: &Ast| {
                expr::expand(ast, &rules.variables)
                    .and_then(|ast| layout.renamed(position, &ast))
                    .map_err(|reason| profile(&place, reason))
            };
            let when = if position == last && index == last_case {
                Ast::Bool(true)
            } else if index == last_case {
                route(position)
            } else {
                Ast::Binary(
                    Binary::And,
                    Box::new(route(position)),
                    Box::new(expand(&case.when)?),
                )
            };
            let mut post = Map::new();
            for (field, value) in &case.post {
                let slot = layout.owned[position]
                    .get(field)
                    .ok_or_else(|| profile(&place, format!("post field {field} is not owned")))?;
                post.insert(
                    state_field(*slot).to_string(),
                    json!(expr::render(&expand(value)?)),
                );
            }
            let mut entry = Map::new();
            entry.insert(
                "rule".to_owned(),
                json!(format!(
                    "{} {}",
                    manifest.leaves[position].name,
                    case.rule.clone().unwrap_or_else(|| format!("case {index}"))
                )),
            );
            entry.insert(
                "when".to_owned(),
                match &when {
                    Ast::Bool(true) => json!(true),
                    other => json!(expr::render(other)),
                },
            );
            entry.insert("class".to_owned(), json!(class_name(case.class)));
            entry.insert("reason".to_owned(), json!(case.reason));
            entry.insert("post".to_owned(), Value::Object(post));
            entry.insert("outbox".to_owned(), json!([]));
            cases.push(Value::Object(entry));
        }
    }
    let mut genesis = Map::new();
    for (slot, (position, field)) in layout.slots.iter().enumerate() {
        let value = compiled[*position]
            .rules
            .genesis
            .get(field)
            .ok_or_else(|| {
                profile(
                    leaf_place(*position),
                    format!("genesis has no field {field}"),
                )
            })?;
        genesis.insert(
            state_field(slot).to_string(),
            match value {
                Constant::Bool(value) => json!(value),
                Constant::Int(value) => json!(value),
            },
        );
    }
    let rules = json!({
        "schema": RULES_SCHEMA,
        "template": manifest.name,
        "roots": {"state": 100, "command": 101, "context": 102},
        "leaf_bindings": {},
        "variables": {},
        "cases": cases,
        "genesis": genesis,
        "law_kinds": law_kinds,
    });
    let mut rules_text = serde_json::to_string_pretty(&rules)
        .map_err(|error| profile("v2/policy.json", error.to_string()))?;
    rules_text.push('\n');
    Ok(AggregateSources {
        project,
        rules: rules_text,
    })
}

/// Unique declaration names: a repeated name gets its ID appended.
#[derive(Default)]
struct Names {
    used: BTreeSet<String>,
}

impl Names {
    fn unique(&mut self, name: &str, id: u32) -> String {
        let base = identifier(name);
        let chosen = if self.used.contains(&base) {
            format!("{base}_{id}")
        } else {
            base
        };
        self.used.insert(chosen.clone());
        chosen
    }
}

/// A `.zeno` identifier from free text: ASCII letters, digits and `_`,
/// starting with a letter.
fn identifier(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert_str(0, "c_");
    }
    out
}

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Accept => "Accept",
        Class::Reject => "Reject",
        Class::CommittedFailure => "CommittedFailure",
    }
}

fn scope(scope: LawScope) -> &'static str {
    match scope {
        LawScope::Always => "any",
        LawScope::Accept => "accept",
        LawScope::Reject => "reject",
        LawScope::CommittedFailure => "failure",
        LawScope::Committing => "commit",
    }
}
