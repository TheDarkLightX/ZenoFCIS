//! The structural judge.
//!
//! A manifest denotes one `zeno-fcis-compose` [`CompositionSpec`]: one
//! component per leaf, whose profile is the leaf's bound program identity;
//! the global state slots `STATE[slot]` and each leaf's command fields
//! `COMMAND[leaf, field]` as access paths, a peer view being a read of the
//! provider's slot; one guarantee for each declared guarantee and, for a
//! leaf with interface fields, one assumption, each named by a commitment to
//! the leaf's ID, its program and the formula, and depending on the paths
//! its formula observes. A leaf may write every slot it owns, since its
//! successor is written back whole; the fields it does not declare written
//! are kept by its local frame check.
//!
//! Compose's constructors and [`verify_assume_guarantee`] decide every rule
//! compose has: identifiers, duplicate paths, the existence of each named
//! provider guarantee and a discharge for each assumption. Its [`conflicts`]
//! decides slot disjointness as a write-write conflict. Every path here is
//! one or two exact atoms without a wildcard, so compose's overlap is
//! equality, and a write-write conflict between two leaves is exactly a slot
//! both own. Compose's read-write conflicts are not refusals in this
//! profile: a peer view is read from the same pre-state, never from a
//! successor. Compose is shown each claim with the claim's own canonical
//! statement as its artifact and a verifier that accepts exactly that
//! statement, so the only blockers it can report are structural; the
//! claims' truth is the checker's own tables, never that verifier.
//!
//! The checker keeps what compose cannot express: each interface field fed
//! by exactly one edge from another listed leaf, a typed view of the
//! provider's state with an identical domain, discharge by the provider's
//! StateFacts only, each reading only the viewed field, and slots `0..n`
//! without a gap, so every slot of the composed state is owned.
//!
//! A `/2` manifest's groups are components too: a group writes nothing,
//! reads its environment inputs and exports, guarantees each conjunct of
//! its outer guarantee and the delivery of its outer assumption to its
//! children, and, below the root, assumes its outer assumption. An
//! interface field no edge feeds is an environment path
//! `ENVIRONMENT[leaf, field]`. A leaf or group with an input from outside
//! its parent names the parent's delivery among its providers; the root's
//! outer assumption is the condition of the claim, not a compose
//! assumption. Compose decides that no group shares an ID with a leaf or
//! another group and that every assumption has a discharge.

use std::collections::BTreeSet;

use zeno_fcis_codec::Hash32;
use zeno_fcis_compose::{
    AccessPath, Assumption, AssumptionDischarge, ClaimEvidence, ComponentContract, ComponentId,
    CompositionBlocker, CompositionClaim, CompositionEvidence, CompositionSpec, ConflictKind,
    ContractError as ComposeError, EvidenceArtifact, EvidenceVerifier, Footprint, Guarantee,
    PathAtom, PathSet, ProviderGuarantee, conflicts, verify_assume_guarantee,
};
use zeno_fcis_crypto::RustCryptoSha256;

use super::hierarchy::{Feeds, Tree};
use super::leaf::{Compiled, sha256};
use super::manifest::{self, Group, GuaranteeKind, Leaf, Manifest, scope_code, text};
use super::refusal::{ComposeFinding, Obligation, Refusal, Stage};
use crate::internals::declarations::Source;

/// The version of every derived specification.
pub const SPEC_VERSION: u16 = 1;
/// The namespace of the composed state's slots.
const STATE: u32 = 1;
/// The namespace of each leaf's command fields.
const COMMAND: u32 = 2;
/// The namespace of interface fields no edge feeds.
const ENVIRONMENT: u32 = 3;
const GUARANTEE: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-GUARANTEE\0";
const ASSUMPTION: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-ASSUMPTION\0";
const UNDECLARED: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-UNDECLARED\0";
const GROUP: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-GROUP\0";
const GROUP_GUARANTEE: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-GROUP-GUARANTEE\0";
const GROUP_ASSUMPTION: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-GROUP-ASSUMPTION\0";
const DELIVERY: &[u8] = b"ZFCIS-CHECKED-COMPOSITION-DELIVERY\0";

/// The structure of a composition the judge accepted.
#[derive(Debug)]
pub(crate) struct Judged {
    pub(crate) spec: CompositionSpec,
    pub(crate) spec_hash: [u8; 32],
    /// Each guarantee and assumption discharge, as compose states them.
    pub(crate) claims: Vec<CompositionClaim>,
    pub(crate) slots: usize,
    pub(crate) edges: Vec<Resolved>,
    /// The group tree of a `/2` manifest.
    pub(crate) tree: Option<Tree>,
}

/// One interface edge, resolved against the manifest and the compiled
/// leaves: leaf positions, the consumer's interface field, the provider's
/// viewed state field and the provider facts.
#[derive(Debug)]
pub(crate) struct Resolved {
    pub(crate) consumer: usize,
    pub(crate) interface: u16,
    pub(crate) provider: usize,
    pub(crate) view_field: u16,
    pub(crate) facts: Vec<u32>,
}

/// Judges the structure: compose on the specification the manifest
/// denotes, then the rules compose cannot express.
///
/// # Errors
/// Compose's first finding, or the first of the checker's own rules that
/// fails.
pub(crate) fn judge(manifest: &Manifest, compiled: &[Compiled]) -> Result<Judged, Refusal> {
    let spec = specification(manifest, compiled)?;
    let (spec_hash, claims) = compose_verdict(manifest, compiled, &spec)?;
    let slots = layout(manifest)?;
    let edges = interfaces(manifest, compiled)?;
    Ok(Judged {
        spec,
        spec_hash,
        claims,
        slots,
        edges,
        tree: Tree::of(manifest)?,
    })
}

/// The specification a manifest denotes.
///
/// # Errors
/// The first compose constructor that refuses a part of it.
pub(crate) fn specification(
    manifest: &Manifest,
    compiled: &[Compiled],
) -> Result<CompositionSpec, Refusal> {
    let mut components = Vec::with_capacity(manifest.leaves.len());
    for (leaf, compiled) in manifest.leaves.iter().zip(compiled) {
        let at = |key: &str| format!("leaf {} `{}` {key}", leaf.id, leaf.name);
        let roots = Roots::of(compiled);
        let paths = |fields: &mut dyn Iterator<Item = (u8, u16)>| -> Result<PathSet, Refusal> {
            let mut set = BTreeSet::new();
            for (source, field) in fields {
                if let Some(path) = location(manifest, leaf, source, field) {
                    set.insert(path.map_err(|error| internal(&at("paths"), error))?);
                }
            }
            PathSet::try_new(set.into_iter().collect()).map_err(|error| {
                constructor(
                    Stage::Ownership,
                    Obligation::CompleteFootprint,
                    at("footprint"),
                    "compose refuses one of its path sets",
                    error,
                )
            })
        };
        let reads = paths(&mut leaf.reads.iter().map(|read| (read.source, read.field)))?;
        // One write path per owned field, unmerged: two fields that claim
        // one slot are a duplicate path, which compose refuses.
        let writes = leaf
            .owns
            .iter()
            .map(|owned| state(owned.slot).map_err(|error| internal(&at("owns"), error)))
            .collect::<Result<Vec<_>, _>>()?;
        let writes = PathSet::try_new(writes).map_err(|error| {
            let obligation = if error == ComposeError::DuplicatePath {
                Obligation::DisjointOwnership
            } else {
                Obligation::CompleteFootprint
            };
            constructor(
                Stage::Ownership,
                obligation,
                at("owns"),
                "two of its state fields claim one slot, or it owns too many; compose refuses its write paths",
                error,
            )
        })?;
        let mut guarantees = Vec::with_capacity(leaf.guarantees.len());
        for (guarantee, (_, _, predicate)) in leaf.guarantees.iter().zip(&compiled.guarantees) {
            let observed = paths(
                &mut predicate
                    .fields()
                    .iter()
                    .filter_map(|field| roots.source(*field)),
            )?;
            guarantees.push(Guarantee::new(guarantee_claim(leaf, guarantee), observed));
        }
        let mut assumptions = Vec::new();
        if !roots.context.is_empty() {
            let observed = paths(
                &mut compiled
                    .assumption
                    .fields()
                    .iter()
                    .filter_map(|field| roots.source(*field)),
            )?;
            assumptions.push(Assumption::new(
                assumption_claim(leaf, &roots.context),
                observed,
            ));
        }
        let footprint = Footprint::new(reads, writes, PathSet::empty(), PathSet::empty());
        let component = ComponentContract::try_new(
            ComponentId::new(leaf.id),
            Hash32::new(leaf.program_sha256),
            footprint,
            assumptions,
            guarantees,
            Vec::new(),
        )
        .map_err(|error| {
            let (stage, obligation) = if error == ComposeError::ZeroHash {
                (Stage::Binding, Obligation::ProgramBinding)
            } else {
                (Stage::Manifest, Obligation::ClosedManifestForm)
            };
            constructor(
                stage,
                obligation,
                at("program_sha256"),
                "compose refuses the leaf as a component",
                error,
            )
        })?;
        components.push(component);
    }
    let groups = manifest
        .hierarchy
        .as_ref()
        .map_or(&[][..], |hierarchy| &hierarchy.groups[..]);
    for group in groups {
        components.push(group_component(manifest, group)?);
    }
    CompositionSpec::try_new(
        SPEC_VERSION,
        components,
        Vec::new(),
        Vec::new(),
        manifest
            .leaves
            .iter()
            .map(|leaf| ComponentId::new(leaf.id))
            .chain(groups.iter().map(|group| ComponentId::new(group.id)))
            .collect(),
    )
    .map_err(|error| {
        constructor(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            "leaves",
            "compose refuses the composition specification",
            error,
        )
    })
}

/// Compose's verdict on the specification: its commitment, then every
/// assume-guarantee blocker and write-write conflict. Returns the
/// commitment and the claims compose was shown.
///
/// # Errors
/// Compose's first finding.
pub(crate) fn compose_verdict(
    manifest: &Manifest,
    compiled: &[Compiled],
    spec: &CompositionSpec,
) -> Result<([u8; 32], Vec<CompositionClaim>), Refusal> {
    let spec_hash = spec.commitment::<RustCryptoSha256>().map_err(|error| {
        constructor(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            "leaves",
            "compose cannot commit the composition specification",
            error,
        )
    })?;
    let tree = Tree::of(manifest)?;
    let (evidence, claims) = evidence(manifest, compiled, tree.as_ref(), spec, spec_hash)?;
    let report = verify_assume_guarantee::<RustCryptoSha256, _>(spec, &evidence, &ExactStatement);
    if let Some(blocker) = report.blockers().first() {
        return Err(blocker_refusal(manifest, spec, blocker));
    }
    let components = spec.components();
    for (index, left) in components.iter().enumerate() {
        for right in &components[index + 1..] {
            if conflicts(left.footprint(), right.footprint())
                .iter()
                .any(|conflict| conflict.kind() == ConflictKind::WriteWrite)
            {
                return Err(write_write(manifest, left.id(), right.id()));
            }
        }
    }
    Ok((*spec_hash.as_bytes(), claims))
}

/// Accepts an artifact exactly when it is the claim's own canonical
/// statement. Compose's evidence checks then hold by construction and only
/// its structural blockers remain; the checker never uses this verdict as
/// evidence of a claim's truth.
struct ExactStatement;

impl EvidenceVerifier for ExactStatement {
    fn verify(&self, claim: &CompositionClaim, artifact: &[u8]) -> bool {
        claim
            .canonical_bytes()
            .is_ok_and(|statement| statement == artifact)
    }
}

/// Each guarantee with its statement, and each assumption with the exact
/// providers its edges name; and those statements in compose's order.
fn evidence(
    manifest: &Manifest,
    compiled: &[Compiled],
    tree: Option<&Tree>,
    spec: &CompositionSpec,
    spec_hash: Hash32,
) -> Result<(CompositionEvidence, Vec<CompositionClaim>), Refusal> {
    let feeds = Feeds::of(manifest);
    let index = |id: u32| manifest.leaves.iter().position(|leaf| leaf.id == id);
    let groups: Vec<&Group> = manifest
        .hierarchy
        .as_ref()
        .map_or_else(Vec::new, |hierarchy| hierarchy.groups.iter().collect());
    let artifact = |statement: &CompositionClaim| {
        statement
            .canonical_bytes()
            .map(EvidenceArtifact::new::<RustCryptoSha256>)
            .map_err(|error| internal("evidence", ComposeError::Encode(error)))
    };
    let mut claims = Vec::new();
    let mut discharges = Vec::new();
    let mut statements = Vec::new();
    for component in spec.components() {
        for guarantee in component.guarantees() {
            let statement = CompositionClaim::Guarantee {
                spec_hash,
                component: component.id(),
                claim: guarantee.claim(),
            };
            claims.push(ClaimEvidence::new(guarantee.claim(), artifact(&statement)?));
            statements.push(statement);
        }
        for assumption in component.assumptions() {
            let mut providers = BTreeSet::new();
            let id = component.id().get();
            // A leaf's inputs, or a group's: (consumer position, field).
            let (inputs, parent): (Vec<(usize, u16)>, Option<u32>) = match (index(id), tree) {
                (Some(position), Some(tree)) => (
                    compiled[position]
                        .fields(Source::Context)
                        .iter()
                        .map(|field| (position, field.field))
                        .collect(),
                    tree.leaf_parent.get(position).copied(),
                ),
                (None, Some(tree)) => (
                    groups
                        .iter()
                        .find(|group| group.id == id)
                        .map_or_else(Vec::new, |group| {
                            group
                                .inputs
                                .iter()
                                .filter_map(|input| {
                                    index(input.leaf).map(|position| (position, input.field))
                                })
                                .collect()
                        }),
                    tree.group_parent.get(&id).copied(),
                ),
                _ => (Vec::new(), None),
            };
            let edges = manifest.edges.iter().filter(|edge| {
                if tree.is_none() {
                    return edge.consumer == id;
                }
                index(edge.consumer).is_some_and(|consumer| {
                    inputs.contains(&(consumer, edge.interface))
                        && parent.zip(tree).is_some_and(|(parent, tree)| {
                            index(edge.provider)
                                .is_some_and(|provider| tree.contains(parent, provider))
                        })
                })
            });
            for edge in edges {
                for fact in &edge.facts {
                    let provider = ProviderGuarantee::try_new(
                        ComponentId::new(edge.provider),
                        fact_claim(manifest, edge.provider, *fact),
                    )
                    .map_err(|error| internal("edges", error))?;
                    providers.insert(provider);
                }
            }
            // An input from outside the parent arrives through the parent's
            // delivery of its outer assumption.
            if let (Some(tree), Some(parent)) = (tree, parent)
                && inputs
                    .iter()
                    .any(|input| feeds.inside(tree, parent, *input).is_none())
                && let Some(group) = groups.iter().find(|group| group.id == parent)
            {
                let provider =
                    ProviderGuarantee::try_new(ComponentId::new(parent), delivery_claim(group))
                        .map_err(|error| internal("groups", error))?;
                providers.insert(provider);
            }
            if providers.is_empty() {
                // Compose reports the missing discharge.
                continue;
            }
            let providers: Vec<ProviderGuarantee> = providers.into_iter().collect();
            let statement = CompositionClaim::AssumptionDischarge {
                spec_hash,
                component: component.id(),
                assumption: assumption.claim(),
                providers: providers.clone().into_boxed_slice(),
            };
            let discharge = AssumptionDischarge::try_new(
                component.id(),
                assumption.claim(),
                providers,
                artifact(&statement)?,
            )
            .map_err(|error| internal("edges", error))?;
            discharges.push(discharge);
            statements.push(statement);
        }
    }
    let evidence = CompositionEvidence::try_new(claims, discharges, None)
        .map_err(|error| internal("evidence", error))?;
    Ok((evidence, statements))
}

fn blocker_refusal(
    manifest: &Manifest,
    spec: &CompositionSpec,
    blocker: &CompositionBlocker,
) -> Refusal {
    let finding = ComposeFinding::Blocker(blocker.clone());
    let leaf = |id: ComponentId| manifest.leaves.iter().find(|leaf| leaf.id == id.get());
    let refusal = match blocker {
        CompositionBlocker::MissingAssumptionDischarge { component, .. } => {
            let place = leaf(*component).map_or_else(
                || format!("leaf {}", component.get()),
                |leaf| format!("leaf {} `{}` assumption", leaf.id, leaf.name),
            );
            Refusal::new(
                Stage::Interface,
                Obligation::ProviderBinding,
                place,
                "no edge feeds its interface fields, so compose finds no discharge of its assumption",
            )
        }
        CompositionBlocker::UnknownProviderGuarantee {
            assumption,
            component,
            guarantee,
        } => {
            let consumer = spec
                .components()
                .iter()
                .find(|candidate| {
                    candidate
                        .assumptions()
                        .iter()
                        .any(|item| item.claim() == *assumption)
                })
                .map(|candidate| candidate.id().get());
            let place = manifest
                .edges
                .iter()
                .position(|edge| {
                    Some(edge.consumer) == consumer
                        && edge.provider == component.get()
                        && edge
                            .facts
                            .iter()
                            .any(|fact| fact_claim(manifest, edge.provider, *fact) == *guarantee)
                })
                .map_or_else(|| "edges".to_owned(), |index| format!("edges[{index}]"));
            match leaf(*component) {
                Some(provider) => Refusal::new(
                    Stage::Interface,
                    Obligation::ProviderClaim,
                    place,
                    format!(
                        "names a fact that leaf {} does not declare; compose finds no such provider guarantee",
                        provider.id
                    ),
                ),
                None => Refusal::new(
                    Stage::Interface,
                    Obligation::ProviderBinding,
                    place,
                    format!(
                        "names leaf {}, which the manifest does not list; compose finds no such provider",
                        component.get()
                    ),
                ),
            }
        }
        _ => Refusal::new(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            "leaves",
            "compose refuses the composition specification",
        ),
    };
    refusal.with_compose(finding)
}

fn write_write(manifest: &Manifest, left: ComponentId, right: ComponentId) -> Refusal {
    let owned = |id: ComponentId| -> BTreeSet<u32> {
        manifest
            .leaves
            .iter()
            .filter(|leaf| leaf.id == id.get())
            .flat_map(|leaf| leaf.owns.iter().map(|owned| owned.slot))
            .collect()
    };
    let shared: Vec<String> = owned(left)
        .intersection(&owned(right))
        .map(u32::to_string)
        .collect();
    Refusal::new(
        Stage::Ownership,
        Obligation::DisjointOwnership,
        format!("slot {}", shared.join(", ")),
        format!(
            "owned by leaf {} and by leaf {}; compose finds a write-write conflict",
            left.get(),
            right.get()
        ),
    )
    .with_compose(ComposeFinding::WriteWrite { left, right })
}

/// Slots `0..n` without a gap, so that every slot of the composed state is
/// owned. Returns `n`. Compose has refused a slot two fields claim.
fn layout(manifest: &Manifest) -> Result<usize, Refusal> {
    let slots: BTreeSet<u32> = manifest
        .leaves
        .iter()
        .flat_map(|leaf| leaf.owns.iter().map(|owned| owned.slot))
        .collect();
    if let Some((index, slot)) = slots
        .iter()
        .enumerate()
        .find(|(index, slot)| u32::try_from(*index).ok() != Some(**slot))
    {
        return Err(Refusal::new(
            Stage::Ownership,
            Obligation::CompleteLayout,
            format!("slot {slot}"),
            format!("slots must be 0..n without a gap; slot {index} is missing"),
        ));
    }
    Ok(slots.len())
}

/// Each interface field is fed by exactly one edge from another listed
/// leaf, through a view of identical domain, with facts that are the
/// provider's StateFacts and read only that view.
fn interfaces(manifest: &Manifest, compiled: &[Compiled]) -> Result<Vec<Resolved>, Refusal> {
    let index = |id: u32| manifest.leaves.iter().position(|leaf| leaf.id == id);
    let mut resolved = Vec::with_capacity(manifest.edges.len());
    for (position, edge) in manifest.edges.iter().enumerate() {
        let place = format!("edges[{position}]");
        let refuse = |obligation: Obligation, reason: String| {
            Refusal::new(Stage::Interface, obligation, &place, reason)
        };
        // Compose has refused a provider the manifest does not list. It does
        // not see an edge into a consumer the manifest does not list, since
        // it looks each discharge up by its own components.
        let (Some(consumer), Some(provider)) = (index(edge.consumer), index(edge.provider)) else {
            return Err(refuse(
                Obligation::ProviderBinding,
                format!(
                    "names leaf {} or {}, which the manifest does not list",
                    edge.consumer, edge.provider
                ),
            ));
        };
        // Compose admits a leaf discharging its own assumption; peer views
        // come from another leaf.
        if consumer == provider {
            return Err(refuse(
                Obligation::ProviderBinding,
                "a leaf cannot provide its own interface; peer views come from another leaf".into(),
            ));
        }
        let Some(interface) = compiled[consumer]
            .fields(Source::Context)
            .into_iter()
            .find(|field| field.field == edge.interface)
        else {
            return Err(refuse(
                Obligation::ProviderBinding,
                format!(
                    "consumer leaf {} has no context field {}",
                    edge.consumer, edge.interface
                ),
            ));
        };
        let provider_entry = &manifest.leaves[provider];
        let Some(view) = provider_entry.view(edge.view) else {
            return Err(refuse(
                Obligation::TypedView,
                format!("provider leaf {} has no view {}", edge.provider, edge.view),
            ));
        };
        let Some(viewed) = compiled[provider]
            .fields(Source::State)
            .into_iter()
            .find(|field| field.field == view.field)
        else {
            return Err(refuse(
                Obligation::TypedView,
                format!(
                    "view {} names field {}, which provider leaf {} does not declare",
                    edge.view, view.field, edge.provider
                ),
            ));
        };
        if viewed.domain != interface.domain {
            return Err(refuse(
                Obligation::TypedView,
                format!(
                    "view {} has domain {}, interface field {} has domain {}; substitution must be exact",
                    edge.view,
                    viewed.domain.json(),
                    edge.interface,
                    interface.domain.json()
                ),
            ));
        }
        for fact in &edge.facts {
            match compiled[provider].guarantee(*fact) {
                Some((GuaranteeKind::StateFact, predicate)) => {
                    if predicate.fields().iter().any(|field| *field != view.field) {
                        return Err(refuse(
                            Obligation::TypedView,
                            format!(
                                "StateFact {fact} reads more than the viewed field {}",
                                view.field
                            ),
                        ));
                    }
                }
                Some((GuaranteeKind::TransitionRelation, _)) => {
                    return Err(refuse(
                        Obligation::ProviderClaim,
                        format!(
                            "guarantee {fact} of leaf {} is a TransitionRelation; only a StateFact of the same pre-state discharges an assumption",
                            edge.provider
                        ),
                    ));
                }
                // Compose has refused a fact the provider does not declare.
                None => {
                    return Err(refuse(
                        Obligation::ProviderClaim,
                        format!("leaf {} declares no guarantee {fact}", edge.provider),
                    ));
                }
            }
        }
        resolved.push(Resolved {
            consumer,
            interface: edge.interface,
            provider,
            view_field: view.field,
            facts: edge.facts.clone(),
        });
    }
    for (position, leaf) in manifest.leaves.iter().enumerate() {
        for field in compiled[position].fields(Source::Context) {
            let feeding = manifest
                .edges
                .iter()
                .filter(|edge| edge.consumer == leaf.id && edge.interface == field.field)
                .count();
            // In a `/2` manifest an unfed field is an environment input,
            // which the group checks account for.
            if feeding > 1 || (feeding == 0 && manifest.hierarchy.is_none()) {
                return Err(Refusal::new(
                    Stage::Interface,
                    Obligation::ProviderBinding,
                    format!(
                        "leaf {} `{}` context.102.{}",
                        leaf.id, leaf.name, field.field
                    ),
                    format!("interface field has {feeding} providers; it needs exactly one"),
                ));
            }
        }
    }
    Ok(resolved)
}

/// A leaf's root field IDs.
struct Roots {
    state: Vec<u16>,
    command: Vec<u16>,
    context: Vec<u16>,
}

impl Roots {
    fn of(compiled: &Compiled) -> Self {
        let ids = |source: Source| {
            compiled
                .fields(source)
                .into_iter()
                .map(|field| field.field)
                .collect()
        };
        Self {
            state: ids(Source::State),
            command: ids(Source::Command),
            context: ids(Source::Context),
        }
    }

    /// The source a field ID belongs to, as the manifest numbers sources.
    fn source(&self, field: u16) -> Option<(u8, u16)> {
        [&self.state, &self.command, &self.context]
            .iter()
            .position(|fields| fields.contains(&field))
            .and_then(|source| u8::try_from(source).ok())
            .map(|source| (source, field))
    }
}

/// Where a leaf's field lives in the composed state and input: its own
/// slot, its command, or the slot of the provider view that feeds it.
/// `None` for an interface field no edge resolves, which the judge refuses.
fn location(
    manifest: &Manifest,
    leaf: &Leaf,
    source: u8,
    field: u16,
) -> Option<Result<AccessPath, ComposeError>> {
    let slot_of = |leaf: &Leaf, field: u16| {
        leaf.owns
            .iter()
            .find(|owned| owned.field == field)
            .map(|owned| owned.slot)
    };
    match source {
        0 => slot_of(leaf, field).map(state),
        1 => Some(AccessPath::try_new(
            COMMAND,
            vec![PathAtom::TupleIndex(leaf.id), PathAtom::Field(field)],
        )),
        _ => {
            let Some(edge) = manifest
                .edges
                .iter()
                .find(|edge| edge.consumer == leaf.id && edge.interface == field)
            else {
                return manifest
                    .hierarchy
                    .is_some()
                    .then(|| environment(leaf.id, field));
            };
            let provider = manifest
                .leaves
                .iter()
                .find(|candidate| candidate.id == edge.provider)?;
            slot_of(provider, provider.view(edge.view)?.field).map(state)
        }
    }
}

fn state(slot: u32) -> Result<AccessPath, ComposeError> {
    AccessPath::try_new(STATE, vec![PathAtom::TupleIndex(slot)])
}

fn environment(leaf: u32, field: u16) -> Result<AccessPath, ComposeError> {
    AccessPath::try_new(
        ENVIRONMENT,
        vec![PathAtom::TupleIndex(leaf), PathAtom::Field(field)],
    )
}

/// A group as a component: it writes nothing and reads its inputs and
/// exports.
fn group_component(manifest: &Manifest, group: &Group) -> Result<ComponentContract, Refusal> {
    let at = |key: &str| format!("group {} `{}` {key}", group.id, group.name);
    let leaf = |id: u32| manifest.leaves.iter().find(|leaf| leaf.id == id);
    let mut inputs = BTreeSet::new();
    for input in &group.inputs {
        if let Some(entry) = leaf(input.leaf)
            && let Some(path) = location(manifest, entry, 2, input.field)
        {
            inputs.insert(path.map_err(|error| internal(&at("inputs"), error))?);
        }
    }
    let mut exports = BTreeSet::new();
    for export in &group.exports {
        if let Some(entry) = leaf(export.leaf)
            && let Some(view) = entry.view(export.view)
            && let Some(path) = location(manifest, entry, 0, view.field)
        {
            exports.insert(path.map_err(|error| internal(&at("exports"), error))?);
        }
    }
    let set = |paths: &BTreeSet<AccessPath>, key: &str| {
        PathSet::try_new(paths.iter().cloned().collect()).map_err(|error| {
            constructor(
                Stage::Hierarchy,
                Obligation::GroupInterface,
                at(key),
                "compose refuses one of its path sets",
                error,
            )
        })
    };
    let reads = set(&inputs.union(&exports).cloned().collect(), "inputs")?;
    let mut guarantees: Vec<Guarantee> = group
        .guarantee
        .iter()
        .map(|guarantee| {
            Ok(Guarantee::new(
                group_guarantee_claim(group, guarantee),
                set(&exports, "exports")?,
            ))
        })
        .collect::<Result<_, Refusal>>()?;
    guarantees.push(Guarantee::new(
        delivery_claim(group),
        set(&inputs, "inputs")?,
    ));
    let is_root = manifest
        .hierarchy
        .as_ref()
        .is_some_and(|hierarchy| hierarchy.root == group.id);
    let assumptions = if is_root || group.inputs.is_empty() {
        Vec::new()
    } else {
        vec![Assumption::new(
            group_assumption_claim(group),
            set(&inputs, "inputs")?,
        )]
    };
    let mut profile = GROUP.to_vec();
    profile.extend_from_slice(&group.canonical_bytes());
    ComponentContract::try_new(
        ComponentId::new(group.id),
        Hash32::new(sha256(&profile)),
        Footprint::new(reads, PathSet::empty(), PathSet::empty(), PathSet::empty()),
        assumptions,
        guarantees,
        Vec::new(),
    )
    .map_err(|error| {
        constructor(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            at("id"),
            "compose refuses the group as a component",
            error,
        )
    })
}

/// The claim of one conjunct of a group's outer guarantee.
fn group_guarantee_claim(group: &Group, guarantee: &manifest::GroupGuarantee) -> Hash32 {
    let mut bytes = GROUP_GUARANTEE.to_vec();
    bytes.extend_from_slice(&group.id.to_be_bytes());
    bytes.extend_from_slice(&guarantee.id.to_be_bytes());
    text(&mut bytes, &guarantee.formula);
    Hash32::new(sha256(&bytes))
}

/// The claim of a group's outer assumption: its inputs and its conjuncts.
fn group_assumption_claim(group: &Group) -> Hash32 {
    let mut bytes = GROUP_ASSUMPTION.to_vec();
    bytes.extend_from_slice(&group.canonical_bytes());
    Hash32::new(sha256(&bytes))
}

/// The claim that a group delivers its outer assumption to its children.
fn delivery_claim(group: &Group) -> Hash32 {
    let mut bytes = DELIVERY.to_vec();
    bytes.extend_from_slice(&group.canonical_bytes());
    Hash32::new(sha256(&bytes))
}

/// The claim of a declared guarantee: its leaf's ID and program, its ID,
/// kind, scope and formula. The leaf's ID keeps two leaves that bind the
/// same program from sharing a claim.
fn guarantee_claim(leaf: &Leaf, guarantee: &manifest::Guarantee) -> Hash32 {
    let mut bytes = GUARANTEE.to_vec();
    bytes.extend_from_slice(&leaf.id.to_be_bytes());
    bytes.extend_from_slice(&leaf.program_sha256);
    bytes.extend_from_slice(&guarantee.id.to_be_bytes());
    bytes.push(guarantee.kind.code());
    bytes.push(scope_code(guarantee.scope));
    text(&mut bytes, &guarantee.formula);
    Hash32::new(sha256(&bytes))
}

/// The claim of a leaf's assumption: its ID and program, its formula and
/// the interface fields it is made over.
fn assumption_claim(leaf: &Leaf, interface: &[u16]) -> Hash32 {
    let mut bytes = ASSUMPTION.to_vec();
    bytes.extend_from_slice(&leaf.id.to_be_bytes());
    bytes.extend_from_slice(&leaf.program_sha256);
    text(&mut bytes, &leaf.assumption);
    bytes.extend_from_slice(
        &u32::try_from(interface.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for field in interface {
        bytes.extend_from_slice(&field.to_be_bytes());
    }
    Hash32::new(sha256(&bytes))
}

/// The claim an edge names for a provider fact: the declared guarantee's
/// claim, or, for a leaf or fact the manifest does not declare, a claim no
/// declared guarantee has.
fn fact_claim(manifest: &Manifest, provider: u32, fact: u32) -> Hash32 {
    manifest
        .leaves
        .iter()
        .find(|leaf| leaf.id == provider)
        .and_then(|leaf| {
            leaf.guarantees
                .iter()
                .find(|guarantee| guarantee.id == fact)
                .map(|guarantee| guarantee_claim(leaf, guarantee))
        })
        .unwrap_or_else(|| {
            let mut bytes = UNDECLARED.to_vec();
            bytes.extend_from_slice(&provider.to_be_bytes());
            bytes.extend_from_slice(&fact.to_be_bytes());
            Hash32::new(sha256(&bytes))
        })
}

fn constructor(
    stage: Stage,
    obligation: Obligation,
    place: impl Into<String>,
    reason: &str,
    error: ComposeError,
) -> Refusal {
    Refusal::new(stage, obligation, place, reason).with_compose(ComposeFinding::Constructor(error))
}

/// A compose refusal of a value this module builds within compose's bounds.
fn internal(place: &str, error: ComposeError) -> Refusal {
    constructor(
        Stage::Manifest,
        Obligation::ClosedManifestForm,
        place,
        "compose refuses a value derived from the manifest",
        error,
    )
}
