use super::*;

const PROJECT: &str = include_str!("../../../../../../examples/apps/escrow-deal/project.zeno");
const POLICY: &str = include_str!("../../../../../../examples/apps/escrow-deal/v2/policy.json");
const PROPERTIES: &str =
    include_str!("../../../../../../examples/apps/escrow-deal/v2/run-properties.json");

fn small_project() -> String {
    PROJECT
        .replace("Units in 0..=3", "Units in 0..=1")
        .replace("DepositUnits in 1..=3", "DepositUnits in 1..=1")
        .replace("Day in 0..=14", "Day in 0..=8")
}

fn results(project: &str, policy: &str, properties: &str) -> Vec<Value> {
    match judge(project, policy, properties, 1_048_576).unwrap_or_else(|e| panic!("{e}")) {
        lemma::Lemmas::Judged(values) => values,
        lemma::Lemmas::Inconclusive(stop) => panic!("{stop:?}"),
        lemma::Lemmas::NoStartingState(value) => panic!("no starting state {value}"),
    }
}

fn weakened_project() -> String {
    small_project().lines().map(|line| if line.starts_with("law 510 ") {
        "law 510 seller_paid_by_delivery on commit = outbox.104 >= 0 && outbox.104.140 >= 0;"
    } else { line }).collect::<Vec<_>>().join("\n")
}

fn repeated_release() -> String {
    let mut policy: Value = serde_json::from_str(POLICY).unwrap_or_else(|_| unreachable!());
    policy["cases"][1]["when"] = json!(policy["cases"][1]["when"].as_str().unwrap_or("").replace(
        "action == 162 && status != 152",
        "action == 162 && status != 152 && status != 153"
    ));
    policy["cases"][5]["post"]["112"] = json!("choose(held == 0, pre.100.112, held)");
    policy["cases"][5]["outbox"][0]["payload"]["140"] =
        json!("choose(held == 0, pre.100.112, held)");
    policy.to_string()
}

#[test]
fn escrow_properties_hold_and_repeated_release_has_a_shortest_replayable_run() {
    let good = results(&small_project(), POLICY, PROPERTIES);
    assert!(good.iter().all(|p| p["verdict"] == "inductive"), "{good:?}");
    let bad = results(&weakened_project(), &repeated_release(), PROPERTIES);
    assert_eq!(bad[0]["verdict"], "false", "{bad:?}");
    assert_eq!(
        bad[0]["trace_from_genesis"].as_array().map(Vec::len),
        Some(4)
    );
    assert_eq!(bad[1]["verdict"], "inductive");
    let text = bad[0]["scenario"]
        .as_array()
        .unwrap_or_else(|| unreachable!())
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let replay =
        super::super::scenario::replay(&weakened_project(), &repeated_release(), &text, "witness")
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(replay.failed, 0);
    eprintln!("repeated-release witness: {text}");
}

#[test]
fn genesis_is_an_independent_run_property_obligation() {
    let properties = json!({"schema":"zeno-fcis/run-properties/1",
        "observer":{"started":{"min":0,"max":1,"initial":1,"update":"0"}},
        "properties":[{"kind":"predicate","name":"genesis","formula":"observer.started == 0"}]});
    let result = results(&small_project(), POLICY, &properties.to_string());
    assert_eq!(result[0]["verdict"], "false");
    assert_eq!(result[0]["trace_from_genesis"], json!([]));
    eprintln!("genesis witness: {}", result[0]);
}

#[test]
fn delivery_count_checks_every_delivery_and_fields_are_observable() {
    let mut policy: Value = serde_json::from_str(POLICY).unwrap_or_else(|_| unreachable!());
    let mut second = policy["cases"][5]["outbox"][0].clone();
    second["ordinal"] = json!(1);
    second["idempotency_ordinal"] = json!(1);
    policy["cases"][5]["outbox"]
        .as_array_mut()
        .unwrap_or_else(|| unreachable!())
        .push(second);
    for id in ["510", "511"] {
        policy["delivery_laws"][id]["max_deliveries"] = json!(2);
    }
    let bad = results(&weakened_project(), &policy.to_string(), PROPERTIES);
    assert_eq!(bad[2]["verdict"], "false", "{bad:?}");
    assert_eq!(
        bad[2]["trace_from_genesis"].as_array().map(Vec::len),
        Some(3)
    );
    let formulas = [
        "outbox.300 <= 1",
        "outbox.300.140 <= 1",
        "delivery.1.exists -> delivery.1.channel == 300",
    ];
    let checked = super::super::lemma::judge_transitions(
        &weakened_project(),
        &policy.to_string(),
        &formulas.map(str::to_owned),
        1_048_576,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let lemma::Lemmas::Judged(checked) = checked else {
        panic!("incomplete")
    };
    assert_eq!(checked[0]["verdict"], "false");
    assert_eq!(checked[1]["verdict"], "false");
    assert_eq!(checked[2]["verdict"], "checked");
    eprintln!("two-delivery witness: {}", bad[2]);
}

#[test]
fn scenario_pins_delivery_and_idempotency_ordinals() {
    use super::super::{evaluate, examples};
    let loaded = Loaded::read(&small_project(), POLICY).unwrap_or_else(|e| panic!("{e}"));
    loaded
        .bind(&loaded.rules, |authority, framer| {
            let text = "152 1 0 0 162 1 170 0 | accept - 153 0 1 0 | 300@0:0 1";
            let good = examples::parse(text, authority.descriptor())?;
            let outcome = evaluate::evaluate(authority, framer, &good[0].inputs);
            assert!(examples::agrees(&good[0], &outcome, loaded.state_width).is_ok());
            for changed in ["300@1:0", "300@0:1"] {
                let bad =
                    examples::parse(&text.replace("300@0:0", changed), authority.descriptor())?;
                assert!(
                    examples::agrees(&bad[0], &outcome, loaded.state_width).is_err(),
                    "assertion: ordinal mismatch was ignored: {changed}"
                );
            }
            eprintln!("ordinal witness: {text}");
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn observer_refusal_updates_bounds_undefined_and_limits_block_claims() {
    let mut properties = json!({"schema":"zeno-fcis/run-properties/1",
        "observer":{"refused":{"min":0,"max":1,"initial":0,"update":"choose(verdict.reject, 1, before.refused)"}},
        "properties":[{"kind":"predicate","name":"rejections_seen","formula":"verdict.reject -> observer.refused == 1"}]});
    assert_eq!(
        results(&small_project(), POLICY, &properties.to_string())[0]["verdict"],
        "inductive"
    );
    properties["observer"]["refused"]["update"] = json!("2");
    assert_eq!(
        results(&small_project(), POLICY, &properties.to_string())[0]["verdict"],
        "undefined"
    );
    properties["observer"]["refused"]["update"] = json!("div_floor(1, 0)");
    assert_eq!(
        results(&small_project(), POLICY, &properties.to_string())[0]["verdict"],
        "undefined"
    );
    properties["observer"]["refused"]["max"] = json!(1_000_000);
    assert_eq!(
        results(&small_project(), POLICY, &properties.to_string())[0]["verdict"],
        "inconclusive"
    );
    assert!(
        judge(
            &small_project(),
            POLICY,
            "{\"schema\":1,\"schema\":2}",
            1_048_576
        )
        .is_err()
    );
}

#[test]
fn per_key_at_most_once_and_strictly_later_delivery_order() {
    let properties = json!({"schema":"zeno-fcis/run-properties/1","properties":[
        {"kind":"at_most_once","name":"per_units","channel":300,"key_field":140},
        {"kind":"never_after","name":"no_later_payout","a":300,"b":300}]});
    let good = results(&small_project(), POLICY, &properties.to_string());
    assert!(good.iter().all(|p| p["verdict"] == "inductive"), "{good:?}");
    let bad = results(
        &weakened_project(),
        &repeated_release(),
        &properties.to_string(),
    );
    assert!(bad.iter().all(|p| p["verdict"] == "false"), "{bad:?}");
}

#[test]
fn technical_refusals_update_observers_and_replay_as_scenario_lines() {
    let mut policy: Value = serde_json::from_str(POLICY).unwrap_or_else(|_| unreachable!());
    policy["cases"][3]["post"]["111"] = json!("div_floor(1, 0)");
    let properties = json!({"schema":"zeno-fcis/run-properties/1",
        "observer":{"seen":{"min":0,"max":1,"initial":0,"update":"choose(verdict.refused, 1, before.seen)"}},
        "properties":[
            {"kind":"predicate","name":"observed","formula":"verdict.refused -> observer.seen == 1"},
            {"kind":"predicate","name":"no_refusal","formula":"!verdict.refused"}]});
    let checked = results(
        &small_project(),
        &policy.to_string(),
        &properties.to_string(),
    );
    assert_eq!(checked[0]["verdict"], "inductive");
    assert_eq!(checked[1]["verdict"], "false");
    assert_eq!(
        checked[1]["trace_from_genesis"].as_array().map(Vec::len),
        Some(1)
    );
    let text = checked[1]["scenario"]
        .as_array()
        .unwrap_or_else(|| unreachable!())
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let replay =
        super::super::scenario::replay(&small_project(), &policy.to_string(), &text, "refusal")
            .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(replay.failed, 0, "{text}");
    let long = "1 + ".repeat(2000) + "0 == 0";
    let names = BTreeMap::new();
    assert!(Expression::compile(&long, &names, true).is_err());
}
