//! The generated aggregate of the register ring is a machine map onto the
//! composition, and each planted defect in an aggregate fails its named
//! assembly obligation.

use serde_json::{Value, json};

use super::aggregate::AggregateSources;
use super::assembly::{self, CompiledAggregate};
use super::leaf;
use super::manifest::Manifest;
use super::structure;
use super::tests::{Ring, assert_refused, fail, pretty};
use super::{CompositionSources, Refusal, aggregate, check};

fn sources<'a>(
    manifest: &'a str,
    ring: &'a Ring,
    aggregate: Option<&'a AggregateSources>,
) -> CompositionSources<'a> {
    CompositionSources {
        manifest,
        leaves: &ring.leaves,
        aggregate,
    }
}

fn generated(ring: &Ring) -> AggregateSources {
    let manifest = ring.manifest_text();
    aggregate(sources(&manifest, ring, None)).unwrap_or_else(fail)
}

fn assembled(
    ring: &Ring,
    aggregate: &AggregateSources,
) -> Result<super::CheckedComposition, Refusal> {
    let manifest = ring.manifest_text();
    check(sources(&manifest, ring, Some(aggregate)))
}

fn refusal(ring: &Ring, aggregate: &AggregateSources) -> Refusal {
    match assembled(ring, aggregate) {
        Ok(checked) => panic!(
            "expected a refusal, checked: {}",
            checked.json()["assembly"]
        ),
        Err(refusal) => refusal,
    }
}

/// The aggregate's rules as JSON, changed by `edit`, written again.
fn with_rules(aggregate: &AggregateSources, edit: impl FnOnce(&mut Value)) -> AggregateSources {
    let mut rules: Value = serde_json::from_str(&aggregate.rules).unwrap_or_else(fail);
    edit(&mut rules);
    AggregateSources {
        project: aggregate.project.clone(),
        rules: pretty(&rules),
    }
}

fn with_project(aggregate: &AggregateSources, from: &str, to: &str) -> AggregateSources {
    assert!(
        aggregate.project.contains(from),
        "{from} in {}",
        aggregate.project
    );
    AggregateSources {
        project: aggregate.project.replacen(from, to, 1),
        rules: aggregate.rules.clone(),
    }
}

#[test]
fn the_ring_aggregate_is_generated_and_validated() {
    for n in [2, 4] {
        let ring = Ring::new(n);
        let aggregate = generated(&ring);
        let checked = assembled(&ring, &aggregate).unwrap_or_else(fail);
        let assembly = &checked.json()["assembly"];
        assert_eq!(assembly["routes"], n);
        // Each leaf's unchanged Reject and changed Accept.
        assert_eq!(assembly["cases"], 2 * n);
        assert_eq!(assembly["mapped_cases"], 2 * n);
        // One conjunct per leaf law, each over the leaf's 8 states.
        assert_eq!(assembly["laws"], json!({"conjuncts": n, "rows": 8 * n}));
        // The generator is deterministic.
        assert_eq!(generated(&ring), aggregate);
    }
}

#[test]
fn the_aggregate_follows_the_assembly_profile() {
    let aggregate = generated(&Ring::new(2));
    for line in [
        "type 100 state ComposedState;",
        "field 1000 100 slot_0 103;",
        "field 1001 100 slot_1 103;",
        "field 1002 101 target 106;",
        "type 106 int Target in 0..=1;",
        "field 201 101 action 105;",
        "field 1003 102 unit 107;",
        "type 107 int Unit in 0..=0;",
        "reason 300 unchanged precedence 0;",
    ] {
        assert!(
            aggregate.project.contains(line),
            "{line} in\n{}",
            aggregate.project
        );
    }
    let rules: Value = serde_json::from_str(&aggregate.rules).unwrap_or_else(fail);
    let cases = rules["cases"].as_array().unwrap_or_else(|| fail("cases"));
    assert_eq!(cases.len(), 4);
    // Leaf 0 copies the peer view from slot 1 of the same pre-state.
    assert_eq!(
        cases[0]["when"],
        "command.101.1002 == 0 && choose(command.101.201 == 4, pre.100.1001, command.101.201) == pre.100.1000"
    );
    assert_eq!(cases[1]["when"], "command.101.1002 == 0");
    assert_eq!(cases[3]["when"], true);
    assert_eq!(rules["genesis"], json!({"1000": 0, "1001": 0}));
}

#[test]
fn an_aggregate_that_writes_an_unselected_slot_is_refused() {
    let ring = Ring::new(2);
    // Leaf 0's Accept also clears register 1.
    let aggregate = with_rules(&generated(&ring), |rules| {
        rules["cases"][1]["post"]["1001"] = json!("0");
    });
    let refusal = refusal(&ring, &aggregate);
    assert_refused(&refusal, "assembly", "unselected slots unchanged");
    assert!(refusal.reason().contains("slot 1"), "{}", refusal.reason());
}

#[test]
fn an_aggregate_that_reads_a_post_state_view_is_refused() {
    let ring = Ring::new(2);
    // Leaf 0's copy takes the value register 1 would get from the same
    // command, not register 1 in the pre-state.
    let aggregate = with_rules(&generated(&ring), |rules| {
        rules["cases"][1]["post"]["1000"] = json!(
            "choose(command.101.201 == 4, choose(command.101.201 == 4, pre.100.1000, command.101.201), command.101.201)"
        );
    });
    assert_refused(
        &refusal(&ring, &aggregate),
        "assembly",
        "views from the same pre-state",
    );
}

#[test]
fn an_unused_eager_trap_in_the_aggregate_is_refused() {
    let ring = Ring::new(2);
    // A case no admitted input selects still evaluates eagerly, and it
    // overflows on every register above 0.
    let aggregate = with_rules(&generated(&ring), |rules| {
        let trap = json!({"rule": "never", "when": "command.101.1002 == 0 && pre.100.1000 > 7",
            "class": "Accept", "reason": null,
            "post": {"1000": "pre.100.1000 + 170141183460469231731687303715884105727 - 170141183460469231731687303715884105727"},
            "outbox": []});
        let cases = rules["cases"]
            .as_array_mut()
            .unwrap_or_else(|| fail("cases"));
        cases.insert(0, trap);
    });
    assert_refused(&refusal(&ring, &aggregate), "assembly", "eager nodes total");
}

#[test]
fn a_different_refusal_precedence_is_refused() {
    let ring = Ring::new(2);
    // Leaf 0's Accept is tried before its unchanged Reject.
    let aggregate = with_rules(&generated(&ring), |rules| {
        let cases = rules["cases"]
            .as_array_mut()
            .unwrap_or_else(|| fail("cases"));
        cases.swap(0, 1);
    });
    assert_refused(
        &refusal(&ring, &aggregate),
        "assembly",
        "decision correspondence",
    );
    // Another business reason for leaf 0's unchanged Reject.
    let aggregate = with_project(
        &generated(&ring),
        "reason 300 unchanged precedence 0;",
        "reason 300 unchanged precedence 0;\nreason 301 other precedence 1;",
    );
    let aggregate = with_rules(&aggregate, |rules| rules["cases"][0]["reason"] = json!(301));
    let refusal = refusal(&ring, &aggregate);
    assert_refused(&refusal, "assembly", "decision correspondence");
    assert!(
        refusal.reason().contains("reason 301"),
        "{}",
        refusal.reason()
    );
}

#[test]
fn an_exhausted_aggregate_meter_is_refused() {
    // The compiler always sizes the meter to fit, so the control lowers the
    // Step limit of the compiled aggregate.
    let ring = Ring::new(2);
    let aggregate = generated(&ring);
    let manifest = Manifest::read(&ring.manifest_text()).unwrap_or_else(fail);
    let compiled: Vec<leaf::Compiled> = manifest
        .leaves
        .iter()
        .zip(&ring.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<_, _>>()
        .unwrap_or_else(fail);
    let judged = structure::judge(&manifest, &compiled).unwrap_or_else(fail);
    let built = CompiledAggregate::read(&aggregate).unwrap_or_else(fail);
    let mut contract = built.contract().unwrap_or_else(fail);
    assembly::validate_contract(
        &manifest,
        &compiled,
        &judged.edges,
        &aggregate,
        &built,
        &contract,
    )
    .unwrap_or_else(fail);
    contract.budgets.step = 10;
    match assembly::validate_contract(
        &manifest,
        &compiled,
        &judged.edges,
        &aggregate,
        &built,
        &contract,
    ) {
        Ok(_) => panic!("expected a refusal"),
        Err(refusal) => assert_refused(&refusal, "assembly", "meter covers eager work"),
    }
}

#[test]
fn a_changed_genesis_target_context_or_domain_is_refused() {
    let ring = Ring::new(2);
    let aggregate = generated(&ring);
    let changed = with_rules(&aggregate, |rules| rules["genesis"]["1001"] = json!(1));
    assert_refused(&refusal(&ring, &changed), "assembly", "aggregate genesis");
    let changed = with_project(&aggregate, "Target in 0..=1", "Target in 0..=2");
    assert_refused(&refusal(&ring, &changed), "assembly", "command routing");
    let changed = with_project(&aggregate, "Unit in 0..=0", "Unit in 0..=1");
    assert_refused(&refusal(&ring, &changed), "assembly", "environment inputs");
    let changed = with_project(
        &aggregate,
        "RegisterValue in 0..=7",
        "RegisterValue in 0..=8",
    );
    assert_refused(&refusal(&ring, &changed), "assembly", "state projection");
}

#[test]
fn an_aggregate_law_outside_the_invariants_is_refused() {
    let ring = Ring::new(2);
    let aggregate = generated(&ring);
    // A law relating both registers is not a per-leaf conjunct.
    let changed = with_project(
        &aggregate,
        "= post.100.1000 <= 3;",
        "= post.100.1000 + post.100.1001 <= 6;",
    );
    assert_refused(
        &refusal(&ring, &changed),
        "assembly",
        "aggregate laws implied by invariants",
    );
    // A conjunct that reads no slot.
    let changed = with_project(
        &aggregate,
        "= post.100.1000 <= 3;",
        "= post.100.1000 <= 3 && true;",
    );
    let slot_free = refusal(&ring, &changed);
    assert_refused(
        &slot_free,
        "assembly",
        "aggregate laws implied by invariants",
    );
    assert!(
        slot_free.reason().contains("reads no slot"),
        "{}",
        slot_free.reason()
    );
    // A law the leaf's invariant does not imply.
    let changed = with_project(&aggregate, "= post.100.1000 <= 3;", "= post.100.1000 <= 2;");
    let refusal = refusal(&ring, &changed);
    assert_refused(&refusal, "assembly", "aggregate laws implied by invariants");
    assert_eq!(refusal.json()["witness"], json!({"post.100.110": 3}));
}
