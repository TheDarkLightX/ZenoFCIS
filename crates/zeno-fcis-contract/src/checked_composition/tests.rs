//! The register ring of separately bound leaves, its complete joint domain,
//! and planted defects: each one fails a named obligation or meets an
//! intended boundary refusal.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    V2Resource, v2_authority::PublicationOutcome, v2_composition as c,
};

use super::leaf::{self, sha256};
use super::manifest::Manifest;
use super::{
    AuthorityRefusal, CompositionSources, ExecutionRefusal, LawRefusal, LeafSources, MeterRefusal,
    Refusal, check, hex,
};
use crate::internals::declarations::Declarations;
use crate::internals::model::Contract;
use crate::internals::review::evaluate::Framer;
use crate::internals::rules::Rules;
use crate::internals::{policy, schema, schema_commitment};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checked-composition/register-ring-n2")
}

pub(super) fn fail<T>(error: impl std::fmt::Debug) -> T {
    panic!("test operation failed: {error:?}")
}

/// The register leaf `index`: x in 0..7, genesis `genesis`, Set(v) for
/// actions 0..3 and CopyNeighbor for action 4 from the peer view in the
/// context; an unchanged value is business Reject 300.
fn register_project(index: usize) -> String {
    format!(
        "zeno 1;
project 1 register_{index};
namespace 2 registers;
type 100 state Register;
type 101 command RegisterCommand;
type 102 context PeerView;
type 103 int RegisterValue in 0..=7;
type 105 int Action in 0..=4;
field 110 100 x 103;
field 201 101 action 105;
field 202 102 peer 103;
reason 300 unchanged precedence 0;
component 400 register {{
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 4096;
}}
merge [400];
law 500 bounded_register on commit, genesis = post.100.110 <= 3;
"
    )
}

fn register_policy(index: usize) -> String {
    pretty(&json!({
        "schema": "zeno-fcis/template-declarative-policy/2",
        "template": format!("register-{index}"),
        "roots": {"state": 100, "command": 101, "context": 102},
        "leaf_bindings": {},
        "variables": {
            "new_value": "choose(command.101.201 == 4, context.102.202, command.101.201)"
        },
        "cases": [
            {"rule": "unchanged", "when": "new_value == pre.100.110", "class": "Reject",
             "reason": 300, "post": {}, "outbox": []},
            {"rule": "changed", "when": true, "class": "Accept", "reason": null,
             "post": {"110": "new_value"}, "outbox": []}
        ],
        "genesis": {"110": 0},
        "law_kinds": {"500": "StateInvariant"}
    }))
}

pub(super) fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(fail);
    text.push('\n');
    text
}

/// The SHA-256 pins of a leaf's files and of the program they bind to,
/// through the same route the checker takes.
fn pins(project: &str, rules: &str) -> [String; 3] {
    let rules_value = Rules::read(rules).unwrap_or_else(fail);
    let declarations = Declarations::read(project, &rules_value.leaf_bindings).unwrap_or_else(fail);
    let schema = schema::encode(&declarations).unwrap_or_else(fail);
    let commitment = schema_commitment(&schema).unwrap_or_else(fail);
    let contract = Contract::build(&declarations, &rules_value, commitment).unwrap_or_else(fail);
    let program =
        policy::with_authority(&contract, &schema, |authority| sha256(authority.identity()))
            .unwrap_or_else(fail);
    [
        hex(&sha256(project.as_bytes())),
        hex(&sha256(rules.as_bytes())),
        hex(&program),
    ]
}

/// A composition held in memory: the manifest and each leaf's files.
#[derive(Clone)]
pub(super) struct Ring {
    pub(super) manifest: Value,
    pub(super) leaves: Vec<LeafSources>,
}

impl Ring {
    /// The ring of `n` registers: register i copies from register i+1 mod n.
    pub(super) fn new(n: usize) -> Self {
        let leaves: Vec<LeafSources> = (0..n)
            .map(|index| LeafSources {
                project: register_project(index),
                rules: register_policy(index),
            })
            .collect();
        let entries: Vec<Value> = leaves
            .iter()
            .enumerate()
            .map(|(index, files)| {
                let [project_sha256, policy_sha256, program_sha256] =
                    pins(&files.project, &files.rules);
                json!({
                    "id": index + 1,
                    "name": format!("register-{index}"),
                    "path": format!("register-{index}"),
                    "project_sha256": project_sha256,
                    "policy_sha256": policy_sha256,
                    "program_sha256": program_sha256,
                    "owns": [{"field": 110, "slot": index}],
                    "footprint": {
                        "reads": ["pre.100.110", "command.101.201", "context.102.202"],
                        "writes": ["post.100.110"]
                    },
                    "invariant": "post.100.110 <= 3",
                    "assumption": "command.101.201 == 4 -> context.102.202 <= 3",
                    "guarantees": [
                        {"id": 1, "kind": "StateFact", "scope": null, "formula": "post.100.110 <= 3"},
                        {"id": 2, "kind": "TransitionRelation", "scope": "accept",
                         "formula": "post.100.110 != pre.100.110"},
                        {"id": 3, "kind": "TransitionRelation", "scope": "reject",
                         "formula": "new_value == pre.100.110"}
                    ],
                    "views": [{"id": 1, "field": 110}],
                    "witnesses": ["accept", "reject"]
                })
            })
            .collect();
        let edges: Vec<Value> = (0..n)
            .map(|index| {
                json!({
                    "consumer": index + 1,
                    "interface": 202,
                    "provider": (index + 1) % n + 1,
                    "view": 1,
                    "facts": [1]
                })
            })
            .collect();
        Self {
            manifest: json!({
                "schema": "zeno-fcis/checked-composition/1",
                "name": format!("register-ring-n{n}"),
                "checker": "zeno-fcis/checked-composition-checker/1",
                "leaves": entries,
                "edges": edges
            }),
            leaves,
        }
    }

    pub(super) fn check(&self) -> Result<super::CheckedComposition, Refusal> {
        let manifest = self.manifest_text();
        check(CompositionSources {
            manifest: &manifest,
            leaves: &self.leaves,
            aggregate: None,
        })
    }

    /// The manifest's bytes.
    pub(super) fn manifest_text(&self) -> String {
        pretty(&self.manifest)
    }

    /// Re-pins leaf `index` after its files changed, as an author would.
    pub(super) fn repin(&mut self, index: usize) {
        let files = &self.leaves[index];
        let [project, rules, program] = pins(&files.project, &files.rules);
        let entry = &mut self.manifest["leaves"][index];
        entry["project_sha256"] = json!(project);
        entry["policy_sha256"] = json!(rules);
        entry["program_sha256"] = json!(program);
    }

    pub(super) fn refusal(&self) -> Refusal {
        match self.check() {
            Ok(checked) => panic!("expected a refusal, checked: {}", checked.json()),
            Err(refusal) => refusal,
        }
    }

    /// Writes the manifest and every leaf's files under `directory`.
    fn write(&self, directory: &Path) {
        let make = |path: &Path| fs::create_dir_all(path).unwrap_or_else(fail);
        make(directory);
        fs::write(directory.join("composition.json"), pretty(&self.manifest)).unwrap_or_else(fail);
        for (index, files) in self.leaves.iter().enumerate() {
            let leaf = directory.join(format!("register-{index}"));
            make(&leaf.join("v2"));
            fs::write(leaf.join("project.zeno"), &files.project).unwrap_or_else(fail);
            fs::write(leaf.join("v2/policy.json"), &files.rules).unwrap_or_else(fail);
        }
    }
}

pub(super) fn assert_refused(refusal: &Refusal, stage: &str, obligation: &str) {
    assert_eq!(
        (refusal.stage().name(), refusal.obligation().name()),
        (stage, obligation),
        "unexpected refusal: {}",
        refusal.json()
    );
}

#[test]
fn the_committed_ring_is_the_generated_ring() {
    let ring = Ring::new(2);
    let directory = fixture();
    let read = |path: PathBuf| fs::read_to_string(path).unwrap_or_else(fail);
    assert_eq!(
        read(directory.join("composition.json")),
        pretty(&ring.manifest)
    );
    for (index, files) in ring.leaves.iter().enumerate() {
        let leaf = directory.join(format!("register-{index}"));
        assert_eq!(read(leaf.join("project.zeno")), files.project);
        assert_eq!(read(leaf.join("v2/policy.json")), files.rules);
    }
}

#[test]
fn the_two_register_ring_is_checked() {
    let checked = Ring::new(2).check().unwrap_or_else(fail);
    let result = checked.json();
    assert_eq!(result["evidence"], "checked");
    assert_eq!(result["cycles"], json!([[1, 2]]));
    assert_eq!(result["slots"], 2);
    for leaf in result["leaves"]
        .as_array()
        .unwrap_or_else(|| fail("leaves"))
    {
        // 8 values of x, 5 actions and 8 peer values.
        assert_eq!(leaf["local"]["rows"], 320);
        // x <= 3; Set(0..3) with any peer, CopyNeighbor with peer <= 3.
        assert_eq!(leaf["local"]["admitted"], 144);
        assert_eq!(leaf["local"]["admitted_accept"], 108);
        assert_eq!(leaf["local"]["admitted_reject"], 36);
        // Copying a peer above 3 from a safe register is refused by law 500.
        assert_eq!(leaf["local"]["excluded_refused"], 28);
        assert_eq!(leaf["local"]["state_fact_rows"], 8);
        assert_eq!(leaf["interface_rows"], 40);
        assert_eq!(
            leaf["observed_footprint"],
            json!({"reads": ["pre.100.110", "command.101.201", "context.102.202"],
                   "writes": ["post.100.110"]})
        );
    }
    assert_eq!(result["totals"]["local_rows"], 640);
    assert_eq!(result["totals"]["interface_rows"], 80);
    // Repeating the check gives the same bytes.
    let again = Ring::new(2).check().unwrap_or_else(fail).json();
    assert_eq!(
        serde_json::to_string(&again).ok(),
        serde_json::to_string(&result).ok()
    );
}

#[test]
fn a_four_register_ring_is_checked() {
    let result = Ring::new(4).check().unwrap_or_else(fail).json();
    assert_eq!(result["cycles"], json!([[1, 2, 3, 4]]));
    assert_eq!(result["totals"]["local_rows"], 1280);
    assert_eq!(result["totals"]["interface_rows"], 160);
}

/// The reference ring step: Set(v) for actions 0..3, CopyNeighbor for 4,
/// an unchanged value rejected, as kernel37 defines `ring_post`.
fn reference(state: [i128; 2], target: usize, action: i128) -> Option<[i128; 2]> {
    let value = if action == 4 {
        state[1 - target]
    } else {
        action
    };
    if value == state[target] {
        return None;
    }
    let mut next = state;
    next[target] = value;
    Some(next)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Joint {
    Accept([i128; 2]),
    Reject,
    Refused,
}

/// Every joint state and command of the two-register ring, decided by
/// routing the command to its target leaf, whose peer view is the other
/// register of the same pre-state, and framing the other register.
#[test]
fn the_complete_joint_domain_agrees_with_the_compositional_result() {
    let ring = Ring::new(2);
    let manifest = Manifest::read(&pretty(&ring.manifest)).unwrap_or_else(fail);
    let compiled: Vec<leaf::Compiled> = manifest
        .leaves
        .iter()
        .zip(&ring.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<_, _>>()
        .unwrap_or_else(fail);
    let contracts: Vec<Contract<'_>> = compiled
        .iter()
        .map(|leaf| Contract::build(&leaf.declarations, &leaf.rules, leaf.commitment))
        .collect::<Result<_, _>>()
        .unwrap_or_else(fail);
    let framers: Vec<Framer> = compiled
        .iter()
        .zip(&contracts)
        .map(|(leaf, contract)| Framer::new(&leaf.positions, contract))
        .collect();
    let table = policy::with_authority(&contracts[0], &compiled[0].schema, |first| {
        policy::with_authority(&contracts[1], &compiled[1].schema, |second| {
            let authorities = [first, second];
            let mut rows = Vec::new();
            for x0 in 0..=7i128 {
                for x1 in 0..=7i128 {
                    for target in 0..2usize {
                        for action in 0..=4i128 {
                            let state = [x0, x1];
                            // Positions: x, action, peer.
                            let tuple = [state[target], action, state[1 - target]];
                            let frames = framers[target].frame(&tuple);
                            let outcome = authorities[target].publish(c::Raw {
                                state: &frames[0],
                                command: &frames[1],
                                context: &frames[2],
                            });
                            let joint = match &outcome {
                                PublicationOutcome::Commit(publication) => {
                                    let post = publication
                                        .evaluation()
                                        .result()
                                        .map(|candidate| candidate.post()[0].value)
                                        .unwrap_or_else(fail);
                                    let c::Atom::I128(value) = post else {
                                        return fail(post);
                                    };
                                    let mut next = state;
                                    next[target] = value;
                                    Joint::Accept(next)
                                }
                                PublicationOutcome::Reject(_) => Joint::Reject,
                                _ => Joint::Refused,
                            };
                            rows.push((state, target, action, joint));
                        }
                    }
                }
            }
            rows
        })
    })
    .unwrap_or_else(fail)
    .unwrap_or_else(fail);
    assert_eq!(table.len(), 640);
    let safe = |state: [i128; 2]| state.iter().all(|value| *value <= 3);
    let (mut safe_rows, mut accept, mut reject) = (0, 0, 0);
    for (state, target, action, joint) in &table {
        if !safe(*state) {
            continue;
        }
        safe_rows += 1;
        // On every jointly safe pre-state the composition is the reference
        // ring step, never refuses, and keeps the joint invariant.
        match (reference(*state, *target, *action), joint) {
            (Some(next), Joint::Accept(actual)) => {
                assert_eq!(next, *actual);
                assert!(safe(*actual));
                accept += 1;
            }
            (None, Joint::Reject) => reject += 1,
            other => panic!("joint row {state:?} {target} {action} differs: {other:?}"),
        }
    }
    // The aggregate reference labels count the same 160 safe rows.
    assert_eq!((safe_rows, accept, reject), (160, 120, 40));
    // Over the whole raw domain each leaf's own law sees only its own
    // register: 504 Accept, 80 Reject and 56 refusals. The single-descriptor
    // fixture of stage 38, whose law bounds both registers, predicts 280,
    // 80 and 280; the 224 rows that differ all start outside the joint
    // invariant, so they are unreachable from genesis.
    let count = |wanted: fn(&Joint) -> bool| table.iter().filter(|row| wanted(&row.3)).count();
    assert_eq!(
        (
            count(|joint| matches!(joint, Joint::Accept(_))),
            count(|joint| *joint == Joint::Reject),
            count(|joint| *joint == Joint::Refused),
        ),
        (504, 80, 56)
    );
    // Outside the joint invariant the leaf runtime law refuses the copy of
    // an unsafe peer: the assumption is operational, not decorative.
    assert!(table.contains(&([0, 4], 0, 4, Joint::Refused)));
    // Every state reachable from genesis is jointly safe, and all 16 are reachable.
    let mut reached = vec![[0i128, 0]];
    let mut frontier = vec![[0i128, 0]];
    while let Some(state) = frontier.pop() {
        for (from, _, _, joint) in &table {
            if *from == state
                && let Joint::Accept(next) = joint
                && !reached.contains(next)
            {
                reached.push(*next);
                frontier.push(*next);
            }
        }
    }
    assert_eq!(reached.len(), 16);
    assert!(reached.iter().all(|state| safe(*state)));
}

#[test]
fn a_missing_provider_is_refused() {
    let mut ring = Ring::new(2);
    ring.manifest["edges"] = json!([ring.manifest["edges"][1].clone()]);
    assert_refused(&ring.refusal(), "interface", "provider binding");
}

#[test]
fn a_swapped_claim_is_refused() {
    let mut ring = Ring::new(2);
    // The provider's post-transition relation cannot discharge a pre-state
    // assumption.
    ring.manifest["edges"][0]["facts"] = json!([2]);
    assert_refused(&ring.refusal(), "interface", "provider claim");
}

#[test]
fn swapped_sources_are_refused() {
    let mut ring = Ring::new(2);
    ring.leaves.swap(0, 1);
    assert_refused(&ring.refusal(), "binding", "source binding");
}

#[test]
fn a_swapped_program_is_refused() {
    let mut ring = Ring::new(2);
    let other = ring.manifest["leaves"][1]["program_sha256"].clone();
    ring.manifest["leaves"][0]["program_sha256"] = other;
    assert_refused(&ring.refusal(), "binding", "program binding");
}

#[test]
fn a_changed_view_is_refused() {
    // The consumer's interface domain no longer equals the provider view.
    let mut ring = Ring::new(2);
    ring.leaves[0].project = ring.leaves[0].project.replace(
        "field 202 102 peer 103;",
        "type 104 int PeerValue in 0..=6;\nfield 202 102 peer 104;",
    );
    ring.repin(0);
    assert_refused(&ring.refusal(), "interface", "typed view");
    // A view of a field the provider does not have.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][1]["views"][0]["field"] = json!(111);
    assert_refused(&ring.refusal(), "ownership", "complete footprint");
}

#[test]
fn changed_bounds_fail_their_obligations() {
    // A weaker assumption admits copying 4, which the leaf cannot keep.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][0]["assumption"] =
        json!("command.101.201 == 4 -> context.102.202 <= 4");
    let refusal = ring.refusal();
    assert_refused(&refusal, "local", "no technical refusal on admitted inputs");
    assert_eq!(
        refusal.json()["witness"],
        json!({"pre.100.110": 0, "command.101.201": 4, "context.102.202": 4})
    );
    // A weaker provider fact no longer implies the consumer's assumption.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][1]["guarantees"][0]["formula"] = json!("post.100.110 <= 4");
    let refusal = ring.refusal();
    assert_refused(
        &refusal,
        "interface",
        "provider facts => consumer assumption",
    );
    assert_eq!(
        refusal.json()["witness"],
        json!({"command.101.201": 4, "context.102.202": 4})
    );
    // A changed source bound without a new pin.
    let mut ring = Ring::new(2);
    ring.leaves[1].project = ring.leaves[1].project.replace("in 0..=7", "in 0..=8");
    assert_refused(&ring.refusal(), "binding", "source binding");
}

#[test]
fn an_invalid_genesis_fails_init_implies_invariant() {
    let mut ring = Ring::new(2);
    ring.leaves[1].rules = ring.leaves[1].rules.replace("\"110\": 0", "\"110\": 4");
    ring.repin(1);
    let refusal = ring.refusal();
    assert_refused(&refusal, "local", "Init => I");
    assert_eq!(refusal.json()["witness"]["pre.100.110"], 4);
}

#[test]
fn circular_self_support_is_refused() {
    // Each fact is supported only by the other leaf's assumption: with no
    // invariant of its own a leaf cannot derive its fact.
    let mut ring = Ring::new(2);
    for index in 0..2 {
        ring.manifest["leaves"][index]["invariant"] = json!("true");
    }
    let refusal = ring.refusal();
    assert_refused(&refusal, "local", "I => G");
    assert_eq!(refusal.json()["witness"], json!({"post.100.110": 4}));
    // A leaf cannot provide its own interface.
    let mut ring = Ring::new(2);
    ring.manifest["edges"][0]["provider"] = json!(1);
    assert_refused(&ring.refusal(), "interface", "provider binding");
}

#[test]
fn overlapping_owners_are_refused() {
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][1]["owns"][0]["slot"] = json!(0);
    assert_refused(&ring.refusal(), "ownership", "disjoint ownership");
}

#[test]
fn a_hidden_write_is_refused() {
    // Leaf 0 keeps a mirror of its peer in a second field that every copy
    // writes, while declaring only its register written.
    let mut ring = Ring::new(2);
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
    ring.manifest["leaves"][0]["owns"] =
        json!([{"field": 110, "slot": 0}, {"field": 111, "slot": 2}]);
    let refusal = ring.refusal();
    assert_refused(&refusal, "ownership", "complete footprint");
    assert!(
        refusal.json()["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("field 111"))
    );
}

#[test]
fn an_eager_unused_trap_is_refused() {
    // The trap sits in a case no input selects; the program evaluates every
    // case eagerly, so it overflows on every register above 0.
    let mut ring = Ring::new(2);
    ring.leaves[0].rules = ring.leaves[0].rules.replace(
        "\"cases\": [",
        "\"cases\": [\n    {\"rule\": \"never\", \"when\": \"pre.100.110 > 7\", \"class\": \"Accept\", \"reason\": null, \"post\": {\"110\": \"pre.100.110 + 170141183460469231731687303715884105727 - 170141183460469231731687303715884105727\"}, \"outbox\": []},",
    );
    ring.repin(0);
    let refusal = ring.refusal();
    assert_refused(&refusal, "local", "no technical refusal on admitted inputs");
    assert_eq!(refusal.json()["witness"]["pre.100.110"], 1);
    // The verdict names the Authority's refusal by its variants.
    assert_eq!(
        refusal.authority(),
        Some(AuthorityRefusal::Execution(ExecutionRefusal::Arithmetic))
    );
    assert_eq!(
        refusal.json()["authority"],
        json!({"kind": "execution", "refusal": "arithmetic"})
    );
    assert_no_rendering(&refusal);
}

/// A reason is the checker's own sentence, never a library value's debug
/// rendering.
fn assert_no_rendering(refusal: &Refusal) {
    for rendering in [
        "Core(",
        "Execution(",
        "Budget(",
        "MeterFailure",
        "Some(",
        "Refused",
    ] {
        assert!(
            !refusal.reason().contains(rendering),
            "{rendering} in {}",
            refusal.reason()
        );
    }
}

#[test]
fn a_wrong_refusal_precedence_fails_the_transition_relation() {
    // A case that accepts every Set comes before the unchanged Reject, so
    // Set of the current value accepts without a change.
    let mut ring = Ring::new(2);
    let mut rules: Value = serde_json::from_str(&ring.leaves[1].rules).unwrap_or_else(fail);
    let first = json!({"rule": "set", "when": "command.101.201 < 4", "class": "Accept",
                       "reason": null, "post": {"110": "new_value"}, "outbox": []});
    rules["cases"] = json!([first, rules["cases"][0].clone(), rules["cases"][1].clone()]);
    ring.leaves[1].rules = pretty(&rules);
    ring.repin(1);
    let refusal = ring.refusal();
    assert_refused(&refusal, "local", "I and A => G");
    assert_eq!(
        refusal.json()["witness"],
        json!({"pre.100.110": 0, "command.101.201": 0, "context.102.202": 0})
    );
}

#[test]
fn an_exhausted_meter_is_refused() {
    // The rules-compiled contract always sizes its meter to fit, so this
    // planted defect lowers the Step limit of the compiled leaf and pins
    // the program that results.
    let ring = Ring::new(2);
    let manifest = Manifest::read(&pretty(&ring.manifest)).unwrap_or_else(fail);
    let mut entry = manifest.leaves[0].clone();
    let compiled =
        leaf::compile(&entry, &ring.leaves[0].project, &ring.leaves[0].rules).unwrap_or_else(fail);
    let mut contract =
        Contract::build(&compiled.declarations, &compiled.rules, compiled.commitment)
            .unwrap_or_else(fail);
    let genesis_steps = policy::with_authority(&contract, &compiled.schema, |authority| {
        let framer = Framer::new(&compiled.positions, &contract);
        let frames = framer.frame(&[0, 0, 0]);
        authority.genesis(&frames[0]).usage().used(V2Resource::Step)
    })
    .unwrap_or_else(fail);
    // Enough for genesis, too little for any transition.
    contract.budgets.step = genesis_steps;
    entry.program_sha256 = policy::with_authority(&contract, &compiled.schema, |authority| {
        sha256(authority.identity())
    })
    .unwrap_or_else(fail);
    let refusal = match leaf::certify_contract(&entry, &compiled, &contract) {
        Ok(certificate) => panic!("expected a refusal, certified {}", certificate.json()),
        Err(refusal) => refusal,
    };
    assert_refused(&refusal, "local", "no technical refusal on admitted inputs");
    // The contract's reject-structure law, 909, is the first to run out of
    // Steps on the shared meter.
    assert_eq!(
        refusal.authority(),
        Some(AuthorityRefusal::Law {
            law: Some(909),
            refusal: LawRefusal::Meter(MeterRefusal {
                resource: V2Resource::Step
            })
        })
    );
    assert_eq!(
        refusal.json()["authority"],
        json!({"kind": "law", "law": 909, "refusal": {"meter": "step"}})
    );
    assert_no_rendering(&refusal);
}

#[test]
fn a_compile_that_binds_another_program_is_refused_through_its_pin() {
    // The same files compiled differently, as a changed front end would
    // compile them: here the leaf's Step budget differs. The checker binds
    // what it compiled, and that program's identity is not the pinned one.
    let ring = Ring::new(2);
    let manifest = Manifest::read(&pretty(&ring.manifest)).unwrap_or_else(fail);
    let entry = &manifest.leaves[0];
    let compiled =
        leaf::compile(entry, &ring.leaves[0].project, &ring.leaves[0].rules).unwrap_or_else(fail);
    let mut contract =
        Contract::build(&compiled.declarations, &compiled.rules, compiled.commitment)
            .unwrap_or_else(fail);
    leaf::certify_contract(entry, &compiled, &contract).unwrap_or_else(fail);
    contract.budgets.step += 1;
    let refusal = match leaf::certify_contract(entry, &compiled, &contract) {
        Ok(certificate) => panic!("expected a refusal, certified {}", certificate.json()),
        Err(refusal) => refusal,
    };
    assert_refused(&refusal, "binding", "program binding");
    assert_eq!(refusal.authority(), None);
}

#[test]
fn undefined_predicates_and_incomplete_domains_are_refused() {
    // An assumption may read only the command and context.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][0]["assumption"] = json!("pre.100.110 <= 3");
    assert_refused(&ring.refusal(), "binding", "defined predicate");
    // A formula over a field the leaf does not declare.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][0]["invariant"] = json!("post.100.119 <= 3");
    assert_refused(&ring.refusal(), "binding", "defined predicate");
    // A local domain above the row limit is refused, not sampled.
    let mut ring = Ring::new(2);
    for index in 0..2 {
        ring.leaves[index].project = ring.leaves[index]
            .project
            .replace("in 0..=7", "in 0..=4095");
        ring.repin(index);
    }
    assert_refused(&ring.refusal(), "local", "complete enumeration");
}

#[test]
fn a_missing_required_witness_is_refused() {
    // The unchanged case can no longer hold, so the leaf never rejects; its
    // relations are dropped so that only the witness is missing.
    let mut ring = Ring::new(2);
    ring.leaves[0].rules = ring.leaves[0].rules.replace(
        "\"new_value == pre.100.110\"",
        "\"new_value == pre.100.110 && new_value > 7\"",
    );
    ring.repin(0);
    ring.manifest["leaves"][0]["guarantees"] = json!([
        {"id": 1, "kind": "StateFact", "scope": null, "formula": "post.100.110 <= 3"}
    ]);
    assert_refused(&ring.refusal(), "local", "required witness");
}

/// Writes the ring fixtures the measurements run: set
/// `CHECKED_COMPOSITION_RING_OUT` to an empty directory.
#[test]
#[ignore = "writes measurement fixtures on request"]
fn write_ring_fixtures() {
    let Some(out) = std::env::var_os("CHECKED_COMPOSITION_RING_OUT") else {
        return;
    };
    for n in [2, 4, 8, 16, 32] {
        Ring::new(n).write(&PathBuf::from(&out).join(format!("ring-n{n}")));
    }
}
