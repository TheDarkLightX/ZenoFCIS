//! The planner and the obligations: exact sizes on hand-computed cases,
//! refusal before any evaluation, the ten-core ladder, every planted fault,
//! the behaviours the profile's specification documents, and the receipt.

use std::fs;

use serde_json::{Value, json};

use super::domain_tests::{Built, budget, counter, fail, files, fixtures, part};
use super::obligations::check_domain_building_tables_first;
use super::{
    DOMAIN_CLAIM, DomainResult, DomainSources, LeafSources, ReplayRefusal, Verdict, check_domain,
    domain_paths, replay_domain,
};

/// The check of a committed manifest, with its component files.
pub(super) fn committed(manifest: &str) -> (String, Vec<LeafSources>) {
    let path = fixtures().join(manifest);
    let text = fs::read_to_string(&path).unwrap_or_else(fail);
    let directory = path
        .parent()
        .unwrap_or_else(|| fail("a manifest directory"));
    let base = directory
        .strip_prefix(fixtures())
        .unwrap_or_else(fail)
        .to_str()
        .unwrap_or_else(|| fail("a UTF-8 path"))
        .to_owned();
    let sources = domain_paths(&text)
        .unwrap_or_else(fail)
        .iter()
        .map(|component| files(&format!("{base}/{component}")))
        .collect();
    (text, sources)
}

fn check_committed(manifest: &str) -> DomainResult {
    let (text, sources) = committed(manifest);
    check_domain(&DomainSources {
        manifest: &text,
        components: &sources,
    })
    .unwrap_or_else(fail)
}

fn sizes(result: &DomainResult) -> Vec<(String, String, String)> {
    result.json()["obligations"]
        .as_array()
        .unwrap_or_else(|| fail("obligations"))
        .iter()
        .map(|obligation| {
            (
                obligation["kind"].as_str().unwrap_or_default().to_owned(),
                obligation["subject"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                obligation["size"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn owned(kind: &str, subject: &str, size: u64) -> (String, String, String) {
    (kind.to_owned(), subject.to_owned(), size.to_string())
}

/// local: the local domain; assume: the request fields read times the
/// states of the components read; couple: the members' and their
/// providers' states times their request fields.
#[test]
fn obligation_sizes_are_exact_on_hand_computed_cases() {
    let result = Built::new(
        vec![counter(), budget()],
        json!([{"name": "k", "members": ["counter", "budget"], "invariant": "counter.value <= budget.remaining + 2"}]),
    )
    .check()
    .unwrap_or_else(fail);
    assert_eq!(
        sizes(&result),
        [
            // value 3 x action 2 x authorized 2.
            owned("local", "counter", 12),
            // op 3 x authorized 2; it reads no state.
            owned("assume", "counter", 6),
            // remaining 3 x amount 3 x authorized 2.
            owned("local", "budget", 18),
            // op 3 x qty 2 x authorized 2.
            owned("assume", "budget", 12),
            // value 3 x remaining 3 x op 3 x qty 2 x authorized 2.
            owned("couple", "k", 108),
        ]
    );
    let json = result.json();
    assert_eq!(json["planned_total"], "156");
    assert_eq!(json["monolithic"]["states"], "9");
    assert_eq!(json["monolithic"]["requests"], "12");
    assert_eq!(json["monolithic"]["pairs"], "108");
    // A binding that reads the counter instead of `qty` adds the counter's
    // states to the budget's assumption: op 3 x authorized 2 x value 3.
    let mut reader = budget();
    reader.entry["bind"]["amount"] = json!("choose(counter.value == 2, 2, 1)");
    let result = Built::new(vec![counter(), reader], json!([]))
        .check()
        .unwrap_or_else(fail);
    assert_eq!(sizes(&result)[3], owned("assume", "budget", 18));
}

#[test]
fn every_obligation_of_a_proved_check_evaluates_its_planned_size() {
    let result = Built::new(vec![counter(), budget()], json!([]))
        .check()
        .unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Proved);
    for obligation in result.json()["obligations"]
        .as_array()
        .unwrap_or_else(|| fail("obligations"))
    {
        assert_eq!(
            obligation["evaluated"].to_string(),
            obligation["size"].as_str().unwrap_or_default(),
            "{obligation}"
        );
    }
    assert_eq!(result.json()["claim"], DOMAIN_CLAIM);
}

/// The ten-core ladder: proved without enumerating its product, every
/// obligation evaluated exactly at its planned size.
#[test]
fn the_ten_core_ladder_is_proved_without_its_product() {
    let result = check_committed("ladder-10/composition.json");
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Proved, "{json}");
    assert_eq!(json["monolithic"]["pairs"], "172179890733018470400");
    assert_eq!(json["monolithic"]["enumerated"], false);
    let pairs: u128 = json["monolithic"]["pairs"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(fail);
    assert!(pairs > 1_000_000_000_000);
    let mut planned = 0u64;
    for obligation in json["obligations"]
        .as_array()
        .unwrap_or_else(|| fail("obligations"))
    {
        let size: u64 = obligation["size"]
            .as_str()
            .unwrap_or_default()
            .parse()
            .unwrap_or_else(fail);
        assert_eq!(obligation["evaluated"], json!(size), "{obligation}");
        assert_eq!(obligation["status"], "proved");
        planned += size;
    }
    assert_eq!(result.evaluated(), planned);
    assert_eq!(json["components"].as_array().map(Vec::len), Some(10));
}

/// The wide coupling over three components is refused before anything is
/// evaluated: no row, no decision, no formula value.
#[test]
fn the_wide_coupling_is_refused_before_evaluation() {
    let result = check_committed("ladder-10/wide.json");
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Refused);
    assert_eq!(result.evaluated(), 0);
    assert_eq!(result.work(), (0, 0));
    assert_eq!(json["evaluated"], 0);
    assert_eq!(json["refusal"]["reason"], "obligation-over-budget");
    assert_eq!(json["refusal"]["obligation"]["kind"], "couple");
    assert_eq!(json["refusal"]["obligation"]["subject"], "wide");
    assert_eq!(json["refusal"]["obligation"]["size"], "86782189536");
    assert_eq!(json["refusal"]["factor"], "20691");
    for obligation in json["obligations"]
        .as_array()
        .unwrap_or_else(|| fail("obligations"))
    {
        assert_eq!(obligation["evaluated"], 0, "{obligation}");
        assert!(obligation.get("witness").is_none());
    }
    assert!(
        json["components"]
            .as_array()
            .unwrap_or_else(|| fail("components"))
            .iter()
            .all(|component| component["decisions_sha256"].is_null())
    );
}

/// A manifest may lower the total; the ladder's 70,508 rows exceed 50,000.
#[test]
fn a_lowered_total_is_refused_before_evaluation() {
    let result = check_committed("ladder-10/lowered-total.json");
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Refused);
    assert_eq!(result.work(), (0, 0));
    assert_eq!(result.evaluated(), 0);
    assert_eq!(json["refusal"]["reason"], "total-over-budget");
    assert_eq!(json["refusal"]["total"], "70508");
    assert_eq!(json["refusal"]["limit"], "50000");
    assert_eq!(json["refusal"]["factor"], "2");
    assert_eq!(json["limits"]["lowered"], true);
}

fn witness(result: &DomainResult) -> Value {
    result.json()["witness"].clone()
}

#[test]
fn a_coupling_false_at_genesis_is_violated() {
    let result = check_committed("faults/coupling-false-at-genesis.json");
    let witness = witness(&result);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "couple", "subject": "k"})
    );
    assert_eq!(
        witness["witness"]["genesis"],
        json!({"budget.remaining": 2, "counter.value": 0})
    );
}

#[test]
fn a_coupling_broken_by_one_step_is_violated_with_the_step() {
    let result = check_committed("faults/coupling-broken-by-step.json");
    let witness = witness(&result);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "couple", "subject": "k"})
    );
    assert_eq!(
        witness["witness"]["states"],
        json!({"budget.remaining": 2, "counter.value": 0})
    );
    assert_eq!(witness["witness"]["request"]["op"], 0);
    assert_eq!(
        witness["witness"]["after"],
        json!({"budget.remaining": 2, "counter.value": 1})
    );
}

#[test]
fn a_binding_that_reads_a_later_component_is_refused_at_validation() {
    let (text, sources) = committed("faults/binding-reads-later.json");
    let refusal = check_domain(&DomainSources {
        manifest: &text,
        components: &sources,
    })
    .err()
    .unwrap_or_else(|| fail("refused"));
    assert!(
        refusal.to_string().contains("reads `budget.remaining`"),
        "{refusal}"
    );
}

#[test]
fn a_binding_out_of_its_domain_violates_the_assumption_obligation() {
    let result = check_committed("faults/binding-out-of-domain.json");
    let witness = witness(&result);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "assume", "subject": "budget"})
    );
    assert_eq!(witness["witness"]["request"]["qty"], 2);
    assert_eq!(
        witness["witness"]["failed"],
        "the binding of `amount` lies above its field's domain"
    );
}

#[test]
fn an_invariant_not_preserved_is_violated_with_a_counterexample_to_induction() {
    let result = check_committed("faults/invariant-not-preserved.json");
    let witness = witness(&result);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "local", "subject": "counter"})
    );
    assert_eq!(witness["witness"]["state"], json!({"value": 1}));
    assert_eq!(witness["witness"]["post"], json!({"value": 2}));
    assert_eq!(witness["witness"]["class"], "Accept");
}

#[test]
fn two_owners_of_one_field_are_refused() {
    let (text, sources) = committed("faults/two-owners.json");
    let refusal = check_domain(&DomainSources {
        manifest: &text,
        components: &sources,
    })
    .err()
    .unwrap_or_else(|| fail("refused"));
    assert_eq!(refusal.json()["obligation"], "disjoint ownership");
}

#[test]
fn a_read_only_component_that_changes_its_state_is_violated() {
    let result = check_committed("faults/read-only-changes.json");
    let witness = witness(&result);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "local", "subject": "counter"})
    );
    assert_eq!(
        witness["witness"]["failed"],
        "a committing decision of a read-only component changes its state"
    );
}

#[test]
fn the_guarded_budget_is_proved() {
    let result = check_committed("guarded/composition.json");
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
    assert_eq!(result.json()["components"][0]["state"], "read-only");
}

/// Documented behaviour: decisions the Authority refuses commit nothing;
/// the local obligation counts them and is not violated by them.
#[test]
fn refused_local_decisions_are_counted_not_failures() {
    let result = check_committed("faults/ticker-tripwire.json");
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Proved, "{json}");
    let ticker = json["obligations"]
        .as_array()
        .unwrap_or_else(|| fail("obligations"))
        .iter()
        .find(|obligation| obligation["kind"] == "local" && obligation["subject"] == "ticker")
        .cloned()
        .unwrap_or_else(|| fail("the ticker's local obligation"));
    assert!(
        ticker["refused_decisions"].as_u64().unwrap_or_default() > 0,
        "{ticker}"
    );
    assert_eq!(ticker["first_refusal"]["refusal"], "law");
}

/// Documented behaviour: a component outside a coupling with a
/// CommittedFailure case may end the step between two members; the
/// coupling must hold there too. Between the counter and the budget, the
/// wire can stop the step after the counter's increment, before the
/// budget's matching spend; after the budget it cannot.
#[test]
fn a_committed_failure_between_members_may_end_the_step_there() {
    let wire = || {
        part(
            "wire",
            "faults/tripwire",
            &["tripped", "touches"],
            "true",
            json!({"fire": "qty == 2", "authorized": "authorized"}),
            "true",
            "true",
        )
    };
    let mut spender = budget();
    spender.entry["participates"] = json!("op == 0");
    spender.entry["bind"]["amount"] = json!("1");
    let coupling = json!([{"name": "k", "members": ["counter", "budget"], "invariant": "counter.value + budget.remaining <= 2"}]);
    let between = Built::new(vec![counter(), wire(), spender], coupling.clone())
        .check()
        .unwrap_or_else(fail);
    let witness = witness(&between);
    assert_eq!(between.verdict(), Verdict::Violated, "{}", between.json());
    assert_eq!(
        witness["obligation"],
        json!({"kind": "couple", "subject": "k"})
    );
    assert_eq!(
        witness["witness"]["ended_by_possible_committed_failure_of"],
        json!(["wire"])
    );
    let mut spender = budget();
    spender.entry["participates"] = json!("op == 0");
    spender.entry["bind"]["amount"] = json!("1");
    let after = Built::new(vec![counter(), spender, wire()], coupling)
        .check()
        .unwrap_or_else(fail);
    assert_eq!(after.verdict(), Verdict::Proved, "{}", after.json());
}

/// Documented behaviour: a Reject ends the step with no change, so a
/// coupling that one member alone would break holds when the other member
/// rejects in the same step.
#[test]
fn a_reject_of_one_member_keeps_the_coupling() {
    let mut spender = budget();
    spender.entry["participates"] = json!("op == 0");
    spender.entry["bind"]["amount"] = json!("1");
    let result = Built::new(
        vec![counter(), spender],
        json!([{"name": "k", "members": ["counter", "budget"], "invariant": "counter.value + budget.remaining <= 2"}]),
    )
    .check()
    .unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
}

#[test]
fn a_receipt_replays_byte_for_byte_and_a_changed_one_or_source_does_not() {
    let (text, sources) = committed("guarded/composition.json");
    let domain = DomainSources {
        manifest: &text,
        components: &sources,
    };
    let receipt = check_domain(&domain).unwrap_or_else(fail).receipt();
    assert!(receipt.ends_with('\n'));
    let replayed = replay_domain(&receipt, &domain).unwrap_or_else(fail);
    assert_eq!(replayed.receipt(), receipt);
    let tampered = receipt.replacen("\"evaluated\":40", "\"evaluated\":41", 1);
    assert_ne!(tampered, receipt);
    assert!(matches!(
        replay_domain(&tampered, &domain),
        Err(ReplayRefusal::Mismatch { .. })
    ));
    let mut changed = sources.clone();
    changed[1].rules.push('\n');
    let changed_domain = DomainSources {
        manifest: &text,
        components: &changed,
    };
    assert!(matches!(
        replay_domain(&receipt, &changed_domain),
        Err(ReplayRefusal::Check(_))
    ));
    // A violated or refused result has a receipt too, and it replays.
    let (text, sources) = committed("ladder-10/wide.json");
    let wide = DomainSources {
        manifest: &text,
        components: &sources,
    };
    let receipt = check_domain(&wide).unwrap_or_else(fail).receipt();
    assert!(replay_domain(&receipt, &wide).is_ok());
}

// The cases the profile's specification documents, each restated over
// ZenoFCIS contracts: a procurement of a budget, a pool and a counter with
// two couplings, and the refusal and CommittedFailure cases.

fn documented(parts: Vec<super::domain_tests::Part>, couplings: Value) -> Built {
    let mut built = Built::new(parts, couplings);
    built.manifest["request"] = json!([
        {"name": "op", "domain": {"kind": "int", "min": 0, "max": 1}},
        {"name": "authorized", "domain": {"kind": "bool"}}
    ]);
    built
}

fn procurement(couplings: Value) -> Built {
    documented(
        vec![
            part(
                "budget",
                "documented/budget",
                &["remaining"],
                "op == 0",
                json!({"amount": "2", "authorized": "authorized"}),
                "true",
                "remaining <= 4",
            ),
            part(
                "pool",
                "documented/pool",
                &["available", "reserved"],
                "true",
                json!({"action": "choose(op == 0, 150, 152)", "quantity": "1", "authorized": "authorized"}),
                "action == 150 || action == 152",
                "available + reserved <= 2",
            ),
            part(
                "counter",
                "documented/counter",
                &["value"],
                "op == 0",
                json!({"action": "150", "authorized": "authorized"}),
                "true",
                "true",
            ),
        ],
        couplings,
    )
}

fn procurement_couplings() -> Value {
    json!([
        {"name": "count", "members": ["pool", "counter"], "invariant": "counter.value + pool.available <= 2"},
        {"name": "spend", "members": ["budget", "pool"], "invariant": "budget.remaining + pool.reserved + pool.reserved <= 4"}
    ])
}

#[test]
fn documented_procurement_is_proved_below_its_product() {
    let result = procurement(procurement_couplings())
        .check()
        .unwrap_or_else(fail);
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Proved, "{json}");
    let pairs: u64 = json["monolithic"]["pairs"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(fail);
    assert!(pairs > result.evaluated(), "{json}");
}

#[test]
fn documented_false_coupling_is_violated_with_the_state_after_the_step() {
    let result = procurement(json!([
        {"name": "wrong", "members": ["pool", "counter"], "invariant": "counter.value <= pool.available"}
    ]))
    .check()
    .unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Violated);
    let witness = witness(&result);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "couple", "subject": "wrong"})
    );
    assert!(witness["witness"].get("after").is_some(), "{witness}");
}

#[test]
fn documented_narrowed_assumption_is_violated() {
    let mut built = procurement(procurement_couplings());
    built.manifest["components"][1]["assume"] = json!("action == 150");
    let result = built.check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Violated);
    assert_eq!(
        witness(&result)["obligation"],
        json!({"kind": "assume", "subject": "pool"})
    );
}

#[test]
fn documented_lowered_obligation_limit_refuses_before_evaluating() {
    let mut built = procurement(procurement_couplings());
    built.manifest["limits"] = json!({"max_obligation": 8, "max_total": 1u64 << 25});
    let result = built.check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Refused);
    assert_eq!(result.evaluated(), 0);
    assert_eq!(result.work(), (0, 0));
}

/// A refused successor commits nothing, so a later binding that reads the
/// refusing component stays in its domain.
#[test]
fn documented_refused_successor_commits_nothing() {
    let result = documented(
        vec![
            part(
                "ticker",
                "documented/ticker",
                &["count"],
                "true",
                json!({"step": "1", "authorized": "authorized"}),
                "true",
                "count <= 2",
            ),
            part(
                "budget",
                "documented/small-budget",
                &["remaining"],
                "true",
                json!({"amount": "choose(ticker.count <= 2, 0, 5)", "authorized": "authorized"}),
                "true",
                "true",
            ),
        ],
        json!([]),
    )
    .check()
    .unwrap_or_else(fail);
    let json = result.json();
    assert_eq!(result.verdict(), Verdict::Proved, "{json}");
    assert!(
        json["obligations"][0]["refused_decisions"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
}

/// A CommittedFailure commits its successor, which can break an invariant.
#[test]
fn documented_committed_failure_can_break_an_invariant() {
    let result = documented(
        vec![part(
            "wire",
            "documented/tripwire",
            &["tripped", "touches"],
            "true",
            json!({"fire": "true", "authorized": "authorized"}),
            "true",
            "tripped == 0",
        )],
        json!([]),
    )
    .check()
    .unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Violated);
    let witness = witness(&result);
    assert_eq!(
        witness["obligation"],
        json!({"kind": "local", "subject": "wire"})
    );
    assert_eq!(witness["witness"]["class"], "CommittedFailure");
}

/// An outside component's CommittedFailure between two lockstep counters can
/// end the step after the first: violated. After both, it cannot: proved.
#[test]
fn documented_outside_failure_between_lockstep_members() {
    let counter = |name: &str| {
        part(
            name,
            "documented/counter-c2",
            &["value"],
            "true",
            json!({"action": "choose(op == 0, 150, 151)", "authorized": "authorized"}),
            "true",
            "true",
        )
    };
    let wire = || {
        part(
            "wire",
            "documented/tripwire",
            &["tripped", "touches"],
            "true",
            json!({"fire": "true", "authorized": "authorized"}),
            "true",
            "true",
        )
    };
    let lockstep =
        json!([{"name": "lock", "members": ["a", "b"], "invariant": "a.value == b.value"}]);
    let between = documented(vec![counter("a"), wire(), counter("b")], lockstep.clone())
        .check()
        .unwrap_or_else(fail);
    assert_eq!(between.verdict(), Verdict::Violated);
    assert_eq!(
        witness(&between)["witness"]["ended_by_possible_committed_failure_of"],
        json!(["wire"])
    );
    let after = documented(vec![counter("a"), counter("b"), wire()], lockstep)
        .check()
        .unwrap_or_else(fail);
    assert_eq!(after.verdict(), Verdict::Proved, "{}", after.json());
}

// The cost of a check: the tables it builds are planned, and the counters
// count the evaluations actually made.

fn planned_tables(result: &DomainResult) -> u64 {
    result.json()["planned_table_values"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .unwrap_or_else(fail)
}

/// Every evaluation the check makes is one cell of a planned table: the
/// planned table values equal the decisions plus the formula values.
#[test]
fn the_planned_tables_are_the_evaluations_made() {
    for manifest in [
        "ladder-10/composition.json",
        "guarded/composition.json",
        "documented/procurement.json",
    ] {
        let result = check_committed(manifest);
        let (decisions, formula_values) = result.work();
        assert!(decisions > 0 && formula_values > 0, "{manifest}");
        assert_eq!(
            planned_tables(&result),
            decisions + formula_values,
            "{manifest}"
        );
    }
}

/// A binding to a field of 4,096 values is computed once per cell, within
/// the default budget.
#[test]
fn a_binding_over_four_thousand_values_is_checked_within_budget() {
    let result = check_committed("limits/wide4095.json");
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
    // Decisions: mark 1 x amount 4,096 x authorized 2. Formula values: the
    // binding of `amount` over r's 4,096 values, and one cell each for the
    // other binding, the participation, the assumption and the invariant.
    assert_eq!(result.work(), (8_192, 4_100));
    assert_eq!(planned_tables(&result), 12_292);
}

/// A wider field is refused by the planner before anything is evaluated.
#[test]
fn a_wider_binding_is_refused_before_evaluation() {
    let result = check_committed("limits/widehuge.json");
    assert_eq!(result.verdict(), Verdict::Refused);
    assert_eq!(result.work(), (0, 0));
    assert_eq!(result.evaluated(), 0);
}

/// The counters count evaluation wherever it happens: with the tables built
/// before the plan, a planted defect, the refused check reports work.
#[test]
fn the_evaluation_counters_see_evaluation_before_a_refusal() {
    let (text, sources) = committed("ladder-10/wide.json");
    let domain = DomainSources {
        manifest: &text,
        components: &sources,
    };
    let honest = check_domain(&domain).unwrap_or_else(fail);
    assert_eq!(honest.verdict(), Verdict::Refused);
    assert_eq!(honest.work(), (0, 0));
    let defect = check_domain_building_tables_first(&domain).unwrap_or_else(fail);
    assert_eq!(defect.verdict(), Verdict::Refused);
    let (decisions, formula_values) = defect.work();
    assert!(decisions > 0 && formula_values > 0, "{:?}", defect.work());
}

/// Request fields of every size, even unread, are planned without being
/// enumerated.
#[test]
fn unread_request_fields_of_any_size_are_never_enumerated() {
    for manifest in [
        "limits/huge/composition.json",
        "limits/medium/composition.json",
        "limits/ovf/composition.json",
    ] {
        let result = check_committed(manifest);
        assert_ne!(result.verdict(), Verdict::Violated, "{manifest}");
        assert!(
            !result.json()["monolithic"]["pairs"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
        );
    }
}

/// A proved composition whose reachable joint steps include committed
/// failures: the wire after two lockstep counters fails on every step it
/// takes, and the coupling holds through each.
#[test]
fn a_proved_composition_reaches_committed_failure_steps() {
    let result = check_committed("documented/lockstep-failure.json");
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
    let (text, sources) = committed("documented/lockstep-failure.json");
    let joint = super::JointComposition::from_sources(&DomainSources {
        manifest: &text,
        components: &sources,
    })
    .unwrap_or_else(fail);
    let rows = joint.reachable(1_000).unwrap_or_else(fail);
    let failures = rows
        .iter()
        .filter(|row| {
            matches!(
                row.outcome.kind,
                super::OutcomeKind::CommittedFailure { .. }
            )
        })
        .count();
    assert!(failures > 0, "{rows:?}");
    // Every reachable successor keeps the counters in lockstep.
    assert!(
        rows.iter()
            .all(|row| row.outcome.successor[0] == row.outcome.successor[1])
    );
}


/// An actual native composition containing up to 50 separately bound .zeno
/// contract *instances* and a nontrivial 2-member coupling. This checks
/// executable parser / Authority / compositional checker behavior; it does
/// NOT prove the missing implementation-to-Verus correspondence.
fn native_27_counter_chain(n: usize, coupling: &str) -> Built {
    let mut parts = Vec::with_capacity(n);
    for i in 0..n {
        let name = format!("counter_{i:02}");
        parts.push(part(
            &name,
            "faults/counter",
            &["value"],
            "op == 0",
            json!({"action": "150", "authorized": "authorized"}),
            "true",
            "true",
        ));
    }
    Built::new(
        parts,
        json!([{
            "name": "twin",
            "members": ["counter_00", "counter_01"],
            "invariant": coupling,
        }]),
    )
}

/// Proof workload scales with local and coupling obligations; it need not
/// enumerate the huge Cartesian product of actual native component states.
#[test]
fn native_27_fifty_source_bound_components_keep_a_coupling_without_global_product() {
    for n in [25usize, 50usize] {
        let built = native_27_counter_chain(n, "counter_00.value == counter_01.value");
        let result = built.check().unwrap_or_else(fail);
        let report = result.json();
        assert_eq!(result.verdict(), Verdict::Proved, "{report}");
        assert_eq!(
            report["monolithic"]["states"],
            (0..n).fold(1u128, |product, _| product * 3).to_string(),
        );
        assert_eq!(report["monolithic"]["requests"], "12");
        assert_eq!(report["monolithic"]["enumerated"], false);
        assert_eq!(
            report["obligations"].as_array().map(Vec::len),
            Some(2 * n + 1),
        );
        assert!(result.evaluated() > 0);
        assert!(result.evaluated() < 10_000);
        // This is an actual 2.6 composition check, not synthetic JS rows.
        let (decisions, formula_cells) = result.work();
        assert!(decisions > 0 && formula_cells > 0);
        println!(
            "native_27_scaling instances={n} local_and_coupling_rows={} monolithic_states={} authority_decisions={decisions} formula_cells={formula_cells}",
            result.evaluated(),
            report["monolithic"]["states"],
        );

        // Evaluate the REAL joint-step API on the same source-backed
        // composition, not merely the compositional proof's row counts.
        let manifest = built.text();
        let joint = super::JointComposition::from_sources(&DomainSources {
            manifest: &manifest,
            components: &built.sources,
        })
        .unwrap_or_else(fail);
        assert_eq!(joint.state_layout().len(), n);
        assert_eq!(joint.genesis(), vec![0i128; n]);
        let initial = vec![0i128; n];
        let positive = joint
            .joint_step(&initial, &[0, 1, 1])
            .unwrap_or_else(fail);
        assert_eq!(positive.kind, super::OutcomeKind::Accept);
        assert_eq!(positive.successor, vec![1i128; n]);

        // A late Reject must roll back every earlier provisional Accept,
        // including across 50 components. This is PURE joint-step behavior,
        // not a SQLite transaction or external-effect durability proof.
        let mut full_late = vec![0i128; n];
        full_late[n - 1] = 2;
        let refusal = joint
            .joint_step(&full_late, &[0, 1, 1])
            .unwrap_or_else(fail);
        assert_eq!(
            refusal.kind,
            super::OutcomeKind::Reject {
                component: format!("counter_{:02}", n - 1),
                reason: Some(201),
            }
        );
        assert_eq!(refusal.successor, full_late);
    }
}

/// A deliberately false coupling must not be accepted merely because all
/// 50 component-local checks passed. It is true at genesis, but falsified
/// when both counters increment from 0 to 1 on an authorized request.
#[test]
fn native_27_noninductive_coupling_refutes_even_with_fifty_valid_components() {
    let built = native_27_counter_chain(50, "counter_00.value == 0");
    let result = built.check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Violated, "{}", result.json());
    let report = result.json();
    assert_eq!(report["witness"]["obligation"]["kind"], "couple");
    assert_eq!(report["witness"]["obligation"]["subject"], "twin");
}
