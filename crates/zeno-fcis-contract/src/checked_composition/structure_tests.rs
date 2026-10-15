//! The structural judge against the checker's earlier, self-contained
//! structural rules, kept here verbatim as the reference: on the ring and on every
//! control whose leaves compile, compose and the checker's remaining rules
//! refuse exactly where the reference refuses, with the same obligation, and
//! compose decides every rule it can express.

use std::collections::BTreeMap;

use serde_json::json;

use super::leaf::{self, Compiled};
use super::manifest::{GuaranteeKind, Manifest};
use super::structure;
use super::tests::{Ring, fail};
use super::{Obligation, Refusal, Stage};
use crate::internals::declarations::Source;

/// The earlier, self-contained structural rules.
mod reference {
    use super::*;

    pub(super) fn judge(manifest: &Manifest, compiled: &[Compiled]) -> Result<(), Refusal> {
        ownership(manifest)?;
        structure(manifest, compiled)
    }

    /// Every global slot is owned by exactly one leaf field, and the slots are
    /// `0..n` with none missing.
    fn ownership(manifest: &Manifest) -> Result<usize, Refusal> {
        let mut slots: BTreeMap<u32, (u32, u16)> = BTreeMap::new();
        for leaf in &manifest.leaves {
            for owned in &leaf.owns {
                if let Some((other, field)) = slots.insert(owned.slot, (leaf.id, owned.field)) {
                    return Err(Refusal::new(
                        Stage::Ownership,
                        Obligation::DisjointOwnership,
                        format!("slot {}", owned.slot),
                        format!(
                            "claimed by leaf {other} field {field} and by leaf {} field {}",
                            leaf.id, owned.field
                        ),
                    ));
                }
            }
        }
        if let Some((index, slot)) = slots
            .keys()
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

    /// Each interface field is fed by exactly one edge from a distinct provider,
    /// through a view of identical domain, with facts that are the provider's
    /// StateFacts and read only that view.
    fn structure(manifest: &Manifest, compiled: &[Compiled]) -> Result<(), Refusal> {
        let index = |id: u32| manifest.leaves.iter().position(|leaf| leaf.id == id);
        for (position, edge) in manifest.edges.iter().enumerate() {
            let place = format!("edges[{position}]");
            let refuse = |obligation: Obligation, reason: String| {
                Refusal::new(Stage::Interface, obligation, &place, reason)
            };
            let (Some(consumer), Some(provider)) = (index(edge.consumer), index(edge.provider))
            else {
                return Err(refuse(
                    Obligation::ProviderBinding,
                    format!(
                        "names leaf {} or {}, which the manifest does not list",
                        edge.consumer, edge.provider
                    ),
                ));
            };
            if consumer == provider {
                return Err(refuse(
                    Obligation::ProviderBinding,
                    "a leaf cannot provide its own interface; peer views come from another leaf"
                        .into(),
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
                    None => {
                        return Err(refuse(
                            Obligation::ProviderClaim,
                            format!("leaf {} declares no guarantee {fact}", edge.provider),
                        ));
                    }
                }
            }
        }
        for (position, leaf) in manifest.leaves.iter().enumerate() {
            for field in compiled[position].fields(Source::Context) {
                let feeding = manifest
                    .edges
                    .iter()
                    .filter(|edge| edge.consumer == leaf.id && edge.interface == field.field)
                    .count();
                if feeding != 1 {
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
        Ok(())
    }
}

/// Who decides a control's structure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decider {
    /// Both judges accept the structure.
    Accepted,
    /// Compose refuses, through its constructors, blockers or conflicts.
    Compose,
    /// Compose accepts and one of the checker's own rules refuses.
    Checker,
    /// A leaf does not compile, so no structure is judged.
    NotCompiled,
}

/// Leaf 0 with a second state field, `mirror` 111, which every copy writes.
fn with_mirror(ring: &mut Ring) {
    ring.leaves[0].project = ring.leaves[0].project.replace(
        "field 110 100 x 103;",
        "field 110 100 x 103;\nfield 111 100 mirror 103;",
    );
    ring.leaves[0].rules = ring.leaves[0]
        .rules
        .replace(
            "\"110\": \"new_value\"",
            "\"110\": \"new_value\",\n        \"111\": \"context.102.202\"",
        )
        .replace("\"110\": 0", "\"110\": 0,\n    \"111\": 0");
    ring.repin(0);
}

/// A control's name and sources, the judge expected to decide its structure
/// and the stage and obligation of the refusal it gives.
type Control = (
    &'static str,
    Ring,
    Decider,
    Option<(&'static str, &'static str)>,
);

/// The ring and every control built from source files.
fn controls() -> Vec<Control> {
    use Decider::{Accepted, Checker, Compose, NotCompiled};
    let ring = || Ring::new(2);
    let mut controls = Vec::new();
    let mut add =
        |name, ring: Ring, decider, refusal| controls.push((name, ring, decider, refusal));
    add("ring of two", ring(), Accepted, None);
    add("ring of four", Ring::new(4), Accepted, None);

    let mut control = ring();
    control.manifest["edges"] = json!([control.manifest["edges"][1].clone()]);
    add(
        "missing provider",
        control,
        Compose,
        Some(("interface", "provider binding")),
    );
    let mut control = ring();
    control.manifest["edges"][0]["facts"] = json!([9]);
    add(
        "undeclared fact",
        control,
        Compose,
        Some(("interface", "provider claim")),
    );
    let mut control = ring();
    control.manifest["edges"][0]["provider"] = json!(7);
    add(
        "unknown provider",
        control,
        Compose,
        Some(("interface", "provider binding")),
    );
    let mut control = ring();
    control.manifest["leaves"][1]["owns"][0]["slot"] = json!(0);
    add(
        "overlapping owners",
        control,
        Compose,
        Some(("ownership", "disjoint ownership")),
    );
    let mut control = ring();
    with_mirror(&mut control);
    control.manifest["leaves"][0]["owns"] =
        json!([{"field": 110, "slot": 0}, {"field": 111, "slot": 0}]);
    add(
        "one leaf claims a slot twice",
        control,
        Compose,
        Some(("ownership", "disjoint ownership")),
    );

    let mut control = ring();
    control.manifest["edges"][0]["facts"] = json!([2]);
    add(
        "swapped claim",
        control,
        Checker,
        Some(("interface", "provider claim")),
    );
    let mut control = ring();
    control.leaves[0].project = control.leaves[0].project.replace(
        "field 202 102 peer 103;",
        "type 104 int PeerValue in 0..=6;\nfield 202 102 peer 104;",
    );
    control.repin(0);
    add(
        "changed view domain",
        control,
        Checker,
        Some(("interface", "typed view")),
    );
    let mut control = ring();
    control.manifest["edges"][0]["provider"] = json!(1);
    add(
        "self provider",
        control,
        Checker,
        Some(("interface", "provider binding")),
    );
    let mut control = ring();
    let edges = control.manifest["edges"]
        .as_array_mut()
        .unwrap_or_else(|| fail("edges"));
    edges.push(json!({"consumer": 7, "interface": 202, "provider": 1, "view": 1, "facts": [1]}));
    add(
        "unknown consumer",
        control,
        Checker,
        Some(("interface", "provider binding")),
    );
    let mut control = ring();
    control.manifest["edges"][0]["interface"] = json!(203);
    add(
        "interface field the consumer lacks",
        control,
        Checker,
        Some(("interface", "provider binding")),
    );
    let mut control = ring();
    control.manifest["edges"][0]["view"] = json!(2);
    add(
        "unknown view",
        control,
        Checker,
        Some(("interface", "typed view")),
    );
    let mut control = ring();
    control.manifest["leaves"][1]["owns"][0]["slot"] = json!(5);
    add(
        "slot gap",
        control,
        Checker,
        Some(("ownership", "complete layout")),
    );
    let mut control = ring();
    with_mirror(&mut control);
    control.manifest["leaves"][0]["owns"] =
        json!([{"field": 110, "slot": 0}, {"field": 111, "slot": 2}]);
    control.manifest["leaves"][0]["guarantees"][0]["formula"] =
        json!("post.100.110 <= 3 && post.100.111 <= 7");
    add(
        "fact reads more than the view",
        control,
        Checker,
        Some(("interface", "typed view")),
    );

    // Controls whose structure is sound; a later check refuses them.
    let mut control = ring();
    let other = control.manifest["leaves"][1]["program_sha256"].clone();
    control.manifest["leaves"][0]["program_sha256"] = other;
    add("swapped program", control, Accepted, None);
    let mut control = ring();
    control.manifest["leaves"][0]["assumption"] =
        json!("command.101.201 == 4 -> context.102.202 <= 4");
    add("weaker assumption", control, Accepted, None);
    let mut control = ring();
    control.manifest["leaves"][1]["guarantees"][0]["formula"] = json!("post.100.110 <= 4");
    add("weaker provider fact", control, Accepted, None);
    let mut control = ring();
    control.leaves[1].rules = control.leaves[1].rules.replace("\"110\": 0", "\"110\": 4");
    control.repin(1);
    add("invalid genesis", control, Accepted, None);
    let mut control = ring();
    for index in 0..2 {
        control.manifest["leaves"][index]["invariant"] = json!("true");
    }
    add("circular self-support", control, Accepted, None);
    let mut control = ring();
    with_mirror(&mut control);
    control.manifest["leaves"][0]["owns"] =
        json!([{"field": 110, "slot": 0}, {"field": 111, "slot": 2}]);
    add("hidden write", control, Accepted, None);
    let mut control = ring();
    control.leaves[0].rules = control.leaves[0].rules.replace(
        "\"cases\": [",
        "\"cases\": [\n    {\"rule\": \"never\", \"when\": \"pre.100.110 > 7\", \"class\": \"Accept\", \"reason\": null, \"post\": {\"110\": \"pre.100.110 + 9223372036854775807 - 9223372036854775807\"}, \"outbox\": []},",
    );
    control.repin(0);
    add("eager unused trap", control, Accepted, None);
    let mut control = ring();
    let mut rules: serde_json::Value =
        serde_json::from_str(&control.leaves[1].rules).unwrap_or_else(fail);
    let first = json!({"rule": "set", "when": "command.101.201 < 4", "class": "Accept",
                       "reason": null, "post": {"110": "new_value"}, "outbox": []});
    rules["cases"] = json!([first, rules["cases"][0].clone(), rules["cases"][1].clone()]);
    control.leaves[1].rules = super::tests::pretty(&rules);
    control.repin(1);
    add("wrong refusal precedence", control, Accepted, None);
    let mut control = ring();
    control.leaves[0].rules = control.leaves[0].rules.replace(
        "\"new_value == pre.100.110\"",
        "\"new_value == pre.100.110 && new_value > 7\"",
    );
    control.repin(0);
    control.manifest["leaves"][0]["guarantees"] = json!([
        {"id": 1, "kind": "StateFact", "scope": null, "formula": "post.100.110 <= 3"}
    ]);
    add("missing witness", control, Accepted, None);
    let mut control = ring();
    for index in 0..2 {
        control.leaves[index].project = control.leaves[index]
            .project
            .replace("in 0..=7", "in 0..=4095");
        control.repin(index);
    }
    add("local domain above the limit", control, Accepted, None);

    // Controls refused before any structure is judged.
    let mut control = ring();
    control.leaves.swap(0, 1);
    add("swapped sources", control, NotCompiled, None);
    let mut control = ring();
    control.manifest["leaves"][1]["views"][0]["field"] = json!(111);
    add("view of an undeclared field", control, NotCompiled, None);
    let mut control = ring();
    control.leaves[1].project = control.leaves[1].project.replace("in 0..=7", "in 0..=8");
    add("changed source without a pin", control, NotCompiled, None);
    let mut control = ring();
    control.manifest["leaves"][0]["assumption"] = json!("pre.100.110 <= 3");
    add("assumption over the state", control, NotCompiled, None);
    controls
}

fn compile(ring: &Ring) -> Option<(Manifest, Vec<Compiled>)> {
    let manifest = Manifest::read(&ring.manifest_text()).unwrap_or_else(fail);
    let compiled = manifest
        .leaves
        .iter()
        .zip(&ring.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some((manifest, compiled))
}

fn named(refusal: &Refusal) -> (&'static str, &'static str) {
    (refusal.stage().name(), refusal.obligation().name())
}

#[test]
fn compose_and_the_checker_rules_refuse_exactly_where_the_reference_refuses() {
    let mut rows = Vec::new();
    for (name, ring, decider, expected) in controls() {
        let Some((manifest, compiled)) = compile(&ring) else {
            assert_eq!(decider, Decider::NotCompiled, "{name} does not compile");
            rows.push((name, Decider::NotCompiled));
            continue;
        };
        let reference = reference::judge(&manifest, &compiled);
        let judged = structure::judge(&manifest, &compiled);
        let compose = structure::specification(&manifest, &compiled)
            .and_then(|spec| structure::compose_verdict(&manifest, &compiled, &spec));
        assert_eq!(
            reference.as_ref().err().map(named),
            judged.as_ref().err().map(named),
            "{name}: the judges differ"
        );
        assert_eq!(judged.as_ref().err().map(named), expected, "{name}");
        let observed = match (&compose, &judged) {
            (_, Ok(_)) => {
                assert!(
                    compose.is_ok(),
                    "{name}: compose refuses an accepted structure"
                );
                Decider::Accepted
            }
            (Err(refusal), Err(judged)) => {
                // The verdict is compose's own finding, carried as it is.
                assert_eq!(refusal, judged, "{name}");
                assert!(judged.compose().is_some(), "{name}: no compose finding");
                Decider::Compose
            }
            (Ok(_), Err(judged)) => {
                assert!(
                    judged.compose().is_none(),
                    "{name}: a compose finding without a refusal"
                );
                Decider::Checker
            }
        };
        assert_eq!(observed, decider, "{name}");
        rows.push((name, observed));
    }
    let count = |decider| rows.iter().filter(|(_, row)| *row == decider).count();
    assert_eq!(
        (
            count(Decider::Accepted),
            count(Decider::Compose),
            count(Decider::Checker),
            count(Decider::NotCompiled),
        ),
        (12, 5, 8, 4)
    );
}

#[test]
fn compose_findings_name_the_rule_that_refused() {
    use zeno_fcis_compose::{CompositionBlocker, ContractError as ComposeError};
    let find = |name: &str| {
        let (_, ring, ..) = controls()
            .into_iter()
            .find(|(control, ..)| *control == name)
            .unwrap_or_else(|| fail(name));
        ring.refusal()
    };
    let refusal = find("overlapping owners");
    assert!(matches!(
        refusal.compose(),
        Some(super::ComposeFinding::WriteWrite { left, right })
            if left.get() == 1 && right.get() == 2
    ));
    assert_eq!(
        refusal.json()["compose"],
        json!({"conflict": "write-write", "left": 1, "right": 2})
    );
    let refusal = find("one leaf claims a slot twice");
    assert_eq!(
        refusal.compose(),
        Some(&super::ComposeFinding::Constructor(
            ComposeError::DuplicatePath
        ))
    );
    let refusal = find("missing provider");
    assert!(matches!(
        refusal.compose(),
        Some(super::ComposeFinding::Blocker(
            CompositionBlocker::MissingAssumptionDischarge { component, .. }
        )) if component.get() == 1
    ));
    for name in ["undeclared fact", "unknown provider"] {
        let refusal = find(name);
        assert!(
            matches!(
                refusal.compose(),
                Some(super::ComposeFinding::Blocker(
                    CompositionBlocker::UnknownProviderGuarantee { .. }
                ))
            ),
            "{name}: {}",
            refusal.json()
        );
        assert_eq!(refusal.place(), "edges[0]", "{name}");
    }
}

#[test]
fn the_specification_is_exact_and_its_write_conflicts_are_owned_slots() {
    use zeno_fcis_compose::{AccessPath, PathAtom, conflicts};
    let ring = Ring::new(2);
    let (manifest, compiled) = compile(&ring).unwrap_or_else(|| fail("ring"));
    let spec = structure::specification(&manifest, &compiled).unwrap_or_else(fail);
    assert_eq!(spec.version(), structure::SPEC_VERSION);
    assert_eq!(spec.components().len(), 2);
    let state =
        |slot| AccessPath::try_new(1, vec![PathAtom::TupleIndex(slot)]).unwrap_or_else(fail);
    let command = |leaf, field| {
        AccessPath::try_new(2, vec![PathAtom::TupleIndex(leaf), PathAtom::Field(field)])
            .unwrap_or_else(fail)
    };
    for (index, component) in spec.components().iter().enumerate() {
        let own = u32::try_from(index).unwrap_or_else(fail);
        let peer = 1 - own;
        let id = own + 1;
        assert_eq!(component.id().get(), id);
        assert_eq!(component.footprint().writes().paths(), &[state(own)]);
        // Its own register, its command and the peer's register through the view.
        let mut reads = vec![state(own), state(peer), command(id, 201)];
        reads.sort();
        assert_eq!(component.footprint().reads().paths(), reads.as_slice());
        assert_eq!(component.guarantees().len(), 3);
        assert_eq!(component.assumptions().len(), 1);
        let mut depends = vec![state(peer), command(id, 201)];
        depends.sort();
        assert_eq!(
            component.assumptions()[0].depends_on().paths(),
            depends.as_slice()
        );
    }
    // A peer's read of a slot another leaf writes is a read-write conflict,
    // which this profile allows; there is no write-write conflict.
    let kinds: Vec<_> = conflicts(
        spec.components()[0].footprint(),
        spec.components()[1].footprint(),
    )
    .iter()
    .map(|conflict| conflict.kind())
    .collect();
    assert!(!kinds.contains(&zeno_fcis_compose::ConflictKind::WriteWrite));
    assert!(!kinds.is_empty());
    // The same sources give the same specification.
    let again = structure::specification(&manifest, &compiled).unwrap_or_else(fail);
    assert_eq!(spec, again);
}
