//! The checked-composition manifest: a closed JSON form, its canonical bytes
//! and their commitment.
//!
//! `zeno-fcis/checked-composition/1` lists leaves and the edges between
//! them. `zeno-fcis/checked-composition/2` adds groups, each with children,
//! environment inputs, exports, an outer assumption and an outer guarantee,
//! and names the root group; a `/1` manifest is the flat case.
//!
//! Every key is required and no other key is admitted. Leaves are listed in
//! strictly increasing ID order and edges in strictly increasing (consumer,
//! interface field) order; groups, children, inputs, exports, assumption
//! names and guarantee IDs are each strictly increasing. One manifest has
//! exactly one canonical form. The canonical bytes cover every field and
//! begin with their own tag and version; the commitment is their SHA-256.

use serde::Deserialize;

use super::leaf::sha256;
use super::refusal::{Obligation, Refusal, Stage};

/// The flat manifest schema this checker reads.
pub const MANIFEST_SCHEMA: &str = "zeno-fcis/checked-composition/1";
/// The hierarchical manifest schema this checker reads.
pub const HIERARCHY_SCHEMA: &str = "zeno-fcis/checked-composition/2";
/// The check algorithm a manifest asks for; any other is refused.
pub const CHECKER: &str = "zeno-fcis/checked-composition-checker/1";
/// The composed-domain manifest schema, which [`super::domain`] reads.
pub const DOMAIN_SCHEMA: &str = "zeno-fcis/checked-composition/3";
const MAGIC: &[u8] = b"ZFCIS-CHECKED-COMPOSITION\0";
/// Leaves, edges and groups one manifest may name.
const MAX_LEAVES: usize = 4_096;
const MAX_EDGES: usize = 16_384;
const MAX_GROUPS: usize = 4_096;

/// Whether a guarantee is a fact about one state, which can discharge a
/// peer's pre-state assumption, or a relation over one transition, which
/// cannot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GuaranteeKind {
    StateFact,
    TransitionRelation,
}

/// The decisions a transition relation applies to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Scope {
    Accept,
    Reject,
    Commit,
    Any,
}

impl GuaranteeKind {
    /// The kind's byte in canonical bytes and claims.
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::StateFact => 0,
            Self::TransitionRelation => 1,
        }
    }
}

/// A scope's byte in canonical bytes and claims; 0 for none.
pub(crate) const fn scope_code(scope: Option<Scope>) -> u8 {
    match scope {
        None => 0,
        Some(Scope::Accept) => 1,
        Some(Scope::Reject) => 2,
        Some(Scope::Commit) => 3,
        Some(Scope::Any) => 4,
    }
}

/// A decision class a leaf must show on an admitted input.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Witness {
    Accept,
    Reject,
}

impl Witness {
    /// The decision class's name.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Reject => "Reject",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Guarantee {
    pub(crate) id: u32,
    pub(crate) kind: GuaranteeKind,
    /// `Some` exactly for a transition relation.
    pub(crate) scope: Option<Scope>,
    pub(crate) formula: String,
}

/// One state field a leaf exports to its peers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    pub(crate) id: u32,
    pub(crate) field: u16,
}

/// One state field and the global slot it occupies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Owned {
    pub(crate) field: u16,
    pub(crate) slot: u32,
}

/// One input a leaf may read: `pre.100.f`, `command.101.f` or `context.102.f`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReadPath {
    /// 0 for the state, 1 for the command and 2 for the context, as the
    /// library numbers its sources.
    pub(crate) source: u8,
    pub(crate) field: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct Leaf {
    pub(crate) id: u32,
    pub(crate) name: String,
    /// Directory holding `project.zeno` and `v2/policy.json`, relative to the
    /// manifest.
    pub(crate) path: String,
    pub(crate) project_sha256: [u8; 32],
    pub(crate) policy_sha256: [u8; 32],
    /// SHA-256 of the bound program's identity bytes.
    pub(crate) program_sha256: [u8; 32],
    pub(crate) owns: Vec<Owned>,
    pub(crate) reads: Vec<ReadPath>,
    pub(crate) writes: Vec<u16>,
    pub(crate) invariant: String,
    pub(crate) assumption: String,
    pub(crate) guarantees: Vec<Guarantee>,
    pub(crate) views: Vec<View>,
    pub(crate) witnesses: Vec<Witness>,
}

/// A consumer's interface field fed by a provider's view of the same
/// pre-state, with the provider facts that discharge the consumer's
/// assumption.
#[derive(Clone, Debug)]
pub(crate) struct Edge {
    pub(crate) consumer: u32,
    pub(crate) interface: u16,
    pub(crate) provider: u32,
    pub(crate) view: u32,
    pub(crate) facts: Vec<u32>,
}

/// An environment input of a group: a leaf's interface field that no edge
/// from inside the group feeds, named `context.102.<id>` in the group's
/// formulas.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Input {
    pub(crate) id: u16,
    pub(crate) leaf: u32,
    pub(crate) field: u16,
}

/// A view of a leaf in a group, named `post.100.<id>` in the group's outer
/// guarantee.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Export {
    pub(crate) id: u16,
    pub(crate) leaf: u32,
    pub(crate) view: u32,
}

/// One named conjunct of a group's outer assumption.
#[derive(Clone, Debug)]
pub(crate) struct Named {
    pub(crate) name: String,
    pub(crate) formula: String,
}

/// One conjunct of a group's outer guarantee.
#[derive(Clone, Debug)]
pub(crate) struct GroupGuarantee {
    pub(crate) id: u32,
    pub(crate) formula: String,
}

/// A group of leaves and groups.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) children: Vec<u32>,
    pub(crate) inputs: Vec<Input>,
    pub(crate) exports: Vec<Export>,
    pub(crate) assumption: Vec<Named>,
    pub(crate) guarantee: Vec<GroupGuarantee>,
}

/// The groups of a `/2` manifest and its root group's ID.
#[derive(Clone, Debug)]
pub(crate) struct Hierarchy {
    pub(crate) groups: Vec<Group>,
    pub(crate) root: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Manifest {
    pub(crate) name: String,
    pub(crate) leaves: Vec<Leaf>,
    pub(crate) edges: Vec<Edge>,
    /// `Some` exactly for a `/2` manifest.
    pub(crate) hierarchy: Option<Hierarchy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    schema: String,
    name: String,
    checker: String,
    leaves: Vec<LeafFile>,
    edges: Vec<EdgeFile>,
    groups: Option<Vec<GroupFile>>,
    root: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupFile {
    id: u32,
    name: String,
    children: Vec<u32>,
    inputs: Vec<InputFile>,
    exports: Vec<ExportFile>,
    assumption: Vec<NamedFile>,
    guarantee: Vec<GroupGuaranteeFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    id: u16,
    leaf: u32,
    field: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportFile {
    id: u16,
    leaf: u32,
    view: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedFile {
    name: String,
    formula: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupGuaranteeFile {
    id: u32,
    formula: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeafFile {
    id: u32,
    name: String,
    path: String,
    project_sha256: String,
    policy_sha256: String,
    program_sha256: String,
    owns: Vec<OwnedFile>,
    footprint: FootprintFile,
    invariant: String,
    assumption: String,
    guarantees: Vec<GuaranteeFile>,
    views: Vec<ViewFile>,
    witnesses: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedFile {
    field: u16,
    slot: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FootprintFile {
    reads: Vec<String>,
    writes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuaranteeFile {
    id: u32,
    kind: String,
    scope: Option<String>,
    formula: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewFile {
    id: u32,
    field: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeFile {
    consumer: u32,
    interface: u16,
    provider: u32,
    view: u32,
    facts: Vec<u32>,
}

fn invalid(place: impl Into<String>, reason: impl Into<String>) -> Refusal {
    Refusal::new(
        Stage::Manifest,
        Obligation::ClosedManifestForm,
        place,
        reason,
    )
}

impl Manifest {
    /// Reads and validates the closed form.
    ///
    /// # Errors
    /// The first key, value or order the closed form refuses.
    pub(crate) fn read(text: &str) -> Result<Self, Refusal> {
        let file: ManifestFile = serde_json::from_str(text)
            .map_err(|error| invalid("composition manifest", error.to_string()))?;
        let hierarchical = match file.schema.as_str() {
            MANIFEST_SCHEMA => false,
            HIERARCHY_SCHEMA => true,
            other => {
                return Err(invalid(
                    "schema",
                    format!(
                        "expected `{MANIFEST_SCHEMA}` or `{HIERARCHY_SCHEMA}`, found `{other}`"
                    ),
                ));
            }
        };
        if file.checker != CHECKER {
            return Err(invalid(
                "checker",
                format!("expected `{CHECKER}`, found `{}`", file.checker),
            ));
        }
        if file.name.is_empty() {
            return Err(invalid("name", "must name the composition"));
        }
        if file.leaves.is_empty() || file.leaves.len() > MAX_LEAVES {
            return Err(invalid(
                "leaves",
                format!("must list 1 to {MAX_LEAVES} leaves"),
            ));
        }
        if file.edges.len() > MAX_EDGES {
            return Err(invalid("edges", format!("may list at most {MAX_EDGES}")));
        }
        let leaves = file
            .leaves
            .into_iter()
            .enumerate()
            .map(|(index, leaf)| read_leaf(index, leaf))
            .collect::<Result<Vec<_>, _>>()?;
        if leaves.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err(invalid("leaves", "IDs must be strictly increasing"));
        }
        let mut paths: Vec<&str> = leaves.iter().map(|leaf| leaf.path.as_str()).collect();
        paths.sort_unstable();
        if paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid("leaves", "two leaves name the same path"));
        }
        let edges = file
            .edges
            .into_iter()
            .enumerate()
            .map(|(index, edge)| {
                let place = format!("edges[{index}]");
                if edge.consumer == 0 || edge.provider == 0 || edge.view == 0 {
                    return Err(invalid(&place, "IDs must be nonzero"));
                }
                ids(&format!("{place}.facts"), &edge.facts)?;
                Ok(Edge {
                    consumer: edge.consumer,
                    interface: edge.interface,
                    provider: edge.provider,
                    view: edge.view,
                    facts: edge.facts,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if edges.windows(2).any(|pair| {
            (pair[0].consumer, pair[0].interface) >= (pair[1].consumer, pair[1].interface)
        }) {
            return Err(invalid(
                "edges",
                "must be strictly increasing by consumer and interface field; one edge feeds one field",
            ));
        }
        let hierarchy = match (hierarchical, file.groups, file.root) {
            (false, None, None) => None,
            (true, Some(groups), Some(root)) => Some(read_hierarchy(groups, root)?),
            (false, ..) => {
                return Err(invalid(
                    "groups",
                    format!("a `{MANIFEST_SCHEMA}` manifest has no groups or root"),
                ));
            }
            (true, ..) => {
                return Err(invalid(
                    "groups",
                    format!("a `{HIERARCHY_SCHEMA}` manifest lists its groups and names its root"),
                ));
            }
        };
        Ok(Self {
            name: file.name,
            leaves,
            edges,
            hierarchy,
        })
    }

    /// The schema the manifest was read under.
    pub(crate) const fn schema(&self) -> &'static str {
        if self.hierarchy.is_some() {
            HIERARCHY_SCHEMA
        } else {
            MANIFEST_SCHEMA
        }
    }

    /// The canonical bytes: every field, in the manifest's canonical order.
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        let version: u16 = if self.hierarchy.is_some() { 2 } else { 1 };
        out.extend_from_slice(&version.to_be_bytes());
        text(&mut out, CHECKER);
        text(&mut out, &self.name);
        count(&mut out, self.leaves.len());
        for leaf in &self.leaves {
            out.extend_from_slice(&leaf.id.to_be_bytes());
            text(&mut out, &leaf.name);
            text(&mut out, &leaf.path);
            for pin in [
                &leaf.project_sha256,
                &leaf.policy_sha256,
                &leaf.program_sha256,
            ] {
                out.extend_from_slice(pin);
            }
            count(&mut out, leaf.owns.len());
            for owned in &leaf.owns {
                out.extend_from_slice(&owned.field.to_be_bytes());
                out.extend_from_slice(&owned.slot.to_be_bytes());
            }
            count(&mut out, leaf.reads.len());
            for read in &leaf.reads {
                out.push(read.source);
                out.extend_from_slice(&read.field.to_be_bytes());
            }
            count(&mut out, leaf.writes.len());
            for field in &leaf.writes {
                out.extend_from_slice(&field.to_be_bytes());
            }
            text(&mut out, &leaf.invariant);
            text(&mut out, &leaf.assumption);
            count(&mut out, leaf.guarantees.len());
            for guarantee in &leaf.guarantees {
                out.extend_from_slice(&guarantee.id.to_be_bytes());
                out.push(guarantee.kind.code());
                out.push(scope_code(guarantee.scope));
                text(&mut out, &guarantee.formula);
            }
            count(&mut out, leaf.views.len());
            for view in &leaf.views {
                out.extend_from_slice(&view.id.to_be_bytes());
                out.extend_from_slice(&view.field.to_be_bytes());
            }
            count(&mut out, leaf.witnesses.len());
            for witness in &leaf.witnesses {
                out.push(match witness {
                    Witness::Accept => 0,
                    Witness::Reject => 1,
                });
            }
        }
        count(&mut out, self.edges.len());
        for edge in &self.edges {
            out.extend_from_slice(&edge.consumer.to_be_bytes());
            out.extend_from_slice(&edge.interface.to_be_bytes());
            out.extend_from_slice(&edge.provider.to_be_bytes());
            out.extend_from_slice(&edge.view.to_be_bytes());
            count(&mut out, edge.facts.len());
            for fact in &edge.facts {
                out.extend_from_slice(&fact.to_be_bytes());
            }
        }
        if let Some(hierarchy) = &self.hierarchy {
            count(&mut out, hierarchy.groups.len());
            for group in &hierarchy.groups {
                out.extend_from_slice(&group.canonical_bytes());
            }
            out.extend_from_slice(&hierarchy.root.to_be_bytes());
        }
        out
    }

    /// The commitment: the SHA-256 of the tagged canonical bytes.
    pub(crate) fn commitment(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }
}

impl Leaf {
    pub(crate) fn view(&self, id: u32) -> Option<View> {
        self.views.iter().copied().find(|view| view.id == id)
    }
}

impl Group {
    /// The group's canonical bytes, part of the manifest's and the group's
    /// own commitment.
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.to_be_bytes());
        text(&mut out, &self.name);
        count(&mut out, self.children.len());
        for child in &self.children {
            out.extend_from_slice(&child.to_be_bytes());
        }
        count(&mut out, self.inputs.len());
        for input in &self.inputs {
            out.extend_from_slice(&input.id.to_be_bytes());
            out.extend_from_slice(&input.leaf.to_be_bytes());
            out.extend_from_slice(&input.field.to_be_bytes());
        }
        count(&mut out, self.exports.len());
        for export in &self.exports {
            out.extend_from_slice(&export.id.to_be_bytes());
            out.extend_from_slice(&export.leaf.to_be_bytes());
            out.extend_from_slice(&export.view.to_be_bytes());
        }
        count(&mut out, self.assumption.len());
        for named in &self.assumption {
            text(&mut out, &named.name);
            text(&mut out, &named.formula);
        }
        count(&mut out, self.guarantee.len());
        for guarantee in &self.guarantee {
            out.extend_from_slice(&guarantee.id.to_be_bytes());
            text(&mut out, &guarantee.formula);
        }
        out
    }
}

fn read_hierarchy(groups: Vec<GroupFile>, root: u32) -> Result<Hierarchy, Refusal> {
    if groups.is_empty() || groups.len() > MAX_GROUPS {
        return Err(invalid(
            "groups",
            format!("must list 1 to {MAX_GROUPS} groups"),
        ));
    }
    let groups = groups
        .into_iter()
        .enumerate()
        .map(|(index, group)| read_group(index, group))
        .collect::<Result<Vec<_>, _>>()?;
    if groups.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(invalid("groups", "IDs must be strictly increasing"));
    }
    if !groups.iter().any(|group| group.id == root) {
        return Err(invalid(
            "root",
            format!("names group {root}, which the manifest does not list"),
        ));
    }
    Ok(Hierarchy { groups, root })
}

fn read_group(index: usize, group: GroupFile) -> Result<Group, Refusal> {
    let place = |key: &str| format!("groups[{index}].{key}");
    let increasing = |values: &mut dyn Iterator<Item = u64>| {
        let values: Vec<u64> = values.collect();
        !values.contains(&0) && values.windows(2).all(|pair| pair[0] < pair[1])
    };
    if group.id == 0 || group.name.is_empty() {
        return Err(invalid(place("id"), "needs a nonzero ID and a name"));
    }
    if group.children.is_empty() || !increasing(&mut group.children.iter().map(|id| u64::from(*id)))
    {
        return Err(invalid(
            place("children"),
            "must list nonzero child IDs in strictly increasing order, at least one",
        ));
    }
    if !increasing(&mut group.inputs.iter().map(|input| u64::from(input.id)))
        || group.inputs.iter().any(|input| input.leaf == 0)
    {
        return Err(invalid(
            place("inputs"),
            "IDs must be nonzero and strictly increasing, each naming a leaf",
        ));
    }
    if !increasing(&mut group.exports.iter().map(|export| u64::from(export.id)))
        || group
            .exports
            .iter()
            .any(|export| export.leaf == 0 || export.view == 0)
    {
        return Err(invalid(
            place("exports"),
            "IDs must be nonzero and strictly increasing, each naming a leaf's view",
        ));
    }
    if group
        .assumption
        .iter()
        .any(|named| named.name.is_empty() || named.formula.trim().is_empty())
        || group
            .assumption
            .windows(2)
            .any(|pair| pair[0].name >= pair[1].name)
    {
        return Err(invalid(
            place("assumption"),
            "each conjunct needs a name and a formula; names strictly increasing",
        ));
    }
    if !increasing(
        &mut group
            .guarantee
            .iter()
            .map(|guarantee| u64::from(guarantee.id)),
    ) || group
        .guarantee
        .iter()
        .any(|guarantee| guarantee.formula.trim().is_empty())
    {
        return Err(invalid(
            place("guarantee"),
            "each conjunct needs a formula; IDs nonzero and strictly increasing",
        ));
    }
    Ok(Group {
        id: group.id,
        name: group.name,
        children: group.children,
        inputs: group
            .inputs
            .iter()
            .map(|input| Input {
                id: input.id,
                leaf: input.leaf,
                field: input.field,
            })
            .collect(),
        exports: group
            .exports
            .iter()
            .map(|export| Export {
                id: export.id,
                leaf: export.leaf,
                view: export.view,
            })
            .collect(),
        assumption: group
            .assumption
            .into_iter()
            .map(|named| Named {
                name: named.name,
                formula: named.formula,
            })
            .collect(),
        guarantee: group
            .guarantee
            .into_iter()
            .map(|guarantee| GroupGuarantee {
                id: guarantee.id,
                formula: guarantee.formula,
            })
            .collect(),
    })
}

/// The leaf directories a manifest names, in leaf order, for the caller
/// that reads their files.
///
/// # Errors
/// The manifest's refusal.
pub fn leaf_paths(text: &str) -> Result<Vec<String>, Refusal> {
    Ok(Manifest::read(text)?
        .leaves
        .into_iter()
        .map(|leaf| leaf.path)
        .collect())
}

fn read_leaf(index: usize, leaf: LeafFile) -> Result<Leaf, Refusal> {
    let place = |key: &str| format!("leaves[{index}].{key}");
    if leaf.id == 0 {
        return Err(invalid(place("id"), "must be nonzero"));
    }
    if leaf.name.is_empty() {
        return Err(invalid(place("name"), "must name the leaf"));
    }
    if !safe_path(&leaf.path) {
        return Err(invalid(
            place("path"),
            "must be a relative path of plain names, without `.` or `..`",
        ));
    }
    let mut owns: Vec<Owned> = leaf
        .owns
        .iter()
        .map(|owned| Owned {
            field: owned.field,
            slot: owned.slot,
        })
        .collect();
    owns.sort_by_key(|owned| owned.field);
    if owns.is_empty() || owns.windows(2).any(|pair| pair[0].field == pair[1].field) {
        return Err(invalid(
            place("owns"),
            "must list each owned state field once, and at least one",
        ));
    }
    let mut reads = leaf
        .footprint
        .reads
        .iter()
        .map(|name| {
            read_path(name).ok_or_else(|| {
                invalid(
                    place("footprint.reads"),
                    format!("`{name}` is not pre.100.f, command.101.f or context.102.f"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    reads.sort_unstable();
    let mut writes = leaf
        .footprint
        .writes
        .iter()
        .map(|name| {
            write_path(name).ok_or_else(|| {
                invalid(
                    place("footprint.writes"),
                    format!("`{name}` is not post.100.f"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    writes.sort_unstable();
    if reads.windows(2).any(|pair| pair[0] == pair[1])
        || writes.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(invalid(place("footprint"), "repeats a path"));
    }
    if leaf.invariant.trim().is_empty() || leaf.assumption.trim().is_empty() {
        return Err(invalid(
            place("invariant"),
            "the invariant and the assumption must be formulas; write `true` for none",
        ));
    }
    let guarantees = leaf
        .guarantees
        .into_iter()
        .enumerate()
        .map(|(position, guarantee)| {
            let at = place(&format!("guarantees[{position}]"));
            let kind = match guarantee.kind.as_str() {
                "StateFact" => GuaranteeKind::StateFact,
                "TransitionRelation" => GuaranteeKind::TransitionRelation,
                other => {
                    return Err(invalid(
                        &at,
                        format!("kind `{other}` is neither StateFact nor TransitionRelation"),
                    ));
                }
            };
            let scope = match (kind, guarantee.scope.as_deref()) {
                (GuaranteeKind::StateFact, None) => None,
                (GuaranteeKind::TransitionRelation, Some("accept")) => Some(Scope::Accept),
                (GuaranteeKind::TransitionRelation, Some("reject")) => Some(Scope::Reject),
                (GuaranteeKind::TransitionRelation, Some("commit")) => Some(Scope::Commit),
                (GuaranteeKind::TransitionRelation, Some("any")) => Some(Scope::Any),
                (GuaranteeKind::StateFact, Some(_)) => {
                    return Err(invalid(&at, "a StateFact has no scope; write null"));
                }
                (GuaranteeKind::TransitionRelation, _) => {
                    return Err(invalid(
                        &at,
                        "a TransitionRelation needs scope accept, reject, commit or any",
                    ));
                }
            };
            if guarantee.id == 0 || guarantee.formula.trim().is_empty() {
                return Err(invalid(&at, "needs a nonzero ID and a formula"));
            }
            Ok(Guarantee {
                id: guarantee.id,
                kind,
                scope,
                formula: guarantee.formula,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if guarantees.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(invalid(
            place("guarantees"),
            "IDs must be strictly increasing",
        ));
    }
    let views: Vec<View> = leaf
        .views
        .iter()
        .map(|view| View {
            id: view.id,
            field: view.field,
        })
        .collect();
    if views.iter().any(|view| view.id == 0)
        || views.windows(2).any(|pair| pair[0].id >= pair[1].id)
    {
        return Err(invalid(
            place("views"),
            "IDs must be nonzero and strictly increasing",
        ));
    }
    let mut witnesses = leaf
        .witnesses
        .iter()
        .map(|name| match name.as_str() {
            "accept" => Ok(Witness::Accept),
            "reject" => Ok(Witness::Reject),
            other => Err(invalid(
                place("witnesses"),
                format!("`{other}` is neither accept nor reject"),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    witnesses.sort_unstable();
    witnesses.dedup();
    if witnesses.first() != Some(&Witness::Accept) || witnesses.len() != leaf.witnesses.len() {
        return Err(invalid(
            place("witnesses"),
            "must list accept, optionally reject, each once",
        ));
    }
    Ok(Leaf {
        id: leaf.id,
        name: leaf.name,
        path: leaf.path,
        project_sha256: pin(&place("project_sha256"), &leaf.project_sha256)?,
        policy_sha256: pin(&place("policy_sha256"), &leaf.policy_sha256)?,
        program_sha256: pin(&place("program_sha256"), &leaf.program_sha256)?,
        owns,
        reads,
        writes,
        invariant: leaf.invariant,
        assumption: leaf.assumption,
        guarantees,
        views,
        witnesses,
    })
}

fn ids(place: &str, values: &[u32]) -> Result<(), Refusal> {
    if values.is_empty() || values.contains(&0) || values.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(invalid(
            place,
            "must list nonzero IDs in strictly increasing order, at least one",
        ));
    }
    Ok(())
}

pub(crate) fn pin(place: &str, text: &str) -> Result<[u8; 32], Refusal> {
    let bytes = text.as_bytes();
    let malformed = || invalid(place, "must be 64 lowercase hexadecimal digits");
    if bytes.len() != 64 {
        return Err(malformed());
    }
    let mut out = [0u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        let (Some(high), Some(low)) = (digit(pair[0]), digit(pair[1])) else {
            return Err(malformed());
        };
        out[index] = high << 4 | low;
    }
    Ok(out)
}

pub(crate) fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
}

fn read_path(name: &str) -> Option<ReadPath> {
    let (source, rest) = if let Some(rest) = name.strip_prefix("pre.100.") {
        (0, rest)
    } else if let Some(rest) = name.strip_prefix("command.101.") {
        (1, rest)
    } else {
        (2, name.strip_prefix("context.102.")?)
    };
    Some(ReadPath {
        source,
        field: field_id(rest)?,
    })
}

fn write_path(name: &str) -> Option<u16> {
    field_id(name.strip_prefix("post.100.")?)
}

fn field_id(text: &str) -> Option<u16> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A length-prefixed text, as the canonical bytes and the claims frame it.
pub(crate) fn text(out: &mut Vec<u8>, value: &str) {
    count(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

/// A length; every length here is bounded far below `u32::MAX` by the
/// manifest limits and the JSON text it came from.
fn count(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes());
}
