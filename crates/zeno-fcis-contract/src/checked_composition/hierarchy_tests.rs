//! The register ring and chain under groups: equation 2.8 at each group,
//! environment assumptions rising to the root, the claim's condition, the
//! controls, and regrouping that keeps the aggregate and its publications.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::journal_tests::{Step, journal};
use super::tests::{Ring, assert_refused, fail};
use super::{
    ClaimKind, ComposeRefusal, CompositionSources, Refusal, aggregate, check, require_checked,
};

/// The ring's manifest under `groups` with root `root`.
pub(super) fn grouped(ring: &Ring, groups: Value, root: u32) -> Ring {
    let mut grouped = ring.clone();
    grouped.manifest["schema"] = json!("zeno-fcis/checked-composition/2");
    grouped.manifest["groups"] = groups;
    grouped.manifest["root"] = json!(root);
    grouped
}

/// The chain of `n` registers: the ring without the edge into leaf 1, whose
/// peer view becomes an environment input.
pub(super) fn chain(n: usize) -> Ring {
    let mut ring = Ring::new(n);
    let edges = ring.manifest["edges"]
        .as_array_mut()
        .unwrap_or_else(|| fail("edges"));
    edges.retain(|edge| edge["consumer"] != json!(1));
    ring
}

pub(super) fn group(id: u32, name: &str, children: &[u32]) -> Value {
    json!({"id": id, "name": name, "children": children, "inputs": [], "exports": [],
           "assumption": [], "guarantee": []})
}

/// The leaves and edges of the four-register ring as two pairs under a root:
/// each pair's second register reads from the other pair.
fn pairs(ring: &Ring, guarantee: &str) -> Ring {
    let mut left = group(11, "left", &[1, 2]);
    left["inputs"] = json!([{"id": 1, "leaf": 2, "field": 202}]);
    left["exports"] = json!([{"id": 1, "leaf": 1, "view": 1}, {"id": 2, "leaf": 2, "view": 1}]);
    left["guarantee"] = json!([{"id": 1, "formula": guarantee}]);
    let mut right = group(12, "right", &[3, 4]);
    right["inputs"] = json!([{"id": 1, "leaf": 4, "field": 202}]);
    right["exports"] = json!([{"id": 1, "leaf": 3, "view": 1}, {"id": 2, "leaf": 4, "view": 1}]);
    right["guarantee"] = json!([{"id": 1, "formula": "post.100.1 + post.100.2 <= 6"}]);
    let mut root = group(10, "ring", &[11, 12]);
    root["exports"] = json!([
        {"id": 1, "leaf": 1, "view": 1}, {"id": 2, "leaf": 2, "view": 1},
        {"id": 3, "leaf": 3, "view": 1}, {"id": 4, "leaf": 4, "view": 1}
    ]);
    root["guarantee"] = json!([
        {"id": 1, "formula": "post.100.1 + post.100.2 + post.100.3 + post.100.4 <= 12"}
    ]);
    grouped(ring, json!([root, left, right]), 10)
}

fn checked(ring: &Ring) -> Result<super::CheckedComposition, Refusal> {
    ring.check()
}

fn refusal(ring: &Ring) -> Refusal {
    match checked(ring) {
        Ok(result) => panic!(
            "expected a refusal, checked: {}",
            result.json()["hierarchy"]
        ),
        Err(refusal) => refusal,
    }
}

fn generated(ring: &Ring) -> super::AggregateSources {
    let manifest = ring.manifest_text();
    aggregate(CompositionSources {
        manifest: &manifest,
        leaves: &ring.leaves,
        aggregate: None,
    })
    .unwrap_or_else(fail)
}

#[test]
fn nested_groups_check_equation_two_eight() {
    let ring = pairs(&Ring::new(4), "post.100.1 + post.100.2 <= 6");
    let result = checked(&ring).unwrap_or_else(fail);
    let json = result.json();
    let hierarchy = &json["hierarchy"];
    assert_eq!(
        json["manifest"]["schema"],
        "zeno-fcis/checked-composition/2"
    );
    assert_eq!(hierarchy["root"], 10);
    // The pairs' environment assumptions rise to the root, where the other
    // pair's StateFacts discharge them: the claim is unconditional.
    assert_eq!(
        hierarchy["claim"],
        json!({"kind": "unconditional", "conditions": []})
    );
    let groups = hierarchy["groups"]
        .as_array()
        .unwrap_or_else(|| fail("groups"));
    let by_id = |id: u32| {
        groups
            .iter()
            .find(|group| group["id"] == id)
            .unwrap_or_else(|| fail("group"))
    };
    // Each pair discharges its first register's assumption inside and lifts
    // the second's, which reads the other pair's register.
    for id in [11, 12] {
        assert_eq!(by_id(id)["discharged"], 1, "{}", by_id(id));
        assert_eq!(by_id(id)["lifted"].as_array().map(Vec::len), Some(1));
    }
    assert_eq!(by_id(10)["discharged"], 2);
    // (ii): each pair's sum from its leaves' StateFacts, 8 * 8 rows; the
    // root's sum from the pairs' guarantees, 8^4 rows.
    assert_eq!(by_id(11)["tables"]["guarantee_rows"], 64);
    assert_eq!(by_id(10)["tables"]["guarantee_rows"], 4096);
    assert_eq!(json["totals"]["group_guarantee_rows"], 64 + 64 + 4096);
    // The composition specification has the groups as components.
    assert_eq!(result.composition_spec().components().len(), 4 + 3);
}

#[test]
fn regrouping_keeps_the_aggregate_and_its_publications() {
    let flat = Ring::new(4);
    let root_only = grouped(&flat, json!([group(10, "ring", &[1, 2, 3, 4])]), 10);
    let nested = pairs(&flat, "post.100.1 + post.100.2 <= 6");
    let files = generated(&flat);
    for regrouped in [&root_only, &nested] {
        checked(regrouped).unwrap_or_else(fail);
        assert_eq!(
            generated(regrouped),
            files,
            "the aggregate depends only on the leaves"
        );
    }
    // The same commands through each grouping's aggregate publish the same
    // bytes.
    let built = super::assembly::CompiledAggregate::read(&files).unwrap_or_else(fail);
    let target = built
        .declarations
        .fields(101)
        .unwrap_or_else(fail)
        .iter()
        .find(|field| field.name == "target")
        .map(|field| field.id)
        .unwrap_or_else(|| fail("target"));
    let steps: Vec<Step> = (0..12)
        .map(|at| Step {
            command: BTreeMap::from([(target, at % 4), (201, (at * 3) % 5)]),
            context: BTreeMap::new(),
        })
        .collect();
    let reference = journal(&files, &steps);
    for regrouped in [&root_only, &nested] {
        assert_eq!(journal(&generated(regrouped), &steps), reference);
    }
    assert!(reference.entries.iter().any(|entry| entry.kind == "commit"));
}

#[test]
fn an_environment_assumption_rises_to_the_root_and_conditions_the_claim() {
    // Leaf 1 reads its peer from the environment, under a root of all three.
    let ring = chain(3);
    let mut root = group(10, "chain", &[1, 2, 3]);
    root["inputs"] = json!([{"id": 1, "leaf": 1, "field": 202}]);
    let conditional = grouped(&ring, json!([root.clone()]), 10);
    let result = checked(&conditional).unwrap_or_else(fail);
    let claim = &result.json()["hierarchy"]["claim"];
    // Leaf 1's assumption reads the action and the input: conditional.
    assert_eq!(claim["kind"], "conditional");
    assert_eq!(claim["conditions"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        claim["conditions"][0]["formula"],
        "command.101.201 == 4 -> context.102.202 <= 3"
    );
    // A named external assumption on the supplied context discharges it.
    root["assumption"] =
        json!([{"name": "supplied-peer-bounded", "formula": "context.102.1 <= 3"}]);
    let external = grouped(&ring, json!([root.clone()]), 10);
    let result = checked(&external).unwrap_or_else(fail);
    let claim = &result.json()["hierarchy"]["claim"];
    assert_eq!(
        claim["kind"],
        "unconditional under named external assumptions"
    );
    assert_eq!(
        claim["conditions"][0]["origin"],
        "group 10 `chain` assumption `supplied-peer-bounded`"
    );
    // The aggregate reads the input as a context field, and is validated.
    let files = generated(&external);
    assert!(
        files.project.contains(" 102 leaf_0_peer "),
        "{}",
        files.project
    );
    let manifest = external.manifest_text();
    check(CompositionSources {
        manifest: &manifest,
        leaves: &external.leaves,
        aggregate: Some(&files),
    })
    .unwrap_or_else(fail);
    // An assumption that holds on no input would make every claim vacuous.
    root["assumption"] = json!([{"name": "never", "formula": "context.102.1 < 0"}]);
    let vacuous = grouped(&ring, json!([root]), 10);
    assert_refused(
        &refusal(&vacuous),
        "hierarchy",
        "outer assumption discharges inner assumptions",
    );
}

#[test]
fn an_outer_assumption_that_does_not_discharge_an_inner_one_is_refused() {
    // The left pair assumes its input is at most 2; at the root that input is
    // fed by register 3, whose StateFact gives only at most 3.
    let mut ring = pairs(&Ring::new(4), "post.100.1 + post.100.2 <= 6");
    ring.manifest["groups"][1]["assumption"] =
        json!([{"name": "peer-small", "formula": "context.102.1 <= 2"}]);
    let refusal = refusal(&ring);
    assert_refused(
        &refusal,
        "hierarchy",
        "outer assumption discharges inner assumptions",
    );
    assert!(
        refusal.place().contains("peer-small"),
        "{}",
        refusal.place()
    );
    assert_eq!(
        refusal.witness(),
        Some(&json!({"leaf 2 context.102.202": 3}))
    );
}

#[test]
fn an_outer_guarantee_not_implied_is_refused() {
    let ring = pairs(&Ring::new(4), "post.100.1 + post.100.2 <= 5");
    let refusal = refusal(&ring);
    assert_refused(
        &refusal,
        "hierarchy",
        "outer guarantee implied by inner guarantees",
    );
    assert_eq!(
        refusal.witness(),
        Some(&json!({"leaf 1 post.100.110": 3, "leaf 2 post.100.110": 3}))
    );
}

#[test]
fn malformed_trees_and_interfaces_are_refused() {
    let ring = Ring::new(4);
    let tree = |groups: Value, root: u32| refusal(&grouped(&ring, groups, root));
    // A leaf in no group, in two groups, an unknown child, a cycle.
    assert_refused(
        &tree(json!([group(10, "r", &[1, 2, 3])]), 10),
        "hierarchy",
        "group tree",
    );
    assert_refused(
        &tree(
            json!([group(10, "r", &[1, 2, 3, 4, 11]), group(11, "g", &[4])]),
            10,
        ),
        "hierarchy",
        "group tree",
    );
    assert_refused(
        &tree(json!([group(10, "r", &[1, 2, 3, 4, 99])]), 10),
        "hierarchy",
        "group tree",
    );
    assert_refused(
        &tree(
            json!([
                group(10, "r", &[1, 2, 3, 4]),
                group(11, "a", &[12]),
                group(12, "b", &[11])
            ]),
            10,
        ),
        "hierarchy",
        "group tree",
    );
    // A group sharing a leaf's ID: compose refuses the duplicate component.
    let duplicate = tree(json!([group(4, "r", &[1, 2, 3, 4])]), 4);
    assert_refused(&duplicate, "manifest", "closed manifest form");
    assert!(duplicate.compose().is_some());
    // A pair that does not list its environment input.
    let mut missing = pairs(&ring, "post.100.1 + post.100.2 <= 6");
    missing.manifest["groups"][1]["inputs"] = json!([]);
    assert_refused(&refusal(&missing), "hierarchy", "group interface");
    // An export of a leaf outside the group.
    let mut outside = pairs(&ring, "post.100.1 + post.100.2 <= 6");
    outside.manifest["groups"][1]["exports"][0]["leaf"] = json!(3);
    assert_refused(&refusal(&outside), "hierarchy", "group interface");
}

#[test]
fn a_flat_manifest_keeps_its_bytes_and_schema() {
    let ring = Ring::new(2);
    let result = checked(&ring).unwrap_or_else(fail);
    let json = result.json();
    assert_eq!(
        json["manifest"]["schema"],
        "zeno-fcis/checked-composition/1"
    );
    assert_eq!(json["hierarchy"], Value::Null);
    assert_eq!(result.composition_spec().components().len(), 2);
}

/// The two-register ring where register 1 (leaf Q) takes actions 0 to 7,
/// has a StateInvariant law no register value breaks and assumes actions up
/// to 4: Q certifies, since its local table checks only inputs its
/// assumption admits, but an action of 5 would leave its invariant.
fn wide_action() -> Ring {
    let mut ring = Ring::new(2);
    let files = &mut ring.leaves[1];
    for (from, to) in [
        (
            "type 105 int Action in 0..=4;",
            "type 105 int Action in 0..=7;",
        ),
        ("= post.100.110 <= 3;", "= post.100.110 <= 7;"),
    ] {
        assert!(files.project.contains(from), "{from}");
        files.project = files.project.replace(from, to);
    }
    ring.repin(1);
    ring.manifest["leaves"][1]["assumption"] =
        json!("command.101.201 <= 4 && (command.101.201 == 4 -> context.102.202 <= 3)");
    ring
}

#[test]
fn a_command_field_shared_with_another_type_is_refused() {
    let ring = wide_action();
    // Flat, the interface table enumerates Q's own actions and refuses.
    assert_refused(
        &ring.refusal(),
        "interface",
        "provider facts => consumer assumption",
    );
    // Under a group, action 201 is one variable of the group formulas; Q's
    // wider type must not be enumerated over P's.
    let grouped = grouped(&ring, json!([group(10, "ring", &[1, 2])]), 10);
    let refusal = refusal(&grouped);
    assert_refused(
        &refusal,
        "hierarchy",
        "command fields shared by ID with one type",
    );
    assert_eq!(refusal.place(), "command.101.201");
}

fn require(ring: &Ring) -> Result<super::CheckedAssumeGuarantee, ComposeRefusal> {
    let result = checked(ring).unwrap_or_else(fail);
    let manifest = ring.manifest_text();
    require_checked(
        result.composition_spec(),
        CompositionSources {
            manifest: &manifest,
            leaves: &ring.leaves,
            aggregate: None,
        },
    )
}

#[test]
fn a_claim_with_conditions_is_not_presented_as_unconditionally_checked() {
    // Unconditional under groups: the checked class of compose's claims.
    let nested = pairs(&Ring::new(4), "post.100.1 + post.100.2 <= 6");
    let result = checked(&nested).unwrap_or_else(fail);
    assert_eq!(result.claim_kind(), ClaimKind::Unconditional);
    assert!(result.conditions().is_empty());
    let class = require(&nested).unwrap_or_else(fail);
    assert_eq!(class.claim_kind(), ClaimKind::Unconditional);
    assert!(class.claims().len() > 4);
    // Conditional: the risen leaf assumption reads the action.
    let mut root = group(10, "chain", &[1, 2, 3]);
    root["inputs"] = json!([{"id": 1, "leaf": 1, "field": 202}]);
    let conditional = grouped(&chain(3), json!([root.clone()]), 10);
    let result = checked(&conditional).unwrap_or_else(fail);
    assert_eq!(result.claim_kind(), ClaimKind::Conditional);
    assert_eq!(result.conditions().len(), 1);
    assert_eq!(
        result.conditions()[0].origin(),
        "leaf 1 `register-0` assumption"
    );
    assert_eq!(
        result.conditions()[0].reads(),
        ["leaf 1 context.102.202", "command.101.201"]
    );
    assert_eq!(
        require(&conditional),
        Err(ComposeRefusal::Conditional {
            kind: ClaimKind::Conditional,
            conditions: 1
        })
    );
    // Under a named external assumption on the supplied context.
    root["assumption"] =
        json!([{"name": "supplied-peer-bounded", "formula": "context.102.1 <= 3"}]);
    let external = grouped(&chain(3), json!([root]), 10);
    assert_eq!(
        checked(&external).unwrap_or_else(fail).claim_kind(),
        ClaimKind::External
    );
    assert_eq!(
        require(&external),
        Err(ComposeRefusal::Conditional {
            kind: ClaimKind::External,
            conditions: 1
        })
    );
    // A root conjunct that reads nothing holds, and conditions nothing.
    let mut fed = group(10, "ring", &[1, 2, 3]);
    fed["assumption"] = json!([{"name": "always", "formula": "true"}]);
    let constant = grouped(&Ring::new(3), json!([fed]), 10);
    let result = checked(&constant).unwrap_or_else(fail);
    assert_eq!(result.claim_kind(), ClaimKind::Unconditional);
    assert!(result.conditions().is_empty());
}
