//! Hierarchy, after equation 2.8 of Capucci and Myers, "Compositionality of
//! Lyapunov functions via assume-guarantee reasoning" (arXiv:2604.03017):
//! groups of leaves and groups, nested to one root, each with an outer
//! assumption and an outer guarantee.
//!
//! A group's environment inputs are the interface fields of its leaves that
//! no edge from inside the group feeds; the root's are the fields no edge
//! feeds at all. A group names each one `context.102.<id>` and each of its
//! exported views `post.100.<id>`. Its outer assumption is a list of named
//! conjuncts over its inputs and the leaves' command fields, shared by ID;
//! its outer guarantee is a list of conjuncts over its exports. Formulas
//! compile with the ordinary predicate compiler against declarations made of
//! exactly those fields, with the leaves' types.
//!
//! For each group, children before parents, the checker shows, exhaustively
//! over the domains of the fields each formula reads:
//!
//! - (i) every conjunct of every child's assumption (a leaf's assumption,
//!   split at its top-level `&&`, or a child group's outer assumption) holds
//!   wherever the provider StateFacts of the edges feeding the child from
//!   inside the group hold and the group's outer assumption holds;
//! - (ii) every conjunct of the outer guarantee holds wherever the children's
//!   guarantees on the group's exports hold: a leaf's StateFacts, a child
//!   group's outer guarantee.
//!
//! A conjunct that (i) does not discharge and that reads only the group's
//! environment inputs and command fields becomes part of the group's outer
//! assumption, so an unresolved environment assumption rises to the root
//! instead of being refused. A conjunct that also reads a field fed inside
//! the group is refused. The root's outer assumption, with every conjunct
//! that rose to it, is the condition of the application claim: none makes
//! it unconditional, conjuncts that read only environment inputs make it
//! unconditional under those named external assumptions, and a conjunct that
//! reads a command field makes it conditional. The root's assumption must
//! hold on some input; a conjunct that reads nothing therefore holds and is
//! not a condition.
//!
//! This is not the strict discharge rule of equation 2.8, under which a
//! group's inner guarantees are its children's outer guarantees. Here an
//! edge's discharge uses the provider leaf's own StateFacts at every level,
//! which hold because every leaf's invariant holds jointly; the outer
//! guarantees are checked by (ii) and reported, but discharge nothing.
//! Outer assumptions cannot read the route: which leaf a command selects.
//! A command field is one variable shared by ID, so every leaf that declares
//! an ID must give it the same type; a clash is refused.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use zeno_fcis_synthesis::finite::v2_composition as c;

use super::aggregate::same_type;
use super::leaf::{self, Compiled};
use super::manifest::{Group, GuaranteeKind, Manifest};
use super::predicate::{Form, Predicate, RootFields, Truth};
use super::refusal::{Obligation, Refusal, Stage};
use super::table::{Table, TableKind};
use crate::internals::declarations::{Declarations, Form as TypeForm, Leaf as Scalar, Source};
use crate::internals::expr::{self, Ast, Binary};
use crate::internals::review::domain::LeafDomain;

fn refuse(obligation: Obligation, place: impl Into<String>, reason: impl Into<String>) -> Refusal {
    Refusal::new(Stage::Hierarchy, obligation, place, reason)
}

fn group_place(group: &Group) -> String {
    format!("group {} `{}`", group.id, group.name)
}

/// The group tree of a `/2` manifest.
#[derive(Debug)]
pub(crate) struct Tree {
    /// The group each leaf, by manifest position, is a child of.
    pub(crate) leaf_parent: Vec<u32>,
    /// The parent of each group but the root.
    pub(crate) group_parent: BTreeMap<u32, u32>,
    /// The leaf positions under each group.
    pub(crate) subtree: BTreeMap<u32, BTreeSet<usize>>,
    /// Every group, children before their parent; the root last.
    pub(crate) order: Vec<u32>,
    pub(crate) root: u32,
}

impl Tree {
    /// The tree of a `/2` manifest; `None` for a flat one.
    ///
    /// # Errors
    /// A child that is neither a leaf nor a group, a leaf or group with no
    /// parent or two, a root with a parent, or groups in a cycle.
    pub(crate) fn of(manifest: &Manifest) -> Result<Option<Self>, Refusal> {
        let Some(hierarchy) = &manifest.hierarchy else {
            return Ok(None);
        };
        let leaf_index: BTreeMap<u32, usize> = manifest
            .leaves
            .iter()
            .enumerate()
            .map(|(position, leaf)| (leaf.id, position))
            .collect();
        let groups: BTreeMap<u32, &Group> = hierarchy
            .groups
            .iter()
            .map(|group| (group.id, group))
            .collect();
        let tree = |reason: String| refuse(Obligation::GroupTree, "groups", reason);
        let mut leaf_parent: Vec<Option<u32>> = vec![None; manifest.leaves.len()];
        let mut group_parent = BTreeMap::new();
        for group in &hierarchy.groups {
            for child in &group.children {
                if let Some(&position) = leaf_index.get(child) {
                    if leaf_parent[position].replace(group.id).is_some() {
                        return Err(tree(format!("leaf {child} is a child of two groups")));
                    }
                } else if groups.contains_key(child) {
                    if *child == hierarchy.root {
                        return Err(tree(format!(
                            "the root group {child} is a child of group {}",
                            group.id
                        )));
                    }
                    if group_parent.insert(*child, group.id).is_some() {
                        return Err(tree(format!("group {child} is a child of two groups")));
                    }
                } else {
                    return Err(tree(format!(
                        "group {} names child {child}, which is neither a leaf nor a group",
                        group.id
                    )));
                }
            }
        }
        let leaf_parent = leaf_parent
            .iter()
            .enumerate()
            .map(|(position, parent)| {
                parent.ok_or_else(|| {
                    tree(format!(
                        "leaf {} is in no group",
                        manifest.leaves[position].id
                    ))
                })
            })
            .collect::<Result<Vec<u32>, _>>()?;
        if let Some(group) = groups
            .keys()
            .find(|id| **id != hierarchy.root && !group_parent.contains_key(id))
        {
            return Err(tree(format!("group {group} is in no group")));
        }
        // Children before parents, from the root; a group outside the
        // root's tree is in a cycle of groups.
        let mut order = Vec::with_capacity(groups.len());
        let mut subtree: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        let mut stack = vec![(hierarchy.root, false)];
        while let Some((id, expanded)) = stack.pop() {
            let group = groups[&id];
            if expanded {
                let mut leaves = BTreeSet::new();
                for child in &group.children {
                    if let Some(&position) = leaf_index.get(child) {
                        leaves.insert(position);
                    } else if let Some(below) = subtree.get(child) {
                        leaves.extend(below.iter().copied());
                    }
                }
                subtree.insert(id, leaves);
                order.push(id);
            } else {
                stack.push((id, true));
                for child in &group.children {
                    if groups.contains_key(child) {
                        stack.push((*child, false));
                    }
                }
            }
        }
        if order.len() != groups.len() {
            let cycle: Vec<String> = groups
                .keys()
                .filter(|id| !subtree.contains_key(id))
                .map(u32::to_string)
                .collect();
            return Err(tree(format!(
                "groups {} form a cycle outside the root's tree",
                cycle.join(", ")
            )));
        }
        Ok(Some(Self {
            leaf_parent,
            group_parent,
            subtree,
            order,
            root: hierarchy.root,
        }))
    }

    /// Whether leaf `position` is under group `group`.
    pub(crate) fn contains(&self, group: u32, position: usize) -> bool {
        self.subtree
            .get(&group)
            .is_some_and(|leaves| leaves.contains(&position))
    }
}

/// The edge feeding each interface field: consumer position and field to
/// the provider's position, its viewed field and the facts.
pub(crate) struct Feeds<'m> {
    by_input: BTreeMap<(usize, u16), (usize, u16, &'m [u32])>,
}

impl<'m> Feeds<'m> {
    pub(crate) fn of(manifest: &'m Manifest) -> Self {
        let index: BTreeMap<u32, usize> = manifest
            .leaves
            .iter()
            .enumerate()
            .map(|(position, leaf)| (leaf.id, position))
            .collect();
        let mut by_input = BTreeMap::new();
        for edge in &manifest.edges {
            let (Some(&consumer), Some(&provider)) =
                (index.get(&edge.consumer), index.get(&edge.provider))
            else {
                continue;
            };
            let Some(view) = manifest.leaves[provider].view(edge.view) else {
                continue;
            };
            by_input.insert(
                (consumer, edge.interface),
                (provider, view.field, edge.facts.as_slice()),
            );
        }
        Self { by_input }
    }

    /// The feed of an interface field from a provider under `group`.
    pub(crate) fn inside(
        &self,
        tree: &Tree,
        group: u32,
        input: (usize, u16),
    ) -> Option<(usize, u16, &'m [u32])> {
        self.by_input
            .get(&input)
            .copied()
            .filter(|(provider, ..)| tree.contains(group, *provider))
    }
}

/// The interface fields of the leaves under `group` that no edge from under
/// the group feeds.
pub(crate) fn environment(
    compiled: &[Compiled],
    tree: &Tree,
    feeds: &Feeds<'_>,
    group: u32,
) -> BTreeSet<(usize, u16)> {
    let mut out = BTreeSet::new();
    for position in tree.subtree.get(&group).into_iter().flatten() {
        for field in compiled[*position].fields(Source::Context) {
            if feeds
                .inside(tree, group, (*position, field.field))
                .is_none()
            {
                out.insert((*position, field.field));
            }
        }
    }
    out
}

/// A value a formula reads, by its identity in the composition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Var {
    /// Leaf `position`'s interface field.
    Input(usize, u16),
    /// A command field, shared by ID.
    Command(u16),
    /// A state slot, read through a view.
    Slot(usize),
}

/// The domains and names of every variable.
struct Domains<'a> {
    manifest: &'a Manifest,
    compiled: &'a [Compiled],
    /// Each slot's owning leaf position and field.
    slots: BTreeMap<usize, (usize, u16)>,
}

impl<'a> Domains<'a> {
    fn new(manifest: &'a Manifest, compiled: &'a [Compiled]) -> Self {
        let mut slots = BTreeMap::new();
        for (position, leaf) in manifest.leaves.iter().enumerate() {
            for owned in &leaf.owns {
                slots.insert(
                    usize::try_from(owned.slot).unwrap_or(usize::MAX),
                    (position, owned.field),
                );
            }
        }
        Self {
            manifest,
            compiled,
            slots,
        }
    }

    fn slot(&self, position: usize, field: u16) -> Option<usize> {
        self.slots
            .iter()
            .find(|(_, owner)| **owner == (position, field))
            .map(|(slot, _)| *slot)
    }

    fn domain(&self, var: Var) -> Option<LeafDomain> {
        let find = |position: usize, source: Source, field: u16| {
            self.compiled[position]
                .fields(source)
                .into_iter()
                .find(|candidate| candidate.field == field)
                .map(|candidate| candidate.domain)
        };
        match var {
            Var::Input(position, field) => find(position, Source::Context, field),
            Var::Command(field) => {
                (0..self.compiled.len()).find_map(|position| find(position, Source::Command, field))
            }
            Var::Slot(slot) => {
                let (position, field) = *self.slots.get(&slot)?;
                find(position, Source::State, field)
            }
        }
    }

    fn name(&self, var: Var) -> String {
        match var {
            Var::Input(position, field) => {
                format!(
                    "leaf {} context.102.{field}",
                    self.manifest.leaves[position].id
                )
            }
            Var::Command(field) => format!("command.101.{field}"),
            Var::Slot(slot) => match self.slots.get(&slot) {
                Some((position, field)) => {
                    format!(
                        "leaf {} post.100.{field}",
                        self.manifest.leaves[*position].id
                    )
                }
                None => format!("slot {slot}"),
            },
        }
    }
}

/// One conjunct of an assumption or guarantee, with the variable each field
/// it reads stands for.
#[derive(Clone, Debug)]
struct Conjunct {
    /// Where it comes from, for a reader.
    origin: String,
    formula: String,
    form: Form,
    predicate: Predicate,
    reads: Vec<(u16, Var)>,
}

impl Conjunct {
    fn vars(&self) -> BTreeSet<Var> {
        self.reads.iter().map(|(_, var)| *var).collect()
    }

    fn truth(&self, values: &BTreeMap<Var, i128>, domains: &Domains<'_>) -> Option<Truth> {
        let mut record = Vec::with_capacity(self.reads.len());
        for (id, var) in &self.reads {
            let domain = domains.domain(*var)?;
            record.push(c::Field {
                id: *id,
                value: leaf::atom(&domain, *values.get(var)?),
            });
        }
        record.sort_by_key(|field| field.id);
        Some(match self.form {
            Form::State => self.predicate.on_state(&record),
            _ => self.predicate.on_input(&record),
        })
    }

    fn json(&self, domains: &Domains<'_>) -> Value {
        json!({
            "origin": self.origin,
            "formula": self.formula,
            "reads": self.vars().into_iter().map(|var| domains.name(var)).collect::<Vec<_>>(),
        })
    }
}

fn split<'a>(ast: &'a Ast, out: &mut Vec<&'a Ast>) {
    match ast {
        Ast::Binary(Binary::And, left, right) => {
            split(left, out);
            split(right, out);
        }
        other => out.push(other),
    }
}

/// A leaf's assumption, split at its top-level `&&`.
fn leaf_conjuncts(
    manifest: &Manifest,
    compiled: &[Compiled],
    position: usize,
) -> Result<Vec<Conjunct>, Refusal> {
    let entry = &manifest.leaves[position];
    let leaf = &compiled[position];
    let place = format!("leaf {} `{}` assumption", entry.id, entry.name);
    let ast = expr::parse(&entry.assumption)
        .map_err(|reason| refuse(Obligation::DefinedPredicate, &place, reason))?;
    let mut parts = Vec::new();
    split(&ast, &mut parts);
    let ids = |source: Source| -> Vec<u16> {
        leaf.fields(source)
            .iter()
            .map(|field| field.field)
            .collect()
    };
    let (state, command, context) = (
        ids(Source::State),
        ids(Source::Command),
        ids(Source::Context),
    );
    let roots = RootFields {
        state: &state,
        command: &command,
        context: &context,
    };
    let count = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            let formula = expr::render(part);
            let predicate = Predicate::compile(
                &leaf.declarations,
                &leaf.rules.variables,
                &formula,
                Form::Input,
                &roots,
            )
            .map_err(|reason| refuse(Obligation::DefinedPredicate, &place, reason))?;
            let reads = predicate
                .fields()
                .iter()
                .map(|field| {
                    let var = if context.contains(field) {
                        Var::Input(position, *field)
                    } else {
                        Var::Command(*field)
                    };
                    (*field, var)
                })
                .collect();
            Ok(Conjunct {
                origin: if count > 1 {
                    format!("{place} conjunct {index}")
                } else {
                    place.clone()
                },
                formula,
                form: Form::Input,
                predicate,
                reads,
            })
        })
        .collect()
}

/// A group's formulas compile against declarations of exactly its fields:
/// its exports in the state root, the leaves' command fields in the command
/// root and its inputs in the context root, each with its leaf's type. A
/// declaration's field IDs are unique across its roots, so exports and
/// inputs take fresh IDs above the command fields, and a formula's
/// `post.100.<export>` and `context.102.<input>` are renamed to them.
struct Scope {
    declarations: Declarations,
    exports: Vec<u16>,
    command: Vec<u16>,
    inputs: Vec<u16>,
    /// The declared field of each export and input ID.
    export_field: BTreeMap<u16, u16>,
    input_field: BTreeMap<u16, u16>,
    /// What each declared field stands for.
    export_var: BTreeMap<u16, Var>,
    input_var: BTreeMap<u16, Var>,
}

impl Scope {
    fn of(
        manifest: &Manifest,
        compiled: &[Compiled],
        tree: &Tree,
        group: &Group,
        domains: &Domains<'_>,
    ) -> Result<Self, Refusal> {
        let place = group_place(group);
        let formula_error = |reason: String| refuse(Obligation::GroupFormula, &place, reason);
        let leaf_index: BTreeMap<u32, usize> = manifest
            .leaves
            .iter()
            .enumerate()
            .map(|(position, leaf)| (leaf.id, position))
            .collect();
        // Each field: (root type, ID, name, leaf position, leaf type ID).
        let mut fields: Vec<(u32, u16, String, usize, u32)> = Vec::new();
        let declared_type = |position: usize, root: u32, field: u16| -> Option<u32> {
            compiled[position]
                .declarations
                .fields(root)
                .ok()?
                .iter()
                .find(|candidate| candidate.id == field)
                .map(|candidate| candidate.type_id)
        };
        let mut command: BTreeMap<u16, (String, usize, u32)> = BTreeMap::new();
        for position in tree.subtree.get(&group.id).into_iter().flatten() {
            for field in compiled[*position]
                .declarations
                .fields(101)
                .map_err(|error| formula_error(error.reason().to_owned()))?
            {
                command
                    .entry(field.id)
                    .or_insert((field.name.clone(), *position, field.type_id));
            }
        }
        for (id, (name, position, type_id)) in &command {
            fields.push((101, *id, name.clone(), *position, *type_id));
        }
        let mut next = command.keys().next_back().copied().unwrap_or(0);
        let mut fresh = || -> Result<u16, Refusal> {
            next = next.checked_add(1).ok_or_else(|| {
                refuse(
                    Obligation::GroupFormula,
                    group_place(group),
                    "too many fields for one group",
                )
            })?;
            Ok(next)
        };
        let mut export_var = BTreeMap::new();
        let mut export_field = BTreeMap::new();
        for export in &group.exports {
            let position = leaf_index
                .get(&export.leaf)
                .copied()
                .filter(|position| tree.contains(group.id, *position))
                .ok_or_else(|| {
                    refuse(
                        Obligation::GroupInterface,
                        &place,
                        format!(
                            "export {} names leaf {}, which is not under the group",
                            export.id, export.leaf
                        ),
                    )
                })?;
            let view = manifest.leaves[position].view(export.view).ok_or_else(|| {
                refuse(
                    Obligation::GroupInterface,
                    &place,
                    format!(
                        "export {} names view {} of leaf {}, which it does not declare",
                        export.id, export.view, export.leaf
                    ),
                )
            })?;
            let type_id = declared_type(position, 100, view.field)
                .ok_or_else(|| formula_error(format!("export {} has no state field", export.id)))?;
            let slot = domains.slot(position, view.field).ok_or_else(|| {
                formula_error(format!("export {} names an unowned field", export.id))
            })?;
            let field = fresh()?;
            fields.push((
                100,
                field,
                format!("export_{}", export.id),
                position,
                type_id,
            ));
            export_var.insert(field, Var::Slot(slot));
            export_field.insert(export.id, field);
        }
        let mut input_var = BTreeMap::new();
        let mut input_field = BTreeMap::new();
        for input in &group.inputs {
            let position = leaf_index.get(&input.leaf).copied().ok_or_else(|| {
                refuse(
                    Obligation::GroupInterface,
                    &place,
                    format!(
                        "input {} names leaf {}, which the manifest does not list",
                        input.id, input.leaf
                    ),
                )
            })?;
            let type_id = declared_type(position, 102, input.field).ok_or_else(|| {
                refuse(
                    Obligation::GroupInterface,
                    &place,
                    format!(
                        "input {} names field {} of leaf {}, which is not an interface field",
                        input.id, input.field, input.leaf
                    ),
                )
            })?;
            let field = fresh()?;
            fields.push((102, field, format!("input_{}", input.id), position, type_id));
            input_var.insert(field, Var::Input(position, input.field));
            input_field.insert(input.id, field);
        }
        // Types by ID, from the leaf that first uses each; one ID, one form.
        let mut types: BTreeMap<u32, (usize, String)> = BTreeMap::new();
        for (_, _, _, position, type_id) in &fields {
            let declared = compiled[*position]
                .declarations
                .get(*type_id)
                .map_err(|error| formula_error(error.reason().to_owned()))?;
            let key = form_text(*type_id, &declared.name, &declared.form).ok_or_else(|| {
                formula_error(format!("type {type_id} is not a bool, int or data type"))
            })?;
            match types.get(type_id) {
                Some((_, existing)) if *existing != key => {
                    return Err(formula_error(format!(
                        "two leaves declare type {type_id} differently"
                    )));
                }
                Some(_) => {}
                None => {
                    types.insert(*type_id, (*position, key));
                }
            }
        }
        let unit_type = types.keys().next_back().copied().unwrap_or(102).max(102) + 1;
        let mut project = format!(
            "zeno 1;\nproject 1 group_{};\nnamespace 2 hierarchy;\ntype 100 state GroupExports;\ntype 101 command GroupCommand;\ntype 102 context GroupInputs;\n",
            group.id
        );
        let mut variants = String::new();
        for (type_id, (position, _)) in &types {
            let declared = compiled[*position]
                .declarations
                .get(*type_id)
                .map_err(|error| formula_error(error.reason().to_owned()))?;
            let name = format!("{}_{type_id}", declared.name);
            match &declared.form {
                TypeForm::Leaf(Scalar::Bool) => {
                    project.push_str(&format!("type {type_id} bool {name};\n"))
                }
                TypeForm::Leaf(Scalar::I128 { min, max }) => {
                    project.push_str(&format!("type {type_id} int {name} in {min}..={max};\n"));
                }
                TypeForm::Sum(declared_variants) => {
                    project.push_str(&format!("type {type_id} data {name};\n"));
                    for variant in declared_variants {
                        variants.push_str(&format!(
                            "variant {} {type_id} {}_{} none;\n",
                            variant.id, variant.name, variant.id
                        ));
                    }
                }
                _ => {
                    return Err(formula_error(format!(
                        "type {type_id} is not a bool, int or data type"
                    )));
                }
            }
        }
        project.push_str(&format!("type {unit_type} int GroupUnit in 0..=0;\n"));
        let mut used: BTreeMap<u32, BTreeSet<u16>> = BTreeMap::new();
        for (root, id, name, _, type_id) in &fields {
            used.entry(*root).or_default().insert(*id);
            project.push_str(&format!("field {id} {root} {name}_{root} {type_id};\n"));
        }
        // An empty root still needs one field; no formula may read it.
        for root in [100u32, 101, 102] {
            if used.get(&root).is_none_or(BTreeSet::is_empty) {
                let taken: BTreeSet<u16> = used.values().flatten().copied().collect();
                let id = (1..=u16::MAX)
                    .find(|id| !taken.contains(id))
                    .unwrap_or(u16::MAX);
                project.push_str(&format!("field {id} {root} unit_{root} {unit_type};\n"));
                used.entry(root).or_default().insert(id);
            }
        }
        project.push_str(&variants);
        project.push_str(
            "reason 300 unused precedence 0;\ncomponent 400 group {\n  owns 100;\n  reads pre.100;\n  writes post.100;\n  contexts context.102;\n  budget steps 4096;\n}\nmerge [400];\n",
        );
        let declarations = Declarations::read(&project, &BTreeMap::new())
            .map_err(|error| formula_error(error.reason().to_owned()))?;
        Ok(Self {
            declarations,
            exports: export_var.keys().copied().collect(),
            command: command.keys().copied().collect(),
            inputs: input_var.keys().copied().collect(),
            export_field,
            input_field,
            export_var,
            input_var,
        })
    }

    /// A formula's names with exports and inputs renamed to their fields.
    fn renamed(&self, ast: &Ast) -> Ast {
        let recurse = |ast: &Ast| Box::new(self.renamed(ast));
        match ast {
            Ast::U128(v) => Ast::U128(*v),
            Ast::ToI128(v) => Ast::ToI128(recurse(v)),
            Ast::ToU128(v) => Ast::ToU128(recurse(v)),
            Ast::Name(name) => {
                let field = |prefix: &str, map: &BTreeMap<u16, u16>| {
                    name.strip_prefix(prefix)
                        .and_then(|rest| rest.parse::<u16>().ok())
                        .and_then(|id| map.get(&id))
                        .map(|field| format!("{prefix}{field}"))
                };
                Ast::Name(
                    field("post.100.", &self.export_field)
                        .or_else(|| field("context.102.", &self.input_field))
                        .unwrap_or_else(|| name.clone()),
                )
            }
            Ast::Bool(_) | Ast::Int(_) | Ast::DecisionReason => ast.clone(),
            Ast::Not(inner) => Ast::Not(recurse(inner)),
            Ast::Neg(inner) => Ast::Neg(recurse(inner)),
            Ast::Binary(operator, left, right) => {
                Ast::Binary(*operator, recurse(left), recurse(right))
            }
            Ast::Choose(condition, then, otherwise) => {
                Ast::Choose(recurse(condition), recurse(then), recurse(otherwise))
            }
            Ast::Div(rounding, left, right) => Ast::Div(*rounding, recurse(left), recurse(right)),
            Ast::MulDiv(r, a, b, d) => Ast::MulDiv(*r, recurse(a), recurse(b), recurse(d)),
            Ast::ScaledMulDiv(r, signed, a, b, d, scale) => {
                Ast::ScaledMulDiv(*r, *signed, recurse(a), recurse(b), recurse(d), *scale)
            }
            Ast::RepeatMulDiv(signed, a, b, d, count) => {
                Ast::RepeatMulDiv(*signed, recurse(a), recurse(b), recurse(d), *count)
            }
            Ast::SignedMulDivFloor(a, b, d) => {
                Ast::SignedMulDivFloor(recurse(a), recurse(b), recurse(d))
            }
            Ast::ProductsGe(arguments) => {
                Ast::ProductsGe(arguments.iter().map(|a| *recurse(a)).collect())
            }
        }
    }

    fn compile(
        &self,
        origin: String,
        formula: &str,
        form: Form,
        place: &str,
    ) -> Result<Conjunct, Refusal> {
        let roots = RootFields {
            state: &self.exports,
            command: &self.command,
            context: &self.inputs,
        };
        let ast = expr::parse(formula)
            .map_err(|reason| refuse(Obligation::GroupFormula, place, reason))?;
        let renamed = expr::render(&self.renamed(&ast));
        let predicate =
            Predicate::compile(&self.declarations, &BTreeMap::new(), &renamed, form, &roots)
                .map_err(|reason| refuse(Obligation::GroupFormula, place, reason))?;
        let reads = predicate
            .fields()
            .iter()
            .map(|field| {
                let var = match form {
                    Form::State => self.export_var.get(field).copied(),
                    _ => self
                        .input_var
                        .get(field)
                        .copied()
                        .or(Some(Var::Command(*field))),
                };
                var.map(|var| (*field, var)).ok_or_else(|| {
                    refuse(
                        Obligation::GroupFormula,
                        place,
                        format!("reads field {field}, which the group does not export"),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Conjunct {
            origin,
            formula: formula.to_owned(),
            form,
            predicate,
            reads,
        })
    }
}

/// A type's declaration form, comparable across leaves.
fn form_text(type_id: u32, name: &str, form: &TypeForm) -> Option<String> {
    Some(match form {
        TypeForm::Leaf(Scalar::Bool) => format!("{type_id} {name} bool"),
        TypeForm::Leaf(Scalar::I128 { min, max }) => format!("{type_id} {name} int {min} {max}"),
        TypeForm::Sum(variants) => format!(
            "{type_id} {name} data {}",
            variants
                .iter()
                .map(|variant| format!("{}:{}", variant.id, variant.name))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => return None,
    })
}

/// How a group's check left a child's conjunct.
enum Discharge {
    /// It holds wherever the inner guarantees and the outer assumption hold.
    Discharged,
    /// It reads only the group's environment inputs and command fields, and
    /// becomes part of the group's outer assumption.
    Lifted,
}

/// The rows of one implication, and the first row where it fails.
struct Rows {
    count: u64,
    failure: Option<Value>,
}

/// Over every value of `target`'s variables and the premises sharing a
/// variable with them, wherever every premise and every fact holds, the
/// target holds. Each row goes into each table.
fn implication(
    target: &Conjunct,
    premises: &[&Conjunct],
    facts: &dyn Fn(Var) -> Option<(usize, u16, Vec<u32>)>,
    compiled: &[Compiled],
    domains: &Domains<'_>,
    tables: &mut [&mut Table],
    place: &str,
) -> Result<Rows, Refusal> {
    let mut vars = target.vars();
    let mut used: Vec<&Conjunct> = Vec::new();
    loop {
        let before = used.len();
        for premise in premises {
            if !used.iter().any(|seen| std::ptr::eq(*seen, *premise))
                && premise.vars().iter().any(|var| vars.contains(var))
            {
                vars.extend(premise.vars());
                used.push(premise);
            }
        }
        if used.len() == before {
            break;
        }
    }
    let ordered: Vec<Var> = vars.into_iter().collect();
    let too_large = || {
        refuse(
            Obligation::CompleteEnumeration,
            place,
            "the table exceeds the row limit",
        )
    };
    let mut values = Vec::with_capacity(ordered.len());
    for var in &ordered {
        let domain = domains.domain(*var).ok_or_else(|| {
            refuse(
                Obligation::GroupFormula,
                place,
                format!("{} has no domain", domains.name(*var)),
            )
        })?;
        values.push(
            domain
                .values_within(leaf::MAX_LOCAL_ROWS)
                .ok_or_else(too_large)?,
        );
    }
    values
        .iter()
        .try_fold(1u64, |size, values| size.checked_mul(values.len() as u64))
        .filter(|size| *size <= leaf::MAX_LOCAL_ROWS)
        .ok_or_else(too_large)?;
    let defined = |truth: Option<Truth>, what: &str| -> Result<bool, Refusal> {
        match truth {
            Some(Truth::Holds) => Ok(true),
            Some(Truth::Fails) => Ok(false),
            Some(other) => Err(refuse(
                Obligation::DefinedPredicate,
                place,
                format!("{what} {}", other.describe()),
            )),
            None => Err(refuse(
                Obligation::DefinedPredicate,
                place,
                format!("{what} reads a value with no domain"),
            )),
        }
    };
    let mut rows = Rows {
        count: 0,
        failure: None,
    };
    leaf::for_each_tuple(&values, |tuple| {
        rows.count += 1;
        let row: BTreeMap<Var, i128> = ordered.iter().copied().zip(tuple.iter().copied()).collect();
        let mut premise = true;
        for var in &ordered {
            let Some((provider, view_field, ids)) = facts(*var) else {
                continue;
            };
            let Some(domain) = domains.domain(*var) else {
                continue;
            };
            let record = [c::Field {
                id: view_field,
                value: leaf::atom(&domain, row[var]),
            }];
            for id in &ids {
                let Some((GuaranteeKind::StateFact, predicate)) = compiled[provider].guarantee(*id)
                else {
                    return Err(refuse(
                        Obligation::ProviderClaim,
                        place,
                        format!("fact {id} is not a StateFact"),
                    ));
                };
                premise &= defined(Some(predicate.on_state(&record)), &format!("fact {id}"))?;
            }
        }
        for conjunct in &used {
            premise &= defined(conjunct.truth(&row, domains), &conjunct.origin)?;
        }
        let holds = !premise || defined(target.truth(&row, domains), &target.origin)?;
        for table in tables.iter_mut() {
            let record = table.row(tuple);
            record.push(u8::from(premise));
            record.push(u8::from(holds));
        }
        if !holds && rows.failure.is_none() {
            rows.failure = Some(Value::Object(
                ordered
                    .iter()
                    .zip(tuple)
                    .map(|(var, value)| (domains.name(*var), super::json_integer(*value)))
                    .collect::<Map<String, Value>>(),
            ));
        }
        Ok(())
    })?;
    Ok(rows)
}

/// What the hierarchy check established for one group.
#[derive(Debug)]
pub(crate) struct GroupResult {
    id: u32,
    name: String,
    children: Vec<u32>,
    inputs: usize,
    exports: usize,
    discharged: usize,
    lifted: Vec<Value>,
    assumption: Vec<Value>,
    guarantee: Vec<Value>,
    assumption_rows: u64,
    assumption_table_sha256: [u8; 32],
    guarantee_rows: u64,
    guarantee_table_sha256: [u8; 32],
}

/// The kind of a checked composition's application claim, decided by the
/// root group's outer assumption with every conjunct that rose to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ClaimKind {
    /// The root assumes nothing: the claim holds for every history from a
    /// checked genesis. A flat manifest's claim is of this kind.
    Unconditional,
    /// Every condition reads only environment inputs: the claim holds for
    /// the histories whose supplied context satisfies them.
    External,
    /// A condition reads a command field: the claim holds only for the
    /// histories whose commands and context satisfy the conditions.
    Conditional,
}

impl ClaimKind {
    /// The kind's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unconditional => "unconditional",
            Self::External => "unconditional under named external assumptions",
            Self::Conditional => "conditional",
        }
    }
}

/// One condition of an application claim: a conjunct of the root group's
/// outer assumption, authored there or risen from a child.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Condition {
    origin: String,
    formula: String,
    reads: Vec<String>,
}

impl Condition {
    /// Where the conjunct comes from: the group and assumption name, or the
    /// leaf whose assumption rose to the root.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The conjunct's formula, in the language of its origin.
    #[must_use]
    pub fn formula(&self) -> &str {
        &self.formula
    }

    /// The values it reads: leaf interface fields and command fields.
    #[must_use]
    pub fn reads(&self) -> &[String] {
        &self.reads
    }

    pub(crate) fn json(&self) -> Value {
        json!({"origin": self.origin, "formula": self.formula, "reads": self.reads})
    }
}

/// What the hierarchy check established.
#[derive(Debug)]
pub(crate) struct Checked {
    groups: Vec<GroupResult>,
    root: u32,
    pub(crate) claim: ClaimKind,
    pub(crate) conditions: Vec<Condition>,
    /// Each leaf's assumption rows at its parent, and their table's SHA-256.
    pub(crate) leaf_rows: Vec<(u64, [u8; 32])>,
}

impl Checked {
    pub(crate) fn json(&self) -> Value {
        json!({
            "root": self.root,
            "claim": {
                "kind": self.claim.name(),
                "conditions": self.conditions.iter().map(Condition::json).collect::<Vec<_>>(),
            },
            "groups": self.groups.iter().map(|group| json!({
                "id": group.id,
                "name": group.name,
                "children": group.children,
                "inputs": group.inputs,
                "exports": group.exports,
                "assumption": group.assumption,
                "guarantee": group.guarantee,
                "discharged": group.discharged,
                "lifted": group.lifted,
                "tables": {
                    "assumption_rows": group.assumption_rows,
                    "assumption_sha256": super::hex(&group.assumption_table_sha256),
                    "guarantee_rows": group.guarantee_rows,
                    "guarantee_sha256": super::hex(&group.guarantee_table_sha256),
                },
            })).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn rows(&self) -> (u64, u64) {
        self.groups
            .iter()
            .fold((0, 0), |(assumption, guarantee), group| {
                (
                    assumption + group.assumption_rows,
                    guarantee + group.guarantee_rows,
                )
            })
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        let (assumption, guarantee) = self.rows();
        vec![
            format!(
                "groups {}: {assumption} outer-assumption rows and {guarantee} outer-guarantee rows checked",
                self.groups.len()
            ),
            format!("application claim: {}", self.claim.name()),
        ]
    }
}

/// Runs the checks of equation 2.8 on every group, children first.
///
/// # Errors
/// A group whose inputs are not its environment inputs, an export that is
/// not a view under it, a formula that does not compile, an inner
/// assumption the group does not discharge or lift, an outer guarantee its
/// children's guarantees do not imply, or a root assumption that holds on
/// no input.
pub(crate) fn check(
    manifest: &Manifest,
    compiled: &[Compiled],
    tree: &Tree,
) -> Result<Checked, Refusal> {
    let Some(hierarchy) = &manifest.hierarchy else {
        return Err(refuse(
            Obligation::GroupTree,
            "groups",
            "the manifest lists no groups",
        ));
    };
    let groups: BTreeMap<u32, &Group> = hierarchy
        .groups
        .iter()
        .map(|group| (group.id, group))
        .collect();
    let leaf_index: BTreeMap<u32, usize> = manifest
        .leaves
        .iter()
        .enumerate()
        .map(|(position, leaf)| (leaf.id, position))
        .collect();
    shared_command_fields(manifest, compiled)?;
    let domains = Domains::new(manifest, compiled);
    let feeds = Feeds::of(manifest);
    let leaf_conjuncts = (0..manifest.leaves.len())
        .map(|position| leaf_conjuncts(manifest, compiled, position))
        .collect::<Result<Vec<_>, _>>()?;
    let mut leaf_rows = vec![(0u64, [0u8; 32]); manifest.leaves.len()];
    let mut effective: BTreeMap<u32, Vec<Conjunct>> = BTreeMap::new();
    let mut outer: BTreeMap<u32, Vec<Conjunct>> = BTreeMap::new();
    let mut results = Vec::with_capacity(groups.len());
    for id in &tree.order {
        let group = groups[id];
        let place = group_place(group);
        // Its inputs are exactly its environment inputs.
        let environment = environment(compiled, tree, &feeds, group.id);
        let declared: BTreeSet<(usize, u16)> = group
            .inputs
            .iter()
            .filter_map(|input| {
                leaf_index
                    .get(&input.leaf)
                    .map(|position| (*position, input.field))
            })
            .collect();
        if declared.len() != group.inputs.len() || declared != environment {
            let name = |set: &BTreeSet<(usize, u16)>| {
                set.iter()
                    .map(|(position, field)| domains.name(Var::Input(*position, *field)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(refuse(
                Obligation::GroupInterface,
                &place,
                format!(
                    "its inputs must be exactly the interface fields no edge from inside it feeds: [{}]; it lists [{}]",
                    name(&environment),
                    name(&declared)
                ),
            ));
        }
        let scope = Scope::of(manifest, compiled, tree, group, &domains)?;
        let authored = group
            .assumption
            .iter()
            .map(|named| {
                scope.compile(
                    format!("{place} assumption `{}`", named.name),
                    &named.formula,
                    Form::Input,
                    &format!("{place} assumption `{}`", named.name),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let guarantee = group
            .guarantee
            .iter()
            .map(|declared| {
                let at = format!("{place} guarantee {}", declared.id);
                scope.compile(at.clone(), &declared.formula, Form::State, &at)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let premises: Vec<&Conjunct> = authored.iter().collect();
        let facts = |var: Var| match var {
            Var::Input(position, field) => feeds
                .inside(tree, group.id, (position, field))
                .map(|(provider, view_field, ids)| (provider, view_field, ids.to_vec())),
            _ => None,
        };
        // (i) every child's assumption.
        let mut table = Table::new(TableKind::GroupAssumption);
        let mut assumption_rows = 0u64;
        let mut discharged = 0usize;
        let mut lifted: Vec<Conjunct> = Vec::new();
        for child in &group.children {
            let (conjuncts, position) = match leaf_index.get(child) {
                Some(position) => (leaf_conjuncts[*position].clone(), Some(*position)),
                None => (effective.get(child).cloned().unwrap_or_default(), None),
            };
            let mut leaf_table = Table::new(TableKind::Interface);
            let mut leaf_count = 0u64;
            for conjunct in conjuncts {
                let at = format!("{place}: {}", conjunct.origin);
                let rows = {
                    let mut tables: Vec<&mut Table> = vec![&mut table];
                    if position.is_some() {
                        tables.push(&mut leaf_table);
                    }
                    implication(
                        &conjunct,
                        &premises,
                        &facts,
                        compiled,
                        &domains,
                        &mut tables,
                        &at,
                    )?
                };
                assumption_rows += rows.count;
                leaf_count += rows.count;
                let outcome = match rows.failure {
                    None => Discharge::Discharged,
                    Some(witness) => {
                        let internal = conjunct.vars().into_iter().any(|var| facts(var).is_some());
                        if internal {
                            return Err(refuse(
                                Obligation::OuterAssumption,
                                at,
                                "the inner guarantees and the group's outer assumption hold and the child's assumption fails; it reads a field fed inside the group, so it cannot rise",
                            )
                            .with_witness(witness));
                        }
                        Discharge::Lifted
                    }
                };
                match outcome {
                    Discharge::Discharged => discharged += 1,
                    Discharge::Lifted => lifted.push(conjunct),
                }
            }
            if let Some(position) = position {
                leaf_rows[position] = (leaf_count, leaf_table.digest());
            }
        }
        // (ii) the outer guarantee from the children's guarantees.
        let exported: BTreeSet<Var> = guarantee_exports(&scope);
        let mut inner: Vec<Conjunct> = Vec::new();
        for child in &group.children {
            if let Some(position) = leaf_index.get(child) {
                inner.extend(state_facts(manifest, compiled, &domains, *position));
            } else {
                inner.extend(outer.get(child).cloned().unwrap_or_default());
            }
        }
        inner.retain(|conjunct| conjunct.vars().is_subset(&exported));
        let inner_premises: Vec<&Conjunct> = inner.iter().collect();
        let mut guarantee_table = Table::new(TableKind::GroupGuarantee);
        let mut guarantee_rows = 0u64;
        for conjunct in &guarantee {
            let rows = implication(
                conjunct,
                &inner_premises,
                &|_| None,
                compiled,
                &domains,
                &mut [&mut guarantee_table],
                &conjunct.origin,
            )?;
            guarantee_rows += rows.count;
            if let Some(witness) = rows.failure {
                return Err(refuse(
                    Obligation::OuterGuarantee,
                    &conjunct.origin,
                    "the children's guarantees on the group's exports hold and the outer guarantee fails",
                )
                .with_witness(witness));
            }
        }
        results.push(GroupResult {
            id: group.id,
            name: group.name.clone(),
            children: group.children.clone(),
            inputs: group.inputs.len(),
            exports: group.exports.len(),
            discharged,
            lifted: lifted
                .iter()
                .map(|conjunct| conjunct.json(&domains))
                .collect(),
            assumption: authored
                .iter()
                .map(|conjunct| conjunct.json(&domains))
                .collect(),
            guarantee: guarantee
                .iter()
                .map(|conjunct| conjunct.json(&domains))
                .collect(),
            assumption_rows,
            assumption_table_sha256: table.digest(),
            guarantee_rows,
            guarantee_table_sha256: guarantee_table.digest(),
        });
        let mut assumption = authored;
        assumption.extend(lifted);
        effective.insert(group.id, assumption);
        outer.insert(group.id, guarantee);
    }
    let conditions = effective.remove(&tree.root).unwrap_or_default();
    satisfiable(&conditions, &domains)?;
    // A conjunct that reads nothing holds (the root assumption is
    // satisfiable), so it conditions nothing.
    let conditions: Vec<Conjunct> = conditions
        .into_iter()
        .filter(|conjunct| !conjunct.vars().is_empty())
        .collect();
    let claim = if conditions.is_empty() {
        ClaimKind::Unconditional
    } else if conditions.iter().all(|conjunct| {
        conjunct
            .vars()
            .iter()
            .all(|var| matches!(var, Var::Input(..)))
    }) {
        ClaimKind::External
    } else {
        ClaimKind::Conditional
    };
    Ok(Checked {
        groups: results,
        root: tree.root,
        claim,
        conditions: conditions
            .iter()
            .map(|conjunct| Condition {
                origin: conjunct.origin.clone(),
                formula: conjunct.formula.clone(),
                reads: conjunct
                    .vars()
                    .into_iter()
                    .map(|var| domains.name(var))
                    .collect(),
            })
            .collect(),
        leaf_rows,
    })
}

/// A command field is one variable of the group formulas, shared by ID, so
/// every leaf that declares an ID must give it the same type.
fn shared_command_fields(manifest: &Manifest, compiled: &[Compiled]) -> Result<(), Refusal> {
    let mut first: BTreeMap<u16, (usize, u32)> = BTreeMap::new();
    for (position, leaf) in compiled.iter().enumerate() {
        let fields = leaf.declarations.fields(101).map_err(|error| {
            refuse(
                Obligation::SharedCommandField,
                "command",
                error.reason().to_owned(),
            )
        })?;
        for field in fields {
            match first.get(&field.id) {
                Some((earlier, type_id)) if !same_type(compiled, *type_id, field.type_id) => {
                    return Err(refuse(
                        Obligation::SharedCommandField,
                        format!("command.101.{}", field.id),
                        format!(
                            "leaf {} and leaf {} declare it with different types; a group formula reads one variable for it",
                            manifest.leaves[*earlier].id, manifest.leaves[position].id
                        ),
                    ));
                }
                Some(_) => {}
                None => {
                    first.insert(field.id, (position, field.type_id));
                }
            }
        }
    }
    Ok(())
}

/// The slots a group exports.
fn guarantee_exports(scope: &Scope) -> BTreeSet<Var> {
    scope.export_var.values().copied().collect()
}

/// A leaf's StateFacts, each over the slots of the fields it reads.
fn state_facts(
    manifest: &Manifest,
    compiled: &[Compiled],
    domains: &Domains<'_>,
    position: usize,
) -> Vec<Conjunct> {
    let entry = &manifest.leaves[position];
    compiled[position]
        .guarantees
        .iter()
        .filter(|(_, kind, _)| *kind == GuaranteeKind::StateFact)
        .filter_map(|(id, _, predicate)| {
            let reads = predicate
                .fields()
                .iter()
                .map(|field| {
                    domains
                        .slot(position, *field)
                        .map(|slot| (*field, Var::Slot(slot)))
                })
                .collect::<Option<Vec<_>>>()?;
            let formula = entry
                .guarantees
                .iter()
                .find(|guarantee| guarantee.id == *id)
                .map(|guarantee| guarantee.formula.clone())
                .unwrap_or_default();
            Some(Conjunct {
                origin: format!("leaf {} `{}` StateFact {id}", entry.id, entry.name),
                formula,
                form: Form::State,
                predicate: predicate.clone(),
                reads,
            })
        })
        .collect()
}

/// The root's outer assumption holds on some input, so a claim under it
/// is not vacuous.
fn satisfiable(conditions: &[Conjunct], domains: &Domains<'_>) -> Result<(), Refusal> {
    let vars: Vec<Var> = conditions
        .iter()
        .flat_map(Conjunct::vars)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let place = "root outer assumption";
    let too_large = || {
        refuse(
            Obligation::CompleteEnumeration,
            place,
            "the table exceeds the row limit",
        )
    };
    let mut values = Vec::with_capacity(vars.len());
    for var in &vars {
        let domain = domains.domain(*var).ok_or_else(|| {
            refuse(
                Obligation::GroupFormula,
                place,
                format!("{} has no domain", domains.name(*var)),
            )
        })?;
        values.push(
            domain
                .values_within(leaf::MAX_LOCAL_ROWS)
                .ok_or_else(too_large)?,
        );
    }
    values
        .iter()
        .try_fold(1u64, |size, values| size.checked_mul(values.len() as u64))
        .filter(|size| *size <= leaf::MAX_LOCAL_ROWS)
        .ok_or_else(too_large)?;
    let mut found = false;
    leaf::for_each_tuple(&values, |tuple| {
        if found {
            return Ok(());
        }
        let row: BTreeMap<Var, i128> = vars.iter().copied().zip(tuple.iter().copied()).collect();
        found = conditions
            .iter()
            .all(|conjunct| conjunct.truth(&row, domains) == Some(Truth::Holds));
        Ok::<(), Refusal>(())
    })?;
    if found {
        Ok(())
    } else {
        Err(refuse(
            Obligation::OuterAssumption,
            place,
            "holds on no input, so every claim under it would be vacuous",
        ))
    }
}
